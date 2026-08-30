//! **Roads form under the traffic that was already there, and the band that graded a tile pays for
//! it** (`docs/plan_standing_upkeep.md` §4.13b, issue #532).
//!
//! These drive the turn's three route systems in **stage order** — `balance_supply_networks` and
//! `routes::advance_roads` in Logistics, then `settle_route_keeping` in Population — because that
//! ordering is load-bearing twice over:
//!
//! - the supply pass records the links and the route pass spends them, so the payoff is read at the
//!   *previous* turn's standing. A fixture that ran those two the other way round would measure a
//!   road reading itself.
//! - the keeping pass stamps and pays **after** the decay pass has judged and cleared, so the
//!   supply `advance_roads` weighs was stamped by **last** turn's Population — the one-turn carry
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
    advance_roads, balance_supply_networks, road_at_risk_rung, road_upkeep_demand,
    road_upkeep_measure, scalar_zero, settle_route_keeping, spawn_initial_world, BandId,
    ConnectionKey, ConnectionLedger, ConnectionsConfig, CultureManager, DiscoveryProgressLedger,
    EquipmentConfigHandle, FactionId, FactionInventory, GenerationId, GenerationRegistry,
    LaborAllocation, LaborTarget, LadderConfig, LadderConfigHandle, LocalStore, MapPresets,
    MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, Road, RoadKeeper, RoadRegistry,
    RouteTrafficLog, RungKey, Scalar, SimulationConfig, SimulationTick, SnapshotOverlaysConfig,
    SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, SupplyNetworkConfigHandle, SupplyNetworkMembership, Tile,
    TileRegistry, FOOD, FULL_TIE, NEAR_ENOUGH_TO_KEEP, NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT,
};

const TEST_FACTION: FactionId = FactionId(7);
const BAND_POP: u32 = 100;
const TEST_BAND_ID_BASE: u64 = 9_500;
static NEXT_TEST_BAND_ID: AtomicU64 = AtomicU64::new(TEST_BAND_ID_BASE);

/// Two tiles apart, so the traced journey crosses three tiles and both camps stand on it — the
/// smallest fixture in which the per-tile wearing is not a single tile.
const CAMP_A: (u32, u32) = (10, 10);
const CAMP_B: (u32, u32) = (12, 10);

/// Comfortably more turns than the shipped `route:trail` price needs at the shipped per-tile rate,
/// so the climb is what is being measured rather than the loop bound.
///
/// **It doubled with the per-tile model**, and that is the model change rather than a slow fixture:
/// the stored-path model banked `rate × path length` onto **one** object, so a 2-tile link put
/// `0.70` a turn into a single road; each tile now banks its own `0.35`.
const TURNS_TO_WEAR_A_TRAIL: u32 = 160;

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
    app.world.insert_resource(RoadRegistry::default());
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
    app.world.run_system_once(advance_roads);
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

/// The cumulative position at which a road **holds** `route:dirt_road`, read from the ladder rather
/// than restated — a retune of `intensification_ladder.json` must move these fixtures with it.
fn dirt_road_top(ladder: &LadderConfig) -> f32 {
    let (base, width) =
        core_sim::road_rung_span(RungKey::RouteDirtRoad, ladder, NEAR_ENOUGH_TO_KEEP);
    base + width
}

/// **Seat a dirt road on `tile` and hand it to `keeper`** — the cheapest rung anybody maintains, so
/// a fixture asks the smallest bill that can be met or missed.
///
/// **The position is written BEFORE the keeper**, and the order is load-bearing: `set_position`
/// releases a keeper on a road that has fallen back into the free floor, so seating a keeper first
/// would hand it straight back.
fn seat_a_dirt_road(app: &mut App, tile: UVec2, keeper: Option<BandId>) {
    let ladder = LadderConfig::builtin();
    let top = dirt_road_top(&ladder);
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.set_position(top, &ladder);
    if let Some(band) = keeper {
        road.take_keeper(
            RoadKeeper {
                faction: TEST_FACTION,
                band,
            },
            NEAR_ENOUGH_TO_KEEP,
            &ladder,
        );
    }
}

