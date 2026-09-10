//! ⛔ **`WorldSnapshot::apply_delta` AGAINST THE SHIPPED PRODUCER.**
//!
//! `sim_schema`'s unit tests pin each merge rule against a hand-built delta; this file pins the
//! merge against the deltas the server actually publishes. A two-seat world is driven through the
//! real publication path, every frame each seat is sent is decoded with the real decoder and
//! applied in order, and after every publication the applied snapshot has to equal the full
//! snapshot the server captured for that seat at that moment. A rule that disagreed with
//! `diff_indexed` / `diff_whole` / `diff_appended` on real data — a section keyed on the wrong
//! field, a whole section left standing, a feed row lost — shows up here as a diff and nowhere
//! else, because a client that drifts is a client that looks calm.
//!
//! **The frames are the sink's**, exactly as `seat_frames.rs` reads them: the bytes a client would
//! receive, tagged with the seat they were addressed to.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    recapture_snapshot_in_place, run_turn, CommandEventEntry, CommandEventKind, CommandEventLog,
    FactionId, FactionInventory, FrameSink, SimulationTick, SnapshotAudiences, SnapshotHistory,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use serde_json::Value;
use sim_schema::{
    decode_frame_flatbuffer, encode_snapshot_json, FramePayload, TileState, WorldSnapshot,
};

/// A stockpiled good each people holds — distinct amounts, so a row applied to the wrong seat's
/// snapshot is unmistakable in the failure message.
const STOCK_ITEM: &str = "provisions";
const HOME_STOCK: i64 = 111;
const RIVAL_STOCK: i64 = 222;
/// What the mid-run command adds — the world mutation a recapture publishes.
const RECAPTURE_STOCK: i64 = 7;

/// Turns driven. Long enough that the feed window below evicts rows several times over.
const TURNS: usize = 14;
/// The turn on which the mid-tick recapture pair fires.
const RECAPTURE_TURN: usize = 6;
/// The feed window — narrow, so eviction happens inside the run rather than after it.
const RETENTION_TURNS: u64 = 4;

/// Every frame the publisher handed the socket, with the seat it was addressed to.
#[derive(Default)]
struct Recorder {
    frames: Mutex<Vec<(FactionId, Arc<Vec<u8>>)>>,
}

impl Recorder {
    fn for_seat(&self, seat: FactionId) -> Vec<Arc<Vec<u8>>> {
        self.frames
            .lock()
            .expect("the recorder's frames")
            .iter()
            .filter(|(addressed, _)| *addressed == seat)
            .map(|(_, frame)| Arc::clone(frame))
            .collect()
    }
}

impl FrameSink for Recorder {
    fn publish_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>) {
        self.frames
            .lock()
            .expect("the recorder's frames")
            .push((seat, Arc::clone(frame)));
    }
}

/// Every accessor on `SnapshotHistory` drains the publisher's queue first, so this is the barrier.
fn drain_publisher(app: &App) {
    let _ = app.world.resource::<SnapshotHistory>().audiences();
}

/// The world as the server captured it for `seat` at its latest publication — the comparison
/// target. A recapture refreshes the ring's current entry, so this is right after either kind.
fn captured_for(app: &App, seat: FactionId) -> WorldSnapshot {
    app.world
        .resource::<SnapshotHistory>()
        .latest_entry_for(seat)
        .unwrap_or_else(|| panic!("seat {seat} was published a frame"))
        .snapshot
        .as_ref()
        .clone()
}

