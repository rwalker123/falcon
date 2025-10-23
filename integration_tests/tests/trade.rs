use core_sim::{
    build_headless_app, restore_world_from_snapshot, scalar_from_f32, scalar_one, scalar_zero,
    DiscoveryProgressLedger, FactionId, FactionRegistry, KnowledgeFragment, PendingMigration,
    PopulationCohort, SnapshotHistory, TradeLink, TradeTelemetry,
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

#[test]
fn trade_pending_fragments_survive_snapshot_restore() {
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
        trade.pending_fragments = vec![
            KnowledgeFragment::new(11, scalar_from_f32(0.4), scalar_one()),
            KnowledgeFragment::new(12, scalar_from_f32(0.2), scalar_one()),
        ];
        trade.leak_timer = 5;
        trade.openness = scalar_one();
        trade.decay = scalar_zero();
    }

    app.update();

    let history = app.world.resource::<SnapshotHistory>();
    let stored = history.latest_entry().expect("snapshot captured");
    let snapshot = stored.snapshot.clone();
    assert!(
        snapshot
            .trade_links
            .iter()
            .any(|state| !state.pending_fragments.is_empty()),
        "Snapshot should retain pending trade fragments"
    );

    let mut restored_app = build_headless_app();
    restored_app.update();
    restore_world_from_snapshot(&mut restored_app.world, snapshot.as_ref());

    {
        let mut query = restored_app.world.query::<&TradeLink>();
        let trade = query
            .iter(&restored_app.world)
            .find(|link| link.from_faction == FactionId(0) && link.to_faction == FactionId(1))
            .expect("restored trade link");
        assert_eq!(trade.pending_fragments.len(), 2);
        assert_eq!(trade.pending_fragments[0].discovery_id, 11);
        assert_eq!(trade.pending_fragments[1].discovery_id, 12);
    }
}

#[test]
fn pending_migration_survives_snapshot_restore() {
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
        cohort.knowledge = vec![
            KnowledgeFragment::new(21, scalar_from_f32(0.5), scalar_one()),
            KnowledgeFragment::new(22, scalar_from_f32(0.3), scalar_one()),
        ];
        cohort.migration = Some(PendingMigration {
            destination: FactionId(1),
            eta: 2,
            fragments: vec![KnowledgeFragment::new(
                23,
                scalar_from_f32(0.4),
                scalar_one(),
            )],
        });
    }

    app.update();

    let history = app.world.resource::<SnapshotHistory>();
    let stored = history.latest_entry().expect("snapshot captured");
    let snapshot = stored.snapshot.clone();
    let migration_state = snapshot
        .populations
        .iter()
        .find_map(|state| state.migration.as_ref())
        .expect("migration persisted in snapshot");
    assert_eq!(migration_state.destination, 1);
    assert_eq!(migration_state.eta, 1);
    assert_eq!(migration_state.fragments.len(), 1);

    let mut restored_app = build_headless_app();
    restored_app.update();
    restore_world_from_snapshot(&mut restored_app.world, snapshot.as_ref());

    {
        let mut query = restored_app.world.query::<&PopulationCohort>();
        let cohort = query
            .iter(&restored_app.world)
            .find(|cohort| cohort.faction == FactionId(0))
            .expect("restored cohort present");
        let migration = cohort
            .migration
            .as_ref()
            .expect("pending migration restored");
        assert_eq!(migration.destination, FactionId(1));
        assert_eq!(migration.eta, 1);
        assert_eq!(migration.fragments.len(), 1);
        assert_eq!(migration.fragments[0].discovery_id, 23);
    }
}