fn road_at(app: &App, tile: UVec2) -> &Road {
    app.world
        .resource::<RoadRegistry>()
        .road(tile)
        .expect("the road is still in the registry")
}

/// **HOW MANY BARE HANDS COVER THIS ROAD'S BILL IN FULL** — `ceil(demand / PER_WORKER_OUTPUT)`, read
/// off the sim's own measure rather than hard-coded, because the ground under a road is whatever the
/// generated map put there.
fn keepers_the_bill_wants(app: &App, tile: UVec2) -> u32 {
    let ladder = LadderConfig::builtin();
    let road = road_at(app, tile);
    let terrain = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .and_then(|entity| app.world.get::<Tile>(entity))
        .expect("a seated road stands on a real tile")
        .terrain;
    let demand = road_upkeep_demand(
        road,
        road_upkeep_measure(terrain, road.keeper_remoteness),
        &ladder,
    );
    assert!(
        demand > 0.0,
        "precondition: a seated dirt road really does owe something ({demand})"
    );
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

fn staff_roadwork(app: &mut App, band: Entity, workers: u32) {
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .map(|allocation| allocation.clone())
        .unwrap_or_default();
    allocation.set_assignment(LaborTarget::Roadwork, workers, workers.max(1), None);
    app.world.entity_mut(band).insert(allocation);
}

/// The tiles between two camps, which is the run a journey between them wears and the run the
/// friction payoff is read over.
fn tiles_between(app: &App, a: Entity, b: Entity) -> Vec<UVec2> {
    let width = app.world.resource::<TileRegistry>().width;
    let height = app.world.resource::<TileRegistry>().height;
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    core_sim::trace_path(
        position_of(app, a),
        position_of(app, b),
        width,
        height,
        wrap,
    )
}

// ---------------------------------------------------------------------------------------------
// TRAFFIC — the free floor, worn in by the pooling that was already happening
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE HEADLINE: NOBODY ORDERS A TRAIL.** Two camps that have shared a larder for a few dozen
/// turns wear one between them, with no command typed and no crew staffed — #532's *"it must not be
/// the one case that produces no trail because nobody typed a command"*.
///
/// **And it is worn on EVERY TILE THE JOURNEY CROSSES**, which is the per-tile model's own claim:
/// the run between the camps is roaded end to end rather than one object carrying a stored path.
#[test]
fn two_camps_that_pool_wear_a_trail_between_them_with_nobody_ordering_it() {
    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);
    let run = tiles_between(&app, a, b);
    assert!(
        run.len() >= 3,
        "precondition: the camps are far enough apart for the run to have a middle ({})",
        run.len()
    );
    assert!(
        app.world.resource::<RoadRegistry>().is_empty(),
        "precondition: turn 1 opens with no roads anywhere — the shipped state"
    );

    for _ in 0..TURNS_TO_WEAR_A_TRAIL {
        set_food(&mut app, a, 100);
        set_food(&mut app, b, 0);
        resolve_turn(&mut app);
    }
    assert!(
        food_of(&app, b) > 0.0,
        "precondition: the two camps really are pooling — the traffic this measures"
    );

    for tile in &run {
        let road = road_at(&app, *tile);
        assert_eq!(
            road.held_rung(),
            RungKey::RouteTrail,
            "every tile of the run is worn to a trail, {tile:?} included — a road is a TILE \
             improvement, so a journey wears the ground it crosses rather than one path object"
        );
        assert_eq!(
            road.keeper, None,
            "and nobody keeps it: the free floor is formed by use and is nobody's job"
        );
    }
}

