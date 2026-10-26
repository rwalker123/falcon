//! **Cohesion** (issue #700, `docs/plan_civilization_steps.md` §"Cohesion is a property of a
//! group"): a band can only hold together at about 150 people. The unit is ONE BAND's own size —
//! how many people can keep track of each other — not how many a tile holds. The band's cohesion
//! ceiling is `base_ceiling + belief_lift × s` (`s = b / (b + half)`, `b` the belief on the tile
//! the band stands on), and it reads `−drag × max(0, size − ceiling) / ceiling`.
//!
//! Driven through the **real** `simulate_population` on a generated world. Every expected number is
//! derived from the shipped config the world loaded, never restated; each claim has its control.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::save::{decode_save, encode_save};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_population_migration, publish_baseline_snapshot, scalar_from_f32, simulate_population,
    split_band_from_parent, BandId, BeliefRegistry, MoraleCause, PopulationCohort, ResidentBand,
    Scalar, SettleConfig, SimulationConfig, SnapshotHistory, Tile, TileRegistry, WellbeingConfig,
    WellbeingConfigHandle,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// A larder many lifetimes deep, so a staged turn kills nobody by starvation.
const DEEP_LARDER: f32 = 1.0e6;
/// The age split a staged head-count is divided into (children, working, elders).
const CHILDREN_SHARE: f32 = 0.3;
const WORKING_SHARE: f32 = 0.6;
const ELDERS_SHARE: f32 = 0.1;
/// A band this far over the ceiling (a share of the ceiling) is oversized.
const OVER_SHARE: f32 = 1.2;
/// Two bands each at this share of the ceiling are under it apart and over it together.
const HALF_PLUS_SHARE: f32 = 0.6;
/// A small band, far under any ceiling.
const SMALL_BAND: f32 = 10.0;
/// Twice the ceiling: the excess is exactly one ceiling's worth.
const DOUBLE_SHARE: f32 = 2.0;
/// Workers split off for the second band of one people.
const SPLIT_WORKERS: u32 = 5;
/// Fixed-point comparison tolerance, in raw units.
const RAW_TOLERANCE: i64 = 2;
/// The wire value of `MoraleCause::Cohesion`.
const COHESION_CAUSE_WIRE: u8 = 5;
/// A drag that dwarfs every other contributor, so cohesion is the dominant negative one.
const OVERWHELMING_DRAG: f32 = 1.0;
/// Morale the leak arm stages: just above migration's onset, so cohesion carries the band under it.
const JUST_ABOVE_ONSET_MARGIN: f32 = 0.01;
/// The happier band's morale — above `attractive_morale` and any source + gap.
const THRIVING: f32 = 0.95;
/// Turns the leak is given to start.
const LEAK_TURNS: usize = 8;

fn wellbeing(app: &App) -> std::sync::Arc<WellbeingConfig> {
    app.world.resource::<WellbeingConfigHandle>().get()
}

fn tune(app: &mut App, edit: impl FnOnce(&mut WellbeingConfig)) {
    let mut tuned = (*wellbeing(app)).clone();
    edit(&mut tuned);
    app.world
        .insert_resource(WellbeingConfigHandle::new(std::sync::Arc::new(tuned)));
}

fn opening_band(app: &mut App, faction: core_sim::FactionId) -> Entity {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .min_by_key(|(_, id, _)| **id)
        .map(|(entity, _, _)| entity)
        .unwrap_or_else(|| panic!("{faction:?} has an opening band"))
}

fn tile_at(app: &App, position: UVec2) -> Entity {
    app.world
        .resource::<TileRegistry>()
        .index(position.x, position.y)
        .expect("the staged tile is on the map")
}

fn position_of(app: &App, band: Entity) -> UVec2 {
    let tile = cohort(app, band).current_tile;
    app.world.get::<Tile>(tile).unwrap().position
}

fn beside(app: &App, at: UVec2) -> UVec2 {
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    UVec2::new((at.x + 1) % width, at.y)
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap()
}

fn stand_on(app: &mut App, band: Entity, position: UVec2) {
    let tile = tile_at(app, position);
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .current_tile = tile;
}

