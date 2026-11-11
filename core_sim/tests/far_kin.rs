//! **Far kin** (`docs/plan_far_kin.md`, `.claude/rules/core_sim/{routes,fission,connections}.md`):
//! a split's founding walk lays a full trail, and messengers is learned by keeping far kin in
//! touch and unlocks a longer relay and a slower drain.
//!
//! The tribe's live-tie locality is `tests/tribe.rs`; messengers' pooling reach is
//! `tests/route_traffic.rs`. Every assertion that a thing did NOT happen is paired with a control
//! that does, so it can fail.

use std::sync::Arc;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_band_movement, advance_connections, advance_roads, available_workers, build_test_app,
    credit_far_kin_contact, run_turn, scalar_from_f32, scalar_zero, split_band_from_parent,
    split_band_from_parent_with, traffic_ceiling, BandId, BandTravel, ConnectionKey,
    ConnectionLedger, ConnectionsConfigHandle, ContactsThisTurn, DiscoveryProgressLedger,
    Expedition, ExpeditionMission, ExpeditionPhase, FactionId, FoundingWalk, GatheringLedger,
    LaborAllocation, LadderConfigHandle, LineDivision, LocalStore, PopulationCohort, ResidentBand,
    RoadRegistry, RouteTrafficLog, Scalar, SettleConfig, Sighting, SimulationConfig,
    SimulationTick, StartingUnit, Tile, TileRegistry, FOOD, MESSENGERS_DISCOVERY_ID,
    MESSENGERS_KNOWLEDGE,
};

const MAP_SEED: u64 = 119_304_647;
/// Working-age people the parent is stocked with, so any number of splits leaves real bands.
const PARENT_WORKERS: f32 = 60.0;
const SPLIT_WORKERS: u32 = 5;
/// How far the founding walk goes: past the shipped free reach, which is what a far split is.
const WALK_TILES: u32 = 5;
/// A shorter second walk, to show a band that has arrived walks like any other.
const SECOND_WALK_TILES: u32 = 2;
/// A delivery's distance: well past the free reach, inside one turn-loop's patience.
const DELIVERY_TILES: u32 = 8;
/// The most turns a delivery is given to arrive.
const DELIVERY_TURN_LIMIT: u32 = 60;
/// Old enough that no band is a founding band, so a meeting counts as a gathering.
const SETTLED_AGE_TURNS: u32 = 50;
/// Quiet turns the drain is measured over: long enough to show, short enough not to park the tie.
const QUIET_TURNS: u64 = 10;
const EPSILON: f32 = 1e-4;

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    run_turn(&mut app);
    // Splits move goods only once the opening windows are shut; these are not about the dowry, but
    // a generous pack keeps every split admissible.
    let mut expedition = (*app
        .world
        .resource::<core_sim::ExpeditionConfigHandle>()
        .get())
    .clone();
    expedition.carry.per_worker_carry = 10_000.0;
    app.world
        .insert_resource(core_sim::ExpeditionConfigHandle::new(Arc::new(expedition)));
    app
}

fn settle() -> SettleConfig {
    SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    }
}

fn parent_band(app: &mut App) -> Entity {
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    let parent = query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns at least one resident band");
    let mut cohort = app.world.get_mut::<PopulationCohort>(parent).unwrap();
    cohort.working = Scalar::from_f32(PARENT_WORKERS);
    cohort.sync_size();
    parent
}

fn entity_for_band(app: &mut App, band: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
        .expect("the band exists")
}

fn position_of(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .unwrap()
        .current_tile;
    app.world.get::<Tile>(tile).unwrap().position
}

fn faction_of(app: &App, band: Entity) -> FactionId {
    app.world.get::<PopulationCohort>(band).unwrap().faction
}

/// A target `tiles` along the row from `from`, on whichever side the map has room for.
fn target_along_row(app: &App, from: UVec2, tiles: u32) -> UVec2 {
    let width = app.world.resource::<TileRegistry>().width;
    let x = if from.x + tiles < width {
        from.x + tiles
    } else {
        from.x - tiles
    };
    UVec2::new(x, from.y)
}

/// Put `band` on the tile at `at` (a fixture standing its kin as far apart as it likes).
fn stand_at(app: &mut App, band: Entity, at: UVec2) {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("the tile is on the map");
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.current_tile = tile;
    cohort.home = tile;
}