/// ⛔ **TRAFFIC BANKS UP TO THE TOP OF THE FREE FLOOR AND STOPS** (§4.13a rule 1), paired with the
/// liveness half — it really does *reach* the trail.
///
/// Without the cap, traffic would wear a **dirt road** in for free and hand the band a standing
/// labour bill it never ordered, one rung later and dearer than the trail 13a billed it for.
#[test]
fn traffic_climbs_to_the_top_of_the_free_floor_and_stops_there() {
    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);
    let run = tiles_between(&app, a, b);

    // Far longer than the trail needs, so the cap is what stops the climb.
    for _ in 0..TURNS_TO_WEAR_A_TRAIL * 4 {
        set_food(&mut app, a, 100);
        set_food(&mut app, b, 0);
        resolve_turn(&mut app);
    }

    let ceiling = core_sim::traffic_ceiling(&LadderConfig::builtin());
    for tile in &run {
        let road = road_at(&app, *tile);
        assert_eq!(
            road.held_rung(),
            RungKey::RouteTrail,
            "the liveness half: traffic really does reach the trail on {tile:?}"
        );
        assert!(
            road.position() <= ceiling + 1.0e-3,
            "and it stops there — {} against a ceiling of {ceiling}",
            road.position()
        );
    }
}

/// ⛔ **A TRAIL STILL COSTS NOTHING AND HAS NO KEEPER**, paired against a dirt road on the same map
/// that owes a real bill — the containment claim would pass on a build with the whole keeping
/// deleted.
#[test]
fn a_trail_costs_nothing_and_has_no_keeper_and_a_dirt_road_owes() {
    let mut app = spawn_world();
    let band = spawn_band(&mut app, CAMP_A, 100);
    let id = band_id(&app, band);
    let trail = UVec2::new(CAMP_A.0, CAMP_A.1);
    let dirt = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);

    {
        let ladder = LadderConfig::builtin();
        let ceiling = core_sim::traffic_ceiling(&ladder);
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads
            .road_or_trail(trail, &ladder)
            .set_position(ceiling, &ladder);
    }
    seat_a_dirt_road(&mut app, dirt, Some(id));
    staff_roadwork(&mut app, band, 0);
    resolve_turn(&mut app);

    let worn = road_at(&app, trail);
    assert_eq!(
        worn.held_rung(),
        RungKey::RouteTrail,
        "precondition: the trail is fully worn, which is the hardest case for the claim"
    );
    assert_eq!(
        worn.upkeep_basis(),
        NO_UPKEEP_DEMAND,
        "a FULLY WORN trail is still free — the whole floor declares no upkeep"
    );
    assert_eq!(worn.keeper, None, "and it is still nobody's job");

    assert!(
        road_at(&app, dirt).upkeep_basis() > NO_UPKEEP_DEMAND,
        "the liveness half: the dirt road beside it owes a real bill, so the trail's zero is a fact \
         about the RUNG and not about a keeping that was never wired"
    );
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band was staffed")
            .last_roadwork_demand
            > 0.0,
        "and the band's roll-up names that bill even with nobody on the role — the alarm"
    );
}

