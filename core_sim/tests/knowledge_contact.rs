//! **The knowledge rider, driven by a real turn** (issue #531,
//! `docs/plan_contact_and_logistics.md` §Settled by #531).
//!
//! The rule's arithmetic — max not sum, the three channels, a parked tie, the threshold, a band of
//! one's own people — is unit-tested against the pure function in `core_sim::knowledge_contact`.
//! What is pinned here is what a unit test cannot see: that the scheduled system actually credits
//! the ledger off the ties the sight sweep just refreshed, and that the readout reaches the
//! **encoded** frame naming the people learned from.

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    run_turn, scalar_from_f32, ConnectionKey, ConnectionLedger, DiscoveryProgressLedger, FactionId,
    KnowledgeContactConfig, LadderConfig, PopulationCohort, ResidentBand, Scalar, SimulationConfig,
    SnapshotHistory, StartProfileKnowledgeTags, TileRegistry, NO_TIE,
};
use faction_support::{two_faction_world, HOME, RIVAL};

/// The taught discovery — fully observable, and taught at the start of no campaign.
const PENNING_TAG: &str = "penning";
/// `ContactLessonState.channel` for watching.
const WATCHING_WIRE: u8 = 0;

fn discovery(tag: &str) -> u32 {
    StartProfileKnowledgeTags::builtin()
        .get(tag)
        .expect("a shipped tag")
        .discovery_id()
}

/// The two peoples' opening camps, side by side, so each band sees the other on the turn's sweep.
fn stage_side_by_side(app: &mut App) {
    let opening = |app: &mut App, faction: FactionId| {
        app.world
            .query_filtered::<(Entity, &core_sim::BandId, &PopulationCohort), With<ResidentBand>>()
            .iter(&app.world)
            .filter(|(_, _, cohort)| cohort.faction == faction)
            .min_by_key(|(_, band, _)| **band)
            .map(|(entity, _, _)| entity)
            .unwrap_or_else(|| panic!("{faction:?} has an opening band"))
    };
    let home = opening(app, HOME);
    let rival = opening(app, RIVAL);
    let home_tile = app
        .world
        .get::<PopulationCohort>(home)
        .unwrap()
        .current_tile;
    let at = app.world.get::<core_sim::Tile>(home_tile).unwrap().position;
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    let beside = app
        .world
        .resource::<TileRegistry>()
        .index((at.x + 1) % width, at.y)
        .expect("the tile beside home is on the map");
    let mut cohort = app.world.get_mut::<PopulationCohort>(rival).unwrap();
    cohort.home = beside;
    cohort.current_tile = beside;
}

/// The strongest live tie any HOME band holds toward any RIVAL band, after the turn.
fn strongest_home_to_rival_tie(app: &mut App) -> f32 {
    let bands: Vec<(core_sim::BandId, FactionId)> = app
        .world
        .query::<(&core_sim::BandId, &PopulationCohort)>()
        .iter(&app.world)
        .map(|(band, cohort)| (*band, cohort.faction))
        .collect();
    let ledger = app.world.resource::<ConnectionLedger>();
    let mut strongest = NO_TIE;
    for (observer, observer_faction) in &bands {
        for (subject, subject_faction) in &bands {
            if *observer_faction != HOME || *subject_faction != RIVAL {
                continue;
            }
            if let Some(tie) = ledger.get(&ConnectionKey::new(*observer, *subject)) {
                strongest = strongest.max(tie.strength);
            }
        }
    }
    strongest.to_f32()
}

#[test]
fn a_people_learns_by_watching_what_a_tied_people_knows_and_the_frame_names_them() {
    let mut app = two_faction_world();
    stage_side_by_side(&mut app);
    let penning = discovery(PENNING_TAG);
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(RIVAL, penning, Scalar::one());
    assert_eq!(
        app.world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(HOME, penning),
        Scalar::zero(),
        "fixture: HOME does not know penning"
    );

    run_turn(&mut app);

    let tie = strongest_home_to_rival_tie(&mut app);
    assert!(tie > 0.0, "liveness: the camps side by side formed a tie");
    let ladder = LadderConfig::builtin();
    let observability = StartProfileKnowledgeTags::builtin()
        .get(PENNING_TAG)
        .unwrap()
        .observability();
    let watching = KnowledgeContactConfig::builtin().channel_rates.watching;
    let want = tie * watching * observability
        / ladder
            .knowledge
            .lesson_cost(PENNING_TAG)
            .expect("penning is priced");
    let progress = app
        .world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(HOME, penning);
    assert_eq!(
        progress,
        scalar_from_f32(want),
        "HOME's ledger rises by exactly tie × watching × observability / lesson_cost"
    );

    // ---- the wire: the encoded envelope, the accessor chain a client uses -------------------
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let lessons = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .knowledge()
        .and_then(|section| section.contactLessons())
        .expect("the contact lessons are published");
    let row = lessons
        .iter()
        .find(|row| row.discoveryId() == penning)
        .expect("the viewer's people is published as learning penning by contact");
    assert_eq!(row.knowledgeId(), Some(PENNING_TAG));
    assert_eq!(
        row.subjectFaction(),
        RIVAL.0,
        "learned from the rival people"
    );
    assert_eq!(row.channel(), WATCHING_WIRE, "by watching");
    assert!(
        (row.credit() - want).abs() < f32::EPSILON,
        "the published credit is the credit paid: {} vs {want}",
        row.credit()
    );
    assert!(
        lessons.iter().all(|row| row.subjectFaction() != HOME.0),
        "the viewer's own people never teaches the viewer"
    );
}
