//! **A stationed party: contact now, news later** (issue #774,
//! `docs/plan_contact_and_logistics.md` "Settled by #774").
//!
//! The launch **gates** (the tie, the borders, the party bound, the refusals leaving the band as it
//! stood) and the recall verb live where the command handlers live, in `core_sim/src/bin/server.rs`'s
//! test module, beside the trade verb's. What is pinned here is what a party does once it is on the
//! map: that its tie rises only as its news lands at home (and a control tie bleeds out beside it), that what it sees reaches
//! the home band a runner's walk behind and not at once, that the home band's larder pays for it, and
//! that each thing that ends a post turns the party for home.
//!
//! Parties are spawned by hand the way `trade_expedition.rs` spawns shipments — the handlers are in a
//! binary these tests cannot link against.
//!
//! **Wire claims are asserted on the encoded envelope**, through the accessor chain a client uses.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::connections::{Connection, ConnectionKey, ConnectionLedger, FULL_TIE, NO_TIE};
use core_sim::{
    build_test_app, run_turn, scalar_from_f32, split_band_from_parent, BandId, BandTravel,
    CommandEventKind, CommandEventLog, ConnectionsConfigHandle, Expedition, ExpeditionMission,
    ExpeditionPhase, FactionId, LaborAllocation, LocalStore, PopulationCohort, ResidentBand,
    Scalar, SettleConfig, Sighting, SimulationConfig, SimulationTick, SnapshotHistory,
    StartingUnit, Tile, TileRegistry, VisibilityLedger, FOOD,
};

mod faction_support;
use faction_support::{two_faction_world, HOME, RIVAL};

/// A pinned earthlike world, so the terrain under every single-faction fixture is the same one every
/// run.
const MAP_SEED: u64 = 119_304_647;
/// Working-age people the home band is stocked with.
const HOME_WORKERS: f32 = 20.0;
/// Workers the splinter bands are founded with.
const SPLIT_WORKERS: u32 = 5;
/// Workers in the stationed party.
const PARTY_WORKERS: u32 = 2;
/// How far the same-faction host is walked from the home band — well past every comm range and sight
/// range, so a report from there is genuinely news.
const OUT_OF_REACH_TILES: u32 = 20;
/// The larder every band is topped up to each turn of a long run, so a post never ends on hunger
/// unless the test says it should.
const LARDER: f32 = 4_000.0;
/// The most turns a fixture waits for a party to arrive.
const ARRIVAL_BUDGET: u32 = 120;
/// Turns a post is held in the long-run tie test.
const LONG_POST_TURNS: u32 = 100;
/// Slack on `f32` sums of `Scalar`-quantised amounts.
const EPSILON: f32 = 0.001;
/// A tech factor wide enough that the effective comm range covers any distance on the map.
const BOUNDLESS_COMM_FACTOR: f32 = 500.0;
/// The host's name carried on the mission.
const HOST_NAME: &str = "the Reed People";
/// The wire's not-food-limited runway sentinel (`NOT_FOOD_LIMITED_TURNS`).
const NOT_FOOD_LIMITED: f32 = 999.0;

fn spawn_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

fn resident_of(app: &mut App, faction: FactionId) -> Entity {
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
    query
        .iter(&app.world)
        .find(|(_, cohort)| cohort.faction == faction)
        .map(|(entity, _)| entity)
        .expect("the campaign spawns a resident band for this people")
}

fn entity_for_band(app: &mut App, band_id: BandId) -> Option<Entity> {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band_id)
        .map(|(entity, _)| entity)
}

fn band_id(app: &App, band: Entity) -> BandId {
    *app.world.get::<BandId>(band).expect("the band has an id")
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

/// Split a splinter off `parent`. A split is co-located, and forms a full tie each way.
fn split_off(app: &mut App, parent: Entity) -> (Entity, BandId) {
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(parent).expect("band");
        cohort.working = Scalar::from_f32(HOME_WORKERS);
        cohort.sync_size();
    }
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let entity = entity_for_band(app, split.band).expect("the split allocated this id");
    (entity, split.band)
}

/// Stand `band` on `target`, home and all, so it is out of every sight range.
fn walk_to(app: &mut App, band: Entity, target: UVec2) {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(target.x, target.y)
        .expect("the target tile is on the map");
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).expect("band");
    cohort.current_tile = tile;
    cohort.home = tile;
}

fn offset_from(app: &App, from: UVec2, dx: u32) -> UVec2 {
    let registry = app.world.resource::<TileRegistry>();
    UVec2::new((from.x + dx) % registry.width.max(1), from.y)
}

/// **Spawn a stationed party by hand**, the way `handle_station_expedition` builds one: a detached
/// cohort with the `Station` mission, no pack and no cargo, and a travel order for the host's tile —
/// and the party's workers taken off the home band's pool, as the launch does.
fn launch_station(app: &mut App, home: Entity, host: BandId, host_pos: UVec2) -> Entity {
    let mut cohort = {
        let mut home_cohort = app
            .world
            .get_mut::<PopulationCohort>(home)
            .expect("the home band exists");
        home_cohort.working -= Scalar::from_u32(PARTY_WORKERS);
        home_cohort.sync_size();
        home_cohort.clone()
    };
    cohort.working = Scalar::from_u32(PARTY_WORKERS);
    cohort.children = Scalar::zero();
    cohort.elders = Scalar::zero();
    cohort.stores = LocalStore::new();
    cohort.sync_size();
    let party_id = app
        .world
        .resource_mut::<core_sim::BandIdAllocator>()
        .allocate();
    app.world
        .spawn((
            cohort,
            party_id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: home,
                mission: ExpeditionMission::Station {
                    host_band: host,
                    host_name: HOST_NAME.to_string(),
                },
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Expedition),
                cargo: LocalStore::new(),
                defection_pull: Scalar::zero(),
                station_reports: Vec::new(),
                goodwill_news: Default::default(),
                aim: Some(host_pos),
            },
            BandTravel::to(host_pos),
        ))
        .id()
}

