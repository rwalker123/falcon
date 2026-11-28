//! **A settlement is derived, never founded** (#176, `.claude/rules/core_sim/campaign.md` ->
//! "Settlements: derived clusters").
//!
//! Each fixture builds bands directly (clones of the world's first band, given an id, a size and a
//! belief anchor) and drives `advance_tribes` and `advance_settlements` on their own, one call per
//! turn, so the stage is read off exactly the inputs the rule names: people, the store rung a member
//! owns, the food stored, and the belief on the anchor.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_settlements, advance_tribes, reconcile_band_culture_layers, scalar_from_f32,
    tile_rung_span, BandId, BandLeader, BandLeadership, BandName, BeliefRegistry, BranchId,
    CommandEventEntry, CommandEventKind, CommandEventLog, ConnectionKey, ConnectionLedger,
    CultureManager, CultureOwner, FactionId, FoundingLines, LadderConfigHandle, LeaderKind,
    LeadershipConfigHandle, PopulationCohort, ResidentBand, RungId, SettlementStageConfigHandle,
    SettlementStages, Sighting, SimulationConfig, SimulationTick, TileImprovementRegistry,
    TribeRegistry, CULTURE_TRAIT_AXES,
};

/// Test bands' ids start well clear of any worldgen band.
const TEST_BAND_BASE: u64 = 9_000;
/// The turn the fixtures start counting from.
const FIXTURE_TURN: u64 = 20;
/// The shared belief anchor.
const PLACE: UVec2 = UVec2::new(3, 3);
/// The tile the stores stand on.
const STORE_TILE: UVec2 = UVec2::new(5, 5);
/// Turns a chiefed kin pair needs to fill its bank (100 points at 8 a turn).
const TURNS_TO_CLOSE_A_KIN_PAIR: usize = 13;
/// More belief than any shipped belief criterion asks.
const A_GREAT_URN: f32 = 1.0e6;
/// More food than any shipped food criterion asks.
const A_FULL_GRANARY: f32 = 2_000.0;
/// A people large enough for the shipped city row.
const CITY_PEOPLE: u32 = 1_000;

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

fn id(n: u64) -> BandId {
    BandId(TEST_BAND_BASE + n)
}

fn band(app: &mut App, n: u64, size: u32, origin: u64) -> Entity {
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
    cohort.belief_anchor = Some(PLACE);
    cohort.founding_lines = FoundingLines::founded(id(origin), 2);
    let entity = app
        .world
        .spawn((
            cohort,
            ResidentBand,
            id(n),
            BandName(format!("Test{n}")),
            BandLeadership::default(),
        ))
        .id();
    app.world.run_system_once(reconcile_band_culture_layers);
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(id(n)))
        .expect("the band has a culture layer");
    for axis in 0..CULTURE_TRAIT_AXES {
        layer
            .traits
            .update_value(axis, core_sim::scalar_from_f32(0.0));
    }
    entity
}

fn crown(app: &mut App, band: Entity, name: &str) {
    app.world.get_mut::<BandLeadership>(band).unwrap().leader = Some(BandLeader {
        name: name.to_string(),
        kind: LeaderKind::Chief,
        since_turn: 1,
    });
}

fn tie_all(app: &mut App, ns: &[u64]) {
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for a in ns {
        for b in ns {
            if a != b {
                ledger.insert_full_tie(
                    ConnectionKey::new(id(*a), id(*b)),
                    &Sighting::new(UVec2::ZERO, 1, "", FactionId(0)),
                    1,
                );
            }
        }
    }
}

fn faction_of(app: &App, band: Entity) -> FactionId {
    app.world.get::<PopulationCohort>(band).unwrap().faction
}

fn tick(app: &mut App) {
    let next = app.world.resource::<SimulationTick>().0.max(FIXTURE_TURN) + 1;
    app.world.resource_mut::<SimulationTick>().0 = next;
}

/// One turn of the two systems under test, tribes first.
fn turn(app: &mut App) {
    tick(app);
    app.world.run_system_once(advance_tribes);
    app.world.run_system_once(advance_settlements);
}

fn settle(app: &mut App) {
    tick(app);
    app.world.run_system_once(advance_settlements);
}

fn stage_id_of(app: &App, index: usize) -> String {
    app.world
        .resource::<SettlementStageConfigHandle>()
        .get()
        .stages[index]
        .id
        .clone()
}

