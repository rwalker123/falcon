//! **The two settlement playtest scenarios** (#176, `core_sim/scenarios/settlement_{town,city}_playtest.json`).
//!
//! Each file opens a tribe one build short of a stage (the file's `tribes` section founds it at
//! load), so a playtester watches the crossing instead of playing to it. Driven through the same
//! library path the server's `load_scenario` runs.

use bevy::prelude::*;
use core_sim::scenario::{build_scenario_world, parse_scenario, ScenarioContext, ScenarioRefusal};
use core_sim::{
    build_test_app, run_turn, BandId, BranchId, CommandEventKind, CommandEventLog,
    PopulationCohort, SettlementStageConfigHandle, SettlementStages, TileImprovementRegistry,
    TribeRegistry,
};
use serde_json::{json, Value};

const TOWN: &str = include_str!("../scenarios/settlement_town_playtest.json");
const CITY: &str = include_str!("../scenarios/settlement_city_playtest.json");
/// The belief the town file stands on its home tile.
const TOWN_BELIEF: f32 = 12.0;
/// The most turns the town crossing may take after load.
const TOWN_CROSSING_CAP: u32 = 6;
/// The most turns the city crossing may take after load (the granary opens part built).
const CITY_CROSSING_CAP: u32 = 6;
/// The fewest turns a city must hold after it is reached (carry and appetite bound it).
const CITY_HOLDS_AT_LEAST: u32 = 30;
/// The beat the city stage fires, as the feed's gloss names it.
const CITY_BEAT_GLOSS: &str = "settlement.city_reached=";

fn try_load(text: &str) -> Result<App, Vec<ScenarioRefusal>> {
    let mut app = build_test_app();
    let plan = parse_scenario(text, &ScenarioContext::from_world(&app.world))?;
    build_scenario_world(&mut app, &plan)?;
    Ok(app)
}

fn load(text: &str) -> App {
    try_load(text).unwrap_or_else(|refusals| panic!("refused: {refusals:?}"))
}

fn stage_id(app: &App, index: usize) -> String {
    app.world
        .resource::<SettlementStageConfigHandle>()
        .get()
        .stages[index]
        .id
        .clone()
}

/// The first tribe's stage id, if a tribe stands and has been resolved.
fn tribe_stage(app: &App) -> Option<String> {
    let resolved = app.world.resource::<SettlementStages>();
    app.world
        .resource::<TribeRegistry>()
        .iter()
        .next()
        .and_then(|tribe| resolved.of_tribe(tribe.id))
        .map(|index| stage_id(app, index))
}

fn members(app: &App) -> usize {
    app.world
        .resource::<TribeRegistry>()
        .iter()
        .next()
        .map_or(0, |tribe| tribe.members.len())
}

fn people(app: &mut App) -> u32 {
    let mut query = app.world.query::<&PopulationCohort>();
    query.iter(&app.world).map(|cohort| cohort.size).sum()
}

fn held_store(app: &App) -> Option<String> {
    let food_stores = BranchId::new("food_stores");
    app.world
        .resource::<TileImprovementRegistry>()
        .iter()
        .find(|improvement| improvement.branch == food_stores)
        .map(|improvement| improvement.held_rung().wire_key())
}

/// Run turns until the tribe's stage is `target`; the turns it took after the call, or `None` past
/// `cap`. A tribe must stand every turn (it is not dissolved by the load).
fn turns_until_tribe_is(app: &mut App, target: &str, cap: u32) -> Option<u32> {
    for turn in 1..=cap {
        run_turn(app);
        assert_eq!(
            app.world.resource::<TribeRegistry>().len(),
            1,
            "turn {turn}"
        );
        if tribe_stage(app).as_deref() == Some(target) {
            return Some(turn);
        }
    }
    None
}

#[test]
fn the_town_scenario_opens_a_village_tribe_and_becomes_a_town_when_the_pit_is_dug() {
    let mut app = load(TOWN);
    assert_eq!(members(&app), 3, "the tribe stands at load");
    assert_eq!(tribe_stage(&app).as_deref(), Some("village"));
    assert_eq!(held_store(&app), Some("food_stores:unstored".to_string()));
    // The `tiles[].belief` setter lands through the registry's one add seam.
    assert!(
        app.world
            .resource::<core_sim::BeliefRegistry>()
            .get(UVec2::new(13, 11))
            >= TOWN_BELIEF,
        "the file's belief stands on its tile"
    );
    let crossing = turns_until_tribe_is(&mut app, "town", TOWN_CROSSING_CAP)
        .expect("the tribe becomes a town within a few turns");
    assert_eq!(
        held_store(&app),
        Some("food_stores:storage_pit".to_string()),
        "it is the dug pit that makes the town"
    );
    println!("town crossing: turn {crossing} after load");
}

