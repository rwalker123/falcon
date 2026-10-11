//! **Another people's lines count only while you are in touch** (issues #689, #691,
//! `docs/plan_civilization_steps.md` §"The mechanism: an isolated people cannot grow past its
//! lines").
//!
//! A breeding population is a people's tie-joined group of bands (the grouping independence reads
//! for a heart); its lines are its members' OWN lines plus the own lines of every band of another
//! people holding a live tie with a member. Nothing is copied: when the tie bleeds out the lines
//! leave. The lift past the inbreeding ceiling is a HEAD-COUNT latched per people.
//!
//! Driven through the REAL `simulate_population` on a generated two-people world, and read off the
//! ENCODED envelope. Contact is the ledger's own stamp, written through
//! `ConnectionLedger::record_contact` (the call `advance_connections` makes). `advance_tick` runs in
//! the Snapshot stage, after the Visibility pass that stamps contact, so the Population pass of the
//! next turn reads a contact stamped on turn `t` at tick `t + 1`: [`population_turn`] takes the
//! tick it runs at.

mod faction_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_band_independence, recapture_snapshot_in_place, scalar_from_f32, simulate_population,
    split_band_from_parent, BandId, ConnectionKey, ConnectionLedger, ConnectionsConfig,
    DemographicsConfigHandle, FactionId, FoundingLines, FreeBreedingPeoples, LaborAllocation,
    PopulationCohort, ResidentBand, SettleConfig, Sighting, SimulationTick, SnapshotHistory,
    ViewerFaction, NO_INBREEDING_CEILING, NO_TIE,
};
use faction_support::{two_faction_world, HOME, RIVAL};

/// Fission floors relaxed to the minimum — a split here only mints a second band of one people.
const SETTLE: SettleConfig = SettleConfig {
    min_founding_workers: 1,
    parent_min_workers: 0,
};
/// Workers a fixture split sends off.
const SPLIT_WORKERS: u32 = 3;
/// Lines each fixture band is seated with — the shipped `L`, stated so the sums read plainly.
const LINES_EACH: u16 = 8;
/// The turn contact happens on.
const CONTACT_TURN: u64 = 5;
/// The tick a Population pass reads that contact at: the next turn's.
const READ_TICK: u64 = CONTACT_TURN + 1;
/// A tick a few quiet turns on — the tie is still live but no contact refreshed it.
const QUIET_TICK: u64 = CONTACT_TURN + 5;
/// Turns long enough for a full tie to drain to zero at the shipped bleed, still inside
/// `forget_turns`.
const TURNS_TO_BLEED_OUT: u64 = 100;
/// Lines each of two bands holds when their union just clears `free_breeding_at` in lines × K.
const LIFTING_LINES_EACH: u16 = 14;
/// Turns a latched population runs to show it grows free.
const FREE_GROWTH_TURNS: usize = 5;
/// Room below `free_breeding_at` that one turn's births cannot cross, in people.
const SHORT_OF_FREE_PEOPLE: f32 = 60.0;
/// Where an independence fixture's aggrieved band stands: half the map away.
const AGGRIEVED: f32 = 5.0;

#[derive(Clone, Copy)]
struct Band {
    entity: Entity,
    id: BandId,
}

fn bands_of(app: &mut App, faction: FactionId) -> Vec<Band> {
    let mut found: Vec<Band> = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, id, _)| Band { entity, id: *id })
        .collect();
    found.sort_by_key(|band| band.id);
    found
}

fn first_band(app: &mut App, faction: FactionId) -> Band {
    bands_of(app, faction)
        .into_iter()
        .next()
        .expect("worldgen spawns a band for every people")
}

/// A second band of the home people, split off the first. Its lines are re-seated by the caller.
fn split_home_band(app: &mut App, parent: &Band) -> Band {
    let split = split_band_from_parent(&mut app.world, parent.entity, SPLIT_WORKERS, &SETTLE)
        .expect("the home band can spare the splinter");
    let entity = app
        .world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == split.band)
        .map(|(entity, _)| entity)
        .expect("the splinter is in the world");
    Band {
        entity,
        id: split.band,
    }
}

/// Seat `band` with `count` lines founded on its own id.
fn seat_lines(app: &mut App, band: &Band, count: u16) {
    seat_line_set(app, band, FoundingLines::founded(band.id, count));
}

