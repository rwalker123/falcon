//! **Defection — people leave to join a better-off people** (`docs/plan_band_fission.md` §Defection,
//! issue #512).
//!
//! One rule: the wellbeing trickle (`advance_population_migration`) with the same-people filter
//! lifted. An UNHAPPY band sheds people toward a happier band within reach; its own people's band
//! first, another people's only when that band is tied to it by a live contact AND its people keep
//! Open Borders. A cross-people move carries a proportional share of the source's knowledge, and one
//! that leaves the source below `settle.parent_min_workers` takes the remnant with it. A detached
//! party goes whole (`advance_party_defection`).
//!
//! The trickle arms run the system once on a two-faction world with the two opening camps staged
//! side by side — the arms differ only in the one fact each is about, so a pass cannot come from the
//! staging.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::{
    advance_party_defection, advance_population_migration, run_turn, scalar_from_f32,
    split_band_from_parent, trace_path, traffic_ceiling, BandId, BandIdAllocator, BandName,
    CommandEventKind, CommandEventLog, ConnectionKey, ConnectionLedger, ConnectionsConfigHandle,
    DiscoveryProgressLedger, Expedition, ExpeditionConfigHandle, ExpeditionMission,
    ExpeditionPhase, FactionBorderPolicies, FactionId, ForageRegistry, HerdRegistry,
    LaborAllocation, LaborAssignment, LaborTarget, LadderConfigHandle, LocalStore, PartySightings,
    PopulationCohort, ResidentBand, RoadKeeper, RoadRegistry, Scalar, SettleConfig, Sighting,
    SimulationConfig, SourcePriority, StartingUnit, TakeSelection, Tile, TileRegistry,
    TransferCause, WellbeingConfigHandle, DEFAULT_ESCAPEMENT_FLOOR, FOOD,
};
use faction_support::{two_faction_world, HOME, RIVAL};

/// Rock-bottom morale: the push is at its `max_rate`, so a staged source always wants to leave.
const MISERABLE: f32 = 0.0;
/// Comfortably above `attractive_morale` (0.5) and any source + `min_morale_gap`.
const THRIVING: f32 = 0.9;
/// Above `attractive_morale` and the source, but below [`THRIVING`] — an own-people destination
/// that is strictly worse off than the foreign one, so preferring it is a rule and not a ranking.
const COMFORTABLE: f32 = 0.6;
/// Above the push threshold (0.25): a band this content never sheds anyone.
const CONTENT: f32 = 0.8;
/// A discontent share for the grievance comparison — any positive value makes grievance accrue.
const DISCONTENT: f32 = 0.5;
/// The turn a staged contact is stamped with. Any turn both clocks agree on.
const SIGHTING_TURN: u64 = 1;
/// A staged source's working-age head count, well clear of `settle.parent_min_workers` so no remnant
/// test fires by accident.
const LARGE_BAND_WORKERS: f32 = 40.0;
/// A party's head count and pack.
const PARTY_WORKERS: f32 = 3.0;
const PARTY_PACK_FOOD: f32 = 5.0;
/// Enough runs for any shipped threshold to be reached at rock-bottom morale.
const MAX_PULL_TURNS: usize = 20;
/// Fixed-point comparison tolerance, in raw units.
const RAW_TOLERANCE: i64 = 2;
/// The regression horizon: the old whole-band flip fired on turn 5 of an ordinary start.
const REGRESSION_TURNS: usize = 20;

struct Stage {
    home: Entity,
    home_id: BandId,
    rival: Entity,
    rival_id: BandId,
}

fn opening_band(app: &mut App, faction: FactionId) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, band, _)| (entity, *band))
        .min_by_key(|(_, band)| *band)
        .unwrap_or_else(|| panic!("{faction:?} has an opening band"))
}

fn entity_of(app: &mut App, band: BandId) -> Entity {
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
        .expect("the band is alive")
}

fn tile_at(app: &App, x: u32, y: u32) -> Entity {
    app.world
        .resource::<TileRegistry>()
        .index(x, y)
        .expect("the staged tile is on the map")
}

