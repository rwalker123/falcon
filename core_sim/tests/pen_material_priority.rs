//! **THE MATERIAL UPKEEP SPLIT IS A SETTLEMENT, NOT A WALK DOWN A VECTOR**
//! (`docs/plan_standing_upkeep.md` §2.7 / §4.9 item 12).
//!
//! The material half of the standing upkeep is the second store a band's holdings draw on every turn,
//! and it goes through the **same** `settle_scarce_store` the hay does — one pass before the
//! assignment loop, `SourcePriority` in full and then proportionally within a tier. This file is the
//! material twin of `pen_feed_priority.rs`, and it drives the identical three arrangements for the
//! identical reason:
//!
//! > **A DRAW IN ITERATION ORDER PASSES THE FIRST ARRANGEMENT.** The row that happens to sit first
//! > eats and the last starves; and because `LaborAllocation::set_assignment` removes an edited row
//! > and re-pushes it at the **END**, the holding the player has just touched is the one served last.
//! > A fixture with a single layout therefore passes on the defect half the time, which is exactly
//! > why all three are swept.
//!
//! ⛔ **AND `upkeep_mode` IS NOT READ HERE.** The rank is the player's own per-row answer; the fund
//! mode exists for a **pool** that has none, and reading both would let a row marked `High` starve
//! with nothing on screen saying why. Every fixture below leaves the mode alone and varies only the
//! marks and the vector.
//!
//! Deterministic (a pinned map seed, no `Date`/rand), following `pen_feed_priority.rs`'s fixture
//! shape: the pens are posed directly so the bill is charged on exactly the biomass seated.

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
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MaterialsConfig, MoraleCause,
    PopulationCohort, RecipesConfig, SimulationConfig, SimulationTick, SizeClass,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, SourcePriority, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry,
    WellbeingConfigHandle,
};

const MAP_SEED: u64 = core_sim::HARNESS_MAP_SEED;
/// A big head-count so the **work** half of the keeping is never what binds — the split under test is
/// about the material store.
const KEEPER_WORKERS: u32 = 5000;
/// The pen species' metabolic demand, its wild breeding rate and its body mass — a rabbit-class
/// fixture, matching `pen_feed_priority.rs`.
const FODDER_RATE: f32 = 0.10;
const WILD_R: f32 = 0.35;
const PEN_BODY_MASS: f32 = 2.0;
const PEN_CAPACITY: f32 = 5_000.0;
/// The two pens' standing biomass. The material rate is `scaled_by: source_load`, so the **bigger**
/// herd owes strictly more of the good — which is what makes the proportional split measurable.
const BIG_PEN_BIOMASS: f32 = 2_000.0;
const SMALL_PEN_BIOMASS: f32 = 1_000.0;
/// The default escapement floor a keeper row carries.
const SUSTAIN: f32 = 0.5;
const EPSILON: f32 = 1e-4;

/// The material the `animal:pen` rung eats, on both its build pile and its upkeep rate.
const PEN_MATERIAL: &str = "hurdles";

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

/// A land tile with pasture on it, so a seated pen is a pen a real map could carry.
fn pen_tile(app: &App) -> UVec2 {
    app.world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
        .0
}

/// Seat two penned, domesticated herds on `tile`, clearing whatever worldgen left.
fn seat_two_pens(app: &mut App, tile: UVec2) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    for (id, biomass) in [
        ("pen_big", BIG_PEN_BIOMASS),
        ("pen_small", SMALL_PEN_BIOMASS),
    ] {
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

fn hunt_row(herd_id: &str, priority: SourcePriority) -> LaborAssignment {
    LaborAssignment {
        target: LaborTarget::Hunt {
            fauna_id: herd_id.to_string(),
            floor: SUSTAIN,
        },
        workers: KEEPER_WORKERS,
        kit: None,
        priority,
    }
}

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
            LaborAllocation {
                assignments,
                ..Default::default()
            },
            core_sim::ResidentBand,
            StartingUnit {
                kind: "BandKeeper".to_string(),
                tags: Vec::new(),
            },
        ))
        .id()
}

