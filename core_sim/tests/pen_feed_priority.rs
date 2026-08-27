//! **THE PEN FEED SPLIT IS A SETTLEMENT, NOT A WALK DOWN A VECTOR**
//! (`docs/plan_standing_upkeep.md` §4.9 item 9b).
//!
//! The corral-tend arm used to draw hay off the band's `FODDER` store **inside** the assignment loop,
//! one row at a time, so a store that could not cover every pen fed the earliest row in `assignments`
//! and starved the last. And because `LaborAllocation::set_assignment` removes an edited row and
//! re-pushes it at the **end**, the pen the player had just adjusted was the one fed last — positional
//! allocation, with the position controlled by the most recent stepper press.
//!
//! Every test here drives the **real** `advance_labor_allocation` against a keeper band holding two
//! pens and a hay store too thin for both, and asserts:
//!
//! - **the mark decides, and the vector does not** — a `High` pen is fed whole and a `Low` one
//!   starves, and the *same* outcome holds with the assignments reversed and again after an edit has
//!   re-pushed the fed row to the end (the arrangement the old code was guaranteed to get wrong);
//! - **equal ranks split in proportion** — two `Normal` pens against a short store are each part-fed
//!   in proportion to what they asked for, and neither pen gets everything;
//! - **the band's larder is not on the table at all** — however hungry the pens, not one unit of
//!   `FOOD` moves for feed.
//!
//! # ⛔ THERE IS ONE STORE, AND IT IS THE HAY
//!
//! There used to be two: hay first, then the keeper's `FOOD` larder for whatever the pasture and hay
//! left unpaid, settled after the loop because provisions are credited inside it. **Human food is not
//! animal feed.** The larder draw was the modelling error, and its real effect was to hide the
//! starvation path — a pen whose pasture failed took the food out of its keepers' mouths instead of
//! shrinking. So the three fixtures that exercised the larder-as-a-flow are gone with it, and the
//! priority rules they asserted are asserted here on the store that is actually spent.
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
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, ForageRegistry,
    GenerationId, GenerationRegistry, GrazeRegistry, Herd, HerdDensityMap, HerdRegistry,
    HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget,
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
/// `f32` sums of `Scalar`-quantized stores — a few ULPs of slack, no more.
const EPSILON: f32 = 1e-5;
/// A larder deep enough that *"nothing was taken from it"* is a claim with something to take. Every
/// fixture stocks it, and every fixture asserts it comes out whole.
const STOCKED_LARDER: f32 = 1_000_000.0;
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
/// No hay at all, and no Foddering to draw it with.
const NO_HAY: f32 = 0.0;

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
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
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
        upkeep_kit: None,
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

/// The share of its own fodder demand a pen actually got — grass plus hay over
/// `fodder_per_biomass × biomass`. With a barren footprint this is exactly the share the hay covered.
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

/// The fodder a pen at `biomass` demands in a turn on a barren footprint — its whole demand,
/// reconstructed from the species rate rather than read off the sim.
fn grass_demand(biomass: f32) -> f32 {
    FODDER_RATE * biomass
}