fn position_of(app: &App, band: Entity) -> UVec2 {
    let tile = app.world.get::<PopulationCohort>(band).unwrap().home;
    app.world.get::<Tile>(tile).unwrap().position
}

fn cohort(app: &App, band: Entity) -> PopulationCohort {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band is alive")
        .clone()
}

fn with_cohort(app: &mut App, band: Entity, edit: impl FnOnce(&mut PopulationCohort)) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    edit(&mut cohort);
    cohort.sync_size();
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

/// The two opening camps side by side: the rival's camp moved onto the tile next to home. Every
/// OTHER resident band is made miserable too, so none of them is an attractive destination that
/// could decide an arm.
fn stage(app: &mut App) -> Stage {
    let (home, home_id) = opening_band(app, HOME);
    let (rival, rival_id) = opening_band(app, RIVAL);
    let at = position_of(app, home);
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    let beside = tile_at(app, (at.x + 1) % width, at.y);
    let others: Vec<Entity> = app
        .world
        .query_filtered::<Entity, With<ResidentBand>>()
        .iter(&app.world)
        .collect();
    for band in others {
        with_cohort(app, band, |c| c.morale = scalar_from_f32(MISERABLE));
    }
    with_cohort(app, rival, |c| {
        c.home = beside;
        c.current_tile = beside;
        c.morale = scalar_from_f32(THRIVING);
    });
    with_cohort(app, home, |c| {
        c.working = scalar_from_f32(LARGE_BAND_WORKERS);
        c.discontent_fraction = scalar_from_f32(DISCONTENT);
    });
    Stage {
        home,
        home_id,
        rival,
        rival_id,
    }
}

fn migrate(app: &mut App) {
    app.world.run_system_once(advance_population_migration);
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

#[test]
fn an_unhappy_band_trickles_to_a_happier_foreign_band_it_is_tied_to() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    tie(&mut app, s.home_id, s.rival_id);
    let (home_before, rival_before) = (cohort(&app, s.home), cohort(&app, s.rival));

    migrate(&mut app);

    let (home_after, rival_after) = (cohort(&app, s.home), cohort(&app, s.rival));
    let left = home_before.total() - home_after.total();
    assert!(left > Scalar::zero(), "the unhappy band lost people");
    assert_eq!(
        rival_after.total() - rival_before.total(),
        left,
        "every one of them arrived in the foreign band — people are conserved"
    );
    assert_eq!(
        home_after.faction, HOME,
        "a large band does not change sides"
    );
    assert!(home_after.last_emigrated > 0 && rival_after.last_immigrated > 0);

    let migrated = events(&app, CommandEventKind::Migrated);
    let out = migrated
        .iter()
        .find(|(faction, _, detail)| *faction == HOME && detail.contains("direction=out"))
        .expect("the source's people are told who left");
    assert!(
        out.1.contains(&format!("to join Faction {}", RIVAL.0)) && out.2.contains("to=1"),
        "and whom they joined: {out:?}"
    );
    let arrived = migrated
        .iter()
        .find(|(faction, _, detail)| *faction == RIVAL && detail.contains("direction=in"))
        .expect("the receiving people are told who arrived");
    assert!(
        arrived.1.contains(&format!("from Faction {}", HOME.0)) && arrived.2.contains("from=0"),
        "and where from: {arrived:?}"
    );
}

#[test]
fn a_happy_band_never_loses_anyone() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    tie(&mut app, s.home_id, s.rival_id);
    with_cohort(&mut app, s.home, |c| c.morale = scalar_from_f32(CONTENT));
    let before = cohort(&app, s.home).total();

    migrate(&mut app);

    assert_eq!(cohort(&app, s.home).total(), before);
    assert_eq!(cohort(&app, s.home).last_emigrated, 0);
}

#[test]
fn with_no_tie_nobody_crosses() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    let before = cohort(&app, s.home).total();

    migrate(&mut app);

    assert_eq!(
        cohort(&app, s.home).total(),
        before,
        "a band next door that nobody has met is not a destination"
    );
}

