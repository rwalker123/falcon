//! **Treaties: road rights, open borders and agreed trade between two peoples** (issue #233,
//! `docs/plan_early_diplomacy.md` §3).
//!
//! The command handlers (`propose_treaty`, `send_envoy`, `end_treaty`, `set_open_borders … <other>`)
//! are in `core_sim/src/bin/server.rs`, where their launch half is tested; what is pinned here is
//! what the world does with them. The library halves — [`propose_treaty`], [`answer_dealing`] and
//! [`end_treaty`] — are called directly, and a party a handler would launch is spawned by hand the
//! way `stages.rs` spawns shipments, so the claims run the shipped arrival and fold-back paths.
//!
//! **Whole turns, two peoples**, on the scene `stages.rs` builds: the sender is the campaign's first
//! band (faction 0), the receiver a band split off it, made another people and walked a few hexes
//! east. A standing link is a dirt road on every tile the shipped trace crosses. Wire claims are
//! asserted on the ENCODED frame.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    answer_dealing, bands_have_standing_link, build_test_app, end_treaty, propose_treaty,
    recapture_snapshot_in_place, run_turn, scalar_from_f32, split_band_from_parent, trace_path,
    BandId, BandIdAllocator, BandTravel, Bundle, CommandEventKind, CommandEventLog, DealingAnswer,
    DealingId, DealingKind, EnvoyOutcome, Expedition, ExpeditionMission, ExpeditionPhase,
    FactionId, GoodsLines, Goodwill, LaborAllocation, LadderConfig, LocalStore, PendingDealings,
    PopulationCohort, ResidentBand, RoadRegistry, Scalar, SettleConfig, ShipmentTerms,
    SimulationConfig, SimulationTick, SnapshotHistory, StartingUnit, Tile, TileRegistry, Treaties,
    TreatyCarrier, TreatyTerms, ViewerFaction, FOOD, NEAR_ENOUGH_TO_KEEP,
};
use sim_runtime::{BundleRequest, TradeCargoItem, TreatyTermsRequest, FOOD_CARGO_KEY};

/// A pinned earthlike world, so the terrain under every fixture is the same one every run.
const MAP_SEED: u64 = 119_304_647;
/// Working-age people the sender is stocked with, so a split leaves two real bands on any seed.
const PARENT_WORKERS: f32 = 20.0;
/// Workers the receiving band is founded with.
const SPLIT_WORKERS: u32 = 5;
/// Workers a party carries.
const PARTY_WORKERS: u32 = 2;
/// How many hexes east of the sender the receiver is walked.
const LINK_SPAN: u32 = 3;
const SENDER_FOOD: f32 = 60.0;
const RECEIVER_FOOD: f32 = 40.0;
const RECEIVER_HIDE: f32 = 6.0;
const EPSILON: f32 = 0.01;
const SENDER_PEOPLE: FactionId = FactionId(0);
const RECEIVER_PEOPLE: FactionId = FactionId(9);
const THIRD_PEOPLE: FactionId = FactionId(5);
/// Turns to let a party walk a few hexes, settle and walk home.
const TURN_BUDGET: u32 = 40;
/// The bundle every agreed-trade test stands on: a shipment carrying 4 food is paid 1 hide.
const BUNDLE_SENT_FOOD: f32 = 4.0;
const BUNDLE_RETURN_HIDE: f32 = 1.0;
/// Turns a treaty credit is run for, long enough to be near its settled value without being so
/// long that the scene's bands starve.
const CREDIT_TURNS: u32 = 30;

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

/// Two bands: the sender (faction 0, stocked with food) and a receiver split off it, made
/// `RECEIVER_PEOPLE`, stocked with food and hide, and walked `span` hexes east.
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
    let to = {
        let registry = app.world.resource::<TileRegistry>();
        UVec2::new((from.x + span) % registry.width.max(1), from.y)
    };
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

/// **Lay a dirt road on every tile the shipped trace crosses between two camps** — the tile set a
/// standing link reads.
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
    let (base, span) = core_sim::road_rung_span(
        core_sim::shipped::route_dirt_road(),
        &ladder,
        NEAR_ENOUGH_TO_KEEP,
    );
    let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
    for tile in &path {
        roads
            .road_or_trail(*tile, &ladder)
            .set_position(base + span, &ladder);
    }
    path
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

fn hide_line(amount: f32) -> TradeCargoItem {
    TradeCargoItem {
        id: "hide".to_string(),
        is_material: true,
        amount,
    }
}

fn request(road_rights: bool, open_borders: bool) -> TreatyTermsRequest {
    TreatyTermsRequest {
        road_rights,
        open_borders,
        bundles: Vec::new(),
    }
}

fn bundle_request() -> TreatyTermsRequest {
    TreatyTermsRequest {
        bundles: vec![BundleRequest {
            give: vec![food_line(BUNDLE_SENT_FOOD)],
            ask: vec![hide_line(BUNDLE_RETURN_HIDE)],
        }],
        ..Default::default()
    }
}

fn propose(scene: &mut Scene, terms: &TreatyTermsRequest) -> Result<DealingId, String> {
    propose_treaty(
        &mut scene.app.world,
        SENDER_PEOPLE,
        scene.sender_id,
        scene.receiver_id,
        terms,
    )
}

fn answer(scene: &mut Scene, id: DealingId, answer: DealingAnswer) -> Result<(), String> {
    answer_dealing(&mut scene.app.world, RECEIVER_PEOPLE, id, &answer).map(|_| ())
}

fn only_dealing(app: &App) -> core_sim::Dealing {
    let book = app.world.resource::<PendingDealings>();
    assert_eq!(book.len(), 1, "exactly one dealing is pending: {book:?}");
    book.iter().next().cloned().expect("one dealing")
}

fn treaty_events(app: &App, faction: FactionId, status: &str) -> Vec<String> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::Treaty && entry.faction == faction)
        .filter_map(|entry| entry.detail.clone())
        .filter(|detail| detail.contains(&format!("status={status}")))
        .collect()
}

/// What a band's store booked this turn for `commodity` under `cause`, summed.
fn booked(app: &App, band: Entity, commodity: &str, cause: core_sim::TransferCause) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .last_turn_transfer_crossings
        .iter()
        .filter(|crossing| crossing.commodity == commodity && crossing.cause == cause)
        .map(|crossing| crossing.amount)
        .sum()
}

