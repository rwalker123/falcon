//! **Belief on a tile** (issue #697, `docs/plan_civilization_steps.md` §"Belief is a property of a
//! place"): deaths while a band stands on a place add to that place's belief, it never decays, and
//! it rides the snapshot and the checkpoint.
//!
//! Driven through the **real** `simulate_population` on a generated world, so the deaths are the
//! demographic model's own — not a hand-fed number — and asserted on the registry, the encoded frame
//! and the save payload.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

mod faction_support;

use core_sim::save::{decode_save, encode_save};
use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    publish_baseline_snapshot, run_turn, scalar_from_f32, simulate_population, BandId,
    BeliefConfig, BeliefConfigHandle, BeliefRegistry, PopulationCohort, ResidentBand,
    SnapshotHistory, Tile, NO_BELIEF,
};
use faction_support::{one_faction_world, HOME};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// An empty larder — the band starves, which is what makes this turn's deaths certain.
const EMPTY_LARDER: f32 = 0.0;
/// A larder many lifetimes deep, so a fed turn kills nobody by starvation.
const DEEP_LARDER: f32 = 1.0e6;
/// How many turns the walked-away tile is watched for decay.
const TURNS_AWAY: usize = 5;
/// A retuned `belief_per_death`, distinct from the shipped `1.0`.
const DOUBLED_BELIEF_PER_DEATH: f32 = 2.0;
/// f32 tolerance on the lever's ratio.
const RATIO_EPSILON: f32 = 1e-4;
/// Belief staged directly on a tile for the wire/checkpoint tests — a few deaths' worth, well above
/// the delta's hundredths deadband.
const STAGED_BELIEF: f32 = 3.5;

/// The home faction's first band, where it stands and where its home is, and an unoccupied tile to
/// move it to.
struct Fixture {
    band: Entity,
    home_tile: Entity,
    home: UVec2,
    away_tile: Entity,
    away: UVec2,
}

fn fixture(app: &mut App) -> Fixture {
    let (band, home_tile) = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .min_by_key(|(_, band, _)| **band)
        .map(|(entity, _, cohort)| (entity, cohort.home))
        .expect("the home faction has a band");
    let occupied: Vec<Entity> = app
        .world
        .query::<&PopulationCohort>()
        .iter(&app.world)
        .flat_map(|cohort| [cohort.home, cohort.current_tile])
        .collect();
    let home = app.world.get::<Tile>(home_tile).unwrap().position;
    let (away_tile, away) = app
        .world
        .query::<(Entity, &Tile)>()
        .iter(&app.world)
        .filter(|(entity, _)| !occupied.contains(entity))
        .map(|(entity, tile)| (entity, tile.position))
        .min_by_key(|(_, position)| (position.y, position.x))
        .expect("the map has a tile no band stands on");
    Fixture {
        band,
        home_tile,
        home,
        away_tile,
        away,
    }
}

fn stand_on(app: &mut App, band: Entity, tile: Entity) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .current_tile = tile;
}

fn stock_larder(app: &mut App, band: Entity, meals: f32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .reset_food("dry", scalar_from_f32(meals));
}

fn belief_at(app: &App, position: UVec2) -> f32 {
    app.world.resource::<BeliefRegistry>().get(position)
}

/// One starving turn for a band standing away from its home; returns the fixture.
fn starve_away_from_home(app: &mut App) -> Fixture {
    let fx = fixture(app);
    stand_on(app, fx.band, fx.away_tile);
    stock_larder(app, fx.band, EMPTY_LARDER);
    app.world.run_system_once(simulate_population);
    fx
}

/// **The dead are buried where the band stands, not where it calls home.**
///
/// The generated world has already resolved its opening turn, so the home tile may hold that turn's
/// dead; what this turn must not do is add to it.
#[test]
fn deaths_on_a_band_away_from_home_credit_the_tile_it_stands_on() {
    let mut app = one_faction_world();
    let before = fixture(&mut app);
    let home_before = belief_at(&app, before.home);
    assert_eq!(
        belief_at(&app, before.away),
        NO_BELIEF,
        "fixture: nobody has died on the tile the band is about to stand on"
    );
    let fx = starve_away_from_home(&mut app);
    assert_ne!(fx.home, fx.away, "fixture: the band stands off its home");
    assert!(
        belief_at(&app, fx.away) > NO_BELIEF,
        "a starving band's dead must credit the tile it stands on"
    );
    assert_eq!(
        belief_at(&app, fx.home),
        home_before,
        "the band's home must not be credited for deaths that happened elsewhere"
    );
}

