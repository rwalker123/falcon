//! **Flora Roster F3 — the fodder coupling convergence gate + the "provisions stays lossy" invariant.**
//!
//! Hay is *delivered graze-flow*: a pen that knows Foddering draws the band's `FODDER` store to cover
//! the gap its footprint left, and its ceiling `K_pen` reads the *sustained fodder inflow* (the flow),
//! never the store's *stock* (`docs/plan_flora_roster.md` §5). This test runs the **real** coupled pen
//! systems forward against a controlled steady hay supply and asserts:
//!
//! - **(C1) the coupled loop converges** — `K → biomass → demand → fodder_draw → flow → K` reaches a
//!   stable fixed point from an over- and an under-stocked start, they agree, and two runs are
//!   bit-identical. This is the loop grazing 2b-ii had to gate; F3 reopens it. Because `K` reads the
//!   *flow* (a constant) and not the *stock* (which balloons as the buffer fills), it cannot oscillate:
//!   a buffer spike would spike K only if the ceiling read the stock, which it deliberately does not.
//! - **(C4) provisions stays lossy** — a pen fed by hay draws **zero** provisions for the hay-covered
//!   share, while an identical pen whose faction has *not* learned Foddering pays the **full** lossy
//!   bread bill; the two stores move independently and `FODDER` never converts to `FOOD`.
//!
//! Deterministic (a pinned map seed, no `Date`/rand), mirroring `grazing_2d_pen.rs`.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    advance_graze_regrowth, advance_herd_grazing, advance_herds, advance_husbandry,
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_graze,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FollowPolicy,
    ForageRegistry, GenerationId, GenerationRegistry, GrazeRegistry, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    SimulationConfig, SimulationTick, SizeClass, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FODDER,
    FODDERING_DISCOVERY_ID, FOOD, RUNG_COMPLETE,
};

const MAP_SEED: u64 = 119304647;
/// Turns per run — well past where the fast pen `r` settles.
const TURNS: u32 = 200;
/// The tail-window whose spread proves convergence.
const SETTLE_WINDOW: usize = 30;
/// The tail band's peak-to-peak span, as a fraction of its mean, must sit under this "small band".
const SMALL_BAND: f32 = 1e-2;
/// A big head-count so tending is never worker-limited.
const KEEPER_WORKERS: u32 = 5000;
/// Provisions re-stocked each turn so the *bread* bill is always payable — the tests read how much of
/// it the pen actually pays, not whether it can.
const RESTOCK: f32 = 1_000_000.0;
/// The pen species' metabolic demand — fodder eaten per unit biomass/turn.
const FODDER_RATE: f32 = 0.10;
/// The pen's wild breeding rate (→ pen `r = min(cap, wild × pen_gain)`).
const WILD_R: f32 = 0.35;
/// Rabbit-class body mass, matching `FODDER_RATE`/`WILD_R` (the pen quantises to whole animals).
const PEN_BODY_MASS: f32 = 2.0;
/// The **sustained hay inflow** — the per-turn output of the keeper's (notional) hay Fields. Constant,
/// which is the whole point: it is a *flow*, so `K_pen` off it is a fixed ceiling the loop settles on.
const HAY_FLOW: f32 = 20.0;

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
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_graze);
    app
}

/// The richest pasture tile on the map — a deterministic anchor for the pen. We strip the graze under
/// it (a **barren footprint**) so the pen is carried entirely by delivered hay — the feedlot / drylot
/// case, exactly what hay is for (property 3).
fn barren_pen_tile(app: &mut App) -> UVec2 {
    let tile = app
        .world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
        .0;
    app.world
        .resource_mut::<GrazeRegistry>()
        .patches
        .remove(&tile);
    tile
}

/// Grant the keeper faction **Foddering** so a pen may draw the hay store.
fn learn_foddering(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), FODDERING_DISCOVERY_ID, scalar_one());
}

