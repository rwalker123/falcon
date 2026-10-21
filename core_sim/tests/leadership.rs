//! **A chief emerges from directed work that succeeds** (#859, `.claude/rules/core_sim/leadership.md`).
//!
//! The emergence system, the kind rule, the permanence of the leader, the checkpoint and the
//! splinter. The two credit sources are pinned where their seams are driven: `build_queue.rs` for a
//! finished directed-work build and `denial_raid.rs` for a party's big-game kill; the unit-level
//! predicates live beside `BandLeadership` itself.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_leadership, BandId, BandLeader, BandLeadership, CommandEventKind, CommandEventLog,
    CultureManager, CultureOwner, CultureTraitAxis, LeaderKind, LeaderNameCatalogHandle,
    LeadershipConfigHandle, PopulationCohort, ResidentBand, SimulationConfig,
};

/// The turn the emergence system is run at in the fixtures that do not care.
const FIXTURE_TURN: u64 = 7;

/// A reading clear of zero on either side, for the kind-follows-the-sign pins.
const LEANING: f32 = 0.5;

fn spawn_world() -> App {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    core_sim::run_turn(&mut app);
    app
}

/// The first resident band: its entity and durable id.
fn a_band(app: &mut App) -> (Entity, BandId) {
    let mut query = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>();
    let mut bands: Vec<_> = query
        .iter(&app.world)
        .map(|(entity, id, _)| (entity, *id))
        .collect();
    bands.sort_by_key(|(_, id)| *id);
    bands.into_iter().next().expect("the world has a band")
}

fn threshold(app: &App) -> f64 {
    app.world
        .resource::<LeadershipConfigHandle>()
        .get()
        .practice_threshold
}

/// Pin the band's own `HierarchicalEgalitarian` reading.
fn set_axis(app: &mut App, band: BandId, value: f32) {
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(band))
        .expect("a band has a culture layer by its first turn");
    layer.traits.update_value(
        CultureTraitAxis::HierarchicalEgalitarian.index(),
        core_sim::scalar_from_f32(value),
    );
}

/// A stage index no band reaches, so the fixtures that pin the threshold are not also paid for the
/// settlement stage their band happens to stand in.
const NO_STAGE_CREDIT: u32 = u32::MAX;

fn bank_practice(app: &mut App, band: Entity, practice: f64) {
    app.world.entity_mut(band).insert(BandLeadership {
        from_builds: practice,
        stages_paid: NO_STAGE_CREDIT,
        ..BandLeadership::default()
    });
}

fn bank_the_threshold(app: &mut App, band: Entity) {
    let practice = threshold(app);
    bank_practice(app, band, practice);
}

fn leader_of(app: &App, band: Entity) -> Option<BandLeader> {
    app.world
        .get::<BandLeadership>(band)
        .and_then(|l| l.leader.clone())
}

fn emergence_lines(app: &App) -> Vec<core_sim::CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::LeaderEmerged)
        .cloned()
        .collect()
}

fn run_leadership(app: &mut App) {
    app.world.resource_mut::<core_sim::SimulationTick>().0 = FIXTURE_TURN;
    app.world.run_system_once(advance_leadership);
}

/// Every worldgen band carries the component, from the first turn.
#[test]
fn every_resident_band_is_spawned_with_a_leadership_record() {
    let mut app = spawn_world();
    let mut query = app
        .world
        .query_filtered::<Option<&BandLeadership>, With<ResidentBand>>();
    for leadership in query.iter(&app.world) {
        assert_eq!(leadership, Some(&BandLeadership::default()));
    }
}

/// **Below the threshold nothing happens; at it, exactly one leader, named and fed.**
#[test]
fn crossing_the_threshold_makes_exactly_one_named_leader() {
    let mut app = spawn_world();
    let (band, id) = a_band(&mut app);
    set_axis(&mut app, id, -LEANING);

    let short = threshold(&app) - 0.5;
    bank_practice(&mut app, band, short);
    run_leadership(&mut app);
    assert_eq!(leader_of(&app, band), None, "short of the threshold");
    assert!(emergence_lines(&app).is_empty());

    bank_the_threshold(&mut app, band);
    run_leadership(&mut app);
    let leader = leader_of(&app, band).expect("the threshold makes a leader");
    assert_eq!(leader.kind, LeaderKind::Chief);
    assert_eq!(leader.since_turn, FIXTURE_TURN);
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    assert_eq!(
        leader.name,
        app.world
            .resource::<LeaderNameCatalogHandle>()
            .catalog()
            .name_for_band(map_seed, id),
        "the name is the catalog's, deterministic in the map seed and the band"
    );

    let lines = emergence_lines(&app);
    assert_eq!(lines.len(), 1, "one feed line: {lines:?}");
    assert_eq!(lines[0].band, id.0, "filed under the band");
    assert_eq!(
        lines[0].detail.as_deref(),
        Some(format!("kind=chief name={}", leader.name).as_str())
    );
    assert!(
        lines[0].label.contains(&leader.name),
        "{:?}",
        lines[0].label
    );
}

