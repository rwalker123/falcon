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
    build_test_app, build_work_per_worker_turn, route_rungs_in_climb_order, rung_grants_sight,
    BandId, FactionId, LaborAllocation, LaborTarget, LadderConfig, PopulationCohort, ResidentBand,
    RoadKeeper, RoadRegistry, RungKey, SimulationConfig, SnapshotHistory, Tile, TileRegistry,
    ViewerFaction, FIRST_BUILT_RUNG, METER_FULL, NEAR_ENOUGH_TO_KEEP, NO_BUILD_GEAR,
    NO_UPKEEP_DEMAND, PER_WORKER_OUTPUT, RUNG_COST_UNSCALED,
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
    build_blocked_reason: String,
    build_material_demand: f32,
    build_material_supplied: f32,
    build_turns_remaining: i32,
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
            build_blocked_reason: row
                .buildBlockedReason()
                .expect("the cause is published, empty or not")
                .to_string(),
            build_material_demand: row.buildMaterialDemand(),
            build_material_supplied: row.buildMaterialSupplied(),
            build_turns_remaining: row.buildTurnsRemaining(),
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

/// [`seat_a_dirt_road`] at a stated **remoteness** — the keeper quote a road takes on when the band
/// that graded it is far from it. Distance is a cost on this branch, never a wall, so the only thing
/// it changes is the price: the same rung, dearer to hold.
fn seat_a_remote_dirt_road(
    app: &mut App,
    tile: UVec2,
    keeper: (FactionId, BandId),
    remoteness: f32,
) {
    let (top, _) = built_road_dials();
    let ladder = LadderConfig::builtin();
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.set_position(top, &ladder);
    let (faction, band) = keeper;
    road.take_keeper(RoadKeeper { faction, band }, remoteness, &ladder);
}

/// **WHAT ONE ROAD OWES ITS KEEPER THIS TURN**, in work units — `keepers_the_bill_wants`' own
/// reading, before the ceil that turns it into a head count.
fn the_bill_one_road_owes(app: &App, tile: UVec2) -> f32 {
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
    core_sim::road_upkeep_demand(
        road,
        core_sim::road_upkeep_measure(terrain, road.keeper_remoteness),
        &ladder,
    )
}

/// **WHAT ONE ROADWORK KEEPER ACTUALLY DELIVERS**, in work units per turn — `PER_WORKER_OUTPUT` plus
/// whatever the roster's derived road-keeping kit adds.
///
/// ⛔ **READ, NEVER ASSUMED.** It was `PER_WORKER_OUTPUT` while no tool served the branch; the road
/// tools tripled it, which is exactly the kind of move a fixture written against a literal absorbs
/// silently — see the `RUN` derivation at its one call site.
fn a_road_keepers_own_output() -> f32 {
    let equipment = core_sim::EquipmentConfig::builtin();
    // The **reference** ledger — one unit of everything. A single keeper needs exactly one, so this
    // is the honest stock for a one-worker coverage and it cannot go stale on the spawn's own dials.
    let ledger = core_sim::BandEquipment::start_stocked(&equipment);
    let rung = RungKey::RouteDirtRoad.wire_key();
    let kit = equipment.keeping_kit_for(None, core_sim::RungBranch::Route, Some(&rung));
    build_work_per_worker_turn(
        equipment
            .coverage(&kit, 1.0, &ledger)
            .weighted_rate(|crew| {
                equipment.build_work_per_worker(
                    crew,
                    &ledger,
                    core_sim::RungBranch::Route,
                    Some(&rung),
                )
            }),
    )
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

/// Stand a band's builders on a `pave` at `tile`, with `stone` in its stores — the state the paving
/// pile is actually drawn in. `stone` of `0.0` leaves the shelf bare, which is the blocked arm.
fn stage_a_paving(app: &mut App, band: Entity, tile: UVec2, builders: u32, stone: f32) {
    let ladder = LadderConfig::builtin();
    let (base, width) = core_sim::road_rung_span(
        RungKey::RouteDirtRoad,
        &ladder,
        core_sim::NEAR_ENOUGH_TO_KEEP,
    );
    {
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads
            .road_mut(tile)
            .expect("the fixture road is seated")
            .set_position(base + width, &ladder);
    }
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(
            FactionId(0),
            core_sim::PAVING_DISCOVERY_ID,
            core_sim::scalar_one(),
        );
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Builders, builders, builders.max(1), None);
    allocation.enqueue_build(
        core_sim::BuildSource::Road(tile),
        core_sim::BuildJob::Rung(core_sim::Improvement::Pave),
    );
    app.world.entity_mut(band).insert(allocation);

    let materials = core_sim::MaterialsConfig::builtin();
    let characteristics = materials
        .materials()
        .find(|(id, _)| *id == "stone")
        .and_then(|(_, def)| def.start_stock.as_ref())
        .map(|stock| stock.characteristics.clone())
        .expect("the shipped roster stocks the paving material at a spawn");
    let key = materials
        .band_key("stone", &characteristics)
        .expect("the shipped roster rates the paving material");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band");
    cohort.stores = core_sim::LocalStore::new();
    if stone > 0.0 {
        cohort.stores.deposit_material(
            "stone",
            key,
            core_sim::scalar_from_f32(stone),
            &characteristics,
        );
    }
}

