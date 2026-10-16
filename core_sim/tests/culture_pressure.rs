//! **Culture strength and pressure** (#820, `docs/plan_minor_peoples.md` §"Peaceful absorption",
//! `.claude/rules/core_sim/factions.md` → "Culture strength and pressure").
//!
//! ```text
//! strength = base + Σ weight_i × (input_i / reference_i)
//! pressure = (strength_B / strength_A) × (people_B / people_A) ^ size_exponent
//! ```
//!
//! The arms stage a two-people world, wipe the contact ledger, and state only the ties each arm
//! needs. The system is run directly, one call per turn; the wire arms read the ENCODED envelope.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::save::{decode_save, encode_save};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_culture_pressure, culture_strength, joining_pressure, publish_baseline_snapshot,
    split_band_from_parent, BandId, BeliefRegistry, CommandEventKind, CommandEventLog,
    ConnectionKey, ConnectionLedger, ConnectionsConfigHandle, CultureCorruptionConfigHandle,
    CultureOwner, CulturePressureLedger, DiscoveredSites, FactionId, JoiningPhase,
    PopulationCohort, ResidentBand, Scalar, SettleConfig, Sighting, SimulationConfig,
    SnapshotHistory, ViewerFaction, CULTURE_TRAIT_AXES,
};
use faction_support::{two_faction_world, HOME, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The people being pulled and the people pulling.
const A: FactionId = HOME;
const B: FactionId = RIVAL;
/// The head-count A's band is steered to; B's is solved from the pressure an arm wants.
const A_PEOPLE: u32 = 20;
/// Pressure targets between the shipped thresholds (pulled 1.5, asked 2.0, forced 4.0).
const BELOW_PULLED: f32 = 1.0;
const PULLED_ONLY: f32 = 1.7;
const ASKED_ZONE: f32 = 3.0;
const FORCED_ZONE: f32 = 5.0;
/// A culture axis gap well beyond `joining.max_culture_distance` (0.3).
const FAR_GAP: f32 = 0.9;
const CULTURE_AXIS: usize = 0;
/// Float comparison tolerance.
const EPSILON: f32 = 1e-4;
/// Workers a staging split takes.
const SPLINTER_WORKERS: u32 = 3;

fn open_settle() -> SettleConfig {
    SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    }
}

fn band_of(app: &mut App, faction: FactionId) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, band, _)| (entity, *band))
        .min_by_key(|(_, band)| *band)
        .expect("every people has an opening band")
}

fn splinter(app: &mut App, parent: Entity) -> (Entity, BandId) {
    let split = split_band_from_parent(&mut app.world, parent, SPLINTER_WORKERS, &open_settle())
        .expect("the staged band can split");
    let entity = app
        .world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == split.band)
        .map(|(entity, _)| entity)
        .expect("the splinter is alive");
    (entity, split.band)
}

fn wipe_ties(app: &mut App) {
    app.world.insert_resource(ConnectionLedger::default());
}

/// Contact through the ledger's own path, in both directions.
fn tie(app: &mut App, a: BandId, b: BandId) {
    let config = app.world.resource::<ConnectionsConfigHandle>().get();
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for key in [ConnectionKey::new(a, b), ConnectionKey::new(b, a)] {
        ledger.record_contact(key, &Sighting::new(UVec2::ZERO, 1, ""), 1, &config);
    }
}

fn set_size(app: &mut App, band: Entity, size: u32) {
    app.world.get_mut::<PopulationCohort>(band).unwrap().size = size;
}

fn headcount(app: &mut App, faction: FactionId) -> u32 {
    app.world
        .query::<&PopulationCohort>()
        .iter(&app.world)
        .filter(|cohort| cohort.faction == faction)
        .map(|cohort| cohort.size)
        .sum()
}

