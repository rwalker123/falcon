use std::time::{Duration, Instant};

use core_sim::{
    build_headless_app, run_turn, scalar_from_f32, scalar_one, scalar_zero,
    ConstellationRequirement, DiscoveryProgressLedger, FactionId, FactionRegistry,
    GreatDiscoveryDefinition, GreatDiscoveryId, GreatDiscoveryLedger, GreatDiscoveryRegistry,
    GreatDiscoveryTelemetry, KnowledgeFragment, ObservationLedger, SimulationConfig,
    SnapshotHistory, TradeLink, TradeTelemetry,
};
use sim_runtime::KnowledgeField;

const FORCED_PUBLICATION_FLAG: u32 = 1 << 3;

#[test]
fn gds_turn_budget_processes_many_constellations_in_single_turn() {
    let mut app = build_headless_app();

    {
        let mut factions = app.world.resource_mut::<FactionRegistry>();
        factions.factions = vec![FactionId(0)];
    }

    {
        let mut observations = app.world.resource_mut::<ObservationLedger>();
        observations.set_observations(FactionId(0), KnowledgeField::Physics, 5);
    }

    const DISCOVERY_COUNT: usize = 32;
    for index in 0..DISCOVERY_COUNT {
        let discovery_id = GreatDiscoveryId(index as u16);
        let requirement_id = 1_000 + index as u32;
        {
            let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
            registry.register(GreatDiscoveryDefinition::new(
                discovery_id,
                format!("Discovery {}", discovery_id.0),
                KnowledgeField::Physics,
                vec![ConstellationRequirement::new(
                    requirement_id,
                    scalar_one(),
                    scalar_zero(),
                )],
                0,
                0,
                None,
                0,
                false,
            ));
        }
        {
            let mut ledger = app.world.resource_mut::<DiscoveryProgressLedger>();
            ledger.add_progress(FactionId(0), requirement_id, scalar_one());
        }
    }

    let start = Instant::now();
    run_turn(&mut app);
    let elapsed = start.elapsed();
    const MAX_STAGE_DURATION_MS: u64 = 1_500;
    assert!(
        elapsed < Duration::from_millis(MAX_STAGE_DURATION_MS),
        "Great Discovery stage exceeded expected turn budget (>{} ms): {:?}",
        MAX_STAGE_DURATION_MS,
        elapsed
    );

    let ledger = app.world.resource::<GreatDiscoveryLedger>();
    assert_eq!(
        ledger.records().len(),
        DISCOVERY_COUNT,
        "all constellations should resolve in a single turn"
    );

    let telemetry = app.world.resource::<GreatDiscoveryTelemetry>();
    assert_eq!(
        telemetry.pending_candidates, DISCOVERY_COUNT as u32,
        "screening stage should evaluate every ready constellation"
    );
    assert_eq!(
        telemetry.active_constellations, DISCOVERY_COUNT as u32,
        "progress evaluation should flag every constellation as active during the resolving turn"
    );

    run_turn(&mut app);

    let telemetry = app.world.resource::<GreatDiscoveryTelemetry>();
    assert_eq!(
        telemetry.pending_candidates, 0,
        "candidate queue should be empty on the subsequent turn"
    );
    assert_eq!(
        telemetry.active_constellations, 0,
        "follow-up turn should report no active constellations"
    );
}

