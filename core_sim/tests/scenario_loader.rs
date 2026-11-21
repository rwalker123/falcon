//! **The scenario loader, slice 1: the bands section** (`docs/plan_scenarios.md`, issue #921).
//!
//! Driven through the library path the server's `load_scenario` handler runs: pass 1
//! ([`parse_scenario`]), then [`build_scenario_world`] (profile, roster, setup, the build turn and
//! `finish_scenario`, one body shared with the server). The server adds only the swap-in around it.

use bevy::prelude::*;
use serde_json::{json, Value};

use core_sim::command_seam::apply_set_starting_loadout;
use core_sim::scenario::{
    build_scenario_world, parse_scenario, ScenarioApplied, ScenarioContext, ScenarioRefusal,
};
use core_sim::{
    build_test_app, knows_messengers, run_turn, scalar_from_f32, ActiveStartProfile, BandId,
    DiscoveryProgressLedger, FactionId, FactionRegistry, KitAllocation, LadderConfigHandle,
    MaterialAllocation, PopulationCohort, SimulationConfig, StartProfileKnowledgeTagsHandle,
    StartProfiles, StartProfilesHandle, StartingLoadout, Tile,
};

const SHIPPED: &str = include_str!("../scenarios/breeding_ceiling_150.json");
const HOME: FactionId = FactionId(0);
/// The messengers knowledge tag the shipped scenario grants.
const MESSENGERS: &str = "messengers";
/// The shipped scenario's listed dry larder. It is below the band's carry (150 x 0.6 working x 7.0 =
/// 630) on purpose: a fixed larder counts against the carry, so a larder over it leaves the outfit no
/// room at all.
const LISTED_LARDER: f32 = 450.0;
/// A band small enough that its carry is below the profile's default outfit.
const SMALL_BAND: u32 = 2;

fn shipped() -> Value {
    serde_json::from_str(SHIPPED).expect("the shipped scenario is JSON")
}

/// Pass 1 alone.
fn pass_one(text: &str) -> Result<(), Vec<ScenarioRefusal>> {
    let app = build_test_app();
    parse_scenario(text, &ScenarioContext::from_world(&app.world)).map(|_| ())
}

/// The whole library path: pass 1, then the one candidate build the server also runs.
fn load(text: &str) -> Result<(App, ScenarioApplied), Vec<ScenarioRefusal>> {
    load_on(build_test_app(), text)
}

fn load_on(mut app: App, text: &str) -> Result<(App, ScenarioApplied), Vec<ScenarioRefusal>> {
    let plan = parse_scenario(text, &ScenarioContext::from_world(&app.world))?;
    let applied = build_scenario_world(&mut app, &plan)?;
    Ok((app, applied))
}

fn band_of(app: &mut App, id: BandId) -> (PopulationCohort, UVec2) {
    let (cohort, tile) = app
        .world
        .query::<(&BandId, &PopulationCohort)>()
        .iter(&app.world)
        .find(|(band, _)| **band == id)
        .map(|(_, cohort)| (cohort.clone(), cohort.current_tile))
        .expect("the band exists");
    let position = app.world.get::<Tile>(tile).expect("the tile").position;
    (cohort, position)
}

