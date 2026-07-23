//! Grazing Phase 2d — **the pen economy convergence gate.** A pen is now a piece of *fenced land*
//! (`docs/plan_grazing_2d.md`): a penned herd's carrying capacity is its fenced footprint's graze flow
//! (`hex_range_tiles(corralled_at, pen_radius)`), it **grazes that footprint** each turn (escapement-
//! floored, exactly like a wild herd), and the grass it eats **offsets its keeper's larder bill**. This
//! test runs the **real** coupled pen systems forward from several start states and asserts:
//!
//! - **(a)** a penned herd converges to a **steady biomass** — at `pen_radius = 0` (one tile) and
//!   `pen_radius = 1` (a 7-tile ring) — from an under- and an over-populated start, settling on the
//!   same fixed point (the harvested pen sits at `K_footprint / 2`), and
//! - **(b)** a penned herd on a **LUSH footprint** drives `pasture_fraction → 1` and its larder feed
//!   bill `→ ~0` (it grazes itself for free), while a penned herd on a **BARREN footprint** pays the
//!   **full** larder bill (`upkeep × biomass`) — the §2.3 thesis, made literal.
//!
//! Deterministic (a pinned map seed, no `Date`/rand), mirroring `grazing_2b_convergence.rs`.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::grid_utils::hex_range_tiles;
use core_sim::{
    advance_graze_regrowth, advance_herd_grazing, advance_herds, advance_husbandry,
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_graze,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FollowPolicy,
    ForageRegistry, GenerationId, GenerationRegistry, GrazePatch, GrazeRegistry, Herd,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment,
    LaborConfigHandle, LaborTarget, LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle,
    MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SizeClass,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FOOD,
    RUNG_COMPLETE,
};

/// A pinned earthlike map (`map_seed` is otherwise entropy — pin it). Only used to stand up a real
/// `TileRegistry` + a seeded `GrazeRegistry`; the pen under test is placed by hand.
const MAP_SEED: u64 = 119304647;
/// Turns per run — well past where the fast pen `r` settles.
const TURNS: u32 = 200;
/// The tail-window whose spread proves convergence.
const SETTLE_WINDOW: usize = 30;
/// The tail band's peak-to-peak span, as a fraction of its mean, must sit under this "small band".
const SMALL_BAND: f32 = 1e-2;
/// A big head-count so tending is never worker-limited (tending is one-worker maintenance anyway).
const KEEPER_WORKERS: u32 = 5000;
/// Re-stocked into the keeper each turn so the feed is always *payable* — this test isolates the
/// pasture offset (how much the footprint covers), not a starvation.
const RESTOCK: f32 = 1_000_000.0;

fn base_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
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
    app.world.insert_resource(GrazeRegistry::default());
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
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_graze);
    app
}

/// The richest pasture tile on the map (a prairie-class patch). Returns `(tile, capacity)`.
///
/// Delegates to `GrazeRegistry::richest_patch`, whose **deterministic tie-break** this test depends on:
/// every tile of the richest biome shares the maximum capacity, so picking the winner off raw `HashMap`
/// order would sample a different neighbourhood (and a different pen footprint) each process.
fn richest_pasture(app: &App) -> (UVec2, f32) {
    app.world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
}

/// Seat a single **penned** herd at `tile` with the given fenced `radius`, wild `r` / metabolic
/// `fodder`, spawn `carrying_capacity` and starting `biomass`. Domesticated (collapse-immune) so it is
/// a managed population, not a wild one. Returns its id.
///
/// **Density-neutral by construction** — the fixture's display name is deliberately *not* a roster
/// species, so its per-species husbandry density gain resolves to the neutral `1.0`
/// ([`fauna_config::DEFAULT_HUSBANDRY_DENSITY`]). These tests validate the pen **economy** (r-driven
/// convergence, the footprint-K, the pasture/larder feed offset), which is **orthogonal** to the
/// per-species density ladder — mixing a real species' density gain (a penned rabbit is `×1.5`) into a
/// single-tile footprint would erode the "lush footprint feeds the pen for free" invariant (§2.3) and
/// the convergence band for reasons that have nothing to do with what these tests measure. The density
/// ladder has its own test (`the_husbandry_density_ladder_scales_carrying_capacity_per_species`).
#[allow(clippy::too_many_arguments)] // every knob of the pen's ecology is a lever under test
fn seat_pen(
    app: &mut App,
    tile: UVec2,
    radius: u32,
    fodder: f32,
    r: f32,
    cap: f32,
    biomass: f32,
    body_mass: f32,
) -> String {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    let mut herd = Herd::new(
        "pen_0".to_string(),
        // A fixture name (NOT a roster species) → neutral density gain; see the doc comment.
        "Fixture Warren".to_string(),
        SizeClass::Small,
        vec![tile],
        biomass,
        cap,
        fodder,
        r,
        body_mass,
    );
    herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
    assert!(herd.corral_at(tile), "the fixture species must be pennable");
    herd.pen_radius = radius;
    registry.herds.push(herd);
    "pen_0".to_string()
}