/// Every resident band's culture at the origin, so the arms state the gaps they need.
fn neutral_culture(app: &mut App) {
    let bands: Vec<BandId> = app
        .world
        .query_filtered::<&BandId, With<ResidentBand>>()
        .iter(&app.world)
        .copied()
        .collect();
    let mut manager = app.world.resource_mut::<core_sim::CultureManager>();
    for band in bands {
        if let Some(layer) = manager.band_layer_mut_by_owner(CultureOwner::from_band(band)) {
            for axis in 0..CULTURE_TRAIT_AXES {
                layer.traits.update_value(axis, Scalar::zero());
            }
        }
    }
}

fn set_axis(app: &mut App, band: BandId, value: f32) {
    let mut manager = app.world.resource_mut::<core_sim::CultureManager>();
    manager
        .band_layer_mut_by_owner(CultureOwner::from_band(band))
        .expect("the band has a culture layer")
        .traits
        .update_value(CULTURE_AXIS, core_sim::scalar_from_f32(value));
}

fn run(app: &mut App) {
    app.world.run_system_once(advance_culture_pressure);
}

fn ledger(app: &App) -> CulturePressureLedger {
    app.world.resource::<CulturePressureLedger>().clone()
}

fn pulls(app: &App) -> Vec<(FactionId, String)> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::CulturePull)
        .map(|entry| (entry.faction, entry.detail.clone().unwrap_or_default()))
        .collect()
}

fn turns(app: &App) -> u16 {
    app.world
        .resource::<CultureCorruptionConfigHandle>()
        .get()
        .culture()
        .joining()
        .turns()
}

struct Staged {
    app: App,
    a: (Entity, BandId),
    b: (Entity, BandId),
}

/// One band each, tied, neutral cultures.
fn staged() -> Staged {
    let mut app = two_faction_world();
    let a = band_of(&mut app, A);
    let b = band_of(&mut app, B);
    wipe_ties(&mut app);
    tie(&mut app, a.1, b.1);
    neutral_culture(&mut app);
    Staged { app, a, b }
}

impl Staged {
    /// Size the two peoples so the first reading's pressure is `target`. Strength does not depend
    /// on size, so one run reads the ratio and the sizes are then solved for it.
    fn steer(&mut self, target: f32) {
        run(&mut self.app);
        let ledger = ledger(&self.app);
        let ratio = ledger.strength(B).unwrap() / ledger.strength(A).unwrap();
        let exponent = self
            .app
            .world
            .resource::<CultureCorruptionConfigHandle>()
            .get()
            .culture()
            .joining()
            .size_exponent();
        set_size(&mut self.app, self.a.0, A_PEOPLE);
        let other_a = headcount(&mut self.app, A) - A_PEOPLE;
        let total_a = (A_PEOPLE + other_a) as f32;
        let total_b = (total_a * (target / ratio).powf(1.0 / exponent)).round() as u32;
        set_size(&mut self.app, self.b.0, 1);
        let other_b = headcount(&mut self.app, B) - 1;
        set_size(
            &mut self.app,
            self.b.0,
            total_b.saturating_sub(other_b).max(1),
        );
    }

    fn reading(&self) -> Option<core_sim::CulturePressureReading> {
        self.app
            .world
            .resource::<CulturePressureLedger>()
            .reading(self.a.1, B)
            .copied()
    }
}

