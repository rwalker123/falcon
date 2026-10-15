//! **The exchange stages: haggling over a kept road (stage 4), a scout's pack (stage 1), and what a
//! standing link is worth** (issue #233, `docs/plan_early_diplomacy.md` §1 and §2).
//!
//! The command handlers (`propose_trade`, `answer_dealing … counter`, `send_expedition … pack`) are in
//! `core_sim/src/bin/server.rs`, where their launch half is tested end to end; what is pinned here is
//! what the world does with them. The library halves are called directly — [`propose_trade`] and
//! [`answer_dealing`], the functions the handlers wrap — and a party the handler would launch is
//! spawned by hand the way `dealings.rs` spawns shipments, so the claims run the shipped resolution
//! and arrival paths, not a mirror of them.
//!
//! **Whole turns, two peoples.** The sender is the campaign's first band (faction 0); the receiver is
//! a band split off it, made another people and walked a few hexes east. A "standing link" between
//! them is a dirt road laid on every tile the shipped trace crosses.
//!
//! **Wire claims are asserted on the ENCODED frame**, through `root_as_envelope`.

use std::sync::Arc;

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    answer_dealing, bands_have_standing_link, build_test_app, propose_trade,
    recapture_snapshot_in_place, run_turn, scalar_from_f32, scalar_zero, split_band_from_parent,
    trace_path, Agreement, BandId, BandTravel, CommandEventKind, CommandEventLog, ConnectionLedger,
    DealingAnswer, DealingId, DealingKind, DiplomacyConfig, Expedition, ExpeditionMission,
    ExpeditionPhase, FactionId, GoodsLines, Goodwill, KnownGoodwill, LaborAllocation, LadderConfig,
    LocalStore, PendingDealings, PopulationCohort, ResidentBand, RoadRegistry, RungKey, Scalar,
    ScoutPack, SettleConfig, ShipmentTerms, SimulationConfig, SimulationTick, SnapshotHistory,
    StartingUnit, StrangersOrder, Tile, TileRegistry, TransferCause, TransferDirection,
    ViewerFaction, VisibilityConfig, VisibilityConfigHandle, FOOD, NEAR_ENOUGH_TO_KEEP,
};
use sim_runtime::{TradeCargoItem, FOOD_CARGO_KEY};

/// A pinned earthlike world, so the terrain under every fixture is the same one every run.
const MAP_SEED: u64 = 119_304_647;
/// Working-age people the sender is stocked with, so a split leaves two real bands on any seed.
const PARENT_WORKERS: f32 = 20.0;
/// Workers the receiving band is founded with.
const SPLIT_WORKERS: u32 = 5;
/// Workers the shipment party carries. Two is enough pack for every manifest below.
const PARTY_WORKERS: u32 = 2;
/// How many hexes east of the sender the receiver is walked, so a trace between them crosses real
/// tiles a road can be laid on (or missing from).
const LINK_SPAN: u32 = 3;
/// How far out a scout's stranger stands — beyond what the home band's own sight reaches, so the
/// scout is the only one who finds it.
const STRANGER_DISTANCE: u32 = 14;
/// Where the scout is bound when it meets the stranger: further along the same row.
const SCOUT_DESTINATION_DISTANCE: u32 = 24;
/// The food a sender holds to give.
const SENDER_FOOD: f32 = 60.0;
/// The food the receiver holds to pay with.
const RECEIVER_FOOD: f32 = 40.0;
/// Hide the receiver holds.
const RECEIVER_HIDE: f32 = 6.0;
/// What a leg of the trade is. Different numbers everywhere, so an assertion that read the wrong leg
/// could not pass by coincidence.
const GIVE_FOOD: f32 = 8.0;
const ASK_HIDE: f32 = 2.0;
const COUNTER_GIVE_FOOD: f32 = 5.0;
const COUNTER_ASK_FOOD: f32 = 3.0;
const EPSILON: f32 = 0.01;
const RECEIVER_PEOPLE: FactionId = FactionId(9);
const SENDER_PEOPLE: FactionId = FactionId(0);
const THIRD_PEOPLE: FactionId = FactionId(5);
/// Turns to let a party walk a few hexes, settle and walk home.
const TURN_BUDGET: u32 = 40;

struct Scene {
    app: App,
    sender: Entity,
    sender_id: BandId,
    receiver: Entity,
    receiver_id: BandId,
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

fn position(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .current_tile;
    app.world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position
}

fn move_band_to(app: &mut App, band: Entity, to: UVec2) {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(to.x, to.y)
        .expect("a tile on the map");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.current_tile = tile;
    cohort.home = tile;
}

/// A tile `span` hexes east of `from`, on the same row.
fn east_of(app: &App, from: UVec2, span: u32) -> UVec2 {
    let registry = app.world.resource::<TileRegistry>();
    UVec2::new((from.x + span) % registry.width.max(1), from.y)
}

/// Two bands: the sender (faction 0, stocked with food) and a receiver split off it, made
/// `RECEIVER_PEOPLE`, stocked with food and hide, and walked `span` hexes east. They hold the full
/// tie a split leaves.
fn scene(span: u32) -> Scene {
    let mut app = spawn_world();
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    let sender = query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns at least one resident band");
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(sender)
            .expect("the band exists");
        cohort.working = Scalar::from_f32(PARENT_WORKERS);
        cohort.stores.add_food("dry", scalar_from_f32(SENDER_FOOD));
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, sender, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let mut by_id = app.world.query::<(Entity, &BandId)>();
    let receiver = by_id
        .iter(&app.world)
        .find(|(_, id)| **id == split.band)
        .map(|(entity, _)| entity)
        .expect("the split band exists");
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(receiver)
            .expect("the band exists");
        cohort.faction = RECEIVER_PEOPLE;
        cohort
            .stores
            .add_food("dry", scalar_from_f32(RECEIVER_FOOD));
        let mut readings = std::collections::BTreeMap::new();
        readings.insert("toughness".to_string(), 0.6);
        cohort.stores.deposit_material(
            "hide",
            core_sim::BandKey(vec![3, 3]),
            scalar_from_f32(RECEIVER_HIDE),
            &readings,
        );
    }
    let from = position(&app, sender);
    let to = east_of(&app, from, span);
    move_band_to(&mut app, receiver, to);
    let sender_id = *app.world.get::<BandId>(sender).expect("a band has an id");
    Scene {
        app,
        sender,
        sender_id,
        receiver,
        receiver_id: split.band,
    }
}

/// The dirt road rung's top, read from the ladder rather than restated.
fn dirt_road_top(ladder: &LadderConfig) -> f32 {
    let (base, width) =
        core_sim::road_rung_span(RungKey::RouteDirtRoad, ladder, NEAR_ENOUGH_TO_KEEP);
    base + width
}

