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
    // **The road ledger `advance_labor_allocation` counts spare road keepers against.** Empty
    // is the shipped turn-1 state — no traffic has worn anything in yet — so this harness's
    // keeping numbers are the roadless reading they have always been.
    app.world.insert_resource(core_sim::RoadRegistry::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
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
        upkeep_kit: None,
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
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
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

// ---------------------------------------------------------------------------------------------
// **A RING IS A PEN BUILD, AND IT EATS THE SAME PILE**
// ---------------------------------------------------------------------------------------------
//
// Widening a fence is the same fencing labour on the same `animal:pen` record — the same
// `build_cost`, the same builders pool, the same keepers' tools — so it draws the same pile, in
// proportion to the work banked, off the **same** `settle_material_upkeep` pass at the source row's
// own `SourcePriority`.
//
// ⛔ It was materially free for one turn of this arc's life: `source_banking_its_first_work` answers
// a **verb** question and a ring names no verb, so nothing bid for one and it was quoted at full
// coverage. The claim is laid by `head_ring_leg` now, off the ring's own `pen_extending` gate.

/// A builders pool small enough that one turn banks a **fraction** of the ring — a crew that closed
/// the fence in a turn would draw the whole pile and leave no proportion to measure.
const RING_BUILDERS: u32 = 25;

/// How many times over the band is sized against the rows it staffs, so `normalize` sheds nobody and
/// every arrangement drives the **same** crew.
const BAND_HEADROOM: u32 = 2;

/// A store no claim on it can bind — so a run measures the ring rather than the shelf.
const AMPLE: f32 = 10_000.0;

/// **THE SLACK A RESTOCKED STORE COSTS, in work units.** A `LocalStore` holds fixed-point `Scalar`
/// batches, so a fixture that stocks *a measured fraction of a measured demand* quantises twice —
/// once stating the amount and once reading it back — and the ring's banked work inherits that as a
/// relative error. It is ~5e-4 at this fixture's scale; [`EPSILON`] is the right bar for the exact
/// arithmetic and this is the right one for a claim routed through the shelf.
const STORE_QUANTUM_SLACK: f32 = 1e-2;

/// The `animal:pen` rung's work cost — what a pen costs and, because the two are one record, what a
/// ring costs.
fn pen_build_cost() -> f32 {
    core_sim::LadderConfig::builtin()
        .rung(core_sim::RungKey::AnimalPen)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the pen rung has a build meter")
}

/// The `animal:pen` rung's build **pile** of the good — the whole amount raising it swallows.
fn pen_build_pile() -> f32 {
    core_sim::LadderConfig::builtin()
        .rung(core_sim::RungKey::AnimalPen)
        .build_materials()
        .find(|(id, _)| *id == PEN_MATERIAL)
        .map(|(_, amount)| amount)
        .expect("the shipped pen rung declares the good on its pile")
}

/// The empty kit, so the pace under test is the pool's own and no start-stocked tool moves it.
fn bare_builders() -> core_sim::KitChoice {
    core_sim::EquipmentConfig::builtin()
        .kit("none")
        .expect("the shipped roster carries the empty kit")
}

/// Staff the band's `builders` row at [`RING_BUILDERS`] and queue `job` on `source` with a bare kit —
/// the sim half of what `handle_extend_pen` and a `corral` order each do.
fn queue_a_build(
    app: &mut App,
    keeper: Entity,
    source: core_sim::BuildSource,
    job: core_sim::BuildJob,
) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(keeper)
        .expect("the keeper band keeps its allocation");
    allocation.assignments.push(LaborAssignment {
        target: LaborTarget::Builders,
        workers: RING_BUILDERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    });
    assert!(
        allocation.enqueue_build(source.clone(), job),
        "the keeper band works the source it is building"
    );
    assert!(
        allocation.set_build_entry_kit(&source, Some(bare_builders())),
        "the entry just declared takes the bare kit"
    );
    // ⛔ **THE BUILDERS ROW SPENDS THE SAME POOL THE KEEPERS DO.** `normalize` sheds crews until
    // `Σ assignments ≤ available`, so a band sized for its hunt rows alone has this row trimmed to a
    // hand or two, and *which* rows it takes them from depends on the vector — so the arrangement
    // sweep would be varying the crew as well as the layout. Widen the band past every row it was
    // just handed, with room to spare: `available_workers` floors a fixed-point `Scalar`, so a band
    // sized to the nose loses a hand to the rounding alone.
    let staffed: u32 = allocation
        .assignments
        .iter()
        .map(|assignment| assignment.workers)
        .sum();
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(keeper)
        .expect("the keeper band persists");
    cohort.working = scalar_from_f32((staffed * BAND_HEADROOM) as f32);
}

