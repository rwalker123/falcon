//! **The scenario loader, slice 1: the bands section** (`docs/plan_scenarios.md`, issue #921).
//!
//! Driven through the library path the server's `load_scenario` handler runs: pass 1
//! ([`parse_scenario`]), then [`build_scenario_world`] (profile, roster, setup, the build turn and
//! `finish_scenario`, one body shared with the server). The server adds only the swap-in around it.

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use serde_json::{json, Value};

use core_sim::command_seam::{
    apply_extend_pen, apply_set_herd_output, apply_set_starting_loadout,
    queue_build_on_working_bands, SOURCE_NAMED_NOT_ASSIGNED,
};
use core_sim::scenario::{
    build_scenario_world, parse_scenario, ScenarioApplied, ScenarioContext, ScenarioRefusal,
};
use core_sim::{
    build_test_app, cancel_replaced_job, habitat_refusal, herd_capacity, knows_messengers,
    run_turn, scalar_from_f32, ActiveStartProfile, BandBench, BandId, BuildDeclaration, BuildJob,
    BuildSource, DepositRegistry, Diet, DiscoveryProgressLedger, EcologyPhase,
    ExtractionConfigHandle, FactionId, FactionRegistry, FaunaConfig, FaunaConfigHandle,
    HabitatRefusal, Herd, HerdRegistry, KitAllocation, LaborAllocation, LaborTarget,
    LadderConfigHandle, MaterialAllocation, PopulationCohort, PreyReach, SimulationConfig,
    StartProfileKnowledgeTagsHandle, StartProfiles, StartProfilesHandle, StartingLoadout, Tile,
    TileRegistry,
};

use core_sim::extraction::tile_deposit_capacity;
use core_sim::NO_DEPOSIT;

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
    file["factions"].as_array_mut().unwrap().push(
        json!({ "faction": 0, "knowledge": [], "rungs": [], "bands": [], "labor": [], "pens": [], "bench": [] }),
    );

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
fn an_unknown_key_is_refused() {
    let mut file = shipped();
    file["world"]["colour"] = json!("blue");
    assert!(pass_one(&file.to_string()).is_err());

    let mut file = shipped();
    file["factions"][0]["labor"] = json!([{ "forage_tile": { "band": "home" } }]);
    let refusals = pass_one(&file.to_string()).expect_err("ForageTile is not a labor entry");
    assert_eq!(refusals.len(), 1, "a parse error is the one refusal");
    assert!(
        refusals[0].reason.contains("forage_tile"),
        "{}",
        refusals[0]
    );
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
        "labor": [],
        "pens": [],
        "bench": [],
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

// ---------------------------------------------------------------------------------------------
// Slice 2c: the herds section
// ---------------------------------------------------------------------------------------------

/// A `herds` entry.
fn herd_entry(id: &str, species: &str, at: UVec2, stock: f64, origin: &str) -> Value {
    json!({ "id": id, "species": species, "at": [at.x, at.y], "stock": stock, "origin": origin })
}

/// The shipped scenario with `herds` as its herds section.
fn with_herds(herds: Vec<Value>) -> String {
    let mut file = shipped();
    file["herds"] = json!(herds);
    file.to_string()
}

fn herds_of(app: &App) -> Vec<Herd> {
    app.world.resource::<HerdRegistry>().herds.clone()
}

fn fauna_of(app: &App) -> std::sync::Arc<FaunaConfig> {
    app.world.resource::<FaunaConfigHandle>().get()
}

/// A species key and tile the generated world proves hospitable: where a non-migratory herbivore
/// stands, read off the world the shipped scenario builds so a worldgen retune moves it with the map.
fn hospitable_tile(app: &App) -> (String, UVec2) {
    let fauna = fauna_of(app);
    herds_of(app)
        .iter()
        .find_map(|herd| {
            let key = fauna.species_key_by_display(&herd.species)?;
            let def = fauna.species.get(key)?;
            (!def.migratory
                && def.diet == Diet::Herbivore
                && def.adjacent_water.satisfied_by(false, false))
            .then(|| (key.to_string(), herd.position()))
        })
        .expect("the map has a wild herbivore on dry ground")
}

#[test]
fn a_placed_herd_lands_where_it_was_put() {
    let (control, _) = load(SHIPPED).expect("the control world loads");
    let (key, at) = hospitable_tile(&control);
    let text = with_herds(vec![herd_entry("cattle", &key, at, 0.6, "placed")]);
    let (app, applied) = load(&text).expect("a herd on ground its species hosts is placed");
    let runtime = applied.herds.get("cattle").expect("the id is mapped");
    let herd = herds_of(&app)
        .into_iter()
        .find(|herd| &herd.id == runtime)
        .expect("the herd is in the registry");
    let fauna = fauna_of(&app);
    let table_capacity = fauna.species[&key].carrying_capacity();
    let capacity = herd.carrying_capacity;
    assert_eq!(herd.position(), at);
    // `stock` is a fraction of the K the herd lives under, which is the ecological one, not the
    // species table's. The inequality keeps the check from passing on the table number.
    assert!(
        (capacity - table_capacity).abs() > 1e-3 * table_capacity,
        "ecological K {capacity} must differ from the table K {table_capacity}"
    );
    assert!(
        (herd.biomass - 0.6 * capacity).abs() < 1e-3 * capacity,
        "{}",
        herd.biomass
    );
    assert!(herd.owner.is_none(), "a placed herd is wild");
    assert!(runtime.starts_with("scenario_"), "{runtime}");
    assert_eq!(
        herds_of(&app).len(),
        herds_of(&control).len() + 1,
        "one herd more than the control world"
    );
}

#[test]
fn a_herd_on_ground_its_species_does_not_hold_is_refused_with_the_habitat_reason() {
    let (mut app, _) = load(SHIPPED).expect("the control world loads");
    let sea = app
        .world
        .query::<&Tile>()
        .iter(&app.world)
        .find(|tile| tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER))
        .map(|tile| tile.position)
        .expect("the map has water");
    let text = with_herds(vec![herd_entry("cattle", "aurochs", sea, 0.5, "placed")]);
    let refusals = load(&text)
        .map(|_| ())
        .expect_err("an aurochs on the sea is refused");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "herds[0].at");
    assert!(
        refusals[0]
            .reason
            .contains(&format!("aurochs at ({},{}): tile is", sea.x, sea.y)),
        "{}",
        refusals[0]
    );

    // A seal on land that is not beside salt water: the shore rule, found by the shared predicate.
    let fauna = fauna_of(&app);
    let seal = fauna.species.get("seal").expect("the roster has a seal");
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let size = app.world.resource::<SimulationConfig>().grid_size;
    let mut state: SystemState<(Query<&Tile>, Res<TileRegistry>)> =
        SystemState::new(&mut app.world);
    let (tiles, registry) = state.get(&app.world);
    let inland = tiles
        .iter()
        .map(|tile| tile.position)
        .find(|pos| {
            matches!(
                habitat_refusal(
                    seal,
                    *pos,
                    size.x,
                    size.y,
                    wrap,
                    &registry,
                    &tiles,
                    &PreyReach::NONE
                ),
                Some(HabitatRefusal::NeedsShore(_))
            )
        })
        .expect("the map has a seal-hosting tile with no salt water beside it");
    let text = with_herds(vec![herd_entry("seals", "seal", inland, 0.5, "placed")]);
    let refusals = load(&text)
        .map(|_| ())
        .expect_err("a seal inland is refused");
    assert_eq!(refusals[0].path, "herds[0].at");
    assert!(
        refusals[0].reason.contains("needs salt water beside it"),
        "{}",
        refusals[0]
    );
}