/// The comparable form of a snapshot: keyed sections sorted, and the fields the WIRE never carries
/// zeroed on both sides.
///
/// The keyed sections are SETS to the producer (`diff_indexed` walks a map), so their order in a
/// capture is not a contract; the merge keeps the baseline's order and appends, which may differ.
/// Sorting by key before comparing is what makes the comparison about content.
///
/// The server's capture holds four things no frame can deliver, so a client cannot be expected to
/// hold them either (the codec round trip in `sim_schema` lists the same set): a culture layer's
/// traits and divergence fields (#386 — topology only goes out), a cohort's raw fixed-point age
/// brackets (`(deprecated)` slots), `start_marker` (no FlatBuffers field at all), and a full
/// snapshot's `base_frame_seq`, which the capture leaves at `0` while the applied header is the
/// delta's and names its base.
fn off_wire_fields_zeroed(snapshot: &WorldSnapshot) -> WorldSnapshot {
    let mut stripped = snapshot.clone();
    stripped.header.base_frame_seq = 0;
    stripped.start_marker = None;
    // Tiles are the one section the producer diffs through a DEADBAND (`diff_indexed` judges them
    // by `TileState::same_published_state`, hundredths precision), so a client legitimately lags
    // the capture by under a hundredth on a drifting field until the drift is worth publishing.
    // They are compared by that same criterion in `assert_tiles_match_within_the_deadband`, not
    // here.
    stripped.tiles.clear();
    for cohort in &mut stripped.populations {
        cohort.children = 0;
        cohort.working = 0;
        cohort.elders = 0;
    }
    for layer in &mut stripped.culture_layers {
        layer.traits.clear();
        layer.divergence = 0;
        layer.soft_threshold = 0;
        layer.hard_threshold = 0;
        layer.ticks_above_soft = 0;
        layer.ticks_above_hard = 0;
        layer.last_updated_tick = 0;
    }
    stripped
}

/// The JSON as a VALUE rather than as text, because one thing the text says the wire cannot: a
/// negative zero. FlatBuffers omits a scalar equal to its default and `-0.0 == 0.0`, so a `-0.0`
/// the sim captured (a subtraction that landed on zero from below) is never written and reads
/// back as `+0.0`; `serde_json::Value` compares the two equal, as every consumer would.
fn as_value(snapshot: &WorldSnapshot) -> Value {
    serde_json::from_str(&encode_snapshot_json(snapshot).expect("json")).expect("parses")
}

/// [`canonical`] without the sort — what "differed only in keyed-row order" is measured against.
fn unsorted(snapshot: &WorldSnapshot) -> Value {
    as_value(&off_wire_fields_zeroed(snapshot))
}

/// The first path at which two JSON values disagree, so a drift names its field rather than
/// dumping two 4,000-tile worlds.
fn first_difference(applied: &Value, captured: &Value, path: &str) -> Option<String> {
    match (applied, captured) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys().filter(|key| !a.contains_key(*key))) {
                let child = format!("{path}.{key}");
                match (a.get(key), b.get(key)) {
                    (Some(x), Some(y)) => {
                        if let Some(found) = first_difference(x, y, &child) {
                            return Some(found);
                        }
                    }
                    (x, y) => {
                        return Some(format!("{child}: applied {x:?} vs captured {y:?}"));
                    }
                }
            }
            None
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!(
                    "{path}: applied has {} rows, captured has {}",
                    a.len(),
                    b.len()
                ));
            }
            a.iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (x, y))| first_difference(x, y, &format!("{path}[{i}]")))
        }
        (x, y) if x == y => None,
        (x, y) => Some(format!("{path}: applied {x} vs captured {y}")),
    }
}

/// Tiles, by the producer's own criterion: the same set of entities, each row judged by
/// `TileState::same_published_state` — exact on every discrete field, hundredths on the drifting
/// ones, which is precisely when `diff_indexed` would have published it.
fn assert_tiles_match_within_the_deadband(
    seat: FactionId,
    applied: &[TileState],
    captured: &[TileState],
    when: &str,
) {
    let mut applied: Vec<&TileState> = applied.iter().collect();
    let mut captured: Vec<&TileState> = captured.iter().collect();
    applied.sort_by_key(|tile| tile.entity);
    captured.sort_by_key(|tile| tile.entity);
    assert_eq!(
        applied.len(),
        captured.len(),
        "seat {seat} holds a different number of tiles than the server captured {when}"
    );
    for (held, server) in applied.iter().zip(&captured) {
        assert!(
            held.same_published_state(server),
            "seat {seat}'s tile {} differs from the capture beyond the wire deadband {when}: \
             applied {held:?} vs captured {server:?}",
            held.entity
        );
    }
}

