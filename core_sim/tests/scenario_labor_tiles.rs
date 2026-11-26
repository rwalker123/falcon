//! **The scenario loader, slice 2a: the `labor` and `tiles` sections** (`docs/plan_scenarios.md`,
//! issue #922).
//!
//! Driven through the library path the server's `load_scenario` handler runs: pass 1
//! ([`parse_scenario`]), then [`build_scenario_world`]. Every claim sits beside a control, and every
//! tile is found by scanning the generated world rather than named, so a worldgen retune moves the
//! tiles and not the tests.

use bevy::prelude::*;
use serde_json::{json, Value};
use sim_runtime::{TerrainTags, TerrainType};

use core_sim::command_seam::{apply_assign_labor, AssignLaborOrder, CommandRefusal};
use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::save::{encode_save, load_save};
use core_sim::scenario::{
    build_scenario_world, parse_scenario, ScenarioApplied, ScenarioContext, ScenarioRefusal,
};
use core_sim::{
    build_test_app, classify_food_module, terrain_definition, tile_flora_composition,
    tile_forage_capacity, BandId, FactionId, FloraConfigHandle, FoodModuleTag, FoodSiteRegistry,
    ForageRegistry, LaborAllocation, LaborConfigHandle, LaborTarget, SimulationConfig,
    SourcePriority, Tile, TileRegistry,
};

const SHIPPED: &str = include_str!("../scenarios/breeding_ceiling_150.json");
const HOME: FactionId = FactionId(0);
/// The id the shipped file gives its band.
const HOME_BAND: &str = "home";
/// Hands the forage entry asks for: well under the 90 working the shipped band has.
const CREW: u32 = 2;
/// A species key no roster carries.
const NO_SUCH_PLANT: &str = "no_such_plant";

fn shipped() -> Value {
    serde_json::from_str(SHIPPED).expect("the shipped scenario is JSON")
}

fn pass_one(text: &str) -> Result<(), Vec<ScenarioRefusal>> {
    let app = build_test_app();
    parse_scenario(text, &ScenarioContext::from_world(&app.world)).map(|_| ())
}

fn load(text: &str) -> Result<(App, ScenarioApplied), Vec<ScenarioRefusal>> {
    let mut app = build_test_app();
    let plan = parse_scenario(text, &ScenarioContext::from_world(&app.world))?;
    let applied = build_scenario_world(&mut app, &plan)?;
    Ok((app, applied))
}

fn shipped_world() -> (App, ScenarioApplied) {
    load(SHIPPED).expect("the shipped scenario loads")
}

fn band_tile(app: &mut App, id: BandId) -> UVec2 {
    let tile = app
        .world
        .query::<(&BandId, &core_sim::PopulationCohort)>()
        .iter(&app.world)
        .find(|(band, _)| **band == id)
        .map(|(_, cohort)| cohort.current_tile)
        .expect("the band exists");
    app.world.get::<Tile>(tile).expect("the tile").position
}

fn forage_entry(x: u32, y: u32) -> Value {
    json!({ "assign_labor": {
        "band": HOME_BAND, "role": "forage", "workers": CREW,
        "target_x": x, "target_y": y, "take_species": [], "move_with_herd": false
    } })
}

fn forage_order(x: u32, y: u32) -> AssignLaborOrder {
    AssignLaborOrder {
        role: "forage".to_string(),
        workers: CREW,
        target_x: Some(x),
        target_y: Some(y),
        ..AssignLaborOrder::default()
    }
}

/// The gathering sites near the band, nearest first, that the handler itself accepts a forage crew on.
/// Found by asking the handler on a throwaway world, so the scan is the rule and not a copy of it.
fn forage_tile_near_band() -> [u32; 2] {
    let (mut app, applied) = shipped_world();
    let band = *applied.bands.get(HOME_BAND).unwrap();
    let from = band_tile(&mut app, band);
    let (width, wrap) = {
        let registry = app.world.resource::<TileRegistry>();
        let config = app.world.resource::<SimulationConfig>();
        (registry.width, config.map_topology.wrap_horizontal)
    };
    let mut sites: Vec<UVec2> = app
        .world
        .resource::<FoodSiteRegistry>()
        .iter()
        .map(|site| site.position)
        .filter(|position| {
            app.world
                .resource::<ForageRegistry>()
                .patch(*position)
                .is_some()
        })
        .collect();
    sites.sort_by_key(|site| {
        (
            hex_distance_wrapped(from, *site, width, wrap),
            site.y,
            site.x,
        )
    });
    for site in sites {
        if apply_assign_labor(
            &mut app.world,
            HOME,
            Some(band.0),
            forage_order(site.x, site.y),
        )
        .is_ok()
        {
            return [site.x, site.y];
        }
    }
    panic!("the map has a gathering site the shipped band may forage");
}

