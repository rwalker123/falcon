//! **A band defects only to a people it has actually met.**
//!
//! The knowledge migration's destination used to be `registry.factions().iter().find(|&&f| f !=
//! cohort.faction)` — the first id that is not your own — and on arrival the cohort's faction is
//! rewritten, permanently. On a one-faction map `find` searched a one-element list and could never
//! return `Some`, so the behaviour had never been reachable; the moment worldgen places a second
//! faction it takes a player's strongest, happiest, most knowledgeable band and hands it to
//! strangers on the far side of the world.
//!
//! The gate is contact, answered by the connection ledger: a live tie, in either direction, between
//! a band of ours and a band of theirs. Distance and prosperity are separate designs and are
//! deliberately not asked here — this suite pins the contact half and nothing more.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::{
    simulate_population, BandId, ConnectionKey, ConnectionLedger, ConnectionsConfigHandle,
    PopulationCohort,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};

/// Comfortably past `migration_min_settled_turns` (5) so the settled gate cannot be what decides the
/// outcome of either arm.
const SETTLED_TURNS: u32 = 40;

/// Comfortably past `migration_morale_threshold` (0.78) — the trigger is HIGH morale, not low.
const HAPPY_MORALE: f32 = 0.95;

/// The turn a staged sighting is stamped with. Any turn does; it only has to be a number both clocks
/// agree on.
const SIGHTING_TURN: u64 = 1;

fn opening_band(app: &mut App, faction: core_sim::FactionId) -> BandId {
    app.world
        .query::<(&BandId, &PopulationCohort)>()
        .iter(&app.world)
        .filter(|(_, cohort)| cohort.faction == faction)
        .map(|(band, _)| *band)
        .min()
        .unwrap_or_else(|| panic!("{faction:?} has an opening band"))
}

/// Put every band in the state the migration trigger wants: settled, happy, and carrying knowledge
/// worth taking. Re-applied before each run, because `simulate_population` ages the band and moves
/// its morale — a second run must exercise the *gate*, not a band that has since fallen below it.
fn make_every_band_ready_to_defect(app: &mut App) {
    let mut query = app.world.query::<&mut PopulationCohort>();
    for mut cohort in query.iter_mut(&mut app.world) {
        cohort.age_turns = SETTLED_TURNS;
        cohort.morale = core_sim::scalar_from_f32(HAPPY_MORALE);
        cohort.migration = None;
        assert!(
            !cohort.knowledge.is_empty(),
            "the profile seeds every spawned band with knowledge, or the trigger cannot fire"
        );
    }
}

fn queued_destinations(app: &mut App) -> Vec<(core_sim::FactionId, core_sim::FactionId)> {
    let mut rows: Vec<_> = app
        .world
        .query::<&PopulationCohort>()
        .iter(&app.world)
        .filter_map(|cohort| {
            cohort
                .migration
                .as_ref()
                .map(|migration| (cohort.faction, migration.destination))
        })
        .collect();
    rows.sort();
    rows
}

/// Record a live tie in both directions between two bands, through the ledger's own contact path
/// rather than by writing an edge by hand — a hand-built edge could hold a strength the config never
/// grants.
fn record_mutual_contact(app: &mut App, a: BandId, b: BandId) {
    let config = app.world.resource::<ConnectionsConfigHandle>().get();
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for key in [ConnectionKey::new(a, b), ConnectionKey::new(b, a)] {
        ledger.record_contact(key, UVec2::ZERO, SIGHTING_TURN, SIGHTING_TURN, &config);
    }
}

#[test]
fn a_band_does_not_defect_to_a_faction_it_has_never_met() {
    let mut world = world_with(ONE_RIVAL, |_| {});
    assert!(
        world.world.resource::<ConnectionLedger>().is_empty(),
        "a freshly generated world has met nobody — that is the premise of this arm"
    );

    make_every_band_ready_to_defect(&mut world);
    world.world.run_system_once(simulate_population);

    assert_eq!(
        queued_destinations(&mut world),
        Vec::new(),
        "a settled, happy, knowledgeable band must not queue a defection to strangers"
    );
}

#[test]
fn a_band_defects_once_the_two_peoples_have_made_contact() {
    let mut world = world_with(ONE_RIVAL, |_| {});
    let home_band = opening_band(&mut world, HOME);
    let rival_band = opening_band(&mut world, RIVAL);
    record_mutual_contact(&mut world, home_band, rival_band);

    make_every_band_ready_to_defect(&mut world);
    world.world.run_system_once(simulate_population);

    let queued = queued_destinations(&mut world);
    assert!(
        queued.contains(&(HOME, RIVAL)),
        "the human's band should queue a defection to the people it has met, got {queued:?}"
    );
}

/// The liveness half of the pair above: contact is what changed, not the trigger. Both arms share
/// one world so the only difference between them is the ledger.
#[test]
fn the_gate_is_the_only_difference_between_the_two_arms() {
    let mut world = world_with(ONE_RIVAL, |_| {});
    let home_band = opening_band(&mut world, HOME);
    let rival_band = opening_band(&mut world, RIVAL);

    make_every_band_ready_to_defect(&mut world);
    world.world.run_system_once(simulate_population);
    assert!(
        queued_destinations(&mut world).is_empty(),
        "before contact, nothing is queued"
    );

    record_mutual_contact(&mut world, home_band, rival_band);
    make_every_band_ready_to_defect(&mut world);
    world.world.run_system_once(simulate_population);
    assert!(
        !queued_destinations(&mut world).is_empty(),
        "after contact, the same bands in the same state do queue"
    );
}
