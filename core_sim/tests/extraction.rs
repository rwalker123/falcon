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
#[test]
fn a_worked_quarry_only_ever_goes_down_and_renews_nothing() {
    let config = ExtractionConfig::builtin();
    let ladder = LadderConfig::builtin();
    let (mut working, ground) = working_at(RungKey::ExtractionQuarry, ROCK, STONE);
    let mut previous = working.stock;
    let mut ever_took = false;
    for _ in 0..400 {
        let outcome = take_from_deposit(&mut working, 5, &ground, &config, &ladder);
        ever_took |= outcome.taken > 0.0;
        assert_eq!(
            outcome.stock, outcome.stock_after_take,
            "renewal must contribute EXACTLY nothing to a rock body — `0 x anything` is still 0"
        );
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
        take_from_deposit(&mut working, 12, &ground, &config, &ladder);
    }
    let cut_to = working.stock;
    assert!(
        cut_to < capacity * 0.5,
        "fixture: the wood must really have been cut down, got {cut_to} of {capacity}"
    );

    let mut previous = cut_to;
    for _ in 0..600 {
        // **Nobody is working it** — the same seam, at a crew of none.
        take_from_deposit(&mut working, 0, &ground, &config, &ladder);
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
        take_from_deposit(&mut working, 10, &ground, &config, &ladder);
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
            total += take_from_deposit(&mut working, 4, &ground, &config, &ladder).taken;
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