/// Capacity every tile of a levelled pen footprint carries. Any positive value works — the tests read
/// the **ratio** between a 1-tile and a 7-tile fence, never an absolute — so this is a fixture
/// constant, not a tuning lever.
const LEVELLED_PASTURE_CAPACITY: f32 = 200.0;

/// The widest fence any test in this file seats. The footprint is levelled out to this radius, so
/// every radius in a sweep reads the same per-tile pasture and the comparison between them is a pure
/// tile-count comparison.
const MAX_SWEPT_PEN_RADIUS: u32 = 2;

/// **Level the pen's footprint to a uniform pasture**, so a fence's K is a function of how many tiles
/// it encloses and nothing else.
///
/// The fixture anchors on `richest_patch()`, which is by construction the map's single best tile — its
/// six neighbours are necessarily no richer, and *how much* poorer they are is whatever worldgen
/// happened to put there. So "a 7-tile fence holds ≥1.5× a 1-tile fence" was being decided by the
/// biomes around one generated tile rather than by the pen economy: a worldgen retune moved the
/// neighbourhood and the ratio fell to 1.42× while the mechanic under test was completely unchanged.
/// Levelling makes the enclosed pasture exactly `tiles × LEVELLED_PASTURE_CAPACITY`, so radius 1 is a
/// true 7× of radius 0 and the assertion is earned by the footprint rule instead of by the map.
fn level_footprint_pasture(app: &mut App, center: UVec2, radius: u32) {
    let (width, height, wrap) = {
        let registry = app.world.resource::<TileRegistry>();
        let wrap = app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        (registry.width, registry.height, wrap)
    };
    let footprint = hex_range_tiles(center, radius, width, height, wrap);
    let mut graze = app.world.resource_mut::<GrazeRegistry>();
    for tile in footprint {
        graze
            .patches
            .insert(tile, GrazePatch::new(tile, LEVELLED_PASTURE_CAPACITY));
    }
}

/// A keeper band standing on the pen tile with a single Hunt assignment (= tending the pen). It pays
/// the feed and harvests the pen each turn. Returns its entity.
fn spawn_keeper(app: &mut App, herd_id: &str, tile: UVec2) -> Entity {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("pen tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile_entity,
                current_tile: tile_entity,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(KEEPER_WORKERS as f32),
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
            StartingUnit {
                kind: "BandKeeper".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.to_string(),
                        policy: FollowPolicy::Sustain,
                    },
                    workers: KEEPER_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// One full pen turn in live stage order: Logistics (herds recompute footprint K + grow → herds graze
/// their footprint → graze regrows → husbandry escape/starve pass) then Population (labor: the keeper
/// FEEDs + HARVESTs). The keeper is re-stocked first so the feed is always payable.
fn run_pen_turn(app: &mut App, keeper: Entity) {
    app.world
        .get_mut::<PopulationCohort>(keeper)
        .expect("keeper")
        .stores
        .set(FOOD, scalar_from_f32(RESTOCK));
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
    app.world.run_system_once(advance_husbandry);
    app.world.run_system_once(advance_labor_allocation);
}

fn biomass_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass)
        .unwrap_or(0.0)
}

/// The peak-to-peak span of the last `SETTLE_WINDOW` samples as a fraction of their mean.
fn tail_spread(series: &[f32]) -> f32 {
    let win = &series[series.len() - SETTLE_WINDOW..];
    let (mut lo, mut hi, mut sum) = (f32::INFINITY, f32::NEG_INFINITY, 0.0f32);
    for &v in win {
        lo = lo.min(v);
        hi = hi.max(v);
        sum += v;
    }
    let mean = sum / win.len() as f32;
    if mean.abs() < 1e-6 {
        hi - lo
    } else {
        (hi - lo) / mean
    }
}

