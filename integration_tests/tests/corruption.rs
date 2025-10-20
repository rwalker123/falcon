use core_sim::{
    build_headless_app, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage, Scalar,
    SentimentAxisBias,
};
use sim_runtime::{CorruptionEntry, CorruptionSubsystem};

#[test]
fn corruption_exposure_updates_sentiment_and_telemetry() {
    let mut app = build_headless_app();

    {
        let mut ledgers = app.world.resource_mut::<CorruptionLedgers>();
        let intensity = Scalar::from_f32(0.2).raw();
        let entry = CorruptionEntry {
            subsystem: CorruptionSubsystem::Military,
            intensity,
            incident_id: 42,
            exposure_timer: 1,
            restitution_window: 0,
            last_update_tick: 0,
        };
        ledgers.ledger_mut().register_incident(entry);
    }

    // First tick should expose the incident and apply trust penalty.
    app.update();

    let telemetry = app.world.resource::<CorruptionTelemetry>();
    assert_eq!(telemetry.exposures_this_turn.len(), 1);
    assert_eq!(telemetry.active_incidents, 0);
    assert_eq!(telemetry.exposures_total, 1);
    let record = &telemetry.exposures_this_turn[0];
    assert_eq!(record.incident_id, 42);
    assert_eq!(record.subsystem, CorruptionSubsystem::Military);
    assert!(
        record.trust_delta < 0,
        "Exposure should record negative trust delta"
    );

    let sentiment = app.world.resource::<SentimentAxisBias>();
    let trust = sentiment.incident_values()[1];
    assert!(
        trust < Scalar::zero(),
        "Trust axis should drop after corruption exposure; got {trust:?}"
    );

    let diplomacy = app.world.resource::<DiplomacyLeverage>();
    assert!(
        diplomacy
            .recent
            .iter()
            .any(|exposure| exposure.incident_id == 42),
        "Diplomacy leverage log should capture exposure"
    );

    // Second tick should clear per-turn telemetry while keeping cumulative totals.
    app.update();
    let telemetry = app.world.resource::<CorruptionTelemetry>();
    assert!(
        telemetry.exposures_this_turn.is_empty(),
        "Per-turn exposures should reset"
    );
    assert_eq!(telemetry.exposures_total, 1);
}
