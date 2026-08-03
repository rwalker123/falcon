//! Early-Game Labor slice 3a: per-worker Forage/Hunt yields, the leashed-follow lapse, and the
//! Σ-workers ≤ working-age invariant.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_labor_allocation, available_workers, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_forage, spawn_initial_herds, spawn_initial_world, CommandEventKind,
    CommandEventLog, CultureManager, DiscoveryProgressLedger, FactionId, FactionInventory,
    FaunaConfigHandle, FoodModuleTag, ForageRegistry, GenerationId, GenerationRegistry,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfig,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, Tile, TileRegistry, WellbeingConfigHandle, FOOD,
    NO_IMPROVEMENT_UNDERWAY, TRADE_GOODS,
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
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
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
                last_fertility_factors: Default::default(),
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
    forage_alloc_policy(tile, workers, 0.5)
}

fn forage_alloc_policy(tile: UVec2, workers: u32, policy: f32) -> LaborAllocation {
    LaborAllocation {
        assignments: vec![LaborAssignment {
            target: LaborTarget::Forage {
                tile,
                floor: policy,
                species: None,
            },
            workers,
            improvement: None,
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
    // Seed the patch above Sustain's escapement floor so the gather has standing stock to take.
    let (cap, before) = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(pos).expect("patch on the food tile");
        // **Above Sustain's escapement floor** (`K/2`): at the floor exactly there is nothing
        // standing above it and the gather is honestly `0` (`docs/plan_harvest_floor.md` §1).
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
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
                    floor: 0.5,
                },
                workers: 1,
                improvement: None,
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
                    floor: 0.5,
                },
                workers: 2,
                improvement: None,
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
    // K/2 operating point over the run — a genuine draw-down the herd survives (Deplete would drive it
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
                    floor: 0.5,
                },
                workers: 4,
                improvement: None,
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
                    floor: 0.5,
                },
                workers: 3,
                improvement: None,
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

/// (c') The plant twin of the leash lapse: a Forage assignment whose tile falls outside
/// `band_work_range` is **abandoned**, not parked at `+0.00`. A patch cannot move, so out of range
/// means the band walked away from it — keeping the assignment would book workers on a tile that
/// still renders as worked while paying a correct-but-indistinguishable zero forever. The in-range
/// band in the same run pins that "lapse" has not widened into "always drop".
#[test]
fn forage_lapses_when_the_band_walks_out_of_work_range() {
    let mut app = spawn_world();
    let (patch_pos, patch_tile) = food_tile(&mut app);
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    // A camp at least 5 tiles away on X (> band_work_range 2), clamped into the grid.
    let far_x = if patch_pos.x + 5 < grid.x {
        patch_pos.x + 5
    } else {
        patch_pos.x.saturating_sub(5)
    };
    let far_tile = app
        .world
        .resource::<TileRegistry>()
        .index(far_x, patch_pos.y)
        .expect("far tile resolves");
    let walked_away = spawn_band(&mut app, far_tile, 10, forage_alloc(patch_pos, 3));
    // A second band camped on the patch itself — same system run, same source, still in range.
    let still_there = spawn_band(&mut app, patch_tile, 10, forage_alloc(patch_pos, 3));

    app.world.run_system_once(advance_labor_allocation);

    let abandoned = app
        .world
        .get::<LaborAllocation>(walked_away)
        .expect("the walked-away band keeps its allocation component");
    assert!(
        abandoned.assignments.is_empty(),
        "an out-of-range Forage assignment should lapse and return its workers to the pool"
    );
    assert!(
        abandoned.last_yields.is_empty(),
        "the lapsed assignment's telemetry row must go with it so `last_yields` stays index-aligned"
    );

    let kept = app
        .world
        .get::<LaborAllocation>(still_there)
        .expect("the in-range band keeps its allocation component");
    assert_eq!(
        kept.assignments.len(),
        1,
        "an in-range Forage assignment must be untouched by the out-of-range lapse"
    );

    let told = app.world.resource::<CommandEventLog>().iter().any(|entry| {
        matches!(entry.kind, CommandEventKind::Forage)
            && entry
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("reason=out_of_range"))
    });
    assert!(
        told,
        "abandoning a tile must push a Forage feed entry naming why, not vanish silently"
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
            floor: 0.5,
            species: None,
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
            floor: 0.5,
            species: None,
        },
        0,
        available,
    );
    assert_eq!(applied, 0);
    assert_eq!(alloc.assigned_total(), 2);

    // Normalize down when working-age shrinks below the assigned total.
    alloc.set_assignment(LaborTarget::Warrior, 2, 4);
    assert_eq!(alloc.assigned_total(), 4);
    let dropped = alloc.normalize(3);
    assert!(
        alloc.assigned_total() <= 3,
        "normalize should trim Σ workers to the new working-age ceiling"
    );
    assert!(
        dropped.is_empty(),
        "trimming 4 → 3 only shrinks the tail assignment; nothing was given up"
    );

    // Sanity: available_workers floors the fractional working scalar.
    assert_eq!(available_workers(scalar_from_f32(5.9)), 5);
}

