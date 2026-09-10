//! The publisher thread, and the ECS-facing handle in front of it (#393).
//!
//! # Why publication is not the turn's work
//!
//! `capture_snapshot` walks the ECS world and assembles a `WorldSnapshot`. Everything after that —
//! stamping the content hash, diffing against the published baselines, encoding the flat delta,
//! writing it to the socket — reads *only* that snapshot and publication's own state. None of it
//! touches the world, and the simulation does not depend on any of it having finished.
//!
//! Measured on an 80×52 map it was **~4.0 ms of an ~8.8 ms turn**, so it was also most of the
//! latency between a player pressing End Turn and the world moving. Moving it here does not make
//! the work cheaper; it makes turn latency *independent of it*, which is the property that holds as
//! the payload and the number of encodings grow (see `.claude/rules/core_sim/turn-profiling.md`).
//!
//! # The three questions this design answers
//!
//! **Ordering.** Frames must reach clients in publication order. One publisher thread behind one
//! FIFO channel gives that for free; a pool would not, and no amount of sequence-number checking on
//! the client makes an out-of-order delta applicable.
//!
//! **That survives per-seat delivery unchanged, and it is why the fan-out is not a pool.** A turn
//! queues one frame per audience on the same FIFO, the same thread publishes them in the order they
//! were queued, and a *global* order is a per-seat order restricted to one seat's frames. A worker
//! per seat would give each seat its own ordering and no cheaper diff — the diff already fans out
//! across sections on a bounded pool inside `publish` (`capture::DIFF_POOL_THREADS`).
//!
//! **Backpressure.** The queue is bounded ([`PUBLISH_QUEUE_DEPTH`]) and a full queue **blocks the
//! turn thread**. Dropping is not available: turn-path deltas chain on `base_frame_seq`, so a
//! dropped intermediate leaves every later delta naming a frame the client never applied, and the
//! client is stuck until it notices and asks for a resync. Blocking degrades to exactly today's
//! behaviour under sustained overload and is never worse than it.
//!
//! **Who owns the ring.** The publisher does — the baselines, the rollback ring and the publication
//! sequence are all [`PublishState`], behind a mutex. The turn thread never reads them. The rare
//! paths that must (rollback, `Resync`, `export_map`, and every test that inspects the last
//! snapshot) go through an accessor here, and **every accessor drains the queue first**
//! ([`SnapshotHistory::locked`]) so a read can never race a frame in flight. Those paths are
//! human-paced, so a round trip costs nothing that matters.

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use bevy::prelude::Resource;
use crossbeam_channel::{bounded, Receiver, Sender};
use sim_runtime::{
    AxisBiasState, CorruptionLedger, InfluentialIndividualState, WorldDelta, WorldSnapshot,
};

use super::capture::{Publication, PublishState, StoredSnapshot};
use crate::orders::FactionId;
use crate::turn_profile::PhaseTiming;

/// How many captured frames may sit between the turn thread and the publisher.
///
/// Two: one being published, one waiting behind it. Deep enough that a turn never waits on a
/// publisher running its normal ~3 ms, shallow enough that "the publisher is falling behind" shows
/// up as turn latency — the honest symptom — within a turn or two rather than as a queue quietly
/// swallowing memory. A `WorldSnapshot` is a whole world; a deep queue here is a deep pile of them.
const PUBLISH_QUEUE_DEPTH: usize = 2;

/// Where a published frame goes.
///
/// A trait rather than a direct `SnapshotServer` because `core_sim`'s socket layer is the *server
/// binary's* concern: the library publishes whether or not anything is listening, which is what
/// lets every test drive real publication with no socket at all.
pub trait FrameSink: Send + Sync + 'static {
    /// Deliver one frame to **the seat it was captured for**. A sink that cannot route (a test
    /// double, a file writer) is free to ignore the seat; the socket does not — a frame is one
    /// viewer's world, and delivering it to another seat's client is the disclosure PR #648 closed
    /// on the frame's contents.
    fn publish_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>);
}

