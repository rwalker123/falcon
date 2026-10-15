//! **Offers: a shipment is an offer, and the receiver answers it** (issue #233,
//! `docs/plan_early_diplomacy.md` §1).
//!
//! The command handlers (`send_trade_expedition … offer`, `answer_dealing`) live in
//! `core_sim/src/bin/server.rs`; what is pinned here is what the world does with them. Parties are
//! spawned by hand the way `trade_expedition.rs` spawns shipments, and the answer is
//! [`core_sim::answer_dealing`], the library function the command handler calls — so the claims below
//! run the shipped resolution path, not a mirror of it.
//!
//! **Whole turns, two peoples.** The sender is the campaign's first band (faction 0); the receiver is
//! a band split off it and made another people. The fixture co-locates them, so a party arrives the
//! turn it launches.
//!
//! **Wire claims are asserted on the ENCODED frame**, through `root_as_envelope` and the accessor
//! chain a client uses — a row that never reached the codec still passes an in-process assertion.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    answer_dealing, build_test_app, recapture_snapshot_in_place, run_turn, scalar_from_f32,
    scalar_zero, split_band_from_parent, BandId, BandKey, BandTravel, CommandEventKind,
    CommandEventLog, DealingAnswer, DealingId, DiplomacyConfig, DiplomacyConfigHandle, Expedition,
    ExpeditionMission, ExpeditionPhase, FactionBorderPolicies, FactionId, LaborAllocation,
    LocalStore, PendingDealings, PopulationCohort, ResidentBand, Scalar, SettleConfig,
    ShipmentTerms, SimulationConfig, SnapshotHistory, StartingUnit, Tile, TransferCause,
    TransferDirection, ViewerFaction, FOOD,
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
/// The food a shipment carries. Well inside the party's carry.
const CARGO_FOOD: f32 = 8.0;
/// The food the receiver pays back. A different number from the cargo, so an assertion that read
/// the wrong leg could not pass by coincidence.
const RETURN_FOOD: f32 = 3.0;
/// The material the receiver pays back, and how much of it.
const RETURN_MATERIAL: &str = "hide";
const RETURN_MATERIAL_AMOUNT: f32 = 2.0;
/// What the receiver holds of that material to begin with.
const RECEIVER_MATERIAL_STOCK: f32 = 5.0;
/// Food stocked in the receiving band, so a return of food is something it holds.
const RECEIVER_FOOD_STOCK: f32 = 40.0;
/// `f32` sums of `Scalar`-quantised amounts; a few ULPs of slack, no more.
const EPSILON: f32 = 0.01;
/// Turns to drive a co-located party home. It is standing in the sender's camp already.
const TURNS_HOME: u32 = 4;
/// A faction no band belongs to — the third people that must see nothing.
const THIRD_PEOPLE: FactionId = FactionId(5);
/// The receiving people.
const RECEIVER_PEOPLE: FactionId = FactionId(9);
/// The sending people.
const SENDER_PEOPLE: FactionId = FactionId(0);

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

fn first_band(app: &mut App) -> Entity {
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    query
        .iter(&app.world)
        .next()
        .expect("the campaign spawns at least one resident band")
}

fn entity_for_band(app: &mut App, band_id: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band_id)
        .map(|(entity, _)| entity)
        .expect("the band exists")
}

