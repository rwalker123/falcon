//! **The tribe leader proposes tribe works** (#863, `.claude/rules/core_sim/leadership.md` -> "The tribe
//! leader proposes tribe works"): what he proposes and when, the two answers, the saved book and the
//! frame's rows. The tribe, its works and the levy are `tests/tribe_levy.rs`'s; the claims here are about
//! the offer.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::spoilage::STORAGE_DISCOVERY_ID;
use core_sim::tribe_levy::{advance_tribe_levy, tribe_levy};
use core_sim::tribe_proposals::{
    approve_tribe_proposal, defer_tribe_proposal, refresh_tribe_proposals, TribeProposalBook,
    NO_SUCH_PROPOSAL,
};
use core_sim::{
    advance_tribes, publish_baseline_snapshot, reconcile_band_culture_layers, scalar_from_f32,
    BandId, BandLeader, BandLeadership, BandName, BeliefRegistry, ConnectionKey, ConnectionLedger,
    CultureManager, CultureOwner, DiscoveryProgressLedger, FactionId, FoundingLines,
    LaborAllocation, LeaderKind, LocalStore, PopulationCohort, ResidentBand, RungId, Sighting,
    SimulationConfig, SimulationTick, SnapshotHistory, Tile, TileImprovementRegistry, TribeId,
    TribeRegistry, CULTURE_TRAIT_AXES, RITUAL_DISCOVERY_ID,
};

const TEST_BAND_BASE: u64 = 9_000;
const FIXTURE_TURN: u64 = 20;
const PLACE: UVec2 = UVec2::new(10, 10);
const EAST: UVec2 = UVec2::new(14, 10);
const TURNS_TO_FILL_BANK: usize = 40;
const FACTION: FactionId = FactionId(0);
/// Belief standing on the gathering place: above the monument's `min_belief`.
const BELIEF_ON_THE_PLACE: f32 = 5.0;
/// The dry lot a rotting band holds, and how long it has kept (the dry class lasts 60 turns unpitted).
const LOT_OF_DRY_FOOD: f32 = 400.0;
const LOT_AGE_TURNS: u32 = 55;
/// A band with next to nobody to levy, and one with hands to spare.
const ONE_PAIR_OF_HANDS: f32 = 2.0;
const PLENTY_OF_HANDS: f32 = 30.0;
const PIT: &str = "food_stores:storage_pit";
const PIT_BRANCH: &str = "food_stores";
const GRADE: &str = "route:dirt_road";

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

/// Two kin in one tribe: band 1 (40 people) at the gathering place, band 2 (90, the leader's) four tiles
/// east. Nothing is proposable yet: no lesson, no belief, no rot, no road.
fn tribe_of_two() -> (App, TribeId, Entity, Entity) {
    let mut app = spawn_world();
    let one = member(&mut app, 1, 40, 1);
    let two = member(&mut app, 2, 90, 1);
    stand_at(&mut app, one, PLACE);
    stand_at(&mut app, two, EAST);
    tie_all(&mut app, &[1, 2]);
    let tribe = until_formed(&mut app);
    refresh_tribe_proposals(&mut app.world);
    (app, tribe, one, two)
}

fn offer(app: &App, tribe: TribeId) -> Option<sim_schema::TribeProposalState> {
    app.world
        .resource::<TribeProposalBook>()
        .current(tribe)
        .cloned()
}

fn learn(app: &mut App, discovery: u32) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FACTION, discovery, scalar_from_f32(1.0));
}

fn learn_for(app: &mut App, rung: &str) {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let discovery = ladder
        .rung(rung.parse::<RungId>().unwrap())
        .unlock_discovery_id()
        .expect("the rung is gated by a lesson");
    let threshold = ladder.knowledge.completion_threshold;
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(FACTION, discovery, scalar_from_f32(threshold));
}

/// The gathering place can take a monument: the lesson and the belief.
fn ready_the_monument(app: &mut App) {
    learn(app, RITUAL_DISCOVERY_ID);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, BELIEF_ON_THE_PLACE);
    refresh_tribe_proposals(&mut app.world);
}

