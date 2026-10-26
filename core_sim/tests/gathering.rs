//! **The gathering is detected, not commanded** (issue #698, `docs/plan_civilization_steps.md`
//! §"The gathering is detected, not commanded"): two bands meeting after having no live tie credit
//! the larger band's standing tile with belief, tell each people, and count toward The Telling's
//! `gatherings.held_this_turn`.
//!
//! Driven through the REAL `advance_connections` on a generated world, with the turn's contacts
//! staged into `ContactsThisTurn` the way the sight sweep and an expedition's flush stage them.

mod faction_support;
mod telling_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, UVec2, With};

use core_sim::{
    advance_connections, BandId, BandName, BeliefConfig, BeliefConfigHandle, BeliefRegistry,
    CommandEventEntry, CommandEventKind, CommandEventLog, ConnectionKey, ConnectionLedger,
    ConnectionsConfig, ContactsThisTurn, FactionId, GatheringLedger, PopulationCohort,
    ResidentBand, Sighting, SimulationTick, Tile, NO_BELIEF, NO_TIE,
};
use faction_support::{two_faction_world, HOME, RIVAL};

/// The larger band of a pair.
const BIG: u32 = 40;
/// The smaller band of a pair.
const SMALL: u32 = 12;
/// A turn on which the first meeting happens.
const FIRST_TURN: u64 = 5;
/// The tick the world's first turn resolves on.
const OPENING_TURN: u64 = 0;
/// Quiet turns run after a meeting, enough to drain a fresh tie (`0.25` at `0.02` a turn) to
/// parked and still well inside `forget_turns`.
const QUIET_TURNS: u64 = 20;
/// Turns run past a gathering to prove the Telling's signal falls back to zero.
const TURNS_AFTER: usize = 8;
/// The Telling's beat for a gathering.
const GATHERING_BEAT: &str = "belief.gathering_held";

/// A band's age once it has lived past its founding turn.
const ESTABLISHED_AGE: u32 = 5;
/// A band's age on the turn it comes into being, as `advance_connections` reads it.
const FOUNDING_AGE: u32 = 1;
/// The first id a spawned kin band takes, well clear of any worldgen band.
const KIN_BAND_ID: u64 = 9_000;

/// One band of the world, by its place in the roster.
struct Band {
    entity: Entity,
    id: BandId,
    tile: UVec2,
}

/// Two bands standing on two different tiles, with the ledger and the belief emptied so every
/// assertion reads only what the test staged.
fn pair(app: &mut App, first: FactionId, second: FactionId) -> (Band, Band) {
    let mut bands: Vec<(Entity, BandId, FactionId)> = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .map(|(entity, id, cohort)| (entity, *id, cohort.faction))
        .collect();
    bands.sort_by_key(|(_, id, _)| *id);
    let pick = |faction: FactionId, skip: Option<Entity>| {
        *bands
            .iter()
            .find(|(entity, _, f)| *f == faction && Some(*entity) != skip)
            .expect("the world has a band of that people")
    };
    let a = pick(first, None);
    let b = pick(second, Some(a.0));
    let mut out = Vec::new();
    for (entity, id, _) in [a, b] {
        let tile_entity = app
            .world
            .get::<PopulationCohort>(entity)
            .unwrap()
            .current_tile;
        let tile = app.world.get::<Tile>(tile_entity).unwrap().position;
        // Past the founding turn, so a meeting here is a real one.
        app.world
            .get_mut::<PopulationCohort>(entity)
            .unwrap()
            .age_turns = ESTABLISHED_AGE;
        out.push(Band { entity, id, tile });
    }
    assert_ne!(out[0].tile, out[1].tile, "fixture: the bands stand apart");
    *app.world.resource_mut::<ConnectionLedger>() = ConnectionLedger::default();
    *app.world.resource_mut::<BeliefRegistry>() = BeliefRegistry::default();
    *app.world.resource_mut::<GatheringLedger>() = GatheringLedger::default();
    let b = out.pop().unwrap();
    let a = out.pop().unwrap();
    (a, b)
}

