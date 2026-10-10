//! **Near / far from the ancestors** (issue #699, `docs/plan_civilization_steps.md` §"What belief
//! does, through seams that exist"): each band remembers one belief tile — its anchor — and its
//! morale moves by `s × (r × near_bonus − (1 − r) × away_drag)`, `s = b / (b + belief_half_saturation)`
//! and `r` its kin-relay strength: `1` within its own walking reach (`+near_bonus × s`),
//! `relay_per_hop ^ n` through `n` bands of kin, `0` unreached (`−away_drag × s`).
//!
//! Driven through the **real** `simulate_population` on a generated world. Every arm starts from an
//! empty registry and a band with no anchor, then stages belief directly on the registry's own
//! `add` seam, so what each test asserts is the term and the anchor rule — not how many people the
//! opening turn happened to bury.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::save::{decode_save, encode_save};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    publish_baseline_snapshot, reconcile_band_culture_layers, reconcile_culture_layers, run_turn,
    scalar_from_f32, sedentarization_tick, simulate_population, split_band_from_parent,
    telling_tick, trace_path, traffic_ceiling, BandId, BandTravel, BeatLedger, BeliefConfig,
    BeliefConfigHandle, BeliefRegistry, CultureConfig, CultureManager, CultureOwner,
    CultureTraitAxis, LadderConfigHandle, MoraleCause, PopulationCohort, ResidentBand,
    RoadRegistry, Scalar, SedentarizationConfigHandle, SedentarizationScore, SettleConfig,
    SimulationConfig, SnapshotHistory, Tile, TileRegistry, WellbeingConfig, WellbeingConfigHandle,
};
use faction_support::{one_faction_world, two_faction_world, HOME, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// A few dead's worth of belief on a place.
const SOME_BELIEF: f32 = 5.0;
/// More than [`SOME_BELIEF`] — a place the anchor should move to.
const MORE_BELIEF: f32 = 8.0;
/// Hex steps from the band to a staged belief tile well inside `base_reach`.
const NEARBY_STEPS: u32 = 2;
/// How many hex steps past plain `base_reach` a just-out-of-reach tile stands.
const JUST_OUT_OF_REACH: u32 = 1;
/// A larder many lifetimes deep, so a staged turn kills nobody by starvation.
const DEEP_LARDER: f32 = 1.0e6;
/// A drag large enough that culture is the dominant negative contributor whatever the band's
/// terrain, climate and unrest read.
const OVERWHELMING_DRAG: f32 = 1.0;
/// A belief deep enough that the anchor's weight is near saturation.
const GREAT_CEMETERY: f32 = 1.0e4;
/// Workers split off for the fission arm (the band holds ~17).
const SPLIT_WORKERS: u32 = 5;
/// Fixed-point comparison tolerance, in raw units — one `scalar_from_f32` rounding either side.
const RAW_TOLERANCE: i64 = 2;
/// The wire value of `MoraleCause::Culture`.
const CULTURE_CAUSE_WIRE: u8 = 4;

struct Fixture {
    band: Entity,
    band_id: BandId,
    /// Where the band stands.
    at: UVec2,
}

/// The home faction's first band, standing on its home, with an empty registry, no anchor and a
/// deep larder.
fn fixture(app: &mut App) -> Fixture {
    let (band, band_id, tile) = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .min_by_key(|(_, id, _)| **id)
        .map(|(entity, id, cohort)| (entity, *id, cohort.home))
        .expect("the home faction has a band");
    app.world.insert_resource(BeliefRegistry::default());
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.current_tile = tile;
    cohort.belief_anchor = None;
    cohort
        .stores
        .reset_food("dry", scalar_from_f32(DEEP_LARDER));
    let at = app.world.get::<Tile>(tile).unwrap().position;
    Fixture { band, band_id, at }
}

fn wellbeing(app: &App) -> std::sync::Arc<WellbeingConfig> {
    app.world.resource::<WellbeingConfigHandle>().get()
}

fn base_reach(app: &App) -> u32 {
    wellbeing(app).migration.base_reach as u32
}

/// The tile `steps` hex steps along `from`'s row (a same-row offset is exactly that many steps).
fn along_row(app: &App, from: UVec2, steps: u32) -> UVec2 {
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    UVec2::new((from.x + steps) % width, from.y)
}

fn tile_at(app: &App, position: UVec2) -> Entity {
    app.world
        .resource::<TileRegistry>()
        .index(position.x, position.y)
        .expect("the staged tile is on the map")
}

fn stage_belief(app: &mut App, position: UVec2, amount: f32) {
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(position, amount);
}

fn stand_on(app: &mut App, band: Entity, position: UVec2) {
    let tile = tile_at(app, position);
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .current_tile = tile;
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world.get::<PopulationCohort>(band).unwrap()
}

fn culture_of(app: &App, band: Entity) -> Scalar {
    cohort(app, band).last_morale_contributions.culture
}

fn morale_turn(app: &mut App) {
    app.world.run_system_once(simulate_population);
}

/// `±lever × s` as the sim's fixed point carries it.
fn expected(lever: f32, belief: f32, culture: &CultureConfig) -> Scalar {
    scalar_from_f32(lever * culture.anchor_weight(belief))
}

fn assert_scalar_eq(actual: Scalar, expected: Scalar, what: &str) {
    assert!(
        (actual.raw() - expected.raw()).abs() <= RAW_TOLERANCE,
        "{what}: got {}, expected {}",
        actual.to_f32(),
        expected.to_f32()
    );
}

/// Seat a fully worn trail on every tile between two points — a kept road by arithmetic
/// (`defection.rs`'s fixture).
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

/// **Near the ancestors:** a band standing within reach of a belief tile makes it its anchor and
/// gains exactly `near_bonus × s`.
#[test]
fn a_band_within_reach_of_belief_gains_near_bonus_times_its_weight() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let place = along_row(&app, fx.at, NEARBY_STEPS);
    stage_belief(&mut app, place, SOME_BELIEF);

    morale_turn(&mut app);

    assert_eq!(cohort(&app, fx.band).belief_anchor, Some(place));
    let culture = wellbeing(&app).culture.clone();
    assert_scalar_eq(
        culture_of(&app, fx.band),
        expected(culture.near_bonus, SOME_BELIEF, &culture),
        "near the anchor",
    );
}