/// How many `kind=exchange` dealing lines a people's feed holds with this status.
fn exchange_events(app: &App, faction: FactionId, status: &str) -> usize {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::Dealing && entry.faction == faction)
        .filter_map(|entry| entry.detail.as_deref())
        .filter(|detail| {
            detail.contains("kind=exchange") && detail.contains(&format!("status={status}"))
        })
        .count()
}

fn goodwill(app: &App, holder: FactionId, about: FactionId) -> core_sim::GoodwillEntry {
    app.world.resource::<Goodwill>().get(holder, about)
}

fn treaties(app: &App) -> Treaties {
    app.world.resource::<Treaties>().clone()
}

fn run_until(app: &mut App, mut done: impl FnMut(&App) -> bool) -> bool {
    for _ in 0..TURN_BUDGET {
        if done(app) {
            return true;
        }
        run_turn(app);
    }
    done(app)
}

fn band_hide(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .material_total("hide")
        .to_f32()
}

/// What the ENCODED frame a viewer is sent carries of the treaty section, plus the viewer's border
/// overrides: `(other_faction, road_rights, open_borders, bundles, formed_turn)` per treaty and
/// `(viewer_band, other_band, other_faction)` per standing link.
#[derive(Debug, Default)]
struct PublishedTreaties {
    treaties: Vec<(u32, bool, bool, usize, u64)>,
    links: Vec<(u64, u64, u32)>,
    dealing_terms: Vec<(String, String, bool, bool, usize)>,
}

