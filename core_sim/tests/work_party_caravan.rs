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
    spoiled_rate_home: f32,
    transit_keeps_turns: f32,
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
    spawn_band_camped_at(app, CAMP, target, kit)
}

/// [`spawn_camp_band`], camped on `at` rather than [`CAMP`].
fn spawn_band_camped_at(
    app: &mut App,
    at: UVec2,
    target: LaborTarget,
    kit: Option<KitChoice>,
) -> Entity {
    let camp = app
        .world
        .resource::<TileRegistry>()
        .index(at.x, at.y)
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
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                last_breeding: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FACTION,
                knowledge: Vec::new(),
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
                last_belief_relay_hops: 0,
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    party: None,
                    target,
                    workers: CREW,
                    kit,
                    priority: SourcePriority::default(),
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
        spoiled_rate_home: row.spoiledRateHome(),
        transit_keeps_turns: row.transitKeepsTurns(),
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

/// **STATE THE TAMED HERD'S LIVE KEEPING BILL** — every quote prices the live bill (one regrowth
/// on), never a stamp, so a fixture states a bill by scaling the rung the herd holds.
fn set_live_herd_bill(app: &mut App, bill: f32) {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let fauna = app.world.resource::<core_sim::FaunaConfigHandle>().get();
    let (held, live) = {
        let herd = &app.world.resource::<HerdRegistry>().herds[0];
        (
            herd.standing().held,
            core_sim::herd_upkeep_demand(
                &core_sim::next_turns_quarry(herd, &fauna),
                &fauna,
                &ladder,
            ),
        )
    };
    let scaled = ladder.with_upkeep_scaled(held, bill / live);
    app.world
        .resource_mut::<core_sim::LadderConfigHandle>()
        .replace(std::sync::Arc::new(scaled));
}

/// ⛔ **A KEPT SITE'S LOCAL RATE IS WHAT ITS CREW COLLECTS AFTER KEEPING** (`docs/plan_site_crews.md`
/// §2.1, §2.2) — the compose sheet nets the keeping exactly as the deposit curve and the turn do.
/// On a tamed herd inside the apron, a crew of [`CREW`] whose bill costs [`KEEPING_HANDS`] whole
/// hands is quoted the rate a crew [`KEEPING_HANDS`] smaller would take from the same herd kept for
/// nothing.
#[test]
fn a_kept_herd_inside_the_apron_is_quoted_what_its_crew_takes_after_keeping() {
    /// Whole hands the stated bill costs.
    const KEEPING_HANDS: u32 = 2;
    /// A probe bill, to read the rate one keeping hand works at off the seam itself.
    const PROBE_BILL: f32 = 1.0;
    /// A bill of nothing — the same tamed herd, unkept.
    const NO_BILL: f32 = 0.0;
    let ask = |bill: f32, workers: u32| {
        let (mut app, _) = world_hunting_at(INSIDE_THE_APRON);
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = &mut registry.herds[0];
            herd.tame_outright(FACTION, &ladder);
        }
        set_live_herd_bill(&mut app, bill);
        let reply = core_sim::forecast_query::answer_forecast_query(
            &mut app.world,
            &QueryPayload::WorkPartyForecast(WorkPartyForecastQuery {
                faction_id: FACTION.0,
                band_id: BAND,
                source: WorkPartySource::Hunt {
                    herd_id: HERD_ID.to_string(),
                },
                kit_id: default_hunt_kit(),
                workers,
                floor: FLOOR,
            }),
        );
        match reply {
            QueryReply::WorkPartyForecast(answer) => answer,
            other => panic!("the work-party query must answer with a forecast, got {other:?}"),
        }
    };
    let rate = {
        let (app, _) = world_hunting_at(INSIDE_THE_APRON);
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        let mut herd = app.world.resource::<HerdRegistry>().herds[0].clone();
        herd.tame_outright(FACTION, &ladder);
        let wear = BandEquipment::start_stocked_for(
            &EquipmentConfig::for_a_stocked_fixture(),
            CREW as f32,
        );
        let keeping = core_sim::CrewKeeping {
            rung: herd.standing().held,
            demand: PROBE_BILL,
        };
        PROBE_BILL
            / core_sim::crew_keep_hands(
                Some(keeping),
                &EquipmentConfig::for_a_stocked_fixture(),
                &wear,
                CREW,
            )
    };
    let kept = ask(KEEPING_HANDS as f32 * rate, CREW);
    let smaller_unkept = ask(NO_BILL, CREW - KEEPING_HANDS);
    let whole_unkept = ask(NO_BILL, CREW);
    assert!(
        !kept.posts_a_party,
        "fixture: a herd inside the apron posts no party"
    );
    assert!(
        whole_unkept.rate_home > smaller_unkept.rate_home,
        "liveness: more hands take more from this herd, or the comparison says nothing"
    );
    assert!(
        (kept.rate_home - smaller_unkept.rate_home).abs() < SAME_FOOD,
        "the kept crew is quoted what its take hands collect: {} against {}",
        kept.rate_home,
        smaller_unkept.rate_home
    );
}

/// ⛔ **ONLY THE HANDS AT THE SOURCE KEEP IT** (`docs/plan_site_crews.md` §2.2). A far kept herd's
/// party walks out first, and while it walks nobody stands at the herd: the keeping it was planned
/// is not paid, and the herd publishes no keeping hands. Found on bench seed 22, where a walking
/// party paid a tended patch's whole bill with nobody there.
#[test]
fn a_party_walking_out_keeps_nothing_at_the_source() {
    /// Far enough to post a party that is still walking out on its first turn.
    const FAR: u32 = 5;
    let (mut app, _) = world_hunting_at(FAR);
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.herds[0].tame_outright(FACTION, &ladder);
    }
    let bill = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        core_sim::herd_keeping_basis(
            &app.world.resource::<HerdRegistry>().herds[0],
            &fauna,
            &ladder,
        )
    };
    assert!(bill > 0.0, "fixture: the tamed herd owes a keeping bill");
    resolve_a_turn(&mut app);
    assert!(
        published_party(&app).walk_out_remaining > 0,
        "fixture: the party is still walking out, so nobody stands at the herd"
    );
    let herd = &app.world.resource::<HerdRegistry>().herds[0];
    assert_eq!(
        (herd.upkeep_hands, herd.upkeep_supplied),
        (0.0, 0.0),
        "a party on the road keeps nothing at the source"
    );
}

