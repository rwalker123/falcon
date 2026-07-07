use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    spawn_initial_herds, spawn_initial_world, CultureManager, DiscoveryProgressLedger,
    FactionInventory, FaunaConfigHandle, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, MapPresets, MapPresetsHandle, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle,
};

/// Phase A acceptance: on a fresh land map, short-range wild game spawns as herds
/// (retiring `game_trail`). Big/small game groups get short routes and sit on land.
#[test]
fn short_range_game_spawns_on_land_biomes() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647; // deterministic land-rich map
    let width = config.grid_size.x;
    let height = config.grid_size.y;
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