fn ceiling(app: &App) -> f32 {
    traffic_ceiling(&app.world.resource::<LadderConfigHandle>().get())
}

/// Step `band` toward the target its `BandTravel` names, one `advance_band_movement` at a time, and
/// return the journeys' per-tile weights in the order they were recorded, with the tiles stood on.
/// `advance_roads` drains the log after each step, as the next turn's Logistics does.
fn walk(app: &mut App, band: Entity, steps: u32) -> (Vec<f32>, Vec<UVec2>) {
    let mut weights = Vec::new();
    let mut stood = vec![position_of(app, band)];
    for _ in 0..steps {
        app.world.run_system_once(advance_band_movement);
        weights.extend(
            app.world
                .resource::<RouteTrafficLog>()
                .journeys
                .iter()
                .map(|journey| journey.work_per_tile),
        );
        stood.push(position_of(app, band));
        app.world.run_system_once(advance_roads);
    }
    (weights, stood)
}

#[test]
fn a_commanded_splits_founding_walk_lays_a_full_trail_on_every_tile_it_crosses() {
    let mut app = spawn_world();
    let parent = parent_band(&mut app);
    let from = position_of(&app, parent);
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle())
        .expect("a stocked parent can split");
    let child = entity_for_band(&mut app, split.band);
    assert!(
        app.world.get::<FoundingWalk>(child).is_some(),
        "the commanded split's new band is on its founding walk"
    );
    assert!(
        app.world.get::<FoundingWalk>(parent).is_none(),
        "and only the new band"
    );

    let target = target_along_row(&app, from, WALK_TILES);
    app.world.entity_mut(child).insert(BandTravel::to(target));
    let (weights, stood) = walk(&mut app, child, WALK_TILES);
    let full_trail = ceiling(&app);
    let workers = available_workers(app.world.get::<PopulationCohort>(child).unwrap().working);
    let marched = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .route_traffic
        .work_per_worker_tile
        * workers as f32;
    assert_eq!(weights.len(), WALK_TILES as usize, "one journey a step");
    for weight in &weights {
        assert_eq!(*weight, full_trail, "each crossing banks a full trail");
    }
    assert!(
        marched < full_trail,
        "control: the ordinary march rate would have left a trail barely begun"
    );
    assert_eq!(*stood.last().unwrap(), target, "the walk arrived");
    let roads = app.world.resource::<RoadRegistry>();
    for tile in &stood {
        let road = roads
            .road(*tile)
            .expect("every tile the walk touched holds a road");
        assert!(
            road.position() >= full_trail - EPSILON,
            "{tile:?} is worn to a full trail, not {}",
            road.position()
        );
    }
    assert!(
        app.world.get::<FoundingWalk>(child).is_none(),
        "the marker clears when the first move order completes"
    );
    assert!(app.world.get::<BandTravel>(child).is_none());
}

#[test]
fn a_second_move_after_arrival_banks_the_ordinary_march() {
    let mut app = spawn_world();
    let parent = parent_band(&mut app);
    let from = position_of(&app, parent);
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle()).unwrap();
    let child = entity_for_band(&mut app, split.band);
    let first = target_along_row(&app, from, WALK_TILES);
    app.world.entity_mut(child).insert(BandTravel::to(first));
    walk(&mut app, child, WALK_TILES);
    assert!(app.world.get::<FoundingWalk>(child).is_none());

    let second = target_along_row(&app, first, SECOND_WALK_TILES);
    app.world.entity_mut(child).insert(BandTravel::to(second));
    let (weights, _) = walk(&mut app, child, SECOND_WALK_TILES);
    let workers = available_workers(app.world.get::<PopulationCohort>(child).unwrap().working);
    let marched = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .route_traffic
        .work_per_worker_tile
        * workers as f32;
    assert_eq!(weights.len(), SECOND_WALK_TILES as usize);
    for weight in weights {
        assert_eq!(weight, marched, "after arrival a march is a march again");
    }
}

