//! **A breeding population above its ceiling sheds** (issue #690).
//!
//! `breeding_ceiling.rs` pins that a population above `|lines| × K` bears nobody and loses no one.
//! This file pins what happens to the excess: it leaks along the ways the unhappy trickle already
//! walks, into groups that have headroom — and when there is nowhere to go, it breaks off as a
//! people of its own, with a COPY of its share of the lines.
//!
//! Driven through the REAL systems on a generated world: `simulate_population` parks each band's
//! reading of its group's ceiling, then `shed_breeding_excess` and `break_off_breeding_excess` act
//! on it. Every claim is read off cohort fields, the registry and the event log.

mod faction_support;

use std::collections::BTreeSet;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, With};

use core_sim::{
    break_off_breeding_excess, scalar_from_f32, shed_breeding_excess, simulate_population, BandId,
    CommandEventKind, CommandEventLog, ConnectionKey, ConnectionLedger, ConnectionsConfig,
    DemographicsConfigHandle, DiscoveryProgressLedger, ExpeditionConfigHandle,
    FactionBorderPolicies, FactionId, FactionRegistry, FoundingLines, FreeBreedingPeoples,
    KnowledgeFragment, LaborAllocation, PopulationCohort, ResidentBand, Sighting, StartingLoadout,
    Tile, FULL_TIE,
};
use faction_support::{one_faction_world, two_faction_world, HOME, RIVAL};

/// A larder many lifetimes deep, so no fixture band is ever hungry and fertility is the only thing
/// the ceiling moves.
const DEEP_LARDER: f32 = 1.0e6;
/// The keeping class the deep larder is stocked in.
const LARDER_CLASS: &str = "dry";
/// The tick the seeded ties were last in contact on.
const SEEDED_ON_TURN: u64 = 0;
/// The shipped `L` — lines the over-ceiling band is founded with.
const HOME_LINES: u16 = 8;
/// Lines the neighbour is founded with: one, so the pair's union (`8 + 1`) lifts the home band's
/// ceiling by a single line's `K` and the neighbour's by eight.
const NEIGHBOUR_LINES: u16 = 1;
/// How many people the over-ceiling band is seated with, against a union ceiling of
/// `(HOME_LINES + NEIGHBOUR_LINES) × K`.
const OVER_PEOPLE: f32 = 250.0;
/// A neighbour with plenty of room under the same union ceiling.
const ROOMY_PEOPLE: f32 = 20.0;
/// A neighbour with only a little room: its group's headroom is below the leak's whole amount.
const CRAMPED_HEADROOM: f32 = 11.0;
/// An alone band seated far over its lines: the excess is a worker count the split can send.
const STRANDED_PEOPLE: f32 = 300.0;
/// Over the ceiling by too little to found a band with.
const BARELY_OVER_BY: f32 = 3.0;
/// The slack, in people, a whole-worker split may leave: the workers sent are rounded to a whole
/// worker, and their families follow by share.
const WORKER_ROUNDING_SLACK_PEOPLE: f32 = 1.0;
/// A discovery id the home band is seeded with, and the progress it holds on it.
const KNOWN_DISCOVERY: u32 = 7;
const KNOWN_PROGRESS: f32 = 0.8;
/// Fixed-point comparisons: how close two head-counts must be to be "the same amount".
const EPSILON_PEOPLE: f32 = 0.01;

#[derive(Clone, Copy)]
struct Band {
    entity: Entity,
    id: BandId,
}

fn first_band(app: &mut App, faction: FactionId) -> Band {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, id, _)| Band { entity, id: *id })
        .min_by_key(|band| band.id)
        .expect("worldgen spawns a band for every people")
}

fn cohort(app: &App, band: Band) -> PopulationCohort {
    app.world
        .get::<PopulationCohort>(band.entity)
        .expect("the band has a cohort")
        .clone()
}

fn total(app: &App, band: Band) -> f32 {
    cohort(app, band).total().to_f32()
}

fn shipped_k(app: &App) -> f32 {
    f32::from(
        app.world
            .resource::<DemographicsConfigHandle>()
            .get()
            .lineage
            .people_per_line
            .get(),
    )
}

