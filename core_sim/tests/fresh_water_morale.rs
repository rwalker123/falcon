//! **The best land is a slow net morale GAIN** (terrain allowance + fresh-water term).
//!
//! `turn_pipeline_config.json` -> `population`:
//! - `attrition_morale_tolerance` (0.05): terrain attrition up to this rate drains no morale, so the
//!   gentlest ground (`AlluvialPlain`, 0.05) costs nothing and settling's gain is kept;
//! - `fresh_water_morale_bonus` (0.005): a band standing on fresh-watered ground
//!   (`forage::tile_is_fresh_watered`) gains it every turn. It is a separate good, never part of the
//!   published `habitability`, and never a `MoraleCause`.
//!
//! Driven through the real `simulate_population` on a generated world; every expected number is
//! derived from the levers the world loaded. Each claim has its control.

use std::sync::Arc;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use sim_runtime::TerrainTags;

mod faction_support;

use core_sim::grid_utils::hex_neighbors_wrapped;
use core_sim::{
    publish_baseline_snapshot, scalar_from_f32, simulate_population, BandId, BeliefRegistry,
    MoraleCause, PopulationCohort, ResidentBand, Scalar, SimulationConfig, SnapshotHistory, Tile,
    TileRegistry, TurnPipelineConfig, TurnPipelineConfigHandle, BUILTIN_TURN_PIPELINE_CONFIG,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_runtime::TerrainType;

/// A larder many lifetimes deep, so a staged turn kills nobody by starvation.
const DEEP_LARDER: f32 = 1.0e6;
/// Fixed-point comparison tolerance, in raw units.
const RAW_TOLERANCE: i64 = 3;
/// Morale a staged band opens a turn at: far from both clamps, so the delta is observable.
const MID_MORALE: f32 = 0.5;
/// Terrain definitions' own numbers, restated from `terrain.rs` as the test's independent oracle.
const ALLUVIAL_ATTRITION: f32 = 0.05;
const ALLUVIAL_LOGISTICS: f32 = 0.95;
const FLOODPLAIN_ATTRITION: f32 = 0.08;
const FLOODPLAIN_LOGISTICS: f32 = 1.05;
/// The shipped tolerance, restated so the control arm can be built by editing the builtin JSON.
const SHIPPED_TOLERANCE_JSON: &str = "\"attrition_morale_tolerance\": 0.05";
const NO_TOLERANCE_JSON: &str = "\"attrition_morale_tolerance\": 0.0";

/// A staging step: the app, the band's position, its neighbours, its own tile.
type Staging = dyn Fn(&mut App, UVec2, &[Entity], Entity);

fn pipeline(app: &App) -> Arc<TurnPipelineConfig> {
    app.world.resource::<TurnPipelineConfigHandle>().get()
}

fn with_tolerance_off(app: &mut App) {
    assert!(BUILTIN_TURN_PIPELINE_CONFIG.contains(SHIPPED_TOLERANCE_JSON));
    let edited = BUILTIN_TURN_PIPELINE_CONFIG.replace(SHIPPED_TOLERANCE_JSON, NO_TOLERANCE_JSON);
    let config = TurnPipelineConfig::from_json_str(&edited).expect("the edited config parses");
    app.world
        .insert_resource(TurnPipelineConfigHandle::new(Arc::new(config)));
}

fn lone_band_world() -> (App, Entity) {
    let mut app = one_faction_world();
    app.world.insert_resource(BeliefRegistry::default());
    let band = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .min_by_key(|(_, id, _)| **id)
        .map(|(entity, _, _)| entity)
        .expect("the home people has an opening band");
    let home = app.world.get::<PopulationCohort>(band).unwrap().home;
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.current_tile = home;
    cohort.morale = scalar_from_f32(MID_MORALE);
    cohort
        .stores
        .reset_food("dry", scalar_from_f32(DEEP_LARDER));
    (app, band)
}

fn tile_at(app: &App, x: u32, y: u32) -> Entity {
    app.world
        .resource::<TileRegistry>()
        .index(x, y)
        .expect("on the map")
}

/// Stage the band's ground: `terrain` with fertile tags only, ambient temperature (no climate
/// drain), no river, and every neighbour a plain dry fertile tile. Returns the band's position and
/// the neighbours' entities.
fn stage_ground(app: &mut App, band: Entity, terrain: TerrainType) -> (UVec2, Vec<Entity>) {
    let home = app.world.get::<PopulationCohort>(band).unwrap().home;
    let position = app.world.get::<Tile>(home).unwrap().position;
    let ambient = app.world.resource::<SimulationConfig>().ambient_temperature;
    {
        let mut tile = app.world.get_mut::<Tile>(home).unwrap();
        tile.terrain = terrain;
        tile.terrain_tags = TerrainTags::FERTILE;
        tile.temperature = ambient;
        tile.river_edges = 0;
    }
    let (width, height, wrap) = {
        let registry = app.world.resource::<TileRegistry>();
        (
            registry.width,
            registry.height,
            app.world
                .resource::<SimulationConfig>()
                .map_topology
                .wrap_horizontal,
        )
    };
    let neighbours: Vec<Entity> =
        hex_neighbors_wrapped(position.x, position.y, width, height, wrap)
            .map(|(x, y)| tile_at(app, x, y))
            .collect();
    for &neighbour in &neighbours {
        let mut tile = app.world.get_mut::<Tile>(neighbour).unwrap();
        tile.terrain_tags = TerrainTags::FERTILE;
        tile.river_edges = 0;
    }
    (position, neighbours)
}

fn turn(app: &mut App) {
    app.world.run_system_once(simulate_population);
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap()
}

fn assert_scalar_eq(actual: Scalar, expected: Scalar, what: &str) {
    assert!(
        (actual.raw() - expected.raw()).abs() <= RAW_TOLERANCE,
        "{what}: got {}, expected {}",
        actual.to_f32(),
        expected.to_f32()
    );
}

/// What the terrain term reads from the levers: `-(max(0, attrition - tol) * scale + hardness)`.
fn expected_terrain(app: &App, attrition: f32, logistics: f32, tolerance_off: bool) -> Scalar {
    let config = pipeline(app);
    let tolerance = if tolerance_off {
        0.0
    } else {
        config.population().attrition_morale_tolerance().to_f32()
    };
    let attrition_term =
        (attrition - tolerance).max(0.0) * config.population().attrition_penalty_scale().to_f32();
    let hardness_term =
        (logistics - 1.0).max(0.0) * config.population().hardness_penalty_scale().to_f32();
    scalar_from_f32(-(attrition_term + hardness_term))
}

/// **AlluvialPlain on temperate, dry ground costs nothing**, and the band's morale rises by
/// settling's gain. Control: at tolerance 0 the old drain returns and eats the gain exactly.
#[test]
fn the_gentlest_ground_drains_nothing_and_morale_rises() {
    let (mut app, band) = lone_band_world();
    stage_ground(&mut app, band, TerrainType::AlluvialPlain);
    let before = cohort(&app, band).morale;
    turn(&mut app);
    let c = cohort(&app, band);
    assert_eq!(
        c.last_morale_contributions.terrain,
        Scalar::zero(),
        "the allowance covers the whole of AlluvialPlain's attrition"
    );
    assert_eq!(c.last_morale_contributions.climate, Scalar::zero());
    assert_eq!(c.last_morale_contributions.fresh_water, Scalar::zero());
    assert!(c.morale > before, "morale rises on the best dry land");
    assert_eq!(c.last_morale_cause, MoraleCause::None);

    // Control: no allowance -> the old 1%/turn drain, exactly cancelling settling.
    let (mut app, band) = lone_band_world();
    with_tolerance_off(&mut app);
    stage_ground(&mut app, band, TerrainType::AlluvialPlain);
    turn(&mut app);
    let terrain = cohort(&app, band).last_morale_contributions.terrain;
    assert_scalar_eq(
        terrain,
        expected_terrain(&app, ALLUVIAL_ATTRITION, ALLUVIAL_LOGISTICS, true),
        "control: tolerance 0 restores the old terrain drain",
    );
    assert!(terrain < Scalar::zero(), "liveness: a real drain");
}

/// **Floodplain** (attrition 0.08, logistics 1.05) pays only the part above the allowance, plus its
/// hardness term: `-((0.08 - 0.05) x scale + 0.05 x hardness_scale)`.
#[test]
fn a_rougher_ground_pays_only_above_the_allowance() {
    let (mut app, band) = lone_band_world();
    stage_ground(&mut app, band, TerrainType::Floodplain);
    turn(&mut app);
    let terrain = cohort(&app, band).last_morale_contributions.terrain;
    assert_scalar_eq(
        terrain,
        expected_terrain(&app, FLOODPLAIN_ATTRITION, FLOODPLAIN_LOGISTICS, false),
        "floodplain terrain term",
    );
    assert!(terrain < Scalar::zero(), "liveness");

    let (mut app, band) = lone_band_world();
    with_tolerance_off(&mut app);
    stage_ground(&mut app, band, TerrainType::Floodplain);
    turn(&mut app);
    let without = cohort(&app, band).last_morale_contributions.terrain;
    assert!(
        without < terrain,
        "control: with no allowance the same ground drains more"
    );
}

/// **Fresh water is worth the lever, and exactly `tile_is_fresh_watered` decides it**: a river edge
/// counts, a FRESHWATER neighbour counts, a salt coast (ContinentalShelf neighbour) does not.
#[test]
fn fresh_water_pays_the_bonus_by_the_one_shared_rule() {
    let bonus = |staging: &Staging| -> (Scalar, Scalar) {
        let (mut app, band) = lone_band_world();
        let (position, neighbours) = stage_ground(&mut app, band, TerrainType::AlluvialPlain);
        let home = app.world.get::<PopulationCohort>(band).unwrap().home;
        staging(&mut app, position, &neighbours, home);
        turn(&mut app);
        let c = cohort(&app, band);
        (
            c.last_morale_contributions.fresh_water,
            c.last_morale_contributions.terrain,
        )
    };
    let lever = {
        let (app, _) = lone_band_world();
        pipeline(&app).population().fresh_water_morale_bonus()
    };
    assert!(lever > Scalar::zero(), "fixture: a live lever");

    let (dry, _) = bonus(&|_, _, _, _| {});
    assert_eq!(dry, Scalar::zero(), "control: dry ground earns nothing");

    let (river, _) = bonus(&|app, _, _, home| {
        app.world.get_mut::<Tile>(home).unwrap().river_edges = 1;
    });
    assert_eq!(river, lever, "a river edge on the band's tile");

    let (lake_side, _) = bonus(&|app, _, neighbours, _| {
        app.world
            .get_mut::<Tile>(neighbours[0])
            .unwrap()
            .terrain_tags = TerrainTags::WATER | TerrainTags::FRESHWATER;
    });
    assert_eq!(lake_side, lever, "a FRESHWATER neighbour");

    let (own, _) = bonus(&|app, _, _, home| {
        app.world.get_mut::<Tile>(home).unwrap().terrain_tags =
            TerrainTags::FERTILE | TerrainTags::FRESHWATER;
    });
    assert_eq!(own, lever, "FRESHWATER on the band's own tile");

    let (salt, _) = bonus(&|app, _, neighbours, _| {
        let mut tile = app.world.get_mut::<Tile>(neighbours[0]).unwrap();
        tile.terrain = TerrainType::ContinentalShelf;
        tile.terrain_tags = TerrainTags::WATER | TerrainTags::COASTAL;
    });
    assert_eq!(salt, Scalar::zero(), "a salt coast is not fresh water");
}

/// **The bonus is a gain, not a cause**: it adds to the morale delta, a band whose only negative
/// term is tiny never names it, and the delta is the signed sum with it included.
#[test]
fn fresh_water_adds_to_the_delta_and_is_never_a_cause() {
    let (mut app, band) = lone_band_world();
    let (_, neighbours) = stage_ground(&mut app, band, TerrainType::Floodplain);
    app.world
        .get_mut::<Tile>(neighbours[0])
        .unwrap()
        .terrain_tags = TerrainTags::WATER | TerrainTags::FRESHWATER;
    turn(&mut app);
    let c = cohort(&app, band);
    let contributions = c.last_morale_contributions;
    assert!(contributions.fresh_water > Scalar::zero());
    assert_eq!(contributions.total(), c.last_morale_delta);
    // Floodplain still drains, so a cause is named, and it is a negative term - never a good.
    assert_eq!(
        c.last_morale_cause == MoraleCause::None,
        c.last_morale_delta >= Scalar::zero()
    );
}

/// **One computation**: the published `habitability` on the wire equals the morale terrain + cold
/// drain the band was charged, and fresh water does not change it.
#[test]
fn habitability_is_the_morale_drain_and_ignores_fresh_water() {
    let read = |watered: bool| -> (i64, Scalar, Scalar) {
        let (mut app, band) = lone_band_world();
        let (position, neighbours) = stage_ground(&mut app, band, TerrainType::Floodplain);
        if watered {
            app.world
                .get_mut::<Tile>(neighbours[0])
                .unwrap()
                .terrain_tags = TerrainTags::WATER | TerrainTags::FRESHWATER;
        }
        turn(&mut app);
        let contributions = cohort(&app, band).last_morale_contributions;
        publish_baseline_snapshot(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let habitability = envelope
            .payload_as_snapshot()
            .expect("a snapshot payload")
            .map()
            .and_then(|map| map.tiles())
            .expect("tiles are published")
            .iter()
            .find(|tile| tile.x() == position.x && tile.y() == position.y)
            .expect("the band's tile is published")
            .habitability();
        (habitability, contributions.terrain, contributions.climate)
    };
    let (dry_hab, terrain, climate) = read(false);
    assert!(terrain < Scalar::zero(), "fixture: a live drain");
    assert!(
        (dry_hab - (-(terrain + climate)).raw()).abs() <= RAW_TOLERANCE,
        "habitability {dry_hab} equals the drain {}",
        -(terrain + climate).raw()
    );
    let (wet_hab, _, _) = read(true);
    assert_eq!(wet_hab, dry_hab, "fresh water never changes habitability");
}

/// **`moraleFreshWater` is on the encoded envelope**, equal to the term the turn priced.
#[test]
fn the_term_is_on_the_encoded_snapshot() {
    let (mut app, band) = lone_band_world();
    let (_, neighbours) = stage_ground(&mut app, band, TerrainType::AlluvialPlain);
    app.world
        .get_mut::<Tile>(neighbours[0])
        .unwrap()
        .terrain_tags = TerrainTags::WATER | TerrainTags::FRESHWATER;
    turn(&mut app);
    let term = cohort(&app, band).last_morale_contributions.fresh_water;
    assert!(term > Scalar::zero(), "fixture: a live term");

    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.entity() == band.to_bits())
        .expect("the band is on the wire");
    assert_eq!(row.moraleFreshWater(), term.raw());
}

/// The two levers reject a negative or non-finite value at parse.
#[test]
fn the_levers_are_validated() {
    for (key, value) in [
        ("attrition_morale_tolerance", "-0.1"),
        ("fresh_water_morale_bonus", "-0.1"),
    ] {
        let from = BUILTIN_TURN_PIPELINE_CONFIG
            .lines()
            .find(|line| line.contains(key))
            .expect("the shipped lever");
        let edited = BUILTIN_TURN_PIPELINE_CONFIG.replace(from, &format!("\"{key}\": {value},"));
        assert!(
            TurnPipelineConfig::from_json_str(&edited).is_err(),
            "{key} = {value} must be refused"
        );
    }
}