fn band_position(app: &App, band: Entity) -> UVec2 {
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

fn food_cargo(amount: f32) -> LocalStore {
    let mut store = LocalStore::new();
    store.add_food("dry", scalar_from_f32(amount));
    store
}

fn band_food(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .get(FOOD)
        .to_f32()
}

fn band_material(app: &App, band: Entity, material: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .material_total(material)
        .to_f32()
}

/// Two bands standing together: the sender (faction 0) and a receiver split off it. The receiver is
/// made `receiving_people` — another people for an offer, or the sender's own for the standing
/// answer — and stocked with food and hide to pay a return from.
fn scene(receiving_people: FactionId) -> Scene {
    let mut app = spawn_world();
    let sender = first_band(&mut app);
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(sender)
            .expect("the band exists");
        cohort.working = Scalar::from_f32(PARENT_WORKERS);
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, sender, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let receiver = entity_for_band(&mut app, split.band);
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(receiver)
            .expect("the band exists");
        cohort.faction = receiving_people;
        cohort
            .stores
            .add_food("dry", scalar_from_f32(RECEIVER_FOOD_STOCK));
        let mut readings = std::collections::BTreeMap::new();
        readings.insert("toughness".to_string(), 0.6);
        readings.insert("suppleness".to_string(), 0.4);
        cohort.stores.deposit_material(
            RETURN_MATERIAL,
            BandKey(vec![3, 3]),
            scalar_from_f32(RECEIVER_MATERIAL_STOCK),
            &readings,
        );
    }
    let sender_id = *app.world.get::<BandId>(sender).expect("a band has an id");
    Scene {
        app,
        sender,
        sender_id,
        receiver,
        receiver_id: split.band,
    }
}

/// **Spawn a shipment party by hand**, the way the launch command builds one, with the terms the
/// order named.
fn launch(scene: &mut Scene, terms: ShipmentTerms, cargo: LocalStore) -> Entity {
    let destination_pos = band_position(&scene.app, scene.receiver);
    let destination_faction = scene
        .app
        .world
        .get::<PopulationCohort>(scene.receiver)
        .expect("the receiver exists")
        .faction;
    let mut cohort = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender exists")
        .clone();
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
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
                    destination_faction,
                    destination_name: String::new(),
                    terms,
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
                scout_pack: Default::default(),
                aim: Some(destination_pos),
            },
            BandTravel::to(destination_pos),
        ))
        .id()
}

fn party_phase(app: &App, party: Entity) -> Option<ExpeditionPhase> {
    app.world
        .get::<Expedition>(party)
        .map(|expedition| expedition.phase)
}

fn pending(app: &App) -> PendingDealings {
    app.world.resource::<PendingDealings>().clone()
}

fn only_dealing_id(app: &App) -> DealingId {
    let book = pending(app);
    assert_eq!(book.len(), 1, "exactly one dealing is pending: {book:?}");
    let id = book.iter().next().expect("one dealing").id;
    id
}