/// Grievance gained this run, for a band staged with the same discontent in both arms.
fn grievance_gain(app: &mut App, band: Entity) -> Scalar {
    let before = cohort(app, band).grievance;
    migrate(app);
    cohort(app, band).grievance - before
}

#[test]
fn a_closed_border_keeps_the_leavers_home_and_traps_them() {
    let mut open = two_faction_world();
    let s = stage(&mut open);
    tie(&mut open, s.home_id, s.rival_id);
    let open_gain = grievance_gain(&mut open, s.home);

    let mut closed = two_faction_world();
    let s = stage(&mut closed);
    tie(&mut closed, s.home_id, s.rival_id);
    closed
        .world
        .resource_mut::<FactionBorderPolicies>()
        .set_open(RIVAL, false);
    let before = cohort(&closed, s.home).total();
    let closed_gain = grievance_gain(&mut closed, s.home);

    assert_eq!(
        cohort(&closed, s.home).total(),
        before,
        "nobody crosses a closed border"
    );
    let multiplier = closed
        .world
        .resource::<WellbeingConfigHandle>()
        .get()
        .discontent
        .trapped_multiplier;
    assert!(multiplier > 1.0, "fixture: the trapped bonus must be live");
    assert!(
        (closed_gain.to_f32() - open_gain.to_f32() * multiplier).abs() < 1e-4,
        "shut out, their grievance grows by the trapped multiplier: open {open_gain:?} closed \
         {closed_gain:?}"
    );
}

#[test]
fn a_band_of_its_own_people_comes_first() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    tie(&mut app, s.home_id, s.rival_id);
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, s.home, 10, &settle)
        .expect("the staged band can split");
    let child = entity_of(&mut app, split.band);
    // The splinter stands on its parent's tile — within reach — and is better off than the source
    // but WORSE off than the foreign band, so only the rule can choose it.
    with_cohort(&mut app, child, |c| c.morale = scalar_from_f32(COMFORTABLE));
    let (own_before, rival_before) = (cohort(&app, child), cohort(&app, s.rival));

    migrate(&mut app);

    assert!(
        cohort(&app, child).total() > own_before.total(),
        "the leavers went to their own people"
    );
    assert_eq!(
        cohort(&app, s.rival).total(),
        rival_before.total(),
        "and not to the happier strangers next door"
    );
}

#[test]
fn a_remnant_too_small_to_staff_itself_goes_over_with_its_leavers() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    tie(&mut app, s.home_id, s.rival_id);
    let floor = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .parent_min_workers;
    // Exactly at the floor with nobody else: the first person to leave takes it below.
    with_cohort(&mut app, s.home, |c| {
        c.working = Scalar::from_u32(floor);
        c.children = Scalar::zero();
        c.elders = Scalar::zero();
    });

    migrate(&mut app);

    assert_eq!(
        cohort(&app, s.home).faction,
        RIVAL,
        "the band changed people as a band"
    );
    let handovers = events(&app, CommandEventKind::BandChangedHands);
    let lost = handovers
        .iter()
        .find(|(faction, _, detail)| *faction == HOME && detail.contains("side=lost"));
    let gained = handovers
        .iter()
        .find(|(faction, _, detail)| *faction == RIVAL && detail.contains("side=gained"));
    assert!(
        lost.is_some() && gained.is_some(),
        "both peoples are told: {handovers:?}"
    );
}

#[test]
fn a_move_among_ones_own_people_never_changes_a_bands_people() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    let floor = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .parent_min_workers;
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, s.home, 10, &settle)
        .expect("the staged band can split");
    let child = entity_of(&mut app, split.band);
    with_cohort(&mut app, child, |c| c.morale = scalar_from_f32(THRIVING));
    with_cohort(&mut app, s.home, |c| {
        c.working = Scalar::from_u32(floor);
        c.children = Scalar::zero();
        c.elders = Scalar::zero();
    });

    migrate(&mut app);

    assert!(cohort(&app, s.home).last_emigrated > 0, "people did leave");
    assert_eq!(cohort(&app, s.home).faction, HOME);
    assert!(events(&app, CommandEventKind::BandChangedHands).is_empty());
}