/// ⛔ **A FAR KEPT HERD'S CARAVAN FORECAST IS THE TAKE ITS PARTY MAKES AFTER KEEPING**
/// (`docs/plan_site_crews.md` §2.1). Every turn the hands present keep the herd first and hunt with
/// the rest — the turn's `SiteKeeping::at_the_source` — and the caravan forecast steps the same
/// split (`work_party::take_hands_present`), so:
///
/// - the query and the row's published `netRateHome` quote one number;
/// - what then lands home over the forecast's horizon is that rate, to within one landing (the
///   food walks home a whole pack at a time);
/// - and the same party priced as if the herd cost nothing to keep quotes more than lands — the
///   forecast that ignored the keeping, which this test exists to keep dead.
///
/// The bill is pinned at [`KEEPING_HANDS`] hands' worth each turn, so the keeping is a known,
/// partial share of a crew that is often short-handed at the source.
#[test]
fn a_far_kept_herds_caravan_forecast_is_what_its_party_lands_after_keeping() {
    /// Whole hands the pinned bill costs — fewer than the crew, so the party both keeps and hunts.
    const KEEPING_HANDS: f32 = 2.0;
    /// A probe bill, to read the rate one keeping hand works at off the seam itself.
    const PROBE_BILL: f32 = 1.0;
    /// A bill of nothing — the same tamed herd, priced unkept.
    const NO_BILL: f32 = 0.0;
    /// Far enough to post a party.
    const FAR: u32 = 5;
    let (mut app, band) = world_hunting_at(FAR);
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    app.world.resource_mut::<HerdRegistry>().herds[0].tame_outright(FACTION, &ladder);
    let bill = {
        let herd = &app.world.resource::<HerdRegistry>().herds[0];
        let stocked = EquipmentConfig::for_a_stocked_fixture();
        let wear = BandEquipment::start_stocked_for(&stocked, CREW as f32);
        let keeping = core_sim::CrewKeeping {
            rung: herd.standing().held,
            demand: PROBE_BILL,
        };
        KEEPING_HANDS * PROBE_BILL / core_sim::crew_keep_hands(Some(keeping), &stocked, &wear, CREW)
    };
    // The keeping scratch the Logistics pass would clear — this fixture
    // drives the labor pass alone.
    // **The bill is stated through the ladder, once** — the turn and the forecast both read it live.
    set_live_herd_bill(&mut app, bill);
    // **And the herd regrows between turns, as Logistics would** — the forecast is struck one
    // regrowth on (`fauna::herd_crew_keeping_next_turn`), so a fixture that skipped it would drift
    // from its own forecast by the regrowth's keeping.
    let between_turns = |app: &mut App| {
        let fauna = app.world.resource::<core_sim::FaunaConfigHandle>().get();
        let herd = &mut app.world.resource_mut::<HerdRegistry>().herds[0];
        *herd = core_sim::next_turns_quarry(herd, &fauna);
        herd.upkeep_supplied = core_sim::NO_UPKEEP_DEMAND;
        herd.upkeep_hands = core_sim::NO_HANDS;
        herd.upkeep_toe.clear();
        herd.upkeep_demanded = None;
    };
    let mut saw_the_road = false;
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        between_turns(&mut app);
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
    assert!(
        app.world.resource::<HerdRegistry>().herds[0].upkeep_hands > core_sim::NO_HANDS,
        "fixture: the party keeps the herd at the source"
    );
    let published = published_party(&app).net_rate_home;
    let answer = ask_the_socket(&mut app);
    assert_eq!(
        answer.rate_home, published,
        "the sheet and the row quote one number for a far kept herd"
    );
    let unkept = {
        let kept_ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        set_live_herd_bill(&mut app, NO_BILL);
        let unkept = ask_the_socket(&mut app).rate_home;
        app.world
            .resource_mut::<core_sim::LadderConfigHandle>()
            .replace(kept_ladder);
        unkept
    };
    let horizon = app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .yield_average_horizon_turns;
    let start = larder(&app, band);
    let mut one_landing: f32 = 0.0;
    for _ in 0..horizon {
        let before = larder(&app, band);
        between_turns(&mut app);
        resolve_a_turn(&mut app);
        one_landing = one_landing.max(larder(&app, band) - before);
    }
    let landed = larder(&app, band) - start;
    let forecast = published * horizon as f32;
    assert!(
        one_landing > 0.0,
        "liveness: food lands home over the horizon"
    );
    assert!(
        (landed - forecast).abs() <= one_landing,
        "a far kept herd's forecast is what lands, to within one landing: \
         landed {landed} against {forecast} (one landing {one_landing})"
    );
    assert!(
        unkept * horizon as f32 - landed > one_landing,
        "the same party priced unkept quotes more than lands — the forecast nets the keeping: \
         unkept {} against landed {landed}",
        unkept * horizon as f32
    );
}

fn larder(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .stores
        .get(FOOD)
        .to_f32()
}

/// What `turns` of a far hunt credited as income, what rotted on the road, and what the larder ends
/// holding. The harness runs the labor pass alone, so the band eats nothing and the larder rot
/// never runs: `last_food_spoiled` is the transit rot alone, accumulated over the run.
fn run_a_caravan(app: &mut App, band: Entity, turns: usize) -> (f32, f32, f32) {
    let mut income = 0.0_f32;
    for _ in 0..turns {
        resolve_a_turn(app);
        income += app
            .world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .last_yields
            .iter()
            .map(|row| row.actual)
            .sum::<f32>();
    }
    let spoiled = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .last_food_spoiled;
    (income, spoiled, larder(app, band))
}

/// ⛔ **A PACK ROTS BY ITS WALK** (#706) — a boar is flesh, which keeps four turns: a herd eight
/// hexes out walks six each way, so every pack is credited as it lands and lost the same turn, and
/// the larder gains nothing; a herd five hexes out walks three, inside the shelf life, so every pack
/// keeps. Both runs must actually land food (liveness), or "nothing rotted" would be trivially true.
#[test]
fn a_walk_longer_than_flesh_keeps_loses_the_pack_and_a_shorter_one_does_not() {
    const TURNS: usize = 40;
    const LEDGER_EPSILON: f32 = 0.01;

    let (mut far, far_band) = world_hunting_at(8);
    let (far_income, far_spoiled, far_larder) = run_a_caravan(&mut far, far_band, TURNS);
    assert!(
        far_income > 0.0,
        "liveness: the far party lands packs within {TURNS} turns"
    );
    assert!(
        (far_spoiled - far_income).abs() < LEDGER_EPSILON,
        "every flesh pack on a six-turn walk rots: spoiled {far_spoiled} vs landed {far_income}"
    );
    assert!(
        far_larder < LEDGER_EPSILON,
        "the larder gains nothing from a rotten caravan: {far_larder}"
    );

    let (mut near, near_band) = world_hunting_at(5);
    let (near_income, near_spoiled, near_larder) = run_a_caravan(&mut near, near_band, TURNS);
    assert!(
        near_income > 0.0,
        "liveness: the near party lands packs within {TURNS} turns"
    );
    assert_eq!(
        near_spoiled, 0.0,
        "a three-turn walk keeps flesh — nothing rots"
    );
    assert!(
        (near_larder - near_income).abs() < LEDGER_EPSILON,
        "every pack that kept is in the larder: {near_larder} vs {near_income}"
    );
}

/// The shipped flesh shelf life — what a boar's meat keeps, and so the `transitKeepsTurns` a far
/// boar hunt must publish.
fn flesh_keeps(app: &App) -> f32 {
    app.world
        .resource::<core_sim::DemographicsConfigHandle>()
        .get()
        .keeping
        .shelf_life("flesh")
        .expect("flesh is a shipped keeping class")
}

/// ⛔ **THE HUNT PANEL SAYS UP FRONT WHAT THE WALK WILL SPOIL** (#706) — on the published row AND on
/// the compose-sheet answer a player reads before committing. A boar hunt eight hexes out walks six
/// turns against flesh's four: its rate home nets to nothing, the spoiled rate carries the whole
/// take, and the shelf life that loses it is flesh's. Five hexes out (three turns) keeps
/// everything. Asserted off the ENCODED row and the query answer, never the in-process party.
#[test]
fn a_far_hunt_publishes_and_quotes_the_take_its_walk_spoils() {
    for (distance, rots) in [(8, true), (5, false)] {
        let (mut app, _) = world_hunting_at(distance);
        let keeps = flesh_keeps(&app);
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
            "liveness: the {distance}-hex party puts somebody on the road"
        );
        let published = published_party(&app);
        let answer = ask_the_socket(&mut app);
        if rots {
            assert!(
                published.net_rate_home.abs() < 1e-4 && answer.rate_home.abs() < 1e-4,
                "a six-turn walk keeps no flesh: row {} / quote {}",
                published.net_rate_home,
                answer.rate_home
            );
            assert!(
                published.spoiled_rate_home > 0.0 && answer.spoiled_rate_home > 0.0,
                "the take is published as spoiled: row {} / quote {}",
                published.spoiled_rate_home,
                answer.spoiled_rate_home
            );
            assert_eq!(published.transit_keeps_turns, keeps);
            assert_eq!(answer.transit_keeps_turns, keeps);
        } else {
            assert!(
                published.net_rate_home > 0.0 && answer.rate_home > 0.0,
                "liveness: a three-turn walk brings food home"
            );
            assert_eq!(published.spoiled_rate_home, 0.0);
            assert_eq!(answer.spoiled_rate_home, 0.0);
            assert_eq!(published.transit_keeps_turns, 0.0);
            assert_eq!(answer.transit_keeps_turns, 0.0);
        }
        assert_eq!(
            answer.spoiled_rate_home, published.spoiled_rate_home,
            "the sheet and the row it becomes quote one spoiled rate"
        );
    }
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

