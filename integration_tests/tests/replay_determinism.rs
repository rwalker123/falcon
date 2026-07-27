//! Determinism under **replay**, which is a strictly harder bar than the one
//! `determinism.rs` holds.
//!
//! `determinism.rs` compares two fresh runs from tick 0. That pins the *forward* simulation, and
//! [`forward_determinism_is_bit_exact`] below asserts the same property through this file's
//! localizing differ so a failure names the field instead of two hashes. What it cannot see is the
//! **restore** direction: whether a world rebuilt from a checkpoint is the world that produced the
//! checkpoint, and whether marching it forward reproduces the ticks that already happened. That is
//! the property a command-sourced rollback (a command log plus sparse checkpoints, replayed
//! forward) rests on entirely, and today it does not hold.
//!
//! The two `#[ignore]`d tests are the oracle for that work, in increasing strength:
//!
//! - [`checkpoint_restore_is_lossless`] catches state that `capture_snapshot` **publishes** but
//!   `restore_world_from_snapshot` does not put back.
//! - [`checkpoint_replay_is_bit_exact`] catches state that merely **influences** what is published.
//!   That is the more valuable half: a dozen mutable resources (`ActiveCrisisLedger`,
//!   `PowerTopology`, `ObservationLedger`, the espionage set, …) have no representation in
//!   `WorldSnapshot` at all, so the round-trip test above is blind to them while this one is not.
//!
//! Together they are meant to be an oracle that fails **automatically** on an omission, rather than
//! a checklist someone has to remember to extend when they add a resource — which is the failure
//! mode the current design has, silently.

mod common;

use core_sim::sim_state::{capture_sim_state, restore_sim_state, SimState};
use core_sim::{
    build_headless_app, recapture_snapshot_in_place, SimulationConfig, SimulationConfigMetadata,
    SnapshotHistory,
};
use serde_json::Value;
use sim_schema::world::WorldSnapshot;
use std::collections::BTreeMap;

/// Turns to run before taking the checkpoint. Enough that the world is past its seeded initial
/// state (bands have moved, herds have grazed, culture layers exist) without making the test slow.
const CHECKPOINT_TICKS: usize = 6;
/// Turns to march past the checkpoint, both on the baseline run and on the replay.
const REPLAY_TICKS: usize = 4;
/// Turns for the forward-determinism comparison. Longer than the checkpoint horizon because
/// divergence in the forward sim tends to be a slow float drift rather than an immediate flip.
const FORWARD_TICKS: usize = 12;
/// Fallback map seed when the loaded config leaves it unset, matching `determinism.rs`.
const FALLBACK_MAP_SEED: u64 = 0x5EED_F00D;
/// Longest rendered value in a diff line, so one enormous string field cannot bury the report.
const MAX_VALUE_CHARS: usize = 48;

fn new_app() -> bevy::prelude::App {
    common::ensure_test_config();
    let mut app = build_headless_app();
    if let Some(mut metadata) = app.world.get_resource_mut::<SimulationConfigMetadata>() {
        metadata.set_seed_random(false);
    }
    if let Some(mut config) = app.world.get_resource_mut::<SimulationConfig>() {
        if config.map_seed == 0 {
            config.map_seed = FALLBACK_MAP_SEED;
        }
    }
    app
}

fn latest_snapshot(app: &bevy::prelude::App) -> WorldSnapshot {
    app.world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .map(|snapshot| (*snapshot).clone())
        .expect("snapshot available")
}

/// Take a checkpoint. Deliberately `SimState`, not `WorldSnapshot`: the snapshot is the client's
/// view and restoring from it is the defect this arc removed.
fn checkpoint(app: &bevy::prelude::App) -> SimState {
    capture_sim_state(&app.world)
}

fn restore_to(app: &mut bevy::prelude::App, checkpoint: &SimState) {
    restore_sim_state(&mut app.world, checkpoint);
}

// ---------------------------------------------------------------------------------------------
// Localizing differ
// ---------------------------------------------------------------------------------------------