fn canonical(snapshot: &WorldSnapshot) -> Value {
    let mut sorted = off_wire_fields_zeroed(snapshot);
    sorted.tiles.sort_by_key(|row| row.entity);
    sorted.populations.sort_by_key(|row| row.entity);
    sorted.power.sort_by_key(|row| row.entity);
    sorted.generations.sort_by_key(|row| row.id);
    sorted.influencers.sort_by_key(|row| row.id);
    sorted.culture_layers.sort_by_key(|row| row.id);
    sorted.knowledge_ledger.sort_by_key(|row| row.wire_key());
    sorted
        .discovery_progress
        .sort_by_key(|row| (row.faction, row.discovery));
    sorted
        .great_discoveries
        .sort_by_key(|row| (row.faction, row.id));
    sorted
        .great_discovery_progress
        .sort_by_key(|row| (row.faction, row.discovery));
    as_value(&sorted)
}

/// One seat's client: the snapshot it holds and how many of its frames it has consumed.
#[derive(Default)]
struct SeatClient {
    held: Option<WorldSnapshot>,
    consumed: usize,
    deltas_applied: usize,
    deltas_with_events: usize,
    deltas_with_removals: usize,
    /// Publications where the raw (unsorted) JSON differed but the canonical form matched.
    order_only_differences: usize,
}

impl SeatClient {
    /// Decode and apply every frame this seat has been sent since the last call.
    fn catch_up(&mut self, seat: FactionId, frames: &[Arc<Vec<u8>>]) {
        for frame in &frames[self.consumed..] {
            match decode_frame_flatbuffer(frame).expect("the frame decodes") {
                FramePayload::Snapshot(snapshot) => {
                    assert!(
                        self.held.is_none(),
                        "seat {seat} was sent a second full frame mid-stream"
                    );
                    self.held = Some(snapshot);
                }
                FramePayload::Delta(delta) => {
                    let held = self
                        .held
                        .as_mut()
                        .unwrap_or_else(|| panic!("seat {seat}'s first frame must be full"));
                    self.deltas_with_events += usize::from(delta.command_events.is_some());
                    let removals = delta.removed_tiles.len()
                        + delta.removed_populations.len()
                        + delta.removed_power.len()
                        + delta.removed_generations.len()
                        + delta.removed_influencers.len()
                        + delta.removed_culture_layers.len()
                        + delta.removed_knowledge_ledger.len();
                    self.deltas_with_removals += usize::from(removals > 0);
                    held.apply_delta(&delta)
                        .unwrap_or_else(|err| panic!("seat {seat}'s delta applies: {err}"));
                    self.deltas_applied += 1;
                }
            }
        }
        self.consumed = frames.len();
    }

    /// The applied snapshot equals the server's capture for this seat, section for section.
    fn assert_matches(&mut self, seat: FactionId, captured: &WorldSnapshot, when: &str) {
        let held = self
            .held
            .as_ref()
            .unwrap_or_else(|| panic!("seat {seat} holds a snapshot {when}"));
        assert_tiles_match_within_the_deadband(seat, &held.tiles, &captured.tiles, when);
        let raw_matches = unsorted(held) == unsorted(captured);
        if let Some(drift) = first_difference(&canonical(held), &canonical(captured), "") {
            panic!(
                "seat {seat}'s applied snapshot drifted from the server's capture {when}: {drift}"
            );
        }
        self.order_only_differences += usize::from(!raw_matches);
    }
}

fn push_one_event_per_seat(app: &mut App, turn: usize) {
    let tick = app.world.resource::<SimulationTick>().0;
    let mut log = app.world.resource_mut::<CommandEventLog>();
    for seat in [HOME, RIVAL] {
        log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Forage,
            seat,
            format!("seat {seat} turn {turn}"),
            None,
        ));
    }
}