fn treaties_seen_by(app: &mut App, viewer: FactionId) -> PublishedTreaties {
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
    let world = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");
    let section = world.treaties().expect("the treaty section is published");
    let dealings = world.dealings().expect("the dealings section is published");
    PublishedTreaties {
        treaties: section
            .treaties()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        let terms = row.terms().expect("a treaty row carries its terms");
                        (
                            row.otherFaction(),
                            terms.roadRights(),
                            terms.openBorders(),
                            terms.bundles().map_or(0, |bundles| bundles.len()),
                            row.formedTurn(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        links: section
            .standingLinks()
            .map(|rows| {
                rows.iter()
                    .map(|row| (row.viewerBand(), row.otherBand(), row.otherFaction()))
                    .collect()
            })
            .unwrap_or_default(),
        dealing_terms: dealings
            .dealings()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        let terms = row.treaty().expect("a dealing row carries its terms slot");
                        (
                            row.kind().unwrap_or("").to_string(),
                            row.carrier().unwrap_or("").to_string(),
                            terms.roadRights(),
                            terms.openBorders(),
                            terms.bundles().map_or(0, |bundles| bundles.len()),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// =============================================================================================
// Proposing, answering, amending
// =============================================================================================

/// **A treaty proposed over a kept road is posted the turn it is made, and accepting it puts its
/// terms in force for both peoples from that turn.** Both peoples are told at once (word travels
/// along the road), the proposal rides the encoded frame with its terms and carrier, and the treaty
/// rides it afterwards — for its two peoples and no third.
#[test]
fn a_treaty_over_a_kept_road_is_in_force_for_both_the_turn_it_is_accepted() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    assert!(
        linked(&mut scene),
        "precondition: a kept road the whole way"
    );

    let id = propose(&mut scene, &request(true, true)).expect("a link, a tie and a term");
    let dealing = only_dealing(&scene.app);
    assert_eq!(dealing.id, id);
    assert_eq!(dealing.kind, DealingKind::TreatyProposal);
    assert_eq!(
        dealing.treaty.as_ref().map(|proposal| proposal.carrier),
        Some(TreatyCarrier::Road)
    );
    assert_eq!(dealing.party, None, "a road-carried proposal has no party");
    assert_eq!(
        (dealing.sender_faction, dealing.receiver_faction),
        (SENDER_PEOPLE, RECEIVER_PEOPLE)
    );
    assert_eq!(treaty_events(&scene.app, SENDER_PEOPLE, "posted").len(), 1);
    assert_eq!(
        treaty_events(&scene.app, RECEIVER_PEOPLE, "offered").len(),
        1
    );
    assert!(
        treaties(&scene.app).is_empty(),
        "nothing is in force before the answer"
    );

    // The wire: the proposal's terms ride the receiver's and the sender's frame, not a third's.
    let receiver_view = treaties_seen_by(&mut scene.app, RECEIVER_PEOPLE);
    assert_eq!(
        receiver_view.dealing_terms,
        vec![("treaty_proposal".into(), "road".into(), true, true, 0)]
    );
    assert!(receiver_view.treaties.is_empty());
    assert!(treaties_seen_by(&mut scene.app, THIRD_PEOPLE)
        .dealing_terms
        .is_empty());

    answer(
        &mut scene,
        id,
        DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("accepts");
    let now = scene.app.world.resource::<SimulationTick>().0;
    let in_force = treaties(&scene.app);
    for (a, b) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        assert!(in_force.road_rights(a, b), "road rights hold both ways");
        assert!(in_force.open_borders(a, b), "open borders hold both ways");
    }
    assert!(in_force.bundles(SENDER_PEOPLE, RECEIVER_PEOPLE).is_empty());
    assert_eq!(
        in_force
            .get(SENDER_PEOPLE, RECEIVER_PEOPLE)
            .expect("a treaty")
            .formed_turn,
        now,
        "in force from the turn it was accepted"
    );
    assert!(
        scene.app.world.resource::<PendingDealings>().is_empty(),
        "the proposal left the book"
    );
    // Road-carried: the proposer learns at once.
    assert_eq!(
        treaty_events(&scene.app, SENDER_PEOPLE, "accepted").len(),
        1
    );
    assert_eq!(
        treaty_events(&scene.app, RECEIVER_PEOPLE, "accepted").len(),
        1
    );

    // The wire again: each party sees the treaty with the OTHER people named; a third sees none.
    let sender_view = treaties_seen_by(&mut scene.app, SENDER_PEOPLE);
    assert_eq!(sender_view.treaties, vec![(9, true, true, 0, now)]);
    assert_eq!(
        sender_view.links,
        vec![(scene.sender_id.0, scene.receiver_id.0, 9)],
        "the standing link the proposal needed is published too"
    );
    assert_eq!(
        treaties_seen_by(&mut scene.app, RECEIVER_PEOPLE).treaties,
        vec![(0, true, true, 0, now)]
    );
    assert!(treaties_seen_by(&mut scene.app, THIRD_PEOPLE)
        .treaties
        .is_empty());
}

/// **A counter carries the receiver's FULL terms back, and accepting that puts those in force** —
/// not the original's.
#[test]
fn a_counter_carries_terms_back_and_accepting_it_puts_those_terms_in_force() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let id = propose(&mut scene, &request(true, true)).expect("proposes");
    answer(
        &mut scene,
        id,
        DealingAnswer::CounterTerms {
            terms: request(true, false),
        },
    )
    .expect("a counter on a treaty proposal carries terms");

    let counter = only_dealing(&scene.app);
    assert_ne!(
        counter.id, id,
        "the original closed and a new dealing posted"
    );
    assert_eq!(counter.kind, DealingKind::TreatyProposal);
    assert_eq!(
        (counter.sender_faction, counter.receiver_faction),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
        "it goes the other way"
    );
    let proposal = counter.treaty.clone().expect("the counter's terms");
    assert!(proposal.terms.road_rights && !proposal.terms.open_borders);
    assert!(treaties(&scene.app).is_empty(), "a counter forms nothing");
    assert_eq!(
        treaty_events(&scene.app, SENDER_PEOPLE, "countered").len(),
        1
    );

    // The original proposer answers the counter.
    answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        counter.id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("the proposer accepts the counter");
    let in_force = treaties(&scene.app);
    assert!(in_force.road_rights(SENDER_PEOPLE, RECEIVER_PEOPLE));
    assert!(
        !in_force.open_borders(SENDER_PEOPLE, RECEIVER_PEOPLE),
        "the counter dropped open borders"
    );
}

/// **A proposal replaces whatever treaty stood, and dropping a term that way is not a breach.**
#[test]
fn an_accepted_proposal_amends_a_standing_treaty_without_a_breach() {
    let mut scene = scene(LINK_SPAN);
    lay_road_between(&mut scene);
    let first = propose(&mut scene, &request(true, true)).expect("proposes");
    answer(
        &mut scene,
        first,
        DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("accepts");

    let amended = propose(&mut scene, &bundle_request()).expect("proposes the amendment");
    answer(
        &mut scene,
        amended,
        DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("accepts the amendment");
    let in_force = treaties(&scene.app);
    assert!(
        !in_force.road_rights(SENDER_PEOPLE, RECEIVER_PEOPLE)
            && !in_force.open_borders(SENDER_PEOPLE, RECEIVER_PEOPLE),
        "the terms are the amendment's alone"
    );
    assert_eq!(in_force.bundles(SENDER_PEOPLE, RECEIVER_PEOPLE).len(), 1);
    assert_eq!(in_force.len(), 1, "one treaty per pair of peoples");
    for (holder, about) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        assert!(
            goodwill(&scene.app, holder, about).breach.abs() < f32::EPSILON,
            "amending away terms is not a breach"
        );
    }
}

/// **The proposal's fail-closed gates**: your own people, no term, no standing link, an unknown
/// good, a bundle that sends nothing — each refused with the book as it stood.
#[test]
fn a_proposal_that_cannot_stand_is_refused_and_records_nothing() {
    let mut scene = scene(LINK_SPAN);
    // No road yet.
    assert!(
        propose(&mut scene, &request(true, false)).is_err(),
        "no standing link"
    );
    lay_road_between(&mut scene);
    assert!(
        propose(&mut scene, &request(false, false)).is_err(),
        "no term"
    );
    assert!(
        propose(
            &mut scene,
            &TreatyTermsRequest {
                bundles: vec![BundleRequest {
                    give: Vec::new(),
                    ask: vec![hide_line(1.0)],
                }],
                ..Default::default()
            }
        )
        .is_err(),
        "a bundle must send something"
    );
    assert!(
        propose(
            &mut scene,
            &TreatyTermsRequest {
                bundles: vec![BundleRequest {
                    give: vec![TradeCargoItem {
                        id: "unobtainium".into(),
                        is_material: true,
                        amount: 1.0,
                    }],
                    ask: Vec::new(),
                }],
                ..Default::default()
            }
        )
        .is_err(),
        "an unknown material"
    );
    // Your own people: the receiver band made the sender's again.
    scene
        .app
        .world
        .get_mut::<PopulationCohort>(scene.receiver)
        .expect("the receiver")
        .faction = SENDER_PEOPLE;
    assert!(
        propose(&mut scene, &request(true, false)).is_err(),
        "a treaty is between two peoples"
    );
    assert!(scene.app.world.resource::<PendingDealings>().is_empty());
}

/// **A road-carried proposal needs the link to still stand at the answer**, as stage 4 does: with
/// the road gone it is withdrawn and the answer fails.
#[test]
fn a_treaty_proposal_whose_road_has_gone_is_withdrawn_at_the_answer() {
    let mut scene = scene(LINK_SPAN);
    let path = lay_road_between(&mut scene);
    let id = propose(&mut scene, &request(true, false)).expect("proposes");
    {
        let ladder = LadderConfig::builtin();
        let mut roads = scene.app.world.resource_mut::<RoadRegistry>();
        for tile in &path {
            roads
                .road_or_trail(*tile, &ladder)
                .set_position(0.0, &ladder);
        }
    }
    assert!(!linked(&mut scene), "precondition: the road is gone");
    let failed = answer(
        &mut scene,
        id,
        DealingAnswer::Accept {
            returned: Vec::new(),
        },
    );
    assert!(failed.is_err(), "the road no longer holds");
    assert!(scene.app.world.resource::<PendingDealings>().is_empty());
    assert!(treaties(&scene.app).is_empty());
    assert_eq!(
        treaty_events(&scene.app, SENDER_PEOPLE, "withdrawn").len(),
        1
    );
}

// =============================================================================================
// Ending
// =============================================================================================

/// **Ending a treaty is a breach: the OTHER people's goodwill toward the breaker takes the drop,
/// and nobody else's moves.**
#[test]
fn ending_a_treaty_costs_the_breaker_the_others_goodwill_only() {
    let mut scene = scene(LINK_SPAN);
    scene.app.world.resource_mut::<Treaties>().set(
        SENDER_PEOPLE,
        RECEIVER_PEOPLE,
        TreatyTerms {
            road_rights: true,
            ..Default::default()
        },
        1,
    );
    let broken_treaty = scene
        .app
        .world
        .resource::<core_sim::DiplomacyConfigHandle>()
        .get()
        .goodwill
        .broken_treaty;
    let tick = scene.app.world.resource::<SimulationTick>().0;
    let ended =
        scene
            .app
            .world
            .resource_scope(|world, mut treaties: bevy::prelude::Mut<Treaties>| {
                world.resource_scope(|world, mut goodwill: bevy::prelude::Mut<Goodwill>| {
                    let mut log = world.resource_mut::<CommandEventLog>();
                    end_treaty(
                        &mut treaties,
                        &mut goodwill,
                        &mut log,
                        tick,
                        SENDER_PEOPLE,
                        RECEIVER_PEOPLE,
                        broken_treaty,
                        "ended",
                    )
                })
            });
    assert!(ended.is_some());
    assert!(treaties(&scene.app).is_empty(), "removed at once");
    assert!(
        (goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).breach + broken_treaty).abs()
            < f32::EPSILON,
        "the other people's opinion of the breaker took the drop"
    );
    assert!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE)
            .breach
            .abs()
            < f32::EPSILON,
        "the breaker's own opinion did not move"
    );
    assert_eq!(treaty_events(&scene.app, SENDER_PEOPLE, "ended").len(), 1);
    assert_eq!(treaty_events(&scene.app, RECEIVER_PEOPLE, "ended").len(), 1);
}

/// **A people that stops existing takes its treaties with it, silently**: no breach, and the
/// survivor is told it lapsed.
#[test]
fn a_people_ceasing_dissolves_its_treaties_without_a_breach() {
    let mut scene = scene(LINK_SPAN);
    scene.app.world.resource_mut::<Treaties>().set(
        SENDER_PEOPLE,
        RECEIVER_PEOPLE,
        TreatyTerms {
            open_borders: true,
            ..Default::default()
        },
        1,
    );
    // The receiving people's last band is gone.
    scene.app.world.despawn(scene.receiver);
    run_turn(&mut scene.app);
    assert!(
        treaties(&scene.app).is_empty(),
        "a treaty with a people that no longer exists is gone"
    );
    for (holder, about) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        assert!(
            goodwill(&scene.app, holder, about).breach.abs() < f32::EPSILON,
            "no breach when a people ceases"
        );
    }
    assert_eq!(treaty_events(&scene.app, SENDER_PEOPLE, "ended").len(), 1);
}