#[test]
fn pass_one_collects_every_refusal_in_one_go() {
    let mut file = shipped();
    file["world"]["seed"] = json!(0);
    file["world"]["preset"] = json!("no_such_preset");
    file["world"]["profile"] = json!("no_such_profile");
    let band = &mut file["factions"][0]["bands"][0];
    band["size"] = json!(0);
    band["ages"] = json!({ "children": 0.5, "working": 0.5, "elders": 0.5 });
    band["larder"] = json!([{ "class": "stale", "amount": 5.0 }]);
    band["outfit"]["kits"] = json!({ "big_gam": 1, "none": 1 });
    band["outfit"]["materials"] = json!({ "unobtainium": 1 });
    file["factions"][0]["knowledge"] = json!(["no_such_tag"]);
    file["factions"][0]["bands"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": "home", "at": [1, 1], "size": 1,
            "ages": { "children": 1.0, "working": 0.0, "elders": 0.0 },
            "founding_lines": 1, "larder": [],
            "outfit": { "kits": {}, "materials": {} }
        }));
    file["factions"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "faction": 0, "knowledge": [], "rungs": [], "bands": [] }));

    let refusals = pass_one(&file.to_string()).expect_err("a faulty file is refused");
    let paths: Vec<&str> = refusals.iter().map(|r| r.path.as_str()).collect();
    for expected in [
        "world.seed",
        "world.preset",
        "world.profile",
        "factions[0].knowledge[0]",
        "factions[0].bands[0].size",
        "factions[0].bands[0].ages",
        "factions[0].bands[0].larder[0].class",
        "factions[0].bands[0].outfit.kits.big_gam",
        "factions[0].bands[0].outfit.kits.none",
        "factions[0].bands[0].outfit.materials.unobtainium",
        "factions[0].bands[1].id",
        "factions[0].bands[1].outfit",
        "factions[1].faction",
        "factions[1].bands",
    ] {
        assert!(
            paths.contains(&expected),
            "missing a refusal at {expected}; got {paths:?}"
        );
    }
    assert!(
        refusals[0].to_string().contains(": "),
        "a refusal reads `path: reason`"
    );
}

#[test]
fn an_unknown_key_is_refused_and_labor_is_not_yet_a_key() {
    let mut file = shipped();
    file["factions"][0]["labor"] = json!([]);
    let refusals = pass_one(&file.to_string()).expect_err("labor is slice 2a");
    assert_eq!(refusals.len(), 1, "a parse error is the one refusal");
    assert!(refusals[0].reason.contains("labor"), "{}", refusals[0]);

    let mut file = shipped();
    file["world"]["colour"] = json!("blue");
    assert!(pass_one(&file.to_string()).is_err());
}

#[test]
fn a_missing_key_is_refused_there_is_no_default() {
    let mut file = shipped();
    file["factions"][0]["bands"][0]
        .as_object_mut()
        .unwrap()
        .remove("larder");
    assert!(pass_one(&file.to_string()).is_err());
}

#[test]
fn the_shipped_scenario_loads_a_band_of_150_exactly_as_written() {
    let (mut app, applied) = load(SHIPPED).expect("the shipped scenario loads");
    let id = *applied
        .bands
        .get("home")
        .expect("the home band is recorded");
    let (cohort, at) = band_of(&mut app, id);

    assert_eq!(cohort.faction, HOME);
    assert_eq!(at, UVec2::new(13, 11), "the band stands exactly on `at`");
    // The brackets are the file's shape, not the profile's `initial_distribution`.
    let size = cohort.size as f32;
    // The world has run its build turn (the meal, ageing, elder mortality), so the band is read a turn
    // past its opening head-count; `ages_within_tolerance_do_not_change_the_size` pins the opening one.
    assert!((size - 150.0).abs() <= 6.0, "size {size}");
    let share = |bracket: f32| bracket / (cohort.total().to_f32());
    assert!((share(cohort.children.to_f32()) - 0.30).abs() < 0.03);
    assert!((share(cohort.working.to_f32()) - 0.60).abs() < 0.03);
    assert!((share(cohort.elders.to_f32()) - 0.10).abs() < 0.03);
    assert_eq!(cohort.founding_lines.len(), 8, "L is the file's");
    // The larder is the listed dry food, less the one meal the build pass eats - nothing else.
    let larder = cohort.stores.food();
    assert_eq!(larder.iter().count(), 1, "only the listed class");
    assert_eq!(larder.iter().next().map(|(class, _)| class), Some("dry"));
    assert!(
        larder.total().to_f32() > 0.85 * LISTED_LARDER && larder.total().to_f32() <= LISTED_LARDER
    );

    // The faction knows what the file adds.
    let tags = app
        .world
        .resource::<StartProfileKnowledgeTagsHandle>()
        .get();
    let discovery = tags.get(MESSENGERS).expect("the tag exists").discovery_id();
    let ledger = app.world.resource::<DiscoveryProgressLedger>();
    assert!(ledger.get_progress(HOME, discovery) > scalar_from_f32(0.0));
    // **Known, not started**: the game's own reader for the lesson answers yes.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    assert!(
        knows_messengers(ledger, HOME, &ladder.knowledge),
        "a listed tag is a tag the faction already has"
    );

    // The outfit replaced the default under the carry cap, and the window is still open for the
    // player to revise it.
    let window = app
        .world
        .resource::<StartingLoadout>()
        .window(id)
        .expect("the opening window");
    assert!(
        window.open,
        "the window is still open when the player lands"
    );
    let kits: Vec<(&str, u32)> = window
        .kits
        .iter()
        .map(|row| (row.kit_id.as_str(), row.count))
        .collect();
    assert!(kits.contains(&("big_game", 10)) && kits.contains(&("gathering", 20)));

    // Nothing scenario-only survives into the running world.
    assert!(app
        .world
        .get_resource::<core_sim::scenario::ScenarioSetup>()
        .is_none());
}