fn seat_line_set(app: &mut App, band: &Band, lines: FoundingLines) {
    app.world
        .get_mut::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .founding_lines = lines;
    // No flow telemetry reads as a neutral trend; irrelevant to a ceiling, and it keeps the
    // fixture independent of the labor pass.
    app.world
        .entity_mut(band.entity)
        .remove::<LaborAllocation>();
}

fn seat_people(app: &mut App, band: &Band, people: f32) {
    let dist = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .initial_distribution
        .clone();
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band.entity)
        .expect("the band has a cohort");
    cohort.children = scalar_from_f32(people * dist.children);
    cohort.working = scalar_from_f32(people * dist.working);
    cohort.elders = scalar_from_f32(people * dist.elders);
    cohort.sync_size();
}

fn lines(app: &App, band: &Band) -> FoundingLines {
    app.world
        .get::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .founding_lines
        .clone()
}

/// Start every fixture from a ledger with no ties — a split seeds a full tie both ways.
fn forget_ties(app: &mut App) {
    *app.world.resource_mut::<ConnectionLedger>() = ConnectionLedger::default();
}

/// Record contact from `observer` to `subject` on `turn` (the directed edge's direction must not
/// matter).
fn contact(app: &mut App, observer: &Band, subject: &Band, turn: u64) {
    let cfg = ConnectionsConfig::default();
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    ledger.record_contact(
        ConnectionKey::new(observer.id, subject.id),
        &Sighting::new(bevy::math::UVec2::ZERO, turn, ""),
        turn,
        &cfg,
    );
}

/// Let every tie bleed out: one `decay_all` per quiet turn, as the schedule does.
fn bleed_out(app: &mut App, from_turn: u64) -> u64 {
    let cfg = ConnectionsConfig::default();
    let until = from_turn + TURNS_TO_BLEED_OUT;
    for turn in from_turn..=until {
        app.world
            .resource_mut::<ConnectionLedger>()
            .decay_all(turn, &cfg);
    }
    until
}

fn assert_parked(app: &App, a: &Band, b: &Band) {
    let strength = app
        .world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(a.id, b.id))
        .expect("a drained edge parks, it is not deleted")
        .strength;
    assert_eq!(strength, NO_TIE, "liveness: the tie really bled out");
}

/// One Population pass, run at `tick`, with every band's larder restocked first so nobody is hungry.
fn population_turn(app: &mut App, tick: u64) {
    app.world.resource_mut::<SimulationTick>().0 = tick;
    let mut query = app.world.query::<&mut PopulationCohort>();
    for mut cohort in query.iter_mut(&mut app.world) {
        cohort.stores.reset_food("dry", scalar_from_f32(1.0e6));
    }
    app.world.run_system_once(simulate_population);
}

fn shipped_k(app: &App) -> u32 {
    u32::from(
        app.world
            .resource::<DemographicsConfigHandle>()
            .get()
            .lineage
            .people_per_line
            .get(),
    )
}

fn shipped_free_at(app: &App) -> u32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .free_breeding_at
        .get()
}

/// One band's breeding rows, read off the ENCODED envelope.
#[derive(Debug, PartialEq)]
struct Wire {
    found_lines: u32,
    population: u32,
    ceiling: u32,
    /// `(band id, lines, people, fading)`
    members: Vec<(u64, u32, u32, bool)>,
    /// `(faction, lines, fading)`
    peoples: Vec<(u32, u32, bool)>,
    people_per_line: u32,
    free_breeding_at: u32,
}

/// The band's rows, viewed as the band's own people — a rival's row is redacted for any other
/// viewer.
fn wire(app: &mut App, band: &Band) -> Wire {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let faction = app
        .world
        .get::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .faction;
    app.world.resource_mut::<ViewerFaction>().0 = faction;
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let snapshot = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");
    let campaign = snapshot.campaign().expect("the campaign section");
    let row = snapshot
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|row| row.entity() == band.entity.to_bits())
        .expect("the band is on the wire");
    Wire {
        found_lines: row.foundingLines(),
        population: row.breedingPopulation(),
        ceiling: row.breedingCeiling(),
        members: row
            .breedingMembers()
            .map(|rows| {
                rows.iter()
                    .map(|m| (m.bandId(), m.lines(), m.people(), m.fading()))
                    .collect()
            })
            .unwrap_or_default(),
        peoples: row
            .breedingPeoples()
            .map(|rows| {
                rows.iter()
                    .map(|p| (p.faction(), p.lines(), p.fading()))
                    .collect()
            })
            .unwrap_or_default(),
        people_per_line: campaign.lineagePeoplePerLine(),
        free_breeding_at: campaign.lineageFreeBreedingAt(),
    }
}

