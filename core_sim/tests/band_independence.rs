//! **Independence — a cut-off, aggrieved band becomes its own people** (#284,
//! `docs/plan_band_fission.md` §Independence, `.claude/rules/core_sim/independence.md`).
//!
//! A people's HEART is the group of its bands joined by live contact ties holding the most people.
//! A band outside it is cut off; a cut-off group whose people-weighted grievance reaches
//! `independence.grievance_threshold` breaks away as ONE new AI people.
//!
//! The arms build their far bands the way the game does — `split_band_from_parent` — then walk them
//! away from home and wipe the contact ledger, so the only ties standing are the ones each arm
//! states. The system is run directly where the claim is about one judgement, and through
//! `run_turn` where it is about the turn (the roster, the queue, the split's first turns).

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::{
    advance_band_independence, publish_baseline_snapshot, run_turn, scalar_from_f32,
    split_band_from_parent, BandId, CommandEventKind, CommandEventLog, ConnectionKey,
    ConnectionLedger, ConnectionsConfigHandle, CounterIntelBudgets, DiscoveryProgressLedger,
    EspionageRoster, FactionBorderPolicies, FactionId, FactionRegistry, FactionSecurityPolicies,
    HeartLedger, KnowledgeFragment, PopulationCohort, ResidentBand, Scalar, SettleConfig, Sighting,
    SimulationConfig, SnapshotHistory, Tile, TileRegistry, TurnQueue, ViewerFaction,
    VisibilityLedger, VisibilityState, WellbeingConfigHandle,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The first people a break-away from a one-faction world founds — the next positional id.
const FIRST_NEW_PEOPLE: FactionId = FactionId(1);
/// The second, when two groups break away on one turn.
const SECOND_NEW_PEOPLE: FactionId = FactionId(2);
/// Workers each splinter takes — small beside the home band, so the home band is the heart.
const SPLINTER_WORKERS: u32 = 3;
/// Grievance well over the shipped threshold (1.0), so one turn's decay cannot bring it under.
const AGGRIEVED: f32 = 5.0;
/// Grievance well under the shipped threshold.
const CONTENT_GRIEVANCE: f32 = 0.2;
/// The turn a staged contact is stamped with.
const SIGHTING_TURN: u64 = 1;
/// A discovery a breaking band knows, and how far along it is.
const KNOWN_DISCOVERY: u32 = 4242;
const KNOWN_PROGRESS: f32 = 0.6;
/// The same discovery, known less well by the second band of a group — the merge takes the best.
const LESSER_PROGRESS: f32 = 0.25;
/// Turns run after a split to show it never reads as lost touch.
const SPLIT_SETTLE_TURNS: usize = 3;
/// Fixed-point comparison tolerance, in raw units.
const RAW_TOLERANCE: i64 = 2;
/// The wire spellings.
const BROKE_AWAY: &str = "band_broke_away";

/// No floors: a staging split is never refused.
fn open_settle() -> SettleConfig {
    SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    }
}

fn home_band(app: &mut App) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .map(|(entity, band, _)| (entity, *band))
        .min_by_key(|(_, band)| *band)
        .expect("the home people has an opening band")
}

fn entity_of(app: &mut App, band: BandId) -> Entity {
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
        .expect("the band is alive")
}

fn cohort(app: &App, band: Entity) -> PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap().clone()
}

fn with_cohort(app: &mut App, band: Entity, edit: impl FnOnce(&mut PopulationCohort)) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    edit(&mut cohort);
    cohort.sync_size();
}

/// A tile half the map away from the home band — far past any sight range.
fn far_tile(app: &mut App, home: Entity) -> Entity {
    let at_tile = app.world.get::<PopulationCohort>(home).unwrap().home;
    let at = app.world.get::<Tile>(at_tile).unwrap().position;
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    app.world
        .resource::<TileRegistry>()
        .index((at.x + width / 2) % width, at.y)
        .expect("the far tile is on the map")
}

/// Split a band off home and walk it to `tile`.
fn splinter_at(app: &mut App, home: Entity, tile: Entity) -> (Entity, BandId) {
    let split = split_band_from_parent(&mut app.world, home, SPLINTER_WORKERS, &open_settle())
        .expect("the staged band can split");
    let child = entity_of(app, split.band);
    with_cohort(app, child, |c| {
        c.home = tile;
        c.current_tile = tile;
    });
    (child, split.band)
}