#[test]
fn an_unknown_species_and_a_repeated_id_are_refused_in_pass_one() {
    let at = UVec2::new(1, 1);
    let text = with_herds(vec![
        herd_entry("a", "unicorn", at, 0.5, "placed"),
        herd_entry("a", "deer", at, 0.5, "placed"),
    ]);
    let refusals = pass_one(&text).expect_err("refused");
    let paths: Vec<&str> = refusals.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains(&"herds[0].species"), "{paths:?}");
    assert!(paths.contains(&"herds[1].id"), "{paths:?}");
    let species = refusals
        .iter()
        .find(|r| r.path == "herds[0].species")
        .unwrap();
    assert!(
        species.reason.contains("'unicorn' is not a fauna species"),
        "{species}"
    );

    let mut file = shipped();
    file.as_object_mut().unwrap().remove("herds");
    assert!(pass_one(&file.to_string()).is_err(), "herds is required");
}

#[test]
fn stock_outside_zero_to_one_is_refused_in_pass_one_and_a_stock_under_the_floor_at_fit() {
    for bad in [0.0, 1.5, -0.1] {
        let text = with_herds(vec![herd_entry(
            "a",
            "deer",
            UVec2::new(1, 1),
            bad,
            "placed",
        )]);
        let refusals = pass_one(&text).expect_err("refused");
        assert_eq!(refusals[0].path, "herds[0].stock", "stock {bad}");
    }
    let (control, _) = load(SHIPPED).expect("the control world loads");
    let (key, at) = hospitable_tile(&control);
    let text = with_herds(vec![herd_entry("a", &key, at, 1e-6, "placed")]);
    let refusals = load(&text)
        .map(|_| ())
        .expect_err("a stock under the extinction floor");
    assert_eq!(refusals[0].path, "herds[0].stock");
    assert!(
        refusals[0].reason.contains("extinction floor"),
        "{}",
        refusals[0]
    );
}

/// How far above or below the extinction floor the boundary tests put a stock.
const FLOOR_MARGIN: f32 = 0.25;
/// The step, as a fraction of the floor, the boundary search climbs by, and how many steps it takes.
const FLOOR_SEARCH_STEP: f32 = 0.01;
const FLOOR_SEARCH_STEPS: u32 = 100;

#[test]
fn a_placed_stock_just_above_the_floor_survives_a_turn_and_one_below_is_refused() {
    let (control, _) = load(SHIPPED).expect("the control world loads");
    let (key, at) = hospitable_tile(&control);
    let probe = with_herds(vec![herd_entry("probe", &key, at, 0.6, "placed")]);
    let (probe_app, applied) = load(&probe).expect("the probe loads");
    let runtime = applied.herds.get("probe").expect("mapped").clone();
    let fauna = fauna_of(&probe_app);
    let placed = herds_of(&probe_app)
        .into_iter()
        .find(|herd| herd.id == runtime)
        .expect("placed");
    let floor = core_sim::herd_ecology(&placed, &fauna).extinction_floor;
    assert!(
        (placed.carrying_capacity - fauna.species[&key].carrying_capacity()).abs() > 1.0,
        "the fixture must have an ecological K apart from the table K"
    );

    // Under `collapse_fraction` of K the herd loses `collapse_rate` of itself on its first turn,
    // so a stock just over the static floor dies on turn 1 and the loader refuses it. The boundary
    // is found by search against the loader itself, not by restating the formula.
    let static_edge = with_herds(vec![herd_entry(
        "edge",
        &key,
        at,
        (floor * (1.0 + FLOOR_MARGIN)) as f64,
        "placed",
    )]);
    let refusals = load(&static_edge)
        .map(|_| ())
        .expect_err("a stock over the static floor that dies on turn 1 is refused");
    assert_eq!(refusals[0].path, "herds[0].stock");
    assert!(refusals[0].reason.contains("first turn"), "{}", refusals[0]);
    let accepted = (1..=FLOOR_SEARCH_STEPS)
        .map(|step| floor * (1.0 + FLOOR_SEARCH_STEP * step as f32))
        .find(|stock| {
            load(&with_herds(vec![herd_entry(
                "edge",
                &key,
                at,
                *stock as f64,
                "placed",
            )]))
            .is_ok()
        })
        .expect("some stock a little above the floor is accepted");
    let above = with_herds(vec![herd_entry(
        "above",
        &key,
        at,
        accepted as f64,
        "placed",
    )]);
    let (mut app, applied) = load(&above).expect("a stock just above the floor is placed");
    let runtime = applied.herds.get("above").expect("mapped").clone();
    run_turn(&mut app);
    assert!(
        herds_of(&app).iter().any(|herd| herd.id == runtime),
        "the herd survives its first turn"
    );

    let below = with_herds(vec![herd_entry(
        "below",
        &key,
        at,
        (floor * (1.0 - FLOOR_MARGIN)) as f64,
        "placed",
    )]);
    let refusals = load(&below)
        .map(|_| ())
        .expect_err("a stock under the floor is refused");
    assert_eq!(refusals[0].path, "herds[0].stock");
}

/// Every tile of the world, in scan order.
fn all_tiles(app: &App) -> Vec<UVec2> {
    let size = app.world.resource::<SimulationConfig>().grid_size;
    (0..size.y)
        .flat_map(|y| (0..size.x).map(move |x| UVec2::new(x, y)))
        .collect()
}

/// The tiles within `radius` hexes of `centre`.
fn within(app: &App, centre: UVec2, radius: u32) -> Vec<UVec2> {
    let config = app.world.resource::<SimulationConfig>();
    let (width, wrap) = (config.grid_size.x, config.map_topology.wrap_horizontal);
    all_tiles(app)
        .into_iter()
        .filter(|tile| {
            core_sim::grid_utils::hex_distance_wrapped(centre, *tile, width, wrap) <= radius
        })
        .collect()
}

/// The habitat verdict for `species` on `pos` with no prey in reach: a carnivore reads
/// `TooLittlePrey` only once every other test has passed.
fn habitat_without_prey(app: &mut App, species: &str, pos: UVec2) -> Option<HabitatRefusal> {
    let fauna = fauna_of(app);
    let config = app.world.resource::<SimulationConfig>();
    let (size, wrap) = (config.grid_size, config.map_topology.wrap_horizontal);
    let mut state: SystemState<(Query<&Tile>, Res<TileRegistry>)> =
        SystemState::new(&mut app.world);
    let (tiles, registry) = state.get(&app.world);
    habitat_refusal(
        &fauna.species[species],
        pos,
        size.x,
        size.y,
        wrap,
        &registry,
        &tiles,
        &PreyReach::NONE,
    )
}

