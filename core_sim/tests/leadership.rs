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

fn bank_practice(app: &mut App, band: Entity, practice: f64) {
    app.world.entity_mut(band).insert(BandLeadership {
        practice,
        leader: None,
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
    let banked = app.world.get::<BandLeadership>(led).unwrap().practice;

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
    assert_eq!(restored.practice, banked);
}

/// **A splinter band starts with no leader and no practice**, whatever its parent has.
#[test]
fn a_splinter_starts_with_no_leader_and_no_practice() {
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
    assert_eq!(
        app.world.get::<BandLeadership>(child),
        Some(&BandLeadership::default())
    );
}