/// Put a ring in flight on `herd_id` and staff it.
fn begin_a_ring(app: &mut App, keeper: Entity, herd_id: &str) {
    let radius_max = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .husbandry
        .pen_radius_max;
    let began = app
        .world
        .resource_mut::<HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == herd_id)
        .expect("the pen is seated")
        .begin_pen_extension(radius_max);
    assert!(began, "a built pen below the radius cap may begin a ring");
    queue_a_build(
        app,
        keeper,
        core_sim::BuildSource::Herd(herd_id.to_string()),
        core_sim::BuildJob::ExtendPen,
    );
}

/// Seat one **tamed but unfenced** herd on `tile`, clearing whatever worldgen left — the source a
/// fresh `Corral` is raised on, against which the ring is measured.
fn seat_a_pastoral_herd(app: &mut App, tile: UVec2) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    let mut herd = Herd::new(
        "pen_fresh".to_string(),
        "Fixture pen_fresh".to_string(),
        SizeClass::Small,
        vec![tile],
        BIG_PEN_BIOMASS,
        PEN_CAPACITY,
        FODDER_RATE,
        WILD_R,
        PEN_BODY_MASS,
    );
    herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    registry.herds.push(herd);
}

/// What the band still holds of the good.
fn store_holds(app: &App, keeper: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(keeper)
        .expect("the keeper band persists")
        .stores
        .material_total(PEN_MATERIAL)
        .to_f32()
}

/// How far the ring got this turn.
fn ring_banked(app: &App, herd_id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("the pen is still seated")
        .pen_extend_progress
}

/// Drive one real turn with a ring in flight on the big pen, laid out as `arrangement` says.
fn run_ring_turn(
    arrangement: Arrangement,
    ring_rank: SourcePriority,
    other_rank: SourcePriority,
    units: f32,
) -> (App, Entity) {
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_two_pens(&mut app, tile);
    let rows = match arrangement {
        Arrangement::First | Arrangement::Edited => vec![
            hunt_row("pen_big", ring_rank),
            hunt_row("pen_small", other_rank),
        ],
        Arrangement::Last => vec![
            hunt_row("pen_small", other_rank),
            hunt_row("pen_big", ring_rank),
        ],
    };
    let keeper = spawn_keeper(&mut app, rows, tile);
    begin_a_ring(&mut app, keeper, "pen_big");
    if arrangement == Arrangement::Edited {
        edit_the_big_row(&mut app, keeper, ring_rank);
    }
    stock_material(&mut app, keeper, units);
    app.world.run_system_once(advance_labor_allocation);
    (app, keeper)
}

/// What the ring itself took off the store — the turn's whole fall, less the two pens' stamped
/// upkeep, which is the only other claim in the fixture.
fn ring_pile_drawn(app: &App, keeper: Entity, stocked: f32) -> f32 {
    stocked - store_holds(app, keeper) - paid(app, "pen_big") - paid(app, "pen_small")
}

/// ⛔ **A RING DRAWS THE PEN'S OWN PILE, IN PROPORTION TO THE WORK IT BANKED.**
///
/// `pile × banked / cost` — the same expression a rung's leg is spread by, on the same rung record.
/// Before the fix the ring's term was **zero**: widening a pen was materially free while raising one
/// cost the whole pile.
#[test]
fn a_ring_draws_the_pens_own_build_pile_in_proportion_to_the_work_banked() {
    let (app, keeper) = run_ring_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        AMPLE,
    );
    let banked = ring_banked(&app, "pen_big");
    let cost = pen_build_cost();
    assert!(
        banked > 0.0 && banked < cost - EPSILON,
        "fixture: the ring must bank a FRACTION of its span this turn, or there is no proportion to \
         measure (banked {banked} of {cost})"
    );
    let expected = pen_build_pile() * banked / cost;
    assert!(
        expected > EPSILON,
        "**LIVENESS**: the expected draw must be a real quantity, or this test passes on a ring \
         that draws nothing — which is the defect it exists to catch (got {expected})"
    );
    let drawn = ring_pile_drawn(&app, keeper, AMPLE);
    assert!(
        (drawn - expected).abs() < EPSILON,
        "the ring ate the pen's own pile at the work it banked: drew {drawn} against {} × {banked} \
         / {cost}",
        pen_build_pile()
    );
}

