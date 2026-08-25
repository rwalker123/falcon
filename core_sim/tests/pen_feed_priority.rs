//! **THE PEN FEED SPLIT IS A SETTLEMENT, NOT A WALK DOWN A VECTOR**
//! (`docs/plan_standing_upkeep.md` §4.9 item 9b).
//!
//! The corral-tend arm used to draw hay and then bread off the band's stores **inside** the assignment
//! loop, one row at a time, so a store that could not cover every pen fed the earliest row in
//! `assignments` and starved the last. And because `LaborAllocation::set_assignment` removes an edited
//! row and re-pushes it at the **end**, the pen the player had just adjusted was the one fed last —
//! positional allocation, with the position controlled by the most recent stepper press.
//!
//! Every test here drives the **real** `advance_labor_allocation` against a keeper band holding two
//! pens and a store too thin for both, and asserts:
//!
//! - **the mark decides, and the vector does not** — a `High` pen is fed whole and a `Low` one
//!   starves, and the *same* outcome holds with the assignments reversed and again after an edit has
//!   re-pushed the fed row to the end (the arrangement the old code was guaranteed to get wrong);
//! - **equal ranks split in proportion** — two `Normal` pens against a short store are each part-fed
//!   in proportion to what they asked for, the shares sum to what the store held, and neither pen
//!   gets everything;
//! - **both stores behave the same way** — the `FODDER` store and then the larder, in the order the
//!   pen economy already spends them.
//!
//! Deterministic (a pinned map seed, no `Date`/rand), mirroring `grazing_f3_fodder.rs`, whose fixture
//! shape this file follows: the graze footprint is posed directly so the feed is charged on exactly
//! the biomass seated.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_graze,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, ForagePatch,
    ForageRegistry, GenerationId, GenerationRegistry, GrazeRegistry, Herd, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    SimulationConfig, SimulationTick, SizeClass, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, SourcePriority, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry, WellbeingConfigHandle, FODDER,
    FODDERING_DISCOVERY_ID, FOOD,
};

const MAP_SEED: u64 = core_sim::HARNESS_MAP_SEED;
/// A big head-count so tending is never worker-limited — the split under test is about stores.
const KEEPER_WORKERS: u32 = 5000;
/// The pen species' metabolic demand — fodder eaten per unit biomass/turn (`grazing_f3_fodder.rs`).
const FODDER_RATE: f32 = 0.10;
/// The pen's wild breeding rate.
const WILD_R: f32 = 0.35;
/// Rabbit-class body mass, matching `FODDER_RATE`/`WILD_R`.
const PEN_BODY_MASS: f32 = 2.0;
/// The pen carrying capacity every fixture herd is seated under — well above the biomass posed, so
/// nothing here is capacity-limited.
const PEN_CAPACITY: f32 = 4000.0;
/// `fauna_config.json`'s `husbandry.pen.upkeep_per_biomass` — the gross bread bill per unit biomass.
/// An INDEPENDENT reconstruction of the sim's own `pen_upkeep`, exactly as `grazing_f3_fodder.rs`
/// keeps one, so a fixture that sizes a store to "one pen's bill" is not sized from the number under
/// test.
const PEN_UPKEEP_PER_BIOMASS: f32 = 0.002;
/// `f32` sums of `Scalar`-quantized stores — a few ULPs of slack, no more.
const EPSILON: f32 = 1e-5;
/// A larder deep enough that the bread bill is never the binding constraint.
const AMPLE_LARDER: f32 = 1_000_000.0;
/// Hay enough that the `FODDER` store is never the binding constraint.
const AMPLE_HAY: f32 = 1_000_000.0;
/// The Sustain floor every fixture keeper works its pen at.
const SUSTAIN: f32 = 0.5;
/// The two pens' biomasses. The **richer** one asks for twice the feed of the **leaner** one, so a
/// proportional split has two visibly different shares and an "each gets half the store" bug cannot
/// pass as one.
const RICH_PEN_BIOMASS: f32 = 200.0;
const LEAN_PEN_BIOMASS: f32 = 100.0;
/// A barren footprint — the drylot case, so the whole grass demand is a shortfall and every term
/// under test is decided by the two stores rather than by the land.
const BARREN: f32 = 0.0;
/// A larder with **nothing in it** at the top of the pass — the hand-to-mouth band whose pens live on
/// what its crews bring home this very turn.
const EMPTY_LARDER: f32 = 0.0;
/// No hay at all, and no Foddering to draw it with.
const NO_HAY: f32 = 0.0;
/// The gathering row's floor: take the whole stand, so the gather is bounded by the crew and the
/// patch rather than by a floor the test would then have to reason about.
const STRIP_THE_STAND: f32 = 0.0;
/// The gathering patch's `K`. Deep enough that one turn's gather is worth many times a pen's bread
/// bill, so *"the pen ate out of this turn's income"* is unambiguous.
const GATHER_CAPACITY: f32 = 500.0;
/// A **thin** stand, for the fixtures whose point is that the day's gather does not go round: one
/// turn's take off it is worth about one small pen's bread bill, and the pens are then posed against
/// the income it actually paid (`gathered_income`), never against this number.
const SCANT_GATHER_CAPACITY: f32 = 6.0;
/// The crew every row of a gather-funded fixture carries. Small enough that all of them together fit
/// the band's worker pool, so the shed walk never runs and the gather is bounded by the **stand** —
/// which is what makes one probe run's income the income the real fixture sees. (The stocked fixtures
/// keep [`KEEPER_WORKERS`]: there the shed is harmless, because a pen's feed is priced off its
/// biomass and not off its crew.)
const GATHERING_CREW: u32 = 1_000;

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

