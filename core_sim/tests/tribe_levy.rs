//! **A tribe levies its bands** (#862, `.claude/rules/core_sim/leadership.md` -> "A tribe levies its
//! bands"): the tribe's works become levy rows on member lists, the overflow chain, adoption, the
//! one scouting range, the four commands' refusals, the acting reading and the save.
//!
//! The staffing's levy step and the stretch geometry are unit-pinned beside the code
//! (`staffing::tests`, `tribe_levy::tests`); the claims here are about the world and the wire.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::tribe_levy::{
    advance_tribe_levy, apply_tribe_pass, tribe_levy, tribe_scout, tribe_work, tribe_work_stop,
};
use core_sim::{
    advance_tribes, points_per_turn, publish_baseline_snapshot, reconcile_band_culture_layers,
    BandId, BandLeader, BandLeadership, BandName, BranchId, BuildCrew, BuildJob, BuildSource,
    ConnectionKey, ConnectionLedger, CultureManager, CultureOwner, FactionId, FoundingLines,
    LaborAllocation, LaborTarget, LeaderKind, LeadershipConfigHandle, LevyCeiling,
    PopulationCohort, ResidentBand, RungId, Sighting, SimulationConfig, SimulationTick,
    SnapshotHistory, Tile, TileImprovementRegistry, TribeId, TribeRegistry, CULTURE_TRAIT_AXES,
};

const TEST_BAND_BASE: u64 = 9_000;
const FIXTURE_TURN: u64 = 20;
const PLACE: UVec2 = UVec2::new(10, 10);
const TURNS_TO_FILL_BANK: usize = 40;
/// A tile-kind branch and its first rung, as the shipped ladder names them.
const PIT_BRANCH: &str = "food_stores";
const PIT_RUNG: &str = "storage_pit";
const FACTION: FactionId = FactionId(0);

fn spawn_world() -> App {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    core_sim::run_turn(&mut app);
    app
}

fn id(n: u64) -> BandId {
    BandId(TEST_BAND_BASE + n)
}

/// A chiefed band descended from starting band `origin`, with a labor allocation of its own.
fn member(app: &mut App, n: u64, size: u32, origin: u64) -> Entity {
    let template = {
        let mut query = app
            .world
            .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
        let mut rows: Vec<_> = query.iter(&app.world).collect();
        rows.sort_by_key(|(id, _)| **id);
        rows.first().expect("the world has a band").1.clone()
    };
    let mut cohort = template;
    cohort.size = size;
    cohort.belief_anchor = Some(PLACE);
    cohort.founding_lines = FoundingLines::founded(id(origin), 2);
    let entity = app
        .world
        .spawn((
            cohort,
            ResidentBand,
            id(n),
            BandName(format!("Test{n}")),
            BandLeadership {
                leader: Some(BandLeader {
                    name: format!("Chief{n}"),
                    kind: LeaderKind::Chief,
                    since_turn: 1,
                }),
                ..BandLeadership::default()
            },
            LaborAllocation::default(),
        ))
        .id();
    app.world.run_system_once(reconcile_band_culture_layers);
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(id(n)))
        .expect("the band has a culture layer");
    for axis in 0..CULTURE_TRAIT_AXES {
        layer
            .traits
            .update_value(axis, core_sim::scalar_from_f32(0.0));
    }
    entity
}

fn tie_all(app: &mut App, ns: &[u64]) {
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for a in ns {
        for b in ns {
            if a != b {
                ledger.insert_full_tie(
                    ConnectionKey::new(id(*a), id(*b)),
                    &Sighting::new(UVec2::ZERO, 1, "", FactionId(0)),
                    1,
                );
            }
        }
    }
}

fn stand_at(app: &mut App, entity: Entity, at: UVec2) {
    let tile = {
        let mut tiles = app.world.query::<(Entity, &Tile)>();
        tiles
            .iter(&app.world)
            .find(|(_, tile)| tile.position == at)
            .map(|(entity, _)| entity)
            .expect("the map has the tile")
    };
    app.world
        .get_mut::<PopulationCohort>(entity)
        .unwrap()
        .current_tile = tile;
}

fn turn(app: &mut App) {
    let next = app.world.resource::<SimulationTick>().0.max(FIXTURE_TURN) + 1;
    app.world.resource_mut::<SimulationTick>().0 = next;
    app.world.run_system_once(advance_tribes);
    app.world.run_system_once(advance_tribe_levy);
}

fn registry(app: &App) -> &TribeRegistry {
    app.world.resource::<TribeRegistry>()
}

fn until_formed(app: &mut App) -> TribeId {
    for _ in 0..TURNS_TO_FILL_BANK {
        turn(app);
        if let Some(tribe) = registry(app).iter().next() {
            return tribe.id;
        }
    }
    panic!("no tribe formed");
}