/// **The kind follows the sign of the band's own axis, and zero goes to the council.**
#[test]
fn the_kind_follows_the_sign_and_zero_is_a_council() {
    for (value, expected) in [
        (-LEANING, LeaderKind::Chief),
        (0.0, LeaderKind::ElderCouncil),
        (LEANING, LeaderKind::ElderCouncil),
    ] {
        let mut app = spawn_world();
        let (band, id) = a_band(&mut app);
        set_axis(&mut app, id, value);
        bank_the_threshold(&mut app, band);
        run_leadership(&mut app);
        let leader = leader_of(&app, band).expect("a leader emerged");
        assert_eq!(leader.kind, expected, "axis {value}");
        let lines = emergence_lines(&app);
        assert!(
            lines[0].label.contains(&leader.name),
            "a council names its speaker too: {:?}",
            lines[0].label
        );
    }
}

/// **A leader is made once and never re-made; the axis drifting later does not flip the kind.**
#[test]
fn a_leader_is_permanent() {
    let mut app = spawn_world();
    let (band, id) = a_band(&mut app);
    set_axis(&mut app, id, -LEANING);
    bank_the_threshold(&mut app, band);
    run_leadership(&mut app);
    let first = leader_of(&app, band).expect("a leader emerged");

    set_axis(&mut app, id, LEANING);
    for _ in 0..3 {
        run_leadership(&mut app);
    }
    assert_eq!(
        leader_of(&app, band),
        Some(first),
        "never re-made or flipped"
    );
    assert_eq!(emergence_lines(&app).len(), 1, "announced once");
}

/// **The leader and the practice survive a checkpoint** -- a rollback that dropped a leader would
/// let the band emerge a second one, under a different name.
#[test]
fn a_checkpoint_round_trip_keeps_the_leader_and_the_practice() {
    let mut app = spawn_world();
    let (led, led_id) = a_band(&mut app);
    set_axis(&mut app, led_id, -LEANING);
    bank_the_threshold(&mut app, led);
    run_leadership(&mut app);
    let leader = leader_of(&app, led).expect("a leader emerged");
    let banked = app.world.get::<BandLeadership>(led).unwrap().practice();

    let checkpoint = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &checkpoint);

    let restored = {
        let mut query = app.world.query::<(&BandId, &BandLeadership)>();
        query
            .iter(&app.world)
            .find(|(id, _)| **id == led_id)
            .map(|(_, leadership)| leadership.clone())
            .expect("the band is restored")
    };
    assert_eq!(restored.leader, Some(leader));
    assert_eq!(restored.practice(), banked);
}

/// **A splinter band starts with no leader and no practice**, whatever its parent has.
#[test]
fn a_splinter_starts_with_no_leader_and_none_of_its_parents_practice() {
    let mut app = spawn_world();
    let (parent, parent_id) = a_band(&mut app);
    set_axis(&mut app, parent_id, -LEANING);
    bank_the_threshold(&mut app, parent);
    run_leadership(&mut app);
    assert!(
        leader_of(&app, parent).is_some(),
        "fixture: the parent leads"
    );

    let settle = core_sim::SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = core_sim::split_band_from_parent(&mut app.world, parent, 6, &settle)
        .expect("the split is admitted");
    let child = {
        let mut query = app.world.query::<(Entity, &BandId)>();
        query
            .iter(&app.world)
            .find(|(_, id)| **id == split.band)
            .map(|(entity, _)| entity)
            .expect("the splinter exists")
    };
    let record = app.world.get::<BandLeadership>(child).unwrap();
    assert_eq!(record.leader, None, "the splinter starts with no leader");
    assert_eq!(
        record.practice(),
        record.from_splits,
        "and only the split's own credit, nothing of its parent's"
    );
}

/// **A COMMANDED split credits both bands once; a breeding break-off credits nobody.**
#[test]
fn a_commanded_split_credits_both_bands_once() {
    let mut app = spawn_world();
    let (parent, _) = a_band(&mut app);
    let quantum = app
        .world
        .resource::<LeadershipConfigHandle>()
        .get()
        .practice_per_split;
    let before = app.world.get::<BandLeadership>(parent).unwrap().from_splits;

    let settle = core_sim::SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = core_sim::split_band_from_parent(&mut app.world, parent, 6, &settle)
        .expect("the split is admitted");
    let child = band_entity(&mut app, split.band);

    assert_eq!(
        app.world.get::<BandLeadership>(parent).unwrap().from_splits,
        before + quantum,
        "the parent is credited once"
    );
    let child_record = app.world.get::<BandLeadership>(child).unwrap();
    assert_eq!(child_record.from_splits, quantum, "so is the new band");
    assert_eq!(child_record.leader, None, "and it starts with no leader");
}