#[test]
fn gds_snapshot_stream_carries_resolved_records() {
    let mut app = build_headless_app();

    {
        let mut factions = app.world.resource_mut::<FactionRegistry>();
        factions.factions = vec![FactionId(0)];
    }

    {
        let mut observations = app.world.resource_mut::<ObservationLedger>();
        observations.set_observations(FactionId(0), KnowledgeField::Biology, 3);
    }

    let discovery_id = GreatDiscoveryId(41);
    let requirement_id = 2_048;
    {
        let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
        registry.register(GreatDiscoveryDefinition::new(
            discovery_id,
            "Biology Leap",
            KnowledgeField::Biology,
            vec![ConstellationRequirement::new(
                requirement_id,
                scalar_one(),
                scalar_zero(),
            )],
            0,
            0,
            None,
            0,
            false,
        ));
    }
    {
        let mut ledger = app.world.resource_mut::<DiscoveryProgressLedger>();
        ledger.add_progress(FactionId(0), requirement_id, scalar_one());
    }

    run_turn(&mut app);

    let history = app.world.resource::<SnapshotHistory>();
    let stored = history
        .latest_entry()
        .expect("Great Discovery snapshot should be captured");

    assert!(
        !stored.encoded_snapshot.is_empty(),
        "binary snapshot stream should not be empty"
    );
    assert!(
        !stored.encoded_snapshot_flat.is_empty(),
        "flatbuffer snapshot stream should not be empty"
    );

    let snapshot = stored.snapshot.as_ref();
    assert!(
        snapshot
            .great_discoveries
            .iter()
            .any(|state| state.id == discovery_id.0 && state.faction == 0),
        "snapshot should include the resolved discovery record"
    );
    assert!(
        snapshot
            .great_discovery_progress
            .iter()
            .all(|state| state.discovery != discovery_id.0),
        "resolved discoveries must be absent from progress list"
    );
    assert_eq!(
        snapshot.great_discovery_telemetry.total_resolved, 1,
        "telemetry summary should count resolved discoveries"
    );

    let delta = stored.delta.as_ref();
    assert!(
        delta
            .great_discoveries
            .iter()
            .any(|state| state.id == discovery_id.0 && state.faction == 0),
        "delta should surface new discovery entries"
    );
}

#[test]
fn gds_forced_publication_accelerates_trade_leaks() {
    let mut app = build_headless_app();

    {
        let mut factions = app.world.resource_mut::<FactionRegistry>();
        factions.factions = vec![FactionId(0), FactionId(1)];
    }

    {
        let mut observations = app.world.resource_mut::<ObservationLedger>();
        observations.set_observations(FactionId(0), KnowledgeField::Data, 2);
    }

    let discovery_id = GreatDiscoveryId(7);
    let requirement_id = 9_001;
    {
        let mut registry = app.world.resource_mut::<GreatDiscoveryRegistry>();
        registry.register(GreatDiscoveryDefinition::new(
            discovery_id,
            "Data Cascade",
            KnowledgeField::Data,
            vec![ConstellationRequirement::new(
                requirement_id,
                scalar_one(),
                scalar_zero(),
            )],
            0,
            0,
            None,
            FORCED_PUBLICATION_FLAG,
            false,
        ));
    }
    {
        let mut ledger = app.world.resource_mut::<DiscoveryProgressLedger>();
        ledger.add_progress(FactionId(0), requirement_id, scalar_one());
    }

    run_turn(&mut app);

    let resolved_record = {
        let ledger = app.world.resource::<GreatDiscoveryLedger>();
        ledger
            .records()
            .iter()
            .find(|record| record.id == discovery_id)
            .cloned()
            .expect("forced-publication discovery should be recorded")
    };
    assert!(
        resolved_record.publicly_deployed,
        "forced publication should mark the ledger entry as deployed"
    );

    {
        let mut query = app.world.query::<&mut TradeLink>();
        let mut trade = query
            .iter_mut(&mut app.world)
            .next()
            .expect("expected baseline trade link");
        trade.from_faction = FactionId(0);
        trade.to_faction = FactionId(1);
        trade.openness = scalar_one();
        trade.decay = scalar_zero();
        trade.leak_timer = 0;
        trade.pending_fragments = vec![KnowledgeFragment::new(
            resolved_record.id.0 as u32,
            scalar_from_f32(0.4),
            scalar_one(),
        )];
    }

    run_turn(&mut app);

    let discovery = app.world.resource::<DiscoveryProgressLedger>();
    let leaked_progress = discovery.get_progress(FactionId(1), resolved_record.id.0 as u32);
    assert!(
        leaked_progress > scalar_zero(),
        "knowledge diffusion should grant progress to the receiving faction"
    );

    let telemetry = app.world.resource::<TradeTelemetry>();
    assert!(
        telemetry
            .records
            .iter()
            .any(|entry| entry.to == FactionId(1)
                && entry.discovery_id == resolved_record.id.0 as u32),
        "trade telemetry should log the leaked discovery"
    );

    let leak_timer_after = {
        let mut query = app.world.query::<&TradeLink>();
        let trade = query
            .iter(&app.world)
            .next()
            .expect("trade link should still exist");
        trade.leak_timer
    };
    let config = app.world.resource::<SimulationConfig>();
    assert_eq!(
        leak_timer_after, config.trade_leak_min_ticks,
        "leak timer should reset according to the configured minimum after diffusion"
    );
}