fn shed_share(app: &App) -> f32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .shed_share
}

/// Seat `band` with `people` in the shipped opening age shape, `lines` founding lines of its own,
/// and no flow telemetry (a neutral `trend`).
fn seat(app: &mut App, band: Band, people: f32, lines: FoundingLines) {
    let dist = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .initial_distribution
        .clone();
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band.entity)
            .expect("the band has a cohort");
        cohort.children = scalar_from_f32(people * dist.children);
        cohort.working = scalar_from_f32(people * dist.working);
        cohort.elders = scalar_from_f32(people * dist.elders);
        cohort.founding_lines = lines;
        cohort.sync_size();
    }
    app.world
        .entity_mut(band.entity)
        .remove::<LaborAllocation>();
}

fn own_lines(band: Band, count: u16) -> FoundingLines {
    FoundingLines::founded(band.id, count)
}

fn forget_ties(app: &mut App) {
    *app.world.resource_mut::<ConnectionLedger>() = ConnectionLedger::default();
}

/// A full-strength live tie both ways, through the ledger's own contact path.
fn tie(app: &mut App, a: Band, b: Band) {
    let cfg = ConnectionsConfig::default();
    let contacts_to_full = (FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil() as u32;
    for (observer, subject) in [(a, b), (b, a)] {
        let key = ConnectionKey::new(observer.id, subject.id);
        let at = {
            let tile = cohort(app, subject).current_tile;
            app.world.get::<Tile>(tile).expect("a tile").position
        };
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for _ in 0..contacts_to_full {
            ledger.record_contact(
                key,
                &Sighting::new(at, SEEDED_ON_TURN, ""),
                SEEDED_ON_TURN,
                &cfg,
            );
        }
    }
}

/// Stand `band` on the tile `anchor` stands on.
fn stand_beside(app: &mut App, band: Band, anchor: Band) {
    let home = cohort(app, anchor).home;
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band.entity)
        .expect("the band has a cohort");
    cohort.home = home;
    cohort.current_tile = home;
}

/// One demographic turn: every band restocked, then the pre-pass parks this turn's ceiling readings.
fn demographic_turn(app: &mut App) {
    let mut query = app.world.query::<&mut PopulationCohort>();
    for mut cohort in query.iter_mut(&mut app.world) {
        cohort
            .stores
            .reset_food(LARDER_CLASS, scalar_from_f32(DEEP_LARDER));
    }
    app.world.run_system_once(simulate_population);
}

fn shed(app: &mut App) {
    app.world.run_system_once(shed_breeding_excess);
    app.world.run_system_once(break_off_breeding_excess);
}

fn band_count(app: &mut App) -> usize {
    app.world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .count()
}

fn events(app: &App, kind: CommandEventKind) -> Vec<(FactionId, String, String)> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == kind)
        .map(|entry| {
            (
                entry.faction,
                entry.label.clone(),
                entry.detail.clone().unwrap_or_default(),
            )
        })
        .collect()
}

/// A home band over its ceiling beside a rival band it holds a live tie with, standing within reach.
/// The pair's union is `(8 + 1)` lines, so both groups ceil at `9 × K`.
///
/// `neighbour_people` is struck from the union ceiling so a fixture can say how much room it leaves.
fn over_ceiling_beside_a_neighbour(neighbour_people: impl FnOnce(f32) -> f32) -> (App, Band, Band) {
    let mut app = two_faction_world();
    let home = first_band(&mut app, HOME);
    let rival = first_band(&mut app, RIVAL);
    let people = neighbour_people(union_ceiling(&app));
    seat(&mut app, home, OVER_PEOPLE, own_lines(home, HOME_LINES));
    seat(&mut app, rival, people, own_lines(rival, NEIGHBOUR_LINES));
    stand_beside(&mut app, rival, home);
    forget_ties(&mut app);
    tie(&mut app, home, rival);
    demographic_turn(&mut app);
    (app, home, rival)
}

fn union_ceiling(app: &App) -> f32 {
    f32::from(HOME_LINES + NEIGHBOUR_LINES) * shipped_k(app)
}