#[test]
fn the_knowledge_that_crosses_is_the_share_of_the_band_that_left() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    tie(&mut app, s.home_id, s.rival_id);
    let before = cohort(&app, s.home);
    assert!(
        !before.knowledge.is_empty(),
        "fixture: the profile seeds every band with knowledge"
    );
    let progress_before: Vec<Scalar> = before
        .knowledge
        .iter()
        .map(|f| {
            app.world
                .resource::<DiscoveryProgressLedger>()
                .get_progress(RIVAL, f.discovery_id)
        })
        .collect();

    migrate(&mut app);

    let after = cohort(&app, s.home);
    let share = (before.total() - after.total()) / before.total();
    assert!(
        share > Scalar::zero() && share < Scalar::one(),
        "part of the band left"
    );
    let config = app.world.resource::<SimulationConfig>().clone();
    let contract: Vec<_> = before.knowledge.iter().map(|f| f.to_contract()).collect();
    let scaled =
        sim_runtime::scale_migration_fragments(&contract, config.migration_fragment_scaling.raw());
    assert!(!scaled.is_empty(), "fixture: something is worth carrying");
    for fragment in &scaled {
        let expected = Scalar::from_raw(fragment.progress) * share;
        let index = before
            .knowledge
            .iter()
            .position(|f| f.discovery_id == fragment.discovery_id)
            .unwrap();
        let gained = app
            .world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(RIVAL, fragment.discovery_id)
            - progress_before[index];
        assert!(
            (gained.raw() - expected.raw()).abs() <= RAW_TOLERANCE,
            "discovery {}: the receiving people gains the leavers' share {expected:?}, got \
             {gained:?}",
            fragment.discovery_id
        );
        assert!(
            gained < Scalar::from_raw(fragment.progress),
            "a proportional share, never the whole"
        );
    }
}

/// A scouting party of `home`'s people standing on `at`.
fn spawn_party(app: &mut App, home: Entity, at: Entity) -> (Entity, BandId) {
    let mut party = cohort(app, home);
    party.home = at;
    party.current_tile = at;
    party.working = scalar_from_f32(PARTY_WORKERS);
    party.children = Scalar::zero();
    party.elders = Scalar::zero();
    party.stores = LocalStore::new();
    party
        .stores
        .add_food("dry", scalar_from_f32(PARTY_PACK_FOOD));
    party.sync_size();
    let id = app.world.resource_mut::<BandIdAllocator>().allocate();
    let entity = app
        .world
        .spawn((
            party,
            id,
            LaborAllocation::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band: home,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::AwaitingOrders,
                announced: true,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Scout),
                cargo: LocalStore::new(),
                defection_pull: Scalar::zero(),
            },
        ))
        .id();
    (entity, id)
}

/// One party-defection pass, with `seen` as this turn's sightings.
fn defect(app: &mut App, party: Entity, seen: Option<BandId>) {
    let mut sightings = PartySightings::default();
    if let Some(band) = seen {
        sightings.record(party, band);
    }
    app.world.insert_resource(sightings);
    app.world.run_system_once(advance_party_defection);
}

fn pull(app: &App, party: Entity) -> Scalar {
    app.world.get::<Expedition>(party).unwrap().defection_pull
}

fn party_stage() -> (App, Stage, Entity) {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    let beside = cohort(&app, s.rival).current_tile;
    let (party, _) = spawn_party(&mut app, s.home, beside);
    (app, s, party)
}