/// ⛔ **A TRAIL NOBODY WALKS FADES**, paired against one that is still being walked. The free floor
/// cannot be *short* — it declares no upkeep — so what takes it back is **disuse**.
#[test]
fn a_trail_nobody_walks_fades_and_one_still_walked_does_not() {
    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);
    let walked = tiles_between(&app, a, b);
    let abandoned = UVec2::new(CAMP_A.0, CAMP_A.1 + 5);
    {
        let ladder = LadderConfig::builtin();
        let ceiling = core_sim::traffic_ceiling(&ladder);
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads
            .road_or_trail(abandoned, &ladder)
            .set_position(ceiling, &ladder);
    }

    let ladder = LadderConfig::builtin();
    let grace = ladder.route_traffic.disuse_grace_turns;
    let loss = ladder.route_traffic.disuse_loss_per_turn;
    let turns_to_lose_it = grace + (core_sim::traffic_ceiling(&ladder) / loss).ceil() as u32 + 2;
    for _ in 0..turns_to_lose_it {
        set_food(&mut app, a, 100);
        set_food(&mut app, b, 0);
        resolve_turn(&mut app);
    }

    assert!(
        app.world
            .resource::<RoadRegistry>()
            .road(abandoned)
            .is_none(),
        "a trail nobody has walked in a season is gone, and the registry has forgotten it"
    );
    for tile in &walked {
        assert!(
            road_at(&app, *tile).position() > 0.0,
            "the liveness half: the run that IS being walked is still there ({tile:?})"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// THE PAYOFF — derived from the tiles a journey crosses
// ---------------------------------------------------------------------------------------------

/// ⛔ **A KEPT ROAD DELIVERS MORE, AND AN UNROUTED NETWORK IS UNTOUCHED.** The friction payoff, read
/// as the **mean** over the tiles between the two camps.
///
/// The unrouted half is the guarantee the whole branch rests on — *"an unrouted pair pools exactly as
/// it does today, at exactly today's friction"* — and it is what makes the routed half a measurement
/// rather than a number.
#[test]
fn a_kept_road_delivers_more_and_an_unrouted_network_is_untouched() {
    const STARTING_FOOD: i64 = 200;

    let delivered = |roaded: bool| -> f32 {
        let mut app = spawn_world();
        let (a, b) = two_neighbouring_camps(&mut app);
        if roaded {
            let run = tiles_between(&app, a, b);
            let id = band_id(&app, a);
            for tile in &run {
                seat_a_dirt_road(&mut app, *tile, Some(id));
            }
            let wanted: u32 = run
                .iter()
                .map(|tile| keepers_the_bill_wants(&app, *tile))
                .sum();
            staff_roadwork(&mut app, a, wanted);
            // One turn so the keeping is stamped and paid — `grants_sight` gates the payoff on the
            // PAID bill, so an unpaid road buys nothing.
            resolve_turn(&mut app);
            for tile in &run {
                assert!(
                    road_at(&app, *tile).grants_sight(),
                    "precondition: every tile of the run is built and kept ({tile:?})"
                );
            }
        }
        set_food(&mut app, a, STARTING_FOOD);
        set_food(&mut app, b, 0);
        resolve_turn(&mut app);
        food_of(&app, b)
    };

    let unrouted = delivered(false);
    let routed = delivered(true);
    assert!(
        unrouted > 0.0,
        "precondition: the unrouted pair really does pool"
    );
    assert!(
        routed > unrouted,
        "a kept road spills less of what crosses it: {routed} against {unrouted}"
    );
}

/// ⛔ **A ROAD OFF THE RUN BUYS THAT NETWORK NOTHING** — the negative control, and the replacement
/// for the retired *"only one camp stands on it"* rule.
///
/// The payoff is derived from **the tiles between the two camps**, so a road somewhere else is not a
/// road this pooling uses. ⚠ **On its own this passes with the whole friction term ripped out**, so
/// it is only worth anything paired with the test above it.
#[test]
fn a_road_off_the_run_between_two_camps_buys_that_network_nothing() {
    const STARTING_FOOD: i64 = 200;

    let mut bare = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut bare);
    set_food(&mut bare, a, STARTING_FOOD);
    set_food(&mut bare, b, 0);
    resolve_turn(&mut bare);
    let unrouted = food_of(&bare, b);

    let mut aside = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut aside);
    let run = tiles_between(&aside, a, b);
    let elsewhere = UVec2::new(CAMP_A.0, CAMP_A.1 + 4);
    assert!(
        !run.contains(&elsewhere),
        "precondition: the road really is off the run"
    );
    let id = band_id(&aside, a);
    seat_a_dirt_road(&mut aside, elsewhere, Some(id));
    let wanted = keepers_the_bill_wants(&aside, elsewhere);
    staff_roadwork(&mut aside, a, wanted);
    resolve_turn(&mut aside);
    assert!(
        road_at(&aside, elsewhere).grants_sight(),
        "precondition: the road aside IS kept — it simply is not on the way"
    );
    set_food(&mut aside, a, STARTING_FOOD);
    set_food(&mut aside, b, 0);
    resolve_turn(&mut aside);

    assert!(
        (food_of(&aside, b) - unrouted).abs() < 1.0e-3,
        "a road nobody's journey crosses buys that network nothing: {} against {unrouted}",
        food_of(&aside, b)
    );
}