/// A deterministic tile for both pens. The footprint is posed directly below, so the tile's own
/// pasture never enters the arithmetic.
fn pen_tile(app: &App) -> UVec2 {
    app.world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
        .0
}

/// Grant the keeper faction **Foddering** so its pens may draw the hay store.
fn learn_foddering(app: &mut App) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), FODDERING_DISCOVERY_ID, scalar_one());
}

/// Seat TWO penned, domesticated herds on `tile`. Both are corralled where the keeper stands, so both
/// are inside the hunt leash and the only thing separating them is the mark their row carries.
fn seat_two_pens(app: &mut App, tile: UVec2, rich_biomass: f32, lean_biomass: f32) {
    seat_pens(
        app,
        tile,
        &[("pen_rich", rich_biomass), ("pen_lean", lean_biomass)],
    );
}

/// Seat one penned, domesticated herd per `(id, biomass)` on `tile`, clearing whatever worldgen left.
fn seat_pens(app: &mut App, tile: UVec2, pens: &[(&str, f32)]) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    for (id, biomass) in pens.iter().copied() {
        let mut herd = Herd::new(
            id.to_string(),
            format!("Fixture {id}"),
            SizeClass::Small,
            vec![tile],
            biomass,
            PEN_CAPACITY,
            FODDER_RATE,
            WILD_R,
            PEN_BODY_MASS,
        );
        herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
        assert!(
            herd.corral_at(tile, &core_sim::LadderConfig::builtin()),
            "the fixture species must be pennable"
        );
        registry.herds.push(herd);
    }
}

/// One keeper row: tend the pen `herd_id` under the mark `priority`, crewed `workers` strong.
fn hunt_row(herd_id: &str, priority: SourcePriority, workers: u32) -> LaborAssignment {
    LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: herd_id.to_string(),
            floor: SUSTAIN,
        },
        workers,
        kit: None,
        priority,
    }
}

/// One gathering row on `tile`, stripping the stand — the food-crediting row the hand-to-mouth cases
/// need. It pays into the same larder the pens are fed from, **inside** the assignment loop.
fn forage_row(tile: UVec2, priority: SourcePriority, workers: u32) -> LaborAssignment {
    LaborAssignment {
        target: LaborTarget::Forage {
            tile,
            floor: STRIP_THE_STAND,
            species: None,
            take_species: Default::default(),
        },
        workers,
        kit: None,
        priority,
    }
}