/// A larder small enough that the band's runway is a real number, not the sentinel.
const SHORT_LARDER: f32 = 40.0;

fn set_larder(app: &mut App, band: Entity, amount: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .reset_food("dry", scalar_from_f32(amount));
}

/// A band's published `turnsOfFood`, off the encoded envelope.
fn published_runway(app: &App, band: BandId) -> f32 {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published")
        .iter()
        .find(|cohort| cohort.bandId() == band.0)
        .expect("the band's row is published")
        .turnsOfFood()
}

fn feed(app: &mut App, band: Entity) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists")
        .stores
        .reset_food("dry", scalar_from_f32(LARDER));
}

/// Hold a band at a steady head-count, so a long run on a far tile cannot lose it to the weather.
fn pin_people(app: &mut App, band: Entity) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).expect("band");
    cohort.working = Scalar::from_f32(SPLIT_WORKERS as f32);
    cohort.children = Scalar::zero();
    cohort.elders = Scalar::zero();
    cohort.sync_size();
}

fn phase(app: &App, party: Entity) -> Option<ExpeditionPhase> {
    app.world.get::<Expedition>(party).map(|e| e.phase)
}

fn edge(app: &App, from: BandId, to: BandId) -> Option<Connection> {
    app.world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(from, to))
        .cloned()
}

fn tick(app: &App) -> u64 {
    app.world.resource::<SimulationTick>().0
}

/// Seed `from → to` at full strength, as a splinter's kinship does.
fn seed_full_tie(app: &mut App, from: BandId, to: BandId, at: UVec2) {
    let turn = tick(app);
    app.world
        .resource_mut::<ConnectionLedger>()
        .insert_full_tie(
            ConnectionKey::new(from, to),
            &Sighting::new(at, turn, HOST_NAME),
            turn,
        );
}

/// Drive turns, topping every listed band up each time, until the party is `Stationed`.
fn run_until_stationed(app: &mut App, party: Entity, fed: &[Entity]) -> u32 {
    for turn in 0..ARRIVAL_BUDGET {
        if phase(app, party) == Some(ExpeditionPhase::Stationed) {
            return turn;
        }
        for band in fed {
            feed(app, *band);
        }
        run_turn(app);
    }
    panic!("the party never reached its host in {ARRIVAL_BUDGET} turns");
}

fn latest_recall_detail(app: &App) -> Option<String> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| matches!(entry.kind, CommandEventKind::ExpeditionRecalled))
        .filter_map(|entry| entry.detail.clone())
        .last()
}

/// A same-people host walked far from the home band, tie seeded by the split.
struct Rig {
    app: App,
    home: Entity,
    home_id: BandId,
    host: Entity,
    host_id: BandId,
    party: Entity,
}

fn same_faction_rig() -> Rig {
    let mut app = spawn_world();
    let home = resident_of(&mut app, HOME);
    let (host, host_id) = split_off(&mut app, home);
    let home_pos = band_position(&app, home);
    let far = offset_from(&app, home_pos, OUT_OF_REACH_TILES);
    walk_to(&mut app, host, far);
    pin_people(&mut app, host);
    let home_id = band_id(&app, home);
    let party = launch_station(&mut app, home, host_id, far);
    Rig {
        app,
        home,
        home_id,
        host,
        host_id,
        party,
    }
}

/// A real RIVAL band as the host: another people, far from the home band, with a full tie seeded
/// toward it (a stand-in for the meeting that would have formed one).
fn cross_faction_rig() -> Rig {
    let mut app = two_faction_world();
    let home = resident_of(&mut app, HOME);
    let host = resident_of(&mut app, RIVAL);
    let (home_id, host_id) = (band_id(&app, home), band_id(&app, host));
    let host_pos = band_position(&app, host);
    seed_full_tie(&mut app, home_id, host_id, host_pos);
    let party = launch_station(&mut app, home, host_id, host_pos);
    Rig {
        app,
        home,
        home_id,
        host,
        host_id,
        party,
    }
}

// ---------------------------------------------------------------------------------------------
// The tie: raised only by news that has reached home
// ---------------------------------------------------------------------------------------------

/// A RIVAL host with a FRESH tie toward it — the one meeting's `gain_per_contact`, not a splinter's
/// kinship — so the tie has room to rise and every rise is visible.
fn cross_faction_rig_with_a_fresh_tie() -> Rig {
    let mut app = two_faction_world();
    let home = resident_of(&mut app, HOME);
    let host = resident_of(&mut app, RIVAL);
    let (home_id, host_id) = (band_id(&app, home), band_id(&app, host));
    let host_pos = band_position(&app, host);
    let turn = tick(&app);
    let cfg = app.world.resource::<ConnectionsConfigHandle>().get();
    app.world.resource_mut::<ConnectionLedger>().record_contact(
        ConnectionKey::new(home_id, host_id),
        &Sighting::new(host_pos, turn, HOST_NAME),
        turn,
        &cfg,
    );
    // The formation turn is itself a contact turn and so is exempt from decay; run it out so the
    // tie enters the post on the ordinary clock.
    run_turn(&mut app);
    let party = launch_station(&mut app, home, host_id, host_pos);
    Rig {
        app,
        home,
        home_id,
        host,
        host_id,
        party,
    }
}