/// Forget every tie: the arms state the ones they need.
fn wipe_ties(app: &mut App) {
    app.world.insert_resource(ConnectionLedger::default());
}

/// Contact through the ledger's own path, in both directions.
fn tie(app: &mut App, a: BandId, b: BandId) {
    let config = app.world.resource::<ConnectionsConfigHandle>().get();
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for key in [ConnectionKey::new(a, b), ConnectionKey::new(b, a)] {
        ledger.record_contact(
            key,
            &Sighting::new(UVec2::ZERO, SIGHTING_TURN, ""),
            SIGHTING_TURN,
            &config,
        );
    }
}

fn set_grievance(app: &mut App, band: Entity, grievance: f32) {
    with_cohort(app, band, |c| c.grievance = scalar_from_f32(grievance));
}

fn judge(app: &mut App) {
    app.world.run_system_once(advance_band_independence);
}

fn events(app: &App, kind: CommandEventKind) -> Vec<(FactionId, String, String)> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == kind)
        .map(|entry| {
            (
                entry.faction,
                entry.label.clone(),
                entry.detail.clone().unwrap_or_default(),
            )
        })
        .collect()
}

/// One far splinter, cut off and aggrieved.
fn stage_one_far_aggrieved_band(app: &mut App) -> (Entity, BandId) {
    let (home, _) = home_band(app);
    let far = far_tile(app, home);
    let (child, child_id) = splinter_at(app, home, far);
    wipe_ties(app);
    set_grievance(app, child, AGGRIEVED);
    (child, child_id)
}

#[test]
fn a_far_aggrieved_band_becomes_a_new_ai_people_and_both_peoples_are_told() {
    let mut app = one_faction_world();
    let (child, child_id) = stage_one_far_aggrieved_band(&mut app);
    let before = app.world.resource::<FactionRegistry>().factions().len();

    judge(&mut app);

    let registry = app.world.resource::<FactionRegistry>();
    assert_eq!(
        registry.factions().len(),
        before + 1,
        "the roster grew by one"
    );
    assert!(
        registry.is_ai(FIRST_NEW_PEOPLE),
        "a people born at runtime is the AI's"
    );
    assert_eq!(cohort(&app, child).faction, FIRST_NEW_PEOPLE);
    assert_eq!(
        cohort(&app, child).grievance,
        Scalar::zero(),
        "it was a grievance against the people they left"
    );

    let told = events(&app, CommandEventKind::BandBrokeAway);
    assert_eq!(told.len(), 2, "one row per side: {told:?}");
    let lost = told
        .iter()
        .find(|(f, ..)| *f == HOME)
        .expect("the old people is told");
    let gained = told
        .iter()
        .find(|(f, ..)| *f == FIRST_NEW_PEOPLE)
        .expect("the new people is told");
    assert!(lost.1.contains("no longer answers to us"), "{}", lost.1);
    assert!(lost.2.contains("side=lost") && gained.2.contains("side=gained"));
    for (_, _, detail) in [lost, gained] {
        assert!(detail.contains(&format!("band={}", child_id.0)), "{detail}");
        assert!(
            detail.contains("from=0") && detail.contains("to=1"),
            "{detail}"
        );
    }

    let queue = app.world.resource::<TurnQueue>();
    assert!(
        queue.factions().contains(&FIRST_NEW_PEOPLE),
        "the queue will await it"
    );
    assert!(
        !queue.awaiting().contains(&FIRST_NEW_PEOPLE),
        "but not on the turn already in flight"
    );
}