/// The fixture species' per-animal body mass — **rabbit-class** (2), matching the `r = 0.35` these
/// fixtures use (the density-neutral `Fixture Warren` carries a rabbit's *metabolism* without its
/// density gain — see `seat_pen`). The pen quantises to whole animals like every other rung (slice 8),
/// so the fixture has to declare a real one: at the pen's `r = min(0.75, 0.35 × 3) = 0.75` on `cap =
/// 300` its MSY is ~56, i.e. ~28 rabbits a turn — the pen never has to wait, the *emergent* steadiness
/// `the_pen_slaughters_whole_animals_every_turn` measures across the roster.
const PEN_BODY_MASS: f32 = 2.0;

/// Run a penned herd (radius `r`, start biomass `start`) to convergence and return its settled biomass.
fn run_pen_to_settle(radius: u32, start: f32, cap: f32, fodder: f32, wild_r: f32) -> f32 {
    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    // The fence's K must depend on how many tiles it encloses, not on the biomes worldgen happened to
    // put around the anchor — see `level_footprint_pasture`. Levelled at the widest radius the sweep
    // uses, so every radius reads the same per-tile pasture.
    level_footprint_pasture(&mut app, tile, MAX_SWEPT_PEN_RADIUS);
    let id = seat_pen(
        &mut app,
        tile,
        radius,
        fodder,
        wild_r,
        cap,
        start,
        PEN_BODY_MASS,
    );
    let keeper = spawn_keeper(&mut app, &id, tile);

    let mut series = Vec::with_capacity(TURNS as usize);
    for _ in 0..TURNS {
        run_pen_turn(&mut app, keeper);
        series.push(biomass_of(&app, &id));
    }
    let settled = *series.last().unwrap();
    let spread = tail_spread(&series);
    assert!(
        spread < SMALL_BAND,
        "radius {radius}, start {start}: a penned herd must settle to a STABLE biomass; tail band \
         {spread:.2e} exceeds {SMALL_BAND:.0e} (settled {settled})"
    );
    assert!(
        settled > 0.0,
        "radius {radius}, start {start}: the pen must not crash to zero (settled {settled})"
    );
    settled
}

#[test]
fn a_penned_herd_converges_at_radius_0_and_1_from_every_start() {
    // Rabbit-class metabolism (fodder 0.10, wild r 0.35 → pen r 0.75). The spawn `cap` is overwritten
    // by the ecological footprint recompute on turn 1, so the starts are deliberately far apart.
    const FODDER: f32 = 0.10;
    const WILD_R: f32 = 0.35;
    const SPAWN_CAP: f32 = 400.0;

    // Every (radius × start) pair must settle to a STABLE biomass (asserted inside `run_pen_to_settle`
    // via the tail-band check) — that is "converges from multiple start states at radius 0 and 1".
    let mut settled = std::collections::HashMap::new();
    for radius in [0u32, 1u32] {
        let under = run_pen_to_settle(radius, 20.0, SPAWN_CAP, FODDER, WILD_R);
        let over = run_pen_to_settle(radius, 4000.0, SPAWN_CAP, FODDER, WILD_R);
        println!("radius {radius}: under -> {under:.1}, over -> {over:.1}");
        settled.insert((radius, "under"), under);
        settled.insert((radius, "over"), over);
    }

    // On the CLEAN single-tile footprint (radius 0) the under- and over-populated pens reach the SAME
    // fixed point — the harvested pen sits at K_footprint/2, set by the fenced land, not by history.
    let (r0_under, r0_over) = (settled[&(0, "under")], settled[&(0, "over")]);
    assert!(
        (r0_under - r0_over).abs() <= r0_over.max(1.0) * 2e-2,
        "radius 0: under- and over-populated pens converge to the same K/2 \
         (under {r0_under}, over {r0_over})"
    );
    // (A radius-1 footprint mixes 7 heterogeneous tiles whose escapement floors admit a small
    // start-dependent hysteresis band; each start still settles STABLY, which is what convergence
    // requires — the same-fixed-point identity is asserted only on the clean single-tile system.)

    // A wider fence feeds more animals: radius 1 (7 tiles around the rich anchor) holds a strictly
    // larger herd than radius 0 (1 tile).
    assert!(
        settled[&(1, "under")] > r0_under * 1.5,
        "a radius-1 fence (7 tiles) holds a larger herd than radius-0 (1 tile): {} vs {r0_under}",
        settled[&(1, "under")]
    );
}

