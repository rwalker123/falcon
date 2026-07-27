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
    fn publish_frame(&self, frame: &Arc<Vec<u8>>);
}

/// What the turn thread hands the publisher.
///
/// The `Frame` variant is ~2 KB against `Sync`'s pointer, and boxing it — clippy's suggestion — would
/// be strictly worse here: the channel holds [`PUBLISH_QUEUE_DEPTH`] slots, so the whole waste is a
/// few KB allocated once, while a `Box` would add a heap allocation *and* the same memcpy to every
/// frame on the turn thread's critical path. That is the one place in this file where a byte counts.
#[allow(clippy::large_enum_variant)]
enum PublishRequest {
    /// A captured world to hash, diff, encode and broadcast.
    Frame {
        snapshot: WorldSnapshot,
        kind: Publication,
    },
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

/// Rollback ring depth when nothing says otherwise (`SimulationConfig::snapshot_history_limit`
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

    /// Publish a resolved TURN. Returns as soon as the snapshot is queued — blocking only if the
    /// publisher is [`PUBLISH_QUEUE_DEPTH`] frames behind.
    pub fn update(&mut self, snapshot: WorldSnapshot) {
        self.send(PublishRequest::Frame {
            snapshot,
            kind: Publication::Turn,
        });
    }

    /// Publish a mid-tick RECAPTURE — a world-mutating command changed the world between turns.
    /// See `PublishState::publish` for why these deltas are cumulative and safe to lose.
    pub fn refresh_latest(&mut self, snapshot: WorldSnapshot) {
        self.send(PublishRequest::Frame {
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

    pub fn latest_entry(&self) -> Option<StoredSnapshot> {
        self.locked().latest_entry()
    }

    pub fn entry(&self, tick: u64) -> Option<StoredSnapshot> {
        self.locked().entry(tick)
    }

    /// The most recently published world. An accessor rather than the field it used to be, because
    /// the frame that produced it may still be in flight — see the module header.
    pub fn last_snapshot(&self) -> Option<Arc<WorldSnapshot>> {
        self.locked().last_snapshot.clone()
    }

    pub fn last_delta(&self) -> Option<Arc<WorldDelta>> {
        self.locked().last_delta.clone()
    }

    /// The last full flat frame published for this world — `Some` only on a world's first
    /// publication and after a rollback / `Resync`. Test and diagnostic access; the publisher puts
    /// frames on the wire itself.
    pub fn encoded_snapshot_flat(&self) -> Option<Arc<Vec<u8>>> {
        self.locked().encoded_snapshot_flat.clone()
    }

    /// The last flat delta published for this world.
    pub fn encoded_delta_flat(&self) -> Option<Arc<Vec<u8>>> {
        self.locked().encoded_delta_flat.clone()
    }

    /// The last published frame's per-phase breakdown, as the publisher measured it. The twin of
    /// `turn_profile::take()` for work that is no longer on the turn.
    pub fn last_publish_profile(&self) -> Vec<PhaseTiming> {
        self.locked().last_publish_profile.clone()
    }

    pub fn reset_to_entry(&mut self, entry: &StoredSnapshot) {
        self.locked().reset_to_entry(entry);
    }

    pub fn publish_full_frame(&mut self, entry: &StoredSnapshot) -> Arc<Vec<u8>> {
        self.locked().publish_full_frame(entry)
    }

    pub fn update_axis_bias(&mut self, bias: AxisBiasState) -> Option<Arc<Vec<u8>>> {
        self.locked().update_axis_bias(bias)
    }

    pub fn update_influencers(
        &mut self,
        states: Vec<InfluentialIndividualState>,
    ) -> Option<Arc<Vec<u8>>> {
        self.locked().update_influencers(states)
    }

    pub fn update_corruption(&mut self, ledger: CorruptionLedger) -> Option<Arc<Vec<u8>>> {
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
            PublishRequest::Frame { snapshot, kind } => {
                let tick = snapshot.header.tick;
                // The sink is cloned out and the frame broadcast *after* the guard is dropped: the
                // socket write is somebody else's thread, and holding publication's lock across it
                // would stall every reader for no reason.
                let (frame, sink) = {
                    let mut publish = lock(&state);
                    let frame = publish.publish(snapshot, kind);
                    publish.last_publish_profile = crate::turn_profile::publish_take();
                    (frame, publish.sink.clone())
                };
                if let (Some(frame), Some(sink)) = (frame.as_ref(), sink.as_ref()) {
                    sink.publish_frame(frame);
                }
                log::debug!(
                    "publish.frame tick={} bytes={}",
                    tick,
                    frame.map(|frame| frame.len()).unwrap_or(0)
                );
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