#[test]
fn every_roster_derived_resource_has_a_row_for_the_new_people() {
    let mut app = one_faction_world();
    stage_one_far_aggrieved_band(&mut app);
    // Control: nothing has a row for an id the world never registered.
    assert!(!app
        .world
        .resource::<FactionBorderPolicies>()
        .contains(FIRST_NEW_PEOPLE));

    judge(&mut app);

    assert!(app
        .world
        .resource::<FactionRegistry>()
        .contains(FIRST_NEW_PEOPLE));
    assert!(app
        .world
        .resource::<TurnQueue>()
        .factions()
        .contains(&FIRST_NEW_PEOPLE));
    assert!(app
        .world
        .resource::<CounterIntelBudgets>()
        .contains(FIRST_NEW_PEOPLE));
    assert!(app
        .world
        .resource::<FactionSecurityPolicies>()
        .contains(FIRST_NEW_PEOPLE));
    assert!(app
        .world
        .resource::<FactionBorderPolicies>()
        .contains(FIRST_NEW_PEOPLE));
    assert!(
        app.world
            .resource::<FactionBorderPolicies>()
            .is_open(FIRST_NEW_PEOPLE),
        "every people starts with its borders open"
    );
    let names = app.world.resource::<core_sim::FactionNames>();
    let new_name = names
        .name(FIRST_NEW_PEOPLE)
        .expect("the new people is named");
    assert_ne!(
        Some(new_name),
        names.name(HOME),
        "two peoples of one world never share a name"
    );
    let espionage = app.world.resource::<EspionageRoster>();
    assert_eq!(
        espionage.agents_for(FIRST_NEW_PEOPLE).len(),
        espionage.agents_for(HOME).len(),
        "the new people's agents are seeded exactly as a founding people's were"
    );
}

#[test]
fn a_band_below_the_threshold_or_still_tied_to_the_heart_stays() {
    // Below the threshold: cut off, but not aggrieved enough.
    let mut app = one_faction_world();
    let (child, _) = stage_one_far_aggrieved_band(&mut app);
    set_grievance(&mut app, child, CONTENT_GRIEVANCE);
    judge(&mut app);
    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 1);
    assert_eq!(cohort(&app, child).faction, HOME);
    assert!(events(&app, CommandEventKind::BandBrokeAway).is_empty());

    // Aggrieved, but still tied to the heart.
    let mut app = one_faction_world();
    let (child, child_id) = stage_one_far_aggrieved_band(&mut app);
    let (_, home_id) = home_band(&mut app);
    tie(&mut app, home_id, child_id);
    judge(&mut app);
    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 1);
    assert_eq!(cohort(&app, child).faction, HOME);
    assert!(!app.world.resource::<HeartLedger>().is_cut_off(child_id));
}

#[test]
fn a_one_band_people_never_breaks_away() {
    let mut app = one_faction_world();
    let (home, home_id) = home_band(&mut app);
    wipe_ties(&mut app);
    set_grievance(&mut app, home, AGGRIEVED);

    judge(&mut app);

    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 1);
    assert_eq!(cohort(&app, home).faction, HOME);
    let reading = *app
        .world
        .resource::<HeartLedger>()
        .reading(home_id)
        .expect("the band was judged");
    assert!(!reading.cut_off, "a people with one band is its own heart");
}

#[test]
fn two_far_bands_tied_to_each_other_break_away_together_as_one_people() {
    let mut app = one_faction_world();
    let (home, _) = home_band(&mut app);
    let far = far_tile(&mut app, home);
    let (first, first_id) = splinter_at(&mut app, home, far);
    let (second, second_id) = splinter_at(&mut app, home, far);
    wipe_ties(&mut app);
    tie(&mut app, first_id, second_id);
    set_grievance(&mut app, first, AGGRIEVED);
    set_grievance(&mut app, second, AGGRIEVED);

    judge(&mut app);

    assert_eq!(
        app.world.resource::<FactionRegistry>().factions().len(),
        2,
        "one cut-off group is one new people, not two"
    );
    assert_eq!(cohort(&app, first).faction, FIRST_NEW_PEOPLE);
    assert_eq!(cohort(&app, second).faction, FIRST_NEW_PEOPLE);
    let hearts = app.world.resource::<HeartLedger>();
    assert!(
        !hearts.is_cut_off(first_id) && !hearts.is_cut_off(second_id),
        "the group is its new people's heart"
    );
}