#[test]
fn strength_counts_each_anchor_once_and_each_foreign_band_once() {
    let mut s = staged();
    // Three bands of A, one band more of B.
    let a2 = splinter(&mut s.app, s.a.0);
    let a3 = splinter(&mut s.app, s.a.0);
    let b2 = splinter(&mut s.app, s.b.0);
    wipe_ties(&mut s.app);
    // Every A band is tied to B's first band; A's first band is tied to B's second too.
    for a in [s.a.1, a2.1, a3.1] {
        tie(&mut s.app, a, s.b.1);
    }
    tie(&mut s.app, s.a.1, b2.1);

    // Two distinct anchors, one shared by two bands: 10 + 4, not 10 + 10 + 4.
    let (first, second) = (UVec2::new(3, 4), UVec2::new(5, 6));
    {
        let mut belief = s.app.world.resource_mut::<BeliefRegistry>();
        belief.add(first, 10.0);
        belief.add(second, 4.0);
    }
    for (entity, anchor) in [(s.a.0, first), (a2.0, first), (a3.0, second)] {
        s.app
            .world
            .get_mut::<PopulationCohort>(entity)
            .unwrap()
            .belief_anchor = Some(anchor);
    }
    run(&mut s.app);

    let config = s
        .app
        .world
        .resource::<CultureCorruptionConfigHandle>()
        .get();
    let strength = config.culture().strength();
    let discoveries = |faction| {
        s.app
            .world
            .resource::<DiscoveredSites>()
            .for_faction(faction)
            .len()
    };
    // A holds 14 belief and is tied to 2 distinct foreign bands (B's two), through 4 edges.
    let expected_a = culture_strength(strength, 14.0, 2, discoveries(A));
    // B holds no anchor and is tied to 3 distinct foreign bands (A's three), through 4 edges.
    let expected_b = culture_strength(strength, 0.0, 3, discoveries(B));
    let ledger = ledger(&s.app);
    assert!((ledger.strength(A).unwrap() - expected_a).abs() < EPSILON);
    assert!((ledger.strength(B).unwrap() - expected_b).abs() < EPSILON);
    // And the formula itself, from known inputs at the shipped levers:
    // 1 + 1.0 × 14/20 + 0.5 × 2/4 + 0.5 × 3/2.
    assert!((culture_strength(strength, 14.0, 2, 3) - (1.0 + 0.7 + 0.25 + 0.75)).abs() < EPSILON);
}

#[test]
fn pressure_is_the_strength_ratio_times_the_size_ratio_to_the_exponent() {
    let mut s = staged();
    s.app
        .world
        .resource_mut::<BeliefRegistry>()
        .add(UVec2::new(3, 4), 40.0);
    s.app
        .world
        .get_mut::<PopulationCohort>(s.b.0)
        .unwrap()
        .belief_anchor = Some(UVec2::new(3, 4));
    set_size(&mut s.app, s.a.0, 20);
    set_size(&mut s.app, s.b.0, 45);
    run(&mut s.app);

    let ledger = ledger(&s.app);
    let (sa, sb) = (ledger.strength(A).unwrap(), ledger.strength(B).unwrap());
    let (pa, pb) = (headcount(&mut s.app, A), headcount(&mut s.app, B));
    let expected = (sb / sa) * (pb as f32 / pa as f32).sqrt();
    let reading = s.reading().expect("B pulls A's band");
    assert!((reading.pressure - expected).abs() < EPSILON);
    // The reverse pull is 1 / expected, well under `pulled`: B's band has no row toward A.
    assert!(ledger.reading(s.b.1, A).is_none());
}

#[test]
fn near_equal_sizes_and_triple_the_strength_read_about_two_point_nine() {
    let config = CultureCorruptionConfigHandle::load_builtin().get();
    let joining = config.culture().joining();
    // One person smaller, three times the strength: 3 × sqrt(12 / 13) = 2.882.
    let pressure = joining_pressure(joining, 1.0, 3.0, 13, 12);
    assert!((pressure - 3.0 * (12.0_f32 / 13.0).sqrt()).abs() < EPSILON);
    assert!((2.85..2.95).contains(&pressure), "got {pressure}");
    // Equal sizes read the bare strength ratio.
    assert!((joining_pressure(joining, 1.0, 3.0, 20, 20) - 3.0).abs() < EPSILON);
}

#[test]
fn headcount_is_the_whole_people_parties_included() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    run(&mut s.app);
    let alone = s.reading().expect("steered into the asked zone");
    let people_b = headcount(&mut s.app, B);

    // A detached party of B: a cohort with no band id and no `ResidentBand`.
    let mut party = s.app.world.get::<PopulationCohort>(s.b.0).unwrap().clone();
    party.size = people_b;
    s.app.world.spawn(party);
    run(&mut s.app);
    let with_party = s.reading().expect("still pulled");
    // B's people doubled: the pressure grows by sqrt(2), and by nothing else.
    let expected = alone.pressure * 2.0_f32.sqrt();
    assert!((with_party.pressure - expected).abs() < 1e-3);
}

