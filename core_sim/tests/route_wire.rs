//! **What a road publishes** (issue #532, `docs/plan_standing_upkeep.md` §4.13,
//! `.claude/rules/core_sim/routes.md`).
//!
//! ⛔ **EVERY ASSERTION HERE IS STRUCK ON THE ENCODED ENVELOPE**, through `root_as_envelope` and the
//! accessor chain a client uses — never on the in-process `RouteLedger`. A field that never reached
//! the codec still passes an in-process assertion, and the route section has no client reader yet to
//! notice.
//!
//! The fixtures drive **whole turns** so the numbers under test are the ones the real schedule
//! produces: `advance_routes` judges and clears in Logistics, `settle_route_keeping` stamps and pays
//! in Population, and the capture runs after both.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    build_test_app, FactionId, LaborAllocation, LaborTarget, LadderConfig, PopulationCohort,
    ResidentBand, RouteId, RouteLedger, RungKey, SimulationConfig, SnapshotHistory, Tile,
    TileRegistry, ViewerFaction, PER_WORKER_OUTPUT,
};

/// A pinned earthlike world, so the terrain under every road below is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// **How long every fixture's road is, in tiles.** Long enough that its bill wants several pairs of
/// hands, so a *part*-funded road is reachable by staffing one keeper.
const ROAD_TILES: u32 = 14;

/// **The one keeper a part-funded fixture staffs.** Strictly between "nobody" and "the whole bill",
/// which is the only regime in which `demand − supplied == shortfall` is a real claim: a fully
/// funded or wholly starved road satisfies it with one side at zero.
const ONE_KEEPER: u32 = 1;

/// A game trail with real work worn into it, as a fraction of the trail rung's own cost — the state
/// every road passes through on its way up, and the one a `buildFraction` of `1.0` must not read.
const WORN_BUT_NOT_YET_A_TRAIL: f32 = 0.5;

/// One road's published row, reduced to what the assertions here need.
#[derive(Debug, Clone, PartialEq)]
struct PublishedRoute {
    id: u64,
    path: Vec<UVec2>,
    rung: String,
    build_fraction: f32,
    demand: f32,
    supplied: f32,
    shortfall: f32,
    workers_needed: u32,
    has_neglect_grace: bool,
    neglect_grace_remaining: u32,
    grants_sight: bool,
    friction_multiplier: f32,
    holds_link_to_tiles: u32,
}

/// The band's published roadwork roll-up.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PublishedRoadwork {
    demand: f32,
    supplied: f32,
    shortfall: f32,
}

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

fn encoded(app: &App) -> Vec<u8> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
}

/// **The `routes` section off the encoded envelope**, through the accessor chain a client uses.
fn published_routes(app: &App) -> Vec<PublishedRoute> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let bytes = encoded(app);
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .routes()
        .and_then(|section| section.routes())
        .expect("the route section is published");
    section
        .iter()
        .map(|row| {
            let xs = row.pathX().expect("a road publishes its path");
            let ys = row.pathY().expect("a road publishes its path");
            assert_eq!(xs.len(), ys.len(), "the zipped path halves must agree");
            PublishedRoute {
                id: row.id(),
                path: (0..xs.len())
                    .map(|index| UVec2::new(xs.get(index), ys.get(index)))
                    .collect(),
                rung: row.rung().expect("a road publishes its rung").to_string(),
                build_fraction: row.buildFraction(),
                demand: row.upkeepDemand(),
                supplied: row.upkeepSupplied(),
                shortfall: row.upkeepShortfall(),
                workers_needed: row.upkeepWorkersNeeded(),
                has_neglect_grace: row.hasNeglectGrace(),
                neglect_grace_remaining: row.neglectGraceRemaining(),
                grants_sight: row.grantsSight(),
                friction_multiplier: row.frictionMultiplier(),
                holds_link_to_tiles: row.holdsLinkToTiles(),
            }
        })
        .collect()
}