/// **A post raises its tie only as its news lands at home.** A far post's tie keeps fading after the
/// party arrives, for as long as the first runner is walking; it then climbs by exactly one
/// `gain_per_contact` per landed report and, once full, holds there while a report lands every turn.
/// An unposted control edge beside it bleeds out to nothing.
///
/// Every turn is classified by the ledger's own stamp: a turn on which `last_contact_turn` is that
/// turn is a landed report, and it is the only kind of turn on which strength may rise.
#[test]
fn a_posted_tie_rises_only_as_reports_land_and_then_holds_at_full() {
    let mut rig = cross_faction_rig_with_a_fresh_tie();
    let (home, host, home_id, host_id, party) =
        (rig.home, rig.host, rig.home_id, rig.host_id, rig.party);
    let (control, control_id) = split_off(&mut rig.app, home);
    let control_pos = offset_from(
        &rig.app,
        band_position(&rig.app, home),
        OUT_OF_REACH_TILES * 2,
    );
    walk_to(&mut rig.app, control, control_pos);
    pin_people(&mut rig.app, control);
    let strength_cfg = rig
        .app
        .world
        .resource::<ConnectionsConfigHandle>()
        .get()
        .strength
        .clone();
    let gain = Scalar::from_f32(strength_cfg.gain_per_contact);
    let drain = Scalar::from_f32(strength_cfg.decay_per_turn);

    let mut previous = edge(&rig.app, home_id, host_id)
        .expect("the fresh tie")
        .strength;
    let mut stationed_without_news = 0u32;
    let mut landed_while_rising = 0u32;
    let mut first_seen_turn = None;
    let mut reached_full = false;
    let mut parked_before_rising = false;
    let mut post_turns = 0u32;
    for _ in 0..ARRIVAL_BUDGET {
        if post_turns >= LONG_POST_TURNS {
            break;
        }
        for band in [home, host, control] {
            feed(&mut rig.app, band);
        }
        pin_people(&mut rig.app, control);
        let was_stationed = phase(&rig.app, party) == Some(ExpeditionPhase::Stationed);
        run_turn(&mut rig.app);
        let ran = tick(&rig.app) - 1;
        let tie = edge(&rig.app, home_id, host_id).expect("a posted tie is never reaped");
        let report_landed = tie.last_contact_turn == ran;
        if report_landed {
            first_seen_turn.get_or_insert(tie.last_seen_turn);
            assert_eq!(
                tie.strength,
                (previous + gain).min(FULL_TIE),
                "turn {ran}: a landed report adds exactly one contact's gain, capped at full"
            );
            if previous < FULL_TIE {
                landed_while_rising += 1;
            }
        } else {
            assert_eq!(
                tie.strength,
                (previous - drain).max(NO_TIE),
                "turn {ran}: with no news landing the tie only decays, posted or not"
            );
            if was_stationed {
                stationed_without_news += 1;
            }
        }
        if was_stationed {
            post_turns += 1;
        }
        parked_before_rising |= tie.strength == NO_TIE && !reached_full;
        if reached_full {
            assert!(
                report_landed && tie.strength == FULL_TIE,
                "turn {ran}: once full, the tie holds there because a report lands every turn"
            );
        }
        reached_full |= tie.strength == FULL_TIE;
        previous = tie.strength;
    }

    assert!(
        stationed_without_news > 0,
        "liveness: the party stood at its host while the first runner was still walking, and the \
         tie did not rise on those turns"
    );
    assert!(
        landed_while_rising > 0,
        "liveness: reports landed while the tie had room to rise"
    );
    assert!(
        first_seen_turn.is_some(),
        "liveness: the home ledger's last_seen_turn advanced off a landed report"
    );
    assert_eq!(
        phase(&rig.app, party),
        Some(ExpeditionPhase::Stationed),
        "the post held for the whole run"
    );
    let posted = edge(&rig.app, home_id, host_id).expect("the posted tie");
    assert_eq!(posted.strength, FULL_TIE, "the post ends the run at full");
    assert!(reached_full, "liveness: the tie did climb to full");
    assert!(
        parked_before_rising,
        "the tie ran down to nothing before the first report landed, and a landing report still \
         raised the parked edge"
    );

    let control_edge = edge(&rig.app, home_id, control_id).expect("the control edge still exists");
    assert_eq!(
        control_edge.strength, NO_TIE,
        "the unposted control tie bled out over the same turns (liveness: decay is running)"
    );
    assert_eq!(
        edge(&rig.app, host_id, home_id),
        None,
        "one-way: the host gains no tie toward the sender from having the party in its camp"
    );
    let party_id = band_id(&rig.app, party);
    let ledger = rig.app.world.resource::<ConnectionLedger>();
    assert!(
        ledger
            .iter()
            .all(|(key, _)| key.subject != party_id && key.observer != party_id),
        "the party is not a contact subject, and no edge names it"
    );
}