/// Two kin in one tribe (band 1 at `(10, 10)`, band 2 four tiles east) and a third band in no tribe.
fn tribe_of_two() -> (App, TribeId, Entity, Entity, Entity) {
    let mut app = spawn_world();
    let one = member(&mut app, 1, 40, 1);
    let two = member(&mut app, 2, 90, 1);
    let three = member(&mut app, 3, 30, 3);
    stand_at(&mut app, one, PLACE);
    stand_at(&mut app, two, PLACE + UVec2::new(4, 0));
    stand_at(&mut app, three, PLACE + UVec2::new(0, 20));
    tie_all(&mut app, &[1, 2]);
    let tribe = until_formed(&mut app);
    (app, tribe, one, two, three)
}

/// The work site: next to band 2, four tiles from band 1.
const SITE: UVec2 = UVec2::new(14, 11);

fn levy_rows(app: &App, entity: Entity) -> Vec<(LaborTarget, Option<TribeId>, Option<u32>)> {
    app.world
        .get::<LaborAllocation>(entity)
        .unwrap()
        .assignments
        .iter()
        .filter(|row| row.for_tribe.is_some())
        .map(|row| (row.target.clone(), row.for_tribe, row.max_workers))
        .collect()
}

fn tile_row_on_site(app: &App, entity: Entity) -> bool {
    app.world
        .get::<LaborAllocation>(entity)
        .unwrap()
        .assignments
        .iter()
        .any(|row| matches!(&row.target, LaborTarget::Tile { tile, .. } if *tile == SITE))
}

fn publish(app: &mut App) -> Vec<u8> {
    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref()).to_vec()
}

fn with_row<T>(app: &mut App, n: u64, read: impl FnOnce(fb::PopulationCohortState<'_>) -> T) -> T {
    let bytes = publish(app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.bandId() == id(n).0)
        .expect("the band is on the wire");
    read(row)
}

#[test]
fn a_work_is_a_for_tribe_row_on_the_nearest_member_only_and_a_non_member_is_unchanged() {
    let (mut app, tribe, one, two, three) = tribe_of_two();
    tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None)
        .expect("a member band may add a work");

    let rows = levy_rows(&app, two);
    assert_eq!(rows.len(), 1, "the nearest member carries the work");
    assert_eq!(rows[0].1, Some(tribe));
    assert!(
        matches!(&rows[0].0, LaborTarget::Tile { tile, .. } if *tile == SITE),
        "the row is the tile-kind row on the site"
    );
    assert_eq!(rows[0].2, None, "the nearest member is not capped");
    assert!(
        levy_rows(&app, one).is_empty(),
        "the far member carries none"
    );
    assert!(
        app.world
            .get::<LaborAllocation>(three)
            .unwrap()
            .assignments
            .is_empty(),
        "a band in no tribe is exactly as it was"
    );

    // The wire: the row says it is the tribe's, the cohort publishes the works and who carries them.
    let carrier = with_row(&mut app, 2, |row| {
        let works = row.tribeWorks().expect("the works ride the member row");
        assert_eq!(works.len(), 1);
        let work = works.get(0);
        assert_eq!((work.x(), work.y()), (SITE.x, SITE.y));
        assert_eq!(work.branch(), Some(PIT_BRANCH));
        assert_eq!(
            work.bands().map(|b| b.iter().collect::<Vec<_>>()),
            Some(vec![id(2).0 as u32])
        );
        assert_eq!(row.tribeLevyCeiling(), core_sim_ceiling_normal());
        row.laborAssignments()
            .expect("labor rows")
            .iter()
            .any(|assignment| assignment.forTribe())
    });
    assert!(carrier, "the carrying member's row is published forTribe");
    with_row(&mut app, 3, |row| {
        assert_eq!(row.tribeLevyCeiling(), 0, "not in a tribe");
        assert!(row.tribeWorks().is_none_or(|works| works.is_empty()));
    });
}

fn core_sim_ceiling_normal() -> u8 {
    LevyCeiling::Normal.wire_code()
}

#[test]
fn the_overflow_goes_to_the_next_nearest_member_one_turn_behind_and_goes_when_no_longer_short() {
    let (mut app, tribe, one, two, _three) = tribe_of_two();
    tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None).unwrap();
    // The nearest member's row finished the last staffing three heads short of its want.
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(two).unwrap();
        let row = allocation
            .assignments
            .iter_mut()
            .find(|row| row.for_tribe.is_some())
            .unwrap();
        row.workers = 1;
        row.staffing.reach_crew = 4;
    }
    apply_tribe_pass(&mut app.world, tribe);
    let overflow = levy_rows(&app, one);
    assert_eq!(
        overflow.len(),
        1,
        "the next nearest member carries the same work"
    );
    assert_eq!(overflow[0].2, Some(3), "with the shortfall as its Max");
    assert_eq!(levy_rows(&app, two).len(), 1, "the nearest keeps its row");

    // No longer short: the overflow row goes.
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(two).unwrap();
        let row = allocation
            .assignments
            .iter_mut()
            .find(|row| row.for_tribe.is_some())
            .unwrap();
        row.workers = 4;
    }
    apply_tribe_pass(&mut app.world, tribe);
    assert!(
        levy_rows(&app, one).is_empty(),
        "the overflow row is dropped"
    );
    assert_eq!(levy_rows(&app, two).len(), 1);
}

