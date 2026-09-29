//! **THE WORK PARTY AS A CARAVAN, DRIVEN THROUGH REAL TURNS AND READ OFF THE ENCODED WIRE**
//! (`docs/plan_civilization_steps.md` §One work party, `.claude/rules/core_sim/work-party.md`).
//!
//! A band camped on the harness map's `(1, 1)` hunts a herd — or works a wood — seated a stated
//! number of hexes along the same row, so the hex distance is exactly that number. Every claim here is
//! asserted on what a client parses — the encoded snapshot, or the query's answer — never on an
//! in-process value the capture might not carry.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, BandEquipment, BandId, EquipmentConfig, FactionId, FaunaConfigHandle,
    GenerationId, Herd, HerdRegistry, KitChoice, LaborAllocation, LaborAssignment, LaborTarget,
    LocalStore, MoraleCause, PopulationCohort, ResidentBand, SizeClass, SnapshotHistory,
    SourcePriority, TileRegistry, FOOD,
};
use sim_runtime::commands::{
    QueryPayload, QueryReply, WorkPartyForecastQuery, WorkPartyForecastReply, WorkPartySource,
};

/// A shipped quarry whose take a small party can actually make.
const BOAR: &str = "Wild Boar";
const HERD_ID: &str = "caravan_probe";
const BAND: u64 = 23;
const FACTION: FactionId = FactionId(0);
/// Where the band camps — a tile the harness map carries.
const CAMP: UVec2 = UVec2::new(1, 1);
/// The crew on the row. Several, so a caravan can have hunters on the road and at the source at
/// once.
const CREW: u32 = 6;
const FLOOR: f32 = 0.5;
/// A herd big enough that the crew's take is never what runs out.
const STANDING_STOCK: f32 = 5_000.0;
/// A bound on how long a caravan may take to put somebody on the road. A guard against hanging,
/// not a prediction.
const TURNS_TO_SEE_A_PORTER: usize = 60;

/// The whole of what one row publishes about its party, read back out of the ENCODED buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PublishedParty {
    party_workers: u32,
    hunters_on_the_road: u32,
    walk_tiles: u32,
    walk_out_remaining: u32,
    next_load_home_in: u32,
    net_rate_home: f32,
}

fn world_hunting_at(distance: u32) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    let herd_tile = UVec2::new(CAMP.x + distance, CAMP.y);
    assert!(
        app.world
            .resource::<TileRegistry>()
            .index(herd_tile.x, herd_tile.y)
            .is_some(),
        "fixture: the herd's tile must be on the map ({herd_tile:?})"
    );
    let herd = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let def = fauna
            .species_by_display(BOAR)
            .expect("the fixture names a shipped species");
        Herd::new(
            HERD_ID.to_string(),
            BOAR.to_string(),
            SizeClass::Big,
            vec![herd_tile],
            STANDING_STOCK,
            STANDING_STOCK,
            def.fodder_per_biomass,
            def.regrowth_rate.unwrap_or(0.1),
            def.body_mass,
        )
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.clear();
        registry.herds.push(herd);
    }
    let band = spawn_camp_band(&mut app, hunt_target(), None);
    (app, band)
}

/// **The fixture band, camped on [`CAMP`] with [`CREW`] hands on one row** — `target`, carrying
/// `kit` (`None` = the job's default).
fn spawn_camp_band(app: &mut App, target: LaborTarget, kit: Option<KitChoice>) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(CAMP.x, CAMP.y)
        .expect("the harness map carries the camp tile");
    // **An empty larder, on purpose**: a party is fed by its band's ordinary consumption, so nothing
    // about the posting may depend on what the band has put by.
    let stores = LocalStore::new();
    app.world
        .spawn((
            ResidentBand,
            BandId(BAND),
            BandEquipment::start_stocked_for(
                &EquipmentConfig::for_a_stocked_fixture(),
                CREW as f32,
            ),
            PopulationCohort {
                home: camp,
                current_tile: camp,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(CREW as f32),
                elders: scalar_zero(),
                stores,
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
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
                faction: FACTION,
                knowledge: Vec::new(),
                migration: None,
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    target,
                    workers: CREW,
                    kit,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}

fn hunt_target() -> LaborTarget {
    LaborTarget::Hunt {
        fauna_id: HERD_ID.to_string(),
        floor: FLOOR,
    }
}

/// One labor pass, then the capture — the order the Snapshot stage publishes in.
fn resolve_a_turn(app: &mut App) {
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

fn published_party(app: &App) -> PublishedParty {
    published_party_of(app, "hunt")
}

/// The party the fixture band's row of `kind` publishes, off the ENCODED buffer.
fn published_party_of(app: &App, kind: &str) -> PublishedParty {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == kind)
        .expect("the fixture band's row is on the wire");
    PublishedParty {
        party_workers: row.partyWorkers(),
        hunters_on_the_road: row.huntersOnTheRoad(),
        walk_tiles: row.walkTiles(),
        walk_out_remaining: row.walkOutRemaining(),
        next_load_home_in: row.nextLoadHomeIn(),
        net_rate_home: row.netRateHome(),
    }
}

fn default_hunt_kit() -> String {
    EquipmentConfig::builtin()
        .default_kit(core_sim::KitJob::Hunt)
        .id()
        .to_string()
}

fn ask_the_socket(app: &mut App) -> WorkPartyForecastReply {
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::WorkPartyForecast(WorkPartyForecastQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            source: WorkPartySource::Hunt {
                herd_id: HERD_ID.to_string(),
            },
            kit_id: default_hunt_kit(),
            workers: CREW,
            floor: FLOOR,
        }),
    );
    match reply {
        QueryReply::WorkPartyForecast(answer) => answer,
        other => panic!("the work-party query must answer with a forecast, got {other:?}"),
    }
}

