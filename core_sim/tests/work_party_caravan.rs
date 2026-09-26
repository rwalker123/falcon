//! **THE WORK PARTY AS A CARAVAN, DRIVEN THROUGH REAL TURNS AND READ OFF THE ENCODED WIRE**
//! (`docs/plan_civilization_steps.md` §One work party, `.claude/rules/core_sim/work-party.md`).
//!
//! A band camped on the harness map's `(1, 1)` hunts a herd seated a stated number of hexes along
//! the same row, so the hex distance is exactly that number. Every claim here is asserted on what a
//! client parses — the encoded snapshot, or the query's answer — never on an in-process value the
//! capture might not carry.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, BandEquipment, BandId, EquipmentConfig, FactionId, FaunaConfigHandle,
    GenerationId, Herd, HerdRegistry, LaborAllocation, LaborAssignment, LaborTarget, LocalStore,
    MoraleCause, PopulationCohort, ResidentBand, SizeClass, SnapshotHistory, SourcePriority,
    TileRegistry, FOOD,
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
/// **More food than any fixture party's upkeep can want**, so the posting is supplied for a reason
/// the fixture states rather than by accident.
const A_DEEP_LARDER: f32 = 1_000.0;
/// A bound on how long a caravan may take to put somebody on the road. A guard against hanging,
/// not a prediction.
const TURNS_TO_SEE_A_PORTER: usize = 60;
/// ⛔ **THE FIXTURE'S HUNTERS EAT A TENTH OF WHAT THE SHIPPED ROSTER EATS**, and it has to be stated.
/// The rule under test is *"the party eats first and only the surplus walks home"*, and at the
/// shipped draw a small party on this quarry eats its whole take — which is the rule working (a
/// posting on thin game walks little home) and leaves nothing on the road to measure. Lowering the
/// draw is what puts a surplus into the load; it changes nothing the assertions compare, because
/// the turn and the query read the same configured draw.
const A_LIGHT_EATER: f32 = 0.1;
/// **The shipped roster's own draw** — what the thin-take case below needs, because the deficit it
/// pins is the one Ray met in play: a small party on boar eating its whole take and still short.
const THE_SHIPPED_DRAW: f32 = 1.0;
/// Turns to step a far posting past its walk out and onto its steady footing. A bound, not a
/// prediction.
const TURNS_TO_SETTLE: usize = 12;
/// ⛔ **How close the sheet's deficit must sit to the row's.** Not bit-equal, and for a stated
/// reason: the row's `partyDeficit` is *this turn's* shortfall against the turn's whole-animal take,
/// while the reply's is the **mean** over the forecast horizon, stepped through the smooth
/// (unquantised) projection every forecast in the sim uses. At a steady footing the two agree to the
/// float noise between a quantised take and its smooth expectation — measured at `3e-7` food on
/// this fixture — so the bound is four orders tighter than any deficit it has to tell apart.
const DEFICIT_TOLERANCE: f32 = 1e-4;

/// The whole of what one row publishes about its party, read back out of the ENCODED buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PublishedParty {
    party_workers: u32,
    hunters_on_the_road: u32,
    walk_tiles: u32,
    walk_out_remaining: u32,
    next_load_home_in: u32,
    net_rate_home: f32,
    party_deficit: f32,
}

fn world_hunting_at(distance: u32) -> (App, Entity) {
    world_hunting_with(distance, A_LIGHT_EATER)
}

/// [`world_hunting_at`] with the hunters' food draw stated — `A_LIGHT_EATER` puts a surplus on the
/// road, `THE_SHIPPED_DRAW` leaves a thin take short of its upkeep.
fn world_hunting_with(distance: u32, draw: f32) -> (App, Entity) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    {
        let mut demographics = (*app
            .world
            .resource::<core_sim::DemographicsConfigHandle>()
            .get())
        .clone();
        demographics.consumption.per_capita_draw *= draw;
        app.world
            .resource_mut::<core_sim::DemographicsConfigHandle>()
            .replace(std::sync::Arc::new(demographics));
    }
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
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(CAMP.x, CAMP.y)
        .expect("the harness map carries the camp tile");
    let mut stores = LocalStore::new();
    stores.add(FOOD, scalar_from_f32(A_DEEP_LARDER));
    let band = app
        .world
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
                    target: hunt_target(),
                    workers: CREW,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
                ..Default::default()
            },
        ))
        .id();
    (app, band)
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
        .find(|assignment| assignment.kind().unwrap_or_default() == "hunt")
        .expect("the fixture band's hunt row is on the wire");
    PublishedParty {
        party_workers: row.partyWorkers(),
        hunters_on_the_road: row.huntersOnTheRoad(),
        walk_tiles: row.walkTiles(),
        walk_out_remaining: row.walkOutRemaining(),
        next_load_home_in: row.nextLoadHomeIn(),
        net_rate_home: row.netRateHome(),
        party_deficit: row.partyDeficit(),
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
    assert!(
        (answer.deficit - published.party_deficit).abs() < DEFICIT_TOLERANCE,
        "the sheet's deficit is the one the row publishes: {} against {}",
        answer.deficit,
        published.party_deficit
    );
}

/// ⛔ **A THIN TAKE AGAINST A REAL UPKEEP: THE SHEET SEES THE DEFICIT THE ROW WILL PRINT.**
///
/// The case Ray met in play — a small party on boar at the shipped draw eats its whole take, no pack
/// ever fills, and the committed row reads *"needs food from home"*. The compose sheet can warn
/// about that only if the query says so, so this pins the reply's `deficit` against the row's
/// published `partyDeficit` off the **encoded** snapshot, at a steady footing past the walk out.
///
/// **The liveness half is the point**: a reply that always answered `0.0` would pass the equality in
/// the surplus case above, so here the deficit has to be genuinely positive on both sides, and
/// nothing may be on the road.
#[test]
fn a_thin_take_quotes_the_deficit_the_row_will_publish() {
    let (mut app, _) = world_hunting_with(5, THE_SHIPPED_DRAW);
    for _ in 0..TURNS_TO_SETTLE {
        resolve_a_turn(&mut app);
    }
    let published = published_party(&app);
    assert_eq!(
        published.walk_out_remaining, 0,
        "fixture: the party has finished walking out"
    );
    assert_eq!(
        published.hunters_on_the_road, 0,
        "a party that eats its whole take never fills a pack"
    );
    assert!(
        published.party_deficit > 0.0,
        "liveness: the row publishes a real deficit ({published:?})"
    );
    let answer = ask_the_socket(&mut app);
    assert!(
        answer.deficit > 0.0,
        "liveness: the sheet warns about it ({answer:?})"
    );
    assert!(
        (answer.deficit - published.party_deficit).abs() < DEFICIT_TOLERANCE,
        "the sheet's deficit is the one the row publishes: {} against {}",
        answer.deficit,
        published.party_deficit
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
    let carried: f32 = party.load_food + party.on_the_road.iter().map(|w| w.food).sum::<f32>();
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
