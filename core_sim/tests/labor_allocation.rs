//! Early-Game Labor slice 3a: per-worker Forage/Hunt yields, the leashed-follow lapse, and the
//! Σ-workers ≤ working-age invariant.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_labor_allocation, available_workers, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_herds, spawn_initial_world, CommandEventLog,
    CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle,
    FogRevealLedger, FollowPolicy, FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig,
    LaborConfigHandle, LaborTarget, LocalStore, MapPresets, MapPresetsHandle, MoraleCause,
    PopulationCohort, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, Tile, TileRegistry, WellbeingConfigHandle, FOOD,
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
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    // Seed depletable forage patches on every food-module tile (§0-ii).
    app.world.run_system_once(spawn_initial_forage);
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
                last_food_consumption: 0.0,
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
    forage_alloc_policy(tile, workers, FollowPolicy::Sustain)
}

fn forage_alloc_policy(tile: UVec2, workers: u32, policy: FollowPolicy) -> LaborAllocation {
    LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Forage { tile, policy },
            workers,
        }],
        ..Default::default()
    }
}

/// Find a food-module tile that actually carries a **patch**: its position + entity. A food-module
/// tile on a biome with no human-edible stock at all (`forage.capacity_by_biome` = 0 — a glacier, a
/// salt pan) is deliberately seeded no patch, so "has a `FoodModuleTag`" is no longer the same
/// question as "is a forage source".
fn food_tile(app: &mut App) -> (UVec2, bevy::prelude::Entity) {
    let pos = {
        let seeded: Vec<UVec2> = app
            .world
            .resource::<ForageRegistry>()
            .patches
            .keys()
            .copied()
            .collect();
        let mut q = app.world.query::<(&FoodModuleTag, &Tile)>();
        q.iter(&app.world)
            .map(|(_, tile)| tile.position)
            .find(|pos| seeded.contains(pos))
            .expect("expected at least one food-module tile carrying a forage patch")
    };
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("food tile resolves");
    (pos, entity)
}

/// The **shipped** labor config with a few levers bent for a scenario. The per-biome forage capacity
/// table (`forage.capacity_by_biome`) is validated as *total* over the 37 biomes, so a test can no
/// longer hand-write a partial `{"forage": {...}}` JSON — it starts from the builtin and overrides.
fn tuned_labor_config(mutate: impl FnOnce(&mut LaborConfig)) -> Arc<LaborConfig> {
    let mut config = (*LaborConfig::builtin()).clone();
    mutate(&mut config);
    Arc::new(config)
}

fn larder(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

/// (a) Forage now draws a **depletable** patch down (§0-ii): a Sustain gather on a below-cap patch
/// yields the regrowth skim (> 0) and reduces the patch's biomass.
#[test]
fn forage_draws_down_depletable_patch() {
    let mut app = spawn_world();
    let (pos, tile) = food_tile(&mut app);
    // Seed the patch below its cap so a Sustain gather skims positive regrowth (a full patch's
    // net regrowth is 0 → no yield, by design).
    let (cap, before) = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(pos).expect("patch on the food tile");
        patch.biomass = patch.carrying_capacity * 0.5;
        (patch.carrying_capacity, patch.biomass)
    };
    let band = spawn_band(&mut app, tile, 10, forage_alloc(pos, 5));

    app.world.run_system_once(advance_labor_allocation);

    let food = larder(&app, band);
    assert!(
        food > 0.0,
        "a Sustain gather yields the regrowth skim, got {food}"
    );
    let after = app
        .world
        .resource::<ForageRegistry>()
        .patch(pos)
        .expect("patch present")
        .biomass;
    assert!(
        after < before,
        "forage must draw the patch down: {before} -> {after}"
    );
    assert!(
        (0.0..=cap).contains(&after),
        "biomass stays in [0, cap]: {after}"
    );
}