/// ⛔ **A ROAD UNDER WAY PUBLISHES A REAL COUNTDOWN, AND IT MOVES.**
///
/// The sim stamped nothing here, so every client road queue model hardcoded
/// `BUILD_NOT_YET_ESTIMATED` — *the sim has not looked at this entry yet* — and a road read
/// **`Queued 97%` on turn 147**. The justification was the claim this branch has now had corrected
/// three times: *a road has no source row for the sim to stamp one on*. `RouteState` is that row.
///
/// **Three arms, and the third is the one the defect would survive.**
/// 1. A queued road under way publishes a **real count**, not `-5` and not `-1`.
/// 2. ⛔ **THE COUNT MOVES AS THE METER CLIMBS.** A field pinned at any *single* value — a constant
///    sentinel, or a frozen number — satisfies arm 1 on the turn it is read. That is exactly the
///    shape of the shipped bug, so *"it changed"* is the assertion that actually distinguishes a
///    live figure from a stuck one.
/// 3. A road **nobody has queued** publishes the honest *no estimate*, never a `0` that would render
///    as a finished build. Only a queued road has a quote; a client sizing an unbuilt rung against a
///    hypothetical crew is doing its own arithmetic and is right to.
#[test]
fn a_queued_road_publishes_a_real_countdown_that_moves_as_it_builds() {
    /// A crew big enough that a turn's accrual is a visible share of the paved rung.
    const BUILDERS: u32 = 4;
    /// Far more stone than the whole pile, so the store is never what stops it.
    const PLENTY: f32 = 1_000.0;

    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    // A second road the same band can see but has NOT queued — arm 3.
    let unqueued = tile_east_of(&app, camp, 1);
    seat_a_dirt_road(&mut app, camp, (faction, id));
    seat_a_dirt_road(&mut app, unqueued, (faction, id));
    stage_a_paving(&mut app, band, camp, BUILDERS, PLENTY);
    app.update();

    let first = published_road(&app, camp).build_turns_remaining;
    assert!(
        first >= 0,
        "a queued road under way must publish a REAL COUNT, not a sentinel: got {first}, where -5 \
         is `the sim has not looked` (the value the client hardcoded for every road, which is why \
         one read `Queued` for 147 turns) and -1 is `no estimate`"
    );

    // **The meter climbs, so the countdown must come down.** One more turn of the same crew.
    app.update();
    let second = published_road(&app, camp).build_turns_remaining;
    assert!(
        second >= 0,
        "and it is still a real count a turn later, not a sentinel: got {second}"
    );
    assert!(
        second < first,
        "⛔ THE COUNTDOWN MUST MOVE: {second} against {first}. A figure that never changes is what \
         the hardcoded sentinel was - arm 1 passes on it, and the player watches `Queued` for a \
         hundred turns"
    );

    // **Arm 3** — a road nobody ordered has no quote, and says so.
    assert_eq!(
        published_road(&app, unqueued).build_turns_remaining,
        sim_schema::NO_BUILD_TURNS_ESTIMATE,
        "a road no band has queued publishes NO ESTIMATE - never 0, which renders as a build that \
         has finished"
    );
}

