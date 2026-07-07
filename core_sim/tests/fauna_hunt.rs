use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_fauna_pursuits, advance_herds, scalar_one, spawn_initial_herds, spawn_initial_world,
    CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory,
    FaunaConfigHandle, FaunaPursuit, FaunaPursuitMode, FogRevealLedger, GenerationId,
    HerdDensityMap, HerdRegistry, HerdTelemetry, MapPresets, MapPresetsHandle, PopulationCohort,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry,
};

/// Build a land-rich world with herds spawned (mirrors `fauna_spawn.rs` setup).
fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(core_sim::GenerationRegistry::with_seed(42, 8));
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
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// A band adjacent to a herd hunts it: biomass drops and provisions/trade accrue.
#[test]
fn hunt_pursuit_takes_biomass_and_yields() {
    let mut app = spawn_world();

    // Pick a short-range game herd and note its id / tile / biomass.
    let (herd_id, herd_pos, biomass_before) = {
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_"))
            .expect("expected short-range game to spawn");
        (herd.id.clone(), herd.position(), herd.biomass)
    };

    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(herd_pos.x, herd_pos.y)
        .expect("herd tile should resolve");

    // Spawn a band already standing on the herd's tile with a hunt pursuit attached.
    let faction = FactionId(0);
    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile_entity,
                current_tile: tile_entity,
                size: 30,
                morale: scalar_one(),
                generation: 0 as GenerationId,
                faction,
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            FaunaPursuit {
                faction,
                band_label: "Test Band".to_string(),
                fauna_id: herd_id.clone(),
                mode: FaunaPursuitMode::Hunt,
                elapsed_turns: 0,
                started_tick: 0,
            },
        ))
        .id();

    app.world.run_system_once(advance_fauna_pursuits);

    // Herd biomass dropped by the take.
    let biomass_after = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd_id)
        .map(|h| h.biomass)
        .expect("herd should still exist after a single hunt");
    assert!(
        biomass_after < biomass_before,
        "expected biomass to drop (before {biomass_before}, after {biomass_after})"
    );

    // Provisions accrued to the hunting faction.
    let provisions = app
        .world
        .resource::<FactionInventory>()
        .stockpile(faction)
        .and_then(|s| s.get("provisions").copied())
        .unwrap_or(0);
    assert!(provisions > 0, "expected provisions from the hunt, got 0");

    // The one-shot pursuit is consumed (component removed).
    assert!(
        app.world.get::<FaunaPursuit>(band).is_none(),
        "FaunaPursuit should be removed after resolving"
    );
}

/// Biomass regrows toward the carrying cap each turn; a group at zero despawns.
#[test]
fn biomass_regrows_and_extinct_group_despawns() {
    let mut app = spawn_world();

    // Choose two herds: one to draw down (regrowth), one to zero out (extinction).
    let (regrow_id, extinct_id, count_before) = {
        let registry = app.world.resource::<HerdRegistry>();
        assert!(registry.herds.len() >= 2, "need at least two herds");
        (
            registry.herds[0].id.clone(),
            registry.herds[1].id.clone(),
            registry.herds.len(),
        )
    };

    let low_biomass = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        // Draw herd 0 down to half its cap: below capacity so logistic regrowth is
        // clearly positive, but above the Phase D collapse threshold so it recovers
        // rather than crashing.
        let cap = registry.herds[0].carrying_capacity;
        let low = (cap * 0.5).max(1.0);
        registry.herds[0].biomass = low;
        // Zero out herd 1 -> local extinction.
        registry.herds[1].biomass = 0.0;
        low
    };

    app.world.run_system_once(advance_herds);

    let registry = app.world.resource::<HerdRegistry>();
    let regrown = registry
        .find(&regrow_id)
        .map(|h| h.biomass)
        .expect("regrowing herd should still exist");
    assert!(
        regrown > low_biomass,
        "expected biomass to regrow above {low_biomass}, got {regrown}"
    );
    assert!(
        registry.find(&extinct_id).is_none(),
        "expected the zero-biomass group to despawn (local extinction)"
    );
    assert_eq!(
        registry.herds.len(),
        count_before - 1,
        "exactly one herd should have gone extinct"
    );
}
