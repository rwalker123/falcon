//! **Is a large group detectable beyond anyone's range?** (issue #533)
//!
//! Yes: a tile holding a large group is *seen from further away* — the bonus belongs to the target
//! tile (faction-blind), it rides the one sight sweep, and contact follows because it is found by
//! the same reveal. These tests drive whole turns through `build_test_app` worlds and assert the
//! wire fact on the **encoded envelope**, because a section that never reached the codec still
//! passes an in-process assertion.
//!
//! Every claim is paired with its negative arm: a 30-person band at the same tile is not seen
//! (so the 100-person arm is the bonus and not a fluke of terrain), an empty tile at the same
//! distance stays dark (only the occupied tile lights), and a ridge still hides a big camp.

use std::sync::Arc;

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    grid_utils::{hex_distance_wrapped, hex_range_tiles},
    run_turn, terrain_definition, terrain_sight_modifier, BandId, ConnectionKey, ConnectionLedger,
    PopulationCohort, Scalar, SightRangeConfig, SimulationConfig, SnapshotHistory,
    TerrainDetectionConfig, Tile, TileRegistry, VisibilityConfig, VisibilityConfigHandle,
    VisibilityLedger, VisibilityState,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_runtime::TerrainTags;

/// The observer band's sight, pinned so the geometry under test is the config's and not a
/// terrain-dependent elevation bonus. `BandScout` ships `6`; any value works as long as it is
/// stated, and a short one keeps the whole disc on the map.
const BASE_RANGE: u32 = 4;

/// One tile beyond the observer's own reach: the distance the subject stands at.
const SUBJECT_DISTANCE: u32 = BASE_RANGE + 1;

/// A start band (below the `size_sight_bonus` threshold) and a camp big enough for a bonus of `2`
/// even after a turn of demographic drift (`floor((110 - 50) / 25) = 2`, and still `2` down to 100).
const SMALL_BAND: f32 = 30.0;
const LARGE_BAND: f32 = 110.0;

/// What a published frame says about the subject band, read off the encoded envelope.
#[derive(Debug)]
struct PublishedSubject {
    faction: u32,
    size: u32,
    morale: i64,
    knowledge_fragments: usize,
    labor_assignments: usize,
}

/// Everything one arm measures.
struct Outcome {
    /// Is the subject's tile `Active` for HOME?
    tile_active: bool,
    /// Does HOME's band hold a connection edge to the subject?
    contacted: bool,
    /// Is the empty control tile (same distance, no band) `Active` for HOME?
    empty_active: bool,
    /// The subject's row in HOME's encoded frame, if it published one.
    row: Option<PublishedSubject>,
}

fn opening_band(app: &mut App, faction: core_sim::FactionId) -> (BandId, Entity) {
    app.world
        .query::<(Entity, &BandId, &PopulationCohort)>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction)
        .map(|(entity, band, _)| (*band, entity))
        .min_by_key(|(band, _)| *band)
        .unwrap_or_else(|| panic!("{faction:?} has an opening band"))
}

fn position_of(app: &App, entity: Entity) -> UVec2 {
    let cohort = app.world.get::<PopulationCohort>(entity).expect("band");
    app.world
        .get::<Tile>(cohort.current_tile)
        .expect("tile")
        .position
}

fn stand_at(app: &mut App, band: Entity, target: UVec2) {
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(target.x, target.y)
        .expect("the target tile is on the map");
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).expect("band");
    cohort.home = tile;
    cohort.current_tile = tile;
}

fn set_people(app: &mut App, band: Entity, people: f32) {
    let mut cohort = app.world.get_mut::<PopulationCohort>(band).expect("band");
    cohort.working = Scalar::from_f32(people);
    cohort.children = Scalar::from_f32(0.0);
    cohort.elders = Scalar::from_f32(0.0);
    cohort.sync_size();
}