/// A keeper band standing on `tile` holding `assignments`, in the order given — which is the axis
/// these tests vary.
fn spawn_keeper(app: &mut App, assignments: Vec<LaborAssignment>, tile: UVec2) -> Entity {
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
                assignments,
                ..Default::default()
            },
        ))
        .id()
}

/// Pose both pens' grazed footprints directly (skipping `advance_herd_grazing`) so the feed is
/// charged on exactly the biomass seated.
fn pose_footprints(app: &mut App, intake: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for herd in registry.herds.iter_mut() {
        herd.footprint_intake = intake;
    }
}

/// Seat a full stand of wild plants on `tile` for the gathering row to work.
fn seat_gathering_patch(app: &mut App, tile: UVec2, capacity: f32) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    registry.patches.clear();
    registry
        .patches
        .insert(tile, ForagePatch::new(tile, capacity));
}

/// What is left in the keeper's larder after the turn.
fn larder_left(app: &App, keeper: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(keeper)
        .expect("keeper")
        .stores
        .get(FOOD)
        .to_f32()
}

/// Stock the keeper's two stores.
fn stock(app: &mut App, keeper: Entity, hay: f32, larder: f32) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(keeper).unwrap();
    cohort.stores.set(FODDER, scalar_from_f32(hay));
    cohort.stores.set(FOOD, scalar_from_f32(larder));
}

/// The share of its own feed bill a pen actually got. With a barren footprint and no hay this is
/// exactly the share of its larder bill the band paid.
fn fed_fraction(app: &App, herd_id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("the pen is still seated")
        .pen_fed_fraction
}

/// The hay one pen actually ate this turn.
fn hay_drawn(app: &App, herd_id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("the pen is still seated")
        .fodder_draw
}

/// The band's whole pen-feed debit this turn — the summed `LocalStore::take`, which is what the food
/// ledger identity is closed with.
fn pen_feed_paid(app: &App, keeper: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(keeper)
        .expect("keeper")
        .last_pen_feed_upkeep
}

/// The gross bread bill of a pen at `biomass`, reconstructed from config rather than from the sim.
fn gross_bill(biomass: f32) -> f32 {
    PEN_UPKEEP_PER_BIOMASS * biomass
}

/// **THE ARRANGEMENT AXIS** — how the two rows are laid out before the turn runs. All three describe
/// the *same* two pens with the *same* two marks; only the vector differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrangement {
    /// The high-ranked pen's row first.
    First,
    /// The high-ranked pen's row last.
    Last,
    /// The high-ranked pen's row edited through `set_assignment`, which re-pushes it to the END of
    /// the vector — the arrangement the shipped loop-order draw was guaranteed to get wrong, because
    /// a stepper press moved the pen the player had just adjusted to the back of the feed line.
    Edited,
}

/// The `−`/`+` the player presses on the rich pen's row: `set_assignment` drops the row and re-pushes
/// it at the END, so this is the arrangement in which the shipped loop-order draw fed it last.
fn edit_the_rich_row(app: &mut App, keeper: Entity, rich_rank: SourcePriority) {
    let mut allocation = app.world.get_mut::<LaborAllocation>(keeper).unwrap();
    allocation.set_assignment(
        LaborTarget::Hunt {
            fauna_id: "pen_rich".to_string(),
            floor: SUSTAIN,
        },
        KEEPER_WORKERS,
        KEEPER_WORKERS,
        None,
    );
    assert_eq!(
        allocation.assignments.last().map(|row| row.priority),
        Some(rich_rank),
        "the edit re-pushed the rich pen's row to the end AND kept its mark — if either half of \
         this fails the arrangement is not the one the test means to drive"
    );
}