/// Seat one **penned, domesticated** herd (fixture name → neutral density gain) at `tile`, radius 0.
fn seat_pen(app: &mut App, tile: UVec2, cap: f32, biomass: f32) -> String {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    let mut herd = Herd::new(
        "pen_0".to_string(),
        "Fixture Warren".to_string(),
        SizeClass::Small,
        vec![tile],
        biomass,
        cap,
        FODDER_RATE,
        WILD_R,
        PEN_BODY_MASS,
    );
    herd.accrue_domestication(FactionId(0), RUNG_COMPLETE);
    assert!(herd.corral_at(tile), "the fixture species must be pennable");
    registry.herds.push(herd);
    "pen_0".to_string()
}

/// A keeper band on the pen tile with one Hunt assignment — it tends the pen (feeds + harvests). The
/// `policy` only decides whether tending **teaches** knowledge (the corral-tend branch feeds + harvests
/// regardless): `Sustain` earns Foddering by running the pen (the real earn path), `Surplus` teaches
/// nothing — the honest way to hold a control faction ignorant of Foddering while it keeps a pen.
fn spawn_keeper(app: &mut App, herd_id: &str, tile: UVec2, policy: FollowPolicy) -> Entity {
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
                        policy,
                    },
                    workers: KEEPER_WORKERS,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// The Logistics half of a controlled pen turn, standing in for a hay Field in range:
/// - the keeper's `FODDER` store gets `hay_to_store` fresh hay (a hay Field harvests into the store
///   regardless of Foddering — growing hay needs only `Sow`), and its `FOOD` store is topped up so any
///   bread bill is payable;
/// - the pen's `fodder_delivery_rate` (the K term) is set to `k_rate` — which the caller sets to the
///   flow for a **foddering** keeper and to `0` for a control, mirroring the Foddering gate the labor
///   arm applies to the real stamping (a hay pile the faction cannot use lifts neither K nor feed).
///
/// Re-set every turn because `advance_labor_allocation` recomputes `fodder_delivery_rate` from the
/// band's (here absent) hay Fields and would otherwise leave it 0. Runs Logistics only, so a caller may
/// read the **post-regrow, pre-harvest** biomass the feed is charged on before running Population.
fn run_fodder_logistics(app: &mut App, keeper: Entity, id: &str, hay_to_store: f32, k_rate: f32) {
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(keeper)
            .expect("keeper");
        cohort.stores.add(FODDER, scalar_from_f32(hay_to_store));
        cohort.stores.set(FOOD, scalar_from_f32(RESTOCK));
    }
    if let Some(herd) = app
        .world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|h| h.id == id)
    {
        herd.fodder_delivery_rate = k_rate;
    }
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
    app.world.run_system_once(advance_husbandry);
}

/// A full controlled pen turn: Logistics then Population (the keeper FEEDs — hay before bread — and
/// HARVESTs).
fn run_fodder_turn(app: &mut App, keeper: Entity, id: &str, hay_to_store: f32, k_rate: f32) {
    run_fodder_logistics(app, keeper, id, hay_to_store, k_rate);
    app.world.run_system_once(advance_labor_allocation);
}

fn biomass_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass)
        .unwrap_or(0.0)
}

fn k_of(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.carrying_capacity)
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

/// Run a hay-carried pen from `start` biomass to convergence; return `(settled_biomass, settled_K)`.
fn run_to_settle(start: f32) -> (f32, f32) {
    let mut app = base_world();
    let tile = barren_pen_tile(&mut app);
    learn_foddering(&mut app);
    let id = seat_pen(&mut app, tile, 400.0, start);
    let keeper = spawn_keeper(&mut app, &id, tile, FollowPolicy::Sustain);

    let mut series = Vec::with_capacity(TURNS as usize);
    for _ in 0..TURNS {
        run_fodder_turn(&mut app, keeper, &id, HAY_FLOW, HAY_FLOW);
        series.push(biomass_of(&app, &id));
    }
    let settled = *series.last().unwrap();
    let spread = tail_spread(&series);
    assert!(
        spread < SMALL_BAND,
        "start {start}: a hay-carried pen must settle to a STABLE biomass; tail band {spread:.2e} \
         exceeds {SMALL_BAND:.0e} (settled {settled})"
    );
    assert!(
        settled > 0.0,
        "start {start}: a hay-carried pen must not crash to zero (settled {settled})"
    );
    (settled, k_of(&app, &id))
}