/// A rotting band: a big larder of dry food that has nearly kept its whole shelf life, so most of it rots
/// inside the forecast's window unless a pit holds it longer.
fn make_it_rot(app: &mut App, entity: Entity) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(entity).unwrap();
    cohort.stores = LocalStore::new();
    cohort
        .stores
        .add_food_aged("dry", scalar_from_f32(LOT_OF_DRY_FOOD), LOT_AGE_TURNS);
}

/// A finished pit on `tile`, owned by band `n`.
fn own_a_pit(app: &mut App, tile: UVec2, n: u64) {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let (base, width) = core_sim::tile_rung_span(PIT.parse().unwrap(), &ladder);
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place_owned(tile, PIT_BRANCH.into(), FACTION, Some(id(n)), &ladder)
        .set_position(base + width, &ladder);
}

#[test]
fn a_monument_is_proposed_once_ritual_is_known_and_the_place_holds_belief_and_not_before() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    assert!(offer(&app, tribe).is_none(), "nothing to propose yet");
    // Belief without the lesson: refused by the gate `tribe_work` runs.
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, BELIEF_ON_THE_PLACE);
    refresh_tribe_proposals(&mut app.world);
    assert!(offer(&app, tribe).is_none(), "no ritual, no monument");
    // The lesson without the belief.
    let (mut app, tribe, _one, _two) = tribe_of_two();
    learn(&mut app, RITUAL_DISCOVERY_ID);
    refresh_tribe_proposals(&mut app.world);
    assert!(offer(&app, tribe).is_none(), "no belief, no monument");
    ready_the_monument(&mut app);
    let proposal = offer(&app, tribe).expect("liveness: the monument is on offer");
    assert_eq!(proposal.kind, "monument");
    assert_eq!(proposal.key, "monument@10,10");
    assert_eq!(
        proposal.text,
        "Chief2 proposes: raise a monument at (10, 10)"
    );
    assert_eq!(
        (proposal.tile_x, proposal.tile_y, proposal.for_band),
        (10, 10, 0)
    );
}

#[test]
fn nothing_is_proposed_while_the_levy_ceiling_is_none() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    ready_the_monument(&mut app);
    assert!(offer(&app, tribe).is_some(), "liveness");
    tribe_levy(&mut app.world, FACTION, id(1), "none").unwrap();
    refresh_tribe_proposals(&mut app.world);
    assert!(
        offer(&app, tribe).is_none(),
        "a levy of none proposes nothing"
    );
    tribe_levy(&mut app.world, FACTION, id(1), "normal").unwrap();
    refresh_tribe_proposals(&mut app.world);
    assert!(offer(&app, tribe).is_some(), "and it returns with the levy");
}

#[test]
fn a_pit_is_proposed_for_the_rotting_band_without_one_and_the_carrier_is_that_band() {
    let (mut app, tribe, one, two) = tribe_of_two();
    learn(&mut app, STORAGE_DISCOVERY_ID);
    make_it_rot(&mut app, one);
    make_it_rot(&mut app, two);
    own_a_pit(&mut app, EAST, 2);
    refresh_tribe_proposals(&mut app.world);
    let proposal = offer(&app, tribe).expect("liveness: band 1 is offered a pit");
    assert_eq!(proposal.kind, "storage");
    assert_eq!(proposal.key, format!("storage:{PIT}@10,10"));
    assert_eq!(
        proposal.for_band,
        id(1).0,
        "band 2 owns a pit and is not offered one"
    );
    assert_eq!(
        proposal.text,
        "Chief2 proposes: dig a storage pit for Test1 at (10, 10)"
    );
    approve_tribe_proposal(&mut app.world, FACTION, id(2), &proposal.key).expect("approved");
    let tribe_row = registry(&app).get(tribe).unwrap();
    assert_eq!(
        tribe_row.works.len(),
        1,
        "approving puts the work on the list"
    );
    assert_eq!(tribe_row.works[0].tile, PLACE);
    assert_eq!(
        tribe_row.carriers[0].first().map(|c| c.band),
        Some(id(1)),
        "the band the pit is for carries it"
    );
    let owner = app
        .world
        .resource::<TileImprovementRegistry>()
        .iter()
        .find(|improvement| improvement.tile == PLACE)
        .and_then(|improvement| improvement.owner);
    assert_eq!(owner, Some(id(1)), "and owns it");
    assert!(
        offer(&app, tribe).is_none_or(|next| next.key != proposal.key),
        "an approved proposal clears"
    );
}