fn published_route(app: &App, id: RouteId) -> PublishedRoute {
    published_routes(app)
        .into_iter()
        .find(|row| row.id == id.0)
        .unwrap_or_else(|| panic!("road {} reached the viewer's frame", id.0))
}

/// **The band's `roadwork*` trio off the encoded envelope.** The campaign runs one cohort per test,
/// so the sole published row is the band's.
fn published_roadwork(app: &App) -> PublishedRoadwork {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let bytes = encoded(app);
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohorts = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published");
    let row = cohorts
        .iter()
        .next()
        .expect("the campaign publishes at least one cohort");
    PublishedRoadwork {
        demand: row.roadworkDemand(),
        supplied: row.roadworkSupplied(),
        shortfall: row.roadworkShortfall(),
    }
}

/// The campaign's first resident band: entity, faction and the tile it stands on. The viewer faction
/// is pinned to it, so what is published is what *this* people can see.
fn first_band(app: &mut App) -> (Entity, FactionId, UVec2) {
    let (entity, faction, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
        let (entity, cohort) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band");
        (entity, cohort.faction, cohort.current_tile)
    };
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    app.world.insert_resource(ViewerFaction(faction));
    (entity, faction, position)
}

/// A straight run of `tiles` tiles starting at `head`.
fn road_from(app: &App, head: UVec2, tiles: u32) -> Vec<UVec2> {
    let width = app.world.resource::<TileRegistry>().width;
    (0..tiles)
        .map(|step| UVec2::new((head.x + step) % width, head.y))
        .collect()
}

/// The shipped `route:trail` price and its two neglect dials, read from the ladder rather than
/// restated — a retune of `intensification_ladder.json` must move these fixtures with it.
fn trail_dials() -> (f32, u32) {
    let ladder = LadderConfig::builtin();
    let rung = ladder.rung(RungKey::RouteTrail);
    let cost = rung
        .build
        .as_ref()
        .expect("the trail rung is built")
        .work_cost;
    let grace = rung
        .upkeep
        .as_ref()
        .expect("the trail rung is kept")
        .grace_turns;
    (cost, grace)
}

/// Lay a road along `path` and seat its position at `position` work units.
fn seat_road(app: &mut App, path: Vec<UVec2>, position: f32) -> RouteId {
    let ladder = LadderConfig::builtin();
    let mut routes = app.world.resource_mut::<RouteLedger>();
    let id = routes.insert(path, &ladder);
    routes
        .get_mut(id)
        .expect("the road was just laid")
        .set_position(position, &ladder);
    id
}

fn seat_a_trail(app: &mut App, path: Vec<UVec2>) -> RouteId {
    let (cost, _) = trail_dials();
    seat_road(app, path, cost)
}

