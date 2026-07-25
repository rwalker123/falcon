//! Regression: the `FoodModuleTag` on a tile must describe the tile's FINAL terrain.
//!
//! `spawn_initial_world` stamps `FoodModuleTag` exactly once, from `classify_food_module` on the
//! terrain as it stands at spawn time. The rest of the Startup chain then keeps repainting that
//! terrain: `generate_hydrology` stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh`/`NavigableRiver`
//! at river mouths and channels, `apply_tag_budget_solver` and `apply_biome_palette_clamp` repaint
//! biomes, and `reconcile_coastal_shelf` converts `DeepOcean` to `ContinentalShelf`. Nothing
//! re-evaluated the tag, so a tile could carry a module describing terrain it no longer has — or,
//! worse, carry **no tag at all** because its pre-hydrology biome classified to `None`.
//!
//! The player-facing symptom is the second test here: `spawn_initial_forage` only seeds a patch on
//! tiles that carry a `FoodModuleTag`, so a hydrology-stamped `RiverDelta` — the richest human
//! ground on the map (`forage.capacity_by_biome` 210) — read "No forage" in the client.
//!
//! Like `navigable_mouth_delta.rs`, this drives the REAL Startup chain through `build_headless_app`
//! rather than running worldgen stages by hand, because the bug *is* the chain's ordering.

use std::collections::BTreeMap;

use bevy::prelude::UVec2;

use core_sim::{
    build_headless_app, classify_food_module, FoodModule, FoodModuleTag, ForageRegistry,
    SimulationConfig, Tile, TileRegistry,
};
use sim_runtime::TerrainType;

/// The census seeds `hydrology_earthlike.rs` / `navigable_mouth_delta.rs` sweep. Never 0 (the
/// "roll from entropy" sentinel).
const CENSUS_SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 119_304_647];

/// The default earthlike size the census sweeps.
const CENSUS_GRID: UVec2 = UVec2::new(80, 52);

/// `hydrology_earthlike.rs`'s river-capable fixture size — big enough that basins clear the
/// navigable-discharge threshold, so navigable rivers and their delta mouths actually exist.
const NAVIGABLE_FIXTURE_GRID: UVec2 = UVec2::new(128, 96);

/// A tile whose final terrain disagrees with the `FoodModuleTag` it carries.
#[derive(Debug)]
struct Violation {
    x: u32,
    y: u32,
    terrain: TerrainType,
    expected: Option<FoodModule>,
    actual: Option<FoodModule>,
}

/// Build the real app and run the shipped Startup chain once for `(seed, grid)`.
fn generated_world(seed: u64, grid: UVec2) -> bevy::app::App {
    let mut app = build_headless_app();

    // Override the env-loaded config with the case under test, before the Startup chain reads it.
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = grid;
    app.world.insert_resource(config);

    // One update runs the Startup worldgen chain in shipped order.
    app.update();
    app
}

/// Every tile whose `FoodModuleTag` disagrees with `classify_food_module` on its FINAL terrain, in
/// both directions (missing tag, stale module, or a tag that should not be there at all).
fn tag_violations(seed: u64, grid: UVec2) -> Vec<Violation> {
    let app = generated_world(seed, grid);
    let registry = app.world.resource::<TileRegistry>();
    let mut violations = Vec::new();
    for &entity in registry.tiles.iter() {
        let Some(tile) = app.world.get::<Tile>(entity) else {
            continue;
        };
        let expected = classify_food_module(tile);
        let tag = app.world.get::<FoodModuleTag>(entity);
        let actual = tag.map(|tag| tag.module);
        let kind_matches = match (expected, tag) {
            (Some(module), Some(tag)) => tag.module == module && tag.kind == module.site_kind(),
            (None, None) => true,
            _ => false,
        };
        if !kind_matches {
            violations.push(Violation {
                x: tile.position.x,
                y: tile.position.y,
                terrain: tile.terrain,
                expected,
                actual,
            });
        }
    }
    violations
}

