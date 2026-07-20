mod common;

use core_sim::{
    build_headless_app, restore_world_from_snapshot, BeatCatalogHandle, BeatLedger, FactionId,
    Scalar, SnapshotHistory,
};

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

/// A fork **answered after the rollback point is un-answered by the rollback**: the declared stance
/// offset, the fired mark, the recorded answer and the pending list all rewind together.
///
/// This is the fork tier's half of the round-trip above. Getting it wrong would leave a player
/// carrying a stance they never declared in the timeline they rolled back into — the narrative
/// equivalent of a beat stuck marked-fired.
#[test]
fn answering_a_fork_after_the_rollback_point_is_rewound_by_the_rollback() {
    common::ensure_test_config();
    let mut app = build_headless_app();
    app.update();

    const FORK: &str = "sedentarization.soft_drift";
    const AXIS: &str = "roam_settle";

    // Put a fork on the table *before* the capture, so the rollback target has one pending.
    {
        let mut ledger = app.world.resource_mut::<BeatLedger>();
        let mut state = ledger.to_state();
        state.pending_forks.push(sim_schema::BeatPendingForkState {
            beat_id: FORK.to_string(),
            wardrobe_id: "soft_drift.river_bend".to_string(),
            faction: 0,
            posted_tick: 1,
            narration: vec![sim_schema::BeatVoiceLineState {
                register: "mythic".to_string(),
                text: "The river-bend remembers us now.".to_string(),
            }],
            choices: vec![
                sim_schema::BeatForkChoiceState {
                    choice_id: "yes_trail".to_string(),
                    is_defer: false,
                    label: vec![sim_schema::BeatVoiceLineState {
                        register: "mythic".to_string(),
                        text: "We are the trail".to_string(),
                    }],
                    echo: vec![sim_schema::BeatVoiceLineState {
                        register: "mythic".to_string(),
                        text: "So it is said.".to_string(),
                    }],
                },
                sim_schema::BeatForkChoiceState {
                    choice_id: "defer".to_string(),
                    is_defer: true,
                    label: vec![sim_schema::BeatVoiceLineState {
                        register: "mythic".to_string(),
                        text: "Say nothing".to_string(),
                    }],
                    echo: vec![sim_schema::BeatVoiceLineState {
                        register: "mythic".to_string(),
                        text: "The fires keep it.".to_string(),
                    }],
                },
            ],
            gloss: vec![sim_schema::BeatSignalValueState {
                signal: "sedentarization.score".to_string(),
                value: Scalar::from_f32(41.0).raw(),
            }],
        });
        *ledger = BeatLedger::from_state(&state);
    }

    // Re-capture so the ring entry we roll back to carries the pending fork.
    app.update();
    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        history
            .latest_entry()
            .expect("snapshot captured")
            .snapshot
            .clone()
    };
    let captured = app.world.resource::<BeatLedger>().clone();
    assert_eq!(
        snapshot.beat_ledger.pending_forks.len(),
        1,
        "capture must persist the pending fork"
    );

    // Answer it — *after* the rollback point.
    {
        let catalog = app.world.resource::<BeatCatalogHandle>().get();
        let mut ledger = app.world.resource_mut::<BeatLedger>();
        ledger
            .answer_fork(&catalog, FactionId(0), FORK, "yes_trail", 5)
            .expect("the pending fork is answerable");
    }
    {
        let answered = app.world.resource::<BeatLedger>();
        assert!(answered.has_fired(FORK));
        assert_eq!(answered.answer(FORK), Some("yes_trail"));
        assert!(answered.stance().contains_key(AXIS));
        assert!(answered.pending_forks().is_empty());
    }

    restore_world_from_snapshot(&mut app.world, snapshot.as_ref());

    let restored = app.world.resource::<BeatLedger>();
    assert!(
        !restored.has_fired(FORK),
        "an answer made after the rollback point must be un-made"
    );
    assert_eq!(restored.answer(FORK), None, "the memory of it rewinds too");
    assert!(
        !restored.stance().contains_key(AXIS),
        "the player must not carry a stance they never declared in this timeline"
    );
    assert_eq!(
        restored.pending_forks().len(),
        1,
        "the question goes back on the table"
    );
    assert_eq!(*restored, captured, "the whole fork tier must rewind");
}