/// **Lay a dirt road on every tile the shipped trace crosses between two camps** — the tile set a
/// standing link reads. Returns the tiles, in walking order.
fn lay_road_between(scene: &mut Scene) -> Vec<UVec2> {
    let a = position(&scene.app, scene.sender);
    let b = position(&scene.app, scene.receiver);
    let (width, height, wrap) = {
        let registry = scene.app.world.resource::<TileRegistry>();
        (
            registry.width,
            registry.height,
            scene
                .app
                .world
                .resource::<SimulationConfig>()
                .map_topology
                .wrap_horizontal,
        )
    };
    let path = trace_path(
        a,
        b,
        width,
        height,
        wrap,
        scene.app.world.resource::<RoadRegistry>(),
    );
    let ladder = LadderConfig::builtin();
    let top = dirt_road_top(&ladder);
    let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
    for tile in &path {
        roads
            .road_or_trail(*tile, &ladder)
            .set_position(top, &ladder);
    }
    path
}

/// Workers the sender puts on its `Roadwork` pool: far more than the few worker-turns a short dirt
/// road's bill asks, so the keeping is met with room to spare.
const ROADWORK_HANDS: u32 = 12;

/// **Hand every tile of the road to the sender band as its job and staff the pool that pays the
/// bill** — a built road is *kept* or it is nothing: the labour pass stamps every built road's bill
/// each turn, and an unkept one is in shortfall.
fn keep_the_road(scene: &mut Scene, path: &[UVec2]) {
    let ladder = LadderConfig::builtin();
    {
        let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
        for tile in path {
            roads.road_or_trail(*tile, &ladder).take_keeper(
                core_sim::RoadKeeper {
                    faction: SENDER_PEOPLE,
                    band: scene.sender_id,
                },
                NEAR_ENOUGH_TO_KEEP,
                &ladder,
            );
        }
    }
    let mut allocation = scene
        .app
        .world
        .get::<LaborAllocation>(scene.sender)
        .cloned()
        .unwrap_or_default();
    allocation.set_assignment(
        core_sim::LaborTarget::Roadwork,
        ROADWORK_HANDS,
        ROADWORK_HANDS,
        None,
    );
    scene.app.world.entity_mut(scene.sender).insert(allocation);
}

fn linked(scene: &mut Scene) -> bool {
    bands_have_standing_link(&mut scene.app.world, scene.sender_id, scene.receiver_id)
}

fn food_line(amount: f32) -> TradeCargoItem {
    TradeCargoItem {
        id: FOOD_CARGO_KEY.to_string(),
        is_material: false,
        amount,
    }
}

fn material_line(id: &str, amount: f32) -> TradeCargoItem {
    TradeCargoItem {
        id: id.to_string(),
        is_material: true,
        amount,
    }
}

fn band_food(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

fn band_hide(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .material_total("hide")
        .to_f32()
}

fn pending(app: &App) -> PendingDealings {
    app.world.resource::<PendingDealings>().clone()
}

fn only_dealing(app: &App) -> core_sim::Dealing {
    let book = pending(app);
    assert_eq!(book.len(), 1, "exactly one dealing is pending: {book:?}");
    let dealing = book.iter().next().cloned().expect("one dealing");
    dealing
}

fn dealing_events(app: &App, faction: FactionId, status: &str) -> Vec<String> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::Dealing && entry.faction == faction)
        .filter_map(|entry| entry.detail.clone())
        .filter(|detail| detail.contains(&format!("status={status}")))
        .collect()
}

fn propose(
    scene: &mut Scene,
    give: &[TradeCargoItem],
    ask: &[TradeCargoItem],
) -> Result<DealingId, String> {
    propose_trade(
        &mut scene.app.world,
        SENDER_PEOPLE,
        scene.sender_id,
        PARTY_WORKERS,
        scene.receiver_id,
        give,
        ask,
    )
}

fn goodwill(app: &App, holder: FactionId, about: FactionId) -> core_sim::GoodwillEntry {
    app.world.resource::<Goodwill>().get(holder, about)
}

/// **Spawn the party an accepted proposal launches**, the way `launch_agreed_party` builds one: the
/// give drawn out of the shipper's larder as the party's cargo, `Agreed` terms carrying the ask,
/// bound for the consignee.
fn launch_agreed(scene: &mut Scene, agreement: &Agreement, dealing: DealingId) -> Entity {
    let cargo = {
        let mut sender = scene
            .app
            .world
            .get_mut::<PopulationCohort>(scene.sender)
            .expect("the sender");
        let food = sender.stores.take_food_mix(agreement.give.food);
        let mut cargo = LocalStore::new();
        cargo.add_food_mix(&food);
        cargo
    };
    let destination_pos = position(&scene.app, scene.receiver);
    let mut cohort = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender")
        .clone();
    cohort.working = Scalar::from_u32(agreement.party_workers);
    cohort.children = scalar_zero();
    cohort.elders = scalar_zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let band_id = scene
        .app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    scene
        .app
        .world
        .spawn((
            cohort,
            band_id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: scene.sender,
                mission: ExpeditionMission::Trade {
                    destination_band: scene.receiver_id,
                    destination_faction: RECEIVER_PEOPLE,
                    destination_name: String::new(),
                    terms: ShipmentTerms::Agreed {
                        agreed_return: agreement.ask.clone(),
                        dealing,
                    },
                },
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo,
                defection_pull: core_sim::Scalar::zero(),
                station_reports: Vec::new(),
                goodwill_news: Default::default(),
                exchange_return: Default::default(),
                unpaid_return: Default::default(),
                scout_pack: Default::default(),
                aim: Some(destination_pos),
            },
            BandTravel::to(destination_pos),
        ))
        .id()
}

/// What a band's store booked this turn for `commodity` under `cause`, summed.
fn booked(app: &App, band: Entity, commodity: &str, cause: TransferCause) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .last_turn_transfer_crossings
        .iter()
        .filter(|crossing| crossing.commodity == commodity && crossing.cause == cause)
        .map(|crossing| crossing.amount)
        .sum()
}

/// Run turns until `done` holds (or the budget runs out); returns whether it held.
fn run_until(app: &mut App, mut done: impl FnMut(&App) -> bool) -> bool {
    for _ in 0..TURN_BUDGET {
        if done(app) {
            return true;
        }
        run_turn(app);
    }
    done(app)
}

/// One row of the viewer's published `DealingsSection`, read off the encoded envelope.
#[derive(Debug)]
struct PublishedDealing {
    id: u64,
    kind: String,
    sender_band: u64,
    receiver_band: u64,
    shipper_band: u64,
    party_workers: u32,
    sent_food: f32,
    asked_materials: Vec<(String, f32, usize)>,
}

