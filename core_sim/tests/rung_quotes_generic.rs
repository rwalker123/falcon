//! **A FOURTH PLANT STEP IS A JSON EDIT, AND ITS QUOTE REACHES THE WIRE.** The patch and herd rows
//! carry one `RungQuoteState` per rung of their branch (`snapshot.fbs` -> `rungQuotes`) instead of
//! a field named after each step, so a `plant:farm` rung added to the ladder - reusing the `managed`
//! and `sown` traits, no Rust edit - is quoted, built to and published by the machinery that
//! already quotes the shipped three.
//!
//! The overlay below is a test-only copy of the shipped Field with the gathering-site rule dropped
//! (the Farm the field rung's own comment names as the next step) and a dearer price. Nothing in
//! `core_sim/src` names it.

use std::sync::Arc;

use bevy::app::App;
use bevy::math::UVec2;
use core_sim::shipped;
use core_sim::{
    build_test_app, BuildJob, BuildSource, DiscoveryProgressLedger, ForageRegistry,
    LaborAllocation, LaborTarget, LadderConfig, LadderConfigHandle, PopulationCohort, RungId,
    SnapshotHistory, TakeSelection, Tile, TileRegistry, ViewerFaction,
    BUILTIN_INTENSIFICATION_LADDER, CULTIVATION_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID,
};
use serde_json::Value;

/// The overlay's rung: branch `plant`, id `farm`, one above the shipped `field`.
const FARM: &str = "farm";

/// The Farm's declared price in work and its upkeep, distinct from the Field's (75 / 4) so a quote
/// wired to the wrong rung cannot pass.
const FARM_WORK_COST: f64 = 100.0;
const FARM_UPKEEP_PER_TURN: f64 = 5.0;

/// The fertility floor the Farm puts back on the ground it stands on (the field rung's own comment:
/// "Farm is this record with the gathering-site rule dropped and a fertility floor put back"). Low
/// enough that every food-bearing tile clears it - the test is about the quote, not the scarcity.
const FARM_FERTILITY_FLOOR: f64 = 1.0;

/// Hands on the builders pool and on the patch's gathering-and-keeping row. The Farm costs
/// `FARM_WORK_COST` and the keeping bill is a handful of work a turn, so these finish it well inside
/// [`TURN_BUDGET`].
const BUILDERS: u32 = 6;
const CREW: u32 = 4;
const ALL_HANDS: u32 = BUILDERS + CREW;

/// Turns any one phase may take - far above what the fixture needs, so a failure is a wrong answer
/// and not a short loop.
const TURN_BUDGET: usize = 120;

/// A pinned earthlike world, so the ground under the band is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// The escapement floor the fixture crew gathers at - the dial the shipped build tests use.
const FOOD_PEAK: f32 = 0.5;

fn farm() -> RungId {
    RungId::new(shipped::plant(), FARM)
}

/// **THE SHIPPED LADDER WITH A FOURTH PLANT RUNG** - a copy of the Field's record (so it inherits
/// `managed` + `sown`, the payoff block and the cost scale) with its own id, verb, order, price and
/// upkeep, and without the gathering-site requirement.
fn ladder_with_a_farm() -> LadderConfig {
    let mut ladder: Value = serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER)
        .expect("the builtin ladder parses as json");
    let rungs = ladder["rungs"].as_array_mut().expect("rungs is an array");
    let mut farm = rungs
        .iter()
        .find(|rung| rung["branch"] == "plant" && rung["id"] == "field")
        .expect("the shipped ladder has a field rung")
        .clone();
    farm["id"] = FARM.into();
    farm["order"] = 4.into();
    farm["verb"] = FARM.into();
    farm["label"] = "Farm".into();
    farm["label_fallback"] = "a farm".into();
    farm["completed_line"] = "Farm raised at ({x}, {y})".into();
    farm["requires_rung"] = "field".into();
    // The Farm UPGRADES a standing Field rather than placing a source, so it asks the patch for its
    // crop (committed when the Field was sown) instead of resolving one against the species'
    // `cultivation_ceiling`s, which stop at the Field on the shipped roster - a roster edit, and one
    // this test does not need to make.
    farm["places_source"] = false.into();
    farm["build"]["work_cost"] = FARM_WORK_COST.into();
    farm["upkeep"]["work_per_turn"] = FARM_UPKEEP_PER_TURN.into();
    farm["site_requirement"] = serde_json::json!({
        "requires_gathering_site": false,
        "min_forage_capacity": FARM_FERTILITY_FLOOR,
        "requires_fresh_water": false
    });
    rungs.push(farm);
    LadderConfig::from_json_str(&ladder.to_string())
        .expect("a fourth plant rung is a pure config edit and the overlay must load")
}

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<core_sim::SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.world
        .insert_resource(LadderConfigHandle::new(Arc::new(ladder_with_a_farm())));
    app.world
        .insert_resource(core_sim::FloraConfigHandle::new(Arc::new(
            roster_that_climbs_to_the_farm(),
        )));
    app.update();
    app
}