#[test]
fn a_wolf_is_counted_against_the_herbivore_placed_beside_it() {
    let (mut control, _) = load(SHIPPED).expect("the control world loads");
    let mut tried = 0;
    for wolf_at in all_tiles(&control) {
        if habitat_without_prey(&mut control, "wolf", wolf_at)
            != Some(HabitatRefusal::TooLittlePrey)
        {
            continue;
        }
        let alone = with_herds(vec![herd_entry("wolf", "wolf", wolf_at, 0.8, "placed")]);
        if load(&alone).is_ok() {
            continue;
        }
        for boar_at in within(&control, wolf_at, 1) {
            if boar_at == wolf_at || habitat_without_prey(&mut control, "boar", boar_at).is_some() {
                continue;
            }
            tried += 1;
            // The wolf is listed FIRST: the order of the file does not matter.
            let both = with_herds(vec![
                herd_entry("wolf", "wolf", wolf_at, 0.8, "placed"),
                herd_entry("boar", "boar", boar_at, 0.5, "placed"),
            ]);
            if let Ok((_, applied)) = load(&both) {
                assert!(applied.herds.contains_key("wolf"));
                return;
            }
        }
    }
    panic!("no wolf tile was fed by a placed boar ({tried} pairs tried)");
}

#[test]
fn a_wolf_is_not_fed_by_a_worldgen_prey_herd_the_file_thins() {
    let (mut control, _) = load(SHIPPED).expect("the control world loads");
    let fauna = fauna_of(&control);
    let attack = fauna.species["wolf"].combat.attack;
    let all = herds_of(&control);
    let prey: Vec<(String, UVec2)> = all
        .iter()
        .filter(|herd| herd.owner.is_none())
        .filter_map(|herd| {
            let key = fauna.species_key_by_display(&herd.species)?;
            let def = fauna.species.get(key)?;
            let alone_here = all
                .iter()
                .filter(|other| {
                    other.species == herd.species && other.position() == herd.position()
                })
                .count()
                == 1;
            (def.diet == Diet::Herbivore && def.combat.defense <= attack && alone_here)
                .then(|| (key.to_string(), herd.position()))
        })
        .collect();
    let mut tried = 0;
    for (key, prey_at) in prey {
        for wolf_at in within(&control, prey_at, 2) {
            if habitat_without_prey(&mut control, "wolf", wolf_at)
                != Some(HabitatRefusal::TooLittlePrey)
            {
                continue;
            }
            tried += 1;
            let file = |stock: f64| {
                with_herds(vec![
                    herd_entry("prey", &key, prey_at, stock, "worldgen"),
                    herd_entry("wolf", "wolf", wolf_at, 0.8, "placed"),
                ])
            };
            // Half the herd's K is its MSY point, where it feeds the most; a stock just over the
            // floor feeds almost nothing.
            if load(&file(0.5)).is_err() {
                continue;
            }
            // Other prey in reach can still carry the wolf; keep looking for a wolf tile this
            // one herd decides.
            let Err(refusals) = load(&file(0.03)).map(|_| ()) else {
                continue;
            };
            assert_eq!(refusals[0].path, "herds[1].at");
            assert!(
                refusals[0].reason.contains("too little prey"),
                "{}",
                refusals[0]
            );
            return;
        }
    }
    panic!("no wolf tile was fed by a prey herd at its best and not at a trickle ({tried} tried)");
}

#[test]
fn the_herd_output_feed_line_reads_the_share_as_a_percentage() {
    let line = core_sim::command_seam::herd_output_feed_line("flock", 0.5);
    assert!(line.contains("give 50% of itself"), "{line}");
    assert!(core_sim::command_seam::herd_output_feed_line("flock", 1.0).contains("give 100% "));
}

/// A worldgen herd that is the only one of its species on its tile.
fn lone_worldgen_herd(app: &App) -> (String, Herd) {
    let fauna = fauna_of(app);
    let herds = herds_of(app);
    herds
        .iter()
        .find(|herd| {
            herd.owner.is_none()
                && herds
                    .iter()
                    .filter(|other| {
                        other.species == herd.species && other.position() == herd.position()
                    })
                    .count()
                    == 1
                && fauna.species_key_by_display(&herd.species).is_some()
        })
        .map(|herd| {
            (
                fauna
                    .species_key_by_display(&herd.species)
                    .unwrap()
                    .to_string(),
                herd.clone(),
            )
        })
        .expect("the map has an unowned herd")
}

#[test]
fn a_worldgen_herd_is_set_to_the_stock_and_its_phase_follows() {
    let (control, _) = load(SHIPPED).expect("the control world loads");
    let (key, herd) = lone_worldgen_herd(&control);
    let text = with_herds(vec![herd_entry(
        "depleted",
        &key,
        herd.position(),
        0.2,
        "worldgen",
    )]);
    let (app, applied) = load(&text).expect("the named herd is set");
    let runtime = applied.herds.get("depleted").expect("mapped");
    assert_eq!(
        runtime, &herd.id,
        "it is the world's own herd, not a new one"
    );
    let set = herds_of(&app)
        .into_iter()
        .find(|h| &h.id == runtime)
        .unwrap();
    assert!(
        (set.biomass - 0.2 * herd.carrying_capacity).abs() < 1e-3 * herd.carrying_capacity,
        "{} of {}",
        set.biomass,
        herd.carrying_capacity
    );
    assert_ne!(
        set.ecology_phase,
        EcologyPhase::Thriving,
        "a 0.2 stock is not thriving"
    );
    assert_eq!(
        herds_of(&app).len(),
        herds_of(&control).len(),
        "no herd was added"
    );

    // A tile with no such herd is refused, naming the nearest.
    let empty = [UVec2::new(0, 0), UVec2::new(79, 51)]
        .into_iter()
        .find(|pos| {
            !herds_of(&control)
                .iter()
                .any(|h| h.species == herd.species && h.position() == *pos)
        })
        .unwrap();
    let text = with_herds(vec![herd_entry("depleted", &key, empty, 0.2, "worldgen")]);
    let refusals = load(&text).map(|_| ()).expect_err("no herd there");
    assert_eq!(refusals[0].path, "herds[0].at");
    assert!(
        refusals[0].reason.contains("the nearest is at ("),
        "{}",
        refusals[0]
    );
    assert!(
        refusals[0]
            .reason
            .contains(&format!("no {} at", herd.species)),
        "{}",
        refusals[0]
    );
}

#[test]
fn every_refusal_in_a_file_is_reported_together() {
    let (mut control, _) = load(SHIPPED).expect("the control world loads");
    let sea = control
        .world
        .query::<&Tile>()
        .iter(&control.world)
        .find(|tile| tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER))
        .map(|tile| tile.position)
        .expect("the map has water");
    let mut file = shipped();
    file["herds"] = json!([herd_entry("cattle", "aurochs", sea, 0.5, "placed")]);
    file["factions"][0]["bands"][0]["at"] = json!([sea.x, sea.y]);
    let refusals = load(&file.to_string()).map(|_| ()).expect_err("two faults");
    let paths: Vec<&str> = refusals.iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains(&"herds[0].at"), "{paths:?}");
    assert!(paths.contains(&"factions[0].bands[0].at"), "{paths:?}");
}

// ---------------------------------------------------------------------------------------------
// Slice 2c: the pens section, and herds named by scenario id
// ---------------------------------------------------------------------------------------------

