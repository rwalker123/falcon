//! The invariant this whole arc exists to establish: **elevation is the sole authority**.
//!
//! The land mask is a pure derived function of the heightfield (`land = elevation > sea_level`) and
//! no stage writes to it — a stage that wants to move a coastline edits the field and re-derives.
//! Two consequences follow, and they are what this test pins on the **final** map (post-restamp,
//! post-hydrology, post-tag-solver, post-palette-clamp):
//!
//! - no salt-water (`is_ocean`) tile sits above sea level (a "water tile you could stand on")
//! - no land tile sits below sea level (a "continent under the sea")
//!
//! The sampled map that motivated the arc had **543** of the first and **218** of the second.
//!
//! # This asserts on the ENCODED OVERLAY, not the in-process `ElevationField`
//!
//! An earlier version of this test read the f32 `ElevationField` resource and reported 0 violations
//! while a **live export showed 42 salt-water tiles above sea level**. It was testing a
//! representation that does not ship. `elevation_overlay_from_field` quantizes the samples onto a
//! u16 lattice; the client decodes `sample / u16::MAX` and compares that against the published
//! `sea_level`. A tile sitting *exactly* at sea level quantizes to `round(0.62 * 65535) = 40632`,
//! which decodes to `0.6200046` — strictly greater than an unquantized `0.62`. Every one of the 42
//! had exactly that raw sample, with zero variance.
//!
//! So this test reproduces the **client's** arithmetic (`MapView.gd:2437-2445`) against the
//! **published** overlay. That is the only representation whose correctness the player can see.
//!
//! # Scoping
//!
//! **Salt water only.** Hydrology's `NavigableRiver` / `RiverDelta` are **freshwater** stamped on
//! land and legitimately sit above sea level, so a `TerrainTags::WATER`-scoped assertion would fail
//! on every real river mouth.
//!
//! **The land bound is `>=`, not `>`.** `mapgen::restamp_elevation` floors non-mountain land at
//! exactly `sea_level` (coastal smoothing blends a shore tile toward an ocean-inclusive mean and
//! would otherwise drag it under), so land tiles legitimately quantize onto the sea-level lattice
//! point. A tile exactly on the coastline contour is a real value.

use core_sim::{build_headless_app, SimulationConfig, SnapshotHistory, Tile, TileRegistry};
use sim_runtime::TerrainTags;

/// Seeds swept per preset. Never `0` (the "roll from entropy" sentinel).
const SEEDS: [u64; 5] = [1, 7, 42, 1234, 99991];

/// Every shipped preset — the invariant is a property of the pipeline, not of one preset's tuning.
const PRESETS: [&str; 2] = ["earthlike", "polar_contrast"];

/// The quantization lattice the overlay's samples live on. **This must be the same constant the
/// encoder uses for both the samples and the published `sea_level`** — that they were two
/// independent literals, with only the samples quantized, is precisely the defect this test pins.
const OVERLAY_SAMPLE_SCALE: f32 = u16::MAX as f32;

/// How far the realized land fraction may drift from `macro_land.target_land_pct`.
const LAND_PCT_TOLERANCE: f32 = 0.02;

struct FinalMapCensus {
    ocean_above_sea: usize,
    land_below_sea: usize,
    land_tiles: usize,
    total_tiles: usize,
    worst_ocean: Option<(u32, u32, u16, f32)>,
    worst_land: Option<(u32, u32, u16, f32)>,
}

/// Salt water — the scope of the "no water above sea level" assertion. Water-tagged and not
/// freshwater, i.e. exactly `mapgen`'s `is_ocean` as it survives to the final map.
fn is_salt_water(tile: &Tile) -> bool {
    tile.terrain_tags.contains(TerrainTags::WATER)
        && !tile.terrain_tags.contains(TerrainTags::FRESHWATER)
}

/// Land — no water tag at all. Freshwater river hexes are neither land nor salt water here.
fn is_land(tile: &Tile) -> bool {
    !tile.terrain_tags.contains(TerrainTags::WATER)
}

