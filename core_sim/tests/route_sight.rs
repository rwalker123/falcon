//! **A kept road holds its own tiles `Seen`, and the connection keystone is untouched** (issue
//! #532, `docs/plan_standing_upkeep.md` §4.13, `.claude/rules/core_sim/routes.md`).
//!
//! Ray: *"If a road exists and is maintained, the assumption is that there is traffic on it and it
//! is seen."* **Maintenance is not free** — a kept road bills a band every turn out of its
//! `Roadwork` pool, and what those hands are doing is being on the road. **Paying the upkeep IS the
//! presence**, which is why this grant is an *instance* of the keystone rather than an exception to
//! it: the sight comes from the **road**, and never from a connection.
//!
//! These drive **whole turns** through [`core_sim::build_test_app`] rather than poking the
//! visibility ledger directly, because the thing under test is precisely that something *hands*
//! `Route::grants_sight` to the fog: a fixture that ran the sweep by hand would keep passing on a
//! sim where the pass was never scheduled.
//!
//! **Every containment claim below is paired with a liveness one** — "the road is dark" also passes
//! on a sim where roads light nothing at all, so each dark assertion sits beside a lit one struck in
//! the same world or the same turn.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    build_test_app, BandId, Connection, ConnectionKey, ConnectionLedger, ConnectionsConfig,
    FactionId, LaborAllocation, LaborTarget, LadderConfig, PopulationCohort, ResidentBand, Route,
    RouteId, RouteLedger, RungKey, SimulationConfig, StartingUnit, Tile, TileRegistry,
    VisibilityLedger, VisibilityState, PER_WORKER_OUTPUT,
};

/// A pinned earthlike world, so the terrain under every road below is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// **How long every fixture's road is, in tiles.** Comfortably past the widest configured sight a
/// band can reach (base range 6 plus an elevation bonus capped at 4), so the far end of a road is
/// ground the camp standing on its head cannot possibly see for itself. Each test asserts that
/// precondition rather than trusting this number.
const ROAD_TILES: u32 = 14;

/// The tile of the road the assertions are struck at — the far end, which is the only end that can
/// distinguish the road's grant from the band's own eyes.
const FAR_END: usize = (ROAD_TILES - 1) as usize;

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The campaign's first resident band: entity, faction and the tile it stands on.
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
    (entity, faction, position)
}

/// A straight run of `tiles` tiles starting at `head`, clamped inside the map so every tile of it is
/// real ground a faction map can carry a state for.
fn road_from(app: &App, head: UVec2, tiles: u32) -> Vec<UVec2> {
    let width = app.world.resource::<TileRegistry>().width;
    (0..tiles)
        .map(|step| UVec2::new((head.x + step) % width, head.y))
        .collect()
}

/// Lay a road along `path` and seat it exactly at the top of the **trail** rung — the cheapest rung
/// anybody maintains, so a fixture asks the smallest bill that can be met or missed.
fn seat_a_trail(app: &mut App, path: Vec<UVec2>) -> RouteId {
    let ladder = LadderConfig::builtin();
    let seated = trail_cost(&ladder);
    let mut routes = app.world.resource_mut::<RouteLedger>();
    let id = routes.insert(path, &ladder);
    routes
        .get_mut(id)
        .expect("the road was just laid")
        .set_position(seated, &ladder);
    id
}

/// The shipped `route:trail` price, read from the ladder rather than restated — a retune of
/// `intensification_ladder.json` must move these fixtures with it, not break them.
fn trail_cost(ladder: &LadderConfig) -> f32 {
    ladder
        .rung(RungKey::RouteTrail)
        .build
        .as_ref()
        .expect("the trail rung is built")
        .work_cost
}

/// **A GAME TRAIL WITH REAL WORK WORN INTO IT, as a fraction of the trail rung's own cost** — which
/// is what every road in the game looks like for the whole of its first fifty-odd turns.
///
/// It has to carry *some* work: a game trail with **nothing** banked is indistinguishable from no
/// road at all, and `advance_routes` prunes it on the turn it is laid (the route arc's rule 3). Any
/// fraction strictly inside `(0, 1)` says the same thing; a half is the least arbitrary of them.
const WORN_BUT_NOT_YET_A_TRAIL: f32 = 0.5;

/// Lay a road along `path` and leave it at the branch's **floor** — a game trail, which nobody
/// maintains and which therefore lights nothing however long it runs.
fn seat_a_game_trail(app: &mut App, path: Vec<UVec2>) -> RouteId {
    let ladder = LadderConfig::builtin();
    let worn = trail_cost(&ladder) * WORN_BUT_NOT_YET_A_TRAIL;
    let mut routes = app.world.resource_mut::<RouteLedger>();
    let id = routes.insert(path, &ladder);
    routes
        .get_mut(id)
        .expect("the road was just laid")
        .set_position(worn, &ladder);
    id
}