#[test]
fn the_control_world_does_not_know_messengers() {
    let mut app = build_test_app();
    run_turn(&mut app);
    let tags = app
        .world
        .resource::<StartProfileKnowledgeTagsHandle>()
        .get();
    let discovery = tags.get(MESSENGERS).unwrap().discovery_id();
    let ledger = app.world.resource::<DiscoveryProgressLedger>();
    assert_eq!(ledger.get_progress(HOME, discovery), scalar_from_f32(0.0));
}

#[test]
fn a_band_placed_on_water_is_refused_and_names_the_tile() {
    // A tile the generated world holds as water, read off the world the shipped scenario builds.
    let (mut app, _) = load(SHIPPED).expect("the shipped scenario loads");
    let sea = app
        .world
        .query::<&Tile>()
        .iter(&app.world)
        .find(|tile| tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER))
        .map(|tile| tile.position)
        .expect("the map has water");
    let mut file = shipped();
    file["factions"][0]["bands"][0]["at"] = json!([sea.x, sea.y]);
    let refusals = match load(&file.to_string()) {
        Err(refusals) => refusals,
        Ok(_) => panic!("a band on the sea is refused"),
    };
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "factions[0].bands[0].at");
    assert!(
        refusals[0]
            .reason
            .contains(&format!("tile ({},{}) is", sea.x, sea.y)),
        "{}",
        refusals[0]
    );
}

#[test]
fn a_tile_off_the_map_is_refused() {
    let mut file = shipped();
    file["factions"][0]["bands"][0]["at"] = json!([500, 500]);
    let refusals = load(&file.to_string()).map(|_| ()).expect_err("refused");
    assert!(
        refusals[0].reason.contains("outside the 80x52 map"),
        "{}",
        refusals[0]
    );
}

#[test]
fn a_listed_faction_the_world_lacks_is_refused() {
    let mut file = shipped();
    file["factions"][0]["faction"] = json!(3);
    let refusals = load(&file.to_string()).map(|_| ()).expect_err("refused");
    assert_eq!(refusals[0].path, "factions[0].faction");
}

#[test]
fn an_outfit_over_the_carry_is_refused_with_the_handlers_own_reason() {
    let mut file = shipped();
    file["factions"][0]["bands"][0]["outfit"]["kits"] = json!({ "big_game": 100000 });
    let refusals = load(&file.to_string()).map(|_| ()).expect_err("refused");
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].path, "factions[0].bands[0].outfit");
    assert!(
        refusals[0].reason.contains("weighs") && refusals[0].reason.contains("carry"),
        "{}",
        refusals[0]
    );
}

