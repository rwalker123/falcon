// Guards the earthlike continental-shelf fraction. ContinentalShelf should be a
// thin, Earth-like slice of the ocean and — critically — that fraction should be
// stable across map sizes, proving the shelf band scales with map dimension
// instead of being a fixed tile count. Earth's shelves are ~5-8% of ocean area;
// procedural coastline variance means any single map may drift a little around
// that, so we assert the MEAN sits in the target band while every individual map
// stays within an Earth-like margin. See core_sim/CLAUDE.md (World Generation).
mod common;

use bevy::math::UVec2;
use core_sim::{build_headless_app, SimulationConfig, SimulationConfigMetadata, SnapshotHistory};
use sim_runtime::{TerrainType, WorldSnapshot};

/// Target band for the MEAN shelf fraction across sampled maps — the Earth-like
/// goal (~5-8% of ocean area).
const MEAN_MIN: f64 = 0.05;
const MEAN_MAX: f64 = 0.08;
/// Per-map margin: any single map must stay Earth-like, but coastline variance
/// lets individual seeds sit a little outside the mean band.
const SAMPLE_MIN: f64 = 0.04;
const SAMPLE_MAX: f64 = 0.09;

fn generate(width: u32, height: u32, seed: u64) -> WorldSnapshot {
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
    app.world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|s| (**s).clone())
        .expect("snapshot after worldgen")
}

/// ContinentalShelf as a fraction of open ocean (DeepOcean + ContinentalShelf).
fn shelf_fraction_of_ocean(snap: &WorldSnapshot) -> (f64, usize, usize) {
    let mut shelf = 0usize;
    let mut deep = 0usize;
    for sample in &snap.terrain.samples {
        match sample.terrain {
            TerrainType::ContinentalShelf => shelf += 1,
            TerrainType::DeepOcean => deep += 1,
            _ => {}
        }
    }
    let ocean = shelf + deep;
    let frac = if ocean > 0 {
        shelf as f64 / ocean as f64
    } else {
        0.0
    };
    (frac, shelf, ocean)
}

#[test]
fn earthlike_shelf_is_earth_like_fraction_across_sizes() {
    // Sizes span the small "Standard" play size up to the preset's native
    // 256x192 — the point is that the fraction stays flat across a ~3.7x range
    // in dimension. Native size uses a single seed to bound CI time (its
    // full-pipeline generation is the slowest).
    let samples: [(u32, u32, u64); 8] = [
        (80, 52, 0x0FA1_C0DE),
        (80, 52, 0x5EED_F00D),
        (80, 52, 0x0000_BEEF),
        (128, 96, 0x0FA1_C0DE),
        (128, 96, 0x5EED_F00D),
        (192, 128, 0x0FA1_C0DE),
        (192, 128, 0x5EED_F00D),
        (256, 192, 0x0FA1_C0DE),
    ];

    let mut fractions: Vec<f64> = Vec::new();
    let mut out_of_margin: Vec<String> = Vec::new();
    println!("\n=== earthlike shelf % of ocean ===");
    for (w, h, seed) in samples {
        let snap = generate(w, h, seed);
        let (frac, shelf, ocean) = shelf_fraction_of_ocean(&snap);
        fractions.push(frac);
        println!(
            "{:>4}x{:<4} seed={:016x}: shelf={:>5} ocean={:>6} -> {:>5.1}% of ocean",
            w,
            h,
            seed,
            shelf,
            ocean,
            frac * 100.0
        );
        if !(SAMPLE_MIN..=SAMPLE_MAX).contains(&frac) {
            out_of_margin.push(format!(
                "{w}x{h} seed={seed:016x}: {:.1}% outside Earth-like margin [{:.0}%, {:.0}%]",
                frac * 100.0,
                SAMPLE_MIN * 100.0,
                SAMPLE_MAX * 100.0
            ));
        }
    }

    let mean = fractions.iter().sum::<f64>() / fractions.len() as f64;
    println!("mean = {:.1}% of ocean", mean * 100.0);

    assert!(
        out_of_margin.is_empty(),
        "some maps left the Earth-like margin:\n{}",
        out_of_margin.join("\n")
    );
    assert!(
        (MEAN_MIN..=MEAN_MAX).contains(&mean),
        "mean shelf fraction {:.1}% outside target band [{:.0}%, {:.0}%]",
        mean * 100.0,
        MEAN_MIN * 100.0,
        MEAN_MAX * 100.0
    );
}