/// A restaff after the pass changes a levy row's heads; the Snapshot stage's refresh then states
/// them on the work, so `tribeWorks[].workers` equals the levy rows' workers in the same frame.
#[test]
fn the_published_work_workers_follow_the_levy_rows_after_the_last_restaff() {
    const RESTAFFED: u32 = 3;
    let (mut app, _tribe, _one, two, _three) = tribe_of_two();
    tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None).unwrap();
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(two).unwrap();
        let row = allocation
            .assignments
            .iter_mut()
            .find(|row| row.for_tribe.is_some())
            .unwrap();
        row.workers = RESTAFFED;
    }
    let published = |app: &mut App| {
        with_row(app, 2, |row| {
            row.tribeWorks()
                .expect("the works ride the row")
                .get(0)
                .workers()
        })
    };
    assert_ne!(
        published(&mut app),
        RESTAFFED,
        "the work still reads the earlier staffing"
    );
    app.world
        .run_system_once(core_sim::tribe_levy::refresh_all_levy_workers);
    assert_eq!(published(&mut app), RESTAFFED);
}

/// A work with no rung is taken where the band's own `assign_labor ... tile` would be: refused on
/// another people's improvement with the same sentence, and an empty site is placed at the root.
#[test]
fn a_work_with_no_rung_is_gated_and_placed_as_assign_labor_would() {
    let (mut app, tribe, _one, _two, _three) = tribe_of_two();
    let branch = BranchId::new(PIT_BRANCH);
    let rival = FactionId(FACTION.0 + 1);
    {
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        app.world
            .resource_mut::<TileImprovementRegistry>()
            .place_owned(SITE, branch, rival, None, &ladder);
    }
    let refusal = tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None)
        .expect_err("another people's pit is refused");
    assert_eq!(
        refusal.message,
        format!(
            "The improvement at ({}, {}) belongs to another people.",
            SITE.x, SITE.y
        )
    );
    assert!(registry(&app).get(tribe).unwrap().works.is_empty());

    // An empty site gets the branch's root, as the band's own crew would place it.
    let empty = SITE + UVec2::new(1, 0);
    assert!(app
        .world
        .resource::<TileImprovementRegistry>()
        .get(empty, branch)
        .is_none());
    tribe_work(
        &mut app.world,
        FACTION,
        id(1),
        PIT_BRANCH,
        empty,
        None,
        None,
    )
    .unwrap();
    let placed = app
        .world
        .resource::<TileImprovementRegistry>()
        .get(empty, branch)
        .expect("the work placed the root");
    assert_eq!(placed.faction, FACTION);
}

/// A levy build the budget ran out before dealing it a single builder is short of its cap, so the
/// work overflows to the next member; the build is open to builders, so the zero is not a gate.
#[test]
fn a_levy_build_dealt_no_builder_because_the_budget_ran_out_overflows() {
    const CAP: u32 = 3;
    let (mut app, tribe, one, two, _three) = tribe_of_two();
    let target = format!("{PIT_BRANCH}:{PIT_RUNG}");
    learn_for(&mut app, &target);
    tribe_work(
        &mut app.world,
        FACTION,
        id(1),
        PIT_BRANCH,
        SITE,
        Some(&target),
        None,
    )
    .unwrap();
    let source = BuildSource::Tile {
        tile: SITE,
        branch: BranchId::new(PIT_BRANCH),
    };
    assert!(
        app.world
            .get::<LaborAllocation>(two)
            .unwrap()
            .build_declaration(&source)
            .is_some(),
        "liveness: the nearest member declared the build"
    );
    // The last staffing dealt it nobody: the levy budget was spent on the member's scouts.
    app.world
        .get_mut::<LaborAllocation>(two)
        .unwrap()
        .apply_build_crews(vec![(
            source,
            BuildCrew {
                kitted: 0,
                bare: 0,
                cap: CAP,
                fed_first: false,
            },
        )]);
    apply_tribe_pass(&mut app.world, tribe);
    let overflow = levy_rows(&app, one);
    assert_eq!(overflow.len(), 1, "the next member carries the overflow");
    assert_eq!(overflow[0].2, Some(CAP), "for the builders it was short");
}