/// Read the keeper's per-turn pen feed bill (the food it actually paid) + the herd's pasture fraction.
fn pen_feed_and_pasture(app: &App, keeper: Entity, id: &str) -> (f32, f32) {
    let feed = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("keeper")
        .last_pen_feed_upkeep;
    let pasture = app
        .world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.pen_pasture_fraction)
        .unwrap_or(0.0);
    (feed, pasture)
}

#[test]
fn a_lush_pen_feeds_itself_for_free_while_a_barren_pen_pays_the_full_bill() {
    const FODDER: f32 = 0.10;
    const WILD_R: f32 = 0.35;
    const SETTLE_TURNS: u32 = 120;

    // --- LUSH footprint: the richest pasture tile. The pen grazes its own land; the larder barely
    // pays. ---
    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    let id = seat_pen(
        &mut app,
        tile,
        0,
        FODDER,
        WILD_R,
        300.0,
        150.0,
        PEN_BODY_MASS,
    );
    let keeper = spawn_keeper(&mut app, &id, tile);
    for _ in 0..SETTLE_TURNS {
        run_pen_turn(&mut app, keeper);
    }
    let (lush_feed, lush_pasture) = pen_feed_and_pasture(&app, keeper, &id);
    let lush_biomass = biomass_of(&app, &id);
    println!("LUSH: pasture_fraction {lush_pasture:.3}, larder feed/turn {lush_feed:.4}");
    assert!(
        lush_pasture > 0.98,
        "a lush footprint feeds the pen for free: pasture_fraction {lush_pasture} should be ~1"
    );
    // The larder bill is a rounding whisper next to what a fully-larder-fed pen of this size costs.
    let full_bill = 0.002 * lush_biomass; // pen.upkeep_per_biomass × biomass
    assert!(
        lush_feed < full_bill * 0.02,
        "a lush pen's larder bill → ~0: paid {lush_feed}/turn vs a full bill of {full_bill}"
    );

    // --- BARREN footprint: strip the graze patch under the pen (radius 0 → the footprint is exactly
    // this tile). A wholly-barren footprint keeps the herd's frozen K and is fully larder-fed — §2.3's
    // preserved worst case. ---
    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    let id = seat_pen(
        &mut app,
        tile,
        0,
        FODDER,
        WILD_R,
        300.0,
        150.0,
        PEN_BODY_MASS,
    );
    app.world
        .resource_mut::<GrazeRegistry>()
        .patches
        .remove(&tile);
    let keeper = spawn_keeper(&mut app, &id, tile);
    // Settle, then run ONE instrumented final turn so we can read the FEED-time biomass (post-regrow,
    // pre-harvest) — the biomass the feed is actually charged on — and compare the bill to it exactly.
    for _ in 0..SETTLE_TURNS - 1 {
        run_pen_turn(&mut app, keeper);
    }
    app.world
        .get_mut::<PopulationCohort>(keeper)
        .unwrap()
        .stores
        .set(FOOD, scalar_from_f32(RESTOCK));
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
    app.world.run_system_once(advance_husbandry);
    let feed_time_biomass = biomass_of(&app, &id); // post-regrow, pre-harvest = what FEED charges on
    app.world.run_system_once(advance_labor_allocation);
    let (barren_feed, barren_pasture) = pen_feed_and_pasture(&app, keeper, &id);
    println!("BARREN: pasture_fraction {barren_pasture:.3}, larder feed/turn {barren_feed:.4}");
    assert!(
        barren_pasture.abs() < 1e-6,
        "a barren footprint covers nothing: pasture_fraction {barren_pasture} should be 0"
    );
    // The keeper pays the FULL bill: upkeep_per_biomass × biomass (charged on the pre-harvest biomass).
    let expected = 0.002 * feed_time_biomass;
    assert!(
        barren_feed > 0.0 && (barren_feed - expected).abs() < expected * 0.02,
        "a barren pen pays the full larder bill: paid {barren_feed}/turn vs expected {expected} \
         (upkeep × feed-time biomass {feed_time_biomass})"
    );
}