/// **A SHORT STORE STALLS THE RING IN PROPORTION, AND A DRY ONE BLOCKS IT** — the same
/// `build_coverage` scaling and the same `BuildGate::Materials` cause a rung build already gets.
#[test]
fn a_short_store_stalls_the_ring_in_proportion_and_a_dry_one_blocks_it() {
    /// The share of the turn's whole material demand the short store can cover.
    const HALF: f32 = 0.5;

    let (full, keeper) = run_ring_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        AMPLE,
    );
    let banked_full = ring_banked(&full, "pen_big");
    assert!(
        banked_full > EPSILON,
        "**LIVENESS**: the well-stocked run must bank real work, or every fraction below is a \
         fraction of nothing (got {banked_full})"
    );
    let demanded = AMPLE - store_holds(&full, keeper);

    // --- half the demand: every claim sits in one tier, so each is served the same fraction ---
    let (short, _) = run_ring_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        demanded * HALF,
    );
    let banked_short = ring_banked(&short, "pen_big");
    assert!(
        (banked_short - banked_full * HALF).abs() < STORE_QUANTUM_SLACK,
        "a store covering half the turn's panels banks half the ring's work: {banked_short} against \
         {banked_full} × {HALF}"
    );

    // --- nothing at all: the ring stalls outright, and says the store is why ---
    let (dry, _) = run_ring_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        0.0,
    );
    assert!(
        ring_banked(&dry, "pen_big").abs() < EPSILON,
        "a dry store banks NOTHING on the ring (got {})",
        ring_banked(&dry, "pen_big")
    );
    assert_eq!(
        dry.world
            .resource::<HerdRegistry>()
            .find("pen_big")
            .expect("the pen is still seated")
            .build_blocked_reason,
        core_sim::BuildGate::Materials,
        "…and it publishes the GOOD as the cause — *raise this band's Builders role* is wrong \
         advice the moment the missing thing is hurdles"
    );
}

/// ⛔ **THE RING'S CLAIM OBEYS THE MARKS AND NOT THE VECTOR — IN ALL THREE ARRANGEMENTS.**
///
/// A ring taking what it wants off the store in iteration order is the exact positional defect this
/// arc spent a slice removing, and a fixture with one layout passes on it half the time.
#[test]
fn the_rings_claim_is_settled_by_the_marks_whatever_the_vector_says() {
    let (full, keeper) = run_ring_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        AMPLE,
    );
    let banked_full = ring_banked(&full, "pen_big");
    // The store holds the ring's own pile and the big pen's own bill and nothing more, so the two
    // claims on that row are covered whole and the other pen goes unpaid — if the marks say so.
    let store = ring_pile_drawn(&full, keeper, AMPLE) + paid(&full, "pen_big");
    assert!(
        store < AMPLE - store_holds(&full, keeper) - EPSILON,
        "fixture: the store must be short of the whole turn's demand, or the marks decide nothing"
    );

    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, _) = run_ring_turn(
            arrangement,
            SourcePriority::High,
            SourcePriority::Low,
            store,
        );
        assert!(
            (ring_banked(&app, "pen_big") - banked_full).abs() < STORE_QUANTUM_SLACK,
            "{arrangement:?}: the High-marked ring is covered whole and banks its full turn (got \
             {} against {banked_full})",
            ring_banked(&app, "pen_big")
        );
        assert!(
            paid(&app, "pen_small").abs() < EPSILON,
            "{arrangement:?}: …and the Low-marked pen gets nothing from what is left (got {})",
            paid(&app, "pen_small")
        );
    }

    // **AND THE MARKS ARE WHAT DECIDES IT.** Swapped, the ring is the one that goes short.
    for arrangement in [Arrangement::First, Arrangement::Last, Arrangement::Edited] {
        let (app, _) = run_ring_turn(
            arrangement,
            SourcePriority::Low,
            SourcePriority::High,
            store,
        );
        assert!(
            ring_banked(&app, "pen_big") < banked_full - STORE_QUANTUM_SLACK,
            "{arrangement:?}: a Low-marked ring is out-ranked by the pens above it and stalls (got \
             {} against {banked_full})",
            ring_banked(&app, "pen_big")
        );
    }
}