#[test]
fn a_build_short_of_builders_holds_the_offer_back_and_a_stalled_one_does_not() {
    // A monument needs stone nobody holds: it is stalled, so it cannot progress and blocks nothing.
    let (mut app, tribe, one, _two) = tribe_of_two();
    ready_the_monument(&mut app);
    learn(&mut app, STORAGE_DISCOVERY_ID);
    make_it_rot(&mut app, one);
    refresh_tribe_proposals(&mut app.world);
    let first = offer(&app, tribe).expect("liveness: the monument comes first");
    assert_eq!(first.kind, "monument");
    approve_tribe_proposal(&mut app.world, FACTION, id(1), &first.key).unwrap();
    let next = offer(&app, tribe).expect("a gated build does not block the next proposal");
    assert_eq!(next.kind, "storage");

    // A pit has no stone to wait on, so it can progress; one builder short of its cap holds the offer.
    let (mut app, tribe, one, _two) = tribe_of_two();
    learn(&mut app, STORAGE_DISCOVERY_ID);
    make_it_rot(&mut app, one);
    refresh_tribe_proposals(&mut app.world);
    let pit = offer(&app, tribe).expect("liveness: a pit is on offer");
    // One pair of hands on the whole band: the levy cannot man the pit's three builder slots.
    app.world.get_mut::<PopulationCohort>(one).unwrap().working =
        scalar_from_f32(ONE_PAIR_OF_HANDS);
    approve_tribe_proposal(&mut app.world, FACTION, id(1), &pit.key).unwrap();
    ready_the_monument(&mut app);
    let source = core_sim::BuildSource::Tile {
        tile: PLACE,
        branch: PIT_BRANCH.into(),
    };
    let crew = app
        .world
        .get::<LaborAllocation>(one)
        .unwrap()
        .build_crew(&source)
        .expect("the carrier declared the build");
    assert!(
        crew.cap > crew.total(),
        "liveness: the pit can take more builders than it has: {crew:?}"
    );
    assert!(
        offer(&app, tribe).is_none(),
        "a build short of builders holds the offer back, though a monument is proposable"
    );
    // Fill the crew: the monument comes through.
    app.world.get_mut::<PopulationCohort>(one).unwrap().working = scalar_from_f32(PLENTY_OF_HANDS);
    // Any tribe command re-runs the pass and restaffs every member.
    tribe_levy(&mut app.world, FACTION, id(1), "high").unwrap();
    refresh_tribe_proposals(&mut app.world);
    let crew = app
        .world
        .get::<LaborAllocation>(one)
        .unwrap()
        .build_crew(&source)
        .unwrap();
    assert_eq!(crew.total(), crew.cap, "the crew is full: {crew:?}");
    assert_eq!(
        offer(&app, tribe).map(|next| next.kind),
        Some("monument".to_string()),
        "a full crew lets the next proposal through"
    );
}

#[test]
fn defer_hides_the_key_for_the_lever_and_then_it_returns() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    ready_the_monument(&mut app);
    let proposal = offer(&app, tribe).expect("liveness");
    defer_tribe_proposal(&mut app.world, FACTION, id(1), &proposal.key).unwrap();
    assert!(offer(&app, tribe).is_none(), "deferred");
    let turns = u64::from(
        app.world
            .resource::<core_sim::LaborConfigHandle>()
            .get()
            .band_orders
            .proposal_not_now_turns,
    );
    let tick = app.world.resource::<SimulationTick>().0;
    app.world.resource_mut::<SimulationTick>().0 = tick + turns - 1;
    refresh_tribe_proposals(&mut app.world);
    assert!(
        offer(&app, tribe).is_none(),
        "still deferred the turn before"
    );
    app.world.resource_mut::<SimulationTick>().0 = tick + turns;
    refresh_tribe_proposals(&mut app.world);
    assert_eq!(offer(&app, tribe).map(|p| p.key), Some(proposal.key));
}

