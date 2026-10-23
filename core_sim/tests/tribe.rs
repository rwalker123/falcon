//! **A tribe forms when linked bands share a gathering place** (#861,
//! `.claude/rules/core_sim/leadership.md`).
//!
//! Each fixture builds its bands directly (a clone of the world's first band, given an id, a size,
//! a belief anchor and a supply network) and runs `advance_tribes` on its own, so the grouping is
//! driven by exactly the three inputs the rule reads. The identity arithmetic is unit-pinned
//! beside `resolve_matches`.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_tribes, reconcile_band_culture_layers, BandId, BandName, BandSupplyMembership,
    CommandEventEntry, CommandEventKind, CommandEventLog, CultureManager, CultureOwner,
    CultureTraitAxis, PopulationCohort, ResidentBand, SimulationConfig, SimulationTick,
    SupplyNetworkMembership, TribeId, TribeLeaderKind, TribeRegistry, MIN_TRIBE_BANDS,
};

/// Test bands' ids start well clear of any worldgen band.
const TEST_BAND_BASE: u64 = 9_000;
/// The turn the fixtures run the tribe system at.
const FIXTURE_TURN: u64 = 20;
/// A reading clear of zero on either side.
const LEANING: f32 = 0.5;
/// Two distinct belief anchors.
const PLACE_A: UVec2 = UVec2::new(3, 3);
const PLACE_B: UVec2 = UVec2::new(8, 8);
/// Two distinct supply networks.
const NET_1: u32 = 1;
const NET_2: u32 = 2;

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

/// Spawn test band number `n`, sized `size`, anchored at `anchor`, in supply network `net`.
fn band(app: &mut App, n: u64, size: u32, anchor: Option<UVec2>, net: u32) -> Entity {
    let template = {
        let mut query = app
            .world
            .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
        let mut rows: Vec<_> = query.iter(&app.world).collect();
        rows.sort_by_key(|(id, _)| **id);
        rows.first().expect("the world has a band").1.clone()
    };
    let mut cohort = template;
    cohort.size = size;
    cohort.belief_anchor = anchor;
    let entity = app
        .world
        .spawn((
            cohort,
            ResidentBand,
            BandId(TEST_BAND_BASE + n),
            BandName(format!("Test{n}")),
        ))
        .id();
    link(app, entity, net);
    entity
}

fn link(app: &mut App, band: Entity, net: u32) {
    app.world
        .resource_mut::<SupplyNetworkMembership>()
        .restore_band(
            band,
            &BandSupplyMembership {
                network_id: net,
                ..BandSupplyMembership::default()
            },
        );
}

fn set_anchor(app: &mut App, band: Entity, anchor: Option<UVec2>) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .belief_anchor = anchor;
}

fn id(n: u64) -> BandId {
    BandId(TEST_BAND_BASE + n)
}

/// Give every test band its culture layer, then run the tribe system at `FIXTURE_TURN`.
fn run_tribes(app: &mut App) {
    app.world.run_system_once(reconcile_band_culture_layers);
    let turn = app.world.resource::<SimulationTick>().0.max(FIXTURE_TURN) + 1;
    app.world.resource_mut::<SimulationTick>().0 = turn;
    app.world.run_system_once(advance_tribes);
}

fn set_axis(app: &mut App, band: BandId, value: f32) {
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(band))
        .expect("the band has a culture layer");
    layer.traits.update_value(
        CultureTraitAxis::HierarchicalEgalitarian.index(),
        core_sim::scalar_from_f32(value),
    );
}

fn events(app: &App, kind: CommandEventKind) -> Vec<CommandEventEntry> {
    let turn = app.world.resource::<SimulationTick>().0;
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == kind && entry.tick == turn)
        .cloned()
        .collect()
}

fn registry(app: &App) -> &TribeRegistry {
    app.world.resource::<TribeRegistry>()
}

/// The bands of the one test tribe, by test number.
fn only_tribe_members(app: &App) -> Vec<BandId> {
    let tribes: Vec<_> = registry(app).iter().collect();
    assert_eq!(tribes.len(), 1, "exactly one tribe");
    tribes[0].members.iter().copied().collect()
}

/// A new turn: advance the fixture tick so the events of different runs can be told apart.
fn next_turn(app: &mut App, turn: u64) {
    app.world.resource_mut::<SimulationTick>().0 = turn;
    app.world.run_system_once(advance_tribes);
}

