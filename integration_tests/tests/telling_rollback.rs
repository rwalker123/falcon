mod common;

use core_sim::{build_headless_app, restore_world_from_snapshot, BeatLedger, SnapshotHistory};

/// Regression: The Telling's `BeatLedger` must round-trip through the rollback snapshot
/// **including restore**. A ledger that is captured but never restored (the `SedentarizationScore`
/// gap) leaves a beat marked fired after a rollback past it, so it could never fire again — and
/// worse, its edge state would be stale, so `crosses` would misfire. This asserts a
/// mutate-then-restore rewinds the narrative memory exactly.
#[test]
fn beat_ledger_rewinds_on_rollback_so_a_beat_can_fire_again() {
    common::ensure_test_config();
    let mut app = build_headless_app();

    // Turn 1: the pipeline runs `telling_tick` and `capture_snapshot` records the ring entry.
    app.update();

    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        let stored = history.latest_entry().expect("snapshot captured");
        stored.snapshot.clone()
    };
    // Turn 0 fires the opening beat, so the captured ledger is non-empty — which is what makes
    // this a real round-trip rather than a default-vs-default comparison.
    assert!(
        !snapshot.beat_ledger.fired.is_empty(),
        "capture must persist the beat ledger's fired-set"
    );
    assert!(
        !snapshot.beat_ledger.edge_state.is_empty(),
        "capture must persist the edge state backing `crosses`"
    );

    let captured = app.world.resource::<BeatLedger>().clone();
    assert!(captured.has_fired("opening.cold_open"));

    // Mutate the live ledger well away from its captured state: mark a beat fired that had not
    // fired, and stale the novelty memory.
    const LATER_BEAT: &str = "sedentarization.soft_drift";
    assert!(
        !captured.has_fired(LATER_BEAT),
        "the beat used for the rewind assertion must not have fired yet"
    );
    {
        let mut ledger = app.world.resource_mut::<BeatLedger>();
        *ledger = BeatLedger::from_state(&{
            let mut state = captured.to_state();
            state.fired.push(sim_schema::BeatFiredState {
                beat: LATER_BEAT.to_string(),
                ticks: vec![1],
            });
            state
                .wardrobe_usage
                .push(sim_schema::BeatWardrobeUsageState {
                    wardrobe: "soft_drift.river_bend".to_string(),
                    last_used_tick: 1,
                });
            state
        });
    }
    assert!(app.world.resource::<BeatLedger>().has_fired(LATER_BEAT));

    // Roll back to the captured snapshot.
    restore_world_from_snapshot(&mut app.world, snapshot.as_ref());

    let restored = app.world.resource::<BeatLedger>();
    assert!(
        !restored.has_fired(LATER_BEAT),
        "a rollback past a beat must un-fire it, so it can fire again"
    );
    assert_eq!(
        restored.wardrobe_last_used("soft_drift.river_bend"),
        None,
        "the novelty memory must rewind with the rest of the ledger"
    );
    assert_eq!(
        *restored, captured,
        "the whole ledger — fired-set, edge state, history, novelty, flags, stance — must rewind"
    );
}
