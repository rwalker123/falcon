//! **Kin relay a tie, within one people** (`.claude/rules/core_sim/connections.md`).
//!
//! The sight sweep is not run here. Each turn the fixture writes the direct sightings it wants into
//! `ContactsThisTurn` and runs `advance_connections` once, so the geometry of who can see whom is a
//! stated fact of the fixture rather than an accident of a seed's terrain. Every assertion that a
//! thing did NOT happen is paired with a control world where it does, so it can fail.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::Entity;
use std::sync::Arc;

use core_sim::{
    advance_connections, build_test_app, split_band_from_parent, BandId, ConnectionKey,
    ConnectionLedger, ConnectionsConfig, ConnectionsConfigHandle, ContactsThisTurn,
    DiscoveryProgressLedger, FactionId, GatheringLedger, PopulationCohort, Scalar, SettleConfig,
    Sighting, SimulationConfig, SimulationMetrics, SimulationTick, Tile, RITUAL_DISCOVERY_ID,
};

const MAP_SEED: u64 = 119_304_647;
const PARENT_WORKERS: f32 = 20.0;
const SPLIT_WORKERS: u32 = 5;
/// Old enough that no band is a founding band, so a meeting counts as a gathering.
const SETTLED_AGE_TURNS: u32 = 50;
/// The first turn the fixture runs; any turn past the opening one.
const FIRST_TURN: u64 = 1;
/// Turns enough for a tie that is never refreshed to drain to nothing at the shipped decay, and
/// still well inside `forget_turns`.
const PARKING_TURNS: u64 = 60;
/// A people that is not the fixture's own.
const FOREIGN_PEOPLE: FactionId = FactionId(9);
/// A few quiet turns, enough for the decay to show without parking the tie.
const DRAIN_PROBE_TURNS: u64 = 5;
const NEAR_NOTHING: f32 = 1e-6;

struct Trio {
    app: App,
    a: BandId,
    b: BandId,
    c: BandId,
    b_entity: Entity,
    people: FactionId,
}

fn id_of(app: &App, entity: Entity) -> BandId {
    *app.world.get::<BandId>(entity).expect("a band has an id")
}