/// Render the first `limit` violations plus a terrain histogram, for a legible assert message.
fn describe(violations: &[Violation]) -> String {
    const SHOWN: usize = 10;
    let mut by_terrain: BTreeMap<String, usize> = BTreeMap::new();
    for violation in violations {
        *by_terrain
            .entry(format!("{:?}", violation.terrain))
            .or_default() += 1;
    }
    let head: Vec<String> = violations
        .iter()
        .take(SHOWN)
        .map(|violation| {
            format!(
                "({}, {}) {:?}: expected {:?}, tag {:?}",
                violation.x, violation.y, violation.terrain, violation.expected, violation.actual
            )
        })
        .collect();
    format!(
        "{} tile(s) mismatched; by terrain {:?}; first {}: {}",
        violations.len(),
        by_terrain,
        head.len(),
        head.join(" | ")
    )
}

#[test]
fn the_food_module_tag_matches_the_final_terrain() {
    for grid in [CENSUS_GRID, NAVIGABLE_FIXTURE_GRID] {
        for seed in CENSUS_SEEDS {
            let violations = tag_violations(seed, grid);
            assert!(
                violations.is_empty(),
                "seed {seed} at {}x{}: {}",
                grid.x,
                grid.y,
                describe(&violations)
            );
        }
    }
}

#[test]
fn every_river_delta_has_a_forage_patch() {
    let mut deltas_by_grid: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for grid in [CENSUS_GRID, NAVIGABLE_FIXTURE_GRID] {
        for seed in CENSUS_SEEDS {
            let app = generated_world(seed, grid);
            let registry = app.world.resource::<TileRegistry>();
            let forage = app.world.resource::<ForageRegistry>();
            let mut missing: Vec<UVec2> = Vec::new();
            let mut deltas = 0usize;
            for &entity in registry.tiles.iter() {
                let Some(tile) = app.world.get::<Tile>(entity) else {
                    continue;
                };
                if tile.terrain != TerrainType::RiverDelta {
                    continue;
                }
                deltas += 1;
                if forage.patch(tile.position).is_none() {
                    missing.push(tile.position);
                }
            }
            *deltas_by_grid.entry((grid.x, grid.y)).or_default() += deltas;
            assert!(
                missing.is_empty(),
                "seed {seed} at {}x{}: {} of {deltas} RiverDelta tile(s) carry no ForagePatch \
                 (RiverDelta forage capacity is well above NO_FORAGE_CAPACITY, so an absent patch \
                 means an absent FoodModuleTag): {:?}",
                grid.x,
                grid.y,
                missing.len(),
                &missing[..missing.len().min(10)]
            );
        }
    }

    let census_deltas = deltas_by_grid
        .get(&(CENSUS_GRID.x, CENSUS_GRID.y))
        .copied()
        .unwrap_or_default();
    let fixture_deltas = deltas_by_grid
        .get(&(NAVIGABLE_FIXTURE_GRID.x, NAVIGABLE_FIXTURE_GRID.y))
        .copied()
        .unwrap_or_default();
    println!(
        "RiverDelta tiles swept: {census_deltas} at {}x{}, {fixture_deltas} at {}x{}",
        CENSUS_GRID.x, CENSUS_GRID.y, NAVIGABLE_FIXTURE_GRID.x, NAVIGABLE_FIXTURE_GRID.y
    );

    // The per-case assertion above only bites on tiles it actually finds, so the delta count it
    // already tracks has to be *asserted* somewhere — a counted-but-unasserted quantity is a
    // vacuous guard, silently passing on a map with no deltas at all. Like `hydrology_earthlike.rs`,
    // non-vacuity is claimed across the sweep and on the river-capable fixture, never per
    // (seed, grid): deltas are legitimately absent from an individual small-grid map, but
    // [`NAVIGABLE_FIXTURE_GRID`] is the size whose basins clear the navigable-discharge threshold,
    // so river mouths — and their deltas — reliably exist there.
    assert!(
        fixture_deltas > 0,
        "no RiverDelta tiles at all at {}x{} across the seed sweep ({census_deltas} at {}x{}): this \
         guard has gone vacuous and no longer proves that deltas carry a ForagePatch",
        NAVIGABLE_FIXTURE_GRID.x,
        NAVIGABLE_FIXTURE_GRID.y,
        CENSUS_GRID.x,
        CENSUS_GRID.y
    );
}