// ---------------------------------------------------------------------------------------------
// THE KEEPING — one keeper per tile, and the case the whole model exists for
// ---------------------------------------------------------------------------------------------

/// ⛔ **RAY'S CASE: TWO BANDS EACH KEEP HALF THE TILES BETWEEN THEM, AND EACH PAYS ONLY FOR ITS OWN
/// HALF.**
///
/// *"a road is a single tile improvement, not the entire path, so one band could maintain 1/2 the
/// tile roads for the distance of a connection between two bands and another the other 1/2."* This
/// is that sentence as a fixture, and it is the state a path object could not express at all.
///
/// **Two phases, because either alone proves nothing.** With both halves staffed every tile is met —
/// which would also pass on a build where one band paid for the lot. So the second phase empties the
/// far band's role and asserts that **its** tiles go short while the near band's stay met: that is
/// the claim that the bill follows the keeper.
#[test]
fn two_bands_each_keep_half_the_tiles_between_them_and_each_pays_only_for_its_own_half() {
    let mut app = spawn_world();
    // Far enough apart that they do not pool — this is a fixture about keeping, and pooling traffic
    // would wear extra tiles into it.
    let west = spawn_band(&mut app, (8, 10), 100);
    let east = spawn_band(&mut app, (14, 10), 100);
    let run = tiles_between(&app, west, east);
    assert!(
        run.len() >= 4,
        "precondition: a run long enough to have two halves ({})",
        run.len()
    );
    let (west_id, east_id) = (band_id(&app, west), band_id(&app, east));
    let split = run.len() / 2;
    for (index, tile) in run.iter().enumerate() {
        let keeper = if index < split { west_id } else { east_id };
        seat_a_dirt_road(&mut app, *tile, Some(keeper));
    }

    // ① Both halves staffed: every tile is met, and each band's roll-up names only its own tiles.
    let west_wants: u32 = run[..split]
        .iter()
        .map(|tile| keepers_the_bill_wants(&app, *tile))
        .sum();
    let east_wants: u32 = run[split..]
        .iter()
        .map(|tile| keepers_the_bill_wants(&app, *tile))
        .sum();
    staff_roadwork(&mut app, west, west_wants);
    staff_roadwork(&mut app, east, east_wants);
    resolve_turn(&mut app);

    for tile in &run {
        assert!(
            road_at(&app, *tile).keeping_is_met(),
            "with both halves staffed, every tile of the run is kept ({tile:?})"
        );
    }
    let west_demand = app
        .world
        .get::<LaborAllocation>(west)
        .expect("staffed")
        .last_roadwork_demand;
    let east_demand = app
        .world
        .get::<LaborAllocation>(east)
        .expect("staffed")
        .last_roadwork_demand;
    let whole_run: f32 = run
        .iter()
        .map(|tile| road_at(&app, *tile).upkeep_basis())
        .sum();
    assert!(
        (west_demand + east_demand - whole_run).abs() < 1.0e-3,
        "the two halves' bills add up to the run's, and neither band is billed for the whole: \
         {west_demand} + {east_demand} against {whole_run}"
    );
    assert!(
        west_demand > 0.0 && east_demand > 0.0 && west_demand < whole_run,
        "each band owes a real, PARTIAL share — {west_demand} and {east_demand} of {whole_run}"
    );

    // ② The eastern band stops keeping. Its tiles go short; the western band's do not.
    staff_roadwork(&mut app, east, 0);
    resolve_turn(&mut app);
    for tile in &run[..split] {
        assert!(
            road_at(&app, *tile).keeping_is_met(),
            "the western band's own tiles are still met ({tile:?}) — it pays for its half and \
             nothing else"
        );
    }
    for tile in &run[split..] {
        assert!(
            road_at(&app, *tile).upkeep_shortfall() > 0.0,
            "and the eastern band's tiles go short the turn it stops ({tile:?}) — nobody else \
             picks them up, because ONE KEEPER PER TILE means there is nobody else"
        );
    }
}

