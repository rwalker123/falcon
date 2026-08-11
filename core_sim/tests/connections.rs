//! **The connection primitive, driven by real turns** (issue #538,
//! `docs/plan_contact_and_logistics.md` §Q1–Q3, §Q6).
//!
//! The arithmetic of the three clocks is unit-tested inside `core_sim::connections`. What is pinned
//! here is the half a unit test cannot see: that contact is actually **found** by the sight sweep,
//! that what it forms reaches the **wire**, and that a connection grants no visibility.
//!
//! **A dead field cannot diverge**, so every assertion below is paired with a liveness one: a test
//! that only checked decay arithmetic would keep passing if contact detection silently stopped
//! firing, and a test that only checked "no extra `Active` tiles" would pass on a sim with no
//! connections at all.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    build_headless_app, split_band_from_parent, BandId, Connection, ConnectionKey,
    ConnectionLedger, ConnectionsConfig, ConnectionsConfigHandle, Expedition, ExpeditionMission,
    ExpeditionPhase, FactionId, LaborAllocation, PopulationCohort, ResidentBand, Scalar,
    SettleConfig, SimulationConfig, SimulationMetrics, SimulationTick, SnapshotHistory,
    StartingUnit, Tile, TileRegistry, ViewerFaction, VisibilityLedger, VisibilityState,
};

/// A pinned earthlike world, so the terrain under every fixture is the same one every run.
const MAP_SEED: u64 = 119_304_647;

/// Workers to hand the second band. Comfortably over `settle.min_founding_workers` and small
/// enough that the parent clears `parent_min_workers` on any seed.
const SPLIT_WORKERS: u32 = 5;

/// Working-age people the parent is stocked with before splitting, so both halves are real bands.
const PARENT_WORKERS: f32 = 20.0;

/// How far the "far away" fixtures put a band from home, in tiles. Well past any band's sight
/// (the widest configured base range is 6, plus an elevation bonus capped at 4) and past the
/// expedition comm range of 2.
const OUT_OF_SIGHT_TILES: u32 = 24;

fn spawn_world() -> App {
    let mut app = build_headless_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// The campaign's first resident band: entity, id, faction and the tile it stands on.
fn first_band(app: &mut App) -> (Entity, BandId, FactionId, UVec2) {
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
    let id = *app.world.get::<BandId>(entity).expect("a band has an id");
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    (entity, id, faction, position)
}

fn entity_for_band(app: &mut App, band_id: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band_id)
        .map(|(entity, _)| entity)
        .expect("the split allocated this id")
}

/// Give the parent enough people that a split leaves two viable bands on any seed.
fn stock(app: &mut App, band: Entity) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.working = Scalar::from_f32(PARENT_WORKERS);
    cohort.sync_size();
}

/// Split a second band off `parent`. A split is **co-located**, which is exactly the fixture the
/// contact primitive wants: two resident bands standing where each can see the other.
fn split_off(app: &mut App, parent: Entity) -> (Entity, BandId) {
    stock(app, parent);
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &settle)
        .expect("a stocked parent can split");
    let entity = entity_for_band(app, split.band);
    (entity, split.band)
}

/// Move `band` to a land tile `OUT_OF_SIGHT_TILES` away along x (wrapping into range if the map
/// edge is nearer), so nothing at `from` can see it.
fn walk_away(app: &mut App, band: Entity, from: UVec2) -> UVec2 {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let target = UVec2::new(
        (from.x + OUT_OF_SIGHT_TILES) % width.max(1),
        from.y.min(height.saturating_sub(1)),
    );
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(target.x, target.y)
        .expect("the target tile is on the map");
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band exists");
    cohort.current_tile = tile;
    cohort.home = tile;
    target
}

fn ledger(app: &App) -> &ConnectionLedger {
    app.world.resource::<ConnectionLedger>()
}

fn edge(app: &App, observer: BandId, subject: BandId) -> Option<Connection> {
    ledger(app)
        .get(&ConnectionKey::new(observer, subject))
        .copied()
}

/// The `connections` section read off the **encoded envelope**, through the accessor chain a client
/// would use. A field that never reached the codec still passes an in-process assertion.
fn published_connections(app: &App) -> Vec<(u64, u64, f32, u32, u32)> {
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
    let section = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .connections()
        .and_then(|section| section.connections())
        .expect("the connection section is published");
    section
        .iter()
        .map(|row| {
            (
                row.observerBandId(),
                row.subjectBandId(),
                row.strength(),
                row.lastSeenX(),
                row.lastSeenY(),
            )
        })
        .collect()
}