fn dealings_seen_by(app: &mut App, viewer: FactionId) -> Vec<PublishedDealing> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    app.world.insert_resource(ViewerFaction(viewer));
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .dealings()
        .expect("the dealings section is published");
    section
        .dealings()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    let sent = row.goodsSent().expect("a row carries its sent goods");
                    let asked = row.goodsAsked().expect("a row carries its asked goods");
                    PublishedDealing {
                        id: row.id(),
                        kind: row.kind().unwrap_or("").to_string(),
                        sender_band: row.senderBand(),
                        receiver_band: row.receiverBand(),
                        shipper_band: row.shipperBand(),
                        party_workers: row.partyWorkers(),
                        sent_food: sent.food(),
                        asked_materials: asked
                            .materials()
                            .map(|rows| {
                                rows.iter()
                                    .map(|batch| {
                                        (
                                            batch.materialId().unwrap_or("").to_string(),
                                            batch.amount(),
                                            batch.readings().map_or(0, |readings| readings.len()),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

// =============================================================================================
// Stage 4 — haggling over a kept road
// =============================================================================================

/// **A proposal over a standing link is posted the turn it is made, names both legs, and draws
/// nothing.** It is on the ENCODED frame of both peoples — unrated lines, the shipper named — and on
/// nobody else's.
#[test]
fn a_proposal_over_a_kept_road_is_posted_at_once_and_draws_nothing() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    assert!(
        linked(&mut scene),
        "precondition: a kept road runs the whole way"
    );
    let larder = band_food(&scene.app, scene.sender);
    let id = propose(
        &mut scene,
        &[food_line(GIVE_FOOD)],
        &[material_line("hide", ASK_HIDE)],
    )
    .expect("a link, a tie and goods held");

    let dealing = only_dealing(&scene.app);
    assert_eq!(dealing.id, id);
    assert_eq!(dealing.kind, DealingKind::Proposal);
    assert_eq!(dealing.party, None, "no party walks yet");
    assert_eq!(
        (dealing.sender_faction, dealing.receiver_faction),
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        "addressed to the destination's people"
    );
    assert!(
        (band_food(&scene.app, scene.sender) - larder).abs() < EPSILON,
        "a proposal draws nothing"
    );

    let to_receiver = dealings_seen_by(&mut scene.app, RECEIVER_PEOPLE);
    assert_eq!(
        to_receiver.len(),
        1,
        "the receiver sees it: {to_receiver:?}"
    );
    let row = &to_receiver[0];
    assert_eq!(row.kind, "proposal");
    assert_eq!(row.id, id.0);
    assert_eq!(
        (row.sender_band, row.receiver_band, row.shipper_band),
        (scene.sender_id.0, scene.receiver_id.0, scene.sender_id.0)
    );
    assert_eq!(row.party_workers, PARTY_WORKERS);
    assert!(
        (row.sent_food - GIVE_FOOD).abs() < EPSILON,
        "the give is the sent leg"
    );
    assert_eq!(
        row.asked_materials,
        vec![("hide".to_string(), ASK_HIDE, 0)],
        "the ask is a material line with no rating: nothing has been drawn to rate"
    );
    assert_eq!(dealings_seen_by(&mut scene.app, SENDER_PEOPLE).len(), 1);
    assert!(
        dealings_seen_by(&mut scene.app, THIRD_PEOPLE).is_empty(),
        "another pair's dealings are theirs to know"
    );
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "offered").len(),
        1
    );
    assert_eq!(dealing_events(&scene.app, SENDER_PEOPLE, "posted").len(), 1);
}

/// **No standing link, no proposal** — and nothing is recorded. A road on only some of the tiles is
/// no link either.
#[test]
fn a_proposal_with_no_standing_link_is_refused_and_records_nothing() {
    let mut scene = scene(LINK_SPAN);
    let err = propose(&mut scene, &[food_line(GIVE_FOOD)], &[])
        .expect_err("no road runs between the camps");
    assert!(err.contains("standing link"), "{err}");
    assert!(pending(&scene.app).is_empty());

    // A road with a hole in the middle is a road that does not run the whole way.
    let path = lay_road_between(&mut scene);
    assert!(path.len() >= 3, "precondition: a middle tile to knock out");
    let middle = path[path.len() / 2];
    scene
        .app
        .world
        .resource_mut::<RoadRegistry>()
        .remove(middle);
    assert!(!linked(&mut scene), "one bare tile breaks the run");
    assert!(propose(&mut scene, &[food_line(GIVE_FOOD)], &[]).is_err());
    assert!(pending(&scene.app).is_empty());
}

/// **A road tile in keeping shortfall breaks the link**, and so does a tile below the floor rung —
/// the link is the weakest tile's, on `path_reach_tiles`' own tile set.
#[test]
fn a_tile_whose_keeping_is_short_or_whose_rung_is_too_low_breaks_the_link() {
    let mut scene = scene(LINK_SPAN);
    let path = lay_road_between(&mut scene);
    assert!(linked(&mut scene));
    let middle = path[path.len() / 2];

    // Keeping: the stamped bill is unpaid.
    {
        let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
        let road = roads.road_mut(middle).expect("the road");
        road.upkeep_demanded = Some(2.0);
        road.upkeep_supplied = 0.0;
    }
    assert!(
        !linked(&mut scene),
        "a road in keeping shortfall holds no link"
    );
    // …and the same road, bill paid, holds it again (the liveness half).
    {
        let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
        roads.road_mut(middle).expect("the road").upkeep_supplied = 2.0;
    }
    assert!(linked(&mut scene), "a paid bill restores the link");

    // Rung: a worn trail is below the floor (a dirt road).
    {
        let ladder = LadderConfig::builtin();
        let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
        roads
            .road_mut(middle)
            .expect("the road")
            .set_position(core_sim::traffic_ceiling(&ladder), &ladder);
    }
    assert!(!linked(&mut scene), "a trail is not a built road");
}

/// **Haggling is a chain of answers, and the party carries the last terms.** A proposal is countered
/// by the receiver (the original closes and a counter goes back, legs stated from the shipper's side),
/// countered again by the sender, and accepted — the accept returns the agreement of the LAST counter,
/// with the party size the proposal fixed.
#[test]
fn a_counter_closes_the_original_and_goes_back_and_accept_means_the_last_terms() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let first = propose(
        &mut scene,
        &[food_line(GIVE_FOOD)],
        &[material_line("hide", ASK_HIDE)],
    )
    .expect("a proposal");

    // --- the receiver counters: it will take less and pay in food ---------------------------
    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        first,
        &DealingAnswer::Counter {
            give: vec![food_line(COUNTER_GIVE_FOOD)],
            ask: vec![food_line(COUNTER_ASK_FOOD)],
        },
    )
    .expect("the receiver holds what it would pay");
    let second = report.counter.expect("a counter was posted");
    assert!(!report.accepted && report.launch.is_none());
    assert!(
        pending(&scene.app).get(first).is_none(),
        "the original closed"
    );
    let counter = only_dealing(&scene.app);
    assert_eq!(counter.id, second);
    assert_eq!(counter.kind, DealingKind::Counter);
    assert_eq!(
        (counter.sender_faction, counter.receiver_faction),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
        "the counter goes the other way"
    );
    let terms = counter.agreement.clone().expect("a counter names its legs");
    assert_eq!(
        (terms.shipper_band, terms.consignee_band),
        (scene.sender_id, scene.receiver_id),
        "the shipper is still the original sender: accept always means the same"
    );
    assert_eq!(
        terms.party_workers, PARTY_WORKERS,
        "the party size carries over"
    );
    assert!((terms.give.food.to_f32() - COUNTER_GIVE_FOOD).abs() < EPSILON);
    assert!((terms.ask.food.to_f32() - COUNTER_ASK_FOOD).abs() < EPSILON);
    assert_eq!(
        dealing_events(&scene.app, SENDER_PEOPLE, "countered").len(),
        1
    );
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "countered").len(),
        1
    );

    // The encoded frame carries it as a counter with the shipper named.
    let seen = dealings_seen_by(&mut scene.app, SENDER_PEOPLE);
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].kind, "counter");
    assert_eq!(seen[0].shipper_band, scene.sender_id.0);
    assert_eq!(seen[0].sender_band, scene.receiver_id.0);

    // --- the sender counters back: unlimited counters, each a proposal the other way ---------
    let report = answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        second,
        &DealingAnswer::Counter {
            give: vec![food_line(GIVE_FOOD)],
            ask: vec![food_line(COUNTER_ASK_FOOD)],
        },
    )
    .expect("the sender holds what it would send");
    let third = report.counter.expect("a second counter");
    let last = only_dealing(&scene.app);
    assert_eq!(last.id, third);
    assert_eq!(
        (last.sender_faction, last.receiver_faction),
        (SENDER_PEOPLE, RECEIVER_PEOPLE)
    );

    // --- the receiver accepts the last terms -------------------------------------------------
    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        third,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("the last terms stand");
    assert!(report.accepted);
    let launch = report
        .launch
        .expect("an accepted counter launches the shipper's party");
    assert_eq!(launch.shipper_band, scene.sender_id);
    assert_eq!(launch.party_workers, PARTY_WORKERS);
    assert!((launch.give.food.to_f32() - GIVE_FOOD).abs() < EPSILON);
    assert!((launch.ask.food.to_f32() - COUNTER_ASK_FOOD).abs() < EPSILON);
    assert!(
        pending(&scene.app).is_empty(),
        "accepted: nothing left to answer"
    );
}

