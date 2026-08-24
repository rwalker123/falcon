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

/// Seat TWO penned, domesticated herds on `tile`, clearing whatever worldgen left. Both are corralled
/// where the keeper stands, so both are inside the hunt leash and the only thing separating them is
/// the mark their row carries.
fn seat_two_pens(app: &mut App, tile: UVec2, rich_biomass: f32, lean_biomass: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    for (id, biomass) in [("pen_rich", rich_biomass), ("pen_lean", lean_biomass)] {
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

/// A keeper band standing on `tile` holding one Hunt row per pen, in the order given — which is the
/// axis these tests vary.
fn spawn_keeper(app: &mut App, rows: &[(&str, SourcePriority)], tile: UVec2) -> Entity {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("pen tile resolves");
    let assignments = rows
        .iter()
        .map(|(herd_id, priority)| LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: (*herd_id).to_string(),
                floor: SUSTAIN,
            },
            workers: KEEPER_WORKERS,
            kit: None,
            priority: *priority,
        })
        .collect();
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
    let rows: Vec<(&str, SourcePriority)> = match arrangement {
        Arrangement::First | Arrangement::Edited => {
            vec![("pen_rich", rich_rank), ("pen_lean", lean_rank)]
        }
        Arrangement::Last => vec![("pen_lean", lean_rank), ("pen_rich", rich_rank)],
    };
    let keeper = spawn_keeper(&mut app, &rows, tile);
    if arrangement == Arrangement::Edited {
        // The `−`/`+` the player presses on the rich pen's row: `set_assignment` drops the row and
        // re-pushes it at the END, so this is the arrangement in which the shipped code fed it last.
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