fn set_size(app: &mut App, band: &Band, size: u32) {
    app.world
        .get_mut::<PopulationCohort>(band.entity)
        .unwrap()
        .size = size;
}

fn set_turn(app: &mut App, turn: u64) {
    app.world.resource_mut::<SimulationTick>().0 = turn;
}

/// Stage a live sighting both ways and run the turn's contact pass.
fn meet(app: &mut App, turn: u64, a: &Band, b: &Band) {
    stage(app, turn, a, b, true);
}

/// Stage the same pair as an expedition's report would, both ways, and run the pass.
fn report(app: &mut App, turn: u64, a: &Band, b: &Band) {
    stage(app, turn, a, b, false);
}

fn stage(app: &mut App, turn: u64, a: &Band, b: &Band, live: bool) {
    set_turn(app, turn);
    let faction_of = |app: &App, band: &Band| {
        app.world
            .get::<PopulationCohort>(band.entity)
            .unwrap()
            .faction
    };
    let (fa, fb) = (faction_of(app, a), faction_of(app, b));
    {
        let mut contacts = app.world.resource_mut::<ContactsThisTurn>();
        for (observer, subject, at, faction) in [(a, b, b.tile, fb), (b, a, a.tile, fa)] {
            let sighting = Sighting::new(at, turn, "seen", faction);
            if live {
                contacts.record(observer.id, subject.id, sighting);
            } else {
                contacts.record_report(observer.id, subject.id, sighting);
            }
        }
    }
    app.world.run_system_once(advance_connections);
}

fn belief_at(app: &App, position: UVec2) -> f32 {
    app.world.resource::<BeliefRegistry>().get(position)
}

fn per_gathering(app: &App) -> f32 {
    app.world
        .resource::<BeliefConfigHandle>()
        .get()
        .belief_per_gathering
}

fn gathering_events(app: &App) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|event| event.kind == CommandEventKind::GatheringHeld)
        .cloned()
        .collect()
}

/// Let the pair's tie bleed out to parked: an edge with strength at/below `NO_TIE` is still there.
fn let_the_tie_fade(app: &mut App, from_turn: u64, a: &Band, b: &Band) {
    let cfg = ConnectionsConfig::default();
    for turn in from_turn + 1..=from_turn + QUIET_TURNS {
        app.world
            .resource_mut::<ConnectionLedger>()
            .decay_all(turn, &cfg);
    }
    let ledger = app.world.resource::<ConnectionLedger>();
    assert!(
        !ledger.tie_is_live(a.id, b.id),
        "fixture: the tie is parked"
    );
    let edge = ledger
        .get(&ConnectionKey::new(a.id, b.id))
        .expect("a parked edge is still a memory");
    assert!(edge.strength <= NO_TIE);
}

#[test]
fn a_first_meeting_credits_the_larger_bands_tile_once() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);
    assert_eq!(belief_at(&app, b.tile), NO_BELIEF, "fixture: nothing yet");

    meet(&mut app, FIRST_TURN, &a, &b);

    let lever = per_gathering(&app);
    assert!(lever > 0.0, "the shipped lever is live");
    assert_eq!(belief_at(&app, b.tile), lever, "the larger band's tile");
    assert_eq!(
        belief_at(&app, a.tile),
        NO_BELIEF,
        "never the smaller band's"
    );
    let ledger = app.world.resource::<GatheringLedger>();
    assert_eq!(ledger.held_by(HOME), 1);
    assert_eq!(ledger.held_by(RIVAL), 1);
}