/// One band of each people, each seated with `each` lines of its own and no ties.
fn two_peoples(each: u16) -> (App, Band, Band) {
    let mut app = two_faction_world();
    let home = first_band(&mut app, HOME);
    let rival = first_band(&mut app, RIVAL);
    seat_lines(&mut app, &home, each);
    seat_lines(&mut app, &rival, each);
    forget_ties(&mut app);
    (app, home, rival)
}

fn people_of(app: &App, band: &Band) -> u32 {
    app.world
        .get::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .total()
        .to_u32()
}

fn latched(app: &App, faction: FactionId) -> bool {
    app.world
        .resource::<FreeBreedingPeoples>()
        .contains(faction)
}

/// **Two peoples in touch each count the other's lines — and neither band's own set changes.**
/// Each population publishes `(8 + 8) × K`, not the `152` that counting a line's holders
/// world-wide would leave; the foreign row carries lines and fading only, and the band's own row
/// shows just itself.
#[test]
fn peoples_in_touch_count_each_others_lines_and_nothing_is_copied() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    let k = shipped_k(&app);
    let own_before = (lines(&app, &home), lines(&app, &rival));

    population_turn(&mut app, READ_TICK - 1);
    for band in [&home, &rival] {
        assert_eq!(
            wire(&mut app, band).ceiling,
            u32::from(LINES_EACH) * k,
            "liveness: out of touch each is capped at its own L × K"
        );
    }

    contact(&mut app, &home, &rival, CONTACT_TURN);
    population_turn(&mut app, READ_TICK);

    assert_eq!(lines(&app, &home), own_before.0, "no line was copied");
    assert_eq!(lines(&app, &rival), own_before.1, "no line was copied");
    for (band, other) in [(&home, &rival), (&rival, &home)] {
        let row = wire(&mut app, band);
        assert_eq!(
            row.ceiling,
            u32::from(LINES_EACH) * 2 * k,
            "(own L + their L) × K, NOT 152: another people's copy must not divide a line's K"
        );
        assert_eq!(row.found_lines, u32::from(LINES_EACH), "own lines only");
        assert_eq!(
            row.members,
            vec![(
                band.id.0,
                u32::from(LINES_EACH),
                people_of(&app, band),
                false
            )],
            "the own row lists only this people's bands"
        );
        let other_faction = app
            .world
            .get::<PopulationCohort>(other.entity)
            .unwrap()
            .faction;
        assert_eq!(
            row.peoples,
            vec![(other_faction.0, u32::from(LINES_EACH), false)],
            "the other people: its lines and whether the tie is fading, no head-count"
        );
        assert_eq!(row.people_per_line, k);
        assert_eq!(row.free_breeding_at, shipped_free_at(&app));
    }
}

/// **The edge's direction does not matter** — a rival observing the home band counts the same.
#[test]
fn the_direction_of_the_edge_does_not_matter() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    contact(&mut app, &rival, &home, CONTACT_TURN);
    population_turn(&mut app, READ_TICK);
    let want = u32::from(LINES_EACH) * 2 * shipped_k(&app);
    assert_eq!(wire(&mut app, &home).ceiling, want);
    assert_eq!(wire(&mut app, &rival).ceiling, want);
}

/// **The tie bleeds out and the foreign lines leave the union.** The ceiling falls back to own
/// `L × K`; the band's own lines never moved.
#[test]
fn a_tie_that_bleeds_out_takes_the_foreign_lines_with_it() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    let k = shipped_k(&app);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    population_turn(&mut app, READ_TICK);
    assert_eq!(
        wire(&mut app, &home).ceiling,
        u32::from(LINES_EACH) * 2 * k,
        "liveness: in touch the union carried both sets"
    );

    let later = bleed_out(&mut app, READ_TICK);
    assert_parked(&app, &home, &rival);
    population_turn(&mut app, later);

    for band in [&home, &rival] {
        let row = wire(&mut app, band);
        assert_eq!(row.ceiling, u32::from(LINES_EACH) * k, "own L × K again");
        assert!(row.peoples.is_empty(), "nobody is in touch any more");
        assert_eq!(lines(&app, band).len(), usize::from(LINES_EACH));
    }
}

