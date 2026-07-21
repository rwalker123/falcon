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
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
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
    app.world.insert_resource(LadderConfigHandle::default());
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

/// **The lumpy `actual` pulses; the forward-projected `realized` reads FLAT.** A whole-animal Sustain
/// hunt on a slow breeder (MSY ≪ `body_mass`) pays nothing for several turns then a whole animal at
/// once — so `actual` swings 0 → spike → 0 — while `realized` (the average food/turn projected over the
/// next N turns, rate-based) holds essentially flat at ≈ MSY every turn, never reaching the spike, and
/// averages to the same long-run mean. This is the regression guard for the whole fix: the headline
/// "Food /turn" is a steady number instead of the jumpy `actual`, and it does NOT sawtooth with the
/// biomass (the instantaneous-rate bug this replaced).
#[test]
fn a_hunt_actual_pulses_while_realized_holds_the_steady_average() {
    let mut app = spawn_world();
    // A stationary game herd (route_len 1 → stays put across the run, so the hunt never lapses).
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
    // Force a SLOW-BREEDER profile so the take pulses: MSY = r·K/4 = 10, body_mass = 30 (3× MSY), so a
    // Sustain hunt kills one body every ~3 turns and waits between. Sustain holds the herd near K/2.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.carrying_capacity = 200.0;
        herd.regrowth_rate = 0.2;
        herd.body_mass = 30.0;
        herd.biomass = herd.carrying_capacity * 0.5; // K/2 — Sustain's operating point.
        herd.biomass_before_regrowth = herd.biomass;
        herd.hunt_credit = 0.0;
    }
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
    // 2 hunters × 40 per-worker = 80 biomass throughput > body_mass, so a killed body is carried whole
    // (no waste) and `realized` is never worker-bound — it reads the policy ceiling.
    let band = spawn_band(
        &mut app,
        tile,
        10,
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: id.clone(),
                    policy: FollowPolicy::Sustain,
                },
                workers: 2,
            }],
            ..Default::default()
        },
    );

    // Warm up past the first bank fill, then sample enough turns to contain many pulses.
    const WARMUP: usize = 8;
    const SAMPLES: usize = 60;
    let mut actual = Vec::with_capacity(SAMPLES);
    let mut realized = Vec::with_capacity(SAMPLES);
    for turn in 0..(WARMUP + SAMPLES) {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_labor_allocation);
        if turn >= WARMUP {
            let row = app.world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
            actual.push(row.actual);
            realized.push(row.realized);
        }
    }

    // `actual` PULSES — it is 0 on wait turns and > 0 on kill turns.
    assert!(
        actual.contains(&0.0),
        "a slow-breeder hunt must wait (actual == 0) on some turns: {actual:?}"
    );
    assert!(
        actual.iter().any(|&a| a > 0.0),
        "a slow-breeder hunt must kill (actual > 0) on some turns: {actual:?}"
    );

    let realized_mean: f32 = realized.iter().sum::<f32>() / realized.len() as f32;
    let actual_mean: f32 = actual.iter().sum::<f32>() / actual.len() as f32;
    let actual_max = actual.iter().cloned().fold(0.0_f32, f32::max);
    let realized_max = realized.iter().cloned().fold(0.0_f32, f32::max);
    assert!(
        realized_mean > 0.0,
        "realized must be positive: {realized:?}"
    );

    // The pulse really is spiky — a kill turn spikes well above the steady rate.
    assert!(
        actual_max > 2.0 * realized_mean,
        "a kill turn must spike above the steady rate (max {actual_max}, steady {realized_mean})"
    );

    // `realized` is FLAT — a settled Sustain herd sits above K/2, where the projected policy rate is
    // MSY every simulated turn regardless of the biomass sawtooth, so the headline barely moves. Its
    // turn-to-turn change is a tiny fraction of the steady rate (NOT the sawtooth the instantaneous
    // rate would show), and it never reaches the kill spike.
    let max_delta_realized = realized
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_delta_realized < 0.05 * realized_mean,
        "realized must read flat turn-to-turn (max Δrealized {max_delta_realized}, \
         steady {realized_mean}): {realized:?}"
    );
    assert!(
        realized_max < 0.7 * actual_max,
        "the steady average must never reach the kill spike (realized max {realized_max}, \
         actual max {actual_max})"
    );

    // The long-run mean of the lumpy `actual` ≈ the (flat) `realized` — the projection is unbiased.
    assert!(
        (actual_mean - realized_mean).abs() < 0.15 * realized_mean,
        "the long-run mean of actual ({actual_mean}) must ≈ realized ({realized_mean})"
    );
}