/// **An accepted proposal's party settles at once on arrival — goods both ways, goodwill both ways.**
/// The cargo lands in the receiver, the agreed return is drawn out of it, the return comes home with
/// the party as `ExchangeIn`, and each people thinks better of the other by what it received.
#[test]
fn an_agreed_party_settles_on_arrival_and_the_return_comes_home() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let id = propose(
        &mut scene,
        &[food_line(GIVE_FOOD)],
        &[material_line("hide", ASK_HIDE)],
    )
    .expect("a proposal");
    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("the receiver holds the hide");
    let agreement = report.launch.expect("a launch");

    let sender_hide = band_hide(&scene.app, scene.sender);
    let party = launch_agreed(&mut scene, &agreement, id);

    // Runs until the party has settled and turned for home, reading the receiver's books each turn
    // (its larder eats between turns, so the books are the exact record).
    let (mut landed, mut paid) = (0.0, 0.0);
    assert!(run_until(&mut scene.app, |app| {
        landed += booked(app, scene.receiver, FOOD, TransferCause::ShipmentIn);
        paid += booked(app, scene.receiver, "hide", TransferCause::ExchangeOut);
        app.world
            .get::<Expedition>(party)
            .is_none_or(|party| party.phase == ExpeditionPhase::Returning)
    }));
    assert!(
        (landed - GIVE_FOOD).abs() < EPSILON,
        "the cargo landed in the receiving band as a shipment: {landed}"
    );
    assert!(
        (paid - ASK_HIDE).abs() < EPSILON,
        "the agreed return left it as ExchangeOut: {paid}"
    );
    assert_eq!(
        dealing_events(&scene.app, SENDER_PEOPLE, "settled").len(),
        1
    );
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "settled").len(),
        1
    );

    // Goodwill, both ways, at settlement.
    assert!(
        goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).exchanges > 0.0,
        "the receiver thinks better of the sender for what it was sent"
    );
    assert!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE).exchanges > 0.0,
        "the sender thinks better of the receiver for what it was paid"
    );

    // The return rides the same party home, booked as the exchange's return.
    let mut came_home = 0.0;
    assert!(run_until(&mut scene.app, |app| {
        came_home += app
            .world
            .get::<PopulationCohort>(scene.sender)
            .expect("the sender")
            .last_turn_transfer_crossings
            .iter()
            .filter(|crossing| {
                crossing.commodity == "hide"
                    && crossing.cause == TransferCause::ExchangeIn
                    && crossing.direction == TransferDirection::In
            })
            .map(|crossing| crossing.amount)
            .sum::<f32>();
        app.world.get::<Expedition>(party).is_none()
    }));
    assert!(
        (came_home - ASK_HIDE).abs() < EPSILON,
        "the hide came home as ExchangeIn: {came_home}"
    );
    assert!(band_hide(&scene.app, scene.sender) >= sender_hide + ASK_HIDE - EPSILON);
}