#[test]
fn a_gathering_is_held_where_the_band_stands_not_where_it_is_from() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);
    // The larger band is away from its home when the meeting happens: walk it to a third tile.
    let home_entity = app.world.get::<PopulationCohort>(b.entity).unwrap().home;
    let home = app.world.get::<Tile>(home_entity).unwrap().position;
    let (away_entity, away) = app
        .world
        .query::<(Entity, &Tile)>()
        .iter(&app.world)
        .find(|(_, tile)| tile.position != home && tile.position != a.tile)
        .map(|(entity, tile)| (entity, tile.position))
        .expect("the map has a third tile");
    app.world
        .get_mut::<PopulationCohort>(b.entity)
        .unwrap()
        .current_tile = away_entity;
    let b = Band { tile: away, ..b };

    meet(&mut app, FIRST_TURN, &a, &b);

    assert_eq!(belief_at(&app, home), NO_BELIEF);
    assert!(belief_at(&app, b.tile) > NO_BELIEF);
}

#[test]
fn a_pair_that_stays_in_contact_does_not_gather_again() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);

    for turn in FIRST_TURN..FIRST_TURN + 6 {
        meet(&mut app, turn, &a, &b);
    }

    assert_eq!(belief_at(&app, b.tile), per_gathering(&app));
    assert_eq!(app.world.resource::<GatheringLedger>().held_by(HOME), 1);
    assert_eq!(gathering_events(&app).len(), 2, "one line per people, once");
}

#[test]
fn a_pair_whose_tie_faded_gathers_again_when_it_re_meets() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);

    meet(&mut app, FIRST_TURN, &a, &b);
    let_the_tie_fade(&mut app, FIRST_TURN, &a, &b);
    meet(&mut app, FIRST_TURN + QUIET_TURNS + 1, &a, &b);

    assert_eq!(belief_at(&app, b.tile), 2.0 * per_gathering(&app));
    assert_eq!(app.world.resource::<GatheringLedger>().held_by(HOME), 2);
    let second_turn = FIRST_TURN + QUIET_TURNS + 1;
    let told_on_second_turn = gathering_events(&app)
        .iter()
        .filter(|event| event.tick == second_turn)
        .count();
    assert_eq!(
        told_on_second_turn, 2,
        "each people hears the second gathering"
    );
}

#[test]
fn two_bands_of_one_people_gather_too_and_that_people_is_told_once() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    // Both bands are of one people.
    app.world
        .get_mut::<PopulationCohort>(b.entity)
        .unwrap()
        .faction = HOME;
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);

    meet(&mut app, FIRST_TURN, &a, &b);

    assert_eq!(belief_at(&app, b.tile), per_gathering(&app));
    let events = gathering_events(&app);
    assert_eq!(events.len(), 1, "one people, one line");
    assert_eq!(events[0].faction, HOME);
    assert_eq!(
        events[0].band, b.id.0,
        "told from the lower band, naming the higher"
    );
    assert_eq!(app.world.resource::<GatheringLedger>().held_by(HOME), 1);
    assert_eq!(app.world.resource::<GatheringLedger>().held_by(RIVAL), 0);
}

#[test]
fn an_expedition_report_is_not_a_gathering() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);

    report(&mut app, FIRST_TURN, &a, &b);

    assert!(
        app.world
            .resource::<ConnectionLedger>()
            .tie_is_live(a.id, b.id),
        "liveness: the report formed the tie"
    );
    assert!(app.world.resource::<BeliefRegistry>().is_empty());
    assert!(gathering_events(&app).is_empty());
    assert_eq!(app.world.resource::<GatheringLedger>().held_by(HOME), 0);
}

#[test]
fn a_tie_on_size_is_held_at_the_lower_band_id() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, BIG);
    set_size(&mut app, &b, BIG);
    let (lower, higher) = if a.id < b.id { (&a, &b) } else { (&b, &a) };

    meet(&mut app, FIRST_TURN, &a, &b);

    assert_eq!(belief_at(&app, lower.tile), per_gathering(&app));
    assert_eq!(belief_at(&app, higher.tile), NO_BELIEF);
}