/// **A herd being drawn down (`B > K/2`) reads `realized` that drifts SMOOTHLY, never sawtooths.** Off
/// the stable operating point — a full herd a Sustain hunt is walking down toward `K/2` — the biomass
/// falls turn by turn *and* sawtooths with every whole-animal kill. The forward projection reads
/// through both: it holds at ≈ MSY with only tiny per-turn steps, where the instantaneous
/// `sustainable_yield(current biomass)` would jitter with the kill sawtooth. This is the draw-down half
/// of the fix.
#[test]
fn a_drawn_down_hunt_realized_drifts_smoothly_never_sawtooths() {
    let mut app = spawn_world();
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
    // A big-bodied slow breeder started well ABOVE K/2, so a Sustain hunt walks it *down* toward the
    // K/2 operating point over the run — a genuine draw-down the herd survives (Market would drive it
    // extinct and lapse the assignment, which measures nothing about smoothness).
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.carrying_capacity = 200.0;
        herd.regrowth_rate = 0.2;
        herd.body_mass = 30.0;
        herd.biomass = herd.carrying_capacity * 0.9; // 0.9K — a standing surplus above the K/2 floor.
        herd.biomass_before_regrowth = herd.biomass;
        herd.hunt_credit = 0.0;
    }
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
    let band = spawn_band(
        &mut app,
        tile,
        10,
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: id.clone(),
                    policy: FollowPolicy::Sustain,
                },
                workers: 4,
            }],
            ..Default::default()
        },
    );

    const TURNS: usize = 20;
    let mut realized = Vec::with_capacity(TURNS);
    for _ in 0..TURNS {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_labor_allocation);
        let allocation = app.world.get::<LaborAllocation>(band).unwrap();
        assert!(
            !allocation.last_yields.is_empty(),
            "the hunt must not lapse during a survivable draw-down"
        );
        realized.push(allocation.last_yields[0].realized);
    }

    let realized_mean: f32 = realized.iter().sum::<f32>() / realized.len() as f32;
    assert!(
        realized_mean > 0.0,
        "realized must be positive: {realized:?}"
    );
    // Every turn-to-turn step is small relative to the level — a smooth drift, never a sawtooth jump.
    let max_delta = realized
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        max_delta < 0.2 * realized_mean,
        "a drawn-down realized must drift smoothly, not sawtooth (max Δ {max_delta}, \
         mean {realized_mean}): {realized:?}"
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
    let y = &yields[0];
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

// ---------------------------------------------------------------------------------------------
// The arrival schedule (`SourceYield::arrivals`) — WHEN the food lands, not how much on average.
// ---------------------------------------------------------------------------------------------

/// Pin a short-range herd's ecology so its lumpiness is a property of the fixture, not of whatever
/// species worldgen happened to place, and staff a band hunting it. Returns `(herd_id, band)`.
///
/// `body` relative to the herd's MSY (`r·K/4`) is the whole dial: a body far heavier than one turn's
/// MSY makes the kill-credit bank wait several turns per animal (lumpy); a body lighter than it clears
/// a carcass every turn (continuous).
fn stage_hunt(
    app: &mut App,
    capacity: f32,
    regrowth: f32,
    body: f32,
    biomass: f32,
    workers: u32,
) -> (String, bevy::prelude::Entity) {
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
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        herd.carrying_capacity = capacity;
        herd.regrowth_rate = regrowth;
        herd.body_mass = body;
        herd.biomass = biomass;
        herd.biomass_before_regrowth = biomass;
        // A fresh bank, so the fixture's first arrival is decided by the fixture's own numbers.
        herd.hunt_credit = 0.0;
    }
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
    let band = spawn_band(
        app,
        tile,
        workers.max(1) * 4,
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: id.clone(),
                    policy: FollowPolicy::Sustain,
                },
                workers,
            }],
            ..Default::default()
        },
    );
    (id, band)
}