/// Run the REAL Startup chain for `(preset, seed)` and census the **published overlay** against the
/// published terrain, using the client's own decode-and-compare.
fn census(preset_id: &str, seed: u64) -> FinalMapCensus {
    let mut app = build_headless_app();

    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = preset_id.to_string();
    config.map_seed = seed;
    app.world.insert_resource(config);

    // One update runs the whole Startup worldgen chain in shipped order and captures a snapshot.
    app.update();

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|s| (**s).clone())
        .expect("snapshot after worldgen");
    let overlay = &snapshot.elevation_overlay;
    let width = overlay.width as usize;
    // The published threshold, exactly as it goes over the wire.
    let sea_level = overlay.sea_level;

    let registry = app.world.resource::<TileRegistry>();

    let mut ocean_above_sea = 0usize;
    let mut land_below_sea = 0usize;
    let mut land_tiles = 0usize;
    let mut total_tiles = 0usize;
    let mut worst_ocean: Option<(u32, u32, u16, f32)> = None;
    let mut worst_land: Option<(u32, u32, u16, f32)> = None;

    for &entity in registry.tiles.iter() {
        let Some(tile) = app.world.get::<Tile>(entity) else {
            continue;
        };
        total_tiles += 1;
        let (x, y) = (tile.position.x, tile.position.y);
        let Some(&raw) = overlay.samples.get(y as usize * width + x as usize) else {
            continue;
        };
        // THE CLIENT'S ARITHMETIC (`MapView.gd:2437-2445`), verbatim.
        let decoded = raw as f32 / OVERLAY_SAMPLE_SCALE;

        if is_salt_water(tile) {
            if decoded > sea_level {
                ocean_above_sea += 1;
                if worst_ocean.is_none_or(|(_, _, _, d)| decoded > d) {
                    worst_ocean = Some((x, y, raw, decoded));
                }
            }
        } else if is_land(tile) {
            land_tiles += 1;
            if decoded < sea_level {
                land_below_sea += 1;
                if worst_land.is_none_or(|(_, _, _, d)| decoded < d) {
                    worst_land = Some((x, y, raw, decoded));
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

/// **The acceptance number for the elevation-authority arc**, on the representation that ships.
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
                    "{preset_id} seed {seed}: {} SALT-WATER tiles above sea level \
                     (worst (x,y,raw,decoded)={:?})",
                    c.ocean_above_sea, c.worst_ocean
                ));
            }
            if c.land_below_sea != 0 {
                failures.push(format!(
                    "{preset_id} seed {seed}: {} LAND tiles below sea level \
                     (worst (x,y,raw,decoded)={:?})",
                    c.land_below_sea, c.worst_land
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "elevation authority violated on the PUBLISHED overlay — \
         the encoded map and the published sea level disagree:\n{}",
        failures.join("\n")
    );
}

/// `target_land_pct` must be met by *shaping the field* (the contour anchor), with no mask
/// repainting anywhere in the pipeline.
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

/// Non-vacuity: the census must be looking at a populated map with both kinds of tile, and at a
/// real overlay rather than an empty one.
#[test]
fn the_census_sees_a_real_map() {
    let c = census("earthlike", SEEDS[0]);
    assert!(c.total_tiles > 0, "no tiles were generated");
    assert!(c.land_tiles > 0, "the map has no land at all");
    assert!(
        c.land_tiles < c.total_tiles,
        "the map is entirely land — no ocean tiles to check"
    );
}

/// The encoder's contract, asserted directly: the published `sea_level` must sit **on the sample
/// lattice**. This is the root cause in one line — a quantized value compared against an
/// unquantized threshold — and it is worth pinning independently of any map, because a boundary
/// tile only exposes it when one happens to land exactly on the contour.
#[test]
fn the_published_sea_level_lies_on_the_sample_quantization_lattice() {
    for preset_id in PRESETS {
        let mut app = build_headless_app();
        let mut config = app.world.resource::<SimulationConfig>().clone();
        config.map_preset_id = preset_id.to_string();
        config.map_seed = SEEDS[0];
        app.world.insert_resource(config);
        app.update();

        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .last_snapshot
            .as_ref()
            .map(|s| (**s).clone())
            .expect("snapshot after worldgen");
        let sea_level = snapshot.elevation_overlay.sea_level;

        let on_lattice = (sea_level * OVERLAY_SAMPLE_SCALE).round() / OVERLAY_SAMPLE_SCALE;
        assert_eq!(
            sea_level.to_bits(),
            on_lattice.to_bits(),
            "{preset_id}: published sea_level {sea_level} is NOT on the u16 sample lattice \
             (nearest lattice point {on_lattice}). Samples are quantized and the threshold is not, \
             so a tile sitting exactly at sea level decodes to strictly greater than it."
        );
    }
}