/// **A receiver that no longer holds the agreed return fails the exchange**: nothing lands, nothing
/// is drawn, no goodwill moves, and the party walks home with its cargo.
#[test]
fn a_receiver_short_of_the_return_fails_the_exchange_and_the_cargo_goes_home() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let id = propose(
        &mut scene,
        &[food_line(GIVE_FOOD)],
        &[material_line("hide", ASK_HIDE)],
    )
    .expect("a proposal");
    let agreement = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("held at the time")
    .launch
    .expect("a launch");
    let party = launch_agreed(&mut scene, &agreement, id);

    // While the party walks, the receiver spends its hide.
    scene
        .app
        .world
        .get_mut::<PopulationCohort>(scene.receiver)
        .expect("the receiver")
        .stores
        .take_material_batches("hide", scalar_from_f32(RECEIVER_HIDE));
    let mut landed = 0.0;
    assert!(run_until(&mut scene.app, |app| {
        landed += booked(app, scene.receiver, FOOD, TransferCause::ShipmentIn);
        app.world
            .get::<Expedition>(party)
            .is_none_or(|party| party.phase == ExpeditionPhase::Returning)
    }));
    assert_eq!(landed, 0.0, "nothing landed");
    assert_eq!(dealing_events(&scene.app, SENDER_PEOPLE, "failed").len(), 1);
    assert!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "failed")[0].contains("reason=receiver_short")
    );
    assert!(
        goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).exchanges == 0.0
            && goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE).exchanges == 0.0
            && goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).gifts == 0.0,
        "a failed exchange moves no goodwill"
    );
    // The cargo is still the party's and comes home as a shipment that never landed.
    let mut returned = 0.0;
    assert!(run_until(&mut scene.app, |app| {
        returned += booked(app, scene.sender, FOOD, TransferCause::ShipmentReturned);
        app.world.get::<Expedition>(party).is_none()
    }));
    assert!(
        (returned - GIVE_FOOD).abs() < EPSILON,
        "the undelivered cargo folded back into the sender's larder: {returned}"
    );
}

/// **A link broken between the proposal and the answer withdraws the dealing** — with a reason, told
/// to both peoples — and the answer fails.
#[test]
fn a_link_broken_before_the_answer_withdraws_the_proposal() {
    let mut scene = scene(LINK_SPAN);
    let path = lay_road_between(&mut scene);
    let id = propose(&mut scene, &[food_line(GIVE_FOOD)], &[]).expect("a proposal");
    scene
        .app
        .world
        .resource_mut::<RoadRegistry>()
        .remove(path[path.len() / 2]);

    let err = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect_err("the road no longer runs the whole way");
    assert!(err.contains("no longer holds"), "{err}");
    assert!(pending(&scene.app).is_empty(), "the dealing was withdrawn");
    for people in [SENDER_PEOPLE, RECEIVER_PEOPLE] {
        let told = dealing_events(&scene.app, people, "withdrawn");
        assert_eq!(told.len(), 1, "{people:?} is told");
        assert!(told[0].contains("reason=link_broken"), "{}", told[0]);
    }
    // A refusal needs the road too: every answer on a proposal does.
    let mut scene = self::scene(LINK_SPAN);
    let path = lay_road_between(&mut scene);
    let id = propose(&mut scene, &[food_line(GIVE_FOOD)], &[]).expect("a proposal");
    scene
        .app
        .world
        .resource_mut::<RoadRegistry>()
        .remove(path[0]);
    assert!(answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Refuse
    )
    .is_err());
    assert!(pending(&scene.app).is_empty());
}

/// **The answer's own refusals leave the book as it stood**: a proposal's terms are written, so a
/// return list on an accept is refused; an offer cannot be countered; a receiver that does not hold
/// the ask cannot accept; and only the addressed people may answer.
#[test]
fn an_answer_that_cannot_be_honoured_is_refused_and_the_book_is_untouched() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let id = propose(
        &mut scene,
        &[food_line(GIVE_FOOD)],
        &[material_line("hide", ASK_HIDE)],
    )
    .expect("a proposal");
    let before = pending(&scene.app);

    let with_return = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: vec![food_line(1.0)],
        },
    )
    .expect_err("the terms are already written");
    assert!(with_return.contains("already written"), "{with_return}");

    let wrong_people = answer_dealing(
        &mut scene.app.world,
        THIRD_PEOPLE,
        id,
        &DealingAnswer::Refuse,
    )
    .expect_err("not addressed to them");
    assert!(
        wrong_people.contains("waiting on your people"),
        "{wrong_people}"
    );

    // The receiver spends its hide: it cannot accept an ask it does not hold.
    scene
        .app
        .world
        .get_mut::<PopulationCohort>(scene.receiver)
        .expect("the receiver")
        .stores
        .take_material_batches("hide", scalar_from_f32(RECEIVER_HIDE));
    let short = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect_err("the receiver cannot pay the ask");
    assert!(short.contains("hide"), "{short}");
    assert_eq!(
        pending(&scene.app),
        before,
        "every refusal left the book as it stood"
    );
}

/// **Silence refuses a proposal** after `offer_wait_turns`, told to both peoples — and a counter
/// lapses the same way.
#[test]
fn silence_expires_a_proposal() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    propose(&mut scene, &[food_line(GIVE_FOOD)], &[]).expect("a proposal");
    let wait = DiplomacyConfig::builtin().offers.offer_wait_turns;
    for _ in 0..=wait {
        run_turn(&mut scene.app);
    }
    assert!(pending(&scene.app).is_empty(), "silence refused it");
    for people in [SENDER_PEOPLE, RECEIVER_PEOPLE] {
        assert_eq!(dealing_events(&scene.app, people, "expired").len(), 1);
    }
}

/// **A pending proposal rides the checkpoint**, legs and all, and is answerable after a restore.
#[test]
fn a_pending_proposal_round_trips_a_checkpoint() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let id = propose(
        &mut scene,
        &[food_line(GIVE_FOOD)],
        &[material_line("hide", ASK_HIDE)],
    )
    .expect("a proposal");
    let before = pending(&scene.app);
    let checkpoint = capture_sim_state(&scene.app.world);
    scene.app.world.resource_mut::<PendingDealings>().remove(id);
    assert!(pending(&scene.app).is_empty());
    restore_sim_state(&mut scene.app.world, &checkpoint);
    assert_eq!(
        pending(&scene.app),
        before,
        "the book and its legs came back whole"
    );
}

// =============================================================================================
// Stage 1 — a scout's pack
// =============================================================================================

/// A scene for the scouts: the receiver is a stranger — another people, no tie with the sender — a
/// good way east, and the sender's own sight cannot reach it.
fn stranger_scene() -> Scene {
    let mut scene = scene(STRANGER_DISTANCE);
    *scene.app.world.resource_mut::<ConnectionLedger>() = ConnectionLedger::default();
    // **Sight pinned to plain geometry**, so what a scout sees is the config's radius and not the
    // terrain's: no line of sight to block it and no forest to shorten it.
    let mut vis = VisibilityConfig::default();
    vis.line_of_sight.enabled = false;
    vis.terrain_detection.tiles_per_unit = 0.0;
    scene
        .app
        .world
        .insert_resource(VisibilityConfigHandle::new(Arc::new(vis)));
    scene
}

