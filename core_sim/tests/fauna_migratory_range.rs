//! Change B — host-biome-aware migratory placement (`build_migratory_route`).
//!
//! Migratory herds used to build their route as a jittered spiral around the player start, ignoring
//! `host_biomes` entirely — so a migratory species clustered at the start regardless of biome. Now a
//! herd's loiter **anchors** sit on tiles suitable for its species (`module_at ∈ host_biomes`), drawn
//! from a regional home range, with the migration legs crossing whatever less-suitable ground lies
//! between. These tests run the real Startup worldgen + `spawn_initial_herds` on pinned seeds.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    classify_food_module, spawn_initial_herds, spawn_initial_world, CultureManager,
    DiscoveryProgressLedger, FactionInventory, FaunaConfig, FaunaConfigHandle, FoodModule,
    GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry, MapPresets, MapPresetsHandle,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
};

/// Mirrors `build_migratory_route`'s own floor (and `build_route`'s `< 3`).
const MIN_ANCHORS: usize = 3;

/// Stand up a land-rich earthlike map on `seed`, insert `fauna` as the herd config, and spawn herds.
fn spawn_world_with_fauna(seed: u64, fauna: FaunaConfig) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world
        .insert_resource(FaunaConfigHandle::new(Arc::new(fauna)));
    app.world.run_system_once(spawn_initial_herds);
    app
}

fn builtin_fauna() -> FaunaConfig {
    (*FaunaConfig::builtin()).clone()
}

fn spawn_world(seed: u64) -> App {
    spawn_world_with_fauna(seed, builtin_fauna())
}

/// The tile's food module at `pos`, or `None` for water / off-map (mirrors `fauna::module_at`).
fn module_at(app: &App, pos: UVec2) -> Option<FoodModule> {
    let entity = app.world.resource::<TileRegistry>().index(pos.x, pos.y)?;
    let tile = app.world.get::<Tile>(entity)?;
    classify_food_module(tile)
}

/// The random-placement baseline: the fraction of module-bearing tiles whose module is in
/// `host_biomes` — i.e. what the pre-change biome-blind spiral would have scored in expectation.
fn land_host_fraction(app: &App, width: u32, height: u32, host_biomes: &[String]) -> f64 {
    let mut module_tiles = 0u64;
    let mut host_tiles = 0u64;
    for y in 0..height {
        for x in 0..width {
            if let Some(module) = module_at(app, UVec2::new(x, y)) {
                module_tiles += 1;
                if host_biomes.iter().any(|b| b == module.as_str()) {
                    host_tiles += 1;
                }
            }
        }
    }
    if module_tiles == 0 {
        0.0
    } else {
        host_tiles as f64 / module_tiles as f64
    }
}

/// Aggregate over several seeds: a majority of migratory herds' route anchors land on host-biome
/// tiles, and that fraction is clearly better than the biome-blind random-placement baseline.
#[test]
fn migratory_anchors_land_predominantly_in_host_biomes() {
    const SEEDS: [u64; 6] = [119304647, 11, 4242, 90210, 7, 13];

    let mut in_biome = 0u64;
    let mut total_anchors = 0u64;
    // Baseline weighted by the same anchor count, so it is comparable to the observed fraction.
    let mut baseline_weighted = 0.0f64;

    for seed in SEEDS {
        let app = spawn_world(seed);
        let (width, height) = {
            let config = app.world.resource::<SimulationConfig>();
            (config.grid_size.x, config.grid_size.y)
        };
        let fauna = builtin_fauna();

        // Snapshot the migratory herds first (release the immutable borrow before classifying tiles).
        let herds: Vec<(String, Vec<UVec2>)> = app
            .world
            .resource::<HerdRegistry>()
            .herds
            .iter()
            .filter(|h| h.id.starts_with("herd_"))
            .map(|h| (h.species.clone(), h.route.clone()))
            .collect();

        for (species, route) in herds {
            let def = fauna
                .species_by_display(&species)
                .unwrap_or_else(|| panic!("species {species} must resolve"));
            let base = land_host_fraction(&app, width, height, &def.host_biomes);
            for anchor in &route {
                total_anchors += 1;
                baseline_weighted += base;
                if let Some(module) = module_at(&app, *anchor) {
                    if def.host_biomes.iter().any(|b| b == module.as_str()) {
                        in_biome += 1;
                    }
                }
            }
        }
    }

    assert!(
        total_anchors > 0,
        "expected migratory herds across the seeds"
    );
    let observed = in_biome as f64 / total_anchors as f64;
    let baseline = baseline_weighted / total_anchors as f64;

    // A majority land in-biome (not all — NN-chain legs and the fallback path legitimately cross
    // non-suitable ground), and clearly above 50%.
    assert!(
        observed > 0.5,
        "expected a majority of anchors in-biome, got {observed:.3} ({in_biome}/{total_anchors})"
    );
    // Strictly better than the biome-blind baseline would have scored.
    assert!(
        observed > baseline + 0.15,
        "expected in-biome fraction ({observed:.3}) to clearly beat the random baseline \
         ({baseline:.3})"
    );
}