/// **Far from the ancestors:** the same band walked beyond reach keeps its anchor and loses
/// `away_drag × s`; with the drag dominant, the turn's morale cause reads Culture (wire `4`).
#[test]
fn a_band_beyond_reach_of_its_anchor_loses_away_drag_and_names_culture() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let place = along_row(&app, fx.at, NEARBY_STEPS);
    stage_belief(&mut app, place, SOME_BELIEF);
    morale_turn(&mut app);
    assert_eq!(cohort(&app, fx.band).belief_anchor, Some(place));

    let far = along_row(&app, place, base_reach(&app) + JUST_OUT_OF_REACH);
    stand_on(&mut app, fx.band, far);
    morale_turn(&mut app);

    assert_eq!(
        cohort(&app, fx.band).belief_anchor,
        Some(place),
        "walking away does not forget the ancestors"
    );
    let culture = wellbeing(&app).culture.clone();
    assert_scalar_eq(
        culture_of(&app, fx.band),
        expected(-culture.away_drag, SOME_BELIEF, &culture),
        "beyond reach of the anchor",
    );

    // The same staging with a drag that dwarfs every other contributor names Culture.
    let mut tuned = (*wellbeing(&app)).clone();
    tuned.culture.away_drag = OVERWHELMING_DRAG;
    app.world
        .insert_resource(WellbeingConfigHandle::new(std::sync::Arc::new(tuned)));
    stage_belief(&mut app, place, GREAT_CEMETERY);
    morale_turn(&mut app);
    let band = cohort(&app, fx.band);
    assert!(band.last_morale_delta < Scalar::from_i64(0), "morale fell");
    assert_eq!(band.last_morale_cause, MoraleCause::Culture);
    assert_eq!(band.last_morale_cause.as_u8(), CULTURE_CAUSE_WIRE);
}

/// **A stranger's cemetery is not yours.** Belief elsewhere on the map, never within the band's
/// reach, gives it no anchor and no term.
#[test]
fn a_band_that_never_stood_near_belief_has_no_anchor_and_no_term() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let elsewhere = along_row(&app, fx.at, base_reach(&app) + JUST_OUT_OF_REACH);
    stage_belief(&mut app, elsewhere, GREAT_CEMETERY);

    morale_turn(&mut app);

    assert!(
        !app.world.resource::<BeliefRegistry>().is_empty(),
        "fixture: belief exists on the map"
    );
    assert_eq!(cohort(&app, fx.band).belief_anchor, None);
    assert_eq!(culture_of(&app, fx.band), Scalar::from_i64(0));
}

/// **A road lengthens reach**, exactly as it lengthens migration's: a belief tile one step past
/// `base_reach` is out of reach with no road and in reach with a road laid between.
#[test]
fn a_road_brings_a_just_out_of_reach_anchor_into_reach() {
    let reached = |road: bool| -> (Option<UVec2>, Scalar, UVec2, f32) {
        let mut app = one_faction_world();
        let fx = fixture(&mut app);
        let place = along_row(&app, fx.at, base_reach(&app) + JUST_OUT_OF_REACH);
        stage_belief(&mut app, place, SOME_BELIEF);
        if road {
            trail_between(&mut app, fx.at, place);
        }
        morale_turn(&mut app);
        let near_bonus = wellbeing(&app).culture.near_bonus;
        let weight = wellbeing(&app).culture.anchor_weight(SOME_BELIEF);
        (
            cohort(&app, fx.band).belief_anchor,
            culture_of(&app, fx.band),
            place,
            near_bonus * weight,
        )
    };

    let (anchor, culture, _, _) = reached(false);
    assert_eq!(anchor, None, "no road: the place is out of reach");
    assert_eq!(culture, Scalar::from_i64(0));

    let (anchor, culture, place, near) = reached(true);
    assert_eq!(anchor, Some(place), "a road brings it into reach");
    assert_scalar_eq(culture, scalar_from_f32(near), "near over the road");
}

/// **The anchor moves to a stronger place in reach, and only a stronger one.**
#[test]
fn the_anchor_moves_to_a_stronger_tile_but_not_an_equal_one() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let first = along_row(&app, fx.at, NEARBY_STEPS);
    stage_belief(&mut app, first, SOME_BELIEF);
    morale_turn(&mut app);
    assert_eq!(cohort(&app, fx.band).belief_anchor, Some(first));

    // An equal place in reach — on the band's own side of the row, so it comes FIRST in row-major
    // order and a first-max scan with no tie rule would take it.
    let width = app.world.resource::<SimulationConfig>().grid_size.x;
    let equal = UVec2::new((fx.at.x + width - NEARBY_STEPS) % width, fx.at.y);
    stage_belief(&mut app, equal, SOME_BELIEF);
    morale_turn(&mut app);
    assert_eq!(
        cohort(&app, fx.band).belief_anchor,
        Some(first),
        "an equal place does not take the anchor"
    );

    stage_belief(&mut app, equal, MORE_BELIEF - SOME_BELIEF);
    morale_turn(&mut app);
    assert_eq!(
        cohort(&app, fx.band).belief_anchor,
        Some(equal),
        "a stronger place in reach does"
    );
}

/// **A splinter inherits its parent's anchor** — the same people, the same dead.
#[test]
fn a_fission_daughter_inherits_the_anchor() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let place = along_row(&app, fx.at, NEARBY_STEPS);
    stage_belief(&mut app, place, SOME_BELIEF);
    morale_turn(&mut app);
    assert_eq!(cohort(&app, fx.band).belief_anchor, Some(place));

    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    let split = split_band_from_parent(&mut app.world, fx.band, SPLIT_WORKERS, &settle)
        .expect("the band can split");
    let daughter = app
        .world
        .query::<(&BandId, &PopulationCohort)>()
        .iter(&app.world)
        .find(|(id, _)| **id == split.band)
        .map(|(_, cohort)| cohort.belief_anchor)
        .expect("the daughter is alive");
    assert_eq!(daughter, Some(place));
}

/// **The anchor rides the checkpoint and the save** with the rest of the cohort.
#[test]
fn the_anchor_round_trips_the_checkpoint_and_the_save() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let place = along_row(&app, fx.at, NEARBY_STEPS);
    app.world
        .get_mut::<PopulationCohort>(fx.band)
        .unwrap()
        .belief_anchor = Some(place);

    let checkpoint = capture_sim_state(&app.world);
    app.world
        .get_mut::<PopulationCohort>(fx.band)
        .unwrap()
        .belief_anchor = None;
    restore_sim_state(&mut app.world, &checkpoint);
    let restored = app
        .world
        .query::<(&BandId, &PopulationCohort)>()
        .iter(&app.world)
        .find(|(id, _)| **id == fx.band_id)
        .map(|(_, cohort)| cohort.belief_anchor)
        .expect("the band is restored");
    assert_eq!(restored, Some(place));

    let blob = encode_save(&app.world).expect("the world encodes");
    let (_, payload) = decode_save(&blob).expect("the save decodes");
    let saved = payload
        .sim
        .bands
        .iter()
        .find(|record| record.id == fx.band_id)
        .expect("the band is saved");
    assert_eq!(saved.cohort.belief_anchor, Some(place));
}

/// **`moraleCulture` is on the encoded frame** — the shipped representation — and carries the
/// cohort's own contribution.
#[test]
fn the_culture_contribution_is_on_the_encoded_snapshot() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let place = along_row(&app, fx.at, NEARBY_STEPS);
    stage_belief(&mut app, place, SOME_BELIEF);
    morale_turn(&mut app);
    let contribution = culture_of(&app, fx.band);
    assert!(
        contribution > Scalar::from_i64(0),
        "fixture: a live, non-zero term"
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
    let published = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.entity() == fx.band.to_bits())
        .expect("the band is on the wire")
        .moraleCulture();
    assert_eq!(published, contribution.raw());
}

/// What one band's row publishes about its ancestors' place, read off the encoded envelope.
struct PublishedAnchor {
    has_anchor: bool,
    anchor: UVec2,
    region: Vec<UVec2>,
}