#[test]
fn a_members_existing_tile_row_is_adopted_and_its_declared_build_and_meter_carry_over() {
    let (mut app, tribe, one, two, _three) = tribe_of_two();
    let branch = BranchId::new(PIT_BRANCH);
    let rung = RungId::new(branch, PIT_RUNG);
    let target = LaborTarget::Tile { tile: SITE, branch };
    // Band 1 (the far one) was already digging the pit by itself.
    {
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        app.world
            .resource_mut::<TileImprovementRegistry>()
            .place_owned(SITE, branch, FACTION, Some(id(1)), &ladder);
        let mut allocation = app.world.get_mut::<LaborAllocation>(one).unwrap();
        allocation.set_staffed_row(target.clone(), None, None);
        assert!(allocation.declare_build(
            BuildSource::Tile { tile: SITE, branch },
            BuildJob::Climb(rung)
        ));
    }
    let meter_before = format!(
        "{:?}",
        app.world
            .resource::<TileImprovementRegistry>()
            .get(SITE, branch)
    );
    tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None).unwrap();

    let work = registry(&app).get(tribe).unwrap().works[0];
    assert_eq!(
        work.target_rung,
        Some(rung),
        "the adopted row's declared build is the work's target"
    );
    let rows = levy_rows(&app, two);
    assert_eq!(rows.len(), 1, "the nearest member now carries it");
    let declared = app
        .world
        .get::<LaborAllocation>(two)
        .unwrap()
        .build_declaration(&BuildSource::Tile { tile: SITE, branch })
        .map(|declaration| declaration.declared);
    assert_eq!(declared, Some(BuildJob::Climb(rung)));
    assert!(
        !tile_row_on_site(&app, one),
        "the far member's own row is the tribe's now, carried by the nearest"
    );
    let meter_after = format!(
        "{:?}",
        app.world
            .resource::<TileImprovementRegistry>()
            .get(SITE, branch)
    );
    assert_eq!(
        meter_before, meter_after,
        "progress stays on the tile's meter"
    );
}

#[test]
fn stopping_a_work_removes_it_and_drops_its_levy_rows() {
    let (mut app, _tribe, _one, two, _three) = tribe_of_two();
    tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None).unwrap();
    assert_eq!(levy_rows(&app, two).len(), 1);
    tribe_work_stop(&mut app.world, FACTION, id(2), PIT_BRANCH, SITE).unwrap();
    assert!(levy_rows(&app, two).is_empty());
    let refused = tribe_work_stop(&mut app.world, FACTION, id(2), PIT_BRANCH, SITE).unwrap_err();
    assert_eq!(refused.token, "no_such_work");
}

#[test]
fn every_command_refuses_by_name() {
    let (mut app, tribe, _one, _two, _three) = tribe_of_two();
    let world = &mut app.world;
    let token = |result: Result<String, core_sim::tribe_levy::TribeRefusal>| {
        result.expect_err("refused").token
    };
    assert_eq!(
        token(tribe_work(
            world,
            FACTION,
            id(3),
            PIT_BRANCH,
            SITE,
            None,
            None
        )),
        "band_not_in_tribe"
    );
    assert_eq!(
        token(tribe_levy(world, FACTION, id(3), "high")),
        "band_not_in_tribe"
    );
    assert_eq!(
        tribe_scout(world, FACTION, id(3), 2).unwrap_err().token,
        "band_not_in_tribe"
    );
    assert_eq!(
        token(tribe_work(world, FACTION, id(1), "plant", SITE, None, None)),
        "not_a_tribe_work",
        "a patch's branch is not a tile improvement"
    );
    assert_eq!(
        token(tribe_work(world, FACTION, id(1), "route", SITE, None, None)),
        "road_needs_rung",
        "a road work names the rung it builds to"
    );
    assert_eq!(
        token(tribe_levy(world, FACTION, id(1), "sideways")),
        "unknown_levy"
    );
    // A build the band's own verb would refuse (the lesson is not known) is refused with its gate.
    let refusal = tribe_work(
        world,
        FACTION,
        id(1),
        PIT_BRANCH,
        SITE,
        Some(&format!("{PIT_BRANCH}:{PIT_RUNG}")),
        None,
    );
    assert_eq!(token(refusal), "build_refused");
    // Another people's band is not yours to order through.
    assert_eq!(
        token(tribe_levy(world, FactionId(7), id(1), "high")),
        "no_such_band"
    );
    // The ceiling and the range stand once the refusals have run.
    assert_eq!(tribe_levy(world, FACTION, id(1), "low").map(|_| ()), Ok(()));
    assert_eq!(registry(&app).get(tribe).unwrap().levy, LevyCeiling::Low);
    let max = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .scout
        .range_past_local_max;
    let (_, set) = tribe_scout(&mut app.world, FACTION, id(1), max + 40).unwrap();
    assert_eq!(set, max, "clamped exactly as assign_labor scout clamps");
    assert_eq!(registry(&app).get(tribe).unwrap().scouting_range, max);
}