#[test]
fn a_party_accrues_pull_and_goes_whole_once_it_reaches_the_threshold() {
    let (mut app, s, party) = party_stage();
    let rival_before = cohort(&app, s.rival);

    defect(&mut app, party, Some(s.rival_id));
    let first = pull(&app, party);
    assert!(first > Scalar::zero(), "a turn in sight accrues pull");
    let threshold = scalar_from_f32(
        app.world
            .resource::<ExpeditionConfigHandle>()
            .get()
            .defection
            .party_pull_threshold,
    );
    assert!(
        first < threshold,
        "fixture: one turn at rock-bottom morale must not already be enough"
    );

    let mut turns = 1;
    while app.world.get_entity(party).is_some() {
        assert!(turns < MAX_PULL_TURNS, "the party never went");
        defect(&mut app, party, Some(s.rival_id));
        turns += 1;
    }
    assert!(turns > 1, "it deliberated before going");

    let rival_after = cohort(&app, s.rival);
    assert_eq!(
        rival_after.working - rival_before.working,
        scalar_from_f32(PARTY_WORKERS),
        "the whole party joined the band it was watching"
    );
    let crossing = app
        .world
        .get::<LaborAllocation>(s.rival)
        .unwrap()
        .last_transfer_crossings
        .iter()
        .find(|row| row.cause == TransferCause::PartyDefected && row.commodity == FOOD)
        .cloned()
        .expect("the pack's food is booked on the receiver, so its ledger still closes");
    assert!((crossing.amount - PARTY_PACK_FOOD).abs() < 1e-3);

    let lines = events(&app, CommandEventKind::PartyDefected);
    let lost: Vec<_> = lines.iter().filter(|(f, ..)| *f == HOME).collect();
    assert_eq!(lost.len(), 1, "the losing people get one line: {lines:?}");
    assert_eq!(lost[0].1, "Your scouting party has left your control.");
    for token in ["band=", "to=", "from=", "x=", "y="] {
        assert!(
            !lost[0].2.contains(token),
            "no place, no destination, no reason: {:?}",
            lost[0].2
        );
    }
    let gained: Vec<_> = lines.iter().filter(|(f, ..)| *f == RIVAL).collect();
    assert_eq!(gained.len(), 1);
    assert!(
        gained[0].1
            == format!(
                "A party of {} from Faction {} joined Band {}",
                PARTY_WORKERS as u32, HOME.0, s.rival_id.0
            )
            && gained[0].2.contains("from=0"),
        "the receivers are told a party joined one of their bands: {gained:?}"
    );
}

#[test]
fn a_partys_pull_resets_when_the_band_is_out_of_sight() {
    let (mut app, s, party) = party_stage();
    defect(&mut app, party, Some(s.rival_id));
    assert!(pull(&app, party) > Scalar::zero());
    defect(&mut app, party, None);
    assert_eq!(
        pull(&app, party),
        Scalar::zero(),
        "a turn without sight resets it"
    );
}

#[test]
fn a_closed_border_turns_a_party_away() {
    let (mut app, s, party) = party_stage();
    app.world
        .resource_mut::<FactionBorderPolicies>()
        .set_open(RIVAL, false);
    for _ in 0..MAX_PULL_TURNS {
        defect(&mut app, party, Some(s.rival_id));
    }
    assert!(
        app.world.get_entity(party).is_some(),
        "the party is still ours"
    );
    assert_eq!(pull(&app, party), Scalar::zero());
}

#[test]
fn a_two_faction_start_keeps_its_founding_camps() {
    // The regression #512 exists for: the retired whole-band flip swapped the two founding camps on
    // turn 5 of an ordinary start.
    let mut app = two_faction_world();
    let opening: Vec<(BandId, FactionId)> = app
        .world
        .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .map(|(band, cohort)| (*band, cohort.faction))
        .collect();
    assert!(opening.iter().any(|(_, f)| *f == HOME) && opening.iter().any(|(_, f)| *f == RIVAL));
    for _ in 0..REGRESSION_TURNS {
        run_turn(&mut app);
    }
    let now: Vec<(BandId, FactionId)> = app
        .world
        .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .map(|(band, cohort)| (*band, cohort.faction))
        .collect();
    for (band, faction) in &opening {
        if let Some((_, current)) = now.iter().find(|(b, _)| b == band) {
            assert_eq!(current, faction, "band {} changed people", band.0);
        }
    }
    assert!(events(&app, CommandEventKind::BandChangedHands).is_empty());
}