/// Set a band's head-count (rounded to whole people by the age split) and give it a deep larder.
fn stage_size(app: &mut App, band: Entity, people: f32) {
    let mut c = app.world.get_mut::<PopulationCohort>(band).unwrap();
    c.set_brackets_from_size(
        people.round() as u32,
        CHILDREN_SHARE,
        WORKING_SHARE,
        ELDERS_SHARE,
    );
    c.stores.reset_food("dry", scalar_from_f32(DEEP_LARDER));
    c.belief_anchor = None;
}

fn cohesion_of(app: &App, band: Entity) -> Scalar {
    cohort(app, band).last_morale_contributions.cohesion
}

fn morale_turn(app: &mut App) {
    app.world.run_system_once(simulate_population);
}

/// `−drag × max(0, size − ceiling) / ceiling` from the lever values, spelled out here rather than
/// through the config's own method.
fn expected_term(group: f32, ceiling: f32, drag: f32) -> Scalar {
    scalar_from_f32(-drag * (group - ceiling).max(0.0) / ceiling)
}

fn assert_scalar_eq(actual: Scalar, expected: Scalar, what: &str) {
    assert!(
        (actual.raw() - expected.raw()).abs() <= RAW_TOLERANCE,
        "{what}: got {}, expected {}",
        actual.to_f32(),
        expected.to_f32()
    );
}

fn zero() -> Scalar {
    Scalar::from_i64(0)
}

/// A world with an empty belief registry and the home faction's opening band alone on its tile.
fn lone_band_world() -> (App, Entity) {
    let mut app = one_faction_world();
    app.world.insert_resource(BeliefRegistry::default());
    let band = opening_band(&mut app, HOME);
    let home = cohort(&app, band).home;
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .current_tile = home;
    (app, band)
}

/// Split a second band of the home people off `band`; returns its entity. It starts on the same
/// tile as its parent.
fn second_band(app: &mut App, band: Entity) -> Entity {
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, band, SPLIT_WORKERS, &settle)
        .expect("the band can split");
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == split.band)
        .map(|(entity, _)| entity)
        .expect("the daughter is alive")
}

/// **A lone band, priced on its own size.** Under the ceiling it reads 0; forced over it, it reads exactly
/// `−drag × excess / ceiling`, and with cohesion dominant the cause is wire `5`.
#[test]
fn a_lone_band_reads_zero_under_the_ceiling_and_the_priced_term_over_it() {
    let (mut app, band) = lone_band_world();
    let cohesion = wellbeing(&app).cohesion.clone();

    morale_turn(&mut app);
    assert!(
        f64::from(cohort(&app, band).total().to_f32()) < f64::from(cohesion.base_ceiling),
        "fixture: the opening band is under the ceiling"
    );
    assert_eq!(
        cohesion_of(&app, band),
        zero(),
        "under the ceiling: no term"
    );

    // The first turn buried a fraction of a death on the tile; start the priced turn from none.
    app.world.insert_resource(BeliefRegistry::default());
    stage_size(&mut app, band, cohesion.base_ceiling * OVER_SHARE);
    let group = cohort(&app, band).total().to_u32() as f32;
    morale_turn(&mut app);
    assert_scalar_eq(
        cohesion_of(&app, band),
        expected_term(group, cohesion.base_ceiling, cohesion.drag),
        "over the ceiling",
    );
    assert!(cohesion_of(&app, band) < zero(), "liveness: a real drag");

    tune(&mut app, |config| config.cohesion.drag = OVERWHELMING_DRAG);
    app.world.insert_resource(BeliefRegistry::default());
    stage_size(&mut app, band, cohesion.base_ceiling * OVER_SHARE);
    morale_turn(&mut app);
    let c = cohort(&app, band);
    assert_eq!(c.last_morale_cause, MoraleCause::Cohesion);
    assert_eq!(c.last_morale_cause.as_u8(), COHESION_CAUSE_WIRE);
}