/// **A quiet turn on a live tie still counts, and says it is fading.** The tie bleeds over ~50 turns;
/// until it parks the lines count, and `fading` is the sim's reading that no contact refreshed it.
#[test]
fn a_live_tie_nobody_refreshed_still_counts_and_reads_fading() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    population_turn(&mut app, QUIET_TICK);
    let row = wire(&mut app, &home);
    assert_eq!(row.ceiling, u32::from(LINES_EACH) * 2 * shipped_k(&app));
    assert_eq!(row.peoples.len(), 1);
    assert!(row.peoples[0].2, "no contact this turn: fading");
}

/// **Not transitive.** A–B and B–C are in touch (A and C are two bands of the home people with
/// distinct lines, B the rival's): A's union has B's lines and not C's.
#[test]
fn what_a_band_borrows_is_not_passed_on() {
    let mut app = two_faction_world();
    let a = first_band(&mut app, HOME);
    let c = split_home_band(&mut app, &a);
    let b = first_band(&mut app, RIVAL);
    for band in [&a, &b, &c] {
        seat_lines(&mut app, band, LINES_EACH);
    }
    forget_ties(&mut app);
    contact(&mut app, &a, &b, CONTACT_TURN);
    contact(&mut app, &b, &c, CONTACT_TURN);
    population_turn(&mut app, READ_TICK);

    let k = shipped_k(&app);
    let each = u32::from(LINES_EACH);
    assert_eq!(wire(&mut app, &a).ceiling, 2 * each * k, "A: its own + B's");
    assert_eq!(wire(&mut app, &c).ceiling, 2 * each * k, "C: its own + B's");
    assert_eq!(
        wire(&mut app, &b).ceiling,
        3 * each * k,
        "B is in touch with both, and A and C are separate populations of one people"
    );
    let b_row = wire(&mut app, &b);
    assert_eq!(b_row.peoples, vec![(HOME.0, 2 * each, false)]);
}

/// **A line two peoples both hold is one line.** A breakaway people holds the same `LineId`s as the
/// people it left; in touch, the union dedups them, they are a different faction so they do not
/// divide it, and the foreign row credits nothing.
#[test]
fn a_line_both_peoples_hold_counts_once_and_divides_nobody() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    let home_lines = lines(&app, &home);
    seat_line_set(&mut app, &rival, home_lines);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    population_turn(&mut app, READ_TICK);
    let want = u32::from(LINES_EACH) * shipped_k(&app);
    for band in [&home, &rival] {
        let row = wire(&mut app, band);
        assert_eq!(row.ceiling, want, "eight shared lines, each at its whole K");
        assert_eq!(row.peoples.len(), 1);
        assert_eq!(
            row.peoples[0].1, 0,
            "no line came only from the other people"
        );
    }
}

/// Two bands of the home people that both hold the SAME single line (a one-line split copies it).
fn one_line_pair() -> (App, Band, Band) {
    let mut app = two_faction_world();
    let parent = first_band(&mut app, HOME);
    let child = split_home_band(&mut app, &parent);
    let shared = FoundingLines::founded(parent.id, 1);
    seat_line_set(&mut app, &parent, shared.clone());
    seat_line_set(&mut app, &child, shared);
    forget_ties(&mut app);
    (app, parent, child)
}

/// **Own bands out of sight are ONE population until the tie parks.** A live but bleeding tie joins
/// two bands of a people into one population with one ceiling, and the member row reads `fading`.
/// Once the tie parks they are two populations, and the shared-K split rule applies to their
/// one-line copies.
#[test]
fn own_bands_out_of_sight_are_one_population_until_the_tie_parks() {
    let (mut app, parent, child) = one_line_pair();
    let k = shipped_k(&app);

    // In sight: a fresh contact keeps the tie, and the rows read not fading.
    contact(&mut app, &parent, &child, CONTACT_TURN);
    contact(&mut app, &child, &parent, CONTACT_TURN);
    population_turn(&mut app, READ_TICK);
    let fresh = wire(&mut app, &parent);
    assert_eq!(fresh.ceiling, k, "one population holding one line whole");
    assert_eq!(fresh.members.len(), 2);
    assert!(fresh.members.iter().all(|member| !member.3), "in sight");

    // Out of sight but the tie still bleeding: still one population, now fading.
    population_turn(&mut app, QUIET_TICK);
    for band in [&parent, &child] {
        let row = wire(&mut app, band);
        assert_eq!(row.ceiling, k, "still one shared ceiling");
        assert_eq!(
            row.members.iter().map(|m| m.0).collect::<Vec<_>>(),
            vec![parent.id.0.min(child.id.0), parent.id.0.max(child.id.0)],
            "BandId order, self included"
        );
        assert!(row.members.iter().all(|member| member.3), "fading");
        assert_eq!(
            row.population,
            people_of(&app, &parent) + people_of(&app, &child)
        );
    }

    // The tie parks: two populations, one line held twice, K split between them.
    let later = bleed_out(&mut app, QUIET_TICK);
    assert_parked(&app, &parent, &child);
    population_turn(&mut app, later);
    for band in [&parent, &child] {
        let row = wire(&mut app, band);
        assert_eq!(row.ceiling, k / 2, "floor(K / 2) each");
        assert_eq!(row.members.len(), 1, "just itself");
        assert!(!row.members[0].3, "a lone band is never fading");
    }
}