fn accept(returned: Vec<TradeCargoItem>) -> DealingAnswer {
    DealingAnswer::Accept { returned }
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

/// One row of the viewer's published `DealingsSection`, read off the encoded envelope.
#[derive(Debug)]
struct PublishedDealing {
    id: u64,
    kind: String,
    sender_faction: u32,
    sender_band: u64,
    receiver_faction: u32,
    receiver_band: u64,
    sent_food: f32,
    sent_materials: usize,
    asked_food: f32,
    asked_materials: usize,
    turns_left: u32,
}

/// **The dealings `viewer`'s frame carries**, off the ENCODED snapshot: the viewer is set, the frame
/// recaptured for them, encoded, and read back through the accessor chain a client uses.
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
                        sender_faction: row.senderFaction(),
                        sender_band: row.senderBand(),
                        receiver_faction: row.receiverFaction(),
                        receiver_band: row.receiverBand(),
                        sent_food: sent.food(),
                        sent_materials: sent.materials().map_or(0, |rows| rows.len()),
                        asked_food: asked.food(),
                        asked_materials: asked.materials().map_or(0, |rows| rows.len()),
                        turns_left: row.turnsLeft(),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The own-people standing answer, as the encoded frame publishes it for `viewer`.
fn published_accept_own_gifts(app: &mut App, viewer: FactionId) -> bool {
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
    let rows = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .campaign()
        .and_then(|campaign| campaign.factionPolicies())
        .expect("the viewer's policy row is published");
    assert_eq!(rows.len(), 1, "a frame carries the viewer's own row only");
    rows.get(0).acceptOwnGifts()
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

/// Rows the band's store published this turn for `commodity` under `cause`, summed.
fn crossing_total(app: &App, band: Entity, commodity: &str, cause: TransferCause) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .last_turn_transfer_crossings
        .iter()
        .filter(|crossing| crossing.commodity == commodity && crossing.cause == cause)
        .map(|crossing| crossing.amount)
        .sum()
}

/// Drive the world until `party` folds back (or the budget runs out), collecting what the sender's
/// store published each turn.
fn run_home(scene: &mut Scene, party: Entity) -> Vec<(String, TransferCause, f32)> {
    let mut rows = Vec::new();
    for _ in 0..TURNS_HOME {
        if scene.app.world.get::<Expedition>(party).is_none() {
            break;
        }
        run_turn(&mut scene.app);
        rows.extend(sender_rows(scene));
    }
    rows
}

/// What the sender's store took in this turn, as `(commodity, cause, amount)`.
fn sender_rows(scene: &Scene) -> Vec<(String, TransferCause, f32)> {
    scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender exists")
        .last_turn_transfer_crossings
        .iter()
        .filter(|crossing| crossing.direction == TransferDirection::In)
        .map(|crossing| (crossing.commodity.clone(), crossing.cause, crossing.amount))
        .collect()
}

fn sum_rows(rows: &[(String, TransferCause, f32)], commodity: &str, cause: TransferCause) -> f32 {
    rows.iter()
        .filter(|(row_commodity, row_cause, _)| row_commodity == commodity && *row_cause == cause)
        .map(|(_, _, amount)| *amount)
        .sum()
}

// ---------------------------------------------------------------------------------------------
// An offer arrives and waits
// ---------------------------------------------------------------------------------------------

/// **An offer is not delivered — it is posted.** The party stands in the receiver's camp with its
/// cargo; the receiver's larder has not moved; and the dealing is on the ENCODED frame of the
/// receiving people and of the sending people, and on nobody else's.
#[test]
fn an_offer_waits_as_a_pending_dealing_visible_to_its_two_peoples_and_no_third() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let receiver_food_before = band_food(&scene.app, scene.receiver);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));

    run_turn(&mut scene.app);

    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::AwaitingOrders),
        "the party waits in their camp for the answer"
    );
    assert!(
        scene
            .app
            .world
            .get::<Expedition>(party)
            .expect("the party stands there")
            .cargo
            .get(FOOD)
            .to_f32()
            > CARGO_FOOD - EPSILON,
        "the cargo is still the party's: nothing was delivered"
    );
    assert!(
        band_food(&scene.app, scene.receiver) <= receiver_food_before + EPSILON,
        "the receiver's larder gained nothing before it answered"
    );

    let id = only_dealing_id(&scene.app);
    let dealing = pending(&scene.app).get(id).cloned().expect("the dealing");
    assert_eq!(dealing.sender_faction, SENDER_PEOPLE);
    assert_eq!(dealing.receiver_faction, RECEIVER_PEOPLE);
    assert_eq!(dealing.sender_band, scene.sender_id);
    assert_eq!(dealing.receiver_band, scene.receiver_id);
    assert_eq!(
        dealing.expires_turn - dealing.posted_turn,
        u64::from(DiplomacyConfig::builtin().offers.offer_wait_turns),
        "it expires offer_wait_turns after it was posted"
    );

    // --- the ENCODED frames -----------------------------------------------------------------
    let to_receiver = dealings_seen_by(&mut scene.app, RECEIVER_PEOPLE);
    assert_eq!(
        to_receiver.len(),
        1,
        "the receiver sees it: {to_receiver:?}"
    );
    let row = &to_receiver[0];
    assert_eq!(row.id, id.0);
    assert_eq!(row.kind, "offer");
    assert_eq!(
        (
            row.sender_faction,
            row.sender_band,
            row.receiver_faction,
            row.receiver_band
        ),
        (
            SENDER_PEOPLE.0,
            scene.sender_id.0,
            RECEIVER_PEOPLE.0,
            scene.receiver_id.0
        )
    );
    assert!(
        (row.sent_food - CARGO_FOOD).abs() < EPSILON,
        "the row names what was sent: {}",
        row.sent_food
    );
    assert_eq!(
        (row.asked_food, row.asked_materials),
        (0.0, 0),
        "a stage-2 offer asks nothing: the receiver names the return"
    );
    assert_eq!(row.sent_materials, 0);
    assert_eq!(row.turns_left, 1, "answer this turn or silence refuses it");

    let to_sender = dealings_seen_by(&mut scene.app, SENDER_PEOPLE);
    assert_eq!(to_sender.len(), 1, "the sender sees its own item");
    assert_eq!(to_sender[0].id, id.0);

    assert!(
        dealings_seen_by(&mut scene.app, THIRD_PEOPLE).is_empty(),
        "another pair's dealings are theirs to know"
    );

    // --- the feed: one line per side ----------------------------------------------------------
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "offered").len(),
        1,
        "the receiver is told an offer arrived"
    );
    let posted = dealing_events(&scene.app, SENDER_PEOPLE, "posted");
    assert_eq!(posted.len(), 1, "the sender is told it is waiting");
    assert!(
        posted[0].contains(&format!("destination={}", scene.receiver_id.0)),
        "band ids ride as tokens a client swaps its own label in for: {}",
        posted[0]
    );
}