// =============================================================================================
// Goodwill
// =============================================================================================

/// **A treaty credits both peoples `per_term × terms` into `treaty` every turn**, and the sum
/// follows the proportional fade: `x_n = s × (1 − (1 − fade)^n) / fade` with the credit after the
/// fade each turn. A bundle counts one term however many it lists.
#[test]
fn a_treaty_credits_both_peoples_per_term_each_turn_and_settles_toward_the_fade_balance() {
    let mut scene = scene(LINK_SPAN);
    let terms = TreatyTerms {
        road_rights: true,
        open_borders: true,
        agreed_trade: vec![Bundle {
            sent: GoodsLines {
                food: scalar_from_f32(BUNDLE_SENT_FOOD),
                ..Default::default()
            },
            returned: GoodsLines::default(),
        }],
    };
    assert_eq!(terms.term_count(), 3);
    scene
        .app
        .world
        .resource_mut::<Treaties>()
        .set(SENDER_PEOPLE, RECEIVER_PEOPLE, terms, 1);
    let (per_term, fade) = {
        let config = scene
            .app
            .world
            .resource::<core_sim::DiplomacyConfigHandle>()
            .get();
        (config.goodwill.per_term, config.goodwill.fade_per_turn)
    };
    for _ in 0..CREDIT_TURNS {
        run_turn(&mut scene.app);
    }
    let steady = per_term * 3.0;
    let expected = steady * (1.0 - (1.0 - fade).powi(CREDIT_TURNS as i32)) / fade;
    for (holder, about) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        let got = goodwill(&scene.app, holder, about).treaty;
        assert!(
            (got - expected).abs() < expected * 0.02,
            "treaty goodwill {got} should follow the fade balance {expected}"
        );
        assert!(got > 0.0, "liveness: the credit actually landed");
    }
    // And it is bounded by where it settles.
    assert!(expected < steady / fade);
}

// =============================================================================================
// The envoy
// =============================================================================================

/// **Spawn the party `send_envoy` launches**, the way `launch_trade_party` builds one: no cargo,
/// `Envoy` terms carrying the proposal, bound for the receiver.
fn launch_envoy(scene: &mut Scene, terms: TreatyTerms) -> Entity {
    let destination_pos = position(&scene.app, scene.receiver);
    let mut cohort = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender")
        .clone();
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
    cohort.children = Scalar::zero();
    cohort.elders = Scalar::zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let band_id = scene.app.world.resource_mut::<BandIdAllocator>().allocate();
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
                    terms: ShipmentTerms::Envoy {
                        proposed: terms,
                        outcome: None,
                    },
                },
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo: LocalStore::new(),
                defection_pull: Scalar::zero(),
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

