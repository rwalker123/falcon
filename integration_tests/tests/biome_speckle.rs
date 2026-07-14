//! **Biome spatial coherence** — measures the "checkerboard of single-hex terrain" the per-map biome
//! palette was originally introduced to solve.
//!
//! The palette attacks the checkerboard by *shrinking the vocabulary* (thinning each niche to `k`
//! members). That does reduce speckle — but only by making the confetti monochrome, and it pays for
//! it by **deleting biomes outright**: `FertileLowland k_small = 2` admitted only the two `must_have`
//! members and silently removed every **forest** and every river **floodplain** from Standard maps,
//! remapping both into `AlluvialPlain` (its niche-mate).
//!
//! Raising that `k` to full membership restores them — but it raises the vocabulary back up, which is
//! exactly what the palette was thinning. **So this test exists to answer the question that decides
//! it: did restoring the biomes bring the checkerboard back?** Argue about the palette with these
//! numbers, not with adjectives.
//!
//! It is a MEASUREMENT (run with `--nocapture`) plus one loose guard. Tiny is the case that matters —
//! a small map has the least room for a large vocabulary to form coherent regions, and it is the map
//! the palette was introduced for.

mod common;

use std::collections::{BTreeMap, HashSet, VecDeque};

use bevy::math::UVec2;
use core_sim::{build_headless_app, SimulationConfig, SimulationConfigMetadata, SnapshotHistory};
use sim_runtime::{TerrainTags, WorldSnapshot};

/// Seeds sampled so the measurement is a *distribution*, not one lucky map.
const SEEDS: [u64; 2] = [11, 4242];

/// Pure confetti — a land hex with **no** same-biome land neighbour. This is the checkerboard,
/// quantified.
///
/// **MEASURED TODAY: ~24% of land on Tiny and ~34% on STANDARD, with a median same-biome region size
/// of ONE HEX.** That is the broken state, and it is broken *with the palette's thinning switched on*
/// — which is the finding that matters: **the palette does not fix the checkerboard.** Dropping
/// `FertileLowland` from k=4 to k=2 (which deletes every forest and every river floodplain from the
/// map) moves Tiny only 24.5% → 22.2%. Two points of speckle, for two whole biomes.
///
/// **And the Standard map — the one the game actually ships — is WORSE than Tiny (34% vs 24%)**, even
/// though it is thinned *less*. That kills the "small maps are the problem, so thin harder there"
/// premise the palette's size-interpolated `k` is built on: speckle tracks the *climate field's*
/// spatial frequency, not the size of the vocabulary.
///
/// So this ceiling is a **regression backstop at today's bad level**, not a target. The target is
/// `< 5%`, bought by a **de-speckle pass** (absorb an island into its dominant neighbour) — which
/// fixes coherence *without deleting anything*. See `docs/plan_biome_coherence.md`. Tighten this
/// constant when that lands; it is the acceptance test for it.
const MAX_ISLAND_LAND_FRACTION: f64 = 0.40;

/// What the de-speckle pass must achieve. Not asserted yet — it is the goal `plan_biome_coherence.md`
/// is written against, recorded here so the acceptance criterion lives beside the measurement.
#[allow(dead_code)]
const TARGET_ISLAND_LAND_FRACTION: f64 = 0.05;

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

/// Odd-r pointy-top hex neighbours, wrap-aware in x (matching the sim's `grid_utils` convention).
fn hex_neighbors(x: u32, y: u32, w: u32, h: u32) -> Vec<(u32, u32)> {
    let odd = y % 2 == 1;
    let deltas: [(i32, i32); 6] = if odd {
        [(1, 0), (-1, 0), (0, -1), (1, -1), (0, 1), (1, 1)]
    } else {
        [(1, 0), (-1, 0), (-1, -1), (0, -1), (-1, 1), (0, 1)]
    };
    let mut out = Vec::with_capacity(6);
    for (dx, dy) in deltas {
        let ny = y as i32 + dy;
        if ny < 0 || ny >= h as i32 {
            continue;
        }
        let nx = (x as i32 + dx).rem_euclid(w as i32);
        out.push((nx as u32, ny as u32));
    }
    out
}

struct Coherence {
    land: usize,
    islands: usize,
    regions: usize,
    singletons: usize,
    tiny: usize,
    median: usize,
    largest: usize,
    biomes_present: usize,
    island_by_biome: Vec<(String, usize)>,
}