/// **Two bands of one people on one tile do not add up.** Each under the ceiling, together over it:
/// NEITHER is charged. Control: one band over the ceiling is charged, whether alone or sharing its
/// tile with a small one.
#[test]
fn bands_sharing_a_tile_are_priced_each_on_its_own_size() {
    let cohesion = WellbeingConfig::builtin().cohesion.clone();
    let each = cohesion.base_ceiling * HALF_PLUS_SHARE;

    let (mut app, first) = lone_band_world();
    let second = second_band(&mut app, first);
    stage_size(&mut app, first, each);
    stage_size(&mut app, second, each);
    assert_eq!(
        cohort(&app, first).current_tile,
        cohort(&app, second).current_tile,
        "fixture: one tile"
    );
    let together = (cohort(&app, first).total() + cohort(&app, second).total()).to_u32() as f32;
    assert!(together > cohesion.base_ceiling, "fixture: over together");
    morale_turn(&mut app);
    assert_eq!(cohesion_of(&app, first), zero(), "first band");
    assert_eq!(cohesion_of(&app, second), zero(), "second band");

    // Control: one oversized band beside a small one on the same tile; only the oversized pays.
    app.world.insert_resource(BeliefRegistry::default());
    stage_size(&mut app, first, cohesion.base_ceiling * OVER_SHARE);
    stage_size(&mut app, second, SMALL_BAND);
    let size = cohort(&app, first).total().to_u32() as f32;
    morale_turn(&mut app);
    let want = expected_term(size, cohesion.base_ceiling, cohesion.drag);
    assert!(want < zero(), "liveness: the oversized band is charged");
    assert_scalar_eq(cohesion_of(&app, first), want, "oversized band");
    assert_eq!(cohesion_of(&app, second), zero(), "small band on its tile");
}

/// **Belief on the band's tile lifts its ceiling** by `belief_lift × s`; belief on an adjacent
/// tile does nothing.
#[test]
fn belief_underfoot_raises_the_ceiling_and_belief_beside_does_not() {
    let cohesion = WellbeingConfig::builtin().cohesion.clone();
    let people = cohesion.base_ceiling * DOUBLE_SHARE;
    let belief = cohesion.belief_half_saturation;
    let s = belief / (belief + cohesion.belief_half_saturation);
    let lifted = cohesion.base_ceiling + cohesion.belief_lift * s;

    let read = |staged: Option<(bool, f32)>| -> (Scalar, u32, f32) {
        let (mut app, band) = lone_band_world();
        stage_size(&mut app, band, people);
        let at = position_of(&app, band);
        if let Some((underfoot, amount)) = staged {
            let place = if underfoot { at } else { beside(&app, at) };
            app.world
                .resource_mut::<BeliefRegistry>()
                .add(place, amount);
        }
        let group = cohort(&app, band).total().to_u32() as f32;
        morale_turn(&mut app);
        (
            cohesion_of(&app, band),
            cohort(&app, band).last_cohesion.ceiling,
            group,
        )
    };

    let (bare, bare_ceiling, group) = read(None);
    assert_scalar_eq(
        bare,
        expected_term(group, cohesion.base_ceiling, cohesion.drag),
        "no belief",
    );
    assert_eq!(bare_ceiling, cohesion.base_ceiling.round() as u32);

    let (under, under_ceiling, _) = read(Some((true, belief)));
    assert_scalar_eq(
        under,
        expected_term(group, lifted, cohesion.drag),
        "belief underfoot",
    );
    assert_eq!(under_ceiling, lifted.round() as u32);
    assert!(under > bare, "belief underfoot: less drag");

    let (aside, aside_ceiling, _) = read(Some((false, belief)));
    assert_eq!(aside, bare, "belief on an adjacent tile does nothing");
    assert_eq!(aside_ceiling, bare_ceiling);
}