#[test]
fn the_scouting_range_follows_the_tribe_formation_join_and_dissolve() {
    // Formation: the largest range any member scouts.
    let mut app = spawn_world();
    let one = member(&mut app, 1, 40, 1);
    let two = member(&mut app, 2, 90, 1);
    let three = member(&mut app, 3, 30, 1);
    for (entity, range) in [(one, 2), (two, 5), (three, 1)] {
        app.world
            .get_mut::<LaborAllocation>(entity)
            .unwrap()
            .set_scouting_range(range, None);
    }
    stand_at(&mut app, one, PLACE);
    stand_at(&mut app, two, PLACE + UVec2::new(4, 0));
    stand_at(&mut app, three, PLACE + UVec2::new(0, 30));
    tie_all(&mut app, &[1, 2]);
    let tribe = until_formed(&mut app);
    assert_eq!(registry(&app).get(tribe).unwrap().scouting_range, 5);
    for entity in [one, two] {
        let allocation = app.world.get::<LaborAllocation>(entity).unwrap();
        assert_eq!(
            allocation.scouting_range_tiles(),
            5,
            "every member takes it"
        );
        let row = allocation
            .assignments
            .iter()
            .find(|row| matches!(row.target, LaborTarget::Scout))
            .unwrap();
        assert_eq!(row.for_tribe, Some(tribe));
        assert!(row.scouting_stretch > 0.0 && row.scouting_stretch <= 1.0);
    }
    assert_eq!(
        app.world
            .get::<LaborAllocation>(three)
            .unwrap()
            .scouting_range_tiles(),
        1,
        "a band outside the tribe keeps its own range"
    );

    // Join: a band that joins takes the tribe's range.
    app.world
        .resource_mut::<TribeRegistry>()
        .get_mut(tribe)
        .unwrap()
        .members
        .insert(id(3));
    app.world.run_system_once(advance_tribe_levy);
    assert_eq!(
        app.world
            .get::<LaborAllocation>(three)
            .unwrap()
            .scouting_range_tiles(),
        5
    );

    // Dissolve: the ties lapse and the tribe breaks up; each band keeps the range as its own, its
    // stretch back to a whole ring.
    {
        let cfg = app
            .world
            .resource::<core_sim::ConnectionsConfigHandle>()
            .get();
        let quiet = (1.0 / cfg.strength.decay_per_turn).ceil() as u64 + 1;
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for _ in 0..quiet {
            ledger.decay_all(FIXTURE_TURN, &cfg);
        }
    }
    turn(&mut app);
    assert!(registry(&app).is_empty(), "the tribe broke up");
    for entity in [one, two, three] {
        let allocation = app.world.get::<LaborAllocation>(entity).unwrap();
        assert_eq!(allocation.scouting_range_tiles(), 5, "the range stays");
        let row = allocation
            .assignments
            .iter()
            .find(|row| matches!(row.target, LaborTarget::Scout))
            .unwrap();
        assert_eq!(row.for_tribe, None);
        assert_eq!(row.scouting_stretch, 1.0);
    }
}

#[test]
fn acting_as_a_tribe_reads_the_levy_and_moves_the_pairs_banked_rate() {
    let (mut app, tribe, one, two, _three) = tribe_of_two();
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    let rate_now = |app: &App| {
        let (_, inputs) = registry(app).pairs_of(id(1)).next().expect("scored");
        (inputs.acting, points_per_turn(&config, inputs))
    };
    app.world.run_system_once(advance_tribes);
    let (idle_reading, idle_rate) = rate_now(&app);
    assert_eq!(idle_reading, 0.0, "nothing levied reads nothing");

    // Levy heads on band 2's row and read it back.
    tribe_work(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE, None, None).unwrap();
    const LEVIED: u32 = 3;
    {
        let mut allocation = app.world.get_mut::<LaborAllocation>(two).unwrap();
        let row = allocation
            .assignments
            .iter_mut()
            .find(|row| row.for_tribe.is_some())
            .unwrap();
        row.workers = LEVIED;
    }
    let pool: u32 = [one, two]
        .iter()
        .map(|entity| {
            let allocation = app.world.get::<LaborAllocation>(*entity).unwrap();
            let cohort = app.world.get::<PopulationCohort>(*entity).unwrap();
            core_sim::staffing::staffable_pool(
                allocation,
                core_sim::available_workers(cohort.working),
            )
        })
        .sum();
    app.world.run_system_once(advance_tribes);
    let (reading, rate) = rate_now(&app);
    let expected =
        (f64::from(LEVIED) / (config.tribe_acting_full_share * f64::from(pool))).clamp(0.0, 1.0);
    assert!(expected > 0.0, "the fixture levies something");
    assert!((reading - expected).abs() < 1e-9, "{reading} vs {expected}");
    assert!(
        (rate - idle_rate - config.tribe_weight_acting * expected).abs() < 1e-9,
        "the reading feeds the pair's rate at the acting weight"
    );
    let _ = tribe;
}

