//! **The deposit, the source and the take** — `docs/plan_extraction.md`, issue #583.
//!
//! Everything here exists to pin one sentence: **a deposit is a stock with a capacity and a
//! regrowth rate, and rock's rate is zero.** The two halves of that are asserted against each other
//! rather than separately — a wood that recovers beside a quarry that cannot, on the same code path,
//! with no branch anywhere between them.

use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, World};

use core_sim::extraction::{
    deposit_payoff, deposit_regrowth, deposit_standing, take_from_deposit, tile_deposit_capacity,
    tile_deposit_regrowth, DepositRegistry, DepositSource,
};
use core_sim::{
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, CommandEventLog,
    DiscoveryProgressLedger, ExtractionConfig, ExtractionConfigHandle, FactionId, FactionInventory,
    FoodSiteRegistry, ForageRegistry, GenerationId, HerdRegistry, LaborAllocation, LaborAssignment,
    LaborConfigHandle, LaborTarget, LadderConfig, LadderConfigHandle, LocalStore, MoraleCause,
    PopulationCohort, RungBranch, RungKey, SimulationConfig, SimulationTick, SourcePriority, Tile,
    TileRegistry,
};
use sim_schema::TerrainType;

/// The wooded ground the take fixtures stand on. It carries **both** deposits — 600 wood and 35
/// stone in the shipped table — which is what lets one tile exercise the *"one tile can hold two"*
/// key without a second fixture.
const WOODED: TerrainType = TerrainType::MixedWoodland;
/// A real rock body: the largest capacity on the table, at a regrowth rate of zero.
const ROCK: TerrainType = TerrainType::AlpineMountain;
/// **Ground that holds nothing at all** — absent from both `by_terrain` tables, which is how the
/// deposits table says *"there is no deposit here"*.
const BARREN: TerrainType = TerrainType::Glacier;

const WOOD: &str = "wood";
const STONE: &str = "stone";

/// The faction every fixture band belongs to.
const FACTION: FactionId = FactionId(0);

/// **THE ESCAPEMENT FLOOR THAT LEAVES THE RUNG'S OWN FLOOR IN CHARGE** — `0`, the identity of the
/// `max` in `extraction::deposit_effective_floor`.
///
/// Every fixture below this line predates the player's dial (#650) and is about the *rung's* reach,
/// so it passes the identity and reads exactly as it read before. Named rather than written `0.0`,
/// because a bare zero in that argument reads as *"strip it bare"* rather than as *"this fixture is
/// not about the dial"* — the fixtures that ARE about it are at the foot of the file and state a
/// real fraction.
const TAKE_WHAT_THE_RUNG_REACHES: f32 = core_sim::STRIP_IT_BARE;

/// **WHAT A ROW THE PLAYER NEVER TOUCHED THE DIAL ON CARRIES** — the shipped default, the food peak.
///
/// Every *labor row* below is built with it rather than with the identity above, because that is
/// what `assign_labor` gives a fresh assignment and because the identity is a **strip order**: at a
/// floor of `0` `intensification::learn_multiplier` is `0`, so a fixture built at the take seam's
/// identity would quietly stop teaching its rung's lesson.
const A_FRESH_ASSIGNMENTS_FLOOR: f32 = core_sim::DEFAULT_ESCAPEMENT_FLOOR;

/// **A 3×1 world of one terrain, with a band standing on tile (0, 0).** Deliberately the smallest
/// world `advance_labor_allocation` will run against: what these tests measure is a deposit, and a
/// generated map would put a hundred other sources in the same band's reach.
fn world_of(terrain: TerrainType) -> (World, Entity) {
    let mut world = World::default();
    let mut config = SimulationConfig::builtin();
    config.map_topology.wrap_horizontal = false;
    world.insert_resource(config);
    world.insert_resource(core_sim::FaunaConfigHandle::default());
    world.insert_resource(LaborConfigHandle::default());
    world.insert_resource(core_sim::FloraConfigHandle::default());
    world.insert_resource(LadderConfigHandle::default());
    world.insert_resource(core_sim::WellbeingConfigHandle::default());
    world.insert_resource(core_sim::CombatConfigHandle::default());
    world.insert_resource(core_sim::CreaturesConfigHandle::default());
    // ⛔ **THE PLAIN ROSTER, NOT THE STOCKED FIXTURE — the bare hands ARE the assertion.** The free
    // floor of both deposit branches has to be workable with nothing at all
    // (`docs/plan_extraction.md` §4d: a felling kit wants a haft, a haft is wood, and wood comes
    // from felling), so every band here spawns with **no `BandEquipment` component**. A fixture that
    // stocked one would still pass and would stop testing the thing.
    world.insert_resource(core_sim::EquipmentConfigHandle::default());
    world.insert_resource(core_sim::MaterialsConfigHandle::default());
    world.insert_resource(core_sim::RecipesConfigHandle::default());
    world.insert_resource(ExtractionConfigHandle::default());
    world.insert_resource(DepositRegistry::default());
    world.insert_resource(FactionInventory::default());
    world.insert_resource(DiscoveryProgressLedger::default());
    world.insert_resource(CommandEventLog::default());
    world.insert_resource(SimulationTick::default());
    world.insert_resource(core_sim::RoadRegistry::default());
    world.insert_resource(HerdRegistry::default());
    world.insert_resource(ForageRegistry::default());
    // No gathering sites: these bands do not forage, and an empty registry is a valid map.
    world.insert_resource(FoodSiteRegistry::new(Vec::new()));

    let tiles: Vec<Entity> = (0..3)
        .map(|x| {
            world
                .spawn(Tile {
                    position: UVec2::new(x, 0),
                    terrain,
                    ..Default::default()
                })
                .id()
        })
        .collect();
    let home = tiles[0];
    world.insert_resource(TileRegistry {
        tiles,
        width: 3,
        height: 1,
    });
    (world, home)
}

/// A content band (morale 1 → output multiplier 1.0) working `material` at (0, 0) with `workers`
/// hands, and **no equipment component at all**.
fn spawn_extractors(world: &mut World, home: Entity, material: &str, workers: u32) -> Entity {
    spawn_band_of(world, home, material, workers, workers)
}