/// The server's `set_starting_loadout` handler and the scenario's `outfit` are the same function:
/// the same allocation on the same band is refused with the same words, and accepted into the same
/// window rows.
#[test]
fn the_seam_is_one_code_path_for_the_server_and_the_scenario() {
    let mut file = shipped();
    file["factions"][0]["bands"][0]
        .as_object_mut()
        .unwrap()
        .remove("outfit");
    let (mut app, applied) = load(&file.to_string()).expect("loads with the default outfit");
    let id = *applied.bands.get("home").unwrap();

    let too_much = [KitAllocation {
        kit_id: "big_game".to_string(),
        count: 100_000,
    }];
    let refusal = apply_set_starting_loadout(&mut app.world, HOME, id, &too_much, &[])
        .expect_err("over the carry");
    let mut over = shipped();
    over["factions"][0]["bands"][0]["outfit"]["kits"] = json!({ "big_game": 100000 });
    over["factions"][0]["bands"][0]["outfit"]["materials"] = json!({});
    let scenario_refusal = load(&over.to_string()).map(|_| ()).expect_err("refused");
    assert_eq!(scenario_refusal[0].reason, refusal.to_string());

    // And an accepted order lands in the open window.
    let kits = [KitAllocation {
        kit_id: "gathering".to_string(),
        count: 3,
    }];
    let materials = [MaterialAllocation {
        material_id: "fibre".to_string(),
        units: 4,
    }];
    apply_set_starting_loadout(&mut app.world, HOME, id, &kits, &materials).expect("fits");
    let window = app.world.resource::<StartingLoadout>().window(id).unwrap();
    assert_eq!(window.kits.len(), 1);
    assert_eq!(window.kits[0].count, 3);
}

/// The default outfit is fitted to the band the scenario built, not the profile's 30-person one: a
/// band of 10 has a carry below the default outfit and is cut to it, where a band of 150 is not.
#[test]
fn the_default_outfit_is_fitted_to_the_scenario_band() {
    let kits_held = |size: u32| -> u32 {
        let mut file = shipped();
        let band = &mut file["factions"][0]["bands"][0];
        band.as_object_mut().unwrap().remove("outfit");
        band["size"] = json!(size);
        band["larder"] = json!([]);
        let (app, applied) = load(&file.to_string()).expect("loads");
        let id = *applied.bands.get("home").unwrap();
        let window = app.world.resource::<StartingLoadout>().window(id).unwrap();
        window.kits.iter().map(|row| row.count).sum()
    };
    let small = kits_held(SMALL_BAND);
    let large = kits_held(150);
    assert!(
        small < large,
        "a small band is cut to its carry ({small} vs {large})"
    );
}

/// A listed tag the archetype also grants lands known all the same, and the control world's faction
/// 0 does not know messengers.
#[test]
fn a_listed_tag_is_known_where_the_control_is_not() {
    let (app, _) = load(SHIPPED).expect("loads");
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let ledger = app.world.resource::<DiscoveryProgressLedger>();
    assert!(knows_messengers(ledger, HOME, &ladder.knowledge));
    let mut control = build_test_app();
    run_turn(&mut control);
    let ladder = control.world.resource::<LadderConfigHandle>().get();
    let ledger = control.world.resource::<DiscoveryProgressLedger>();
    assert!(!knows_messengers(ledger, HOME, &ladder.knowledge));
}

#[test]
fn a_knowledge_tag_listed_twice_is_refused() {
    let mut file = shipped();
    file["factions"][0]["knowledge"] = json!(["messengers", "messengers"]);
    let refusals = pass_one(&file.to_string()).expect_err("refused");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "factions[0].knowledge[1]");
    assert!(
        refusals[0].reason.contains("listed twice"),
        "{}",
        refusals[0]
    );
}

/// serde_json keeps the last of two equal keys in silence; a scenario refuses it, at a line and column.
#[test]
fn a_repeated_object_key_is_refused_for_kits_materials_and_archetypes() {
    let cases = [
        (
            r#""kits": { "big_game": 10, "gathering": 20 }"#,
            r#""kits": { "big_game": 10, "big_game": 20 }"#,
            "big_game",
        ),
        (
            r#""materials": { "fibre": 20, "hide": 12 }"#,
            r#""materials": { "fibre": 20, "fibre": 12 }"#,
            "fibre",
        ),
        (
            r#""archetypes": { "major": 0, "minor": 0 }"#,
            r#""archetypes": { "major": 0, "major": 1 }"#,
            "major",
        ),
    ];
    for (from, to, key) in cases {
        assert!(SHIPPED.contains(from), "the shipped file holds {from}");
        let refusals = pass_one(&SHIPPED.replace(from, to)).expect_err("a repeated key is refused");
        assert_eq!(refusals.len(), 1, "{refusals:?}");
        assert!(
            refusals[0].path.starts_with("line ") && refusals[0].path.contains("column"),
            "{}",
            refusals[0]
        );
        assert!(
            refusals[0]
                .reason
                .contains(&format!("'{key}' is listed twice")),
            "{}",
            refusals[0]
        );
    }
}

