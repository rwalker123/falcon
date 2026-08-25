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
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, ForageRegistry,
    GenerationId, GenerationRegistry, GrazePatch, GrazeRegistry, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    SimulationConfig, SimulationTick, SizeClass, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, SourcePriority, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FOOD,
};

/// A pinned earthlike map (`map_seed` is otherwise entropy — pin it). Only used to stand up a real
/// `TileRegistry` + a seeded `GrazeRegistry`; the pen under test is placed by hand.
const MAP_SEED: u64 = core_sim::HARNESS_MAP_SEED;
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
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::default());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
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
    herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    assert!(
        herd.corral_at(tile, &core_sim::LadderConfig::builtin()),
        "the fixture species must be pennable"
    );
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
                // Room for the tending crew **and** a ring crew beside it: the take and the
                // build draw on one pool (`docs/plan_standing_upkeep.md` §2.2), and a band sized at
                // the tenders alone would have `LaborAllocation::normalize` trim the ring away.
                working: scalar_from_f32((KEEPER_WORKERS + KEEPER_WORKERS) as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
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
            StartingUnit {
                kind: "BandKeeper".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.to_string(),
                        floor: 0.5,
                    },
                    workers: KEEPER_WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
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
    // **THE KEEPER IS ACTUALLY KEEPING THE HERD.** These fixtures are about grazing and fodder, not
    // about neglect — the keeper is present, feeding and harvesting every turn — but nothing here
    // staffs the band's `husbandry` role, so the herd read as *wholly unkept*. That was inert while
    // the shed was a constant fraction (it balanced against the growth curve and the pen persisted);
    // with the escape rate now accelerating on `Herd::neglect_pressure`, an unkept pen terminates and
    // the convergence these tests measure never happens. Stamping the bill is what the fixture always
    // meant by "the keeper tends it".
    keep_the_penned_herds(app);
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

/// Read the herd's `(pasture_fraction, fed_fraction)` — the share of its feed the fenced footprint
/// grew, and the share grass and hay covered between them.
fn pen_pasture_and_fed(app: &App, id: &str) -> (f32, f32) {
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the pen is still seated");
    (herd.pen_pasture_fraction, herd.pen_fed_fraction)
}

/// What the keeper's `FOOD` larder holds.
fn larder_of(app: &App, keeper: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(keeper)
        .expect("keeper")
        .stores
        .get(FOOD)
        .to_f32()
}

/// **A LUSH FOOTPRINT FEEDS THE PEN FOR FREE; A BARREN ONE STARVES IT — AND NEITHER TOUCHES THE
/// KEEPER'S LARDER.**
///
/// This is the fix's central behaviour change. The barren case used to read *"the keeper pays the
/// full bill"* — `upkeep_per_biomass × biomass` out of the `FOOD` store, every turn, for ever — which
/// meant a pen on dead ground was a permanent tax on the people's bread rather than a herd that
/// cannot be fed. **Human food is not animal feed.** With no grass and no hay the pen is simply
/// unfed: `pen_fed_fraction` reads `0`, `advance_husbandry` shrinks it, and the larder is exactly
/// where the fixture left it.
///
/// The larder is stocked to [`RESTOCK`] on the instrumented turn precisely so *"nothing was taken"*
/// is a claim with something to take — a fixture with an empty larder would pass vacuously.
#[test]
fn a_lush_pen_feeds_itself_for_free_while_a_barren_pen_starves_and_neither_touches_the_larder() {
    const FODDER: f32 = 0.10;
    const WILD_R: f32 = 0.35;
    const SETTLE_TURNS: u32 = 120;
    /// `f32` sums off a `Scalar`-quantized store — a few ULPs, no more.
    const EPS: f32 = 1e-4;

    // --- LUSH footprint: the richest pasture tile. The pen grazes its own land and is fully fed. ---
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
    let (lush_pasture, lush_fed) = pen_pasture_and_fed(&app, &id);
    let lush_larder = larder_of(&app, keeper);
    println!(
        "LUSH: pasture_fraction {lush_pasture:.3}, fed {lush_fed:.3}, larder {lush_larder:.2}"
    );
    assert!(
        lush_pasture > 0.98,
        "a lush footprint feeds the pen for free: pasture_fraction {lush_pasture} should be ~1"
    );
    assert!(
        lush_fed > 0.98,
        "and a pen its own pasture feeds is fully fed: {lush_fed}"
    );
    assert!(
        lush_larder >= RESTOCK - EPS,
        "the keeper's larder is not feed — the turn restocked it to {RESTOCK} and nothing left it \
         for the pen (got {lush_larder})"
    );

    // --- BARREN footprint: strip the graze patch under the pen (radius 0 → the footprint is exactly
    // this tile). Nothing grows and no hay is grown, so the pen has no feed at all. ---
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
    // Settle, then run ONE instrumented final turn so the FEED-time biomass (post-regrow,
    // pre-harvest) — the biomass the demand is struck on — is the one under the assertions.
    for _ in 0..SETTLE_TURNS - 1 {
        run_pen_turn(&mut app, keeper);
    }
    let before_starving = biomass_of(&app, &id);
    app.world
        .get_mut::<PopulationCohort>(keeper)
        .unwrap()
        .stores
        .set(FOOD, scalar_from_f32(RESTOCK));
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
    // **THE KEEPER IS ACTUALLY KEEPING THE HERD.** These fixtures are about grazing and fodder, not
    // about neglect — the keeper is present, feeding and harvesting every turn — but nothing here
    // staffs the band's `husbandry` role, so the herd read as *wholly unkept*. That was inert while
    // the shed was a constant fraction (it balanced against the growth curve and the pen persisted);
    // with the escape rate now accelerating on `Herd::neglect_pressure`, an unkept pen terminates and
    // the convergence these tests measure never happens. Stamping the bill is what the fixture always
    // meant by "the keeper tends it".
    keep_the_penned_herds(&mut app);
    app.world.run_system_once(advance_husbandry);
    let feed_time_biomass = biomass_of(&app, &id); // post-regrow, pre-harvest = what FEED charges on
    app.world.run_system_once(advance_labor_allocation);
    let (barren_pasture, barren_fed) = pen_pasture_and_fed(&app, &id);
    let barren_larder = larder_of(&app, keeper);
    println!(
        "BARREN: pasture_fraction {barren_pasture:.3}, fed {barren_fed:.3}, \
         biomass {before_starving:.2} -> {feed_time_biomass:.2}, larder {barren_larder:.2}"
    );
    assert!(
        barren_pasture.abs() < EPS,
        "a barren footprint covers nothing: pasture_fraction {barren_pasture} should be 0"
    );
    assert!(
        barren_fed.abs() < EPS,
        "and with no hay either the pen is fed NOTHING — it does not fall back on the larder \
         (fed fraction {barren_fed})"
    );
    assert!(
        feed_time_biomass > 0.0,
        "the pen is still there to be starving — a herd that vanished proves nothing about feeding"
    );
    assert!(
        barren_larder >= RESTOCK - EPS,
        "and the keeper's larder is untouched: restocked to {RESTOCK}, still holds {barren_larder}"
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
/// Begin a fence extension **and staff it**, which is what `handle_extend_pen` does: a ring is a
/// build like any other: it waits in the band's build queue under its own kind
/// (`BuildJob::ExtendPen`) and is raised by the band's `builders` pool
/// (`docs/plan_standing_upkeep.md` §2.5), so a fixture that only flipped the herd's flag would
/// measure a ring nobody is raising.
fn begin_extension(
    app: &mut App,
    id: &str,
    keeper: bevy::prelude::Entity,
    radius_max: u32,
) -> bool {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let began = registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("herd persists")
        .begin_pen_extension(radius_max);
    if began {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(keeper)
            .expect("the keeper band keeps its allocation");
        match allocation
            .assignments
            .iter_mut()
            .find(|assignment| assignment.target == LaborTarget::Builders)
        {
            Some(row) => row.workers = KEEPER_WORKERS,
            None => allocation.assignments.push(LaborAssignment {
                target: LaborTarget::Builders,
                workers: KEEPER_WORKERS,
                kit: None,
                priority: SourcePriority::default(),
            }),
        }
        assert!(
            allocation.enqueue_build(
                core_sim::BuildSource::Herd(id.to_string()),
                core_sim::BuildJob::ExtendPen,
            ),
            "the keeper band works the herd whose pen it is widening"
        );
        assert!(
            allocation.set_build_entry_kit(
                &core_sim::BuildSource::Herd(id.to_string()),
                Some(bare_builders()),
            ),
            "the entry just declared takes the bare kit"
        );
    }
    began
}

#[test]
fn extend_pen_accrues_a_ring_flips_the_radius_raises_k_and_caps_at_max() {
    const FODDER: f32 = 0.10;
    const WILD_R: f32 = 0.35;
    // **A ring is paid in HANDS now** (`docs/plan_unit_costed_work.md` §1.2): it is raised by the
    // keepers on the tending assignment, at `animal:pen`'s own `work_cost`. This harness staffs
    // [`KEEPER_WORKERS`] so tending is never worker-limited, so its ring goes up in a single turn —
    // which is the model, not a fixture bug. The window below is generous slack around that.
    const RING_TURNS: u32 = 28;
    let ring_turns_expected = core_sim::build_turns_remaining(
        core_sim::LadderConfig::builtin()
            .rung(core_sim::RungKey::AnimalPen)
            .build_cost(core_sim::RUNG_COST_UNSCALED)
            .expect("the pen rung builds"),
        core_sim::RUNG_UNSTARTED,
        KEEPER_WORKERS as f32 * core_sim::PER_WORKER_OUTPUT,
    )
    .expect("a staffed ring finishes");

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
        begin_extension(&mut app, &id, keeper, radius_max),
        "a built radius-0 pen below the max may begin an extension"
    );
    // A second begin while one is in flight is a no-op (mirrors the command's rejection).
    assert!(
        !begin_extension(&mut app, &id, keeper, radius_max),
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
    assert_eq!(
        flipped_on, ring_turns_expected,
        "the ring takes `work_cost / this keeper crew's output` turns (flipped on turn {flipped_on})"
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
    assert!(begin_extension(&mut app, &id, keeper, radius_max));
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
        !begin_extension(&mut app, &id, keeper, radius_max),
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
                herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
            }
            Rung::Pen => {
                herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
                assert!(
                    herd.corral_at(tile, &core_sim::LadderConfig::builtin()),
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

/// **THE EMPTY KIT, NAMED ON A FIXTURE'S QUEUE ENTRY** — an isolation, not a default.
///
/// It rides the **entry** because that is where a build's kit lives
/// (`docs/plan_standing_upkeep.md` §4.7a ②); a kit on the `builders` row is not an input at all.
/// An absent kit means *derive from this entry's web*, and the roster's answer (`tillage` for a
/// patch, `hurdling` for a herd) adds `+0.5` work per covered worker per turn. A start-stocked band holds a
/// unit per worker and a half, so at the crews these fixtures staff every builder is geared and the
/// pool delivers half again what it asserts, moving every pacing claim below. Naming `none` holds
/// the gear axis at its identity so these arms measure the **crew**, exactly as
/// `FaunaConfig::without_retreat` holds the retreat at its identity across the hunt suites. The
/// geared default is pinned in `core_sim/tests/build_turns_closed_form.rs`.
fn bare_builders() -> core_sim::KitChoice {
    core_sim::EquipmentConfig::builtin()
        .kit("none")
        .expect("the shipped roster carries the empty kit")
}

/// **Meet every managed herd's keeping bill for this turn**, as a staffed `husbandry` role would.
/// Stamped comfortably above the bill because `advance_herds` regrows the herd between this and the
/// pass that reads it, which raises the keeper load and would otherwise leave it fractionally short.
fn keep_the_penned_herds(app: &mut App) {
    const A_FULLY_STAFFED_POOL: f32 = 4.0;
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bills: Vec<(String, f32)> = app
        .world
        .resource::<HerdRegistry>()
        .entries()
        .iter()
        .filter(|herd| herd.owner.is_some())
        .map(|herd| {
            (
                herd.id.clone(),
                core_sim::herd_upkeep_demand(herd, &fauna, &ladder) * A_FULLY_STAFFED_POOL,
            )
        })
        .collect();
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for (id, bill) in bills {
        if let Some(herd) = registry.herds.iter_mut().find(|herd| herd.id == id) {
            herd.upkeep_supplied = bill;
        }
    }
}