/// Drive one real corral-tend turn on two pens with `larder`/`hay` in the band's stores, laying the
/// rows out as `arrangement` says and marking the **rich** pen `rich_rank` and the lean one
/// `lean_rank`.
fn run_two_pen_turn(
    arrangement: Arrangement,
    rich_rank: SourcePriority,
    lean_rank: SourcePriority,
    foddering: bool,
    hay: f32,
    larder: f32,
) -> (App, Entity) {
    let mut app = base_world();
    let tile = pen_tile(&app);
    if foddering {
        learn_foddering(&mut app);
    }
    seat_two_pens(&mut app, tile, RICH_PEN_BIOMASS, LEAN_PEN_BIOMASS);
    let rows: Vec<LaborAssignment> = match arrangement {
        Arrangement::First | Arrangement::Edited => {
            vec![
                hunt_row("pen_rich", rich_rank, KEEPER_WORKERS),
                hunt_row("pen_lean", lean_rank, KEEPER_WORKERS),
            ]
        }
        Arrangement::Last => vec![
            hunt_row("pen_lean", lean_rank, KEEPER_WORKERS),
            hunt_row("pen_rich", rich_rank, KEEPER_WORKERS),
        ],
    };
    let keeper = spawn_keeper(&mut app, rows, tile);
    if arrangement == Arrangement::Edited {
        edit_the_rich_row(&mut app, keeper, rich_rank);
    }
    stock(&mut app, keeper, hay, larder);
    pose_footprints(&mut app, BARREN);
    app.world.run_system_once(advance_labor_allocation);
    (app, keeper)
}

/// **THE HIGH PEN EATS AND THE LOW PEN STARVES — IN ALL THREE ARRANGEMENTS.**
///
/// The larder holds exactly the rich pen's bread bill and the two pens together ask for one and a
/// half times that, so somebody goes hungry and the marks say who. The shipped loop-order draw
/// passes none of the three: it fed whichever pen's row came first, and
/// [`Arrangement::Edited`] is the case where that is the pen the player just touched.
#[test]
fn a_high_marked_pen_is_fed_whole_and_a_low_one_starves_whatever_the_vector_says() {
    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, keeper) = run_two_pen_turn(
            arrangement,
            SourcePriority::High,
            SourcePriority::Low,
            false,
            0.0,
            gross_bill(RICH_PEN_BIOMASS),
        );
        let rich = fed_fraction(&app, "pen_rich");
        let lean = fed_fraction(&app, "pen_lean");
        println!("{arrangement:?}: rich {rich:.6}, lean {lean:.6}");
        assert!(
            (rich - 1.0).abs() < EPSILON,
            "{arrangement:?}: the High pen is fed whole (got {rich})"
        );
        assert!(
            lean.abs() < EPSILON,
            "{arrangement:?}: the Low pen gets nothing once the store is spent (got {lean})"
        );
        assert!(
            (pen_feed_paid(&app, keeper) - gross_bill(RICH_PEN_BIOMASS)).abs() < EPSILON,
            "{arrangement:?}: the band's real debit is the whole larder and no more"
        );
    }
}

/// **AND THE MARKS ARE WHAT DECIDES IT, NOT THE PENS.** The same fixture with the ranks swapped feeds
/// the *other* pen — so the answer above cannot be an artifact of which pen is richer or of where
/// either row sits.
#[test]
fn swapping_the_marks_swaps_which_pen_eats() {
    // The lean pen is High now, and the larder holds exactly ITS bill.
    let (app, _) = run_two_pen_turn(
        Arrangement::First,
        SourcePriority::Low,
        SourcePriority::High,
        false,
        0.0,
        gross_bill(LEAN_PEN_BIOMASS),
    );
    let rich = fed_fraction(&app, "pen_rich");
    let lean = fed_fraction(&app, "pen_lean");
    assert!(
        (lean - 1.0).abs() < EPSILON,
        "the pen marked High is fed whole (got {lean})"
    );
    assert!(
        rich.abs() < EPSILON,
        "and the pen marked Low goes without, though it is the bigger holding (got {rich})"
    );
}