/// Focused reindeer case: strip every migratory row except reindeer so all migratory herds are
/// reindeer, then assert their anchors sit predominantly on `boreal_arctic`/`montane_highland`.
#[test]
fn reindeer_anchors_are_predominantly_boreal_or_montane() {
    const SEED: u64 = 119304647;
    let mut fauna = builtin_fauna();
    // Keep reindeer + every non-migratory row; drop the other migratory species.
    fauna
        .species
        .retain(|_, def| !def.migratory || def.display_name == "Wild Reindeer");
    assert!(
        fauna.migratory_species().len() == 1,
        "test setup: only reindeer should remain migratory"
    );

    let app = spawn_world_with_fauna(SEED, fauna);
    let reindeer_hosts = ["boreal_arctic", "montane_highland"];

    let routes: Vec<Vec<UVec2>> = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|h| h.species == "Wild Reindeer")
        .map(|h| h.route.clone())
        .collect();
    assert!(!routes.is_empty(), "expected reindeer herds to spawn");

    let mut in_biome = 0u64;
    let mut total = 0u64;
    for route in &routes {
        assert!(
            route.len() >= MIN_ANCHORS,
            "reindeer route must have >= {MIN_ANCHORS} anchors, got {}",
            route.len()
        );
        for anchor in route {
            total += 1;
            if let Some(module) = module_at(&app, *anchor) {
                if reindeer_hosts.contains(&module.as_str()) {
                    in_biome += 1;
                }
            }
        }
    }
    let observed = in_biome as f64 / total as f64;
    assert!(
        observed > 0.5,
        "expected reindeer anchors predominantly boreal/montane, got {observed:.3} \
         ({in_biome}/{total})"
    );
}

/// Graceful fallback: a migratory species whose host biomes match no tile on the map still produces
/// a valid >=3-anchor route. Strip every migratory row's host biomes so `suitable` is empty for all
/// of them, forcing the `build_route(base, ..)` fallback.
#[test]
fn a_migratory_species_with_no_host_tiles_falls_back_to_a_valid_route() {
    const SEED: u64 = 119304647;
    let mut fauna = builtin_fauna();
    for def in fauna.species.values_mut() {
        if def.migratory {
            def.host_biomes.clear();
        }
    }

    let app = spawn_world_with_fauna(SEED, fauna);
    let migratory: Vec<Vec<UVec2>> = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|h| h.id.starts_with("herd_"))
        .map(|h| h.route.clone())
        .collect();

    assert!(
        !migratory.is_empty(),
        "expected migratory herds to spawn even with no host tiles"
    );
    for route in &migratory {
        assert!(
            route.len() >= MIN_ANCHORS,
            "fallback route must still be valid (>= {MIN_ANCHORS} anchors), got {}",
            route.len()
        );
    }
}

/// Determinism: two spawns with the same seed produce byte-identical herd routes (the whole engine
/// is rollback-deterministic).
#[test]
fn migratory_spawn_is_deterministic() {
    const SEED: u64 = 90210;
    let a = spawn_world(SEED);
    let b = spawn_world(SEED);

    let routes_a: Vec<(String, Vec<UVec2>)> = a
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| (h.id.clone(), h.route.clone()))
        .collect();
    let routes_b: Vec<(String, Vec<UVec2>)> = b
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .map(|h| (h.id.clone(), h.route.clone()))
        .collect();

    assert_eq!(
        routes_a, routes_b,
        "identical seeds must produce byte-identical herd routes"
    );
}
