//! **The scenario loader, slice 2e: the roads section** (`docs/plan_scenarios.md`, issue #926).
//!
//! Driven through `parse_scenario` + `build_scenario_world`, the path `tests/scenario_loader.rs`
//! runs. Land tiles are found in the built world, never hardcoded, so a retune of worldgen moves the
//! fixtures with it.

use bevy::prelude::*;
use serde_json::{json, Value};

use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::scenario::{
    build_scenario_world, parse_scenario, ScenarioApplied, ScenarioContext, ScenarioRefusal,
};
use core_sim::{
    build_test_app, remoteness_multiplier, road_keeping_range, run_turn, shipped, FactionId,
    LadderConfigHandle, RoadRegistry, SnapshotHistory, Tile, ViewerFaction, NEAR_ENOUGH_TO_KEEP,
};

const FIXTURE: &str = include_str!("../scenarios/kept_roads.json");
const HOME: FactionId = FactionId(0);
/// Where the fixture's only band stands.
const BAND_AT: UVec2 = UVec2::new(13, 11);
const ROADBUILDING: &str = "roadbuilding";
const PAVING: &str = "paving";

/// Longer than the paved road's `upkeep.grace_turns` (12), so an unpaid road would have slipped.
const KEPT_TURNS: usize = 20;

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("the fixture is JSON")
}

/// The fixture with its `roads` and `knowledge` replaced.
fn scenario(knowledge: &[&str], roads: Value) -> Value {
    let mut file = fixture();
    file["factions"][0]["knowledge"] = json!(knowledge);
    file["factions"][0]["roads"] = roads;
    file
}

fn pass_one(file: &Value) -> Result<(), Vec<ScenarioRefusal>> {
    let app = build_test_app();
    parse_scenario(&file.to_string(), &ScenarioContext::from_world(&app.world)).map(|_| ())
}

fn load(file: &Value) -> Result<(App, ScenarioApplied), Vec<ScenarioRefusal>> {
    let mut app = build_test_app();
    let plan = parse_scenario(&file.to_string(), &ScenarioContext::from_world(&app.world))?;
    let applied = build_scenario_world(&mut app, &plan)?;
    Ok((app, applied))
}

fn paths(refusals: &[ScenarioRefusal]) -> Vec<&str> {
    refusals.iter().map(|r| r.path.as_str()).collect()
}

/// Every land tile of the world the fixture builds, nearest the band first, the band's own tile
/// excluded. The same seed builds the same world, so these are land in every load below.
fn land_by_distance() -> Vec<(UVec2, u32)> {
    let (mut app, _) = load(&scenario(&[], json!([]))).expect("the fixture builds");
    let width = app.world.resource::<core_sim::TileRegistry>().width;
    let mut land: Vec<(UVec2, u32)> = app
        .world
        .query::<&Tile>()
        .iter(&app.world)
        .filter(|tile| {
            !tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER) && tile.position != BAND_AT
        })
        .map(|tile| {
            (
                tile.position,
                hex_distance_wrapped(BAND_AT, tile.position, width, false),
            )
        })
        .collect();
    land.sort_by_key(|(tile, distance)| (*distance, tile.y, tile.x));
    land
}

fn water_tile() -> UVec2 {
    let (mut app, _) = load(&scenario(&[], json!([]))).expect("the fixture builds");
    app.world
        .query::<&Tile>()
        .iter(&app.world)
        .find(|tile| tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER))
        .map(|tile| tile.position)
        .expect("the map has water")
}

fn road_json(tile: UVec2, rung: &str, keeper: Option<&str>) -> Value {
    let mut entry = json!({ "at": [tile.x, tile.y], "rung": rung });
    if let Some(keeper) = keeper {
        entry["keeper"] = json!(keeper);
    }
    entry
}

fn one_road(rung: &str, keeper: Option<&str>, knowledge: &[&str]) -> (Value, UVec2) {
    let tile = land_by_distance()[0].0;
    (
        scenario(knowledge, json!([road_json(tile, rung, keeper)])),
        tile,
    )
}

fn held(app: &App, tile: UVec2) -> core_sim::Road {
    app.world
        .resource::<RoadRegistry>()
        .road(tile)
        .unwrap_or_else(|| panic!("a road at {tile:?}"))
        .clone()
}

#[test]
fn the_checked_in_fixture_builds_with_no_refusals() {
    let (app, applied) = load(&fixture()).expect("kept_roads.json builds");
    let file = fixture();
    for entry in file["factions"][0]["roads"].as_array().unwrap() {
        let tile = UVec2::new(
            entry["at"][0].as_u64().unwrap() as u32,
            entry["at"][1].as_u64().unwrap() as u32,
        );
        let road = held(&app, tile);
        assert_eq!(road.held_rung().id_str(), entry["rung"].as_str().unwrap());
        assert_eq!(road.keeper.is_some(), entry.get("keeper").is_some());
    }
    assert_eq!(applied.bands.len(), 1);
}