#[test]
fn a_breeding_break_off_is_on_no_founding_walk() {
    let mut app = spawn_world();
    let parent = parent_band(&mut app);
    let from = position_of(&app, parent);
    let split = split_band_from_parent_with(
        &mut app.world,
        parent,
        SPLIT_WORKERS,
        &settle(),
        LineDivision::Copy {
            people_per_line: std::num::NonZeroU16::new(1).unwrap(),
        },
    )
    .expect("a stocked parent can break a band off");
    let child = entity_for_band(&mut app, split.band);
    assert!(
        app.world.get::<FoundingWalk>(child).is_none(),
        "nobody ordered the walk"
    );
    let target = target_along_row(&app, from, WALK_TILES);
    app.world.entity_mut(child).insert(BandTravel::to(target));
    let (weights, _) = walk(&mut app, child, WALK_TILES);
    let full_trail = ceiling(&app);
    assert!(!weights.is_empty());
    for weight in weights {
        assert!(weight < full_trail, "an ordinary march banks no full trail");
    }
}

#[test]
fn the_founding_walk_survives_a_checkpoint() {
    let mut app = spawn_world();
    let parent = parent_band(&mut app);
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle()).unwrap();
    let state = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &state);
    let child = entity_for_band(&mut app, split.band);
    assert!(
        app.world.get::<FoundingWalk>(child).is_some(),
        "a save taken mid-walk keeps the marker"
    );
    let parent = entity_for_band(&mut app, state.bands.iter().map(|b| b.id).min().unwrap());
    assert!(app.world.get::<FoundingWalk>(parent).is_none());
}

// ---------------------------------------------------------------------------------------------
// MESSENGERS — what teaches it
// ---------------------------------------------------------------------------------------------

fn messengers_progress(app: &App, faction: FactionId) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(faction, MESSENGERS_DISCOVERY_ID)
        .to_f32()
}

/// Ledger progress `practice` units of messengers are worth.
fn worth(app: &App, practice: f32) -> f32 {
    app.world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .ledger_credit(MESSENGERS_KNOWLEDGE, practice)
        .expect("messengers is priced")
}

fn teach_messengers(app: &mut App, faction: FactionId) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, MESSENGERS_DISCOVERY_ID, Scalar::from_f32(1.0));
}

/// The ladder with the passive lesson switched off, so a delivery's credit stands alone.
fn without_the_passive_lesson(app: &mut App) {
    let mut ladder = (*app.world.resource::<LadderConfigHandle>().get()).clone();
    ladder.knowledge.messengers_lesson_per_turn = 0.0;
    app.world
        .insert_resource(LadderConfigHandle::new(Arc::new(ladder)));
}

#[test]
fn far_kin_in_touch_teach_messengers_once_a_turn_per_people_and_near_kin_teach_nothing() {
    let mut app = spawn_world();
    let parent = parent_band(&mut app);
    let home = position_of(&app, parent);
    let first = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle()).unwrap();
    let second = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle()).unwrap();
    let (first, second) = (
        entity_for_band(&mut app, first.band),
        entity_for_band(&mut app, second.band),
    );
    let faction = faction_of(&app, parent);
    let reach = core_sim::SupplyNetworkConfigHandle::default()
        .get()
        .reach_tiles;

    // Co-located kin, tied both ways by the split: in touch, but not FAR.
    app.world.run_system_once(credit_far_kin_contact);
    assert_eq!(
        messengers_progress(&app, faction),
        0.0,
        "kin within the free reach teach nothing"
    );

    // Two kin bands move far from the parent (and from each other): three far pairs, one credit.
    let (near, far) = (
        target_along_row(&app, home, reach + 2),
        target_along_row(&app, home, reach + 4),
    );
    stand_at(&mut app, first, near);
    stand_at(&mut app, second, far);
    app.world.run_system_once(credit_far_kin_contact);
    let per_turn = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .messengers_lesson_per_turn;
    assert!(per_turn > 0.0);
    let credited = messengers_progress(&app, faction);
    assert!(
        (credited - worth(&app, per_turn)).abs() < EPSILON,
        "once per people per turn however many far pairs: {credited}"
    );

    // Control: let every tie lapse (a parked edge is not live) and the credit stops.
    let cfg = app.world.resource::<ConnectionsConfigHandle>().get();
    {
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for _ in 0..((1.0 / cfg.strength.decay_per_turn).ceil() as u64 + 1) {
            ledger.decay_all(1, &cfg);
        }
    }
    app.world.run_system_once(credit_far_kin_contact);
    assert!(
        (messengers_progress(&app, faction) - credited).abs() < EPSILON,
        "out of touch teaches nothing"
    );
}