/// **Inside the comm range the tie rises from the first turn the party has news.** The report lands
/// the turn it leaves, so there is no runner's walk to fade through: from the first sighting every
/// turn adds one contact's gain until the tie is full, and no turn before it rises.
#[test]
fn a_post_inside_comm_range_raises_its_tie_from_the_first_report() {
    let mut rig = cross_faction_rig_with_a_fresh_tie();
    let (home, host, home_id, host_id, party) =
        (rig.home, rig.host, rig.home_id, rig.host_id, rig.party);
    let mut config = (*core_sim::ExpeditionConfig::builtin()).clone();
    config.comm_range_tech_factor = BOUNDLESS_COMM_FACTOR;
    rig.app
        .world
        .resource_mut::<core_sim::ExpeditionConfigHandle>()
        .replace(std::sync::Arc::new(config));
    let gain = Scalar::from_f32(
        rig.app
            .world
            .resource::<ConnectionsConfigHandle>()
            .get()
            .strength
            .gain_per_contact,
    );

    let mut previous = edge(&rig.app, home_id, host_id)
        .expect("the fresh tie")
        .strength;
    let mut rises = 0u32;
    for _ in 0..ARRIVAL_BUDGET {
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
        let ran = tick(&rig.app) - 1;
        let tie = edge(&rig.app, home_id, host_id).expect("the posted tie");
        if tie.last_contact_turn == ran {
            assert_eq!(
                tie.last_seen_turn, ran,
                "turn {ran}: a report inside the comm range was observed the turn it landed"
            );
            assert_eq!(tie.strength, (previous + gain).min(FULL_TIE), "turn {ran}");
            rises += 1;
        } else {
            assert!(
                tie.strength <= previous,
                "turn {ran}: the tie never rises on a turn no report lands"
            );
        }
        previous = tie.strength;
        if previous == FULL_TIE {
            break;
        }
    }
    assert!(rises > 0, "liveness: reports landed");
    assert_eq!(
        previous, FULL_TIE,
        "the tie climbed to full on landed reports"
    );
    assert_eq!(phase(&rig.app, party), Some(ExpeditionPhase::Stationed));
}

// ---------------------------------------------------------------------------------------------
// The news: a runner's walk behind
// ---------------------------------------------------------------------------------------------

/// **Report lag is exactly the walk.** With the host beyond the comm range, the home ledger's
/// `last_seen_turn` trails the turn the news landed by `walk_turns(distance)`, while
/// `last_contact_turn` is the current turn. The ground the party sees reaches the home faction as
/// `Discovered`, never `Active`.
#[test]
fn a_far_hosts_news_arrives_a_runners_walk_behind() {
    let mut rig = same_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    let host_pos = band_position(&rig.app, host);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    // It stops walking only on the host's own tile; the runner's distance is measured from there.
    for _ in 0..ARRIVAL_BUDGET {
        if band_position(&rig.app, party) == host_pos {
            break;
        }
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }
    assert_eq!(band_position(&rig.app, party), host_pos);

    let home_pos = band_position(&rig.app, home);
    let comm = core_sim::ExpeditionConfig::builtin().effective_comm_range();
    let tiles_per_turn = rig
        .app
        .world
        .resource::<core_sim::LaborConfigHandle>()
        .get()
        .band_move_tiles_per_turn;
    let (width, wrap) = {
        let registry = rig.app.world.resource::<TileRegistry>();
        let wrap = rig
            .app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        (registry.width, wrap)
    };
    let party_pos = band_position(&rig.app, party);
    let distance = core_sim::grid_utils::hex_distance_wrapped(party_pos, home_pos, width, wrap);
    assert!(
        distance > comm,
        "the fixture puts the host beyond the comm range"
    );
    let delay = u64::from(core_sim::work_party::walk_turns(distance, tiles_per_turn));
    assert!(delay > 0, "liveness: the walk takes real turns");

    // Long enough that every report in the pipeline was dispatched from the stationary post.
    for _ in 0..(delay + 4) {
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }

    let tie = edge(&rig.app, rig.home_id, rig.host_id).expect("the posted tie");
    let now = tick(&rig.app) - 1;
    assert_eq!(
        tie.last_contact_turn, now,
        "a report lands every turn once the pipeline is full"
    );
    assert_eq!(
        tie.last_seen_turn + delay,
        now,
        "what the home band knows of the host is a runner's walk old: seen {} on turn {now}, walk {delay}",
        tie.last_seen_turn
    );
    assert_eq!(
        tie.last_seen_position, host_pos,
        "and it is where the host stood when the party saw it"
    );
    assert!(
        !rig.app
            .world
            .get::<Expedition>(party)
            .expect("the party")
            .station_reports
            .is_empty(),
        "liveness: reports are in transit"
    );
}

/// **A report grants `Discovered`, never `Active`.** Another people's camp: the home faction has no
/// live eyes there (the host's own sight is the rival's), so once the party's news lands the ground is
/// remembered and not watched.
#[test]
fn a_landed_report_is_memory_not_presence() {
    let mut rig = cross_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    let host_pos = band_position(&rig.app, host);
    assert!(
        !rig.app
            .world
            .resource::<VisibilityLedger>()
            .is_discovered(HOME, host_pos.x, host_pos.y),
        "the rival's camp is unknown ground to the home faction at the start"
    );
    run_until_stationed(&mut rig.app, party, &[home, host]);
    for _ in 0..ARRIVAL_BUDGET {
        if band_position(&rig.app, party) == host_pos {
            break;
        }
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }
    // Long enough for the first report from the host's own tile to walk home.
    for _ in 0..ARRIVAL_BUDGET {
        if rig
            .app
            .world
            .resource::<VisibilityLedger>()
            .is_discovered(HOME, host_pos.x, host_pos.y)
        {
            break;
        }
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }
    let ledger = rig.app.world.resource::<VisibilityLedger>();
    assert!(
        ledger.is_discovered(HOME, host_pos.x, host_pos.y),
        "the ground the party saw is on the home faction's map once its report lands"
    );
    assert!(
        !ledger.is_visible(HOME, host_pos.x, host_pos.y),
        "Discovered, never Active: a report grants memory, not presence"
    );
}

