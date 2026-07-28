//! Determinism under **replay**, which is a strictly harder bar than the one
//! `determinism.rs` holds.
//!
//! `determinism.rs` compares two fresh runs from tick 0. That pins the *forward* simulation, and
//! [`forward_determinism_is_bit_exact`] below asserts the same property through this file's
//! localizing differ so a failure names the field instead of two hashes. What it cannot see is the
//! **restore** direction: whether a world rebuilt from a checkpoint is the world that produced the
//! checkpoint, and whether marching it forward reproduces the ticks that already happened. That is
//! the property a command-sourced rollback rests on entirely. **It holds**, and nothing here is
//! `#[ignore]`d — a regression fails the build rather than sitting in a disabled test.
//!
//! The two round-trip oracles, in increasing strength:
//!
//! - [`checkpoint_restore_is_lossless`] catches state a restore does not put back, as far as the
//!   **published frame** can see it.
//! - [`a_restored_world_simulates_forward_identically`] catches state that never reaches the wire
//!   but still **influences** what is published. That is the more valuable half: a dozen mutable
//!   resources (`ActiveCrisisLedger`, `PowerTopology`, the espionage set, …) have no representation
//!   in `WorldSnapshot` at all, so the round-trip test above is blind to them while this one is not.
//!
//! Both restore from **`SimState`** (`core_sim/src/sim_state.rs`) and never from the published
//! `WorldSnapshot`: the snapshot is the client's view — fog-filtered herds, derived rasters,
//! display-only readouts — and using it as a save state is the defect this arc removed.
//!
//! **These are behavioural oracles, and the structural half lives elsewhere.** They prove that
//! restoring reproduces *this* world; they cannot prove a newly added resource is carried, because
//! state that nothing exercises within [`REPLAY_TICKS`] diverges in neither direction.
//! `core_sim/tests/sim_state_coverage.rs` is the complement — it enumerates a built app's resources
//! and components at runtime and fails until each is classified, so an omission arrives as a test
//! failure instead of silently.

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