fn published_anchor(app: &mut App, band: Entity) -> PublishedAnchor {
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
    let xs: Vec<u32> = row
        .beliefReachX()
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    let ys: Vec<u32> = row
        .beliefReachY()
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    assert_eq!(xs.len(), ys.len(), "the region's x and y lists zip");
    PublishedAnchor {
        has_anchor: row.hasBeliefAnchor(),
        anchor: UVec2::new(row.beliefAnchorX(), row.beliefAnchorY()),
        region: xs
            .into_iter()
            .zip(ys)
            .map(|(x, y)| UVec2::new(x, y))
            .collect(),
    }
}

/// A band anchored to a place `NEARBY_STEPS` along its row.
fn anchored_band(app: &mut App) -> (Fixture, UVec2) {
    let fx = fixture(app);
    let place = along_row(app, fx.at, NEARBY_STEPS);
    stage_belief(app, place, SOME_BELIEF);
    morale_turn(app);
    assert_eq!(cohort(app, fx.band).belief_anchor, Some(place));
    (fx, place)
}

/// **The anchor and its reach region ride the encoded frame**: the anchor's tile, gated, and a
/// region that holds the anchor itself and the tile the band stands on.
#[test]
fn the_anchor_and_its_reach_region_are_on_the_encoded_snapshot() {
    let mut app = one_faction_world();
    let (fx, place) = anchored_band(&mut app);

    let published = published_anchor(&mut app, fx.band);
    assert!(published.has_anchor);
    assert_eq!(published.anchor, place);
    assert!(
        published.region.contains(&place),
        "standing on the anchor is near it"
    );
    assert!(
        published.region.contains(&fx.at),
        "the band stands within reach, so its tile is in the region"
    );
    let far = along_row(&app, place, base_reach(&app) + JUST_OUT_OF_REACH);
    assert!(
        !published.region.contains(&far),
        "a tile past base_reach with no road is outside the region"
    );
}

/// **A band with no anchor ships no anchor and an empty region.**
#[test]
fn a_band_with_no_anchor_publishes_an_empty_region() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    morale_turn(&mut app);
    assert_eq!(cohort(&app, fx.band).belief_anchor, None);

    let published = published_anchor(&mut app, fx.band);
    assert!(!published.has_anchor);
    assert_eq!(published.anchor, UVec2::ZERO);
    assert!(published.region.is_empty());
}

/// **A road widens the published region exactly as it widens the term**: a tile one step past
/// `base_reach` from the anchor joins the region only once a road connects it.
#[test]
fn a_road_brings_a_just_out_of_reach_tile_into_the_published_region() {
    let mut app = one_faction_world();
    let (fx, place) = anchored_band(&mut app);
    let beyond = along_row(&app, place, base_reach(&app) + JUST_OUT_OF_REACH);

    assert!(
        !published_anchor(&mut app, fx.band).region.contains(&beyond),
        "no road: the tile is outside the region"
    );
    trail_between(&mut app, beyond, place);
    assert!(
        published_anchor(&mut app, fx.band).region.contains(&beyond),
        "a road between them brings it in"
    );
    // And the change rode the stream's delta, not only the full frame.
    let delta = app
        .world
        .resource::<SnapshotHistory>()
        .last_delta()
        .expect("a delta per publication");
    let row = delta
        .populations
        .iter()
        .find(|row| row.entity == fx.band.to_bits())
        .expect("a band whose reach region moved rides the delta");
    assert!(row
        .belief_reach_x
        .iter()
        .zip(&row.belief_reach_y)
        .any(|(&x, &y)| UVec2::new(x, y) == beyond));
}

/// **The drawn region and the term cannot disagree.** With a road bending the region out of a plain
/// disk, a band standing on EVERY published tile reads the near value, and on every tile bordering
/// the region reads the away value.
#[test]
fn the_published_region_agrees_with_the_term_at_every_tile() {
    let mut app = one_faction_world();
    let (fx, place) = anchored_band(&mut app);
    let beyond = along_row(&app, place, base_reach(&app) + JUST_OUT_OF_REACH);
    trail_between(&mut app, beyond, place);

    let region = published_anchor(&mut app, fx.band).region;
    let config = app.world.resource::<SimulationConfig>().clone();
    let (width, height, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    // Every tile one step outside the region: the region's neighbourhood minus the region.
    let mut border: Vec<UVec2> = region
        .iter()
        .flat_map(|&tile| core_sim::grid_utils::hex_range_tiles(tile, 1, width, height, wrap))
        .filter(|tile| !region.contains(tile))
        .collect();
    border.sort_by_key(|tile| (tile.y, tile.x));
    border.dedup();
    assert!(!border.is_empty(), "fixture: the region has an edge");

    let zero = Scalar::from_i64(0);
    for &tile in &region {
        stand_on(&mut app, fx.band, tile);
        morale_turn(&mut app);
        assert_eq!(cohort(&app, fx.band).belief_anchor, Some(place));
        assert!(
            culture_of(&app, fx.band) > zero,
            "standing on {tile} inside the region must read near"
        );
    }
    for &tile in &border {
        stand_on(&mut app, fx.band, tile);
        morale_turn(&mut app);
        assert_eq!(cohort(&app, fx.band).belief_anchor, Some(place));
        assert!(
            culture_of(&app, fx.band) < zero,
            "standing on {tile} just outside the region must read away"
        );
    }
}

/// Just under one whole death's worth of belief.
const UNDER_ONE_DEATH: f32 = 0.9;
/// Exactly one whole death's worth — the shipped `min_anchor_belief`.
const ONE_DEATH: f32 = 1.0;
/// A retuned `min_anchor_belief`, and a place that falls short of it and one that meets it.
const RAISED_MIN_ANCHOR: f32 = 3.0;
const SHORT_OF_RAISED_MIN: f32 = 2.0;
/// Turns of ordinary mortality the reported bug took to anchor a band to its start tile.
const QUIET_TURNS: usize = 2;

fn with_min_anchor_belief(app: &mut App, min_anchor_belief: f32) {
    let mut tuned = (*wellbeing(app)).clone();
    tuned.culture.min_anchor_belief = min_anchor_belief;
    app.world
        .insert_resource(WellbeingConfigHandle::new(std::sync::Arc::new(tuned)));
}

/// **The reported bug.** Turns of ordinary mortality credit the band's start tile a FRACTION of a
/// death; that fraction must not anchor the band, put an anchor on the wire, or draw a region.
#[test]
fn fractional_deaths_on_the_start_tile_do_not_anchor_the_band() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    for _ in 0..QUIET_TURNS {
        morale_turn(&mut app);
    }
    let accrued = app.world.resource::<BeliefRegistry>().get(fx.at);
    assert!(
        accrued > 0.0 && accrued < ONE_DEATH,
        "fixture: the start tile holds a fraction of a death ({accrued})"
    );
    assert_eq!(cohort(&app, fx.band).belief_anchor, None);
    assert_eq!(culture_of(&app, fx.band), Scalar::from_i64(0));
    let published = published_anchor(&mut app, fx.band);
    assert!(!published.has_anchor, "no anchor on the wire");
    assert!(published.region.is_empty(), "no region drawn");
}