/// ⛔ **A ROAD IS A SOURCE ROW, AND ITS ROW SAYS WHY THE POOL IS STUCK ON IT.**
///
/// It was stated for years that a road *"carries no source row for an estimate to be stamped on"*,
/// and that claim is what made the material half of this branch look impossible to build.
/// `RouteState` is keyed by tile exactly as a patch row is, so the same three facts a patch
/// publishes about the build in front of it ride here.
///
/// **Both arms in one sweep, because either alone passes on a stuck field.** A band with stone draws
/// it and reports no block; the same band with a bare shelf reports `"materials"` — the cause a
/// rung whose own gate **holds** publishes, which is the one a client could never derive for itself.
///
/// The `demand`/`supplied` pair obeys `RouteState`'s standing rule: the difference is the shortfall,
/// verbatim on the wire.
#[test]
fn a_paving_road_publishes_its_material_draw_and_says_when_the_store_stopped_it() {
    /// A crew big enough that a turn's accrual is a visible share of the rung.
    const BUILDERS: u32 = 4;
    /// Far more stone than one turn's share, so the stocked arm is never itself the short one.
    const PLENTY: f32 = 1_000.0;

    let paved = |stone: f32| {
        let mut app = spawn_world();
        let (band, faction, id, camp) = first_band(&mut app);
        seat_a_dirt_road(&mut app, camp, (faction, id));
        stage_a_paving(&mut app, band, camp, BUILDERS, stone);
        app.update();
        published_road(&app, camp)
    };

    let stocked = paved(PLENTY);
    assert!(
        stocked.build_material_demand > 0.0,
        "**LIVENESS**: a paving in progress must ask the stores for stone, or the blocked arm below \
         is comparing two zeroes"
    );
    assert_eq!(
        stocked.build_material_supplied, stocked.build_material_demand,
        "a band that holds plenty pays the whole of this turn's share: demand - supplied is the \
         shortfall, verbatim"
    );
    assert_eq!(
        stocked.build_blocked_reason, "",
        "a paving the store can cover is not blocked at all"
    );

    let bare = paved(0.0);
    assert_eq!(
        bare.build_blocked_reason, "materials",
        "⛔ a paving the store cannot cover says SO, and says WHY: the rung's own gate holds, so a \
         client reading the gate alone would see a block with no cause"
    );
    assert_eq!(
        bare.build_material_supplied, 0.0,
        "and it paid nothing toward the pile it asked for"
    );
    assert!(
        bare.build_material_demand > 0.0,
        "the bill it could not pay is still stated - a demand that vanished with the stock would \
         publish 'nothing was owed' for a road that is stuck"
    );
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
    //
    // ⛔ **THE RUN IS PRICED OUT OF THE KEEPER'S REACH, AND THE MULTIPLE IS DERIVED.** Four roads
    // next door outran one *bare-handed* keeper, and that is what this fixture used to rely on; the
    // route branch's own builders kits made a keeper three times as productive
    // (`a_road_keepers_own_output`), so the same four rows came back fully **met** and the part-funded
    // regime — the one the identity is hardest in — stopped being reached at all. The run stays four
    // tiles long, because every tile has to be inside the band's sight to reach its frame; what moves
    // is what distance does to the price. **Distance is a cost, never a wall.**
    const RUN: u32 = 4;
    let run: Vec<UVec2> = (0..RUN)
        .map(|step| tile_east_of(&app, camp, step))
        .collect();
    // Seat once next door to read what the ground under this run charges, then re-seat the whole run
    // far enough out that one keeper cannot cover it.
    seat_a_dirt_road(&mut app, run[0], (faction, id));
    let near_bill = the_bill_one_road_owes(&app, run[0]);
    let supply = a_road_keepers_own_output();
    assert!(
        near_bill > 0.0 && supply > 0.0,
        "fixture: a dirt road owes something and a keeper delivers something"
    );
    // One keeper covers `supply / bill` roads at this price; take the run past that, with a whole
    // multiple of headroom for the terrain varying tile to tile along the run.
    let remoteness = (supply / (RUN as f32 * near_bill)).ceil().max(1.0) + 1.0;
    for tile in &run {
        seat_a_remote_dirt_road(&mut app, *tile, (faction, id), remoteness);
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

/// The route branch as the **coded** climb has it, bottom rung first. `RungKey::above` is pinned
/// against the records' own `order` by `routes.rs`'s own module, so this is a second reading of the
/// shipped ladder rather than a second authority over it — and it is what makes the catalog
/// assertions below *liveness* claims rather than a comparison of the config with itself.
/// **A rung no crew builds costs nothing to reach** — what a record with no `build` block publishes,
/// which on the shipped ladder is the branch's floor and nothing else.
const NO_BUILD_WORK: f32 = 0.0;

const SHIPPED_ROUTE_CLIMB: [RungKey; 4] = [
    RungKey::RoutePath,
    RungKey::RouteTrail,
    RungKey::RouteDirtRoad,
    RungKey::RoutePavedRoad,
];

/// One published rung-catalog row, read off the encoded envelope.
#[derive(Debug, Clone)]
struct PublishedRouteRung {
    rung_key: String,
    order: u32,
    display_name: String,
    verb: String,
    unlock_knowledge: String,
    requires_rung: String,
    work_cost: f32,
    upkeep_work_per_turn: f32,
    friction_multiplier: f32,
    holds_link_to_tiles: u32,
    grants_sight: bool,
    earns_knowledge: String,
    build_work_per_worker_turn: f32,
    build_material_cost: f32,
    build_material_id: String,
}

/// **The `routeRungs` catalog off the encoded envelope**, through the accessor chain a client uses.
/// It rides the subsistence section beside `ladderKnowledge` — both are declarations of what the
/// ladder holds, carrying no faction and no tile.
fn published_route_rungs(app: &App) -> Vec<PublishedRouteRung> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let bytes = encoded(app);
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let catalog = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.routeRungs())
        .expect("the route rung catalog is published");
    catalog
        .iter()
        .map(|row| PublishedRouteRung {
            rung_key: row.rungKey().expect("a rung publishes its key").to_string(),
            order: row.order(),
            display_name: row
                .displayName()
                .expect("a rung publishes a display name")
                .to_string(),
            verb: row
                .verb()
                .expect("the verb is published, empty or not")
                .to_string(),
            unlock_knowledge: row
                .unlockKnowledge()
                .expect("the gate is published, empty or not")
                .to_string(),
            requires_rung: row
                .requiresRung()
                .expect("the rung beneath is published, empty or not")
                .to_string(),
            work_cost: row.workCost(),
            upkeep_work_per_turn: row.upkeepWorkPerTurn(),
            friction_multiplier: row.frictionMultiplier(),
            holds_link_to_tiles: row.holdsLinkToTiles(),
            grants_sight: row.grantsSight(),
            earns_knowledge: row
                .earnsKnowledge()
                .expect("the lesson is published, empty or not")
                .to_string(),
            build_work_per_worker_turn: row.buildWorkPerWorkerTurn(),
            build_material_cost: row.buildMaterialCost(),
            build_material_id: row
                .buildMaterialId()
                .expect("the material id is published, empty or not")
                .to_string(),
        })
        .collect()
}