/// A two-faction world with HOME's sight pinned: base range `BASE_RANGE`, no elevation bonus, and
/// line of sight on or off as the arm asks.
fn world(los: bool) -> App {
    let mut app = world_with(ONE_RIVAL, |_| {});
    let mut cfg = VisibilityConfig::default();
    cfg.elevation.enabled = false;
    cfg.line_of_sight.enabled = los;
    cfg.sight_ranges.insert(
        "BandScout".to_string(),
        SightRangeConfig {
            base_range: BASE_RANGE,
            elevation_bonus_factor: 1.0,
        },
    );
    app.world
        .insert_resource(VisibilityConfigHandle::new(Arc::new(cfg)));
    app
}

/// Land tiles exactly `SUBJECT_DISTANCE` hex steps from `home` whose terrain adds nothing to sight
/// (so the only thing that can lift them into view is the size bonus), in row-major order.
fn ring_candidates(app: &App, home: UVec2) -> Vec<UVec2> {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let terrain_cfg = TerrainDetectionConfig::default();
    let mut tiles: Vec<UVec2> = hex_range_tiles(home, SUBJECT_DISTANCE, width, height, wrap)
        .into_iter()
        .filter(|tile| hex_distance_wrapped(home, *tile, width, wrap) == SUBJECT_DISTANCE)
        .filter(|tile| {
            let entity = app
                .world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .expect("on the map");
            let data = app.world.get::<Tile>(entity).expect("tile");
            !data.terrain_tags.contains(TerrainTags::WATER)
                && terrain_sight_modifier(
                    terrain_definition(data.terrain).detection_modifier,
                    &terrain_cfg,
                ) == 0
        })
        .collect();
    tiles.sort_by_key(|tile| (tile.y, tile.x));
    tiles
}

/// A ridge between HOME and the subject: every tile at hex distance `2..SUBJECT_DISTANCE` from
/// home is tagged `HIGHLAND`, which is a configured line-of-sight blocker. Any ray from home to a
/// tile `SUBJECT_DISTANCE` away crosses one.
fn raise_ridge(app: &mut App, home: UVec2) {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let wrap = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let ridge: Vec<Entity> = hex_range_tiles(home, SUBJECT_DISTANCE - 1, width, height, wrap)
        .into_iter()
        .filter(|tile| hex_distance_wrapped(home, *tile, width, wrap) >= 2)
        .map(|tile| {
            app.world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .expect("on the map")
        })
        .collect();
    for entity in ridge {
        let mut tile = app.world.get_mut::<Tile>(entity).expect("tile");
        tile.terrain_tags |= TerrainTags::HIGHLAND;
    }
}

fn published_subject(app: &App, subject: BandId) -> Option<PublishedSubject> {
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
        .find(|row| row.bandId() == subject.0)
        .map(|row| PublishedSubject {
            faction: row.faction(),
            size: row.size(),
            morale: row.morale(),
            knowledge_fragments: row.knowledgeFragments().map(|v| v.len()).unwrap_or(0),
            labor_assignments: row.laborAssignments().map(|v| v.len()).unwrap_or(0),
        })
}

/// Run one arm: the RIVAL band, `people` strong, stands on `target`; HOME's band is untouched.
/// `empty` is a second tile at the same distance that holds nobody.
fn run_arm(people: f32, target: UVec2, empty: UVec2, los: bool, ridge: bool) -> Outcome {
    let mut app = world(los);
    let (home_band, home_entity) = opening_band(&mut app, HOME);
    let (rival_band, rival_entity) = opening_band(&mut app, RIVAL);
    let home = position_of(&app, home_entity);
    // The candidates were chosen against HOME's position, which is the same in every arm: the
    // world is pinned by `HARNESS_MAP_SEED`.
    assert_eq!(
        hex_distance_wrapped(
            home,
            target,
            app.world.resource::<TileRegistry>().width,
            true
        ),
        SUBJECT_DISTANCE
    );
    if ridge {
        raise_ridge(&mut app, home);
    }
    stand_at(&mut app, rival_entity, target);
    set_people(&mut app, rival_entity, people);
    run_turn(&mut app);

    let ledger = app.world.resource::<VisibilityLedger>();
    let tile_active = ledger.visibility_state(HOME, target.x, target.y) == VisibilityState::Active;
    let empty_active = ledger.visibility_state(HOME, empty.x, empty.y) == VisibilityState::Active;
    let contacted = app
        .world
        .resource::<ConnectionLedger>()
        .get(&ConnectionKey::new(home_band, rival_band))
        .is_some();
    Outcome {
        tile_active,
        contacted,
        empty_active,
        row: published_subject(&app, rival_band),
    }
}

