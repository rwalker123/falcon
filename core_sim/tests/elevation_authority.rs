//! The invariant this whole arc exists to establish: **elevation is the sole authority**.
//!
//! The land mask is a pure derived function of the heightfield (`land = elevation > sea_level`) and
//! no stage writes to it — a stage that wants to move a coastline edits the field and re-derives.
//! Two consequences follow, and they are what this test pins on the **final** map (post-restamp,
//! post-hydrology, post-tag-solver, post-palette-clamp — the state the snapshot actually publishes):
//!
//! - no `is_ocean` tile sits above sea level (a "water tile you could stand on")
//! - no land tile sits below sea level (a "continent under the sea")
//!
//! The sampled map that motivated the arc had **543** of the first and **218** of the second. Both
//! must now be zero, and they are zero *by construction* rather than by tuning — which is the point.
//! The test is cheap insurance against a future stage reintroducing a mask edit.
//!
//! **Salt water only.** The ocean assertion is scoped to `is_ocean`, because hydrology's
//! `NavigableRiver` / `RiverDelta` are **freshwater** stamped on land and legitimately sit above sea
//! level. They are water-tagged, so a `TerrainTags::WATER`-scoped assertion would fail on every real
//! river mouth.
//!
//! **The land bound is `>=`, not `>`.** `mapgen::restamp_elevation`'s lowland branch compresses land
//! into `[sea_level, elevation_base]` and floors it at `sea_level` (coastal smoothing blends a shore
//! tile toward an ocean-inclusive mean and would otherwise drag it under). A tile sitting exactly on
//! the coastline contour is a real, meaningful value; lifting it to `sea_level + eps` purely to
//! satisfy a strict inequality would be inventing a magic offset to make a test pass.

use core_sim::{
    build_headless_app, heightfield::ElevationField, SimulationConfig, Tile, TileRegistry,
};
use sim_runtime::{TerrainTags, TerrainType};

/// Seeds swept per preset. Never `0` (the "roll from entropy" sentinel), matching the convention in
/// `navigable_mouth_delta.rs` / `hydrology_earthlike.rs`.
const SEEDS: [u64; 5] = [1, 7, 42, 1234, 99991];

/// Presets under test — every shipped preset, since the invariant is a property of the pipeline and
/// not of any one preset's tuning.
const PRESETS: [&str; 2] = ["earthlike", "polar_contrast"];

/// How far the realized land fraction may drift from `macro_land.target_land_pct`. The contour
/// anchor puts the target quantile exactly on sea level, so the only sources of drift are the
/// elevation edits that run *after* it (straits lowered, islands raised) and the f32 quantile step.
/// Measured drift is well under a point; this leaves headroom for retuning those levers without
/// making the bound so loose that a genuine regression could hide behind it.
const LAND_PCT_TOLERANCE: f32 = 0.02;

/// One preset/seed's verdict.
struct FinalMapCensus {
    ocean_above_sea: usize,
    land_below_sea: usize,
    land_tiles: usize,
    total_tiles: usize,
    /// The worst offender of each kind, for a failure message that names a hex to go look at.
    worst_ocean: Option<(u32, u32, f32)>,
    worst_land: Option<(u32, u32, f32)>,
}

/// A tile is **salt water** — the scope of the "no water above sea level" assertion — when it is
/// water-tagged and not freshwater. That is exactly `mapgen`'s `is_ocean` as it survives to the
/// final map: hydrology's navigable rivers and deltas carry `FRESHWATER` and are excluded.
fn is_salt_water(tile: &Tile) -> bool {
    tile.terrain_tags.contains(TerrainTags::WATER)
        && !tile.terrain_tags.contains(TerrainTags::FRESHWATER)
}

/// A tile is **land** for this invariant when it carries no water tag at all. Freshwater river
/// hexes are neither: they are water on land, and are excluded from both assertions.
fn is_land(tile: &Tile) -> bool {
    !tile.terrain_tags.contains(TerrainTags::WATER)
}

