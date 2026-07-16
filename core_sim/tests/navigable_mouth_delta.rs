//! Regression: a navigable river's mouth hex must never be left as dry land.
//!
//! Every hex that carries a `river_channel` bit is part of a navigable river's hex chain (or its
//! delta mouth), so its terrain must be `NavigableRiver` or `RiverDelta`. A channel bit sitting on
//! dry land means a later worldgen pass restamped a hydrology-placed water/delta tile back to land,
//! orphaning the channel — the client's navigable-render pass (gated on `terrain == NavigableRiver`)
//! then draws the river ending one hex short of the sea with no delta.
//!
//! The bug this pins: `apply_tag_budget_solver`'s Fertile **add** branch converted a hydrology
//! placed `RiverDelta` (a polar delta, lacking the `Fertile` tag, so not caught by the pass's
//! `Fertile`/`Water` skips) to `AlluvialPlain` — the one reduction/addition branch that lacked the
//! `terrain != RiverDelta` guard every other water/wetland/fertile/coastal branch already carries.
//! Concretely reproduced on earthlike seed 12736602826901522706 at 104×64, hex @(50,17).
//!
//! Unlike the harness in `hydrology_earthlike.rs` (which runs `generate_hydrology` *last*, after the
//! rest of Startup, and so cannot see a later-pass clobber), this test drives the REAL Startup chain
//! via `build_headless_app` — hydrology → tag solver → palette clamp → reconcile — exactly as a live
//! game generates its map.

use bevy::prelude::UVec2;

use core_sim::{build_headless_app, SimulationConfig, Tile, TileRegistry};
use sim_runtime::TerrainType;

/// The census seeds `hydrology_earthlike.rs` sweeps its structural river invariants across, at that
/// file's default earthlike size (80×52). Never 0 (the "roll from entropy" sentinel).
const CENSUS_SEEDS: [u64; 6] = [1, 2, 3, 4, 5, 119_304_647];

/// A hex carrying a channel bit must be water-terrain (navigable) or its delta mouth — never land.
fn channel_bit_allows(terrain: TerrainType) -> bool {
    matches!(
        terrain,
        TerrainType::NavigableRiver | TerrainType::RiverDelta
    )
}

/// Build the real app, generate the map for `(seed, w, h)` on the earthlike preset, and return the
/// list of `(x, y, terrain)` violations — hexes with a `river_channel` bit but non-navigable,
/// non-delta terrain.
fn channel_on_land_violations(seed: u64, w: u32, h: u32) -> Vec<(u32, u32, TerrainType)> {
    let mut app = build_headless_app();

    // Override the env-loaded config with the case under test, before the Startup chain reads it.
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = UVec2::new(w, h);
    app.world.insert_resource(config);

    // One update runs the Startup worldgen chain in shipped order.
    app.update();

    let registry = app.world.resource::<TileRegistry>();
    let mut violations = Vec::new();
    for &entity in registry.tiles.iter() {
        let Some(tile) = app.world.get::<Tile>(entity) else {
            continue;
        };
        if tile.river_channel != 0 && !channel_bit_allows(tile.terrain) {
            violations.push((tile.position.x, tile.position.y, tile.terrain));
        }
    }
    violations
}

#[test]
fn no_river_channel_bit_sits_on_dry_land_across_census_seeds() {
    for seed in CENSUS_SEEDS {
        let violations = channel_on_land_violations(seed, 80, 52);
        assert!(
            violations.is_empty(),
            "seed {seed}: {} hex(es) carry a river_channel bit on non-navigable/non-delta terrain: \
             {violations:?}",
            violations.len()
        );
    }
}

#[test]
fn navigable_mouth_is_a_delta_not_dry_land_on_the_reported_seed() {
    // The exact reported repro: without the Fertile-add RiverDelta guard, @(50,17) is AlluvialPlain
    // (id 10) carrying river_channel = 3.
    let violations = channel_on_land_violations(12_736_602_826_901_522_706, 104, 64);
    assert!(
        violations.is_empty(),
        "{} orphaned channel hex(es) on the reported map (expected the navigable mouth to be a \
         RiverDelta): {violations:?}",
        violations.len()
    );
}