// ---------------------------------------------------------------------------------------------
// Accept, with a return
// ---------------------------------------------------------------------------------------------

/// **Accept takes the cargo and pays the named return, and the return rides the same party home.**
/// The cargo lands in the receiving band; the return leaves it as `ExchangeOut`; and on the
/// fold-back the return lands at home as `ExchangeIn` — never as a shipment that failed to arrive.
#[test]
fn accepting_with_a_return_lands_the_cargo_and_the_return_comes_home() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);

    let receiver_food_before = band_food(&scene.app, scene.receiver);
    let receiver_hide_before = band_material(&scene.app, scene.receiver, RETURN_MATERIAL);
    let sender_hide_before = band_material(&scene.app, scene.sender, RETURN_MATERIAL);

    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(vec![
            food_line(RETURN_FOOD),
            material_line(RETURN_MATERIAL, RETURN_MATERIAL_AMOUNT),
        ]),
    )
    .expect("the receiver holds what it named");
    assert!(report.accepted);
    assert_eq!(report.returned.len(), 2, "food and one material rated");

    // The answer resolves in the turn it is submitted: the cargo is the receiver's now, the return
    // is the party's, the party has turned for home, and the dealing is gone.
    assert!(
        (band_food(&scene.app, scene.receiver) - (receiver_food_before + CARGO_FOOD - RETURN_FOOD))
            .abs()
            < EPSILON,
        "the receiver gained the cargo and paid the return"
    );
    assert!(
        (band_material(&scene.app, scene.receiver, RETURN_MATERIAL)
            - (receiver_hide_before - RETURN_MATERIAL_AMOUNT))
            .abs()
            < EPSILON,
        "the receiver paid its hide"
    );
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::Returning)
    );
    {
        let carried = &scene
            .app
            .world
            .get::<Expedition>(party)
            .expect("the party")
            .cargo;
        assert!((carried.get(FOOD).to_f32() - RETURN_FOOD).abs() < EPSILON);
        assert!(
            (carried.material_total(RETURN_MATERIAL).to_f32() - RETURN_MATERIAL_AMOUNT).abs()
                < EPSILON
        );
    }
    assert!(pending(&scene.app).is_empty(), "the dealing is resolved");
    assert_eq!(
        dealing_events(&scene.app, SENDER_PEOPLE, "accepted").len(),
        1
    );
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "accepted").len(),
        1
    );

    // The receiver's books: the cargo in as ShipmentIn, the return out as ExchangeOut. The party is
    // standing in the sender's camp too, so it may fold back on this very turn — the sender's rows
    // are kept from it.
    run_turn(&mut scene.app);
    let mut rows = sender_rows(&scene);
    assert!(
        (crossing_total(&scene.app, scene.receiver, FOOD, TransferCause::ShipmentIn) - CARGO_FOOD)
            .abs()
            < EPSILON,
        "the cargo landed as a shipment"
    );
    assert!(
        (crossing_total(&scene.app, scene.receiver, FOOD, TransferCause::ExchangeOut)
            - RETURN_FOOD)
            .abs()
            < EPSILON,
        "the return left the receiver as ExchangeOut"
    );

    // And the return lands at home on the fold-back, booked as the exchange's return.
    rows.extend(run_home(&mut scene, party));
    assert!(
        scene.app.world.get::<Expedition>(party).is_none(),
        "the party folds back"
    );
    assert!(
        (sum_rows(&rows, FOOD, TransferCause::ExchangeIn) - RETURN_FOOD).abs() < EPSILON,
        "the food return came home as ExchangeIn: {rows:?}"
    );
    assert!(
        (sum_rows(&rows, RETURN_MATERIAL, TransferCause::ExchangeIn) - RETURN_MATERIAL_AMOUNT)
            .abs()
            < EPSILON,
        "the material return came home as ExchangeIn: {rows:?}"
    );
    assert_eq!(
        sum_rows(&rows, FOOD, TransferCause::ShipmentReturned),
        0.0,
        "an accepted offer's return is not a shipment that failed to arrive"
    );
    assert!(
        band_material(&scene.app, scene.sender, RETURN_MATERIAL)
            >= sender_hide_before + RETURN_MATERIAL_AMOUNT - EPSILON,
        "the hide is in the sender's store"
    );
}

