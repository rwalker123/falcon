//! Early-Game Labor slice 3a: per-worker Forage/Hunt yields, the leashed-follow lapse, and the
//! Σ-workers ≤ working-age invariant.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_labor_allocation, available_workers, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FogRevealLedger,
    FollowPolicy, FoodModuleTag, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig, LaborConfigHandle, LaborTarget,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
    WellbeingConfigHandle, FOOD,
};

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
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// Spawn a content band (morale 1 → output multiplier 1.0) on `tile` with `working` whole workers
/// and the given labor allocation.
fn spawn_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    working: u32,
    allocation: LaborAllocation,
) -> bevy::prelude::Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(working as f32),
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
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            allocation,
        ))
        .id()
}

fn forage_alloc(tile: UVec2, workers: u32) -> LaborAllocation {
    LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Forage { tile },
            workers,
        }],
    }
}

/// Find a food-module tile: its position + entity.
fn food_tile(app: &mut App) -> (UVec2, bevy::prelude::Entity) {
    let pos = {
        let mut q = app.world.query::<(&FoodModuleTag, &Tile)>();
        q.iter(&app.world)
            .map(|(_, tile)| tile.position)
            .next()
            .expect("expected at least one food-module tile")
    };
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("food tile resolves");
    (pos, entity)
}

fn larder(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

/// (a) Forage yield scales linearly with the assigned worker count.
#[test]
fn forage_yield_scales_with_workers() {
    let mut app = spawn_world();
    let (pos, tile) = food_tile(&mut app);
    let one = spawn_band(&mut app, tile, 10, forage_alloc(pos, 1));
    let two = spawn_band(&mut app, tile, 10, forage_alloc(pos, 2));

    app.world.run_system_once(advance_labor_allocation);

    let a = larder(&app, one);
    let b = larder(&app, two);
    assert!(a > 0.0, "a single forager should yield food, got {a}");
    // Same tile, same output multiplier → exactly double for double the workers.
    assert!(
        (b - 2.0 * a).abs() < 1e-4,
        "two foragers should yield ~2× one: {a} vs {b}"
    );
}

/// (b) A Sustain hunt whose per-worker cap is below the herd's regrowth leaves the herd growing.
#[test]
fn sustain_hunt_below_regrowth_lets_herd_grow() {
    let mut app = spawn_world();
    // Tiny per-worker biomass cap so `worker_cap < net regrowth` at any sane worker count.
    let json = r#"{
        "band_work_range": 2,
        "hunt_leash_tiles": 3,
        "band_move_tiles_per_turn": 1,
        "forage": { "per_worker_yield": 0.25 },
        "hunt": { "per_worker_biomass_capacity": 0.05 },
        "scout": { "vantage_distance_base": 2, "vantage_distance_per_scout": 1, "vantage_distance_max": 6, "vantage_range": 2 }
    }"#;
    app.world.insert_resource(LaborConfigHandle::new(Arc::new(
        LaborConfig::from_json_str(json).expect("custom labor config parses"),
    )));

    // A stationary herd at half its cap → clear positive regrowth.
    let (id, start) = {
        let id = {
            let registry = app.world.resource::<HerdRegistry>();
            registry
                .herds
                .iter()
                .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
                .or_else(|| registry.herds.iter().find(|h| h.id.starts_with("game_")))
                .map(|h| h.id.clone())
                .expect("expected short-range game")
        };
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.biomass = (herd.carrying_capacity * 0.5).max(1.0);
        (id, herd.biomass)
    };
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    spawn_band(
        &mut app,
        tile,
        10,
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: id.clone(),
                    policy: FollowPolicy::Sustain,
                },
                workers: 1,
            }],
        },
    );

    for _ in 0..8 {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_labor_allocation);
    }

    let after = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .map(|h| h.biomass)
        .expect("under-hunted herd survives");
    assert!(
        after > start,
        "under-hunting (worker_cap < regrowth) should let the herd grow: {start} -> {after}"
    );
}

/// (c) A Hunt assignment lapses once the herd is beyond `band_work_range + hunt_leash_tiles`.
#[test]
fn hunt_lapses_beyond_leash() {
    let mut app = spawn_world();
    let (id, herd_pos) = {
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_"))
            .expect("expected game herd");
        (herd.id.clone(), herd.position())
    };
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    // A tile at least 7 tiles away on X (> band_work_range 2 + hunt_leash_tiles 3 = 5).
    let far_x = if herd_pos.x + 7 < grid.x {
        herd_pos.x + 7
    } else {
        herd_pos.x.saturating_sub(7)
    };
    let far = UVec2::new(far_x, herd_pos.y);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(far.x, far.y)
        .expect("far tile resolves");
    let band = spawn_band(
        &mut app,
        tile,
        10,
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: id,
                    policy: FollowPolicy::Sustain,
                },
                workers: 3,
            }],
        },
    );

    app.world.run_system_once(advance_labor_allocation);

    let assignments = app
        .world
        .get::<LaborAllocation>(band)
        .map(|a| a.assignments.len())
        .unwrap_or(0);
    assert_eq!(
        assignments, 0,
        "an out-of-leash Hunt assignment should lapse and return its workers to the pool"
    );
}

/// (d) `Σ assignments.workers` is clamped to the band's working-age head-count.
#[test]
fn assignment_sum_clamps_to_working_age() {
    let mut alloc = LaborAllocation::default();
    let available = 5;

    // Forage 3 workers (fits).
    let applied = alloc.set_assignment(
        LaborTarget::Forage {
            tile: UVec2::new(1, 1),
        },
        3,
        available,
    );
    assert_eq!(applied, 3);

    // Scout 4 workers requested, but only 2 headroom left → clamped to 2.
    let applied = alloc.set_assignment(LaborTarget::Scout, 4, available);
    assert_eq!(applied, 2, "over-budget assignment clamps to free headroom");
    assert_eq!(alloc.assigned_total(), available);

    // Zero-worker unassign removes the forage source.
    let applied = alloc.set_assignment(
        LaborTarget::Forage {
            tile: UVec2::new(1, 1),
        },
        0,
        available,
    );
    assert_eq!(applied, 0);
    assert_eq!(alloc.assigned_total(), 2);

    // Normalize down when working-age shrinks below the assigned total.
    alloc.set_assignment(LaborTarget::Warrior, 2, 4);
    assert_eq!(alloc.assigned_total(), 4);
    alloc.normalize(3);
    assert!(
        alloc.assigned_total() <= 3,
        "normalize should trim Σ workers to the new working-age ceiling"
    );

    // Sanity: available_workers floors the fractional working scalar.
    assert_eq!(available_workers(scalar_from_f32(5.9)), 5);
}