/// ⛔ **A ROAD FAR FROM ITS KEEPER COSTS MORE TO HOLD THAN THE SAME ROAD BESIDE IT** — the same tile,
/// the same ground, the same rung; only the distance quoted when the band took it on differs.
///
/// **Distance is a COST, never a wall**: the far road is not refused, it is dearer.
#[test]
fn a_road_far_from_its_keeper_costs_more_to_hold_than_the_same_road_beside_it() {
    let ladder = LadderConfig::builtin();
    let base = core_sim::road_keeping_range(&ladder);
    let far = core_sim::remoteness_multiplier(base + 1, &ladder);
    assert!(
        far > NEAR_ENOUGH_TO_KEEP,
        "precondition: the shipped config really does price a remote road above the rung"
    );

    // **Both roads HOLD the dirt road**, each seated at its own remoteness-priced top. That is the
    // comparison the claim is about: a remote road is dearer to *hold*, and it is also a bigger
    // pile to *raise* — seating both at the near top would compare a whole dirt road against a
    // half-built one and measure the wrong thing.
    let billed = |remoteness: f32| -> f32 {
        let mut app = spawn_world();
        let band = spawn_band(&mut app, CAMP_A, 100);
        let id = band_id(&app, band);
        let tile = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
        {
            let ladder = LadderConfig::builtin();
            let ceiling = core_sim::traffic_ceiling(&ladder);
            let (base, width) =
                core_sim::road_rung_span(RungKey::RouteDirtRoad, &ladder, remoteness);
            let mut roads = app.world.resource_mut::<RoadRegistry>();
            let road = roads.road_or_trail(tile, &ladder);
            road.set_position(ceiling, &ladder);
            road.take_keeper(
                RoadKeeper {
                    faction: TEST_FACTION,
                    band: id,
                },
                remoteness,
                &ladder,
            );
            road.set_position(base + width, &ladder);
        }
        assert_eq!(
            road_at(&app, tile).held_rung(),
            RungKey::RouteDirtRoad,
            "precondition: both roads are whole dirt roads, priced at their own remoteness"
        );
        staff_roadwork(&mut app, band, 0);
        resolve_turn(&mut app);
        road_at(&app, tile).upkeep_basis()
    };

    let near_bill = billed(NEAR_ENOUGH_TO_KEEP);
    let far_bill = billed(far);
    assert!(near_bill > 0.0, "precondition: a dirt road owes something");
    assert!(
        far_bill > near_bill,
        "the same road is dearer to hold when its keeper is far from it: {far_bill} against \
         {near_bill}"
    );
}