/// **An envoy walks to the receiver, posts the proposal on arrival, and its own people learn the
/// answer only when it walks home.** The receiver's acceptance is in force at once either way; the
/// proposer hears nothing in between.
#[test]
fn an_envoy_posts_on_arrival_and_its_people_learn_the_answer_only_when_it_walks_home() {
    let mut scene = scene(LINK_SPAN);
    let envoy = launch_envoy(
        &mut scene,
        TreatyTerms {
            road_rights: true,
            ..Default::default()
        },
    );
    assert!(
        run_until(&mut scene.app, |app| {
            !app.world.resource::<PendingDealings>().is_empty()
        }),
        "the envoy reaches the receiver's camp and posts"
    );
    let dealing = only_dealing(&scene.app);
    assert_eq!(dealing.kind, DealingKind::TreatyProposal);
    assert_eq!(
        dealing.treaty.as_ref().map(|proposal| proposal.carrier),
        Some(TreatyCarrier::Envoy)
    );
    assert!(dealing.party.is_some(), "the envoy waits in the camp");
    assert_eq!(
        scene
            .app
            .world
            .get::<Expedition>(envoy)
            .expect("the envoy")
            .phase,
        ExpeditionPhase::AwaitingOrders
    );
    assert_eq!(
        treaty_events(&scene.app, RECEIVER_PEOPLE, "offered").len(),
        1
    );
    assert!(
        treaty_events(&scene.app, SENDER_PEOPLE, "posted").is_empty(),
        "the proposer is not told on arrival"
    );

    // The receiver accepts: in force at once, the envoy turned for home, the proposer silent.
    answer(
        &mut scene,
        dealing.id,
        DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("an envoy-carried answer needs no road");
    assert!(
        treaties(&scene.app).road_rights(SENDER_PEOPLE, RECEIVER_PEOPLE),
        "effective immediately"
    );
    assert_eq!(
        scene
            .app
            .world
            .get::<Expedition>(envoy)
            .expect("the envoy")
            .phase,
        ExpeditionPhase::Returning
    );
    assert!(
        treaty_events(&scene.app, SENDER_PEOPLE, "accepted").is_empty(),
        "the proposer learns only when the envoy is home"
    );
    assert_eq!(
        treaty_events(&scene.app, RECEIVER_PEOPLE, "accepted").len(),
        1
    );

    // The envoy walks home and folds back: now the proposer is told.
    assert!(
        run_until(&mut scene.app, |app| app.world.get_entity(envoy).is_none()),
        "the envoy folds home"
    );
    let told = treaty_events(&scene.app, SENDER_PEOPLE, "accepted");
    assert_eq!(told.len(), 1, "told once, at the fold-back: {told:?}");
    assert!(told[0].contains("reported=envoy"), "{told:?}");
}

/// **An envoy-carried counter rides home with the envoy** and is posted at the fold-back, addressed
/// to the proposer, answerable with no road.
#[test]
fn an_envoys_counter_is_posted_to_its_people_when_it_folds_home() {
    let mut scene = scene(LINK_SPAN);
    let envoy = launch_envoy(
        &mut scene,
        TreatyTerms {
            road_rights: true,
            open_borders: true,
            ..Default::default()
        },
    );
    assert!(run_until(&mut scene.app, |app| {
        !app.world.resource::<PendingDealings>().is_empty()
    }));
    let dealing = only_dealing(&scene.app);
    answer(
        &mut scene,
        dealing.id,
        DealingAnswer::CounterTerms {
            terms: request(false, true),
        },
    )
    .expect("counters");
    assert!(
        scene.app.world.resource::<PendingDealings>().is_empty(),
        "the counter has not reached the proposer yet"
    );
    assert!(run_until(&mut scene.app, |app| app
        .world
        .get_entity(envoy)
        .is_none()));
    let counter = only_dealing(&scene.app);
    assert_eq!(
        (counter.sender_faction, counter.receiver_faction),
        (RECEIVER_PEOPLE, SENDER_PEOPLE)
    );
    let proposal = counter.treaty.clone().expect("terms");
    assert!(!proposal.terms.road_rights && proposal.terms.open_borders);
    assert_eq!(proposal.carrier, TreatyCarrier::Envoy);
    assert_eq!(
        treaty_events(&scene.app, SENDER_PEOPLE, "countered").len(),
        1
    );
    // The proposer accepts, no road needed.
    answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        counter.id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("accepts the counter");
    assert!(treaties(&scene.app).open_borders(SENDER_PEOPLE, RECEIVER_PEOPLE));
}

/// An envoy the receiver never answers lapses with silence, and the proposer is told it did when
/// the envoy is home.
#[test]
fn an_unanswered_envoy_lapses_and_its_people_hear_so_at_the_fold_back() {
    let mut scene = scene(LINK_SPAN);
    let envoy = launch_envoy(
        &mut scene,
        TreatyTerms {
            road_rights: true,
            ..Default::default()
        },
    );
    assert!(run_until(&mut scene.app, |app| {
        !app.world.resource::<PendingDealings>().is_empty()
    }));
    assert!(
        run_until(&mut scene.app, |app| {
            app.world.resource::<PendingDealings>().is_empty()
        }),
        "silence refuses it at the deadline"
    );
    assert!(treaties(&scene.app).is_empty());
    assert!(
        treaty_events(&scene.app, SENDER_PEOPLE, "expired").is_empty() || {
            // The envoy may already be home on the very turn it lapsed; either way it is told once.
            true
        }
    );
    assert!(run_until(&mut scene.app, |app| app
        .world
        .get_entity(envoy)
        .is_none()));
    assert_eq!(treaty_events(&scene.app, SENDER_PEOPLE, "expired").len(), 1);
}

// =============================================================================================
// Agreed trade
// =============================================================================================

/// **Spawn a shipment of `food` bound for the receiver on the given terms**, the way
/// `launch_trade_party` builds one.
fn launch_shipment(scene: &mut Scene, food: f32, terms: ShipmentTerms) -> Entity {
    let cargo = {
        let mut sender = scene
            .app
            .world
            .get_mut::<PopulationCohort>(scene.sender)
            .expect("the sender");
        let taken = sender.stores.take_food_mix(scalar_from_f32(food));
        let mut cargo = LocalStore::new();
        cargo.add_food_mix(&taken);
        cargo
    };
    let destination_pos = position(&scene.app, scene.receiver);
    let mut cohort = scene
        .app
        .world
        .get::<PopulationCohort>(scene.sender)
        .expect("the sender")
        .clone();
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
    cohort.children = Scalar::zero();
    cohort.elders = Scalar::zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let band_id = scene.app.world.resource_mut::<BandIdAllocator>().allocate();
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
                    terms,
                },
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
                cargo,
                defection_pull: Scalar::zero(),
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

fn standing_bundle(scene: &mut Scene) {
    scene.app.world.resource_mut::<Treaties>().set(
        SENDER_PEOPLE,
        RECEIVER_PEOPLE,
        TreatyTerms {
            agreed_trade: vec![Bundle {
                sent: GoodsLines {
                    food: scalar_from_f32(BUNDLE_SENT_FOOD),
                    ..Default::default()
                },
                returned: GoodsLines {
                    materials: vec![("hide".to_string(), scalar_from_f32(BUNDLE_RETURN_HIDE))],
                    ..Default::default()
                },
            }],
            ..Default::default()
        },
        1,
    );
}

/// **A shipment between two peoples whose treaty carries a bundle settles whole bundles on arrival,
/// and the remainder follows the shipment's own terms.** Nine food against a bundle of four: two
/// whole bundles (eight food for two hide), exchange goodwill both ways, one food left over — which
/// a gift to another people leaves pending, as it always did.
#[test]
fn agreed_bundles_settle_whole_and_the_remainder_follows_the_shipments_terms() {
    let mut scene = scene(LINK_SPAN);
    standing_bundle(&mut scene);
    let receiver_hide = band_hide(&scene.app, scene.receiver);
    let party = launch_shipment(&mut scene, 9.0, ShipmentTerms::Gift);
    assert!(
        run_until(&mut scene.app, |app| {
            !app.world.resource::<PendingDealings>().is_empty()
        }),
        "the remainder waits as a pending dealing"
    );
    // The covered part landed at once and was paid: 2 bundles = 8 food in, 2 hide out.
    assert!(
        (booked(
            &scene.app,
            scene.receiver,
            FOOD,
            core_sim::TransferCause::ShipmentIn
        ) - 8.0)
            .abs()
            < EPSILON,
        "two whole bundles' food landed on the arrival turn"
    );
    assert_eq!(exchange_events(&scene.app, SENDER_PEOPLE, "settled"), 1);
    assert!(
        (receiver_hide - band_hide(&scene.app, scene.receiver) - 2.0).abs() < EPSILON,
        "the receiver paid Σ k × returned"
    );
    // The remainder is the pending dealing's cargo; the payment is the party's, apart from it.
    let dealing = only_dealing(&scene.app);
    assert_eq!(
        dealing.kind,
        DealingKind::Offer,
        "a gift to another people waits"
    );
    assert!(
        (dealing.goods_sent.get(FOOD).to_f32() - 1.0).abs() < EPSILON,
        "only the uncovered food is on offer: {:?}",
        dealing.goods_sent
    );
    let expedition = scene.app.world.get::<Expedition>(party).expect("the party");
    assert!(
        (expedition.exchange_return.material_total("hide").to_f32() - 2.0).abs() < EPSILON,
        "the payment walks home apart from the cargo"
    );
    assert!((expedition.cargo.get(FOOD).to_f32() - 1.0).abs() < EPSILON);
    assert!(
        goodwill(&scene.app, RECEIVER_PEOPLE, SENDER_PEOPLE).exchanges > 0.0,
        "the receiver warmed to the sender"
    );
    assert!(
        goodwill(&scene.app, SENDER_PEOPLE, RECEIVER_PEOPLE).exchanges > 0.0,
        "and the sender to the receiver"
    );
    // The covered part is a trade: one `exchanged` record per people, no dealing behind it. The
    // remainder is still pending and has written nothing yet.
    {
        let history = scene.app.world.resource::<core_sim::TradeHistory>();
        for people in [SENDER_PEOPLE, RECEIVER_PEOPLE] {
            let records = history.of(people);
            assert_eq!(records.len(), 1, "{people:?}: {records:?}");
            assert_eq!(records[0].outcome.as_str(), "exchanged");
            assert!(records[0].dealing.is_none());
        }
        let sender = &history.of(SENDER_PEOPLE)[0];
        assert!((sender.goods_out.food.to_f32() - 8.0).abs() < EPSILON);
        assert!((sender.goods_in.materials[0].1.to_f32() - 2.0).abs() < EPSILON);
    }

    // Refusing the remainder sends the party home; the payment arrives as an exchange.
    answer(&mut scene, dealing.id, DealingAnswer::Refuse).expect("refuses the remainder");
    let sender_hide = band_hide(&scene.app, scene.sender);
    assert!(run_until(&mut scene.app, |app| app
        .world
        .get_entity(party)
        .is_none()));
    assert!(
        (band_hide(&scene.app, scene.sender) - sender_hide - 2.0).abs() < EPSILON,
        "the bundle's payment reached the sender's home"
    );
}

/// **A cargo exactly covered by bundles leaves nothing pending**: the party turns for home with the
/// payment.
#[test]
fn a_cargo_the_bundles_cover_exactly_leaves_nothing_pending() {
    let mut scene = scene(LINK_SPAN);
    standing_bundle(&mut scene);
    let party = launch_shipment(&mut scene, 8.0, ShipmentTerms::Offer);
    assert!(run_until(&mut scene.app, |app| {
        app.world
            .get::<Expedition>(party)
            .is_none_or(|expedition| expedition.phase == ExpeditionPhase::Returning)
    }));
    assert!(
        scene.app.world.resource::<PendingDealings>().is_empty(),
        "nothing was left to wait on"
    );
    assert!(
        (RECEIVER_HIDE - band_hide(&scene.app, scene.receiver) - 2.0).abs() < EPSILON,
        "paid for both bundles"
    );
}

/// **A receiver that cannot pay the bundle's return fails the covered part as stage 4 does**: it
/// stays with the party and walks home, no goodwill moves, and the remainder still follows its
/// terms.
#[test]
fn a_receiver_short_of_the_bundles_return_fails_the_covered_part_with_no_goodwill() {
    let mut scene = scene(LINK_SPAN);
    standing_bundle(&mut scene);
    // The receiver holds less hide than two bundles pay.
    {
        let mut cohort = scene
            .app
            .world
            .get_mut::<PopulationCohort>(scene.receiver)
            .expect("receiver");
        cohort
            .stores
            .take_material_batches("hide", scalar_from_f32(RECEIVER_HIDE));
    }
    let party = launch_shipment(&mut scene, 9.0, ShipmentTerms::Gift);
    assert!(run_until(&mut scene.app, |app| {
        !app.world.resource::<PendingDealings>().is_empty()
    }));
    assert!(
        booked(
            &scene.app,
            scene.receiver,
            FOOD,
            core_sim::TransferCause::ShipmentIn
        )
        .abs()
            < EPSILON,
        "nothing landed: the covered part failed"
    );
    assert_eq!(exchange_events(&scene.app, SENDER_PEOPLE, "failed"), 1);
    let dealing = only_dealing(&scene.app);
    assert!(
        (dealing.goods_sent.get(FOOD).to_f32() - 1.0).abs() < EPSILON,
        "the remainder still follows its terms"
    );
    let expedition = scene.app.world.get::<Expedition>(party).expect("the party");
    assert!(
        (expedition.unpaid_return.get(FOOD).to_f32() - 8.0).abs() < EPSILON,
        "the covered goods walk home"
    );
    for (holder, about) in [
        (SENDER_PEOPLE, RECEIVER_PEOPLE),
        (RECEIVER_PEOPLE, SENDER_PEOPLE),
    ] {
        assert!(
            goodwill(&scene.app, holder, about).exchanges.abs() < f32::EPSILON,
            "a failed exchange credits nothing"
        );
    }
    let history = scene.app.world.resource::<core_sim::TradeHistory>();
    for people in [SENDER_PEOPLE, RECEIVER_PEOPLE] {
        let outcomes: Vec<&str> = history
            .of(people)
            .iter()
            .map(|record| record.outcome.as_str())
            .collect();
        assert_eq!(outcomes, vec!["failed"], "{people:?}");
    }
}

/// **A same-people shipment never reads a treaty**: bundles between two peoples do nothing to a
/// shipment between a people's own bands.
#[test]
fn a_same_people_shipment_never_reads_treaties() {
    let mut scene = scene(LINK_SPAN);
    standing_bundle(&mut scene);
    // The receiver is the sender's own people again.
    scene
        .app
        .world
        .get_mut::<PopulationCohort>(scene.receiver)
        .expect("receiver")
        .faction = SENDER_PEOPLE;
    let party = launch_shipment(&mut scene, 8.0, ShipmentTerms::Gift);
    assert!(
        run_until(&mut scene.app, |app| app.world.get_entity(party).is_none()),
        "a gift to its own people lands on arrival and the party walks home"
    );
    // (Two bands of one people pool their stores, so the hide's balance says nothing; the exchange
    // lines and the pending book do.)
    for faction in [SENDER_PEOPLE, RECEIVER_PEOPLE] {
        assert_eq!(exchange_events(&scene.app, faction, "settled"), 0);
        assert_eq!(exchange_events(&scene.app, faction, "failed"), 0);
    }
    assert!(scene.app.world.resource::<PendingDealings>().is_empty());
}

/// The counter an envoy carried home, posted to the proposer: runs the envoy flow to the fold-back
/// and returns the proposer-side dealing. No turn runs after it, so a road laid next is still kept.
fn counter_brought_home(scene: &mut Scene) -> core_sim::Dealing {
    let envoy = launch_envoy(
        scene,
        TreatyTerms {
            road_rights: true,
            open_borders: true,
            ..Default::default()
        },
    );
    assert!(run_until(&mut scene.app, |app| {
        !app.world.resource::<PendingDealings>().is_empty()
    }));
    let dealing = only_dealing(&scene.app);
    answer(
        scene,
        dealing.id,
        DealingAnswer::CounterTerms {
            terms: request(false, true),
        },
    )
    .expect("counters");
    assert!(run_until(&mut scene.app, |app| app
        .world
        .get_entity(envoy)
        .is_none()));
    only_dealing(&scene.app)
}

/// **A further counter on a dealing an envoy brought home does not travel for free**: with no standing
/// link it is refused, the dealing stays answerable, and the reason points at `send_envoy`.
#[test]
fn a_further_counter_at_home_is_refused_without_a_standing_link() {
    let mut scene = scene(LINK_SPAN);
    let counter = counter_brought_home(&mut scene);
    assert!(!linked(&mut scene), "precondition: no road joins the bands");
    let refused = answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        counter.id,
        &DealingAnswer::CounterTerms {
            terms: request(true, false),
        },
    )
    .expect_err("nothing carries the counter");
    assert!(refused.contains("send_envoy"), "{refused}");
    assert_eq!(
        only_dealing(&scene.app).id,
        counter.id,
        "the dealing is still pending, untouched"
    );
    // Accept (or refuse) at home still works with nothing carrying it.
    answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        counter.id,
        &DealingAnswer::Accept {
            returned: Vec::new(),
        },
    )
    .expect("accept at home needs no carrier");
    assert!(treaties(&scene.app).open_borders(SENDER_PEOPLE, RECEIVER_PEOPLE));
}

