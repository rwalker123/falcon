//! **Roads form under the traffic that was already there, and a kept road pays**
//! (`docs/plan_standing_upkeep.md` §4.13, issue #532).
//!
//! These drive the two systems in **stage order** — `balance_supply_networks`, then
//! `routes::advance_routes` — because that ordering is load-bearing: the supply pass records the
//! links, the route pass spends them, and the payoff is therefore read at the *previous* turn's
//! standing. A fixture that ran them the other way round would measure a road reading itself.
//!
//! The harness mirrors `supply_network.rs`, which owns the pooling half of this fixture shape.

use std::sync::atomic::{AtomicU64, Ordering};

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    advance_routes, balance_supply_networks, scalar_zero, spawn_initial_world, BandId,
    ConnectionKey, ConnectionLedger, ConnectionsConfig, CultureManager, DiscoveryProgressLedger,
    FactionId, FactionInventory, GenerationId, GenerationRegistry, LadderConfig, LocalStore,
    MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, RouteLedger,
    RouteTrafficLog, RungKey, Scalar, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, SupplyNetworkConfigHandle, SupplyNetworkMembership, Tile,
    TileRegistry, FOOD, FULL_TIE,
};

const TEST_FACTION: FactionId = FactionId(7);
const BAND_POP: u32 = 100;
const TEST_BAND_ID_BASE: u64 = 9_500;
static NEXT_TEST_BAND_ID: AtomicU64 = AtomicU64::new(TEST_BAND_ID_BASE);

/// Two tiles apart, so the traced road is three tiles long and both camps stand on it — the
/// smallest fixture in which *"a band is served while standing on the road"* is not vacuous.
const CAMP_A: (u32, u32) = (10, 10);
const CAMP_B: (u32, u32) = (12, 10);

/// Comfortably more turns than the shipped `route:trail` price needs at this link's length, so the
/// climb is what is being measured rather than the loop bound.
const TURNS_TO_WEAR_A_TRAIL: u32 = 80;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));
    app.world
        .insert_resource(SupplyNetworkConfigHandle::default());
    app.world
        .insert_resource(SupplyNetworkMembership::default());
    app.world.insert_resource(ConnectionLedger::default());
    app.world.insert_resource(RouteLedger::default());
    app.world.insert_resource(RouteTrafficLog::default());
    app.world
        .insert_resource(core_sim::LadderConfigHandle::default());

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    app
}

fn spawn_band(app: &mut App, (x, y): (u32, u32), food: i64) -> Entity {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(x, y)
        .expect("tile coords resolve");
    let mut stores = LocalStore::new();
    stores.set(FOOD, Scalar::from_i64(food));
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: BAND_POP,
                children: scalar_zero(),
                working: Scalar::from_u32(BAND_POP),
                elders: scalar_zero(),
                stores,
                morale: scalar_zero(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: TEST_FACTION,
                knowledge: Vec::new(),
                migration: None,
            },
            ResidentBand,
            BandId(NEXT_TEST_BAND_ID.fetch_add(1, Ordering::Relaxed)),
        ))
        .id()
}

fn band_id(app: &App, band: Entity) -> BandId {
    *app.world
        .get::<BandId>(band)
        .expect("a test band has an id")
}

fn position_of(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .current_tile;
    app.world.get::<Tile>(tile).expect("a real tile").position
}

fn seed_directed_tie(app: &mut App, observer: Entity, subject: Entity) {
    const SEEDED_ON_TURN: u64 = 0;
    let key = ConnectionKey::new(band_id(app, observer), band_id(app, subject));
    let position = position_of(app, subject);
    let cfg = ConnectionsConfig::default();
    let contacts_to_full = (FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil() as u32;
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for _ in 0..contacts_to_full {
        ledger.record_contact(key, position, SEEDED_ON_TURN, SEEDED_ON_TURN, &cfg);
    }
}

fn seed_mutual_tie(app: &mut App, a: Entity, b: Entity) {
    seed_directed_tie(app, a, b);
    seed_directed_tie(app, b, a);
}

fn food_of(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

fn set_food(app: &mut App, band: Entity, food: i64) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .set(FOOD, Scalar::from_i64(food));
}

/// One turn, in stage order.
fn resolve_turn(app: &mut App) {
    app.world.run_system_once(balance_supply_networks);
    app.world.run_system_once(advance_routes);
}

/// A pair of neighbouring camps of one people who have met — the commonest arrangement in the game.
fn two_neighbouring_camps(app: &mut App) -> (Entity, Entity) {
    let a = spawn_band(app, CAMP_A, 100);
    let b = spawn_band(app, CAMP_B, 0);
    seed_mutual_tie(app, a, b);
    (a, b)
}

/// ⛔ **THE HEADLINE: NOBODY ORDERS A ROAD.** Two camps that have shared a larder for a few dozen
/// turns wear a trail between them, with no command typed and no crew staffed — #532's *"it must not
/// be the one case that produces no trail because nobody typed a command"*, and #215's origin one
/// step on: the second roads are the ones two neighbouring camps wore between them.
#[test]
fn two_camps_that_pool_wear_a_trail_between_them_with_nobody_ordering_it() {
    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);

    assert!(
        app.world.resource::<RouteLedger>().is_empty(),
        "precondition: no road exists before anybody walks anywhere"
    );

    for _ in 0..TURNS_TO_WEAR_A_TRAIL {
        resolve_turn(&mut app);
    }

    let ledger = app.world.resource::<RouteLedger>();
    assert_eq!(
        ledger.len(),
        1,
        "RULE 4 — the pair wears ONE road and every later turn widens it, rather than tracing a \
         near-duplicate every turn ({} roads after {TURNS_TO_WEAR_A_TRAIL} turns)",
        ledger.len()
    );

    let (id, route) = ledger.iter().next().expect("the road exists");
    assert_eq!(
        route.held_rung(),
        RungKey::RouteTrail,
        "the traffic climbed off the free game-trail floor and onto the first rung anyone pays for"
    );

    // **RULE 2, and the reason `trace_path` carries both ends**: the camps that wore it in are
    // standing on it, so the road serves them from the moment it exists.
    for camp in [position_of(&app, a), position_of(&app, b)] {
        assert!(
            ledger.routes_on_tile(camp).contains(&id),
            "the camp at {camp:?} stands on the road its own pooling wore in"
        );
    }
}

