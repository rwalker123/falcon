//! **A whole band changing faction is told to BOTH peoples.**
//!
//! A band changes people only in the extreme: a cross-people move under the wellbeing trickle leaves
//! the source below `settle.parent_min_workers`, and the remnant goes over with its leavers
//! (`advance_population_migration`, `docs/plan_band_fission.md` §Defection). A player gains — or
//! loses — an entire band there, so both sides need a line.
//!
//! Asserted on the **published** `command_events`, per viewer, off the encoded envelope: the feed is
//! filtered by `entry.faction == viewer` (`snapshot::campaign::command_events_to_state`), so an
//! in-process log check cannot tell "both sides were told" from "one row exists".

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::{
    advance_population_migration, publish_baseline_snapshot, run_turn, scalar_from_f32, BandId,
    ConnectionKey, ConnectionLedger, ConnectionsConfigHandle, ExpeditionConfigHandle, FactionId,
    PopulationCohort, ResidentBand, Scalar, Sighting, SimulationConfig, SnapshotHistory, Tile,
    TileRegistry, ViewerFaction,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The wire spelling of `CommandEventKind::BandChangedHands`.
const KIND: &str = "band_changed_hands";

/// Rock-bottom morale: the source wants out at the trickle's full rate.
const MISERABLE: f32 = 0.0;
/// Comfortably above `attractive_morale` and any source + `min_morale_gap`.
const THRIVING: f32 = 0.9;
/// The turn a staged contact is stamped with.
const SIGHTING_TURN: u64 = 1;

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

/// A faction's first resident band, and its durable id.
fn opening_band(app: &mut App, faction: FactionId) -> (Entity, BandId) {
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort, &BandId), With<ResidentBand>>();
    query
        .iter(&app.world)
        .filter(|(_, cohort, _)| cohort.faction == faction)
        .map(|(entity, _, band)| (entity, *band))
        .min_by_key(|(_, band)| *band)
        .expect("the faction opens with a resident band")
}

/// Stage the one situation in which a band changes people: a miserable home band exactly at the
/// parent floor, a thriving rival camp on the next tile, and a live tie between them. Every other
/// band is made miserable too, so none of them is a destination.
fn stage_a_remnant_handover(app: &mut App) -> (Entity, BandId) {
    let (home, home_id) = opening_band(app, HOME);
    let (rival, rival_id) = opening_band(app, RIVAL);
    let floor = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .parent_min_workers;
    let at = {
        let tile = app.world.get::<PopulationCohort>(home).unwrap().home;
        app.world.get::<Tile>(tile).unwrap().position
    };
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    let beside = app
        .world
        .resource::<TileRegistry>()
        .index((at.x + 1) % width, at.y)
        .expect("the staged tile is on the map");
    let bands: Vec<Entity> = app
        .world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .collect();
    for band in bands {
        app.world.get_mut::<PopulationCohort>(band).unwrap().morale = scalar_from_f32(MISERABLE);
    }
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(rival).unwrap();
        cohort.home = beside;
        cohort.current_tile = beside;
        cohort.morale = scalar_from_f32(THRIVING);
    }
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(home).unwrap();
        cohort.working = Scalar::from_u32(floor);
        cohort.children = Scalar::zero();
        cohort.elders = Scalar::zero();
        cohort.sync_size();
    }
    let config = app.world.resource::<ConnectionsConfigHandle>().get();
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for key in [
        ConnectionKey::new(home_id, rival_id),
        ConnectionKey::new(rival_id, home_id),
    ] {
        ledger.record_contact(
            key,
            &Sighting::new(UVec2::ZERO, SIGHTING_TURN, ""),
            SIGHTING_TURN,
            &config,
        );
    }
    (home, home_id)
}

#[test]
fn a_remnant_that_goes_over_tells_the_people_who_lost_the_band_and_the_people_who_gained_it() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    let (entity, band) = stage_a_remnant_handover(&mut app);

    // The trickle alone, once: a full turn would re-resolve morale off the staging.
    app.world.run_system_once(advance_population_migration);

    assert_eq!(
        app.world
            .entity(entity)
            .get::<PopulationCohort>()
            .expect("the band survived the turn")
            .faction,
        RIVAL,
        "the remnant must have gone over, or there is no handover to announce"
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