/// Spawn a scouting party from the sender, bound for `destination` tiles east, with a food pack and
/// a standing order.
fn launch_scout(scene: &mut Scene, order: StrangersOrder, pack: f32) -> Entity {
    let from = position(&scene.app, scene.sender);
    let destination = east_of(&scene.app, from, SCOUT_DESTINATION_DISTANCE);
    let mut cohort = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender")
        .clone();
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
    cohort.children = scalar_zero();
    cohort.elders = scalar_zero();
    cohort.stores = LocalStore::new();
    cohort.stores.add_food("dry", scalar_from_f32(SENDER_FOOD));
    cohort.sync_size();
    let mut cargo = LocalStore::new();
    cargo.add_food("dry", scalar_from_f32(pack));
    let band_id = scene
        .app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    scene
        .app
        .world
        .spawn((
            cohort,
            band_id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: scene.sender,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Expedition),
                cargo,
                defection_pull: core_sim::Scalar::zero(),
                station_reports: Vec::new(),
                goodwill_news: Default::default(),
                exchange_return: Default::default(),
                unpaid_return: Default::default(),
                scout_pack: ScoutPack {
                    order,
                    ..Default::default()
                },
                aim: None,
            },
            BandTravel::to(destination),
        ))
        .id()
}

fn scout_pack(app: &App, scout: Entity) -> ScoutPack {
    app.world
        .get::<Expedition>(scout)
        .expect("the scout")
        .scout_pack
        .clone()
}

fn scout_phase(app: &App, scout: Entity) -> Option<ExpeditionPhase> {
    app.world.get::<Expedition>(scout).map(|party| party.phase)
}

/// **A scout holding a `give` order turns toward the first stranger it sights, hands over at the
/// shipment arrival rule, and waits for the answer** — a pending offer on the stranger's people,
/// the scout standing in `AwaitingOrders`, the order spent.
#[test]
fn a_give_scout_meets_a_stranger_and_posts_a_pending_offer() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Give, GIVE_FOOD);
    let posted = run_until(&mut scene.app, |app| !pending(app).is_empty());
    assert!(posted, "the scout reached the stranger and posted its pack");
    let dealing = only_dealing(&scene.app);
    assert_eq!(dealing.kind, DealingKind::Offer);
    assert_eq!(
        (dealing.sender_faction, dealing.receiver_faction),
        (SENDER_PEOPLE, RECEIVER_PEOPLE)
    );
    assert_eq!(dealing.receiver_band, scene.receiver_id);
    assert!(
        (dealing.goods_sent.get(FOOD).to_f32() - GIVE_FOOD).abs() < EPSILON,
        "the offer names the pack"
    );
    assert_eq!(
        scout_phase(&scene.app, scout),
        Some(ExpeditionPhase::AwaitingOrders),
        "the scout waits in the stranger's camp"
    );
    let pack = scout_pack(&scene.app, scout);
    assert!(pack.spent, "one pack, one handover: the order is spent");
    assert_eq!(pack.target, None);
    assert!(
        pack.resume.is_some(),
        "the scouting it interrupted is recorded"
    );
}

/// **An accepted handover raises the receiver's opinion of the giver, and the scout resumes its
/// scouting** — to the destination it was bound for — then comes home and folds back.
#[test]
fn an_accepted_gift_is_a_gift_and_the_scout_goes_back_to_its_scouting() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Give, GIVE_FOOD);
    assert!(run_until(&mut scene.app, |app| !pending(app).is_empty()));
    let id = only_dealing(&scene.app).id;
    let receiver_food = band_food(&scene.app, scene.receiver);

    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("an empty return needs nothing");
    assert!(report.accepted);
    assert!((band_food(&scene.app, scene.receiver) - (receiver_food + GIVE_FOOD)).abs() < EPSILON);
    assert!(
        goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).gifts > 0.0,
        "the receiver thinks better of the giver"
    );
    assert_eq!(
        scout_phase(&scene.app, scout),
        Some(ExpeditionPhase::Outbound),
        "the scout picks its scouting back up"
    );

    // It walks on to the destination it recorded…
    let destination = east_of(
        &scene.app,
        position(&scene.app, scene.sender),
        SCOUT_DESTINATION_DISTANCE,
    );
    run_turn(&mut scene.app);
    assert_eq!(
        scene
            .app
            .world
            .get::<BandTravel>(scout)
            .map(|travel| travel.target),
        Some(destination),
        "the recorded scouting destination is walked to"
    );
    // …and the pack is spent: a second stranger would get nothing.
    assert!(scout_pack(&scene.app, scout).spent);

    // Recalled, it folds home.
    scene
        .app
        .world
        .get_mut::<Expedition>(scout)
        .expect("the scout")
        .phase = ExpeditionPhase::Returning;
    assert!(run_until(&mut scene.app, |app| app
        .world
        .get::<Expedition>(scout)
        .is_none()));
}

/// **An `offer` order comes home with the named return**, booked as the exchange's return and not as
/// a shipment that failed to arrive.
#[test]
fn an_offer_scout_comes_home_with_the_receivers_return() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Offer, GIVE_FOOD);
    assert!(run_until(&mut scene.app, |app| !pending(app).is_empty()));
    let id = only_dealing(&scene.app).id;
    let sender_hide = band_hide(&scene.app, scene.sender);

    answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Accept {
            returned: vec![material_line("hide", ASK_HIDE)],
        },
    )
    .expect("the receiver holds the hide");
    assert!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE).exchanges > 0.0,
        "an exchange settled: the giver thinks better of the receiver too"
    );
    {
        let carried = &scene
            .app
            .world
            .get::<Expedition>(scout)
            .expect("the scout")
            .cargo;
        assert!((carried.material_total("hide").to_f32() - ASK_HIDE).abs() < EPSILON);
    }

    scene
        .app
        .world
        .get_mut::<Expedition>(scout)
        .expect("the scout")
        .phase = ExpeditionPhase::Returning;
    let mut came_home = 0.0;
    assert!(run_until(&mut scene.app, |app| {
        came_home += app
            .world
            .get::<PopulationCohort>(scene.sender)
            .expect("the sender")
            .last_turn_transfer_crossings
            .iter()
            .filter(|crossing| {
                crossing.commodity == "hide" && crossing.cause == TransferCause::ExchangeIn
            })
            .map(|crossing| crossing.amount)
            .sum::<f32>();
        app.world.get::<Expedition>(scout).is_none()
    }));
    assert!((came_home - ASK_HIDE).abs() < EPSILON, "{came_home}");
    assert!(band_hide(&scene.app, scene.sender) >= sender_hide + ASK_HIDE - EPSILON);
}