/// **One whole death is the line**: a place at 0.9 in reach is not adopted and gives no term; the
/// same place at 1.0 is.
#[test]
fn a_place_is_adopted_only_once_it_holds_one_whole_death() {
    let under = {
        let mut app = one_faction_world();
        let fx = fixture(&mut app);
        let place = along_row(&app, fx.at, NEARBY_STEPS);
        stage_belief(&mut app, place, UNDER_ONE_DEATH);
        morale_turn(&mut app);
        (
            cohort(&app, fx.band).belief_anchor,
            culture_of(&app, fx.band),
        )
    };
    assert_eq!(under, (None, Scalar::from_i64(0)));

    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let place = along_row(&app, fx.at, NEARBY_STEPS);
    stage_belief(&mut app, place, ONE_DEATH);
    morale_turn(&mut app);
    assert_eq!(cohort(&app, fx.band).belief_anchor, Some(place));
    assert!(culture_of(&app, fx.band) > Scalar::from_i64(0));
}

/// **`min_anchor_belief` is honoured**: raised to 3.0, a place holding 2.0 is not adopted and one
/// holding 3.0 is.
#[test]
fn min_anchor_belief_scales_the_adoption_line() {
    let adopted = |belief: f32| -> bool {
        let mut app = one_faction_world();
        with_min_anchor_belief(&mut app, RAISED_MIN_ANCHOR);
        let fx = fixture(&mut app);
        let place = along_row(&app, fx.at, NEARBY_STEPS);
        stage_belief(&mut app, place, belief);
        morale_turn(&mut app);
        cohort(&app, fx.band).belief_anchor == Some(place)
    };
    assert!(
        !adopted(SHORT_OF_RAISED_MIN),
        "2.0 falls short of a 3.0 line"
    );
    assert!(adopted(RAISED_MIN_ANCHOR), "3.0 meets it");
}

// ---- Kin relay the reach (`core_sim::belief_relay`) ----

/// The column the relay layouts start at — far enough from the map's edge that every staged band
/// sits on the same row with exact hex distances.
const RELAY_ANCHOR_X: u32 = 10;
/// Workers each kin band is split off with.
const KIN_WORKERS: u32 = 3;
/// A kin band staged far beyond every chain, to read the unreached sentinel.
const FAR_KIN_STEPS: u32 = 30;
const HALF_STRENGTH: f32 = 0.5;
const QUARTER_STRENGTH: f32 = 0.25;
const FULL_STRENGTH: f32 = 1.0;
const NO_STRENGTH: f32 = 0.0;
const NO_RELAY: f32 = 0.0;

/// The anchor, and the columns of a chain laid out along one row at the walk's own reach: A within
/// reach of the anchor, B within reach of A only, C within reach of B only.
struct RelayLayout {
    anchor: UVec2,
    a: UVec2,
    b: UVec2,
    c: UVec2,
}

fn relay_layout(app: &App, row: u32) -> RelayLayout {
    let reach = base_reach(app);
    let anchor = UVec2::new(RELAY_ANCHOR_X, row);
    let a = UVec2::new(RELAY_ANCHOR_X + NEARBY_STEPS, row);
    let b = UVec2::new(a.x + reach, row);
    let c = UVec2::new(b.x + reach, row);
    assert!(
        b.x - anchor.x > reach && c.x - a.x > reach,
        "fixture: B and C stand beyond the anchor's reach, C beyond A's"
    );
    RelayLayout { anchor, a, b, c }
}

/// Split `count` kin bands off the fixture band (all the same people), each a resident band.
fn split_kin(app: &mut App, parent: Entity, count: usize) -> Vec<Entity> {
    let settle = SettleConfig {
        min_founding_workers: 1,
        parent_min_workers: 0,
    };
    (0..count)
        .map(|_| {
            let split = split_band_from_parent(&mut app.world, parent, KIN_WORKERS, &settle)
                .expect("the band can split");
            app.world
                .query::<(Entity, &BandId)>()
                .iter(&app.world)
                .find(|(_, id)| **id == split.band)
                .map(|(entity, _)| entity)
                .expect("the splinter is alive")
        })
        .collect()
}

/// Stand `band` at `position`, holding `anchor`, with a deep larder.
fn place(app: &mut App, band: Entity, position: UVec2, anchor: Option<UVec2>) {
    stand_on(app, band, position);
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
    cohort.belief_anchor = anchor;
    cohort
        .stores
        .reset_food("dry", scalar_from_f32(DEEP_LARDER));
}

fn expected_at(app: &App, strength: f32) -> Scalar {
    let culture = wellbeing(app).culture.clone();
    let weight = culture.anchor_weight(SOME_BELIEF);
    scalar_from_f32(
        weight * (strength * culture.near_bonus - (FULL_STRENGTH - strength) * culture.away_drag),
    )
}

/// A two-band chain on the fixture's row: the fixture band as A, one kin band as B.
fn two_kin(app: &mut App) -> (Entity, Entity, RelayLayout) {
    let fx = fixture(app);
    let layout = relay_layout(app, fx.at.y);
    stage_belief(app, layout.anchor, SOME_BELIEF);
    let b = split_kin(app, fx.band, 1)[0];
    place(app, fx.band, layout.a, Some(layout.anchor));
    place(app, b, layout.b, Some(layout.anchor));
    (fx.band, b, layout)
}

/// **One hop of kin halves the tie**: B stands beyond the anchor's reach but within A's, and reads
/// `w × (0.5 × near − 0.5 × away)`; A, standing within reach itself, reads the full `w × near`.
#[test]
fn a_band_within_reach_of_a_near_kin_band_is_near_at_half_strength() {
    let mut app = one_faction_world();
    let (a, b, _) = two_kin(&mut app);
    morale_turn(&mut app);
    assert_scalar_eq(
        culture_of(&app, a),
        expected_at(&app, FULL_STRENGTH),
        "A direct",
    );
    assert_scalar_eq(
        culture_of(&app, b),
        expected_at(&app, HALF_STRENGTH),
        "B one hop",
    );
}

/// **Each further hop halves it again**: the end of a three-band chain reads 0.25.
#[test]
fn a_three_band_chain_reads_a_quarter_at_its_end() {
    let mut app = one_faction_world();
    let (_, b, layout) = two_kin(&mut app);
    let c = split_kin(&mut app, b, 1)[0];
    place(&mut app, c, layout.c, Some(layout.anchor));
    morale_turn(&mut app);
    assert_scalar_eq(culture_of(&app, b), expected_at(&app, HALF_STRENGTH), "B");
    assert_scalar_eq(
        culture_of(&app, c),
        expected_at(&app, QUARTER_STRENGTH),
        "C",
    );
}

