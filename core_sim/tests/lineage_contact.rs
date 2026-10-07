//! **Contact between two peoples merges their founding lines** (issue #689,
//! `docs/plan_civilization_steps.md` §"The mechanism: an isolated people cannot grow past its
//! lines").
//!
//! Driven through the REAL `merge_founding_lines_on_contact` and the REAL `simulate_population`
//! on a generated two-people world. The contact itself is the ledger's own stamp
//! (`last_contact_turn == SimulationTick`), written through `ConnectionLedger::record_contact` —
//! the same call `advance_connections` makes — so the tests drive exactly the signal the system
//! reads without having to walk a rival band across the map.
//!
//! Population runs AFTER the merge here, as it does the turn after contact in the live schedule:
//! `TurnStage::Population` precedes `TurnStage::Visibility`, so a merge lifts the ceiling on the
//! NEXT turn's population pass.

mod faction_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, With};

use core_sim::{
    breeding_ceiling, merge_founding_lines_on_contact, recapture_snapshot_in_place,
    simulate_population, split_band_from_parent, BandId, ConnectionKey, ConnectionLedger,
    ConnectionsConfig, DemographicsConfigHandle, FactionId, FoundingLines, LaborAllocation,
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
/// A later turn, well past the contact, for the "tie has bled out" arms.
const LATER_TURN: u64 = CONTACT_TURN + 1;
/// Turns long enough for a full tie to drain to zero at the shipped bleed, still inside
/// `forget_turns`.
const TURNS_TO_BLEED_OUT: u64 = 100;
/// Lines each of two bands holds when their union just clears `free_breeding_at`.
const LIFTING_LINES_EACH: u16 = 14;

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

fn seat_lines(app: &mut App, band: &Band, count: u16) {
    app.world
        .get_mut::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .founding_lines = FoundingLines::founded(band.id, count);
    // No flow telemetry reads as a neutral trend; irrelevant to a ceiling, and it keeps the
    // fixture independent of the labor pass.
    app.world
        .entity_mut(band.entity)
        .remove::<LaborAllocation>();
}

fn lines(app: &App, band: &Band) -> FoundingLines {
    app.world
        .get::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .founding_lines
        .clone()
}

fn line_count(app: &App, band: &Band) -> usize {
    lines(app, band).len()
}

fn set_turn(app: &mut App, turn: u64) {
    app.world.resource_mut::<SimulationTick>().0 = turn;
}

/// Start every fixture from a ledger with no ties — a split seeds a full tie both ways.
fn forget_ties(app: &mut App) {
    *app.world.resource_mut::<ConnectionLedger>() = ConnectionLedger::default();
}

/// Record contact between `a` and `b` on `turn`, in one direction (the directed edge's direction
/// must not matter).
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

fn merge(app: &mut App) {
    app.world.run_system_once(merge_founding_lines_on_contact);
}

fn population_turn(app: &mut App) {
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

/// The band's `(foundingLines, breedingCeiling)` read off the ENCODED envelope, viewed as the
/// band's own people — a rival's row is redacted for any other viewer.
fn published(app: &mut App, band: &Band) -> (u16, u32) {
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
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|row| row.entity() == band.entity.to_bits())
        .expect("the band is on the wire");
    (row.foundingLines() as u16, row.breedingCeiling())
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

/// **Two peoples in contact for one turn each end up holding both sets**, and each publishes a
/// breeding ceiling of `16 × K` — not the `152` that counting a line's holders world-wide would
/// leave: with both peoples holding all sixteen lines, a world-wide tally gives every line two
/// holders and each ceiling stays at `16 × 19 / 2 = 152`, so contact would achieve nothing.
#[test]
fn contact_between_two_peoples_merges_the_lines_and_lifts_both_ceilings() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    let k = shipped_k(&app);
    let union = usize::from(LINES_EACH) * 2;
    let before = (published(&mut app, &home), published(&mut app, &rival));
    assert_eq!(before.0 .0, LINES_EACH);

    set_turn(&mut app, CONTACT_TURN);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    merge(&mut app);
    population_turn(&mut app);

    assert_eq!(line_count(&app, &home), union);
    assert_eq!(line_count(&app, &rival), union);
    assert_eq!(
        lines(&app, &home),
        lines(&app, &rival),
        "both hold the union"
    );
    let want = breeding_ceiling(union, core_sim_k(&app));
    assert_eq!(want, union as u32 * k);
    for band in [&home, &rival] {
        let (count, ceiling) = published(&mut app, band);
        assert_eq!(usize::from(count), union, "the count on the wire");
        assert_eq!(
            ceiling, want,
            "16 x K, NOT 152: another people holding a copy must not divide a line's K"
        );
        assert_ne!(ceiling, want / 2, "the world-wide holder-count failure");
    }
}

fn core_sim_k(app: &App) -> std::num::NonZeroU16 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .people_per_line
}

/// **The edge's direction does not matter** — a rival observing the home band merges the same.
#[test]
fn the_direction_of_the_edge_does_not_matter() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    set_turn(&mut app, CONTACT_TURN);
    contact(&mut app, &rival, &home, CONTACT_TURN);
    merge(&mut app);
    assert_eq!(line_count(&app, &home), usize::from(LINES_EACH) * 2);
    assert_eq!(line_count(&app, &rival), usize::from(LINES_EACH) * 2);
}