/// **Inside the comm range a report lands the turn it leaves** — the scouts' rule. Widening the
/// effective range (the tech hook) reaches the same instant zone from any distance: nothing stays in
/// transit and the sighting's observed turn is the contact turn.
#[test]
fn a_report_inside_the_comm_range_lands_the_turn_it_leaves() {
    let mut rig = same_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    let mut config = (*core_sim::ExpeditionConfig::builtin()).clone();
    config.comm_range_tech_factor = BOUNDLESS_COMM_FACTOR;
    rig.app
        .world
        .resource_mut::<core_sim::ExpeditionConfigHandle>()
        .replace(std::sync::Arc::new(config));

    run_until_stationed(&mut rig.app, party, &[home, host]);
    // The party starts inside the (now boundless) range, so it is `Stationed` at once; give it the
    // walk to the host so it actually sees the band.
    let host_pos = band_position(&rig.app, host);
    for _ in 0..ARRIVAL_BUDGET {
        if band_position(&rig.app, party) == host_pos {
            break;
        }
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }
    assert_eq!(band_position(&rig.app, party), host_pos);
    for _ in 0..3 {
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }

    let tie = edge(&rig.app, rig.home_id, rig.host_id).expect("the posted tie");
    assert_eq!(
        tie.last_seen_turn, tie.last_contact_turn,
        "no lag inside the comm range"
    );
    assert_eq!(tie.last_contact_turn, tick(&rig.app) - 1);
    assert!(
        rig.app
            .world
            .get::<Expedition>(party)
            .expect("the party")
            .station_reports
            .is_empty(),
        "nothing waits in transit"
    );
}