/// Read a herd's `(pen_radius, pen_extending, carrying_capacity)`.
fn pen_state(app: &App, id: &str) -> (u32, bool, f32) {
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("herd persists");
    (herd.pen_radius, herd.pen_extending, herd.carrying_capacity)
}

/// Put the penned herd into the ExtendPen "extending" state (the sim half of `handle_extend_pen`).
fn begin_extension(app: &mut App, id: &str, radius_max: u32) -> bool {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("herd persists")
        .begin_pen_extension(radius_max)
}

#[test]
fn extend_pen_accrues_a_ring_flips_the_radius_raises_k_and_caps_at_max() {
    const FODDER: f32 = 0.10;
    const WILD_R: f32 = 0.35;
    // corral_build_progress_per_turn = 0.04 → 25 turns per ring; give a couple turns of slack.
    const RING_TURNS: u32 = 28;

    let radius_max = FaunaConfigHandle::default().get().husbandry.pen_radius_max;
    assert!(
        radius_max >= 2,
        "this test wants at least two rings to grow"
    );

    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    // Same reason as the convergence sweep: the ring must raise K because it encloses more tiles, not
    // because worldgen put good ground next door. See `level_footprint_pasture`.
    level_footprint_pasture(&mut app, tile, MAX_SWEPT_PEN_RADIUS);
    // Seat a radius-0 pen at equilibrium-ish so K is stable before the extension.
    let id = seat_pen(
        &mut app,
        tile,
        0,
        FODDER,
        WILD_R,
        300.0,
        150.0,
        PEN_BODY_MASS,
    );
    let keeper = spawn_keeper(&mut app, &id, tile);
    for _ in 0..60 {
        run_pen_turn(&mut app, keeper);
    }
    let (r0, extending0, k0) = pen_state(&app, &id);
    assert_eq!(
        (r0, extending0),
        (0, false),
        "starts a settled radius-0 pen"
    );

    // --- Ring 1: begin extending, then work it off. ---
    assert!(
        begin_extension(&mut app, &id, radius_max),
        "a built radius-0 pen below the max may begin an extension"
    );
    // A second begin while one is in flight is a no-op (mirrors the command's rejection).
    assert!(
        !begin_extension(&mut app, &id, radius_max),
        "no second extension may start while one is in flight"
    );

    let mut flipped_on = None;
    for turn in 1..=RING_TURNS {
        run_pen_turn(&mut app, keeper);
        if pen_state(&app, &id).0 == 1 {
            flipped_on = Some(turn);
            break;
        }
    }
    let flipped_on = flipped_on.expect("the ring completes within its build window");
    assert!(
        (24..=RING_TURNS).contains(&flipped_on),
        "the ring takes ~25 turns at the corral build rate (flipped on turn {flipped_on})"
    );
    let (r1, extending1, _) = pen_state(&app, &id);
    assert_eq!(
        (r1, extending1),
        (1, false),
        "on completion pen_radius is 1 and the extending state clears"
    );

    // Let the larger footprint's K settle, then confirm it ROSE (7 tiles of pasture > 1 tile).
    for _ in 0..40 {
        run_pen_turn(&mut app, keeper);
    }
    let (_, _, k1) = pen_state(&app, &id);
    assert!(
        k1 > k0 * 1.5,
        "the extended (7-tile) footprint raises K well above the single-tile pen: {k1} vs {k0}"
    );

    // --- Ring 2 → reach the max, then REFUSE to go past it. ---
    assert!(begin_extension(&mut app, &id, radius_max));
    for _ in 0..RING_TURNS {
        run_pen_turn(&mut app, keeper);
        if pen_state(&app, &id).0 == 2 {
            break;
        }
    }
    assert_eq!(
        pen_state(&app, &id).0,
        2,
        "the second ring reaches radius 2"
    );
    // At the max, a further extension is refused (the command's `at_max` rejection, sim-side).
    assert!(
        !begin_extension(&mut app, &id, radius_max),
        "a pen at pen_radius_max ({radius_max}) refuses to extend further"
    );
}