fn forage_row(app: &mut App, id: BandId, tile: [u32; 2]) -> Option<(u32, SourcePriority)> {
    let at = UVec2::new(tile[0], tile[1]);
    let mut query = app.world.query::<(&BandId, &LaborAllocation)>();
    let (_, allocation) = query.iter(&app.world).find(|(band, _)| **band == id)?;
    allocation
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Forage { tile, .. } if tile == at))
        .map(|row| (row.workers, row.priority))
}

fn with_labor(entries: Vec<Value>) -> Value {
    let mut file = shipped();
    file["factions"][0]["labor"] = Value::Array(entries);
    file
}

fn with_tiles(entries: Vec<Value>) -> Value {
    let mut file = shipped();
    file["tiles"] = Value::Array(entries);
    file
}

fn first_reason(refusals: &[ScenarioRefusal], path: &str) -> String {
    refusals
        .iter()
        .find(|refusal| refusal.path == path)
        .unwrap_or_else(|| panic!("no refusal at {path}: {refusals:?}"))
        .reason
        .clone()
}

// --- keys ------------------------------------------------------------------------------------

#[test]
fn labor_and_tiles_are_required_keys_and_an_unknown_one_is_refused() {
    let mut file = shipped();
    file["factions"][0].as_object_mut().unwrap().remove("labor");
    assert!(pass_one(&file.to_string()).is_err(), "labor is required");

    let mut file = shipped();
    file.as_object_mut().unwrap().remove("tiles");
    assert!(pass_one(&file.to_string()).is_err(), "tiles is required");

    let mut file = with_labor(vec![json!({ "forage_tile": { "band": HOME_BAND } })]);
    assert!(
        pass_one(&file.to_string()).is_err(),
        "ForageTile is a retired verb, not a variant"
    );
    file = with_labor(vec![json!({ "assign_labor": {
        "band": HOME_BAND, "role": "forage", "workers": 1, "take_species": [],
        "move_with_herd": false, "policy": "sustain"
    } })]);
    assert!(pass_one(&file.to_string()).is_err(), "policy is retired");
    file = with_labor(vec![json!({ "assign_labor": {
        "band": HOME_BAND, "role": "forage", "workers": 1
    } })]);
    assert!(
        pass_one(&file.to_string()).is_err(),
        "take_species and move_with_herd are required"
    );
    assert!(pass_one(&with_labor(vec![]).to_string()).is_ok());
}

// --- labor: apply ----------------------------------------------------------------------------

#[test]
fn an_assign_labor_entry_puts_the_named_crew_on_the_row_and_the_control_has_none() {
    let [x, y] = forage_tile_near_band();
    let (mut control, applied) = shipped_world();
    let id = *applied.bands.get(HOME_BAND).unwrap();
    assert_eq!(
        forage_row(&mut control, id, [x, y]),
        None,
        "no entry, no row"
    );

    let file = with_labor(vec![forage_entry(x, y)]);
    let (mut app, applied) = load(&file.to_string()).expect("the entry lands");
    let id = *applied.bands.get(HOME_BAND).unwrap();
    let (workers, priority) = forage_row(&mut app, id, [x, y]).expect("the row exists");
    assert_eq!(workers, CREW);
    assert_eq!(priority, SourcePriority::Normal);
}