/// **THE SAME RULE ON THE HAY STORE.** `FODDER` is served before the larder and by the same
/// settlement, so a `High` pen draws its whole grass shortfall and a `Low` one draws nothing.
#[test]
fn a_short_hay_store_serves_the_high_pen_first_too() {
    // Barren footprints, so each pen's grass shortfall is its whole demand: FODDER_RATE x biomass.
    let rich_grass = FODDER_RATE * RICH_PEN_BIOMASS;
    let (app, _) = run_two_pen_turn(
        Arrangement::Last,
        SourcePriority::High,
        SourcePriority::Low,
        true,
        rich_grass,
        AMPLE_LARDER,
    );
    let rich_hay = hay_drawn(&app, "pen_rich");
    let lean_hay = hay_drawn(&app, "pen_lean");
    assert!(
        (rich_hay - rich_grass).abs() < EPSILON,
        "the High pen draws its whole shortfall off the hay store (got {rich_hay})"
    );
    assert!(
        lean_hay.abs() < EPSILON,
        "and the Low pen draws nothing from what is left (got {lean_hay})"
    );
    assert!(
        (fed_fraction(&app, "pen_rich") - 1.0).abs() < EPSILON,
        "a pen fed entirely by hay reads fully fed"
    );
}

/// **TWO PENS ON THE SAME MARK SPLIT A SHORT STORE IN PROPORTION TO WHAT THEY ASKED FOR** — which is
/// what makes the settlement need no second ordering rule at all, and therefore nothing a vector
/// position could decide.
///
/// The larder holds half the two bills together. Each pen must come out at half its own bill — so the
/// rich pen gets twice the food the lean one does while both read the *same* fed fraction, and
/// neither reads `0` or `1`.
#[test]
fn two_equally_ranked_pens_split_a_short_larder_in_proportion_to_demand() {
    const HALF: f32 = 0.5;
    let total_bill = gross_bill(RICH_PEN_BIOMASS) + gross_bill(LEAN_PEN_BIOMASS);
    let (app, keeper) = run_two_pen_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        false,
        0.0,
        total_bill * HALF,
    );
    let rich = fed_fraction(&app, "pen_rich");
    let lean = fed_fraction(&app, "pen_lean");
    println!("proportional: rich {rich:.6}, lean {lean:.6}");
    assert!(
        (rich - HALF).abs() < EPSILON,
        "the rich pen is fed half its bill (got {rich})"
    );
    assert!(
        (lean - HALF).abs() < EPSILON,
        "the lean pen is fed half of its own, smaller bill (got {lean})"
    );
    assert!(
        rich > EPSILON && rich < 1.0 - EPSILON && lean > EPSILON && lean < 1.0 - EPSILON,
        "NEITHER pen gets everything and neither is starved out: rich {rich}, lean {lean}"
    );
    assert!(
        (pen_feed_paid(&app, keeper) - total_bill * HALF).abs() < EPSILON,
        "and the shares together are exactly what the larder held"
    );
}

/// **THE SAME PROPORTIONAL RULE ON THE HAY STORE**, for the reason the priority rule is asserted on
/// both: the two stores are served by one settlement and a fix applied to only one of them would
/// leave half the defect standing.
#[test]
fn two_equally_ranked_pens_split_a_short_hay_store_in_proportion_to_demand() {
    const HALF: f32 = 0.5;
    let rich_grass = FODDER_RATE * RICH_PEN_BIOMASS;
    let lean_grass = FODDER_RATE * LEAN_PEN_BIOMASS;
    let (app, _) = run_two_pen_turn(
        Arrangement::Last,
        SourcePriority::Normal,
        SourcePriority::Normal,
        true,
        (rich_grass + lean_grass) * HALF,
        AMPLE_LARDER,
    );
    let rich_hay = hay_drawn(&app, "pen_rich");
    let lean_hay = hay_drawn(&app, "pen_lean");
    assert!(
        (rich_hay - rich_grass * HALF).abs() < EPSILON,
        "the rich pen draws half its shortfall (got {rich_hay})"
    );
    assert!(
        (lean_hay - lean_grass * HALF).abs() < EPSILON,
        "the lean pen draws half of its own (got {lean_hay})"
    );
    assert!(
        rich_hay > lean_hay,
        "proportional, not equal: the bigger demand draws the bigger share"
    );
}