/// Raise the founding floor so no break-off can ever be made: these cases are about the leak, and
/// a stranded group would otherwise (correctly) found a people instead.
fn forbid_break_offs(app: &mut App) {
    let mut config = (*app.world.resource::<ExpeditionConfigHandle>().get()).clone();
    config.settle.min_founding_workers = u32::MAX;
    app.world
        .resource_mut::<ExpeditionConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// **1. The leak.** A group over its ceiling beside a reachable foreign band (live tie, open
/// borders) with room sheds `shed_share × E` to it. No line moves, and the destination's group is
/// not pushed over its own ceiling.
#[test]
fn an_over_ceiling_group_leaks_a_share_of_its_excess_to_a_neighbour_with_room() {
    let (mut app, home, rival) = over_ceiling_beside_a_neighbour(|_| ROOMY_PEOPLE);
    let ceiling = union_ceiling(&app);
    let before = (total(&app, home), total(&app, rival));
    let excess = before.0 - ceiling;
    assert!(excess > 0.0, "liveness: the home band is over its ceiling");
    let lines = (
        cohort(&app, home).founding_lines,
        cohort(&app, rival).founding_lines,
    );
    app.world
        .get_mut::<PopulationCohort>(home.entity)
        .expect("a cohort")
        .knowledge = vec![KnowledgeFragment::new(
        KNOWN_DISCOVERY,
        scalar_from_f32(KNOWN_PROGRESS),
    )];
    let known_before = app
        .world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(RIVAL, KNOWN_DISCOVERY);

    shed(&mut app);

    // Knowledge crosses with the people, as it does for migration's leavers: the destination
    // people is credited and the destination band merges it, in proportion to the share that left.
    assert!(
        app.world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(RIVAL, KNOWN_DISCOVERY)
            > known_before,
        "the neighbour's people learned from the arrivals"
    );
    assert!(
        cohort(&app, rival)
            .knowledge
            .iter()
            .any(|fragment| fragment.discovery_id == KNOWN_DISCOVERY),
        "the destination band merged the fragment"
    );

    let expected = shed_share(&app) * excess;
    let (home_after, rival_after) = (total(&app, home), total(&app, rival));
    assert!(
        (before.0 - home_after - expected).abs() < EPSILON_PEOPLE,
        "the home band shed {} against an expected {expected}",
        before.0 - home_after
    );
    assert!(
        (rival_after - before.1 - expected).abs() < EPSILON_PEOPLE,
        "the neighbour took what left: {}",
        rival_after - before.1
    );
    assert_eq!(
        (
            cohort(&app, home).founding_lines,
            cohort(&app, rival).founding_lines
        ),
        lines,
        "shed people carry no founding lines"
    );
    assert!(
        rival_after <= ceiling,
        "the destination's group stays under its ceiling"
    );

    // Booked through migration's own readouts and feed.
    assert_eq!(cohort(&app, home).last_emigrated, expected.round() as u32);
    assert_eq!(cohort(&app, rival).last_immigrated, expected.round() as u32);
    let told = events(&app, CommandEventKind::Migrated);
    assert!(
        told.iter().any(|(faction, label, detail)| *faction == HOME
            && label.contains("left")
            && detail.contains(&format!("to={}", RIVAL.0))),
        "the home people hears who its leavers joined: {told:?}"
    );
    assert!(
        told.iter().any(|(faction, _, detail)| *faction == RIVAL
            && detail.contains(&format!("from={}", HOME.0))),
        "the neighbour hears where its arrivals came from: {told:?}"
    );
}

/// **1b. The headroom cap.** A destination group never takes more than its remaining room, and what
/// cannot be placed stays (no break-off: a destination exists).
#[test]
fn a_destination_takes_no_more_than_its_headroom_and_the_rest_stays() {
    // Seat the neighbour `CRAMPED_HEADROOM` under the union ceiling.
    let (mut app, home, rival) =
        over_ceiling_beside_a_neighbour(|ceiling| ceiling - CRAMPED_HEADROOM);
    let ceiling = union_ceiling(&app);
    let before = (total(&app, home), total(&app, rival));
    let room = ceiling - before.1;
    let wanted = shed_share(&app) * (before.0 - ceiling);
    assert!(
        room > 0.0 && room < wanted,
        "liveness: room {room} against a leak of {wanted}"
    );
    let bands = band_count(&mut app);

    shed(&mut app);

    assert!(
        (total(&app, rival) - ceiling).abs() < EPSILON_PEOPLE,
        "the neighbour fills to its ceiling and no further: {}",
        total(&app, rival)
    );
    assert!(
        (before.0 - total(&app, home) - room).abs() < EPSILON_PEOPLE,
        "the home band lost exactly what fit"
    );
    assert_eq!(
        band_count(&mut app),
        bands,
        "a destination existed, so nothing broke off"
    );
}

/// **2. Under its ceiling, nothing moves.**
#[test]
fn a_group_under_its_ceiling_sheds_nobody() {
    let mut app = two_faction_world();
    let home = first_band(&mut app, HOME);
    let rival = first_band(&mut app, RIVAL);
    let under = union_ceiling(&app) - 2.0 * CRAMPED_HEADROOM;
    seat(&mut app, home, under, own_lines(home, HOME_LINES));
    seat(
        &mut app,
        rival,
        ROOMY_PEOPLE,
        own_lines(rival, NEIGHBOUR_LINES),
    );
    stand_beside(&mut app, rival, home);
    forget_ties(&mut app);
    tie(&mut app, home, rival);
    demographic_turn(&mut app);
    let before = (total(&app, home), total(&app, rival));
    let bands = band_count(&mut app);

    shed(&mut app);

    assert_eq!((total(&app, home), total(&app, rival)), before);
    assert_eq!(band_count(&mut app), bands);
    assert!(events(&app, CommandEventKind::Migrated).is_empty());
}

/// **3. A people that breeds freely sheds nobody**, however far over `lines × K` it sits — the
/// control is the same fixture unlatched, in `an_over_ceiling_group_leaks…`.
#[test]
fn a_free_breeding_people_over_its_lines_sheds_nobody() {
    let mut app = two_faction_world();
    let home = first_band(&mut app, HOME);
    let rival = first_band(&mut app, RIVAL);
    seat(&mut app, home, OVER_PEOPLE, own_lines(home, HOME_LINES));
    seat(
        &mut app,
        rival,
        ROOMY_PEOPLE,
        own_lines(rival, NEIGHBOUR_LINES),
    );
    stand_beside(&mut app, rival, home);
    forget_ties(&mut app);
    tie(&mut app, home, rival);
    app.world.resource_mut::<FreeBreedingPeoples>().latch(HOME);
    demographic_turn(&mut app);
    assert!(
        total(&app, home) > union_ceiling(&app),
        "liveness: the band really is over lines × K"
    );
    let before = (total(&app, home), total(&app, rival));
    let bands = band_count(&mut app);

    shed(&mut app);

    assert_eq!((total(&app, home), total(&app, rival)), before);
    assert_eq!(band_count(&mut app), bands);
}

/// **4. A band of the same breeding group is never a destination**, however much room the
/// group's ceiling seems to leave it — the group's head-count is what is over.
#[test]
fn a_band_in_the_same_breeding_group_is_never_a_destination() {
    let mut app = one_faction_world();
    let parent = first_band(&mut app, HOME);
    let split = core_sim::split_band_from_parent(
        &mut app.world,
        parent.entity,
        3,
        &core_sim::SettleConfig {
            min_founding_workers: 1,
            parent_min_workers: 0,
        },
    )
    .expect("the opening band can spare a splinter");
    let sibling = Band {
        entity: app
            .world
            .query::<(Entity, &BandId)>()
            .iter(&app.world)
            .find(|(_, id)| **id == split.band)
            .map(|(entity, _)| entity)
            .expect("the splinter exists"),
        id: split.band,
    };
    // One line set between them: the group's union is `HOME_LINES` whole, ceiling `8 × K`.
    let lines = own_lines(parent, HOME_LINES);
    let ceiling = f32::from(HOME_LINES) * shipped_k(&app);
    seat(&mut app, parent, ceiling, lines.clone());
    seat(&mut app, sibling, BARELY_OVER_BY * 2.0, lines);
    // The split's own full tie keeps them one group.
    demographic_turn(&mut app);
    forbid_break_offs(&mut app);
    let before = (total(&app, parent), total(&app, sibling));
    assert!(before.0 + before.1 > ceiling, "liveness: the group is over");

    shed(&mut app);

    assert_eq!(
        (total(&app, parent), total(&app, sibling)),
        before,
        "nobody moved between two bands of one group"
    );
}

/// **5a. Closed borders.** A foreign band whose people keep their borders closed is not a
/// destination — and the same fixture with them open IS (1).
#[test]
fn a_foreign_band_behind_closed_borders_is_not_a_destination() {
    let (mut app, home, rival) = over_ceiling_beside_a_neighbour(|_| ROOMY_PEOPLE);
    forbid_break_offs(&mut app);
    app.world
        .resource_mut::<FactionBorderPolicies>()
        .set_open(RIVAL, false);
    let before = (total(&app, home), total(&app, rival));

    shed(&mut app);

    assert_eq!((total(&app, home), total(&app, rival)), before);
    assert!(events(&app, CommandEventKind::Migrated).is_empty());
}

/// **5b. Out of walk reach.** A neighbour with a live tie and room, but half the map away.
#[test]
fn a_foreign_band_out_of_walk_reach_is_not_a_destination() {
    let (mut app, home, rival) = over_ceiling_beside_a_neighbour(|_| ROOMY_PEOPLE);
    forbid_break_offs(&mut app);
    // Back to its own distant start: the worldgen-placed rival is far past `base_reach`.
    let far = {
        let mut other = two_faction_world();
        let rival_far = first_band(&mut other, RIVAL);
        let tile = cohort(&other, rival_far).home;
        other.world.get::<Tile>(tile).expect("a tile").position
    };
    let tile = app
        .world
        .resource::<core_sim::TileRegistry>()
        .index(far.x, far.y)
        .expect("the rival's start is on the map");
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(rival.entity)
            .expect("a cohort");
        cohort.home = tile;
        cohort.current_tile = tile;
    }
    let before = (total(&app, home), total(&app, rival));

    shed(&mut app);

    assert_eq!((total(&app, home), total(&app, rival)), before);
}

