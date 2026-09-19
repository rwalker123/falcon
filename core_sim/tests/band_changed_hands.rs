//! **A whole band changing faction is told to BOTH peoples.**
//!
//! The knowledge-migration branch in `systems::population` used to flip `cohort.faction` in
//! silence: the two events it sends (`TradeDiffusionEvent`, `MigrationKnowledgeEvent`) have no
//! `EventReader` anywhere in the crate, and the branch pushed nothing to the event log. So a player
//! gained — or lost — an entire band with no line on any surface, which is how the handover came to
//! be mistaken for a bug.
//!
//! Asserted on the **published** `command_events`, per viewer, off the encoded envelope: the feed is
//! filtered by `entry.faction == viewer` (`snapshot::campaign::command_events_to_state`), so an
//! in-process log check cannot tell "both sides were told" from "one row exists".

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    publish_baseline_snapshot, run_turn, BandId, FactionId, PendingMigration, PopulationCohort,
    ResidentBand, SnapshotHistory, ViewerFaction,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The wire spelling of `CommandEventKind::BandChangedHands`.
const KIND: &str = "band_changed_hands";

/// One turn is all the branch needs once the payload is queued: the eta is decremented and then
/// tested against zero in the same pass, so `1` completes on the very next Population stage.
const ETA_COMPLETING_THIS_TURN: u16 = 1;

/// Every published `band_changed_hands` row this viewer's frame carries, as `(label, detail)`.
fn published_handovers(app: &App) -> Vec<(String, String)> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let Some(rows) = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .campaign()
        .and_then(|section| section.commandEvents())
    else {
        return Vec::new();
    };
    rows.iter()
        .filter(|row| row.kind().unwrap_or_default() == KIND)
        .map(|row| {
            (
                row.label().unwrap_or_default().to_string(),
                row.detail().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

/// Capture a frame for `viewer` and read its handover rows. Re-published rather than re-resolved:
/// the same turn's log has to be read through two different viewer filters.
fn handovers_seen_by(app: &mut App, viewer: FactionId) -> Vec<(String, String)> {
    app.world.insert_resource(ViewerFaction(viewer));
    publish_baseline_snapshot(&mut app.world);
    published_handovers(app)
}

/// The home faction's first resident band, and its durable id.
fn home_band(app: &mut App) -> (Entity, BandId) {
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort, &BandId), With<ResidentBand>>();
    query
        .iter(&app.world)
        .find(|(_, cohort, _)| cohort.faction == HOME)
        .map(|(entity, _, band)| (entity, *band))
        .expect("the home faction opens with a resident band")
}

#[test]
fn a_completed_migration_tells_the_people_who_lost_the_band_and_the_people_who_gained_it() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    let (entity, band) = home_band(&mut app);

    // **The payload is queued directly rather than earned.** What is under test is the handover's
    // announcement, not the morale/contact/settled-turns gate that decides to queue one — driving
    // that gate would make this a test of the gate's tuning. An empty fragment list is a real
    // payload: the diffusion loop simply moves nothing, and the faction still changes hands.
    app.world
        .entity_mut(entity)
        .get_mut::<PopulationCohort>()
        .expect("the band carries a cohort")
        .migration = Some(PendingMigration {
        destination: RIVAL,
        eta: ETA_COMPLETING_THIS_TURN,
        fragments: Vec::new(),
    });

    run_turn(&mut app);

    assert_eq!(
        app.world
            .entity(entity)
            .get::<PopulationCohort>()
            .expect("the band survived the turn")
            .faction,
        RIVAL,
        "the migration must have completed, or there is no handover to announce"
    );

    let lost = handovers_seen_by(&mut app, HOME);
    assert_eq!(
        lost.len(),
        1,
        "the people who lost the band get exactly one line: {lost:?}"
    );
    assert!(
        lost[0].0.contains(&format!("Band {}", band.0)),
        "the line names the band: {:?}",
        lost[0].0
    );
    assert!(
        lost[0].1.contains("side=lost"),
        "and says which side of the handover it is: {:?}",
        lost[0].1
    );

    let gained = handovers_seen_by(&mut app, RIVAL);
    assert_eq!(
        gained.len(),
        1,
        "and the people who gained it get exactly one: {gained:?}"
    );
    assert!(
        gained[0].0.contains(&format!("Band {}", band.0)),
        "naming the same band: {:?}",
        gained[0].0
    );
    assert!(
        gained[0].1.contains("side=gained"),
        "from the other side: {:?}",
        gained[0].1
    );

    // The two rows are the same event read twice, so they must agree about who it happened between
    // — a pair that named different factions would be two handovers, not one.
    let endpoints = format!("from={} to={}", HOME.0, RIVAL.0);
    assert!(
        lost[0].1.contains(&endpoints) && gained[0].1.contains(&endpoints),
        "both halves carry the same endpoints: {:?} / {:?}",
        lost[0].1,
        gained[0].1
    );
    assert_ne!(
        lost[0].0, gained[0].0,
        "each people is told what happened to IT, so the two lines do not read alike"
    );
}

#[test]
fn a_band_that_does_not_change_hands_announces_nothing() {
    // The negative control. Without it "exactly one row" above passes on a sim that pushes a
    // handover line every turn for every band.
    let mut app = world_with(ONE_RIVAL, |_| {});
    run_turn(&mut app);

    for viewer in [HOME, RIVAL] {
        let rows = handovers_seen_by(&mut app, viewer);
        assert!(
            rows.is_empty(),
            "no band changed hands, so faction {} is told nothing: {rows:?}",
            viewer.0
        );
    }
}