/// **The leak.** An oversized band with a reachable, happier band of its own people loses people to it
/// through discontent migration; the same staging with the drag switched off loses no one.
#[test]
fn an_oversized_band_leaks_people_to_a_happier_neighbour() {
    let leaks = |drag: Option<f32>| -> u32 {
        let (mut app, oversized) = lone_band_world();
        let cohesion = wellbeing(&app).cohesion.clone();
        let onset = wellbeing(&app).migration.morale_threshold;
        let happier = second_band(&mut app, oversized);
        if let Some(drag) = drag {
            tune(&mut app, |config| config.cohesion.drag = drag);
        }
        stage_size(&mut app, oversized, cohesion.base_ceiling * DOUBLE_SHARE);
        stage_size(&mut app, happier, 1.0);
        let at = position_of(&app, oversized);
        let next = beside(&app, at);
        stand_on(&mut app, happier, next);
        app.world
            .get_mut::<PopulationCohort>(oversized)
            .unwrap()
            .morale = scalar_from_f32(onset + JUST_ABOVE_ONSET_MARGIN);
        let mut emigrated = 0;
        for _ in 0..LEAK_TURNS {
            app.world
                .get_mut::<PopulationCohort>(happier)
                .unwrap()
                .morale = scalar_from_f32(THRIVING);
            morale_turn(&mut app);
            app.world
                .get_mut::<PopulationCohort>(happier)
                .unwrap()
                .morale = scalar_from_f32(THRIVING);
            app.world.run_system_once(advance_population_migration);
            emigrated += cohort(&app, oversized).last_emigrated;
        }
        emigrated
    };

    assert!(
        leaks(None) > 0,
        "the oversized band loses people to the happier one"
    );
    assert_eq!(leaks(Some(0.0)), 0, "control: no drag, no leak");
}

/// **`moraleCohesion`, `cohesionHeadcount` and `cohesionCeiling` are on the encoded frame** and equal the
/// values the turn priced.
#[test]
fn the_reading_is_on_the_encoded_snapshot() {
    let (mut app, band) = lone_band_world();
    let cohesion = wellbeing(&app).cohesion.clone();
    stage_size(&mut app, band, cohesion.base_ceiling * OVER_SHARE);
    morale_turn(&mut app);
    let c = cohort(&app, band);
    let (term, reading) = (c.last_morale_contributions.cohesion, c.last_cohesion);
    assert!(term < zero(), "fixture: a live, non-zero term");
    assert!(
        reading.headcount > 0 && reading.ceiling > 0,
        "fixture: priced"
    );

    publish_baseline_snapshot(&mut app.world);
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
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.entity() == band.to_bits())
        .expect("the band is on the wire");
    assert_eq!(row.moraleCohesion(), term.raw());
    assert_eq!(row.cohesionHeadcount(), reading.headcount);
    assert_eq!(row.cohesionCeiling(), reading.ceiling);
}

/// **The reading rides the checkpoint and the save** with the rest of the cohort.
#[test]
fn the_reading_round_trips_the_checkpoint_and_the_save() {
    let (mut app, band) = lone_band_world();
    let cohesion = wellbeing(&app).cohesion.clone();
    stage_size(&mut app, band, cohesion.base_ceiling * OVER_SHARE);
    morale_turn(&mut app);
    let band_id = *app.world.get::<BandId>(band).unwrap();
    let (term, reading) = {
        let c = cohort(&app, band);
        (c.last_morale_contributions.cohesion, c.last_cohesion)
    };
    assert!(
        term < zero() && reading.headcount > 0,
        "fixture: a live reading"
    );

    let checkpoint = capture_sim_state(&app.world);
    {
        let mut c = app.world.get_mut::<PopulationCohort>(band).unwrap();
        c.last_cohesion = core_sim::CohesionReading::default();
        c.last_morale_contributions.cohesion = zero();
    }
    restore_sim_state(&mut app.world, &checkpoint);
    let (restored_term, restored) = app
        .world
        .query::<(&BandId, &PopulationCohort)>()
        .iter(&app.world)
        .find(|(id, _)| **id == band_id)
        .map(|(_, c)| (c.last_morale_contributions.cohesion, c.last_cohesion))
        .expect("the band is restored");
    assert_eq!(restored, reading);
    assert_eq!(restored_term, term);

    let blob = encode_save(&app.world).expect("the world encodes");
    let (_, payload) = decode_save(&blob).expect("the save decodes");
    let saved = payload
        .sim
        .bands
        .iter()
        .find(|record| record.id == band_id)
        .expect("the band is saved");
    assert_eq!(saved.cohort.last_cohesion, reading);
    assert_eq!(saved.cohort.last_morale_contributions.cohesion, term);
}