/// Shares that sum to 0.99 are forgiven and normalised: the band is the size the same shape at a sum of
/// 1.0 gives, not about 1% short of it. (The size is read after the build turn, so the control is the
/// same proportions at a sum of one rather than the bare `150`.)
#[test]
fn ages_within_tolerance_do_not_change_the_size() {
    let size_with = |ages: Value| -> u32 {
        let mut file = shipped();
        file["factions"][0]["bands"][0]["ages"] = ages;
        let (mut app, applied) = load(&file.to_string()).expect("loads");
        let id = *applied.bands.get("home").unwrap();
        band_of(&mut app, id).0.size
    };
    let whole = size_with(json!({ "children": 0.30, "working": 0.60, "elders": 0.10 }));
    let short = size_with(json!({ "children": 0.297, "working": 0.594, "elders": 0.099 }));
    assert_eq!(short, whole, "0.99 of the same shape is the same band");
}

/// A land tile far from the shipped band, for a second faction.
fn far_land_tile() -> [u32; 2] {
    let (mut app, _) = load(SHIPPED).expect("loads");
    let tile = app
        .world
        .query::<&Tile>()
        .iter(&app.world)
        .find(|tile| {
            !tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER) && tile.position.x >= 60
        })
        .map(|tile| tile.position)
        .expect("the map has land far east");
    [tile.x, tile.y]
}

fn two_faction_file(major: u32) -> Value {
    let mut file = shipped();
    file["world"]["archetypes"] = json!({ "major": major, "minor": 0 });
    file["factions"].as_array_mut().unwrap().push(json!({
        "faction": 1,
        "knowledge": [],
        "rungs": [],
        "bands": [{
            "id": "rival", "at": far_land_tile(), "size": 30,
            "ages": { "children": 0.3, "working": 0.6, "elders": 0.1 },
            "founding_lines": 8, "larder": []
        }]
    }));
    file
}

/// `world.archetypes` seats the roster through the same path the server uses, so a section for faction
/// 1 loads when one major rival was asked for and is refused when none was.
#[test]
fn the_archetype_counts_seat_the_roster_a_faction_section_needs() {
    let (mut app, applied) = load(&two_faction_file(1).to_string()).expect("a rival was seated");
    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 2);
    let rival = *applied.bands.get("rival").expect("the rival band");
    assert_eq!(band_of(&mut app, rival).0.faction, FactionId(1));

    let refusals = load(&two_faction_file(0).to_string())
        .map(|_| ())
        .expect_err("no rival was asked for");
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.path == "factions[1].faction"),
        "{refusals:?}"
    );
}

/// The world section's profile is the one the world is built with, not the app's default.
#[test]
fn a_scenario_builds_with_the_profile_it_names() {
    let shipped_profiles: Value =
        serde_json::from_str(include_str!("../src/data/start_profiles.json")).unwrap();
    let mut second = shipped_profiles["profiles"][0].clone();
    second["id"] = json!("second_tribe");
    let mut all = shipped_profiles.clone();
    all["profiles"].as_array_mut().unwrap().push(second);
    let profiles = StartProfiles::from_json_str(&all.to_string()).expect("two profiles parse");

    let mut app = build_test_app();
    app.world
        .insert_resource(StartProfilesHandle::new(std::sync::Arc::new(profiles)));
    let mut file = shipped();
    file["world"]["profile"] = json!("second_tribe");
    let (app, _) = load_on(app, &file.to_string()).expect("loads");
    assert_eq!(
        app.world.resource::<ActiveStartProfile>().profile().id,
        "second_tribe"
    );
    assert_eq!(
        app.world.resource::<SimulationConfig>().start_profile_id,
        "second_tribe"
    );
}