/// **THE SHIPPED ROSTER WITH EVERY FIELD-CEILING CROP LIFTED TO THE FARM** - `cultivation_ceiling` is
/// the species-side gate on a rung (an oak's mast is a wild harvest forever), and the shipped
/// roster's stop at the Field, so a fourth step is reachable by a crop only once the roster says so.
/// That is a JSON edit too, applied here as a text substitution on the builtin file.
fn roster_that_climbs_to_the_farm() -> core_sim::FloraConfig {
    let json = core_sim::BUILTIN_FLORA_CONFIG.replace(
        "\"cultivation_ceiling\": \"plant:field\"",
        "\"cultivation_ceiling\": \"plant:farm\"",
    );
    core_sim::FloraConfig::from_json_str(&json).expect("the lifted roster loads")
}

/// **A PATCH THE BAND CAN STAND ON AND CROP**: the first food site with a patch whose tile can name
/// a crop for the field rung, scanned in `(y, x)` order so the choice is deterministic.
fn cultivable_site(app: &mut App) -> UVec2 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let tiles: std::collections::HashMap<UVec2, Tile> = {
        let mut query = app.world.query::<&Tile>();
        query
            .iter(&app.world)
            .map(|tile| (tile.position, tile.clone()))
            .collect()
    };
    let registry = app.world.resource::<ForageRegistry>();
    let mut sites: Vec<UVec2> = app
        .world
        .resource::<core_sim::FoodSiteRegistry>()
        .sites()
        .iter()
        .map(|site| site.position)
        .filter(|position| registry.patch(*position).is_some())
        .filter(|position| {
            tiles.get(position).is_some_and(|tile| {
                let composition =
                    core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed);
                core_sim::default_species_for_rung(
                    &composition,
                    &flora,
                    &ladder,
                    shipped::plant_field(),
                )
                .is_some()
            })
        })
        .collect();
    sites.sort_by_key(|tile| (tile.y, tile.x));
    *sites
        .first()
        .expect("the fixture map carries a cultivable site")
}

/// **STAND A BAND ON A PATCH THAT HOLDS THE FIELD, WITH THE CLIMB TO FARM QUEUED**: the band's tile
/// is moved onto the site, the patch is raised to the top of the (shipped) field rung and owned, the
/// faction knows what gates both rungs, and a crew keeps the patch while the builders raise it.
fn stand_on_a_field_and_queue_the_farm(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let site = cultivable_site(app);
    let site_entity = app
        .world
        .resource::<TileRegistry>()
        .index(site.x, site.y)
        .expect("the site is on the map");
    let (band, faction) = {
        let mut query = app
            .world
            .query::<(bevy::prelude::Entity, &PopulationCohort)>();
        let (entity, cohort) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns a band");
        (entity, cohort.faction)
    };
    app.world.insert_resource(ViewerFaction(faction));
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .current_tile = site_entity;
    for knowledge in [CULTIVATION_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID] {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, knowledge, core_sim::scalar_one());
    }
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (field_base, field_width) = core_sim::plant_rung_span(shipped::plant_field(), &ladder);
    // The crop a standing Field was committed to - the build refuses a patch with none.
    let crop = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let ground = app
            .world
            .get::<Tile>(site_entity)
            .expect("the site tile")
            .clone();
        let composition =
            core_sim::tile_flora_composition(&flora, &labor.forage, &ground, map_seed);
        core_sim::default_species_for_rung(&composition, &flora, &ladder, shipped::plant_field())
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(site).expect("the site carries a patch");
        patch.species = crop;
        patch.owner = Some(faction);
        patch.set_ladder_position(field_base + field_width, &ladder);
    }
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(
        LaborTarget::Forage {
            tile: site,
            floor: FOOD_PEAK,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        CREW,
        ALL_HANDS,
        None,
    );
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, ALL_HANDS, None);
    allocation.enqueue_build(BuildSource::Patch(site), BuildJob::Climb(farm()));
    app.world.entity_mut(band).insert(allocation);
    (band, site)
}

fn held_rung(app: &App, site: UVec2) -> RungId {
    app.world
        .resource::<ForageRegistry>()
        .patch(site)
        .expect("the patch survives")
        .standing()
        .held
}

/// One published quote, read off the ENCODED envelope through the generated reader a client uses.
#[derive(Debug, Clone, PartialEq)]
struct PublishedQuote {
    rung_key: String,
    reached: bool,
    progress: f32,
    work_cost: f32,
    upkeep_demand: f32,
    food_yield: f32,
    site_refusal: String,
}

