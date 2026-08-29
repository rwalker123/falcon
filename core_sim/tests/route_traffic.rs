//! **Roads form under the traffic that was already there, and a kept road pays**
//! (`docs/plan_standing_upkeep.md` §4.13, issue #532).
//!
//! These drive the turn's three route systems in **stage order** — `balance_supply_networks` and
//! `routes::advance_routes` in Logistics, then `settle_route_keeping` in Population — because that
//! ordering is load-bearing twice over:
//!
//! - the supply pass records the links and the route pass spends them, so the payoff is read at the
//!   *previous* turn's standing. A fixture that ran those two the other way round would measure a
//!   road reading itself.
//! - the keeping pass stamps and pays **after** the decay pass has judged and cleared, so the
//!   supply `advance_routes` weighs was stamped by **last** turn's Population — the one-turn carry
//!   `forage::advance_cultivation` and `fauna::advance_husbandry` already run on. **A test that
//!   expects a decay on the turn the keepers left is wrong about the arrangement, not the code.**
//!
//! The harness mirrors `supply_network.rs`, which owns the pooling half of this fixture shape.

use std::sync::atomic::{AtomicU64, Ordering};

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    advance_routes, balance_supply_networks, route_at_risk_rung, route_upkeep_demand, scalar_zero,
    settle_route_keeping, span_of_terrains, spawn_initial_world, BandId, ConnectionKey,
    ConnectionLedger, ConnectionsConfig, CultureManager, DiscoveryProgressLedger,
    EquipmentConfigHandle, FactionId, FactionInventory, GenerationId, GenerationRegistry,
    LaborAllocation, LaborTarget, LadderConfig, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, Route, RouteId, RouteLedger,
    RouteTrafficLog, RungKey, Scalar, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, SupplyNetworkConfigHandle, SupplyNetworkMembership, Tile,
    TileRegistry, FOOD, FULL_TIE, NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT, WHOLLY_UNSUPPLIED,
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
    app.world.insert_resource(LadderConfigHandle::default());
    // The keeping pass resolves the road keepers' kit off the roster. The shipped
    // `default_kits.roadwork` is the bare `none` kit, so they work bare — intended, not a gap.
    app.world.insert_resource(EquipmentConfigHandle::default());

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

/// One turn, in stage order: the two Logistics passes, then the Population one.
fn resolve_turn(app: &mut App) {
    app.world.run_system_once(balance_supply_networks);
    app.world.run_system_once(advance_routes);
    app.world.run_system_once(settle_route_keeping);
}

fn resolve_turns(app: &mut App, turns: u32) {
    for _ in 0..turns {
        resolve_turn(app);
    }
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

// ---------------------------------------------------------------------------------------------
// The `Roadwork` keeping pool, and the decay it pays for.
//
// Every fixture below seats a road by hand at the **trail** rung rather than wearing one in, so the
// measurement is about the keeping and not about the climb the first test in this file owns. A road
// seated under a lone camp earns no traffic at all (there is nobody to pool with), which is what
// makes the position a clean reading of the decay.
// ---------------------------------------------------------------------------------------------

/// The trail rung's own three dials, read from the shipped ladder rather than restated — a retune of
/// `intensification_ladder.json` must move these tests' arithmetic with it, not break it.
fn trail_dials() -> (f32, f32, u16) {
    let ladder = LadderConfig::builtin();
    let rung = ladder.rung(RungKey::RouteTrail);
    let build = rung.build.as_ref().expect("the trail rung is built");
    let upkeep = rung.upkeep.as_ref().expect("the trail rung is kept");
    let decay = upkeep
        .meter_decay
        .as_ref()
        .expect("the trail rung bleeds its meter");
    (build.work_cost, decay.per_turn, upkeep.grace_turns as u16)
}

/// Lay a road along `path` and seat it exactly at the top of the **trail** rung — the rung whose
/// grace and rot rate every assertion below is quoted against.
fn seat_a_trail(app: &mut App, path: Vec<UVec2>) -> RouteId {
    let ladder = LadderConfig::builtin();
    let (trail_cost, _, _) = trail_dials();
    let mut routes = app.world.resource_mut::<RouteLedger>();
    let id = routes.insert(path, &ladder);
    routes
        .get_mut(id)
        .expect("the road was just laid")
        .set_position(trail_cost, &ladder);
    id
}

/// A straight run of `tiles` tiles starting at the band's own tile, so the band stands on it (rule 2)
/// and the road is long enough that its bill wants more than one pair of hands.
fn road_under(app: &App, band: Entity, tiles: u32) -> Vec<UVec2> {
    let head = position_of(app, band);
    (0..tiles)
        .map(|step| UVec2::new(head.x + step, head.y))
        .collect()
}

fn route(app: &App, id: RouteId) -> &Route {
    app.world
        .resource::<RouteLedger>()
        .get(id)
        .expect("the road is still in the ledger")
}

fn staff_roadwork(app: &mut App, band: Entity, workers: u32) {
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Roadwork, workers, workers.max(1), None);
    app.world.entity_mut(band).insert(allocation);
}