/// ⛔ **A PRICED RUNG NAMES WHAT IT EATS, AND A FREE ONE NAMES NOTHING.**
///
/// `buildMaterialCost` shipped as a bare float on the reasoning that *"the branch eats exactly one
/// material"* — true of the **amount** and not of the **name**. The client's rung row read
/// **"+ 20 to raise it"**: twenty of what? An amount with no noun cannot be rendered into a sentence.
///
/// ⛔ **AND THE CLIENT MUST NOT SUPPLY THE NOUN.** *"The route branch eats stone"* is a fact about
/// `intensification_ladder.json`, so a client holding it is a second authority that goes stale in
/// silence the day a rung is retuned — the transcription mistake `buildWorkPerWorkerTurn` beside it
/// exists to have prevented. Which is why this asserts the id against the **rung record's own**
/// declared material rather than against the literal `"stone"`: a retune must reach the wire with no
/// edit here.
///
/// **The pairing is asserted BOTH WAYS**, because either half alone passes on a broken wire: a rung
/// that eats something must name it, and a rung that eats nothing must name nothing — an id beside a
/// zero amount is as meaningless as an amount beside no id.
#[test]
fn a_route_rung_that_eats_a_material_publishes_which_one() {
    let app = spawn_world();
    let ladder = LadderConfig::builtin();
    let published = published_route_rungs(&app);
    let mut named = 0;
    for row in &published {
        let rung = route_rungs_in_climb_order(&ladder)
            .into_iter()
            .find(|def| def.wire_key() == row.rung_key)
            .expect("every published rung is one the config declares");
        let declared: Option<(&str, f32)> = rung.build_materials().next();
        match declared {
            Some((id, amount)) => {
                assert_eq!(
                    row.build_material_id, id,
                    "{}: the published noun is the rung record's own, never a client-side \
                     transcription",
                    row.rung_key
                );
                assert_eq!(
                    row.build_material_cost, amount,
                    "{}: and the amount beside it is that same declaration - the pair is resolved \
                     from one lookup so it cannot disagree",
                    row.rung_key
                );
                named += 1;
            }
            None => assert!(
                row.build_material_id.is_empty(),
                "{}: a rung that eats nothing must name nothing - an id beside a zero amount is a \
                 noun with no quantity, which says as little as a quantity with no noun",
                row.rung_key
            ),
        }
    }
    assert_eq!(
        named, 1,
        "**LIVENESS**: exactly one route rung eats a material on the shipped ladder \
         (`route:paved_road`); a catalog publishing an empty id everywhere would satisfy the `None` \
         arm on every row and assert nothing at all"
    );
}