/// (b) A Sustain hunt whose per-worker cap is below the herd's regrowth leaves the herd growing.
#[test]
fn sustain_hunt_below_regrowth_lets_herd_grow() {
    let mut app = spawn_world();
    // Tiny per-worker biomass cap so `worker_cap < net regrowth` at any sane worker count.
    app.world
        .insert_resource(LaborConfigHandle::new(tuned_labor_config(|config| {
            config.hunt.per_worker_biomass_capacity = 0.05;
        })));

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
            ..Default::default()
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
            ..Default::default()
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
            policy: FollowPolicy::Sustain,
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
            policy: FollowPolicy::Sustain,
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

/// Run one turn of forage under `policy` on a Thriving (0.8×cap) patch with ample workers, returning
/// the assignment's `(actual, sustainable)` food yield and the biomass drawn down this turn.
fn run_forage_yield(policy: FollowPolicy) -> (f32, f32, f32) {
    let mut app = spawn_world();
    let (pos, tile) = food_tile(&mut app);
    let before = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(pos).expect("patch on the food tile");
        patch.biomass = patch.carrying_capacity * 0.8; // Thriving, positive net regrowth.
        patch.biomass
    };
    let band = spawn_band(&mut app, tile, 10, forage_alloc_policy(pos, 10, policy));
    app.world.run_system_once(advance_labor_allocation);
    let yields = app
        .world
        .get::<LaborAllocation>(band)
        .expect("band allocation")
        .last_yields
        .clone();
    let y = yields[0];
    let after = app
        .world
        .resource::<ForageRegistry>()
        .patch(pos)
        .expect("patch present")
        .biomass;
    (y.actual, y.sustainable, before - after)
}

/// (§0-iii over-forage): a non-Sustain gather makes `actual > sustainable` (the client overdraw ⚠
/// trips) while a Sustain gather keeps `actual ≈ sustainable` (the regrowth skim, no overdraw).
#[test]
fn non_sustain_forage_trips_overdraw_while_sustain_does_not() {
    let (sustain_actual, sustain_sustainable, _) = run_forage_yield(FollowPolicy::Sustain);
    assert!(
        (sustain_actual - sustain_sustainable).abs() < 1e-4,
        "Sustain reads actual ≈ sustainable: {sustain_actual} vs {sustain_sustainable}"
    );

    let (erad_actual, erad_sustainable, _) = run_forage_yield(FollowPolicy::Eradicate);
    assert!(
        erad_actual > erad_sustainable + 1e-4,
        "Eradicate overdraws (actual > sustainable): {erad_actual} vs {erad_sustainable}"
    );
}

/// (§0-iii Market): a Market gather sells the take as trade goods (→ `FactionInventory`) and strips
/// the patch harder than the Sustain skim; Sustain/Eradicate generate no trade goods.
#[test]
fn market_forage_sells_trade_goods_others_do_not() {
    // Bump the trade-goods rate so a single Market gather on a small patch clears integer rounding.
    let run = |policy: FollowPolicy| -> (i64, f32) {
        let mut app = spawn_world();
        app.world
            .insert_resource(LaborConfigHandle::new(tuned_labor_config(|config| {
                config.forage.market.trade_goods_per_biomass = 1.0;
                config.hunt.per_worker_biomass_capacity = 40.0;
            })));
        let (pos, tile) = food_tile(&mut app);
        let before = {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(pos).expect("patch on the food tile");
            patch.biomass = patch.carrying_capacity * 0.8;
            patch.biomass
        };
        spawn_band(&mut app, tile, 10, forage_alloc_policy(pos, 10, policy));
        app.world.run_system_once(advance_labor_allocation);
        let trade = app
            .world
            .resource::<FactionInventory>()
            .stockpile(FactionId(0))
            .and_then(|s| s.get("trade_goods").copied())
            .unwrap_or(0);
        let after = app
            .world
            .resource::<ForageRegistry>()
            .patch(pos)
            .expect("patch present")
            .biomass;
        (trade, before - after)
    };

    let (market_trade, market_take) = run(FollowPolicy::Market);
    let (sustain_trade, sustain_take) = run(FollowPolicy::Sustain);
    let (erad_trade, _) = run(FollowPolicy::Eradicate);

    assert!(
        market_trade > 0,
        "Market forage sells gathered goods as trade goods: {market_trade}"
    );
    assert_eq!(sustain_trade, 0, "Sustain generates no trade goods");
    assert_eq!(erad_trade, 0, "Eradicate is denial, not commerce");
    assert!(
        market_take > sustain_take,
        "Market depletes the patch faster than the Sustain skim: {market_take} vs {sustain_take}"
    );
}