fn route(app: &App, id: RouteId) -> &Route {
    app.world
        .resource::<RouteLedger>()
        .get(id)
        .expect("the road is still in the ledger")
}

/// **HOW MANY BARE HANDS COVER THIS ROAD'S BILL IN FULL** — `ceil(demand / PER_WORKER_OUTPUT)`, read
/// off the sim's own stamped bill rather than hard-coded, because the span is whatever the generated
/// map's terrain under the road happens to cost.
fn keepers_the_bill_wants(app: &App, id: RouteId) -> u32 {
    let ladder = LadderConfig::builtin();
    let route = route(app, id);
    let registry = app.world.resource::<TileRegistry>();
    let span = core_sim::span_of_terrains(route.path.iter().filter_map(|pos| {
        registry
            .index(pos.x, pos.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .map(|tile| tile.terrain)
    }));
    let demand = core_sim::route_upkeep_demand(route, span, &ladder);
    assert!(
        demand > 0.0,
        "precondition: a seated trail really does owe something ({demand})"
    );
    (demand / PER_WORKER_OUTPUT).ceil() as u32
}

fn staff_roadwork(app: &mut App, band: Entity, workers: u32) {
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Roadwork, workers, workers.max(1), None);
    app.world.entity_mut(band).insert(allocation);
}

/// **A SECOND PEOPLE, CAMPED WHERE WE ASK** - a copy of the campaign band's own cohort under a new
/// faction and a new [`BandId`], with its `roadwork` role staffed.
///
/// It exists because *"a faction with nobody on a kept road"* is **only reachable across factions**:
/// `settle_route_keeping` pays a road from the bands standing on it, so within one people a road
/// that is kept is by construction a road somebody of yours is on. A stranger keeping it is the only
/// fixture in which the road is genuinely **kept** and genuinely **not ours**.
fn plant_a_stranger(app: &mut App, at: UVec2, faction: FactionId, keepers: u32) -> Entity {
    /// The id the stranger band takes - far above anything the campaign allocates.
    const STRANGER_BAND_ID: u64 = 9_700;

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
    app.world
        .spawn((
            cohort,
            unit,
            ResidentBand,
            BandId(STRANGER_BAND_ID),
            allocation,
        ))
        .id()
}

fn state_at(app: &App, faction: FactionId, pos: UVec2) -> VisibilityState {
    app.world
        .resource::<VisibilityLedger>()
        .visibility_state(faction, pos.x, pos.y)
}

// ---------------------------------------------------------------------------------------------
// THE HEADLINE, and both halves are load-bearing
// ---------------------------------------------------------------------------------------------

/// ⛔ **A KEPT ROAD LIGHTS ITS WHOLE PATH; THE SAME ROAD IN SHORTFALL GOES DARK — AND IT GOES DARK
/// BEFORE IT DECAYS.**
///
/// The two halves are one test because either alone proves nothing: the lit half passes on a sim
/// that lights every tile of every road whatever its bill, and the dark half passes on a sim where a
/// road lights nothing at all.
///
/// **The far end is the whole measurement.** It is asserted `Unexplored` in the shortfall world, so
/// the lit world's `Active` cannot be the band's own eyes reaching it — that is the distance
/// precondition, struck against the sim rather than against a number.
///
/// **`Active`, not `Discovered`.** A road grants exactly what a band's own camp grants; it is the
/// *road* that is the presence.
#[test]
fn a_kept_road_lights_its_whole_path_and_the_same_road_in_shortfall_goes_dark_before_it_decays() {
    // ① Kept: the bill is met, so the road holds its tiles Seen.
    let mut kept = spawn_world();
    let (band, faction, camp) = first_band(&mut kept);
    let path = road_from(&kept, camp, ROAD_TILES);
    let far = path[FAR_END];
    let road = seat_a_trail(&mut kept, path.clone());
    let wanted = keepers_the_bill_wants(&kept, road);
    staff_roadwork(&mut kept, band, wanted);
    kept.update();

    assert!(
        route(&kept, road).grants_sight(),
        "precondition: a built road whose bill was met is a road that lights its tiles"
    );
    for tile in &path {
        assert_eq!(
            state_at(&kept, faction, *tile),
            VisibilityState::Active,
            "every tile of a kept road is Seen, including {tile} at the far end"
        );
    }

    // ② The same road with the role empty: the bill goes unpaid and the road goes dark.
    let mut short = spawn_world();
    let (band, faction, camp) = first_band(&mut short);
    let path = road_from(&short, camp, ROAD_TILES);
    let road = seat_a_trail(&mut short, path.clone());
    staff_roadwork(&mut short, band, 0);
    short.update();

    assert!(
        !route(&short, road).grants_sight(),
        "precondition: a road nobody paid for is not lighting anything"
    );
    assert_eq!(
        state_at(&short, faction, far),
        VisibilityState::Unexplored,
        "⛔ THE DISTANCE PRECONDITION: the far end of the road is ground the camp's own sight \
         cannot reach, so the kept world's Active reading above came from the ROAD"
    );

    // ⛔ **AND IT WENT DARK BEFORE IT DECAYED** — the honest early warning. The trail rung's grace
    // has not run out after one short turn, so the road still holds exactly the rung it was seated
    // at while already showing nothing.
    assert_eq!(
        route(&short, road).held_rung(),
        RungKey::RouteTrail,
        "the road is dark on the FIRST short turn, well inside its grace and with its rung intact"
    );
}