/// **WITHOUT FODDERING NOTHING CHANGES AT ALL.** A faction that has not learned to hay a herd bids
/// `0` into the settlement, so a full `FODDER` store is untouched and both pens fall back on the
/// larder exactly as a pasture-only pen always did.
#[test]
fn a_faction_without_foddering_draws_no_hay_however_full_the_store_is() {
    let (app, keeper) = run_two_pen_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        false,
        AMPLE_HAY,
        AMPLE_LARDER,
    );
    for id in ["pen_rich", "pen_lean"] {
        assert!(
            hay_drawn(&app, id).abs() < EPSILON,
            "{id} drew hay with no Foddering"
        );
        assert!(
            (fed_fraction(&app, id) - 1.0).abs() < EPSILON,
            "{id} is fed in full off the larder, as it always was"
        );
    }
    let both_bills = gross_bill(RICH_PEN_BIOMASS) + gross_bill(LEAN_PEN_BIOMASS);
    assert!(
        (pen_feed_paid(&app, keeper) - both_bills).abs() < EPSILON,
        "and the whole debit is bread, on the pre-hay basis"
    );
    let hay_left = app
        .world
        .get::<PopulationCohort>(keeper)
        .unwrap()
        .stores
        .get(FODDER)
        .to_f32();
    assert!(
        (hay_left - AMPLE_HAY).abs() < EPSILON,
        "the hay store is untouched (got {hay_left})"
    );
}

/// **A BAND THAT OPENED THE TURN WITH AN EMPTY LARDER STILL FEEDS ITS PEN OUT OF WHAT IT GATHERED
/// TODAY** — and it does so whichever way round the two rows sit.
///
/// The larder is a **flow** as much as a stock: `FOOD` is credited *inside* the assignment loop, by
/// the gather, the pen's own harvest and the hunt take, so a keeper living hand to mouth has paid its
/// pens out of the same turn's income for as long as pens have existed. Settling the larder against
/// the store standing at the **top** of the pass would take that away: the fed fraction would
/// collapse to the land/hay share, `last_pen_feed_upkeep` would publish `0`, and next turn's
/// `advance_husbandry` would shrink a herd whose keeper was carrying home food all along.
///
/// The hay half is deliberately different (it is settled off the top-of-pass `FODDER` stock, a
/// buffer), which is why this is asserted on the larder alone — see `settle_pen_larder`.
#[test]
fn a_pen_is_fed_from_the_same_turns_gather_when_the_larder_opened_empty() {
    let bill = gross_bill(LEAN_PEN_BIOMASS);
    let mut fed_each_way: Vec<f32> = Vec::new();
    for gather_first in [true, false] {
        let mut app = base_world();
        let tile = pen_tile(&app);
        seat_pens(&mut app, tile, &[("pen_lean", LEAN_PEN_BIOMASS)]);
        seat_gathering_patch(&mut app, tile, GATHER_CAPACITY);
        let gather = forage_row(tile, SourcePriority::Normal, GATHERING_CREW);
        let pen = hunt_row("pen_lean", SourcePriority::Normal, GATHERING_CREW);
        let rows = if gather_first {
            vec![gather, pen]
        } else {
            vec![pen, gather]
        };
        let keeper = spawn_keeper(&mut app, rows, tile);
        stock(&mut app, keeper, NO_HAY, EMPTY_LARDER);
        pose_footprints(&mut app, BARREN);
        app.world.run_system_once(advance_labor_allocation);

        let fed = fed_fraction(&app, "pen_lean");
        let paid = pen_feed_paid(&app, keeper);
        let left = larder_left(&app, keeper);
        println!("gather_first={gather_first}: fed {fed:.6}, paid {paid:.6}, left {left:.6}");
        // **Not vacuous**: the gather really did credit more food than the pen's whole bill, so a
        // fully-fed pen means the income paid it and not that there was nothing to pay.
        assert!(
            left > EPSILON,
            "gather_first={gather_first}: the gathering row must credit food this turn, or this \
             fixture proves nothing (larder left {left})"
        );
        assert!(
            (paid - bill).abs() < EPSILON,
            "gather_first={gather_first}: the pen is paid its whole bread bill out of this turn's \
             income (paid {paid}, bill {bill})"
        );
        assert!(
            (fed - 1.0).abs() < EPSILON,
            "gather_first={gather_first}: a pen paid in full reads fully fed (got {fed})"
        );
        fed_each_way.push(fed);
    }
    assert!(
        (fed_each_way[0] - fed_each_way[1]).abs() < EPSILON,
        "and the answer does not depend on which row came first: {fed_each_way:?}"
    );
}