/// The same, with a **larger workforce than the take crew** — the shape a fixture needs when it is
/// going to staff the builders too. The shedding order trims a band that cannot field what it holds,
/// so a fixture that put a build crew on top of a full take crew would measure the shed rather than
/// the build.
fn spawn_band_of(
    world: &mut World,
    home: Entity,
    material: &str,
    workers: u32,
    take_crew: u32,
) -> Entity {
    world
        .spawn((
            PopulationCohort {
                home,
                current_tile: home,
                size: 60,
                children: scalar_zero(),
                working: scalar_from_f32(workers as f32),
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
                faction: FACTION,
                knowledge: Vec::new(),
                migration: None,
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Extract {
                        tile: UVec2::new(0, 0),
                        material: material.to_string(),
                        floor: A_FRESH_ASSIGNMENTS_FLOOR,
                    },
                    workers: take_crew,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}

/// One whole labour pass.
fn run_turn(world: &mut World) {
    world.run_system_once(advance_labor_allocation);
}

/// **A WHOLE TURN, IN STAGE ORDER** — Logistics' decay-and-bill, then Population's labour pass.
///
/// The keeping only means anything across the pair: `advance_deposits` stamps the bill and judges
/// last turn's payment, and `settle_bands_extraction` (inside the labour pass) pays against that
/// stamp. A fixture that ran either alone would measure a working that is never billed or one that
/// is never kept.
fn run_full_turn(world: &mut World) {
    world.run_system_once(core_sim::advance_deposits);
    world.run_system_once(advance_labor_allocation);
}

/// **SEAT A WORKING EXACTLY ON A RUNG'S TOP** — the standing `RungStanding::arrived_at` describes,
/// written straight into the registry so a keeping fixture does not have to spend forty turns of
/// builders getting there first.
fn seat_working(world: &mut World, tile: UVec2, material: &str, rung: RungKey) {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let config = world.resource::<ExtractionConfigHandle>().get();
    let entity = world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("the fixture tile is on the map");
    let ground = world
        .get::<Tile>(entity)
        .expect("the fixture tile carries terrain");
    let capacity = tile_deposit_capacity(&config, material, ground);
    assert!(capacity > 0.0, "fixture: that ground must hold {material}");
    let mut working = DepositSource::opening(tile, material, capacity, rung.branch());
    let (base, width) = core_sim::extraction::deposit_rung_span(rung, &ladder);
    working.set_ladder_position(base + width, &ladder, rung.branch());
    assert_eq!(working.rung(), rung, "fixture: seated on the wrong rung");
    world.resource_mut::<DepositRegistry>().insert(working);
}

/// A band with an `extract` row on each of `workings`, plus `keepers` hands on the `quarrywork`
/// pool — the shape every keeping fixture below wants.
fn spawn_keepers(
    world: &mut World,
    home: Entity,
    workings: &[(UVec2, &str)],
    take_crew: u32,
    keepers: u32,
) -> Entity {
    let band = spawn_band_of(world, home, workings[0].1, 40, take_crew);
    {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.clear();
        for (tile, material) in workings {
            allocation.assignments.push(LaborAssignment {
                target: LaborTarget::Extract {
                    tile: *tile,
                    material: (*material).to_string(),
                    floor: A_FRESH_ASSIGNMENTS_FLOOR,
                },
                workers: take_crew,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
        }
        if keepers > 0 {
            allocation.assignments.push(LaborAssignment {
                target: LaborTarget::Quarrywork,
                workers: keepers,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
        }
    }
    band
}

/// The live working's ladder position.
fn position(world: &World, tile: UVec2, material: &str) -> f32 {
    world
        .resource::<DepositRegistry>()
        .source(tile, material)
        .expect("the working stands")
        .ladder_position()
}

/// How much of `material` the band is holding.
fn held(world: &World, band: Entity, material: &str) -> f32 {
    world
        .get::<PopulationCohort>(band)
        .expect("the fixture band survives the turn")
        .stores
        .material_total(material)
        .to_f32()
}

/// The live working's stock, or `None` if none was ever opened on that ground.
fn stock(world: &World, material: &str) -> Option<f32> {
    world
        .resource::<DepositRegistry>()
        .source(UVec2::new(0, 0), material)
        .map(|working| working.stock)
}

// ---------------------------------------------------------------------------------------------
// The bootstrap
// ---------------------------------------------------------------------------------------------

/// ⛔ **BOTH FREE FLOORS ARE WORKABLE BARE-HANDED, AND THAT IS LOAD-BEARING RATHER THAN A
/// CONVENIENCE** (`docs/plan_extraction.md` §4d).
///
/// A felling kit wants a haft, a haft is wood, and wood comes from felling; each of the two shipped
/// quarry tools costs 3 wood. So if either floor rung needed gear the material economy could never
/// start — and a band that spawns with nothing is exactly the shipped opening
/// (`equipment.json`'s `start_stock_fraction: 0.0`).
///
/// The band here carries **no `BandEquipment` component at all**, which is stronger than an empty
/// one: there is not even a roster to resolve a tier against.
#[test]
fn a_band_with_no_kit_at_all_takes_from_the_floor_rung_of_both_branches() {
    for (terrain, material) in [(WOODED, WOOD), (ROCK, STONE)] {
        let (mut world, home) = world_of(terrain);
        let band = spawn_extractors(&mut world, home, material, 4);
        assert!(
            world.get::<core_sim::BandEquipment>(band).is_none(),
            "fixture: the band must hold no kit, or this asserts nothing"
        );
        run_turn(&mut world);
        assert!(
            held(&world, band, material) > 0.0,
            "{material} on {terrain:?}: a bare-handed crew must take something from the free floor"
        );
    }
}

/// **A row on a tile that holds none of that material opens no working and takes nothing.** Absence
/// is the answer — there is no `enabled` flag and no zero row for a crew to work.
#[test]
fn ground_that_holds_none_of_a_material_opens_no_working() {
    let (mut world, home) = world_of(BARREN);
    let band = spawn_extractors(&mut world, home, WOOD, 6);
    run_turn(&mut world);
    assert_eq!(
        stock(&world, WOOD),
        None,
        "no working may be opened on ground with no deposit"
    );
    assert_eq!(held(&world, band, WOOD), 0.0);
}

/// **And the LADDER refuses the verb there too**, through the same capacity reading — so the free
/// floor's absence and the built rung's site rule are one fact about the ground, not two.
#[test]
fn the_quarry_rung_refuses_ground_with_no_rock_and_a_scatter_alike() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let site = ladder
        .rung(RungKey::ExtractionQuarry)
        .site_requirement
        .expect("the quarry rung states what the ground must be");

    let refuses = |terrain: TerrainType| {
        let ground = Tile {
            position: UVec2::new(0, 0),
            terrain,
            ..Default::default()
        };
        site.refusal(
            true,
            0.0,
            true,
            tile_deposit_capacity(&config, STONE, &ground),
        )
    };

    assert_eq!(
        refuses(BARREN),
        Some(core_sim::SiteRefusal::NoDeposit),
        "ground with no rock at all takes no working"
    );
    assert_eq!(
        refuses(WOODED),
        Some(core_sim::SiteRefusal::NoDeposit),
        "a 35-unit scatter of field flint is not a body of rock — this is the whole of \
         'you cannot quarry just anywhere'"
    );
    assert_eq!(
        refuses(ROCK),
        None,
        "a real rock body takes a working, or the rung is unbuildable"
    );
}

/// ⛔ **A DEEPER FLOOR TEACHES THE DEPOSIT BRANCH FASTER — THROUGH THE SHARED SEAM, NOT A SECOND
/// ONE** (issue #650).
///
/// `intensification::learn_multiplier` is `floor / MSY_BIOMASS_FRACTION` and belongs to no web: it
/// prices *what you left standing* against *what you learned*, and a deposit crew now has the same
/// dial to trade with as a gatherer. The deposit earn sites used to pass a named fixed point
/// (`PRACTICE_AT_THE_PLAIN_RATE`) on the reading that a working carried no floor; that reading is
/// what this arc removed, so the constant went with it.
///
/// **Measured as a RATIO against the shipped multiplier**, not as an ordering: an ordering alone
/// would pass on any monotone term someone happened to wire in, where the ratio says it is *this*
/// function. Both bands cut the same ground with the same hands, so the floor is the only thing that
/// differs.
#[test]
fn a_deeper_floor_teaches_the_deposit_branch_faster() {
    /// Long enough for the ratio to be a rate and short enough that neither band has completed the
    /// lesson and clamped at `1.0`, which would flatten the comparison to nothing.
    const A_SPELL_OF_GATHERING: u32 = 5;
    /// Two floors either side of the food peak, so one multiplier is below `1.0` and one above it.
    const A_SHALLOW_FLOOR: f32 = 0.4;
    const A_DEEP_FLOOR: f32 = 0.8;

    let practice_at = |floor: f32| {
        let (mut world, home) = world_of(WOODED);
        let band = spawn_band_of(&mut world, home, WOOD, 20, 6);
        {
            let mut allocation = world
                .get_mut::<LaborAllocation>(band)
                .expect("the fixture band has an allocation");
            allocation.assignments[0].target = LaborTarget::Extract {
                tile: UVec2::new(0, 0),
                material: WOOD.to_string(),
                floor,
            };
        }
        for _ in 0..A_SPELL_OF_GATHERING {
            run_turn(&mut world);
        }
        world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FACTION, core_sim::extraction::WOODCRAFT_DISCOVERY_ID)
            .to_f32()
    };

    let shallow = practice_at(A_SHALLOW_FLOOR);
    let deep = practice_at(A_DEEP_FLOOR);
    assert!(
        shallow > 0.0,
        "**LIVENESS**: the shallow crew must still be learning something, or the ratio below is \
         a statement about zero"
    );
    assert!(
        deep < 1.0,
        "fixture: neither band may finish the lesson inside the window, or the clamp flattens the \
         comparison"
    );
    let expected =
        core_sim::learn_multiplier(A_DEEP_FLOOR) / core_sim::learn_multiplier(A_SHALLOW_FLOOR);
    assert!(
        (deep / shallow - expected).abs() < A_CLOSE_ENOUGH_RATIO,
        "the lesson rides `learn_multiplier(floor)` exactly: {deep} against {shallow} is \
         {ratio}, expected {expected}",
        ratio = deep / shallow
    );
}

/// A few turns of a single-precision accrual compared as a quotient, so an exact `==` would be a
/// statement about float layout rather than about the multiplier.
const A_CLOSE_ENOUGH_RATIO: f32 = 1e-3;

/// ⛔ **A WORKING WHOSE DIAL IS NOT OFFERED LEARNS AT THE PLAIN RATE, AND A RENEWING ONE STILL
/// LEARNS AT THE PLAYER'S** (issue #650).
///
/// The escapement dial participates only where the deposit renews
/// (`extraction::deposit_effective_floor`), so on a rate-0 body the row's floor is stored, published
/// and inert. A lesson priced off it would pay a crew a learning bonus calibrated to a choice that
/// changed nothing about the take — and `extraction:gathering` is exactly where that bites, because
/// picking loose stone off a mountain is a live teaching rung and a rock body is finite.
///
/// **One rung, two grounds, and that is the whole design of the fixture.** Both arms stand on
/// `extraction:gathering` earning `quarrying`, so nothing but the terrain's own rate differs: the
/// shipped stone table is two populations, and this rung is the one place both readings are live.
///
/// **The plain rate is pinned as a VALUE, not merely as invariance.** A rock crew asked to leave
/// nothing and one asked to leave almost everything must learn the same amount *and* that amount
/// must be what a renewing crew at [`core_sim::PRACTICE_AT_THE_PLAIN_RATE`] earns — an
/// equal-to-each-other assertion alone passes against a lesson that has stopped being credited.
#[test]
fn a_finite_working_learns_at_the_plain_rate_and_a_renewing_one_rides_the_dial() {
    /// Long enough for the accrual to be a rate, short enough that no arm clamps at `1.0` and
    /// flattens every comparison below.
    const A_SPELL_OF_PICKING: u32 = 5;
    /// Two floors either side of the food peak, so one multiplier is under `1.0` and one over it.
    const A_SHALLOW_FLOOR: f32 = 0.4;
    const A_DEEP_FLOOR: f32 = 0.8;

    let practice_on = |terrain: TerrainType, floor: f32| {
        let (mut world, home) = world_of(terrain);
        let band = spawn_band_of(&mut world, home, STONE, 20, 6);
        {
            let mut allocation = world
                .get_mut::<LaborAllocation>(band)
                .expect("the fixture band has an allocation");
            allocation.assignments[0].target = LaborTarget::Extract {
                tile: UVec2::new(0, 0),
                material: STONE.to_string(),
                floor,
            };
        }
        for _ in 0..A_SPELL_OF_PICKING {
            run_turn(&mut world);
        }
        world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FACTION, core_sim::extraction::QUARRYING_DISCOVERY_ID)
            .to_f32()
    };

    // **The fixture's own precondition**: the two grounds really do fall on opposite sides of the
    // condition `deposit_effective_floor` forks on. Without this the whole test could be two
    // readings of one population.
    let config = ExtractionConfig::builtin();
    let rate_of = |terrain: TerrainType| {
        tile_deposit_regrowth(
            &config,
            STONE,
            &Tile {
                position: UVec2::new(0, 0),
                terrain,
                ..Default::default()
            },
        )
    };
    assert_eq!(
        rate_of(ROCK),
        core_sim::NEVER_RENEWS,
        "fixture: the finite arm must stand on a rock body"
    );
    assert!(
        rate_of(A_STONE_SCATTER) > core_sim::NEVER_RENEWS,
        "fixture: the renewing arm must stand on a scatter that comes back"
    );

    let finite_shallow = practice_on(ROCK, A_SHALLOW_FLOOR);
    let finite_deep = practice_on(ROCK, A_DEEP_FLOOR);
    assert!(
        finite_shallow > 0.0 && finite_deep < 1.0,
        "**LIVENESS**: the rock crews must still be learning and must not have finished, or the \
         equality below is a statement about zero or about the clamp: {finite_shallow} / \
         {finite_deep}"
    );
    assert_eq!(
        finite_shallow, finite_deep,
        "a working whose dial does not participate must learn the same whatever the row carries"
    );

    let renewing_shallow = practice_on(A_STONE_SCATTER, A_SHALLOW_FLOOR);
    let renewing_deep = practice_on(A_STONE_SCATTER, A_DEEP_FLOOR);
    let expected =
        core_sim::learn_multiplier(A_DEEP_FLOOR) / core_sim::learn_multiplier(A_SHALLOW_FLOOR);
    assert!(
        (renewing_deep / renewing_shallow - expected).abs() < A_CLOSE_ENOUGH_RATIO,
        "a renewing working must still be paced by the player's own dial: {renewing_deep} against \
         {renewing_shallow} is {ratio}, expected {expected}",
        ratio = renewing_deep / renewing_shallow
    );

    // **And the plain rate is exactly the identity, not merely a constant.** The renewing arm at the
    // fixed point is what a rock crew earns, which is what makes this a bonus of neither sign.
    let renewing_at_the_fixed_point =
        practice_on(A_STONE_SCATTER, core_sim::PRACTICE_AT_THE_PLAIN_RATE);
    assert!(
        (finite_shallow - renewing_at_the_fixed_point).abs() < A_CLOSE_ENOUGH_RATIO,
        "the plain rate is `learn_multiplier`'s fixed point, so a rock crew must earn exactly what \
         a renewing crew earns there: {finite_shallow} against {renewing_at_the_fixed_point}"
    );
}

/// **A SCATTER OF LOOSE STONE THAT COMES BACK** — the renewing half of the shipped stone table
/// (periglacial steppe, 70 units at `0.015`), against [`ROCK`]'s rate of zero. Named because what
/// the fixture wants of it is the *rate*, not the terrain.
const A_STONE_SCATTER: TerrainType = TerrainType::PeriglacialSteppe;

/// ⛔ **THE LADDER IS REACHABLE FROM A STANDING START, THROUGH THE ORDINARY BUILD QUEUE** — the
/// liveness check every gate above is worth nothing without.
///
/// It drives the whole chain the player drives: work the free floor until the faction learns
/// `woodcraft`, queue a `fell` on the working, staff the band's builders, and watch the position
/// climb until the rung is held and the take jumps.
///
/// **The knowledge is EARNED, not granted.** A fixture that seeded the discovery would pass with the
/// earn seam disconnected, which is the half of the ladder that has no other test.
#[test]
fn a_band_learns_woodcraft_at_the_free_floor_and_then_raises_a_felling_working() {
    let (mut world, home) = world_of(WOODED);
    // Twenty hands, six of them cutting — leaving room for a builders pool without the shedding
    // order trimming the band down to what it can field.
    let band = spawn_band_of(&mut world, home, WOOD, 20, 6);

    // 1. The free floor teaches.
    let mut learned = None;
    for turn in 0..200 {
        run_turn(&mut world);
        if core_sim::knows(
            world.resource::<DiscoveryProgressLedger>(),
            FACTION,
            core_sim::extraction::WOODCRAFT_DISCOVERY_ID,
            world
                .resource::<LadderConfigHandle>()
                .get()
                .knowledge
                .completion_threshold,
        ) {
            learned = Some(turn);
            break;
        }
    }
    let learned = learned.expect("gathering deadfall must teach woodcraft");
    assert!(
        learned > 0,
        "the lesson must be EARNED over turns, not granted at spawn"
    );

    // 2. One more floor turn, measured as a DELTA — the comparison at the foot of this test is
    //    per-turn, and the running total is not it.
    let before_floor = held(&world, band, WOOD);
    run_turn(&mut world);
    let floor_take = held(&world, band, WOOD) - before_floor;
    assert!(floor_take > 0.0, "the free floor pays a real trickle");

    // 3. Queue the rung and staff the pool.
    {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Builders,
            workers: 8,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
        assert!(allocation.enqueue_build(
            core_sim::BuildSource::Deposit {
                tile: UVec2::new(0, 0),
                material: WOOD.to_string(),
            },
            core_sim::BuildJob::Rung(core_sim::Improvement::Fell),
        ));
    }

    // 4. The builders raise it.
    let mut raised = false;
    for _ in 0..40 {
        run_turn(&mut world);
        if world
            .resource::<DepositRegistry>()
            .source(UVec2::new(0, 0), WOOD)
            .expect("the working stands")
            .rung()
            == RungKey::ForestryFelling
        {
            raised = true;
            break;
        }
    }
    assert!(
        raised,
        "a staffed builders pool must raise the felling rung"
    );

    // 5. And the take really did jump — the rung buys a rate, so the turn after it is held pays
    //    more than a turn at the floor did.
    let before = held(&world, band, WOOD);
    run_turn(&mut world);
    let felling_turn = held(&world, band, WOOD) - before;
    assert!(
        felling_turn > floor_take,
        "felling must out-take the free floor: {felling_turn} against the floor's {floor_take}"
    );
}

// ---------------------------------------------------------------------------------------------
// The one idea
// ---------------------------------------------------------------------------------------------

/// A working seated at `rung`, on ground of `terrain`, at full stock.
fn working_at(rung: RungKey, terrain: TerrainType, material: &str) -> (DepositSource, Tile) {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let ground = Tile {
        position: UVec2::new(0, 0),
        terrain,
        ..Default::default()
    };
    let capacity = tile_deposit_capacity(&config, material, &ground);
    assert!(capacity > 0.0, "fixture: {terrain:?} must hold {material}");
    let mut source = DepositSource::opening(UVec2::new(0, 0), material, capacity, rung.branch());
    // Seat the position exactly on the rung's top, which is what `RungStanding::arrived_at`
    // describes: the rung held in full, raising whatever is above it at no credit.
    let (base, width) = core_sim::extraction::deposit_rung_span(rung, &ladder);
    source.set_ladder_position(base + width, &ladder, rung.branch());
    assert_eq!(
        source.rung(),
        rung,
        "fixture: the working must actually stand on the rung under test"
    );
    (source, ground)
}

/// ⛔ **A ROCK DEPOSIT ONLY EVER GOES DOWN, AND ITS REGROWTH CONTRIBUTES EXACTLY ZERO.**
///
/// Both halves are asserted, because either alone is weak: a monotonically-falling stock also
/// describes a deposit whose renewal is simply outpaced, and a zero regrowth term also describes a
/// deposit nobody is working.
///
/// **The turn is spelled out — renew, then take** — because that is the order the sim runs and the
/// renewal is no longer inside the take (`renew_deposit` is the pass's, once per working).
#[test]
fn a_worked_quarry_only_ever_goes_down_and_renews_nothing() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let (mut working, ground) = working_at(RungKey::ExtractionQuarry, ROCK, STONE);
    let mut previous = working.stock;
    let mut ever_took = false;
    for _ in 0..400 {
        core_sim::renew_deposit(&mut working, &ground, &config, &ladder);
        assert_eq!(
            working.stock, previous,
            "renewal must contribute EXACTLY nothing to a rock body — `0 x anything` is still 0"
        );
        let outcome = take_from_deposit(
            &mut working,
            5,
            TAKE_WHAT_THE_RUNG_REACHES,
            &ground,
            &config,
            &ladder,
        );
        ever_took |= outcome.taken > 0.0;
        assert!(
            working.stock <= previous,
            "a quarry's stock must never rise: {} after {previous}",
            working.stock
        );
        previous = working.stock;
    }
    assert!(
        ever_took,
        "**LIVENESS**: the crew must actually have been working, or the monotonicity is vacuous"
    );
    assert!(
        working.stock < tile_deposit_capacity(&config, STONE, &ground),
        "**LIVENESS**: four hundred turns of quarrying must have moved the stock"
    );
}

/// **A wood recovers — and never past its capacity.** Cut it down, take the crew off, and the stand
/// climbs back on the deposit's own rate.
#[test]
fn a_cut_wood_climbs_back_and_stops_at_capacity() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let (mut working, ground) = working_at(RungKey::ForestryFelling, WOODED, WOOD);
    let capacity = tile_deposit_capacity(&config, WOOD, &ground);

    for _ in 0..40 {
        core_sim::renew_deposit(&mut working, &ground, &config, &ladder);
        take_from_deposit(
            &mut working,
            12,
            TAKE_WHAT_THE_RUNG_REACHES,
            &ground,
            &config,
            &ladder,
        );
    }
    let cut_to = working.stock;
    assert!(
        cut_to < capacity * 0.5,
        "fixture: the wood must really have been cut down, got {cut_to} of {capacity}"
    );

    let mut previous = cut_to;
    for _ in 0..600 {
        // ⛔ **Nobody is working it, AND NOTHING NEEDS TO BE.** Renewal is the pass's and runs on
        // every working whether or not a band holds a row — which is exactly the half that was
        // broken while it lived inside the take, where an abandoned wood was frozen at its
        // low-water mark for ever.
        core_sim::renew_deposit(&mut working, &ground, &config, &ladder);
        assert!(
            working.stock >= previous,
            "an unworked wood must not fall: {} after {previous}",
            working.stock
        );
        assert!(
            working.stock <= capacity,
            "renewal must never take a wood past the capacity the land sets"
        );
        previous = working.stock;
    }
    assert!(
        (working.stock - capacity).abs() < 1e-3,
        "six hundred quiet turns must refill the wood, reached {} of {capacity}",
        working.stock
    );
}

/// ⛔ **OVER-CUTTING IS REACHABLE, and it must stay so.** The take is deliberately not clamped to
/// the sustainable rate: the whole point of the renewable half is that you can ruin a wood, and the
/// warning is a readout rather than a guard.
#[test]
fn enough_hands_drive_a_wood_down_turn_on_turn() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let (mut working, ground) = working_at(RungKey::ForestryFelling, WOODED, WOOD);
    let mut previous = working.stock;
    for turn in 0..30 {
        core_sim::renew_deposit(&mut working, &ground, &config, &ladder);
        take_from_deposit(
            &mut working,
            10,
            TAKE_WHAT_THE_RUNG_REACHES,
            &ground,
            &config,
            &ladder,
        );
        assert!(
            working.stock < previous,
            "turn {turn}: ten fellers must out-cut the wood's own renewal, {} after {previous}",
            working.stock
        );
        previous = working.stock;
    }
}

/// **A RUNG RAISES THE REACH** — the same rock body yields far more over its whole life at
/// `extraction:quarry` than at `extraction:gathering`, because the floor moved **down**.
///
/// Measured as the total ever taken rather than as a per-turn rate: what the extraction branch buys
/// is *how much of the body you can ever get out*, which a single turn cannot show.
#[test]
fn a_quarry_reaches_far_more_of_one_body_than_gathering_ever_can() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();

    let lifetime_take = |rung: RungKey| {
        let (mut working, ground) = working_at(rung, ROCK, STONE);
        let mut total = 0.0;
        for _ in 0..4000 {
            core_sim::renew_deposit(&mut working, &ground, &config, &ladder);
            total += take_from_deposit(
                &mut working,
                4,
                TAKE_WHAT_THE_RUNG_REACHES,
                &ground,
                &config,
                &ladder,
            )
            .taken;
        }
        total
    };

    let shallow = lifetime_take(RungKey::ExtractionGathering);
    let deep = lifetime_take(RungKey::ExtractionQuarry);
    assert!(
        deep > shallow * 3.0,
        "a quarry must reach several times what gathering can: {deep} against {shallow}"
    );
    // And the reason, stated as arithmetic rather than left to be inferred from the totals: the
    // rung lowers the floor and touches nothing else.
    let capacity = tile_deposit_capacity(&config, STONE, &{
        Tile {
            position: UVec2::new(0, 0),
            terrain: ROCK,
            ..Default::default()
        }
    });
    let floor_at = |rung: RungKey| {
        let standing = deposit_standing(
            core_sim::extraction::deposit_rung_span(rung, &ladder).0
                + core_sim::extraction::deposit_rung_span(rung, &ladder).1,
            &ladder,
            rung.branch(),
        );
        core_sim::extraction::deposit_floor(capacity, &deposit_payoff(&standing, &ladder))
    };
    assert!(floor_at(RungKey::ExtractionQuarry) < floor_at(RungKey::ExtractionGathering));
}