#[test]
fn two_separate_groups_on_one_turn_found_two_peoples_in_band_order() {
    let mut app = one_faction_world();
    let (home, _) = home_band(&mut app);
    let far = far_tile(&mut app, home);
    let (first, first_id) = splinter_at(&mut app, home, far);
    let (second, second_id) = splinter_at(&mut app, home, far);
    assert!(first_id < second_id);
    wipe_ties(&mut app);
    set_grievance(&mut app, first, AGGRIEVED);
    set_grievance(&mut app, second, AGGRIEVED);

    judge(&mut app);

    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 3);
    assert_eq!(
        cohort(&app, first).faction,
        FIRST_NEW_PEOPLE,
        "lowest band first"
    );
    assert_eq!(cohort(&app, second).faction, SECOND_NEW_PEOPLE);
}

#[test]
fn the_new_people_remembers_the_map_sees_nothing_and_keeps_what_it_knew() {
    let mut app = one_faction_world();
    let (child, _) = stage_one_far_aggrieved_band(&mut app);
    with_cohort(&mut app, child, |c| {
        c.knowledge = vec![KnowledgeFragment::new(
            KNOWN_DISCOVERY,
            scalar_from_f32(KNOWN_PROGRESS),
        )];
    });
    let (width, height) = {
        let tiles = app.world.resource::<TileRegistry>();
        (tiles.width, tiles.height)
    };
    {
        let mut fog = app.world.resource_mut::<VisibilityLedger>();
        let map = fog.ensure_faction(HOME, width, height);
        map.mark_active(0, 0, SIGHTING_TURN);
        map.discover(1, 0, SIGHTING_TURN);
    }
    let (_, old_discovered, old_active) = app
        .world
        .resource::<VisibilityLedger>()
        .get_faction(HOME)
        .unwrap()
        .count_by_state();
    assert!(old_active > 0, "the old people had ground in sight");

    judge(&mut app);

    let fog = app.world.resource::<VisibilityLedger>();
    let new_map = fog
        .get_faction(FIRST_NEW_PEOPLE)
        .expect("the new people has a map");
    let (_, discovered, active) = new_map.count_by_state();
    assert_eq!(
        active, 0,
        "presence is rebuilt from where its own bands stand"
    );
    assert_eq!(
        discovered,
        old_discovered + old_active,
        "every tile the old people had seen is remembered"
    );
    assert_eq!(new_map.state_at(0, 0), VisibilityState::Discovered.as_u8());
    let progress = app
        .world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FIRST_NEW_PEOPLE, KNOWN_DISCOVERY);
    assert!(
        (progress.raw() - scalar_from_f32(KNOWN_PROGRESS).raw()).abs() <= RAW_TOLERANCE,
        "they keep what they knew, in full"
    );
}

/// **A lesson earned by practice lives only on the faction's ledger** — no band's `knowledge`
/// carries Cultivation — and a break-away keeps it: they keep everything they knew. The band's own
/// fragment of the same lesson, known less well, does not drag the seed down; the best source wins.
#[test]
fn a_lesson_the_old_people_earned_by_practice_goes_with_the_break_away() {
    let mut app = one_faction_world();
    let (child, _) = stage_one_far_aggrieved_band(&mut app);
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(
            HOME,
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_from_f32(KNOWN_PROGRESS),
        );
    with_cohort(&mut app, child, |c| {
        c.knowledge = vec![KnowledgeFragment::new(
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_from_f32(LESSER_PROGRESS),
        )];
    });

    judge(&mut app);

    let ledger = app.world.resource::<DiscoveryProgressLedger>();
    let carried = ledger.get_progress(FIRST_NEW_PEOPLE, core_sim::CULTIVATION_DISCOVERY_ID);
    assert!(
        (carried.raw() - scalar_from_f32(KNOWN_PROGRESS).raw()).abs() <= RAW_TOLERANCE,
        "the practice-earned lesson carries over in full, got {carried:?}"
    );
    assert_eq!(
        ledger.get_progress(HOME, core_sim::CULTIVATION_DISCOVERY_ID),
        scalar_from_f32(KNOWN_PROGRESS),
        "the old people does not forget it"
    );
}