/// **Another people is not kin.** A rival band beside A, holding the same anchor, is not reached
/// through A, and a home band beyond it is not reached through the rival.
#[test]
fn another_peoples_band_relays_nothing_either_way() {
    let mut app = two_faction_world();
    let fx = fixture(&mut app);
    let layout = relay_layout(&app, fx.at.y);
    stage_belief(&mut app, layout.anchor, SOME_BELIEF);
    let rival = app
        .world
        .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .find(|(_, cohort)| cohort.faction == RIVAL)
        .map(|(entity, _)| entity)
        .expect("the rival has a band");
    let home_beyond = split_kin(&mut app, fx.band, 1)[0];
    place(&mut app, fx.band, layout.a, Some(layout.anchor));
    place(&mut app, rival, layout.b, Some(layout.anchor));
    place(&mut app, home_beyond, layout.c, Some(layout.anchor));
    morale_turn(&mut app);
    assert_scalar_eq(
        culture_of(&app, rival),
        expected_at(&app, NO_STRENGTH),
        "rival",
    );
    assert_scalar_eq(
        culture_of(&app, home_beyond),
        expected_at(&app, NO_STRENGTH),
        "home band beyond the rival",
    );
}

/// **A detached party does not relay.** A cohort with no `ResidentBand` — a party's shape, a clone
/// of its band's cohort off the resident set — standing between A and B ties nobody in.
#[test]
fn a_detached_party_does_not_relay() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    let layout = relay_layout(&app, fx.at.y);
    stage_belief(&mut app, layout.anchor, SOME_BELIEF);
    let beyond = split_kin(&mut app, fx.band, 1)[0];
    place(&mut app, fx.band, layout.a, Some(layout.anchor));
    place(&mut app, beyond, layout.c, Some(layout.anchor));
    let mut party = cohort(&app, fx.band).clone();
    party.current_tile = tile_at(&app, layout.b);
    app.world.spawn(party);
    morale_turn(&mut app);
    assert_scalar_eq(
        culture_of(&app, beyond),
        expected_at(&app, NO_STRENGTH),
        "nothing relays through a party",
    );
}

/// **`relay_per_hop = 0` is today's term**: the direct band reads near, the kin band the full drag.
#[test]
fn relay_per_hop_zero_reproduces_the_unrelayed_term() {
    let mut app = one_faction_world();
    let mut tuned = (*wellbeing(&app)).clone();
    tuned.culture.relay_per_hop = NO_RELAY;
    app.world
        .insert_resource(WellbeingConfigHandle::new(std::sync::Arc::new(tuned)));
    let (a, b, _) = two_kin(&mut app);
    morale_turn(&mut app);
    let culture = wellbeing(&app).culture.clone();
    let weight = culture.anchor_weight(SOME_BELIEF);
    assert_scalar_eq(
        culture_of(&app, a),
        scalar_from_f32(culture.near_bonus * weight),
        "A",
    );
    assert_scalar_eq(
        culture_of(&app, b),
        scalar_from_f32(-culture.away_drag * weight),
        "B",
    );
}

/// **Relaying is never adoption.** B, with no anchor of its own, stands within reach of A and gains
/// no anchor and no term through it.
#[test]
fn a_band_does_not_adopt_a_place_through_kin() {
    let mut app = one_faction_world();
    let (_, b, _) = two_kin(&mut app);
    app.world
        .get_mut::<PopulationCohort>(b)
        .unwrap()
        .belief_anchor = None;
    morale_turn(&mut app);
    assert_eq!(cohort(&app, b).belief_anchor, None);
    assert_eq!(culture_of(&app, b), Scalar::from_i64(0));
}

/// What one band's row publishes about how kin tie it in.
struct PublishedRelay {
    hops: u8,
    direct: Vec<UVec2>,
    relayed: Vec<UVec2>,
}

fn published_relay(app: &mut App, band: Entity) -> PublishedRelay {
    let direct = published_anchor(app, band).region;
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
    let xs: Vec<u32> = row
        .beliefRelayReachX()
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    let ys: Vec<u32> = row
        .beliefRelayReachY()
        .map(|v| v.iter().collect())
        .unwrap_or_default();
    assert_eq!(xs.len(), ys.len(), "the relayed region's x and y lists zip");
    PublishedRelay {
        hops: row.beliefRelayHops(),
        direct,
        relayed: xs
            .into_iter()
            .zip(ys)
            .map(|(x, y)| UVec2::new(x, y))
            .collect(),
    }
}

/// **On the encoded envelope**: B publishes one hop and a relayed region that holds its own tile
/// and shares no tile with the direct region; A publishes direct; a kin band far beyond every chain
/// publishes the unreached sentinel.
#[test]
fn the_relay_hops_and_region_are_on_the_encoded_snapshot() {
    let mut app = one_faction_world();
    let (a, b, layout) = two_kin(&mut app);
    let far = split_kin(&mut app, a, 1)[0];
    place(
        &mut app,
        far,
        UVec2::new(layout.anchor.x + FAR_KIN_STEPS, layout.anchor.y),
        Some(layout.anchor),
    );
    morale_turn(&mut app);

    assert_eq!(published_relay(&mut app, a).hops, 0);
    let relay_b = published_relay(&mut app, b);
    assert_eq!(relay_b.hops, 1);
    assert!(
        relay_b.relayed.contains(&layout.b),
        "B stands where kin tie it in"
    );
    assert!(
        relay_b
            .relayed
            .iter()
            .all(|tile| !relay_b.direct.contains(tile)),
        "the relayed region excludes the direct one"
    );
    assert_eq!(
        published_relay(&mut app, far).hops,
        sim_schema::BELIEF_RELAY_UNREACHED
    );
}