/// A member's scouts are levy once: the Scout row is `for_tribe` AND counted as scouts, and the
/// acting reading must count each scout head a single time.
#[test]
fn acting_as_a_tribe_counts_each_scout_once() {
    const SCOUTING_RANGE: u32 = 1;
    let (mut app, _tribe, one, two, _three) = tribe_of_two();
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    tribe_scout(&mut app.world, FACTION, id(1), SCOUTING_RANGE).unwrap();
    {
        let allocation = app.world.get::<LaborAllocation>(two).unwrap();
        let row = allocation
            .assignments
            .iter()
            .find(|row| matches!(row.target, LaborTarget::Scout))
            .expect("the tribe's range puts a Scout row on the member");
        assert!(row.for_tribe.is_some(), "the fixture row is the tribe's");
    }
    // Every scout head the two members field, each counted once.
    let scouts: u32 = [one, two]
        .iter()
        .map(|entity| {
            let allocation = app.world.get::<LaborAllocation>(*entity).unwrap();
            allocation
                .assignments
                .iter()
                .filter(|row| matches!(row.target, LaborTarget::Scout))
                .map(|row| row.workers)
                .sum::<u32>()
        })
        .sum();
    let pool: u32 = [one, two]
        .iter()
        .map(|entity| {
            let allocation = app.world.get::<LaborAllocation>(*entity).unwrap();
            let cohort = app.world.get::<PopulationCohort>(*entity).unwrap();
            core_sim::staffing::staffable_pool(
                allocation,
                core_sim::available_workers(cohort.working),
            )
        })
        .sum();
    app.world.run_system_once(advance_tribes);
    let (_, inputs) = registry(&app).pairs_of(id(1)).next().expect("scored");
    let once = f64::from(scouts) / (config.tribe_acting_full_share * f64::from(pool));
    assert!(scouts > 0, "liveness: the members field scouts");
    assert!(once * 2.0 <= 1.0, "the fixture can tell once from twice");
    assert!(
        (inputs.acting - once).abs() < 1e-9,
        "{} vs {once}",
        inputs.acting
    );
}

#[test]
fn the_tribes_new_fields_and_the_rows_survive_a_checkpoint() {
    let (mut app, tribe, _one, two, _three) = tribe_of_two();
    tribe_work(
        &mut app.world,
        FACTION,
        id(1),
        PIT_BRANCH,
        SITE,
        None,
        Some(core_sim::SourcePriority::High),
    )
    .unwrap();
    tribe_levy(&mut app.world, FACTION, id(1), "low").unwrap();
    tribe_scout(&mut app.world, FACTION, id(1), 3).unwrap();
    let before = registry(&app).clone();
    let rows_before = levy_rows(&app, two);
    let checkpoint = capture_sim_state(&app.world);
    // Disturb, then restore.
    tribe_levy(&mut app.world, FACTION, id(1), "high").unwrap();
    tribe_work_stop(&mut app.world, FACTION, id(1), PIT_BRANCH, SITE).unwrap();
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(
        registry(&app),
        &before,
        "works, ceiling, range and carriers are saved"
    );
    let restored_two = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &BandId), With<ResidentBand>>();
        query
            .iter(&app.world)
            .find(|(_, band)| **band == id(2))
            .map(|(entity, _)| entity)
            .unwrap()
    };
    assert_eq!(levy_rows(&app, restored_two), rows_before);
    assert_eq!(registry(&app).get(tribe).unwrap().levy, LevyCeiling::Low);
}

// ---- Road works: one road tile, one carrier, built by the levy, kept by the keeper ----

/// The road tile: one tile from band 2, three from band 1.
const ROAD: UVec2 = UVec2::new(13, 10);
const ROUTE: &str = "route";
const GRADE_TARGET: &str = "route:dirt_road";

/// Credit the people with the lesson that gates `rung`, past the completion threshold.
fn learn_for(app: &mut App, rung: &str) {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let id = ladder
        .rung(rung.parse::<RungId>().unwrap())
        .unlock_discovery_id()
        .expect("the rung is gated by a lesson");
    let threshold = ladder.knowledge.completion_threshold;
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(FACTION, id, core_sim::scalar_from_f32(threshold));
}

/// A trail on `ROAD`, kept by `keeper` when one is named.
fn seat_trail(app: &mut App, keeper: Option<u64>) {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let mut roads = app.world.resource_mut::<core_sim::RoadRegistry>();
    let road = roads.road_or_trail(ROAD, &ladder);
    if let Some(n) = keeper {
        road.take_keeper(
            core_sim::RoadKeeper {
                faction: FACTION,
                band: id(n),
            },
            1.0,
            &ladder,
        );
    }
    // Worn in to the top of the trail rung: the rung beneath a `grade`.
    let (base, width) = core_sim::road_rung_span(
        core_sim::shipped::route_trail(),
        &ladder,
        core_sim::NEAR_ENOUGH_TO_KEEP,
    );
    road.set_position(base + width, &ladder);
    assert_eq!(road.held_rung(), core_sim::shipped::route_trail());
}