/// Every quote the patch at `site` publishes, in wire order.
fn published_quotes(app: &App, site: UVec2) -> Vec<PublishedQuote> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let patch = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("a subsistence section")
        .foragePatches()
        .expect("the forage patches")
        .iter()
        .find(|patch| patch.x() == site.x && patch.y() == site.y)
        .expect("the patch is on the wire");
    patch
        .rungQuotes()
        .expect("the patch publishes its rung quotes")
        .iter()
        .map(|quote| PublishedQuote {
            rung_key: quote.rungKey().unwrap_or_default().to_string(),
            reached: quote.reached(),
            progress: quote.progress(),
            work_cost: quote.workCost(),
            upkeep_demand: quote.upkeepDemand(),
            food_yield: quote.foodYield(),
            site_refusal: quote.siteRefusal().unwrap_or_default().to_string(),
        })
        .collect()
}

fn quote_of(quotes: &[PublishedQuote], rung: RungId) -> &PublishedQuote {
    quotes
        .iter()
        .find(|quote| quote.rung_key == rung.wire_key())
        .unwrap_or_else(|| panic!("the patch publishes a quote for {rung}: {quotes:?}"))
}

/// **THE FOURTH RUNG IS QUOTED BEFORE ANYONE BUILDS IT**: one quote per rung above the root, in
/// climb order, the Farm's carrying its own price and upkeep and *not* the Field's, and the rungs
/// the patch holds reading `reached`.
#[test]
fn a_fourth_plant_rung_is_quoted_on_the_wire_with_its_own_price() {
    let mut app = spawn_world();
    let (_, site) = stand_on_a_field_and_queue_the_farm(&mut app);
    app.update();

    let quotes = published_quotes(&app, site);
    let keys: Vec<&str> = quotes.iter().map(|quote| quote.rung_key.as_str()).collect();
    assert_eq!(
        keys,
        ["plant:tended", "plant:field", "plant:farm"],
        "one quote per rung above the root, in climb order"
    );

    let (tended, field, farm_quote) = (
        quote_of(&quotes, shipped::plant_tended()),
        quote_of(&quotes, shipped::plant_field()),
        quote_of(&quotes, farm()),
    );
    assert!(tended.reached && field.reached, "the patch holds the Field");
    assert!(!farm_quote.reached, "…and has not built the Farm");
    assert!(
        (f64::from(farm_quote.work_cost) - FARM_WORK_COST).abs() < 1.0e-3,
        "the Farm quotes its own declared price: {}",
        farm_quote.work_cost
    );
    assert!(
        farm_quote.work_cost > field.work_cost,
        "…which is dearer than the Field's"
    );
    assert!(
        farm_quote.upkeep_demand > field.upkeep_demand,
        "…and so is holding it: {} vs {}",
        farm_quote.upkeep_demand,
        field.upkeep_demand
    );
    assert!(
        farm_quote.site_refusal.is_empty(),
        "the Farm drops the gathering-site rule, so no ground refuses it"
    );
    assert!(
        farm_quote.food_yield > 0.0,
        "a sown rung pays through the Field's payoff block, resolved off its traits"
    );
}

/// **THE CROP PICKER QUOTES THE FOURTH RUNG TOO**: every plant in the patch's composition carries one
/// `RungShareQuoteState` per rung above the root, the Farm's among them, with the Farm's own price
/// where the plant can climb to it and the Field's payoff block (resolved off the shared traits).
#[test]
fn a_fourth_plant_rung_is_quoted_per_species_in_the_crop_picker() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let (_, site) = stand_on_a_field_and_queue_the_farm(&mut app);
    app.update();

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let patch = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("a subsistence section")
        .foragePatches()
        .expect("the forage patches")
        .iter()
        .find(|patch| patch.x() == site.x && patch.y() == site.y)
        .expect("the patch is on the wire");
    let composition = patch.composition().expect("the patch names its plants");
    assert!(!composition.is_empty(), "the tile grows something");
    for share in composition.iter() {
        let keys: Vec<&str> = share
            .rungQuotes()
            .expect("every plant publishes its rung quotes")
            .iter()
            .map(|quote| quote.rungKey().unwrap_or_default())
            .collect();
        assert_eq!(
            keys,
            ["plant:tended", "plant:field", "plant:farm"],
            "one quote per rung above the root, for every plant"
        );
        let field = sim_schema::wire_share_rung_quote(share.rungQuotes(), "plant:field")
            .expect("the field is quoted");
        let farm_quote = sim_schema::wire_share_rung_quote(share.rungQuotes(), "plant:farm")
            .expect("the farm is quoted");
        assert!(
            farm_quote.canClimb(),
            "a crop whose ceiling is the Farm can climb to it"
        );
        assert!(
            (f64::from(farm_quote.workCost()) - FARM_WORK_COST).abs() < 1.0e-3
                || farm_quote.workCost() > field.workCost(),
            "the Farm quotes its own price: {}",
            farm_quote.workCost()
        );
        assert_eq!(
            farm_quote.foodPayoff(),
            field.foodPayoff(),
            "…and pays through the Field's block, resolved off the shared traits"
        );
    }
}