/// A destination band of the fixture's own people, `DELIVERY_TILES` away, and a Trade party launched
/// from the parent at `launch_tile`. Returns the party and the two ends' positions.
fn launch_shipment_to_kin(app: &mut App, same_people: bool) -> (Entity, UVec2, UVec2) {
    let parent = parent_band(app);
    let home = position_of(app, parent);
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle()).unwrap();
    let destination = entity_for_band(app, split.band);
    let destination_pos = target_along_row(app, home, DELIVERY_TILES);
    stand_at(app, destination, destination_pos);
    if !same_people {
        app.world
            .get_mut::<PopulationCohort>(destination)
            .unwrap()
            .faction = FactionId(9);
    }
    let destination_faction = faction_of(app, destination);

    let mut cohort = app.world.get::<PopulationCohort>(parent).unwrap().clone();
    cohort.working = Scalar::from_u32(2);
    cohort.children = scalar_zero();
    cohort.elders = scalar_zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let mut cargo = LocalStore::new();
    cargo.add_food("dry", scalar_from_f32(4.0));
    let band_id = app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    let party = app
        .world
        .spawn((
            cohort,
            band_id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: parent,
                mission: ExpeditionMission::Trade {
                    destination_band: split.band,
                    destination_faction,
                    destination_name: "the kin".to_string(),
                    terms: Default::default(),
                },
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo,
                defection_pull: Scalar::zero(),
                station_reports: Vec::new(),
                goodwill_news: Default::default(),
                exchange_return: Default::default(),
                unpaid_return: Default::default(),
                launch_tile: Some(home),
                scout_pack: Default::default(),
                aim: Some(destination_pos),
            },
            BandTravel::to(destination_pos),
        ))
        .id();
    let _ = FOOD;
    (party, home, destination_pos)
}

/// Drive turns until the party has handed over (it is no longer `Outbound`), and say how many ran.
fn until_delivered(app: &mut App, party: Entity) {
    for _ in 0..DELIVERY_TURN_LIMIT {
        run_turn(app);
        let phase = app
            .world
            .get::<Expedition>(party)
            .map(|expedition| expedition.phase);
        if phase != Some(ExpeditionPhase::Outbound) {
            return;
        }
    }
    panic!("the party never arrived");
}

#[test]
fn a_delivery_between_kin_teaches_messengers_per_tile_walked_and_one_to_strangers_does_not() {
    let mut kin = spawn_world();
    without_the_passive_lesson(&mut kin);
    let parent = parent_band(&mut kin);
    let faction = faction_of(&kin, parent);
    let (party, home, destination) = launch_shipment_to_kin(&mut kin, true);
    assert_eq!(messengers_progress(&kin, faction), 0.0);
    until_delivered(&mut kin, party);
    let width = kin.world.resource::<TileRegistry>().width;
    let wrap = kin
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let tiles = core_sim::grid_utils::hex_distance_wrapped(home, destination, width, wrap);
    let per_tile = kin
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .messengers_lesson_per_tile;
    let credited = messengers_progress(&kin, faction);
    assert!(
        (credited - worth(&kin, per_tile * tiles as f32)).abs() < EPSILON,
        "credit is per tile walked, launch to arrival: {credited} for {tiles} tiles"
    );
    assert!(credited > 0.0);

    // Control: the same delivery to another people teaches nothing.
    let mut strangers = spawn_world();
    without_the_passive_lesson(&mut strangers);
    let parent = parent_band(&mut strangers);
    let faction = faction_of(&strangers, parent);
    let (party, ..) = launch_shipment_to_kin(&mut strangers, false);
    until_delivered(&mut strangers, party);
    assert_eq!(messengers_progress(&strangers, faction), 0.0);
}

// ---------------------------------------------------------------------------------------------
// MESSENGERS — what it unlocks
// ---------------------------------------------------------------------------------------------

