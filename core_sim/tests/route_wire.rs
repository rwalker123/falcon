//! **The route section on the wire — one row per ROAD TILE** (`docs/plan_standing_upkeep.md`
//! §4.13b, issue #532).
//!
//! Every assertion here reads the **encoded envelope** through `root_as_envelope`, never the
//! in-process `RouteState`: a field that never reached the codec still passes an in-process
//! assertion, and the route section has no client reader yet to notice.
//!
//! The fixtures drive **whole turns** through [`core_sim::build_test_app`], so the numbers under
//! test are the ones the real stage order produced — the keeping pass stamps in Population, the
//! Snapshot stage publishes what it stamped.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    build_test_app, BandId, FactionId, LaborAllocation, LaborTarget, LadderConfig,
    PopulationCohort, ResidentBand, RoadKeeper, RoadRegistry, RungKey, SimulationConfig,
    SnapshotHistory, Tile, TileRegistry, ViewerFaction, METER_FULL, NEAR_ENOUGH_TO_KEEP,
    PER_WORKER_OUTPUT,
};

/// A pinned earthlike world, so the terrain under every road below is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// **How far from the band's camp a published road sits.** Far enough that the fog gate is a real
/// gate — the band has not explored that ground — and each test asserts that rather than trusting it.
const FAR_TILES: u32 = 20;