/// **The husbandry DENSITY ladder** — the per-species K multiplier: domestication makes the *land*
/// hold more animals, non-linearly by species. On the **same pasture tile** a wild herd's carrying
/// capacity is unchanged (`×1.0`), a **mobile-tamed** (pastoral) herd's is `base × pastoral_density`,
/// and a **corralled** herd's footprint K is `base × pen_density`. A species with the **default**
/// (neutral) dials is byte-identical at every rung — so this is orthogonal to the r-gains (which scale
/// the breeding *rate*, not the ceiling — measured in
/// `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`).
///
/// All three rungs read the **same single-tile footprint** at `tile` (a `Small` herd's roam radius is
/// 0, a `pen_radius = 0` pen is one tile), and `advance_herd_grazing` is **not** run, so the graze is at
/// capacity for every probe — the base K is identical and the ratio isolates the density gain.
#[test]
fn the_husbandry_density_ladder_scales_carrying_capacity_per_species() {
    #[derive(Clone, Copy)]
    enum Rung {
        Wild,
        Pastoral,
        Pen,
    }

    // The range-derived K a herd of `species` settles on at `tile` in the given rung, after one
    // `advance_herds` (which is the one seam that writes `carrying_capacity`).
    fn k_for(app: &mut App, tile: UVec2, species: &str, rung: Rung) -> f32 {
        let mut herd = Herd::new(
            "k_probe".to_string(),
            species.to_string(),
            SizeClass::Small,
            vec![tile],
            100.0, // biomass
            100.0, // spawn cap (overwritten by the ecological recompute)
            0.10,  // fodder_per_biomass — same for every probe, so the base is shared
            0.20,  // wild r
            20.0,  // body_mass
        );
        match rung {
            Rung::Wild => {}
            Rung::Pastoral => {
                herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
            }
            Rung::Pen => {
                herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
                assert!(
                    herd.corral_at(tile),
                    "the fixture herd defaults to the full ladder"
                );
            }
        }
        {
            let mut reg = app.world.resource_mut::<HerdRegistry>();
            reg.herds.clear();
            reg.herds.push(herd);
        }
        app.world.run_system_once(advance_herds);
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find("k_probe")
            .expect("the probe herd survives");
        assert_eq!(
            herd.position(),
            tile,
            "a single-anchor Small herd stays on the probe tile, so the base footprint is shared"
        );
        herd.carrying_capacity
    }

    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    // Leave graze on ONLY the probe tile: every neighbour is barren, so a mobile (wild/pastoral) herd
    // is hemmed in and stays put, and the single-tile footprint K is shared across all three rungs — so
    // the ratio isolates the density gain from any incidental roam.
    {
        let mut graze = app.world.resource_mut::<GrazeRegistry>();
        graze.patches.retain(|&t, _| t == tile);
    }

    // --- Crag Goats: the prime grazer domesticate, dials 2.0 / 5.0. ---
    let goat_wild = k_for(&mut app, tile, "Crag Goats", Rung::Wild);
    let goat_pastoral = k_for(&mut app, tile, "Crag Goats", Rung::Pastoral);
    let goat_pen = k_for(&mut app, tile, "Crag Goats", Rung::Pen);
    assert!(
        goat_wild > 0.0,
        "a wild goat has a positive range-derived K"
    );
    let eps = goat_wild * 1e-3;
    assert!(
        (goat_pastoral - goat_wild * 2.0).abs() < eps,
        "a tamed goat's K = base × pastoral_density (2.0): base {goat_wild} → {goat_pastoral}"
    );
    assert!(
        (goat_pen - goat_wild * 5.0).abs() < eps,
        "a penned goat's K = base × pen_density (5.0): base {goat_wild} → {goat_pen}"
    );

    // --- Red Deer: a `wild`-ceiling species that omits the dials → neutral 1.0 at every rung. ---
    let deer_wild = k_for(&mut app, tile, "Red Deer", Rung::Wild);
    let deer_pastoral = k_for(&mut app, tile, "Red Deer", Rung::Pastoral);
    let deer_pen = k_for(&mut app, tile, "Red Deer", Rung::Pen);
    let deer_eps = deer_wild * 1e-3;
    assert!(
        (deer_pastoral - deer_wild).abs() < deer_eps && (deer_pen - deer_wild).abs() < deer_eps,
        "a default-dial species is byte-identical up the ladder: wild {deer_wild}, \
         pastoral {deer_pastoral}, pen {deer_pen}"
    );
    // The goat's wild K matches the deer's (same tile, same fodder) — the ladder diverges only above wild.
    assert!(
        (goat_wild - deer_wild).abs() < eps,
        "the two species share the same wild base on the same tile ({goat_wild} vs {deer_wild})"
    );
}