#[test]
fn a_group_seeds_the_best_any_of_its_bands_knew() {
    let mut app = one_faction_world();
    let (home, _) = home_band(&mut app);
    let far = far_tile(&mut app, home);
    let (first, first_id) = splinter_at(&mut app, home, far);
    let (second, second_id) = splinter_at(&mut app, home, far);
    wipe_ties(&mut app);
    tie(&mut app, first_id, second_id);
    for (band, progress) in [(first, LESSER_PROGRESS), (second, KNOWN_PROGRESS)] {
        with_cohort(&mut app, band, |c| {
            c.grievance = scalar_from_f32(AGGRIEVED);
            c.knowledge = vec![KnowledgeFragment::new(
                KNOWN_DISCOVERY,
                scalar_from_f32(progress),
            )];
        });
    }

    judge(&mut app);

    let progress = app
        .world
        .resource::<DiscoveryProgressLedger>()
        .get_progress(FIRST_NEW_PEOPLE, KNOWN_DISCOVERY);
    assert!(
        (progress.raw() - scalar_from_f32(KNOWN_PROGRESS).raw()).abs() <= RAW_TOLERANCE,
        "two bands knowing one thing do not know it twice"
    );
}

#[test]
fn a_split_never_reads_as_lost_touch_on_its_first_turns() {
    let mut app = one_faction_world();
    // A turn first, so the parent holds a reading the splinter inherits — on a world no turn has
    // judged, nothing has a previous reading and no edge could fire whatever the ties said.
    run_turn(&mut app);
    let (home, home_id) = home_band(&mut app);
    assert!(app
        .world
        .resource::<HeartLedger>()
        .reading(home_id)
        .is_some());
    let split = split_band_from_parent(&mut app.world, home, SPLINTER_WORKERS, &open_settle())
        .expect("the band can split");
    assert!(
        app.world
            .resource::<HeartLedger>()
            .reading(split.band)
            .is_some(),
        "the splinter inherits its parent's standing"
    );
    assert!(
        app.world
            .resource::<ConnectionLedger>()
            .tie_is_live(home_id, split.band),
        "the halves start tied"
    );

    for _ in 0..SPLIT_SETTLE_TURNS {
        run_turn(&mut app);
    }

    assert!(
        events(&app, CommandEventKind::LostTouch).is_empty(),
        "a band standing on its parent's tile was never out of touch"
    );
    assert!(!app.world.resource::<HeartLedger>().is_cut_off(split.band));
}

#[test]
fn losing_the_last_live_tie_to_the_heart_is_told_once() {
    let mut app = one_faction_world();
    let (home, _) = home_band(&mut app);
    let far = far_tile(&mut app, home);
    let (child, child_id) = splinter_at(&mut app, home, far);
    set_grievance(&mut app, child, CONTENT_GRIEVANCE);

    judge(&mut app);
    assert!(
        events(&app, CommandEventKind::LostTouch).is_empty(),
        "still tied from the split"
    );

    wipe_ties(&mut app);
    judge(&mut app);
    judge(&mut app);

    let lost = events(&app, CommandEventKind::LostTouch);
    assert_eq!(lost.len(), 1, "an edge, not a state: {lost:?}");
    let (faction, label, detail) = &lost[0];
    assert_eq!(*faction, HOME, "told to the band's own people");
    assert!(label.starts_with("We have lost touch with"), "{label}");
    assert!(detail.contains(&format!("band={}", child_id.0)), "{detail}");
    assert!(app.world.resource::<HeartLedger>().is_cut_off(child_id));
    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 1);
}

/// The row a viewer's encoded frame publishes for `band`.
fn published_row<R>(
    app: &mut App,
    band: BandId,
    read: impl Fn(fb::PopulationCohortState) -> R,
) -> R {
    app.world.insert_resource(ViewerFaction(HOME));
    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("populations are published");
    let row = rows
        .iter()
        .find(|row| row.bandId() == band.0)
        .expect("the band has a row");
    read(row)
}