/// ⛔ **THE PILE ON THE CATALOG IS THE LADDER'S OWN, AND IT IS FLAT.**
///
/// Two claims, and the second is the one that needs a test. The first is the ordinary catalog rule:
/// the figure is read off the rung record, so a retune reaches the wire with no edit here.
///
/// **The second is the asymmetry.** `workCost` beside it is quoted *unscaled* because a tile's own
/// `keeperRemoteness` still has to be applied to it; `buildMaterialCost` takes no such multiplier at
/// all, so the catalog figure is already the whole truth for every tile on the map. A client that
/// scaled the stone the way it scales the work would over-quote every remote road — which is why
/// this asserts against the **unscaled ladder record** rather than against anything a tile carries.
#[test]
fn the_route_rung_catalog_publishes_the_flat_material_pile() {
    let app = spawn_world();
    let ladder = LadderConfig::builtin();
    let published = published_route_rungs(&app);
    let mut rungs_that_eat = 0;
    for row in &published {
        let rung = route_rungs_in_climb_order(&ladder)
            .into_iter()
            .find(|def| def.wire_key() == row.rung_key)
            .expect("every published rung is one the config declares");
        let declared: f32 = rung.build_materials().map(|(_, pile)| pile).sum();
        assert_eq!(
            row.build_material_cost, declared,
            "{}: the published pile is the rung's own, unscaled and unrounded",
            row.rung_key
        );
        if declared > 0.0 {
            rungs_that_eat += 1;
        }
    }
    assert_eq!(
        rungs_that_eat, 1,
        "**LIVENESS**: exactly one route rung eats a material on the shipped ladder \
         (`route:paved_road`); a catalog publishing 0 everywhere would pass the loop above and \
         assert nothing"
    );
}

