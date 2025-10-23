use core_sim::{
    build_headless_app, scalar_from_f32, scalar_one, scalar_zero, DiscoveryProgressLedger,
    FactionId, FactionRegistry, KnowledgeFragment, PopulationCohort, TradeLink, TradeTelemetry,
};

#[test]
fn trade_diffusion_leaks_after_timer() {
    let mut app = build_headless_app();

    {
        let mut factions = app.world.resource_mut::<FactionRegistry>();
        factions.factions = vec![FactionId(0), FactionId(1)];
    }

    app.update();

    {
        let mut query = app.world.query::<&mut TradeLink>();
        let mut trade = query
            .iter_mut(&mut app.world)
            .next()
            .expect("trade link available");
        trade.from_faction = FactionId(0);
        trade.to_faction = FactionId(1);
        trade.pending_fragments = vec![KnowledgeFragment::new(
            99,
            scalar_from_f32(0.3),
            scalar_one(),
        )];
        trade.leak_timer = 1;
        trade.openness = scalar_one();
        trade.decay = scalar_zero();
    }

    app.update();

    let ledger = app.world.resource::<DiscoveryProgressLedger>();
    let progress = ledger.get_progress(FactionId(1), 99);
    assert!(
        progress > scalar_zero(),
        "Receiving faction should gain progress via trade leak"
    );

    let telemetry = app.world.resource::<TradeTelemetry>();
    assert_eq!(telemetry.tech_diffusion_applied, 1);
    assert!(
        telemetry
            .records
            .iter()
            .any(|record| record.discovery_id == 99 && !record.via_migration),
        "Telemetry should record trade diffusion event"
    );
}

#[test]
fn migration_seeding_transfers_knowledge() {
    let mut app = build_headless_app();

    {
        let mut factions = app.world.resource_mut::<FactionRegistry>();
        factions.factions = vec![FactionId(0), FactionId(1)];
    }

    app.update();

    {
        let mut query = app.world.query::<&mut PopulationCohort>();
        let mut cohort = query
            .iter_mut(&mut app.world)
            .next()
            .expect("population cohort available");
        cohort.faction = FactionId(0);
        cohort.morale = scalar_from_f32(0.9);
        cohort.knowledge = vec![KnowledgeFragment::new(
            7,
            scalar_from_f32(0.6),
            scalar_one(),
        )];
        cohort.migration = None;
    }

    app.update();

    {
        let mut query = app.world.query::<&PopulationCohort>();
        let cohort = query
            .iter(&app.world)
            .next()
            .expect("cohort present after first tick");
        let migration = cohort
            .migration
            .as_ref()
            .expect("migration should be scheduled after high morale");
        assert_eq!(migration.destination, FactionId(1));
        assert!(!migration.fragments.is_empty());
        assert!(migration.fragments[0].progress <= scalar_from_f32(0.6));
    }

    app.update();

    {
        let mut query = app.world.query::<&PopulationCohort>();
        let cohort = query
            .iter(&app.world)
            .next()
            .expect("cohort present after second tick");
        assert!(cohort.migration.is_none());
        assert_eq!(cohort.faction, FactionId(1));
    }

    let ledger = app.world.resource::<DiscoveryProgressLedger>();
    let progress = ledger.get_progress(FactionId(1), 7);
    assert!(
        progress > scalar_zero(),
        "Migration should seed knowledge in destination faction"
    );

    let telemetry = app.world.resource::<TradeTelemetry>();
    assert!(telemetry.migration_transfers >= 1);
}