/// **The drawn relayed region and the term cannot disagree.** A kin band standing on every tile of
/// its published relayed region reads `r > 0`; on every tile bordering it outside both regions, `r
/// == 0` (the full drag).
#[test]
fn the_published_relayed_region_agrees_with_the_term_at_every_tile() {
    let mut app = one_faction_world();
    let (_, b, layout) = two_kin(&mut app);
    // A great cemetery, so the fractional deaths the sweep credits to the tiles A and B stand on
    // over a hundred-odd turns can never out-weigh the anchor and move it.
    stage_belief(&mut app, layout.anchor, GREAT_CEMETERY);
    morale_turn(&mut app);
    let published = published_relay(&mut app, b);
    assert!(!published.relayed.is_empty(), "fixture: a relayed region");

    let config = app.world.resource::<SimulationConfig>().clone();
    let (width, height, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let mut border: Vec<UVec2> = published
        .relayed
        .iter()
        .flat_map(|&tile| core_sim::grid_utils::hex_range_tiles(tile, 1, width, height, wrap))
        .filter(|tile| !published.relayed.contains(tile) && !published.direct.contains(tile))
        .collect();
    border.sort_by_key(|tile| (tile.y, tile.x));
    border.dedup();
    assert!(
        !border.is_empty(),
        "fixture: the relayed region has an outer edge"
    );

    let culture = wellbeing(&app).culture.clone();
    let anchor_belief = app.world.resource::<BeliefRegistry>().get(layout.anchor);
    let fully_away = scalar_from_f32(-culture.away_drag * culture.anchor_weight(anchor_belief));
    for &tile in &published.relayed {
        stand_on(&mut app, b, tile);
        morale_turn(&mut app);
        assert!(
            culture_of(&app, b).raw() > fully_away.raw() + RAW_TOLERANCE,
            "standing on {tile} in the relayed region must read r > 0"
        );
    }
    for &tile in &border {
        stand_on(&mut app, b, tile);
        morale_turn(&mut app);
        assert_scalar_eq(
            culture_of(&app, b),
            fully_away,
            &format!("standing on {tile} outside both regions must read r == 0"),
        );
    }
}

/// **The frame's hop count is the one its `moraleCulture` was priced from.** B starts one hop out
/// and walks into direct reach during the turn (`advance_band_movement` runs after
/// `simulate_population`): the term was priced at one hop, so the frame must publish `1` beside the
/// blended term — not the `0` a recount on B's new tile would give.
#[test]
fn the_published_hop_count_is_the_one_the_term_was_priced_from() {
    let mut app = one_faction_world();
    let (_, b, layout) = two_kin(&mut app);
    let reach = base_reach(&app);
    // One step past the anchor's reach, still within A's; the order walks it one tile closer.
    let start = UVec2::new(layout.anchor.x + reach + JUST_OUT_OF_REACH, layout.anchor.y);
    let inside = UVec2::new(layout.anchor.x + reach, layout.anchor.y);
    place(&mut app, b, start, Some(layout.anchor));
    app.world.entity_mut(b).insert(BandTravel {
        target: inside,
        departed: false,
    });

    run_turn(&mut app);

    let standing = app
        .world
        .get::<Tile>(cohort(&app, b).current_tile)
        .unwrap()
        .position;
    assert_eq!(
        standing, inside,
        "fixture: B walked into direct reach this turn"
    );
    let published = published_relay(&mut app, b);
    assert!(
        published.direct.contains(&inside),
        "fixture: on the frame's positions B now stands in the direct region"
    );
    assert_eq!(published.hops, 1, "the hop count the term was priced from");

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let morale_culture = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.entity() == b.to_bits())
        .expect("B is on the wire")
        .moraleCulture();
    let culture = wellbeing(&app).culture.clone();
    let anchor_belief = app.world.resource::<BeliefRegistry>().get(layout.anchor);
    let weight = culture.anchor_weight(anchor_belief);
    let blend = scalar_from_f32(
        weight
            * (HALF_STRENGTH * culture.near_bonus
                - (FULL_STRENGTH - HALF_STRENGTH) * culture.away_drag),
    );
    assert!(
        (morale_culture - blend.raw()).abs() <= RAW_TOLERANCE,
        "moraleCulture {morale_culture} is the one-hop blend {}",
        blend.raw()
    );
}

// ---------------------------------------------------------------------------------------------
// The ancestor pull (issue #701, `.claude/rules/core_sim/belief.md` → "The ancestor pull"): honouring
// the dead pulls a band's own culture layer toward Devout and Traditionalist by `tie × ancestor_pull`,
// a stateless TARGET offset the band's elasticity lags behind. Each arm is a PAIR of worlds staged
// identically but for the one thing under test, so the gap is the pull and nothing else (a band's
// character offset is seeded from its id and is the same in both).
// ---------------------------------------------------------------------------------------------

/// Culture reconcile turns each pair of worlds runs — enough for a band's elasticity to close most
/// of the distance to its target.
const PULL_TURNS: u32 = 60;
/// The least gap, in axis units, a saturated direct tie must open between the pulled band and its
/// unpulled twin after [`PULL_TURNS`] — liveness: a dead pull would read `0`. Full tie is `0.3`.
const MIN_PULL_GAP: f32 = 0.1;
/// How close two gaps (or two layers that must be identical) must read, in axis units.
const GAP_TOLERANCE: f32 = 1.0e-3;
/// The least a faction's rolled-up Devout signal must rise when every band is tied to its dead.
const MIN_SIGNAL_RISE: f32 = 0.05;

fn devout() -> usize {
    CultureTraitAxis::SecularDevout.index()
}

fn traditionalist() -> usize {
    CultureTraitAxis::TraditionalistRevisionist.index()
}