/// ⛔ **THE CATALOG IS `intensification_ladder.json`'S OWN ROUTE BRANCH, IN CLIMB ORDER** — one row
/// per rung the config declares, every value read off that rung's record.
///
/// **This is what lets a client draw a ladder of rungs nothing has built yet**, and it is asserted
/// against the *records* rather than against literals for the reason the whole catalog exists: a
/// rung added to the config, or a figure retuned on one, must reach the wire with no edit here and
/// none on the client. The liveness half is the shipped climb above — a catalog that published
/// nothing, or published the plant branch, fails the count and the keys before any figure is read.
#[test]
fn the_route_rung_catalog_is_the_configs_own_climb() {
    let ladder = LadderConfig::builtin();
    let declared = route_rungs_in_climb_order(&ladder);
    assert_eq!(
        declared.len(),
        SHIPPED_ROUTE_CLIMB.len(),
        "the shipped ladder declares the four route rungs the coded climb names"
    );

    let app = spawn_world();
    let published = published_route_rungs(&app);
    assert_eq!(
        published.len(),
        declared.len(),
        "one published row per rung the config declares"
    );

    for (index, (row, rung)) in published.iter().zip(declared.iter()).enumerate() {
        let key = SHIPPED_ROUTE_CLIMB[index];
        assert_eq!(
            row.rung_key,
            key.wire_key(),
            "row {index} is the rung the climb puts there"
        );
        assert_eq!(row.rung_key, rung.wire_key(), "…and the record's own key");
        assert_eq!(row.order, rung.order, "the record's own climb order");
        assert_eq!(
            row.verb,
            rung.verb.clone().unwrap_or_default(),
            "{} publishes the verb its record declares",
            row.rung_key
        );
        assert_eq!(
            row.unlock_knowledge,
            rung.unlock_knowledge.clone().unwrap_or_default(),
            "{} publishes the knowledge its record waits on",
            row.rung_key
        );
        assert_eq!(
            row.requires_rung,
            rung.requires_rung_wire_key().unwrap_or_default(),
            "{} names the rung beneath it, branch-qualified",
            row.rung_key
        );
        assert_eq!(
            row.work_cost,
            rung.build_cost(RUNG_COST_UNSCALED).unwrap_or(NO_BUILD_WORK),
            "{} publishes its record's unscaled build cost",
            row.rung_key
        );
        assert_eq!(
            row.upkeep_work_per_turn,
            rung.upkeep
                .as_ref()
                .map_or(NO_UPKEEP_DEMAND, |upkeep| upkeep.work_per_turn),
            "{} publishes its record's unscaled standing rate",
            row.rung_key
        );
        let payoff = rung
            .route_payoff
            .expect("every route rung declares a payoff");
        assert_eq!(row.friction_multiplier, payoff.friction_multiplier);
        assert_eq!(row.holds_link_to_tiles, payoff.holds_link_to_tiles);
        assert_eq!(
            row.grants_sight,
            rung_grants_sight(rung),
            "{} publishes whether a road standing there lights its tile",
            row.rung_key
        );
        assert_eq!(
            row.earns_knowledge,
            rung.earns_knowledge.clone().unwrap_or_default(),
            "{} publishes the lesson its record teaches",
            row.rung_key
        );
        // ⛔ **THE BARE RATE IS THE SIM'S, PUBLISHED ON EVERY ROW** — the one catalog field that is
        // not the config rung's. A road has no source row to carry it, so a client without this has
        // to transcribe `PER_WORKER_OUTPUT`; asserted against the sim's own sum-of-terms seam at
        // no gear, so a second term landing there moves this with it rather than past it.
        assert_eq!(
            row.build_work_per_worker_turn,
            build_work_per_worker_turn(NO_BUILD_GEAR),
            "{} publishes the sim's bare per-worker work rate",
            row.rung_key
        );
    }

    // …and it is the SAME figure on every rung, which is what makes it a catalog fact rather than a
    // per-rung one: a ladder quotes one rate for the whole branch.
    let rates: Vec<f32> = published
        .iter()
        .map(|row| row.build_work_per_worker_turn)
        .collect();
    assert!(
        rates.windows(2).all(|pair| pair[0] == pair[1]),
        "the bare work rate is one number for the branch, not a per-rung one: {rates:?}"
    );
    assert!(
        rates[0] > NO_BUILD_GEAR,
        "a rate of zero would leave every reader with no estimate at all"
    );
}

