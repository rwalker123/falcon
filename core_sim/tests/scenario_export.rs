//! **The scenario exporter** (`docs/plan_scenarios.md` section 2.10, issue #928): the live world
//! written out as a scenario file, and the round trip export -> load -> export.
//!
//! Driven through the library path the server's `export_scenario` runs
//! ([`export_scenario`]) and the loader path `tests/scenario_loader.rs` runs
//! ([`parse_scenario`] + [`build_scenario_world`]).

use bevy::prelude::*;
use serde_json::Value;

use core_sim::scenario::{
    build_scenario_world, export_scenario, parse_scenario, ScenarioContext, ScenarioExport,
};
use core_sim::{build_test_app, run_turn};

const BREEDING: &str = include_str!("../scenarios/breeding_ceiling_150.json");
const BENCH: &str = include_str!("../scenarios/bench_queue.json");
const DEPOSITS: &str = include_str!("../scenarios/deposits_playtest.json");
const HELD_RUNGS: &str = include_str!("../scenarios/held_rungs_playtest.json");
const HERDS_PENS: &str = include_str!("../scenarios/herds_pens_playtest.json");
const ROADS: &str = include_str!("../scenarios/kept_roads.json");

/// Turns run on the loaded world before it is exported, so the state has moved.
const TURNS_BEFORE_EXPORT: usize = 3;
/// The name the exports are given.
const EXPORT_NAME: &str = "round_trip";

/// Load `text` into a fresh world, run `turns`, and return the world.
fn loaded(text: &str, turns: usize) -> App {
    let mut app = build_test_app();
    let plan = parse_scenario(text, &ScenarioContext::from_world(&app.world))
        .unwrap_or_else(|refusals| panic!("the file is refused: {refusals:?}"));
    build_scenario_world(&mut app, &plan)
        .unwrap_or_else(|refusals| panic!("the file does not load: {refusals:?}"));
    for _ in 0..turns {
        run_turn(&mut app);
    }
    app
}

fn export_of(app: &mut App) -> (ScenarioExport, Value, String) {
    let export = export_scenario(&mut app.world, EXPORT_NAME);
    let text = serde_json::to_string_pretty(&export.file).expect("the export serializes");
    let value: Value = serde_json::from_str(&text).expect("the export is JSON");
    (export, value, text)
}

/// The keys of `value` whose values differ from `other`'s, for a failure that names the field.
fn drifting_keys(a: &Value, b: &Value) -> Vec<String> {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return vec!["<not objects>".to_string()];
    };
    a.keys()
        .chain(b.keys())
        .filter(|key| a.get(*key) != b.get(*key))
        .cloned()
        .collect()
}

/// Export A, rebuild from it, export B, and assert they are the same file. Returns A.
///
/// **`note` is not compared**: it describes the live world the file was exported from (a herd that
/// wandered, a party out), so a reload, which has not lived, has less to say. Every other key is.
fn round_trips(text: &str, turns: usize) -> Value {
    let without_note = |file: &Value| {
        let mut file = file.clone();
        if let Some(object) = file.as_object_mut() {
            object.remove("note");
        }
        file
    };
    let mut live = loaded(text, turns);
    let (_, a, a_text) = export_of(&mut live);
    let mut reloaded = loaded(&a_text, 0);
    let (_, b, _) = export_of(&mut reloaded);
    let (left, right) = (without_note(&a), without_note(&b));
    assert_eq!(
        left,
        right,
        "export -> load -> export drifted in {:?}",
        drifting_keys(&left, &right)
    );
    a
}

fn len_of(file: &Value, pointer: &str) -> usize {
    file.pointer(pointer)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

#[test]
fn the_exported_file_reparses_through_pass_one_for_every_checked_in_scenario() {
    for (name, text) in [
        ("breeding", BREEDING),
        ("bench", BENCH),
        ("deposits", DEPOSITS),
        ("held_rungs", HELD_RUNGS),
        ("herds_pens", HERDS_PENS),
        ("roads", ROADS),
    ] {
        let mut live = loaded(text, TURNS_BEFORE_EXPORT);
        let (_, _, exported) = export_of(&mut live);
        let ctx = ScenarioContext::from_world(&live.world);
        let refused = parse_scenario(&exported, &ctx).err();
        assert!(
            refused.is_none(),
            "{name}: its own export is refused by pass 1: {refused:?}"
        );
    }
}

#[test]
fn a_world_with_a_band_exports_its_band_larder_and_outfit_and_round_trips() {
    let a = round_trips(BREEDING, TURNS_BEFORE_EXPORT);
    assert_eq!(a["name"], EXPORT_NAME);
    assert!(
        len_of(&a, "/factions/0/bands") >= 1,
        "liveness: the export holds the band"
    );
    let band = &a["factions"][0]["bands"][0];
    assert_eq!(band["id"], "f0_band0", "ids are positional");
    assert!(band["size"].as_u64().unwrap_or(0) > 0);
    assert!(
        !band["larder"].as_array().expect("a larder").is_empty(),
        "liveness: the larder is written"
    );
    assert!(
        band.get("outfit").is_some(),
        "the first band's outfit is written"
    );
    assert!(
        a["note"]
            .as_str()
            .unwrap_or("")
            .contains("tile overrides are not exported"),
        "the tiles section says what it cannot"
    );
    assert_eq!(a["tiles"], serde_json::json!([]));
}

#[test]
fn herds_pens_and_labor_round_trip() {
    let a = round_trips(HERDS_PENS, TURNS_BEFORE_EXPORT);
    assert!(len_of(&a, "/herds") > 0, "liveness: herds are written");
    assert!(
        len_of(&a, "/factions/0/labor") > 0,
        "liveness: standing labor rows are written"
    );
}

#[test]
fn held_rungs_round_trip() {
    let a = round_trips(HELD_RUNGS, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/factions/0/rungs") > 0,
        "liveness: held rungs or builds are written"
    );
}

#[test]
fn deposits_round_trip() {
    let a = round_trips(DEPOSITS, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/deposits") > 0,
        "liveness: workings are written"
    );
}

#[test]
fn roads_round_trip() {
    let a = round_trips(ROADS, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/factions/0/roads") > 0,
        "liveness: held roads are written"
    );
}

#[test]
fn every_checked_in_scenario_round_trips_whole() {
    for text in [BREEDING, BENCH, DEPOSITS, HELD_RUNGS, HERDS_PENS, ROADS] {
        round_trips(text, TURNS_BEFORE_EXPORT);
    }
}

#[test]
fn the_bench_queue_round_trips() {
    let a = round_trips(BENCH, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/factions/0/bench") > 0,
        "liveness: the bench queue is written"
    );
}