#[test]
fn each_people_is_told_in_its_own_words_with_the_band_it_met_tagged() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);
    let name_a = "Elk Band";
    let name_b = "River Band";
    app.world
        .entity_mut(a.entity)
        .insert(BandName(name_a.to_string()));
    app.world
        .entity_mut(b.entity)
        .insert(BandName(name_b.to_string()));

    meet(&mut app, FIRST_TURN, &a, &b);

    let events = gathering_events(&app);
    assert_eq!(events.len(), 2);
    let home_line = events
        .iter()
        .find(|e| e.faction == HOME)
        .expect("home told");
    let rival_line = events
        .iter()
        .find(|e| e.faction == RIVAL)
        .expect("rival told");
    assert_eq!(home_line.band, b.id.0, "the line names the band met");
    assert_eq!(rival_line.band, a.id.0, "the line names the band met");
    assert_eq!(home_line.tick, FIRST_TURN);
    assert_eq!(home_line.label, "A gathering");
    let place = format!("({}, {})", b.tile.x, b.tile.y);
    assert_eq!(
        home_line.detail.as_deref(),
        Some(format!("{name_a} met {name_b} at {place}.").as_str())
    );
    assert_eq!(
        rival_line.detail.as_deref(),
        Some(format!("{name_b} met {name_a} at {place}.").as_str())
    );
}

#[test]
fn belief_per_gathering_scales_the_credit() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);
    const TRIPLED: f32 = 3.0;
    app.world
        .insert_resource(BeliefConfigHandle::new(std::sync::Arc::new(BeliefConfig {
            belief_per_gathering: TRIPLED,
            ..BeliefConfig::default()
        })));

    meet(&mut app, FIRST_TURN, &a, &b);

    assert_eq!(belief_at(&app, b.tile), TRIPLED);
}

/// The Telling reads a gathering as `gatherings.held_this_turn`: 1 on the turn it is counted, then
/// back to 0 — so the beat fires once, and a fixed total never fires it again.
#[test]
fn the_telling_hears_a_gathering_once() {
    let mut app = telling_support::spawn_world();
    telling_support::run_turn(&mut app); // turn 0 spends the beat budget on the cold open
    assert!(!app
        .world
        .resource::<core_sim::BeatLedger>()
        .has_fired(GATHERING_BEAT));

    app.world
        .resource_mut::<GatheringLedger>()
        .record(FactionId(0));
    telling_support::run_turn(&mut app);
    assert!(
        app.world
            .resource::<core_sim::BeatLedger>()
            .has_fired(GATHERING_BEAT),
        "the turn a gathering is counted, the signal reads 1 and the beat lands"
    );
    let told = |app: &App| {
        telling_support::beats(app)
            .into_iter()
            .filter(|event| {
                event
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("gatherings.held_this_turn"))
            })
            .count()
    };
    assert_eq!(told(&app), 1);

    // Past the beat's cooldown, with the cumulative total unchanged, the signal reads 0.
    for _ in 0..TURNS_AFTER {
        telling_support::run_turn(&mut app);
    }
    assert_eq!(told(&app), 1, "an unchanged total is not a new gathering");
}

/// Mark `band` as in its founding turn, or past it.
fn set_age(app: &mut App, band: &Band, age_turns: u32) {
    app.world
        .get_mut::<PopulationCohort>(band.entity)
        .unwrap()
        .age_turns = age_turns;
}

/// A kin band of `of`'s people (a clone of it, given a fresh id), standing where `of` stands: the
/// parent or sibling a founded band is split from.
fn kin_of(app: &mut App, of: &Band, id: u64) -> Band {
    let mut cohort = app
        .world
        .get::<PopulationCohort>(of.entity)
        .unwrap()
        .clone();
    cohort.age_turns = ESTABLISHED_AGE;
    let id = BandId(id);
    let entity = app.world.spawn((id, cohort, ResidentBand)).id();
    Band {
        entity,
        id,
        tile: of.tile,
    }
}

/// Give `observer` a live tie to `subject`, as an earlier turn's contact would have.
fn already_tied(app: &mut App, observer: &Band, subject: &Band) {
    let faction = app
        .world
        .get::<PopulationCohort>(subject.entity)
        .unwrap()
        .faction;
    let sighting = Sighting::new(subject.tile, FIRST_TURN - 1, "seen", faction);
    app.world
        .resource_mut::<ConnectionLedger>()
        .insert_full_tie(
            ConnectionKey::new(observer.id, subject.id),
            &sighting,
            FIRST_TURN - 1,
        );
}