/// **What the fixture band publishes about its stood-down parties walking home**, off the ENCODED
/// buffer — with `idleWorkers`, which must not count them.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PublishedHomeward {
    workers: u32,
    food: f32,
    food_spoils: f32,
    next_load_in: u32,
    all_home_in: u32,
    idle: u32,
}

fn published_homeward(app: &App) -> PublishedHomeward {
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
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .expect("the fixture band is on the wire");
    PublishedHomeward {
        workers: cohort.homewardWorkers(),
        food: cohort.homewardFood(),
        food_spoils: cohort.homewardFoodSpoils(),
        next_load_in: cohort.homewardNextLoadIn(),
        all_home_in: cohort.homewardAllHomeIn(),
        idle: cohort.idleWorkers(),
    }
}

/// The band's stood-down walks still on the road, in process — only to drive the loop; every claim
/// is asserted on the wire.
fn homeward_walks(app: &App, band: Entity) -> Vec<core_sim::work_party::HomewardWalk> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .homeward
        .clone()
}

/// **CANCEL THE FAR HUNT THE WAY THE PLAYER DOES** — `abandon` / `cancel_order`'s own path: the row
/// is dropped and its party handed to [`core_sim::bring_the_dropped_party_home`], in the command
/// window, between turns. The snapshot is recaptured so the wire reads the stood-down band.
fn cancel_the_hunt(app: &mut App, band: Entity) {
    let row = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .drop_source_row(&hunt_target())
        .expect("the far row is there to drop");
    core_sim::bring_the_dropped_party_home(&mut app.world, band, &row);
    recapture_snapshot_in_place(&mut app.world);
}

/// A bound on a stood-down party's walk home — the longest fixture walk, with room. A guard against
/// hanging, not a prediction.
const TURNS_TO_WALK_HOME: usize = 20;

/// ⛔ **A CANCELLED FAR HUNT HANDS NOTHING OVER — EVERYBODY WALKS HOME, AND THE FOOD ROTS BY ITS
/// WALK** (#706). Five hexes out (a three-turn walk, inside flesh's four) every pack keeps; eight out
/// (six turns) every pack is credited as it lands and struck as spoiled. Either way:
///
/// - the cancel itself lands nothing, and the wire says who is walking home, with what;
/// - the hands are away from the pool (`idleWorkers`) until their group arrives — never all at once
///   while a pack is still on the road;
/// - every pack lands on its own remaining walk, the load on the whole walk, so the last hand is home
///   exactly `walk` turns after the cancel;
/// - all of it is booked on the route arm, and the rot is `foodSpoiled`'s.
#[test]
fn a_cancelled_far_hunt_walks_every_pack_home_and_rots_it_by_its_walk() {
    for (distance, walk, rots) in [(5_u32, 3_u32, false), (8, 6, true)] {
        let (mut app, band) = world_hunting_at(distance);
        let carried = a_caravan_carrying_food(&mut app, band);
        let larder_before = larder(&app, band);
        let routed_before = route_received(&app, band);
        let spoiled_before = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the band keeps its cohort")
            .last_food_spoiled;

        cancel_the_hunt(&mut app, band);
        assert_eq!(
            larder(&app, band),
            larder_before,
            "{distance} hexes: the cancel hands no food over"
        );
        let at_cancel = published_homeward(&app);
        assert_eq!(
            at_cancel.workers, CREW,
            "{distance} hexes: the whole crew is walking home"
        );
        assert_eq!(
            at_cancel.idle, 0,
            "{distance} hexes: none of them is idle yet"
        );
        assert!(
            (at_cancel.food - carried).abs() < SAME_FOOD,
            "{distance} hexes: the wire carries what they bring: {} of {carried}",
            at_cancel.food
        );
        assert_eq!(at_cancel.all_home_in, walk, "the load walks the whole walk");
        assert!(at_cancel.next_load_in > 0, "the soonest load is still out");
        if rots {
            assert!(
                (at_cancel.food_spoils - carried).abs() < SAME_FOOD,
                "a six-turn walk will spoil every flesh pack: {}",
                at_cancel.food_spoils
            );
        } else {
            assert_eq!(at_cancel.food_spoils, 0.0, "a three-turn walk keeps it all");
        }

        let mut turns = 0;
        let mut previous = at_cancel;
        while !homeward_walks(&app, band).is_empty() {
            assert!(turns < TURNS_TO_WALK_HOME, "the walk home ends");
            let landing_now: u32 = homeward_walks(&app, band)
                .iter()
                .filter(|walk| walk.turns_left <= 1)
                .map(|walk| walk.workers)
                .sum();
            resolve_a_turn(&mut app);
            turns += 1;
            let now = published_homeward(&app);
            assert_eq!(
                now.workers,
                previous.workers - landing_now,
                "{distance} hexes, turn {turns}: hands rejoin only as their group arrives"
            );
            assert_eq!(
                now.idle,
                CREW - now.workers,
                "{distance} hexes, turn {turns}: every hand not walking is idle again"
            );
            previous = now;
        }
        assert_eq!(
            turns, walk as usize,
            "the last hand is home after the whole walk"
        );
        assert_eq!(
            previous,
            PublishedHomeward {
                idle: CREW,
                ..at_cancel_zeroed()
            }
        );

        let landed = larder(&app, band) - larder_before;
        let routed = route_received(&app, band) - routed_before;
        let spoiled = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the band keeps its cohort")
            .last_food_spoiled
            - spoiled_before;
        assert!(
            (routed - carried).abs() < SAME_FOOD,
            "{distance} hexes: every pack is booked home on the route arm: {routed} of {carried}"
        );
        if rots {
            assert!(
                landed.abs() < SAME_FOOD && (spoiled - carried).abs() < SAME_FOOD,
                "a six-turn walk keeps no flesh: landed {landed}, spoiled {spoiled} of {carried}"
            );
        } else {
            assert!(
                (landed - carried).abs() < SAME_FOOD && spoiled.abs() < SAME_FOOD,
                "a three-turn walk keeps every pack: landed {landed}, spoiled {spoiled}"
            );
        }
    }
}

/// The band row with nobody walking home — every homeward field `0`.
fn at_cancel_zeroed() -> PublishedHomeward {
    PublishedHomeward {
        workers: 0,
        food: 0.0,
        food_spoils: 0.0,
        next_load_in: 0,
        all_home_in: 0,
        idle: 0,
    }
}

/// **Run the band's stood-down walks until the last one lands**, and answer how many turns it took.
fn walk_them_all_home(app: &mut App, band: Entity) -> usize {
    let mut turns = 0;
    while !homeward_walks(app, band).is_empty() {
        assert!(turns < TURNS_TO_WALK_HOME, "the walk home ends");
        resolve_a_turn(app);
        turns += 1;
    }
    turns
}