/// **An empty return is allowed** — the receiver names the return, and it may name nothing. The
/// cargo still lands, and the party walks home empty-handed.
#[test]
fn accepting_with_no_return_lands_the_cargo_and_the_party_walks_home_empty() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);
    let before = band_food(&scene.app, scene.receiver);

    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(Vec::new()),
    )
    .expect("an empty return needs nothing the band might lack");
    assert!(report.accepted && report.returned.is_empty());
    assert!((band_food(&scene.app, scene.receiver) - (before + CARGO_FOOD)).abs() < EPSILON);
    assert!(
        scene
            .app
            .world
            .get::<Expedition>(party)
            .expect("the party")
            .cargo
            .get(FOOD)
            .to_f32()
            .abs()
            < EPSILON
    );
}

// ---------------------------------------------------------------------------------------------
// Refuse, and silence
// ---------------------------------------------------------------------------------------------

/// **Refusal sends the party home with its cargo**, over the existing `ShipmentReturned` path.
#[test]
fn refusing_sends_the_cargo_home() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);
    let receiver_before = band_food(&scene.app, scene.receiver);

    let report = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Refuse,
    )
    .expect("a refusal needs nothing");
    assert!(!report.accepted);
    assert!(pending(&scene.app).is_empty());
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::Returning)
    );
    assert!((band_food(&scene.app, scene.receiver) - receiver_before).abs() < EPSILON);
    assert_eq!(
        dealing_events(&scene.app, SENDER_PEOPLE, "refused").len(),
        1
    );
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "refused").len(),
        1
    );

    let rows = run_home(&mut scene, party);
    assert!(scene.app.world.get::<Expedition>(party).is_none());
    assert!(
        (sum_rows(&rows, FOOD, TransferCause::ShipmentReturned) - CARGO_FOOD).abs() < EPSILON,
        "the whole shipment came home as ShipmentReturned: {rows:?}"
    );
}

/// **Silence is a refusal after `offer_wait_turns`.** One unanswered turn at the opening value and
/// the offer lapses, the party turns home with its own cargo, and both peoples are told.
#[test]
fn silence_expires_the_offer_and_the_cargo_comes_home() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    assert_eq!(pending(&scene.app).len(), 1);

    run_turn(&mut scene.app);

    assert!(pending(&scene.app).is_empty(), "silence refused it");
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::Returning)
    );
    assert_eq!(
        dealing_events(&scene.app, SENDER_PEOPLE, "expired").len(),
        1
    );
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "expired").len(),
        1
    );
    let rows = run_home(&mut scene, party);
    assert!(
        (sum_rows(&rows, FOOD, TransferCause::ShipmentReturned) - CARGO_FOOD).abs() < EPSILON,
        "the cargo came home: {rows:?}"
    );
}

