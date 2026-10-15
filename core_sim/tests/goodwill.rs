//! **Goodwill: what one people thinks of another, and who knows it** (issue #233,
//! `docs/plan_early_diplomacy.md` §2).
//!
//! Whole turns on a two-people world, driven the way `dealings.rs` drives offers: parties are spawned
//! by hand and answered through [`core_sim::answer_dealing`]. The pure arithmetic (worth, clamp,
//! fade, the report clock) is unit-tested beside the code in `goodwill.rs`; what is pinned here is
//! that the sources credit the right pair, that nothing else does, that the opinion fades to its
//! equilibrium, and that **news is carried** — it reaches a people's `KnownGoodwill` only with the
//! party that saw it, never on the turn it was credited.
//!
//! **Wire claims are asserted on the ENCODED frame**, through `root_as_envelope`.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    answer_dealing, build_test_app, cargo_worth, recapture_snapshot_in_place, run_turn,
    scalar_from_f32, scalar_zero, split_band_from_parent, BandId, BandTravel, DealingAnswer,
    DiplomacyConfig, Expedition, ExpeditionMission, ExpeditionPhase, FactionBorderPolicies,
    FactionId, Goodwill, GoodwillEntry, GoodwillNews, GoodwillSource, KnownGoodwill,
    LaborAllocation, LocalStore, PendingDealings, PopulationCohort, ResidentBand, Scalar,
    SettleConfig, ShipmentTerms, SimulationConfig, SnapshotHistory, StartingUnit, Tile,
    TileRegistry, ViewerFaction,
};
use sim_runtime::{TradeCargoItem, FOOD_CARGO_KEY};

const MAP_SEED: u64 = 119_304_647;
const PARENT_WORKERS: f32 = 20.0;
const SPLIT_WORKERS: u32 = 5;
const PARTY_WORKERS: u32 = 2;
const CARGO_FOOD: f32 = 8.0;
const RETURN_FOOD: f32 = 3.0;
const RECEIVER_FOOD_STOCK: f32 = 40.0;
const EPSILON: f32 = 1e-3;
const RECEIVER_PEOPLE: FactionId = FactionId(9);
const SENDER_PEOPLE: FactionId = FactionId(0);
const THIRD_PEOPLE: FactionId = FactionId(5);
/// How far the receiver is walked from the sender when a test needs the party genuinely on the road.
const OUT_OF_REACH_TILES: u32 = 20;
/// Turns to let a party walk out, wait and walk home from [`OUT_OF_REACH_TILES`].
const MAX_TURNS_ROUND_TRIP: u32 = 90;
/// Workers of the stationed post. At the opening `per_person_stationed` they credit a tenth a turn
/// a hundred of them — ten of them a hundredth.
const POST_WORKERS: u32 = 10;
/// Turns a post is held in the equilibrium test.
const POST_TURNS: u32 = 200;
/// How near the closed form the settled opinion must sit. The closed form is exact in `f32` up to
/// accumulated rounding.
const CLOSED_FORM_TOLERANCE: f32 = 2e-3;