/// **6. Nowhere to go: the excess breaks off as a people of its own.** Exactly one new band in a
/// new faction; the child holds a COPY of its share of the lines and the parent keeps every one of
/// its own; the parent lands at its ceiling and the turn after breaks nothing more off.
#[test]
fn a_stranded_group_breaks_its_excess_off_as_a_new_people_with_copied_lines() {
    let mut app = one_faction_world();
    let parent = first_band(&mut app, HOME);
    seat(
        &mut app,
        parent,
        STRANDED_PEOPLE,
        own_lines(parent, HOME_LINES),
    );
    demographic_turn(&mut app);
    let ceiling = f32::from(HOME_LINES) * shipped_k(&app);
    let parent_before = total(&app, parent);
    let lines_before = cohort(&app, parent).founding_lines;
    let peoples = app.world.resource::<FactionRegistry>().factions().len();
    let bands = band_count(&mut app);
    assert!(
        parent_before > ceiling,
        "liveness: stranded over its ceiling"
    );

    shed(&mut app);

    assert_eq!(
        app.world.resource::<FactionRegistry>().factions().len(),
        peoples + 1,
        "one new people"
    );
    assert_eq!(band_count(&mut app), bands + 1, "exactly one new band");
    let (child_entity, child_id) = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .find(|(_, id)| **id != parent.id)
        .map(|(entity, id)| (entity, *id))
        .expect("the break-off band exists");
    let child = Band {
        entity: child_entity,
        id: child_id,
    };
    let new_people = cohort(&app, child).faction;
    assert_ne!(new_people, HOME, "it formed its own faction");

    // Lines: copied, never divided.
    let child_lines: BTreeSet<_> = cohort(&app, child).founding_lines.iter().copied().collect();
    let parent_lines: BTreeSet<_> = lines_before.iter().copied().collect();
    assert_eq!(
        cohort(&app, parent).founding_lines,
        lines_before,
        "the parent keeps every one of its own lines"
    );
    let child_people = total(&app, child);
    let expected_copies =
        ((f32::from(HOME_LINES) * child_people / parent_before).ceil() as usize).max(1);
    assert_eq!(child_lines.len(), expected_copies);
    assert!(
        child_lines.is_subset(&parent_lines),
        "the child holds copies of the parent's lines"
    );

    // Sizes: about E people left, so the parent sits at its ceiling.
    assert!(
        (child_people - (parent_before - ceiling)).abs() <= WORKER_ROUNDING_SLACK_PEOPLE,
        "about the excess left: {child_people} against {}",
        parent_before - ceiling
    );
    assert!(
        total(&app, parent) <= ceiling + WORKER_ROUNDING_SLACK_PEOPLE,
        "the parent's group is at its ceiling: {}",
        total(&app, parent)
    );

    // Told to both peoples with the breeding cause; no outfitting window for a split nobody made.
    let told = events(&app, CommandEventKind::BandBrokeAway);
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told
        .iter()
        .all(|(_, _, detail)| detail.contains("cause=breeding")));
    assert!(!app.world.resource::<StartingLoadout>().is_open(child.id));

    // No cascade: the next turn breaks nothing more off.
    let parent_after_break = total(&app, parent);
    demographic_turn(&mut app);
    shed(&mut app);
    assert_eq!(band_count(&mut app), bands + 1, "no second break-off");
    assert_eq!(
        app.world.resource::<FactionRegistry>().factions().len(),
        peoples + 1
    );
    assert!(
        total(&app, parent) <= parent_after_break + EPSILON_PEOPLE,
        "a parent at its ceiling bears nobody"
    );
    assert_eq!(cohort(&app, parent).founding_lines, lines_before);
}