/// **The wait is the config's.** At `offer_wait_turns = 3` the offer survives two silent turns and
/// lapses on the third — and answering inside the window still works.
#[test]
fn a_longer_wait_holds_the_offer_open_that_many_turns() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_DIPLOMACY_CONFIG).expect("the builtin is JSON");
    json["offers"]["offer_wait_turns"] = serde_json::json!(3);
    scene
        .app
        .world
        .insert_resource(DiplomacyConfigHandle::new(std::sync::Arc::new(
            DiplomacyConfig::from_json_str(&json.to_string()).expect("a valid config"),
        )));
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);
    assert_eq!(
        dealings_seen_by(&mut scene.app, RECEIVER_PEOPLE)[0].turns_left,
        3
    );

    run_turn(&mut scene.app);
    run_turn(&mut scene.app);
    assert_eq!(pending(&scene.app).len(), 1, "still open after two turns");
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::AwaitingOrders)
    );
    run_turn(&mut scene.app);
    assert!(pending(&scene.app).is_empty(), "lapsed on the third");
    assert!(
        answer_dealing(
            &mut scene.app.world,
            RECEIVER_PEOPLE,
            id,
            &DealingAnswer::Refuse
        )
        .is_err(),
        "an expired dealing cannot be answered"
    );
}

// ---------------------------------------------------------------------------------------------
// The standing answer
// ---------------------------------------------------------------------------------------------

/// **A gift to one's own people lands on arrival, exactly as it did before offers existed** — no
/// dealing is posted, the party turns home, and a delivery line is written.
#[test]
fn a_same_people_gift_still_lands_on_arrival() {
    let mut scene = scene(SENDER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Gift, food_cargo(CARGO_FOOD));

    run_turn(&mut scene.app);

    assert!(
        pending(&scene.app).is_empty(),
        "nothing waits to be answered"
    );
    assert!(
        scene
            .app
            .world
            .resource::<CommandEventLog>()
            .iter()
            .any(|entry| entry.kind == CommandEventKind::TradeDelivered),
        "the delivery line was written"
    );
    assert!(
        scene
            .app
            .world
            .get::<Expedition>(party)
            .is_none_or(|expedition| expedition.cargo.get(FOOD).to_f32() < EPSILON
                && expedition.phase == ExpeditionPhase::Returning),
        "the cargo left the party and it turned for home"
    );
    assert!(
        dealing_events(&scene.app, SENDER_PEOPLE, "posted").is_empty(),
        "no dealing line"
    );
}

/// **The standing answer is the people's to turn off.** With "accept gifts from my own people" off
/// the very same shipment waits to be answered like any other.
#[test]
fn a_same_people_gift_with_the_answer_off_becomes_pending() {
    let mut scene = scene(SENDER_PEOPLE);
    assert!(
        published_accept_own_gifts(&mut scene.app, SENDER_PEOPLE),
        "every people starts with the answer on, and the frame says so"
    );
    scene
        .app
        .world
        .resource_mut::<FactionBorderPolicies>()
        .set_accept_own_gifts(SENDER_PEOPLE, false);
    assert!(
        !published_accept_own_gifts(&mut scene.app, SENDER_PEOPLE),
        "the setting is on the viewer's own frame"
    );
    let party = launch(&mut scene, ShipmentTerms::Gift, food_cargo(CARGO_FOOD));

    run_turn(&mut scene.app);

    assert_eq!(pending(&scene.app).len(), 1, "it waits");
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::AwaitingOrders)
    );
    // Both ends are the same people, so it is addressed to the people it came from.
    assert_eq!(
        dealings_seen_by(&mut scene.app, SENDER_PEOPLE).len(),
        1,
        "the people answers its own band's gift"
    );
}