fn config(app: &App) -> std::sync::Arc<ConnectionsConfig> {
    app.world.resource::<ConnectionsConfigHandle>().get()
}

// ---------------------------------------------------------------------------------------------
// Contact is FOUND, and what it forms reaches the wire
// ---------------------------------------------------------------------------------------------

/// **The liveness test.** Two resident bands standing where each can see the other form ties, those
/// ties are in the ledger, and the viewer's own edges are on the **published wire section**.
///
/// Asserted on the encoded envelope rather than the in-process ledger, because a section that never
/// reached the codec — or a viewer filter that dropped everything — is invisible to a ledger check.
#[test]
fn two_bands_in_sight_of_each_other_form_ties_that_reach_the_wire() {
    let mut app = spawn_world();
    let (parent, parent_id, faction, _) = first_band(&mut app);
    let (_, child_id) = split_off(&mut app, parent);
    app.world.insert_resource(ViewerFaction(faction));

    app.update();

    assert!(
        !ledger(&app).is_empty(),
        "co-located resident bands must find each other"
    );
    let forward = edge(&app, parent_id, child_id).expect("the parent found the band beside it");
    let reverse = edge(&app, child_id, parent_id).expect("and it found the parent");
    assert!(forward.strength > Scalar::zero());
    assert!(reverse.strength > Scalar::zero());

    let published = published_connections(&app);
    assert!(
        !published.is_empty(),
        "the viewer's own ties must reach the wire section"
    );
    assert!(
        published.iter().any(
            |(observer, subject, strength, _, _)| *observer == parent_id.0
                && *subject == child_id.0
                && *strength > 0.0
        ),
        "the parent's tie to the band beside it is published, at a real strength: {published:?}"
    );

    let metrics = app.world.resource::<SimulationMetrics>();
    assert_eq!(metrics.connections_live as usize, ledger(&app).len());
    assert!(
        metrics.connections_formed > 0,
        "the turn that formed the ties must count them"
    );
}

/// **A tie climbs over consecutive turns of contact**, and stops at a full tie.
#[test]
fn a_tie_climbs_while_the_bands_stay_in_sight() {
    let mut app = spawn_world();
    let (parent, parent_id, _, _) = first_band(&mut app);
    let (_, child_id) = split_off(&mut app, parent);

    app.update();
    let first = edge(&app, parent_id, child_id)
        .expect("the tie forms on the first turn of contact")
        .strength;
    app.update();
    let second = edge(&app, parent_id, child_id)
        .expect("the tie survives")
        .strength;

    assert!(
        second > first,
        "a second turn of contact raises the tie: {first:?} -> {second:?}"
    );
    // Four turns of contact at the shipped gain is a full tie; run well past it and check the cap.
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(
        edge(&app, parent_id, child_id)
            .expect("the tie survives")
            .strength,
        core_sim::FULL_TIE,
        "strength is a 0..=1 fraction and cannot climb past a full tie"
    );
}

