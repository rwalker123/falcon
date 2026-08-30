//! **A kept road holds its own tile `Seen` for its KEEPER, and the connection keystone is
//! untouched** (issue #532, `docs/plan_standing_upkeep.md` §4.13b,
//! `.claude/rules/core_sim/routes.md`).
//!
//! Ray: *"If a road exists and is maintained, the assumption is that there is traffic on it and it
//! is seen."* **Maintenance is not free** — a kept road bills its keeper every turn out of that
//! band's `Roadwork` pool, and what those hands are doing is being on the road. **Paying the upkeep
//! IS the presence**, which is why this grant is an *instance* of the keystone rather than an
//! exception to it: the sight comes from the **road**, and never from a connection.
//!
//! These drive **whole turns** through [`core_sim::build_test_app`] rather than poking the
//! visibility ledger directly, because the thing under test is precisely that something *hands*
//! `Road::grants_sight` to the fog: a fixture that ran the sweep by hand would keep passing on a sim
//! where the pass was never scheduled.
//!
//! **Every containment claim below is paired with a liveness one** — "the road is dark" also passes
//! on a sim where roads light nothing at all, so each dark assertion sits beside a lit one struck in
//! the same world or the same turn.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    build_test_app, BandId, ConnectionKey, ConnectionLedger, ConnectionsConfig, FactionId,
    LaborAllocation, LaborTarget, LadderConfig, PopulationCohort, ResidentBand, Road, RoadKeeper,
    RoadRegistry, RungKey, SimulationConfig, StartingUnit, Tile, TileRegistry, VisibilityLedger,
    VisibilityState, NEAR_ENOUGH_TO_KEEP, PER_WORKER_OUTPUT,
};

/// A pinned earthlike world, so the terrain under every road below is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// **How far from its keeper's camp the measured road tile sits**, in tiles. Comfortably past the
/// widest configured sight a band can reach (base range 6 plus an elevation bonus capped at 4), so
/// the tile is ground the camp cannot possibly see for itself. Each test asserts that precondition
/// rather than trusting this number.
const ROAD_DISTANCE: u32 = 14;

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The campaign's first resident band: entity, faction, its `BandId` and the tile it stands on.
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
    (entity, faction, band, position)
}

/// A tile `steps` east of `head`, wrapped inside the map so it is real ground a faction map can
/// carry a state for.
fn tile_east_of(app: &App, head: UVec2, steps: u32) -> UVec2 {
    let width = app.world.resource::<TileRegistry>().width;
    UVec2::new((head.x + steps) % width, head.y)
}

/// The cumulative position at which a road **holds** `route:dirt_road`, read from the ladder rather
/// than restated — a retune of `intensification_ladder.json` must move these fixtures with it.
fn dirt_road_top(ladder: &LadderConfig) -> f32 {
    let (base, width) =
        core_sim::road_rung_span(RungKey::RouteDirtRoad, ladder, NEAR_ENOUGH_TO_KEEP);
    base + width
}

/// Seat a road on `tile` at `position` and hand it to `keeper`.
///
/// **The position is written BEFORE the keeper**, and the order is load-bearing: `set_position`
/// releases a keeper on a road that has fallen back into the free floor, so seating a keeper first
/// would hand it straight back.
fn seat_at(app: &mut App, tile: UVec2, position: f32, keeper: Option<(FactionId, BandId)>) {
    let ladder = LadderConfig::builtin();
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.set_position(position, &ladder);
    if let Some((faction, band)) = keeper {
        road.take_keeper(RoadKeeper { faction, band }, NEAR_ENOUGH_TO_KEEP, &ladder);
    }
}

/// Seat a **dirt road** — the cheapest rung anybody maintains
/// (`docs/plan_standing_upkeep.md` §4.13a), so a fixture asks the smallest bill that can be met or
/// missed.
///
/// ⛔ **NOT THE TRAIL.** The trail is the free floor's second storey: it is worn in by traffic and
/// costs nothing to hold, so a fixture seated there would have no bill to meet or miss and every
/// sight claim below would be vacuous.
fn seat_a_dirt_road(app: &mut App, tile: UVec2, keeper: (FactionId, BandId)) {
    seat_at(
        app,
        tile,
        dirt_road_top(&LadderConfig::builtin()),
        Some(keeper),
    );
}