/// **7. An excess too small to found a band with: nothing happens.** The split refuses (below
/// `min_founding_workers`) and everyone stays.
#[test]
fn an_excess_too_small_to_found_a_band_stays_put() {
    let mut app = one_faction_world();
    let parent = first_band(&mut app, HOME);
    let ceiling = f32::from(HOME_LINES) * shipped_k(&app);
    seat(
        &mut app,
        parent,
        ceiling + BARELY_OVER_BY,
        own_lines(parent, HOME_LINES),
    );
    demographic_turn(&mut app);
    let before = total(&app, parent);
    let (peoples, bands) = (
        app.world.resource::<FactionRegistry>().factions().len(),
        band_count(&mut app),
    );
    assert!(before > ceiling, "liveness: over the ceiling");

    shed(&mut app);

    assert_eq!(total(&app, parent), before);
    assert_eq!(band_count(&mut app), bands);
    assert_eq!(
        app.world.resource::<FactionRegistry>().factions().len(),
        peoples
    );
    assert!(events(&app, CommandEventKind::BandBrokeAway).is_empty());
}

/// **A band that changed people this turn is skipped, whichever side of the shed it is on.** Its
/// reading was struck for the people it left, so it is neither a source nor a destination until the
/// next turn's pre-pass regroups it. The control is the unflipped fixture of the first test.
#[test]
fn a_band_that_changed_people_after_the_pre_pass_is_not_grouped_by_its_stale_reading() {
    let (mut app, home, rival) = over_ceiling_beside_a_neighbour(|_| ROOMY_PEOPLE);
    forbid_break_offs(&mut app);
    app.world
        .get_mut::<PopulationCohort>(home.entity)
        .expect("a cohort")
        .faction = RIVAL;
    let before = (total(&app, home), total(&app, rival));

    shed(&mut app);

    assert_eq!((total(&app, home), total(&app, rival)), before);
    assert!(events(&app, CommandEventKind::Migrated).is_empty());
}