/// **NOT ONE UNIT OF `FOOD` LEFT THIS BAND'S LARDER.** Every fixture stocks the larder and calls
/// this: a pen is fed grass and hay, so however short its feed runs, the store the *people* eat from
/// is exactly where the fixture left it.
///
/// The corral harvest CREDITS food, so the assertion is one-sided by construction — the larder may
/// only go **up**. `>=` is the whole claim: a debit of any size fails it.
fn assert_larder_untouched(app: &App, keeper: Entity, context: &str) {
    let left = larder_left(app, keeper);
    assert!(
        left >= STOCKED_LARDER - EPSILON,
        "{context}: the keeper's larder is not feed — it held {STOCKED_LARDER} and must still hold \
         at least that (got {left})"
    );
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
/// The `FODDER` store holds exactly the rich pen's whole grass demand and the two pens together ask
/// for one and a half times that, so somebody goes hungry and the marks say who. The shipped
/// loop-order draw passes none of the three: it fed whichever pen's row came first, and
/// [`Arrangement::Edited`] is the case where that is the pen the player just touched.
///
/// **And the starving pen really starves.** It used to fall back on the keeper's larder and read fed;
/// with hay the only feed there is, a `Low` pen behind a `High` one on a short store gets nothing at
/// all — and the larder it would have eaten is untouched.
#[test]
fn a_high_marked_pen_is_fed_whole_and_a_low_one_starves_whatever_the_vector_says() {
    let rich_grass = grass_demand(RICH_PEN_BIOMASS);
    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, keeper) = run_two_pen_turn(
            arrangement,
            SourcePriority::High,
            SourcePriority::Low,
            true,
            rich_grass,
            STOCKED_LARDER,
        );
        let rich = fed_fraction(&app, "pen_rich");
        let lean = fed_fraction(&app, "pen_lean");
        let rich_hay = hay_drawn(&app, "pen_rich");
        let lean_hay = hay_drawn(&app, "pen_lean");
        println!("{arrangement:?}: rich {rich:.6} (hay {rich_hay:.4}), lean {lean:.6} (hay {lean_hay:.4})");
        assert!(
            (rich_hay - rich_grass).abs() < EPSILON,
            "{arrangement:?}: the High pen draws its whole shortfall off the hay store (got {rich_hay})"
        );
        assert!(
            lean_hay.abs() < EPSILON,
            "{arrangement:?}: and the Low pen draws nothing from what is left (got {lean_hay})"
        );
        assert!(
            (rich - 1.0).abs() < EPSILON,
            "{arrangement:?}: the High pen is fed whole (got {rich})"
        );
        assert!(
            lean.abs() < EPSILON,
            "{arrangement:?}: the Low pen gets NOTHING once the store is spent — no larder catches it \
             (got {lean})"
        );
        assert_larder_untouched(&app, keeper, &format!("{arrangement:?}"));
    }
}

/// **AND THE MARKS ARE WHAT DECIDES IT, NOT THE PENS.** The same fixture with the ranks swapped feeds
/// the *other* pen — so the answer above cannot be an artifact of which pen is richer or of where
/// either row sits.
#[test]
fn swapping_the_marks_swaps_which_pen_eats() {
    // The lean pen is High now, and the hay store holds exactly ITS demand.
    let (app, keeper) = run_two_pen_turn(
        Arrangement::First,
        SourcePriority::Low,
        SourcePriority::High,
        true,
        grass_demand(LEAN_PEN_BIOMASS),
        STOCKED_LARDER,
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
    assert_larder_untouched(&app, keeper, "swapped marks");
}

/// **TWO PENS ON THE SAME MARK SPLIT A SHORT STORE IN PROPORTION TO WHAT THEY ASKED FOR** — which is
/// what makes the settlement need no second ordering rule at all, and therefore nothing a vector
/// position could decide.
///
/// The hay store holds half the two demands together. Each pen must come out at half its own demand —
/// so the rich pen draws twice the hay the lean one does while both read the *same* fed fraction, and
/// neither reads `0` or `1`.
#[test]
fn two_equally_ranked_pens_split_a_short_hay_store_in_proportion_to_demand() {
    const HALF: f32 = 0.5;
    let rich_grass = grass_demand(RICH_PEN_BIOMASS);
    let lean_grass = grass_demand(LEAN_PEN_BIOMASS);
    let (app, keeper) = run_two_pen_turn(
        Arrangement::Last,
        SourcePriority::Normal,
        SourcePriority::Normal,
        true,
        (rich_grass + lean_grass) * HALF,
        STOCKED_LARDER,
    );
    let rich_hay = hay_drawn(&app, "pen_rich");
    let lean_hay = hay_drawn(&app, "pen_lean");
    let rich = fed_fraction(&app, "pen_rich");
    let lean = fed_fraction(&app, "pen_lean");
    println!(
        "proportional: rich {rich:.6} (hay {rich_hay:.4}), lean {lean:.6} (hay {lean_hay:.4})"
    );
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
    assert!(
        (rich - HALF).abs() < EPSILON && (lean - HALF).abs() < EPSILON,
        "and both are fed the SAME fraction of their own demands (rich {rich}, lean {lean})"
    );
    assert!(
        rich > EPSILON && rich < 1.0 - EPSILON && lean > EPSILON && lean < 1.0 - EPSILON,
        "NEITHER pen gets everything and neither is starved out: rich {rich}, lean {lean}"
    );
    assert_larder_untouched(&app, keeper, "equal marks");
}

/// **WITHOUT FODDERING NOTHING FEEDS THEM AT ALL.** A faction that has not learned to hay a herd bids
/// `0` into the settlement, so a full `FODDER` store is untouched — and on a barren footprint that
/// leaves both pens with **no feed whatsoever**.
///
/// This is the fix stated at its plainest. The same fixture used to assert *"both pens fall back on
/// the larder exactly as a pasture-only pen always did"* and *"the whole debit is bread": a band that
/// could not use its hay fed its animals its people's food instead. Now the hay pile and the larder
/// both come out whole and the herds shrink.
#[test]
fn a_faction_without_foddering_draws_no_hay_and_its_pens_go_unfed() {
    let (app, keeper) = run_two_pen_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        false,
        AMPLE_HAY,
        STOCKED_LARDER,
    );
    for id in ["pen_rich", "pen_lean"] {
        assert!(
            hay_drawn(&app, id).abs() < EPSILON,
            "{id} drew hay with no Foddering"
        );
        assert!(
            fed_fraction(&app, id).abs() < EPSILON,
            "{id} is fed NOTHING — a barren footprint and undrawable hay leave no feed, and the \
             larder is not a fallback (got {})",
            fed_fraction(&app, id)
        );
    }
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
    assert_larder_untouched(&app, keeper, "no Foddering");
}