#[test]
fn a_band_with_no_live_tie_to_a_people_gets_no_entry() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    run(&mut s.app);
    assert!(s.reading().is_some());

    wipe_ties(&mut s.app);
    run(&mut s.app);
    assert!(s.reading().is_none(), "no tie, no pressure");
    assert!(ledger(&s.app).pairs.is_empty());
    // A people always reads its own strength, tied or not.
    assert!(ledger(&s.app).strength(A).is_some());
}

#[test]
fn a_pair_below_pulled_is_not_stored() {
    let mut s = staged();
    s.steer(BELOW_PULLED);
    run(&mut s.app);
    assert!(s.reading().is_none());
    assert!(pulls(&s.app).is_empty());
}

#[test]
fn pulled_is_immediate_and_asked_comes_after_exactly_turns_held_turns() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    let turns = turns(&s.app);

    for held in 1..turns {
        run(&mut s.app);
        let reading = s.reading().expect("pulled at once");
        assert_eq!(reading.phase, JoiningPhase::Pulled, "turn {held}");
        assert_eq!(reading.asked_turns, held);
        assert!(reading.close);
    }
    run(&mut s.app);
    let reading = s.reading().unwrap();
    assert_eq!(reading.phase, JoiningPhase::Asked);
    assert_eq!(reading.asked_turns, turns);
    assert_eq!(reading.forced_turns, 0, "3.0 never reaches forced");
}

#[test]
fn the_clock_resets_the_turn_closeness_fails() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    let turns = turns(&s.app);
    for _ in 0..turns - 1 {
        run(&mut s.app);
    }
    assert_eq!(s.reading().unwrap().asked_turns, turns - 1);

    // The band's culture drifts out of reach of B's: the clock restarts, the pull stands.
    set_axis(&mut s.app, s.a.1, FAR_GAP);
    run(&mut s.app);
    let reading = s.reading().expect("pulled needs no closeness");
    assert!(!reading.close);
    assert_eq!(reading.asked_turns, 0);
    assert_eq!(reading.phase, JoiningPhase::Pulled);

    // Close again: a full `turns` held turns are owed afresh.
    set_axis(&mut s.app, s.a.1, 0.0);
    for _ in 0..turns - 1 {
        run(&mut s.app);
        assert_eq!(s.reading().unwrap().phase, JoiningPhase::Pulled);
    }
    run(&mut s.app);
    assert_eq!(s.reading().unwrap().phase, JoiningPhase::Asked);
}

#[test]
fn forced_comes_after_turns_at_or_above_the_forced_line() {
    let mut s = staged();
    s.steer(FORCED_ZONE);
    let turns = turns(&s.app);
    for _ in 0..turns - 1 {
        run(&mut s.app);
        assert_eq!(s.reading().unwrap().phase, JoiningPhase::Pulled);
    }
    run(&mut s.app);
    let reading = s.reading().unwrap();
    // Both clocks have run the same turns, and forced outranks asked.
    assert_eq!(reading.asked_turns, turns);
    assert_eq!(reading.forced_turns, turns);
    assert_eq!(reading.phase, JoiningPhase::Forced);

    // Falling back under the forced line resets only that clock.
    s.steer(ASKED_ZONE);
    run(&mut s.app);
    let reading = s.reading().unwrap();
    assert_eq!(reading.forced_turns, 0);
    assert_eq!(reading.phase, JoiningPhase::Asked);
}

#[test]
fn a_pair_that_drops_out_loses_its_clock() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    for _ in 0..3 {
        run(&mut s.app);
    }
    assert_eq!(s.reading().unwrap().asked_turns, 3);
    s.steer(BELOW_PULLED);
    run(&mut s.app);
    assert!(s.reading().is_none());
    s.steer(ASKED_ZONE);
    run(&mut s.app);
    assert_eq!(
        s.reading().unwrap().asked_turns,
        1,
        "a new pull starts at one"
    );
}