/// **A refused handover brings the pack home** and the scout carries on.
#[test]
fn a_refused_handover_sends_the_pack_home_with_the_scout() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Offer, GIVE_FOOD);
    assert!(run_until(&mut scene.app, |app| !pending(app).is_empty()));
    let id = only_dealing(&scene.app).id;
    answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Refuse,
    )
    .expect("a refusal needs nothing");
    assert_eq!(
        scout_phase(&scene.app, scout),
        Some(ExpeditionPhase::Outbound)
    );
    assert!(
        (scene
            .app
            .world
            .get::<Expedition>(scout)
            .expect("the scout")
            .cargo
            .get(FOOD)
            .to_f32()
            - GIVE_FOOD)
            .abs()
            < EPSILON,
        "the pack is still the scout's"
    );
    assert!(
        scout_pack(&scene.app, scout).spent,
        "refused or not, the order is spent"
    );
}

/// **Silence is a refusal for a scout too**: the offer lapses and the scout resumes.
#[test]
fn a_lapsed_handover_leaves_the_scout_free_to_resume() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Give, GIVE_FOOD);
    assert!(run_until(&mut scene.app, |app| !pending(app).is_empty()));
    let wait = DiplomacyConfig::builtin().offers.offer_wait_turns;
    for _ in 0..=wait {
        run_turn(&mut scene.app);
    }
    assert!(pending(&scene.app).is_empty());
    assert_ne!(
        scout_phase(&scene.app, scout),
        Some(ExpeditionPhase::AwaitingOrders),
        "the scout is no longer waiting"
    );
}

/// **A band the home band already holds a live tie with is not a stranger.** The scout passes it by,
/// its order unspent and nobody chosen.
#[test]
fn a_band_the_home_band_holds_a_tie_with_is_not_a_stranger() {
    // The split's own full tie is left in place.
    let mut scene = scene(STRANGER_DISTANCE);
    assert!(
        scene
            .app
            .world
            .resource::<ConnectionLedger>()
            .tie_is_live(scene.sender_id, scene.receiver_id),
        "precondition: a split leaves a full tie"
    );
    let scout = launch_scout(&mut scene, StrangersOrder::Give, GIVE_FOOD);
    for _ in 0..12 {
        run_turn(&mut scene.app);
    }
    assert!(pending(&scene.app).is_empty(), "nothing was handed over");
    let pack = scout_pack(&scene.app, scout);
    assert!(!pack.spent && pack.target.is_none(), "{pack:?}");
}

/// **A `keep` order never hands over**, however many strangers the scout passes.
#[test]
fn a_keep_order_never_hands_anything_over() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Keep, GIVE_FOOD);
    for _ in 0..12 {
        run_turn(&mut scene.app);
    }
    assert!(pending(&scene.app).is_empty());
    assert!(!scout_pack(&scene.app, scout).spent);
}

/// **A stranger lost before arrival leaves the order unspent and the scouting resumed.**
#[test]
fn a_stranger_lost_before_arrival_leaves_the_order_unspent() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Give, GIVE_FOOD);
    // Run until the scout has picked its stranger…
    assert!(run_until(&mut scene.app, |app| scout_pack(app, scout)
        .target
        .is_some()));
    assert!(pending(&scene.app).is_empty(), "it has not arrived yet");
    // …then the stranger vanishes (it walks out of the world).
    scene.app.world.despawn(scene.receiver);
    run_turn(&mut scene.app);
    let pack = scout_pack(&scene.app, scout);
    assert_eq!(pack.target, None, "the chase is forgotten");
    assert!(
        !pack.spent,
        "the order is NOT spent: the pack was never handed over"
    );
    assert!(pending(&scene.app).is_empty());
    assert!(
        scene
            .app
            .world
            .get::<Expedition>(scout)
            .expect("the scout")
            .cargo
            .get(FOOD)
            .to_f32()
            > GIVE_FOOD - EPSILON,
        "the pack is still aboard"
    );
}

/// **A scout mid-handover and an agreed party in flight ride the checkpoint whole**: the strangers
/// order, the chosen stranger, the way back to the scouting, the spent flag and the agreed return all
/// survive a capture and a restore.
#[test]
fn a_scouts_pack_state_and_an_agreed_trade_round_trip_a_checkpoint() {
    let mut scene = stranger_scene();
    let scout = launch_scout(&mut scene, StrangersOrder::Offer, GIVE_FOOD);
    let want_pack = ScoutPack {
        order: StrangersOrder::Offer,
        target: Some(scene.receiver_id),
        resume: Some(UVec2::new(7, 9)),
        spent: true,
        handover: Some(core_sim::TransferCounterparty {
            band: scene.receiver_id,
            faction: RECEIVER_PEOPLE,
        }),
        returned: true,
    };
    scene
        .app
        .world
        .get_mut::<Expedition>(scout)
        .expect("the scout")
        .scout_pack = want_pack.clone();
    let agreed_return = GoodsLines {
        food: scalar_from_f32(COUNTER_ASK_FOOD),
        fodder: scalar_zero(),
        materials: vec![("hide".to_string(), scalar_from_f32(ASK_HIDE))],
    };
    let (sender_id, receiver_id) = (scene.sender_id, scene.receiver_id);
    let party = launch_agreed(
        &mut scene,
        &Agreement {
            shipper_faction: SENDER_PEOPLE,
            shipper_band: sender_id,
            consignee_faction: RECEIVER_PEOPLE,
            consignee_band: receiver_id,
            party_workers: PARTY_WORKERS,
            give: GoodsLines {
                food: scalar_from_f32(GIVE_FOOD),
                ..Default::default()
            },
            ask: agreed_return.clone(),
        },
        DealingId(7),
    );

    let checkpoint = capture_sim_state(&scene.app.world);
    let party_id = *scene.app.world.get::<BandId>(party).expect("an id");
    let scout_id = *scene.app.world.get::<BandId>(scout).expect("an id");
    scene.app.world.despawn(party);
    scene.app.world.despawn(scout);
    restore_sim_state(&mut scene.app.world, &checkpoint);

    let mut parties = scene.app.world.query::<(&BandId, &Expedition)>();
    let restored: std::collections::HashMap<BandId, &Expedition> = parties
        .iter(&scene.app.world)
        .map(|(id, expedition)| (*id, expedition))
        .collect();
    assert_eq!(restored[&scout_id].scout_pack, want_pack);
    match &restored[&party_id].mission {
        ExpeditionMission::Trade {
            terms:
                ShipmentTerms::Agreed {
                    agreed_return: restored_return,
                    dealing,
                },
            ..
        } => {
            assert_eq!(*restored_return, agreed_return);
            assert_eq!(*dealing, DealingId(7));
        }
        other => panic!("an agreed trade, got {other:?}"),
    }
}

// =============================================================================================
// What a standing link is worth
// =============================================================================================