fn with_hops(app: &mut App, max_hops: u32) {
    let json = format!(r#"{{ "kin_relay": {{ "max_hops": {max_hops} }} }}"#);
    let cfg = ConnectionsConfig::from_json_str(&json).expect("the fixture config parses");
    app.world
        .insert_resource(ConnectionsConfigHandle::new(Arc::new(cfg)));
}

/// Three resident bands of one people, with no ties yet.
fn trio(max_hops: u32) -> Trio {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();

    let a_entity = {
        let mut query = app
            .world
            .query_filtered::<Entity, bevy::prelude::With<core_sim::ResidentBand>>();
        query.iter(&app.world).next().expect("a resident band")
    };
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(a_entity).unwrap();
        cohort.working = Scalar::from_f32(PARENT_WORKERS * 2.0);
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let mut split = || {
        let band = split_band_from_parent(&mut app.world, a_entity, SPLIT_WORKERS, &settle)
            .expect("a stocked parent can split")
            .band;
        let mut query = app.world.query::<(Entity, &BandId)>();
        query
            .iter(&app.world)
            .find(|(_, id)| **id == band)
            .map(|(entity, _)| entity)
            .expect("the split allocated this id")
    };
    let b_entity = split();
    let c_entity = split();
    // A split starts its halves fully tied; the fixture wants bands that have never met.
    app.world.insert_resource(ConnectionLedger::default());
    app.world.insert_resource(GatheringLedger::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    for entity in [a_entity, b_entity, c_entity] {
        app.world
            .get_mut::<PopulationCohort>(entity)
            .unwrap()
            .age_turns = SETTLED_AGE_TURNS;
    }
    let people = app.world.get::<PopulationCohort>(a_entity).unwrap().faction;
    with_hops(&mut app, max_hops);
    let (a, b, c) = (
        id_of(&app, a_entity),
        id_of(&app, b_entity),
        id_of(&app, c_entity),
    );
    Trio {
        app,
        a,
        b,
        c,
        b_entity,
        people,
    }
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

/// One turn: the listed `(observer, subject)` pairs sight each other live, then the system runs.
fn turn(app: &mut App, tick: u64, sightings: &[(BandId, BandId)]) {
    app.world.resource_mut::<SimulationTick>().0 = tick;
    for (observer, subject) in sightings {
        let sighting = sighting_of(app, *subject);
        app.world
            .resource_mut::<ContactsThisTurn>()
            .record(*observer, *subject, sighting);
    }
    app.world.run_system_once(advance_connections);
}

/// A sights B and B sights C every turn (directed); A never sights C and nobody sights back.
fn chain_sightings(t: &Trio) -> [(BandId, BandId); 2] {
    [(t.a, t.b), (t.b, t.c)]
}

fn run_chain(t: &mut Trio, turns: u64) {
    let sightings = chain_sightings(t);
    for tick in FIRST_TURN..FIRST_TURN + turns {
        turn(&mut t.app, tick, &sightings);
    }
}

fn strength(app: &App, from: BandId, to: BandId) -> Option<Scalar> {
    app.world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(from, to))
        .map(|edge| edge.strength)
}

fn live(app: &App, from: BandId, to: BandId) -> bool {
    app.world
        .resource::<ConnectionLedger>()
        .tie_is_live(from, to)
}

#[test]
fn kin_two_hops_away_stay_tied_past_the_point_an_unrelayed_tie_parks() {
    let mut relayed = trio(2);
    run_chain(&mut relayed, PARKING_TURNS);
    let (a, b, c) = (relayed.a, relayed.b, relayed.c);
    assert!(live(&relayed.app, a, c), "A holds a live tie to C");
    assert!(
        strength(&relayed.app, a, c).unwrap() > Scalar::zero(),
        "the directed A to C edge is the one a Trade launch gates on"
    );
    assert!(
        relayed
            .app
            .world
            .resource::<ConnectionLedger>()
            .get(&ConnectionKey::new(c, a))
            .is_none(),
        "relay never creates a reverse edge"
    );
    // The relayed tie names where C is.
    let edge = relayed
        .app
        .world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(a, c))
        .unwrap()
        .clone();
    assert_eq!(edge.last_seen_turn, FIRST_TURN + PARKING_TURNS - 1);
    assert!(live(&relayed.app, a, b) && live(&relayed.app, b, c));

    // Control: relay off, the same world parks A to C... if it ever existed. It never forms, because
    // nobody sighted anybody across the gap.
    let mut off = trio(0);
    run_chain(&mut off, PARKING_TURNS);
    assert!(!live(&off.app, off.a, off.c));
    assert!(
        live(&off.app, off.a, off.b),
        "the control's direct ties are live"
    );
}

#[test]
fn one_hop_does_not_reach_kin_two_ties_away() {
    // A and C are two ties apart. At one hop A hears of B and C hears of B, and neither hears of
    // the other; at two hops they do. At zero the world is exactly the pre-change one.
    let mut one = trio(1);
    run_chain(&mut one, PARKING_TURNS);
    assert!(live(&one.app, one.a, one.b) && live(&one.app, one.b, one.c));
    assert!(
        one.app
            .world
            .resource::<ConnectionLedger>()
            .get(&ConnectionKey::new(one.a, one.c))
            .is_none(),
        "max_hops 1 relays no tie across two hops"
    );

    let mut two = trio(2);
    run_chain(&mut two, PARKING_TURNS);
    assert!(live(&two.app, two.a, two.c), "control: two hops reach");

    let mut off = trio(0);
    run_chain(&mut off, PARKING_TURNS);
    assert!(off
        .app
        .world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(off.a, off.c))
        .is_none());
    assert_eq!(off.app.world.resource::<ConnectionLedger>().len(), 2);
}

#[test]
fn a_chain_through_another_people_relays_nothing() {
    let mut t = trio(2);
    t.app
        .world
        .get_mut::<PopulationCohort>(t.b_entity)
        .unwrap()
        .faction = FOREIGN_PEOPLE;
    run_chain(&mut t, PARKING_TURNS);
    assert!(live(&t.app, t.a, t.b), "A is tied to the stranger");
    assert!(live(&t.app, t.b, t.c));
    assert!(
        t.app
            .world
            .resource::<ConnectionLedger>()
            .get(&ConnectionKey::new(t.a, t.c))
            .is_none(),
        "no tie to C is relayed through a foreign band"
    );
    assert_ne!(t.people, FOREIGN_PEOPLE);

    // Control: the same chain with B of the same people does relay.
    let mut same = trio(2);
    run_chain(&mut same, PARKING_TURNS);
    assert!(live(&same.app, same.a, same.c));
}