/// **The fold-back lands every report still in transit** — the party carries its own news home.
#[test]
fn the_fold_back_lands_the_reports_still_in_transit() {
    let mut rig = same_faction_rig();
    let (home, host_id) = (rig.home, rig.host_id);
    let home_pos = band_position(&rig.app, home);
    // A party standing in its home camp, turned for home, with a report that would not land for a
    // thousand turns.
    let party = launch_station(&mut rig.app, home, host_id, home_pos);
    let (width, height) = {
        let registry = rig.app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let unseen_tile = UVec2::new(width - 1, height - 1);
    let seen_at = UVec2::new(3, 3);
    const SEEN_TURN: u64 = 3;
    const FAR_FUTURE: u64 = 1_000;
    assert!(
        !rig.app.world.resource::<VisibilityLedger>().is_discovered(
            HOME,
            unseen_tile.x,
            unseen_tile.y
        ),
        "the report's tile is news to the home faction"
    );
    let land_turn = tick(&rig.app) + FAR_FUTURE;
    {
        let mut expedition = rig.app.world.get_mut::<Expedition>(party).expect("party");
        expedition.phase = ExpeditionPhase::Returning;
        let mut sightings = std::collections::BTreeMap::new();
        sightings.insert(host_id, Sighting::new(seen_at, SEEN_TURN, HOST_NAME));
        expedition.station_reports.push(core_sim::StationReport {
            land_turn,
            tiles: vec![unseen_tile],
            sightings,
            goodwill: Default::default(),
        });
    }
    feed(&mut rig.app, home);
    run_turn(&mut rig.app);

    assert!(
        rig.app.world.get::<Expedition>(party).is_none(),
        "the party folded back"
    );
    assert!(
        rig.app.world.resource::<VisibilityLedger>().is_discovered(
            HOME,
            unseen_tile.x,
            unseen_tile.y
        ),
        "its in-transit ground landed with it"
    );
    let tie = edge(&rig.app, rig.home_id, host_id).expect("the tie");
    assert_eq!(
        (tie.last_seen_turn, tie.last_seen_position),
        (SEEN_TURN, seen_at),
        "the observed turn and place ride the report into clock 1"
    );
}

// ---------------------------------------------------------------------------------------------
// What ends a post
// ---------------------------------------------------------------------------------------------

/// **The host's people closing their borders ends a cross-faction post**, with its reason and the
/// host's id on the feed line.
#[test]
fn a_host_closing_its_borders_ends_a_cross_faction_post() {
    let mut rig = cross_faction_rig();
    let (home, host, party, host_id) = (rig.home, rig.host, rig.party, rig.host_id);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    rig.app
        .world
        .resource_mut::<core_sim::FactionBorderPolicies>()
        .set_open(RIVAL, false);
    feed(&mut rig.app, home);
    run_turn(&mut rig.app);

    assert_eq!(phase(&rig.app, party), Some(ExpeditionPhase::Returning));
    let detail = latest_recall_detail(&rig.app).expect("the post's end is on the feed");
    assert!(
        detail.contains("reason=borders_closed") && detail.contains(&format!("host={}", host_id.0)),
        "the line names why and which host: {detail}"
    );
}

/// **A host of the sender's own people never asks Open Borders**: closing the home people's borders
/// leaves a same-people post standing — and the paired control: the same closure against another
/// people's host ended it above.
#[test]
fn a_same_people_host_never_asks_open_borders() {
    let mut rig = same_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    rig.app
        .world
        .resource_mut::<core_sim::FactionBorderPolicies>()
        .set_open(HOME, false);
    for _ in 0..5 {
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        pin_people(&mut rig.app, host);
        run_turn(&mut rig.app);
        assert_eq!(
            phase(&rig.app, party),
            Some(ExpeditionPhase::Stationed),
            "own people's borders are not a border to their own party"
        );
    }
}

/// **A home band that cannot cover its meal, party included, does NOT end the post.** The party
/// stays and keeps its tie; the shortfall is the band's, borne through its ordinary meal.
#[test]
fn a_starving_home_band_keeps_its_party_stationed() {
    let mut rig = same_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    for _ in 0..5 {
        set_larder(&mut rig.app, home, 0.0);
        feed(&mut rig.app, host);
        pin_people(&mut rig.app, host);
        run_turn(&mut rig.app);

        let cohort = rig.app.world.get::<PopulationCohort>(home).expect("home");
        assert!(
            cohort.last_food_need > cohort.last_food_consumption,
            "liveness: the band is short this turn"
        );
        assert_eq!(cohort.stores.get(FOOD), Scalar::zero());
        assert_eq!(phase(&rig.app, party), Some(ExpeditionPhase::Stationed));
        assert!(
            edge(&rig.app, rig.home_id, rig.host_id).is_some(),
            "and the tie to the host is still held"
        );
    }
}

/// **A host that is no longer a band ends the post.**
#[test]
fn a_host_that_is_gone_ends_the_post() {
    let mut rig = same_faction_rig();
    let (home, host, party, host_id) = (rig.home, rig.host, rig.party, rig.host_id);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    rig.app.world.despawn(host);
    feed(&mut rig.app, home);
    run_turn(&mut rig.app);

    assert_eq!(phase(&rig.app, party), Some(ExpeditionPhase::Returning));
    let detail = latest_recall_detail(&rig.app).expect("the post's end is on the feed");
    assert!(
        detail.contains("reason=host_gone") && detail.contains(&format!("host={}", host_id.0)),
        "{detail}"
    );
}

/// **A home band that no longer exists ends the post.** The party has nobody to feed it or take its
/// reports, so it turns for home on its own reason and folds where it stands.
#[test]
fn a_home_band_that_is_gone_ends_the_post_and_the_party_leaves_the_map() {
    let mut rig = same_faction_rig();
    let (home, host, party, host_id) = (rig.home, rig.host, rig.party, rig.host_id);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    rig.app.world.despawn(home);
    feed(&mut rig.app, host);
    run_turn(&mut rig.app);

    let detail = latest_recall_detail(&rig.app).expect("the post's end is on the feed");
    assert!(
        detail.contains("reason=home_gone") && detail.contains(&format!("host={}", host_id.0)),
        "{detail}"
    );
    // The post turns `Returning` and, with no home to walk to, folds where it stands the same turn.
    assert!(
        rig.app.world.get_entity(party).is_none(),
        "the party folds where it stands and leaves the map"
    );
}

// ---------------------------------------------------------------------------------------------
// The home band feeds its party
// ---------------------------------------------------------------------------------------------

/// **The party's rations are part of the home band's ordinary meal**, charged at the same ration the
/// band's own workers eat, and the larder identity still closes in fixed point.
///
/// Measured against a twin world whose band has the same people and no party: the difference in what
/// the band ate is exactly the party's ration (non-zero — the liveness half), and the larder dropped
/// by exactly what the band reports having eaten.
#[test]
fn the_home_band_eats_for_its_party_and_the_larder_identity_closes() {
    let ration = {
        let app = spawn_world();
        let cfg = app
            .world
            .resource::<core_sim::DemographicsConfigHandle>()
            .get();
        cfg.consumption.per_capita_draw * cfg.consumption.working_factor * PARTY_WORKERS as f32
    };

    // The twin: the same band holding the same people, none of them out.
    let mut control = spawn_world();
    let control_home = resident_of(&mut control, HOME);
    {
        let mut cohort = control
            .world
            .get_mut::<PopulationCohort>(control_home)
            .expect("home");
        cohort.working = Scalar::from_f32(HOME_WORKERS - PARTY_WORKERS as f32);
        cohort.sync_size();
    }
    set_larder(&mut control, control_home, SHORT_LARDER);
    run_turn(&mut control);
    let control_id = band_id(&control, control_home);
    let runway_without_party = published_runway(&control, control_id);
    let without_party = control
        .world
        .get::<PopulationCohort>(control_home)
        .expect("home")
        .last_food_consumption;

    let mut app = spawn_world();
    let home = resident_of(&mut app, HOME);
    {
        let mut cohort = app.world.get_mut::<PopulationCohort>(home).expect("home");
        cohort.working = Scalar::from_f32(HOME_WORKERS);
        cohort.sync_size();
    }
    let host_pos = band_position(&app, home);
    // A real host id is not needed for the meal; the guard turns a host-less party for home AFTER
    // the meal, which is the order this test measures.
    // The party is out for the whole meal (`simulate_population` runs before `advance_expeditions`);
    // with no real host the turn then sends it home, which does not touch the larder.
    launch_station(&mut app, home, BandId(u64::MAX), host_pos);
    set_larder(&mut app, home, SHORT_LARDER);
    let before = app
        .world
        .get::<PopulationCohort>(home)
        .expect("home")
        .stores
        .get(FOOD);
    run_turn(&mut app);
    let cohort = app.world.get::<PopulationCohort>(home).expect("home");
    let after = cohort.stores.get(FOOD);
    let with_party = cohort.last_food_consumption;
    let fed_party = cohort.fed_party_workers;
    let home_id = band_id(&app, home);
    let runway_with_party = published_runway(&app, home_id);

    assert_eq!(
        fed_party,
        Scalar::from_u32(PARTY_WORKERS),
        "the band counted its party's mouths into the turn"
    );
    assert!(
        runway_with_party < runway_without_party,
        "and its published runway counts them too: {runway_with_party} with the party out, \
         {runway_without_party} without"
    );
    assert!(
        (with_party - without_party - ration).abs() < EPSILON,
        "the band ate the party's ration on top of its own: with {with_party}, without \
         {without_party}, ration {ration}"
    );
    assert!(ration > 0.0, "liveness: the charge is non-zero");
    assert_eq!(
        (before - after).to_f32(),
        with_party,
        "larder_delta == -consumption, to the fixed-point unit: nothing else moved the larder"
    );
}

// ---------------------------------------------------------------------------------------------
// Checkpoint and wire
// ---------------------------------------------------------------------------------------------

/// **Reports in transit ride the checkpoint.** The wipe between capture and restore is the liveness
/// half.
#[test]
fn reports_in_transit_survive_a_checkpoint_round_trip() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let mut rig = same_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    run_until_stationed(&mut rig.app, party, &[home, host]);
    for _ in 0..4 {
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }
    let party_band = band_id(&rig.app, party);
    let in_transit = rig
        .app
        .world
        .get::<Expedition>(party)
        .expect("party")
        .station_reports
        .clone();
    assert!(
        in_transit.len() > 1,
        "liveness: several reports are in transit at the checkpoint"
    );
    let checkpoint = capture_sim_state(&rig.app.world);
    rig.app
        .world
        .get_mut::<Expedition>(party)
        .expect("party")
        .station_reports
        .clear();

    restore_sim_state(&mut rig.app.world, &checkpoint);

    let restored = entity_for_band(&mut rig.app, party_band).expect("the party comes back");
    let expedition = rig.app.world.get::<Expedition>(restored).expect("mission");
    assert_eq!(
        expedition.station_reports, in_transit,
        "every report in transit comes back, with its landing turn"
    );
    assert_eq!(expedition.phase, ExpeditionPhase::Stationed);
}