/// **HOW MANY BARE HANDS COVER THIS ROAD'S BILL IN FULL**, read off the sim's own demand rather than
/// hard-coded: the span is whatever the generated map's terrain under the road happens to cost.
fn keepers_the_bill_wants(app: &App, id: RouteId) -> u32 {
    let ladder = LadderConfig::builtin();
    let route = app
        .world
        .resource::<RouteLedger>()
        .get(id)
        .expect("the road is in the ledger");
    let registry = app.world.resource::<TileRegistry>();
    let span = core_sim::span_of_terrains(route.path.iter().filter_map(|pos| {
        registry
            .index(pos.x, pos.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .map(|tile| tile.terrain)
    }));
    let demand = core_sim::route_upkeep_demand(route, span, &ladder);
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

fn staff_roadwork(app: &mut App, band: Entity, workers: u32) {
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Roadwork, workers, workers.max(1), None);
    app.world.entity_mut(band).insert(allocation);
}

// ---------------------------------------------------------------------------------------------
// The bill
// ---------------------------------------------------------------------------------------------

/// ⛔ **`demand − supplied == shortfall`, VERBATIM ON THE WIRE, ON A PART-FUNDED ROAD.**
///
/// The part-funded regime is the whole point: a fully funded road closes the identity with a
/// shortfall of zero and a starved one closes it with a supplied of zero, so either would pass with
/// the terms read off three different bills. This one is staffed with a single keeper against a bill
/// that wants several, and both preconditions are asserted before the identity is.
///
/// All three read the **stamped** basis (`routes::route_keeping_basis`), never the live interpolated
/// demand — which moves *within* a turn as bands walk on and off a road. This branch has had that
/// defect twice.
#[test]
fn the_published_bill_closes_on_a_part_funded_road() {
    let mut app = spawn_world();
    let (band, _, camp) = first_band(&mut app);
    let path = road_from(&app, camp, ROAD_TILES);
    let road = seat_a_trail(&mut app, path);
    assert!(
        keepers_the_bill_wants(&app, road) > ONE_KEEPER,
        "precondition: this road's bill wants more than the one keeper the fixture staffs"
    );
    staff_roadwork(&mut app, band, ONE_KEEPER);
    app.update();

    let row = published_route(&app, road);
    assert!(
        row.supplied > 0.0,
        "precondition: the one keeper really paid something ({row:?})"
    );
    assert!(
        row.shortfall > 0.0,
        "precondition: and the bill is still short, so this is the PART-funded regime ({row:?})"
    );
    assert_eq!(
        row.demand - row.supplied,
        row.shortfall,
        "the three published terms must close exactly: {row:?}"
    );
    // The crew count rides the same bill, so it cannot quote a rung the three terms do not.
    assert_eq!(
        row.workers_needed,
        (row.demand / PER_WORKER_OUTPUT).ceil() as u32,
        "the published crew count is ceil of the SAME bill: {row:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The meter
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE BUILD METER IS THE RUNG'S OWN, AND READS *EXACTLY* FULL ON A COMPLETED RUNG.**
///
/// A road seated exactly at the top of `route:trail` has nothing banked in the rung above, so the
/// meter describes the rung it just finished and must read `1.0` — not `0.0` (which is what reading
/// `standing.raising` unconditionally would publish) and not `0.99999994` (which is what deriving
/// `fl(base + width) − base` published for a completed Field).
///
/// **Paired with a half-worn road in the same world**, whose meter reads a real fraction — otherwise
/// "always 1.0" passes this test.
///
/// **And the rung string is the bool.** The completed road publishes `"route:trail"` while the
/// half-worn one publishes `"route:game_trail"`, so a client never has to threshold the float.
#[test]
fn the_build_meter_is_the_rungs_own_and_reads_exactly_full_on_a_completed_rung() {
    let (cost, _) = trail_dials();
    let mut app = spawn_world();
    let (band, _, camp) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;

    let done_path = road_from(&app, camp, ROAD_TILES);
    let done = seat_a_trail(&mut app, done_path);
    let part_head = UVec2::new((camp.x + width - ROAD_TILES + 1) % width, camp.y);
    let part_path = road_from(&app, part_head, ROAD_TILES);
    let part = seat_road(&mut app, part_path, cost * WORN_BUT_NOT_YET_A_TRAIL);
    let wanted = keepers_the_bill_wants(&app, done) + keepers_the_bill_wants(&app, part);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    let finished = published_route(&app, done);
    assert_eq!(
        finished.rung, "route:trail",
        "the completed road publishes the rung it HOLDS: {finished:?}"
    );
    assert_eq!(
        finished.build_fraction, 1.0,
        "a road that has just completed a rung reads EXACTLY full: {finished:?}"
    );

    let climbing = published_route(&app, part);
    assert_eq!(
        climbing.rung, "route:game_trail",
        "a road half-way to a trail still HOLDS the floor: {climbing:?}"
    );
    assert_eq!(
        climbing.build_fraction, WORN_BUT_NOT_YET_A_TRAIL,
        "and its meter is the real fraction of the rung it is raising: {climbing:?}"
    );

    // What the rung is buying rides the same row, off the road's stamped payoff.
    let ladder = LadderConfig::builtin();
    let payoff = ladder
        .rung(RungKey::RouteTrail)
        .route_payoff
        .as_ref()
        .expect("every route rung declares a payoff");
    assert_eq!(finished.friction_multiplier, payoff.friction_multiplier);
    assert_eq!(finished.holds_link_to_tiles, payoff.holds_link_to_tiles);
    assert!(
        finished.grants_sight,
        "a built road whose bill was met publishes the resolved 'it is lighting its tiles'"
    );
}

// ---------------------------------------------------------------------------------------------
// The countdown
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE COUNTDOWN, NOT THE COUNTER — `0` MEANS REVERTING NOW.**
///
/// A kept road reads its rung's full `grace_turns + 1` ("walk away and you have this long"); the
/// same road after `grace + 1` unpaid turns reads `0`, which is the penalty biting. Both halves are
/// asserted because "always 0" and "always grace + 1" each pass one of them alone.
///
/// Resolved through the same at-risk-rung seam `advance_routes` bleeds through, so the wire cannot
/// count down against a rung the sim is not touching — which is why the neglected road still reads
/// `hasNeglectGrace: true` after its position has bled below the trail boundary: the rung at risk is
/// the one carrying the banked work, not the free floor it has fallen back onto.
#[test]
fn the_countdown_reads_the_full_grace_on_a_kept_road_and_zero_on_one_reverting_now() {
    let (_, grace) = trail_dials();

    // ① Kept.
    let mut kept = spawn_world();
    let (band, _, camp) = first_band(&mut kept);
    let path = road_from(&kept, camp, ROAD_TILES);
    let road = seat_a_trail(&mut kept, path);
    let wanted = keepers_the_bill_wants(&kept, road);
    staff_roadwork(&mut kept, band, wanted);
    kept.update();
    let row = published_route(&kept, road);
    assert!(row.has_neglect_grace, "a kept trail has a rung at risk");
    assert_eq!(
        row.neglect_grace_remaining,
        grace + 1,
        "a road whose bill is met reads its rung's whole grace: {row:?}"
    );

    // ② The same road, unpaid for one turn longer than its grace forgives.
    let mut lost = spawn_world();
    let (band, _, camp) = first_band(&mut lost);
    let path = road_from(&lost, camp, ROAD_TILES);
    let road = seat_a_trail(&mut lost, path);
    staff_roadwork(&mut lost, band, 0);
    // The first turn's `advance_routes` judges a bill nobody had stamped yet, so the count of
    // *consecutive short turns* starts on the second — hence `grace + 2` turns to reach `grace + 1`
    // of them.
    for _ in 0..grace + 2 {
        lost.update();
    }
    let row = published_route(&lost, road);
    assert!(
        row.has_neglect_grace,
        "the rung carrying the banked work is still what is at risk: {row:?}"
    );
    assert_eq!(
        row.neglect_grace_remaining, 0,
        "0 is the penalty biting NOW, and it is what a reverting road publishes: {row:?}"
    );
    assert!(
        !row.grants_sight,
        "and it went dark on the way, which is the early warning"
    );
    // ⛔ **THE ROLL-UP IS UNGATED BY THE HEAD COUNT.** This band has nobody on `roadwork` and still
    // owes exactly this road's bill; the demand is the alarm, and summing it behind the head-count
    // gate would publish a reassuring zero for the band that is losing its road. `fodderNeed`'s own
    // rule, and the shortfall is the whole demand because nothing was paid.
    let roll_up = published_roadwork(&lost);
    assert_eq!(
        roll_up.demand, row.demand,
        "a band with the role empty still publishes what its road is billing: {roll_up:?}"
    );
    assert_eq!(roll_up.supplied, 0.0);
    assert_eq!(roll_up.shortfall, roll_up.demand);
}

// ---------------------------------------------------------------------------------------------
// The band roll-up — the sim sums it, and the client must not
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE BAND'S ROADWORK BILL IS THE SUM OF THE ROADS IT STANDS ON, SUMMED BY THE SIM.**
///
/// **Two roads under one band**, because a single-road fixture cannot tell a sum from a copy: it is
/// asserted that the roll-up equals `a + b` *and* that it equals neither road alone.
///
/// The client must not do this addition itself — route rows are fog-filtered, so a road out of sight
/// would silently drop out of a client-side total while the band certainly still owes its keeping.
/// That is `fodderNeed`'s own rule.
#[test]
fn the_band_roll_up_is_the_sum_of_both_its_roads_stamped_demands() {
    let mut app = spawn_world();
    let (band, _, camp) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;

    // Two roads out of the same camp, in opposite directions: the camp tile is on both, and nothing
    // else is.
    let east_path = road_from(&app, camp, ROAD_TILES);
    let east = seat_a_trail(&mut app, east_path);
    let west_head = UVec2::new((camp.x + width - ROAD_TILES + 1) % width, camp.y);
    let west_path = road_from(&app, west_head, ROAD_TILES);
    let west = seat_a_trail(&mut app, west_path);
    staff_roadwork(&mut app, band, ONE_KEEPER);
    app.update();

    let east_row = published_route(&app, east);
    let west_row = published_route(&app, west);
    let roll_up = published_roadwork(&app);

    assert!(
        east_row.demand > 0.0 && west_row.demand > 0.0,
        "precondition: both roads really are billing something ({east_row:?}, {west_row:?})"
    );
    assert_eq!(
        roll_up.demand,
        east_row.demand + west_row.demand,
        "the band's roll-up is the sum of the roads under its own tile: {roll_up:?}"
    );
    assert_ne!(
        roll_up.demand, east_row.demand,
        "and it is not one road's bill wearing a sum's clothes"
    );
    assert_ne!(roll_up.demand, west_row.demand);
    assert_eq!(
        roll_up.demand - roll_up.supplied,
        roll_up.shortfall,
        "the roll-up's own three terms close, exactly as a road row's do: {roll_up:?}"
    );
    assert!(
        roll_up.supplied > 0.0 && roll_up.shortfall > 0.0,
        "precondition: the one keeper paid part of a bill it could not cover: {roll_up:?}"
    );
    // **The supplied half ACCUMULATES too.** This band is the only payer on either road, so its
    // roll-up must equal what both roads received — an assignment in place of the `+=` would
    // publish only the last road's payment and still close the identity above.
    assert_eq!(
        roll_up.supplied,
        east_row.supplied + west_row.supplied,
        "the band's payment is summed across both its roads: {roll_up:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The fog
// ---------------------------------------------------------------------------------------------

/// ⛔ **A ROAD ON GROUND THIS FACTION HAS NEVER SEEN IS ABSENT FROM ITS FRAME.**
///
/// Paired with a road under the band's own feet, which **is** published — otherwise an empty section
/// passes this test, and an empty section is exactly what a broken producer emits.
#[test]
fn a_road_on_ground_the_faction_has_never_seen_is_absent_from_its_frame() {
    let mut app = spawn_world();
    let (band, faction, camp) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;

    let known_path = road_from(&app, camp, ROAD_TILES);
    let known = seat_a_trail(&mut app, known_path.clone());
    let away_head = UVec2::new((camp.x + width / 2) % width, camp.y);
    let away_path = road_from(&app, away_head, ROAD_TILES);
    let hidden = seat_a_trail(&mut app, away_path.clone());
    let wanted = keepers_the_bill_wants(&app, known);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    // Precondition, off the sim's own fog ledger: not one tile of the far road has been explored.
    let visibility = app.world.resource::<core_sim::VisibilityLedger>();
    assert!(
        away_path
            .iter()
            .all(|pos| !visibility.is_discovered(faction, pos.x, pos.y)),
        "precondition: the far road runs entirely over ground this people has never seen"
    );

    let published = published_routes(&app);
    let seen = published
        .iter()
        .find(|row| row.id == known.0)
        .expect("the road under the band's own feet is published");
    assert_eq!(
        seen.path, known_path,
        "and it publishes its WHOLE path in path order, including the far tiles nobody has seen — \
         a road is one object, and the fog gate is about whether you know of it: {seen:?}"
    );
    assert!(
        !published.iter().any(|row| row.id == hidden.0),
        "and the one on ground nobody has seen must not leak: {published:?}"
    );
}