fn band_stage(app: &App, n: u64) -> String {
    let index = app
        .world
        .resource::<SettlementStages>()
        .of_band(id(n))
        .expect("the band was resolved");
    stage_id_of(app, index)
}

fn tribe_stage(app: &App) -> String {
    let tribe = app
        .world
        .resource::<TribeRegistry>()
        .iter()
        .next()
        .expect("a tribe formed")
        .id;
    let index = app
        .world
        .resource::<SettlementStages>()
        .of_tribe(tribe)
        .expect("the tribe was resolved");
    stage_id_of(app, index)
}

fn reached(app: &App) -> Vec<CommandEventEntry> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::SettlementStageReached)
        .cloned()
        .collect()
}

fn food_stores() -> BranchId {
    BranchId::new("food_stores")
}

/// A finished store at `rung` on [`STORE_TILE`], owned by `owner` of `faction`, holding `food`.
fn keep_store(app: &mut App, owner: BandId, faction: FactionId, rung: &str, food: f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (base, width) = tile_rung_span(RungId::new(food_stores(), rung), &ladder);
    let mut registry = app.world.resource_mut::<TileImprovementRegistry>();
    let store = registry.place_owned(STORE_TILE, food_stores(), faction, Some(owner), &ladder);
    store.set_position(base + width, &ladder);
    store.contents = Default::default();
    store.contents.add_aged("dry", scalar_from_f32(food), 0);
}

/// A lone band of `size` with a granary of `food` and `belief` on its anchor.
fn city_grade_band(size: u32, food: f32, belief: f32) -> (App, Entity) {
    let mut app = spawn_world();
    let entity = band(&mut app, 1, size, 1);
    let faction = faction_of(&app, entity);
    keep_store(&mut app, id(1), faction, "granary", food);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, belief);
    (app, entity)
}

#[test]
fn a_lone_band_is_its_own_settlement_and_resolves_from_its_people() {
    let mut app = spawn_world();
    band(&mut app, 1, 30, 1);
    band(&mut app, 2, 60, 2);
    band(&mut app, 3, 120, 3);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "nomadic");
    assert_eq!(band_stage(&app, 2), "camp");
    assert_eq!(band_stage(&app, 3), "village");
    assert!(
        app.world.resource::<TribeRegistry>().is_empty(),
        "fixture: nobody is in a tribe"
    );
}

#[test]
fn a_tribe_is_one_settlement_summing_its_members_and_each_member_keeps_its_own_stage() {
    let mut app = spawn_world();
    let one = band(&mut app, 1, 40, 1);
    band(&mut app, 2, 90, 1);
    crown(&mut app, one, "Chief1");
    tie_all(&mut app, &[1, 2]);
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR {
        turn(&mut app);
    }
    assert_eq!(app.world.resource::<TribeRegistry>().len(), 1);
    // 40 + 90 = 130 people: a village, though neither band alone is one.
    assert_eq!(tribe_stage(&app), "village");
    assert_eq!(band_stage(&app, 1), "nomadic", "40 people alone");
    assert_eq!(band_stage(&app, 2), "camp", "90 people alone");
}

#[test]
fn the_store_rung_food_and_belief_each_gate_the_city() {
    let (mut app, _) = city_grade_band(CITY_PEOPLE, A_FULL_GRANARY, A_GREAT_URN);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "city");

    // Without belief: a town (people, a store and food all hold).
    let (mut app, _) = city_grade_band(CITY_PEOPLE, A_FULL_GRANARY, 0.0);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "town", "no belief on the anchor");

    // Without the food stored: a town.
    let (mut app, entity) = city_grade_band(CITY_PEOPLE, 0.0, A_GREAT_URN);
    app.world
        .get_mut::<PopulationCohort>(entity)
        .unwrap()
        .stores = core_sim::LocalStore::new();
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "town", "no food stored");

    // A pit is a store but not a granary: a town, not a city.
    let mut app = spawn_world();
    let entity = band(&mut app, 1, CITY_PEOPLE, 1);
    let faction = faction_of(&app, entity);
    keep_store(&mut app, id(1), faction, "storage_pit", A_FULL_GRANARY);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, A_GREAT_URN);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "town", "a pit is not a granary");

    // No store at all: a village however many.
    let mut app = spawn_world();
    band(&mut app, 1, CITY_PEOPLE, 1);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, A_GREAT_URN);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "village", "no store");
}