/// Stock the keeper with exactly `units` of the pen's material, at the reading the shipped recipe
/// stamps its output with — so the batch is the one a bench would have made.
fn stock_material(app: &mut App, keeper: Entity, units: f32) {
    let materials = MaterialsConfig::builtin();
    let recipes = RecipesConfig::builtin();
    let characteristics = recipes
        .recipes()
        .find_map(|(_, recipe)| {
            recipe
                .outputs
                .iter()
                .find(|output| output.material_id() == Some(PEN_MATERIAL))
                .map(|output| output.characteristics.clone())
        })
        .expect("the shipped book makes the pen's material");
    let band = materials
        .band_key(PEN_MATERIAL, &characteristics)
        .expect("the shipped roster rates the pen's material");
    let mut cohort = app.world.get_mut::<PopulationCohort>(keeper).unwrap();
    cohort
        .stores
        .deposit_material(PEN_MATERIAL, band, scalar_from_f32(units), &characteristics);
}

/// What one pen was **billed** in the good this turn — the stamped bill, which is what the keeping is
/// judged against.
fn billed(app: &App, herd_id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("the pen is still seated")
        .upkeep_materials_demanded
        .get(PEN_MATERIAL)
        .copied()
        .unwrap_or(0.0)
}

/// What the store actually **paid** toward that bill.
fn paid(app: &App, herd_id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("the pen is still seated")
        .upkeep_materials_supplied
        .get(PEN_MATERIAL)
        .copied()
        .unwrap_or(0.0)
}

/// **THE ARRANGEMENT AXIS** — how the two rows are laid out before the turn runs. All three describe
/// the *same* two pens with the *same* two marks; only the vector differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrangement {
    /// The big pen's row first.
    First,
    /// The big pen's row last.
    Last,
    /// The big pen's row edited through `set_assignment`, which re-pushes it to the END of the vector
    /// — the arrangement a positional draw was guaranteed to get wrong, because a stepper press moved
    /// the holding the player had just adjusted to the back of the line.
    Edited,
}

fn edit_the_big_row(app: &mut App, keeper: Entity, rank: SourcePriority) {
    let mut allocation = app.world.get_mut::<LaborAllocation>(keeper).unwrap();
    allocation.set_assignment(
        LaborTarget::Hunt {
            fauna_id: "pen_big".to_string(),
            floor: SUSTAIN,
        },
        KEEPER_WORKERS,
        KEEPER_WORKERS,
        None,
    );
    assert_eq!(
        allocation.assignments.last().map(|row| row.priority),
        Some(rank),
        "the edit re-pushed the big pen's row to the end AND kept its mark — if either half of this \
         fails the arrangement is not the one the test means to drive"
    );
}

/// Drive one real turn on two pens holding `units` of the good, laying the rows out as `arrangement`
/// says and marking the big pen `big_rank` and the small one `small_rank`.
fn run_two_pen_turn(
    arrangement: Arrangement,
    big_rank: SourcePriority,
    small_rank: SourcePriority,
    units: f32,
) -> (App, Entity) {
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_two_pens(&mut app, tile);
    let rows = match arrangement {
        Arrangement::First | Arrangement::Edited => vec![
            hunt_row("pen_big", big_rank),
            hunt_row("pen_small", small_rank),
        ],
        Arrangement::Last => vec![
            hunt_row("pen_small", small_rank),
            hunt_row("pen_big", big_rank),
        ],
    };
    let keeper = spawn_keeper(&mut app, rows, tile);
    if arrangement == Arrangement::Edited {
        edit_the_big_row(&mut app, keeper, big_rank);
    }
    stock_material(&mut app, keeper, units);
    app.world.run_system_once(advance_labor_allocation);
    (app, keeper)
}

/// **THE BILL EACH PEN WOULD BE HANDED WITH THE STORE FULL** — the denominator every fraction below
/// is read against, taken from a run the store cannot bind.
fn full_bills() -> (f32, f32) {
    const AMPLE: f32 = 10_000.0;
    let (app, _) = run_two_pen_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        AMPLE,
    );
    (billed(&app, "pen_big"), billed(&app, "pen_small"))
}