/// Cut a rendered value to [`MAX_VALUE_CHARS`] **characters**.
///
/// Chars, not bytes, and that is not pedantry: the values walked here include narrative prose
/// (`BeatVoiceLineState.text`, and `core_sim/src/data/beat_definitions.json` is full of 3-byte
/// em-dashes), so a byte index lands mid-codepoint and panics. This code path exists to *localize a
/// determinism failure* — a panic here replaces the diff someone needed with a stack trace.
fn truncate(value: &str) -> String {
    let mut cut: String = value.chars().take(MAX_VALUE_CHARS).collect();
    if value.chars().nth(MAX_VALUE_CHARS).is_some() {
        cut.push('…');
    }
    cut
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
/// `restore_sim_state` despawns and respawns every tile, link and cohort, so bevy hands
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
    // `owner` is blanked, but **no longer because it carries entity bits** — `CultureOwner` is
    // derived from tile position, region id or `BandId` now, none of which a restore renumbers, and
    // probing the compared frames finds only `TILE_OWNER_TAG | y << 32 | x` values and small region
    // ids across all 390 rows. Removing the blank keeps every test here green.
    //
    // It stays because that has not been *established*: these rows come from `CultureManager.locals`,
    // a `HashMap`, so their order may be only incidentally aligned between the two frames — which
    // would make an un-blanked `owner` flaky rather than stronger, and a flaky determinism oracle is
    // worse than a slightly loose one. Ordering the rows is what would let this blank go.
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

/// The differ's own truncation survives multi-byte text.
///
/// A one-line guard for a one-line fix, because the byte-indexed version it replaces was reachable
/// from real data and would regress silently: nothing else in this file renders a long string, so
/// the panic would only ever appear the day a determinism test failed on a narrative field — the
/// worst possible moment to lose the diff.
#[test]
fn truncate_cuts_on_a_char_boundary() {
    // An em-dash is 3 bytes, so a byte-indexed cut at MAX_VALUE_CHARS lands mid-codepoint.
    let prose = "—".repeat(MAX_VALUE_CHARS * 2);
    let cut = truncate(&prose);
    assert_eq!(
        cut.chars().count(),
        MAX_VALUE_CHARS + 1,
        "expected {MAX_VALUE_CHARS} characters plus the ellipsis, got {cut}"
    );
    assert!(cut.ends_with('…'), "a truncated value must say it was cut");

    // A value at the limit is returned whole, multi-byte or not.
    let exact = "—".repeat(MAX_VALUE_CHARS);
    assert_eq!(truncate(&exact), exact);
}

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
/// It failed by ~1,000 canonicalized leaves for as long as the checkpoint *was* the published
/// `WorldSnapshot`: a client view cannot restore what it never carried. It restores from `SimState`
/// now and holds exactly, which is why it runs rather than being `#[ignore]`d.
///
/// It is the weaker of the two round-trip oracles — it can only see divergence that reaches the
/// published frame. [`a_restored_world_simulates_forward_identically`] covers the rest.
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

/// **A restored world simulates forward identically** to the world it was restored from — tick for
/// tick, from the tick it was restored at.
///
/// It is the stronger of the two round-trip oracles: it sees state that never reaches the wire but
/// still steers the simulation, which the restore-fidelity test above is blind to.
///
/// **It does not cover the rollback path's replay-forward.** It restores a checkpoint taken at the
/// tick it starts marching from, so deleting the replay loop in `handle_rollback` changes nothing
/// here — a survey confirmed it still passes with that loop removed. It was named
/// `checkpoint_replay_is_bit_exact`, and that name is what made the sparse-checkpoint command
/// defect easy to miss: the oracle trusted most had a blind spot exactly where the new risk was, and
/// its name said otherwise. The tests that *do* cover replay-forward are
/// `a_rollback_across_a_command_reproduces_the_world_that_tick_had` and
/// `a_rollback_still_reaches_an_early_tick_after_many_commands`, both in
/// `core_sim/src/bin/server.rs` — the rollback path replays a **command log**, and commands live
/// there.
///
/// The last thing it caught was not a checkpoint bug at all: `simulate_logistics` ordered its mass
/// transfers by `Entity::to_bits()`, so a restore — which renumbers every entity — moved mass in a
/// different order and every tile landed on a different value. Entity ids are stable within one
/// process run, which is why the forward-determinism tests never saw it. That system now sorts on
/// the links' endpoint positions, the same natural key this checkpoint stores them under.
#[test]
fn a_restored_world_simulates_forward_identically() {
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

/// **A recapture during replay publishes nothing.**
///
/// `capture_snapshot` is gated on `not_replaying` in the turn schedule, but
/// `recapture_snapshot_in_place` reaches it through `World::run_system_once`, which invokes the
/// system directly — **run conditions are a schedule feature and do not apply**. So the schedule's
/// gate protects the turn path and nothing else, and any caller that recaptures mid-replay would
/// broadcast a frame the client must not see: a rollback is one publication.
///
/// The gate therefore lives *inside* `recapture_snapshot_in_place`, covering every caller rather
/// than the call sites anyone happened to think of. This test is what says the bypass is real
/// rather than theoretical — it fails if the check is removed.
#[test]
fn a_recapture_while_replaying_publishes_nothing() {
    use core_sim::sim_state::Replaying;

    let mut app = new_app();
    for _ in 0..CHECKPOINT_TICKS {
        app.update();
    }
    let before = latest_snapshot(&app).header.frame_seq;

    app.world.resource_mut::<Replaying>().0 = true;
    recapture_snapshot_in_place(&mut app.world);
    let during = latest_snapshot(&app).header.frame_seq;
    app.world.resource_mut::<Replaying>().0 = false;

    assert_eq!(
        before, during,
        "a recapture published a frame while replaying — the schedule's `not_replaying` gate does \
         not cover `run_system_once`, so the check has to be inside the function"
    );

    // ...and still works normally once replay is over, or the gate would be a mute button.
    recapture_snapshot_in_place(&mut app.world);
    assert!(
        latest_snapshot(&app).header.frame_seq > during,
        "recapture stopped publishing outside replay"
    );
}