/// Turns long enough for a full tie to drain to zero at the shipped bleed (the same figure
/// `lineage_contact.rs` uses).
const TURNS_TO_BLEED_OUT: u64 = 100;

/// Let every tie bleed out: one `decay_all` per quiet turn, as the schedule does. Only the ledger's
/// own clock runs — nothing here records contact, so the two bands stay out of touch.
fn bleed_ties_out(app: &mut App) {
    let cfg = ConnectionsConfig::default();
    for turn in 0..=TURNS_TO_BLEED_OUT {
        app.world
            .resource_mut::<ConnectionLedger>()
            .decay_all(turn, &cfg);
    }
}

/// The published reading of `band`'s breeding group: `(ceiling, head-count)`.
fn reading(app: &App, band: Band) -> (u32, u32) {
    let reading = cohort(app, band).last_breeding;
    (reading.ceiling, reading.headcount)
}

/// The home band seated stranded over its lines, with this turn's ceiling readings parked.
fn stranded_home_band() -> (App, Band) {
    let mut app = one_faction_world();
    let parent = first_band(&mut app, HOME);
    seat(
        &mut app,
        parent,
        STRANDED_PEOPLE,
        own_lines(parent, HOME_LINES),
    );
    demographic_turn(&mut app);
    (app, parent)
}