const PLAYTEST: &str = include_str!("../scenarios/held_rungs_playtest.json");
/// The scenario id the playtest file gives the wild sheep, and the runtime id of the same herd.
const SHEEP: &str = "sheep";
const SHEEP_RUNTIME: &str = "game_wild_sheep_00";
/// The share a `set_herd_output` entry asks for.
const ASKED_SHARE: f32 = 0.5;
/// A fraction above the accepted range.
const OVER_RANGE: f64 = 1.5;
/// A fraction below the accepted range.
const UNDER_RANGE: f64 = -0.1;

/// The playtest scenario with only its held pen kept (or none), plus `labor` and `pens`.
fn pen_scenario(held_pen: bool, labor: Vec<Value>, pens: Vec<Value>) -> Value {
    let mut file: Value = serde_json::from_str(PLAYTEST).expect("the playtest file is JSON");
    file["factions"][0]["rungs"]
        .as_array_mut()
        .expect("rungs")
        .retain(|entry| held_pen && entry["held"]["rung"] == "animal:pen");
    file["factions"][0]["labor"] = Value::Array(labor);
    file["factions"][0]["pens"] = Value::Array(pens);
    file
}

fn extend_entry(herd: &str) -> Value {
    json!({ "extend_pen": { "herd": herd } })
}

fn output_entry(herd: &str, fraction: f64) -> Value {
    json!({ "set_herd_output": { "herd": herd, "fraction": fraction } })
}

/// Every declared build on the home faction's bands that matches `wanted`.
fn declared_builds(app: &mut App, wanted: fn(&BuildJob) -> bool) -> usize {
    app.world
        .query::<(&PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(cohort, _)| cohort.faction == HOME)
        .flat_map(|(_, allocation)| allocation.builds.iter())
        .filter(|declaration| wanted(&declaration.declared))
        .count()
}

#[test]
fn extend_pen_starts_the_ring_and_a_keeper_carries_the_declaration() {
    let file = pen_scenario(true, vec![], vec![extend_entry(SHEEP)]);
    let (mut app, applied) = load(&file.to_string()).expect("the pen is extended");
    let runtime = applied.herds.get(SHEEP).expect("the herd is mapped");
    assert_eq!(runtime, SHEEP_RUNTIME);
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(runtime)
        .expect("the herd")
        .clone();
    assert!(herd.is_corralled(), "the held pen stands");
    assert!(herd.pen_extending, "the ring is under way");
    assert_eq!(
        declared_builds(&mut app, |job| matches!(job, BuildJob::ExtendPen)),
        1,
        "the keeper band carries the declaration"
    );

    // Control: without the entry nothing is extending and nothing is declared.
    let file = pen_scenario(true, vec![], vec![]);
    let (mut app, _) = load(&file.to_string()).expect("the control loads");
    assert!(
        !app.world
            .resource::<HerdRegistry>()
            .find(SHEEP_RUNTIME)
            .expect("the herd")
            .pen_extending
    );
    assert_eq!(
        declared_builds(&mut app, |job| matches!(job, BuildJob::ExtendPen)),
        0
    );
}

#[test]
fn set_herd_output_puts_the_recommit_in_flight_at_the_asked_share() {
    let file = pen_scenario(
        true,
        vec![],
        vec![output_entry(SHEEP, f64::from(ASKED_SHARE))],
    );
    let (mut app, _) = load(&file.to_string()).expect("the commitment is made");
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(SHEEP_RUNTIME)
        .expect("the herd")
        .clone();
    assert_eq!(herd.standing_output_target, Some(ASKED_SHARE));
    assert_eq!(
        declared_builds(&mut app, |job| matches!(job, BuildJob::SetHerdOutput(_))),
        1
    );
}

#[test]
fn pass_one_refuses_an_undeclared_herd_and_a_fraction_out_of_range_in_one_load() {
    let file = pen_scenario(
        true,
        vec![],
        vec![
            extend_entry("ghost"),
            output_entry(SHEEP, OVER_RANGE),
            output_entry("phantom", UNDER_RANGE),
        ],
    );
    let refusals = pass_one(&file.to_string()).expect_err("refused");
    let paths: Vec<&str> = refusals.iter().map(|r| r.path.as_str()).collect();
    assert!(
        paths.contains(&"factions[0].pens[0].extend_pen.herd"),
        "{paths:?}"
    );
    assert!(
        paths.contains(&"factions[0].pens[1].set_herd_output.fraction"),
        "{paths:?}"
    );
    assert!(
        paths.contains(&"factions[0].pens[2].set_herd_output.herd"),
        "{paths:?}"
    );
    assert!(
        paths.contains(&"factions[0].pens[2].set_herd_output.fraction"),
        "{paths:?}"
    );
    assert!(
        !paths.contains(&"factions[0].pens[1].set_herd_output.herd"),
        "a declared herd is not refused: {paths:?}"
    );
}

#[test]
fn a_pens_entry_the_verb_refuses_stops_the_load_with_the_verbs_own_reason() {
    // No pen: the herd stands wild.
    let file = pen_scenario(false, vec![], vec![extend_entry(SHEEP)]);
    let refusals = load(&file.to_string())
        .map(|_| ())
        .expect_err("a herd with no pen cannot be extended");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "factions[0].pens[0]");
    assert!(
        refusals[0].reason.contains("no pen to extend"),
        "{}",
        refusals[0].reason
    );

    // A kept herd and nobody keeping it: the elders keep the pen beside the band, so this is a tamed
    // herd, the penable one that stands farthest from the band.
    let (mut control, _) =
        load(&pen_scenario(false, vec![], vec![]).to_string()).expect("the control loads");
    let band_tile = {
        let mut query = control.world.query::<(&BandId, &PopulationCohort)>();
        let (_, cohort) = query.iter(&control.world).next().expect("the band");
        control
            .world
            .get::<Tile>(cohort.current_tile)
            .expect("its tile")
            .position
    };
    let fauna = fauna_of(&control);
    let ladder = control.world.resource::<LadderConfigHandle>().get();
    let herds = herds_of(&control);
    let far = herds
        .iter()
        .filter(|herd| herd.owner.is_none() && herd.can_pen(&ladder) && herd.id != SHEEP_RUNTIME)
        .filter(|herd| {
            herds
                .iter()
                .filter(|o| o.species == herd.species && o.position() == herd.position())
                .count()
                == 1
        })
        .max_by_key(|herd| {
            let at = herd.position();
            (
                at.x.abs_diff(band_tile.x) + at.y.abs_diff(band_tile.y),
                herd.id.clone(),
            )
        })
        .expect("a penable herd far from the band");
    let at = far.position();
    let mut file = pen_scenario(
        true,
        vec![],
        vec![output_entry("far", f64::from(ASKED_SHARE))],
    );
    file["herds"] = json!([{
        "id": "far",
        "species": fauna.species_key_by_display(&far.species).expect("a species key"),
        "at": [at.x, at.y],
        "stock": far.biomass / herd_capacity(far, &fauna),
        "origin": "worldgen",
    }]);
    file["factions"][0]["rungs"][0]["held"] = json!({ "rung": "animal:pastoral", "herd": "far" });
    let refusals = load(&file.to_string())
        .map(|_| ())
        .expect_err("a herd with no keeper cannot be re-sorted");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "factions[0].pens[0]");
    assert_eq!(
        refusals[0].reason,
        format!(
            "No band is keeping {}. Assign herders to it first, then set its output.",
            far.id
        )
    );
}