#[test]
fn open_borders_and_a_partys_pull_survive_a_save() {
    let (mut app, s, party) = party_stage();
    app.world
        .resource_mut::<FactionBorderPolicies>()
        .set_open(RIVAL, false);
    let staged_pull = scalar_from_f32(0.2);
    app.world
        .get_mut::<Expedition>(party)
        .unwrap()
        .defection_pull = staged_pull;
    let _ = s;

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");

    let borders = loaded.world.resource::<FactionBorderPolicies>();
    assert!(!borders.is_open(RIVAL), "a closed border stays closed");
    assert!(borders.is_open(HOME), "and an open one open");
    let mut loaded = loaded;
    let pulls: Vec<Scalar> = loaded
        .world
        .query::<&Expedition>()
        .iter(&loaded.world)
        .map(|expedition| expedition.defection_pull)
        .collect();
    assert_eq!(
        pulls,
        vec![staged_pull],
        "the party's pull comes back with it"
    );
}

/// Stage the extreme case: the home band exactly at the parent floor, tied to a thriving rival camp
/// next door, so this run's trickle takes it below the floor and it goes over.
fn stage_a_remnant_flip(app: &mut App) -> Stage {
    let s = stage(app);
    tie(app, s.home_id, s.rival_id);
    let floor = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .parent_min_workers;
    with_cohort(app, s.home, |c| {
        c.working = Scalar::from_u32(floor);
        c.children = Scalar::zero();
        c.elders = Scalar::zero();
    });
    s
}

fn name_of(app: &App, entity: Entity) -> String {
    app.world.get::<BandName>(entity).unwrap().0.clone()
}

#[test]
fn a_band_that_goes_over_takes_the_roads_it_keeps() {
    let mut app = two_faction_world();
    let s = stage_a_remnant_flip(&mut app);
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (kept, other) = (UVec2::new(0, 0), UVec2::new(1, 0));
    let bystander = BandId(s.home_id.0 + s.rival_id.0 + 1);
    {
        let mut roads = app.world.resource_mut::<RoadRegistry>();
        roads.road_or_trail(kept, &ladder).keeper = Some(RoadKeeper {
            faction: HOME,
            band: s.home_id,
        });
        roads.road_or_trail(other, &ladder).keeper = Some(RoadKeeper {
            faction: HOME,
            band: bystander,
        });
    }

    migrate(&mut app);

    assert_eq!(
        cohort(&app, s.home).faction,
        RIVAL,
        "fixture: the band went over"
    );
    let roads = app.world.resource::<RoadRegistry>();
    assert_eq!(
        roads.road(kept).unwrap().keeper,
        Some(RoadKeeper {
            faction: RIVAL,
            band: s.home_id
        }),
        "the road it keeps is now its new people's"
    );
    assert_eq!(
        roads.road(other).unwrap().keeper.map(|k| k.faction),
        Some(HOME),
        "a road some other band keeps stays where it was"
    );
}

#[test]
fn a_band_that_goes_over_keeps_its_name_unless_its_new_people_already_use_it() {
    // Collision: the rival camp answers to the home band's own name.
    let mut clash = two_faction_world();
    let s = stage_a_remnant_flip(&mut clash);
    let original = name_of(&clash, s.home);
    clash.world.get_mut::<BandName>(s.rival).unwrap().0 = original.clone();
    migrate(&mut clash);
    let renamed = name_of(&clash, s.home);
    assert_ne!(renamed, original, "a clashing name is re-minted");
    assert!(!renamed.is_empty());
    let rival_names: Vec<String> = clash
        .world
        .query_filtered::<(Entity, &PopulationCohort, &BandName), With<ResidentBand>>()
        .iter(&clash.world)
        .filter(|(e, c, _)| *e != s.home && c.faction == RIVAL)
        .map(|(_, _, n)| n.0.clone())
        .collect();
    assert!(
        !rival_names.contains(&renamed),
        "and the new name is one its new people do not already use: {renamed} vs {rival_names:?}"
    );

    // Control: no clash, and the name is kept.
    let mut calm = two_faction_world();
    let s = stage_a_remnant_flip(&mut calm);
    calm.world.get_mut::<BandName>(s.rival).unwrap().0 = "A name nobody else carries".into();
    let original = name_of(&calm, s.home);
    migrate(&mut calm);
    assert_eq!(
        cohort(&calm, s.home).faction,
        RIVAL,
        "fixture: the band went over"
    );
    assert_eq!(name_of(&calm, s.home), original, "no clash, no rename");
}