/// ⛔ **A RING AND A FRESH PEN DRAW THE SAME PANELS FOR THE SAME WORK BANKED.**
///
/// The whole of the ruling this section exists for: *the extension is just a pen build, nothing
/// special about it other than that it is connected to the one next to it.* Driven as two real turns
/// off the same crew and the same kit, so nothing but the queue kind differs — which is what stops
/// the two drifting later.
#[test]
fn a_ring_and_a_fresh_pen_draw_the_same_pile_for_the_same_work_banked() {
    // --- the ring: a built pen being widened ---
    let (ring_app, ring_keeper) = run_ring_turn(
        Arrangement::First,
        SourcePriority::Normal,
        SourcePriority::Normal,
        AMPLE,
    );
    let ring_work = ring_banked(&ring_app, "pen_big");
    let ring_pile = ring_pile_drawn(&ring_app, ring_keeper, AMPLE);

    // --- the fresh pen: a tamed herd raising its first fence, same crew, same kit ---
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_a_pastoral_herd(&mut app, tile);
    let keeper = spawn_keeper(
        &mut app,
        vec![hunt_row("pen_fresh", SourcePriority::Normal)],
        tile,
    );
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FactionId(0), core_sim::PENNING_DISCOVERY_ID, scalar_one());
    queue_a_build(
        &mut app,
        keeper,
        core_sim::BuildSource::Herd("pen_fresh".to_string()),
        core_sim::BuildJob::Rung(core_sim::Improvement::Corral),
    );
    stock_material(&mut app, keeper, AMPLE);
    app.world.run_system_once(advance_labor_allocation);
    let build_work = app
        .world
        .resource::<HerdRegistry>()
        .find("pen_fresh")
        .expect("the herd persists")
        .rung_work_done(
            core_sim::RungKey::AnimalPen,
            &core_sim::LadderConfig::builtin(),
        );
    // A herd below the pen rung owes no material RATE, so the whole of the store's fall is the pile.
    assert!(
        paid(&app, "pen_fresh").abs() < EPSILON,
        "fixture: a pastoral herd names no material on its upkeep, so the store's fall is the pile \
         alone (got {} on the bill)",
        paid(&app, "pen_fresh")
    );
    let build_pile = AMPLE - store_holds(&app, keeper);

    assert!(
        (ring_work - build_work).abs() < EPSILON,
        "**FIXTURE**: the two crews must bank the same work, or the comparison below is not about \
         the pile (ring {ring_work}, pen {build_work})"
    );
    assert!(
        ring_pile > EPSILON && build_pile > EPSILON,
        "**LIVENESS**: both must draw a real quantity, or this passes on the very defect it exists \
         to catch (ring {ring_pile}, pen {build_pile})"
    );
    assert!(
        (ring_pile - build_pile).abs() < EPSILON,
        "a ring and a fresh pen draw the SAME panels for the same work banked: ring {ring_pile}, \
         pen {build_pile}"
    );
}

/// **THE BENCH IS HALF THE BAND'S MATERIAL INCOME, AND THE ALERT MUST JUDGE AGAINST BOTH HALVES**
/// (`docs/plan_standing_upkeep.md` §2.7 / §4.9 item 12).
///
/// `hurdles` have **no producer but a bench** on the shipped roster, so a shortfall struck against
/// the *credited take* alone sees zero income for ever, calls the whole pen bill a gap, and fires
/// *"Hurdles is running out"* for **every band that keeps a pen** — including one whose bench
/// out-produces its pens. The wire's `materialUpkeepIncome` row already summed both halves, so the
/// two disagreed about the same turn; `LaborAllocation::material_income` is now the one producer they
/// share.
///
/// **Paired on the bench alone.** The two arms differ in exactly one thing — whether the hurdles
/// recipe is on the bench — so an absent Alert cannot be an artifact of the store, the marks or the
/// vector. The bench arm's income is checked against the bill through the same public producer the
/// wire reads, which is what makes *"the bench covers it"* a stated state rather than a hope.
#[test]
fn a_bench_that_out_produces_the_pens_silences_the_shortfall_alert() {
    /// A store deliberately far below `5 × the bill`, which is the Alert's own *"about to run out"*
    /// window. Both arms hold it, so the shelf is not what tells them apart.
    const A_THIN_SHELF: f32 = 0.01;
    /// Hands on the bench. `70 × 1.0 progress-per-worker × 0.5 bare-handed wood ÷ 7.0 work` is
    /// **5 hurdles a turn**, which is far above what two rabbit-class pens mend — and the fixture
    /// asserts that rather than trusting it.
    const BENCH_HANDS: u32 = 70;
    /// Enough of both inputs that a pass is affordable, which is what makes the projected rate real
    /// (a bench that cannot draw promises nothing).
    const A_PASS_AND_MORE: f32 = 200.0;

    let shouting = shortfall_lines(&run_two_pen_turn_with_bench(None, A_THIN_SHELF));
    assert_eq!(
        shouting, 1,
        "fixture: a band keeping two pens on a nearly empty shelf and making nothing MUST be \
         warned, or the silent arm below proves nothing"
    );

    let (app, keeper) =
        run_two_pen_turn_with_bench(Some((BENCH_HANDS, A_PASS_AND_MORE)), A_THIN_SHELF);
    let need = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("the keeper carries an allocation")
        .last_material_need
        .get(PEN_MATERIAL)
        .copied()
        .unwrap_or(0.0);
    assert!(
        need > 0.0,
        "fixture: the pens must actually owe hurdles, or there is no gap to close"
    );
    let income = band_material_income(&app, keeper);
    assert!(
        income > need,
        "fixture: the bench must out-produce the pens, which is the state under test (income \
         {income} against a bill of {need})"
    );
    assert_eq!(
        shortfall_lines(&(app, keeper)),
        0,
        "**A BAND WHOSE BENCH OUT-PRODUCES ITS PENS IS NOT RUNNING OUT** — the Alert reads the same \
         inflow the `materialUpkeepIncome` row publishes, bench included"
    );
}