/// ⛔ **THE HIGH-MARKED PEN IS PAID IN FULL AND THE LOW ONE GETS NOTHING — IN ALL THREE
/// ARRANGEMENTS.**
///
/// The store holds exactly the big pen's whole bill and the two together ask for more, so somebody
/// goes unpaid and the marks say who. **A draw in iteration order passes the first arrangement and
/// fails the other two**, which is the whole reason the sweep exists.
#[test]
fn a_high_marked_pen_is_paid_whole_and_a_low_one_gets_nothing_whatever_the_vector_says() {
    let (big_bill, small_bill) = full_bills();
    assert!(
        big_bill > 0.0 && small_bill > 0.0 && big_bill > small_bill,
        "fixture: both pens must owe the good and the bigger herd must owe MORE, or the split has \
         nothing to order (big {big_bill}, small {small_bill})"
    );

    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, _) = run_two_pen_turn(
            arrangement,
            SourcePriority::High,
            SourcePriority::Low,
            big_bill,
        );
        let big = paid(&app, "pen_big");
        let small = paid(&app, "pen_small");
        println!("{arrangement:?}: big paid {big:.6}, small paid {small:.6}");
        assert!(
            (big - big_bill).abs() < EPSILON,
            "{arrangement:?}: the High pen's whole bill is paid off the store (got {big} of \
             {big_bill})"
        );
        assert!(
            small.abs() < EPSILON,
            "{arrangement:?}: and the Low pen gets NOTHING from what is left (got {small})"
        );
    }
}

/// **AND THE MARKS ARE WHAT DECIDES IT, NOT THE PENS.** The same fixture with the ranks swapped pays
/// the *other* pen — so the answer above cannot be an artifact of which herd is bigger or of where
/// either row sits.
#[test]
fn swapping_the_marks_swaps_which_pen_is_paid() {
    let (_, small_bill) = full_bills();
    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, _) = run_two_pen_turn(
            arrangement,
            SourcePriority::Low,
            SourcePriority::High,
            small_bill,
        );
        let big = paid(&app, "pen_big");
        let small = paid(&app, "pen_small");
        assert!(
            (small - small_bill).abs() < EPSILON,
            "{arrangement:?}: the High pen is the SMALL one now, and it is paid whole (got {small} \
             of {small_bill})"
        );
        assert!(
            big.abs() < EPSILON,
            "{arrangement:?}: …and the Low pen — the bigger herd, first in the vector on one of \
             these arrangements — gets nothing (got {big})"
        );
    }
}

/// **TWO EQUALLY RANKED PENS SPLIT A SHORT STORE IN PROPORTION TO WHAT THEY ASKED FOR** — which is
/// what makes the settlement need no second ordering rule at all, and therefore leaves nothing for a
/// vector position to decide.
#[test]
fn two_equally_ranked_pens_split_a_short_store_in_proportion_to_demand() {
    /// The share of the two bills together the store can cover.
    const COVERAGE: f32 = 0.5;

    let (big_bill, small_bill) = full_bills();
    let store = (big_bill + small_bill) * COVERAGE;
    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, _) = run_two_pen_turn(
            arrangement,
            SourcePriority::Normal,
            SourcePriority::Normal,
            store,
        );
        let big = paid(&app, "pen_big");
        let small = paid(&app, "pen_small");
        assert!(
            (big - big_bill * COVERAGE).abs() < EPSILON,
            "{arrangement:?}: the bigger pen gets its own share of a short store (got {big} against \
             {big_bill} × {COVERAGE})"
        );
        assert!(
            (small - small_bill * COVERAGE).abs() < EPSILON,
            "{arrangement:?}: …and so does the smaller (got {small} against {small_bill} × \
             {COVERAGE})"
        );
        assert!(
            big < big_bill - EPSILON && small > EPSILON,
            "**LIVENESS**: NEITHER pen may be paid whole and neither may get nothing, or the \
             proportional arm is not the one under test (big {big}, small {small})"
        );
    }
}

/// **THE RATE SCALES WITH THE HERD, and the bill says so** — `scaled_by: source_load` reads the same
/// keeper load the work term does, so a pen holding twice the herd mends twice the fence.
///
/// Asserted on the **stamped bill** off a real turn, against the two herds' own biomass ratio, so it
/// is a claim about what the sim charged rather than about the seam it charged through.
#[test]
fn a_pen_holding_twice_the_herd_is_billed_twice_the_material() {
    let (big_bill, small_bill) = full_bills();
    let herds = BIG_PEN_BIOMASS / SMALL_PEN_BIOMASS;
    assert!(
        (big_bill / small_bill - herds).abs() < EPSILON,
        "the material bill is linear in the herd, exactly as the work bill is: {big_bill} / \
         {small_bill} against {herds}"
    );
}