/// **Bands of one people already share lines**: contact among them gains nothing.
#[test]
fn contact_between_bands_of_one_people_gains_no_lines() {
    let mut app = two_faction_world();
    let parent = first_band(&mut app, HOME);
    seat_lines(&mut app, &parent, LINES_EACH);
    let child = split_home_band(&mut app, &parent);
    forget_ties(&mut app);
    let parent_before = lines(&app, &parent);
    let child_before = lines(&app, &child);
    assert_ne!(
        parent_before, child_before,
        "liveness: the split partitioned"
    );

    set_turn(&mut app, CONTACT_TURN);
    contact(&mut app, &parent, &child, CONTACT_TURN);
    contact(&mut app, &child, &parent, CONTACT_TURN);
    merge(&mut app);

    assert_eq!(lines(&app, &parent), parent_before);
    assert_eq!(lines(&app, &child), child_before);
}

/// **Repeated contact is idempotent**: the count stays at the union.
#[test]
fn repeated_contact_turns_do_not_grow_the_union() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    let union = usize::from(LINES_EACH) * 2;
    for turn in CONTACT_TURN..CONTACT_TURN + 4 {
        set_turn(&mut app, turn);
        contact(&mut app, &home, &rival, turn);
        merge(&mut app);
        assert_eq!(line_count(&app, &home), union, "turn {turn}");
        assert_eq!(line_count(&app, &rival), union, "turn {turn}");
    }
}

/// **Lines never decay**: after the tie bleeds out and many turns pass, the gained lines remain,
/// and a turn with no contact merges nothing.
#[test]
fn gained_lines_outlive_the_tie() {
    let (mut app, home, rival) = two_peoples(LINES_EACH);
    let union = usize::from(LINES_EACH) * 2;
    set_turn(&mut app, CONTACT_TURN);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    merge(&mut app);

    let bled_out = CONTACT_TURN + TURNS_TO_BLEED_OUT;
    set_turn(&mut app, bled_out);
    let cfg = ConnectionsConfig::default();
    // The bleed is one step per turn without contact, so drain it turn by turn as the schedule does.
    for turn in LATER_TURN..=bled_out {
        app.world
            .resource_mut::<ConnectionLedger>()
            .decay_all(turn, &cfg);
    }
    let strength = app
        .world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(home.id, rival.id))
        .expect("a drained edge parks, it is not deleted")
        .strength;
    assert_eq!(strength, NO_TIE, "liveness: the tie really bled out");
    merge(&mut app);

    assert_eq!(line_count(&app, &home), union);
    assert_eq!(line_count(&app, &rival), union);
    // A stale edge (contact on an earlier turn) merges nothing new either.
    set_turn(&mut app, bled_out + LATER_TURN);
    merge(&mut app);
    assert_eq!(line_count(&app, &home), union);
}

/// **Within one turn the merge is non-transitive and order-independent.** A (home) meets B
/// (rival), and B meets C (home, a second band): each gains B's PRE-merge lines, B gains both
/// sets, and C does NOT get A's lines this turn (it can the next).
#[test]
fn a_chain_of_contacts_in_one_turn_does_not_relay_lines() {
    let mut app = two_faction_world();
    let a = first_band(&mut app, HOME);
    let c = split_home_band(&mut app, &a);
    let b = first_band(&mut app, RIVAL);
    for band in [&a, &b, &c] {
        seat_lines(&mut app, band, LINES_EACH);
    }
    forget_ties(&mut app);
    let (a0, b0, c0) = (lines(&app, &a), lines(&app, &b), lines(&app, &c));

    set_turn(&mut app, CONTACT_TURN);
    contact(&mut app, &a, &b, CONTACT_TURN);
    contact(&mut app, &b, &c, CONTACT_TURN);
    merge(&mut app);

    let expect = |mut base: FoundingLines, others: &[&FoundingLines]| {
        for other in others {
            base.absorb(other);
        }
        base
    };
    assert_eq!(lines(&app, &a), expect(a0.clone(), &[&b0]));
    assert_eq!(lines(&app, &c), expect(c0.clone(), &[&b0]));
    assert_eq!(lines(&app, &b), expect(b0.clone(), &[&a0, &c0]));
    let mut a_lines_in_c = a0
        .iter()
        .filter(|line| lines(&app, &c).iter().any(|l| l == *line));
    assert!(
        a_lines_in_c.next().is_none(),
        "C must not hold A's lines the turn A and C both met B"
    );

    // Next turn the chain's far end does pick them up.
    set_turn(&mut app, LATER_TURN);
    contact(&mut app, &b, &c, LATER_TURN);
    merge(&mut app);
    assert!(a0
        .iter()
        .all(|line| lines(&app, &c).iter().any(|l| l == line)));
}

/// **A contact that carries a population past `free_breeding_at` lifts its ceiling altogether**
/// and publishes `NO_INBREEDING_CEILING`.
#[test]
fn contact_that_reaches_the_free_breeding_size_publishes_a_lifted_ceiling() {
    let (mut app, home, rival) = two_peoples(LIFTING_LINES_EACH);
    let k = shipped_k(&app);
    let free_at = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .free_breeding_at
        .get();
    let union = u32::from(LIFTING_LINES_EACH) * 2;
    assert!(union * k >= free_at, "fixture: the union clears the lift");
    assert!(
        u32::from(LIFTING_LINES_EACH) * k < free_at,
        "fixture: alone it is capped"
    );

    population_turn(&mut app);
    assert_eq!(
        published(&mut app, &home).1,
        u32::from(LIFTING_LINES_EACH) * k
    );

    set_turn(&mut app, CONTACT_TURN);
    contact(&mut app, &home, &rival, CONTACT_TURN);
    merge(&mut app);
    population_turn(&mut app);
    for band in [&home, &rival] {
        assert_eq!(published(&mut app, band).1, NO_INBREEDING_CEILING);
    }
}