/// The band's whole per-turn material inflow, through the **public producer** the wire row uses —
/// never a re-derivation here, which is the defect this test is about.
fn band_material_income(app: &App, keeper: Entity) -> f32 {
    let recipes = app.world.resource::<core_sim::RecipesConfigHandle>().get();
    let materials = app
        .world
        .resource::<core_sim::MaterialsConfigHandle>()
        .get();
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let cohort = app
        .world
        .get::<PopulationCohort>(keeper)
        .expect("the keeper carries a cohort");
    let bench = app.world.get::<core_sim::BandBench>(keeper);
    let no_kit = core_sim::BandEquipment::default();
    let wear = app
        .world
        .get::<core_sim::BandEquipment>(keeper)
        .unwrap_or(&no_kit);
    let rate = core_sim::bench_material_rate(
        bench,
        &cohort.stores,
        &recipes,
        &materials,
        &equipment,
        wear,
    );
    app.world
        .get::<LaborAllocation>(keeper)
        .expect("the keeper carries an allocation")
        .material_income(&rate)
        .get(PEN_MATERIAL)
        .copied()
        .unwrap_or(0.0)
}

/// How many *"Hurdles is running out"* lines this turn pushed.
fn shortfall_lines(run: &(App, Entity)) -> usize {
    run.0
        .world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == core_sim::CommandEventKind::MaterialShortfall)
        .count()
}

/// [`run_two_pen_turn`] with a bench: `Some((hands, input pile))` puts the hurdles recipe on it and
/// banks both of its inputs, `None` leaves the band with no bench at all.
fn run_two_pen_turn_with_bench(bench: Option<(u32, f32)>, units: f32) -> (App, Entity) {
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_two_pens(&mut app, tile);
    let keeper = spawn_keeper(
        &mut app,
        vec![
            hunt_row("pen_big", SourcePriority::Normal),
            hunt_row("pen_small", SourcePriority::Normal),
        ],
        tile,
    );
    stock_material(&mut app, keeper, units);
    if let Some((hands, pile)) = bench {
        // **HANDS ENOUGH FOR THE PENS *AND* THE BENCH.** The two keeper rows ask for
        // `KEEPER_WORKERS` each so the *work* half of the keeping never binds; a bench beside them
        // would otherwise be trimmed to a single crafter by `LaborAllocation::normalize` and the
        // fixture would measure a bench it had starved itself.
        app.world
            .get_mut::<PopulationCohort>(keeper)
            .expect("the keeper carries a cohort")
            .working = scalar_from_f32((KEEPER_WORKERS * 2 + hands) as f32);
        deposit_input(
            &mut app,
            keeper,
            "wood",
            pile,
            &[("hardness", 0.5), ("pliancy", 0.6)],
        );
        deposit_input(
            &mut app,
            keeper,
            "hide",
            pile,
            &[("toughness", 0.5), ("suppleness", 0.6)],
        );
        let mut bench = core_sim::BandBench::default();
        bench.set_job(BENCH_RECIPE, hands);
        app.world.entity_mut(keeper).insert(bench);
    }
    app.world.run_system_once(advance_labor_allocation);
    (app, keeper)
}