#[test]
fn a_work_priority_entry_after_it_sets_the_rows_level() {
    let [x, y] = forage_tile_near_band();
    let ranked = json!({ "work_priority": {
        "band": HOME_BAND, "target_x": x, "target_y": y, "level": "high"
    } });
    let (mut app, applied) =
        load(&with_labor(vec![forage_entry(x, y), ranked]).to_string()).expect("both land");
    let id = *applied.bands.get(HOME_BAND).unwrap();
    assert_eq!(
        forage_row(&mut app, id, [x, y]).map(|row| row.1),
        Some(SourcePriority::High)
    );

    let (mut control, applied) = load(&with_labor(vec![forage_entry(x, y)]).to_string()).unwrap();
    let id = *applied.bands.get(HOME_BAND).unwrap();
    assert_eq!(
        forage_row(&mut control, id, [x, y]).map(|row| row.1),
        Some(SourcePriority::Normal),
        "the control is normal"
    );
}

/// A tile the generated world holds as water.
fn sea_tile(app: &mut App) -> UVec2 {
    app.world
        .query::<&Tile>()
        .iter(&app.world)
        .find(|tile| tile.terrain_tags.contains(TerrainTags::WATER))
        .map(|tile| tile.position)
        .expect("the map has water")
}

#[test]
fn a_labor_entry_the_handler_refuses_fails_the_load_with_the_handlers_own_reason() {
    let (mut control, applied) = shipped_world();
    let id = *applied.bands.get(HOME_BAND).unwrap();
    let sea = sea_tile(&mut control);

    // The handler's refusal, asked directly on the control world.
    let refusal = apply_assign_labor(
        &mut control.world,
        HOME,
        Some(id.0),
        forage_order(sea.x, sea.y),
    )
    .expect_err("forage on the sea is refused");
    assert!(matches!(refusal, CommandRefusal::Refused { .. }));

    // The same entry, in a file: the same words, at the entry's own path. A first entry that lands
    // proves the path counts entries rather than naming the section.
    let [x, y] = forage_tile_near_band();
    let file = with_labor(vec![forage_entry(x, y), forage_entry(sea.x, sea.y)]);
    let refusals = load(&file.to_string()).map(|_| ()).expect_err("refused");
    assert_eq!(
        refusals.len(),
        1,
        "the load stops at the entry: {refusals:?}"
    );
    assert_eq!(refusals[0].path, "factions[0].labor[1]");
    assert_eq!(refusals[0].reason, refusal.to_string());
}

#[test]
fn a_refused_entry_stops_the_load_before_the_entries_after_it() {
    let (mut probe, _) = shipped_world();
    let sea = sea_tile(&mut probe);
    let [x, y] = forage_tile_near_band();
    let bad_after_good = with_labor(vec![forage_entry(sea.x, sea.y), forage_entry(x, y)]);
    let refusals = load(&bad_after_good.to_string())
        .map(|_| ())
        .expect_err("refused");
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].path, "factions[0].labor[0]");
}

// --- labor: pass 1 ---------------------------------------------------------------------------

#[test]
fn pass_one_collects_every_labor_fault_in_one_load() {
    let file = with_labor(vec![
        // an unknown band
        json!({ "assign_labor": {
            "band": "nobody", "role": "forage", "workers": 1, "take_species": [],
            "move_with_herd": false } }),
        // a target off the map, and one coordinate without the other
        json!({ "assign_labor": {
            "band": HOME_BAND, "role": "forage", "workers": 1, "target_x": 5000,
            "target_y": 5000, "take_species": [], "move_with_herd": false } }),
        json!({ "assign_labor": {
            "band": HOME_BAND, "role": "forage", "workers": 1, "target_x": 3,
            "take_species": [], "move_with_herd": false } }),
        // an unknown kit, species and take species
        json!({ "assign_labor": {
            "band": HOME_BAND, "role": "forage", "workers": 1, "kit_id": "no_such_kit",
            "species": NO_SUCH_PLANT, "take_species": [NO_SUCH_PLANT],
            "move_with_herd": false } }),
        // an unknown role
        json!({ "assign_labor": {
            "band": HOME_BAND, "role": "sing", "workers": 1, "take_species": [],
            "move_with_herd": false } }),
        // a work_priority with a bad level, band and target
        json!({ "work_priority": {
            "band": "nobody", "target_x": 5000, "target_y": 5000, "level": "urgent" } }),
    ]);
    let refusals = pass_one(&file.to_string()).expect_err("refused");
    let a = |k: usize| format!("factions[0].labor[{k}].assign_labor");
    first_reason(&refusals, &format!("{}.band", a(0)));
    first_reason(&refusals, &format!("{}.target_x", a(1)));
    first_reason(&refusals, &format!("{}.target_y", a(2)));
    first_reason(&refusals, &format!("{}.kit_id", a(3)));
    first_reason(&refusals, &format!("{}.species", a(3)));
    first_reason(&refusals, &format!("{}.take_species[0]", a(3)));
    first_reason(&refusals, &format!("{}.role", a(4)));
    let w = "factions[0].labor[5].work_priority";
    first_reason(&refusals, &format!("{w}.band"));
    first_reason(&refusals, &format!("{w}.target_x"));
    first_reason(&refusals, &format!("{w}.level"));
    assert_eq!(refusals.len(), 10, "{refusals:?}");
}