/// ⛔ **A ROAD WITH A KEEPER HOLDS; THE SAME ROAD WITHOUT ONE LOSES ITS RUNG.** Two worlds, one
/// difference — the containment half is worthless without the liveness half beside it.
#[test]
fn a_road_with_a_keeper_holds_and_the_same_road_without_one_loses_its_rung() {
    let ladder = LadderConfig::builtin();
    let grace = ladder
        .rung(RungKey::RouteDirtRoad)
        .upkeep
        .as_ref()
        .expect("a dirt road is kept")
        .grace_turns;
    let turns = grace + 4;

    let held = |staffed: bool| -> f32 {
        let mut app = spawn_world();
        let band = spawn_band(&mut app, CAMP_A, 100);
        let id = band_id(&app, band);
        let tile = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
        seat_a_dirt_road(&mut app, tile, Some(id));
        let wanted = keepers_the_bill_wants(&app, tile);
        staff_roadwork(&mut app, band, if staffed { wanted } else { 0 });
        resolve_turns(&mut app, turns);
        app.world
            .resource::<RoadRegistry>()
            .road(tile)
            .map_or(0.0, |road| road.position())
    };

    let kept = held(true);
    let neglected = held(false);
    assert_eq!(
        kept,
        dirt_road_top(&ladder),
        "a road whose bill is met has not moved at all"
    );
    assert!(
        neglected < kept,
        "and the same road with nobody on the role has bled: {neglected} against {kept}"
    );
}

/// ⛔ **HALF A ROAD'S BILL FUNDED LOSES HALF THE RUNG'S RATE** — the bleed is
/// `shortfall_fraction × meter_decay.per_turn`, not a flat rate whenever anything is short.
///
/// **The supplied side is written directly rather than staffed**, and that is deliberate: a keeper
/// is a **whole worker**, so on a bill this small every staffing above zero covers it in full and a
/// crew-driven fixture could only ever measure `0` or `1`. What is under test is the *arithmetic* of
/// the bleed, so the fixture hands `advance_roads` the fraction it is meant to scale by.
#[test]
fn half_a_roads_bill_funded_loses_half_the_rungs_rate() {
    let ladder = LadderConfig::builtin();
    let grace = ladder
        .rung(RungKey::RouteDirtRoad)
        .upkeep
        .as_ref()
        .expect("a dirt road is kept")
        .grace_turns;

    let lost = |funded: f32| -> f32 {
        let mut app = spawn_world();
        let band = spawn_band(&mut app, CAMP_A, 100);
        let id = band_id(&app, band);
        let tile = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
        seat_a_dirt_road(&mut app, tile, Some(id));
        staff_roadwork(&mut app, band, 0);
        // Past the grace with the bill wholly unpaid, so the next turn's judgement is the one that
        // bleeds — and the position is read either side of exactly that turn.
        resolve_turns(&mut app, grace + 2);
        let before = road_at(&app, tile).position();
        {
            let mut roads = app.world.resource_mut::<RoadRegistry>();
            let road = roads.road_mut(tile).expect("seated");
            road.upkeep_supplied = road.upkeep_basis() * funded;
        }
        app.world.run_system_once(advance_roads);
        before - road_at(&app, tile).position()
    };

    const NOTHING_FUNDED: f32 = 0.0;
    const HALF_FUNDED: f32 = 0.5;
    let wholly_short = lost(NOTHING_FUNDED);
    assert!(
        wholly_short > 0.0,
        "precondition: an unfunded road past its grace really does bleed"
    );
    let half_short = lost(HALF_FUNDED);
    assert!(
        (half_short - wholly_short * HALF_FUNDED).abs() < 1.0e-3,
        "half the bill funded loses half the rung's rate: {half_short} against half of \
         {wholly_short}"
    );
    assert!(
        (lost(1.0)).abs() < 1.0e-6,
        "and a bill met in full loses nothing at all — otherwise the proportion above is a \
         coincidence of a flat rate"
    );
}