/// **The head-count latch.** A population whose uncapped births carry it to `free_breeding_at` people frees its
/// whole people for good: it publishes ceiling `0`, grows past that head-count, and stays uncapped
/// after the tie that lifted its lines bleeds out. The other people, still small, is not latched.
#[test]
fn a_population_that_reaches_the_free_head_count_is_free_for_good() {
    let (mut app, home, rival) = two_peoples(LIFTING_LINES_EACH);
    let free_at = shipped_free_at(&app);
    let k = shipped_k(&app);
    assert!(
        u32::from(LIFTING_LINES_EACH) * 2 * k >= free_at,
        "fixture: union × K clears 500"
    );
    assert!(
        u32::from(LIFTING_LINES_EACH) * k < free_at,
        "fixture: alone it is capped"
    );
    seat_people(&mut app, &home, free_at as f32);
    contact(&mut app, &home, &rival, CONTACT_TURN);

    assert!(!latched(&app, HOME), "fixture: nothing has latched yet");
    population_turn(&mut app, READ_TICK);
    assert!(
        latched(&app, HOME),
        "an opening head-count of 500 latches the people"
    );
    assert!(!latched(&app, RIVAL), "the other people is untouched");
    let row = wire(&mut app, &home);
    assert_eq!(row.ceiling, NO_INBREEDING_CEILING);
    assert_ne!(
        wire(&mut app, &rival).ceiling,
        NO_INBREEDING_CEILING,
        "control: the small people still publishes a ceiling"
    );

    // The tie bleeds out: the latch does not.
    let later = bleed_out(&mut app, READ_TICK);
    assert_parked(&app, &home, &rival);
    let before = people_of(&app, &home);
    for turn in 0..FREE_GROWTH_TURNS as u64 {
        population_turn(&mut app, later + turn);
        assert_eq!(wire(&mut app, &home).ceiling, NO_INBREEDING_CEILING);
    }
    assert!(
        people_of(&app, &home) > before,
        "liveness: the freed people kept growing past {before}"
    );
    assert!(latched(&app, HOME));
}

/// **Births that fall short of `free_breeding_at` leave the people capped** at
/// `min(union × K, free_breeding_at)`, and when the tie bleeds out the ceiling falls to the band's
/// own lines.
#[test]
fn a_people_whose_births_fall_short_of_the_free_head_count_stays_capped() {
    let (mut app, home, rival) = two_peoples(LIFTING_LINES_EACH);
    let free_at = shipped_free_at(&app);
    let k = shipped_k(&app);
    seat_people(&mut app, &home, free_at as f32 - SHORT_OF_FREE_PEOPLE);
    contact(&mut app, &home, &rival, CONTACT_TURN);

    population_turn(&mut app, READ_TICK);
    assert!(!latched(&app, HOME), "opening + births stays under 500");
    assert_eq!(
        wire(&mut app, &home).ceiling,
        free_at,
        "union × K above 500 lets births run to 500 and no further"
    );

    let later = bleed_out(&mut app, READ_TICK);
    population_turn(&mut app, later);
    assert!(!latched(&app, HOME));
    assert_eq!(
        wire(&mut app, &home).ceiling,
        u32::from(LIFTING_LINES_EACH) * k,
        "out of touch again: own lines × K"
    );
}

/// Where the parked-tie fixture is seated: ten people under the free size, which one turn's
/// uncapped births (a few tens) cross.
const NEAR_FREE_GAP_PEOPLE: f32 = 10.0;
/// Quiet turns the parked-tie fixture runs.
const PARKED_TURNS: u64 = 3;