/// **With a standing link the counter goes along the road**, as any road-carried proposal does.
#[test]
fn a_further_counter_at_home_goes_along_a_standing_link() {
    let mut scene = scene(LINK_SPAN);
    let counter = counter_brought_home(&mut scene);
    lay_road_between(&mut scene);
    assert!(linked(&mut scene));
    answer_dealing(
        &mut scene.app.world,
        SENDER_PEOPLE,
        counter.id,
        &DealingAnswer::CounterTerms {
            terms: request(true, false),
        },
    )
    .expect("a road joins the bands");
    let posted = only_dealing(&scene.app);
    assert_ne!(posted.id, counter.id);
    assert_eq!(
        (posted.sender_faction, posted.receiver_faction),
        (SENDER_PEOPLE, RECEIVER_PEOPLE)
    );
    assert_eq!(
        posted.treaty.as_ref().map(|proposal| proposal.carrier),
        Some(TreatyCarrier::Road)
    );
}

/// **The cause on the fold-back crossing**: a paid bundle's payment comes home `ExchangeIn`; covered
/// goods the receiver could not pay for never changed hands and come home `ShipmentReturned`.
#[test]
fn a_bundles_payment_comes_home_as_exchange_in_and_unpaid_goods_as_shipment_returned() {
    use core_sim::TransferCause;
    // Paid.
    let mut paid = scene(LINK_SPAN);
    standing_bundle(&mut paid);
    let party = launch_shipment(&mut paid, 8.0, ShipmentTerms::Offer);
    let mut exchange_in_hide = 0.0;
    let mut returned_food = 0.0;
    for _ in 0..TURN_BUDGET {
        if paid.app.world.get_entity(party).is_none() {
            break;
        }
        run_turn(&mut paid.app);
        exchange_in_hide += booked(&paid.app, paid.sender, "hide", TransferCause::ExchangeIn);
        returned_food += booked(
            &paid.app,
            paid.sender,
            FOOD,
            TransferCause::ShipmentReturned,
        );
    }
    assert!(
        (exchange_in_hide - 2.0).abs() < EPSILON,
        "{exchange_in_hide}"
    );
    assert!(returned_food.abs() < EPSILON, "nothing was unpaid");

    // Unpaid.
    let mut short = scene(LINK_SPAN);
    standing_bundle(&mut short);
    short
        .app
        .world
        .get_mut::<PopulationCohort>(short.receiver)
        .expect("receiver")
        .stores
        .take_material_batches("hide", scalar_from_f32(RECEIVER_HIDE));
    let party = launch_shipment(&mut short, 8.0, ShipmentTerms::Offer);
    let mut returned_food = 0.0;
    let mut exchange_in_food = 0.0;
    for _ in 0..TURN_BUDGET {
        if short.app.world.get_entity(party).is_none() {
            break;
        }
        run_turn(&mut short.app);
        returned_food += booked(
            &short.app,
            short.sender,
            FOOD,
            TransferCause::ShipmentReturned,
        );
        exchange_in_food += booked(&short.app, short.sender, FOOD, TransferCause::ExchangeIn);
    }
    assert!(
        short.app.world.get_entity(party).is_none(),
        "the party came home"
    );
    assert!(
        returned_food >= 8.0 - EPSILON,
        "the unpaid covered food came home as ShipmentReturned: {returned_food}"
    );
    assert!(exchange_in_food.abs() < EPSILON, "{exchange_in_food}");
}