/// `count` resident bands of one people that have never met: ties cleared, settled, no gatherings.
fn kin_bands(count: usize) -> (App, Vec<BandId>, FactionId) {
    let mut app = spawn_world();
    let parent = parent_band(&mut app);
    let mut entities = vec![parent];
    for _ in 1..count {
        let split =
            split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle()).unwrap();
        entities.push(entity_for_band(&mut app, split.band));
    }
    app.world.insert_resource(ConnectionLedger::default());
    app.world.insert_resource(GatheringLedger::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    for entity in &entities {
        app.world
            .get_mut::<PopulationCohort>(*entity)
            .unwrap()
            .age_turns = SETTLED_AGE_TURNS;
    }
    let faction = faction_of(&app, parent);
    let ids = entities
        .iter()
        .map(|entity| *app.world.get::<BandId>(*entity).unwrap())
        .collect();
    (app, ids, faction)
}

fn sighting_of(app: &mut App, subject: BandId) -> Sighting {
    let mut query = app.world.query::<(&BandId, &PopulationCohort)>();
    let (_, cohort) = query
        .iter(&app.world)
        .find(|(id, _)| **id == subject)
        .expect("the subject exists");
    let position = app.world.get::<Tile>(cohort.current_tile).unwrap().position;
    let tick = app.world.resource::<SimulationTick>().0;
    Sighting::new(position, tick, "seen", cohort.faction)
}

fn connections_turn(app: &mut App, tick: u64, sightings: &[(BandId, BandId)]) {
    app.world.resource_mut::<SimulationTick>().0 = tick;
    for (observer, subject) in sightings {
        let sighting = sighting_of(app, *subject);
        app.world
            .resource_mut::<ContactsThisTurn>()
            .record(*observer, *subject, sighting);
    }
    app.world.run_system_once(advance_connections);
}

fn edge_is_live(app: &App, from: BandId, to: BandId) -> bool {
    app.world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(from, to))
        .is_some_and(|edge| edge.strength > Scalar::zero())
}

/// Four kin in a directed chain `a sees b sees c sees d` (nobody sees back): `a` hears of `d` only
/// across three hops.
fn chain_of_four(known: bool) -> bool {
    let (mut app, ids, faction) = kin_bands(4);
    if known {
        teach_messengers(&mut app, faction);
    }
    let sightings = [(ids[0], ids[1]), (ids[1], ids[2]), (ids[2], ids[3])];
    for tick in 1..=5 {
        connections_turn(&mut app, tick, &sightings);
    }
    assert!(
        edge_is_live(&app, ids[0], ids[2]),
        "two hops relay at the shipped depth either way"
    );
    edge_is_live(&app, ids[0], ids[3])
}

#[test]
fn a_people_that_knows_messengers_relays_a_third_hop() {
    assert!(
        !chain_of_four(false),
        "without messengers a tie relays across two hops and no further"
    );
    assert!(
        chain_of_four(true),
        "with messengers the relay reaches a third band"
    );
}

/// The strength a tie `A to B` has after `QUIET_TURNS` with no contact at all.
fn strength_after_quiet_turns(known: bool) -> f32 {
    let (mut app, ids, faction) = kin_bands(2);
    if known {
        teach_messengers(&mut app, faction);
    }
    let sighting = sighting_of(&mut app, ids[1]);
    app.world
        .resource_mut::<ConnectionLedger>()
        .insert_full_tie(ConnectionKey::new(ids[0], ids[1]), &sighting, 1);
    for tick in 2..2 + QUIET_TURNS {
        connections_turn(&mut app, tick, &[]);
    }
    app.world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(ids[0], ids[1]))
        .expect("the tie is still there")
        .strength
        .to_f32()
}

#[test]
fn a_people_that_knows_messengers_loses_its_ties_more_slowly() {
    let cfg = core_sim::ConnectionsConfig::default();
    let quiet = QUIET_TURNS as f32;
    let without = strength_after_quiet_turns(false);
    let with = strength_after_quiet_turns(true);
    assert!(
        (without - (1.0 - quiet * cfg.strength.decay_per_turn)).abs() < EPSILON,
        "the shipped drain: {without}"
    );
    assert!(
        (with - (1.0 - quiet * cfg.strength.decay_per_turn_with_messengers)).abs() < EPSILON,
        "the messengers drain: {with}"
    );
    assert!(with > without);
}