fn larder(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .stores
        .get(FOOD)
        .to_f32()
}

/// ⛔ **RAY'S FORMULA, ON THE WIRE** — a source 8 hexes out walks `8 − 2 = 6` each way with no road.
/// Measured from the band's apron, never from the supply network's `reach_tiles` (which would read
/// 5) and never from the band's own hex (which would read 8).
#[test]
fn a_source_eight_hexes_out_walks_six_each_way() {
    let (mut app, _) = world_hunting_at(8);
    resolve_a_turn(&mut app);
    let party = published_party(&app);
    assert_eq!(
        party.party_workers, CREW,
        "fixture: the far row posted a party"
    );
    assert_eq!(party.walk_tiles, 6);
    assert!(
        party.walk_out_remaining > 0,
        "a party six turns out is still walking out after its first turn"
    );
}

/// ⛔ **FORECAST == ACTUAL, ON THE PUBLISHED ARTIFACT** — the compose-sheet query's rate home is
/// exactly the `netRateHome` the assigned row publishes, for the same band, source, kit, crew and
/// floor. Both are answered by stepping one caravan function from one state; this is what says so.
///
/// Asked after several turns rather than one, so the caravan has walked out and has hunters on the
/// road — the state where a second arithmetic would most easily part company with the first.
#[test]
fn the_query_quotes_exactly_the_rate_the_row_publishes() {
    let (mut app, _) = world_hunting_at(5);
    let mut saw_the_road = false;
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        resolve_a_turn(&mut app);
        if published_party(&app).hunters_on_the_road > 0 {
            saw_the_road = true;
            break;
        }
    }
    assert!(
        saw_the_road,
        "liveness: the caravan must put somebody on the road"
    );
    let published = published_party(&app);
    let answer = ask_the_socket(&mut app);
    assert!(
        answer.posts_a_party,
        "a source past the apron posts a party"
    );
    assert!(
        answer.rate_home > 0.0,
        "liveness: a boar hunt brings food home ({answer:?})"
    );
    assert_eq!(answer.walk_tiles, published.walk_tiles);
    assert_eq!(
        answer.rate_home, published.net_rate_home,
        "the sheet and the row it becomes must quote one number"
    );
    assert!(
        published.next_load_home_in > 0,
        "a hunter carrying a pack publishes when it lands"
    );
}

/// ⛔ **UNASSIGNING A CARAVAN MID-WALK BRINGS EVERY PACK HOME** — the load at the source and every
/// walker's pack, settled into the larder and entered on the food ledger's route arm, so a posting
/// that ends early loses nothing that was on the road.
#[test]
fn unassigning_a_caravan_mid_walk_brings_every_pack_home() {
    let (mut app, band) = world_hunting_at(5);
    let mut on_the_road = None;
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        resolve_a_turn(&mut app);
        let party = app
            .world
            .get::<LaborAllocation>(band)
            .and_then(|allocation| allocation.assignments.first())
            .and_then(|row| row.party.clone())
            .expect("the far row carries a party");
        if party.hunters_on_the_road() > 0 {
            on_the_road = Some(party);
            break;
        }
    }
    let party = on_the_road.expect("liveness: the caravan must put somebody on the road");
    let carried: f32 = party.load_cargo + party.on_the_road.iter().map(|w| w.cargo).sum::<f32>();
    assert!(carried > 0.0, "fixture: the caravan is carrying something");
    let before = larder(&app, band);
    let received_before = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_food_transfers
        .received();
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(hunt_target(), 0, CREW, None);
    resolve_a_turn(&mut app);
    let landed = larder(&app, band) - before;
    let received = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_food_transfers
        .received()
        - received_before;
    assert!(
        (landed - carried).abs() < 1e-2,
        "every pack on the road and the load must land home: {landed} of {carried}"
    );
    assert!(
        (received - carried).abs() < 1e-2,
        "and it lands on the route arm of the food ledger, outside this turn's income: {received}"
    );
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .assignments
            .iter()
            .all(|row| row.party.is_none()),
        "the party is stood down"
    );
}