/// The breeding break-off and the independence break-away run the shared body, not the command.
#[test]
fn a_split_nobody_ordered_credits_nothing() {
    let mut app = spawn_world();
    let (parent, _) = a_band(&mut app);
    let settle = core_sim::SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = core_sim::split_band_from_parent_with(
        &mut app.world,
        parent,
        6,
        &settle,
        core_sim::LineDivision::Partition,
    )
    .expect("the split is admitted");
    let child = band_entity(&mut app, split.band);
    assert_eq!(
        app.world.get::<BandLeadership>(parent).unwrap().from_splits,
        0.0
    );
    assert_eq!(
        app.world.get::<BandLeadership>(child).unwrap().from_splits,
        0.0
    );
}

fn band_entity(app: &mut App, id: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, band)| **band == id)
        .map(|(entity, _)| entity)
        .expect("the band exists")
}

/// **Herd-following hexes credit, per hex, and nothing else the band does on the move does.**
#[test]
fn hexes_walked_following_a_herd_credit_per_hex() {
    let mut app = spawn_world();
    let (band, id) = a_band(&mut app);
    bank_practice(&mut app, band, 0.0);
    app.world
        .entity_mut(band)
        .get_mut::<BandLeadership>()
        .unwrap()
        .stages_paid = NO_STAGE_CREDIT;
    let per_hex = app
        .world
        .resource::<LeadershipConfigHandle>()
        .get()
        .practice_per_herd_hex;
    run_leadership(&mut app);
    assert_eq!(
        app.world.get::<BandLeadership>(band).unwrap().from_herd,
        0.0,
        "a band that followed no herd earns nothing (a plain move order records no steps)"
    );

    const HEXES: u32 = 3;
    app.world
        .resource_mut::<core_sim::HerdFollowSteps>()
        .record(id, HEXES);
    run_leadership(&mut app);
    assert_eq!(
        app.world.get::<BandLeadership>(band).unwrap().from_herd,
        f64::from(HEXES) * per_hex
    );
}

/// **A settlement stage pays once: reaching it, not standing in it.**
#[test]
fn a_settlement_stage_pays_once_and_a_shrink_and_regrow_does_not_repay() {
    let mut app = spawn_world();
    let (band, _) = a_band(&mut app);
    app.world.entity_mut(band).insert(BandLeadership::default());
    let per_stage = app
        .world
        .resource::<LeadershipConfigHandle>()
        .get()
        .practice_per_stage_reached;
    let stages = app
        .world
        .resource::<core_sim::SettlementStageConfigHandle>()
        .get();
    let size_of = |index: usize| stages.stages[index].criteria.min_size;
    let set_size = |app: &mut App, size: u32| {
        app.world.get_mut::<PopulationCohort>(band).unwrap().size = size;
    };
    let paid = |app: &App| app.world.get::<BandLeadership>(band).unwrap().from_stages;

    set_size(&mut app, 0);
    run_leadership(&mut app);
    assert_eq!(paid(&app), 0.0, "the first stage is never paid");

    set_size(&mut app, size_of(1));
    run_leadership(&mut app);
    assert_eq!(paid(&app), per_stage, "reaching the second stage pays");

    set_size(&mut app, 0);
    run_leadership(&mut app);
    set_size(&mut app, size_of(1));
    run_leadership(&mut app);
    assert_eq!(paid(&app), per_stage, "regrowing into it pays nothing more");

    set_size(&mut app, size_of(2));
    run_leadership(&mut app);
    assert_eq!(
        paid(&app),
        2.0 * per_stage,
        "the third stage pays once more"
    );
}

/// **A leader freezes the bar**, so the figure the client draws stops where it stood.
#[test]
fn practice_is_frozen_once_there_is_a_leader() {
    let mut app = spawn_world();
    let (band, id) = a_band(&mut app);
    set_axis(&mut app, id, -LEANING);
    bank_the_threshold(&mut app, band);
    run_leadership(&mut app);
    let frozen = app.world.get::<BandLeadership>(band).unwrap().clone();
    assert!(frozen.leader.is_some());

    app.world
        .resource_mut::<core_sim::HerdFollowSteps>()
        .record(id, 5);
    run_leadership(&mut app);
    assert_eq!(app.world.get::<BandLeadership>(band), Some(&frozen));
}

/// **The per-source fields on the ENCODED row sum to `leadershipPractice`**, and the threshold is
/// echoed -- what a split bar draws is read off the buffer the client reads.
#[test]
fn the_per_source_fields_sum_to_the_practice_on_the_encoded_row() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = spawn_world();
    let (band, id) = a_band(&mut app);
    app.world.entity_mut(band).insert(BandLeadership {
        from_builds: 3.0,
        from_splits: 1.5,
        from_herd: 2.0,
        from_stages: 0.5,
        stages_paid: NO_STAGE_CREDIT,
        leader: None,
    });
    core_sim::recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published");
    let row = rows
        .iter()
        .find(|row| row.bandId() == id.0)
        .expect("the band is on the wire");
    assert_eq!(
        row.leadershipFromBuilds()
            + row.leadershipFromSplits()
            + row.leadershipFromHerd()
            + row.leadershipFromStages(),
        row.leadershipPractice()
    );
    assert_eq!(row.leadershipPractice(), 7.0);
    assert_eq!(row.leadershipThreshold(), threshold(&app) as f32);
}