/// What the turn thread hands the publisher.
///
/// The `Frame` variant is ~2 KB against `Sync`'s pointer, and boxing it — clippy's suggestion — would
/// be strictly worse here: the channel holds [`PUBLISH_QUEUE_DEPTH`] slots, so the whole waste is a
/// few KB allocated once, while a `Box` would add a heap allocation *and* the same memcpy to every
/// frame on the turn thread's critical path. That is the one place in this file where a byte counts.
#[allow(clippy::large_enum_variant)]
enum PublishRequest {
    /// A captured world to diff, encode and deliver — and **the seat it was captured for**, which
    /// selects the baselines it is diffed against and the clients it goes to.
    Frame {
        seat: FactionId,
        snapshot: WorldSnapshot,
        kind: Publication,
    },
    /// **The audience set this capture is about to publish for.** Queued ahead of its frames and
    /// applied in the same order, so publication forgets a seat nothing captures for any more
    /// without the turn thread having to wait on the publisher to say so.
    Audiences(Vec<FactionId>),
    /// A FIFO barrier. The publisher answers it only after every frame queued ahead of it has been
    /// published, which is what makes [`SnapshotHistory::locked`] a safe read rather than a race.
    Sync(Sender<()>),
}

/// The ECS-facing handle on snapshot publication: a channel to the publisher thread, and shared
/// access to the state that thread owns.
///
/// It kept its old name because it is still what every reader asks for — `resource::<SnapshotHistory>()`
/// then `latest_entry()` / `entry(tick)` — and those reads mean exactly what they used to. What
/// changed is that `last_snapshot`, `last_delta` and the two encoded-frame fields are **accessors
/// rather than fields**: reading them has to drain the queue first, and a plain field could not.
#[derive(Resource)]
pub struct SnapshotHistory {
    /// `None` once [`SnapshotHistory::shutdown`] has run. Dropping the sender is what tells the
    /// publisher to finish.
    sender: Option<Sender<PublishRequest>>,
    state: Arc<Mutex<PublishState>>,
    publisher: Option<JoinHandle<()>>,
}

impl Default for SnapshotHistory {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_HISTORY_CAPACITY)
    }
}

/// Publication ring depth when nothing says otherwise (`snapshot::PUBLICATION_RING_DEPTH`
/// overrides it for a real world).
const DEFAULT_HISTORY_CAPACITY: usize = 256;

impl SnapshotHistory {
    pub fn with_capacity(capacity: usize) -> Self {
        let state = Arc::new(Mutex::new(PublishState::with_capacity(capacity)));
        let (sender, receiver) = bounded(PUBLISH_QUEUE_DEPTH);
        let publisher_state = Arc::clone(&state);
        let publisher = thread::Builder::new()
            .name("snapshot-publisher".to_string())
            .spawn(move || run_publisher(receiver, publisher_state))
            .expect("spawning the snapshot publisher thread");
        Self {
            sender: Some(sender),
            state,
            publisher: Some(publisher),
        }
    }

    /// Attach the socket published frames go to. Called once per world by the server, **before**
    /// that world's first turn resolves, so the baseline frame is broadcast like any other.
    pub fn attach_sink(&self, sink: Arc<dyn FrameSink>) {
        self.locked().sink = Some(sink);
    }

    /// **State the audience set the frames about to be queued belong to.** Called by the capture
    /// once per pass, before its frames.
    pub fn retain_audiences(&mut self, audiences: Vec<FactionId>) {
        self.send(PublishRequest::Audiences(audiences));
    }

    /// Publish a resolved TURN for one seat. Returns as soon as the snapshot is queued — blocking
    /// only if the publisher is [`PUBLISH_QUEUE_DEPTH`] frames behind.
    ///
    /// **One call per audience per turn.** The capture builds a frame per seat and queues each here;
    /// the publisher applies them in order, so seat 0's frame is never diffed against seat 1's
    /// baselines.
    pub fn update(&mut self, seat: FactionId, snapshot: WorldSnapshot) {
        self.send(PublishRequest::Frame {
            seat,
            snapshot,
            kind: Publication::Turn,
        });
    }