/// A hex inside the band's own apron (`band_work_range` 2), on the camp's row.
const INSIDE_THE_APRON: u32 = 1;
/// How close two food totals must agree — the settled load is summed in `f32` across walkers.
const SAME_FOOD: f32 = 1e-2;

/// **A CARAVAN WITH A NONZERO LOAD AND SOMEBODY ON THE ROAD** — run a far hunt until both hold, and
/// hand back what the party is carrying in all.
fn a_caravan_carrying_food(app: &mut App, band: Entity) -> f32 {
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        resolve_a_turn(app);
        let party = app
            .world
            .get::<LaborAllocation>(band)
            .and_then(|allocation| allocation.assignments.first())
            .and_then(|row| row.party.clone())
            .expect("the far row carries a party");
        if party.hunters_on_the_road() > 0 && party.load_cargo > 0.0 {
            return party.load_cargo + party.on_the_road.iter().map(|w| w.cargo).sum::<f32>();
        }
    }
    panic!("liveness: the caravan must put somebody on the road with food still in the load");
}

/// What the band's food ledger has taken in on its route arm so far — where a party's homecoming
/// is entered.
fn route_received(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_food_transfers
        .received()
}

/// The food this band's crossings list has booked as its own party coming home
/// (`TransferCause::PartyHome`) — the cause the route arm's homecoming must carry, so a work party's
/// caravan never reads as trade.
fn party_home_booked(app: &App, band: Entity) -> f32 {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_transfer_crossings
        .iter()
        .filter(|row| {
            row.commodity == FOOD
                && row.cause == core_sim::TransferCause::PartyHome
                && row.direction == core_sim::TransferDirection::In
        })
        .map(|row| row.amount)
        .sum()
}

/// ⛔ **A HERD THAT WANDERS BACK INSIDE THE APRON TAKES THE PARTY HOME, ONCE.** The row stops
/// posting — the band's own hands reach the herd again — so its caravan has ended: the load and
/// every walker's pack must reach the larder on that turn, on the route arm, and never again; the
/// row carries no party afterwards and publishes none.
#[test]
fn a_herd_back_inside_the_apron_brings_its_caravan_home_once() {
    let (mut app, band) = world_hunting_at(5);
    let carried = a_caravan_carrying_food(&mut app, band);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == HERD_ID)
            .expect("the fixture herd is still seated");
        let inside = UVec2::new(CAMP.x + INSIDE_THE_APRON, CAMP.y);
        herd.route = vec![inside];
        herd.step_index = 0;
        herd.current_pos = inside;
    }
    let larder_before = larder(&app, band);
    let route_before = route_received(&app, band);
    resolve_a_turn(&mut app);
    let local_take = app
        .world
        .get::<LaborAllocation>(band)
        .and_then(|allocation| allocation.last_yields.first().cloned())
        .expect("the local row publishes its yield")
        .actual;
    let landed = larder(&app, band) - larder_before - local_take;
    let routed = route_received(&app, band) - route_before;
    assert!(
        (landed - carried).abs() < SAME_FOOD,
        "the load and every pack land home beside the local take: {landed} of {carried}"
    );
    assert!(
        (routed - carried).abs() < SAME_FOOD,
        "…on the route arm, outside this turn's income: {routed} of {carried}"
    );
    let row = app
        .world
        .get::<LaborAllocation>(band)
        .and_then(|allocation| allocation.assignments.first().cloned())
        .expect("the row survives as a local hunt");
    assert!(row.party.is_none(), "the row is plainly local again");
    let published = published_party(&app);
    assert_eq!(
        (
            published.party_workers,
            published.hunters_on_the_road,
            published.walk_tiles,
            published.walk_out_remaining,
            published.next_load_home_in,
        ),
        (0, 0, 0, 0, 0),
        "the published row carries no party fields: {published:?}"
    );
    // **Once**: nothing more arrives on the route arm the turn after.
    let route_after = route_received(&app, band);
    resolve_a_turn(&mut app);
    assert_eq!(
        route_received(&app, band),
        route_after,
        "the caravan came home once — a later turn hands nothing more over"
    );
}