#[test]
fn the_wire_carries_the_bands_standing_toward_the_heart() {
    let mut app = one_faction_world();
    let (home, home_id) = home_band(&mut app);
    let far = far_tile(&mut app, home);
    let (child, child_id) = splinter_at(&mut app, home, far);
    set_grievance(&mut app, child, CONTENT_GRIEVANCE);
    judge(&mut app);

    // In touch: the split's full tie, both ways.
    let (cut_off, bond, last) = published_row(&mut app, child_id, |row| {
        (row.cutOff(), row.heartBond(), row.heartLastContactTurn())
    });
    assert!(!cut_off);
    assert!(
        (bond - 1.0).abs() < f32::EPSILON,
        "a full tie reads 1.0, got {bond}"
    );
    assert!(last >= 0, "a contact turn is published");
    let threshold = app
        .world
        .resource::<WellbeingConfigHandle>()
        .get()
        .independence
        .grievance_threshold;
    let echoed = published_row(&mut app, home_id, |row| {
        row.independenceGrievanceThreshold()
    });
    assert!(
        (echoed - threshold).abs() < f32::EPSILON,
        "the threshold is echoed"
    );

    // Cut off: no tie at all.
    wipe_ties(&mut app);
    judge(&mut app);
    let (cut_off, bond, last) = published_row(&mut app, child_id, |row| {
        (row.cutOff(), row.heartBond(), row.heartLastContactTurn())
    });
    assert!(cut_off);
    assert_eq!(bond, 0.0);
    assert_eq!(last, sim_schema::state::NO_HEART_CONTACT);
}

#[test]
fn a_checkpoint_and_a_save_keep_the_grown_roster_and_the_cut_off_flag() {
    let mut app = one_faction_world();
    let (home, _) = home_band(&mut app);
    let far = far_tile(&mut app, home);
    let (broke, _) = splinter_at(&mut app, home, far);
    let (drifting, drifting_id) = splinter_at(&mut app, home, far);
    wipe_ties(&mut app);
    set_grievance(&mut app, broke, AGGRIEVED);
    set_grievance(&mut app, drifting, CONTENT_GRIEVANCE);
    judge(&mut app);
    // `drifting` was alone and content: it stays, cut off. `broke` founded a people... but the two
    // were not tied, so they are two groups; only the aggrieved one went.
    assert_eq!(app.world.resource::<FactionRegistry>().factions().len(), 2);
    assert!(app.world.resource::<HeartLedger>().is_cut_off(drifting_id));

    let checkpoint = core_sim::sim_state::capture_sim_state(&app.world);
    app.world.insert_resource(FactionRegistry::default());
    app.world.insert_resource(HeartLedger::default());
    core_sim::sim_state::restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(
        app.world.resource::<FactionRegistry>().factions().len(),
        2,
        "a rollback puts the roster back"
    );
    assert!(app.world.resource::<HeartLedger>().is_cut_off(drifting_id));

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");
    assert_eq!(
        loaded.world.resource::<FactionRegistry>().factions().len(),
        2
    );
    assert!(loaded
        .world
        .resource::<FactionRegistry>()
        .is_ai(FIRST_NEW_PEOPLE));
    assert!(loaded
        .world
        .resource::<HeartLedger>()
        .is_cut_off(drifting_id));
    assert!(
        loaded
            .world
            .resource::<TurnQueue>()
            .factions()
            .contains(&FIRST_NEW_PEOPLE),
        "a load's queue awaits the restored roster"
    );
}

#[test]
fn a_break_away_through_the_turn_reaches_the_published_feed_of_both_peoples() {
    let mut app = one_faction_world();
    let (child, child_id) = stage_one_far_aggrieved_band(&mut app);

    run_turn(&mut app);

    assert_eq!(cohort(&app, child).faction, FIRST_NEW_PEOPLE);
    for viewer in [HOME, FIRST_NEW_PEOPLE] {
        app.world.insert_resource(ViewerFaction(viewer));
        publish_baseline_snapshot(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
        let rows: Vec<String> = envelope
            .payload_as_snapshot()
            .expect("a snapshot")
            .campaign()
            .and_then(|section| section.commandEvents())
            .map(|rows| {
                rows.iter()
                    .filter(|row| row.kind().unwrap_or_default() == BROKE_AWAY)
                    .map(|row| row.detail().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            rows.len(),
            1,
            "{viewer:?} sees exactly its own side: {rows:?}"
        );
        assert!(rows[0].contains(&format!("band={}", child_id.0)));
    }
}