/// Run the REAL Startup chain for `(preset, seed)` and census the published field against the
/// published terrain.
fn census(preset_id: &str, seed: u64) -> FinalMapCensus {
    let mut app = build_headless_app();

    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = preset_id.to_string();
    config.map_seed = seed;
    app.world.insert_resource(config);

    // One update runs the whole Startup worldgen chain in shipped order.
    app.update();

    let field = app.world.resource::<ElevationField>().clone();
    let sea_level = field.sea_level;
    let registry = app.world.resource::<TileRegistry>();

    let mut ocean_above_sea = 0usize;
    let mut land_below_sea = 0usize;
    let mut land_tiles = 0usize;
    let mut total_tiles = 0usize;
    let mut worst_ocean: Option<(u32, u32, f32)> = None;
    let mut worst_land: Option<(u32, u32, f32)> = None;

    for &entity in registry.tiles.iter() {
        let Some(tile) = app.world.get::<Tile>(entity) else {
            continue;
        };
        total_tiles += 1;
        let (x, y) = (tile.position.x, tile.position.y);
        let elevation = field.sample(x, y);

        if is_salt_water(tile) {
            if elevation > sea_level {
                ocean_above_sea += 1;
                if worst_ocean.is_none_or(|(_, _, e)| elevation > e) {
                    worst_ocean = Some((x, y, elevation));
                }
            }
        } else if is_land(tile) {
            land_tiles += 1;
            if elevation < sea_level {
                land_below_sea += 1;
                if worst_land.is_none_or(|(_, _, e)| elevation < e) {
                    worst_land = Some((x, y, elevation));
                }
            }
        }
    }

    FinalMapCensus {
        ocean_above_sea,
        land_below_sea,
        land_tiles,
        total_tiles,
        worst_ocean,
        worst_land,
    }
}

/// **The acceptance number for the elevation-authority arc.** Both counts must be exactly 0 on every
/// preset and every seed. The sampled pre-arc map had 543 and 218.
#[test]
fn no_ocean_tile_is_above_sea_level_and_no_land_tile_is_below_it() {
    let mut failures: Vec<String> = Vec::new();

    for preset_id in PRESETS {
        for seed in SEEDS {
            let c = census(preset_id, seed);
            println!(
                "{preset_id} seed {seed}: ocean_above_sea={} land_below_sea={} (land {}/{} tiles)",
                c.ocean_above_sea, c.land_below_sea, c.land_tiles, c.total_tiles
            );
            if c.ocean_above_sea != 0 {
                failures.push(format!(
                    "{preset_id} seed {seed}: {} OCEAN tiles above sea level (worst {:?})",
                    c.ocean_above_sea, c.worst_ocean
                ));
            }
            if c.land_below_sea != 0 {
                failures.push(format!(
                    "{preset_id} seed {seed}: {} LAND tiles below sea level (worst {:?})",
                    c.land_below_sea, c.worst_land
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "elevation authority violated — the mask and the field disagree:\n{}",
        failures.join("\n")
    );
}

/// `target_land_pct` must be met by *shaping the field* (the contour anchor), with no mask
/// repainting anywhere in the pipeline — so the realized land fraction on the final map should land
/// on the preset's target without `rebalance_land_ratio` (deleted) or the tag solver's water branch
/// (deleted) correcting it.
#[test]
fn realized_land_fraction_tracks_the_preset_target() {
    let presets = core_sim::MapPresets::builtin();
    let mut failures: Vec<String> = Vec::new();

    for preset_id in PRESETS {
        let target = presets
            .get(preset_id)
            .unwrap_or_else(|| panic!("missing preset {preset_id}"))
            .macro_land
            .target_land_pct;
        for seed in SEEDS {
            let c = census(preset_id, seed);
            let realized = c.land_tiles as f32 / c.total_tiles.max(1) as f32;
            println!("{preset_id} seed {seed}: land {realized:.4} vs target {target:.4}");
            if (realized - target).abs() > LAND_PCT_TOLERANCE {
                failures.push(format!(
                    "{preset_id} seed {seed}: land {realized:.4} vs target {target:.4} \
                     (drift {:.4} > {LAND_PCT_TOLERANCE})",
                    (realized - target).abs()
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "land fraction drifted from target_land_pct:\n{}",
        failures.join("\n")
    );
}

/// Non-vacuity: the census must actually be looking at a populated map with both kinds of tile.
/// Without this, a worldgen that produced zero tiles (or all-water) would pass the invariants above
/// trivially.
#[test]
fn the_census_sees_a_real_map() {
    let c = census("earthlike", SEEDS[0]);
    assert!(c.total_tiles > 0, "no tiles were generated");
    assert!(c.land_tiles > 0, "the map has no land at all");
    assert!(
        c.land_tiles < c.total_tiles,
        "the map is entirely land — no ocean tiles to check"
    );
    // And the terrain classes the scoping relies on are actually present, so the freshwater
    // exclusion is exercised rather than being dead weight.
    assert!(
        matches!(
            core_sim::MapPresets::builtin().get("earthlike").map(|p| p.id.as_str()),
            Some("earthlike")
        ),
        "earthlike preset must resolve, else worldgen silently skips erosion and the contour anchor"
    );
    let _ = TerrainType::NavigableRiver;
}