/// **A GAME TRAIL LIGHTS NOTHING, HOWEVER LONG IT IS AND HOWEVER WELL FUNDED.**
///
/// **The gate is the BUILT rung, and this fixture is what makes that visible.** The road here is
/// worn half-way to the trail rung, so its bill *interpolates* to half a trail's — a real bill, and
/// this test **funds it in full**. It still lights nothing, because a path the animals made is not a
/// road somebody keeps: `Route::grants_sight` is `is_built() && keeping_is_met()`, and the first
/// half is false at the floor whatever the second says.
///
/// A game trail with **nothing** worn into it cannot be observed at all — `advance_routes` prunes
/// it on the turn it is laid, which is the route arc's rule 3 — so a part-worn one is the only game
/// trail there is to test.
///
/// Paired with a kept trail out of the same camp in the same turn, so "dark" cannot be the whole
/// feature being absent.
#[test]
fn a_game_trail_lights_nothing_however_long_it_is() {
    let mut app = spawn_world();
    let (band, faction, camp) = first_band(&mut app);

    // Two roads out of the same camp, in opposite directions along the row: one at the free floor,
    // one seated at the trail rung and funded.
    let width = app.world.resource::<TileRegistry>().width;
    let free_path = road_from(&app, camp, ROAD_TILES);
    let kept_head = UVec2::new((camp.x + width - ROAD_TILES + 1) % width, camp.y);
    let kept_path = road_from(&app, kept_head, ROAD_TILES);

    let free = seat_a_game_trail(&mut app, free_path.clone());
    let kept = seat_a_trail(&mut app, kept_path.clone());
    // **Both** roads' bills, because the camp stands on both and a half-worn game trail owes a
    // half-worn trail's keeping. Funding only one would leave the other short, and the test would
    // then be measuring a shortfall rather than the rung.
    let wanted = keepers_the_bill_wants(&app, kept) + keepers_the_bill_wants(&app, free);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    assert_eq!(
        route(&app, free).held_rung(),
        RungKey::RouteGameTrail,
        "precondition: the free road is still at the branch's floor"
    );
    assert!(
        route(&app, free).keeping_is_met(),
        "precondition: and its bill was PAID IN FULL, so what follows is about the rung, not the bill"
    );
    assert_eq!(
        state_at(&app, faction, free_path[FAR_END]),
        VisibilityState::Unexplored,
        "nobody maintains a game trail, so its far end is dark"
    );
    // Liveness, in the same world and the same turn: a kept road out of the same camp DOES light.
    assert_eq!(
        state_at(&app, faction, kept_path[0]),
        VisibilityState::Active,
        "and the kept road beside it lights its own far end, so 'dark' is not the feature missing"
    );
}

