use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FogRevealLedger,
    FollowPolicy, ForageRegistry, GenerationId, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FOOD,
};

/// Whole-worker head-count assigned to the hunt — large enough that the per-worker biomass cap
/// never binds, so the take is set by the policy ceiling.
const HUNT_WORKERS: u32 = 5000;

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
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// A band adjacent to a herd hunts it: biomass drops and provisions accrue to its larder.
#[test]
fn hunt_assignment_takes_biomass_and_yields() {
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

    // Spawn a band standing on the herd's tile with an Eradicate Hunt assignment.
    let faction = FactionId(0);
    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile_entity,
                current_tile: tile_entity,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(HUNT_WORKERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction,
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.clone(),
                        policy: FollowPolicy::Eradicate,
                    },
                    workers: HUNT_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id();

    app.world.run_system_once(advance_labor_allocation);

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

    // Provisions accrued to the hunting band's local larder (food is band-local now).
    let food_store = app
        .world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0);
    assert!(
        food_store > 0.0,
        "expected provisions in the band larder from the hunt, got {food_store}"
    );

    // The Hunt assignment persists (the band keeps hunting while in range).
    let still_hunting = app
        .world
        .get::<LaborAllocation>(band)
        .map(|a| {
            a.assignments
                .iter()
                .any(|x| matches!(x.target, LaborTarget::Hunt { .. }))
        })
        .unwrap_or(false);
    assert!(
        still_hunting,
        "Hunt assignment should persist while in range"
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
        let cap = registry.herds[0].carrying_capacity;
        let low = (cap * 0.5).max(1.0);
        registry.herds[0].biomass = low;
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