#[test]
fn held_rungs_name_a_herd_by_its_scenario_id() {
    let (app, applied) = load(PLAYTEST).expect("the converted playtest file loads");
    let runtime = applied.herds.get(SHEEP).expect("the sheep are mapped");
    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(runtime)
            .expect("the herd")
            .is_corralled(),
        "the held pen is on the herd the id names"
    );

    // A runtime id is not a scenario id: it is refused in pass 1, held and built alike.
    let mut file: Value = serde_json::from_str(PLAYTEST).expect("JSON");
    file["factions"][0]["rungs"][1]["held"]["herd"] = json!(SHEEP_RUNTIME);
    let refusals = pass_one(&file.to_string()).expect_err("an undeclared herd id");
    assert!(
        refusals
            .iter()
            .any(|r| r.path == "factions[0].rungs[1].held.herd"),
        "{refusals:?}"
    );
    let mut file: Value = serde_json::from_str(PLAYTEST).expect("JSON");
    file["factions"][0]["rungs"][3]["build_rung"]["herd_id"] = json!("ghost");
    let refusals = pass_one(&file.to_string()).expect_err("an undeclared herd id");
    assert!(
        refusals
            .iter()
            .any(|r| r.path == "factions[0].rungs[3].build_rung.herd_id"),
        "{refusals:?}"
    );
}

const HERDS_PENS: &str = include_str!("../scenarios/herds_pens_playtest.json");
/// How close a float stock must land to the file's, as a fraction of the herd's capacity.
const STOCK_TOLERANCE: f32 = 1e-3;
/// How many turns the playtest band gets to craft its hurdles and start the ring.
const RING_BUILT_WITHIN_TURNS: u32 = 40;

#[test]
fn the_herds_and_pens_playtest_file_places_thins_extends_and_recommits() {
    let (app, applied) = load(HERDS_PENS).expect("the shipped herds and pens playtest loads");
    let file: Value = serde_json::from_str(HERDS_PENS).expect("JSON");
    let entry = |id: &str| {
        file["herds"]
            .as_array()
            .expect("herds")
            .iter()
            .find(|herd| herd["id"] == id)
            .unwrap_or_else(|| panic!("the file declares {id}"))
            .clone()
    };
    let fauna = fauna_of(&app);
    let herds = herds_of(&app);
    let runtime = |id: &str| {
        let runtime = applied.herds.get(id).expect("the id is mapped");
        herds
            .iter()
            .find(|herd| &herd.id == runtime)
            .expect("the herd is in the registry")
            .clone()
    };

    let placed = runtime("new_aurochs");
    let at = entry("new_aurochs")["at"].clone();
    assert_eq!(
        placed.position(),
        UVec2::new(
            at[0].as_u64().unwrap() as u32,
            at[1].as_u64().unwrap() as u32
        )
    );
    assert!(placed.owner.is_none(), "a placed herd is wild");

    let thin = runtime("thin_boar");
    let stock = entry("thin_boar")["stock"].as_f64().unwrap() as f32;
    assert!(
        (thin.biomass / herd_capacity(&thin, &fauna) - stock).abs() < STOCK_TOLERANCE,
        "the worldgen herd holds the file's stock"
    );

    // The sheep's ring is the declaration its keeper's row carries: nothing replaced it.
    let sheep = runtime("sheep");
    assert!(sheep.is_corralled(), "the held pen stands");
    assert!(sheep.pen_extending, "the pen is widening");
    assert_eq!(sheep.standing_output_target, None);
    let mut app = app;
    let sheep_source = BuildSource::Herd(sheep.id.clone());
    let sheep_jobs: Vec<BuildJob> = app
        .world
        .query::<&LaborAllocation>()
        .iter(&app.world)
        .filter_map(|allocation| allocation.build_declaration(&sheep_source))
        .map(|declaration| declaration.declared)
        .collect();
    assert!(
        !sheep_jobs.is_empty()
            && sheep_jobs
                .iter()
                .all(|job| matches!(job, BuildJob::ExtendPen)),
        "the sheep's row declares the ring: {sheep_jobs:?}"
    );

    // The flock is tamed and kept, and its re-sort is in flight at the asked share.
    let flock = runtime("flock");
    assert_eq!(flock.owner, Some(HOME));
    assert!(flock.is_domesticated());
    assert_eq!(flock.standing_output_target, Some(ASKED_SHARE));
    let flock_source = BuildSource::Herd(flock.id.clone());
    assert!(
        app.world
            .query::<&LaborAllocation>()
            .iter(&app.world)
            .any(|allocation| matches!(
                allocation
                    .build_declaration(&flock_source)
                    .map(|d| d.declared),
                Some(BuildJob::SetHerdOutput(_))
            )),
        "the flock's keeper declares the re-sort"
    );

    // The band was outfitted wood and holds the tanning craft, and auto-craft is on, so its bench
    // makes the hurdles the ring draws: within a bounded number of turns the ring has builders and
    // is working (its meter moves), where without hurdles it would sit blocked on materials.
    let ring_staffed = |app: &mut App| {
        app.world
            .query::<&LaborAllocation>()
            .iter(&app.world)
            .filter_map(|allocation| allocation.build_crew(&sheep_source))
            .map(|crew| crew.total())
            .sum::<u32>()
    };
    let mut built_by = None;
    for turn in 1..=RING_BUILT_WITHIN_TURNS {
        run_turn(&mut app);
        let herd = sheep_of(&app, &sheep.id);
        let worked = herd.pen_extend_progress > 0.0 || herd.pen_radius > sheep.pen_radius;
        if ring_staffed(&mut app) > 0 && worked {
            built_by = Some(turn);
            break;
        }
    }
    assert!(
        built_by.is_some(),
        "the ring has builders and moves within {RING_BUILT_WITHIN_TURNS} turns"
    );
}

// ---------------------------------------------------------------------------------------------
// A second verb on a herd replaces the first in place, and takes the first's in-flight state
// ---------------------------------------------------------------------------------------------

/// The playtest world with its held sheep pen, kept by the home band, and no pen verb replayed.
fn kept_pen_world() -> (App, String, UVec2) {
    let (app, applied) =
        load(&pen_scenario(true, vec![], vec![]).to_string()).expect("the pen world loads");
    let id = applied.herds.get(SHEEP).expect("the sheep").clone();
    let pen = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .and_then(|herd| herd.corralled_at)
        .expect("the held pen stands");
    (app, id, pen)
}

fn sheep_of(app: &App, id: &str) -> Herd {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the herd")
        .clone()
}

/// What every home band's declaration on the herd says, in band order.
fn declared_on(app: &mut App, id: &str) -> Vec<BuildJob> {
    let source = BuildSource::Herd(id.to_string());
    app.world
        .query::<&LaborAllocation>()
        .iter(&app.world)
        .filter_map(|allocation| allocation.build_declaration(&source))
        .map(|declaration| declaration.declared)
        .collect()
}