fn road_build(app: &App, entity: Entity) -> Option<BuildJob> {
    app.world
        .get::<LaborAllocation>(entity)
        .unwrap()
        .build_declaration(&BuildSource::Road(ROAD))
        .map(|declaration| declaration.declared)
}

fn road_keeper(app: &App) -> Option<BandId> {
    app.world
        .resource::<core_sim::RoadRegistry>()
        .road(ROAD)
        .and_then(|road| road.keeper)
        .map(|keeper| keeper.band)
}

fn builders(app: &App, entity: Entity) -> u32 {
    app.world
        .get::<LaborAllocation>(entity)
        .unwrap()
        .build_crew(&BuildSource::Road(ROAD))
        .map_or(0, |crew| crew.total())
}

fn road_work(
    app: &mut App,
    rung: Option<&str>,
) -> Result<String, core_sim::tribe_levy::TribeRefusal> {
    tribe_work(&mut app.world, FACTION, id(1), ROUTE, ROAD, rung, None)
}

fn dirt_road() -> RungId {
    GRADE_TARGET.parse().unwrap()
}

#[test]
fn the_command_takes_a_route_work_with_a_rung_and_refuses_one_without() {
    let (mut app, tribe, _one, _two, _three) = tribe_of_two();
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, None);
    let refusal = road_work(&mut app, None).unwrap_err();
    assert_eq!(refusal.token, "road_needs_rung");
    assert!(registry(&app).get(tribe).unwrap().works.is_empty());
    road_work(&mut app, Some(GRADE_TARGET)).expect("a road work with a rung is taken");
    let work = registry(&app).get(tribe).unwrap().works[0];
    assert_eq!(work.target_rung, Some(dirt_road()));
    assert_eq!(work.branch, BranchId::new(ROUTE));
}

#[test]
fn the_carrier_is_the_nearest_member_with_no_keeper_and_grading_makes_it_the_keeper() {
    let (mut app, _tribe, one, two, three) = tribe_of_two();
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, None);
    road_work(&mut app, Some(GRADE_TARGET)).unwrap();
    assert_eq!(road_build(&app, two), Some(BuildJob::Climb(dirt_road())));
    assert_eq!(road_build(&app, one), None, "no second band gets the tile");
    assert_eq!(road_build(&app, three), None);
    assert_eq!(road_keeper(&app), Some(id(2)), "grading names the keeper");
    for _ in 0..3 {
        turn(&mut app);
    }
    assert_eq!(road_build(&app, two), Some(BuildJob::Climb(dirt_road())));
    assert_eq!(road_build(&app, one), None);
    // The wire names the carrier and the heads on the build.
    let builders_on_build = builders(&app, two);
    with_row(&mut app, 2, |row| {
        let work = row.tribeWorks().expect("the works ride the row").get(0);
        assert_eq!(work.branch(), Some(ROUTE));
        assert_eq!(
            work.bands().map(|b| b.iter().collect::<Vec<_>>()),
            Some(vec![id(2).0 as u32])
        );
        assert_eq!(work.workers(), builders_on_build);
    });
}

#[test]
fn the_keeper_carries_its_tile_even_when_another_member_is_nearer() {
    let (mut app, _tribe, one, two, _three) = tribe_of_two();
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, Some(1));
    road_work(&mut app, Some(GRADE_TARGET)).unwrap();
    assert_eq!(road_build(&app, one), Some(BuildJob::Climb(dirt_road())));
    assert_eq!(road_build(&app, two), None, "one band per road tile");
    assert_eq!(road_keeper(&app), Some(id(1)));
    for _ in 0..3 {
        turn(&mut app);
    }
    assert_eq!(road_build(&app, two), None);
    assert_eq!(road_keeper(&app), Some(id(1)));
}

#[test]
fn the_builders_come_off_the_levy_budget_and_a_none_levy_deals_none() {
    let (mut app, _tribe, _one, two, _three) = tribe_of_two();
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, None);
    road_work(&mut app, Some(GRADE_TARGET)).unwrap();
    let levied = builders(&app, two);
    assert!(levied > 0, "the levy deals builders onto the tribe's road");
    tribe_levy(&mut app.world, FACTION, id(1), "none").unwrap();
    assert_eq!(
        builders(&app, two),
        0,
        "with no levy the tribe's road build gets no builders"
    );
    assert_eq!(road_build(&app, two), Some(BuildJob::Climb(dirt_road())));
}

