//! `SimState` encodes and decodes without losing anything — the save format's smoke test.
//!
//! ## Why this compares BYTES rather than values
//!
//! The obvious test is `assert_eq!(original, decoded)`, and it would be **wrong here** for a
//! specific, checked reason: `LaborAllocation` has a hand-written `PartialEq` that compares its
//! assignments, its upkeep mode and its build queue — and **none of its telemetry**
//! (`last_yields`, `last_raid_forfeit`, …). That impl is correct for what it exists to do (compare
//! *intent*, not readouts) and it is documented as such, but it means a value-equality round-trip
//! would pass while silently dropping every one of those fields. Dropping exactly that class of
//! field is the defect the whole checkpoint arc exists to prevent
//! (`.claude/rules/core_sim/checkpoints.md`), so the one assertion this file makes must not be
//! blind to it.
//!
//! So the comparison is on the **encoded artifact**: encode, decode, re-encode, and require the two
//! encodings to agree. Nothing can opt a field out of that, because serde walks every field
//! regardless of what `PartialEq` thinks. It also avoids deriving `PartialEq` across 85 types to
//! serve one test, which would put a comparison on a lot of types nothing else compares.
//!
//! The comparison is order-insensitive: `bevy::utils::HashMap` iteration order is a function of the
//! table's capacity as well as its contents, and a decoded map is built with the capacity serde's
//! size hint gave it rather than the one the sim grew into — so two encodings of the same map can
//! order their entries differently and still say the same thing.

use bevy::prelude::Entity;

use core_sim::sim_state::{capture_sim_state, SimState};
use core_sim::{build_test_app, run_turn, SimulationConfig};

/// Turns to resolve before capturing, so the checkpoint holds a world that has actually run rather
/// than the bare output of worldgen.
const TURNS_BEFORE_CAPTURE: usize = 3;

fn spawn_world() -> bevy::app::App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

fn encode(state: &SimState) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::into_writer(state, &mut bytes).expect("SimState encodes");
    bytes
}