#[test]
fn a_known_role_and_level_pass_pass_one_whatever_the_case() {
    let file = with_labor(vec![
        json!({ "assign_labor": {
            "band": HOME_BAND, "role": "Warrior", "workers": 1, "take_species": [],
            "move_with_herd": false } }),
        json!({ "work_priority": { "band": HOME_BAND, "target_x": 1, "target_y": 1,
            "level": "LOW" } }),
    ]);
    pass_one(&file.to_string()).expect("the handler accepts both");
}

// --- tiles -----------------------------------------------------------------------------------

/// Land biomes a tile can be swapped to, tried in turn until one changes the tile's food module.
const SWAP_TARGETS: [TerrainType; 5] = [
    TerrainType::HotDesertErg,
    TerrainType::BorealTaiga,
    TerrainType::PrairieSteppe,
    TerrainType::MixedWoodland,
    TerrainType::Tundra,
];

/// A lowland tile bearing a forage patch, and a swap target that changes its food module and its
/// forage capacity.
fn swappable_tile() -> (UVec2, TerrainType) {
    let (mut app, _) = shipped_world();
    let forage = app.world.resource::<LaborConfigHandle>().get();
    let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
    for (tile, tag) in query.iter(&app.world) {
        let lowland = tile.mountain.is_none()
            && tile.terrain != TerrainType::NavigableRiver
            && !tile.terrain_tags.contains(TerrainTags::WATER);
        if !lowland
            || app
                .world
                .resource::<ForageRegistry>()
                .patch(tile.position)
                .is_none()
        {
            continue;
        }
        for target in SWAP_TARGETS {
            let mut swapped = tile.clone();
            swapped.terrain = target;
            swapped.terrain_tags = terrain_definition(target).tags;
            let capacity = tile_forage_capacity(&forage.forage, &swapped);
            if target != tile.terrain
                && classify_food_module(&swapped).is_some_and(|module| module != tag.module)
                && capacity > 0.0
                && capacity != tile_forage_capacity(&forage.forage, tile)
            {
                return (tile.position, target);
            }
        }
    }
    panic!("the map has a tile whose swap changes its module and capacity");
}

#[test]
fn a_terrain_override_is_a_worldgen_input_everything_derived_reads() {
    let (at, target) = swappable_tile();
    let file = with_tiles(vec![
        json!({ "at": [at.x, at.y], "terrain": format!("{target:?}") }),
    ]);
    let (app, _) = load(&file.to_string()).expect("the override fits");
    let forage = app.world.resource::<LaborConfigHandle>().get();
    let registry = app.world.resource::<TileRegistry>();
    let entity = registry.index(at.x, at.y).unwrap();
    let tile = app.world.get::<Tile>(entity).unwrap().clone();
    assert_eq!(tile.terrain, target);
    assert_eq!(tile.terrain_tags, terrain_definition(target).tags);
    // The food module is the one the new terrain classifies to: the override ran before the
    // reconcile, so nothing stale survived.
    let tag = app.world.get::<FoodModuleTag>(entity).expect("a module");
    assert_eq!(Some(tag.module), classify_food_module(&tile));
    // And the forage patch was seeded at the new biome's capacity.
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(at)
        .expect("a patch");
    assert_eq!(
        patch.carrying_capacity,
        tile_forage_capacity(&forage.forage, &tile)
    );

    // The control: the same tile, no override, is the generated biome with its own module.
    let (control, _) = shipped_world();
    let entity = control
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .unwrap();
    let generated = control.world.get::<Tile>(entity).unwrap().clone();
    assert_ne!(generated.terrain, target);
    assert_ne!(
        control
            .world
            .get::<FoodModuleTag>(entity)
            .map(|tag| tag.module),
        Some(tag.module)
    );
}