/// **A ROAD INSIDE ITS GRACE HAS NOT MOVED, AND ONE PAST IT HAS.** The grace is *consecutive turns
/// short*, and `RungDef::upkeep_decay` owns the strictly-greater comparison.
#[test]
fn a_road_inside_its_grace_has_not_moved_and_one_past_it_has() {
    let ladder = LadderConfig::builtin();
    let grace = ladder
        .rung(RungKey::RouteDirtRoad)
        .upkeep
        .as_ref()
        .expect("a dirt road is kept")
        .grace_turns;

    let after = |turns: u32| -> f32 {
        let mut app = spawn_world();
        let band = spawn_band(&mut app, CAMP_A, 100);
        let id = band_id(&app, band);
        let tile = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
        seat_a_dirt_road(&mut app, tile, Some(id));
        staff_roadwork(&mut app, band, 0);
        resolve_turns(&mut app, turns);
        road_at(&app, tile).position()
    };

    let top = dirt_road_top(&ladder);
    assert_eq!(
        after(grace),
        top,
        "inside the grace the road has not moved a unit"
    );
    assert!(
        after(grace + 3) < top,
        "and past it, it has — otherwise the line above passes on a build with no decay at all"
    );
}

/// ⛔ **A KEEPERLESS ROAD DECAYS AND IS FINALLY PRUNED**, which is what a band walking away from the
/// game leaves behind.
///
/// **The bill is stamped on EVERY road, keeper or not**, and this is the test that says so: a pass
/// that stamped only the roads somebody keeps would leave this one reading as *kept* for ever —
/// never arming its neglect, never decaying, never pruned. It would fail as **no decay at all**.
#[test]
fn a_keeperless_road_decays_and_is_finally_pruned() {
    let mut app = spawn_world();
    let tile = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
    seat_a_dirt_road(&mut app, tile, None);
    assert_eq!(
        road_at(&app, tile).keeper,
        None,
        "precondition: nobody keeps this road"
    );
    assert_eq!(
        road_at(&app, tile).held_rung(),
        RungKey::RouteDirtRoad,
        "precondition: and it stands at a rung somebody has to pay for"
    );

    // Long enough to bleed the whole dirt road AND the trail beneath it away.
    let ladder = LadderConfig::builtin();
    let bleed = ladder
        .rung(RungKey::RouteDirtRoad)
        .upkeep
        .as_ref()
        .expect("a dirt road is kept")
        .meter_decay
        .as_ref()
        .expect("and it declares a decay")
        .per_turn;
    let turns = (dirt_road_top(&ladder) / bleed).ceil() as u32
        + ladder.route_traffic.disuse_grace_turns
        + (core_sim::traffic_ceiling(&ladder) / ladder.route_traffic.disuse_loss_per_turn).ceil()
            as u32
        + 20;
    resolve_turns(&mut app, turns);

    assert!(
        app.world.resource::<RoadRegistry>().road(tile).is_none(),
        "a road nobody keeps is lost and finally forgotten — the registry is bounded"
    );
}

/// **THE AT-RISK RUNG IS THE NEWEST ONE CARRYING WORK**, which is the rung the bill interpolates
/// through, the grace is read at and the decay eats.
#[test]
fn the_at_risk_rung_is_the_newest_rung_carrying_work() {
    let ladder = LadderConfig::builtin();
    let (base, width) =
        core_sim::road_rung_span(RungKey::RouteDirtRoad, &ladder, NEAR_ENOUGH_TO_KEEP);

    let mut app = spawn_world();
    let tile = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
    {
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads
            .road_or_trail(tile, &ladder)
            .set_position(base + width, &ladder);
    }
    assert_eq!(
        road_at(&app, tile).held_rung(),
        RungKey::RouteDirtRoad,
        "a road exactly at the top of the dirt road HOLDS it"
    );
    assert_eq!(
        road_at_risk_rung(&road_at(&app, tile).standing()),
        RungKey::RouteDirtRoad,
        "and nothing is banked above it, so the dirt road is what is at risk — not the paved road's \
         empty meter"
    );

    {
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads
            .road_mut(tile)
            .expect("seated")
            .set_position(base + width + 1.0, &ladder);
    }
    assert_eq!(
        road_at_risk_rung(&road_at(&app, tile).standing()),
        RungKey::RoutePavedRoad,
        "one unit into the paving, the paved road is the rung at risk"
    );
}