/// **WHAT ONE TURN'S GATHERING PAYS INTO AN EMPTY LARDER** on this fixture, measured by running the
/// gathering row on its own — so the tests below can size a pen's bread bill against the band's real
/// income rather than pin a number that a flora retune would silently invalidate.
fn gathered_income() -> f32 {
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_pens(&mut app, tile, &[]);
    seat_gathering_patch(&mut app, tile, SCANT_GATHER_CAPACITY);
    let keeper = spawn_keeper(
        &mut app,
        vec![forage_row(tile, SourcePriority::Normal, GATHERING_CREW)],
        tile,
    );
    stock(&mut app, keeper, NO_HAY, EMPTY_LARDER);
    app.world.run_system_once(advance_labor_allocation);
    let income = larder_left(&app, keeper);
    assert!(
        income > EPSILON,
        "the gathering row must credit real food, or every fixture built on it is vacuous"
    );
    income
}

/// The pen biomass whose gross bread bill is exactly `bill` — [`gross_bill`] inverted, so a fixture
/// can pose a pen that asks for a named share of the day's income.
fn biomass_billed(bill: f32) -> f32 {
    bill / PEN_UPKEEP_PER_BIOMASS
}

/// Drive one real turn on two pens whose **only** larder is what the band gathers during it: the store
/// opens empty and one gathering row, sitting **behind both pens** in the vector, pays into it as the
/// loop walks. `rich`/`lean` name each pen's mark and the bread bill it is posed to ask for.
fn run_gather_funded_turn(
    arrangement: Arrangement,
    rich: (SourcePriority, f32),
    lean: (SourcePriority, f32),
) -> (App, Entity) {
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_pens(
        &mut app,
        tile,
        &[
            ("pen_rich", biomass_billed(rich.1)),
            ("pen_lean", biomass_billed(lean.1)),
        ],
    );
    seat_gathering_patch(&mut app, tile, SCANT_GATHER_CAPACITY);
    let mut rows = match arrangement {
        Arrangement::First | Arrangement::Edited => vec![
            hunt_row("pen_rich", rich.0, GATHERING_CREW),
            hunt_row("pen_lean", lean.0, GATHERING_CREW),
        ],
        Arrangement::Last => vec![
            hunt_row("pen_lean", lean.0, GATHERING_CREW),
            hunt_row("pen_rich", rich.0, GATHERING_CREW),
        ],
    };
    // **The earner goes LAST**, behind both pens: the income has to reach a row the loop visited
    // before it was banked, which a walk down the vector could never do.
    rows.push(forage_row(tile, SourcePriority::Normal, GATHERING_CREW));
    let keeper = spawn_keeper(&mut app, rows, tile);
    if arrangement == Arrangement::Edited {
        edit_the_rich_row(&mut app, keeper, rich.0);
    }
    stock(&mut app, keeper, NO_HAY, EMPTY_LARDER);
    pose_footprints(&mut app, BARREN);
    app.world.run_system_once(advance_labor_allocation);
    (app, keeper)
}