/// ⛔ **A HERD GONE FROM THE REGISTRY TAKES ITS PARTY HOME BEFORE THE ROW LAPSES.** The row ends
/// (`status=lapsed reason=herd_gone`), and the load and every walker's pack must reach the larder
/// on that turn, on the route arm, exactly once.
#[test]
fn a_vanished_herd_brings_its_caravan_home_as_the_row_lapses() {
    let (mut app, band) = world_hunting_at(5);
    let carried = a_caravan_carrying_food(&mut app, band);
    app.world.resource_mut::<HerdRegistry>().clear();
    let larder_before = larder(&app, band);
    let route_before = route_received(&app, band);
    let party_home_before = party_home_booked(&app, band);
    resolve_a_turn(&mut app);
    let landed = larder(&app, band) - larder_before;
    let routed = route_received(&app, band) - route_before;
    let booked = party_home_booked(&app, band) - party_home_before;
    assert!(
        (booked - carried).abs() < SAME_FOOD,
        "the homecoming is booked as the band's own party coming home, not as trade: {booked} of \
         {carried}"
    );
    assert!(
        (landed - carried).abs() < SAME_FOOD,
        "the load and every pack land home as the row lapses: {landed} of {carried}"
    );
    assert!(
        (routed - carried).abs() < SAME_FOOD,
        "…on the route arm, outside this turn's income: {routed} of {carried}"
    );
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .assignments
            .is_empty(),
        "the row whose herd is gone lapses"
    );
    let larder_after = larder(&app, band);
    resolve_a_turn(&mut app);
    assert_eq!(
        larder(&app, band),
        larder_after,
        "the caravan came home once — a later turn hands nothing more over"
    );
}

// ---------------------------------------------------------------------------------------------
// THE DEPOSIT WEB — a far working posts a party exactly as a far patch does
// ---------------------------------------------------------------------------------------------
//
// ⛔ **There is nothing special about wood or stone.** A far working posts a party by the same
// geometry, walks the same walk, and its porters carry the hunt's own haul carry divided by the
// material's `weight` — so every test below would read the same on stone, and the last one proves it
// by changing the weight rather than the material.

/// The deposit every fixture below works — wood, which the ground below is re-terrained to hold.
const WOOD: &str = "wood";
/// Ground that holds a wood deposit in the shipped `extraction.json` (600 at a positive rate).
const WOODED: sim_schema::TerrainType = sim_schema::TerrainType::MixedWoodland;
/// The wire's `kind` for an extract row (`LaborTarget::kind`).
const EXTRACT_KIND: &str = "extract";
/// The shipped forestry kit — `sled` + `axe`. On the deadfall floor the row claims its sleds; on
/// felling it claims its axes (`LaborAssignment::take_kit`).
const WOODCUTTING_KIT: &str = "woodcutting";

fn extract_target(distance: u32) -> LaborTarget {
    LaborTarget::Extract {
        tile: UVec2::new(CAMP.x + distance, CAMP.y),
        material: WOOD.to_string(),
        floor: FLOOR,
    }
}

/// The kit `id`, resolved for the extract job the way `assign_labor` resolves one.
fn extract_kit(id: &str) -> KitChoice {
    EquipmentConfig::builtin()
        .resolve_kit_for_job(Some(id), core_sim::KitJob::Extraction)
        .expect("the roster sends this kit on a deposit")
}

/// **A band on [`CAMP`] working a wood `distance` hexes along its row**, carrying `kit`. The deposit's
/// tile is re-terrained to [`WOODED`] so the harness map's own terrain there cannot decide whether
/// it holds wood.
fn world_extracting_at(distance: u32, kit: Option<&str>) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    let tile = UVec2::new(CAMP.x + distance, CAMP.y);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .unwrap_or_else(|| panic!("fixture: the deposit's tile must be on the map ({tile:?})"));
    app.world
        .get_mut::<core_sim::Tile>(entity)
        .expect("a map tile carries terrain")
        .terrain = WOODED;
    let band = spawn_camp_band(&mut app, extract_target(distance), kit.map(extract_kit));
    (app, band)
}

/// The fixture band's extract row's party, in-process — for the load and the packs, which the wire
/// does not carry.
fn extract_party(app: &App, band: Entity) -> Option<core_sim::WorkParty> {
    app.world
        .get::<LaborAllocation>(band)
        .and_then(|allocation| allocation.assignments.first())
        .and_then(|row| row.party.clone())
}

/// Run turns until a porter is on the road with a pack, and hand back that first pack's cargo.
fn first_pack_on_the_road(app: &mut App, band: Entity) -> f32 {
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        resolve_a_turn(app);
        if let Some(walker) =
            extract_party(app, band).and_then(|party| party.on_the_road.first().cloned())
        {
            return walker.cargo;
        }
    }
    panic!("liveness: the caravan must put a porter on the road");
}