struct Scene {
    app: App,
    sender: Entity,
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

fn scene(receiving_people: FactionId) -> Scene {
    let mut app = spawn_world();
    let mut query = app
        .world
        .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
    let sender = query.iter(&app.world).next().expect("a resident band");
    {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(sender)
            .expect("the band");
        cohort.working = Scalar::from_f32(PARENT_WORKERS);
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
            .expect("the band");
        cohort.faction = receiving_people;
        cohort
            .stores
            .add_food("dry", scalar_from_f32(RECEIVER_FOOD_STOCK));
    }
    Scene {
        app,
        sender,
        receiver,
        receiver_id: split.band,
    }
}

fn position(app: &App, band: Entity) -> UVec2 {
    let tile = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the band")
        .current_tile;
    app.world.get::<Tile>(tile).expect("a tile").position
}

/// Walk the receiver `OUT_OF_REACH_TILES` east of where it stands, so a party bound for it spends
/// real turns on the road.
fn walk_receiver_away(scene: &mut Scene) {
    let from = position(&scene.app, scene.receiver);
    let (width, height) = {
        let registry = scene.app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let target = UVec2::new(
        (from.x + OUT_OF_REACH_TILES) % width.max(1),
        from.y.min(height.saturating_sub(1)),
    );
    let tile = scene
        .app
        .world
        .resource::<TileRegistry>()
        .index(target.x, target.y)
        .expect("a tile on the map");
    let mut cohort = scene
        .app
        .world
        .get_mut::<PopulationCohort>(scene.receiver)
        .expect("the receiver");
    cohort.current_tile = tile;
    cohort.home = tile;
}

fn food_cargo(amount: f32) -> LocalStore {
    let mut store = LocalStore::new();
    store.add_food("dry", scalar_from_f32(amount));
    store
}

fn launch(scene: &mut Scene, terms: ShipmentTerms, cargo: LocalStore) -> Entity {
    let destination_pos = position(&scene.app, scene.receiver);
    let destination_faction = scene
        .app
        .world
        .get::<PopulationCohort>(scene.receiver)
        .expect("the receiver")
        .faction;
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
                exchange_return: Default::default(),
                scout_pack: Default::default(),
                aim: Some(destination_pos),
            },
            BandTravel::to(destination_pos),
        ))
        .id()
}

fn goodwill(app: &App, holder: FactionId, about: FactionId) -> GoodwillEntry {
    app.world.resource::<Goodwill>().get(holder, about)
}

fn per_load() -> f32 {
    DiplomacyConfig::builtin().goodwill.per_load
}

fn dealing_id(app: &App) -> core_sim::DealingId {
    let book = app.world.resource::<PendingDealings>();
    assert_eq!(book.len(), 1, "one dealing is pending");
    let id = book.iter().next().expect("one").id;
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

fn carry() -> core_sim::CarryConfig {
    core_sim::ExpeditionConfig::builtin().carry.clone()
}

/// An offer posted and waiting, co-located: returns the scene, the party and the dealing's id.
fn posted_offer() -> (Scene, Entity, core_sim::DealingId) {
    let mut scene = scene(RECEIVER_PEOPLE);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = dealing_id(&scene.app);
    (scene, party, id)
}

// ---------------------------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------------------------

/// **A gift raises the receiver's opinion of the giver — and only that.** An accepted offer with an
/// empty return is a gift: `per_load × worth`, receiver → giver, into `gifts`, with the worth judged
/// against what the receiving band held immediately before.
#[test]
fn a_gift_credits_the_receivers_opinion_of_the_giver_only() {
    let (mut scene, _party, id) = posted_offer();
    let held_before = scene
        .app
        .world
        .get::<PopulationCohort>(scene.receiver)
        .expect("the receiver")
        .stores
        .clone();

    answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(Vec::new()),
    )
    .expect("a bare accept");

    let expected = per_load() * cargo_worth(&food_cargo(CARGO_FOOD), &held_before, &carry());
    let entry = goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE);
    assert!(expected > 0.0, "liveness: the gift was worth something");
    assert!(
        (entry.gifts - expected).abs() < EPSILON,
        "receiver -> giver, in `gifts`: {} vs {expected}",
        entry.gifts
    );
    assert_eq!(entry.exchanges, 0.0);
    assert_eq!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE),
        GoodwillEntry::default(),
        "the giver's opinion of the receiver does not move"
    );
    assert_eq!(scene.app.world.resource::<Goodwill>().len(), 1);
}