/// **A standing link raises each people's opinion of the other — once per people pair per turn,
/// however many band pairs the road joins — and refreshes what each knows of the other's opinion,
/// with no party carrying it.**
#[test]
fn a_standing_link_credits_each_people_once_a_turn_and_refreshes_what_is_known() {
    let mut scene = scene(LINK_SPAN);
    // A second band of the receiving people, also on the road, joined to the sender by it too.
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let second = split_band_from_parent(&mut scene.app.world, scene.sender, SPLIT_WORKERS, &settle)
        .expect("a second split");
    let mut by_id = scene.app.world.query::<(Entity, &BandId)>();
    let second_entity = by_id
        .iter(&scene.app.world)
        .find(|(_, id)| **id == second.band)
        .map(|(entity, _)| entity)
        .expect("the second band");
    scene
        .app
        .world
        .get_mut::<PopulationCohort>(second_entity)
        .expect("the band")
        .faction = RECEIVER_PEOPLE;
    let mid = east_of(&scene.app, position(&scene.app, scene.sender), 2);
    move_band_to(&mut scene.app, second_entity, mid);
    let path = lay_road_between(&mut scene);
    keep_the_road(&mut scene, &path);

    let per_link = DiplomacyConfig::builtin().goodwill.per_link;
    run_turn(&mut scene.app);
    assert!(
        linked(&mut scene),
        "precondition: the kept road still runs the whole way"
    );
    let tick = scene.app.world.resource::<SimulationTick>().0;

    for (holder, about) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        let entry = goodwill(&scene.app, holder, about);
        assert!(
            (entry.link - per_link).abs() < 1e-6,
            "{holder:?} -> {about:?}: one credit per people pair per turn, not one per band pair \
             (got {} against {per_link})",
            entry.link
        );
    }
    // What each knows of the other: the standing value, this turn, no party anywhere.
    let known = scene.app.world.resource::<KnownGoodwill>();
    for (viewer, other) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        let news = known
            .get(viewer, other)
            .expect("word travels along the kept road");
        assert!(
            news.turn + 1 >= tick,
            "refreshed this turn, not carried: {news:?}"
        );
        assert!((news.components.link - per_link).abs() < 1e-6);
    }
    assert_eq!(
        scene
            .app
            .world
            .query::<&Expedition>()
            .iter(&scene.app.world)
            .count(),
        0,
        "no party carried it"
    );
}

/// **No road, no link goodwill** — the control: the same two peoples, a tie, no road.
#[test]
fn no_road_no_link_goodwill() {
    let mut scene = scene(LINK_SPAN);
    run_turn(&mut scene.app);
    assert_eq!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE).link,
        0.0
    );
    assert_eq!(
        goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).link,
        0.0
    );
    assert!(scene
        .app
        .world
        .resource::<KnownGoodwill>()
        .get(SENDER_PEOPLE, RECEIVER_PEOPLE)
        .is_none());
}

/// **A built road nobody keeps is no link**: the labour pass stamps its bill every turn and nobody
/// pays it, so after one real turn the road holds nothing — and credits nothing.
#[test]
fn an_unkept_road_holds_no_link_once_a_turn_has_judged_it() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    assert!(
        linked(&mut scene),
        "before any turn has judged the bill, nothing is owed"
    );
    run_turn(&mut scene.app);
    assert!(
        !linked(&mut scene),
        "the bill was stamped and nobody paid it"
    );
    assert_eq!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE).link,
        0.0
    );
}

/// The scalar type is used above only through `Scalar::from_*`; this keeps the import honest.
#[test]
fn goods_lines_in_a_proposal_are_summed_per_good() {
    let scene = scene(LINK_SPAN);
    let lines = GoodsLines::resolve(
        &scene.app.world,
        &[food_line(2.0), food_line(3.0), material_line("hide", 1.0)],
        "test",
    )
    .expect("known goods");
    assert!((lines.food.to_f32() - 5.0).abs() < EPSILON);
    assert_eq!(lines.materials.len(), 1);
    assert!(GoodsLines::resolve(&scene.app.world, &[food_line(-1.0)], "test").is_err());
    assert!(GoodsLines::resolve(
        &scene.app.world,
        &[material_line("unobtainium", 1.0)],
        "test"
    )
    .is_err());
    let _ = scalar_zero();
}

/// **What the link pass costs** — an `--ignored` measurement, not a claim. Thirty bands of four
/// peoples stand along a kept road; the pass walks every cross-people pair (the worst case: every band
/// qualifies), and a whole turn of the same world is timed beside it.
/// Run with `cargo test -p core_sim --test stages -- --ignored --nocapture link_pass_cost`.
#[test]
#[ignore = "a measurement, run on demand"]
fn link_pass_cost() {
    use bevy::ecs::system::RunSystemOnce;
    const BANDS: u32 = 30;
    const PEOPLES: u32 = 4;
    const PASSES: u32 = 200;
    let mut scene = scene(LINK_SPAN);
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let origin = position(&scene.app, scene.sender);
    {
        let mut cohort = scene
            .app
            .world
            .get_mut::<PopulationCohort>(scene.sender)
            .expect("the sender");
        cohort.working = Scalar::from_f32(400.0);
        cohort.sync_size();
    }
    let mut bands = vec![scene.sender];
    for i in 1..BANDS {
        let split = split_band_from_parent(&mut scene.app.world, scene.sender, 3, &settle)
            .expect("a split");
        let mut by_id = scene.app.world.query::<(Entity, &BandId)>();
        let entity = by_id
            .iter(&scene.app.world)
            .find(|(_, id)| **id == split.band)
            .map(|(entity, _)| entity)
            .expect("the band");
        scene
            .app
            .world
            .get_mut::<PopulationCohort>(entity)
            .expect("the band")
            .faction = FactionId(i % PEOPLES);
        let to = east_of(&scene.app, origin, i);
        move_band_to(&mut scene.app, entity, to);
        bands.push(entity);
    }
    // A kept road the whole row.
    let far = east_of(&scene.app, origin, BANDS);
    let path = trace_path(
        origin,
        far,
        scene.app.world.resource::<TileRegistry>().width,
        scene.app.world.resource::<TileRegistry>().height,
        false,
        scene.app.world.resource::<RoadRegistry>(),
    );
    let ladder = LadderConfig::builtin();
    let top = dirt_road_top(&ladder);
    {
        let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
        for tile in &path {
            roads
                .road_or_trail(*tile, &ladder)
                .set_position(top, &ladder);
        }
    }
    let started = std::time::Instant::now();
    for _ in 0..PASSES {
        scene
            .app
            .world
            .run_system_once(core_sim::goodwill::credit_link_goodwill);
    }
    let pass = started.elapsed() / PASSES;
    let started = std::time::Instant::now();
    for _ in 0..10 {
        run_turn(&mut scene.app);
    }
    let turn = started.elapsed() / 10;
    println!(
        "link pass: {pass:?} per pass over {BANDS} bands / {PEOPLES} peoples; whole turn: {turn:?}"
    );
}
