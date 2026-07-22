//! **Running the Startup chain twice must be a no-op, not a second world.**
//!
//! `spawn_initial_world` was the lone Startup spawner without an idempotency guard — its three
//! siblings (`spawn_initial_herds`, `spawn_initial_forage`, `spawn_initial_graze`) all early-return
//! on an already-populated registry. Worldgen instead laid down a full second `width × height` tile
//! set, a second batch of starting cohorts, and a second helping of the start profile's inventory.
//!
//! That is not a live defect on the shipped server — it boots idle and builds every world through
//! `rebuild_world_from_config`, which constructs a **brand-new `App`** rather than re-running Startup
//! on the existing one. It is a trap for *tests*: bevy 0.13's `Main::run_main` gates the startup
//! labels behind a `Local<bool>` owned by the Main schedule's own system, which
//! `world.run_schedule(Startup)` never touches — so a harness that drives Startup by hand and then
//! calls `update()` silently pays for two worlds, and every broad `Query<&Tile>` reads doubled.
//!
//! Two tests, because the doubled state has two different lifetimes:
//! - the **chain** test drives the real `Startup` schedule twice and reads what survives it (tiles
//!   and cohorts — state that persists into the game);
//! - the **stockpile** test runs `spawn_initial_world` alone, because the start profile's inventory
//!   grant does *not* survive the chain: `apply_trade_goods_bonus`, two systems later, drains
//!   `trade_goods` to zero on every pass. The double-grant is real but unobservable downstream of
//!   it, so it is asserted at the seam that produces it.

use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    build_headless_app, spawn_initial_world, FactionInventory, PopulationCohort, SimulationConfig,
    Tile, TileRegistry,
};

/// Map dimensions for the runs. Smaller than the shipped standard (80×52) because these tests care
/// only about *how many* of each thing exist, not about the terrain that comes out — and a second
/// worldgen pass over a big grid is the slowest part of the run. Still large enough for a real
/// generated map with a start location and population clusters.
const GRID: UVec2 = UVec2::new(40, 26);

/// Fixed seed so the runs are reproducible. **Never 0** — `map_seed == 0` is the "roll a seed from
/// entropy" sentinel in `spawn_initial_world`.
const MAP_SEED: u64 = 20_260_721;

/// A headless app configured for a small, reproducible generated map.
fn app_on_test_map() -> bevy::prelude::App {
    let mut app = build_headless_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    config.grid_size = GRID;
    app.world.insert_resource(config);
    app
}

/// The world-scale counts a duplicate worldgen pass inflates and that survive the whole chain.
#[derive(Debug, PartialEq, Eq)]
struct WorldCensus {
    /// Live `Tile` entities — the surface the original report measured (8320 on an 80×52 map).
    tile_entities: usize,
    /// `TileRegistry`'s own view, which worldgen *replaces* wholesale on a second pass. Asserted
    /// separately from `tile_entities` because a duplicate pass leaves the two disagreeing: the
    /// registry indexes only the newest set while the orphaned first set stays queryable.
    registry_tiles: usize,
    /// Starting population cohorts from the profile's `starting_units`.
    cohorts: usize,
}

fn census(app: &mut bevy::prelude::App) -> WorldCensus {
    let tile_entities = app.world.query::<&Tile>().iter(&app.world).count();
    let cohorts = app
        .world
        .query::<&PopulationCohort>()
        .iter(&app.world)
        .count();
    let registry_tiles = app.world.resource::<TileRegistry>().tiles.len();
    WorldCensus {
        tile_entities,
        registry_tiles,
        cohorts,
    }
}

#[test]
fn second_startup_pass_does_not_build_a_second_world() {
    let mut app = app_on_test_map();

    app.world.run_schedule(bevy::app::Startup);
    let first = census(&mut app);

    let expected_tiles = (GRID.x * GRID.y) as usize;
    assert_eq!(
        first.tile_entities, expected_tiles,
        "the first Startup pass should stamp exactly one tile per grid cell"
    );
    assert!(
        first.cohorts > 0,
        "the start profile should seed at least one cohort, or the doubling assertion below is vacuous"
    );

    app.world.run_schedule(bevy::app::Startup);
    let second = census(&mut app);

    assert_eq!(
        second, first,
        "a second Startup pass must be a no-op — worldgen re-ran and built a second world"
    );
}

/// Total quantity across every faction stockpile.
fn stockpile_total(app: &bevy::prelude::App) -> i64 {
    app.world
        .resource::<FactionInventory>()
        .iter()
        .flat_map(|(_, items)| items.values())
        .sum()
}

#[test]
fn second_worldgen_pass_does_not_re_grant_starting_inventory() {
    let mut app = app_on_test_map();

    app.world.run_system_once(spawn_initial_world);
    let first = stockpile_total(&app);
    assert!(
        first > 0,
        "the start profile should grant a starting stockpile, or this test is vacuous"
    );

    app.world.run_system_once(spawn_initial_world);

    assert_eq!(
        stockpile_total(&app),
        first,
        "a second worldgen pass must not re-grant the start profile's inventory"
    );
}