#[test]
fn a_party_out_from_a_band_that_goes_over_goes_with_it_and_still_comes_home() {
    let mut app = two_faction_world();
    let s = stage_a_remnant_flip(&mut app);
    let home_tile = cohort(&app, s.home).current_tile;
    let (party, _) = spawn_party(&mut app, s.home, home_tile);
    let home_name = BandName(name_of(&app, s.home));
    app.world.entity_mut(party).insert(home_name);

    migrate(&mut app);

    assert_eq!(
        cohort(&app, s.home).faction,
        RIVAL,
        "fixture: the band went over"
    );
    assert_eq!(
        cohort(&app, party).faction,
        RIVAL,
        "the party's families went over, so the party did too"
    );
    assert!(
        events(&app, CommandEventKind::PartyDefected).is_empty(),
        "the losing side hears nothing beyond the band's own handover"
    );
    let lost: Vec<_> = events(&app, CommandEventKind::BandChangedHands)
        .into_iter()
        .filter(|(f, ..)| *f == HOME)
        .collect();
    assert_eq!(lost.len(), 1, "one line to the losing people: {lost:?}");

    // Standing in its (flipped) home band's camp, turned for home: it folds back normally.
    app.world.get_mut::<Expedition>(party).unwrap().phase = ExpeditionPhase::Returning;
    run_turn(&mut app);
    assert!(
        app.world.get_entity(party).is_none(),
        "the party folded back into its band"
    );
    let returned = events(&app, CommandEventKind::ExpeditionReturned);
    assert!(
        returned.iter().any(|(f, ..)| *f == RIVAL),
        "and its homecoming is told to the people it now belongs to: {returned:?}"
    );
}

/// A labor row on `target`, held at zero hands — a held row is still that band's work.
fn work(app: &mut App, band: Entity, target: LaborTarget) {
    let row = LaborAssignment {
        party: None,
        target,
        workers: 0,
        kit: None,
        priority: SourcePriority::default(),
    };
    match app.world.get_mut::<LaborAllocation>(band) {
        Some(mut allocation) => allocation.assignments.push(row),
        None => {
            app.world.entity_mut(band).insert(LaborAllocation {
                assignments: vec![row],
                ..Default::default()
            });
        }
    }
}

fn forage_row(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: DEFAULT_ESCAPEMENT_FLOOR,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// Two patches and a herd the home people own. `(sole, shared, herd)`.
fn owned_improvements(app: &mut App) -> (UVec2, UVec2, String) {
    let mut tiles: Vec<UVec2> = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .collect();
    tiles.sort_by_key(|t| (t.y, t.x));
    assert!(
        tiles.len() >= 2,
        "fixture: the world carries forage patches"
    );
    let (sole, shared) = (tiles[0], tiles[1]);
    {
        let mut forage = app.world.resource_mut::<ForageRegistry>();
        for tile in [sole, shared] {
            forage.patches.get_mut(&tile).unwrap().owner = Some(HOME);
        }
    }
    let herd = {
        let mut herds = app.world.resource_mut::<HerdRegistry>();
        let herd = herds
            .herds
            .first_mut()
            .expect("fixture: the world carries herds");
        herd.owner = Some(HOME);
        herd.id.clone()
    };
    (sole, shared, herd)
}

#[test]
fn a_band_that_goes_over_takes_the_improvements_only_it_works() {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    tie(&mut app, s.home_id, s.rival_id);
    // A second band of the home people, sharing one patch with the band that will go over.
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, s.home, 10, &settle)
        .expect("the staged band can split");
    let sibling = entity_of(&mut app, split.band);
    let floor = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .parent_min_workers;
    with_cohort(&mut app, s.home, |c| {
        c.working = Scalar::from_u32(floor);
        c.children = Scalar::zero();
        c.elders = Scalar::zero();
    });
    let (sole, shared, herd) = owned_improvements(&mut app);
    work(&mut app, s.home, forage_row(sole));
    work(&mut app, s.home, forage_row(shared));
    work(
        &mut app,
        s.home,
        LaborTarget::Hunt {
            fauna_id: herd.clone(),
            floor: DEFAULT_ESCAPEMENT_FLOOR,
        },
    );
    work(&mut app, sibling, forage_row(shared));

    migrate(&mut app);

    assert_eq!(
        cohort(&app, s.home).faction,
        RIVAL,
        "fixture: the band went over"
    );
    assert_eq!(
        cohort(&app, sibling).faction,
        HOME,
        "fixture: its sibling did not"
    );
    let forage = app.world.resource::<ForageRegistry>();
    assert_eq!(
        forage.patches[&sole].owner,
        Some(RIVAL),
        "a patch only it worked goes with it"
    );
    assert_eq!(
        forage.patches[&shared].owner,
        Some(HOME),
        "a patch a band of its old people still works stays theirs"
    );
    assert_eq!(
        app.world
            .resource::<HerdRegistry>()
            .find(&herd)
            .unwrap()
            .owner,
        Some(RIVAL),
        "and so does a herd only it worked"
    );
}