/// The standing crop these fixtures seat a patch at — Thriving, and **above** Sustain's escapement
/// floor (`K/2`), so a gather has stock standing above it to take.
const STOCKED_STANDING_CROP: f32 = 0.8;

/// Run one turn of forage under `policy` on a Thriving (0.8×cap) patch with ample workers, returning
/// the assignment's yield row and the biomass drawn down this turn.
fn run_forage_yield(policy: f32) -> (core_sim::SourceYield, f32) {
    let mut app = spawn_world();
    let (pos, tile) = food_tile(&mut app);
    let before = {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry.patch_mut(pos).expect("patch on the food tile");
        patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
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
    let y = yields[0].clone();
    let after = app
        .world
        .resource::<ForageRegistry>()
        .patch(pos)
        .expect("patch present")
        .biomass;
    (y, before - after)
}

/// **The over-forage ⚠ is a fact about the stance's FLOOR, not about this turn's number.** A gather
/// is an overdraw when it stops below the patch's most productive biomass — which is exactly what
/// `components::floor_overdraws` reads — so a strip trips it and a peak-floor gather never can.
///
/// It is deliberately **not** `actual > sustainable`. Since the harvest floor a take is constant
/// escapement, so the first harvest of a stocked patch is its accumulated stock and legitimately
/// exceeds one turn's regrowth under *every* stance, Sustain included. `sustainable` stays on the
/// row as the MSY reference the player reads beside it, and the draw-down ordering below is what
/// makes the two stances differ in the way the ⚠ claims.
#[test]
fn non_sustain_forage_trips_overdraw_while_sustain_does_not() {
    let (sustain, sustain_drawdown) = run_forage_yield(0.5);
    let (erad, erad_drawdown) = run_forage_yield(0.0);

    assert!(
        !sustain.overdraws,
        "a Sustain gather stops at the MSY point — no ⚠: {sustain:?}"
    );
    assert!(
        erad.overdraws,
        "an Eradicate gather strips past it — the ⚠: {erad:?}"
    );
    assert!(
        sustain.actual > 0.0 && erad.actual > sustain.actual,
        "and the ⚠ is earned: Eradicate really takes more this turn ({} vs {})",
        erad.actual,
        sustain.actual
    );
    assert!(
        erad_drawdown > sustain_drawdown,
        "…and draws the patch down harder: {erad_drawdown} vs {sustain_drawdown}"
    );
}

/// **A DEEPER FLOOR SELLS MORE — because it takes more biomass, and for no other reason.**
///
/// The retired `market.trade_goods_multiplier` paid a `Deplete`-depth draw a 4× trade *bonus*; §4 of
/// `docs/plan_harvest_floor.md` deleted it, so **no option carries a factor of any kind**. What is
/// left is the intensity ladder doing the work: a deeper floor leaves less standing, so it takes more
/// stock, so it sells more.
///
/// Asserted as an **ordering that tracks the drawdown**, not a ratio against a lever: the trade
/// ordering must match the biomass ordering exactly, which is a statement a bonus would break. It
/// reads the **band's own `TRADE_GOODS` store** — the fixed-point account every ongoing harvest
/// credits — so the compared numbers are the unrounded ones the sim really banks.
#[test]
fn a_deeper_floor_sells_more_because_it_takes_more() {
    /// A crew large enough that the escapement ceiling is always the binding term. With a small
    /// crew every floor takes the same amount — the worker cap — and the ordering under test would
    /// be about labour rather than about the floor.
    const CEILING_BOUND_CREW: u32 = 5_000;

    let run = |floor: f32| -> (f32, f32) {
        let mut app = spawn_world();
        let (pos, tile) = food_tile(&mut app);
        let before = {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(pos).expect("patch on the food tile");
            patch.biomass = patch.carrying_capacity * STOCKED_STANDING_CROP;
            patch.biomass
        };
        // **Staffed past any worker cap**, so the FLOOR is the binding term at every depth — the
        // ordering below is about the floor, and a labour-bound gather takes the same amount at
        // every floor by construction.
        let band = spawn_band(
            &mut app,
            tile,
            CEILING_BOUND_CREW,
            forage_alloc_policy(pos, CEILING_BOUND_CREW, floor),
        );
        app.world.run_system_once(advance_labor_allocation);
        // Read the band's own fixed-point store: the point is the proportionality, and the retired
        // integer faction stockpile would have rounded it away on a staple basket.
        let trade = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the foraging band still exists")
            .stores
            .get(TRADE_GOODS)
            .to_f32();
        let after = app
            .world
            .resource::<ForageRegistry>()
            .patch(pos)
            .expect("patch present")
            .biomass;
        (trade, before - after)
    };

    let (deep_trade, deep_take) = run(0.15);
    let (peak_trade, peak_take) = run(0.5);
    let (strip_trade, strip_take) = run(0.0);

    assert!(
        peak_trade > 0.0,
        "every harvest sells its basket's trade component: {peak_trade}"
    );
    assert!(
        strip_trade > deep_trade && deep_trade > peak_trade,
        "a deeper floor sells more: strip {strip_trade} > deep {deep_trade} > peak {peak_trade}"
    );
    // **And in exactly the proportion of the biomass taken.** A per-depth bonus would show up here
    // as a trade ratio that outran the take ratio — which is precisely what the retired 4× markup did.
    let trade_ratio = deep_trade / peak_trade;
    let take_ratio = deep_take / peak_take;
    assert!(
        (trade_ratio - take_ratio).abs() < 1e-3,
        "the trade ordering must track the DRAWDOWN, with no factor of its own: trade ×{trade_ratio} \
         against take ×{take_ratio}"
    );
    assert!(
        strip_take > deep_take && deep_take > peak_take,
        "…and the drawdown itself is ordered: {strip_take} > {deep_take} > {peak_take}"
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
                    floor: 0.5,
                },
                workers,
                improvement: None,
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
        0.5,
        NO_IMPROVEMENT_UNDERWAY,
        horizon,
    );
    let schedule = core_sim::project_arrivals_hunt(
        herd,
        &fauna,
        &ladder,
        per_worker,
        1.0,
        4,
        0.5,
        NO_IMPROVEMENT_UNDERWAY,
        horizon,
    );

    let total: f32 = schedule.iter().sum();
    let smooth = realized.provisions * horizon as f32;
    // One whole animal's provisions: the most that can still be sitting in the bank, undelivered.
    let one_animal = core_sim::herd_hunt_yield(herd, &fauna)
        .apply(herd.body_mass, 1.0)
        .provisions;
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
        0.5,
        NO_IMPROVEMENT_UNDERWAY,
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

/// **A trimmed-away assignment is announced, and the lost build is named.**
///
/// `LaborAllocation::normalize` drops from the tail when a band's working-age head-count shrinks
/// below what it has committed, and it did so in **total silence** — the one place in the labor
/// system that abandoned work without telling the player, against the out-of-range Forage lapse a
/// few tests above which has always pushed a feed entry. The improvement rides the *assignment*, not
/// the source, so a population dip could destroy a 25-turn `Cultivate` commitment with nothing said
/// anywhere; the likely upstream cause of a playtest report of a tended patch that quietly ended up
/// with zero workers.
#[test]
fn a_trimmed_assignment_is_announced_and_names_the_lost_build() {
    let mut app = spawn_world();
    let (patch_pos, patch_tile) = food_tile(&mut app);
    let herd_id = app.world.resource::<HerdRegistry>().herds[0].id.clone();

    // Two assignments, Σ = 6, on a band with only 3 hands: `normalize` drops the tail (the hunt) and
    // trims the forage. The tail carries a build verb, which is the half that cannot come back.
    let mut allocation = forage_alloc(patch_pos, 3);
    allocation.assignments.push(LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: herd_id.clone(),
            floor: 0.5,
        },
        workers: 3,
        improvement: Some(core_sim::Improvement::Tame),
    });
    let band = spawn_band(&mut app, patch_tile, 3, allocation);

    app.world.run_system_once(advance_labor_allocation);

    assert_eq!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("band allocation")
            .assignments
            .len(),
        1,
        "the tail assignment is dropped — this test is meaningless if nothing was trimmed"
    );
    let entry = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .find(|entry| {
            entry
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("reason=too_few_workers"))
        })
        .cloned()
        .expect("trimming an assignment must push a feed entry, not vanish silently");
    let detail = entry.detail.clone().unwrap_or_default();
    assert!(
        detail.contains(&format!("herd={herd_id}")),
        "the entry names the source that was given up: {detail}"
    );
    assert!(
        detail.contains("action=tame") && entry.label.contains("tame"),
        "and names the build that was abandoned — the expensive half: {detail} / {}",
        entry.label
    );
    assert!(
        matches!(entry.kind, CommandEventKind::Hunt),
        "on the source's own feed channel"
    );
}