/// **HOW MANY BARE HANDS COVER THIS ROAD'S BILL IN FULL** — `ceil(demand / PER_WORKER_OUTPUT)`, the
/// same count `RungDef::upkeep_crew_needed` publishes one branch over. Read off the sim's own
/// interpolated demand rather than hard-coded, because the span is whatever the generated map's
/// terrain under the road happens to cost.
fn keepers_the_bill_wants(app: &App, id: RouteId) -> u32 {
    let ladder = LadderConfig::builtin();
    let route = route(app, id);
    let registry = app.world.resource::<TileRegistry>();
    let span = span_of_terrains(route.path.iter().filter_map(|pos| {
        registry
            .index(pos.x, pos.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .map(|tile| tile.terrain)
    }));
    let demand = route_upkeep_demand(route, span, &ladder);
    assert!(
        demand > NO_UPKEEP_DEMAND,
        "precondition: a seated trail really does owe something ({demand})"
    );
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

/// ⛔ **THE HEADLINE, AND BOTH HALVES ARE LOAD-BEARING.** A road with keepers on it holds exactly
/// where it stands; the same road with the role empty loses its rung once its grace runs out.
///
/// **The kept half is the liveness assertion.** Without it the neglected half passes just as well on
/// a sim where roads never hold anything at all — a position pinned at the floor decays to nothing
/// and reports the same "it fell" the real mechanism reports.
#[test]
fn a_road_with_keepers_holds_and_the_same_road_without_them_loses_its_rung() {
    const ROAD_TILES: u32 = 4;
    let (trail_cost, _, grace) = trail_dials();
    let turns = u32::from(grace) + 2;

    // ① Kept.
    let mut kept = spawn_world();
    let band = spawn_band(&mut kept, CAMP_A, 100);
    let path = road_under(&kept, band, ROAD_TILES);
    let road = seat_a_trail(&mut kept, path);
    let wanted = keepers_the_bill_wants(&kept, road);
    staff_roadwork(&mut kept, band, wanted);
    resolve_turns(&mut kept, turns);
    assert_eq!(
        route(&kept, road).position(),
        trail_cost,
        "a road whose bill is met holds exactly where it stands, however long nobody walks it"
    );
    assert_eq!(
        route(&kept, road).held_rung(),
        RungKey::RouteTrail,
        "and it still holds the rung it was seated at"
    );

    // ② The same road, with nobody on the role.
    let mut lost = spawn_world();
    let band = spawn_band(&mut lost, CAMP_A, 100);
    let path = road_under(&lost, band, ROAD_TILES);
    let road = seat_a_trail(&mut lost, path);
    staff_roadwork(&mut lost, band, 0);
    resolve_turns(&mut lost, turns);
    assert!(
        route(&lost, road).position() < trail_cost,
        "an unkept road loses ground once its grace is spent ({} of {trail_cost})",
        route(&lost, road).position()
    );
}

/// **THE DECAY IS PROPORTIONAL TO HOW SHORT YOU ARE.** Half a road's bill funded loses half the
/// rung's rate — the shape both food webs pin, and the whole reason `shortfall` stopped *being* the
/// decay.
///
/// Exactly **one** decaying turn is measured, so the demand cannot have moved under the reading: the
/// bill interpolates on the position, and a second decaying turn would be judged against a bill
/// struck at a position the first one had already lowered.
#[test]
fn half_a_roads_bill_funded_loses_half_the_rungs_rate() {
    const ROAD_TILES: u32 = 12;
    const EPSILON: f32 = 1.0e-3;
    let (trail_cost, rate, grace) = trail_dials();

    let mut app = spawn_world();
    let band = spawn_band(&mut app, CAMP_A, 100);
    let path = road_under(&app, band, ROAD_TILES);
    let road = seat_a_trail(&mut app, path);
    let wanted = keepers_the_bill_wants(&app, road);
    assert!(
        wanted >= 2,
        "precondition: the fixture's road wants more than one pair of hands ({wanted}), or \
         'half the keepers' is not a state it can be in"
    );
    staff_roadwork(&mut app, band, wanted / 2);

    // Up to the last turn the grace still forgives; the next one is the one that bites.
    resolve_turns(&mut app, u32::from(grace) + 1);
    let (demand, supplied, before) = {
        let route = route(&app, road);
        (
            route.upkeep_demanded.expect("every road is billed"),
            route.upkeep_supplied,
            route.position(),
        )
    };
    assert_eq!(
        before, trail_cost,
        "precondition: nothing has decayed yet — the grace is still forgiving"
    );
    let fraction = (demand - supplied) / demand;
    assert!(
        (0.3..0.7).contains(&fraction),
        "precondition: the fixture really does fund about half the bill (short {fraction} of it)"
    );

    resolve_turn(&mut app);
    let lost = before - route(&app, road).position();
    assert!(
        (lost - fraction * rate).abs() < EPSILON,
        "the bleed is the shortfall FRACTION times the rung's own rate, not the rate flat: \
         lost {lost} against {fraction} x {rate}"
    );
}

/// **THE GRACE IS HONOURED, AND IT IS STRICTLY GREATER.** A road one turn into shortfall has not
/// moved; a road past its rung's grace has.
#[test]
fn a_road_inside_its_grace_has_not_moved_and_one_past_it_has() {
    const ROAD_TILES: u32 = 4;
    let (trail_cost, _, grace) = trail_dials();

    let mut app = spawn_world();
    let band = spawn_band(&mut app, CAMP_A, 100);
    let path = road_under(&app, band, ROAD_TILES);
    let road = seat_a_trail(&mut app, path);
    staff_roadwork(&mut app, band, 0);

    // The first turn stamps the bill; the grace then forgives that many consecutive short turns.
    resolve_turns(&mut app, u32::from(grace) + 1);
    assert_eq!(
        route(&app, road).position(),
        trail_cost,
        "inside its grace the road has not moved at all"
    );

    resolve_turn(&mut app);
    assert!(
        route(&app, road).position() < trail_cost,
        "one turn past the grace it is going backwards ({})",
        route(&app, road).position()
    );
}

/// ⛔ **AN ABANDONED ROAD DECAYS AND IS EVENTUALLY FORGOTTEN — AND THIS IS THE TEST THAT CATCHES A
/// BILL STAMPED ONLY WHERE A BAND STANDS.**
///
/// `Route::keeping_is_met` answers `true` for a road with **no stamped bill**, so a keeping pass that
/// visited only the roads under a band would leave this one reading as kept for ever: it would never
/// arm its neglect counter, never decay, and never be pruned. With that bug this test does not
/// measure a slower decay — it measures **none at all**, which is the route arc's rule 3 deleted.
#[test]
fn a_road_no_band_stands_on_decays_and_is_finally_forgotten() {
    const ROAD_TILES: u32 = 3;
    let (trail_cost, rate, grace) = trail_dials();
    // Every work unit of the rung, at the full unmet rate, plus the grace that precedes the first
    // one and the turn that stamps the first bill.
    let turns_to_nothing = (trail_cost / rate).ceil() as u32 + u32::from(grace) + 2;

    let mut app = spawn_world();
    // A road out in open country, with no band anywhere near it and no camp on any of its tiles.
    let lone = UVec2::new(CAMP_A.0, CAMP_A.1);
    let road = seat_a_trail(
        &mut app,
        (0..ROAD_TILES)
            .map(|step| UVec2::new(lone.x, lone.y + step))
            .collect(),
    );

    resolve_turns(&mut app, u32::from(grace) + 2);
    assert!(
        route(&app, road).position() < trail_cost,
        "a road nobody stands on is billed like any other, and nobody pays it ({} of {trail_cost})",
        route(&app, road).position()
    );

    resolve_turns(&mut app, turns_to_nothing);
    assert!(
        app.world.resource::<RouteLedger>().get(road).is_none(),
        "a road back at the floor with no work in it is indistinguishable from no road, so the \
         ledger forgets it rather than growing without bound"
    );
}

/// **A GAME TRAIL OWES NOTHING, OVER ANY TERRAIN, AND HAS NO RUNG TO LOSE.** That is the whole of
/// what makes the branch's floor free: nobody maintains a game trail.
///
/// It is asserted on the **arithmetic** rather than through the ledger, and deliberately: a road at
/// [`RUNG_UNSTARTED`] carries no work, so `advance_routes` prunes it on the turn it is laid — there
/// is no persisted road in that state for a system-level fixture to watch. The claim being made is
/// that the floor falls out of the interpolation rather than being branched around, and that is a
/// statement about the numbers.
///
/// **Over the real map's terrains**, so *"any terrain"* is the shipped 37 rather than one the test
/// chose: the span multiplies the demand, and a floor that owed anything would owe *more* over hard
/// country.
#[test]
fn a_game_trail_owes_nothing_over_any_terrain_and_has_no_rung_to_lose() {
    /// More consecutive short turns than any route rung's grace, so *"still inside the grace"*
    /// cannot be what makes the decay below read zero.
    const LONG_PAST_ANY_GRACE: u16 = 500;
    let ladder = LadderConfig::builtin();
    let app = spawn_world();

    let terrains: Vec<_> = {
        let registry = app.world.resource::<TileRegistry>();
        let mut seen: Vec<_> = (0..registry.height)
            .flat_map(|y| (0..registry.width).map(move |x| (x, y)))
            .filter_map(|(x, y)| registry.index(x, y))
            .filter_map(|entity| app.world.get::<Tile>(entity))
            .map(|tile| tile.terrain)
            .collect();
        seen.sort_by_key(|terrain| format!("{terrain:?}"));
        seen.dedup();
        seen
    };
    assert!(
        terrains.len() > 1,
        "precondition: the generated map really does carry several terrains ({})",
        terrains.len()
    );

    for terrain in terrains {
        let span = span_of_terrains([terrain, terrain, terrain]);
        let trail = Route::worn_in(vec![UVec2::new(1, 1)], &ladder);
        assert_eq!(
            trail.held_rung(),
            RungKey::RouteGameTrail,
            "precondition: a road with no work in it holds the branch's floor"
        );
        assert_eq!(
            route_upkeep_demand(&trail, span, &ladder),
            NO_UPKEEP_DEMAND,
            "a game trail owes nothing over {terrain:?} (span {span}) — the floor declares no \
             upkeep, so the interpolation answers zero rather than an `is_built` branch answering \
             it"
        );
        let at_risk = route_at_risk_rung(&trail.standing());
        assert_eq!(
            at_risk,
            RungKey::RouteGameTrail,
            "with no work banked the rung at risk is the one it HOLDS, not the one above it"
        );
        assert_eq!(
            ladder
                .rung(at_risk)
                .upkeep_decay(WHOLLY_UNSUPPLIED, LONG_PAST_ANY_GRACE),
            0.0,
            "and it can never decay: there is no rung under the floor to lose"
        );
    }
}

/// ⛔ **SEVERAL BANDS MAY PAY ONE ROAD, AND EACH PAYS A PART.** §2.5's rule, on the branch most
/// exposed to it: a road has no owner, so *every* band standing on it draws the same bill.
///
/// **This is the test that catches `upkeep_supplied = ` in place of `+=`.** With an assignment the
/// second band's share replaces the first's rather than adding to it, the road is short by exactly
/// one band's worth for ever, and it bleeds with its whole keeping staffed — the plant web's own
/// *"a crew gathering a patch a second crew is sowing overwrites the sowers' supply with its own"*,
/// one branch over. Every other fixture in this file staffs one band, where the two spellings are
/// indistinguishable.
#[test]
fn two_bands_on_one_road_each_pay_a_part_of_its_bill() {
    /// Long enough that its bill wants more hands than either band brings on its own.
    const ROAD_TILES: u32 = 12;
    let (trail_cost, _, grace) = trail_dials();
    let turns = u32::from(grace) + 2;

    let mut app = spawn_world();
    let a = spawn_band(&mut app, CAMP_A, 100);
    // The neighbouring tile, which is the road's second tile — so both camps stand on one road
    // (rule 2) without either standing on the other.
    let b = spawn_band(&mut app, (CAMP_A.0 + 1, CAMP_A.1), 100);
    let path = road_under(&app, a, ROAD_TILES);
    let road = seat_a_trail(&mut app, path);
    let wanted = keepers_the_bill_wants(&app, road);
    assert!(
        wanted >= 2,
        "precondition: the road's bill wants more than one band's worth of hands ({wanted})"
    );
    // Half each, rounded up, so the two together cover it and neither does alone.
    let each = wanted.div_ceil(2);
    staff_roadwork(&mut app, a, each);
    staff_roadwork(&mut app, b, each);

    resolve_turns(&mut app, turns);
    assert_eq!(
        route(&app, road).position(),
        trail_cost,
        "two bands' shares ADD, so between them the bill is met and the road holds"
    );

    // The negative half: one of them alone cannot cover it, so the same road bleeds.
    let mut alone = spawn_world();
    let a = spawn_band(&mut alone, CAMP_A, 100);
    let path = road_under(&alone, a, ROAD_TILES);
    let road = seat_a_trail(&mut alone, path);
    staff_roadwork(&mut alone, a, each);
    resolve_turns(&mut alone, turns);
    assert!(
        route(&alone, road).position() < trail_cost,
        "and one band's share alone genuinely is short — otherwise the half above measured nothing"
    );
}