#[test]
fn the_fodder_loop_converges_to_one_fixed_point_from_over_and_under_stocked_starts() {
    // K_pen off the flow = HAY_FLOW / FODDER_RATE (× neutral density) = 200; the harvested pen settles
    // at K/2 ≈ 100, from BOTH a far-under and a far-over start.
    let (under_b, under_k) = run_to_settle(20.0);
    let (over_b, over_k) = run_to_settle(4000.0);
    println!(
        "F3 convergence: under -> biomass {under_b:.2}, K {under_k:.2}; \
         over -> biomass {over_b:.2}, K {over_k:.2}"
    );

    // The ceiling reads the FLOW, so it is the same regardless of start (a stock-based K would differ
    // by the buffer each start built).
    let expected_k = HAY_FLOW / FODDER_RATE;
    for (label, k) in [("under", under_k), ("over", over_k)] {
        assert!(
            (k - expected_k).abs() <= expected_k * 2e-2,
            "{label}: K_pen must equal the hay FLOW / fodder ({expected_k}), not a stock-driven \
             value (got {k})"
        );
    }
    // Same fixed point from both starts — the fenced land + hay set it, not history.
    assert!(
        (under_b - over_b).abs() <= over_b.max(1.0) * 2e-2,
        "the under- and over-stocked pens converge to the SAME biomass (under {under_b}, over {over_b})"
    );
    // And it is K/2 (constant-escapement harvest operating point).
    assert!(
        (under_b - expected_k / 2.0).abs() <= expected_k * 5e-2,
        "the settled pen sits at K/2 ({}): got {under_b}",
        expected_k / 2.0
    );
}

#[test]
fn the_coupled_fodder_loop_is_deterministic_across_two_runs() {
    let (a_b, a_k) = run_to_settle(20.0);
    let (b_b, b_k) = run_to_settle(20.0);
    assert_eq!(
        a_b.to_bits(),
        b_b.to_bits(),
        "two runs of the coupled fodder loop must be bit-identical (biomass {a_b} vs {b_b})"
    );
    assert_eq!(
        a_k.to_bits(),
        b_k.to_bits(),
        "two runs must agree on K bit-for-bit ({a_k} vs {b_k})"
    );
}

/// Read the keeper's per-turn PROVISIONS (bread) bill + the herd's `fodder_draw` (hay eaten).
fn feed_split(app: &App, keeper: Entity, id: &str) -> (f32, f32) {
    let bread = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("keeper")
        .last_pen_feed_upkeep;
    let hay = app
        .world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.fodder_draw)
        .unwrap_or(0.0);
    (bread, hay)
}