    /// Publish a mid-tick RECAPTURE — a world-mutating command changed the world between turns.
    /// See `PublishState::publish` for why these deltas are cumulative and safe to lose.
    pub fn refresh_latest(&mut self, seat: FactionId, snapshot: WorldSnapshot) {
        self.send(PublishRequest::Frame {
            seat,
            snapshot,
            kind: Publication::Recapture,
        });
    }

    /// Drain the queue and finish the publisher thread. Idempotent.
    ///
    /// The server calls this on the outgoing world before building the next one, so the two
    /// publishers can never overlap: the old world's queued frames are all on the wire before the
    /// new world's baseline is. (The client would survive the overlap — a stale-epoch delta names a
    /// `base_frame_seq` it does not hold and is dropped — but "the previous world's frames arrive
    /// after the new world's" is precisely the failure `world-handoff.md` exists to prevent, and it
    /// should not be left resting on the client noticing.)
    pub fn shutdown(&mut self) {
        self.sender = None;
        if let Some(publisher) = self.publisher.take() {
            let _ = publisher.join();
        }
    }

    pub fn capacity(&self) -> usize {
        self.locked().capacity()
    }

    pub fn set_capacity(&mut self, capacity: usize) {
        self.locked().set_capacity(capacity);
    }

    pub fn len(&self) -> usize {
        self.locked().len()
    }

    pub fn is_empty(&self) -> bool {
        self.locked().is_empty()
    }

    /// The primary audience's latest published frame — see [`Self::primary_audience`].
    pub fn latest_entry(&self) -> Option<StoredSnapshot> {
        self.locked().latest_entry()
    }

    /// **The seat every seat-blind accessor on this handle answers for**: the lowest-numbered
    /// audience that has published. One client — single player, every test, the idle boot app —
    /// means exactly one audience, so those accessors mean what they always meant.
    pub fn primary_audience(&self) -> Option<FactionId> {
        self.locked().primary_audience()
    }

    /// The audiences this world has published to, in seat order.
    pub fn audiences(&self) -> Vec<FactionId> {
        self.locked().audiences()
    }

    /// One named seat's latest published frame.
    pub fn latest_entry_for(&self, seat: FactionId) -> Option<StoredSnapshot> {
        self.locked().latest_entry_for(seat)
    }

    /// **Forget a seat's baselines**, so whoever claims that seat next is baselined on a full frame.
    /// Called when a seat is released; a stale baseline would leave the next occupant holding rows
    /// it was never sent.
    pub fn drop_audience(&mut self, seat: FactionId) {
        self.locked().drop_audience(seat);
    }

    pub fn entry(&self, tick: u64) -> Option<StoredSnapshot> {
        self.locked().entry(tick)
    }

    /// The most recently published world. An accessor rather than the field it used to be, because
    /// the frame that produced it may still be in flight — see the module header.
    pub fn last_snapshot(&self) -> Option<Arc<WorldSnapshot>> {
        self.locked().last_snapshot()
    }

    pub fn last_delta(&self) -> Option<Arc<WorldDelta>> {
        self.locked().last_delta()
    }

    /// The last full flat frame published for this world — `Some` only on a world's first
    /// publication and after a rollback / `Resync`. Test and diagnostic access; the publisher puts
    /// frames on the wire itself.
    pub fn encoded_snapshot_flat(&self) -> Option<Arc<Vec<u8>>> {
        self.locked().encoded_snapshot_flat()
    }

    /// The last flat delta published for this world.
    pub fn encoded_delta_flat(&self) -> Option<Arc<Vec<u8>>> {
        self.locked().encoded_delta_flat()
    }

    /// The last published frame's per-phase breakdown, as the publisher measured it. The twin of
    /// `turn_profile::take()` for work that is no longer on the turn.
    pub fn last_publish_profile(&self) -> Vec<PhaseTiming> {
        self.locked().last_publish_profile.clone()
    }

    /// Rewind one seat's baselines to a frame it published.
    pub fn reset_to_entry_for(&mut self, seat: FactionId, entry: &StoredSnapshot) {
        self.locked().reset_to_entry_for(seat, entry);
    }