/// **The copy-vs-divide decision matters once the two LOSE TOUCH.** While their tie is live each
/// counts the other's lines, so both read the full set either way. After it bleeds out the parent
/// counts only its own: with the lines COPIED it still holds all of them and sits at its ceiling;
/// DIVIDED it would be left over it. The control arm is the same split with the lines partitioned.
#[test]
fn after_the_break_off_drifts_out_of_touch_the_parent_keeps_its_whole_ceiling() {
    let k = {
        let probe = one_faction_world();
        shipped_k(&probe)
    };
    let full_ceiling = f32::from(HOME_LINES) * k;

    // ---- The break-off, as the sim makes it ----
    let (mut app, parent) = stranded_home_band();
    shed(&mut app);
    let child = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .find(|(_, id)| **id != parent.id)
        .map(|(entity, id)| Band { entity, id: *id })
        .expect("the break-off band exists");
    let child_lines = cohort(&app, child).founding_lines.len() as f32;
    let child_workers = core_sim::available_workers(cohort(&app, child).working);
    assert!(
        child_lines < f32::from(HOME_LINES),
        "liveness: it copied a share, not the lot"
    );

    bleed_ties_out(&mut app);
    demographic_turn(&mut app);

    let (parent_ceiling, parent_heads) = reading(&app, parent);
    assert_eq!(
        parent_ceiling as f32, full_ceiling,
        "out of touch, the parent still counts all of its own lines"
    );
    assert!(
        parent_heads as f32 <= full_ceiling + WORKER_ROUNDING_SLACK_PEOPLE,
        "and sits at its ceiling: {parent_heads}"
    );
    assert_eq!(
        reading(&app, child).0 as f32,
        child_lines * k,
        "the child's ceiling fell to its copied lines × K"
    );
    let parent_before = total(&app, parent);
    let lines_before = cohort(&app, parent).founding_lines;

    shed(&mut app);

    assert_eq!(
        total(&app, parent),
        parent_before,
        "nothing sheds from the parent"
    );
    assert_eq!(cohort(&app, parent).founding_lines, lines_before);
    let home_bands = app
        .world
        .query_filtered::<&PopulationCohort, With<ResidentBand>>()
        .iter(&app.world)
        .filter(|cohort| cohort.faction == HOME)
        .count();
    assert_eq!(
        home_bands, 1,
        "no second break-off from the parent's people"
    );

    // ---- Control: the same split, lines DIVIDED ----
    let (mut twin, twin_parent) = stranded_home_band();
    let settle = twin
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .clone();
    core_sim::split_band_from_parent_with(
        &mut twin.world,
        twin_parent.entity,
        child_workers,
        &settle,
        core_sim::LineDivision::Partition,
    )
    .expect("the same split is allowed");
    bleed_ties_out(&mut twin);
    demographic_turn(&mut twin);
    let (divided_ceiling, divided_heads) = reading(&twin, twin_parent);
    assert!(
        divided_ceiling < divided_heads,
        "divided, the parent is left over its ceiling ({divided_heads} people against \
         {divided_ceiling}) — the cascade copying prevents"
    );
}