/// **THE MARK STILL DECIDES WHO EATS WHEN THE LARDER IS A DAY'S GATHER RATHER THAN A STOCK** — in all
/// three arrangements.
///
/// The band opens with nothing; the gathering row pays in exactly one pen's bill; the two pens
/// together ask for half as much again. So the settlement is short, the marks say who is served, and
/// the answer is the same one the stocked fixtures give.
#[test]
fn the_mark_decides_who_eats_out_of_the_days_gather_whatever_the_vector_says() {
    /// The Low pen asks for half what the High one does, so a short settlement that ignored the marks
    /// and split in proportion would read `0.67`/`0.67` rather than `1`/`0`.
    const LEAN_SHARE_OF_INCOME: f32 = 0.5;
    let income = gathered_income();
    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, keeper) = run_gather_funded_turn(
            arrangement,
            (SourcePriority::High, income),
            (SourcePriority::Low, income * LEAN_SHARE_OF_INCOME),
        );
        let rich = fed_fraction(&app, "pen_rich");
        let lean = fed_fraction(&app, "pen_lean");
        let paid = pen_feed_paid(&app, keeper);
        let left = larder_left(&app, keeper);
        println!("{arrangement:?}: rich {rich:.6}, lean {lean:.6}, paid {paid:.6}, left {left:.6}");
        assert!(
            (paid + left - income).abs() < EPSILON,
            "{arrangement:?}: the day's gather is the WHOLE larder under test — paid {paid} + left \
             {left} must be the measured income {income}, or something else fed these pens"
        );
        assert!(
            (rich - 1.0).abs() < EPSILON,
            "{arrangement:?}: the High pen is fed whole out of the day's gather (got {rich})"
        );
        assert!(
            lean.abs() < EPSILON,
            "{arrangement:?}: the Low pen gets nothing once the gather is spent (got {lean})"
        );
        assert!(
            (paid - income).abs() < EPSILON,
            "{arrangement:?}: and the whole day's gather went into the High pen (paid {paid} of \
             {income})"
        );
    }
}

/// **AND TWO PENS ON THE SAME MARK SPLIT THE DAY'S GATHER IN PROPORTION TO WHAT THEY ASKED FOR** — the
/// income-funded twin of the stocked proportional case, so the late larder settlement is shown to be
/// the *same* settlement and not merely a payment that happens later.
#[test]
fn two_equally_marked_pens_split_the_days_gather_in_proportion_to_demand() {
    /// The lean pen asks for half what the rich one does, so the two shares are visibly different
    /// while the two fed fractions are equal.
    const LEAN_SHARE_OF_INCOME: f32 = 0.5;
    let income = gathered_income();
    let lean_bill = income * LEAN_SHARE_OF_INCOME;
    // Every pen in a short tier is served the same fraction of its own bill: the store over what the
    // tier asked for.
    let served = income / (income + lean_bill);
    let (app, keeper) = run_gather_funded_turn(
        Arrangement::First,
        (SourcePriority::Normal, income),
        (SourcePriority::Normal, lean_bill),
    );
    let rich = fed_fraction(&app, "pen_rich");
    let lean = fed_fraction(&app, "pen_lean");
    println!("gather-funded proportional: rich {rich:.6}, lean {lean:.6}, served {served:.6}");
    assert!(
        (rich - served).abs() < EPSILON && (lean - served).abs() < EPSILON,
        "both pens are served the same fraction {served} of their own bills (rich {rich}, lean \
         {lean})"
    );
    assert!(
        rich > EPSILON && rich < 1.0 - EPSILON,
        "NEITHER pen is fed whole and neither is starved out (rich {rich})"
    );
    assert!(
        (pen_feed_paid(&app, keeper) - income).abs() < EPSILON,
        "and the shares together are exactly the day's gather"
    );
}