// ---------------------------------------------------------------------------------------------
// The §6 floor trap's guard
// ---------------------------------------------------------------------------------------------

/// ⛔ **NO RUNG ON EITHER BRANCH MAY RAISE `capacity`**, and this is the guard rather than a comment
/// (`docs/plan_standing_upkeep.md` §6).
///
/// The trap: a rung raised a herd's ceiling, the floor `floor_fraction × K` climbed with it while the
/// herd stayed the size it was, the build's own eligibility gate read the room above that floor, and
/// a tame begun on its floor **never completed at any crew size** — building faster starved you
/// sooner. Here the floor is `(1 − recovery_fraction) × capacity`, so a capacity that moved with the
/// position would reopen it exactly.
///
/// It walks **every rung of both branches** and asserts the capacity is the terrain's at each — which
/// is a claim about the seam, not about today's numbers: `tile_deposit_capacity` takes no position
/// and no standing, so there is nowhere for a rung to reach it, and this fails the day one is added.
#[test]
fn no_rung_on_either_branch_may_raise_capacity() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    for (terrain, material) in [(WOODED, WOOD), (ROCK, STONE)] {
        let ground = Tile {
            position: UVec2::new(0, 0),
            terrain,
            ..Default::default()
        };
        let land = tile_deposit_capacity(&config, material, &ground);
        assert!(land > 0.0, "fixture: {terrain:?} must hold {material}");
        let branch = core_sim::extraction::deposit_branch(&config, material)
            .expect("a shipped deposit names its branch");
        let mut rung = Some(branch.root_rung());
        let mut seen = 0;
        while let Some(key) = rung {
            let (base, width) = core_sim::extraction::deposit_rung_span(key, &ladder);
            // Both ends of the rung, and the middle: a capacity that moved *within* a rung would be
            // an interpolated one, which is the shape every other per-rung quantity has.
            for position in [base, base + width * 0.5, base + width] {
                let mut working = DepositSource::opening(UVec2::new(0, 0), material, land, branch);
                working.set_ladder_position(position, &ladder, branch);
                assert_eq!(
                    tile_deposit_capacity(&config, material, &ground),
                    land,
                    "{material} on {terrain:?} at position {position}: capacity is the TERRAIN's \
                     and no rung may move it — the floor must only ever be able to go down"
                );
                // And the floor it implies really does only go down as the position climbs.
                let floor = core_sim::extraction::deposit_floor(
                    land,
                    &deposit_payoff(working.standing(), &ladder),
                );
                assert!(
                    floor <= land,
                    "the floor can never exceed what the land holds"
                );
            }
            seen += 1;
            rung = key.above();
        }
        assert!(
            seen >= 2,
            "fixture: {branch:?} must have more than one rung, or the walk proves nothing"
        );
    }
    // Both ladders were walked — stated so a branch dropping out of the loop is a failure rather
    // than a quieter pass.
    assert!(core_sim::ALL_BRANCHES.contains(&RungBranch::Forestry));
    assert!(core_sim::ALL_BRANCHES.contains(&RungBranch::Extraction));
}

/// **The forestry rungs raise the ground's OWN renewal, and rock's own rate is what refuses that
/// same multiplier** — one expression, no branch check anywhere.
#[test]
fn the_regrowth_multiplier_scales_the_grounds_own_rate_and_rock_has_none() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let wooded = Tile {
        position: UVec2::new(0, 0),
        terrain: WOODED,
        ..Default::default()
    };
    let rock = Tile {
        position: UVec2::new(0, 0),
        terrain: ROCK,
        ..Default::default()
    };

    let multiplier_of = |rung: RungKey| {
        let (base, width) = core_sim::extraction::deposit_rung_span(rung, &ladder);
        deposit_payoff(
            &deposit_standing(base + width, &ladder, rung.branch()),
            &ladder,
        )
        .regrowth_multiplier
    };
    let coppice = multiplier_of(RungKey::ForestryCoppice);
    assert!(
        coppice > multiplier_of(RungKey::ForestryFelling),
        "conservationism buys RENEWAL, so the coppice rung must raise the multiplier"
    );

    // The same multiplier, applied to each ground's own rate.
    let wood_rate = tile_deposit_regrowth(&config, WOOD, &wooded);
    let rock_rate = tile_deposit_regrowth(&config, STONE, &rock);
    let capacity = 1000.0;
    let half = capacity * 0.5;
    assert!(
        deposit_regrowth(half, capacity, wood_rate * coppice, config.seed_fraction) > half,
        "a managed wood renews faster"
    );
    assert_eq!(
        deposit_regrowth(half, capacity, rock_rate * coppice, config.seed_fraction),
        half,
        "and the very same multiplier renews a rock body by exactly nothing — `0 x anything` is \
         still 0, which is why one payoff block serves both branches"
    );
}

// ---------------------------------------------------------------------------------------------
// The standing upkeep
// ---------------------------------------------------------------------------------------------

/// **A working left unkept slides back down its ladder, and staffing it stops the slide.**
///
/// This is the half that makes holding a working cost something at all: without it a quarry is free
/// to hold for ever, and an improvement free to hold cannot weigh on move-or-stay
/// (`docs/plan_standing_upkeep.md`).
///
/// **Both halves in one drive**, because either alone is weak — a position that never moves also
/// describes a decay that was never wired, and one that always falls describes a keeping pool that
/// pays nothing.
#[test]
fn an_unkept_working_slides_and_a_kept_one_stops_sliding() {
    let at = UVec2::new(0, 0);
    let (mut world, home) = world_of(WOODED);
    seat_working(&mut world, at, WOOD, RungKey::ForestryFelling);
    let band = spawn_keepers(&mut world, home, &[(at, WOOD)], 2, 0);
    let seated = position(&world, at, WOOD);

    // The grace absorbs the first turns, then the meter bleeds.
    for _ in 0..12 {
        run_full_turn(&mut world);
    }
    let slumped = position(&world, at, WOOD);
    assert!(
        slumped < seated,
        "a working nobody keeps must lose its meter: {slumped} against {seated}"
    );

    // Put one keeper on it — the shipped `forestry:felling` bill on the reference wood is exactly
    // 1.0 work a turn, which one bare hand covers.
    {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Quarrywork,
            workers: 1,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }
    run_full_turn(&mut world);
    let held_at = position(&world, at, WOOD);
    for _ in 0..40 {
        run_full_turn(&mut world);
        assert_eq!(
            position(&world, at, WOOD),
            held_at,
            "a kept working must not lose a unit — the slide stops the turn the keepers arrive"
        );
    }
}

/// **A staffed working holds its position indefinitely.** The neglect counter never arms, so the
/// grace is never spent and the meter never rots — which is what makes the keeping a *standing*
/// cost rather than a countdown to losing the rung anyway.
#[test]
fn a_staffed_working_holds_its_rung_for_ever() {
    let at = UVec2::new(0, 0);
    let (mut world, home) = world_of(WOODED);
    seat_working(&mut world, at, WOOD, RungKey::ForestryFelling);
    spawn_keepers(&mut world, home, &[(at, WOOD)], 2, 1);
    let seated = position(&world, at, WOOD);
    for _ in 0..200 {
        run_full_turn(&mut world);
    }
    assert_eq!(position(&world, at, WOOD), seated);
    assert_eq!(
        world
            .resource::<DepositRegistry>()
            .source(at, WOOD)
            .expect("the working stands")
            .neglect_turns,
        0,
        "**LIVENESS**: a met bill must reset the run, or the hold above is a grace that never ran out"
    );
}

/// **THE BAND'S OWN LEDGER PUBLISHES AND CLEARS.** A band that has put its last working down must
/// stop republishing a bill it no longer owes — `settle_bands_roadwork`'s `(c)` failure mode, which
/// is why both fields are cleared ahead of every exit rather than only on the paying path.
///
/// It also pins the *ungated* half: the demand is summed **before** the head-count gate, so a band
/// with nobody on the role publishes the bill it is failing to pay rather than a reassuring zero.
#[test]
fn the_quarrywork_ledger_publishes_a_bill_nobody_is_paying_and_clears_when_the_row_goes() {
    let at = UVec2::new(0, 0);
    let (mut world, home) = world_of(WOODED);
    seat_working(&mut world, at, WOOD, RungKey::ForestryFelling);
    let band = spawn_keepers(&mut world, home, &[(at, WOOD)], 2, 0);

    run_full_turn(&mut world);
    let ledger = |world: &World| {
        let allocation = world
            .get::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        (
            allocation.last_quarrywork_demand,
            allocation.last_quarrywork_supplied,
        )
    };
    let (demand, supplied) = ledger(&world);
    assert!(
        demand > 0.0,
        "a band holding a felling working owes a bill even with nobody on the role"
    );
    assert_eq!(supplied, 0.0, "and pays none of it with no keepers");

    // Staff it, and the supplied half fills.
    {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Quarrywork,
            workers: 1,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }
    run_full_turn(&mut world);
    let (demand, supplied) = ledger(&world);
    assert!(supplied > 0.0 && (supplied - demand).abs() < 1e-4);

    // Put the whole holding down. The bill must go with it.
    world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band has an allocation")
        .assignments
        .clear();
    run_full_turn(&mut world);
    assert_eq!(
        ledger(&world),
        (0.0, 0.0),
        "a band that put its last working down must stop republishing last turn's bill"
    );
}

/// **A SHORT POOL FUNDS THE MOST-INVESTED WORKING FIRST.** `UpkeepFundMode::Priority` walks the
/// claims in the order the caller ranked them, and this pool ranks on the working's own ladder
/// position — the same *"most invested"* a road is ranked by, because on both branches the position
/// **is** the accumulator.
#[test]
fn a_band_short_of_keepers_funds_its_deepest_working_first() {
    let coppice_at = UVec2::new(0, 0);
    let felling_at = UVec2::new(1, 0);
    let (mut world, home) = world_of(WOODED);
    seat_working(&mut world, coppice_at, WOOD, RungKey::ForestryCoppice);
    seat_working(&mut world, felling_at, WOOD, RungKey::ForestryFelling);
    let band = spawn_keepers(
        &mut world,
        home,
        &[(coppice_at, WOOD), (felling_at, WOOD)],
        1,
        // One hand against a 2.0 + 1.0 bill: the pool is short on purpose.
        1,
    );
    world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band has an allocation")
        .upkeep_fund_mode = core_sim::UpkeepFundMode::Priority;

    run_full_turn(&mut world);
    let supplied = |world: &World, tile: UVec2| {
        world
            .resource::<DepositRegistry>()
            .source(tile, WOOD)
            .expect("the working stands")
            .upkeep_supplied
    };
    assert!(
        supplied(&world, coppice_at) > 0.0,
        "the coppice is the deeper working and is paid first"
    );
    assert_eq!(
        supplied(&world, felling_at),
        0.0,
        "and the shallower one gets what is left, which at one keeper is nothing"
    );
}

/// ⛔ **THE QUARRY THRESHOLD FALLS IN THE GAP BETWEEN THE TWO STONE POPULATIONS**, and every rate-0
/// row is on the quarryable side of it.
///
/// This is the rule the docs have always stated and nothing asserted, and the failure it exists to
/// catch is not a tuning slip: **a rate-0 row below the threshold is a dead work site that still
/// accepts a crew.** A band on one works `extraction:gathering`, takes its `recovery_fraction` of
/// the body once, and then reaches nothing for the rest of the game — the stock never returns, and
/// the rung that would reach deeper is refused for ever. It shipped that way for three rows
/// (`AquiferCeiling` 600, `FumaroleBasin` 300, `AshPlain` 120) against a threshold of 800.
///
/// **It reads the shipped table AND the shipped ladder**, because the invariant is a relation
/// between two files: either one moving alone is what breaks it.
#[test]
fn the_quarry_threshold_splits_the_finite_rows_from_the_renewing_ones() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let threshold = ladder
        .rung(RungKey::ExtractionQuarry)
        .site_requirement
        .expect("the quarry rung states what the ground must be")
        .min_deposit_capacity;
    let stone = config
        .deposit(STONE)
        .expect("the shipped table carries stone");

    let mut finite = 0;
    let mut renewing = 0;
    for (terrain, ground) in &stone.by_terrain {
        if ground.regrowth_rate == core_sim::NEVER_RENEWS {
            finite += 1;
            assert!(
                ground.capacity >= threshold,
                "{terrain:?} never renews and holds {} — under the quarry threshold of \
                 {threshold} it is a DEAD WORK SITE that still accepts a crew: gathering takes its \
                 share once and nothing ever reaches deeper",
                ground.capacity
            );
        } else {
            renewing += 1;
            assert!(
                ground.capacity < threshold,
                "{terrain:?} renews and holds {} — at or above the quarry threshold of \
                 {threshold} a scatter of loose stone would take a working face, which is exactly \
                 what 'you cannot quarry just anywhere' refuses",
                ground.capacity
            );
        }
    }
    assert!(
        finite > 0 && renewing > 0,
        "**LIVENESS**: both populations must be present, or the split is asserted against nothing"
    );
}