/// ⛔ **THE PAYOFF, AND IT IS PAIRED — an unrouted network must be untouched.** This is the whole of
/// *"no early-game regression, by construction"*: the same two camps, the same opening stores, run
/// once with no road and once with a kept trail. The routed run delivers strictly more, and the
/// unrouted run delivers exactly what it delivered before the route branch existed.
#[test]
fn a_kept_road_delivers_more_and_an_unrouted_network_is_untouched() {
    const OPENING_SURPLUS: i64 = 100;

    // ① The unrouted reading — the number the shipped game has always produced.
    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);
    app.world.run_system_once(balance_supply_networks);
    let unrouted_delivery = food_of(&app, b);
    assert!(
        unrouted_delivery > 0.0,
        "precondition: the pair really does pool ({unrouted_delivery})"
    );

    // ② The same opening, with a kept trail under both camps. Seated directly rather than worn in,
    //    so the measurement is about the payoff and not about the climb the test above owns.
    set_food(&mut app, a, OPENING_SURPLUS);
    set_food(&mut app, b, 0);
    let ladder = LadderConfig::builtin();
    let trail_cost = ladder
        .rung(RungKey::RouteTrail)
        .build
        .as_ref()
        .expect("the trail rung is built")
        .work_cost;
    let path = vec![position_of(&app, a), position_of(&app, b)];
    let mut routes = app.world.resource_mut::<RouteLedger>();
    let id = routes.insert(path, &ladder);
    routes
        .get_mut(id)
        .expect("the road was just laid")
        .set_position(trail_cost, &ladder);
    let road_is_kept = routes.get(id).expect("the road exists").grants_sight();
    assert!(
        road_is_kept,
        "precondition: the seated road is BUILT and its bill is met, which is what a payoff reads"
    );

    app.world.run_system_once(balance_supply_networks);
    let routed_delivery = food_of(&app, b);

    assert!(
        routed_delivery > unrouted_delivery,
        "a kept trail spills less in transit, so more of the same surplus arrives \
         (routed {routed_delivery} vs unrouted {unrouted_delivery})"
    );
}

/// **A road one band stands on carries nothing.** The payoff needs two of a component's bands on the
/// same road (rule 2) — a camp *beside* a road is not a link *over* one — so a road laid under one
/// of the pair changes nothing at all.
///
/// ⛔ **THIS IS A NEGATIVE CONTROL AND IT PASSES WITH THE WHOLE FEATURE RIPPED OUT** — *"nothing
/// changed"* is exactly what a dead payoff reports. It is only worth anything **paired with
/// `a_kept_road_delivers_more_and_an_unrouted_network_is_untouched`**, which fails the moment the
/// payoff stops being read. Measured: reverting the friction term in `balance_supply_networks` fails
/// that test and leaves this one green. Do not delete its twin.
#[test]
fn a_road_only_one_camp_stands_on_buys_that_network_nothing() {
    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);

    app.world.run_system_once(balance_supply_networks);
    let unrouted_delivery = food_of(&app, b);

    set_food(&mut app, a, 100);
    set_food(&mut app, b, 0);
    let ladder = LadderConfig::builtin();
    let trail_cost = ladder
        .rung(RungKey::RouteTrail)
        .build
        .as_ref()
        .expect("the trail rung is built")
        .work_cost;
    // A road running away from camp A, touching neither camp B nor anything between them.
    let lone = position_of(&app, a);
    let path = vec![
        lone,
        UVec2::new(lone.x, lone.y + 1),
        UVec2::new(lone.x, lone.y + 2),
    ];
    let mut routes = app.world.resource_mut::<RouteLedger>();
    let id = routes.insert(path, &ladder);
    routes
        .get_mut(id)
        .expect("the road was just laid")
        .set_position(trail_cost, &ladder);

    app.world.run_system_once(balance_supply_networks);

    assert!(
        (food_of(&app, b) - unrouted_delivery).abs() < 1.0e-3,
        "one camp on a road is a camp BESIDE a road — it carries none of this network's pooling \
         ({} vs the unrouted {unrouted_delivery})",
        food_of(&app, b)
    );
}