    /// **Rewind every audience's baselines to its own latest frame** — what a rollback owes, since
    /// it moved the world under all of them at once.
    pub fn reset_all_to_latest_entry(&mut self) {
        self.locked().reset_all_to_latest_entry();
    }

    /// Re-publish one seat's latest frame whole, on a **fresh** sequence number. The `Resync`
    /// answer, for the asking seat.
    pub fn publish_full_frame_for(&mut self, seat: FactionId) -> Option<Arc<Vec<u8>>> {
        self.locked().publish_full_frame_for(seat)
    }

    /// [`Self::publish_full_frame_for`] every audience, each on its own fresh number.
    pub fn publish_full_frame_for_all(&mut self) -> Vec<(FactionId, Arc<Vec<u8>>)> {
        self.locked().publish_full_frame_for_all()
    }

    pub fn update_axis_bias(&mut self, bias: AxisBiasState) -> Vec<(FactionId, Arc<Vec<u8>>)> {
        self.locked().update_axis_bias(bias)
    }

    pub fn update_influencers(
        &mut self,
        states: Vec<InfluentialIndividualState>,
    ) -> Vec<(FactionId, Arc<Vec<u8>>)> {
        self.locked().update_influencers(states)
    }

    pub fn update_corruption(
        &mut self,
        ledger: CorruptionLedger,
    ) -> Vec<(FactionId, Arc<Vec<u8>>)> {
        self.locked().update_corruption(ledger)
    }

    fn send(&self, request: PublishRequest) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(request).is_err() {
            log::error!("snapshot publisher is gone; frame dropped");
        }
    }

    /// Drain the publisher's queue, then take the state lock.
    ///
    /// **Every reader and every inline writer goes through here**, and the order is the whole
    /// point: sync first (so nothing queued is still unapplied), lock second. Locking first would
    /// deadlock against the publisher, which holds the same lock while publishing.
    fn locked(&self) -> MutexGuard<'_, PublishState> {
        self.sync();
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Block until the publisher has finished every frame queued before this call. A no-op once the
    /// publisher has been shut down, when there is nothing left in flight by definition.
    fn sync(&self) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        let (reply, answered) = bounded(1);
        if sender.send(PublishRequest::Sync(reply)).is_ok() {
            let _ = answered.recv();
        }
    }
}

impl Drop for SnapshotHistory {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The publisher thread's body: publish each queued frame in order, answer each barrier in turn.
///
/// It exits when the channel disconnects, which is the sender being dropped by
/// [`SnapshotHistory::shutdown`] or by the whole handle going away with its app.
fn run_publisher(requests: Receiver<PublishRequest>, state: Arc<Mutex<PublishState>>) {
    while let Ok(request) = requests.recv() {
        match request {
            PublishRequest::Frame {
                seat,
                snapshot,
                kind,
            } => {
                let tick = snapshot.header.tick;
                // The sink is cloned out and the frame broadcast *after* the guard is dropped: the
                // socket write is somebody else's thread, and holding publication's lock across it
                // would stall every reader for no reason.
                let (frame, sink) = {
                    let mut publish = lock(&state);
                    let frame = publish.publish(seat, snapshot, kind);
                    publish.last_publish_profile = crate::turn_profile::publish_take();
                    (frame, publish.sink.clone())
                };
                if let (Some(frame), Some(sink)) = (frame.as_ref(), sink.as_ref()) {
                    sink.publish_frame(seat, frame);
                }
                log::debug!(
                    "publish.frame tick={} seat={} bytes={}",
                    tick,
                    seat,
                    frame.map(|frame| frame.len()).unwrap_or(0)
                );
            }
            PublishRequest::Audiences(audiences) => {
                lock(&state).retain_audiences(&audiences);
            }
            // Dropping the reply sender answers just as well as sending does — the waiter is
            // `recv`ing, and a disconnect wakes it. Sending is the honest form of "queue drained".
            PublishRequest::Sync(reply) => {
                let _ = reply.send(());
            }
        }
    }
}

/// Lock publication's state, recovering from poisoning rather than propagating it: a panic in one
/// frame's encode must not take every later read down with it.
fn lock(state: &Arc<Mutex<PublishState>>) -> MutexGuard<'_, PublishState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