/// ⛔ **DECAY MUST NOT RESURRECT THE §6 FLOOR TRAP.** A falling position lowers
/// `recovery_fraction`, which raises the floor `(1 − recovery) × capacity` and so **reduces** what
/// the working can reach. That is correct and intended — you reach less of the deposit as the face
/// slumps — but it must never leave the working stuck at a position it cannot climb out of.
///
/// The proof is a round trip: starve a quarry until its rung is gone, then staff the builders and
/// watch it climb back. **The build gate reads the ground and the knowledge and never the stock**,
/// which is the property that makes the climb out possible at all, and this is what would fail if
/// somebody ever added a stock term to it.
#[test]
fn a_slumped_working_can_be_cut_back_open() {
    let at = UVec2::new(0, 0);
    let (mut world, home) = world_of(ROCK);
    seat_working(&mut world, at, STONE, RungKey::ExtractionQuarry);
    let band = spawn_keepers(&mut world, home, &[(at, STONE)], 2, 0);
    // The quarrying lesson, so the rung is buildable once the fixture wants it back.
    world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(
            FACTION,
            core_sim::extraction::QUARRYING_DISCOVERY_ID,
            scalar_one(),
        );

    // **The props the rung swallows.** `extraction:quarry` draws 8 wood as its meter climbs, so a
    // band with an empty shelf is blocked on materials however many builders it staffs — which is
    // §2.7 working, and not what this test is about.
    {
        let materials = world.resource::<core_sim::MaterialsConfigHandle>().get();
        let characteristics: std::collections::BTreeMap<String, f32> =
            [("hardness".to_string(), 0.5), ("pliancy".to_string(), 0.5)]
                .into_iter()
                .collect();
        let key = materials
            .band_key(WOOD, &characteristics)
            .expect("wood is on the materials table");
        world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band survives")
            .stores
            .deposit_material(WOOD, key, scalar_from_f32(40.0), &characteristics);
    }

    let seated = position(&world, at, STONE);
    let reachable = |world: &World| {
        let ladder = world.resource::<LadderConfigHandle>().get();
        let config = world.resource::<ExtractionConfigHandle>().get();
        let working = world
            .resource::<DepositRegistry>()
            .source(at, STONE)
            .expect("the working stands");
        let ground = Tile {
            position: at,
            terrain: ROCK,
            ..Default::default()
        };
        core_sim::extraction::deposit_reachable(
            working.stock,
            tile_deposit_capacity(&config, STONE, &ground),
            core_sim::extraction::tile_deposit_regrowth(&config, STONE, &ground),
            &deposit_payoff(working.standing(), &ladder),
            TAKE_WHAT_THE_RUNG_REACHES,
        )
    };

    // Starve it to the floor of its branch.
    for _ in 0..400 {
        run_full_turn(&mut world);
        assert!(
            reachable(&world) >= 0.0,
            "a rising floor may reduce the reach and must never make it negative"
        );
    }
    let slumped = position(&world, at, STONE);
    assert!(slumped < seated, "the quarry must really have slumped");
    assert_eq!(
        world
            .resource::<DepositRegistry>()
            .source(at, STONE)
            .expect("the working stands")
            .rung(),
        RungKey::ExtractionGathering,
        "and slid the whole way back to its free floor"
    );

    // Now cut it open again: builders on the head of the queue, and the rung is re-queued.
    {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Builders,
            workers: 12,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Quarrywork,
            workers: 4,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
        assert!(allocation.enqueue_build(
            core_sim::BuildSource::Deposit {
                tile: at,
                material: STONE.to_string(),
            },
            core_sim::BuildJob::Rung(core_sim::Improvement::Quarry),
        ));
    }
    let mut recovered = false;
    for _ in 0..200 {
        run_full_turn(&mut world);
        if world
            .resource::<DepositRegistry>()
            .source(at, STONE)
            .expect("the working stands")
            .rung()
            == RungKey::ExtractionQuarry
        {
            recovered = true;
            break;
        }
    }
    assert!(
        recovered,
        "a slumped working must be re-cuttable at some crew size — the build gate reads the ground \
         and the knowledge, never the stock, and that is what keeps this reachable"
    );
}

/// **A DEPOSIT RUNG MAY NOT DECLARE A STANDING MATERIAL RATE.** The work half of the keeping is
/// settled and the material half is not, so a rate here would publish a demand nothing ever pays —
/// which is exactly what `route:paved_road` shipped for one slice before its rot term learned to
/// read both currencies.
///
/// The quarry's 8 wood stays where it is: a **build pile**, timbered into the face once as it is
/// opened.
#[test]
fn a_standing_material_rate_on_a_deposit_rung_is_refused() {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_INTENSIFICATION_LADDER)
            .expect("the shipped ladder parses as json");
    let rungs = json["rungs"].as_array_mut().expect("the ladder has rungs");
    let quarry = rungs
        .iter_mut()
        .find(|rung| rung["branch"] == "extraction" && rung["id"] == "quarry")
        .expect("the shipped ladder carries the quarry rung");
    quarry["upkeep"]["materials"] = serde_json::json!({ "wood": 0.1 });
    let err = LadderConfig::from_json_str(&json.to_string())
        .expect_err("a standing material rate on a deposit rung must be rejected");
    assert!(
        format!("{err}").contains("WORK alone"),
        "the refusal must name the reason, got {err}"
    );

    // And the pile it is not: the shipped rung really does swallow wood as it is raised, so the
    // rejection above is about the *rate* and not about the material.
    assert!(
        LadderConfig::builtin()
            .rung(RungKey::ExtractionQuarry)
            .build
            .as_ref()
            .expect("the quarry rung is built")
            .materials
            .contains_key(WOOD),
        "**LIVENESS**: the quarry's props are a build pile, and this test means nothing without one"
    );
}

// ---------------------------------------------------------------------------------------------
// The growth term runs ONCE PER WORKING
// ---------------------------------------------------------------------------------------------

/// ⛔ **TWO BANDS ON ONE WOOD RENEW IT ONCE, NOT TWICE.**
///
/// Renewal lived inside `take_from_deposit` for one slice, and that function is called **once per
/// band-row** — so a working `K` bands shared ran the growth term `K` times a turn, undercutting
/// *"over-cutting is possible and must stay so"* in exact proportion to how many bands were on it.
///
/// The measurement is a **difference against a one-band control**: two bands taking nothing must
/// leave the stand exactly where one band taking nothing does. Asserting only that the stock rose
/// would pass against the defect.
#[test]
fn two_bands_on_one_working_renew_it_once() {
    let at = UVec2::new(0, 0);
    let stand = |bands: u32| {
        let (mut world, home) = world_of(WOODED);
        seat_working(&mut world, at, WOOD, RungKey::ForestryFelling);
        // Cut it down first, so there is real room for the growth term to fill.
        world
            .resource_mut::<DepositRegistry>()
            .source_mut(at, WOOD)
            .expect("the working stands")
            .stock = 100.0;
        for _ in 0..bands {
            // **A crew of nobody**: every band holds a row on the working, and none of them takes
            // anything, so the only thing that can move the stock is the renewal.
            spawn_band_of(&mut world, home, WOOD, 10, 0);
        }
        run_full_turn(&mut world);
        stock(&world, WOOD).expect("the working stands")
    };

    let one = stand(1);
    let two = stand(2);
    let five = stand(5);
    assert!(
        one > 100.0,
        "**LIVENESS**: the wood must actually have grown"
    );
    assert_eq!(
        two, one,
        "two bands on one wood must renew it ONCE — the growth term is the working's, not the row's"
    );
    assert_eq!(five, one, "and five bands the same");
}