/// **THE STRONGEST STATEMENT OF THE RULE: A BAND'S LARDER IS UNTOUCHED BY ITS PENS, TURN AFTER TURN.**
///
/// Two pens on a barren footprint with no hay at all — the maximum possible feed shortfall — and a
/// larder deep enough to have paid every bill many times over. Driven for several passes so a debit
/// that only lands on, say, the turn a pen first goes hungry cannot hide.
///
/// **It is not vacuous**: the pens really are demanding feed (`grass_demand > 0`) and really are going
/// without (`fed_fraction == 0`), which is exactly the state in which the retired `settle_pen_larder`
/// would have drawn `upkeep_per_biomass × biomass` out of this store every single turn.
#[test]
fn a_bands_larder_is_untouched_by_its_pens_across_repeated_turns() {
    const TURNS: u32 = 5;

    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_two_pens(&mut app, tile, RICH_PEN_BIOMASS, LEAN_PEN_BIOMASS);
    let keeper = spawn_keeper(
        &mut app,
        vec![
            hunt_row("pen_rich", SourcePriority::Normal, KEEPER_WORKERS),
            hunt_row("pen_lean", SourcePriority::Normal, KEEPER_WORKERS),
        ],
        tile,
    );

    for turn in 0..TURNS {
        // Re-pose the barren footprint and re-stock both stores each pass: `advance_labor_allocation`
        // is the only system driven here, so nothing else would restore them, and pinning the larder
        // to a known figure is what makes "it did not move" measurable rather than inferred.
        stock(&mut app, keeper, NO_HAY, STOCKED_LARDER);
        pose_footprints(&mut app, BARREN);
        app.world.run_system_once(advance_labor_allocation);

        for id in ["pen_rich", "pen_lean"] {
            assert!(
                fed_fraction(&app, id).abs() < EPSILON,
                "turn {turn}: {id} must be genuinely unfed, or this fixture proves nothing (got {})",
                fed_fraction(&app, id)
            );
        }
        assert_larder_untouched(&app, keeper, &format!("turn {turn}"));
    }

    // And the demand really was positive throughout — the assertion above is about a pen that WANTED
    // feed, not about a pen with nothing left to feed.
    assert!(
        grass_demand(LEAN_PEN_BIOMASS) > 0.0,
        "the leaner pen demands real fodder ({})",
        grass_demand(LEAN_PEN_BIOMASS)
    );
}