/// **The mission, the host and the phase are on the encoded frame**, and a party its home band feeds
/// does not read as starving.
#[test]
fn the_mission_host_and_phase_are_published() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    fn party_row(app: &App, party_id: BandId) -> (String, String, u64, String, f32) {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let populations = envelope
            .payload_as_snapshot()
            .expect("a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the population section is published");
        let row = populations
            .iter()
            .find(|cohort| cohort.bandId() == party_id.0)
            .expect("the party's own row is published");
        (
            row.expeditionMission().unwrap_or("").to_string(),
            row.expeditionPhase().unwrap_or("").to_string(),
            row.expeditionDestinationBand(),
            row.expeditionDestinationName().unwrap_or("").to_string(),
            row.turnsOfFood(),
        )
    }

    let mut rig = same_faction_rig();
    let (home, host, party, host_id) = (rig.home, rig.host, rig.party, rig.host_id);
    let party_id = band_id(&rig.app, party);
    feed(&mut rig.app, home);
    run_turn(&mut rig.app);
    let (mission, phase_key, bound_for, name, runway) = party_row(&rig.app, party_id);
    assert_eq!(mission, "station");
    assert_eq!(phase_key, "outbound", "still walking to the host");
    assert_eq!(
        bound_for, host_id.0,
        "the host's id rides the destination field"
    );
    assert_eq!(name, HOST_NAME);
    assert_eq!(runway, NOT_FOOD_LIMITED, "its home band feeds it");

    run_until_stationed(&mut rig.app, party, &[home, host]);
    feed(&mut rig.app, home);
    run_turn(&mut rig.app);
    let (_, phase_key, bound_for, _, _) = party_row(&rig.app, party_id);
    assert_eq!(phase_key, "stationed");
    assert_eq!(bound_for, host_id.0);
}

/// **The messenger reach rides every cohort row**, equal to `effective_comm_range()` — the number a
/// client mirrors `station_report_delay` with. Non-zero at the shipped config (liveness).
#[test]
fn the_comm_range_is_published_on_every_cohort_row() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut rig = same_faction_rig();
    let home = rig.home;
    feed(&mut rig.app, home);
    run_turn(&mut rig.app);

    let expected = rig
        .app
        .world
        .resource::<core_sim::ExpeditionConfigHandle>()
        .get()
        .effective_comm_range();
    assert!(expected > 0, "liveness: the shipped reach is non-zero");

    let snapshot = rig
        .app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let populations = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section is published");
    assert!(!populations.is_empty());
    for row in populations.iter() {
        assert_eq!(row.commRangeTiles(), expected);
    }
}

// ---------------------------------------------------------------------------------------------
// A party aims at what its people know
// ---------------------------------------------------------------------------------------------

/// How far from the home band the host was last seen — far enough that the walk takes many turns.
const LAST_SEEN_TILES: u32 = 14;
/// How far the host has moved on since it was last seen, in the "gone out of sight" cases — past
/// every sight range a party has.
const MOVED_FAR_TILES: u32 = 30;
/// How far the host has moved in the control: three tiles, inside the party's sight from the
/// last-seen tile. **The last-seen tile is chosen on open ground** — sight is line-of-sight, and a host
/// behind a ridge is genuinely unseen (at 12 tiles out the host's tile sat up a slope, hidden from
/// two tiles away).
const MOVED_NEAR_TILES: u32 = 3;