fn measure(snapshot: &WorldSnapshot, w: u32, h: u32) -> Coherence {
    // Land biome by coord; water is excluded entirely (the shoreline is not a speckle question).
    let mut biome: BTreeMap<(u32, u32), String> = BTreeMap::new();
    for tile in &snapshot.tiles {
        if tile.terrain_tags.contains(TerrainTags::WATER) {
            continue;
        }
        biome.insert((tile.x, tile.y), format!("{:?}", tile.terrain));
    }

    let mut islands = 0usize;
    let mut island_by_biome: BTreeMap<String, usize> = BTreeMap::new();
    for (&(x, y), me) in &biome {
        let mut land_neighbours = 0usize;
        let mut same = 0usize;
        for (nx, ny) in hex_neighbors(x, y, w, h) {
            if let Some(nb) = biome.get(&(nx, ny)) {
                land_neighbours += 1;
                if nb == me {
                    same += 1;
                }
            }
        }
        if land_neighbours > 0 && same == 0 {
            islands += 1;
            *island_by_biome.entry(me.clone()).or_default() += 1;
        }
    }

    // Connected components of same-biome land.
    let mut seen: HashSet<(u32, u32)> = HashSet::new();
    let mut sizes: Vec<usize> = Vec::new();
    for (&start, me) in &biome {
        if seen.contains(&start) {
            continue;
        }
        let mut n = 0usize;
        let mut q = VecDeque::from([start]);
        seen.insert(start);
        while let Some((cx, cy)) = q.pop_front() {
            n += 1;
            for (nx, ny) in hex_neighbors(cx, cy, w, h) {
                if seen.contains(&(nx, ny)) {
                    continue;
                }
                if biome.get(&(nx, ny)) == Some(me) {
                    seen.insert((nx, ny));
                    q.push_back((nx, ny));
                }
            }
        }
        sizes.push(n);
    }
    sizes.sort_unstable();

    let regions = sizes.len();
    let biomes_present = biome.values().collect::<HashSet<_>>().len();
    let mut rows: Vec<(String, usize)> = island_by_biome.into_iter().collect();
    rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));

    Coherence {
        land: biome.len(),
        islands,
        regions,
        singletons: sizes.iter().filter(|&&n| n == 1).count(),
        tiny: sizes.iter().filter(|&&n| n <= 3).count(),
        median: sizes.get(regions / 2).copied().unwrap_or(0),
        largest: sizes.last().copied().unwrap_or(0),
        biomes_present,
        island_by_biome: rows,
    }
}

fn report(label: &str, w: u32, h: u32, seed: u64) -> Coherence {
    let snapshot = generate(w, h, seed);
    let c = measure(&snapshot, w, h);
    let pct = |n: usize, d: usize| 100.0 * n as f64 / d.max(1) as f64;

    println!("\n=== biome coherence — {label} ({w}x{h}, seed {seed}) ===");
    println!("land tiles           : {}", c.land);
    println!("distinct biomes      : {}", c.biomes_present);
    println!(
        "ISLANDS (0 same-biome nbrs): {}  ({:.1}% of land)   <-- the checkerboard",
        c.islands,
        pct(c.islands, c.land)
    );
    println!("same-biome regions   : {}", c.regions);
    println!(
        "  singletons (size 1) : {}  ({:.1}% of regions)",
        c.singletons,
        pct(c.singletons, c.regions)
    );
    println!(
        "  tiny (size <= 3)    : {}  ({:.1}% of regions)",
        c.tiny,
        pct(c.tiny, c.regions)
    );
    println!("  median region size  : {}", c.median);
    println!("  largest region      : {}", c.largest);
    if !c.island_by_biome.is_empty() {
        println!("  islands by biome    :");
        for (b, n) in c.island_by_biome.iter().take(6) {
            println!("    {b:<24} {n}");
        }
    }
    c
}

/// The report. Read it with `--nocapture`; the assertion is only a regression floor.
#[test]
fn biome_coherence_report() {
    for &seed in &SEEDS {
        for (label, w, h) in [("Tiny", 56u32, 36u32), ("Standard", 80, 52)] {
            let c = report(label, w, h, seed);
            let island_frac = c.islands as f64 / c.land.max(1) as f64;
            assert!(
                island_frac <= MAX_ISLAND_LAND_FRACTION,
                "{label} seed {seed}: {:.1}% of land is single-hex biome confetti, past even the \
                 regression backstop ({:.0}%). Note the backstop is pinned at TODAY'S BROKEN level — \
                 the target is {:.0}% (`docs/plan_biome_coherence.md`), bought by a de-speckle pass, \
                 not by thinning the vocabulary (measured: thinning buys ~2 points and costs whole \
                 biomes).",
                island_frac * 100.0,
                MAX_ISLAND_LAND_FRACTION * 100.0,
                TARGET_ISLAND_LAND_FRACTION * 100.0
            );
        }
    }
}
