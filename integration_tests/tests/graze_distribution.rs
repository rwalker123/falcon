//! **Look at the pasture layer on a real map** (Grazing Phase 2a — `docs/plan_grazing_foundation.md`
//! §7.1). The whole point of shipping the graze layer *inert* is to check its distribution before the
//! fauna model is bet on it: herd carrying capacity, competition, overgrazing, migration and spawn
//! placement all become functions of this layer in Phase 2b/2c. If prairie isn't pasture and forest
//! isn't poor, we need to know **now**, not after every herd in the game has resized.
//!
//! `graze_distribution_report` prints the measurement (run it with `--nocapture`); the assertions
//! around it are the guards that keep the model claims true as biomes and levers are retuned.

mod common;

use std::collections::BTreeMap;

use bevy::math::UVec2;
use core_sim::{
    build_headless_app, FaunaConfig, GrazeRegistry, SimulationConfig, SimulationConfigMetadata,
    SnapshotHistory,
};
use sim_runtime::{TerrainTags, TerrainType, WorldSnapshot};

/// The standard campaign map. Big enough for a real biome mix, small enough to run in a test.
const MAP_WIDTH: u32 = 80;
const MAP_HEIGHT: u32 = 52;

/// Seeds sampled so the measurement is a *distribution*, not one lucky map.
const SEEDS: [u64; 3] = [11, 4242, 90210];

/// A land tile with **no pasture at all** is a real thing (glacier, salt flat, lava), but it must stay
/// a minority: graze is the substrate herds live on, and a map that is mostly dead ground has no
/// fauna model on it. Well clear of the measured ~1-3%.
const MAX_ZERO_GRAZE_LAND_FRACTION: f64 = 0.15;

fn generate(seed: u64) -> (WorldSnapshot, GrazeRegistry) {
    common::ensure_test_config();
    let mut app = build_headless_app();
    if let Some(mut md) = app.world.get_resource_mut::<SimulationConfigMetadata>() {
        md.set_seed_random(false);
    }
    if let Some(mut cfg) = app.world.get_resource_mut::<SimulationConfig>() {
        cfg.map_preset_id = "earthlike".to_string();
        cfg.grid_size = UVec2::new(MAP_WIDTH, MAP_HEIGHT);
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
    let graze = app.world.resource::<GrazeRegistry>().clone();
    (snapshot, graze)
}

/// Per-biome roll-up of the land the map actually generated.
#[derive(Default, Clone, Copy)]
struct BiomeRow {
    tiles: usize,
    total_capacity: f64,
}

fn land_biome_rows(snapshot: &WorldSnapshot) -> (BTreeMap<String, BiomeRow>, usize) {
    let mut rows: BTreeMap<String, BiomeRow> = BTreeMap::new();
    let mut land_tiles = 0usize;
    for tile in &snapshot.tiles {
        if tile.terrain_tags.contains(TerrainTags::WATER) {
            continue;
        }
        land_tiles += 1;
        let row = rows.entry(format!("{:?}", tile.terrain)).or_default();
        row.tiles += 1;
        row.total_capacity += f64::from(tile.graze_capacity);
    }
    (rows, land_tiles)
}

/// THE DELIVERABLE: total capacity, the zero-graze fraction of land, and the per-biome histogram.
/// Prints; asserts only the model claims (below), so retuning the table changes the numbers, not the
/// verdict.
#[test]
fn graze_distribution_report() {
    for seed in SEEDS {
        let (snapshot, registry) = generate(seed);
        let (rows, land_tiles) = land_biome_rows(&snapshot);

        let total_capacity: f64 = rows.values().map(|row| row.total_capacity).sum();
        let zero_graze_land: usize = rows
            .values()
            .filter(|row| row.total_capacity <= 0.0)
            .map(|row| row.tiles)
            .sum();
        let zero_fraction = zero_graze_land as f64 / land_tiles.max(1) as f64;

        println!("\n=== graze distribution — earthlike {MAP_WIDTH}x{MAP_HEIGHT} seed {seed} ===");
        println!(
            "land tiles          : {land_tiles}\n\
             graze patches       : {}  (one per land tile with positive capacity)\n\
             total capacity      : {total_capacity:.0}\n\
             zero-graze land     : {zero_graze_land} tiles ({:.1}% of land)",
            registry.len(),
            zero_fraction * 100.0
        );

        let mut ranked: Vec<(&String, &BiomeRow)> = rows.iter().collect();
        ranked.sort_by(|a, b| {
            b.1.total_capacity
                .partial_cmp(&a.1.total_capacity)
                .expect("finite capacities")
        });
        println!(
            "\n  {:<22} {:>6} {:>12} {:>8} {:>7}",
            "biome", "tiles", "capacity", "per-tile", "share"
        );
        for (biome, row) in ranked.iter() {
            println!(
                "  {:<22} {:>6} {:>12.0} {:>8.0} {:>6.1}%",
                biome,
                row.tiles,
                row.total_capacity,
                row.total_capacity / row.tiles.max(1) as f64,
                row.total_capacity / total_capacity.max(1.0) * 100.0
            );
        }

        // --- The model claims, guarded ---

        // Every patch is seeded FULL and nothing eats it in Phase 2a, so the live biomass IS the
        // capacity. If this ever fails, something started consuming graze — which is Phase 2b.
        let live_biomass: f64 = registry
            .patches
            .values()
            .map(|patch| f64::from(patch.biomass))
            .sum();
        assert!(
            (live_biomass - total_capacity).abs() < 1.0,
            "Phase 2a is INERT: nothing may draw graze down ({live_biomass:.0} vs {total_capacity:.0})"
        );

        // Dead ground exists, but the map is not made of it.
        assert!(
            zero_fraction < MAX_ZERO_GRAZE_LAND_FRACTION,
            "seed {seed}: {:.1}% of land carries no graze — the fauna model has no substrate",
            zero_fraction * 100.0
        );

        // Water is never pasture, and it is never a *patch* either (an absent reading, not a zero one).
        for tile in &snapshot.tiles {
            if tile.terrain_tags.contains(TerrainTags::WATER) {
                assert_eq!(tile.graze_capacity, 0.0, "water is not pasture");
                assert!(registry.patch(UVec2::new(tile.x, tile.y)).is_none());
            }
        }
    }
}

/// **Prairie is pasture; forest is poor.** The inversion the whole two-stock split exists to create —
/// a closed canopy shades out the ground cover, so your best farm is usually not your best pasture.
/// Asserted on the *per-tile* capacity (the land's property), not on totals, which merely reflect how
/// much of each biome the map happened to roll.
#[test]
fn prairie_out_pastures_forest_per_tile() {
    let graze = &FaunaConfig::builtin().graze;
    let prairie = graze.capacity_for(TerrainType::PrairieSteppe);
    for poor in [
        TerrainType::MixedWoodland,
        TerrainType::BorealTaiga,
        TerrainType::Tundra,
        TerrainType::HotDesertErg,
        TerrainType::AlpineMountain,
    ] {
        assert!(
            prairie > graze.capacity_for(poor),
            "prairie ({prairie}) must out-pasture {poor:?} ({})",
            graze.capacity_for(poor)
        );
    }
}