/// **An abandoned place keeps its dead.** The band walks home and lives on there for several full
/// turns; the place it left reads exactly what it held when it left.
#[test]
fn belief_never_decays_after_the_band_walks_away() {
    let mut app = one_faction_world();
    let fx = starve_away_from_home(&mut app);
    let left_behind = belief_at(&app, fx.away);
    assert!(left_behind > NO_BELIEF, "fixture: the place holds belief");

    stand_on(&mut app, fx.band, fx.home_tile);
    for _ in 0..TURNS_AWAY {
        stock_larder(&mut app, fx.band, DEEP_LARDER);
        run_turn(&mut app);
        assert_eq!(
            belief_at(&app, fx.away),
            left_behind,
            "belief on a place nobody stands on must neither decay nor grow"
        );
    }
}

/// **`belief_per_death` is honoured** — the same deaths on a doubled lever credit twice the belief.
#[test]
fn belief_per_death_scales_the_accrual() {
    let mut shipped = one_faction_world();
    let shipped_fx = starve_away_from_home(&mut shipped);

    let mut doubled = one_faction_world();
    doubled
        .world
        .insert_resource(BeliefConfigHandle::new(std::sync::Arc::new(BeliefConfig {
            belief_per_death: DOUBLED_BELIEF_PER_DEATH,
        })));
    let doubled_fx = starve_away_from_home(&mut doubled);

    assert_eq!(shipped_fx.away, doubled_fx.away, "fixture: the same world");
    let base = belief_at(&shipped, shipped_fx.away);
    assert!(base > NO_BELIEF, "fixture: the shipped lever accrues");
    let ratio = belief_at(&doubled, doubled_fx.away) / base;
    let expected = DOUBLED_BELIEF_PER_DEATH / BeliefConfig::default().belief_per_death;
    assert!(
        (ratio - expected).abs() < RATIO_EPSILON,
        "belief must scale with the lever: ratio {ratio}, expected {expected}"
    );
}

/// **The frame carries it, and a change rides the delta.** Asserted on the encoded envelope — the
/// shipped representation — and on the stream's tile delta.
#[test]
fn a_tiles_belief_is_on_the_frame_and_a_change_produces_a_tile_delta() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    publish_baseline_snapshot(&mut app.world);

    app.world
        .resource_mut::<BeliefRegistry>()
        .add(fx.away, STAGED_BELIEF);
    publish_baseline_snapshot(&mut app.world);

    let history = app.world.resource::<SnapshotHistory>();
    let delta = history.last_delta().expect("a delta per publication");
    let changed = delta
        .tiles
        .iter()
        .find(|tile| tile.x == fx.away.x && tile.y == fx.away.y)
        .expect("a tile whose belief moved must ride the delta");
    assert_eq!(changed.belief, STAGED_BELIEF);

    let snapshot = history
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let tiles = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .map()
        .and_then(|map| map.tiles())
        .expect("the tile list is published");
    let read = |position: UVec2| {
        tiles
            .iter()
            .find(|tile| tile.x() == position.x && tile.y() == position.y)
            .expect("every tile is published")
            .belief()
    };
    assert_eq!(read(fx.away), STAGED_BELIEF);
    assert_eq!(
        read(fx.home),
        app.world.resource::<BeliefRegistry>().get(fx.home),
        "every tile publishes the registry's own value, not a default"
    );
}

/// **The checkpoint and the save carry it.** A rollback that dropped belief would restore a world
/// whose places had never buried anyone.
#[test]
fn belief_round_trips_the_checkpoint_and_the_save() {
    let mut app = one_faction_world();
    let fx = fixture(&mut app);
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(fx.away, STAGED_BELIEF);
    let staged = app.world.resource::<BeliefRegistry>().clone();

    let checkpoint = capture_sim_state(&app.world);
    app.world.insert_resource(BeliefRegistry::default());
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(*app.world.resource::<BeliefRegistry>(), staged);

    let blob = encode_save(&app.world).expect("the world encodes");
    let (_, payload) = decode_save(&blob).expect("the save decodes");
    assert_eq!(payload.sim.belief, staged);
}