/// The result of walking two snapshots together: what was compared, what differed, and what could
/// not be compared at all.
///
/// **A difference count on its own is unreadable, and was actively misleading here.** "12,204
/// differing leaves" moves when correctness changes *and* when the set of comparable leaves
/// changes, and one integer cannot tell those apart — a fix that re-aligns two collections can
/// raise the count by making thousands of leaves comparable that previously were not. Every figure
/// this file reports is therefore a fraction with the denominator shown.
///
/// [`Self::asymmetric`] is the other half of the same problem. A leaf on one side and not the other
/// is not a difference, it is a *failure to compare*, and counting it as either would hide it. Those
/// are the cases where "the number went down" can mean "we stopped looking", so they are reported
/// separately and asserted on separately.
#[derive(Default)]
struct Comparison {
    /// Index-stripped path -> (leaves compared, of which differing).
    per_shape: BTreeMap<String, (usize, usize)>,
    /// One example line per differing shape.
    samples: BTreeMap<String, String>,
    /// Structural mismatches that stopped a comparison happening at all.
    asymmetric: Vec<String>,
}

impl Comparison {
    fn leaf(&mut self, path: &str, left: &Value, right: &Value) {
        let shape = shape_of(path);
        let entry = self.per_shape.entry(shape.clone()).or_default();
        entry.0 += 1;
        if left != right {
            entry.1 += 1;
            self.samples.entry(shape).or_insert_with(|| {
                format!(
                    "{path}: {} != {}",
                    truncate(&left.to_string()),
                    truncate(&right.to_string())
                )
            });
        }
    }

    fn compared(&self) -> usize {
        self.per_shape.values().map(|(total, _)| total).sum()
    }

    fn differing(&self) -> usize {
        self.per_shape.values().map(|(_, bad)| bad).sum()
    }

    fn is_identical(&self) -> bool {
        self.differing() == 0 && self.asymmetric.is_empty()
    }

    fn render(&self) -> String {
        let compared = self.compared();
        let differing = self.differing();
        let percent = if compared == 0 {
            0.0
        } else {
            (differing as f64 / compared as f64) * 100.0
        };
        let mut out = format!(
            "{differing} of {compared} compared leaves differ ({percent:.2}%); \
             {} asymmetric (not compared)",
            self.asymmetric.len()
        );
        for (shape, (total, bad)) in &self.per_shape {
            if *bad == 0 {
                continue;
            }
            let sample = self.samples.get(shape).map(String::as_str).unwrap_or("");
            out.push_str(&format!("\n  [{bad:>5} / {total:>5}] {shape}  |  {sample}"));
        }
        if !self.asymmetric.is_empty() {
            out.push_str("\n  NOT COMPARED (present on one side only, or a length mismatch):");
            for line in &self.asymmetric {
                out.push_str(&format!("\n    {line}"));
            }
        }
        out
    }
}

/// Walk two serialized snapshots together.
///
/// A bare `assert_eq!` on `WorldSnapshot` is useless — the payload is tens of thousands of leaves
/// and the failure would be two unreadable blobs. A failure here names *which field* drifted and
/// out of how many, because that list is the work item.
fn compare(left: &Value, right: &Value, path: &str, out: &mut Comparison) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys() {
                match b.get(key) {
                    Some(other) => compare(&a[key], other, &format!("{path}.{key}"), out),
                    None => out
                        .asymmetric
                        .push(format!("{path}.{key}: present on the left only")),
                }
            }
            for key in b.keys().filter(|key| !a.contains_key(*key)) {
                out.asymmetric
                    .push(format!("{path}.{key}: present on the right only"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            // Compare the common prefix rather than bailing out. Stopping at a length mismatch
            // discarded every element the two sides *do* share, which is most of the signal.
            for (index, (x, y)) in a.iter().zip(b.iter()).enumerate() {
                compare(x, y, &format!("{path}[{index}]"), out);
            }
            if a.len() != b.len() {
                let skipped = a.len().abs_diff(b.len());
                out.asymmetric.push(format!(
                    "{path}: length {} vs {} — {skipped} element(s) not compared",
                    a.len(),
                    b.len()
                ));
            }
        }
        // `Option::None` serializes to `null`. Against a value it is a structural mismatch, not a
        // difference in a shared field, so it is surfaced rather than folded into the count.
        (Value::Null, other) if !other.is_null() => out.asymmetric.push(format!(
            "{path}: null on the left, {} on the right",
            truncate(&other.to_string())
        )),
        (other, Value::Null) if !other.is_null() => out.asymmetric.push(format!(
            "{path}: {} on the left, null on the right",
            truncate(&other.to_string())
        )),
        _ => out.leaf(path, left, right),
    }
}