/// **Births only count where the population's own ceiling would let them carry it to the free
/// size.** A population at ~490 whose foreign tie has parked has a ceiling of its own lines × K,
/// below 500: it bears nobody and does not latch on births it cannot have. The control arm, the
/// same population still in touch, latches in the same turn.
#[test]
fn a_population_whose_foreign_tie_parked_does_not_latch_on_births_its_ceiling_forbids() {
    let free_at;
    let near;
    {
        let (mut app, home, rival) = two_peoples(LIFTING_LINES_EACH);
        free_at = shipped_free_at(&app);
        near = free_at as f32 - NEAR_FREE_GAP_PEOPLE;
        seat_people(&mut app, &home, near);
        contact(&mut app, &home, &rival, CONTACT_TURN);
        population_turn(&mut app, READ_TICK);
        assert!(
            latched(&app, HOME),
            "control: in touch, the ceiling lets the births carry it to the free size"
        );
    }

    let (mut app, home, rival) = two_peoples(LIFTING_LINES_EACH);
    let k = shipped_k(&app);
    seat_people(&mut app, &home, near);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    let parked = bleed_out(&mut app, CONTACT_TURN);
    assert_parked(&app, &home, &rival);
    let before = people_of(&app, &home);
    for turn in 0..PARKED_TURNS {
        population_turn(&mut app, parked + turn);
        assert!(
            !latched(&app, HOME),
            "turn {turn}: its ceiling forbids the births"
        );
    }
    let row = wire(&mut app, &home);
    assert_eq!(row.ceiling, u32::from(LIFTING_LINES_EACH) * k);
    assert!(
        row.ceiling < free_at,
        "fixture: the ceiling is below the free size"
    );
    assert!(
        people_of(&app, &home) <= before,
        "above its ceiling it bears nobody and only shrinks"
    );
}

/// **The latch survives save and load, and a checkpoint restore.**
#[test]
fn the_latch_survives_a_save_and_a_checkpoint() {
    let mut app = two_faction_world();
    app.world.resource_mut::<FreeBreedingPeoples>().latch(HOME);

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");
    assert!(
        loaded
            .world
            .resource::<FreeBreedingPeoples>()
            .contains(HOME),
        "a save/load dropped the latch"
    );
    assert!(
        !loaded
            .world
            .resource::<FreeBreedingPeoples>()
            .contains(RIVAL),
        "control: only the latched people"
    );

    let checkpoint = capture_sim_state(&app.world);
    app.world.insert_resource(FreeBreedingPeoples::default());
    restore_sim_state(&mut app.world, &checkpoint);
    assert!(
        latched(&app, HOME),
        "a checkpoint restore dropped the latch"
    );
}

/// **A people that breaks away from a latched people is latched.** Control: from an unlatched
/// people it is not.
#[test]
fn a_breakaway_inherits_the_latch() {
    for parent_latched in [true, false] {
        let mut app = core_sim_one_faction();
        if parent_latched {
            app.world.resource_mut::<FreeBreedingPeoples>().latch(HOME);
        }
        let home = first_band(&mut app, HOME);
        let child = split_home_band(&mut app, &home);
        // Half the map away, no tie, aggrieved: it breaks away on the next independence pass.
        let far = far_tile(&mut app, &home);
        {
            let mut cohort = app.world.get_mut::<PopulationCohort>(child.entity).unwrap();
            cohort.home = far;
            cohort.current_tile = far;
            cohort.grievance = scalar_from_f32(AGGRIEVED);
        }
        forget_ties(&mut app);
        app.world.run_system_once(advance_band_independence);

        let new_people = FactionId(HOME.0 + 1);
        assert!(
            app.world
                .resource::<core_sim::FactionRegistry>()
                .contains(new_people),
            "liveness: the group broke away"
        );
        assert_eq!(latched(&app, new_people), parent_latched);
    }
}

fn core_sim_one_faction() -> App {
    faction_support::one_faction_world()
}

fn far_tile(app: &mut App, home: &Band) -> Entity {
    use core_sim::{SimulationConfig, Tile, TileRegistry};
    let at_tile = app.world.get::<PopulationCohort>(home.entity).unwrap().home;
    let at = app.world.get::<Tile>(at_tile).unwrap().position;
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    app.world
        .resource::<TileRegistry>()
        .index((at.x + width / 2) % width, at.y)
        .expect("the far tile is on the map")
}