#[test]
fn a_hay_fed_pen_draws_no_bread_while_a_bread_fed_pen_pays_the_full_lossy_bill() {
    const SETTLE_TURNS: u32 = 120;
    const START: f32 = 80.0;

    // --- HAY-FED: the faction knows Foddering and a hay store is filled each turn. ---
    let mut app = base_world();
    let tile = barren_pen_tile(&mut app);
    learn_foddering(&mut app);
    let id = seat_pen(&mut app, tile, 400.0, START);
    let keeper = spawn_keeper(&mut app, &id, tile, FollowPolicy::Sustain);
    let food_before = RESTOCK;
    for _ in 0..SETTLE_TURNS {
        run_fodder_turn(&mut app, keeper, &id, HAY_FLOW, HAY_FLOW);
    }
    let (hay_bread, hay_draw) = feed_split(&app, keeper, &id);
    let hay_food_remaining = app
        .world
        .get::<PopulationCohort>(keeper)
        .unwrap()
        .stores
        .get(FOOD)
        .to_f32();
    println!("HAY-FED: bread bill/turn {hay_bread:.5}, hay drawn/turn {hay_draw:.4}");
    // The hay covers the whole (barren-footprint) demand, so the provisions bill is ~0 and real hay
    // was drawn.
    assert!(
        hay_draw > 0.0,
        "a hay-fed pen must actually draw hay (got {hay_draw})"
    );
    assert!(
        hay_bread < 1e-3,
        "a hay-fed pen draws ~0 bread for the hay-covered share (paid {hay_bread}/turn)"
    );
    // FODDER never became FOOD: the store is topped up to RESTOCK at the start of the turn and only the
    // (tiny) bread bill is debited, so what remains is essentially the full RESTOCK.
    assert!(
        (food_before - hay_food_remaining) < 1e-2,
        "the provisions store barely moved for a hay-fed pen — FODDER never converts to FOOD \
         (spent {})",
        food_before - hay_food_remaining
    );

    // --- BREAD-FED CONTROL: identical pen, hay STILL grown into its store each turn (a hay Field
    // harvests regardless of Foddering), but the faction has NOT learned Foddering — so the K term is
    // gated off (`k_rate = 0`, mirroring the labor arm's gate) and the hay is undrawable. The pen pays
    // the full lossy provisions bill and the hay just piles up. `Surplus` so tending never teaches it
    // Foddering. ---
    let mut app = base_world();
    let tile = barren_pen_tile(&mut app);
    // (no learn_foddering)
    let id = seat_pen(&mut app, tile, 400.0, START);
    let keeper = spawn_keeper(&mut app, &id, tile, FollowPolicy::Surplus);
    // Settle, then run ONE instrumented final turn: read the FEED-time biomass (post-regrow,
    // pre-harvest) — what the bill is actually charged on — and compare exactly, as `grazing_2d_pen`
    // does for the barren-pen case.
    for _ in 0..SETTLE_TURNS - 1 {
        run_fodder_turn(&mut app, keeper, &id, HAY_FLOW, 0.0);
    }
    run_fodder_logistics(&mut app, keeper, &id, HAY_FLOW, 0.0);
    let feed_biomass = biomass_of(&app, &id); // post-regrow, pre-harvest = what FEED charges on
    app.world.run_system_once(advance_labor_allocation);
    let (bread_bread, bread_draw) = feed_split(&app, keeper, &id);
    let bread_hay_store = app
        .world
        .get::<PopulationCohort>(keeper)
        .unwrap()
        .stores
        .get(FODDER)
        .to_f32();
    println!("BREAD-FED: bread bill/turn {bread_bread:.5}, hay drawn/turn {bread_draw:.4}");
    // No Foddering → no hay draw, and the FODDER store only accumulates (never spent) — the plainest
    // statement that FODDER never converts to FOOD: the pen paid bread while sitting on a hay pile.
    assert!(
        bread_draw.abs() < 1e-9,
        "a pen whose faction lacks Foddering draws no hay (got {bread_draw})"
    );
    assert!(
        bread_hay_store > HAY_FLOW,
        "the hay store only accumulated for a non-foddering pen (got {bread_hay_store})"
    );
    // It pays the FULL lossy bill — `upkeep_per_biomass × biomass` charged on the pre-harvest biomass.
    let full_bill = 0.002 * feed_biomass; // pen.upkeep_per_biomass × biomass
    assert!(
        bread_bread > 0.0 && (bread_bread - full_bill).abs() < full_bill * 0.02,
        "a bread-fed pen pays the full lossy provisions bill: paid {bread_bread}/turn vs expected \
         {full_bill} (upkeep × pre-harvest biomass {feed_biomass})"
    );
    // And the two stores are independent: the bread-fed pen spent provisions while its hay pile grew,
    // where the hay-fed pen paid ~0 bread by drawing hay instead.
    assert!(
        bread_bread > hay_bread * 100.0,
        "feeding a pen bread ({bread_bread}) stays exactly as lossy as ever, while hay ({hay_bread}) \
         pays it down — the two never trade"
    );
}