#[test]
fn a_store_owned_by_another_band_is_not_this_bands_store() {
    let mut app = spawn_world();
    let entity = band(&mut app, 1, CITY_PEOPLE, 1);
    band(&mut app, 2, 30, 2);
    let faction = faction_of(&app, entity);
    keep_store(&mut app, id(2), faction, "granary", A_FULL_GRANARY);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, A_GREAT_URN);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "village", "band 2 owns the granary");
    assert_eq!(band_stage(&app, 2), "nomadic");
}

#[test]
fn reaching_the_city_is_told_once_per_rising_crossing() {
    let (mut app, entity) = city_grade_band(CITY_PEOPLE, A_FULL_GRANARY, A_GREAT_URN);
    // A town does not announce: shrink under the city row first.
    app.world.get_mut::<PopulationCohort>(entity).unwrap().size = 300;
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "town");
    assert!(reached(&app).is_empty(), "a town is not announced");

    // The rising crossing fires once...
    app.world.get_mut::<PopulationCohort>(entity).unwrap().size = CITY_PEOPLE;
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "city");
    let events = reached(&app);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(
        events[0].detail.as_deref(),
        Some(format!("stage=city band={}", id(1).0).as_str())
    );
    assert!(
        events[0].label.contains("becomes a city"),
        "{:?}",
        events[0]
    );

    // ...and not again while it stays a city.
    for _ in 0..3 {
        settle(&mut app);
    }
    assert_eq!(
        reached(&app).len(),
        1,
        "standing in the city is not reaching it"
    );

    // Falling back and rising again is a new crossing.
    app.world.get_mut::<PopulationCohort>(entity).unwrap().size = 300;
    settle(&mut app);
    app.world.get_mut::<PopulationCohort>(entity).unwrap().size = CITY_PEOPLE;
    settle(&mut app);
    assert_eq!(reached(&app).len(), 2);
}

#[test]
fn a_tribe_reaching_the_city_is_told_once_against_the_tribe() {
    let mut app = spawn_world();
    let one = band(&mut app, 1, 40, 1);
    band(&mut app, 2, 40, 1);
    crown(&mut app, one, "Chief1");
    tie_all(&mut app, &[1, 2]);
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR {
        turn(&mut app);
    }
    assert_eq!(app.world.resource::<TribeRegistry>().len(), 1);
    assert!(reached(&app).is_empty());

    let faction = faction_of(&app, one);
    keep_store(&mut app, id(1), faction, "granary", A_FULL_GRANARY);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, A_GREAT_URN);
    for band in [1, 2] {
        let entity = app_band(&mut app, band);
        app.world.get_mut::<PopulationCohort>(entity).unwrap().size = CITY_PEOPLE / 2;
    }
    turn(&mut app);
    assert_eq!(tribe_stage(&app), "city");
    let events = reached(&app);
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(
        events[0]
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("tribe=")),
        "{:?}",
        events[0]
    );
    assert!(events[0].label.contains("tribe becomes a city"));
    // Neither member alone is a city, so a lone-band line cannot fire beside it.
    for _ in 0..3 {
        turn(&mut app);
    }
    assert_eq!(reached(&app).len(), 1);
}

fn app_band(app: &mut App, n: u64) -> Entity {
    let mut query = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>();
    query
        .iter(&app.world)
        .find(|(_, band)| **band == id(n))
        .map(|(entity, _)| entity)
        .expect("the band")
}