/// **Direction: A can know B without B knowing A.**
///
/// The asymmetry is built by taking the `StartingUnit` marker off the second band, which is what
/// `calculate_visibility` requires of a *vision source*. It stays a `ResidentBand`, so it is still a
/// legal **subject** — the scout on the ridge watching a settlement that has no idea anyone is
/// there, with the roles reversed.
#[test]
fn a_band_that_cannot_see_forms_no_edge_of_its_own() {
    let mut app = spawn_world();
    let (parent, parent_id, _, _) = first_band(&mut app);
    let (child, child_id) = split_off(&mut app, parent);
    app.world.entity_mut(child).remove::<StartingUnit>();

    app.update();

    assert!(
        edge(&app, parent_id, child_id).is_some(),
        "the seeing band finds the one standing beside it"
    );
    assert!(
        edge(&app, child_id, parent_id).is_none(),
        "a band that is not a vision source finds nobody"
    );
    assert_eq!(
        ledger(&app).len(),
        1,
        "exactly one edge, not a mutual pair: {:?}",
        ledger(&app).iter().collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------------------------
// The three clocks, over real turns
// ---------------------------------------------------------------------------------------------

/// **Losing sight drains the tie and parks it at zero — and clock 1 still reports where they were.**
///
/// The remembered position is asserted against the tile the subject was seen on, *not* the tile it
/// walked to, which is the whole of "you know where they *were*".
#[test]
fn losing_sight_drains_the_tie_and_leaves_the_last_known_position_standing() {
    let mut app = spawn_world();
    let (parent, parent_id, _, home) = first_band(&mut app);
    let (child, child_id) = split_off(&mut app, parent);

    // Contact, at the shared tile.
    app.update();
    let seen_at = edge(&app, parent_id, child_id)
        .expect("the tie formed")
        .last_seen_position;
    assert_eq!(seen_at, home, "they were seen where they stood");
    let peak = edge(&app, parent_id, child_id)
        .expect("the tie formed")
        .strength;

    // …and now they walk out of sight.
    let gone_to = walk_away(&mut app, child, home);
    assert_ne!(gone_to, home, "the fixture must actually move the band");
    app.update();

    let after = edge(&app, parent_id, child_id).expect("a drained tie is not deleted");
    assert!(
        after.strength < peak,
        "a turn without contact drains the tie: {peak:?} -> {:?}",
        after.strength
    );
    assert_eq!(
        after.last_seen_position, seen_at,
        "clock 1 is not touched by a turn without contact"
    );

    // Long enough to drain a full tie several times over, and still far inside `forget_turns`.
    let cfg = config(&app);
    let drain_turns = (1.0 / cfg.strength.decay_per_turn).ceil() as usize + 2;
    for _ in 0..drain_turns {
        app.update();
    }
    let parked =
        edge(&app, parent_id, child_id).expect("zero PARKS the edge, it does not delete it");
    assert_eq!(parked.strength, core_sim::NO_TIE);
    assert_eq!(
        parked.last_seen_position, seen_at,
        "a parked tie still remembers where they were"
    );
}

/// **Clock 3 — the fact of them is forgotten, and the metric counts the reaping.**
///
/// Driven through the real system on a **retuned** `forget_turns`, rather than by running 200 turns
/// of a full world: the lever is what decides when the edge goes, so turning it down is the honest
/// short form. The shipped value of 200 is asserted in the config module.
#[test]
fn the_fact_of_them_is_forgotten_and_the_reaping_is_counted() {
    let mut app = spawn_world();
    let (parent, parent_id, _, home) = first_band(&mut app);
    let (child, child_id) = split_off(&mut app, parent);

    // Forget one turn after the last contact, so the reap lands on the first turn out of sight.
    let impatient = ConnectionsConfig::from_json_str(
        r#"{ "strength": { "gain_per_contact": 0.25, "decay_per_turn": 0.02 }, "forget_turns": 1 }"#,
    )
    .expect("the fixture config parses");
    app.world
        .insert_resource(ConnectionsConfigHandle::new(std::sync::Arc::new(impatient)));

    app.update();
    assert!(edge(&app, parent_id, child_id).is_some(), "the tie formed");

    walk_away(&mut app, child, home);
    app.update();

    assert!(
        edge(&app, parent_id, child_id).is_none(),
        "an edge nobody has seen in `forget_turns` is gone"
    );
    assert!(
        app.world.resource::<SimulationMetrics>().connections_reaped > 0,
        "the reaping is counted"
    );
}

// ---------------------------------------------------------------------------------------------
// THE KEYSTONE
// ---------------------------------------------------------------------------------------------

/// **Only presence makes a tile `Seen` (`Active`). A connection can only ever grant `Discovered`.**
///
/// Nothing in this slice grants visibility from a connection, so the rule is asserted as a
/// *difference*: two identical worlds, one carrying a full set of ties pointing at far-away ground,
/// must produce the **same** `Active` set. The day a rider lights a tile from an edge, this fails.
///
/// Paired with a liveness half — the seeded ties must actually have been decayed by the turn — so it
/// cannot pass on a sim that dropped the ledger on the floor.
#[test]
fn a_connection_grants_no_active_tile() {
    let seeded_positions = [UVec2::new(1, 1), UVec2::new(70, 40), UVec2::new(40, 25)];

    let mut without = spawn_world();
    let (_, band_id, faction, _) = first_band(&mut without);

    let mut with = spawn_world();
    let cfg = ConnectionsConfig::default();
    let mut ties = ConnectionLedger::default();
    for (index, position) in seeded_positions.iter().enumerate() {
        // A subject id no band carries: a tie to a people that is not standing anywhere near.
        let subject = BandId(u64::MAX - index as u64);
        // Four turns of contact is a FULL tie, so the seeded edges are as strong as they can get.
        for _ in 0..4 {
            ties.record_contact(ConnectionKey::new(band_id, subject), *position, 0, 0, &cfg);
        }
    }
    let seeded_len = ties.len();
    with.world.insert_resource(ties);

    without.update();
    with.update();

    let active_without = active_tiles(&without, faction);
    let active_with = active_tiles(&with, faction);
    assert!(
        !active_without.is_empty(),
        "the fixture must reveal SOME ground, or the comparison proves nothing"
    );
    assert_eq!(
        active_with,
        active_without,
        "a connection may only ever grant Discovered — it lit {} tile(s) nothing stands on",
        active_with.difference(&active_without).count()
    );

    // Liveness: the seeded ties were really carried through the turn and decayed by it.
    let ledger_after = with.world.resource::<ConnectionLedger>();
    assert_eq!(
        ledger_after.len(),
        seeded_len,
        "the seeded ties survive the turn (well inside `forget_turns`)"
    );
    assert!(
        ledger_after
            .iter()
            .any(|(_, connection)| connection.strength < core_sim::FULL_TIE),
        "and the turn's clock 2 drained them, so this world genuinely ran connections"
    );
}

fn active_tiles(app: &App, faction: FactionId) -> std::collections::BTreeSet<(u32, u32)> {
    let ledger = app.world.resource::<VisibilityLedger>();
    let map = ledger
        .get_faction(faction)
        .expect("the faction has a visibility map");
    map.iter_tiles()
        .filter(|(_, tile)| tile.state == VisibilityState::Active)
        .map(|(pos, _)| (pos.x, pos.y))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Expedition-borne contact
// ---------------------------------------------------------------------------------------------

/// **A party's findings reach its HOME BAND, and only on the comm flush.**
///
/// The party watches a band its home cannot see, out past comm range: nothing is credited while it
/// is away, and one contact lands for the home band the moment it comes within range — not one per
/// turn it watched.
#[test]
fn an_expedition_reports_a_people_only_when_it_comes_within_comm_range() {
    let mut app = spawn_world();
    let (parent, parent_id, faction, home) = first_band(&mut app);
    let (child, child_id) = split_off(&mut app, parent);
    let far = walk_away(&mut app, child, home);

    // A scouting party standing on the far band, well past its home's comm range.
    let far_tile = app
        .world
        .resource::<TileRegistry>()
        .index(far.x, far.y)
        .expect("the far tile is on the map");
    let party = {
        // Cloned off a real band rather than hand-built, so the party is an ordinary detached
        // cohort in every respect but its position and its `Expedition`.
        let mut cohort = app
            .world
            .get::<PopulationCohort>(parent)
            .expect("the home band is alive")
            .clone();
        cohort.faction = faction;
        cohort.home = far_tile;
        cohort.current_tile = far_tile;
        cohort.working = Scalar::from_f32(3.0);
        cohort.sync_size();
        app.world
            .spawn((
                cohort,
                LaborAllocation::default(),
                StartingUnit::new("expedition".to_string(), Vec::new()),
                Expedition {
                    home_band: parent,
                    mission: ExpeditionMission::Scout,
                    phase: ExpeditionPhase::AwaitingOrders,
                    announced: true,
                    pending_reveal: Vec::new(),
                    pending_contacts: Default::default(),
                    kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Scout),
                },
            ))
            .id()
    };

    // Several turns of watching, all of them out of comm range.
    for _ in 0..3 {
        app.update();
    }
    assert!(
        edge(&app, parent_id, child_id).is_none(),
        "a party out of comm range has reported nothing"
    );
    let buffered = app
        .world
        .get::<Expedition>(party)
        .expect("the party is alive")
        .pending_contacts
        .clone();
    assert_eq!(
        buffered.get(&child_id).map(|(pos, _)| *pos),
        Some(far),
        "the party is holding the finding, with the position it saw it at: {buffered:?}"
    );
    let observed_turn = buffered
        .get(&child_id)
        .map(|(_, turn)| *turn)
        .expect("the party is holding the finding");

    // …and now it walks home.
    {
        let home_tile = app
            .world
            .get::<PopulationCohort>(parent)
            .expect("the home band is alive")
            .current_tile;
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(party)
            .expect("the party is alive");
        cohort.current_tile = home_tile;
    }
    // Read BEFORE the update: `advance_tick` bumps the counter at the end of the turn, so the turn
    // the flush runs on is the tick standing now.
    let flush_turn = app.world.resource::<SimulationTick>().0;
    app.update();

    let reported = edge(&app, parent_id, child_id).expect("the flush credits the HOME band");
    assert_eq!(
        reported.last_seen_position, far,
        "the report names where the party saw them, not where it handed the report in"
    );
    // **Seen then, told now.** The two turns are separate fields because the report is old by the
    // time it lands: clock 1 dates the sighting on the march, clocks 2 and 3 date the telling.
    assert!(
        observed_turn < flush_turn,
        "the fixture must actually report a STALE sighting: seen {observed_turn}, flushed \
         {flush_turn}"
    );
    assert_eq!(
        reported.last_seen_turn, observed_turn,
        "clock 1 dates the turn the party SAW them, not the turn it walked the news home"
    );
    assert_eq!(
        reported.last_contact_turn, flush_turn,
        "clocks 2 and 3 run off the turn the report arrived"
    );
    assert_eq!(
        reported.first_contact_turn, flush_turn,
        "the tie only exists once the report lands, so it can post-date the sighting"
    );
    assert_eq!(
        reported.strength,
        Scalar::from_f32(config(&app).strength.gain_per_contact),
        "ONE contact per subject per flush, however many turns the party watched them"
    );
    assert!(
        edge(&app, child_id, parent_id).is_none(),
        "the watched band saw nothing — the party is not a subject and the home band is far away"
    );
}

/// **A stale report refreshes the tie but cannot rewrite where they were.**
///
/// The band saw them itself, here, on turn T. A party then walks in with an *older* sighting from
/// somewhere else — reachable whenever a march is longer than the time the subject stayed put. The
/// news is real, so clocks 2 and 3 move; clock 1 does not, because the observer's own eyes are the
/// fresher source.
#[test]
fn an_older_report_refreshes_the_tie_without_rewriting_where_they_were() {
    let mut app = spawn_world();
    let (parent, parent_id, faction, home) = first_band(&mut app);
    let (child, child_id) = split_off(&mut app, parent);

    // 1. Direct sight: the parent sees the band standing beside it, here and now.
    app.update();
    let seen = edge(&app, parent_id, child_id).expect("the co-located band is seen directly");
    let seen_at = seen.last_seen_position;
    let seen_turn = seen.last_seen_turn;
    assert_eq!(seen_at, home, "they were seen where they stood");

    // 2. They walk out of sight, so nothing the parent can see refreshes clock 1 again.
    let gone_to = walk_away(&mut app, child, home);
    assert_ne!(gone_to, home, "the fixture must actually move the band");

    // 3. A party turns up at home carrying an older sighting of that same band, somewhere else.
    //    Hand-seeded rather than marched, so the staleness is exact rather than incidental.
    const STALE_OBSERVATION_TURN: u64 = 0;
    assert!(
        STALE_OBSERVATION_TURN < seen_turn,
        "the seeded report must predate what the band saw itself"
    );
    let home_tile = app
        .world
        .get::<PopulationCohort>(parent)
        .expect("the home band is alive")
        .current_tile;
    {
        let mut cohort = app
            .world
            .get::<PopulationCohort>(parent)
            .expect("the home band is alive")
            .clone();
        cohort.faction = faction;
        cohort.home = home_tile;
        cohort.current_tile = home_tile;
        cohort.working = Scalar::from_f32(3.0);
        cohort.sync_size();
        let mut pending_contacts = std::collections::BTreeMap::new();
        pending_contacts.insert(child_id, (gone_to, STALE_OBSERVATION_TURN));
        app.world.spawn((
            cohort,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: parent,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::AwaitingOrders,
                announced: true,
                pending_reveal: Vec::new(),
                pending_contacts,
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Scout),
            },
        ));
    }
    let before_flush = edge(&app, parent_id, child_id)
        .expect("the tie survives losing sight")
        .strength;

    // Read BEFORE the update, for the reason the sibling test states: the tick advances at the end
    // of the turn.
    let flush_turn = app.world.resource::<SimulationTick>().0;
    app.update();

    let after = edge(&app, parent_id, child_id).expect("the tie survives the report");
    assert_eq!(
        after.last_seen_position, seen_at,
        "an older report cannot drag clock 1 back to a tile they have already left"
    );
    assert_eq!(
        after.last_seen_turn, seen_turn,
        "and it cannot re-stamp an older sighting as the fresher one"
    );
    assert!(
        after.strength > before_flush,
        "the news still arrived, so the tie is refreshed: {before_flush:?} -> {:?}",
        after.strength
    );
    assert_eq!(
        after.last_contact_turn, flush_turn,
        "clocks 2 and 3 run off the turn the report landed"
    );
}