/// The recipe whose only output is the material a pen eats.
const BENCH_RECIPE: &str = "hurdles";

/// Bank one of the bench's inputs at an exact per-axis reading.
fn deposit_input(app: &mut App, keeper: Entity, material: &str, amount: f32, axes: &[(&str, f32)]) {
    let readings: std::collections::BTreeMap<String, f32> = axes
        .iter()
        .map(|(axis, value)| ((*axis).to_string(), *value))
        .collect();
    let key = MaterialsConfig::builtin()
        .band_key(material, &readings)
        .expect("the shipped roster rates this material");
    let mut cohort = app.world.get_mut::<PopulationCohort>(keeper).unwrap();
    cohort
        .stores
        .deposit_material(material, key, scalar_from_f32(amount), &readings);
}

/// ⛔ **A ROW THE ARM WILL NOT REACH BIDS FOR NOTHING** (`docs/plan_standing_upkeep.md` §2.7).
///
/// The Hunt arm `continue`s past `apply_material_keeping` for a herd beyond `hunt_reach`, so a claim
/// settled for that row reserves hurdles **nothing ever spends** — and the pen that *is* in reach is
/// judged short by the difference, taking the neglect counter, the decay fraction and the shed for a
/// shortage it did not cause. `settle_pen_hay` has always filtered the hay by the same leash; the
/// material settlement now shares the rule through `BandReach`.
///
/// **The store holds exactly the in-reach pen's whole bill.** Under a leash-blind settlement the two
/// `Normal` pens split it in proportion to demand and the in-reach one is paid about half; the claim
/// here is that it is paid **whole**.
#[test]
fn a_pen_past_the_leash_reserves_nothing_from_the_store() {
    let (big_bill, small_bill) = full_bills();
    assert!(
        big_bill > 0.0 && small_bill > 0.0,
        "fixture: both pens must owe the good, or an out-of-reach claim has nothing to reserve \
         (big {big_bill}, small {small_bill})"
    );

    let (app, keeper) = run_one_pen_past_the_leash(big_bill);
    assert!(
        app.world
            .resource::<CommandEventLog>()
            .iter()
            .any(|entry| entry
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("reason=out_of_leash"))),
        "fixture: the far pen's row must actually LAPSE — if the arm reached it there is no \
         unspendable reservation to make"
    );
    assert!(
        app.world
            .get::<LaborAllocation>(keeper)
            .expect("the keeper carries an allocation")
            .assignments
            .iter()
            .any(|row| matches!(&row.target, LaborTarget::Hunt { fauna_id, .. } if fauna_id == "pen_big")),
        "fixture: the IN-reach pen's row must survive the turn, or nothing was paid at all"
    );

    let paid_in_reach = paid(&app, "pen_big");
    assert!(
        (paid_in_reach - big_bill).abs() < EPSILON,
        "**THE PEN IN REACH IS PAID WHOLE** — the store held exactly its bill, and the pen the arm \
         lapses must not have reserved a share of it (paid {paid_in_reach} of {big_bill})"
    );
    assert_eq!(
        paid(&app, "pen_small"),
        0.0,
        "…and the far pen spent nothing, which is what makes any reservation for it dead"
    );
}

/// Two `Normal` pens, one on the band's own tile and one past `hunt_reach`, with `units` of the good
/// on the shelf. The far pen is seated by hand rather than through [`seat_two_pens`], which puts both
/// on one tile.
fn run_one_pen_past_the_leash(units: f32) -> (App, Entity) {
    let mut app = base_world();
    let tile = pen_tile(&app);
    seat_two_pens(&mut app, tile);
    let far = {
        let reach = app.world.resource::<LaborConfigHandle>().get().hunt_reach();
        let far = UVec2::new(tile.x + reach + 1, tile.y);
        assert!(
            app.world
                .resource::<TileRegistry>()
                .index(far.x, far.y)
                .is_some(),
            "fixture: the far tile must be on the map ({far:?})"
        );
        far
    };
    {
        let ladder = core_sim::LadderConfig::builtin();
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == "pen_small")
            .expect("the fixture seated both pens");
        assert!(
            herd.corral_at(far, &ladder),
            "the fixture species must be pennable"
        );
    }
    let keeper = spawn_keeper(
        &mut app,
        vec![
            hunt_row("pen_big", SourcePriority::Normal),
            hunt_row("pen_small", SourcePriority::Normal),
        ],
        tile,
    );
    stock_material(&mut app, keeper, units);
    app.world.run_system_once(advance_labor_allocation);
    (app, keeper)
}