/// How much wood the band holds.
fn wood_held(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .stores
        .material_total(WOOD)
        .to_f32()
}

/// The material the extract row published as landing this turn, off the ENCODED buffer.
fn published_material_yield(app: &App) -> f32 {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == EXTRACT_KIND)
        .expect("the fixture band's extract row is on the wire");
    row.materialYield()
        .into_iter()
        .flatten()
        .filter(|payoff| payoff.materialId().unwrap_or_default() == WOOD)
        .map(|payoff| payoff.amount())
        .sum()
}

/// The pack a party of this row carries, struck through the caravan's own pricing — the haul carry
/// over the material's weight.
fn priced_pack(app: &App, band: Entity, kit: &KitChoice) -> f32 {
    let equipment = EquipmentConfig::builtin();
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let wear = app
        .world
        .get::<BandEquipment>(band)
        .cloned()
        .unwrap_or_default();
    let pricing =
        core_sim::work_party::CaravanPricing::resolve(&equipment, kit, CREW, &wear, &[], &labor);
    let weight = app
        .world
        .resource::<core_sim::MaterialsConfigHandle>()
        .get()
        .material(WOOD)
        .expect("wood is a material")
        .weight;
    core_sim::work_party::material_pack(pricing.haul_carry, weight)
}

/// ⛔ **A DEPOSIT EIGHT HEXES OUT WALKS SIX EACH WAY, ON THE WIRE** — the Forage arm's posting, with
/// no range lapse: the row survives and publishes its party.
#[test]
fn a_deposit_eight_hexes_out_posts_a_party_that_walks_six_each_way() {
    let (mut app, band) = world_extracting_at(8, None);
    resolve_a_turn(&mut app);
    let party = published_party_of(&app, EXTRACT_KIND);
    assert_eq!(
        party.party_workers, CREW,
        "a far working posts a party rather than lapsing"
    );
    assert_eq!(party.walk_tiles, 6, "Ray's formula: 8 − 2");
    assert!(
        party.walk_out_remaining > 0,
        "a party six turns out is still walking out after its first turn"
    );
    assert_eq!(
        wood_held(&app, band),
        0.0,
        "nothing lands while the party is still walking out"
    );
}

/// ⛔ **THE LOCAL IDENTITY, ON THE DEPOSIT WEB** — a working inside the apron posts no party, and
/// every number is the take seam's own: the row publishes and the store receives exactly
/// `take_from_deposit` at the whole crew, uncapped by any carry.
#[test]
fn a_local_working_takes_no_party_and_its_numbers_are_unchanged() {
    let (mut app, band) = world_extracting_at(INSIDE_THE_APRON, None);
    let expected = {
        let tile = UVec2::new(CAMP.x + INSIDE_THE_APRON, CAMP.y);
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(tile.x, tile.y)
            .expect("the deposit's tile is on the map");
        let ground = app
            .world
            .get::<core_sim::Tile>(entity)
            .expect("a map tile carries terrain")
            .clone();
        let extraction = app
            .world
            .resource::<core_sim::ExtractionConfigHandle>()
            .get();
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        let mut working = core_sim::extraction::projected_working(
            app.world.resource::<core_sim::DepositRegistry>(),
            tile,
            WOOD,
            &ground,
            &extraction,
        )
        .expect("fixture: the ground holds wood");
        core_sim::extraction::take_from_deposit(
            &mut working,
            CREW,
            core_sim::extraction::NO_DEPOSIT_GEAR,
            FLOOR,
            &ground,
            &extraction,
            &ladder,
        )
        .taken
    };
    resolve_a_turn(&mut app);
    assert!(expected > 0.0, "liveness: a crew on a full wood takes some");
    assert!(
        extract_party(&app, band).is_none(),
        "a working the band's own hands reach takes no party"
    );
    assert_eq!(
        published_party_of(&app, EXTRACT_KIND).party_workers,
        0,
        "and publishes none"
    );
    assert_eq!(
        published_material_yield(&app),
        expected,
        "the row publishes the take seam's own figure, uncapped by any carry"
    );
    assert!(
        (wood_held(&app, band) - expected).abs() < 1e-3,
        "the whole take lands this turn: {} of {expected}",
        wood_held(&app, band)
    );
}