/// One published route row, read off the encoded envelope.
#[derive(Debug, Clone)]
struct PublishedRoad {
    tile: UVec2,
    rung: String,
    build_fraction: f32,
    has_keeper: bool,
    keeper_band_id: u64,
    keeper_remoteness: f32,
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

/// The band's `roadwork*` trio, read off the encoded envelope.
#[derive(Debug, Clone, Copy)]
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
fn published_roads(app: &App) -> Vec<PublishedRoad> {
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
        .map(|row| PublishedRoad {
            tile: UVec2::new(row.tileX(), row.tileY()),
            rung: row.rung().expect("a road publishes its rung").to_string(),
            build_fraction: row.buildFraction(),
            has_keeper: row.hasKeeper(),
            keeper_band_id: row.keeperBandId(),
            keeper_remoteness: row.keeperRemoteness(),
            demand: row.upkeepDemand(),
            supplied: row.upkeepSupplied(),
            shortfall: row.upkeepShortfall(),
            workers_needed: row.upkeepWorkersNeeded(),
            has_neglect_grace: row.hasNeglectGrace(),
            neglect_grace_remaining: row.neglectGraceRemaining(),
            grants_sight: row.grantsSight(),
            friction_multiplier: row.frictionMultiplier(),
            holds_link_to_tiles: row.holdsLinkToTiles(),
        })
        .collect()
}

fn published_road(app: &App, tile: UVec2) -> PublishedRoad {
    published_roads(app)
        .into_iter()
        .find(|row| row.tile == tile)
        .unwrap_or_else(|| panic!("the road at {tile:?} reached the viewer's frame"))
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

/// The campaign's first resident band: entity, faction, `BandId` and the tile it stands on. The
/// viewer faction is pinned to it, so what is published is what *this* people can see.
fn first_band(app: &mut App) -> (Entity, FactionId, BandId, UVec2) {
    let (entity, faction, band, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort, &BandId), With<ResidentBand>>();
        let (entity, cohort, band) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band");
        (entity, cohort.faction, *band, cohort.current_tile)
    };
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    app.world.insert_resource(ViewerFaction(faction));
    (entity, faction, band, position)
}

fn tile_east_of(app: &App, head: UVec2, steps: u32) -> UVec2 {
    let width = app.world.resource::<TileRegistry>().width;
    UVec2::new((head.x + steps) % width, head.y)
}

/// The shipped `route:dirt_road` **top position** and its neglect grace, read from the ladder rather
/// than restated — a retune of `intensification_ladder.json` must move these fixtures with it.
///
/// ⛔ **THE DIRT ROAD, NOT THE TRAIL.** The trail is the free floor's second storey — worn in by
/// traffic and billed nothing — so a fixture about a *bill* has to stand on the first rung anybody
/// keeps.
fn built_road_dials() -> (f32, u32) {
    let ladder = LadderConfig::builtin();
    let rung = ladder.rung(RungKey::RouteDirtRoad);
    let (base, width) =
        core_sim::road_rung_span(RungKey::RouteDirtRoad, &ladder, NEAR_ENOUGH_TO_KEEP);
    let grace = rung
        .upkeep
        .as_ref()
        .expect("the dirt road is the first rung anybody keeps")
        .grace_turns;
    (base + width, grace)
}

/// Seat a road on `tile` at `position` and hand it to `keeper`. The position is written **before**
/// the keeper: `set_position` releases a keeper on a road inside the free floor.
fn seat_road(app: &mut App, tile: UVec2, position: f32, keeper: Option<(FactionId, BandId)>) {
    let ladder = LadderConfig::builtin();
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.set_position(position, &ladder);
    if let Some((faction, band)) = keeper {
        road.take_keeper(RoadKeeper { faction, band }, NEAR_ENOUGH_TO_KEEP, &ladder);
    }
}

fn seat_a_dirt_road(app: &mut App, tile: UVec2, keeper: (FactionId, BandId)) {
    let (top, _) = built_road_dials();
    seat_road(app, tile, top, Some(keeper));
}

/// **HOW MANY BARE HANDS COVER THIS ROAD'S BILL IN FULL**, read off the sim's own measure rather
/// than hard-coded: the ground under a road is whatever the generated map put there.
fn keepers_the_bill_wants(app: &App, tile: UVec2) -> u32 {
    let ladder = LadderConfig::builtin();
    let road = app
        .world
        .resource::<RoadRegistry>()
        .road(tile)
        .expect("the road is in the registry");
    let terrain = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .and_then(|entity| app.world.get::<Tile>(entity))
        .expect("a seated road stands on a real tile")
        .terrain;
    let demand = core_sim::road_upkeep_demand(
        road,
        core_sim::road_upkeep_measure(terrain, road.keeper_remoteness),
        &ladder,
    );
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

fn staff_roadwork(app: &mut App, band: Entity, workers: u32) {
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Roadwork, workers, workers.max(1), None);
    app.world.entity_mut(band).insert(allocation);
}

// ---------------------------------------------------------------------------------------------
// The row's identity, and the keeper it names
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE ROW IS A TILE, AND IT NAMES ITS KEEPER** — the whole of what the per-tile model changed
/// on the wire.
///
/// **Both keeper states are asserted in one world**, because `hasKeeper: false` passes on a build
/// that never publishes a keeper at all.
#[test]
fn a_road_row_is_a_tile_and_names_the_band_whose_job_it_is() {
    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    let kept = camp;
    let free = tile_east_of(&app, camp, 1);
    seat_a_dirt_road(&mut app, kept, (faction, id));
    seat_road(
        &mut app,
        free,
        core_sim::traffic_ceiling(&LadderConfig::builtin()),
        None,
    );
    let wanted = keepers_the_bill_wants(&app, kept);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    let row = published_road(&app, kept);
    assert_eq!(
        row.tile, kept,
        "the row IS the tile — there is no path on it"
    );
    assert_eq!(
        row.rung, "route:dirt_road",
        "and it states the rung it holds"
    );
    assert!(row.has_keeper, "somebody keeps this one");
    assert_eq!(
        row.keeper_band_id, id.0,
        "and the row names WHICH band — one keeper per tile, never a share"
    );
    assert_eq!(
        row.keeper_remoteness, NEAR_ENOUGH_TO_KEEP,
        "quoted at the band's own camp, so distance costs it nothing"
    );

    let floor = published_road(&app, free);
    assert_eq!(
        floor.rung, "route:trail",
        "precondition: the second row really is on the free floor"
    );
    assert!(
        !floor.has_keeper,
        "and the free floor is NOBODY's job — the liveness half of the claim above"
    );
    assert_eq!(
        floor.demand, 0.0,
        "so it owes nothing, and `hasNeglectGrace` says there is nothing at risk"
    );
    assert!(!floor.has_neglect_grace);
}

// ---------------------------------------------------------------------------------------------
// The bill
// ---------------------------------------------------------------------------------------------

/// ⛔ **`demand − supplied == shortfall`, VERBATIM ON THE WIRE, ON A PART-FUNDED ROAD.**
///
/// The part-funded regime is the whole point: a fully funded road closes the identity with a
/// shortfall of zero and a starved one closes it with a supplied of zero, so either would pass with
/// the terms read off three different bills.
///
/// All three read the **stamped** basis (`routes::road_keeping_basis`), never the live interpolated
/// demand. This branch has had that defect twice.
#[test]
fn the_published_bill_closes_on_a_part_funded_road() {
    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    // A short run of tiles, all kept by the one band and staffed with a single keeper — so the pool
    // is spread thin enough that the rows are genuinely part funded rather than met or starved.
    const RUN: u32 = 4;
    let run: Vec<UVec2> = (0..RUN)
        .map(|step| tile_east_of(&app, camp, step))
        .collect();
    for tile in &run {
        seat_a_dirt_road(&mut app, *tile, (faction, id));
    }
    staff_roadwork(&mut app, band, 1);
    app.update();

    let rows: Vec<PublishedRoad> = published_roads(&app);
    assert_eq!(
        rows.len(),
        RUN as usize,
        "every tile of the run reaches the frame"
    );
    for row in &rows {
        assert!(row.demand > 0.0, "a dirt road owes something");
        assert!(
            (row.demand - row.supplied - row.shortfall).abs() < 1.0e-4,
            "demand − supplied == shortfall, verbatim: {row:?}"
        );
        assert_eq!(
            row.workers_needed,
            (row.demand / PER_WORKER_OUTPUT).ceil() as u32,
            "and the worker count is ceil of the SAME bill"
        );
    }
    assert!(
        rows.iter().any(|row| row.supplied > 0.0),
        "precondition: the single keeper really did pay something"
    );
    assert!(
        rows.iter()
            .any(|row| row.supplied > 0.0 && row.shortfall > 0.0),
        "precondition: and one keeper is not enough for a run of four — at least one row is PART \
         funded, which is the regime the identity is hardest in"
    );
}

/// ⛔ **THE BUILD METER IS THE RUNG'S OWN, AND READS EXACTLY FULL ON A COMPLETED RUNG.**
///
/// Never derived by subtraction: `fl(base + width) − base` is not `width` when that addition rounds,
/// which is the defect that published a completed Field as *"99%"*.
#[test]
fn the_build_meter_is_the_rungs_own_and_reads_exactly_full_on_a_completed_rung() {
    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    let tile = camp;
    seat_a_dirt_road(&mut app, tile, (faction, id));
    let wanted = keepers_the_bill_wants(&app, tile);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    let row = published_road(&app, tile);
    assert_eq!(row.rung, "route:dirt_road");
    assert_eq!(
        row.build_fraction, METER_FULL,
        "a road that has just COMPLETED a rung reads exactly full, not the next rung's zero and \
         not 99%"
    );

    // Half-way into the paving, the meter is the PAVED road's — a different rung from the one held.
    let (dirt_top, _) = built_road_dials();
    let (_, paved_width) = core_sim::road_rung_span(
        RungKey::RoutePavedRoad,
        &LadderConfig::builtin(),
        NEAR_ENOUGH_TO_KEEP,
    );
    {
        let ladder = LadderConfig::builtin();
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads
            .road_mut(tile)
            .expect("seated")
            .set_position(dirt_top + paved_width / 2.0, &ladder);
    }
    app.update();
    let row = published_road(&app, tile);
    assert_eq!(
        row.rung, "route:dirt_road",
        "the rung it HOLDS has not moved — the string is the bool"
    );
    assert!(
        (row.build_fraction - 0.5).abs() < 1.0e-3,
        "and the meter beside it is the PAVED road's, half raised: {}",
        row.build_fraction
    );
}

/// ⛔ **THE COUNTDOWN, NOT THE COUNTER.** A kept road reads its rung's full grace + 1; one reverting
/// now reads `0`. Both are asserted, because either alone passes on a build publishing a constant.
#[test]
fn the_countdown_reads_the_full_grace_on_a_kept_road_and_zero_on_one_reverting_now() {
    let (_, grace) = built_road_dials();

    let mut kept = spawn_world();
    let (band, faction, id, camp) = first_band(&mut kept);
    seat_a_dirt_road(&mut kept, camp, (faction, id));
    let wanted = keepers_the_bill_wants(&kept, camp);
    staff_roadwork(&mut kept, band, wanted);
    kept.update();
    let row = published_road(&kept, camp);
    assert!(row.has_neglect_grace, "a dirt road has something at risk");
    assert_eq!(
        row.neglect_grace_remaining,
        grace + 1,
        "a road whose bill is met reads its rung's full grace — walk away and you have this long"
    );

    let mut short = spawn_world();
    let (band, faction, id, camp) = first_band(&mut short);
    seat_a_dirt_road(&mut short, camp, (faction, id));
    staff_roadwork(&mut short, band, 0);
    // **One turn to stamp the bill, then `grace + 1` turns of judged shortfall.** The keeping pass
    // runs in Population *after* the decay pass has judged in Logistics, so the first turn's
    // shortfall is only counted on the second — the one-turn carry, not an off-by-one here.
    for _ in 0..grace + 2 {
        short.update();
    }
    assert_eq!(
        published_road(&short, camp).neglect_grace_remaining,
        0,
        "and one past its grace reads 0 — it is reverting now"
    );
}

/// ⛔ **THE BAND ROLL-UP IS THE SUM OF THE ROADS IT KEEPS, AND THE SIM SUMS IT.**
///
/// Route rows are fog-filtered, so a road out of sight would silently drop out of any client-side
/// total while the band certainly still owes its keeping — `fodderNeed`'s own rule.
///
/// **The demand is summed before the head-count gate**, so a band with nobody on the role publishes
/// the bill it is failing to pay rather than a reassuring zero. That is the alarm.
#[test]
fn the_band_roll_up_is_the_sum_of_the_roads_it_keeps() {
    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    let near = camp;
    let far = tile_east_of(&app, camp, FAR_TILES);
    seat_a_dirt_road(&mut app, near, (faction, id));
    seat_a_dirt_road(&mut app, far, (faction, id));
    staff_roadwork(&mut app, band, 0);
    app.update();

    let near_bill = published_road(&app, near).demand;
    assert!(
        near_bill > 0.0,
        "precondition: the near road owes something"
    );
    let far_bill = app
        .world
        .resource::<RoadRegistry>()
        .road(far)
        .expect("the far road exists in the sim")
        .upkeep_basis();
    assert!(far_bill > 0.0, "precondition: so does the far one");
    assert!(
        published_roads(&app).iter().all(|row| row.tile != far),
        "precondition: the FAR road is out of sight, which is what makes this a real test of the \
         sim summing it"
    );

    let roll_up = published_roadwork(&app);
    assert!(
        (roll_up.demand - (near_bill + far_bill)).abs() < 1.0e-3,
        "the roll-up carries BOTH roads' bills, the invisible one included: {} against {}",
        roll_up.demand,
        near_bill + far_bill
    );
    assert_eq!(
        roll_up.supplied, 0.0,
        "with nobody on the role nothing is supplied…"
    );
    assert!(
        roll_up.shortfall > 0.0,
        "…and the shortfall is the alarm the band is failing to answer"
    );
}

/// ⛔ **A ROAD ON GROUND THE FACTION HAS NEVER SEEN IS ABSENT FROM ITS FRAME**, and the near road
/// beside it is present — the fog gate, with its liveness half.
#[test]
fn a_road_on_ground_the_faction_has_never_seen_is_absent_from_its_frame() {
    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    let near = camp;
    let far = tile_east_of(&app, camp, FAR_TILES);
    seat_a_dirt_road(&mut app, near, (faction, id));
    seat_a_dirt_road(&mut app, far, (faction, id));
    staff_roadwork(&mut app, band, 0);
    app.update();

    let published: Vec<UVec2> = published_roads(&app).iter().map(|row| row.tile).collect();
    assert!(
        published.contains(&near),
        "the road under the band's own feet reaches its frame"
    );
    assert!(
        !published.contains(&far),
        "and one on ground nobody of theirs has ever stood on does not"
    );
}

/// **WHAT THE RUNG IS BUYING RIDES THE ROW**, off the road's own stamped payoff — the friction it
/// saves and the reach it holds open.
#[test]
fn the_row_states_what_the_rung_is_buying() {
    let ladder = LadderConfig::builtin();
    let dirt = ladder
        .rung(RungKey::RouteDirtRoad)
        .route_payoff
        .expect("every route rung declares a payoff");

    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    seat_a_dirt_road(&mut app, camp, (faction, id));
    let wanted = keepers_the_bill_wants(&app, camp);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    let row = published_road(&app, camp);
    assert_eq!(row.friction_multiplier, dirt.friction_multiplier);
    assert_eq!(row.holds_link_to_tiles, dirt.holds_link_to_tiles);
    assert!(
        row.grants_sight,
        "and the resolved *is this lighting its tile* answer, which a client cannot re-derive"
    );
}
