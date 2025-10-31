mod common;

use core_sim::{
    build_headless_app, CorruptionLedgers, CorruptionTelemetry, DiplomacyLeverage, LogisticsLink,
    PowerNode, Scalar, SentimentAxisBias, TradeLink,
};
use sim_runtime::{CorruptionEntry, CorruptionSubsystem};

#[test]
fn corruption_exposure_updates_sentiment_and_telemetry() {
    common::ensure_test_config();
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

#[test]
fn corruption_modifiers_reduce_outputs() {
    common::ensure_test_config();
    let mut clean = build_headless_app();
    let mut corrupt = build_headless_app();

    {
        let mut ledgers = corrupt.world.resource_mut::<CorruptionLedgers>();
        let intensity = Scalar::from_f32(0.6).raw();
        let mut register = |id: u64, subsystem: CorruptionSubsystem| {
            let entry = CorruptionEntry {
                subsystem,
                intensity,
                incident_id: id,
                exposure_timer: 50,
                restitution_window: 0,
                last_update_tick: 0,
            };
            ledgers.ledger_mut().register_incident(entry);
        };
        register(1, CorruptionSubsystem::Logistics);
        register(2, CorruptionSubsystem::Trade);
        register(3, CorruptionSubsystem::Military);
    }

    for _ in 0..4 {
        clean.update();
    }
    for _ in 0..4 {
        corrupt.update();
    }

    let flow_clean = {
        let mut query = clean.world.query::<&LogisticsLink>();
        query
            .iter(&clean.world)
            .next()
            .expect("logistics link in clean app")
            .flow
            .to_f32()
            .abs()
    };
    let flow_corrupt = {
        let mut query = corrupt.world.query::<&LogisticsLink>();
        query
            .iter(&corrupt.world)
            .next()
            .expect("logistics link in corrupt app")
            .flow
            .to_f32()
            .abs()
    };
    assert!(
        flow_corrupt <= flow_clean,
        "logistics corruption should not improve throughput"
    );

    let tariff_clean = {
        let mut query = clean.world.query::<&TradeLink>();
        query
            .iter(&clean.world)
            .next()
            .expect("trade link in clean app")
            .tariff
            .to_f32()
    };
    let tariff_corrupt = {
        let mut query = corrupt.world.query::<&TradeLink>();
        query
            .iter(&corrupt.world)
            .next()
            .expect("trade link in corrupt app")
            .tariff
            .to_f32()
    };
    assert!(
        tariff_corrupt <= tariff_clean,
        "trade corruption should reduce tariff yield"
    );

    let power_clean = {
        let mut query = clean.world.query::<&PowerNode>();
        query
            .iter(&clean.world)
            .next()
            .expect("power node in clean app")
            .generation
            .to_f32()
    };
    let power_corrupt = {
        let mut query = corrupt.world.query::<&PowerNode>();
        query
            .iter(&corrupt.world)
            .next()
            .expect("power node in corrupt app")
            .generation
            .to_f32()
    };
    assert!(
        power_corrupt <= power_clean,
        "military corruption should reduce effective procurement output"
    );
}