/// ⛔ **FORECAST == ACTUAL, ON THE DEPOSIT WEB** — the query's rate home for a deposit is exactly the
/// `netRateHome` the extract row publishes, in wood per turn, off the encoded snapshot.
#[test]
fn the_query_quotes_exactly_the_rate_an_extract_row_publishes() {
    let (mut app, band) = world_extracting_at(5, None);
    first_pack_on_the_road(&mut app, band);
    let published = published_party_of(&app, EXTRACT_KIND);
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::WorkPartyForecast(WorkPartyForecastQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            source: WorkPartySource::Extract {
                x: CAMP.x + 5,
                y: CAMP.y,
                material: WOOD.to_string(),
            },
            kit_id: EquipmentConfig::builtin()
                .default_kit(core_sim::KitJob::Extraction)
                .id()
                .to_string(),
            workers: CREW,
            floor: FLOOR,
        }),
    );
    let answer = match reply {
        QueryReply::WorkPartyForecast(answer) => answer,
        other => panic!("the work-party query must answer with a forecast, got {other:?}"),
    };
    assert!(
        answer.posts_a_party,
        "a working past the apron posts a party"
    );
    assert!(
        answer.rate_home > 0.0,
        "liveness: a wood brings timber home ({answer:?})"
    );
    assert_eq!(answer.walk_tiles, published.walk_tiles);
    assert_eq!(
        answer.rate_home, published.net_rate_home,
        "the sheet and the row it becomes must quote one number"
    );
}

/// **Ground holding none of the asked material is refused by name**, never answered with a zero.
#[test]
fn the_query_refuses_ground_that_holds_no_such_deposit() {
    let (mut app, _) = world_extracting_at(5, None);
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::WorkPartyForecast(WorkPartyForecastQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            source: WorkPartySource::Extract {
                x: CAMP.x + 5,
                y: CAMP.y,
                material: "hide".to_string(),
            },
            kit_id: EquipmentConfig::builtin()
                .default_kit(core_sim::KitJob::Extraction)
                .id()
                .to_string(),
            workers: CREW,
            floor: FLOOR,
        }),
    );
    assert_eq!(
        reply,
        QueryReply::Error(sim_runtime::commands::query_error::UNKNOWN_DEPOSIT.to_string())
    );
}

/// ⛔ **UNASSIGNING A DEPOSIT CARAVAN MID-WALK BRINGS EVERY PACK HOME AS MATERIAL** — into the store,
/// booked as the band's own party coming home, and **nothing** onto the larder or the food ledger's
/// route arm: a working's timber is not food.
#[test]
fn unassigning_a_deposit_caravan_mid_walk_brings_every_pack_home_as_material() {
    let (mut app, band) = world_extracting_at(5, None);
    first_pack_on_the_road(&mut app, band);
    let party = extract_party(&app, band).expect("the far row carries a party");
    let carried: f32 = party.load_cargo + party.on_the_road.iter().map(|w| w.cargo).sum::<f32>();
    assert!(carried > 0.0, "fixture: the caravan is carrying something");
    let wood_before = wood_held(&app, band);
    let larder_before = larder(&app, band);
    let food_routed_before = route_received(&app, band);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(extract_target(5), 0, CREW, None);
    resolve_a_turn(&mut app);
    let landed = wood_held(&app, band) - wood_before;
    let booked: f32 = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .last_transfer_crossings
        .iter()
        .filter(|row| {
            row.commodity == WOOD
                && row.cause == core_sim::TransferCause::PartyHome
                && row.direction == core_sim::TransferDirection::In
        })
        .map(|row| row.amount)
        .sum();
    assert!(
        (landed - carried).abs() < SAME_FOOD,
        "every pack on the road and the load land in the store as wood: {landed} of {carried}"
    );
    assert!(
        (booked - carried).abs() < SAME_FOOD,
        "…booked as the band's own party coming home: {booked} of {carried}"
    );
    assert_eq!(
        larder(&app, band),
        larder_before,
        "not one unit of it reaches the larder"
    );
    assert_eq!(
        route_received(&app, band),
        food_routed_before,
        "…nor the food ledger's route arm"
    );
    assert!(
        extract_party(&app, band).is_none(),
        "the party is stood down"
    );
}

/// **The row's kit narrowed to the tool it claims at `rung`** — what the turn prices a far party's
/// haul carry over (`LaborAssignment::take_kit` → `EquipmentConfig::deposit_rung_kit`).
fn claimed_at(kit_id: &str, rung: core_sim::RungKey) -> KitChoice {
    EquipmentConfig::builtin().deposit_rung_kit(
        &extract_kit(kit_id),
        rung.branch(),
        &rung.wire_key(),
    )
}

/// The bare-handed pack of wood: `labor_config`'s sledless haul over wood's weight.
fn bare_wood_pack(app: &App) -> f32 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let weight = app
        .world
        .resource::<core_sim::MaterialsConfigHandle>()
        .get()
        .material(WOOD)
        .expect("wood is a material")
        .weight;
    core_sim::work_party::material_pack(labor.hunt.per_worker_biomass_capacity, weight)
}