/// Seat a fully worn trail on every tile of the path between two points — a kept road by
/// arithmetic, since the free floor owes nothing.
fn trail_between(app: &mut App, a: UVec2, b: UVec2) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let config = app.world.resource::<SimulationConfig>().clone();
    let (width, height, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let path = {
        let roads = app.world.resource::<RoadRegistry>();
        trace_path(a, b, width, height, wrap, roads)
    };
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    for tile in path {
        roads
            .road_or_trail(tile, &ladder)
            .set_position(traffic_ceiling(&ladder), &ladder);
    }
}

/// How many hex steps past plain `base_reach` a staged destination stands.
const JUST_OUT_OF_REACH: u32 = 1;

/// Move `band` to the tile `base_reach + JUST_OUT_OF_REACH` hex steps along `at`'s row.
fn place_just_out_of_reach(app: &mut App, band: Entity, at: UVec2) -> UVec2 {
    let reach = app
        .world
        .resource::<WellbeingConfigHandle>()
        .get()
        .migration
        .base_reach as u32;
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    let there = UVec2::new((at.x + reach + JUST_OUT_OF_REACH) % width, at.y);
    let tile = tile_at(app, there.x, there.y);
    with_cohort(app, band, |c| {
        c.home = tile;
        c.current_tile = tile;
    });
    there
}

/// Stage a miserable home band and a thriving destination just out of plain reach, with or without
/// a trail between them. `foreign`: the destination is the rival's camp (tied); otherwise it is a
/// home-people splinter.
fn reach_along_a_road(foreign: bool, road: bool) -> bool {
    let mut app = two_faction_world();
    let s = stage(&mut app);
    let at = position_of(&app, s.home);
    let destination = if foreign {
        tie(&mut app, s.home_id, s.rival_id);
        s.rival
    } else {
        let settle = SettleConfig {
            min_founding_workers: 1,
            parent_min_workers: 0,
        };
        let split = split_band_from_parent(&mut app.world, s.home, 10, &settle)
            .expect("the staged band can split");
        let child = entity_of(&mut app, split.band);
        with_cohort(&mut app, child, |c| c.morale = scalar_from_f32(THRIVING));
        // The rival is not tied, so it is no destination; only the splinter can take them.
        child
    };
    let there = place_just_out_of_reach(&mut app, destination, at);
    if road {
        trail_between(&mut app, at, there);
    }
    let before = cohort(&app, s.home).total();
    migrate(&mut app);
    cohort(&app, s.home).total() < before
}

#[test]
fn a_band_just_out_of_reach_is_brought_in_by_a_road_between_them() {
    for foreign in [false, true] {
        assert!(
            !reach_along_a_road(foreign, false),
            "foreign={foreign}: just out of reach and no road — nobody moves"
        );
        assert!(
            reach_along_a_road(foreign, true),
            "foreign={foreign}: the same pair with a road between them — they move"
        );
    }
}