/// Past the paved road's grace (the longest of the built rungs'), every KEPT road of the fixture
/// still stands at the rung the file names: its `roadwork` row (no Max, so the sim staffs what the
/// roads need) and its stone pay the keeping. The
/// trails are not asserted — a trail nobody walks wears back to a path, which no keeping prevents.
#[test]
fn the_checked_in_fixture_keeps_its_roads() {
    let (mut app, _) = load(&fixture()).expect("kept_roads.json builds");
    for _ in 0..KEPT_TURNS {
        run_turn(&mut app);
    }
    let file = fixture();
    let kept = file["factions"][0]["roads"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry.get("keeper").is_some());
    for entry in kept {
        let tile = UVec2::new(
            entry["at"][0].as_u64().unwrap() as u32,
            entry["at"][1].as_u64().unwrap() as u32,
        );
        let road = held(&app, tile);
        assert_eq!(
            road.held_rung().id_str(),
            entry["rung"].as_str().unwrap(),
            "the road at {tile:?} after {KEPT_TURNS} turns"
        );
    }
}

#[test]
fn a_trail_is_held_with_no_keeper() {
    let (file, tile) = one_road("trail", None, &[]);
    let (app, _) = load(&file).expect("a trail loads");
    let road = held(&app, tile);
    assert_eq!(road.held_rung(), shipped::route_trail());
    assert!(road.keeper.is_none());
    assert!(!road.is_built());
}

#[test]
fn a_dirt_road_needs_roadbuilding() {
    let (file, tile) = one_road("dirt_road", Some("home"), &[ROADBUILDING]);
    let (app, applied) = load(&file).expect("known roadbuilding loads a dirt road");
    let road = held(&app, tile);
    assert_eq!(road.held_rung(), shipped::route_dirt_road());
    assert!(road.is_built());
    let keeper = road.keeper.expect("a dirt road has its keeper");
    assert_eq!(keeper.band, applied.bands["home"]);
    assert_eq!(keeper.faction, HOME);

    let (file, _) = one_road("dirt_road", Some("home"), &[]);
    let refusals = load(&file).expect_err("without roadbuilding it is refused");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "factions[0].roads[0]");
    assert!(
        refusals[0].reason.to_lowercase().contains("roadbuilding"),
        "the gate's own words: {}",
        refusals[0]
    );
}

#[test]
fn a_paved_road_needs_paving_as_well() {
    let (file, tile) = one_road("paved_road", Some("home"), &[ROADBUILDING, PAVING]);
    let (app, _) = load(&file).expect("both known loads a paved road");
    assert_eq!(held(&app, tile).held_rung(), shipped::route_paved_road());

    let (file, _) = one_road("paved_road", Some("home"), &[ROADBUILDING]);
    let refusals = load(&file).expect_err("roadbuilding alone is not paving");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(
        refusals[0].reason.to_lowercase().contains("paving"),
        "{}",
        refusals[0]
    );
}

#[test]
fn a_remote_road_is_quoted_dearer_than_a_near_one() {
    let ladder = build_test_app()
        .world
        .resource::<LadderConfigHandle>()
        .get();
    let range = road_keeping_range(&ladder);
    let land = land_by_distance();
    let near = land
        .iter()
        .find(|(_, distance)| *distance <= range)
        .expect("land inside the keeping range")
        .0;
    let far = land
        .iter()
        .find(|(_, distance)| *distance > range)
        .expect("land beyond the keeping range")
        .0;
    let roads = json!([
        road_json(near, "dirt_road", Some("home")),
        road_json(far, "dirt_road", Some("home")),
    ]);
    let (app, _) = load(&scenario(&[ROADBUILDING], roads)).expect("both load");
    assert_eq!(held(&app, near).keeper_remoteness, NEAR_ENOUGH_TO_KEEP);
    let remote = remoteness_multiplier(range + 1, &ladder);
    assert_ne!(remote, NEAR_ENOUGH_TO_KEEP, "the control is a real quote");
    assert_eq!(held(&app, far).keeper_remoteness, remote);
    assert_eq!(held(&app, far).held_rung(), shipped::route_dirt_road());
}

#[test]
fn a_held_road_survives_a_turn_at_its_rung() {
    let land = land_by_distance();
    let (trail, dirt) = (land[0].0, land[1].0);
    let roads = json!([
        road_json(dirt, "dirt_road", Some("home")),
        road_json(trail, "trail", None),
    ]);
    let (mut app, _) = load(&scenario(&[ROADBUILDING], roads)).expect("loads");
    run_turn(&mut app);
    assert_eq!(held(&app, dirt).held_rung(), shipped::route_dirt_road());
    assert!(held(&app, dirt).keeper.is_some());
    assert_eq!(held(&app, trail).held_rung(), shipped::route_trail());
}