/// Run one real turn (Logistics regrow → Population take) and hand back the resolved telemetry row.
fn resolve_turn(app: &mut App, band: bevy::prelude::Entity) -> core_sim::SourceYield {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_labor_allocation);
    app.world
        .get::<LaborAllocation>(band)
        .expect("band keeps its allocation")
        .last_yields
        .first()
        .expect("the staffed hunt has a telemetry row")
        .clone()
}

/// **THE test: the schedule is pinned to REAL behaviour, not to another forecast.** A big-game Sustain
/// hunt (body 30 against an MSY of 10 — the bank needs three turns per animal) predicts a genuinely
/// lumpy schedule at turn 0; driving the *real* systems forward must then deliver on exactly the turns
/// the schedule named, in exactly the amounts. If the projection ever drifts from `hunt_take`, this
/// fails — which is the point: a schedule agreeing with a sibling forecast proves nothing.
#[test]
fn the_arrival_schedule_matches_a_real_driven_hunt() {
    let mut app = spawn_world();
    // K 200 at r 0.2 → MSY = r·K/4 = 10 biomass/turn against a 30-unit body: one animal per ~3 turns.
    let (_id, band) = stage_hunt(&mut app, 200.0, 0.2, 30.0, 100.0, 4);

    // Turn 0 resolves and publishes the schedule for the turns that follow it.
    let schedule = resolve_turn(&mut app, band).arrivals;
    let horizon = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .arrivals_horizon_turns as usize;
    assert_eq!(
        schedule.len(),
        horizon,
        "the schedule is exactly `arrivals_horizon_turns` long: {schedule:?}"
    );

    // Now drive the REAL systems forward and record what each turn actually delivered.
    let delivered: Vec<f32> = (0..horizon)
        .map(|_| resolve_turn(&mut app, band).actual)
        .collect();

    // It must be genuinely lumpy — otherwise the test proves nothing about timing.
    assert!(
        delivered.iter().any(|d| *d <= 0.0) && delivered.iter().any(|d| *d > 0.0),
        "the fixture must produce a lumpy hunt (zeros between hauls): {delivered:?}"
    );
    // The `Scalar` grid the larder accumulates on is coarser than the projection's `f32`, so compare
    // on the arrival *turns* exactly and the amounts to within a grid step.
    let predicted_turns: Vec<usize> = schedule
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0.0)
        .map(|(i, _)| i)
        .collect();
    let actual_turns: Vec<usize> = delivered
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0.0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(
        predicted_turns, actual_turns,
        "the schedule must name the turns the sim really delivers on\n  predicted {schedule:?}\n  \
         delivered {delivered:?}"
    );
    for (i, (predicted, actual)) in schedule.iter().zip(delivered.iter()).enumerate() {
        assert!(
            (predicted - actual).abs() < 1e-3,
            "turn {} predicted {predicted} but the sim delivered {actual}\n  predicted {schedule:?}\
             \n  delivered {delivered:?}",
            i + 1
        );
    }
}

/// **A fast/small-game source is CONTINUOUS — every slot positive.** A body lighter than one turn's
/// MSY clears a carcass every turn, so the bank never has to wait and the client draws a solid run.
/// The same code path that produces the mammoth's gaps produces this, with no special case.
#[test]
fn fast_game_arrives_every_turn() {
    let mut app = spawn_world();
    // K 200 at r 0.35 → MSY = 17.5 biomass/turn against a 2-unit body: several rabbits every turn.
    let (_id, band) = stage_hunt(&mut app, 200.0, 0.35, 2.0, 100.0, 4);

    let schedule = resolve_turn(&mut app, band).arrivals;
    assert!(
        schedule.iter().all(|v| *v > 0.0),
        "fast game delivers on every turn — a continuous source, no wait turns: {schedule:?}"
    );
}