/// ⛔ **A WOODCUTTING CREW ON THE DEADFALL FLOOR HAULS ON ITS SLEDS** — the floor rung is where the
/// row claims the sled (`take_kit`), so its porters carry the sled's `hunt_carry` over the material's
/// weight, against the bare-handed carry over the same weight. Read off the first porter the turn
/// actually sends, and pinned to the caravan pricing's own figure over the CLAIMED kit.
#[test]
fn a_woodcutting_crew_on_the_deadfall_floor_carries_a_larger_pack_than_bare_hands() {
    let (mut bare_app, bare_band) = world_extracting_at(5, None);
    let bare = first_pack_on_the_road(&mut bare_app, bare_band);
    let (mut sled_app, sled_band) = world_extracting_at(5, Some(WOODCUTTING_KIT));
    let sled = first_pack_on_the_road(&mut sled_app, sled_band);
    let bare_kit = EquipmentConfig::builtin().default_kit(core_sim::KitJob::Extraction);
    assert!(
        (bare - priced_pack(&bare_app, bare_band, &bare_kit)).abs() < 1e-3,
        "a bare-handed porter carries the bare haul over the weight: {bare}"
    );
    let claimed = claimed_at(WOODCUTTING_KIT, core_sim::RungKey::ForestryDeadfall);
    assert_eq!(
        claimed.uses().collect::<Vec<_>>(),
        vec!["sled"],
        "fixture: on deadfall the woodcutting row claims its sleds"
    );
    assert!(
        (sled - priced_pack(&sled_app, sled_band, &claimed)).abs() < 1e-3,
        "a sledded porter carries the sled's haul over the weight: {sled}"
    );
    assert!(
        sled > bare,
        "the sled must carry more timber home per trip: {sled} vs {bare}"
    );
}

/// ⛔ **A WOODCUTTING CREW ON FELLING HAULS BARE-HANDED** — on the felling rung the row claims its
/// AXES, not its sleds (`take_kit`, main's held-rung rule), so the party's porters carry the bare
/// haul over the weight however many sleds the band owns. Paired against the same kit on the
/// deadfall floor, which hauls on the sled — so "every pack is bare" cannot pass it.
#[test]
fn a_woodcutting_crew_on_felling_hauls_at_bare_carry() {
    let (mut app, band) = world_extracting_at(5, Some(WOODCUTTING_KIT));
    seat_the_wood_at_felling(&mut app, 5);
    let felling = first_pack_on_the_road(&mut app, band);
    let claimed = claimed_at(WOODCUTTING_KIT, core_sim::RungKey::ForestryFelling);
    assert!(
        !claimed.uses().any(|item| item == "sled"),
        "fixture: on felling the woodcutting row claims no sled"
    );
    assert!(
        app.world
            .get::<BandEquipment>(band)
            .expect("the fixture band carries gear")
            .count_of("sled")
            > 0,
        "fixture: the band does own sleds, so the bare pack is about the claim and not the stock"
    );
    assert!(
        (felling - bare_wood_pack(&app)).abs() < 1e-3,
        "a felling party's porter carries the bare haul over the weight: {felling}"
    );
    assert!(
        (felling - priced_pack(&app, band, &claimed)).abs() < 1e-3,
        "…which is the caravan pricing over the claimed kit"
    );
    let (mut floor_app, floor_band) = world_extracting_at(5, Some(WOODCUTTING_KIT));
    let deadfall = first_pack_on_the_road(&mut floor_app, floor_band);
    assert!(
        deadfall > felling,
        "the same kit on the deadfall floor hauls on its sleds: {deadfall} vs {felling}"
    );
}

/// **SEAT THE FIXTURE'S WOOD AT THE TOP OF `forestry:felling`**, written straight into the registry
/// so a test about the claim does not spend forty turns of builders raising it first.
fn seat_the_wood_at_felling(app: &mut App, distance: u32) {
    let tile = UVec2::new(CAMP.x + distance, CAMP.y);
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let extraction = app
        .world
        .resource::<core_sim::ExtractionConfigHandle>()
        .get();
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("the deposit's tile is on the map");
    let ground = app
        .world
        .get::<core_sim::Tile>(entity)
        .expect("a map tile carries terrain")
        .clone();
    let felling = core_sim::RungKey::ForestryFelling;
    let mut working = core_sim::extraction::projected_working(
        app.world.resource::<core_sim::DepositRegistry>(),
        tile,
        WOOD,
        &ground,
        &extraction,
    )
    .expect("fixture: the ground holds wood");
    let (base, width) = core_sim::extraction::deposit_rung_span(felling, &ladder);
    working.set_ladder_position(base + width, &ladder, felling.branch());
    assert_eq!(working.rung(), felling, "fixture: seated on felling");
    app.world
        .resource_mut::<core_sim::DepositRegistry>()
        .insert(working);
}