#[test]
fn an_entry_read_for_another_owner_restarts() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    for _ in 0..4 {
        run(&mut s.app);
    }
    assert_eq!(s.reading().unwrap().asked_turns, 4);
    // The band changed hands: the entry was read for A, the band is now B's own.
    s.app
        .world
        .get_mut::<PopulationCohort>(s.a.0)
        .unwrap()
        .faction = B;
    run(&mut s.app);
    assert!(
        s.reading().is_none(),
        "a band is never pulled by its own people"
    );
}

#[test]
fn the_culture_pull_event_fires_once_on_entry_and_not_the_next_turn() {
    let mut s = staged();
    s.steer(PULLED_ONLY);
    run(&mut s.app);
    let pulls_after_entry = pulls(&s.app);
    assert_eq!(pulls_after_entry.len(), 1);
    let (filed_under, detail) = &pulls_after_entry[0];
    assert_eq!(*filed_under, A, "filed under the band's owner");
    assert_eq!(detail, &format!("band={} faction={}", s.a.1 .0, B.0));

    run(&mut s.app);
    assert_eq!(pulls(&s.app).len(), 1, "not every turn");

    // Pulled -> Asked is not an entry.
    s.steer(ASKED_ZONE);
    for _ in 0..turns(&s.app) {
        run(&mut s.app);
    }
    assert_eq!(s.reading().unwrap().phase, JoiningPhase::Asked);
    assert_eq!(pulls(&s.app).len(), 1, "a later phase fires nothing here");

    // Dropping out and being pulled again is a new entry.
    s.steer(BELOW_PULLED);
    run(&mut s.app);
    s.steer(PULLED_ONLY);
    run(&mut s.app);
    assert_eq!(pulls(&s.app).len(), 2);
}

#[test]
fn the_ledger_round_trips_the_checkpoint_and_the_save() {
    let mut s = staged();
    s.steer(ASKED_ZONE);
    for _ in 0..4 {
        run(&mut s.app);
    }
    let staged_ledger = ledger(&s.app);
    assert!(!staged_ledger.pairs.is_empty() && !staged_ledger.strengths.is_empty());

    let checkpoint = capture_sim_state(&s.app.world);
    s.app
        .world
        .insert_resource(CulturePressureLedger::default());
    restore_sim_state(&mut s.app.world, &checkpoint);
    assert_eq!(ledger(&s.app), staged_ledger);

    let blob = encode_save(&s.app.world).expect("the world encodes");
    let (_, payload) = decode_save(&blob).expect("the save decodes");
    assert_eq!(payload.sim.culture_pressure, staged_ledger);
}

fn published(app: &mut App, viewer: FactionId, band: BandId) -> Vec<(u32, f32, u8, bool, u16)> {
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
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("populations are published");
    let row = rows
        .iter()
        .find(|row| row.bandId() == band.0)
        .expect("the band has a row for this viewer");
    row.culturePressures()
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r.faction(),
                        r.pressure(),
                        r.phase(),
                        r.close(),
                        r.turnsToNext(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_owner_sees_the_rows_and_a_foreign_viewer_sees_none() {
    let mut s = staged();
    // A second people pulling the same band, so the sort has two rows to order.
    s.steer(ASKED_ZONE);
    let turns = turns(&s.app);
    for _ in 0..3 {
        run(&mut s.app);
    }

    let rows = published(&mut s.app, A, s.a.1);
    assert_eq!(rows.len(), 1);
    let (faction, pressure, phase, close, to_next) = rows[0];
    assert_eq!(faction, B.0);
    assert!((pressure - s.reading().unwrap().pressure).abs() < EPSILON);
    assert_eq!(phase, 1, "pulled");
    assert!(close);
    assert_eq!(to_next, turns - 3, "the asked clock has 3 turns on it");

    // A rival seeing the band (fog off, so the redacted row exists) reads nothing of it.
    s.app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
    let foreign = published(&mut s.app, B, s.a.1);
    assert!(foreign.is_empty(), "a foreign row is redacted: {foreign:?}");
}