/// Seat a fully worn **trail** and hand it to nobody — the free floor, which nobody maintains and
/// which therefore lights nothing however worn it is.
///
/// **The top of the floor rather than a half-worn game trail**, because the top is where the claim
/// is hardest: a road carrying every work unit traffic can ever put into it still grants no sight,
/// since `Road::grants_sight` reasons from the **paid bill** and the whole floor is free.
fn seat_a_worn_trail(app: &mut App, tile: UVec2) {
    seat_at(
        app,
        tile,
        core_sim::traffic_ceiling(&LadderConfig::builtin()),
        None,
    );
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
    let demand = core_sim::road_upkeep_demand(
        road,
        core_sim::road_upkeep_measure(terrain, road.keeper_remoteness),
        &ladder,
    );
    assert!(
        demand > 0.0,
        "precondition: a seated dirt road really does owe something ({demand})"
    );
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

fn staff_roadwork(app: &mut App, band: Entity, workers: u32) {
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Roadwork, workers, workers.max(1), None);
    app.world.entity_mut(band).insert(allocation);
}

/// **A SECOND PEOPLE, CAMPED WHERE WE ASK** — a copy of the campaign band's own cohort under a new
/// faction and a new [`BandId`], with its `roadwork` role staffed.
fn plant_a_stranger(
    app: &mut App,
    at: UVec2,
    faction: FactionId,
    keepers: u32,
) -> (Entity, BandId) {
    /// The id the stranger band takes — far above anything the campaign allocates.
    const STRANGER_BAND_ID: BandId = BandId(9_700);

    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
        .expect("the stranger's camp is on the map");
    let (mut cohort, unit) = {
        let mut query = app
            .world
            .query_filtered::<(&PopulationCohort, &StartingUnit), With<ResidentBand>>();
        let (cohort, unit) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band to copy");
        (cohort.clone(), unit.clone())
    };
    cohort.faction = faction;
    cohort.current_tile = tile;
    cohort.home = tile;
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Roadwork, keepers, keepers.max(1), None);
    let entity = app
        .world
        .spawn((cohort, unit, ResidentBand, STRANGER_BAND_ID, allocation))
        .id();
    (entity, STRANGER_BAND_ID)
}

fn state_at(app: &App, faction: FactionId, pos: UVec2) -> VisibilityState {
    app.world
        .resource::<VisibilityLedger>()
        .visibility_state(faction, pos.x, pos.y)
}

// ---------------------------------------------------------------------------------------------
// THE HEADLINE, and both halves are load-bearing
// ---------------------------------------------------------------------------------------------

/// ⛔ **A KEPT ROAD LIGHTS ITS OWN TILE; THE SAME ROAD IN SHORTFALL GOES DARK — AND IT GOES DARK
/// BEFORE IT DECAYS.**
///
/// The two halves are one test because either alone proves nothing: the lit half passes on a sim
/// that lights every road tile whatever its bill, and the dark half passes on a sim where a road
/// lights nothing at all.
///
/// **The distance is the whole measurement.** The tile is asserted `Unexplored` in the shortfall
/// world, so the lit world's `Active` cannot be the band's own eyes reaching it — that is the
/// precondition, struck against the sim rather than against a number.
///
/// **`Active`, not `Discovered`.** A road grants exactly what a band's own camp grants; it is the
/// *road* that is the presence.
#[test]
fn a_kept_road_lights_its_tile_and_the_same_road_in_shortfall_goes_dark_before_it_decays() {
    // ① Kept: the bill is met, so the road holds its tile Seen.
    let mut kept = spawn_world();
    let (band, faction, id, camp) = first_band(&mut kept);
    let far = tile_east_of(&kept, camp, ROAD_DISTANCE);
    seat_a_dirt_road(&mut kept, far, (faction, id));
    let wanted = keepers_the_bill_wants(&kept, far);
    staff_roadwork(&mut kept, band, wanted);
    kept.update();

    assert!(
        road_at(&kept, far).grants_sight(),
        "precondition: a built road whose bill was met is a road that lights its tile"
    );
    assert_eq!(
        state_at(&kept, faction, far),
        VisibilityState::Active,
        "a kept road's tile is SEEN for its keeper's people — paying the upkeep IS the presence"
    );

    // ② Short: nobody on the role, so the bill is missed and the road goes dark — while the rung is
    //    still standing, which is the honest early warning.
    let mut short = spawn_world();
    let (band, faction, id, camp) = first_band(&mut short);
    let far = tile_east_of(&short, camp, ROAD_DISTANCE);
    seat_a_dirt_road(&mut short, far, (faction, id));
    staff_roadwork(&mut short, band, 0);
    short.update();

    assert_eq!(
        road_at(&short, far).held_rung(),
        RungKey::RouteDirtRoad,
        "the road has NOT decayed yet — it goes dark first"
    );
    assert!(
        road_at(&short, far).upkeep_shortfall() > 0.0,
        "precondition: this road really is short"
    );
    assert_eq!(
        state_at(&short, faction, far),
        VisibilityState::Unexplored,
        "a road in SHORTFALL lights nothing — and this is also the distance precondition for ①: \
         the camp's own eyes do not reach this tile"
    );
}