#[test]
fn a_keeper_that_leaves_the_tribe_releases_the_tile_and_the_nearest_member_picks_it_up() {
    let (mut app, tribe, _one, two, _three) = tribe_of_two();
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, Some(1));
    road_work(&mut app, Some(GRADE_TARGET)).unwrap();
    assert_eq!(road_keeper(&app), Some(id(1)));
    app.world
        .resource_mut::<TribeRegistry>()
        .get_mut(tribe)
        .unwrap()
        .members
        .remove(&id(1));
    apply_tribe_pass(&mut app.world, tribe);
    assert_eq!(
        road_keeper(&app),
        Some(id(2)),
        "released, then adopted by grading it"
    );
    assert_eq!(road_build(&app, two), Some(BuildJob::Climb(dirt_road())));
}

/// Two tribes of one people that both list one road tile: the band that keeps it for one tribe is
/// not released by the other, and the other does not take the tile, so the keeper holds steady.
#[test]
fn two_tribes_listing_one_road_tile_leave_its_keeper_where_it_is() {
    const SETTLE_TURNS: usize = 4;
    let (mut app, tribe_a, _one, _two, _three) = tribe_of_two();
    let four = member(&mut app, 4, 50, 4);
    let five = member(&mut app, 5, 60, 4);
    stand_at(&mut app, four, ROAD + UVec2::new(0, 1));
    stand_at(&mut app, five, ROAD + UVec2::new(1, 1));
    tie_all(&mut app, &[4, 5]);
    for _ in 0..TURNS_TO_FILL_BANK {
        turn(&mut app);
        if registry(&app).iter().count() == 2 {
            break;
        }
    }
    let tribe_b = registry(&app)
        .iter()
        .find(|tribe| tribe.members.contains(&id(4)))
        .expect("the second tribe formed")
        .id;
    assert_ne!(tribe_a, tribe_b);
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, Some(1));
    road_work(&mut app, Some(GRADE_TARGET)).unwrap();
    // The command refuses a tile another tribe's band keeps, and says so.
    let refusal = tribe_work(
        &mut app.world,
        FACTION,
        id(4),
        ROUTE,
        ROAD,
        Some(GRADE_TARGET),
        None,
    )
    .expect_err("another tribe's road is refused");
    assert_eq!(refusal.token, "road_kept_by_another_tribe");
    assert_eq!(
        refusal.message,
        format!("Another tribe keeps the road at ({}, {}).", ROAD.x, ROAD.y)
    );
    // A second tribe's list that already names the same tile (written directly, as the command
    // refuses it) leaves the keeper alone.
    app.world
        .resource_mut::<TribeRegistry>()
        .get_mut(tribe_b)
        .unwrap()
        .works
        .push(core_sim::TribeWork {
            tile: ROAD,
            branch: BranchId::new(ROUTE),
            target_rung: Some(dirt_road()),
            priority: core_sim::SourcePriority::default(),
        });
    assert_eq!(road_keeper(&app), Some(id(1)), "liveness: band 1 keeps it");
    for _ in 0..SETTLE_TURNS {
        turn(&mut app);
        assert_eq!(road_keeper(&app), Some(id(1)), "the keeper holds steady");
    }
    assert_eq!(
        road_build(&app, four),
        None,
        "the other tribe declares nothing"
    );
    assert_eq!(road_build(&app, five), None);
}

#[test]
fn stopping_a_road_work_removes_the_build_and_leaves_the_road_and_its_keeper() {
    let (mut app, tribe, _one, two, _three) = tribe_of_two();
    learn_for(&mut app, GRADE_TARGET);
    seat_trail(&mut app, None);
    road_work(&mut app, Some(GRADE_TARGET)).unwrap();
    assert!(road_build(&app, two).is_some());
    tribe_work_stop(&mut app.world, FACTION, id(1), ROUTE, ROAD).unwrap();
    assert!(registry(&app).get(tribe).unwrap().works.is_empty());
    assert_eq!(road_build(&app, two), None, "the tribe's build is gone");
    assert!(
        app.world
            .resource::<core_sim::RoadRegistry>()
            .road(ROAD)
            .is_some(),
        "the road stands"
    );
    assert_eq!(road_keeper(&app), Some(id(2)), "its keeper stays");
    turn(&mut app);
    assert_eq!(road_build(&app, two), None, "and it is not re-declared");
}

#[test]
fn a_road_work_is_refused_with_the_gates_own_sentence_without_the_knowledge() {
    let (mut app, tribe, _one, _two, _three) = tribe_of_two();
    seat_trail(&mut app, None);
    let refusal = road_work(&mut app, Some(GRADE_TARGET)).unwrap_err();
    assert_eq!(refusal.token, "build_refused");
    assert!(
        refusal.message.starts_with("Your people have not learned"),
        "the gate's own sentence: {}",
        refusal.message
    );
    assert!(registry(&app).get(tribe).unwrap().works.is_empty());
}