fn truncate(value: &str) -> String {
    if value.len() > MAX_VALUE_CHARS {
        format!("{}…", &value[..MAX_VALUE_CHARS])
    } else {
        value.to_string()
    }
}

/// Collapse `foo[3].bar[7].baz` to `foo[].bar[].baz` so per-element results aggregate into one row
/// carrying a count, a denominator and a sample, instead of thousands of lines.
fn shape_of(path: &str) -> String {
    let mut shape = String::new();
    let mut inside_index = false;
    for ch in path.chars() {
        match ch {
            '[' => {
                inside_index = true;
                shape.push_str("[]");
            }
            ']' => inside_index = false,
            other if !inside_index => shape.push(other),
            _ => {}
        }
    }
    shape
}

// ---------------------------------------------------------------------------------------------
// Canonicalization
// ---------------------------------------------------------------------------------------------

/// Serialize a snapshot with **entity identity canonicalized away**.
///
/// `restore_world_from_snapshot` despawns and respawns every tile, link and cohort, so bevy hands
/// back fresh `Entity` generations. `capture_snapshot` publishes tiles sorted by entity bits, which
/// means a restored world emits the *same* tiles in a *different* order — roughly 6,300 spurious
/// leaves that drown every real divergence. Re-keying on world coordinates removes that noise.
///
/// Entity renumbering is itself a genuine rollback defect (local culture layers are keyed by
/// `CultureOwner(entity.to_bits())` and are orphaned by it), but it is a separate, structural piece
/// of work: it needs a stable sim-level id, and that changes the wire. **Canonicalizing here is not
/// forgiving it** — it is keeping this oracle pointed at value fidelity so both can be worked on
/// without one hiding the other.
fn canonical(snapshot: &WorldSnapshot) -> Value {
    let mut snapshot = snapshot.clone();
    // Publication bookkeeping, not simulation state: these count frames on the wire and legitimately
    // differ between a baseline run and a replay of it.
    snapshot.header.frame_seq = 0;
    snapshot.header.base_frame_seq = 0;
    snapshot.header.world_epoch = 0;
    snapshot.header.hash = 0;

    let tile_coords: BTreeMap<u64, (u32, u32)> = snapshot
        .tiles
        .iter()
        .map(|tile| (tile.entity, (tile.x, tile.y)))
        .collect();

    snapshot.tiles.sort_by_key(|tile| (tile.y, tile.x));
    let mut value = serde_json::to_value(&snapshot).expect("snapshot serializes");

    blank_fields(&mut value, "tiles", &["entity"]);
    // Only `entity` is blanked here. `node_id` is NOT — it is `PowerNodeId(y * width + x)`
    // (`systems/worldgen.rs`), a linear index off the tile's position, so it is already a stable
    // id and a restore that renumbers entities must still reproduce it exactly. Blanking it would
    // hide a field the oracle exists to check.
    rekey_by_tile(&mut value, "power", "entity", &tile_coords, &["entity"]);
    rekey_by_tile(
        &mut value,
        "logistics",
        "from",
        &tile_coords,
        &["entity", "from", "to"],
    );
    sort_populations(&mut value);
    // Local culture layers are owned by an entity, addressed by its bits.
    blank_fields(&mut value, "culture_layers", &["owner"]);
    value
}

fn blank_fields(value: &mut Value, section: &str, fields: &[&str]) {
    let Some(rows) = value.get_mut(section).and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows.iter_mut() {
        for field in fields {
            row[*field] = Value::from(0u64);
        }
    }
}

fn rekey_by_tile(
    value: &mut Value,
    section: &str,
    tile_ref: &str,
    tile_coords: &BTreeMap<u64, (u32, u32)>,
    blank: &[&str],
) {
    let Some(rows) = value.get_mut(section).and_then(Value::as_array_mut) else {
        return;
    };
    rows.sort_by_key(|row| {
        let entity = row[tile_ref].as_u64().unwrap_or_default();
        tile_coords
            .get(&entity)
            .copied()
            .unwrap_or((u32::MAX, u32::MAX))
    });
    for row in rows.iter_mut() {
        for field in blank {
            row[*field] = Value::from(0u64);
        }
    }
}

