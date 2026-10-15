//! **THE LADDER CATALOG ON THE WIRE** - `ladderRungs` and `ladderBranches`, once per world, derived
//! from `intensification_ladder.json` and nothing else. They replace the route and deposit catalogs
//! (and give the plant and animal branches one), so these tests read the ENCODED envelope through
//! the accessor chain a client uses and hold every row to the config's own record - a catalog that
//! drifted from the config would otherwise draw a ladder the sim does not run.

use bevy::app::App;
use core_sim::{build_test_app, LadderConfigHandle, SnapshotHistory};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn a_world_with_a_catalog() -> App {
    let mut app = build_test_app();
    app.update();
    app
}

fn encoded(app: &App) -> Vec<u8> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
}

/// **EVERY RUNG OF EVERY BRANCH IS PUBLISHED, IN CLIMB ORDER WITHIN ITS BRANCH**, and each row's
/// identity, copy, verb, kind and traits are the config record's own.
#[test]
fn the_catalog_publishes_every_rung_as_the_config_declares_it() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("the subsistence section");
    let rows = section
        .ladderRungs()
        .expect("the rung catalog is published");
    assert_eq!(rows.len(), ladder.rungs.len(), "one row per config rung");

    let mut at = 0;
    for branch in ladder.branches_in_config_order() {
        let kind = ladder
            .source_kind(branch)
            .expect("a declared branch has a kind")
            .as_str();
        for rung in ladder.rungs_of(branch) {
            let row = rows.get(at);
            at += 1;
            assert_eq!(row.rungKey(), Some(rung.wire_key().as_str()));
            assert_eq!(row.branch(), Some(branch.as_str()));
            assert_eq!(row.order(), rung.order);
            assert_eq!(row.label(), Some(rung.label.as_str()));
            assert_eq!(row.labelFallback(), Some(rung.label_fallback.as_str()));
            assert_eq!(row.verb(), Some(rung.verb.as_deref().unwrap_or_default()));
            assert_eq!(row.sourceKind(), Some(kind));
            let traits: Vec<&str> = row
                .traits()
                .map(|names| names.iter().collect())
                .unwrap_or_default();
            let expected: Vec<&str> = rung.trait_set().members().map(|t| t.as_str()).collect();
            assert_eq!(traits, expected, "{} traits", rung.wire_key());
            assert_eq!(
                row.requiresRung(),
                Some(rung.requires_rung_wire_key().unwrap_or_default().as_str())
            );
            assert_eq!(
                row.earnsKnowledge(),
                Some(rung.earns_knowledge.as_deref().unwrap_or_default())
            );
        }
    }
    assert_eq!(at, rows.len());
}

/// **THE PAYOFF BLOCKS RIDE THEIR OWN KIND AND NEUTRAL ELSEWHERE** - a road rung carries its
/// friction and link reach, a deposit rung its take, and a plant rung neither: a defaulted row must
/// never claim to be a road that helps a haul or a rung that stops the ground growing.
#[test]
fn a_rung_carries_only_the_payoff_block_its_kind_declares() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.ladderRungs())
        .expect("the rung catalog is published");
    let mut saw_road = false;
    let mut saw_deposit = false;
    let mut saw_neutral = false;
    for row in rows.iter() {
        let rung = ladder
            .rungs
            .iter()
            .find(|rung| Some(rung.wire_key().as_str()) == row.rungKey())
            .expect("every published row names a config rung");
        if let Some(payoff) = &rung.route_payoff {
            saw_road = true;
            assert_eq!(row.frictionMultiplier(), payoff.friction_multiplier);
            assert_eq!(row.holdsLinkToTiles(), payoff.holds_link_to_tiles);
        } else {
            saw_neutral = true;
            assert_eq!(row.frictionMultiplier(), 1.0, "no road payoff, no help");
            assert_eq!(row.holdsLinkToTiles(), 0);
            assert!(!row.grantsSight());
        }
        if let Some(payoff) = &rung.extraction_payoff {
            saw_deposit = true;
            assert_eq!(row.yieldPerWorkerTurn(), payoff.yield_per_worker_turn);
            assert_eq!(row.recoveryFraction(), payoff.recovery_fraction);
            assert_eq!(row.regrowthMultiplier(), payoff.regrowth_multiplier);
        } else {
            assert_eq!(row.yieldPerWorkerTurn(), 0.0);
            assert_eq!(row.recoveryFraction(), 0.0);
            assert_eq!(row.regrowthMultiplier(), 1.0, "the ground grows as it did");
        }
    }
    assert!(
        saw_road && saw_deposit && saw_neutral,
        "the shipped ladder has road, deposit and plain rungs, or this proves nothing"
    );
}

/// **THE BRANCHES ARE PUBLISHED IN THE CATALOG'S OWN ORDER** with their area, kind and heading.
#[test]
fn the_branches_are_published_beside_the_rungs() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("the subsistence section");
    let rows = section
        .ladderBranches()
        .expect("the branch list is published");
    let declared = ladder.branches_in_config_order();
    assert_eq!(rows.len(), declared.len());
    for (row, branch) in rows.iter().zip(declared) {
        let def = ladder.branch_def(branch).expect("a declared branch");
        assert_eq!(row.branch(), Some(branch.as_str()));
        assert_eq!(row.area(), Some(def.area.as_str()));
        assert_eq!(row.sourceKind(), Some(def.source.as_str()));
        assert_eq!(row.label(), Some(def.label.as_str()));
    }
}