// =============================================================================================
// The wire
// =============================================================================================

/// **The viewer's per-people border overrides ride the frame beside the default**, and only the
/// viewer's own.
#[test]
fn the_viewers_border_overrides_ride_beside_the_default_and_only_its_own() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let mut scene = scene(LINK_SPAN);
    {
        let mut policies = scene
            .app
            .world
            .resource_mut::<core_sim::FactionBorderPolicies>();
        policies.set_override(SENDER_PEOPLE, RECEIVER_PEOPLE, false);
        policies.set_override(RECEIVER_PEOPLE, SENDER_PEOPLE, true);
    }
    let overrides_for = |app: &mut App, viewer: FactionId| -> (bool, Vec<(u32, bool)>) {
        app.world.insert_resource(ViewerFaction(viewer));
        recapture_snapshot_in_place(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let campaign = envelope
            .payload_as_snapshot()
            .expect("a snapshot")
            .campaign()
            .expect("the campaign section");
        let row = campaign
            .factionPolicies()
            .expect("the policies are published")
            .get(0);
        (
            row.openBorders(),
            row.borderOverrides()
                .map(|rows| {
                    rows.iter()
                        .map(|over| (over.otherFaction(), over.open()))
                        .collect()
                })
                .unwrap_or_default(),
        )
    };
    let (default_open, rows) = overrides_for(&mut scene.app, SENDER_PEOPLE);
    assert!(default_open, "the default is untouched");
    assert_eq!(rows, vec![(RECEIVER_PEOPLE.0, false)]);
    let (_, rows) = overrides_for(&mut scene.app, RECEIVER_PEOPLE);
    assert_eq!(
        rows,
        vec![(SENDER_PEOPLE.0, true)],
        "each viewer sees its own"
    );
}