/// What a band founded after the opening turn sees of a band its people already knew is where it
/// started, not a meeting: the parent or a sibling held a live tie to that band before the split.
#[test]
fn a_founded_band_holds_no_gathering_with_a_band_its_people_already_knew() {
    for (founded_a, founded_b) in [(true, true), (true, false), (false, true)] {
        let mut app = two_faction_world();
        let (a, b) = pair(&mut app, HOME, RIVAL);
        set_size(&mut app, &a, SMALL);
        set_size(&mut app, &b, BIG);
        // Each founded side's parent already knew the other side.
        for (founded, band, other, kin_id) in [
            (founded_a, &a, &b, KIN_BAND_ID),
            (founded_b, &b, &a, KIN_BAND_ID + 1),
        ] {
            if founded {
                set_age(&mut app, band, FOUNDING_AGE);
                let parent = kin_of(&mut app, band, kin_id);
                already_tied(&mut app, &parent, other);
            }
        }

        meet(&mut app, FIRST_TURN, &a, &b);

        assert!(app.world.resource::<BeliefRegistry>().is_empty());
        assert!(gathering_events(&app).is_empty());
        assert!(
            app.world
                .resource::<ConnectionLedger>()
                .tie_is_live(a.id, b.id),
            "liveness: the tie still forms on the founding turn"
        );

        // Both established now, still in sight: the live tie means no gathering later either.
        set_age(&mut app, &a, ESTABLISHED_AGE);
        set_age(&mut app, &b, ESTABLISHED_AGE);
        meet(&mut app, FIRST_TURN + 1, &a, &b);
        assert!(gathering_events(&app).is_empty());
    }
}

/// A band founded after the opening turn that sees a people its own had never met holds a
/// gathering on its founding turn: Ray's split-off bands met three peoples on turn 1 and were told
/// of none.
#[test]
fn a_founded_band_that_meets_a_people_its_own_never_met_holds_a_gathering() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);
    set_age(&mut app, &a, FOUNDING_AGE);
    // Its parent stands beside it, tied to it by the split, and has never met `b`.
    let parent = kin_of(&mut app, &a, KIN_BAND_ID);
    already_tied(&mut app, &parent, &a);

    meet(&mut app, FIRST_TURN, &a, &b);

    assert_eq!(gathering_events(&app).len(), 2, "each people is told once");
    assert_eq!(belief_at(&app, b.tile), per_gathering(&app));
}

/// The world's first resolved turn reads the opening tick: constructing the world runs one update
/// (Startup, then the first turn), and `advance_tick` bumps the tick behind it.
#[test]
fn the_first_resolved_turn_reads_the_opening_tick() {
    let app = two_faction_world();
    assert_eq!(
        app.world.resource::<SimulationTick>().0,
        OPENING_TURN + 1,
        "one turn has resolved and advance_tick has run behind it"
    );
}

/// Bands in sight of each other when the game starts met "at turn 0": the opening turn holds the
/// gathering although both bands carry founding ages.
#[test]
fn bands_in_sight_at_world_start_hold_a_gathering_on_the_opening_turn() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app, HOME, RIVAL);
    set_size(&mut app, &a, SMALL);
    set_size(&mut app, &b, BIG);
    for band in [&a, &b] {
        app.world
            .get_mut::<PopulationCohort>(band.entity)
            .unwrap()
            .age_turns = FOUNDING_AGE;
    }

    meet(&mut app, OPENING_TURN, &a, &b);

    assert_eq!(
        belief_at(&app, b.tile),
        per_gathering(&app),
        "the larger band's tile is credited"
    );
    assert_eq!(belief_at(&app, a.tile), NO_BELIEF);
    let events = gathering_events(&app);
    assert_eq!(events.len(), 2, "one line per people");
    assert!(events.iter().any(|e| e.faction == HOME));
    assert!(events.iter().any(|e| e.faction == RIVAL));
}