/// **A settled exchange credits both ways**, each by the worth of what that side received, judged
/// against that side's own larder at that moment — the cargo against the receiver's, the return
/// against the sender's home band's.
#[test]
fn an_exchange_credits_both_peoples_by_what_each_received() {
    let (mut scene, _party, id) = posted_offer();
    let receiver_held = scene
        .app
        .world
        .get::<PopulationCohort>(scene.receiver)
        .expect("the receiver")
        .stores
        .clone();
    let home_held = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender")
        .stores
        .clone();

    answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(vec![food_line(RETURN_FOOD)]),
    )
    .expect("the receiver holds the return");

    let to_receiver = per_load() * cargo_worth(&food_cargo(CARGO_FOOD), &receiver_held, &carry());
    let to_sender = per_load() * cargo_worth(&food_cargo(RETURN_FOOD), &home_held, &carry());
    let receiver_view = goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE);
    let sender_view = goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE);
    assert!(to_receiver > 0.0 && to_sender > 0.0, "liveness");
    assert!((receiver_view.exchanges - to_receiver).abs() < EPSILON);
    assert!((sender_view.exchanges - to_sender).abs() < EPSILON);
    assert_eq!((receiver_view.gifts, sender_view.gifts), (0.0, 0.0));
}

/// **A refusal costs nothing, and neither does silence.** Saying no is not an insult.
#[test]
fn a_refusal_and_a_lapse_credit_nothing() {
    let (mut refused, _party, id) = posted_offer();
    answer_dealing(
        &mut refused.app.world,
        RECEIVER_PEOPLE,
        id,
        &DealingAnswer::Refuse,
    )
    .expect("a refusal");
    assert!(refused.app.world.resource::<Goodwill>().is_empty());

    let (mut lapsed, _party, _id) = posted_offer();
    run_turn(&mut lapsed.app);
    assert!(lapsed.app.world.resource::<PendingDealings>().is_empty());
    assert!(lapsed.app.world.resource::<Goodwill>().is_empty());
}

/// **Nobody holds an opinion of their own people.** A gift between two bands of one people (the
/// standing answer off, so it waits) is accepted and credits nothing — the one credit seam skips a
/// self-pair, so there is no entry to find.
#[test]
fn a_self_pair_is_never_stored() {
    let mut scene = scene(SENDER_PEOPLE);
    scene
        .app
        .world
        .resource_mut::<FactionBorderPolicies>()
        .set_accept_own_gifts(SENDER_PEOPLE, false);
    launch(&mut scene, ShipmentTerms::Gift, food_cargo(CARGO_FOOD));
    run_turn(&mut scene.app);
    let id = dealing_id(&scene.app);
    answer_dealing(&mut scene.app.world, SENDER_PEOPLE, id, &accept(Vec::new())).expect("accept");
    answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        core_sim::DealingId(id.0 + 1),
        &accept(Vec::new()),
    )
    .expect_err("there is no such dealing");
    assert!(
        scene.app.world.resource::<Goodwill>().is_empty(),
        "no self-pair entry"
    );
}

// ---------------------------------------------------------------------------------------------
// Fade and the stationed post
// ---------------------------------------------------------------------------------------------