/// HOME's position in this world (the same in every arm).
fn home_position() -> (UVec2, Vec<UVec2>) {
    let mut app = world(false);
    let (_, home_entity) = opening_band(&mut app, HOME);
    let home = position_of(&app, home_entity);
    let candidates = ring_candidates(&app, home);
    (home, candidates)
}

/// Pick the first ring tile that the big band actually lights, and a different ring tile to serve
/// as the empty control. Searching rather than hard-coding a coordinate keeps the fixture honest
/// about terrain it cannot see, and the *liveness* of the pick is itself asserted by the caller.
fn pick(los: bool, ridge: bool) -> Option<(UVec2, UVec2)> {
    let (_, candidates) = home_position();
    for (index, target) in candidates.iter().enumerate() {
        let Some(empty) = candidates
            .iter()
            .enumerate()
            .find(|(other, _)| *other != index)
            .map(|(_, tile)| *tile)
        else {
            continue;
        };
        if run_arm(LARGE_BAND, *target, empty, los, ridge).tile_active {
            return Some((*target, empty));
        }
    }
    None
}

/// ⛔ **A band of 30 at `base + 1` is not seen; the same tile with 100+ people is — contacted,
/// `Active`, published as a tier-2 row — and an empty tile at the same distance stays dark.**
#[test]
fn a_large_group_is_seen_and_contacted_beyond_the_base_range_and_a_small_one_is_not() {
    let (target, empty) = pick(false, false)
        .expect("some ring tile is lit by a large band (the liveness of the fixture)");

    let small = run_arm(SMALL_BAND, target, empty, false, false);
    assert!(
        !small.tile_active,
        "a 30-person band one tile past the observer's reach is not seen"
    );
    assert!(!small.contacted, "and so it is not contacted");
    assert!(
        small.row.is_none(),
        "and it is not in the frame at all (tier 3): {:?}",
        small.row
    );

    let large = run_arm(LARGE_BAND, target, empty, false, false);
    assert!(
        large.tile_active,
        "the same tile with a large camp on it is Active"
    );
    assert!(
        large.contacted,
        "seeing them is meeting them: the contact rode the same reveal"
    );
    assert!(
        !large.empty_active,
        "an empty tile at the same distance stays dark — only the occupied tile lights"
    );
    let row = large
        .row
        .expect("the camp is in HOME's frame as a foreign band in view");
    assert_eq!(row.faction, RIVAL.0, "the row says whose people they are");
    assert!(
        row.size >= 100,
        "and how large: the bonus of 2 needs at least 100 people, row says {}",
        row.size
    );
    assert_eq!(row.morale, 0, "tier 2 is redacted: no morale");
    assert_eq!(row.knowledge_fragments, 0, "no knowledge");
    assert_eq!(row.labor_assignments, 0, "no labor");
}

/// ⛔ **A ridge still hides a big camp.** The size bonus extends a range; it does not see through
/// terrain. Paired with its control: the same camp at the same tile, no ridge, is seen under the
/// same line-of-sight rule.
#[test]
fn a_ridge_still_hides_a_large_group() {
    let (target, empty) = pick(true, false)
        .expect("some ring tile is lit by a large band with line of sight on (liveness)");

    let open = run_arm(LARGE_BAND, target, empty, true, false);
    assert!(
        open.tile_active && open.contacted,
        "control: seen without the ridge"
    );

    let hidden = run_arm(LARGE_BAND, target, empty, true, true);
    assert!(
        !hidden.tile_active,
        "behind a HIGHLAND ridge the camp's tile stays dark"
    );
    assert!(!hidden.contacted, "and it is not contacted");
}