/// ⛔ **WHICH RUNG TEACHES WHICH LESSON IS PUBLISHED, BECAUSE IT CANNOT BE INFERRED** — the REMEDY
/// half of a gate.
///
/// `unlockKnowledge` says what a rung waits on; `earnsKnowledge` says where that lesson is learned,
/// and a client that recovered the second from `requiresRung` would be reading a coincidence. On the
/// shipped ladder the trail both teaches `roadbuilding` and sits beneath the rung it gates — a
/// config that separates them is legal, and the inference would then name the wrong rung in the one
/// place a player is being told what to go and do.
///
/// Asserted against the **records**, so moving `earns_knowledge` in the config moves this with it;
/// the shipped pairing beneath is the liveness half.
#[test]
fn the_catalog_names_the_rung_that_teaches_each_gate() {
    let ladder = LadderConfig::builtin();
    let app = spawn_world();
    let published = published_route_rungs(&app);

    // Every gate on the branch is answered by some rung's lesson — the join the client makes.
    for row in &published {
        if row.unlock_knowledge.is_empty() {
            continue;
        }
        let teacher = published
            .iter()
            .find(|other| other.earns_knowledge == row.unlock_knowledge)
            .unwrap_or_else(|| {
                panic!(
                    "{} waits on '{}', and some rung on the branch teaches it",
                    row.rung_key, row.unlock_knowledge
                )
            });
        assert_ne!(
            teacher.rung_key, row.rung_key,
            "a rung cannot teach the lesson that gates it"
        );
    }

    // …and the shipped answer, read off the records rather than restated: exactly the rungs whose
    // `earns_knowledge` is set publish a lesson, and the rest publish none.
    for (row, rung) in published
        .iter()
        .zip(route_rungs_in_climb_order(&ladder).iter())
    {
        match rung.earns_knowledge.as_deref() {
            Some(lesson) => assert_eq!(
                row.earns_knowledge, lesson,
                "{} teaches its record's lesson",
                row.rung_key
            ),
            None => assert!(
                row.earns_knowledge.is_empty(),
                "{} teaches nothing, and publishes nothing",
                row.rung_key
            ),
        }
    }

    let teaching: Vec<&str> = published
        .iter()
        .filter(|row| !row.earns_knowledge.is_empty())
        .map(|row| row.earns_knowledge.as_str())
        .collect();
    assert!(
        !teaching.is_empty(),
        "the branch teaches something — the liveness half of the loop above"
    );
    assert!(
        published[0].earns_knowledge.is_empty(),
        "the floor teaches nothing: you wear a path by walking it"
    );
    assert!(
        published[published.len() - 1].earns_knowledge.is_empty(),
        "and the top of the branch has nothing above it to open"
    );
}

/// ⛔ **THE FREE FLOOR NAMES NO VERB AND THE FLOOR REQUIRES NOTHING** — the two facts a ladder
/// readout has to render differently, pinned on the published rows.
///
/// A path and a trail are formed by **use**: there is no command to name the job, nothing to staff
/// and nothing to pay, so a client must not draw a build button on either. And the floor is where
/// every road already stands, so it waits on no rung beneath it — the `""` that ends the chain.
#[test]
fn the_catalogs_free_floor_names_no_verb_and_its_floor_requires_nothing() {
    let app = spawn_world();
    let published = published_route_rungs(&app);
    let ladder = LadderConfig::builtin();

    let floor = &published[0];
    assert_eq!(
        floor.rung_key,
        RungKey::RoutePath.wire_key(),
        "the catalog opens at the branch's floor"
    );
    assert!(
        floor.requires_rung.is_empty(),
        "the floor waits on nothing beneath it"
    );
    assert_eq!(
        floor.display_name, "Path",
        "the id, read as a player reads it"
    );

    for row in &published {
        let key = SHIPPED_ROUTE_CLIMB
            .iter()
            .copied()
            .find(|key| key.wire_key() == row.rung_key)
            .expect("every published row is a rung the coded climb names");
        if key.is_at_or_above(FIRST_BUILT_RUNG) {
            assert!(
                !row.verb.is_empty(),
                "{} is built by a command, so it names one",
                row.rung_key
            );
            assert!(
                row.upkeep_work_per_turn > NO_UPKEEP_DEMAND,
                "{} costs work to hold",
                row.rung_key
            );
            assert!(
                row.grants_sight,
                "{} is paid for, and paying the bill IS the presence",
                row.rung_key
            );
        } else {
            assert!(
                row.verb.is_empty(),
                "{} is formed by use — there is no command to name",
                row.rung_key
            );
            assert_eq!(
                row.upkeep_work_per_turn, NO_UPKEEP_DEMAND,
                "{} costs nothing to hold",
                row.rung_key
            );
            assert!(
                !row.grants_sight,
                "{} lights nothing, however worn it is",
                row.rung_key
            );
        }
    }

    assert_eq!(
        published.iter().filter(|row| row.verb.is_empty()).count(),
        route_rungs_in_climb_order(&ladder)
            .iter()
            .filter(|rung| rung.verb.is_none())
            .count(),
        "as many verb-less rows as the config declares verb-less rungs"
    );
}
