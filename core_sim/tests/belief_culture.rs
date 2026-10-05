//! **Near / far from the ancestors** (issue #699, `docs/plan_civilization_steps.md` §"What belief
//! does, through seams that exist"): each band remembers one belief tile — its anchor — and its
//! morale gains `near_bonus × s` while it stands within walking reach of it and loses
//! `away_drag × s` beyond it, `s = b / (b + belief_half_saturation)`.
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
    publish_baseline_snapshot, scalar_from_f32, simulate_population, split_band_from_parent,
    trace_path, traffic_ceiling, BandId, BeliefRegistry, CultureConfig, LadderConfigHandle,
    MoraleCause, PopulationCohort, ResidentBand, RoadRegistry, Scalar, SettleConfig,
    SimulationConfig, SnapshotHistory, Tile, TileRegistry, WellbeingConfig, WellbeingConfigHandle,
};
use faction_support::{one_faction_world, HOME};
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