/// ⛔ **THE FREE FLOOR LIGHTS NOTHING AND OWES NOTHING, HOWEVER WORN IT IS** (§4.13a).
///
/// A fully worn trail is the hardest case for the claim: every work unit traffic can ever bank is in
/// it, and it still grants no sight — because nobody keeps it, and the grant reasons from the paid
/// bill.
///
/// **Paired with a dirt road in the same world**, which does light its tile, so the dark half is not
/// passing on a build where roads light nothing.
#[test]
fn the_free_floor_lights_nothing_and_owes_nothing_however_worn_it_is() {
    let mut app = spawn_world();
    let (band, faction, id, camp) = first_band(&mut app);
    let trail = tile_east_of(&app, camp, ROAD_DISTANCE);
    let road = tile_east_of(&app, camp, ROAD_DISTANCE + 1);
    seat_a_worn_trail(&mut app, trail);
    seat_a_dirt_road(&mut app, road, (faction, id));
    let wanted = keepers_the_bill_wants(&app, road);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    assert_eq!(
        road_at(&app, trail).held_rung(),
        RungKey::RouteTrail,
        "precondition: the trail really is fully worn"
    );
    assert_eq!(
        road_at(&app, trail).keeper,
        None,
        "and it is nobody's job — the free floor has no keeper"
    );
    assert!(
        !road_at(&app, trail).grants_sight(),
        "a fully worn trail grants no sight: it costs nothing to hold, so nobody is on it"
    );
    assert_eq!(
        state_at(&app, faction, trail),
        VisibilityState::Unexplored,
        "so its tile is dark"
    );
    assert_eq!(
        state_at(&app, faction, road),
        VisibilityState::Active,
        "the liveness half: the kept dirt road one tile over DOES light its own"
    );
}

/// ⛔ **THE FOG IT LIFTS IS THE KEEPER'S, AND NOBODY ELSE'S.**
///
/// A road tile is one band's job, so the faction that sees it is that band's. A people with no claim
/// on the road sees nothing from it however close they camp — which is what makes the grant *paid
/// presence* rather than proximity.
#[test]
fn a_faction_that_does_not_keep_a_road_sees_nothing_from_it() {
    const STRANGERS: FactionId = FactionId(4_242);

    let mut app = spawn_world();
    let (_, ours, _, camp) = first_band(&mut app);
    let far = tile_east_of(&app, camp, ROAD_DISTANCE);
    // The stranger camps ON the far tile and keeps the road there; our own band never touches it.
    let (stranger, stranger_id) = plant_a_stranger(&mut app, far, STRANGERS, 0);
    seat_a_dirt_road(&mut app, far, (STRANGERS, stranger_id));
    let wanted = keepers_the_bill_wants(&app, far);
    staff_roadwork(&mut app, stranger, wanted);
    app.update();

    assert!(
        road_at(&app, far).grants_sight(),
        "precondition: the road really is kept — this is about WHOSE fog it lifts, not whether"
    );
    assert_eq!(
        state_at(&app, STRANGERS, far),
        VisibilityState::Active,
        "the liveness half: the keeper's own people see the tile they pay for"
    );
    assert_eq!(
        state_at(&app, ours, far),
        VisibilityState::Unexplored,
        "and a people with no claim on that road sees nothing from it — the grant is the KEEPER's"
    );
}

/// ⛔ **THE CONNECTION KEYSTONE DOES NOT BEND.** A live tie to a people never travelled to grants
/// `Discovered` at most, and never `Active` — `connections.rs` states it as inviolable and names
/// **logistics** as the first rider that will be tempted to break it.
///
/// This test exists beside the road grant precisely because the road grant *looks* like that
/// temptation. It is not: the sight is granted by the **road** — maintained presence on specific
/// ground — and never by the edge.
#[test]
fn a_live_tie_to_a_people_never_travelled_to_grants_no_active_tile() {
    const STRANGERS: FactionId = FactionId(4_243);

    let mut app = spawn_world();
    let (_, ours, our_band, camp) = first_band(&mut app);
    let far = tile_east_of(&app, camp, ROAD_DISTANCE);
    let (_, stranger_id) = plant_a_stranger(&mut app, far, STRANGERS, 0);
    {
        let cfg = ConnectionsConfig::default();
        let key = ConnectionKey::new(our_band, stranger_id);
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for _ in 0..((core_sim::FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil() as u32) {
            ledger.record_contact(key, far, 0, 0, &cfg);
        }
    }
    app.update();

    assert!(
        app.world
            .resource::<ConnectionLedger>()
            .get(&ConnectionKey::new(our_band, stranger_id))
            .is_some_and(|tie| tie.strength.to_f32() > 0.0),
        "precondition: the tie really is live"
    );
    assert_ne!(
        state_at(&app, ours, far),
        VisibilityState::Active,
        "ONLY PRESENCE MAKES A TILE SEEN — a connection can only ever grant Discovered"
    );
}