#[test]
fn an_extend_pen_then_a_set_herd_output_cancels_the_ring_and_keeps_the_recommit() {
    let (mut app, id, pen) = kept_pen_world();
    apply_extend_pen(&mut app.world, HOME, pen).expect("the ring starts");
    assert!(
        sheep_of(&app, &id).pen_extending,
        "control: the ring is in flight"
    );
    apply_set_herd_output(&mut app.world, HOME, &id, ASKED_SHARE).expect("the recommit starts");

    let herd = sheep_of(&app, &id);
    assert!(
        !herd.pen_extending,
        "the replaced ring is cancelled, not stranded"
    );
    assert_eq!(herd.standing_output_target, Some(ASKED_SHARE));
    let jobs = declared_on(&mut app, &id);
    assert!(
        !jobs.is_empty()
            && jobs
                .iter()
                .all(|job| matches!(job, BuildJob::SetHerdOutput(_))),
        "the row declares the recommit: {jobs:?}"
    );
}

#[test]
fn a_set_herd_output_then_an_extend_pen_cancels_the_recommit_and_keeps_the_ring() {
    let (mut app, id, pen) = kept_pen_world();
    apply_set_herd_output(&mut app.world, HOME, &id, ASKED_SHARE).expect("the recommit starts");
    assert_eq!(
        sheep_of(&app, &id).standing_output_target,
        Some(ASKED_SHARE)
    );
    apply_extend_pen(&mut app.world, HOME, pen).expect("the ring starts");

    let herd = sheep_of(&app, &id);
    assert_eq!(
        herd.standing_output_target, None,
        "the replaced recommit is cancelled"
    );
    assert!(herd.pen_extending);
    let jobs = declared_on(&mut app, &id);
    assert!(
        !jobs.is_empty() && jobs.iter().all(|job| matches!(job, BuildJob::ExtendPen)),
        "the row declares the ring: {jobs:?}"
    );
}

#[test]
fn restating_the_same_kind_of_job_cancels_nothing() {
    let (mut app, id, pen) = kept_pen_world();
    apply_extend_pen(&mut app.world, HOME, pen).expect("the ring starts");
    let keeper = LaborTarget::Hunt {
        fauna_id: id.clone(),
        floor: SOURCE_NAMED_NOT_ASSIGNED,
        move_with_herd: false,
    };
    // The ring declared again, as a restated order: the ring is still the herd's.
    queue_build_on_working_bands(&mut app.world, HOME, &keeper, BuildJob::ExtendPen);
    assert!(
        sheep_of(&app, &id).pen_extending,
        "a restated ring keeps its ring"
    );

    // A Corral restated for another rung: the chosen pen site stays.
    let site = UVec2::new(1, 1);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the herd");
        herd.pen_site = Some(site);
    }
    let climb = |rung| BuildDeclaration {
        source: BuildSource::Herd(id.clone()),
        declared: BuildJob::Climb(rung),
    };
    let pen_rung = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .enclosed_herd_rung()
        .rung_id();
    cancel_replaced_job(
        &mut app.world.resource_mut::<HerdRegistry>(),
        Some(climb(pen_rung)),
        BuildJob::Climb(pen_rung),
    );
    assert_eq!(
        sheep_of(&app, &id).pen_site,
        Some(site),
        "a restated Climb keeps its site"
    );

    // Control: a Climb replaced by a different kind of job loses its site.
    cancel_replaced_job(
        &mut app.world.resource_mut::<HerdRegistry>(),
        Some(climb(pen_rung)),
        BuildJob::ExtendPen,
    );
    assert_eq!(sheep_of(&app, &id).pen_site, None);
}

#[test]
fn a_labor_row_names_its_herd_by_scenario_id_and_targets_the_runtime_herd() {
    let (mut app, applied) = load(HERDS_PENS).expect("the shipped file loads");
    let runtime = applied
        .herds
        .get("flock")
        .expect("the flock is mapped")
        .clone();
    assert_eq!(
        runtime, "game_fowl_71",
        "the scenario id is not the runtime id"
    );
    let hunting_it = app
        .world
        .query::<&LaborAllocation>()
        .iter(&app.world)
        .flat_map(|allocation| allocation.assignments.iter())
        .any(
            |row| matches!(&row.target, LaborTarget::Hunt { fauna_id, .. } if *fauna_id == runtime),
        );
    assert!(hunting_it, "the band's hunt row targets the runtime herd");
}

#[test]
fn a_labor_entry_naming_an_undeclared_herd_is_refused_in_pass_one() {
    // The runtime id is not a scenario id.
    let mut file: Value = serde_json::from_str(HERDS_PENS).expect("JSON");
    file["factions"][0]["labor"][0]["assign_labor"]["fauna_id"] = json!("game_fowl_71");
    let refusals = pass_one(&file.to_string()).expect_err("an undeclared herd id");
    assert!(
        refusals
            .iter()
            .any(|r| r.path == "factions[0].labor[0].assign_labor.fauna_id"),
        "{refusals:?}"
    );

    let mut file: Value = serde_json::from_str(HERDS_PENS).expect("JSON");
    file["factions"][0]["labor"] = json!([{ "work_priority": {
        "band": "home", "target_x": null, "target_y": null, "herd_id": "ghost",
        "material": null, "branch": null, "level": "high" } }]);
    let refusals = pass_one(&file.to_string()).expect_err("an undeclared herd id");
    assert!(
        refusals
            .iter()
            .any(|r| r.path == "factions[0].labor[0].work_priority.herd_id"),
        "{refusals:?}"
    );
}

/// Whether the playtest sheep's ring is being worked within the turn bound, for a file.
fn ring_worked_within_bound(text: &str) -> bool {
    let (mut app, applied) = load(text).expect("the file loads");
    let id = applied.herds.get(SHEEP).expect("the sheep").clone();
    let source = BuildSource::Herd(id.clone());
    (0..RING_BUILT_WITHIN_TURNS).any(|_| {
        run_turn(&mut app);
        let herd = sheep_of(&app, &id);
        let staffed: u32 = app
            .world
            .query::<&LaborAllocation>()
            .iter(&app.world)
            .filter_map(|allocation| allocation.build_crew(&source))
            .map(|crew| crew.total())
            .sum();
        staffed > 0 && herd.pen_extend_progress > 0.0
    })
}

#[test]
fn the_playtest_ring_is_built_only_because_the_band_crafts_hurdles() {
    assert!(
        ring_worked_within_bound(HERDS_PENS),
        "the shipped file builds its ring"
    );
    // Control: the same file with auto-craft off never crafts the hurdles, so the ring stays blocked.
    let mut file: Value = serde_json::from_str(HERDS_PENS).expect("JSON");
    file["factions"][0]["bands"][0]["auto_craft"] = Value::Null;
    assert!(
        !ring_worked_within_bound(&file.to_string()),
        "without auto-craft the ring is blocked on hurdles"
    );
}

#[test]
fn a_band_with_auto_craft_true_loads_with_its_bench_auto_on_and_one_without_it_off() {
    let bench_auto = |app: &mut App| {
        app.world
            .query::<&core_sim::BandBench>()
            .iter(&app.world)
            .next()
            .is_some_and(|bench| bench.auto)
    };
    let (mut on, _) = load(HERDS_PENS).expect("the playtest loads");
    assert!(bench_auto(&mut on), "auto_craft true switches the bench on");
    let (mut off, _) = load(SHIPPED).expect("the shipped file loads");
    assert!(!bench_auto(&mut off), "an absent auto_craft leaves it off");
}