#[test]
fn two_linked_bands_on_one_place_form_a_tribe_with_a_leader_and_one_line() {
    let mut app = spawn_world();
    // The larger band's axis decides the kind; the larger band is 2.
    band(&mut app, 1, 40, Some(PLACE_A), NET_1);
    band(&mut app, 2, 90, Some(PLACE_A), NET_1);
    app.world.run_system_once(reconcile_band_culture_layers);
    set_axis(&mut app, id(2), -LEANING);
    run_tribes(&mut app);

    let tribe = registry(&app).tribe_of(id(1)).expect("a tribe formed");
    assert_eq!(tribe.members.len(), MIN_TRIBE_BANDS);
    assert!(tribe.members.contains(&id(2)));
    assert_eq!(tribe.name, "Test2 tribe", "named for the largest band");
    assert_eq!(tribe.leader.kind, TribeLeaderKind::ParamountChief);
    assert!(!tribe.leader.name.is_empty());
    let formed = events(&app, CommandEventKind::TribeFormed);
    assert_eq!(formed.len(), 1);
    assert_eq!(formed[0].band, id(2).0);
    let detail = formed[0].detail.as_deref().unwrap();
    assert!(detail.contains("kind=paramount_chief"), "{detail}");
    assert!(detail.contains("leader="), "{detail}");
    assert!(!detail.contains("name="), "{detail}");
    assert!(
        formed[0].label.starts_with("Test2 tribe forms; ")
            && formed[0].label.ends_with(" is paramount chief"),
        "{}",
        formed[0].label
    );
}

#[test]
fn nothing_forms_without_a_shared_place_a_network_or_an_anchor() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A), NET_1);
    band(&mut app, 2, 50, Some(PLACE_B), NET_1); // another place
    band(&mut app, 3, 50, Some(PLACE_A), 0); // no network
    band(&mut app, 4, 50, None, NET_1); // no anchor
    band(&mut app, 5, 50, Some(PLACE_A), NET_2); // another network
    run_tribes(&mut app);
    assert!(registry(&app).is_empty());
    assert!(events(&app, CommandEventKind::TribeFormed).is_empty());
}

#[test]
fn a_third_band_joins_and_a_broken_link_leaves() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A), NET_1);
    band(&mut app, 2, 50, Some(PLACE_A), NET_1);
    let third = band(&mut app, 3, 50, Some(PLACE_A), 0);
    run_tribes(&mut app);
    let before = registry(&app).tribe_of(id(1)).unwrap().id;
    assert_eq!(only_tribe_members(&app).len(), 2);

    link(&mut app, third, NET_1);
    run_tribes(&mut app);
    assert_eq!(
        registry(&app).tribe_of(id(3)).unwrap().id,
        before,
        "same tribe"
    );
    let joined = events(&app, CommandEventKind::TribeBandJoined);
    assert_eq!(joined.len(), 1);
    assert_eq!(
        joined[0].label,
        format!("Band {} joins the Test1 tribe", id(3).0)
    );
    assert!(joined[0]
        .detail
        .as_deref()
        .unwrap()
        .ends_with(&format!("band={}", id(3).0)));

    link(&mut app, third, NET_2);
    run_tribes(&mut app);
    assert!(registry(&app).tribe_of(id(3)).is_none());
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, before);
    let left = events(&app, CommandEventKind::TribeBandLeft);
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].label,
        format!("Band {} leaves the Test1 tribe", id(3).0)
    );
    assert!(left[0]
        .detail
        .as_deref()
        .unwrap()
        .ends_with(&format!("band={}", id(3).0)));
    assert_eq!(left[0].band, id(3).0);
}

#[test]
fn a_tribe_down_to_one_band_dissolves_and_its_leader_is_gone() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A), NET_1);
    let second = band(&mut app, 2, 80, Some(PLACE_A), NET_1);
    run_tribes(&mut app);
    let first_leader = registry(&app).tribe_of(id(1)).unwrap().leader.clone();

    set_anchor(&mut app, second, Some(PLACE_B));
    run_tribes(&mut app);
    assert!(registry(&app).is_empty());
    let dissolved = events(&app, CommandEventKind::TribeDissolved);
    assert_eq!(dissolved.len(), 1);
    assert_eq!(
        dissolved[0].band,
        id(2).0,
        "against the largest former band"
    );

    // A re-formed tribe is a new tribe with a new leader.
    set_anchor(&mut app, second, Some(PLACE_A));
    run_tribes(&mut app);
    let reformed = registry(&app).tribe_of(id(1)).unwrap();
    assert_ne!(reformed.id, TribeId(0));
    assert_ne!(reformed.leader.name, first_leader.name);
}