/// A lowland tile with a basket of at least two plants, and one plant of it.
fn flora_tile() -> (UVec2, Vec<String>) {
    let (mut app, _) = shipped_world();
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let mut query = app.world.query::<(&Tile, &FoodModuleTag)>();
    for (tile, _) in query.iter(&app.world) {
        if tile.terrain == TerrainType::NavigableRiver {
            continue;
        }
        let hosted = flora.composition(tile.resource_terrain());
        if hosted.len() >= 3 {
            let first_two = hosted
                .iter()
                .take(2)
                .map(|share| share.species.clone())
                .collect();
            return (tile.position, first_two);
        }
    }
    panic!("the map has a tile hosting three plants");
}

#[test]
fn a_flora_override_names_exactly_the_plants_it_lists_and_survives_a_save() {
    let (at, keys) = flora_tile();
    let file = with_tiles(vec![json!({ "at": [at.x, at.y], "flora": keys })]);
    let (app, _) = load(&file.to_string()).expect("the override fits");
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let forage = app.world.resource::<LaborConfigHandle>().get();
    let seed = app.world.resource::<SimulationConfig>().map_seed;
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .unwrap();
    let tile = app.world.get::<Tile>(entity).unwrap().clone();

    let basket = tile_flora_composition(&flora, &forage.forage, &tile, seed);
    let mut named: Vec<&str> = basket.iter().map(|share| share.species.as_str()).collect();
    named.sort_unstable();
    let mut expected: Vec<&str> = keys.iter().map(String::as_str).collect();
    expected.sort_unstable();
    assert_eq!(named, expected, "exactly the named plants");
    let total: f32 = basket.iter().map(|share| share.share).sum();
    assert!(
        (total - 1.0).abs() < 1e-5,
        "the shares sum to one, got {total}"
    );

    // The control tile, without the override, is the seeded realization.
    let (control, _) = shipped_world();
    let entity = control
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .unwrap();
    let generated = control.world.get::<Tile>(entity).unwrap().clone();
    assert_eq!(generated.flora_override, None);
    let seeded = tile_flora_composition(&flora, &forage.forage, &generated, seed);
    assert_eq!(
        seeded.as_ref(),
        flora
            .realized_composition(generated.resource_terrain(), generated.position, seed)
            .as_slice()
    );

    // The override rides the save: it is on the tile a loaded world holds.
    let blob = encode_save(&app.world).expect("the world encodes");
    let (loaded, _) = load_save(&blob).expect("the save loads");
    let entity = loaded
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .unwrap();
    assert_eq!(
        loaded.world.get::<Tile>(entity).unwrap().flora_override,
        tile.flora_override
    );
    assert!(tile.flora_override.is_some());
}

/// A tile satisfying `pick`, read off the shipped world.
fn find_tile(pick: impl Fn(&Tile) -> bool) -> Option<UVec2> {
    let (mut app, _) = shipped_world();
    let found = app
        .world
        .query::<&Tile>()
        .iter(&app.world)
        .find(|tile| pick(tile))
        .map(|tile| tile.position);
    found
}