/// ⛔ **A FACTION WITH NOBODY ON A KEPT ROAD SEES NOTHING FROM IT — AND THE PEOPLE ON IT DO.**
///
/// A road belongs to nobody, so the grant is scoped by *who is standing on it* (rule 2 —
/// `RouteLedger::routes_on_tile` at the band's own tile, and there is no radius).
///
/// **The far road is kept by a STRANGER, and it has to be.** `settle_route_keeping` pays a road from
/// the bands standing on it, so within one people a road that is *kept* is by construction a road
/// somebody of yours is on: a same-faction fixture would be measuring the shortfall, not the
/// scoping. With a second people holding it, the road is genuinely kept and genuinely not ours — and
/// the liveness half is the same road, the same turn, lit for **them**.
#[test]
fn a_faction_with_nobody_on_a_kept_road_sees_nothing_from_it() {
    /// The stranger people's id. Any faction the campaign does not allocate will do.
    const STRANGERS: FactionId = FactionId(41);

    let mut app = spawn_world();
    let (band, faction, camp) = first_band(&mut app);
    let width = app.world.resource::<TileRegistry>().width;
    assert_ne!(
        faction, STRANGERS,
        "the two peoples must be different peoples"
    );

    // The road under our camp, and an identical one a long way off that a stranger keeps.
    let under_path = road_from(&app, camp, ROAD_TILES);
    let away_head = UVec2::new((camp.x + width / 2) % width, camp.y);
    let away_path = road_from(&app, away_head, ROAD_TILES);

    let under = seat_a_trail(&mut app, under_path.clone());
    let away = seat_a_trail(&mut app, away_path.clone());
    let ours = keepers_the_bill_wants(&app, under);
    let theirs = keepers_the_bill_wants(&app, away);
    staff_roadwork(&mut app, band, ours);
    plant_a_stranger(&mut app, away_path[0], STRANGERS, theirs);
    app.update();

    assert!(
        route(&app, away).grants_sight(),
        "precondition: the far road really is KEPT — the stranger paid its bill in full"
    );
    // Liveness: the road under our camp is lit end to end for us…
    assert_eq!(
        state_at(&app, faction, under_path[FAR_END]),
        VisibilityState::Active,
        "the road under our own camp lights its far end for us"
    );
    // …and the far road is lit end to end for THEM, so it is a live grant and not a dead road.
    assert_eq!(
        state_at(&app, STRANGERS, away_path[FAR_END]),
        VisibilityState::Active,
        "and the far road lights its far end for the people standing on it"
    );
    // Containment: that same kept road grants US nothing, because none of ours is on it.
    for tile in &away_path {
        assert_eq!(
            state_at(&app, faction, *tile),
            VisibilityState::Unexplored,
            "a kept road no band of ours stands on grants us nothing at {tile}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// THE KEYSTONE — stated HERE, because this is the file where somebody would break it
// ---------------------------------------------------------------------------------------------

/// ⛔ **ONLY PRESENCE MAKES A TILE `Seen`. A CONNECTION CAN ONLY EVER GRANT `Discovered`.**
///
/// `connections.rs` names **logistics** as the first rider that will be tempted to break this, and
/// the road grant above is the rider that arrived. It does not bend the rule: the sight is granted
/// by the **road** — maintained presence on specific ground — and never by the tie.
///
/// Asserted here as well as in `core_sim/tests/connections.rs` because this is the file where the
/// two mechanisms sit side by side, and therefore the file where routing the road's grant through
/// the connection path would look like a tidy simplification.
///
/// **Paired with the road's own liveness in the same world and the same turn**: a full tie to a
/// people this band has never travelled to lights nothing, while the road under its feet lights
/// everything — so this cannot pass on a sim that grants no sight at all.
#[test]
fn a_live_tie_to_a_people_never_travelled_to_grants_no_active_tile() {
    let mut app = spawn_world();
    let (band, faction, camp) = first_band(&mut app);
    let band_id = *app.world.get::<BandId>(band).expect("a band has an id");
    let width = app.world.resource::<TileRegistry>().width;

    // A full tie pointing at ground on the far side of the map, to a subject id no band carries.
    let told_about = UVec2::new((camp.x + width / 2) % width, camp.y);
    let subject = BandId(u64::MAX);
    let cfg = ConnectionsConfig::default();
    let key = ConnectionKey::new(band_id, subject);
    {
        let mut ties = app.world.resource_mut::<ConnectionLedger>();
        let contacts_to_full = (core_sim::FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil();
        for _ in 0..contacts_to_full as u32 {
            ties.record_contact(key, told_about, 0, 0, &cfg);
        }
    }

    // The same band's own kept road, which is the liveness half.
    let path = road_from(&app, camp, ROAD_TILES);
    let road = seat_a_trail(&mut app, path.clone());
    let wanted = keepers_the_bill_wants(&app, road);
    staff_roadwork(&mut app, band, wanted);
    app.update();

    let tie: Option<Connection> = app.world.resource::<ConnectionLedger>().get(&key).copied();
    assert!(
        tie.is_some_and(|tie| tie.strength.to_f32() > 0.0),
        "precondition: the seeded tie really survived the turn, so this world ran connections"
    );
    assert_ne!(
        state_at(&app, faction, told_about),
        VisibilityState::Active,
        "⛔ THE KEYSTONE: a connection may only ever grant Discovered — it must never make a tile Seen"
    );
    assert_eq!(
        state_at(&app, faction, path[FAR_END]),
        VisibilityState::Active,
        "and the road under the same band's feet DOES grant Seen, so this world grants sight at all"
    );
}