/// **AND IT IS BUILT TO**: the climb from the Field completes through the same machinery the shipped
/// rungs use, the patch ends holding the Farm, and the wire's Farm quote flips to `reached` with a
/// full meter.
#[test]
fn a_patch_at_field_climbs_to_the_fourth_rung_and_the_wire_says_so() {
    let mut app = spawn_world();
    let (_, site) = stand_on_a_field_and_queue_the_farm(&mut app);

    let mut turns = 0;
    while held_rung(&app, site) != farm() {
        assert!(
            turns < TURN_BUDGET,
            "the climb to the farm did not complete within {TURN_BUDGET} turns"
        );
        app.update();
        turns += 1;
    }
    app.update();

    let quotes = published_quotes(&app, site);
    let farm_quote = quote_of(&quotes, farm());
    assert!(farm_quote.reached, "the wire says the Farm is built");
    assert!(
        (farm_quote.progress - 1.0).abs() < 1.0e-4,
        "a built rung reads a full meter: {}",
        farm_quote.progress
    );
    assert!(quotes.iter().all(|quote| quote.reached));
}

/// The Ranch's declared price in work, distinct from the shipped pen's 75.
const RANCH_WORK_COST: f64 = 150.0;

/// **THE ANIMAL BRANCH IS THE SAME STORY**: a fourth animal rung that reuses the `domesticated` and
/// `enclosed` traits (a `ranch` above the pen) is quoted on every herd row under its own key, with
/// its own price and the pen's payoff block, and no Rust edit.
#[test]
fn a_fourth_animal_rung_is_quoted_on_every_herd_row() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut ladder: Value = serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER)
        .expect("the builtin ladder parses as json");
    let rungs = ladder["rungs"].as_array_mut().expect("rungs is an array");
    let mut ranch = rungs
        .iter()
        .find(|rung| rung["branch"] == "animal" && rung["id"] == "pen")
        .expect("the shipped ladder has a pen rung")
        .clone();
    ranch["id"] = "ranch".into();
    ranch["order"] = 4.into();
    ranch["verb"] = "ranch".into();
    ranch["label"] = "Ranch".into();
    ranch["label_fallback"] = "a ranch".into();
    ranch["completed_line"] = "Ranched {id} at ({x}, {y})".into();
    ranch["requires_rung"] = "pen".into();
    ranch["build"]["work_cost"] = RANCH_WORK_COST.into();
    ranch["ceiling_required"] = "animal:ranch".into();
    rungs.push(ranch);

    let mut app = build_test_app();
    let mut config = app.world.resource::<core_sim::SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    // The quote is a fact about the ladder and the herd, not about what this viewer has explored, so
    // the fog is off and every herd is on the list.
    config.fog_enabled = false;
    app.world.insert_resource(config);
    app.world.insert_resource(LadderConfigHandle::new(Arc::new(
        LadderConfig::from_json_str(&ladder.to_string())
            .expect("a fourth animal rung is a pure config edit and the overlay must load"),
    )));
    app.update();
    app.update();

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let herds = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("a subsistence section")
        .herds()
        .expect("the herds");
    assert!(!herds.is_empty(), "the world publishes herds");
    for herd in herds.iter() {
        let quotes = herd.rungQuotes().expect("every herd publishes its quotes");
        let keys: Vec<&str> = quotes
            .iter()
            .map(|quote| quote.rungKey().unwrap_or_default())
            .collect();
        assert_eq!(
            keys,
            ["animal:pastoral", "animal:pen", "animal:ranch"],
            "one quote per rung above the root"
        );
        let pen = sim_schema::wire_rung_quote(herd.rungQuotes(), "animal:pen")
            .expect("the pen is quoted");
        let ranch = sim_schema::wire_rung_quote(herd.rungQuotes(), "animal:ranch")
            .expect("the ranch is quoted");
        assert!(
            (f64::from(ranch.workCost()) - RANCH_WORK_COST).abs() < 1.0e-3,
            "the Ranch quotes its own declared price: {}",
            ranch.workCost()
        );
        assert_eq!(
            ranch.foodYield(),
            pen.foodYield(),
            "…and pays through the pen's payoff block, resolved off its traits"
        );
        assert!(!ranch.reached(), "no herd stands at the Ranch");
    }
}