/// **The bank moves the TIMING, not the TOTAL.** `realized` deliberately drops the kill-credit bank
/// and the schedule keeps it, so over the same horizon from the same state they must agree:
/// `Σ arrivals ≈ realized × horizon`. The tolerance is the partial body still banked at the end —
/// at most one animal's worth of provisions.
#[test]
fn the_schedule_total_matches_the_realized_average_over_the_horizon() {
    let mut app = spawn_world();
    let (id, _band) = stage_hunt(&mut app, 200.0, 0.2, 30.0, 100.0, 4);

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(&id).expect("the staged herd is live");
    // Both projections from the SAME state over the SAME horizon — the comparison is only meaningful
    // if the only difference is the bank.
    let horizon = labor.arrivals_horizon_turns;
    let per_worker = labor.hunt.per_worker_biomass_capacity;
    let realized = core_sim::project_realized_hunt(
        herd,
        &fauna,
        &ladder,
        per_worker,
        1.0,
        4,
        FollowPolicy::Sustain,
        horizon,
    );
    let schedule = core_sim::project_arrivals_hunt(
        herd,
        &fauna,
        &ladder,
        per_worker,
        1.0,
        4,
        FollowPolicy::Sustain,
        horizon,
    );

    let total: f32 = schedule.iter().sum();
    let smooth = realized * horizon as f32;
    // One whole animal's provisions: the most that can still be sitting in the bank, undelivered.
    let one_animal = core_sim::hunt_provisions(herd.body_mass, &fauna, 1.0);
    assert!(
        (total - smooth).abs() <= one_animal,
        "the schedule's total ({total}) must match the smooth average over the horizon ({smooth}) \
         to within the partial body still banked ({one_animal}): {schedule:?}"
    );
}

/// **A spent source schedules nothing — an all-zero run, and no panic.** Two ways to have nothing to
/// take, both of which the client must be able to render as "this source will feed no one": a herd
/// already at the extinction floor, and a herd whose animals are heavier than anything the stock
/// could ever spare (`affordable < 1` forever — the wait that never ends).
#[test]
fn a_spent_source_schedules_nothing() {
    let mut app = spawn_world();
    // **The floor case is projected directly**, not driven: `advance_herds` *despawns* a herd this
    // far gone and the assignment lapses with it, so there would be no telemetry row left to read.
    // The projection still has to answer for that state without dividing by a dead herd.
    let (id, _band) = stage_hunt(&mut app, 200.0, 0.2, 30.0, 0.0, 4);
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let schedule = core_sim::project_arrivals_hunt(
        app.world.resource::<HerdRegistry>().find(&id).unwrap(),
        &app.world.resource::<FaunaConfigHandle>().get(),
        &app.world.resource::<LadderConfigHandle>().get(),
        labor.hunt.per_worker_biomass_capacity,
        1.0,
        4,
        FollowPolicy::Sustain,
        labor.arrivals_horizon_turns,
    );
    assert_eq!(
        schedule.len(),
        labor.arrivals_horizon_turns as usize,
        "even a dead source reports a full-length, all-zero schedule: {schedule:?}"
    );
    assert!(
        schedule.iter().all(|v| *v == 0.0),
        "a herd at the floor delivers nothing at any point in the horizon: {schedule:?}"
    );

    let mut app = spawn_world();
    // A body 100× the whole standing stock: the bank can never clear one, so the hunt waits forever.
    let (_id, band) = stage_hunt(&mut app, 200.0, 0.2, 20_000.0, 100.0, 4);
    let schedule = resolve_turn(&mut app, band).arrivals;
    assert!(
        schedule.iter().all(|v| *v == 0.0),
        "a herd that can never spare a whole animal delivers nothing: {schedule:?}"
    );
}