/// **A post of ten settles at `0.01 / 0.02 = 0.5`.** Each turn the opinion fades by 2% and the post
/// credits `per_person_stationed × 10` — the fade runs first, so after `n` turns the opinion is
/// exactly `0.5 × (1 − 0.98^n)`, which this pins turn by turn's closed form and, at the end, against
/// the equilibrium.
#[test]
fn a_stationed_post_settles_at_its_equilibrium() {
    let mut scene = scene(RECEIVER_PEOPLE);
    let config = DiplomacyConfig::builtin();
    let credit = config.goodwill.per_person_stationed * POST_WORKERS as f32;
    let fade = config.goodwill.fade_per_turn;
    let equilibrium = credit / fade;

    let host_pos = position(&scene.app, scene.receiver);
    let mut cohort = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender")
        .clone();
    cohort.working = Scalar::from_u32(POST_WORKERS);
    cohort.children = scalar_zero();
    cohort.elders = scalar_zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let band_id = scene
        .app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    let party = scene
        .app
        .world
        .spawn((
            cohort,
            band_id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: scene.sender,
                mission: ExpeditionMission::Station {
                    host_band: scene.receiver_id,
                    host_name: String::new(),
                },
                phase: ExpeditionPhase::Stationed,
                announced: true,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo: LocalStore::new(),
                defection_pull: core_sim::Scalar::zero(),
                station_reports: Vec::new(),
                goodwill_news: Default::default(),
                exchange_return: Default::default(),
                scout_pack: Default::default(),
                aim: Some(host_pos),
            },
        ))
        .id();

    for turn in 1..=POST_TURNS {
        run_turn(&mut scene.app);
        let expected = equilibrium * (1.0 - (1.0 - fade).powi(turn as i32));
        let held = goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE);
        assert!(
            (held.total() - expected).abs() < CLOSED_FORM_TOLERANCE,
            "turn {turn}: {} vs closed form {expected}",
            held.total()
        );
        assert!(
            (held.stationed - held.total()).abs() < EPSILON,
            "all of it is the post"
        );
    }
    assert_eq!(
        scene
            .app
            .world
            .get::<Expedition>(party)
            .map(|expedition| expedition.phase),
        Some(ExpeditionPhase::Stationed),
        "liveness: the post stood the whole time"
    );
    let settled = goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).total();
    assert!(
        (settled - equilibrium).abs() < 0.02 * equilibrium + CLOSED_FORM_TOLERANCE,
        "it has settled near {equilibrium}: {settled}"
    );
    assert_eq!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE),
        GoodwillEntry::default(),
        "the host's opinion moves, not the sender's"
    );
}

// ---------------------------------------------------------------------------------------------
// Who knows it: news is carried
// ---------------------------------------------------------------------------------------------

/// **The sender learns what the receiver thinks of it only when a party comes home with the news.**
/// The receiver is a long walk away. The offer is answered and credited while the party is still at
/// the far camp — and the sender's `KnownGoodwill` is empty then, and for every turn the party is on
/// the road. It lands on the fold-back, stamped with the turn the party was last in contact, which is
/// earlier than the turn it got home.
#[test]
fn news_of_an_opinion_arrives_only_with_the_returning_party() {
    let mut scene = scene(RECEIVER_PEOPLE);
    walk_receiver_away(&mut scene);
    let party = launch(&mut scene, ShipmentTerms::Offer, food_cargo(CARGO_FOOD));

    // Walk out until the offer is posted.
    let mut posted = false;
    for _ in 0..MAX_TURNS_ROUND_TRIP {
        run_turn(&mut scene.app);
        assert!(
            scene.app.world.resource::<KnownGoodwill>().is_empty(),
            "nothing has come home yet"
        );
        if !scene.app.world.resource::<PendingDealings>().is_empty() {
            posted = true;
            break;
        }
    }
    assert!(
        posted,
        "liveness: the party reached the far camp and posted"
    );

    let id = dealing_id(&scene.app);
    answer_dealing(
        &mut scene.app.world,
        RECEIVER_PEOPLE,
        id,
        &accept(Vec::new()),
    )
    .expect("accepted");
    let credited_at = scene.app.world.resource::<core_sim::SimulationTick>().0;
    let credited = goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE);
    assert!(
        credited.gifts > 0.0,
        "the gift is credited the moment it is accepted"
    );
    assert!(
        scene.app.world.resource::<KnownGoodwill>().is_empty(),
        "but the sender cannot know: the party is still at the far camp"
    );

    // Walk home. Until the fold-back, still nothing.
    let mut got_home_at = None;
    for _ in 0..MAX_TURNS_ROUND_TRIP {
        let alive = scene.app.world.get::<Expedition>(party).is_some();
        run_turn(&mut scene.app);
        let known = scene.app.world.resource::<KnownGoodwill>().len();
        if alive && scene.app.world.get::<Expedition>(party).is_some() {
            assert_eq!(known, 0, "still on the road: no news yet");
        }
        if known > 0 {
            got_home_at = Some(scene.app.world.resource::<core_sim::SimulationTick>().0);
            break;
        }
    }
    let got_home_at = got_home_at.expect("the news came home with the party");
    let news = scene
        .app
        .world
        .resource::<KnownGoodwill>()
        .get(SENDER_PEOPLE, RECEIVER_PEOPLE)
        .expect("the sender now holds the receiver's opinion of it");
    assert!(
        news.components.gifts >= credited.gifts - EPSILON,
        "the news carries the gift: {:?} vs {credited:?}",
        news.components
    );
    assert!(
        news.turn >= credited_at && news.turn < got_home_at,
        "stamped with the turn it was seen ({}), not the turn it arrived ({got_home_at})",
        news.turn
    );
    assert!(
        scene
            .app
            .world
            .resource::<KnownGoodwill>()
            .get(RECEIVER_PEOPLE, SENDER_PEOPLE)
            .is_none(),
        "the receiver learned nothing about the sender's opinion of it"
    );
}

