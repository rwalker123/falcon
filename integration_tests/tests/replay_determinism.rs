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

use core_sim::{
    build_headless_app, recapture_snapshot_in_place, restore_world_from_snapshot, SimulationConfig,
    SimulationConfigMetadata, SimulationTick, SnapshotHistory,
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

fn restore_to(app: &mut bevy::prelude::App, checkpoint: &WorldSnapshot) {
    restore_world_from_snapshot(&mut app.world, checkpoint);
    let mut tick = app.world.resource_mut::<SimulationTick>();
    tick.0 = checkpoint.header.tick;
}

// ---------------------------------------------------------------------------------------------
// Localizing differ
// ---------------------------------------------------------------------------------------------

/// Walk two serialized snapshots together and collect a `path: a != b` line per differing leaf.
///
/// A bare `assert_eq!` on `WorldSnapshot` is useless here — the payload is tens of thousands of
/// leaves and the failure output would be two unreadable blobs. The whole point of this file is
/// that a failure names *which field* drifted, because that list is the work item.
fn collect_diffs(a: &Value, b: &Value, path: &str, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(left), Value::Object(right)) => {
            let mut keys: Vec<&String> = left.keys().collect();
            keys.extend(right.keys().filter(|key| !left.contains_key(*key)));
            for key in keys {
                let null = Value::Null;
                collect_diffs(
                    left.get(key).unwrap_or(&null),
                    right.get(key).unwrap_or(&null),
                    &format!("{path}.{key}"),
                    out,
                );
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                out.push(format!("{path}: LEN {} != {}", left.len(), right.len()));
                return;
            }
            for (index, (x, y)) in left.iter().zip(right.iter()).enumerate() {
                collect_diffs(x, y, &format!("{path}[{index}]"), out);
            }
        }
        _ => {
            if a != b {
                out.push(format!(
                    "{path}: {} != {}",
                    truncate(&a.to_string()),
                    truncate(&b.to_string())
                ));
            }
        }
    }
}

fn truncate(value: &str) -> String {
    if value.len() > MAX_VALUE_CHARS {
        format!("{}…", &value[..MAX_VALUE_CHARS])
    } else {
        value.to_string()
    }
}

/// Collapse `foo[3].bar[7].baz` to `foo[].bar[].baz` so thousands of per-element diffs report as
/// one row with a count and a sample, instead of thousands of lines.
fn shape_of(line: &str) -> String {
    let path = line.split(':').next().unwrap_or(line);
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

fn render(diffs: &[String]) -> String {
    let mut by_shape: BTreeMap<String, (usize, &String)> = BTreeMap::new();
    for line in diffs {
        let entry = by_shape.entry(shape_of(line)).or_insert((0, line));
        entry.0 += 1;
    }
    by_shape
        .iter()
        .map(|(shape, (count, sample))| format!("  [{count:>5}] {shape}  |  {sample}"))
        .collect::<Vec<_>>()
        .join("\n")
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
    let mut diffs = Vec::new();
    collect_diffs(&canonical(expected), &canonical(actual), "", &mut diffs);
    assert!(
        diffs.is_empty(),
        "{label}: {} differing leaves (entity identity canonicalized)\n{}",
        diffs.len(),
        render(&diffs)
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
#[ignore = "restore from WorldSnapshot is lossy; awaiting the dedicated full-world checkpoint"]
fn checkpoint_restore_is_lossless() {
    let mut app = new_app();
    for _ in 0..CHECKPOINT_TICKS {
        app.update();
    }
    let checkpoint = latest_snapshot(&app);

    // March past the checkpoint first, so the world genuinely holds later state that the restore
    // has to undo. Restoring into an untouched world would pass without proving anything.
    for _ in 0..REPLAY_TICKS {
        app.update();
    }

    restore_to(&mut app, &checkpoint);
    recapture_snapshot_in_place(&mut app.world);

    assert_snapshots_match(
        "restoring a checkpoint did not reproduce it",
        &checkpoint,
        &latest_snapshot(&app),
    );
}

/// A restored checkpoint marched forward reproduces the ticks that already happened.
///
/// This is the property command-sourced rollback needs, and the stronger of the two oracles: it
/// sees state that never reaches the wire but still steers the simulation. Ignored for the same
/// reason as [`checkpoint_restore_is_lossless`], and it cannot pass before that one does.
#[test]
#[ignore = "replay diverges from unrestored sim state; awaiting the dedicated full-world checkpoint"]
fn checkpoint_replay_is_bit_exact() {
    let mut app = new_app();
    for _ in 0..CHECKPOINT_TICKS {
        app.update();
    }
    let checkpoint = latest_snapshot(&app);

    let mut baseline = Vec::with_capacity(REPLAY_TICKS);
    for _ in 0..REPLAY_TICKS {
        app.update();
        baseline.push(latest_snapshot(&app));
    }

    restore_to(&mut app, &checkpoint);
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