/// A hex inside the band's own apron (`band_work_range` 2), on the camp's row.
const INSIDE_THE_APRON: u32 = 1;
/// How close two food totals must agree — the settled load is summed in `f32` across walkers.
const SAME_FOOD: f32 = 1e-2;

/// **A CARAVAN WITH A NONZERO LOAD, A HAND AT THE SOURCE AND SOMEBODY ON THE ROAD** — run a far hunt until both hold, and
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
        // A hand at the source too, so the load has a carrier — a load nobody is there to carry
        // is left behind when the party stands down.
        if party.hunters_on_the_road() > 0 && party.load_cargo > 0.0 && party.hunters_present() > 0
        {
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

/// ⛔ **A HERD THAT WANDERS BACK INSIDE THE APRON SENDS THE PARTY WALKING HOME, ONCE.** The row
/// stops posting — the band's own hands reach the herd again — so its caravan has ended. Nothing is
/// handed over at once: the party takes the turn's step, and every pack and the load land over the
/// walk home, on the route arm, and never again; the row carries no party and publishes none.
#[test]
fn a_herd_back_inside_the_apron_walks_its_caravan_home_once() {
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
    let route_before = route_received(&app, band);
    resolve_a_turn(&mut app);
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
    let walking = published_homeward(&app);
    assert!(
        walking.workers > 0 && walking.food > 0.0,
        "the band publishes the party still walking home with its load: {walking:?}"
    );
    walk_them_all_home(&mut app, band);
    let routed = route_received(&app, band) - route_before;
    assert!(
        (routed - carried).abs() < SAME_FOOD,
        "the load and every pack land home on the route arm: {routed} of {carried}"
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

/// ⛔ **A HERD GONE FROM THE REGISTRY SENDS ITS PARTY WALKING HOME AS THE ROW LAPSES.** The row ends
/// (`status=lapsed reason=herd_gone`) that turn; the load and every walker's pack outlive it on the
/// band's homeward list and land over the walk, booked as the band's own party coming home, once.
#[test]
fn a_vanished_herd_walks_its_caravan_home_as_the_row_lapses() {
    let (mut app, band) = world_hunting_at(5);
    let carried = a_caravan_carrying_food(&mut app, band);
    app.world.resource_mut::<HerdRegistry>().clear();
    let larder_before = larder(&app, band);
    let route_before = route_received(&app, band);
    let party_home_before = party_home_booked(&app, band);
    resolve_a_turn(&mut app);
    assert!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .assignments
            .is_empty(),
        "the row whose herd is gone lapses"
    );
    assert!(
        published_homeward(&app).workers > 0,
        "…while its party is still walking home"
    );
    walk_them_all_home(&mut app, band);
    // The harness never runs the per-turn clear, so the crossings list holds the whole run.
    let booked = party_home_booked(&app, band) - party_home_before;
    let landed = larder(&app, band) - larder_before;
    let routed = route_received(&app, band) - route_before;
    assert!(
        (booked - carried).abs() < SAME_FOOD,
        "the homecoming is booked as the band's own party coming home, not as trade: {booked} of \
         {carried}"
    );
    assert!(
        (landed - carried).abs() < SAME_FOOD,
        "the load and every pack land home over the walk: {landed} of {carried}"
    );
    assert!(
        (routed - carried).abs() < SAME_FOOD,
        "…on the route arm, outside any turn's income: {routed} of {carried}"
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
/// The shipped forestry kit — `sled` + `axe`, claimed whole like every job's kit.
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
    let pricing = core_sim::work_party::CaravanPricing::resolve(
        &equipment,
        kit,
        // The whole crew cuts and claims: the pack is one hand's haul, whatever the crew.
        CREW as f32,
        CREW as f32,
        core_sim::SourcePriority::default(),
        &wear,
        &[],
        &labor,
    );
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
        // **The bare crew's own lift**: no tools, and the bare haul over the weight as its carry.
        let carry = core_sim::extraction::DepositCarry::of(
            &app.world.resource::<core_sim::LaborConfigHandle>().get(),
            &app.world
                .resource::<core_sim::MaterialsConfigHandle>()
                .get(),
            WOOD,
        )
        .expect("wood is a material");
        core_sim::extraction::take_from_deposit(
            &mut working,
            CREW as f32,
            core_sim::extraction::CrewLift {
                tools: core_sim::extraction::NO_DEPOSIT_GEAR,
                carry: carry.crew_carry(carry.haul_baseline, CREW as f32),
            },
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
        "the row publishes the take seam's own figure"
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

/// ⛔ **UNASSIGNING A DEPOSIT CARAVAN MID-WALK WALKS EVERY PACK HOME AS MATERIAL** — into the store
/// over the walk home, booked as the band's own party coming home, and **nothing** onto the larder,
/// the food ledger's route arm or the published homeward food: a working's timber is not food, and
/// it keeps however long the walk.
#[test]
fn unassigning_a_deposit_caravan_mid_walk_walks_every_pack_home_as_material() {
    let (mut app, band) = world_extracting_at(5, None);
    first_pack_on_the_road(&mut app, band);
    let party = extract_party(&app, band).expect("the far row carries a party");
    // The load goes home only with a hand at the source to carry it — so the fixture needs one,
    // and the hands an unassign leaves there must walk home with it, not vanish into the pool.
    assert!(
        party.hunters_present() > 0 && party.load_cargo > 0.0,
        "fixture: hands at the source with a load"
    );
    let carried: f32 = party.load_cargo + party.on_the_road.iter().map(|w| w.cargo).sum::<f32>();
    assert!(carried > 0.0, "fixture: the caravan is carrying something");
    let wood_before = wood_held(&app, band);
    let larder_before = larder(&app, band);
    let food_routed_before = route_received(&app, band);
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(extract_target(5), 0, CREW, None);
    let wood_booked = |app: &App| -> f32 {
        app.world
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
            .sum()
    };
    let booked_before = wood_booked(&app);
    resolve_a_turn(&mut app);
    assert!(
        extract_party(&app, band).is_none(),
        "the party is stood down"
    );
    let walking = published_homeward(&app);
    assert_eq!(
        walking.workers, CREW,
        "every hand it held walks home — the source hands too, not just the porter"
    );
    assert_eq!(
        (walking.food, walking.food_spoils),
        (0.0, 0.0),
        "wood is not homeward food"
    );
    walk_them_all_home(&mut app, band);
    // The harness never runs the per-turn clear, so the crossings list holds the whole run.
    let booked = wood_booked(&app) - booked_before;
    let landed = wood_held(&app, band) - wood_before;
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

/// ⛔ **A WOODCUTTING CREW HAULS ON ITS SLEDS** — the kit is claimed whole, so its porters carry the
/// sled's `hunt_carry` over the material's weight, against the bare-handed carry over the same
/// weight. Read off the first porter the turn actually sends, and pinned to the caravan pricing's
/// own figure over the row's kit.
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
    assert!(
        (sled - priced_pack(&sled_app, sled_band, &extract_kit(WOODCUTTING_KIT))).abs() < 1e-3,
        "a sledded porter carries the sled's haul over the weight: {sled}"
    );
    assert!(
        sled > bare,
        "the sled must carry more timber home per trip: {sled} vs {bare}"
    );
}

/// ⛔ **A FAR FELLING CREW WITH THE WOODCUTTING KIT CARRIES THE SLED PACK** — the kit is claimed
/// whole wherever the working stands, so a felling party (whose axes cut) still hauls on its sleds:
/// the pack is the sled's haul over the weight, the caravan pricing's own figure, and larger than
/// the bare-handed pack.
#[test]
fn a_far_felling_crew_with_the_woodcutting_kit_carries_the_sled_pack() {
    let (mut app, band) = world_extracting_at(5, Some(WOODCUTTING_KIT));
    seat_the_wood_at_felling(&mut app, 5);
    let felling = first_pack_on_the_road(&mut app, band);
    assert!(
        (felling - priced_pack(&app, band, &extract_kit(WOODCUTTING_KIT))).abs() < 1e-3,
        "a felling party's porter carries the pricing's haul over the weight: {felling}"
    );
    assert!(
        felling > bare_wood_pack(&app),
        "…which is the sled's, larger than the bare pack: {felling} vs {}",
        bare_wood_pack(&app)
    );
}

/// **A WEIGHT HEAVY ENOUGH THAT A LOCAL CREW'S CARRY BINDS** — six bare hands at deadfall cut
/// `6 × 0.3 = 1.8` a turn, and carry `6 × 12 ÷ 100 = 0.72`; with sleds they cut more and carry
/// `6 × 40 ÷ 100` at full cover. A fixture weight, not a tuning.
const A_LOAD_TOO_HEAVY_TO_CUT_FREELY: f64 = 100.0;

/// Install `weight` as wood's weight in the materials table.
fn weigh_wood_at(app: &mut App, weight: f64) {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("builtin parses");
    json["materials"][WOOD]["weight"] = serde_json::json!(weight);
    let table = core_sim::MaterialsConfig::from_json_str(&json.to_string())
        .expect("the heavier table validates");
    app.world
        .resource_mut::<core_sim::MaterialsConfigHandle>()
        .replace(std::sync::Arc::new(table));
}

/// ⛔ **A LOCAL CUT IS CAPPED BY WHAT THE CREW CAN CARRY, LIKE A HUNT** — the crew's haul carry over
/// the material's weight, in addition to the rung's cut rate. On a material heavy enough that the
/// carry binds, a bare crew's take is exactly its bare carry over the weight, and a sledded crew
/// (the woodcutting kit) takes more — the sled raises the cap as it raises a hunter's haul.
#[test]
fn a_local_extract_take_is_capped_by_carry_over_weight() {
    let (mut bare_app, bare_band) = world_extracting_at(INSIDE_THE_APRON, None);
    weigh_wood_at(&mut bare_app, A_LOAD_TOO_HEAVY_TO_CUT_FREELY);
    resolve_a_turn(&mut bare_app);
    let bare = wood_held(&bare_app, bare_band);
    let labor = bare_app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get();
    let bare_cap = CREW as f32 * labor.hunt.per_worker_biomass_capacity
        / A_LOAD_TOO_HEAVY_TO_CUT_FREELY as f32;
    assert!(bare > 0.0, "liveness: the crew cuts something");
    assert!(
        (bare - bare_cap).abs() < 1e-3,
        "a bare crew takes exactly its carry over the weight: {bare} vs {bare_cap}"
    );
    let (mut sled_app, sled_band) = world_extracting_at(INSIDE_THE_APRON, Some(WOODCUTTING_KIT));
    weigh_wood_at(&mut sled_app, A_LOAD_TOO_HEAVY_TO_CUT_FREELY);
    resolve_a_turn(&mut sled_app);
    let sled = wood_held(&sled_app, sled_band);
    assert!(sled > bare, "the sled raises the cap: {sled} vs {bare}");
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

/// The work-party query for the fixture's far wood, asked at `kit`, over [`CREW`].
fn ask_about_the_wood(app: &mut App, distance: u32, kit: &str) -> WorkPartyForecastReply {
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::WorkPartyForecast(WorkPartyForecastQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            source: WorkPartySource::Extract {
                x: CAMP.x + distance,
                y: CAMP.y,
                material: WOOD.to_string(),
            },
            kit_id: kit.to_string(),
            workers: CREW,
            floor: FLOOR,
        }),
    );
    match reply {
        QueryReply::WorkPartyForecast(answer) => answer,
        other => panic!("the work-party query must answer with a forecast, got {other:?}"),
    }
}

/// The kit whose `deposit_take` names deadfall and not felling — the control a felling forecast's
/// tool term must differ from.
const SLEDDING_KIT: &str = "sledding";

/// ⛔ **ON THE TURN A FAR WORKING COMPLETES A RUNG, THE ROW'S `netRateHome` IS THE QUERY'S
/// `rate_home`** (PR #757 review). The turn's take is cut at the rung the working held when the turn
/// began, but the caravan forecast it publishes steps the working as the turn LEAVES it — so its
/// tools must be priced at the rung it holds now. Priced at the old rung, a wood raised from deadfall
/// to felling this turn published a horizon of deadfall tool terms while the query, which reads the
/// working as it stands, quoted felling's axes: two numbers for one row.
///
/// A far wood one hair short of felling's top, a queued `fell` and one builder, so the raise completes
/// on the first turn. Then, off the encoded snapshot, the row's `netRateHome` equals the query's
/// `rate_home` at the row's own Woodcutting kit exactly — and that figure carries felling's tool: the
/// same query at the Sled kit, whose sled lifts deadfall and not felling, quotes a different rate.
#[test]
fn a_far_workings_rate_home_on_the_turn_it_completes_a_rung_is_the_querys() {
    const DISTANCE: u32 = 5;
    const BUILDERS: u32 = 1;
    /// How far below felling's top the working is seated — far less than one builder banks in a
    /// turn, so the raise completes on the first turn.
    const SHORT_OF_THE_TOP: f32 = 0.01;

    let (mut app, band) = world_extracting_at(DISTANCE, Some(WOODCUTTING_KIT));
    let tile = UVec2::new(CAMP.x + DISTANCE, CAMP.y);
    {
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
        working.set_ladder_position(base + width - SHORT_OF_THE_TOP, &ladder, felling.branch());
        assert_eq!(
            working.rung(),
            core_sim::RungKey::ForestryDeadfall,
            "fixture: the working must still hold deadfall when the turn begins"
        );
        app.world
            .resource_mut::<core_sim::DepositRegistry>()
            .insert(working);
    }
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(
            FACTION,
            core_sim::extraction::WOODCRAFT_DISCOVERY_ID,
            scalar_one(),
        );
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band")
        .working = scalar_from_f32((CREW + BUILDERS) as f32);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.push(LaborAssignment {
            party: None,
            target: LaborTarget::Builders,
            workers: BUILDERS,
            kit: None,
            priority: SourcePriority::default(),
        });
        assert!(allocation.enqueue_build(
            core_sim::BuildSource::Deposit {
                tile,
                material: WOOD.to_string(),
            },
            core_sim::BuildJob::Rung(core_sim::Improvement::Fell),
        ));
    }

    resolve_a_turn(&mut app);
    assert_eq!(
        app.world
            .resource::<core_sim::DepositRegistry>()
            .source(tile, WOOD)
            .expect("the far working")
            .rung(),
        core_sim::RungKey::ForestryFelling,
        "fixture: the raise must complete on this turn"
    );
    assert!(
        extract_party(&app, band).is_some(),
        "fixture: a working past the apron posts a party"
    );

    let published = published_party_of(&app, EXTRACT_KIND);
    let answer = ask_about_the_wood(&mut app, DISTANCE, WOODCUTTING_KIT);
    assert!(
        answer.rate_home > 0.0,
        "liveness: a wood brings timber home ({answer:?})"
    );
    assert_eq!(
        published.net_rate_home, answer.rate_home,
        "the row and the query quote one rate on the turn the working climbed"
    );
    let sled_only = ask_about_the_wood(&mut app, DISTANCE, SLEDDING_KIT);
    assert_ne!(
        answer.rate_home, sled_only.rate_home,
        "the rate carries felling's tool: the axe lifts felling where the sled does not"
    );
}

/// **Run the fixture hunt until its party stands at the herd with every hand at the source** — the
/// turn its walk out ends, before any pack has left.
fn a_party_arrived_at_the_herd(app: &mut App) {
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        resolve_a_turn(app);
        let party = published_party(app);
        if party.walk_out_remaining == 0 && party.party_workers == CREW {
            assert_eq!(
                party.hunters_on_the_road, 0,
                "fixture: all six at the source"
            );
            return;
        }
    }
    panic!("liveness: the party reaches the herd");
}

/// ⛔ **A CREW CUT SHORT OF ZERO WALKS ITS DROPPED HANDS HOME, FROM THE COMMAND** (#706) — a
/// six-hunter party five hexes out (a three-turn walk), cut to two with all six at the source. The
/// four dropped hands leave the source in the command itself and walk the whole walk carrying
/// nothing. **Before any turn runs**, the band publishes them walking home and `idleWorkers` does not
/// offer them, and `set_assignment` refuses them to another row — they are not in the band to give.
/// They rejoin the pool only as they arrive. Asserted on the encoded wire.
#[test]
fn cutting_a_far_crew_walks_the_dropped_hands_home() {
    /// The crew the row is cut to.
    const KEPT: u32 = 2;
    /// The walk at five hexes, in turns.
    const WALK: u32 = 3;
    let (mut app, band) = world_hunting_at(5);
    a_party_arrived_at_the_herd(&mut app);
    let before = published_homeward(&app);
    let workforce = |app: &App| {
        core_sim::BandWorkforce::resolve(
            app.world.get::<PopulationCohort>(band),
            app.world.get::<LaborAllocation>(band),
            None,
        )
    };
    let assignable = workforce(&app).assignable();
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(hunt_target(), KEPT, assignable, None);
    recapture_snapshot_in_place(&mut app.world);
    let cut = published_homeward(&app);
    assert_eq!(cut.idle, before.idle, "the cut frees nobody");
    assert_eq!(
        cut.workers,
        CREW - KEPT,
        "the four dropped hands walk home from the command"
    );
    assert_eq!(cut.food, 0.0, "they carry nothing — the load stays");
    assert_eq!(
        cut.all_home_in, WALK,
        "from the source, over the whole walk"
    );
    assert_eq!(workforce(&app).assignable(), CREW - (CREW - KEPT));
    let assignable = workforce(&app).assignable();
    let given = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(LaborTarget::Scout, CREW - KEPT, assignable, None);
    assert_eq!(given, 0, "the walkers cannot be given to another row");
    for turn in 1..=WALK {
        resolve_a_turn(&mut app);
        let now = published_homeward(&app);
        assert_eq!(published_party(&app).party_workers, KEPT);
        if turn < WALK {
            assert_eq!(now.idle, before.idle, "turn {turn}: still walking");
            assert_eq!(now.workers, CREW - KEPT);
        } else {
            assert_eq!(now.workers, 0, "home after the whole walk");
            assert_eq!(now.idle, before.idle + CREW - KEPT, "…and back in the pool");
        }
    }
}

/// The fixture hunt row's own walkers, off the ENCODED row — `None` when the row is gone.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RowWalkingHome {
    crew: u32,
    workers: u32,
    all_home_in: u32,
    food: f32,
}

fn published_row_walking_home(app: &App) -> Option<RowWalkingHome> {
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
    let cohort = envelope
        .payload_as_snapshot()
        .and_then(|snapshot| snapshot.population())
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .find(|cohort| cohort.bandId() == BAND)
        .expect("the fixture band is on the wire");
    cohort
        .laborAssignments()
        .into_iter()
        .flatten()
        .find(|row| row.kind().unwrap_or_default() == "hunt")
        .map(|row| RowWalkingHome {
            crew: row.workers(),
            workers: row.homewardWorkers(),
            all_home_in: row.homewardAllHomeIn(),
            food: row.homewardFood(),
        })
}

/// **A far row unassigned to zero publishes its own walkers.** The row survives at a crew of `0`,
/// and it carries the hands that left it walking home — the same count, last-home turn and food as
/// the band's total, which is all of them here. Read off the ENCODED row.
#[test]
fn a_far_row_unassigned_to_zero_publishes_the_hands_walking_home_from_it() {
    let (mut app, band) = world_hunting_at(5);
    a_party_arrived_at_the_herd(&mut app);
    let assignable = core_sim::BandWorkforce::resolve(
        app.world.get::<PopulationCohort>(band),
        app.world.get::<LaborAllocation>(band),
        None,
    )
    .assignable();
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(hunt_target(), 0, assignable, None);
    recapture_snapshot_in_place(&mut app.world);

    let band_total = published_homeward(&app);
    let row = published_row_walking_home(&app).expect("the unassigned row is still on the wire");
    assert_eq!(row.crew, 0);
    assert_eq!(row.workers, CREW, "every hand that left the row walks home");
    assert_eq!(row.workers, band_total.workers);
    assert_eq!(row.all_home_in, band_total.all_home_in);
    assert!(row.all_home_in > 0, "liveness: they are still on the road");
    assert_eq!(row.food, band_total.food);
}

/// **A partial cut shows the row's crew AND its walkers**, and once they arrive the row reads
/// nobody walking.
#[test]
fn a_partly_cut_far_row_shows_its_crew_and_its_walkers_until_they_arrive() {
    /// The crew the row is cut to.
    const KEPT: u32 = 2;
    let (mut app, band) = world_hunting_at(5);
    a_party_arrived_at_the_herd(&mut app);
    let assignable = core_sim::BandWorkforce::resolve(
        app.world.get::<PopulationCohort>(band),
        app.world.get::<LaborAllocation>(band),
        None,
    )
    .assignable();
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_assignment(hunt_target(), KEPT, assignable, None);
    recapture_snapshot_in_place(&mut app.world);

    let row = published_row_walking_home(&app).expect("the cut row is on the wire");
    assert_eq!(row.crew, KEPT);
    assert_eq!(row.workers, CREW - KEPT);
    assert_eq!(row.all_home_in, published_homeward(&app).all_home_in);

    for _ in 0..TURNS_TO_WALK_HOME {
        if homeward_walks(&app, band).is_empty() {
            break;
        }
        resolve_a_turn(&mut app);
    }
    let row = published_row_walking_home(&app).expect("the cut row is on the wire");
    assert_eq!(row.crew, KEPT);
    assert_eq!((row.workers, row.all_home_in), (0, 0), "everyone is home");
    assert_eq!(row.food, 0.0);
}

/// **An abandoned row's walkers are on the band only** — the row is gone, so no row carries them,
/// and the band's total still does.
#[test]
fn an_abandoned_rows_walkers_appear_on_the_band_only() {
    let (mut app, band) = world_hunting_at(5);
    a_party_arrived_at_the_herd(&mut app);
    cancel_the_hunt(&mut app, band);
    assert_eq!(published_row_walking_home(&app), None, "the row is gone");
    assert!(
        published_homeward(&app).workers > 0,
        "the band still counts its hands walking home"
    );
}

/// ⛔ **THE STARVATION SHED'S HANDS LEAVE WITHOUT WALKING HOME — IT SHEDS ONCE.** A far party of six
/// whose band loses two working people: the shed trims the row to four, and the two it removed are
/// people the band no longer has. Listing them as walking home would count them against the pool
/// next turn and fire the shed again (6 → 4 → 2 → 0). The row holds at four, nobody walks home, and
/// nothing is idle, turn after turn. Asserted on the encoded wire.
#[test]
fn a_far_row_the_shed_trims_sheds_once_and_nobody_walks_home() {
    /// The working people the band keeps.
    const LEFT: u32 = CREW - 2;
    /// Turns watched after the loss — several, so a repeat shed would show.
    const TURNS_AFTER: usize = 6;
    let (mut app, band) = world_hunting_at(5);
    a_party_arrived_at_the_herd(&mut app);
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .working = scalar_from_f32(LEFT as f32);
    for turn in 1..=TURNS_AFTER {
        resolve_a_turn(&mut app);
        let party = published_party(&app);
        let homeward = published_homeward(&app);
        assert_eq!(
            party.party_workers, LEFT,
            "turn {turn}: the shed cut two, once"
        );
        assert_eq!(
            homeward.workers, 0,
            "turn {turn}: the shed's hands walk nowhere"
        );
        assert_eq!(
            homeward.idle, 0,
            "turn {turn}: every hand the band has is on the row"
        );
    }
}

/// ⛔ **A FAR HAY ROW PRINTS SMOOTHED FODDER AND MATERIALS RATES HOME, NOT LUMPS** (#706) — its hay
/// and fibre ride the packs, so the row's per-turn `fodderYield` / `materialYield` read only what
/// landed and are zero between packs. `fodderRateHome` / `materialsRateHome` are their
/// `netRateHome`: off the same caravan forecast, non-zero on exactly those turns. Read off the
/// ENCODED row.
#[test]
fn a_far_hay_row_publishes_smoothed_fodder_and_materials_rates_home() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    /// Hexes between the band and the patch along one row — past the apron.
    const FAR: u32 = 8;
    const HAY: &str = "hay_grass";
    /// A stocked stand, so the crew's take is never what runs out.
    const STOCKED: f32 = 0.8;

    let mut app = build_test_app();
    app.update();
    let (coord, camp) = {
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
        let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
        let mut tiles: Vec<core_sim::Tile> = app
            .world
            .query::<&core_sim::Tile>()
            .iter(&app.world)
            .cloned()
            .collect();
        tiles.sort_by_key(|tile| (tile.position.y, tile.position.x));
        let registry = app.world.resource::<core_sim::ForageRegistry>();
        let index = app.world.resource::<TileRegistry>();
        tiles
            .iter()
            .filter(|tile| registry.patch(tile.position).is_some())
            .find_map(|tile| {
                let camp = UVec2::new(tile.position.x + FAR, tile.position.y);
                index.index(camp.x, camp.y)?;
                core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed)
                    .iter()
                    .any(|share| share.species == HAY && share.share > 0.0)
                    .then_some((tile.position, camp))
            })
            .expect("the harness map grows hay somewhere with room to camp far off")
    };
    {
        let mut registry = app.world.resource_mut::<core_sim::ForageRegistry>();
        let patch = registry.patch_mut(coord).expect("the chosen patch");
        patch.biomass = patch.carrying_capacity * STOCKED;
    }
    {
        let mut sites = app.world.resource_mut::<core_sim::FoodSiteRegistry>();
        if !sites.is_site(coord) {
            let module = core_sim::FoodModule::SavannaGrassland;
            let mut entries = sites.sites().to_vec();
            entries.push(core_sim::FoodSiteEntry {
                position: coord,
                module,
                kind: module.site_kind(),
                seasonal_weight: 1.0,
            });
            sites.set_sites(entries);
        }
    }
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(FACTION, core_sim::FODDERING_DISCOVERY_ID, scalar_one());
    spawn_band_camped_at(
        &mut app,
        camp,
        LaborTarget::Forage {
            tile: coord,
            floor: FLOOR,
            species: None,
            take_species: core_sim::TakeSelection::from_keys([HAY]),
        },
        None,
    );
    let row_of = |app: &App| -> (u32, f32, f32, f32, Vec<(String, f32)>) {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let row = envelope
            .payload_as_snapshot()
            .and_then(|snapshot| snapshot.population())
            .and_then(|section| section.populations())
            .expect("the cohort list")
            .iter()
            .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
            .find(|row| row.kind().unwrap_or_default() == "forage")
            .expect("the far forage row is on the wire");
        let yielded: f32 = row
            .materialYield()
            .map(|list| list.iter().map(|payoff| payoff.amount()).sum())
            .unwrap_or(0.0);
        (
            row.partyWorkers(),
            row.fodderYield(),
            yielded,
            row.fodderRateHome(),
            row.materialsRateHome()
                .map(|list| {
                    list.iter()
                        .map(|payoff| {
                            (
                                payoff.materialId().unwrap_or_default().to_string(),
                                payoff.amount(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        )
    };
    let mut between_packs = false;
    let mut a_landing = false;
    for _ in 0..TURNS_TO_SEE_A_PORTER {
        resolve_a_turn(&mut app);
        let (party, fodder_now, materials_now, fodder_home, materials_home) = row_of(&app);
        assert!(party > 0, "the far row posts a party");
        a_landing |= fodder_now > 0.0;
        if fodder_home > 0.0 && fodder_now == 0.0 && materials_now == 0.0 {
            assert!(
                materials_home
                    .iter()
                    .any(|(id, amount)| id == "fibre" && *amount > 0.0),
                "the fibre home rate rides beside the hay's: {materials_home:?}"
            );
            between_packs = true;
        }
        if between_packs && a_landing {
            break;
        }
    }
    assert!(
        between_packs,
        "a turn with no pack landing still publishes the smoothed home rates"
    );
    assert!(a_landing, "liveness: a pack of hay lands within the run");
}

/// ⛔ **A FAR HUNT'S HIDES WALK HOME IN ITS PACKS** (#706) — a boar party five hexes out (a
/// three-turn walk, inside flesh's shelf life) kills from its first turn at the herd, but the band's
/// hide arrives only on the turns a pack lands: the encoded row's `materialYield` is non-zero exactly
/// when its food `actualYield` is (one pack, one landing), and the store holds no hide before the
/// first. On a turn no pack lands, the row still prints a smoothed `materialsRateHome` for hide, and
/// the compose reply quotes the same one. Liveness: a pack lands, and a no-landing turn is seen.
#[test]
fn a_far_hunt_lands_its_hides_with_its_packs_and_prints_a_smoothed_rate_home() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    const HIDE: &str = "hide";
    let (mut app, band) = world_hunting_at(5);
    let hide_held = |app: &App| {
        app.world
            .get::<PopulationCohort>(band)
            .expect("the band keeps its cohort")
            .stores
            .material_total(HIDE)
            .to_f32()
    };
    let row_of = |app: &App| -> (f32, f32, f32) {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let row = envelope
            .payload_as_snapshot()
            .and_then(|snapshot| snapshot.population())
            .and_then(|section| section.populations())
            .expect("the cohort list")
            .iter()
            .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
            .find(|row| row.kind().unwrap_or_default() == "hunt")
            .expect("the far hunt row is on the wire");
        let hide_of = |list: Vec<(Option<&str>, f32)>| -> f32 {
            list.into_iter()
                .filter(|(id, _)| *id == Some(HIDE))
                .map(|(_, amount)| amount)
                .sum()
        };
        (
            row.actualYield(),
            hide_of(
                row.materialYield()
                    .map(|list| list.iter().map(|p| (p.materialId(), p.amount())).collect())
                    .unwrap_or_default(),
            ),
            hide_of(
                row.materialsRateHome()
                    .map(|list| list.iter().map(|p| (p.materialId(), p.amount())).collect())
                    .unwrap_or_default(),
            ),
        )
    };
    let mut landed = false;
    let mut between_packs = false;
    for turn in 0..TURNS_TO_SEE_A_PORTER {
        let held_before = hide_held(&app);
        resolve_a_turn(&mut app);
        let (food_landed, hide_landed, hide_home) = row_of(&app);
        assert!(
            (hide_held(&app) - held_before - hide_landed).abs() < SAME_FOOD,
            "turn {turn}: the store gains exactly the hide the row reports landed"
        );
        assert_eq!(
            hide_landed > 0.0,
            food_landed > 0.0,
            "turn {turn}: hide lands when, and only when, a pack does"
        );
        if !landed && hide_landed == 0.0 {
            assert_eq!(
                hide_held(&app),
                0.0,
                "turn {turn}: no hide before the first pack"
            );
        }
        landed |= hide_landed > 0.0;
        if landed && food_landed == 0.0 && hide_home > 0.0 {
            let answer = ask_the_socket(&mut app);
            let quoted: f32 = answer
                .materials_rate_home
                .iter()
                .filter(|payoff| payoff.material_id == HIDE)
                .map(|payoff| payoff.amount)
                .sum();
            assert_eq!(
                quoted, hide_home,
                "the compose reply quotes the row's hide rate home"
            );
            between_packs = true;
            break;
        }
    }
    assert!(landed, "liveness: a pack of boar lands");
    assert!(
        between_packs,
        "a turn with no pack landing still prints the smoothed hide rate home"
    );
}

// ---------------------------------------------------------------------------------------------
// EVERY HUNT KEEPS THE WHOLE KILL — LOCALITY IS DECIDED PER KILL, OFF WHERE THE HERD STANDS
// ---------------------------------------------------------------------------------------------

/// A body heavier than the whole crew's pack, so every kill leaves part of the carcass the pack
/// cannot seat — the case a resident hunt used to waste.
const CARCASS_BIGGER_THAN_THE_PACKS: f32 = 1_000.0;
/// A standing stock deep enough that the escapement floor leaves many bodies to take.
const DEEP_STOCK: f32 = 40_000.0;
/// The share of a body the herd must lose in a turn for that turn to count as a kill (its own
/// regrowth is a small fraction of one).
const A_KILL_IS_AT_LEAST: f32 = 0.5;
/// A hex past the band's own range (`band_work_range` 2), on the camp's row.
const BEYOND_THE_APRON: u32 = 5;
/// Turns a fixture waits for a kill before the guard trips.
const TURNS_TO_SEE_A_KILL: usize = 80;
/// The band's output multiplier the fixture's takes are paid at.
const NEUTRAL_OUTPUT: f32 = 1.0;

fn the_fixture_herd(app: &App) -> Herd {
    app.world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.id == HERD_ID)
        .cloned()
        .expect("the fixture herd is seated")
}

/// Walk the fixture herd to `distance` hexes from the camp, mid-run.
fn move_the_herd_to(app: &mut App, distance: u32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|herd| herd.id == HERD_ID)
        .expect("the fixture herd is seated");
    let tile = UVec2::new(CAMP.x + distance, CAMP.y);
    herd.route = vec![tile];
    herd.step_index = 0;
    herd.current_pos = tile;
}

fn sled_wear(app: &App, band: Entity) -> f32 {
    app.world
        .get::<BandEquipment>(band)
        .expect("the band keeps its kit")
        .wear_of("sled")
}

fn lost_a_body(before: f32, app: &App) -> bool {
    before - the_fixture_herd(app).biomass > CARCASS_BIGGER_THAN_THE_PACKS * A_KILL_IS_AT_LEAST
}

/// Resolve turns until the herd has lost a body, handing back the food the row credited on that
/// turn, whether the row held a party, and the sled wear that turn cost.
fn until_a_kill(app: &mut App, band: Entity) -> (f32, bool, f32) {
    for _ in 0..TURNS_TO_SEE_A_KILL {
        let wear_before = sled_wear(app, band);
        let biomass_before = the_fixture_herd(app).biomass;
        resolve_a_turn(app);
        if lost_a_body(biomass_before, app) {
            let row = app
                .world
                .get::<LaborAllocation>(band)
                .expect("the band keeps its allocation");
            assert_eq!(
                row.last_yields[0].wasted, 0.0,
                "a hunt keeps its whole kill, so its row wastes nothing"
            );
            return (
                row.last_yields[0].actual,
                row.assignments[0].party.is_some(),
                sled_wear(app, band) - wear_before,
            );
        }
    }
    panic!("liveness: the crew must bring a body down within {TURNS_TO_SEE_A_KILL} turns");
}

/// ⛔ **EVERY HUNT KEEPS THE WHOLE KILL, AND WHETHER IT IS HAULED IS DECIDED PER KILL.** A herd
/// inside `band_work_range` is a camp kill: the whole carcass lands in the larder the turn it
/// falls and the sled is not worn. The same herd wandered past the range posts a party and its kill
/// is hauled (the sled wears). Wandering back inside makes the next kill a camp kill again. Each
/// kill is decided off where the herd stands that turn, never off where it stood before.
#[test]
fn a_herd_crossing_the_apron_changes_how_each_kill_is_taken_but_never_what_is_kept() {
    let (mut app, band) = world_hunting_at(INSIDE_THE_APRON);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == HERD_ID).unwrap();
        herd.body_mass = CARCASS_BIGGER_THAN_THE_PACKS;
        herd.carrying_capacity = DEEP_STOCK;
        herd.biomass = DEEP_STOCK;
    }
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let full_body = core_sim::herd_hunt_yield(&the_fixture_herd(&app), &fauna)
        .apply(CARCASS_BIGGER_THAN_THE_PACKS, NEUTRAL_OUTPUT)
        .provisions;
    assert!(full_body > 0.0, "liveness: a body is worth food");

    // Inside the range: a camp kill.
    let (camp_credit, camp_posted, camp_haul_wear) = until_a_kill(&mut app, band);
    assert!(!camp_posted, "inside the range the row posts no party");
    assert_eq!(
        camp_haul_wear, 0.0,
        "a camp kill hauls nothing: no sled wear"
    );
    assert!(
        camp_credit >= full_body * (1.0 - SAME_FOOD),
        "the camp kill keeps the whole body ({full_body}), not the part the packs seat: {camp_credit}"
    );

    // The herd wanders past the range: a posted kill, hauled.
    move_the_herd_to(&mut app, BEYOND_THE_APRON);
    let wear_before_the_far_hunt = sled_wear(&app, band);
    let mut far_posted = false;
    for _ in 0..TURNS_TO_SEE_A_KILL {
        let biomass_before = the_fixture_herd(&app).biomass;
        resolve_a_turn(&mut app);
        far_posted |= app
            .world
            .get::<LaborAllocation>(band)
            .is_some_and(|allocation| allocation.assignments[0].party.is_some());
        if lost_a_body(biomass_before, &app) {
            break;
        }
    }
    assert!(far_posted, "past the range the row posts a work party");
    assert!(
        sled_wear(&app, band) > wear_before_the_far_hunt,
        "a posted kill is hauled, so it wears the sled"
    );

    // Back inside: the next kill is a camp kill once more.
    move_the_herd_to(&mut app, INSIDE_THE_APRON);
    let (second_credit, second_posted, second_wear) = until_a_kill(&mut app, band);
    assert!(
        !second_posted,
        "wandered back inside, the row is local again"
    );
    assert_eq!(
        second_wear, 0.0,
        "the second camp kill hauls nothing either"
    );
    assert!(
        second_credit >= full_body * (1.0 - SAME_FOOD),
        "and keeps the whole body: {second_credit} against {full_body}"
    );
}