#[test]
fn relay_fires_no_gathering_and_teaches_no_ritual() {
    fn first_turn(max_hops: u32) -> (u64, f32, bool) {
        let mut t = trio(max_hops);
        let sightings = chain_sightings(&t);
        turn(&mut t.app, FIRST_TURN, &sightings);
        let held = t.app.world.resource::<GatheringLedger>().held_by(t.people);
        let progress = t
            .app
            .world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(t.people, RITUAL_DISCOVERY_ID)
            .to_f32();
        let ac_formed = t
            .app
            .world
            .resource::<ConnectionLedger>()
            .get(&ConnectionKey::new(t.a, t.c))
            .is_some();
        let formed = t
            .app
            .world
            .resource::<SimulationMetrics>()
            .connections_formed;
        // Two direct edges, plus the relayed A to C edge when relay is on.
        let expected = if max_hops > 0 { 3 } else { 2 };
        assert_eq!(formed, expected, "relay-formed edges are counted");
        (held, progress, ac_formed)
    }
    let (held_on, progress_on, formed_on) = first_turn(2);
    let (held_off, progress_off, formed_off) = first_turn(0);

    assert!(formed_on, "relay formed a brand-new A to C edge");
    assert!(!formed_off, "control: with relay off there is no such edge");
    // Liveness controls: the two direct first meetings DID hold gatherings and teach ritual.
    assert!(held_off > 0, "the direct meetings hold gatherings");
    assert!(progress_off > NEAR_NOTHING, "and teach the ritual lesson");
    assert_eq!(held_on, held_off, "relay made no extra gathering");
    assert!(
        (progress_on - progress_off).abs() < NEAR_NOTHING,
        "relay taught no ritual: {progress_on} vs {progress_off}"
    );
}

/// **Relayed kin who then meet have not been apart**: the relayed tie is live, so a direct sighting
/// between them is no gathering. The control (relay off) has no A to C tie, and the same meeting IS
/// a gathering.
#[test]
fn relayed_kin_who_later_meet_directly_hold_no_gathering() {
    fn gatherings_the_meeting_adds(max_hops: u32) -> u64 {
        let mut t = trio(max_hops);
        let chain = chain_sightings(&t);
        turn(&mut t.app, FIRST_TURN, &chain);
        let before = t.app.world.resource::<GatheringLedger>().held_by(t.people);
        let meeting = [(t.a, t.c), (t.c, t.a)];
        turn(&mut t.app, FIRST_TURN + 1, &meeting);
        t.app.world.resource::<GatheringLedger>().held_by(t.people) - before
    }
    assert_eq!(
        gatherings_the_meeting_adds(2),
        0,
        "A and C were tied by news, so meeting is not a gathering"
    );
    assert!(
        gatherings_the_meeting_adds(0) > 0,
        "control: without relay the same meeting is a gathering"
    );
}

// The Trade-shipment launch gate lives in the `server` binary's command handler
// (`bin/server.rs`, which reads `ConnectionLedger::get(home -> target).strength > NO_TIE`) and is
// not reachable from an integration test. The first test above asserts exactly that predicate on
// the relayed A to C edge.

#[test]
fn a_relayed_tie_drains_once_the_direct_news_stops() {
    let mut t = trio(2);
    run_chain(&mut t, PARKING_TURNS);
    let before = strength(&t.app, t.a, t.c).expect("relay formed A to C");
    assert_eq!(before, core_sim::FULL_TIE);

    // B stops sighting C; A still sights B. No fresh direct news reaches A about C.
    let only_ab = [(t.a, t.b)];
    let mut tick = FIRST_TURN + PARKING_TURNS;
    for _ in 0..DRAIN_PROBE_TURNS {
        turn(&mut t.app, tick, &only_ab);
        tick += 1;
    }
    let after = strength(&t.app, t.a, t.c).expect("a drained tie is not deleted");
    assert!(
        after < before,
        "the relayed tie drains: {before:?} -> {after:?}"
    );
    assert!(
        strength(&t.app, t.a, t.b).unwrap() > after,
        "control: the directly sighted A to B tie is still fresh"
    );
    for _ in 0..PARKING_TURNS {
        turn(&mut t.app, tick, &only_ab);
        tick += 1;
    }
    assert_eq!(strength(&t.app, t.a, t.c), Some(core_sim::NO_TIE));
}