/// The turns the recommit job's countdown is read over: enough to see it fall, short of completion.
const COUNTDOWN_TURNS_READ: u32 = 5;

#[test]
fn a_set_herd_output_build_on_a_tamed_herd_counts_down_against_its_own_meter() {
    let (mut app, applied) = load(HERDS_PENS).expect("the playtest loads");
    let id = applied.herds.get("flock").expect("the flock").clone();
    let mut last_progress = 0.0_f32;
    let mut last_turns: Option<u32> = None;
    for turn in 1..=COUNTDOWN_TURNS_READ {
        run_turn(&mut app);
        let herd = sheep_of(&app, &id);
        let rate = herd.output_recommit_progress - last_progress;
        assert!(rate > 0.0, "turn {turn}: the commitment is being worked");
        let expected = core_sim::build_turns_remaining(
            herd.output_recommit_cost,
            herd.output_recommit_progress,
            rate,
        )
        .expect("a worked meter has a finish date");
        assert_eq!(
            herd.build_turns_remaining,
            Some(core_sim::BuildTurns::Turns(expected)),
            "turn {turn}: the countdown is the commitment's own remainder over its rate, not the \
             pen rung above it"
        );
        if let Some(before) = last_turns {
            assert!(expected < before, "turn {turn}: the countdown falls");
        }
        last_turns = Some(expected);
        last_progress = herd.output_recommit_progress;
    }
}

// ---------------------------------------------------------------------------------------------
// Slice 2d: the deposits section
// ---------------------------------------------------------------------------------------------

const WOOD: &str = "wood";

/// A tile of the shipped scenario's world that holds `material` (with its capacity), and one that
/// holds none of it, both read off the generated tiles.
fn wood_and_bare_tiles() -> ((UVec2, f32), UVec2) {
    let (mut app, _) = load(SHIPPED).expect("the shipped scenario loads");
    let config = app.world.resource::<ExtractionConfigHandle>().get();
    let tiles: Vec<Tile> = app
        .world
        .query::<&Tile>()
        .iter(&app.world)
        .cloned()
        .collect();
    let wooded = tiles
        .iter()
        .map(|tile| (tile.position, tile_deposit_capacity(&config, WOOD, tile)))
        .find(|(_, capacity)| *capacity > NO_DEPOSIT)
        .expect("the map has wood");
    let bare = tiles
        .iter()
        .find(|tile| tile_deposit_capacity(&config, WOOD, tile) <= NO_DEPOSIT)
        .map(|tile| tile.position)
        .expect("the map has ground without wood");
    (wooded, bare)
}

fn with_deposits(deposits: Value) -> String {
    let mut file = shipped();
    file["deposits"] = deposits;
    file.to_string()
}

#[test]
fn a_deposit_stock_is_set_exactly_and_the_first_turn_regrows_it() {
    let ((tile, capacity), _) = wood_and_bare_tiles();
    let stock = capacity * 0.5;
    let text = with_deposits(json!([{ "at": [tile.x, tile.y], "material": WOOD, "stock": stock }]));
    let (mut app, _) = load(&text).expect("a stock within capacity loads");
    let held = app
        .world
        .resource::<DepositRegistry>()
        .source(tile, WOOD)
        .expect("the working is opened")
        .stock;
    assert_eq!(held, stock, "the stock is exactly as written");
    run_turn(&mut app);
    let after = app
        .world
        .resource::<DepositRegistry>()
        .source(tile, WOOD)
        .expect("the working stands")
        .stock;
    assert!(
        after > stock && after <= capacity,
        "the first played turn is the first regrowth: {stock} -> {after}"
    );
}

#[test]
fn deposit_pass_one_refusals_are_collected_together() {
    let ((tile, _), _) = wood_and_bare_tiles();
    let at = json!([tile.x, tile.y]);
    let text = with_deposits(json!([
        { "at": at, "material": "unobtainium", "stock": 1.0 },
        { "at": at, "material": WOOD, "stock": -1.0 },
        { "at": at, "material": WOOD, "stock": 1.0 },
    ]));
    let refusals = pass_one(&text).expect_err("refused");
    let paths: Vec<&str> = refusals.iter().map(|r| r.path.as_str()).collect();
    for expected in ["deposits[0].material", "deposits[1].stock", "deposits[2]"] {
        assert!(
            paths.contains(&expected),
            "missing {expected}; got {paths:?}"
        );
    }
    let duplicate = refusals.iter().find(|r| r.path == "deposits[2]").unwrap();
    assert!(duplicate.reason.contains("deposits[1]"), "{duplicate}");
}

#[test]
fn deposit_fit_refusals_are_collected_and_a_refused_load_applies_nothing() {
    let ((tile, capacity), bare) = wood_and_bare_tiles();
    let text = with_deposits(json!([
        { "at": [bare.x, bare.y], "material": WOOD, "stock": 1.0 },
        { "at": [tile.x, tile.y], "material": WOOD, "stock": capacity * 2.0 },
        { "at": [500, 500], "material": WOOD, "stock": 1.0 },
    ]));
    let refusals = load(&text).map(|_| ()).expect_err("refused");
    assert_eq!(refusals.len(), 3, "{refusals:?}");
    assert_eq!(refusals[0].path, "deposits[0]");
    assert!(
        refusals[0].reason.contains("holds no wood"),
        "{}",
        refusals[0]
    );
    assert_eq!(refusals[1].path, "deposits[1].stock");
    assert!(
        refusals[1]
            .reason
            .contains(&format!("capacity of {capacity}")),
        "{}",
        refusals[1]
    );
    assert_eq!(refusals[2].path, "deposits[2].at");

    // A fit fault anywhere means nothing is applied: the one valid entry beside them is not opened.
    let mut app = build_test_app();
    let text = with_deposits(json!([
        { "at": [tile.x, tile.y], "material": WOOD, "stock": 1.0 },
        { "at": [bare.x, bare.y], "material": WOOD, "stock": 1.0 },
    ]));
    let plan = parse_scenario(&text, &ScenarioContext::from_world(&app.world)).expect("pass 1");
    assert!(build_scenario_world(&mut app, &plan).is_err());
    assert!(
        app.world.resource::<DepositRegistry>().is_empty(),
        "a refused load applies nothing"
    );
}

const DEPOSITS_PLAYTEST: &str = include_str!("../scenarios/deposits_playtest.json");

#[test]
fn the_deposits_playtest_loads_a_part_cut_wood_that_keeps_its_held_rung() {
    let file: Value = serde_json::from_str(DEPOSITS_PLAYTEST).expect("the playtest file is JSON");
    let entry = &file["deposits"][0];
    let tile = UVec2::new(
        entry["at"][0].as_u64().expect("x") as u32,
        entry["at"][1].as_u64().expect("y") as u32,
    );
    let material = entry["material"].as_str().expect("material");
    let stock = entry["stock"].as_f64().expect("stock") as f32;

    let (app, _) = load(DEPOSITS_PLAYTEST).expect("the deposits playtest loads");
    let working = app
        .world
        .resource::<DepositRegistry>()
        .source(tile, material)
        .expect("the working is opened")
        .clone();
    assert_eq!(working.stock, stock, "the stock is the file's");
    assert!(
        working.peak_rung.is_some(),
        "the held rung on the same working survives the stock setter"
    );
}