#[test]
fn a_stale_approve_is_refused_by_token() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    ready_the_monument(&mut app);
    let proposal = offer(&app, tribe).expect("liveness");
    let wrong = approve_tribe_proposal(&mut app.world, FACTION, id(1), "monument@1,1").unwrap_err();
    assert_eq!(wrong.token, NO_SUCH_PROPOSAL);
    approve_tribe_proposal(&mut app.world, FACTION, id(1), &proposal.key).expect("approved");
    let again = approve_tribe_proposal(&mut app.world, FACTION, id(1), &proposal.key).unwrap_err();
    assert_eq!(again.token, NO_SUCH_PROPOSAL, "the key no longer matches");
    let outsider =
        approve_tribe_proposal(&mut app.world, FACTION, BandId(999_999), "x").unwrap_err();
    assert_eq!(outsider.token, "no_such_band");
}

#[test]
fn approving_a_road_adds_one_work_per_tile_still_below_grade() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    learn_for(&mut app, GRADE);
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let (base, width) = core_sim::road_rung_span(
        core_sim::shipped::route_trail(),
        &ladder,
        core_sim::NEAR_ENOUGH_TO_KEEP,
    );
    let path = {
        let tiles = app.world.resource::<core_sim::TileRegistry>();
        core_sim::trace_path(
            PLACE,
            EAST,
            tiles.width,
            tiles.height,
            false,
            app.world.resource::<core_sim::RoadRegistry>(),
        )
    };
    assert!(
        path.len() > 2,
        "liveness: the camps are several tiles apart"
    );
    {
        let mut roads = app.world.resource_mut::<core_sim::RoadRegistry>();
        for tile in &path {
            roads
                .road_or_trail(*tile, &ladder)
                .set_position(base + width, &ladder);
        }
    }
    refresh_tribe_proposals(&mut app.world);
    let proposal = offer(&app, tribe).expect("liveness: a road is on offer");
    assert_eq!(proposal.kind, "road");
    assert_eq!(proposal.key, format!("road:{}-{}", id(1).0, id(2).0));
    assert_eq!(
        proposal.text,
        "Chief2 proposes: build a road from Test1 to Test2"
    );
    approve_tribe_proposal(&mut app.world, FACTION, id(1), &proposal.key).expect("approved");
    assert_eq!(
        registry(&app).get(tribe).unwrap().works.len(),
        path.len(),
        "one route work per tile below grade"
    );
}

#[test]
fn the_book_survives_a_checkpoint() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    ready_the_monument(&mut app);
    let proposal = offer(&app, tribe).expect("liveness");
    defer_tribe_proposal(&mut app.world, FACTION, id(1), &proposal.key).unwrap();
    let before = app.world.resource::<TribeProposalBook>().clone();
    assert!(!before.tribes[&tribe].deferred.is_empty(), "liveness");
    let checkpoint = capture_sim_state(&app.world);
    app.world.resource_mut::<TribeProposalBook>().tribes.clear();
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(app.world.resource::<TribeProposalBook>(), &before);
}

#[test]
fn the_frame_carries_the_offer_on_every_member_and_the_ids_parallel_the_names() {
    let (mut app, tribe, _one, _two) = tribe_of_two();
    ready_the_monument(&mut app);
    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref()).to_vec();
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published");
    for n in [1, 2] {
        let row = section
            .iter()
            .find(|row| row.bandId() == id(n).0)
            .expect("the member is on the wire");
        assert_eq!(row.tribeId(), tribe.0, "band {n} names its tribe");
        let proposal = row.tribeProposal().expect("a proposal row");
        assert_eq!(proposal.key(), Some("monument@10,10"), "band {n}");
        assert_eq!(proposal.kind(), Some("monument"));
        assert_eq!((proposal.tileX(), proposal.tileY()), (10, 10));
        let names: Vec<_> = row.tribeMemberNames().unwrap().iter().collect();
        let bands: Vec<u64> = row.tribeMemberBands().unwrap().iter().collect();
        assert_eq!(names, ["Test2", "Test1"], "largest band first");
        assert_eq!(bands, [id(2).0, id(1).0], "parallel to the names");
    }
    let outsider = section.iter().find(|row| row.tribeId() == 0);
    assert!(
        outsider.is_none_or(|row| row
            .tribeProposal()
            .is_none_or(|p| p.key().unwrap_or("").is_empty())),
        "a band in no tribe carries no offer"
    );
}