/// A CBOR tree with every map's entries put in a canonical order, recursively.
///
/// Two encodings of one `HashMap` may list its entries in different orders (see the module note),
/// which is a difference in the *encoding* and not in the state. Sorting by the encoded form of
/// each key removes it without weakening anything else: a missing entry, an extra one, or a changed
/// value all still differ.
fn canonical(value: &ciborium::value::Value) -> ciborium::value::Value {
    use ciborium::value::Value;
    match value {
        Value::Map(entries) => {
            let mut canonical_entries: Vec<(Value, Value)> = entries
                .iter()
                .map(|(key, val)| (canonical(key), canonical(val)))
                .collect();
            canonical_entries.sort_by_cached_key(|(key, _)| {
                let mut bytes = Vec::new();
                ciborium::into_writer(key, &mut bytes).expect("a map key re-encodes");
                bytes
            });
            Value::Map(canonical_entries)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Tag(tag, inner) => Value::Tag(*tag, Box::new(canonical(inner))),
        other => other.clone(),
    }
}

fn canonical_tree(bytes: &[u8]) -> ciborium::value::Value {
    let value: ciborium::value::Value = ciborium::from_reader(bytes).expect("the encoding parses");
    canonical(&value)
}

#[test]
fn a_captured_sim_state_survives_a_cbor_round_trip() {
    let mut app = spawn_world();
    for _ in 0..TURNS_BEFORE_CAPTURE {
        run_turn(&mut app);
    }

    let original = capture_sim_state(&app.world);
    assert!(
        !original.tiles.is_empty() && !original.bands.is_empty(),
        "the checkpoint must hold a real world, or a round trip of nothing would pass"
    );

    let first = encode(&original);
    let decoded: SimState = ciborium::from_reader(first.as_slice()).expect("SimState decodes");
    let second = encode(&decoded);

    assert_eq!(
        canonical_tree(&first),
        canonical_tree(&second),
        "a decoded checkpoint must re-encode to the same thing it was decoded from"
    );

    // Liveness: the two encodings agreeing proves nothing if the encoding is empty.
    assert_eq!(decoded.tiles.len(), original.tiles.len());
    assert_eq!(decoded.bands.len(), original.bands.len());
    assert_eq!(decoded.settlements.len(), original.settlements.len());
    assert_eq!(decoded.tick.0, original.tick.0);
}

/// **No `Entity` survives the codec** — rule 1 of the checkpoint format, enforced against serde.
///
/// `bevy_ecs` implements `Serialize`/`Deserialize` for `Entity` unconditionally, so a plain derive
/// on `PopulationCohort` or `Expedition` compiles and quietly encodes an ECS handle that the
/// restore reading it has already renumbered. The three fields are `#[serde(skip)]` with an
/// explicit `Entity::PLACEHOLDER` default, and this is what says so out loud.
#[test]
fn no_entity_handle_crosses_the_codec() {
    let mut app = spawn_world();
    run_turn(&mut app);

    let original = capture_sim_state(&app.world);
    assert!(!original.bands.is_empty(), "the campaign spawns bands");

    // Capture already places the placeholder; the round trip must land on the same value rather
    // than on whatever bits happened to be encoded.
    for record in &original.bands {
        assert_eq!(record.cohort.home, Entity::PLACEHOLDER);
        assert_eq!(record.cohort.current_tile, Entity::PLACEHOLDER);
    }

    let bytes = encode(&original);
    let decoded: SimState = ciborium::from_reader(bytes.as_slice()).expect("SimState decodes");

    for record in &decoded.bands {
        assert_eq!(
            record.cohort.home,
            Entity::PLACEHOLDER,
            "a decoded cohort must name no tile entity"
        );
        assert_eq!(
            record.cohort.current_tile,
            Entity::PLACEHOLDER,
            "a decoded cohort must name no current tile entity"
        );
        if let Some(expedition) = &record.expedition {
            assert_eq!(
                expedition.expedition.home_band,
                Entity::PLACEHOLDER,
                "a decoded expedition must name no home band entity"
            );
        }
    }

    // The assertions above would pass on a codec that faithfully encoded the placeholder, because
    // capture puts a placeholder there in the first place. So: poke REAL handles in and require the
    // codec to lose them. Without `#[serde(skip)]` these survive and the assertions below fail.
    let mut poked = original;
    let planted = Entity::from_raw(4242);
    for record in &mut poked.bands {
        record.cohort.home = planted;
        record.cohort.current_tile = planted;
        if let Some(expedition) = &mut record.expedition {
            expedition.expedition.home_band = planted;
        }
    }
    let poked_bytes = encode(&poked);
    let recovered: SimState =
        ciborium::from_reader(poked_bytes.as_slice()).expect("SimState decodes");
    for record in &recovered.bands {
        assert_eq!(
            record.cohort.home,
            Entity::PLACEHOLDER,
            "a planted entity handle must not survive the codec"
        );
        assert_eq!(record.cohort.current_tile, Entity::PLACEHOLDER);
        if let Some(expedition) = &record.expedition {
            assert_eq!(expedition.expedition.home_band, Entity::PLACEHOLDER);
        }
    }

    // And they are absent from the encoding itself, not merely overwritten on the way back in.
    // `current_tile` and `home_band` are checked by name; `PopulationCohort::home` deliberately is
    // NOT, because `BandRecord::home` is a legitimate `UVec2` — the position that handle was
    // resolved to — and the two share the word. The planted-handle check above is what covers it.
    let tree = canonical_tree(&bytes);
    for key in ["current_tile", "home_band"] {
        assert!(
            !cbor_mentions_key(&tree, key),
            "`{key}` names an Entity and must not appear in the encoding at all"
        );
    }
    assert!(
        cbor_mentions_key(&tree, "home"),
        "`BandRecord::home` — the POSITION — must still be there"
    );
}

/// Whether any map in the tree carries `key` as a text key.
fn cbor_mentions_key(value: &ciborium::value::Value, key: &str) -> bool {
    use ciborium::value::Value;
    match value {
        Value::Map(entries) => entries.iter().any(|(k, v)| {
            matches!(k, Value::Text(text) if text == key) || cbor_mentions_key(v, key)
        }),
        Value::Array(items) => items.iter().any(|item| cbor_mentions_key(item, key)),
        Value::Tag(_, inner) => cbor_mentions_key(inner, key),
        _ => false,
    }
}