#[test]
fn pass_one_refuses_every_tile_override_fault() {
    let water = TerrainType::DeepOcean;
    let mountain = TerrainType::AlpineMountain;
    let file = with_tiles(vec![
        json!({ "at": [5000, 5000], "terrain": "AlluvialPlain" }),
        json!({ "at": [3, 3], "terrain": "AlluvialPlain" }),
        json!({ "at": [3, 3], "terrain": "AlluvialPlain" }),
        json!({ "at": [4, 4] }),
        json!({ "at": [5, 5], "terrain": "NotATerrain" }),
        json!({ "at": [6, 6], "terrain": format!("{water:?}") }),
        json!({ "at": [7, 7], "terrain": "NavigableRiver" }),
        json!({ "at": [8, 8], "terrain": format!("{mountain:?}") }),
        json!({ "at": [9, 9], "flora": [] }),
        json!({ "at": [10, 10], "flora": [NO_SUCH_PLANT] }),
        json!({ "at": [11, 11], "flora": ["wild_emmer", "wild_emmer"] }),
    ]);
    let refusals = pass_one(&file.to_string()).expect_err("refused");
    first_reason(&refusals, "tiles[0].at");
    assert!(first_reason(&refusals, "tiles[2].at").contains("twice"));
    first_reason(&refusals, "tiles[3]");
    first_reason(&refusals, "tiles[4].terrain");
    assert!(first_reason(&refusals, "tiles[5].terrain").contains("water"));
    assert!(first_reason(&refusals, "tiles[6].terrain").contains("water"));
    assert!(first_reason(&refusals, "tiles[7].terrain").contains("mountain"));
    assert!(first_reason(&refusals, "tiles[8].flora").contains("at least one plant"));
    first_reason(&refusals, "tiles[9].flora[0]");
    assert!(first_reason(&refusals, "tiles[10].flora[1]").contains("twice"));
}

#[test]
fn a_swap_to_a_lowland_biome_and_a_listed_plant_pass_pass_one() {
    let file = with_tiles(vec![
        json!({ "at": [3, 3], "terrain": "AlluvialPlain" }),
        json!({ "at": [4, 4], "flora": ["wild_emmer"] }),
    ]);
    pass_one(&file.to_string()).expect("both are valid names");
}

#[test]
fn a_terrain_override_on_a_water_or_mountain_tile_is_refused_at_fit() {
    let sea = find_tile(|tile| tile.terrain_tags.contains(TerrainTags::WATER))
        .expect("the map has water");
    let refusals = load(
        &with_tiles(vec![
            json!({ "at": [sea.x, sea.y], "terrain": "AlluvialPlain" }),
        ])
        .to_string(),
    )
    .map(|_| ())
    .expect_err("refused");
    assert_eq!(refusals[0].path, "tiles[0]");
    assert!(refusals[0].reason.contains("elevation"), "{}", refusals[0]);

    let peak = find_tile(|tile| {
        tile.mountain.is_some() && !tile.terrain_tags.contains(TerrainTags::WATER)
    })
    .expect("the map has a mountain");
    let refusals = load(
        &with_tiles(vec![
            json!({ "at": [peak.x, peak.y], "terrain": "AlluvialPlain" }),
        ])
        .to_string(),
    )
    .map(|_| ())
    .expect_err("refused");
    assert_eq!(refusals[0].path, "tiles[0]");
    assert!(refusals[0].reason.contains("mountain"), "{}", refusals[0]);

    // The control: a flora override on the same sea tile is not refused for its terrain (it fails
    // only for lacking a basket), and a lowland swap fits.
    let (at, target) = swappable_tile();
    load(
        &with_tiles(vec![
            json!({ "at": [at.x, at.y], "terrain": format!("{target:?}") }),
        ])
        .to_string(),
    )
    .expect("a lowland swap fits");
}

#[test]
fn a_plant_the_biome_does_not_host_is_refused_naming_the_plant_and_the_biome() {
    let (mut app, _) = shipped_world();
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let (at, terrain, stranger) = {
        let mut found = None;
        for tile in app.world.query::<&Tile>().iter(&app.world) {
            let hosted = flora.composition(tile.resource_terrain());
            if hosted.is_empty() || tile.terrain == TerrainType::NavigableRiver {
                continue;
            }
            if let Some(key) = flora
                .species
                .keys()
                .filter(|key| !hosted.iter().any(|share| &share.species == *key))
                .min()
            {
                found = Some((tile.position, tile.terrain, key.clone()));
                break;
            }
        }
        found.expect("some plant does not grow somewhere")
    };
    let refusals =
        load(&with_tiles(vec![json!({ "at": [at.x, at.y], "flora": [stranger] })]).to_string())
            .map(|_| ())
            .expect_err("refused");
    assert_eq!(refusals[0].path, "tiles[0]");
    assert!(
        refusals[0].reason.contains(&stranger)
            && refusals[0].reason.contains(&format!("{terrain:?}")),
        "{}",
        refusals[0]
    );
}

// --- labor: a pool role -----------------------------------------------------------------------

/// The hands the roadwork entry sets as the row's Max.
const ROADWORK_MAX: u32 = 3;