fn sort_populations(value: &mut Value) {
    let Some(rows) = value.get_mut("populations").and_then(Value::as_array_mut) else {
        return;
    };
    rows.sort_by_key(|row| {
        (
            row["current_x"].as_u64().unwrap_or_default(),
            row["current_y"].as_u64().unwrap_or_default(),
            row["size"].as_u64().unwrap_or_default(),
        )
    });
    for row in rows.iter_mut() {
        row["entity"] = Value::from(0u64);
        row["home"] = Value::from(0u64);
        row["home_band_entity"] = Value::from(0u64);
    }
}

fn assert_snapshots_match(label: &str, expected: &WorldSnapshot, actual: &WorldSnapshot) {
    let mut comparison = Comparison::default();
    compare(
        &canonical(expected),
        &canonical(actual),
        "",
        &mut comparison,
    );
    assert!(
        comparison.is_identical(),
        "{label} (entity identity canonicalized): {}",
        comparison.render()
    );
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

/// Two independent runs of the same seed agree, field for field.
///
/// `determinism.rs` asserts the same thing via `hash_snapshot`; this states it through the
/// localizing differ so a regression arrives naming the field rather than as two unequal `u64`s.
#[test]
fn forward_determinism_is_bit_exact() {
    let mut first = new_app();
    let mut second = new_app();
    for _ in 0..FORWARD_TICKS {
        first.update();
    }
    for _ in 0..FORWARD_TICKS {
        second.update();
    }
    assert_snapshots_match(
        "two fresh runs of the same seed diverged",
        &latest_snapshot(&first),
        &latest_snapshot(&second),
    );
}

/// Restoring a checkpoint reproduces the world that checkpoint was taken from.
///
/// Ignored: fails today by ~1,000 canonicalized leaves. `WorldSnapshot` is the **client view** —
/// fog-filtered herds, derived rasters, display-only readouts — and it is being used as a save
/// state, so restoring from it cannot recover what it never carried. Turn this on when the
/// checkpoint becomes a dedicated full-world payload rather than the client's snapshot.
#[test]
fn checkpoint_restore_is_lossless() {
    let mut app = new_app();
    for _ in 0..CHECKPOINT_TICKS {
        app.update();
    }
    let state = checkpoint(&app);
    let expected = latest_snapshot(&app);

    // March past the checkpoint first, so the world genuinely holds later state that the restore
    // has to undo. Restoring into an untouched world would pass without proving anything.
    for _ in 0..REPLAY_TICKS {
        app.update();
    }

    restore_to(&mut app, &state);
    recapture_snapshot_in_place(&mut app.world);

    assert_snapshots_match(
        "restoring a checkpoint did not reproduce it",
        &expected,
        &latest_snapshot(&app),
    );
}

/// A restored checkpoint marched forward reproduces the ticks that already happened.
///
/// This is the property command-sourced rollback needs, and the stronger of the two oracles: it
/// sees state that never reaches the wire but still steers the simulation.
///
/// The last thing it caught was not a checkpoint bug at all: `simulate_logistics` ordered its mass
/// transfers by `Entity::to_bits()`, so a restore — which renumbers every entity — moved mass in a
/// different order and every tile landed on a different value. Entity ids are stable within one
/// process run, which is why the forward-determinism tests never saw it. That system now sorts on
/// the links' endpoint positions, the same natural key this checkpoint stores them under.
#[test]
fn checkpoint_replay_is_bit_exact() {
    let mut app = new_app();
    for _ in 0..CHECKPOINT_TICKS {
        app.update();
    }
    let state = checkpoint(&app);

    let mut baseline = Vec::with_capacity(REPLAY_TICKS);
    for _ in 0..REPLAY_TICKS {
        app.update();
        baseline.push(latest_snapshot(&app));
    }

    restore_to(&mut app, &state);
    // Compare every replayed tick, not just the last: the first divergence is the one worth
    // reading, and a later tick's report is that drift plus everything it has since fed.
    for (step, expected) in baseline.iter().enumerate() {
        app.update();
        assert_snapshots_match(
            &format!(
                "replay diverged at step {} (tick {})",
                step + 1,
                expected.header.tick
            ),
            expected,
            &latest_snapshot(&app),
        );
    }
}

/// **A rollback to tick T produces the world T had, and the frame the client receives is derived
/// from that world** — the *dense* path, where T has a checkpoint of its own and nothing replays.
///
/// Its sparse counterpart is `rolling_back_to_a_non_checkpoint_tick_reproduces_that_tick`. Both are
/// kept: `checkpoint_interval = 1` is a supported setting (restore every tick, never replay), and
/// it is the configuration in which a restore bug cannot hide behind a replay that re-derives the
/// state anyway.
///
/// This is the whole guarantee, and it is stronger than the tick-agreement property it replaces.
/// That earlier version asserted two parallel histories filed under matching ticks — which was the
/// right assertion while `handle_rollback` read a stored snapshot to re-baseline the client. It no
/// longer does: it recaptures the frame from the restored world, so there is one history of worlds
/// and nothing left to disagree. What matters now is that the ring hands back the right checkpoint
/// and that recapturing it reproduces the published view, which is what this checks end to end.
#[test]
fn a_rollback_produces_the_world_that_tick_had() {
    use core_sim::sim_state::CheckpointHistory;

    let mut app = new_app();
    if let Some(mut config) = app.world.get_resource_mut::<SimulationConfig>() {
        config.checkpoint_interval = 1;
    }
    for _ in 0..CHECKPOINT_TICKS {
        app.update();
    }
    let target_tick = latest_snapshot(&app).header.tick;
    let published = latest_snapshot(&app);

    // March on, so the restore has a genuinely different world to undo.
    for _ in 0..REPLAY_TICKS {
        app.update();
    }

    let state = app
        .world
        .resource::<CheckpointHistory>()
        .entry(target_tick)
        .expect("the rollback ring holds a checkpoint for every recent tick");
    restore_sim_state(&mut app.world, state.as_ref());
    recapture_snapshot_in_place(&mut app.world);

    assert_snapshots_match(
        "rolling back to a tick did not reproduce the world that tick had",
        &published,
        &latest_snapshot(&app),
    );
}

/// **Rolling back to a tick with no checkpoint of its own reproduces that tick exactly.**
///
/// This is the whole of sparse checkpointing. Checkpoints are taken every
/// `SimulationConfig::checkpoint_interval` turns, so most ticks have none — restoring one of those
/// means restoring the nearest checkpoint *before* it and replaying forward, and the world that
/// comes out has to be the world that tick originally had, not merely a plausible one.
///
/// Without this the replay-forward path is unguarded: `checkpoint_replay_is_bit_exact` proves a
/// replayed turn is exact, but only this proves the rollback path picks the right checkpoint and
/// replays the right number of turns.
#[test]
fn rolling_back_to_a_non_checkpoint_tick_reproduces_that_tick() {
    use core_sim::sim_state::{CheckpointHistory, Replaying};

    const INTERVAL: u64 = 4;
    let mut app = new_app();
    if let Some(mut config) = app.world.get_resource_mut::<SimulationConfig>() {
        config.checkpoint_interval = INTERVAL;
    }

    // Run to a tick that is deliberately NOT a multiple of the interval.
    let mut published = Vec::new();
    for _ in 0..(INTERVAL * 3 + 1) {
        app.update();
        published.push(latest_snapshot(&app));
    }
    let target = published.last().expect("published frames").header.tick;
    assert_ne!(
        target % INTERVAL,
        0,
        "the test target must be a tick with no checkpoint of its own, or it proves nothing"
    );
    let expected = published.last().expect("published frames").clone();

    // March on, so the rollback has a genuinely later world to undo.
    for _ in 0..INTERVAL {
        app.update();
    }

    let (checkpoint_tick, state) = app
        .world
        .resource::<CheckpointHistory>()
        .nearest_at_or_before(target)
        .expect("a checkpoint at or before the target");
    assert!(
        checkpoint_tick < target,
        "the nearest checkpoint must be strictly before the target, or no replay happens and this \
         test degenerates into `checkpoint_restore_is_lossless`"
    );

    restore_sim_state(&mut app.world, state.as_ref());
    app.world.resource_mut::<Replaying>().0 = true;
    let ring_before = app.world.resource::<CheckpointHistory>().len();
    for _ in checkpoint_tick..target {
        core_sim::run_turn(&mut app);
    }
    app.world.resource_mut::<Replaying>().0 = false;
    assert_eq!(
        app.world.resource::<CheckpointHistory>().len(),
        ring_before,
        "replaying forward pushed entries into the ring it was rewinding — a rollback must not grow \
         its own history"
    );

    recapture_snapshot_in_place(&mut app.world);
    assert_snapshots_match(
        "rolling back to a non-checkpoint tick did not reproduce that tick",
        &expected,
        &latest_snapshot(&app),
    );
}