/// ⛔ **A WORKING NO BAND HOLDS A ROW ON STILL RENEWS — AND A ROCK ONE STILL DOES NOT.**
///
/// The other half of the same defect: with renewal inside the take, an **abandoned** over-cut wood
/// was frozen at its low-water mark for ever, because nothing called the take on it. That
/// contradicts the arc's own headline — a wood recovers and rock does not — for exactly the ground
/// the branch's move-or-stay pressure is made of.
///
/// **Both materials in one drive**, because either alone is weak: a stand that grows also describes
/// a pass that renews everything, and a quarry that does not also describes a pass that renews
/// nothing.
#[test]
fn an_abandoned_wood_still_recovers_and_an_abandoned_quarry_still_does_not() {
    let at = UVec2::new(0, 0);
    for (terrain, material, floor, should_grow) in [
        (WOODED, WOOD, RungKey::ForestryDeadfall, true),
        (ROCK, STONE, RungKey::ExtractionGathering, false),
    ] {
        let (mut world, _home) = world_of(terrain);
        seat_working(&mut world, at, material, floor);
        world
            .resource_mut::<DepositRegistry>()
            .source_mut(at, material)
            .expect("the working stands")
            .stock = 100.0;
        // **No band at all** — not an unstaffed row, no row and no cohort.
        for _ in 0..50 {
            run_full_turn(&mut world);
        }
        let after = stock(&world, material).expect("the working stands");
        if should_grow {
            assert!(
                after > 100.0,
                "an abandoned wood must still come back: {after} after 50 quiet turns"
            );
        } else {
            assert_eq!(
                after, 100.0,
                "and an abandoned quarry must not — its rate is zero, and nothing renews it"
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The build quote nets the LIVE rot
// ---------------------------------------------------------------------------------------------

/// ⛔ **A WORKING WHOSE KEEPING IS SHORT QUOTES `Rotting`, NOT A FINITE DATE.**
///
/// The countdown is `banked ÷ (what the crew banks − what the next decay pass takes)`, so a quote
/// struck against `NO_UPKEEP_DECAY` promises a rung will finish while the pass takes more off it
/// than the builders put on. **Its blast radius is the whole queue**: `publish_build_chain`
/// accumulates the head's turns into `cumulative`, so an understated deposit head carries its error
/// onto every entry behind it — including the patch, herd and road entries that do reach the wire.
///
/// It asserts on the **composition the arm performs** — `deposit_meter_rot` into
/// `RungDef::build_balance` into `build_turns_estimate` — because a working publishes no wire row for
/// the resolved quote to be read back off (`docs/plan_extraction.md` §7).
///
/// **Both halves**, or "it said Rotting" passes against a rot that is simply always larger than the
/// crew: a *kept* working at the same crew must still quote a real number.
#[test]
fn a_working_whose_keeping_is_short_quotes_a_rotting_meter() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let (mut working, ground) = working_at(RungKey::ForestryFelling, WOODED, WOOD);
    // Part-way up the coppice rung above it, so there is a meter carrying work for a rot to eat and
    // an estimate to be given.
    let (base, width) = core_sim::extraction::deposit_rung_span(RungKey::ForestryCoppice, &ladder);
    working.set_ladder_position(base + width * 0.5, &ladder, RungBranch::Forestry);

    let measure = core_sim::deposit_measure(&working, &ground, &config);
    let bill = core_sim::deposit_keeping_basis(&working, measure, &ladder);
    assert!(bill > 0.0, "fixture: a coppice in flight owes a real bill");

    // **Past the grace**, which is what arms the bleed at all.
    working.neglect_turns = 9;

    let quote = |working: &core_sim::DepositSource, builders: u32| {
        let rung = ladder.rung(RungKey::ForestryCoppice);
        let rot = core_sim::deposit_meter_rot(working, measure, &ladder);
        let balance = rung.build_balance(
            Some(core_sim::Improvement::Coppice),
            true,
            builders,
            core_sim::NO_BUILD_GEAR,
            rot,
            1.0,
        );
        (
            rot,
            core_sim::build_turns_estimate(
                base + width,
                working.ladder_position() - base,
                balance,
                true,
                builders,
            ),
        )
    };

    // Nobody keeping it, and one builder: the pass takes 1.5 a turn and the crew banks 1.0.
    let (rot, turns) = quote(&working, 1);
    assert!(
        rot > 0.0,
        "**LIVENESS**: an unkept coppice really is bleeding"
    );
    assert_eq!(
        turns,
        Some(core_sim::BuildTurns::Rotting),
        "a build losing ground to its own decay must not publish a finite date"
    );

    // Now pay the bill in full. The same crew gets a real number.
    working.upkeep_supplied = bill;
    let (rot, turns) = quote(&working, 1);
    assert_eq!(rot, 0.0, "a fully kept meter loses nothing");
    assert!(
        matches!(turns, Some(core_sim::BuildTurns::Turns(_))),
        "and the same one builder then finishes it, so the Rotting above is about the KEEPING and \
         not about the crew: got {turns:?}"
    );
}

/// ⛔ **PUTTING A WORKING DOWN STOPS THE BILL AT ONCE, AND ITS NEIGHBOUR STOPS SLIDING**
/// (issue #650) — the gameplay claim `abandon_working` exists for, measured against the leak it
/// closes.
///
/// **The leak.** A working raised above its free floor is a *holding*
/// (`systems::labor::source_has_a_meter_at_risk`), so a crew of zero keeps the row —
/// `assign_labor … extract … 0` is *"stop cutting"*, never *"this band has nothing here"*. And
/// `extraction_keeping_claims` reads the **row**, so an abandoned working goes on claiming its share
/// of the band's one `quarrywork` pool for the whole ~104 turns its meter takes to slide back to the
/// free floor. Under the default `UpkeepFundMode::Spread` that share comes out of the workings the
/// band still wants.
///
/// **Measured on this fixture, before the verb existed**: one keeper covers one `forestry:felling`
/// working on the reference wood exactly (1.0 work a turn), so the live working holds at its seated
/// **60.0** for ever alone — and slides to **49.96 in 40 turns** the moment a walked-away sibling
/// sits beside it, with no command able to drop the sibling.
///
/// **The two arms are one drive apart**, because either alone is weak: an arm that only asserted the
/// held reading describes a fixture where nothing was ever at risk, and one that only asserted the
/// slide describes a keeping pool that pays nothing.
#[test]
fn putting_a_working_down_stops_its_bill_and_its_neighbour_stops_sliding() {
    /// Long enough for the slide to be unmistakable and short of the grace-plus-bleed the sibling
    /// needs to reach its own free floor, so the leak is still running when the arm ends.
    const A_SPELL_OF_NEGLECT: u32 = 40;

    let live = UVec2::new(0, 0);
    let walked_away = UVec2::new(1, 0);
    let drive = |put_it_down: bool| {
        let (mut world, home) = world_of(WOODED);
        seat_working(&mut world, live, WOOD, RungKey::ForestryFelling);
        seat_working(&mut world, walked_away, WOOD, RungKey::ForestryFelling);
        let band = spawn_keepers(
            &mut world,
            home,
            &[(live, WOOD), (walked_away, WOOD)],
            2,
            // One keeper: exactly one felling working's bill on the reference wood, so the second
            // working is the whole of what makes the pool short.
            1,
        );
        if put_it_down {
            // **The verb's own seam**, which is what `handle_abandon_working` calls per band: the
            // row goes and `drop_source_row`'s prune takes the working's queue entry with it.
            core_sim::drop_holding_and_cancel_ring(
                &mut world,
                band,
                &LaborTarget::Extract {
                    tile: walked_away,
                    material: WOOD.to_string(),
                    floor: A_FRESH_ASSIGNMENTS_FLOOR,
                },
            );
        }
        for _ in 0..A_SPELL_OF_NEGLECT {
            run_full_turn(&mut world);
        }
        let demand = world
            .get::<LaborAllocation>(band)
            .expect("the fixture band survives the drive")
            .last_quarrywork_demand;
        (
            position(&world, live, WOOD),
            position(&world, walked_away, WOOD),
            demand,
        )
    };

    let (kept_live, _, kept_demand) = drive(false);
    let seated = {
        let (mut world, _) = world_of(WOODED);
        seat_working(&mut world, live, WOOD, RungKey::ForestryFelling);
        position(&world, live, WOOD)
    };
    assert!(
        kept_live < seated,
        "**THE LEAK**: while the band still holds the walked-away working, the one it wants slides \
         — {kept_live} against a seated {seated}"
    );

    let (dropped_live, dropped_walked, dropped_demand) = drive(true);
    assert_eq!(
        dropped_live, seated,
        "with the walked-away working put down, the one keeper covers what is left and the live \
         working holds: {dropped_live} against {seated}"
    );
    assert!(
        dropped_demand < kept_demand,
        "and the band's bill falls to one working's: {dropped_demand} against {kept_demand}"
    );
    assert!(
        dropped_walked < seated,
        "**THE METER IS UNTOUCHED, NOT DESTROYED**: nobody holds the walked-away working now, so \
         `advance_deposits` slides it back at the rung's own rate exactly as it slides an unkept \
         one — {dropped_walked} against a seated {seated}"
    );
}

// ---------------------------------------------------------------------------------------------
// The working on the wire (`docs/plan_extraction.md` §7, issue #650)
// ---------------------------------------------------------------------------------------------
//
// Every assertion below reads the **encoded envelope** through `root_as_envelope`, never the
// in-process `DepositSource`: a field that never reached the codec still passes an in-process
// assertion, and the deposit section has no client reader yet to notice.
//
// The fixtures drive **whole turns** through `core_sim::build_test_app`, so the numbers under test
// are the ones the real stage order produced — Logistics stamps the bill and renews the stock,
// Population takes and pays, and the Snapshot stage publishes what they left.

mod wire {
    use bevy::app::App;
    use bevy::math::UVec2;
    use bevy::prelude::{Entity, With};

    use std::sync::Arc;

    use core_sim::extraction::{tile_deposit_capacity, DepositRegistry, DepositSource};
    use core_sim::{
        build_test_app, build_work_per_worker_turn, deposit_rungs_in_climb_order, BandId,
        ExtractionConfig, LaborAllocation, LaborTarget, LadderConfig, LadderConfigHandle,
        PopulationCohort, ResidentBand, RungKey, SnapshotHistory, Tile, TileRegistry,
        ViewerFaction, VisibilityLedger, BUILTIN_INTENSIFICATION_LADDER, MSY_BIOMASS_FRACTION,
        NO_BUILD_GEAR, NO_DEPOSIT_FLOOR, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED,
    };
    use sim_schema::{TerrainType, DEPOSIT_RUNWAY_NOT_APPLICABLE, DEPOSIT_RUNWAY_NO_TAKE};

    use super::A_FRESH_ASSIGNMENTS_FLOOR;

    /// **The renewing half of the §7 fork.** Mixed woodland carries 600 wood at a rate of 0.03 —
    /// and 35 stone at 0.02 beside it, which is what makes *"one tile can hold two"* a fact this
    /// fixture could exercise without a second terrain.
    const RENEWING_GROUND: TerrainType = TerrainType::MixedWoodland;
    /// **The finite half.** The largest rock body on the shipped table, at a rate of exactly zero —
    /// and it carries timber too, which is what lets one hex hold a worked source beside an
    /// untouched one.
    const FINITE_GROUND: TerrainType = TerrainType::AlpineMountain;
    /// **Ground that holds neither material** — a glacier is absent from both `by_terrain` tables,
    /// which is `extraction.json`'s `_comment_absence`: there is no `enabled` flag and no parked
    /// `0.0` row, so absence is the whole of *"there is nothing here"*.
    const BARE_GROUND: TerrainType = TerrainType::Glacier;

    const WOOD: &str = "wood";
    const STONE: &str = "stone";

    /// **The crew on each take row**, and the keepers on the band's `quarrywork` pool. One keeper
    /// deliberately does **not** cover the seated quarry's bill, which is what makes the published
    /// `demand − supplied == shortfall` identity a statement about three different numbers.
    const A_TAKE_CREW: u32 = 1;
    const TOO_FEW_KEEPERS: u32 = 1;

    /// One published working, read off the encoded envelope.
    #[derive(Debug, Clone)]
    struct PublishedWorking {
        tile: UVec2,
        material: String,
        branch: String,
        stock: f32,
        capacity: f32,
        reachable: f32,
        floor: f32,
        rung_floor_fraction: f32,
        per_worker_biomass: f32,
        regrowth_samples: Vec<f32>,
        regrowth_rate: f32,
        rung: String,
        sustainable_take: f32,
        actual_take: f32,
        turns_remaining: i32,
        demand: f32,
        supplied: f32,
        shortfall: f32,
        workers_needed: u32,
        has_neglect_grace: bool,
        build_turns_remaining: i32,
        build_blocked_reason: String,
        is_queued: bool,
        upkeep_kit_id: String,
    }

    /// The band's `quarrywork*` trio, read off the encoded envelope.
    #[derive(Debug, Clone, Copy)]
    struct PublishedQuarrywork {
        demand: f32,
        supplied: f32,
        shortfall: f32,
    }

    /// One published `extract` labor row.
    #[derive(Debug, Clone)]
    struct PublishedExtractRow {
        tile: UVec2,
        material: String,
    }

    fn encoded(app: &App) -> Vec<u8> {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
    }

    fn published_workings(app: &App) -> Vec<PublishedWorking> {
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        let bytes = encoded(app);
        let envelope =
            fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
        let section = envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .subsistence()
            .and_then(|section| section.deposits())
            .expect("the deposit section is published");
        section
            .iter()
            .map(|row| PublishedWorking {
                tile: UVec2::new(row.tileX(), row.tileY()),
                material: row
                    .material()
                    .expect("a working names its material")
                    .to_string(),
                branch: row
                    .branch()
                    .expect("a working names its branch")
                    .to_string(),
                stock: row.stock(),
                capacity: row.capacity(),
                reachable: row.reachable(),
                floor: row.floor(),
                rung_floor_fraction: row.rungFloorFraction(),
                per_worker_biomass: row.perWorkerBiomass(),
                regrowth_samples: row
                    .regrowthSamples()
                    .map(|samples| samples.iter().collect())
                    .unwrap_or_default(),
                regrowth_rate: row.regrowthRate(),
                rung: row
                    .rung()
                    .expect("a working publishes its rung")
                    .to_string(),
                sustainable_take: row.sustainableTake(),
                actual_take: row.actualTake(),
                turns_remaining: row.turnsRemaining(),
                demand: row.upkeepDemand(),
                supplied: row.upkeepSupplied(),
                shortfall: row.upkeepShortfall(),
                workers_needed: row.upkeepWorkersNeeded(),
                has_neglect_grace: row.hasNeglectGrace(),
                build_turns_remaining: row.buildTurnsRemaining(),
                build_blocked_reason: row
                    .buildBlockedReason()
                    .expect("the cause is published, empty or not")
                    .to_string(),
                is_queued: row.isQueued(),
                upkeep_kit_id: row
                    .upkeepKitId()
                    .expect("the keeping kit is published")
                    .to_string(),
            })
            .collect()
    }

    fn published_working(app: &App, tile: UVec2, material: &str) -> PublishedWorking {
        published_workings(app)
            .into_iter()
            .find(|row| row.tile == tile && row.material == material)
            .unwrap_or_else(|| {
                panic!("the {material} working at {tile:?} reached the viewer's frame")
            })
    }

    /// The band's `quarrywork*` trio and its `extract` rows, off one decode of the envelope. The
    /// campaign runs one cohort per test, so the sole published row is the band's.
    fn published_band(app: &App) -> (PublishedQuarrywork, Vec<PublishedExtractRow>) {
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        let bytes = encoded(app);
        let envelope =
            fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
        let cohorts = envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the population section is published");
        let row = cohorts
            .iter()
            .next()
            .expect("the campaign publishes at least one cohort");
        let rows = row
            .laborAssignments()
            .expect("the cohort publishes its labor rows")
            .iter()
            .filter(|assignment| assignment.kind().is_some_and(|kind| kind == "extract"))
            .map(|assignment| PublishedExtractRow {
                tile: UVec2::new(assignment.targetX(), assignment.targetY()),
                material: assignment
                    .material()
                    .expect("an extract row publishes its material")
                    .to_string(),
            })
            .collect();
        (
            PublishedQuarrywork {
                demand: row.quarryworkDemand(),
                supplied: row.quarryworkSupplied(),
                shortfall: row.quarryworkShortfall(),
            },
            rows,
        )
    }

    /// The campaign's first resident band, with the viewer pinned to its faction so what is
    /// published is what *this* people can see. The last term is the band's working-age pool, which
    /// every `set_assignment` below clamps against.
    fn first_band(app: &mut App) -> (Entity, UVec2, u32) {
        let (entity, faction, tile, working) = {
            let mut query = app
                .world
                .query_filtered::<(Entity, &PopulationCohort, &BandId), With<ResidentBand>>();
            let (entity, cohort, _band) = query
                .iter(&app.world)
                .next()
                .expect("the campaign spawns at least one resident band");
            (
                entity,
                cohort.faction,
                cohort.current_tile,
                cohort.working.to_f32() as u32,
            )
        };
        let position = app
            .world
            .get::<Tile>(tile)
            .expect("a band stands on a real tile")
            .position;
        app.world.insert_resource(ViewerFaction(faction));
        (entity, position, working)
    }

    /// **Re-ground a tile**, so a fixture about a deposit is not at the mercy of what the generated
    /// map happened to put under the band. The deposits table reads `tile.terrain` and nothing
    /// else, so this is the whole of what decides which deposit stands here.
    fn reground(app: &mut App, tile: UVec2, terrain: TerrainType) {
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(tile.x, tile.y)
            .expect("the fixture tile is on the map");
        app.world
            .get_mut::<Tile>(entity)
            .expect("the fixture tile carries terrain")
            .terrain = terrain;
    }

    /// **Seat a working exactly on a rung's top**, written straight into the registry so a keeping
    /// fixture does not have to spend 250 work units of builders getting there first.
    fn seat_working(app: &mut App, tile: UVec2, material: &str, rung: RungKey) {
        let ladder = LadderConfig::builtin();
        let config = ExtractionConfig::builtin();
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(tile.x, tile.y)
            .expect("the fixture tile is on the map");
        let ground = app
            .world
            .get::<Tile>(entity)
            .expect("the fixture tile carries terrain");
        let capacity = tile_deposit_capacity(&config, material, ground);
        assert!(capacity > 0.0, "fixture: that ground must hold {material}");
        let mut working = DepositSource::opening(tile, material, capacity, rung.branch());
        let (base, width) = core_sim::extraction::deposit_rung_span(rung, &ladder);
        working.set_ladder_position(base + width, &ladder, rung.branch());
        assert_eq!(working.rung(), rung, "fixture: seated on the wrong rung");
        app.world.resource_mut::<DepositRegistry>().insert(working);
    }

    /// Put a take crew of `cutters` on each working and `keepers` hands on the band's `quarrywork`
    /// pool. The crew size is a parameter because the over-cut pair is a statement *about* it: the
    /// same wood reads within its means at one cutter and over-cut at enough of them.
    fn staff(
        app: &mut App,
        band: Entity,
        workings: &[(UVec2, &str)],
        available: u32,
        cutters: u32,
        keepers: u32,
    ) {
        staff_at_floor(
            app,
            band,
            workings,
            available,
            cutters,
            keepers,
            A_FRESH_ASSIGNMENTS_FLOOR,
        );
    }

    /// The same, with the crews told where to stop — the escapement dial (issue #650), which every
    /// fixture above passes at the identity because it predates the dial.
    #[allow(clippy::too_many_arguments)] // one fixture, one parameter per thing it states
    fn staff_at_floor(
        app: &mut App,
        band: Entity,
        workings: &[(UVec2, &str)],
        available: u32,
        cutters: u32,
        keepers: u32,
        floor: f32,
    ) {
        let mut allocation = LaborAllocation::default();
        for (tile, material) in workings {
            allocation.set_assignment(
                LaborTarget::Extract {
                    tile: *tile,
                    material: (*material).to_string(),
                    floor,
                },
                cutters,
                available,
                None,
            );
        }
        allocation.set_assignment(LaborTarget::Quarrywork, keepers, available, None);
        app.world.entity_mut(band).insert(allocation);
    }

    /// **A wooded tile beside a rock body, both worked, one turn resolved** — the fixture every
    /// test below reads. It returns the two tiles in the order `(renewing, finite)`.
    fn a_wood_and_a_quarry() -> (App, UVec2, UVec2) {
        let mut app = build_test_app();
        app.update();
        let (band, home, working) = first_band(&mut app);
        let width = app.world.resource::<TileRegistry>().width;
        let rock = UVec2::new((home.x + 1) % width, home.y);
        reground(&mut app, home, RENEWING_GROUND);
        reground(&mut app, rock, FINITE_GROUND);
        // **The rock working stands at the QUARRY rung**, so it owes a real keeping bill and the
        // published quad is three different numbers rather than three zeros.
        seat_working(&mut app, rock, STONE, RungKey::ExtractionQuarry);
        staff(
            &mut app,
            band,
            &[(home, WOOD), (rock, STONE)],
            working,
            A_TAKE_CREW,
            TOO_FEW_KEEPERS,
        );
        app.update();
        (app, home, rock)
    }

    /// **THE TEXTBOOK MSY OF A LOGISTIC STOCK** — the growth term `r·S·(1 − S/K)` read at its peak
    /// `S = MSY_BIOMASS_FRACTION · K`, which is what `deposit_sustainable_take` must answer.
    ///
    /// Written out from the curve rather than borrowed from `deposit_regrowth`, so the assertions
    /// below are an independent statement about the shape and not a restatement of the code under
    /// test. `rate` is the row's **published** `regrowthRate` — the ground's own rate already scaled
    /// by what the rung bought — so a coppiced wood sustains twice what a felled one does for free.
    fn msy_of(rate: f32, capacity: f32) -> f32 {
        let peak = MSY_BIOMASS_FRACTION * capacity;
        rate * peak * (1.0 - MSY_BIOMASS_FRACTION)
    }

    /// **The margin every sustainable-take comparison is made to** — these are single-precision
    /// products of three config numbers, so an exact `==` would be a statement about float layout
    /// rather than about the curve.
    const A_CLOSE_ENOUGH_TAKE: f32 = 1e-4;

    /// **The margin on a comparison of WHOLE-DEPOSIT quantities**, where the one above is sized for
    /// a per-turn take. A rock body is in the thousands, so a single-precision product of a fraction
    /// and a capacity carries three or four fewer decimal places than one of a rate and a crew.
    const A_CLOSE_ENOUGH_BODY: f32 = 1e-2;

    /// ⛔ **THE ⚠ MUST NOT FIRE ON THE MOST ORDINARY ACTION IN THE FEATURE** (issue #650).
    ///
    /// The sustainable half used to be the growth term read at the deposit's *current* stock. A
    /// mature wood stands at `K`, where `(1 − S/K)` is zero, so the very first cut read as
    /// over-drawing — and it never cleared: the stock converges on the point where growth equals the
    /// take *from above*, an asymptote, so `actual > sustainable` stayed true for ever on a harvest
    /// fifteen times inside the wood's means. A warning that fires on correct play teaches players
    /// to ignore it.
    ///
    /// It is now the **MSY** reading, `fauna::sustainable_yield`'s own expression with the deposit's
    /// curve substituted for the food web's — the same answer a full forage patch gives, which is
    /// what `docs/plan_extraction.md` §7 means by *"the existing breakdown pointed at a new source"*.
    #[test]
    fn a_full_wood_sustains_an_ordinary_crew_rather_than_warning_on_the_first_cut() {
        let (mut app, wood_tile, _rock) = a_wood_and_a_quarry();
        for turn in 0..A_FEW_TURNS_OF_CUTTING {
            let wood = published_working(&app, wood_tile, WOOD);
            assert!(
                (wood.sustainable_take - msy_of(wood.regrowth_rate, wood.capacity)).abs()
                    < A_CLOSE_ENOUGH_TAKE,
                "the sustainable half is the MSY of the deposit's curve, not the growth at today's \
                 stock (turn {turn}): {wood:?}"
            );
            assert!(
                wood.actual_take <= wood.sustainable_take,
                "one cutter on a whole wood is inside its means, so the over-cut pair must stay \
                 quiet (turn {turn}): {wood:?}"
            );
            app.update();
        }
    }

    /// Long enough for the old defect's *"it clears in a turn or two"* defence to be false, and short
    /// enough to stay a unit test. The stock only converges on its equilibrium asymptotically, so no
    /// finite count proves it clears — what this count buys is that it demonstrably does not.
    const A_FEW_TURNS_OF_CUTTING: u32 = 8;

    /// ⛔ **AND THE ⚠ MUST STILL HAVE TEETH.** The MSY reading is not a way of never warning: a crew
    /// whose take out-runs `r·K/4` is ruining the wood however full it looks today, and that is
    /// precisely the state §7's pair exists to name. Asserted on the **same ground and rung** as the
    /// quiet case above, so the only thing that differs is the number of hands.
    #[test]
    fn a_crew_that_out_cuts_the_msy_reads_as_over_cutting() {
        let mut app = build_test_app();
        app.update();
        let (band, home, working) = first_band(&mut app);
        reground(&mut app, home, RENEWING_GROUND);
        // **Felling, not deadfall** — the free floor's 0.3 a worker would need more hands than the
        // band has to out-cut mixed woodland's 4.5. Over-cutting first becomes *possible* at
        // `felling`, which is exactly why that rung is what teaches conservationism.
        seat_working(&mut app, home, WOOD, RungKey::ForestryFelling);
        staff(
            &mut app,
            band,
            &[(home, WOOD)],
            working,
            A_CREW_THAT_OUT_CUTS_A_WOOD,
            TOO_FEW_KEEPERS,
        );
        app.update();

        let wood = published_working(&app, home, WOOD);
        assert!(
            (wood.sustainable_take - msy_of(wood.regrowth_rate, wood.capacity)).abs()
                < A_CLOSE_ENOUGH_TAKE,
            "fixture: the same MSY reading as the quiet case: {wood:?}"
        );
        assert!(
            wood.actual_take > wood.sustainable_take,
            "a take above the wood's MSY is over-cutting and must read as it: {wood:?}"
        );
    }

    /// Three fellers at `forestry:felling`'s 2.0 a turn take 6.0 against mixed woodland's MSY of
    /// 4.5 — over the line by a margin no rounding closes, and small enough for a starting band's
    /// working-age pool to actually field.
    const A_CREW_THAT_OUT_CUTS_A_WOOD: u32 = 3;

    /// ⛔ **THE LEVER ACTUALLY WORKS: RAISING THE FLOOR CLEARS THE OVER-CUT WARNING** (issue #650).
    ///
    /// This is the whole point of giving the deposit branches an escapement dial. Before it, the sim
    /// told a player they were cutting faster than the wood grows and offered no answer but pulling
    /// people off the job; now the warning is a consequence the player chose and can un-choose.
    ///
    /// It is the **same ground, the same rung and the same crew** as
    /// `a_crew_that_out_cuts_the_msy_reads_as_over_cutting` — the only thing that differs is the
    /// dial, which is what makes this a statement about the dial.
    #[test]
    fn raising_the_floor_above_the_stand_clears_the_over_cut_warning() {
        let mut app = build_test_app();
        app.update();
        let (band, home, working) = first_band(&mut app);
        reground(&mut app, home, RENEWING_GROUND);
        seat_working(&mut app, home, WOOD, RungKey::ForestryFelling);
        staff_at_floor(
            &mut app,
            band,
            &[(home, WOOD)],
            working,
            A_CREW_THAT_OUT_CUTS_A_WOOD,
            TOO_FEW_KEEPERS,
            LEAVE_THE_WHOLE_STAND,
        );
        app.update();

        let wood = published_working(&app, home, WOOD);
        assert!(
            wood.sustainable_take > 0.0,
            "fixture: the wood must still have a means to be inside — a zero would make the \
             comparison below vacuous: {wood:?}"
        );
        assert!(
            wood.actual_take <= wood.sustainable_take,
            "the crew that over-cut at the free dial is inside the wood's means once told to leave \
             it standing: {wood:?}"
        );
        assert_eq!(
            wood.floor, LEAVE_THE_WHOLE_STAND,
            "…and the working publishes the floor its crews worked to: {wood:?}"
        );
        assert_eq!(
            wood.reachable, 0.0,
            "which leaves nothing above it for anyone to reach: {wood:?}"
        );
    }

    /// **THE TOP OF THE DIAL** — leave the whole stand, take nothing. Used rather than a value just
    /// under the crossing so the assertion is about the dial and not about a rate a retune of
    /// `forestry:felling` would move.
    const LEAVE_THE_WHOLE_STAND: f32 = 1.0;

    /// **THE CHART TERMS REACH THE CLIENT** — the escapement instrument on a deposit is the plant
    /// web's, and it is composed from the same three things: a stock against a capacity, a sampled
    /// growth curve, and one worker's throughput.
    ///
    /// ⛔ **PLUS ONE A PATCH DOES NOT HAVE — `rungFloorFraction`.** A deposit has a *second* floor,
    /// and a projection that walked the stock down to the player's alone would draw a crew reaching
    /// past ground its rung cannot touch. It is published in the **same units** as the floor beside
    /// it precisely so the client composes them as a maximum.
    #[test]
    fn a_deposit_row_carries_the_floor_and_the_terms_the_chart_is_drawn_from() {
        let (app, wood_tile, rock_tile) = a_wood_and_a_quarry();

        let wood = published_working(&app, wood_tile, WOOD);
        assert!(
            wood.per_worker_biomass > 0.0,
            "one cutter's own rate is what every crew target divides by: {wood:?}"
        );
        assert!(
            wood.regrowth_samples.len() >= 2,
            "a curve needs at least two points to interpolate between: {wood:?}"
        );
        assert!(
            wood.regrowth_samples.iter().any(|delta| *delta > 0.0),
            "**LIVENESS**: a wood really grows, so its curve is not all zeros: {wood:?}"
        );
        assert!(
            wood.regrowth_samples.iter().all(|delta| *delta >= 0.0),
            "and a deposit has no Allee term, so no sample may be negative: {wood:?}"
        );
        // ⛔ **THE PUBLISHED REACH IS THE STOCK ABOVE THE GREATER OF THE TWO FLOORS** — the whole
        // reason both are on the wire, and the arithmetic a client's projection has to reproduce.
        assert!(
            (wood.reachable
                - (wood.stock - wood.rung_floor_fraction.max(wood.floor) * wood.capacity))
                .abs()
                < A_CLOSE_ENOUGH_TAKE,
            "the published reach is the stock above `max(rung floor, crew floor)`: {wood:?}"
        );

        // ⛔ **A QUARRY'S CURVE IS ALL ZEROS AND IS STILL PUBLISHED** — *"this does not grow"* is a
        // reading, where an empty vector would be *"no curve was sent"* and blank the chart.
        let rock = published_working(&app, rock_tile, STONE);
        assert_eq!(
            rock.regrowth_samples.len(),
            wood.regrowth_samples.len(),
            "one x-axis for every curve on the wire: {rock:?}"
        );
        assert!(
            rock.regrowth_samples.iter().all(|delta| *delta == 0.0),
            "rock's rate is zero, so its curve is flat at zero rather than absent: {rock:?}"
        );
        // ⛔ **AND ON THE ROCK ROW THE CREW'S FLOOR IS PUBLISHED AND DOES NOT BIND** (issue #650).
        // A floor protects regrowth and rock has none, so `extraction:quarry`'s own remainder is the
        // only floor a quarry has — even though the row carries the shipped default of 0.5, which is
        // the *deeper* of the two and would bind on any renewing ground.
        assert!(
            rock.rung_floor_fraction > 0.0 && rock.floor > rock.rung_floor_fraction,
            "fixture: the crew's floor must be the DEEPER of the two here, or this asserts nothing \
             about which one was dropped: {rock:?}"
        );
        assert!(
            (rock.reachable - (rock.stock - rock.rung_floor_fraction * rock.capacity)).abs()
                < A_CLOSE_ENOUGH_TAKE,
            "a finite working reaches everything above its RUNG's floor: {rock:?}"
        );
        assert!(
            rock.reachable > rock.stock - rock.floor * rock.capacity,
            "…and strictly more than the crew's floor would have left it: {rock:?}"
        );
    }

    /// ⛔ **A QUARRY REACHES ITS RUNG'S 85% AT THE FLOOR EVERY ROW CARRIES BY DEFAULT** (issue #650),
    /// and the runway and the take agree with it.
    ///
    /// The client offers the dial only where `regrowthRate > 0`, so a finite working's row carries
    /// the omitted-token default of `0.5` — and `max(rung floor, player floor)` let that bind
    /// **above** `extraction:quarry`'s own 0.15. A crew stopped at half a rock body while the same
    /// sheet's verdict promised 85% of it, which is the entire argument for paying 250 work and 8
    /// wood to open one.
    ///
    /// **What is asserted is the SUM** — what the crew has taken plus what it can still reach is the
    /// rung's whole recovery of the body — because that is the promise the readout makes and it is
    /// one number rather than a pair that could each drift. Against the defect it read `0.50`.
    #[test]
    fn a_quarry_at_the_default_floor_reaches_the_whole_of_what_its_rung_recovers() {
        let (app, _wood, rock_tile) = a_wood_and_a_quarry();
        let rock = published_working(&app, rock_tile, STONE);

        assert_eq!(
            rock.floor, A_FRESH_ASSIGNMENTS_FLOOR,
            "fixture: the row carries the default the grammar supplies — stored and published on a \
             finite working, and inert: {rock:?}"
        );
        assert!(
            rock.floor > rock.rung_floor_fraction,
            "fixture: and it is the DEEPER of the two, or the maximum would never have bound: \
             {rock:?}"
        );
        assert!(
            rock.actual_take > 0.0,
            "**LIVENESS**: the crew must actually have cut, or the sum below is the reach alone: \
             {rock:?}"
        );

        let recovered = (1.0 - rock.rung_floor_fraction) * rock.capacity;
        assert!(
            (rock.reachable + rock.actual_take - recovered).abs() < A_CLOSE_ENOUGH_BODY,
            "the quarry crew must be able to work {recovered} of the body — what it has cut plus \
             what it can still reach — not {}: {rock:?}",
            rock.reachable + rock.actual_take
        );
        assert!(
            rock.reachable > rock.stock - rock.floor * rock.capacity,
            "…which is strictly more than the sent floor would have left it: {rock:?}"
        );
        assert_eq!(
            rock.turns_remaining,
            (rock.reachable / rock.actual_take).floor() as i32,
            "and the runway is projected off that same reach, so the two readouts agree: {rock:?}"
        );
    }

    /// ⛔ **THE §7 FORK, AND IT IS DECIDED BY THE RATE RATHER THAN BY THE BRANCH.**
    ///
    /// A wood renews, so it does not run out: it quotes the *not applicable* sentinel and warns
    /// with the over-cut pair instead. A quarry's rate is zero, so it has no take to sustain and
    /// answers with the runway. The two are asserted **against each other in one run**, on the same
    /// code path with no branch anywhere between them.
    #[test]
    fn a_renewing_working_quotes_the_pair_and_a_finite_one_quotes_the_runway() {
        let (app, wood_tile, rock_tile) = a_wood_and_a_quarry();
        let wood = published_working(&app, wood_tile, WOOD);
        let rock = published_working(&app, rock_tile, STONE);

        assert!(
            wood.regrowth_rate > 0.0,
            "fixture: mixed woodland renews, so the wood row must take the over-cut fork: {wood:?}"
        );
        assert_eq!(
            wood.turns_remaining, DEPOSIT_RUNWAY_NOT_APPLICABLE,
            "a working that renews does not run out, so it quotes no runway: {wood:?}"
        );
        assert!(
            wood.sustainable_take > 0.0,
            "a renewing working's warning IS the sustainable-versus-actual pair, so the \
             sustainable half must be a real rate: {wood:?}"
        );

        assert_eq!(
            rock.regrowth_rate, 0.0,
            "fixture: an alpine rock body never renews: {rock:?}"
        );
        assert_eq!(
            rock.sustainable_take, 0.0,
            "there is no take a quarry can hold indefinitely, and 0 is the honest answer rather \
             than a gap: {rock:?}"
        );
        assert!(
            rock.actual_take > 0.0,
            "fixture: the quarry crew must have cut something for there to be a rate to project: \
             {rock:?}"
        );
        assert!(
            rock.turns_remaining >= 0,
            "a finite working being cut quotes a real count of turns: {rock:?}"
        );
        assert_eq!(
            rock.turns_remaining,
            (rock.reachable / rock.actual_take).floor() as i32,
            "the runway is a FORWARD projection of this turn's own take, never a trailing \
             average: {rock:?}"
        );
    }

    /// **The row's identity is the PAIR**, and the branch, the rung and the ground come with it.
    #[test]
    fn a_worked_deposit_publishes_a_row_keyed_by_tile_and_material() {
        let (app, wood_tile, rock_tile) = a_wood_and_a_quarry();
        let wood = published_working(&app, wood_tile, WOOD);
        let rock = published_working(&app, rock_tile, STONE);

        assert_eq!(
            wood.branch, "forestry",
            "wood is worked by forestry: {wood:?}"
        );
        assert_eq!(
            wood.rung, "forestry:deadfall",
            "a fresh working opens on its branch's free floor: {wood:?}"
        );
        assert_eq!(
            rock.branch, "extraction",
            "stone is worked by extraction: {rock:?}"
        );
        assert_eq!(
            rock.rung, "extraction:quarry",
            "the seated rock working holds the rung it was seated on: {rock:?}"
        );
        assert!(
            rock.capacity > wood.capacity,
            "capacity is read LIVE off the tile, so the rock body must out-measure the wood: \
             {rock:?} vs {wood:?}"
        );
        assert!(
            rock.reachable <= rock.stock,
            "a rung reaches at most what is standing: {rock:?}"
        );
        assert!(
            rock.stock < rock.capacity,
            "fixture: the quarry crew drew the body down this turn: {rock:?}"
        );
        // **A working nobody has queued is not blocked, it is simply not being built** — and its
        // countdown is the honest *no estimate* rather than the `-5` a client used to hardcode.
        assert!(!rock.is_queued, "no band queued a build here: {rock:?}");
        assert_eq!(
            rock.build_blocked_reason, "",
            "an unqueued working publishes no cause: {rock:?}"
        );
        assert_eq!(
            rock.build_turns_remaining,
            sim_schema::NO_BUILD_TURNS_ESTIMATE,
            "a rung nobody ordered has no quote, and never a 0 that renders as finished: {rock:?}"
        );
        assert!(
            !rock.upkeep_kit_id.is_empty(),
            "a worked working resolves a keeping kit, the bare-handed one included: {rock:?}"
        );
    }

    /// ⛔ **`demand − supplied == shortfall` HOLDS VERBATIM ON BOTH QUADS**, and all three numbers
    /// are different — a quad of zeros would pass this identity while saying nothing.
    #[test]
    fn the_standing_bill_holds_its_identity_on_the_working_and_on_the_band() {
        let (app, wood_tile, rock_tile) = a_wood_and_a_quarry();
        let rock = published_working(&app, rock_tile, STONE);
        let (quarrywork, _) = published_band(&app);

        assert!(
            rock.demand > 0.0 && rock.supplied > 0.0 && rock.shortfall > 0.0,
            "fixture: one keeper must part-pay a real bill, or the identity is three zeros: \
             {rock:?}"
        );
        assert_eq!(
            rock.shortfall,
            rock.demand - rock.supplied,
            "the working's quad reads the STAMPED basis, so the identity is verbatim: {rock:?}"
        );
        assert!(
            rock.workers_needed > TOO_FEW_KEEPERS,
            "the bill wants more keepers than the band staffed: {rock:?}"
        );
        assert!(
            rock.has_neglect_grace,
            "a built rung has a meter to lose, so there is a countdown here: {rock:?}"
        );

        assert_eq!(
            quarrywork.shortfall,
            quarrywork.demand - quarrywork.supplied,
            "the band's quarrywork triple holds the same identity: {quarrywork:?}"
        );
        assert_eq!(
            quarrywork.demand, rock.demand,
            "the band keeps exactly this one billed working, so its summed demand is that \
             working's: {quarrywork:?} vs {rock:?}"
        );

        // **The free floor owes nothing, and that is what makes it free.**
        let wood = published_working(&app, wood_tile, WOOD);
        assert_eq!(
            wood.demand, 0.0,
            "`forestry:deadfall` declares no upkeep at all: {wood:?}"
        );
        assert!(
            !wood.has_neglect_grace,
            "nothing is at risk on a free floor: {wood:?}"
        );
    }

    /// **An `extract` row names its deposit**, because `targetX`/`targetY` alone cannot tell a
    /// felling crew from a quarrying crew on the same hex.
    #[test]
    fn an_extract_labor_row_publishes_the_material_it_works() {
        let (app, wood_tile, rock_tile) = a_wood_and_a_quarry();
        let (_, rows) = published_band(&app);

        // Sorted on the wire spelling of the key, because `UVec2` carries no `Ord` and the row
        // order is the allocation's rather than anything this test may assume.
        let key = |tile: UVec2, material: &str| (tile.x, tile.y, material.to_string());
        let mut named: Vec<(u32, u32, String)> = rows
            .into_iter()
            .map(|row| key(row.tile, &row.material))
            .collect();
        named.sort();
        let mut expected = vec![key(wood_tile, WOOD), key(rock_tile, STONE)];
        expected.sort();
        assert_eq!(
            named, expected,
            "each extract row publishes both halves of the working's key"
        );
    }

    /// ⛔ **THE ROW IS ABOUT THE GROUND, AND THIS IS THE REGRESSION TEST FOR THE DEFECT** (issue
    /// #650). `deposit_states` published the registry, the registry is filled lazily by
    /// `DepositRegistry::open`, and the client builds its `Workings ▸` affordance off these rows —
    /// so a fresh world offered **no way to open the first working anywhere on the map**, and the
    /// feature was unreachable in a real game.
    ///
    /// It also pins *"one tile can hold two"* on ground nobody has touched, and both §7 readouts on
    /// an unopened row: a renewing deposit standing at capacity has **no growth left to quote** and
    /// a finite one nobody is cutting has **no rate to project**.
    #[test]
    fn ground_nobody_has_worked_still_publishes_what_it_holds() {
        let mut app = build_test_app();
        app.update();
        let (_band, home, _working) = first_band(&mut app);
        let width = app.world.resource::<TileRegistry>().width;
        let rock = UVec2::new((home.x + 1) % width, home.y);
        reground(&mut app, home, RENEWING_GROUND);
        reground(&mut app, rock, FINITE_GROUND);
        app.update();

        assert!(
            app.world.resource::<DepositRegistry>().is_empty(),
            "fixture: no band was ever put on a deposit, so the registry must still be empty — \
             this test is about the ground nobody has worked, not about a working"
        );

        // **Both materials on the one hex**, and neither of them opened.
        let mut on_the_wood: Vec<String> = published_workings(&app)
            .into_iter()
            .filter(|row| row.tile == home)
            .map(|row| row.material)
            .collect();
        on_the_wood.sort();
        assert_eq!(
            on_the_wood,
            vec![STONE.to_string(), WOOD.to_string()],
            "mixed woodland holds timber AND loose stone, so it publishes a row for each"
        );

        let wood = published_working(&app, home, WOOD);
        assert_eq!(
            wood.stock, wood.capacity,
            "a deposit nobody has worked stands at exactly its tile's capacity: {wood:?}"
        );
        assert!(
            wood.capacity > 0.0,
            "fixture: mixed woodland must hold timber: {wood:?}"
        );
        assert_eq!(
            wood.rung, "forestry:deadfall",
            "an unopened deposit stands on its branch's free floor: {wood:?}"
        );
        assert_eq!(
            wood.actual_take, 0.0,
            "nobody cut it, so nothing came out of it: {wood:?}"
        );
        assert_eq!(
            wood.demand, 0.0,
            "the free floor owes no keeping, which is what makes it free: {wood:?}"
        );
        assert!(
            !wood.has_neglect_grace,
            "nothing is at risk on a deposit nobody has raised: {wood:?}"
        );
        assert!(
            !wood.is_queued,
            "no band queued a build on ground nobody is standing on: {wood:?}"
        );
        assert_eq!(
            wood.build_turns_remaining,
            sim_schema::NO_BUILD_TURNS_ESTIMATE,
            "a rung nobody ordered has no quote: {wood:?}"
        );
        // **The kit passes are keyed `(tile, material)` and simply find nothing here** — the empty
        // answer rather than a fabricated tool, and never a panic on a missing key.
        assert_eq!(
            wood.upkeep_kit_id, "",
            "nobody is keeping this ground, so no keeping kit resolves: {wood:?}"
        );

        // **THE TWO §7 READOUTS ON AN UNOPENED ROW**, published as the seams compute them with no
        // special case for *"nobody has opened this"*.
        assert!(
            wood.regrowth_rate > 0.0,
            "fixture: mixed woodland renews, so this row takes the over-cut fork: {wood:?}"
        );
        assert_eq!(
            wood.turns_remaining, DEPOSIT_RUNWAY_NOT_APPLICABLE,
            "a deposit that renews does not run out, worked or not: {wood:?}"
        );
        assert!(
            (wood.sustainable_take - msy_of(wood.regrowth_rate, wood.capacity)).abs()
                < A_CLOSE_ENOUGH_TAKE,
            "an untouched wood quotes what it could keep paying for ever — its MSY — and NOT the \
             zero the logistic term reads at capacity: {wood:?}"
        );

        // The finite half: a rock body nobody is cutting *will* run out, just not while it stands
        // idle — which is a different sentinel from *"it never runs out"*.
        let stone = published_working(&app, rock, STONE);
        assert_eq!(
            stone.regrowth_rate, 0.0,
            "fixture: an alpine rock body never renews: {stone:?}"
        );
        assert_eq!(
            stone.stock, stone.capacity,
            "nobody quarried it, so the whole body is standing: {stone:?}"
        );
        assert_eq!(
            stone.turns_remaining, DEPOSIT_RUNWAY_NO_TAKE,
            "there is no take to project a finite deposit's runway off: {stone:?}"
        );
    }

    /// ⛔ **THE MERGE PREFERS THE REGISTRY; THE DERIVATION ONLY FILLS WHAT IS MISSING.** One hex,
    /// two materials — an alpine rock body holds timber as well as stone — with a crew on the stone
    /// alone. If a derived opening state could overwrite a live source, the seated quarry would read
    /// back at full stock on its free floor, which is precisely the row beside it.
    #[test]
    fn a_live_working_wins_over_the_derived_opening_state_on_one_tile() {
        let (app, _wood_tile, rock_tile) = a_wood_and_a_quarry();
        let worked = published_working(&app, rock_tile, STONE);
        let untouched = published_working(&app, rock_tile, WOOD);

        assert_eq!(
            worked.rung, "extraction:quarry",
            "the seated working keeps the rung it was seated on: {worked:?}"
        );
        assert!(
            worked.stock < worked.capacity,
            "the live source keeps the stock its crew drew down: {worked:?}"
        );
        assert!(
            worked.actual_take > 0.0,
            "fixture: the quarry crew must have cut something: {worked:?}"
        );

        assert_eq!(
            untouched.rung, "forestry:deadfall",
            "the timber on the same hex is nobody's working, so it opens on the free floor: \
             {untouched:?}"
        );
        assert_eq!(
            untouched.stock, untouched.capacity,
            "and it stands at the tile's whole capacity: {untouched:?}"
        );
        assert_eq!(
            untouched.actual_take, 0.0,
            "nobody felled anything here: {untouched:?}"
        );
    }

    /// **Absence is the answer** (`extraction.json`'s `_comment_absence`): a terrain absent from
    /// both tables holds nothing, so there is nothing to publish and nothing to work. The tile is
    /// the band's own, so it is unambiguously **discovered** — which is what makes the empty answer
    /// a statement about the ground rather than about the fog.
    #[test]
    fn ground_that_holds_neither_material_publishes_no_row() {
        let mut app = build_test_app();
        app.update();
        let (_band, home, _working) = first_band(&mut app);
        reground(&mut app, home, BARE_GROUND);
        app.update();

        let viewer = app.world.resource::<ViewerFaction>().0;
        assert!(
            app.world
                .resource::<VisibilityLedger>()
                .is_discovered(viewer, home.x, home.y),
            "fixture: the band stands here, so its own tile must be explored"
        );
        assert!(
            published_workings(&app).iter().all(|row| row.tile != home),
            "a glacier is on neither deposit table, so it publishes no row at all"
        );
    }

    /// One row of the published deposit rung catalog, read off the encoded envelope.
    #[derive(Debug, Clone)]
    struct PublishedRung {
        rung_key: String,
        branch: String,
        order: u32,
        display_name: String,
        verb: String,
        unlock_knowledge: String,
        requires_rung: String,
        earns_knowledge: String,
        work_cost: f32,
        upkeep_work_per_turn: f32,
        build_material_cost: f32,
        build_material_id: String,
        build_work_per_worker_turn: f32,
        yield_per_worker_turn: f32,
        recovery_fraction: f32,
        regrowth_multiplier: f32,
        min_deposit_capacity: f32,
    }

    /// **The `depositRungs` catalog off the encoded envelope**, through the accessor chain a client
    /// uses. It rides the subsistence section beside `routeRungs` — both are declarations of what a
    /// ladder holds, carrying no faction and no tile.
    fn published_deposit_rungs(app: &App) -> Vec<PublishedRung> {
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        let bytes = encoded(app);
        let envelope =
            fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
        let catalog = envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .subsistence()
            .and_then(|section| section.depositRungs())
            .expect("the deposit rung catalog is published");
        catalog
            .iter()
            .map(|row| PublishedRung {
                rung_key: row.rungKey().expect("a rung publishes its key").to_string(),
                branch: row.branch().expect("a rung names its branch").to_string(),
                order: row.order(),
                display_name: row
                    .displayName()
                    .expect("a rung publishes a display name")
                    .to_string(),
                verb: row
                    .verb()
                    .expect("the verb is published, empty or not")
                    .to_string(),
                unlock_knowledge: row
                    .unlockKnowledge()
                    .expect("the gate is published, empty or not")
                    .to_string(),
                requires_rung: row
                    .requiresRung()
                    .expect("the rung beneath is published, empty or not")
                    .to_string(),
                earns_knowledge: row
                    .earnsKnowledge()
                    .expect("the lesson is published, empty or not")
                    .to_string(),
                work_cost: row.workCost(),
                upkeep_work_per_turn: row.upkeepWorkPerTurn(),
                build_material_cost: row.buildMaterialCost(),
                build_material_id: row
                    .buildMaterialId()
                    .expect("the material id is published, empty or not")
                    .to_string(),
                build_work_per_worker_turn: row.buildWorkPerWorkerTurn(),
                yield_per_worker_turn: row.yieldPerWorkerTurn(),
                recovery_fraction: row.recoveryFraction(),
                regrowth_multiplier: row.regrowthMultiplier(),
                min_deposit_capacity: row.minDepositCapacity(),
            })
            .collect()
    }

    /// The catalog off a world with nothing opened on it — a catalog is a per-world constant, so it
    /// needs no working anywhere to be published.
    fn a_world_with_a_catalog() -> App {
        let mut app = build_test_app();
        app.update();
        app
    }

    fn published_rung(app: &App, key: &str) -> PublishedRung {
        published_deposit_rungs(app)
            .into_iter()
            .find(|row| row.rung_key == key)
            .unwrap_or_else(|| panic!("{key} is published in the catalog"))
    }

    /// **The shipped climb, branch by branch** — the keys the config declares today, in the order the
    /// catalog publishes them. It is a liveness statement and nothing more: the figures below are
    /// asserted against the *records*, so this list is what fails when the catalog publishes nothing,
    /// publishes the route branch, or loses a branch's rows.
    const SHIPPED_DEPOSIT_CLIMB: [&str; 5] = [
        "forestry:deadfall",
        "forestry:felling",
        "forestry:coppice",
        "extraction:gathering",
        "extraction:quarry",
    ];

    /// **The titles the sim resolves for that climb**, beside the keys rather than derived here: a
    /// client must never author a second spelling of a rung's name, so the wire's answer is pinned as
    /// text. The derivation itself is pinned by the appended rung below, whose title no shipped rung
    /// carries.
    const SHIPPED_DEPOSIT_TITLES: [&str; 5] =
        ["Deadfall", "Felling", "Coppice", "Gathering", "Quarry"];

    /// ⛔ **THE CATALOG IS `intensification_ladder.json`'S OWN TWO DEPOSIT BRANCHES, IN CLIMB ORDER**
    /// — one row per rung the config declares, every value read off that rung's record.
    ///
    /// **This is what lets a client draw a ladder of rungs nothing has opened yet**, and it is
    /// asserted against the *records* rather than against literals for the reason the whole catalog
    /// exists: a rung added to the config, or a figure retuned on one, must reach the wire with no
    /// edit here and none on the client. The liveness half is `SHIPPED_DEPOSIT_CLIMB` — a catalog
    /// that published nothing, or published the route branch, fails the count and the keys before any
    /// figure is read.
    #[test]
    fn the_deposit_rung_catalog_is_the_configs_own_two_climbs() {
        let ladder = LadderConfig::builtin();
        let declared = deposit_rungs_in_climb_order(&ladder);
        assert_eq!(
            declared.len(),
            SHIPPED_DEPOSIT_CLIMB.len(),
            "the shipped ladder declares the five deposit rungs the climb above names"
        );

        let app = a_world_with_a_catalog();
        let published = published_deposit_rungs(&app);
        assert_eq!(
            published.len(),
            declared.len(),
            "one published row per rung the config declares"
        );

        for (index, (row, rung)) in published.iter().zip(declared.iter()).enumerate() {
            assert_eq!(
                row.rung_key, SHIPPED_DEPOSIT_CLIMB[index],
                "row {index} is the rung the climb puts there"
            );
            assert_eq!(row.rung_key, rung.wire_key(), "…and the record's own key");
            assert_eq!(
                row.branch,
                rung.branch.as_str(),
                "{} publishes the branch its record names",
                row.rung_key
            );
            assert_eq!(row.order, rung.order, "the record's own climb order");
            assert_eq!(
                row.verb,
                rung.verb.clone().unwrap_or_default(),
                "{} publishes the verb its record declares",
                row.rung_key
            );
            assert_eq!(
                row.unlock_knowledge,
                rung.unlock_knowledge.clone().unwrap_or_default(),
                "{} publishes the knowledge its record waits on",
                row.rung_key
            );
            assert_eq!(
                row.requires_rung,
                rung.requires_rung_wire_key().unwrap_or_default(),
                "{} publishes the rung directly beneath it",
                row.rung_key
            );
            assert_eq!(
                row.earns_knowledge,
                rung.earns_knowledge.clone().unwrap_or_default(),
                "{} publishes the lesson standing there teaches",
                row.rung_key
            );
            assert_eq!(
                row.work_cost,
                rung.build_cost(RUNG_COST_UNSCALED).unwrap_or(NO_BUILD_WORK),
                "{} publishes its record's own build cost",
                row.rung_key
            );
            assert_eq!(
                row.upkeep_work_per_turn,
                rung.upkeep
                    .as_ref()
                    .map_or(NO_UPKEEP_DEMAND, |upkeep| upkeep.work_per_turn),
                "{} publishes its record's own standing bill",
                row.rung_key
            );
            let pile = rung.build_materials().next();
            assert_eq!(
                row.build_material_cost,
                pile.map_or(NO_BUILD_MATERIAL, |(_, amount)| amount),
                "{} publishes its record's own pile",
                row.rung_key
            );
            assert_eq!(
                row.build_material_id,
                pile.map_or_else(String::new, |(id, _)| id.to_string()),
                "{} publishes the material that pile is counted in",
                row.rung_key
            );
            let payoff = rung
                .extraction_payoff
                .as_ref()
                .expect("validate requires an extraction_payoff on every deposit rung");
            assert_eq!(
                row.yield_per_worker_turn, payoff.yield_per_worker_turn,
                "{} publishes what one worker takes at it",
                row.rung_key
            );
            assert_eq!(
                row.recovery_fraction, payoff.recovery_fraction,
                "{} publishes how far into the body it reaches",
                row.rung_key
            );
            assert_eq!(
                row.regrowth_multiplier, payoff.regrowth_multiplier,
                "{} publishes what it multiplies the ground's renewal by",
                row.rung_key
            );
            assert_eq!(
                row.min_deposit_capacity,
                rung.site_requirement
                    .as_ref()
                    .map_or(NO_DEPOSIT_FLOOR, |site| site.min_deposit_capacity),
                "{} publishes what the ground must hold for it",
                row.rung_key
            );
            assert_eq!(
                row.display_name, SHIPPED_DEPOSIT_TITLES[index],
                "{} publishes the title the sim resolves, so no client spells it a second way",
                row.rung_key
            );
            assert_eq!(
                row.build_work_per_worker_turn,
                build_work_per_worker_turn(NO_BUILD_GEAR),
                "{} publishes the sim's own bare work rate, so no client transcribes the constant",
                row.rung_key
            );
        }
    }

    /// **A RUNG NOBODY BUILDS COSTS NOTHING TO REACH** — the `workCost` a rung with no `build` block
    /// publishes, which on the shipped ladder is the two free floors and nothing else.
    const NO_BUILD_WORK: f32 = 0.0;

    /// **A RUNG THAT EATS NOTHING SWALLOWS NO PILE** — the `buildMaterialCost` a rung declaring no
    /// `build.materials` publishes, and it rides with an empty `buildMaterialId`: the pair is one
    /// reading, so *no amount* and *no noun* are the same answer said twice.
    const NO_BUILD_MATERIAL: f32 = 0.0;

    /// ⛔ **THE QUARRY IS THE ROW THE WHOLE STONE BRANCH TURNS ON**, so its four prices, its gate, its
    /// chain, its reach and its placement rule are pinned as **literals** here rather than against the
    /// record — the one place in this file where a retune should have to be typed twice, because
    /// every one of these figures is a decision `docs/plan_extraction.md` argues for.
    #[test]
    fn the_quarry_publishes_its_price_its_pile_its_reach_and_its_placement_rule() {
        let app = a_world_with_a_catalog();
        let quarry = published_rung(&app, "extraction:quarry");

        assert_eq!(quarry.branch, "extraction", "{quarry:?}");
        assert_eq!(quarry.verb, "quarry", "{quarry:?}");
        assert_eq!(quarry.work_cost, 250.0, "{quarry:?}");
        assert_eq!(quarry.build_material_id, "wood", "{quarry:?}");
        assert_eq!(quarry.build_material_cost, 8.0, "{quarry:?}");
        assert_eq!(quarry.upkeep_work_per_turn, 1.5, "{quarry:?}");
        assert_eq!(quarry.recovery_fraction, 0.85, "{quarry:?}");
        assert_eq!(quarry.unlock_knowledge, "quarrying", "{quarry:?}");
        assert_eq!(quarry.requires_rung, "extraction:gathering", "{quarry:?}");
        assert_eq!(
            quarry.min_deposit_capacity, 100.0,
            "the one placement rule on either branch, and the whole of *you cannot quarry just \
             anywhere*: {quarry:?}"
        );
    }

    /// ⛔ **A FREE FLOOR IS PRICED AT NOTHING AND STILL CARRIES A REAL PAYOFF.** Nobody builds a
    /// stone scatter and nobody holds one, so the two prices are zero and there is no verb to name a
    /// job — but the rung still reaches 15% of the body and pays a bare-handed rate, which is what
    /// makes it a *rung* rather than the absence of one.
    #[test]
    fn the_free_floor_is_priced_at_nothing_and_still_reaches_the_surface() {
        let app = a_world_with_a_catalog();
        let gathering = published_rung(&app, "extraction:gathering");

        assert_eq!(gathering.verb, "", "{gathering:?}");
        assert_eq!(gathering.work_cost, 0.0, "{gathering:?}");
        assert_eq!(gathering.upkeep_work_per_turn, 0.0, "{gathering:?}");
        assert_eq!(gathering.build_material_id, "", "{gathering:?}");
        assert_eq!(gathering.build_material_cost, 0.0, "{gathering:?}");
        assert_eq!(gathering.requires_rung, "", "{gathering:?}");
        assert_eq!(
            gathering.recovery_fraction, 0.15,
            "a surface rung reaches the scatter and no further: {gathering:?}"
        );
        assert!(
            gathering.yield_per_worker_turn > 0.0,
            "the floor's rate is bare-handed, never a zero — the whole material economy bootstraps \
             through it: {gathering:?}"
        );
    }

    /// **CONSERVATIONISM EXPRESSED MECHANICALLY** — the coppice is the only rung on either branch
    /// that moves the ground's own renewal, and the catalog is where a client reads *what this rung
    /// buys* before anybody has laid one out.
    #[test]
    fn a_coppice_publishes_the_regrowth_it_buys() {
        let app = a_world_with_a_catalog();
        let coppice = published_rung(&app, "forestry:coppice");

        assert_eq!(
            coppice.regrowth_multiplier, 2.0,
            "a managed wood renews twice as fast: {coppice:?}"
        );
        assert_eq!(
            coppice.recovery_fraction, 1.0,
            "and it reaches the whole wood, like every forestry rung: {coppice:?}"
        );
        assert_eq!(coppice.unlock_knowledge, "conservationism", "{coppice:?}");
    }

    /// The rung the override below appends — a fourth forestry step above the coppice, declaring
    /// figures no shipped rung carries so its row cannot be confused with one.
    const AN_APPENDED_RUNG: &str = r#"{
        "id": "old_growth",
        "branch": "forestry",
        "order": 4,
        "verb": null,
        "unlock_knowledge": "conservationism",
        "earns_knowledge": null,
        "requires_rung": "coppice",
        "ceiling_required": null,
        "site_requirement": null,
        "build": { "work_cost": 400.0, "grace_turns": null },
        "upkeep": {
            "work_per_turn": 3.0,
            "scaled_by": "source_load",
            "meter_decay": { "per_turn": 4.0 },
            "grace_turns": 2
        },
        "extraction_payoff": {
            "yield_per_worker_turn": 3.5,
            "recovery_fraction": 1.0,
            "regrowth_multiplier": 3.0
        },
        "behavior": { "movement": "fixed" }
    }"#;

    /// ⛔ **A RUNG ADDED TO THE CONFIG APPEARS ON THE WIRE WITH NO CODE CHANGE — THE WHOLE REASON
    /// THIS CATALOG EXISTS.** The plant and animal branches are drawn from hardcoded client-side rung
    /// arrays, which is a second authority that goes stale the day a rung is added; this asserts the
    /// route branch's precedent holds here, and it is not hypothetical — the minerals arc's `mine` is
    /// already reserved above `extraction:quarry` on the extraction branch.
    ///
    /// The rung is appended to the **shipped config's own JSON** and loaded through
    /// `LadderConfig::from_json_str`, so what is under test is the catalog's derivation and not a
    /// hand-built `LadderConfig` that could disagree with what a file would produce.
    #[test]
    fn a_rung_added_to_the_config_is_published_with_no_code_change() {
        let mut ladder: serde_json::Value =
            serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER).expect("the builtin parses");
        ladder["rungs"]
            .as_array_mut()
            .expect("the ladder declares its rungs as an array")
            .push(serde_json::from_str(AN_APPENDED_RUNG).expect("the appended rung parses"));
        let overridden = LadderConfig::from_json_str(&ladder.to_string())
            .expect("a ladder with one more forestry rung is a valid ladder");

        let mut app = build_test_app();
        app.update();
        app.world
            .resource_mut::<LadderConfigHandle>()
            .replace(Arc::new(overridden));
        app.update();

        let published = published_deposit_rungs(&app);
        assert_eq!(
            published.len(),
            SHIPPED_DEPOSIT_CLIMB.len() + 1,
            "the appended rung is a row of the catalog: {published:?}"
        );
        let appended = published_rung(&app, "forestry:old_growth");
        assert_eq!(appended.branch, "forestry", "{appended:?}");
        assert_eq!(appended.order, 4, "{appended:?}");
        assert_eq!(appended.display_name, "Old Growth", "{appended:?}");
        assert_eq!(appended.requires_rung, "forestry:coppice", "{appended:?}");
        assert_eq!(appended.work_cost, 400.0, "{appended:?}");
        assert_eq!(appended.upkeep_work_per_turn, 3.0, "{appended:?}");
        assert_eq!(appended.regrowth_multiplier, 3.0, "{appended:?}");
        assert_eq!(appended.yield_per_worker_turn, 3.5, "{appended:?}");
        assert_eq!(
            published
                .iter()
                .filter(|row| row.branch == "forestry")
                .count(),
            4,
            "and it climbs on its own branch, leaving the extraction ladder alone: {published:?}"
        );
    }
}