/// Switch contact drift off, so a comparison between two worlds isolates the ancestor pull: two
/// co-located kin bands are tied, and drift would move both toward each other differently in the
/// pulled and unpulled arms.
fn drift_off(app: &mut App) {
    app.world
        .resource_mut::<core_sim::CultureCorruptionConfigHandle>()
        .replace_from_json(r#"{"culture":{"contact_drift":{"rate":0}}}"#)
        .expect("the lever parses");
}

/// Turn the pull off for this world: an empty `ancestor_pull` map.
fn pull_off(app: &mut App) {
    let config = BeliefConfig {
        ancestor_pull: Default::default(),
        ..BeliefConfig::default()
    };
    app.world
        .insert_resource(BeliefConfigHandle::new(std::sync::Arc::new(config)));
}

/// Run the culture pass `turns` times through the real systems, on the anchors and hop counts the
/// last `simulate_population` stored on the cohorts.
fn culture_turns(app: &mut App, turns: u32) {
    app.world.run_system_once(reconcile_band_culture_layers);
    for _ in 0..turns {
        app.world.run_system_once(reconcile_culture_layers);
    }
}

fn band_axes(app: &App, band: BandId) -> (f32, f32) {
    let layer = app
        .world
        .resource::<CultureManager>()
        .band_layer_by_owner(CultureOwner::from_band(band))
        .expect("the band has a culture layer");
    let values = layer.traits.values();
    (values[devout()].to_f32(), values[traditionalist()].to_f32())
}

/// A band standing within reach of a saturated anchor (`r = 1`) — or, unanchored, standing on bare
/// ground — after the culture pass.
fn direct_band_world(pull: bool, anchored: bool) -> (App, BandId) {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    if !pull {
        pull_off(&mut app);
    }
    if anchored {
        stage_belief(&mut app, fx.at, GREAT_CEMETERY);
    }
    morale_turn(&mut app);
    assert_eq!(
        cohort(&app, fx.band).belief_anchor.is_some(),
        anchored,
        "fixture: the anchor is held exactly when belief was staged"
    );
    culture_turns(&mut app, PULL_TURNS);
    (app, fx.band_id)
}

/// **The pull is live:** a band tied to a saturated anchor sits measurably more Devout and more
/// Traditionalist than the same band with no ancestors.
#[test]
fn a_band_tied_to_its_dead_sits_more_devout_and_more_traditionalist() {
    let (anchored, id) = direct_band_world(true, true);
    let (bare, _) = direct_band_world(true, false);
    let (devout_with, trad_with) = band_axes(&anchored, id);
    let (devout_without, trad_without) = band_axes(&bare, id);
    assert!(
        devout_with - devout_without > MIN_PULL_GAP,
        "devout {devout_with} vs {devout_without}"
    );
    assert!(
        trad_without - trad_with > MIN_PULL_GAP,
        "traditionalist (lower is more traditionalist) {trad_with} vs {trad_without}"
    );
}

/// **A band no chain reaches is not pulled** (`r = 0`), though it still holds its anchor.
#[test]
fn an_anchored_band_reached_by_no_chain_gets_no_pull() {
    let stranded = |anchored: bool| -> (App, BandId) {
        let mut app = one_faction_world();
        let fx = fixture(&mut app);
        let place = along_row(&app, fx.at, NEARBY_STEPS);
        if anchored {
            stage_belief(&mut app, place, GREAT_CEMETERY);
            morale_turn(&mut app);
        }
        let far = along_row(&app, place, base_reach(&app) + JUST_OUT_OF_REACH);
        stand_on(&mut app, fx.band, far);
        morale_turn(&mut app);
        assert_eq!(cohort(&app, fx.band).belief_anchor.is_some(), anchored);
        culture_turns(&mut app, PULL_TURNS);
        (app, fx.band_id)
    };
    let (held, id) = stranded(true);
    let (none, _) = stranded(false);
    let (a, b) = (band_axes(&held, id), band_axes(&none, id));
    assert!((a.0 - b.0).abs() < GAP_TOLERANCE && (a.1 - b.1).abs() < GAP_TOLERANCE);
}

/// **One hop of kin halves the pull:** the band beyond the anchor's reach but within A's is pulled
/// half as far as A, which stands within reach itself.
#[test]
fn one_hop_of_kin_gets_half_the_pull_of_a_direct_band() {
    let world = |pull: bool| -> (App, BandId, BandId) {
        let mut app = one_faction_world();
        drift_off(&mut app);
        if !pull {
            pull_off(&mut app);
        }
        let (a, b, layout) = two_kin(&mut app);
        // Deepen the anchor so `s` is near one and the gap clears the liveness floor.
        stage_belief(&mut app, layout.anchor, GREAT_CEMETERY);
        morale_turn(&mut app);
        assert_eq!(
            cohort(&app, b).last_belief_relay_hops,
            1,
            "fixture: B is one hop out"
        );
        culture_turns(&mut app, PULL_TURNS);
        let ia = *app.world.get::<BandId>(a).unwrap();
        let ib = *app.world.get::<BandId>(b).unwrap();
        (app, ia, ib)
    };
    let (with, a, b) = world(true);
    let (without, _, _) = world(false);
    let gap = |band: BandId| band_axes(&with, band).0 - band_axes(&without, band).0;
    let (direct, kin) = (gap(a), gap(b));
    assert!(
        direct > MIN_PULL_GAP,
        "fixture: the direct band is pulled ({direct})"
    );
    // Both bands tie to the same anchor at the same belief, so `s` cancels: the gaps are in the
    // ratio of `r` — one over `relay_per_hop`.
    let relay = wellbeing(&with).culture.relay_per_hop;
    assert!(
        (kin - direct * relay).abs() < GAP_TOLERANCE,
        "kin {kin} vs {relay} of direct {direct}"
    );
}

/// **An empty `ancestor_pull` is the unpulled culture, exactly.**
#[test]
fn an_empty_ancestor_pull_reproduces_the_unpulled_culture() {
    let (off, id) = direct_band_world(false, true);
    let (bare, _) = direct_band_world(true, false);
    assert_eq!(band_axes(&off, id), band_axes(&bare, id));
}

/// The faction's rolled-up `culture.axis.secular_devout` Telling signal, as the ledger records it
/// (`BeatLedger::to_state` history), after one Telling turn.
fn devout_signal(app: &mut App) -> f32 {
    app.world.run_system_once(telling_tick);
    let state = app.world.resource::<BeatLedger>().to_state();
    let series = state
        .history
        .iter()
        .find(|h| h.signal == "culture.axis.secular_devout")
        .expect("the signal is sampled");
    Scalar::from_raw(*series.samples.last().expect("a sample")).to_f32()
}

/// **The shipped representation:** the signal The Telling reads rises for a faction whose bands are
/// tied to their dead.
#[test]
fn the_secular_devout_signal_rises_for_a_faction_tied_to_its_dead() {
    let world = |anchored: bool| -> App {
        let mut app = one_faction_world();
        app.world.insert_resource(BeliefRegistry::default());
        let bands: Vec<(Entity, Entity)> = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>()
            .iter(&app.world)
            .filter(|(_, c)| c.faction == HOME)
            .map(|(e, c)| (e, c.current_tile))
            .collect();
        for (band, tile) in bands {
            let at = app.world.get::<Tile>(tile).unwrap().position;
            app.world
                .get_mut::<PopulationCohort>(band)
                .unwrap()
                .stores
                .reset_food("dry", scalar_from_f32(DEEP_LARDER));
            if anchored {
                stage_belief(&mut app, at, GREAT_CEMETERY);
            }
        }
        morale_turn(&mut app);
        culture_turns(&mut app, PULL_TURNS);
        app
    };
    let mut tied = world(true);
    let mut bare = world(false);
    let (with, without) = (devout_signal(&mut tied), devout_signal(&mut bare));
    assert!(
        with - without > MIN_SIGNAL_RISE,
        "signal {with} vs {without}"
    );
}

// ---------------------------------------------------------------------------------------------
// Belief feeds the tether (`SedentarizationScore`'s belief input).
// ---------------------------------------------------------------------------------------------

/// Where, relative to each band's standing tile, belief is staged for the tether arm.
#[derive(Clone, Copy)]
enum Stage {
    Nowhere,
    OnStandingTile,
    BesideStandingTile,
}

/// The least a saturated belief tether must add to the score, in score points (full is `5`).
const MIN_TETHER_GAIN: f32 = 1.0;
/// A belief far past the reference.
const OVERSHOOT: f32 = 5.0;

/// A fresh world's HOME faction score after one `sedentarization_tick`, with `amount` of belief
/// staged per `stage` for every resident band.
fn tether_score(stage: Stage, amount: f32) -> f32 {
    let app = tether_world(stage, amount);
    app.world.resource::<SedentarizationScore>().score(HOME)
}

/// The world [`tether_score`] reads, after its one `sedentarization_tick`.
fn tether_world(stage: Stage, amount: f32) -> App {
    let mut app = one_faction_world();
    app.world.insert_resource(BeliefRegistry::default());
    let standing_tiles: Vec<Entity> = app
        .world
        .query_filtered::<&PopulationCohort, With<ResidentBand>>()
        .iter(&app.world)
        .filter(|c| c.faction == HOME)
        .map(|c| c.current_tile)
        .collect();
    let standing: Vec<UVec2> = standing_tiles
        .into_iter()
        .map(|t| app.world.get::<Tile>(t).unwrap().position)
        .collect();
    for at in &standing {
        match stage {
            Stage::Nowhere => {}
            Stage::OnStandingTile => stage_belief(&mut app, *at, amount),
            Stage::BesideStandingTile => {
                let beside = along_row(&app, *at, NEARBY_STEPS);
                assert!(
                    !standing.contains(&beside),
                    "fixture: the beside tile is nobody's standing tile"
                );
                stage_belief(&mut app, beside, amount);
            }
        }
    }
    app.world.run_system_once(sedentarization_tick);
    app
}

fn tether_config(app: &App) -> std::sync::Arc<core_sim::SedentarizationConfig> {
    app.world.resource::<SedentarizationConfigHandle>().get()
}

/// **A band standing on belief scores higher; one standing beside it adds nothing.**
#[test]
fn belief_underfoot_raises_the_sedentarization_score_and_belief_beside_does_not() {
    let config = tether_config(&one_faction_world());
    let reference = config.references.belief;
    let bare = tether_score(Stage::Nowhere, 0.0);
    let on = tether_score(Stage::OnStandingTile, reference);
    let beside = tether_score(Stage::BesideStandingTile, reference);
    assert!(on - bare > MIN_TETHER_GAIN, "on {on} vs bare {bare}");
    assert!(
        (beside - bare).abs() < GAP_TOLERANCE,
        "beside {beside} vs bare {bare}"
    );
    // One tick from a zero score, the EMA keeps `1 − smoothing` of the raw blend, and the belief
    // input is `weights.belief` of it at saturation.
    let expected = (1.0 - config.smoothing) * 100.0 * config.weights.belief;
    assert!(
        ((on - bare) - expected).abs() < GAP_TOLERANCE,
        "gain {} vs {expected}",
        on - bare
    );
}

/// **The contribution saturates at `references.belief`:** five times the reference scores the same
/// as the reference, and half the reference scores half.
#[test]
fn the_belief_contribution_saturates_at_the_reference() {
    let reference = tether_config(&one_faction_world()).references.belief;
    let bare = tether_score(Stage::Nowhere, 0.0);
    let at_reference = tether_score(Stage::OnStandingTile, reference) - bare;
    let beyond = tether_score(Stage::OnStandingTile, reference * OVERSHOOT) - bare;
    let half = tether_score(Stage::OnStandingTile, reference * HALF_STRENGTH) - bare;
    assert!((beyond - at_reference).abs() < GAP_TOLERANCE);
    assert!((half - at_reference * HALF_STRENGTH).abs() < GAP_TOLERANCE);
}

// ---------------------------------------------------------------------------------------------
// What the client reads: the band's own culture, the pull applied to it, and the belief share of
// the settle score — each asserted on the ENCODED envelope.
// ---------------------------------------------------------------------------------------------

/// The axis count a published `cultureTraits` / `cultureAncestorPull` carries.
const PUBLISHED_AXES: usize = core_sim::CULTURE_TRAIT_AXES;

/// One band's culture readout as the envelope carries it.
struct PublishedCulture {
    traits: Vec<f32>,
    pull: Vec<f32>,
}

fn publish(app: &mut App) -> Vec<u8> {
    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref()).to_vec()
}

