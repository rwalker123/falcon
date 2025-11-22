use bevy::app::App;
use bevy::MinimalPlugins;

use core_sim::{
    generate_hydrology, spawn_initial_world, CultureManager, DiscoveryProgressLedger,
    FactionInventory, GenerationRegistry, HydrologyOverrides, HydrologyState, MapPresets,
    MapPresetsHandle, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle,
};

#[test]
fn earthlike_preset_generates_rivers() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647; // deterministic seed
    config.hydrology = HydrologyOverrides {
        river_density: Some(1.4),
        river_min_count: Some(8),
        river_max_count: Some(24),
        accumulation_threshold_factor: Some(0.2),
        source_percentile: Some(0.55),
        source_sea_buffer: Some(0.04),
        min_length: Some(8),
        fallback_min_length: Some(4),
        spacing: Some(8.0),
        uphill_gain_pct: Some(0.07),
    };

    app.world.insert_resource(config);
    let presets = MapPresets::builtin();
    app.world
        .insert_resource(MapPresetsHandle::new(presets.clone()));
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

    generate_hydrology(&mut app.world);

    let hydrology = app.world.resource::<HydrologyState>();
    assert!(
        !hydrology.rivers.is_empty(),
        "expected earthlike preset to generate rivers"
    );
    let max_len = hydrology
        .rivers
        .iter()
        .map(|r| r.path.len())
        .max()
        .unwrap_or(0);
    assert!(
        max_len >= 8,
        "expected at least one river to reach config minimum length, got {}",
        max_len
    );
}