#[test]
fn a_village_tribe_pulls_a_neighbour_but_a_pair_with_no_chief_still_banks_nothing() {
    let mut app = spawn_world();
    let one = band(&mut app, 1, 60, 1);
    band(&mut app, 2, 60, 1);
    band(&mut app, 3, 30, 3);
    crown(&mut app, one, "Chief1");
    tie_all(&mut app, &[1, 2, 3]);
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR {
        turn(&mut app);
    }
    assert_eq!(app.world.resource::<TribeRegistry>().len(), 1);
    assert_eq!(tribe_stage(&app), "village");
    // One more turn so `advance_tribes` reads the stage the last turn resolved.
    turn(&mut app);

    let config = app.world.resource::<LeadershipConfigHandle>().get();
    let stages = app.world.resource::<SettlementStageConfigHandle>().get();
    let village = stages.pull_from_index();
    let pull = stages.pull_strength(village);
    assert!(pull > 0.0, "a village pulls");

    let registry = app.world.resource::<TribeRegistry>();
    let pair_of = |a: u64, b: u64| {
        registry
            .pairs_of(id(a))
            .find(|(partner, _)| *partner == id(b))
            .map(|(_, inputs)| *inputs)
            .expect("the pair is local")
    };
    // The tribe's own pair and the pairs reaching toward the outsider carry the pull.
    assert!((pair_of(1, 3).settlement - pull).abs() < 1e-9);
    assert!((pair_of(2, 3).settlement - pull).abs() < 1e-9);
    assert!(
        core_sim::points_per_turn(&config, &pair_of(1, 3)) >= config.tribe_weight_settlement * pull,
        "the pull is part of the rate"
    );
    // A chiefed side banks...
    assert!(registry.points(id(1), id(3)) > 0.0);
    // ...and band 2 and 3 have no chief between them, so the pull banks nothing for that pair.
    assert_eq!(registry.points(id(2), id(3)), 0.0);
}

#[test]
fn a_band_below_the_pull_stage_pulls_nothing() {
    let mut app = spawn_world();
    let one = band(&mut app, 1, 30, 1);
    band(&mut app, 2, 30, 1);
    band(&mut app, 3, 30, 3);
    crown(&mut app, one, "Chief1");
    tie_all(&mut app, &[1, 2, 3]);
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR {
        turn(&mut app);
    }
    assert_eq!(app.world.resource::<TribeRegistry>().len(), 1);
    assert_eq!(tribe_stage(&app), "camp", "60 people");
    turn(&mut app);
    let registry = app.world.resource::<TribeRegistry>();
    let (_, inputs) = registry
        .pairs_of(id(1))
        .find(|(partner, _)| *partner == id(3))
        .expect("local");
    assert_eq!(inputs.settlement, 0.0);
}

#[test]
fn the_stages_ride_the_checkpoint() {
    let (mut app, _) = city_grade_band(CITY_PEOPLE, A_FULL_GRANARY, A_GREAT_URN);
    settle(&mut app);
    let before = app.world.resource::<SettlementStages>().clone();
    assert_eq!(band_stage(&app, 1), "city");
    let checkpoint = capture_sim_state(&app.world);
    app.world.insert_resource(SettlementStages::default());
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(app.world.resource::<SettlementStages>(), &before);
    // The edge gate survived: a restored city does not announce itself again.
    let announced = reached(&app).len();
    settle(&mut app);
    assert_eq!(reached(&app).len(), announced);
}

/// A chiefed, tie-joined pair of bands (ids 1 and 2) closed into one tribe over the bank's fill.
fn form_a_tribe(app: &mut App, one: Entity) {
    crown(app, one, "Chief1");
    tie_all(app, &[1, 2]);
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR {
        turn(app);
    }
    assert_eq!(app.world.resource::<TribeRegistry>().len(), 1);
}

#[test]
fn a_new_tribe_at_its_members_stage_does_not_announce_it_again() {
    let (mut app, one) = city_grade_band(CITY_PEOPLE, A_FULL_GRANARY, A_GREAT_URN);
    band(&mut app, 2, 30, 1);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "city");
    assert_eq!(reached(&app).len(), 1, "the lone band announced its city");

    form_a_tribe(&mut app, one);
    assert_eq!(
        tribe_stage(&app),
        "city",
        "the tribe stands where its member did"
    );
    assert_eq!(
        reached(&app).len(),
        1,
        "a city already announced is not announced again for the tribe: {:?}",
        reached(&app)
    );
}

#[test]
fn a_new_tribe_above_its_members_stage_announces_the_climb() {
    let mut app = spawn_world();
    let one = band(&mut app, 1, CITY_PEOPLE / 2, 1);
    band(&mut app, 2, CITY_PEOPLE / 2, 1);
    let faction = faction_of(&app, one);
    keep_store(&mut app, id(1), faction, "granary", A_FULL_GRANARY);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE, A_GREAT_URN);
    settle(&mut app);
    assert_eq!(band_stage(&app, 1), "town");
    assert!(reached(&app).is_empty(), "neither band is a city");

    form_a_tribe(&mut app, one);
    assert_eq!(tribe_stage(&app), "city", "together they are a city");
    let events = reached(&app);
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(events[0].detail.as_deref().unwrap().contains("tribe="));
}