fn published_culture(app: &mut App, band: Entity) -> PublishedCulture {
    let bytes = publish(app);
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
    PublishedCulture {
        traits: row
            .cultureTraits()
            .map(|v| v.iter().collect())
            .unwrap_or_default(),
        pull: row
            .cultureAncestorPull()
            .map(|v| v.iter().collect())
            .unwrap_or_default(),
    }
}

/// **A tied band's pull is on the wire as reconcile applied it**, beside its resolved traits.
#[test]
fn a_tied_bands_ancestor_pull_and_traits_are_on_the_encoded_snapshot() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    stage_belief(&mut app, fx.at, GREAT_CEMETERY);
    morale_turn(&mut app);
    culture_turns(&mut app, PULL_TURNS);

    let published = published_culture(&mut app, fx.band);
    let manager = app.world.resource::<CultureManager>();
    let owner = CultureOwner::from_band(fx.band_id);
    let applied = manager
        .applied_band_pull(owner)
        .expect("the band took a pull");
    assert_eq!(published.pull.len(), PUBLISHED_AXES);
    for (wire, stored) in published.pull.iter().zip(applied) {
        assert_eq!(*wire, stored.to_f32());
    }
    // And it is the formula: tie x the shipped lever (direct, so r = 1).
    let culture = wellbeing(&app).culture.clone();
    let tie = culture.anchor_weight(GREAT_CEMETERY);
    let lever = BeliefConfig::default().ancestor_pull_vector();
    assert!((published.pull[devout()] - tie * lever[devout()]).abs() < GAP_TOLERANCE);
    assert!(
        published.pull[devout()] > MIN_PULL_GAP,
        "liveness: the pull is not zero"
    );
    assert!(published.pull[traditionalist()] < -MIN_PULL_GAP);

    let layer = manager
        .band_layer_by_owner(owner)
        .expect("the band has a layer");
    assert_eq!(published.traits.len(), PUBLISHED_AXES);
    assert_eq!(
        published.traits[devout()],
        layer.traits.values()[devout()].to_f32()
    );
}

/// **An untied band publishes its traits and no pull.**
#[test]
fn an_untied_bands_pull_is_empty_on_the_encoded_snapshot() {
    let (mut app, _) = direct_band_world(true, false);
    let band = app
        .world
        .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .find(|(_, c)| c.faction == HOME && c.belief_anchor.is_none())
        .map(|(e, _)| e)
        .expect("an unanchored band");
    let published = published_culture(&mut app, band);
    assert!(
        published.pull.is_empty(),
        "no tie, no pull: {:?}",
        published.pull
    );
    assert_eq!(published.traits.len(), PUBLISHED_AXES);
}

/// **A change in the pull rides the delta**: switching the lever off empties the band's published
/// pull, and the band's row is in the next delta with the empty value.
#[test]
fn a_change_in_the_pull_rides_the_delta() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    stage_belief(&mut app, fx.at, GREAT_CEMETERY);
    morale_turn(&mut app);
    culture_turns(&mut app, 1);
    assert!(!published_culture(&mut app, fx.band).pull.is_empty());

    pull_off(&mut app);
    culture_turns(&mut app, 1);
    assert!(published_culture(&mut app, fx.band).pull.is_empty());

    let delta = app
        .world
        .resource::<SnapshotHistory>()
        .last_delta()
        .expect("a delta per publication");
    let row = delta
        .populations
        .iter()
        .find(|row| row.entity == fx.band.to_bits())
        .expect("a band whose pull changed rides the delta");
    assert!(row.culture_ancestor_pull.is_empty());
}

/// The published `beliefPoints` for the HOME faction's row.
fn published_belief_points(app: &mut App) -> f32 {
    let bytes = publish(app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .subsistence()
        .and_then(|section| section.sedentarization())
        .expect("the sedentarization rows are published")
        .iter()
        .find(|row| row.faction() == HOME.0)
        .expect("the viewer's row")
        .beliefPoints()
}

/// **The belief share of the settle score is on the wire**: `100 x weights.belief x norm` for a
/// faction standing on belief (norm `1` at the reference), `0` for one beside it.
#[test]
fn belief_points_are_on_the_encoded_snapshot() {
    let config = core_sim::SedentarizationConfig::builtin();
    let reference = config.references.belief;
    let mut on = tether_world(Stage::OnStandingTile, reference);
    let mut half = tether_world(Stage::OnStandingTile, reference * HALF_STRENGTH);
    let mut beside = tether_world(Stage::BesideStandingTile, reference);
    let full = 100.0 * config.weights.belief;
    assert!((published_belief_points(&mut on) - full).abs() < GAP_TOLERANCE);
    assert!((published_belief_points(&mut half) - full * HALF_STRENGTH).abs() < GAP_TOLERANCE);
    assert_eq!(published_belief_points(&mut beside), 0.0);
}