/// ⛔ Full frame → delta → delta … for both seats, matching the server's own capture after every
/// publication, across a feed that overflows its window and a mid-tick recapture pair.
#[test]
fn the_applied_stream_equals_the_servers_capture_after_every_publication() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    for (faction, stock) in [(HOME, HOME_STOCK), (RIVAL, RIVAL_STOCK)] {
        app.world.resource_mut::<FactionInventory>().add_stockpile(
            faction,
            STOCK_ITEM.to_string(),
            stock,
        );
    }
    app.world
        .resource_mut::<SnapshotAudiences>()
        .set(vec![HOME, RIVAL]);
    app.world
        .resource_mut::<CommandEventLog>()
        .set_retention_turns(RETENTION_TURNS);
    // The boot `update` already published to the viewer seat, so its stream is mid-chain before
    // any sink exists. Dropping both seats' publication state is what a fresh claim does, and it
    // is what makes each seat's first RECORDED frame the full baseline a real client starts from.
    for seat in [HOME, RIVAL] {
        app.world
            .resource_mut::<SnapshotHistory>()
            .drop_audience(seat);
    }
    let recorder = Arc::new(Recorder::default());
    app.world
        .resource::<SnapshotHistory>()
        .attach_sink(Arc::clone(&recorder) as Arc<dyn FrameSink>);

    let mut clients = [
        (HOME, SeatClient::default()),
        (RIVAL, SeatClient::default()),
    ];
    let sync = |app: &App, when: &str, clients: &mut [(FactionId, SeatClient); 2]| {
        drain_publisher(app);
        for (seat, client) in clients.iter_mut() {
            client.catch_up(*seat, &recorder.for_seat(*seat));
            client.assert_matches(*seat, &captured_for(app, *seat), when);
        }
    };

    for turn in 0..TURNS {
        push_one_event_per_seat(&mut app, turn);
        run_turn(&mut app);
        sync(&app, &format!("after turn {turn}"), &mut clients);

        if turn == RECAPTURE_TURN {
            // A world-mutating command between turns publishes a HELD frame; a second one
            // restates what the first carried. Both must merge to the capture, and the
            // restatement must be a no-op on top of the first.
            app.world.resource_mut::<FactionInventory>().add_stockpile(
                HOME,
                STOCK_ITEM.to_string(),
                RECAPTURE_STOCK,
            );
            recapture_snapshot_in_place(&mut app.world);
            sync(&app, "after the mid-tick recapture", &mut clients);

            recapture_snapshot_in_place(&mut app.world);
            sync(&app, "after the restating recapture", &mut clients);

            // Idempotency on real data: the restating delta, applied once more against the frame
            // it produced, changes nothing but the frame number.
            for (seat, client) in clients.iter_mut() {
                let frames = recorder.for_seat(*seat);
                let FramePayload::Delta(mut restated) =
                    decode_frame_flatbuffer(frames.last().expect("a frame")).expect("decodes")
                else {
                    panic!("seat {seat}'s restating recapture must be a delta");
                };
                let held = client.held.as_mut().expect("holds a snapshot");
                let before = canonical(held);
                let tiles_before = held.tiles.clone();
                restated.header.base_frame_seq = held.header.frame_seq;
                restated.header.frame_seq = held.header.frame_seq + 1;
                held.apply_delta(&restated).expect("applies again");
                held.header = captured_for(&app, *seat).header.clone();
                if let Some(drift) = first_difference(&canonical(held), &before, "") {
                    panic!(
                        "seat {seat}: re-applying the restating delta changed the world: {drift}"
                    );
                }
                assert_eq!(
                    held.tiles, tiles_before,
                    "seat {seat}: re-applying the restating delta moved a tile"
                );
            }
        }
    }

    for (seat, client) in &clients {
        assert!(
            client.deltas_applied >= TURNS,
            "seat {seat} applied only {} deltas over {TURNS} turns",
            client.deltas_applied
        );
        assert!(
            client.deltas_with_events > 0,
            "seat {seat} was never sent a feed delta, so the append rule was not exercised"
        );
        let held = client.held.as_ref().expect("holds a snapshot");
        assert!(
            held.command_events.len() < TURNS,
            "seat {seat} holds {} feed rows after {TURNS} turns of one row each: the {RETENTION_TURNS}-turn window never evicted",
            held.command_events.len()
        );
        assert!(
            held.command_events
                .iter()
                .all(|event| event.faction == seat.0),
            "seat {seat}'s feed carries another people's rows"
        );
        eprintln!(
            "seat {seat}: {} deltas applied, {} carried feed rows, {} carried removals, {} publications differed only in keyed-row order",
            client.deltas_applied,
            client.deltas_with_events,
            client.deltas_with_removals,
            client.order_only_differences
        );
    }
}