/// **A gift to another people is pending too** — the standing answer covers only one's own.
#[test]
fn a_gift_to_another_people_waits_to_be_answered() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Gift, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    assert_eq!(pending(&scene.app).len(), 1);
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::AwaitingOrders)
    );
}

// ---------------------------------------------------------------------------------------------
// The answer fails closed
// ---------------------------------------------------------------------------------------------

/// **An answer that cannot be honoured moves nothing.** Return goods the receiver lacks, an unknown
/// material, an amount that is not a number, and a return over the party's carry are each a refusal
/// with a reason — and the receiver, the party and the book are exactly as they stood.
#[test]
fn an_answer_that_cannot_be_honoured_is_refused_and_nothing_moves() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);
    let book_before = pending(&scene.app);
    let receiver_food = band_food(&scene.app, scene.receiver);
    let receiver_hide = band_material(&scene.app, scene.receiver, RETURN_MATERIAL);

    let too_much_food = RECEIVER_FOOD_STOCK * 10.0;
    let hide_held = receiver_hide;
    let beyond_the_carry = [
        // The receiver holds only RECEIVER_FOOD_STOCK of food.
        vec![food_line(too_much_food)],
        // More hide than it holds.
        vec![material_line(RETURN_MATERIAL, hide_held + 1.0)],
        // A material nobody has heard of.
        vec![material_line("unobtainium", 1.0)],
        // Not an amount.
        vec![food_line(f32::NAN)],
        vec![food_line(-1.0)],
        // A commodity key that is not the larder's.
        vec![TradeCargoItem {
            id: "gold".to_string(),
            is_material: false,
            amount: 1.0,
        }],
        // Held, but more than two workers can carry home (and a held line beside a lacking one
        // fails as a whole, not by trimming).
        vec![food_line(RETURN_FOOD), food_line(too_much_food)],
    ];
    for returned in beyond_the_carry {
        let reason = answer_dealing(
            &mut scene.app.world,
            RECEIVER_PEOPLE,
            id,
            &accept(returned.clone()),
        )
        .expect_err("an impossible return is refused");
        assert!(!reason.is_empty(), "a refusal carries a reason");
        assert_eq!(pending(&scene.app), book_before, "{returned:?}: book moved");
        assert!((band_food(&scene.app, scene.receiver) - receiver_food).abs() < 1e-6);
        assert!(
            (band_material(&scene.app, scene.receiver, RETURN_MATERIAL) - receiver_hide).abs()
                < 1e-6
        );
        assert_eq!(
            party_phase(&scene.app, party),
            Some(ExpeditionPhase::AwaitingOrders),
            "the party still waits"
        );
        assert!(
            scene
                .app
                .world
                .get::<Expedition>(party)
                .expect("the party")
                .cargo
                .get(FOOD)
                .to_f32()
                > CARGO_FOOD - EPSILON,
            "the cargo is still aboard"
        );
    }

    // And the same receiver, naming what it does hold, is answered.
    assert!(answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(vec![food_line(RETURN_FOOD)])
    )
    .is_ok());
}

/// **A return over the party's pack is refused**, with the carry in the reason. The receiver holds
/// it; the two workers cannot walk it home.
#[test]
fn a_return_over_the_partys_carry_is_refused() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);
    {
        let mut cohort = scene
            .app
            .world
            .get_mut::<PopulationCohort>(scene.receiver)
            .expect("the receiver");
        cohort.stores.add_food("dry", scalar_from_f32(1_000.0));
    }
    let before = band_food(&scene.app, scene.receiver);

    let reason = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(vec![food_line(500.0)]),
    )
    .expect_err("two workers cannot carry five hundred home");
    assert!(reason.contains("carry"), "{reason}");
    assert!((band_food(&scene.app, scene.receiver) - before).abs() < 1e-6);
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::AwaitingOrders)
    );
    assert_eq!(pending(&scene.app).len(), 1);
}

