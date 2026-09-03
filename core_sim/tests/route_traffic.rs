//! **Roads form under the traffic that was already there, and the band that graded a tile pays for
//! it** (`docs/plan_standing_upkeep.md` §4.13b, issue #532).
//!
//! These drive the turn's route passes in **stage order** — `balance_supply_networks` and
//! `routes::advance_roads` in Logistics, then `bill_and_stock_roads` and the roadwork payment in
//! Population — because that ordering is load-bearing twice over:
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
use bevy::prelude::{Entity, Query, Res, ResMut, With};
use bevy::MinimalPlugins;

use core_sim::{
    advance_band_movement, advance_roads, balance_supply_networks, bill_and_stock_roads,
    credit_route_lessons, knows, road_at_risk_rung, road_upkeep_demand, road_upkeep_measure,
    scalar_zero, settle_bands_roadwork, spawn_initial_world, BandEquipment, BandId, BandTravel,
    ConnectionKey, ConnectionLedger, ConnectionsConfig, CultureManager, DiscoveryProgressLedger,
    EquipmentConfigHandle, FactionId, FactionInventory, GenerationId, GenerationRegistry,
    LaborAllocation, LaborConfigHandle, LaborTarget, LadderConfig, LadderConfigHandle, LocalStore,
    MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, ResidentBand, Road, RoadKeeper,
    RoadRegistry, RouteTrafficLog, RungKey, Scalar, SimulationConfig, SimulationTick,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation, StartProfileKnowledgeTags,
    StartProfileKnowledgeTagsHandle, SupplyNetworkConfigHandle, SupplyNetworkMembership, Tile,
    TileRegistry, FOOD, FULL_TIE, NEAR_ENOUGH_TO_KEEP, NO_UPKEEP_DEMAND, PAVING_DISCOVERY_ID,
    PER_WORKER_OUTPUT, ROADBUILDING_DISCOVERY_ID,
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
    // The movement pass reads `band_move_tiles_per_turn` off it — a marching party is the second
    // source of route traffic.
    app.world.insert_resource(LaborConfigHandle::default());

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
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
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

/// **THE ROADWORK PAYMENT, DRIVEN BY HAND — this harness does not run the labour pass.**
///
/// The payment is `core_sim::settle_bands_roadwork`, and in production it is called from **inside**
/// `advance_labor_allocation`: it divides the `roadwork` head count the shedding order left, and it
/// has to land before the road build quote struck in that same pass reads `Road::upkeep_supplied`.
/// It was a system of its own (`settle_route_keeping`, `.after` the labour pass) until that ordering
/// was found to publish a full rot for every road, funded or not.
///
/// This file drives the **route** passes in stage order and deliberately runs none of the labour
/// pass's hunting, foraging, building or wear — so it calls the one payer directly rather than
/// standing that whole system up. ⛔ **It is a driver, not a second arithmetic**: every number it
/// produces comes out of the same function production calls, so a change to the split cannot pass
/// here and fail there.
fn pay_road_keepers(
    mut registry: ResMut<RoadRegistry>,
    ladder: Res<LadderConfigHandle>,
    equipment: Res<EquipmentConfigHandle>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    // `With<BandId>` — a keeper *is* a `BandId`, so a cohort without one has nothing to claim with.
    mut bands: Query<
        (
            &PopulationCohort,
            &mut LaborAllocation,
            Option<&mut BandEquipment>,
            &BandId,
        ),
        With<BandId>,
    >,
) {
    let ladder = ladder.get();
    let equipment_cfg = equipment.get();
    for (cohort, mut allocation, mut band_equipment, band) in bands.iter_mut() {
        settle_bands_roadwork(
            &mut registry,
            cohort,
            &mut allocation,
            band_equipment.as_deref_mut(),
            *band,
            &equipment_cfg,
            &ladder,
            &tile_registry,
            &tiles,
        );
    }
}

/// One turn, in stage order: the three Logistics passes, then the two Population ones.
///
/// ⛔ **`advance_band_movement` IS ON THE POPULATION SIDE OF THE LINE**, which is what makes a
/// march's one-turn lag visible in a fixture: the journey it records is drained by the **next**
/// turn's `advance_roads`, while a pooling link recorded by `balance_supply_networks` is drained by
/// the same turn's. Running it in Logistics would hide the lag the arrangement really has.
fn resolve_turn(app: &mut App) {
    app.world.run_system_once(balance_supply_networks);
    app.world.run_system_once(advance_roads);
    app.world.run_system_once(credit_route_lessons);
    app.world.run_system_once(advance_band_movement);
    // **THE BILL AND THE STONE, THEN THE HANDS.** `bill_and_stock_roads` stamps both of a road's
    // bills at the pre-accrual position and spends the standing material; the roadwork payment
    // settles the WORK half afterwards, against that same stamp. The split is what puts a band's
    // standing roads ahead of a new paving build on the store — see `bill_and_stock_roads`.
    //
    // **`pay_road_keepers` stands in for `advance_labor_allocation`**, which is where the payment
    // runs in production — see that driver for why this harness does not stand the whole labour pass
    // up.
    app.world.run_system_once(bill_and_stock_roads);
    app.world.run_system_once(pay_road_keepers);
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

/// **Two camps `apart` tiles apart on one row**, of one people, who have met — the fixture the reach
/// payoff is measured over, since `reach_tiles` is `3` and a road's whole point is the distances
/// beyond it.
fn two_camps_apart(app: &mut App, apart: u32) -> (Entity, Entity) {
    let a = spawn_band(app, CAMP_A, 0);
    let b = spawn_band(app, (CAMP_A.0 + apart, CAMP_A.1), 0);
    seed_mutual_tie(app, a, b);
    let run = tiles_between(app, a, b);
    assert_eq!(
        run.len() as u32,
        apart + 1,
        "precondition: the camps really are {apart} hexes apart along one row"
    );
    (a, b)
}

/// **Wear every tile of a run to a fully worn TRAIL** — the free floor's top: nobody's job, owing
/// nothing, and therefore kept by arithmetic.
fn seat_a_trail_along(app: &mut App, run: &[UVec2]) {
    let ladder = LadderConfig::builtin();
    let ceiling = core_sim::traffic_ceiling(&ladder);
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    for tile in run {
        roads
            .road_or_trail(*tile, &ladder)
            .set_position(ceiling, &ladder);
    }
}

/// **Take one tile of a run back down to `route:path`** — the weakest link, and the containment
/// fixture for both the reach payoff and the lesson.
///
/// It leaves a *little* work on the tile rather than removing the road, so the prune keeps it: what
/// is under test is a run whose weakest tile is a **path**, not a run with a hole in the registry.
fn break_one_tile_back_to_a_path(app: &mut App, tile: UVec2) {
    const A_TOUCH_OF_WEAR: f32 = 1.0;
    let ladder = LadderConfig::builtin();
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.set_position(A_TOUCH_OF_WEAR, &ladder);
    assert_eq!(
        road.held_rung(),
        RungKey::RoutePath,
        "precondition: the broken tile holds the branch's floor and nothing more"
    );
}

/// This faction's progress toward a lesson, as the ledger holds it.
fn progress(app: &App, discovery: u32) -> f32 {
    app.world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(TEST_FACTION, discovery)
        .to_f32()
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
        app.world.resource::<RoadRegistry>(),
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

// ---------------------------------------------------------------------------------------------
// TRAFFIC — the people who march, and the one-turn lag that carries them
// ---------------------------------------------------------------------------------------------

/// ⛔ **A MARCHING PARTY WEARS THE GROUND IT CROSSES, AND ITS WORK LANDS NEXT TURN — ONCE.**
///
/// `advance_band_movement` is in `TurnStage::Population` and `advance_roads` drains the log in
/// `TurnStage::Logistics`, so a march is banked in the **following** turn's Logistics. That lag is
/// the arrangement, not a defect: the log has exactly one drain, so nothing is lost and nothing
/// doubles — which is what the third turn below measures, with the band standing still.
///
/// One band, so nothing pools: what is measured is the **march** and nothing else.
#[test]
fn a_marching_band_wears_its_journey_in_on_the_next_turn_and_banks_it_once() {
    let mut app = spawn_world();
    let band = spawn_band(&mut app, CAMP_A, 100);
    let from = position_of(&app, band);
    let to = UVec2::new(CAMP_A.0 + 1, CAMP_A.1);
    app.world.entity_mut(band).insert(BandTravel { target: to });

    // Turn 1 — Logistics finds an empty log; the march is recorded afterwards, in Population.
    resolve_turn(&mut app);
    assert_eq!(
        position_of(&app, band),
        to,
        "precondition: the band really did move this turn"
    );
    assert!(
        app.world.resource::<RoadRegistry>().is_empty(),
        "and nothing is worn yet — a march crosses the stage line before it is spent"
    );

    // Turn 2 — the next Logistics drains it.
    resolve_turn(&mut app);
    let ladder = LadderConfig::builtin();
    let expected = ladder.route_traffic.work_per_worker_tile * BAND_POP as f32;
    let banked: Vec<f32> = [from, to]
        .iter()
        .map(|tile| road_at(&app, *tile).position())
        .collect();
    for (tile, position) in [from, to].iter().zip(&banked) {
        assert!(
            (position - expected).abs() < 1.0e-3,
            "each tile of the journey carries `work_per_worker_tile x workers`: {tile:?} holds \
             {position} against {expected}"
        );
    }

    // Turn 3 — the band is standing still, so there is nothing left to bank.
    resolve_turn(&mut app);
    for (tile, position) in [from, to].iter().zip(&banked) {
        assert!(
            (road_at(&app, *tile).position() - position).abs() < 1.0e-3,
            "and it is banked EXACTLY ONCE: {tile:?} moved to {} after a turn of standing still",
            road_at(&app, *tile).position()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// REACH — what `holds_link_to_tiles` buys, consumed at last
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE CAPABILITY: TWO CAMPS TOO FAR APART TO POOL DO SO OVER A ROAD.** The reach payoff's
/// first consumer, paired with its own liveness half — the same pair, the same distance, the only
/// difference being the road on the ground between them.
///
/// **Goods actually move**, which is the claim; a component forming would pass with the balancer
/// itself broken.
#[test]
fn two_camps_beyond_the_free_reach_pool_only_over_a_kept_road() {
    /// Beyond `reach_tiles` (3) and beyond a trail's 6, inside a dirt road's 10 — so only the built
    /// rung can hold this link open.
    const BEYOND_A_TRAIL: u32 = 8;
    const STARTING_FOOD: i64 = 200;

    let delivered = |roaded: bool| -> f32 {
        let mut app = spawn_world();
        let (a, b) = two_camps_apart(&mut app, BEYOND_A_TRAIL);
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
            // One turn so the keeping is stamped and paid — an unmaintained road holds nothing open.
            resolve_turn(&mut app);
            for tile in &run {
                assert!(
                    road_at(&app, *tile).keeping_is_met(),
                    "precondition: every tile of the run is kept ({tile:?})"
                );
            }
        }
        set_food(&mut app, a, STARTING_FOOD);
        set_food(&mut app, b, 0);
        resolve_turn(&mut app);
        food_of(&app, b)
    };

    assert_eq!(
        delivered(false),
        0.0,
        "precondition: at {BEYOND_A_TRAIL} tiles the free reach holds nothing — this pair cannot \
         pool at all without a road"
    );
    assert!(
        delivered(true) > 0.0,
        "and an unbroken kept road between them holds the link open — a CAPABILITY, not a discount"
    );
}

/// ⛔ **ONE BARE TILE IN THE RUN CLOSES THE LINK AGAIN.** Reach takes the run's **weakest** tile, so
/// a link goods must get *through* is not most-of-the-way-there.
#[test]
fn one_broken_tile_in_a_routed_run_closes_the_link() {
    const BEYOND_A_TRAIL: u32 = 8;
    const STARTING_FOOD: i64 = 200;

    let mut app = spawn_world();
    let (a, b) = two_camps_apart(&mut app, BEYOND_A_TRAIL);
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
    resolve_turn(&mut app);

    set_food(&mut app, a, STARTING_FOOD);
    set_food(&mut app, b, 0);
    resolve_turn(&mut app);
    assert!(
        food_of(&app, b) > 0.0,
        "precondition: the unbroken run really is holding this link open"
    );

    // One tile in the middle taken back to bare ground.
    let middle = run[run.len() / 2];
    app.world.resource_mut::<RoadRegistry>().remove(middle);
    set_food(&mut app, a, STARTING_FOOD);
    set_food(&mut app, b, 0);
    resolve_turn(&mut app);
    assert_eq!(
        food_of(&app, b),
        0.0,
        "and one bare tile at {middle:?} closes it: the weakest tile is the answer"
    );
}

/// ⛔ **THE FREE FLOOR CARRIES IT TOO** — a wholly trailed run holds a link open at a distance the
/// free reach cannot, with nobody keeping anything and nobody paying anything.
///
/// **This is the test that proves the payoff filter is right.** Under the old `grants_sight` filter
/// a fully worn trail read as bare ground, so this pair would not pool at all — and the whole branch
/// would be unclimbable, since `grade` waits on a lesson only a standing connection teaches.
#[test]
fn a_wholly_trailed_run_holds_a_link_open_beyond_the_free_reach() {
    /// Beyond `reach_tiles` (3), inside the trail rung's own 6.
    const BEYOND_THE_FREE_REACH: u32 = 5;
    const STARTING_FOOD: i64 = 200;

    let delivered = |trailed: bool| -> f32 {
        let mut app = spawn_world();
        let (a, b) = two_camps_apart(&mut app, BEYOND_THE_FREE_REACH);
        if trailed {
            let run = tiles_between(&app, a, b);
            seat_a_trail_along(&mut app, &run);
            for tile in &run {
                let road = road_at(&app, *tile);
                assert_eq!(road.held_rung(), RungKey::RouteTrail);
                assert_eq!(road.keeper, None, "and nobody keeps a trail");
            }
        }
        set_food(&mut app, a, STARTING_FOOD);
        set_food(&mut app, b, 0);
        resolve_turn(&mut app);
        food_of(&app, b)
    };

    assert_eq!(
        delivered(false),
        0.0,
        "precondition: bare ground holds nothing open at {BEYOND_THE_FREE_REACH} tiles"
    );
    assert!(
        delivered(true) > 0.0,
        "FREE IS NOT WORTHLESS: a trail holds a link open where there was none"
    );
}

/// **NO EARLY-GAME REGRESSION** — a pair inside `reach_tiles` with no roads anywhere pools exactly as
/// it always did. The reach test is purely additive, and this is the half that says so.
#[test]
fn a_pair_inside_the_free_reach_pools_with_no_roads_at_all() {
    const STARTING_FOOD: i64 = 200;

    let mut app = spawn_world();
    let (a, b) = two_neighbouring_camps(&mut app);
    assert!(
        app.world.resource::<RoadRegistry>().is_empty(),
        "precondition: the shipped turn-1 state — no roads anywhere"
    );
    set_food(&mut app, a, STARTING_FOOD);
    set_food(&mut app, b, 0);
    resolve_turn(&mut app);
    assert!(
        food_of(&app, b) > 0.0,
        "two neighbours pool on the free reach alone, exactly as before the payoff was consumed"
    );
}

// ---------------------------------------------------------------------------------------------
// THE KNOWLEDGE CHAIN — a connection that stands teaches roadbuilding
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE TEST THE WHOLE SLICE EXISTS FOR: THE CHAIN IS LIVE END TO END.** Two bands within reach
/// of each other over an unbroken kept trail teach the faction `roadbuilding`, which is what opens
/// `grade` — and before this slice nothing in the sim credited a route lesson at all, so the branch
/// stopped dead at its free floor.
///
/// **Paired with the negative the model turns on**: one tile of the run taken back to a `path` and
/// the credit stops. Without that half, a broken connection predicate that credited everything would
/// pass the liveness claim just as well.
#[test]
fn a_standing_trail_connection_teaches_roadbuilding_and_one_broken_tile_stops_it() {
    const BEYOND_THE_FREE_REACH: u32 = 5;

    let mut app = spawn_world();
    let (a, b) = two_camps_apart(&mut app, BEYOND_THE_FREE_REACH);
    let run = tiles_between(&app, a, b);
    seat_a_trail_along(&mut app, &run);
    assert_eq!(
        progress(&app, ROADBUILDING_DISCOVERY_ID),
        0.0,
        "precondition: nothing has taught this faction roadbuilding"
    );

    resolve_turn(&mut app);
    let learned = progress(&app, ROADBUILDING_DISCOVERY_ID);
    assert!(
        learned > 0.0,
        "a standing connection over a trail teaches roadbuilding — the lesson the branch waits on"
    );

    resolve_turn(&mut app);
    assert!(
        progress(&app, ROADBUILDING_DISCOVERY_ID) > learned,
        "and it is credited EVERY TURN THE CONNECTION STANDS, not once on completion"
    );

    // One tile of the run back to the branch's floor.
    let held = progress(&app, ROADBUILDING_DISCOVERY_ID);
    break_one_tile_back_to_a_path(&mut app, run[run.len() / 2]);
    resolve_turn(&mut app);
    resolve_turn(&mut app);
    assert_eq!(
        progress(&app, ROADBUILDING_DISCOVERY_ID),
        held,
        "and one broken tile stops it dead: the lesson is the connection's WEAKEST tile, and a run \
         through a path is not a road you travel"
    );
}

/// **LENGTH IS THE MULTIPLIER** — a long connection out-credits a short one per turn, in proportion
/// to the tiles it runs over.
///
/// That is the route branch's own reading of the same currency the food webs scale by their floor:
/// there the multiplier is *how hard you are pressing the source*, here it is *how far the connection
/// runs*.
#[test]
fn a_longer_connection_teaches_more_in_proportion_to_its_length() {
    let credited = |apart: u32| -> f32 {
        let mut app = spawn_world();
        let (a, b) = two_camps_apart(&mut app, apart);
        let run = tiles_between(&app, a, b);
        seat_a_trail_along(&mut app, &run);
        resolve_turn(&mut app);
        progress(&app, ROADBUILDING_DISCOVERY_ID)
    };

    // Two tiles apart is a three-tile run; five apart is a six-tile one — twice the lesson.
    let short = credited(2);
    let long = credited(5);
    assert!(
        short > 0.0,
        "precondition: the short connection teaches too"
    );
    let ratio = long / short;
    assert!(
        (ratio - 2.0).abs() < 0.05,
        "a 6-tile connection is worth twice a 3-tile one: {long} against {short} is x{ratio}"
    );
}

/// ⛔ **THE WEAKEST TILE PICKS THE LESSON.** An all-dirt-road run teaches `paving`; the same run with
/// one trail tile in it teaches `roadbuilding` instead — because what you travel is the gap.
#[test]
fn the_weakest_tile_of_a_run_picks_the_lesson_it_teaches() {
    const BEYOND_THE_FREE_REACH: u32 = 5;

    let taught = |with_a_trail_tile: bool| -> (f32, f32) {
        let mut app = spawn_world();
        let (a, b) = two_camps_apart(&mut app, BEYOND_THE_FREE_REACH);
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
        if with_a_trail_tile {
            seat_a_trail_along(&mut app, &run[run.len() / 2..][..1]);
        }
        // One turn to stamp and pay the keeping, then the turn under measurement.
        resolve_turn(&mut app);
        let before = (
            progress(&app, ROADBUILDING_DISCOVERY_ID),
            progress(&app, PAVING_DISCOVERY_ID),
        );
        resolve_turn(&mut app);
        (
            progress(&app, ROADBUILDING_DISCOVERY_ID) - before.0,
            progress(&app, PAVING_DISCOVERY_ID) - before.1,
        )
    };

    let (roadbuilding, paving) = taught(false);
    assert!(
        paving > 0.0 && roadbuilding == 0.0,
        "a wholly dirt-roaded run teaches PAVING and nothing else: roadbuilding {roadbuilding}, \
         paving {paving}"
    );

    let (roadbuilding, paving) = taught(true);
    assert!(
        roadbuilding > 0.0 && paving == 0.0,
        "and one trail tile in it drops the whole run's lesson back to ROADBUILDING: roadbuilding \
         {roadbuilding}, paving {paving}"
    );
}

/// ⛔ **A RUN OF PATHS TEACHES NOTHING, THROUGH THE CONFIG RATHER THAN A BRANCH.** `route:path`
/// declares `earns_knowledge: null`, so the accrual answers `None` on its own — exactly as the free
/// floor owing no upkeep falls out of the arithmetic. There is no `path` special case in the code and
/// there must not be one.
#[test]
fn a_run_of_paths_teaches_nothing() {
    let mut app = spawn_world();
    let (a, b) = two_camps_apart(&mut app, 2);
    let run = tiles_between(&app, a, b);
    for tile in &run {
        break_one_tile_back_to_a_path(&mut app, *tile);
    }
    resolve_turn(&mut app);
    assert_eq!(
        progress(&app, ROADBUILDING_DISCOVERY_ID),
        0.0,
        "a connection over paths is still a connection, and it teaches nothing"
    );
    assert_eq!(progress(&app, PAVING_DISCOVERY_ID), 0.0);
}

/// ⛔ **THE GATE ACTUALLY OPENS** — `grade` is refused while `roadbuilding` is unlearned and accepted
/// once it is, asserted through the very expression the command's refusal reads
/// (`knows(ledger, faction, rung.unlock_discovery_id(), completion_threshold)`).
///
/// Before this slice the lesson could not be credited at all, so this gate was shut for ever and the
/// two built rungs were unreachable.
#[test]
fn the_grade_gate_opens_once_the_connection_has_taught_roadbuilding() {
    /// Comfortably more than the `lesson_cost / (learn_rate x tiles)` the fixture's run needs, so
    /// what is measured is the gate rather than the loop bound.
    const TURNS_TO_LEARN_ROADBUILDING: u32 = 40;
    const BEYOND_THE_FREE_REACH: u32 = 5;

    let ladder = LadderConfig::builtin();
    let threshold = ladder.knowledge.completion_threshold;
    let gate = ladder
        .rung(RungKey::RouteDirtRoad)
        .unlock_discovery_id()
        .expect("`grade` is gated on a lesson");

    let mut app = spawn_world();
    let (a, b) = two_camps_apart(&mut app, BEYOND_THE_FREE_REACH);
    let run = tiles_between(&app, a, b);
    seat_a_trail_along(&mut app, &run);
    assert!(
        !knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            TEST_FACTION,
            gate,
            threshold
        ),
        "precondition: `grade` is refused — the faction has not learned to build a road"
    );

    for _ in 0..TURNS_TO_LEARN_ROADBUILDING {
        resolve_turn(&mut app);
    }
    assert!(
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            TEST_FACTION,
            gate,
            threshold
        ),
        "and a connection the players kept standing opens it: {} against a threshold of {threshold}",
        progress(&app, ROADBUILDING_DISCOVERY_ID)
    );
}