/// A RIVAL host last seen `LAST_SEEN_TILES` from the home band (the tie's remembered position, and
/// the party's launch aim), and standing `moved` tiles from there now. Returns the rig and the
/// last-seen tile.
fn stale_host_rig(moved: u32) -> (Rig, UVec2) {
    let mut app = two_faction_world();
    let home = resident_of(&mut app, HOME);
    let host = resident_of(&mut app, RIVAL);
    let (home_id, host_id) = (band_id(&app, home), band_id(&app, host));
    let last_seen = offset_from(&app, band_position(&app, home), LAST_SEEN_TILES);
    let now_at = offset_from(&app, last_seen, moved);
    walk_to(&mut app, host, now_at);
    seed_full_tie(&mut app, home_id, host_id, last_seen);
    let party = launch_station(&mut app, home, host_id, last_seen);
    (
        Rig {
            app,
            home,
            home_id,
            host,
            host_id,
            party,
        },
        last_seen,
    )
}

/// **A party walks to where its people last saw a foreign host, not to where the host really is** —
/// and on standing there with nothing in sight, the trip ends: `Returning`, `host_not_found`.
#[test]
fn a_party_finds_no_sign_of_a_host_that_moved_out_of_sight() {
    let (mut rig, last_seen) = stale_host_rig(MOVED_FAR_TILES);
    let (home, host, party, host_id) = (rig.home, rig.host, rig.party, rig.host_id);
    let host_now = band_position(&rig.app, host);
    assert_ne!(
        host_now, last_seen,
        "liveness: the host really has moved on"
    );

    let mut turns = 0;
    while phase(&rig.app, party) == Some(ExpeditionPhase::Outbound) {
        assert!(turns < ARRIVAL_BUDGET, "the party never gave up looking");
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
        turns += 1;
    }

    assert_eq!(phase(&rig.app, party), Some(ExpeditionPhase::Returning));
    assert_eq!(
        band_position(&rig.app, party),
        last_seen,
        "it walked to the last-seen tile, not toward the host's real position"
    );
    assert!(
        turns > 1,
        "liveness: the walk to the last-seen tile took turns"
    );
    let detail = latest_recall_detail(&rig.app).expect("the trip's end is on the feed");
    assert_eq!(
        detail,
        format!(
            "status=returning reason=host_not_found host={} expedition={}",
            host_id.0,
            party.to_bits()
        )
    );
}

/// **Control: a host that moved only a short way is still in the party's sight from the last-seen
/// spot**, so the party sees it, re-aims, and stations — the not-found end is not just "the host is
/// not on the remembered tile".
#[test]
fn a_party_that_sees_a_host_that_moved_a_little_stations_with_it() {
    let (mut rig, last_seen) = stale_host_rig(MOVED_NEAR_TILES);
    let (home, host, party) = (rig.home, rig.host, rig.party);
    assert_ne!(
        band_position(&rig.app, host),
        last_seen,
        "liveness: the host is not on the remembered tile"
    );
    run_until_stationed(&mut rig.app, party, &[home, host]);
    assert_eq!(
        rig.app
            .world
            .get::<Expedition>(party)
            .and_then(|expedition| expedition.aim),
        Some(band_position(&rig.app, host)),
        "the party re-aimed at the host it saw"
    );
    assert!(
        latest_recall_detail(&rig.app).is_none(),
        "nothing ended the trip"
    );
}

/// **A host of the sender's own people is always known**: the aim follows its live tile with no
/// sight at all, and the party stations with it.
#[test]
fn a_same_people_host_that_moved_is_followed_without_sight() {
    let mut rig = same_faction_rig();
    let (home, host, party) = (rig.home, rig.host, rig.party);
    let launch_aim = rig
        .app
        .world
        .get::<Expedition>(party)
        .and_then(|expedition| expedition.aim)
        .expect("a station party carries an aim");
    let moved_to = offset_from(&rig.app, launch_aim, MOVED_FAR_TILES);
    walk_to(&mut rig.app, host, moved_to);
    pin_people(&mut rig.app, host);

    feed(&mut rig.app, home);
    feed(&mut rig.app, host);
    run_turn(&mut rig.app);
    assert_eq!(
        rig.app
            .world
            .get::<Expedition>(party)
            .and_then(|expedition| expedition.aim),
        Some(moved_to),
        "the aim moved to the host's live tile on the first turn, unseen"
    );
    run_until_stationed(&mut rig.app, party, &[home, host]);
}

/// **The aim rides the checkpoint** with the rest of the `Expedition`.
#[test]
fn an_aim_set_mid_walk_survives_a_checkpoint_round_trip() {
    use core_sim::sim_state::{capture_sim_state, restore_sim_state};

    let (mut rig, last_seen) = stale_host_rig(MOVED_FAR_TILES);
    let (home, host, party) = (rig.home, rig.host, rig.party);
    for _ in 0..3 {
        feed(&mut rig.app, home);
        feed(&mut rig.app, host);
        run_turn(&mut rig.app);
    }
    let party_band = band_id(&rig.app, party);
    assert_eq!(
        phase(&rig.app, party),
        Some(ExpeditionPhase::Outbound),
        "liveness: the party is mid-walk"
    );
    let checkpoint = capture_sim_state(&rig.app.world);
    rig.app
        .world
        .get_mut::<Expedition>(party)
        .expect("party")
        .aim = None;

    restore_sim_state(&mut rig.app.world, &checkpoint);

    let restored = entity_for_band(&mut rig.app, party_band).expect("the party comes back");
    assert_eq!(
        rig.app
            .world
            .get::<Expedition>(restored)
            .and_then(|expedition| expedition.aim),
        Some(last_seen)
    );
}
