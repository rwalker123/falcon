// Guards the per-map biome palette (`docs/plan_biome_palette.md` §9). The palette is a
// curated, seed-driven, map-size-scaled subset of the 37 biomes chosen at world-gen; it
// must be a HARD invariant of the finished map (after the tag solver + post-solver clamp).
// These tests generate maps in-process and inspect the resulting tile biomes.
mod common;

use std::collections::HashSet;

use bevy::math::UVec2;
use core_sim::{
    build_headless_app, BiomePalette, SimulationConfig, SimulationConfigMetadata, SnapshotHistory,
};
use sim_runtime::{TerrainType, WorldSnapshot};

/// Generate an earthlike map and return the finished snapshot plus the palette the sim
/// actually computed and enforced for it.
fn generate(width: u32, height: u32, seed: u64) -> (WorldSnapshot, BiomePalette) {
    common::ensure_test_config();
    let mut app = build_headless_app();
    if let Some(mut md) = app.world.get_resource_mut::<SimulationConfigMetadata>() {
        md.set_seed_random(false);
    }
    if let Some(mut cfg) = app.world.get_resource_mut::<SimulationConfig>() {
        cfg.map_preset_id = "earthlike".to_string();
        cfg.grid_size = UVec2::new(width, height);
        cfg.map_seed = seed;
    }
    app.update();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|s| (**s).clone())
        .expect("snapshot after worldgen");
    let palette = app
        .world
        .get_resource::<BiomePalette>()
        .expect("palette resource inserted for a preset map")
        .clone();
    (snapshot, palette)
}

/// The distinct set of biomes present on a finished map.
fn distinct_biomes(snap: &WorldSnapshot) -> HashSet<TerrainType> {
    snap.terrain.samples.iter().map(|s| s.terrain).collect()
}

#[test]
fn no_off_palette_biome_on_a_finished_map() {
    // The core invariant: every biome present after the tag solver + clamp is on the
    // map's computed palette. Check across several sizes and seeds.
    for &(w, h) in &[(64u32, 48u32), (128, 96), (200, 150)] {
        for seed in [1u64, 7, 42] {
            let (snap, palette) = generate(w, h, seed);
            for sample in &snap.terrain.samples {
                assert!(
                    palette.contains(sample.terrain),
                    "off-palette biome {:?} on a {w}x{h} seed={seed} map",
                    sample.terrain
                );
            }
        }
    }
}

#[test]
fn small_map_has_fewer_distinct_biomes_than_large_map() {
    let (small, _) = generate(64, 48, 42);
    let (large, _) = generate(256, 192, 42);
    let small_n = distinct_biomes(&small).len();
    let large_n = distinct_biomes(&large).len();
    assert!(
        small_n < large_n,
        "expected small map to be more legible: small={small_n} large={large_n}"
    );
}

#[test]
fn different_seeds_produce_different_biome_sets() {
    let (a, _) = generate(160, 120, 1);
    let (b, _) = generate(160, 120, 2);
    assert_ne!(
        distinct_biomes(&a),
        distinct_biomes(&b),
        "two seeds produced identical biome sets"
    );
}

#[test]
fn climate_coverage_is_preserved_on_a_large_map() {
    // A large earthlike map spans ocean, fertile lowland, and polar niches — none of
    // those spanned niches may be left empty by the palette.
    let (snap, _) = generate(256, 192, 42);
    let present = distinct_biomes(&snap);
    let has_niche = |members: &[TerrainType]| members.iter().any(|t| present.contains(t));
    assert!(
        has_niche(&[
            TerrainType::DeepOcean,
            TerrainType::ContinentalShelf,
            TerrainType::InlandSea,
        ]),
        "no ocean biome present"
    );
    assert!(
        has_niche(&[
            TerrainType::AlluvialPlain,
            TerrainType::PrairieSteppe,
            TerrainType::Floodplain,
            TerrainType::MixedWoodland,
        ]),
        "no fertile lowland biome present"
    );
    assert!(
        has_niche(&[
            TerrainType::Tundra,
            TerrainType::PeriglacialSteppe,
            TerrainType::SeasonalSnowfield,
            TerrainType::Glacier,
        ]),
        "no polar biome present"
    );
}