// ---------------------------------------------------------------------------------------------
// The wire
// ---------------------------------------------------------------------------------------------

struct PublishedGoodwill {
    other: u32,
    gifts: f32,
    exchanges: f32,
    total: f32,
    reported_turn: u64,
}

fn published_goodwill(app: &mut App, viewer: FactionId) -> (Vec<PublishedGoodwill>, f32, f32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    app.world.insert_resource(ViewerFaction(viewer));
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .goodwill()
        .expect("the goodwill section is published");
    let rows = section
        .goodwill()
        .map(|rows| {
            rows.iter()
                .map(|row| PublishedGoodwill {
                    other: row.otherFaction(),
                    gifts: row.gifts(),
                    exchanges: row.exchanges(),
                    total: row.total(),
                    reported_turn: row.reportedTurn(),
                })
                .collect()
        })
        .unwrap_or_default();
    let cuts = section.cuts().expect("the cuts ride the section");
    (rows, cuts.waryBelow(), cuts.friendlyAbove())
}

/// **The frame carries the viewer's own known rows and the word cuts, and nobody else's.** The cuts
/// are the config's, published once.
#[test]
fn the_frame_carries_the_viewers_known_goodwill_and_the_word_cuts() {
    let mut app = spawn_world();
    let news = GoodwillNews {
        components: GoodwillEntry {
            gifts: 0.3,
            exchanges: 0.1,
            ..Default::default()
        },
        turn: 7,
    };
    app.world
        .resource_mut::<KnownGoodwill>()
        .record(SENDER_PEOPLE, RECEIVER_PEOPLE, news);

    let (rows, wary, friendly) = published_goodwill(&mut app, SENDER_PEOPLE);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].other, RECEIVER_PEOPLE.0);
    assert!((rows[0].gifts - 0.3).abs() < EPSILON);
    assert!((rows[0].exchanges - 0.1).abs() < EPSILON);
    assert!(
        (rows[0].total - 0.4).abs() < EPSILON,
        "the total is the sum of the reasons"
    );
    assert_eq!(rows[0].reported_turn, 7);
    let config = DiplomacyConfig::builtin();
    assert_eq!(
        (wary, friendly),
        (config.goodwill.wary_below, config.goodwill.friendly_above),
        "the client never restates the cuts"
    );

    for other in [RECEIVER_PEOPLE, THIRD_PEOPLE] {
        let (rows, _, _) = published_goodwill(&mut app, other);
        assert!(
            rows.is_empty(),
            "{other:?} is not told what the sender knows"
        );
    }
}