/// **Only the receiver's people may answer.** The sender cannot accept its own offer, and a third
/// people cannot see it, let alone answer it.
#[test]
fn only_the_receiving_people_may_answer() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);

    for impostor in [SENDER_PEOPLE, THIRD_PEOPLE] {
        assert!(
            answer_dealing(&mut scene.app.world, impostor, id, &accept(Vec::new())).is_err(),
            "{impostor:?} cannot answer"
        );
        assert!(
            answer_dealing(&mut scene.app.world, impostor, id, &DealingAnswer::Refuse).is_err(),
            "{impostor:?} cannot refuse it either"
        );
    }
    assert_eq!(pending(&scene.app).len(), 1);
    assert_eq!(
        party_phase(&scene.app, party),
        Some(ExpeditionPhase::AwaitingOrders)
    );
}

/// **A dealing whose party has left is withdrawn, not answered.** The sender recalls its party;
/// the receiver's later accept is refused and the dealing leaves the book.
#[test]
fn a_dealing_whose_party_has_gone_is_withdrawn() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);

    scene
        .app
        .world
        .get_mut::<Expedition>(party)
        .expect("the party")
        .phase = ExpeditionPhase::Returning;

    let reason = answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(vec![food_line(RETURN_FOOD)]),
    )
    .expect_err("there is nobody left to answer");
    assert!(reason.contains("gone"), "{reason}");
    assert!(pending(&scene.app).is_empty());
    assert_eq!(
        dealing_events(&scene.app, RECEIVER_PEOPLE, "withdrawn").len(),
        1
    );
}

// ---------------------------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------------------------

/// **A pending dealing rides the checkpoint whole, counter and all.** Capture it waiting, let it
/// lapse, restore: the dealing is back, the party is back in the camp awaiting the answer, and the
/// next id continues from where the counter stood.
#[test]
fn a_pending_dealing_round_trips_a_checkpoint() {
    let mut scene = scene(RECEIVER_PEOPLE);
    launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = only_dealing_id(&scene.app);
    let book = pending(&scene.app);
    let checkpoint = capture_sim_state(&scene.app.world);

    run_turn(&mut scene.app);
    assert!(pending(&scene.app).is_empty(), "silence lapsed it");

    restore_sim_state(&mut scene.app.world, &checkpoint);

    assert_eq!(pending(&scene.app), book, "the book comes back whole");
    let mut parties = scene
        .app
        .world
        .query_filtered::<&Expedition, With<Expedition>>();
    let restored: Vec<_> = parties.iter(&scene.app.world).collect();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].phase, ExpeditionPhase::AwaitingOrders);
    assert!(restored[0].cargo.get(FOOD).to_f32() > CARGO_FOOD - EPSILON);
    assert_eq!(
        restored[0].mission.shipment_terms(),
        Some(&ShipmentTerms::Offer),
        "the terms ride the mission"
    );

    // The restored dealing is answerable, and the counter did not rewind below the id in use.
    answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(vec![food_line(RETURN_FOOD)]),
    )
    .expect("a restored dealing can be answered");
    let next = scene
        .app
        .world
        .resource_mut::<PendingDealings>()
        .post(core_sim::Dealing {
            id: DealingId::default(),
            kind: core_sim::DealingKind::Offer,
            sender_faction: SENDER_PEOPLE,
            sender_band: scene.sender_id,
            receiver_faction: RECEIVER_PEOPLE,
            receiver_band: scene.receiver_id,
            party: None,
            goods_sent: LocalStore::new(),
            goods_asked: LocalStore::new(),
            agreement: None,
            posted_turn: 0,
            expires_turn: 1,
        });
    assert!(next > id, "ids are never reused across a restore");
}

/// **A fresh world's standing answer is on for every people**, and the book starts empty.
#[test]
fn a_new_world_starts_with_the_answer_on_and_no_dealings() {
    let mut app = spawn_world();
    assert!(pending(&app).is_empty());
    let policies = app.world.resource::<FactionBorderPolicies>();
    assert!(policies.accepts_own_gifts(FactionId(0)));
    assert!(policies.contains(FactionId(0)));
    assert!(published_accept_own_gifts(&mut app, FactionId(0)));
}