#[test]
fn the_city_scenario_opens_a_town_tribe_and_becomes_a_city_when_the_granary_is_built() {
    let mut app = load(CITY);
    assert_eq!(members(&app), 10, "the tribe stands at load");
    assert!(
        people(&mut app) >= 1_000,
        "the people already number a city"
    );
    assert_eq!(tribe_stage(&app).as_deref(), Some("town"));
    assert_eq!(
        held_store(&app),
        Some("food_stores:storage_pit".to_string())
    );

    let crossing = turns_until_tribe_is(&mut app, "city", CITY_CROSSING_CAP)
        .expect("the granary makes a city");
    assert_eq!(
        held_store(&app),
        Some("food_stores:granary".to_string()),
        "it is the finished granary that makes the city"
    );
    let log = app.world.resource::<CommandEventLog>();
    assert!(
        log.iter()
            .any(|entry| entry.kind == CommandEventKind::SettlementStageReached),
        "the stage-reached event is told"
    );
    assert!(
        log.iter().any(|entry| {
            entry.kind == CommandEventKind::NarrativeBeat
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(CITY_BEAT_GLOSS))
        }),
        "the settlement.city_reached beat fires"
    );
    // The city holds: it keeps its people and its food for a good while after the crossing.
    for turn in 1..=CITY_HOLDS_AT_LEAST {
        run_turn(&mut app);
        assert_eq!(
            tribe_stage(&app).as_deref(),
            Some("city"),
            "still a city {turn} turns on"
        );
    }
    println!("city crossing: turn {crossing} after load, held {CITY_HOLDS_AT_LEAST}+ turns");
}

/// The town file with its `tribes` entry replaced.
fn town_with_tribes(tribes: Value) -> String {
    let mut file: Value = serde_json::from_str(TOWN).unwrap();
    file["factions"][0]["tribes"] = tribes;
    file.to_string()
}

fn refusal_paths(text: &str) -> Vec<String> {
    match try_load(text) {
        Ok(_) => Vec::new(),
        Err(refusals) => refusals.into_iter().map(|r| r.path).collect(),
    }
}

#[test]
fn a_tribe_the_game_could_never_form_is_refused_with_a_path() {
    let fewer = refusal_paths(&town_with_tribes(json!([{ "bands": ["home"] }])));
    assert!(
        fewer.contains(&"factions[0].tribes[0].bands".to_string()),
        "{fewer:?}"
    );

    let unknown = refusal_paths(&town_with_tribes(json!([{ "bands": ["home", "nobody"] }])));
    assert!(
        unknown.contains(&"factions[0].tribes[0].bands[1]".to_string()),
        "{unknown:?}"
    );

    let twice = refusal_paths(&town_with_tribes(json!([{ "bands": ["home", "home"] }])));
    assert!(
        twice.contains(&"factions[0].tribes[0].bands[1]".to_string()),
        "{twice:?}"
    );

    let two_tribes = refusal_paths(&town_with_tribes(
        json!([{ "bands": ["home", "east"] }, { "bands": ["east", "south"] }]),
    ));
    assert!(
        two_tribes.contains(&"factions[0].tribes[1].bands[0]".to_string()),
        "{two_tribes:?}"
    );

    // No chief among the members: two small bands added with no `leader` key are chiefless.
    let mut file: Value = serde_json::from_str(TOWN).unwrap();
    for (id, at) in [("stragglers_a", [13, 8]), ("stragglers_b", [10, 11])] {
        file["factions"][0]["bands"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "id": id, "at": at, "size": 40,
                "ages": { "children": 0.30, "working": 0.60, "elders": 0.10 },
                "founding_lines": 8, "larder": [{ "class": "dry", "amount": 150.0 }]
            }));
    }
    file["factions"][0]["tribes"] = json!([{ "bands": ["stragglers_a", "stragglers_b"] }]);
    let chiefless = refusal_paths(&file.to_string());
    assert!(
        chiefless.contains(&"factions[0].tribes[0]".to_string()),
        "{chiefless:?}"
    );
}

#[test]
fn a_loaded_tribe_survives_the_turns_and_a_member_is_a_band() {
    let mut app = load(TOWN);
    let mut query = app.world.query::<&BandId>();
    let bands = query.iter(&app.world).count();
    assert_eq!(bands, 3);
    for _ in 0..TOWN_CROSSING_CAP {
        run_turn(&mut app);
    }
    assert_eq!(members(&app), 3, "no member left the tribe");
}

fn city_with_progress(progress: Value) -> Vec<String> {
    let mut file: Value = serde_json::from_str(CITY).unwrap();
    file["factions"][0]["rungs"][1]["build_rung"]["progress"] = progress;
    refusal_paths(&file.to_string())
}

#[test]
fn a_builds_progress_is_refused_where_it_means_nothing() {
    let path = "factions[0].rungs[1].build_rung.progress".to_string();
    // Negative, and the whole rung's work (a finished rung is a held one).
    assert!(city_with_progress(json!(-1.0)).contains(&path));
    assert!(city_with_progress(json!(120.0)).contains(&path));
    assert!(city_with_progress(json!(119.0)).is_empty());
    // A verb that raises a patch is not a tile build.
    let mut file: Value = serde_json::from_str(CITY).unwrap();
    file["factions"][0]["rungs"][1]["build_rung"]["verb"] = json!("cultivate");
    file["factions"][0]["rungs"][1]["build_rung"]["progress"] = json!(5.0);
    assert!(refusal_paths(&file.to_string()).contains(&path));
}