/// The viewer's OWN live rows, off the encoded envelope (`GoodwillSection.own`).
fn published_own_goodwill(app: &mut App, viewer: FactionId) -> Vec<PublishedGoodwill> {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    app.world.insert_resource(ViewerFaction(viewer));
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .goodwill()
        .expect("the goodwill section is published")
        .own()
        .map(|rows| {
            rows.iter()
                .map(|row| PublishedGoodwill {
                    other: row.otherFaction(),
                    gifts: row.gifts(),
                    exchanges: row.exchanges(),
                    total: row.total(),
                    reported_turn: row.reportedTurn(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// **The frame carries the viewer's OWN live opinion of each other people, and only its own.** A
/// people's opinion is its own to know: the AI weighs a proposal on it and a client shows "what we
/// think of them". The rows are live (`reported_turn` is the current turn), never the carried news
/// in `goodwill`, and another people's opinion never rides this viewer's frame.
#[test]
fn the_frame_carries_the_viewers_own_live_opinion_and_nobody_elses() {
    let mut app = spawn_world();
    {
        let mut opinions = app.world.resource_mut::<Goodwill>();
        opinions.credit(SENDER_PEOPLE, RECEIVER_PEOPLE, GoodwillSource::Gift, 0.3);
        opinions.credit(
            SENDER_PEOPLE,
            RECEIVER_PEOPLE,
            GoodwillSource::Exchange,
            0.1,
        );
        opinions.credit(RECEIVER_PEOPLE, SENDER_PEOPLE, GoodwillSource::Gift, -0.15);
    }
    let tick = app.world.resource::<core_sim::SimulationTick>().0;

    let own = published_own_goodwill(&mut app, SENDER_PEOPLE);
    assert_eq!(
        own.len(),
        1,
        "one row per people the viewer holds an entry about"
    );
    assert_eq!(
        own[0].other, RECEIVER_PEOPLE.0,
        "the row names who is thought about"
    );
    assert!((own[0].gifts - 0.3).abs() < EPSILON);
    assert!((own[0].exchanges - 0.1).abs() < EPSILON);
    assert!((own[0].total - 0.4).abs() < EPSILON);
    assert_eq!(own[0].reported_turn, tick, "live, not carried news");

    let theirs = published_own_goodwill(&mut app, RECEIVER_PEOPLE);
    assert_eq!(theirs.len(), 1);
    assert_eq!(theirs[0].other, SENDER_PEOPLE.0);
    assert!(
        (theirs[0].total + 0.15).abs() < EPSILON,
        "each viewer reads its own opinion"
    );

    assert!(
        published_own_goodwill(&mut app, THIRD_PEOPLE).is_empty(),
        "a people holding no opinion publishes no row"
    );
    // The carried-news rows are a different list: nothing was reported to anyone.
    let (known, _, _) = published_goodwill(&mut app, SENDER_PEOPLE);
    assert!(known.is_empty(), "live opinion is not carried news");
}

// ---------------------------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------------------------

/// **Both ledgers ride the checkpoint whole** — the opinions as their reasons, and the known rows
/// with the turn they were reported (a restore must not let an older report win back).
#[test]
fn goodwill_and_known_goodwill_round_trip_a_checkpoint() {
    let mut app = spawn_world();
    {
        let mut goodwill = app.world.resource_mut::<Goodwill>();
        goodwill.credit(
            RECEIVER_PEOPLE,
            SENDER_PEOPLE,
            core_sim::GoodwillSource::Gift,
            0.25,
        );
        goodwill.credit(
            RECEIVER_PEOPLE,
            SENDER_PEOPLE,
            core_sim::GoodwillSource::Stationed,
            0.1,
        );
    }
    app.world.resource_mut::<KnownGoodwill>().record(
        SENDER_PEOPLE,
        RECEIVER_PEOPLE,
        GoodwillNews {
            components: GoodwillEntry {
                gifts: 0.2,
                ..Default::default()
            },
            turn: 11,
        },
    );
    let goodwill_before = app.world.resource::<Goodwill>().clone();
    let known_before = app.world.resource::<KnownGoodwill>().clone();
    let checkpoint = capture_sim_state(&app.world);

    app.world.insert_resource(Goodwill::default());
    app.world.insert_resource(KnownGoodwill::default());
    restore_sim_state(&mut app.world, &checkpoint);

    assert_eq!(*app.world.resource::<Goodwill>(), goodwill_before);
    assert_eq!(*app.world.resource::<KnownGoodwill>(), known_before);
    assert_eq!(
        app.world
            .resource::<KnownGoodwill>()
            .get(SENDER_PEOPLE, RECEIVER_PEOPLE)
            .map(|n| n.turn),
        Some(11)
    );
}