/// ⛔ **ONE PACK IS THE HAUL CARRY OVER THE MATERIAL'S WEIGHT, AND NOTHING ELSE** — the same wood,
/// the same crew, the same carry, with only `weight` changed in the materials table: a material
/// twice as heavy sends porters home with exactly half the units. No code path knows which material
/// it is; the one number on the material is the whole difference.
#[test]
fn a_pack_is_the_haul_carry_over_the_materials_weight() {
    const HEAVIER: f32 = 2.0;
    let (mut shipped_app, shipped_band) = world_extracting_at(5, None);
    let shipped = first_pack_on_the_road(&mut shipped_app, shipped_band);

    let (mut heavy_app, heavy_band) = world_extracting_at(5, None);
    let heavier = {
        let mut json: serde_json::Value =
            serde_json::from_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("builtin parses");
        let weight = json["materials"][WOOD]["weight"]
            .as_f64()
            .expect("wood declares a weight");
        json["materials"][WOOD]["weight"] = serde_json::json!(weight * f64::from(HEAVIER));
        core_sim::MaterialsConfig::from_json_str(&json.to_string())
            .expect("the heavier table validates")
    };
    heavy_app
        .world
        .resource_mut::<core_sim::MaterialsConfigHandle>()
        .replace(std::sync::Arc::new(heavier));
    let heavy = first_pack_on_the_road(&mut heavy_app, heavy_band);
    assert!(shipped > 0.0, "liveness: a porter left with a pack");
    assert!(
        (shipped / heavy - HEAVIER).abs() < 1e-3,
        "twice the weight, half the units per pack: {shipped} vs {heavy}"
    );
}

/// How close the crew curve's quote and the turn's cut must agree — both run `deposit_take` off one
/// renewed stock, so this only absorbs float order.
const SAME_CUT: f32 = 1e-4;

/// ⛔ **A FAR WORKING'S CREW CURVE QUOTES THE CUT AT THE SOURCE, NOT ZERO** — past the apron the row
/// posts a party and its crew cuts the working exactly as a near crew does, so the compose sheet's
/// row for the crew PRESENT must equal what the turn then cuts. Staged mid-posting (the walk out done,
/// porters on the road) so the present crew is genuinely short of the staffed one, and read against
/// the working's own `last_take` after a whole turn in stage order (Logistics' renewal, then the
/// take) — the "as the next turn will find it" state the curve quotes. `in_range` stays a plain
/// fact: `false` out here, and it zeroes nothing.
#[test]
fn a_far_workings_crew_curve_quotes_the_cut_the_turn_makes_at_the_source() {
    let (mut app, band) = world_extracting_at(5, None);
    // Past the walk out, so there are hands at the source to quote.
    first_pack_on_the_road(&mut app, band);
    let present = {
        let mut party = extract_party(&app, band).expect("the far row carries a party");
        party.open_turn().present
    };
    assert!(
        present > 0 && present < CREW,
        "fixture: some hands at the source next turn and some on the road ({present} of {CREW})"
    );
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::DepositCrewTake(sim_runtime::commands::DepositCrewTakeQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            x: CAMP.x + 5,
            y: CAMP.y,
            material: WOOD.to_string(),
            kit_id: EquipmentConfig::builtin()
                .default_kit(core_sim::KitJob::Extraction)
                .id()
                .to_string(),
            floor: FLOOR,
            max_workers: CREW,
        }),
    );
    let curve = match reply {
        QueryReply::DepositCrewTake(curve) => curve,
        other => panic!("the deposit crew query must answer with a curve, got {other:?}"),
    };
    assert!(!curve.in_range, "past the apron, in_range says so");
    let quoted = curve.per_crew[present as usize - 1].take;
    assert!(
        quoted > 0.0,
        "a far working's curve quotes a real cut, never the retired zero"
    );
    app.world.run_system_once(core_sim::advance_deposits);
    resolve_a_turn(&mut app);
    let cut = app
        .world
        .resource::<core_sim::DepositRegistry>()
        .source(UVec2::new(CAMP.x + 5, CAMP.y), WOOD)
        .expect("the working stands")
        .last_take;
    assert!(
        (quoted - cut).abs() < SAME_CUT,
        "the curve's row for the {present} hands present is what the turn cut: {quoted} vs {cut}"
    );
}