fn roadwork_entry(kit: Option<&str>) -> Value {
    let mut order = json!({
        "band": HOME_BAND, "role": "roadwork", "workers": ROADWORK_MAX,
        "take_species": [], "move_with_herd": false
    });
    if let Some(kit) = kit {
        order["kit_id"] = json!(kit);
    }
    json!({ "assign_labor": order })
}

/// The band's roadwork row's Max, if it has the row.
fn roadwork_max(app: &mut App, id: BandId) -> Option<Option<u32>> {
    let mut query = app.world.query::<(&BandId, &LaborAllocation)>();
    let (_, allocation) = query.iter(&app.world).find(|(band, _)| **band == id)?;
    allocation
        .assignments
        .iter()
        .find(|row| matches!(row.target, LaborTarget::Roadwork))
        .map(|row| row.max_workers)
}

#[test]
fn a_pool_role_needs_no_target_and_no_kit_and_lands_on_the_band() {
    let (mut control, applied) = shipped_world();
    let id = *applied.bands.get(HOME_BAND).unwrap();
    assert_eq!(roadwork_max(&mut control, id), None, "no entry, no row");

    let file = with_labor(vec![roadwork_entry(None)]);
    pass_one(&file.to_string()).expect("a pool role names no tile and no kit");
    let (mut app, applied) = load(&file.to_string()).expect("the roadwork row lands");
    let id = *applied.bands.get(HOME_BAND).unwrap();
    assert_eq!(roadwork_max(&mut app, id), Some(Some(ROADWORK_MAX)));
}

#[test]
fn a_kit_named_on_a_pool_row_is_refused_with_the_servers_own_text() {
    let (mut control, applied) = shipped_world();
    let id = *applied.bands.get(HOME_BAND).unwrap();
    let order = AssignLaborOrder {
        role: "roadwork".to_string(),
        workers: ROADWORK_MAX,
        kit_id: Some("gathering".to_string()),
        ..AssignLaborOrder::default()
    };
    let refusal = apply_assign_labor(&mut control.world, HOME, Some(id.0), order)
        .expect_err("a pool names no kit");
    assert!(
        refusal.to_string().contains(
            "a pool names no kit \u{2014} the road keepers' tools follow from the rung each road stands on"
        ),
        "{refusal}"
    );

    let file = with_labor(vec![roadwork_entry(Some("gathering"))]);
    let refusals = load(&file.to_string()).map(|_| ()).expect_err("refused");
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].path, "factions[0].labor[0]");
    assert_eq!(refusals[0].reason, refusal.to_string());
}

#[test]
fn a_highland_biome_tile_is_refused_a_swap_whether_or_not_the_mask_placed_it() {
    use core_sim::scenario::tiles::is_elevation_derived_highland;
    // Prefer a highland biome the mountain mask did NOT place (the case the metadata test missed).
    let highland = |tile: &Tile| {
        is_elevation_derived_highland(tile.terrain)
            && !tile.terrain_tags.contains(TerrainTags::WATER)
    };
    let at = find_tile(|tile| highland(tile) && tile.mountain.is_none())
        .or_else(|| find_tile(highland))
        .expect("the map has a highland tile");
    let refusals = load(
        &with_tiles(vec![
            json!({ "at": [at.x, at.y], "terrain": "AlluvialPlain" }),
        ])
        .to_string(),
    )
    .map(|_| ())
    .expect_err("a highland tile cannot be swapped away");
    assert_eq!(refusals[0].path, "tiles[0]");
    assert!(refusals[0].reason.contains("mountain"), "{}", refusals[0]);

    // The control: a lowland source tile takes the same swap.
    let lowland = find_tile(|tile| {
        !is_elevation_derived_highland(tile.terrain)
            && tile.mountain.is_none()
            && tile.terrain != TerrainType::NavigableRiver
            && !tile.terrain_tags.contains(TerrainTags::WATER)
            && tile.terrain != TerrainType::AlluvialPlain
    })
    .expect("the map has a lowland tile");
    load(
        &with_tiles(vec![
            json!({ "at": [lowland.x, lowland.y], "terrain": "AlluvialPlain" }),
        ])
        .to_string(),
    )
    .expect("a lowland tile is accepted");
}