#[test]
fn a_held_road_reaches_the_encoded_snapshot() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let tile = land_by_distance()[0].0;
    let roads = json!([road_json(tile, "dirt_road", Some("home"))]);
    let (mut app, applied) = load(&scenario(&[ROADBUILDING], roads)).expect("loads");
    app.world.insert_resource(ViewerFaction(HOME));
    run_turn(&mut app);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.routes())
        .and_then(|section| section.routes())
        .expect("the route section is published");
    let row = rows
        .iter()
        .find(|row| UVec2::new(row.tileX(), row.tileY()) == tile)
        .expect("the held road is on the frame");
    assert_eq!(
        row.rung(),
        Some(shipped::route_dirt_road().to_string().as_str())
    );
    assert!(row.hasKeeper());
    assert_eq!(row.keeperBandId(), applied.bands["home"].0);
}

#[test]
fn pass_one_names_each_fault() {
    let tile = |n: usize| land_by_distance()[n].0;
    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "unknown rung",
            json!([road_json(tile(0), "highway", None)]),
            "factions[0].roads[0].rung",
        ),
        (
            "path",
            json!([road_json(tile(0), "path", None)]),
            "factions[0].roads[0].rung",
        ),
        (
            "keeper on a trail",
            json!([road_json(tile(0), "trail", Some("home"))]),
            "factions[0].roads[0].keeper",
        ),
        (
            "no keeper on a dirt road",
            json!([road_json(tile(0), "dirt_road", None)]),
            "factions[0].roads[0].keeper",
        ),
        (
            "unknown keeper",
            json!([road_json(tile(0), "dirt_road", Some("nobody"))]),
            "factions[0].roads[0].keeper",
        ),
        (
            "duplicate tile",
            json!([
                road_json(tile(0), "trail", None),
                road_json(tile(0), "trail", None)
            ]),
            "factions[0].roads[1].at",
        ),
        (
            "off the map",
            json!([{ "at": [80, 0], "rung": "trail" }]),
            "factions[0].roads[0].at",
        ),
    ];
    for (name, roads, expected) in cases {
        let refusals = pass_one(&scenario(&[ROADBUILDING], roads)).expect_err(name);
        assert!(paths(&refusals).contains(&expected), "{name}: {refusals:?}");
    }
}

#[test]
fn a_keeper_of_another_faction_is_refused() {
    let tile = land_by_distance()[0].0;
    let mut file = scenario(
        &[ROADBUILDING],
        json!([road_json(tile, "dirt_road", Some("other"))]),
    );
    file["factions"].as_array_mut().unwrap().push(json!({
        "faction": 1, "knowledge": [], "rungs": [], "labor": [], "pens": [], "roads": [], "bench": [],
        "bands": [{
            "id": "other", "at": [40, 20], "size": 10,
            "ages": { "children": 0.3, "working": 0.6, "elders": 0.1 },
            "founding_lines": 1, "larder": []
        }]
    }));
    let refusals = pass_one(&file).expect_err("somebody else's people");
    assert!(paths(&refusals).contains(&"factions[0].roads[0].keeper"));
}

#[test]
fn a_duplicate_tile_across_factions_is_refused() {
    let tile = land_by_distance()[0].0;
    let mut file = scenario(&[], json!([road_json(tile, "trail", None)]));
    file["factions"].as_array_mut().unwrap().push(json!({
        "faction": 1, "knowledge": [], "rungs": [], "labor": [], "pens": [], "bench": [],
        "bands": [{
            "id": "other", "at": [40, 20], "size": 10,
            "ages": { "children": 0.3, "working": 0.6, "elders": 0.1 },
            "founding_lines": 1, "larder": []
        }],
        "roads": [road_json(tile, "trail", None)]
    }));
    let refusals = pass_one(&file).expect_err("one road per tile");
    assert!(paths(&refusals).contains(&"factions[1].roads[0].at"));
}

#[test]
fn a_road_on_water_is_refused_at_apply() {
    let water = water_tile();
    let file = scenario(&[], json!([road_json(water, "trail", None)]));
    pass_one(&file).expect("pass 1 cannot see the terrain");
    let refusals = load(&file).expect_err("a road cannot lie on water");
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].path, "factions[0].roads[0]");
    assert!(
        refusals[0]
            .reason
            .contains(&format!("({},{})", water.x, water.y)),
        "{}",
        refusals[0]
    );
}

#[test]
fn two_bad_entries_are_two_refusals() {
    let land = land_by_distance();
    let roads = json!([
        road_json(land[0].0, "highway", None),
        road_json(land[1].0, "path", None),
    ]);
    let refusals = pass_one(&scenario(&[], roads)).expect_err("both are refused");
    assert_eq!(refusals.len(), 2, "{refusals:?}");

    let roads = json!([
        road_json(land[0].0, "dirt_road", Some("home")),
        road_json(land[1].0, "paved_road", Some("home")),
    ]);
    let refusals = load(&scenario(&[], roads)).expect_err("apply collects too");
    assert_eq!(refusals.len(), 2, "{refusals:?}");
}