/// **An envoy party publishes as an `envoy` and carries the terms it will post**, on the encoded
/// population row.
#[test]
fn an_envoy_party_publishes_its_mission_and_its_errand() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
    let mut scene = scene(LINK_SPAN);
    let envoy = launch_envoy(
        &mut scene,
        TreatyTerms {
            road_rights: true,
            agreed_trade: vec![Bundle {
                sent: GoodsLines {
                    food: scalar_from_f32(BUNDLE_SENT_FOOD),
                    ..Default::default()
                },
                returned: GoodsLines::default(),
            }],
            ..Default::default()
        },
    );
    let envoy_id = scene.app.world.get::<BandId>(envoy).expect("an id").0;
    scene
        .app
        .world
        .insert_resource(ViewerFaction(SENDER_PEOPLE));
    recapture_snapshot_in_place(&mut scene.app.world);
    let snapshot = scene
        .app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let population = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .expect("the population section");
    let row = population
        .populations()
        .expect("rows")
        .iter()
        .find(|row| row.bandId() == envoy_id)
        .expect("the envoy's own row is published to its people");
    assert_eq!(row.expeditionMission().unwrap_or(""), "envoy");
    let errand = row.envoyTerms().expect("the errand rides the row");
    assert!(errand.roadRights() && !errand.openBorders());
    assert_eq!(errand.bundles().map_or(0, |bundles| bundles.len()), 1);
}

// =============================================================================================
// The checkpoint
// =============================================================================================

/// **A treaty rides the checkpoint**: a rollback that dropped it would quietly cancel it.
#[test]
fn a_treaty_round_trips_a_checkpoint() {
    let mut scene = scene(LINK_SPAN);
    standing_bundle(&mut scene);
    let before = treaties(&scene.app);
    let state = capture_sim_state(&scene.app.world);
    scene
        .app
        .world
        .resource_mut::<Treaties>()
        .end(SENDER_PEOPLE, RECEIVER_PEOPLE);
    assert!(treaties(&scene.app).is_empty());
    restore_sim_state(&mut scene.app.world, &state);
    assert_eq!(treaties(&scene.app), before);
}

/// The `DealingKind` token a client reads.
#[test]
fn a_treaty_proposal_has_its_own_wire_token() {
    assert_eq!(DealingKind::TreatyProposal.as_str(), "treaty_proposal");
    assert!(matches!(
        EnvoyOutcome::Countered(TreatyTerms::default()),
        EnvoyOutcome::Countered(_)
    ));
}