#[test]
fn a_network_split_keeps_the_tribe_on_the_majority_side_and_forms_a_new_one() {
    let mut app = spawn_world();
    let bands: Vec<Entity> = (1..=5)
        .map(|n| band(&mut app, n, 50, Some(PLACE_A), NET_1))
        .collect();
    run_tribes(&mut app);
    let original = registry(&app).tribe_of(id(1)).unwrap().id;

    link(&mut app, bands[3], NET_2);
    link(&mut app, bands[4], NET_2);
    run_tribes(&mut app);
    assert_eq!(registry(&app).len(), 2);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, original);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().members.len(), 3);
    let other = registry(&app).tribe_of(id(4)).unwrap();
    assert_ne!(other.id, original);
    assert_eq!(other.members.len(), 2);
    assert_eq!(events(&app, CommandEventKind::TribeFormed).len(), 1);
    assert_eq!(events(&app, CommandEventKind::TribeBandLeft).len(), 2);
}

#[test]
fn two_tribes_merging_leave_the_one_holding_more() {
    let mut app = spawn_world();
    for n in 1..=2 {
        band(&mut app, n, 50, Some(PLACE_A), NET_1);
    }
    let late: Vec<Entity> = (3..=5)
        .map(|n| band(&mut app, n, 50, Some(PLACE_A), NET_2))
        .collect();
    run_tribes(&mut app);
    let small = registry(&app).tribe_of(id(1)).unwrap().id;
    let big = registry(&app).tribe_of(id(3)).unwrap().id;
    assert_eq!(registry(&app).len(), 2);

    for entity in late {
        link(&mut app, entity, NET_1);
    }
    run_tribes(&mut app);
    assert_eq!(registry(&app).len(), 1);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, big);
    assert!(registry(&app).get(small).is_none());
    assert_eq!(events(&app, CommandEventKind::TribeDissolved).len(), 1);
    assert_eq!(events(&app, CommandEventKind::TribeBandJoined).len(), 2);
}

#[test]
fn the_leader_kind_is_the_largest_bands_and_fixed_at_formation() {
    let mut app = spawn_world();
    band(&mut app, 1, 90, Some(PLACE_A), NET_1);
    band(&mut app, 2, 40, Some(PLACE_A), NET_1);
    app.world.run_system_once(reconcile_band_culture_layers);
    set_axis(&mut app, id(1), LEANING);
    set_axis(&mut app, id(2), -LEANING); // the smaller band leans hierarchical: ignored
    run_tribes(&mut app);
    let leader = registry(&app).tribe_of(id(1)).unwrap().leader.clone();
    assert_eq!(leader.kind, TribeLeaderKind::CouncilOfChiefs);

    set_axis(&mut app, id(1), -LEANING);
    run_tribes(&mut app);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().leader, leader);

    // A tie in size goes to the lowest band id.
    let mut tie = spawn_world();
    band(&mut tie, 1, 60, Some(PLACE_A), NET_1);
    band(&mut tie, 2, 60, Some(PLACE_A), NET_1);
    tie.world.run_system_once(reconcile_band_culture_layers);
    set_axis(&mut tie, id(1), -LEANING);
    set_axis(&mut tie, id(2), LEANING);
    run_tribes(&mut tie);
    let tribe = registry(&tie).tribe_of(id(1)).unwrap();
    assert_eq!(tribe.name, "Test1 tribe");
    assert_eq!(tribe.leader.kind, TribeLeaderKind::ParamountChief);
}

#[test]
fn the_registry_round_trips_through_a_checkpoint() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A), NET_1);
    band(&mut app, 2, 50, Some(PLACE_A), NET_1);
    run_tribes(&mut app);
    let before = registry(&app).clone();
    assert_eq!(before.len(), 1);

    let checkpoint = capture_sim_state(&app.world);
    next_turn(&mut app, FIXTURE_TURN + 1);
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(registry(&app), &before);
}

#[test]
fn the_telling_tells_a_formation_once() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A), NET_1);
    band(&mut app, 2, 50, Some(PLACE_A), NET_1);
    run_tribes(&mut app);
    assert_eq!(registry(&app).len(), 1);

    let told = |app: &App| {
        app.world
            .resource::<CommandEventLog>()
            .iter()
            .filter(|entry| {
                entry.kind == CommandEventKind::NarrativeBeat
                    && entry
                        .detail
                        .as_deref()
                        .is_some_and(|d| d.contains("tribes.formed_this_turn=1"))
            })
            .count()
    };
    core_sim::run_turn(&mut app);
    assert_eq!(
        told(&app),
        1,
        "the beat fires on the turn after the formation count moves"
    );
    for _ in 0..3 {
        core_sim::run_turn(&mut app);
    }
    assert_eq!(told(&app), 1, "and not again");
}