// ---- slice 2f: the bench queue ----

/// A recipe the shipped start already knows how to make, and the one that fills the one-item kit.
const PLAIN_RECIPE: &str = "spears";
const KIT_RECIPE: &str = "sled";
/// The one-item kit `KIT_RECIPE` fills.
const ONE_ITEM_KIT: &str = "sledding";
/// A recipe that needs a tool craft the opening people has not learned.
const UNLEARNED_RECIPE: &str = "tanning_frame";
/// Counts the file asks for.
const PLAIN_COUNT: u32 = 4;
const KIT_COUNT: u32 = 2;

fn with_bench(entries: Value) -> String {
    let mut file = shipped();
    file["factions"][0]["bench"] = entries;
    file.to_string()
}

fn bench_of(app: &mut App, band: BandId) -> BandBench {
    app.world
        .query::<(&BandId, &BandBench)>()
        .iter(&app.world)
        .find(|(id, _)| **id == band)
        .map(|(_, bench)| bench.clone())
        .expect("the band holds a bench")
}

#[test]
fn a_bench_section_builds_the_queue_in_file_order() {
    let text = with_bench(json!([
        { "bench_enqueue": { "band": "home", "recipe_id": PLAIN_RECIPE, "count": PLAIN_COUNT } },
        { "bench_enqueue_kit": {
            "band": "home", "kit_id": ONE_ITEM_KIT, "kits": KIT_COUNT, "recipes": [KIT_RECIPE]
        } },
    ]));
    let (mut app, applied) = load(&text).expect("the scenario loads");
    let bench = bench_of(&mut app, applied.bands["home"]);
    let queue: Vec<(&str, u32, u32)> = bench
        .orders
        .iter()
        .map(|order| (order.recipe_id.as_str(), order.count, order.kit_order))
        .collect();
    assert_eq!(
        queue,
        vec![(PLAIN_RECIPE, PLAIN_COUNT, 0), (KIT_RECIPE, KIT_COUNT, 1)],
        "file order, the kit part grouped under kit order 1"
    );
    assert_eq!(bench.kit_orders.len(), 1);
    assert_eq!(bench.kit_orders[0].kit_id, ONE_ITEM_KIT);
    assert_eq!(bench.kit_orders[0].kits, KIT_COUNT);
}

/// The checked-in bench playtest fixture (`run_stack.sh --scenario core_sim/scenarios/bench_queue.json`).
const SHIPPED_BENCH: &str = include_str!("../scenarios/bench_queue.json");

#[test]
fn the_shipped_bench_scenario_opens_with_its_queue() {
    let (mut app, applied) = load(SHIPPED_BENCH).expect("the shipped bench scenario loads");
    let bench = bench_of(&mut app, applied.bands["home"]);
    let queue: Vec<(&str, u32)> = bench
        .orders
        .iter()
        .map(|order| (order.recipe_id.as_str(), order.count))
        .collect();
    assert_eq!(queue, vec![("spears", 4), ("sled", 2)]);
    assert_eq!(bench.kit_orders.len(), 1);
    assert!(bench.auto, "auto-craft is on");
}

#[test]
fn a_faction_section_without_bench_is_a_parse_refusal() {
    let mut file = shipped();
    file["factions"][0].as_object_mut().unwrap().remove("bench");
    let refusals = pass_one(&file.to_string()).expect_err("a missing bench key is refused");
    assert_eq!(refusals.len(), 1);
    assert!(
        refusals[0].path.starts_with("line ") && refusals[0].path.contains("column"),
        "{}",
        refusals[0].path
    );
}

#[test]
fn pass_one_names_every_bad_bench_entry() {
    let text = with_bench(json!([
        { "bench_enqueue": { "band": "nobody", "recipe_id": PLAIN_RECIPE, "count": 1 } },
        { "bench_enqueue": { "band": "home", "recipe_id": "no_such_recipe", "count": 1 } },
        { "bench_enqueue_kit": {
            "band": "home", "kit_id": ONE_ITEM_KIT, "kits": 1,
            "recipes": [KIT_RECIPE, "no_such_recipe"]
        } },
        { "bench_enqueue_kit": { "band": "home", "kit_id": "none", "kits": 1, "recipes": [] } },
        { "bench_enqueue": { "band": "elsewhere", "recipe_id": PLAIN_RECIPE, "count": 1 } },
    ]));
    let refusals = pass_one(&text).expect_err("five bad entries");
    let paths: Vec<&str> = refusals.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "factions[0].bench[0].band",
            "factions[0].bench[1].recipe_id",
            "factions[0].bench[2].recipes[1]",
            "factions[0].bench[3].kit_id",
            "factions[0].bench[4].band",
        ],
        "{refusals:?}"
    );
    assert!(refusals[0]
        .reason
        .contains("no band 'nobody' in this faction"));
}

#[test]
fn a_replayed_refusal_stops_the_load_with_the_handlers_sentence() {
    const SECOND: usize = 1;
    let text = with_bench(json!([
        { "bench_enqueue": { "band": "home", "recipe_id": PLAIN_RECIPE, "count": PLAIN_COUNT } },
        { "bench_enqueue": { "band": "home", "recipe_id": PLAIN_RECIPE, "count": 0 } },
        { "bench_enqueue": { "band": "home", "recipe_id": UNLEARNED_RECIPE, "count": 1 } },
    ]));
    let refusals = load(&text)
        .map(|_| ())
        .expect_err("a zero count is refused");
    assert_eq!(refusals.len(), 1, "the first refusal stops the load");
    assert_eq!(refusals[0].path, format!("factions[0].bench[{SECOND}]"));
    assert!(
        refusals[0]
            .reason
            .starts_with("bench_enqueue: an order of ")
            && refusals[0].reason.ends_with("needs a count of at least 1."),
        "{}",
        refusals[0].reason
    );

    let text = with_bench(json!([
        { "bench_enqueue": { "band": "home", "recipe_id": UNLEARNED_RECIPE, "count": 1 } },
    ]));
    let refusals = load(&text)
        .map(|_| ())
        .expect_err("an unlearned craft is refused");
    assert_eq!(refusals[0].path, "factions[0].bench[0]");
    assert!(
        refusals[0]
            .reason
            .contains("which this people has not learned"),
        "{}",
        refusals[0].reason
    );
}

#[test]
fn an_unknown_field_or_variant_in_a_bench_entry_is_a_parse_refusal() {
    for entry in [
        json!({ "bench_enqueue": {
            "band": "home", "recipe_id": PLAIN_RECIPE, "count": 1, "priority": 2
        } }),
        json!({ "bench_remove": { "band": "home", "order": 0 } }),
        json!({ "bench_auto": { "band": "home", "enabled": true } }),
    ] {
        let refusals = pass_one(&with_bench(json!([entry]))).expect_err("refused");
        assert_eq!(refusals.len(), 1);
        assert!(
            refusals[0].path.starts_with("line ") && refusals[0].path.contains("column"),
            "{}",
            refusals[0].path
        );
    }
}
