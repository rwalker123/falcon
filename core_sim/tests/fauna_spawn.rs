use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    spawn_initial_herds, spawn_initial_world, CultureManager, DiscoveryProgressLedger,
    FactionInventory, FaunaConfig, FaunaConfigHandle, GenerationRegistry, HerdDensityMap,
    HerdRegistry, HerdTelemetry, HusbandryCeiling, MapPresets, MapPresetsHandle, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
};

/// Stand up a land-rich earthlike map on `seed` and spawn the initial herds; return the app.
fn spawn_world_with_herds(seed: u64) -> App {
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

    // Build the world (populates TileRegistry + StartLocation), then spawn herds.
    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// Phase A acceptance: on a fresh land map, short-range wild game spawns as herds
/// (retiring `game_trail`). Big/small game groups get short routes and sit on land.
#[test]
fn short_range_game_spawns_on_land_biomes() {
    const SEED: u64 = 119304647; // deterministic land-rich map
    let app = spawn_world_with_herds(SEED);
    let (width, height) = {
        let config = app.world.resource::<SimulationConfig>();
        (config.grid_size.x, config.grid_size.y)
    };

    let registry = app.world.resource::<HerdRegistry>();
    assert!(
        !registry.herds.is_empty(),
        "expected herds to spawn on a land-rich map"
    );

    // Short-range game groups use the `game_*` id prefix; migratory herds use `herd_*`.
    let game: Vec<_> = registry
        .herds
        .iter()
        .filter(|h| h.id.starts_with("game_"))
        .collect();
    assert!(
        !game.is_empty(),
        "expected short-range wild game to spawn (found none; total herds = {})",
        registry.herds.len()
    );

    // Game groups roam short routes and stay in-bounds.
    for herd in &game {
        assert!(
            herd.route_length() >= 1 && herd.route_length() <= 3,
            "game group {} should have a short route (1-3), got {}",
            herd.id,
            herd.route_length()
        );
        let pos = herd.position();
        assert!(
            pos.x < width && pos.y < height,
            "game group {} position {:?} out of bounds",
            herd.id,
            pos
        );
    }

    // At least one recognizable game species surfaced (drives the client icon).
    let game_species = ["Red Deer", "Wild Boar", "Rabbit Warren", "Wild Fowl"];
    assert!(
        registry
            .herds
            .iter()
            .any(|h| game_species.contains(&h.species.as_str())),
        "expected at least one big/small game species among spawned herds"
    );
}

/// Grazing 2d: the two pennable grazer livestock (aurochs on grass/woodland, crag_goat on
/// highland/dry-upland) are wired to REAL `FoodModule` keys and actually spawn on their host biomes.
#[test]
fn pennable_grazers_are_wired_to_real_biomes_and_spawn() {
    let fauna = FaunaConfig::builtin();

    // --- Config wiring: both host on live module keys, and both are `pen`-ceiling (pennable). ---
    let aurochs = &fauna.species["aurochs"];
    assert_eq!(aurochs.husbandry_ceiling, HusbandryCeiling::Pen);
    for key in ["savanna_grassland", "temperate_forest", "mixed_woodland"] {
        assert!(aurochs.hosts_biome(key), "aurochs must host {key}");
        // A live key with a positive abundance density (or the species never spawns).
        assert!(
            fauna.abundance.probability_for(key) > 0.0,
            "aurochs host {key} has zero abundance"
        );
        // A non-migratory species hosting the key is discoverable by the spawn picker.
        assert!(fauna
            .game_species_for_biome(key)
            .iter()
            .any(|(k, _)| k.as_str() == "aurochs"));
    }
    let crag_goat = &fauna.species["crag_goat"];
    assert_eq!(crag_goat.husbandry_ceiling, HusbandryCeiling::Pen);
    for key in ["montane_highland", "semi_arid_scrub"] {
        assert!(crag_goat.hosts_biome(key), "crag_goat must host {key}");
        assert!(
            fauna.abundance.probability_for(key) > 0.0,
            "crag_goat host {key} has zero abundance"
        );
        assert!(fauna
            .game_species_for_biome(key)
            .iter()
            .any(|(k, _)| k.as_str() == "crag_goat"));
    }

    // --- Real spawn: on the pinned land-rich map both actually place at least one group. ---
    const SEED: u64 = 119304647;
    let app = spawn_world_with_herds(SEED);
    let registry = app.world.resource::<HerdRegistry>();
    let count = |display: &str| {
        registry
            .herds
            .iter()
            .filter(|h| h.species == display)
            .count()
    };
    let aurochs_spawned = count("Wild Aurochs");
    let goat_spawned = count("Crag Goats");
    println!(
        "spawned Wild Aurochs={aurochs_spawned} Crag Goats={goat_spawned} (total herds {})",
        registry.herds.len()
    );
    assert!(
        aurochs_spawned > 0,
        "Wild Aurochs must spawn on the grassland/woodland map (got {aurochs_spawned}; \
         total herds {})",
        registry.herds.len()
    );
    assert!(
        goat_spawned > 0,
        "Crag Goats must spawn on the highland/upland map (got {goat_spawned}; total herds {})",
        registry.herds.len()
    );

    // A spawned aurochs carries the cached `pen` ceiling → it is pennable.
    let aurochs_herd = registry
        .herds
        .iter()
        .find(|h| h.species == "Wild Aurochs")
        .unwrap();
    assert!(
        aurochs_herd.can_pen() && aurochs_herd.husbandry_ceiling == HusbandryCeiling::Pen,
        "a spawned aurochs is pennable"
    );
}
