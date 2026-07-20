//! **Measurement sweep for the continental-relief terms** — not a guard. Run manually:
//! `cargo test -p core_sim --release --test relief_sweep -- --ignored --nocapture`
//!
//! It decomposes `macro_land`'s three relief terms (warp / tilt / spine) against the bare dome and
//! reports, over six seeds, what each buys and costs: navigable rivers, drainage accumulation,
//! sowable ground, land fraction, landmass count, and the shape of the alpine ranges.
//!
//! # Range WIDTH is `alpine_thickness`, NOT mean component size — do not confuse them again
//!
//! "Mountain ranges are too wide" is a claim about a range's **thickness**, and mean connected
//! component **area** does not measure it: a long thin cordillera and a fat blob can have the same
//! area, and the mean is dominated by whichever single component happened to merge (measured
//! per-seed spread on one config: 15.8 to 424.0 — a statistic that swings 27x between seeds is
//! noise, not a measurement). Reading it as a width is what produced the claim that the tilt
//! "widens ranges 55 -> 102".
//!
//! [`alpine_thickness`] measures the thing directly — every alpine tile's hex distance to the
//! nearest non-alpine tile — and it says the opposite: thickness is **flat at ~2.2-2.4 across the
//! bare dome and every combination of the three terms**. None of these levers controls range width.
//! Whatever does, it is downstream in the mountain mask (`derive_mountain_mask`'s `belt_width_tiles`
//! dilation, `apply_belt_relief`, `terrain_classifier.alpine_relief_threshold`), not in the
//! continental envelope.
//!
//! # The width lever is `alpine_relief_threshold` — measured by [`belt_sweep`]
//!
//! [`belt_sweep`] crosses the three mountain-mask belt levers and finds they all collapse onto one
//! **integer** knob: the alpine distance-from-plate-boundary cutoff `D`, giving a `2D + 1`-tile
//! ribbon. `alpine_relief_threshold` 1.45 → 1.85 takes `D` 3 → 1 (thickness mean 2.43 → 1.57, p95
//! 5.0 → 3.0) while leaving the belt's relief profile untouched, so the peaks stay tall and the
//! belt's shoulders become foothills. `belt_width_tiles` and `relief_belt_gain` reach the same
//! cutoff only by shrinking the foothill skirt or flattening the peaks respectively. See
//! `core_sim/CLAUDE.md` → "Highland biomes are mask-driven".

use std::sync::Arc;

use bevy::app::App;
use bevy::prelude::{UVec2, World};
use bevy::MinimalPlugins;

use core_sim::{
    debug_drainage_census, generate_hydrology, grid_utils::hex_neighbors_wrapped,
    rung_site_refusal, spawn_initial_world, tile_is_fresh_watered, CultureManager,
    DiscoveryProgressLedger, FactionInventory, GenerationRegistry, HydrologyState, LaborConfig,
    LaborConfigHandle, LadderConfig, LadderConfigHandle, MapPresets, MapPresetsHandle, RungKey,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, Tile, TileRegistry,
    BUILTIN_MAP_PRESETS,
};
use sim_runtime::TerrainType;

const TEST_SEED: u64 = 119_304_647;
const SEEDS: [u64; 6] = [1, 2, 3, 4, 5, TEST_SEED];
const ALPINE_GRID: UVec2 = UVec2::new(384, 288);

/// One candidate setting of the three **mountain-mask belt** levers that actually control alpine
/// range WIDTH (the continental-envelope terms measurably do not — see the module docs).
#[derive(Clone, Copy)]
struct Belt {
    label: &'static str,
    belt_width_tiles: u64,
    relief_belt_gain: f64,
    alpine_relief_threshold: f64,
}

/// The **pre-arc** setting, kept as the baseline the shipped one is measured against: a 7-tile
/// alpine slab.
const BELT_PRE_ARC: Belt = Belt {
    label: "pre-arc (w3 g1.2 t1.45)",
    belt_width_tiles: 3,
    relief_belt_gain: 1.2,
    alpine_relief_threshold: 1.45,
};
/// The **shipped** setting. Must reproduce the "SHIPPED (builtin, unpatched)" arm below — if it
/// does not, the patched-preset arms are measuring something the sim does not actually run.
const BELT_SHIPPED: Belt = Belt {
    label: "shipped (w3 g1.2 t1.85)",
    alpine_relief_threshold: 1.85,
    ..BELT_PRE_ARC
};
/// Raise the alpine cut-off so the belt keeps its full width but only its **core** reads Alpine.
const BELT_T160: Belt = Belt {
    label: "w3 g1.2 t1.60",
    alpine_relief_threshold: 1.60,
    ..BELT_PRE_ARC
};
const BELT_T175: Belt = Belt {
    label: "w3 g1.2 t1.75",
    alpine_relief_threshold: 1.75,
    ..BELT_PRE_ARC
};
const BELT_T190: Belt = Belt {
    label: "w3 g1.2 t1.90",
    alpine_relief_threshold: 1.90,
    ..BELT_PRE_ARC
};
/// Narrow the dilation itself — shrinks the whole mountain belt, foothills included.
const BELT_W2: Belt = Belt {
    label: "w2 g1.2 t1.45",
    belt_width_tiles: 2,
    ..BELT_PRE_ARC
};
const BELT_W2_T160: Belt = Belt {
    label: "w2 g1.2 t1.60",
    belt_width_tiles: 2,
    alpine_relief_threshold: 1.60,
    ..BELT_PRE_ARC
};
/// Flatten the relief ramp across the belt instead of moving the cut-off.
const BELT_G090: Belt = Belt {
    label: "w3 g0.90 t1.45",
    relief_belt_gain: 0.90,
    ..BELT_PRE_ARC
};
const BELT_G070: Belt = Belt {
    label: "w3 g0.70 t1.45",
    relief_belt_gain: 0.70,
    ..BELT_PRE_ARC
};

fn presets_with_belt(belt: Belt) -> Arc<MapPresets> {
    let mut file: serde_json::Value =
        serde_json::from_str(BUILTIN_MAP_PRESETS).expect("builtin map presets parse");
    for preset in file["presets"].as_array_mut().expect("presets").iter_mut() {
        let mountains = preset["mountains"].as_object_mut().expect("mountains");
        mountains.insert("belt_width_tiles".into(), belt.belt_width_tiles.into());
        mountains.insert("relief_belt_gain".into(), belt.relief_belt_gain.into());
        let classifier = preset
            .as_object_mut()
            .expect("preset object")
            .entry("terrain_classifier")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        classifier
            .as_object_mut()
            .expect("terrain_classifier object")
            .insert(
                "alpine_relief_threshold".into(),
                belt.alpine_relief_threshold.into(),
            );
    }
    Arc::new(MapPresets::from_json_str(&file.to_string()).expect("patched presets parse"))
}

#[derive(Clone, Copy)]
struct Relief {
    label: &'static str,
    warp: f64,
    tilt: f64,
    spine: f64,
}

const DOME: Relief = Relief {
    label: "dome",
    warp: 0.0,
    tilt: 0.0,
    spine: 0.0,
};
const TILT_ON: Relief = Relief {
    label: "tilt-on",
    warp: 0.18,
    tilt: 2.0,
    spine: 0.35,
};
const TILT_OFF: Relief = Relief {
    label: "tilt-off (SHIPPED)",
    warp: 0.18,
    tilt: 0.0,
    spine: 0.35,
};
const WARP_ONLY: Relief = Relief {
    label: "warp-only",
    warp: 0.18,
    tilt: 0.0,
    spine: 0.0,
};
const SPINE_ONLY: Relief = Relief {
    label: "spine-only",
    warp: 0.0,
    tilt: 0.0,
    spine: 0.35,
};

fn presets_with(relief: Relief) -> Arc<MapPresets> {
    let mut file: serde_json::Value =
        serde_json::from_str(BUILTIN_MAP_PRESETS).expect("builtin map presets parse");
    for preset in file["presets"].as_array_mut().expect("presets").iter_mut() {
        let macro_land = preset["macro_land"].as_object_mut().expect("macro_land");
        macro_land.insert("continental_warp_amplitude".into(), relief.warp.into());
        macro_land.insert("continental_tilt_strength".into(), relief.tilt.into());
        macro_land.insert("continental_spine_amplitude".into(), relief.spine.into());
    }
    Arc::new(MapPresets::from_json_str(&file.to_string()).expect("patched presets parse"))
}

fn world(seed: u64, grid: UVec2, presets: Arc<MapPresets>, hydrology: bool) -> World {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = seed;
    config.grid_size = grid;
    app.world.insert_resource(config);
    app.world.insert_resource(MapPresetsHandle::new(presets));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));
    app.world
        .insert_resource(LaborConfigHandle::new(LaborConfig::builtin()));
    app.world
        .insert_resource(LadderConfigHandle::new(LadderConfig::builtin()));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();
    if hydrology {
        generate_hydrology(&mut app.world);
    }
    app.world
}

fn is_water(t: TerrainType) -> bool {
    matches!(
        t,
        TerrainType::DeepOcean
            | TerrainType::ContinentalShelf
            | TerrainType::CoralShelf
            | TerrainType::HydrothermalVentField
            | TerrainType::InlandSea
            | TerrainType::NavigableRiver
    )
}

fn tile_at(world: &World, x: u32, y: u32) -> &Tile {
    let reg = world.resource::<TileRegistry>();
    world
        .get::<Tile>(reg.index(x, y).expect("tile"))
        .expect("tile component")
}

/// Land fraction + landmass components >= min_area.
fn land_stats(world: &World, min_area: usize) -> (f64, usize) {
    let config = world.resource::<SimulationConfig>();
    let (w, h, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let total = (w * h) as usize;
    let is_land: Vec<bool> = (0..total)
        .map(|i| {
            let (x, y) = ((i as u32) % w, (i as u32) / w);
            let t = tile_at(world, x, y).terrain;
            t == TerrainType::NavigableRiver || !is_water(t)
        })
        .collect();
    let land = is_land.iter().filter(|b| **b).count();
    let mut seen = vec![false; total];
    let mut big = 0usize;
    for start in 0..total {
        if !is_land[start] || seen[start] {
            continue;
        }
        let mut size = 0usize;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(idx) = stack.pop() {
            size += 1;
            let (x, y) = ((idx as u32) % w, (idx as u32) / w);
            for (nx, ny) in hex_neighbors_wrapped(x, y, w, h, wrap) {
                let n = (ny * w + nx) as usize;
                if is_land[n] && !seen[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        if size >= min_area {
            big += 1;
        }
    }
    (land as f64 / total as f64, big)
}

/// AlpineMountain connected components: (count, mean size).
fn alpine_components(world: &World) -> (usize, f64, usize, usize) {
    let config = world.resource::<SimulationConfig>();
    let (w, h, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let total = (w * h) as usize;
    let alpine: Vec<bool> = (0..total)
        .map(|i| {
            let (x, y) = ((i as u32) % w, (i as u32) / w);
            tile_at(world, x, y).terrain == TerrainType::AlpineMountain
        })
        .collect();
    let mut seen = vec![false; total];
    let mut sizes = Vec::new();
    for start in 0..total {
        if !alpine[start] || seen[start] {
            continue;
        }
        let mut size = 0usize;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(idx) = stack.pop() {
            size += 1;
            let (x, y) = ((idx as u32) % w, (idx as u32) / w);
            for (nx, ny) in hex_neighbors_wrapped(x, y, w, h, wrap) {
                let n = (ny * w + nx) as usize;
                if alpine[n] && !seen[n] {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        sizes.push(size);
    }
    let mean = if sizes.is_empty() {
        0.0
    } else {
        sizes.iter().sum::<usize>() as f64 / sizes.len() as f64
    };
    let total_tiles = sizes.iter().sum::<usize>();
    let largest = sizes.iter().copied().max().unwrap_or(0);
    (sizes.len(), mean, total_tiles, largest)
}

/// **Range WIDTH, not area.** For every AlpineMountain tile, its hex distance to the nearest
/// non-alpine tile — the range's local half-thickness. A long thin cordillera and a fat blob can
/// have the same component *area*; only this separates them, and "too wide" is a claim about
/// thickness. Returns (mean, p95) over alpine tiles.
fn alpine_thickness(world: &World) -> (f64, f64) {
    let config = world.resource::<SimulationConfig>();
    let (w, h, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let total = (w * h) as usize;
    let alpine: Vec<bool> = (0..total)
        .map(|i| {
            let (x, y) = ((i as u32) % w, (i as u32) / w);
            tile_at(world, x, y).terrain == TerrainType::AlpineMountain
        })
        .collect();
    // Multi-source BFS seeded from every non-alpine tile; depth = distance into the range.
    let mut depth = vec![u32::MAX; total];
    let mut frontier: Vec<usize> = Vec::new();
    for (i, &is_alpine) in alpine.iter().enumerate() {
        if !is_alpine {
            depth[i] = 0;
            frontier.push(i);
        }
    }
    let mut d = 0u32;
    while !frontier.is_empty() {
        let mut next = Vec::new();
        for idx in frontier {
            let (x, y) = ((idx as u32) % w, (idx as u32) / w);
            for (nx, ny) in hex_neighbors_wrapped(x, y, w, h, wrap) {
                let n = (ny * w + nx) as usize;
                if depth[n] == u32::MAX {
                    depth[n] = d + 1;
                    next.push(n);
                }
            }
        }
        d += 1;
        frontier = next;
    }
    let mut depths: Vec<f64> = alpine
        .iter()
        .enumerate()
        .filter(|(_, &a)| a)
        .map(|(i, _)| f64::from(depth[i]))
        .collect();
    if depths.is_empty() {
        return (0.0, 0.0);
    }
    depths.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let mean = depths.iter().sum::<f64>() / depths.len() as f64;
    let p95 = depths[(depths.len() as f64 * 0.95) as usize % depths.len()];
    (mean, p95)
}

fn navigable_and_accum(world: &World) -> (usize, f64) {
    let hydrology = world.resource::<HydrologyState>();
    let navigable = hydrology
        .rivers
        .iter()
        .filter(|r| !r.navigable_hexes.is_empty())
        .count();
    let census = debug_drainage_census(world);
    let max_accum = census
        .land_accumulation
        .iter()
        .copied()
        .fold(0.0f32, f32::max);
    (navigable, f64::from(max_accum))
}

fn sowable_and_deltas(world: &World) -> (usize, usize, usize) {
    let config = world.resource::<SimulationConfig>();
    let (w, h, wrap) = (
        config.grid_size.x,
        config.grid_size.y,
        config.map_topology.wrap_horizontal,
    );
    let labor = world.resource::<LaborConfigHandle>().get();
    let ladder = world.resource::<LadderConfigHandle>().get();
    let mut sowable = 0usize;
    let mut deltas = 0usize;
    let mut floodplain = 0usize;
    for y in 0..h {
        for x in 0..w {
            let ground = tile_at(world, x, y);
            match ground.terrain {
                TerrainType::RiverDelta => deltas += 1,
                TerrainType::Floodplain => floodplain += 1,
                _ => {}
            }
            let fresh = tile_is_fresh_watered(ground, w, h, wrap, |n| {
                let reg = world.resource::<TileRegistry>();
                reg.index(n.x, n.y)
                    .and_then(|e| world.get::<Tile>(e))
                    .map(|t| t.terrain_tags)
            });
            if rung_site_refusal(
                ladder.rung(RungKey::PlantField),
                ground,
                &labor.forage,
                fresh,
            )
            .is_none()
            {
                sowable += 1;
            }
        }
    }
    (sowable, deltas, floodplain)
}

#[test]
#[ignore]
fn relief_sweep() {
    let shipped = SimulationConfig::builtin().grid_size;
    for relief in [DOME, WARP_ONLY, SPINE_ONLY, TILT_ON, TILT_OFF] {
        let presets = presets_with(relief);
        let min_area = presets
            .get("earthlike")
            .expect("earthlike")
            .macro_land
            .min_area as usize;

        let mut navigable_total = 0usize;
        let mut seeds_with_river = 0usize;
        let mut accums = Vec::new();
        let mut land_fracs = Vec::new();
        let mut continents = Vec::new();

        for seed in SEEDS {
            let w = world(seed, shipped, presets.clone(), true);
            let (nav, accum) = navigable_and_accum(&w);
            navigable_total += nav;
            if nav >= 1 {
                seeds_with_river += 1;
            }
            accums.push(accum);
            let (frac, big) = land_stats(&w, min_area);
            land_fracs.push(frac);
            continents.push(big);
        }

        let pinned = world(TEST_SEED, shipped, presets.clone(), true);
        let (sowable, deltas, floodplain) = sowable_and_deltas(&pinned);

        let mut alpine_counts = Vec::new();
        let mut alpine_means = Vec::new();
        let mut alpine_tiles = Vec::new();
        let mut alpine_max = Vec::new();
        let mut thick_mean = Vec::new();
        let mut thick_p95 = Vec::new();
        for seed in SEEDS {
            let big = world(seed, ALPINE_GRID, presets.clone(), false);
            let (n, mean, tiles, max) = alpine_components(&big);
            let (tmean, tp95) = alpine_thickness(&big);
            thick_mean.push(tmean);
            thick_p95.push(tp95);
            alpine_counts.push(n);
            alpine_means.push(mean);
            alpine_tiles.push(tiles);
            alpine_max.push(max);
        }

        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        println!("=== {} ===", relief.label);
        println!("  navigable segments (6 seeds total): {navigable_total}");
        println!("  seeds with >=1 navigable river:     {seeds_with_river}/6");
        println!(
            "  max drainage accumulation mean:     {:.1}  (per seed {:?})",
            mean(&accums),
            accums.iter().map(|v| v.round()).collect::<Vec<_>>()
        );
        println!("  sowable tiles @ {TEST_SEED}:  {sowable}");
        println!("  RiverDelta / Floodplain:            {deltas} / {floodplain}");
        println!(
            "  land fraction:                      {:.3}  (range {:.3}-{:.3})",
            mean(&land_fracs),
            land_fracs.iter().cloned().fold(f64::MAX, f64::min),
            land_fracs.iter().cloned().fold(0.0, f64::max)
        );
        println!(
            "  landmasses >= min_area:             mean {:.1} {:?}",
            continents.iter().sum::<usize>() as f64 / continents.len() as f64,
            continents
        );
        println!(
            "  alpine components @384x288:         mean {:.1} {:?}",
            alpine_counts.iter().sum::<usize>() as f64 / alpine_counts.len() as f64,
            alpine_counts
        );
        println!(
            "  alpine mean component size:         {:.1}  (per seed {:?})",
            mean(&alpine_means),
            alpine_means
                .iter()
                .map(|v| (v * 10.0).round() / 10.0)
                .collect::<Vec<_>>()
        );
        println!("  alpine largest component:           {:?}", alpine_max);
        println!(
            "  alpine THICKNESS mean:              {:.2}  (per seed {:?})",
            mean(&thick_mean),
            thick_mean
                .iter()
                .map(|v| (v * 100.0).round() / 100.0)
                .collect::<Vec<_>>()
        );
        println!(
            "  alpine THICKNESS p95:               {:.2}  (per seed {:?})",
            mean(&thick_p95),
            thick_p95
        );
        println!(
            "  alpine tiles total:                 mean {:.0} {:?}",
            alpine_tiles.iter().sum::<usize>() as f64 / alpine_tiles.len() as f64,
            alpine_tiles
        );
    }
}

/// Land-tile count, so alpine can be reported as a share of LAND (not of the whole grid, which
/// would swing with the ocean).
fn land_tiles(world: &World) -> usize {
    let config = world.resource::<SimulationConfig>();
    let (w, h) = (config.grid_size.x, config.grid_size.y);
    let mut land = 0usize;
    for y in 0..h {
        for x in 0..w {
            if !is_water(tile_at(world, x, y).terrain) {
                land += 1;
            }
        }
    }
    land
}

/// Report one belt configuration: alpine thickness / share of land / component count at both the
/// statistics grid and the shipped grid, plus the must-not-regress sowable + land-fraction checks.
fn measure_belt(label: &str, presets: Arc<MapPresets>, shipped: UVec2) {
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    println!("=== {label} ===");

    for (grid, tag) in [(ALPINE_GRID, "384x288"), (shipped, "80x52")] {
        let mut thick_mean = Vec::new();
        let mut thick_p95 = Vec::new();
        let mut counts = Vec::new();
        let mut shares = Vec::new();
        for seed in SEEDS {
            let world = world(seed, grid, presets.clone(), false);
            let (tmean, tp95) = alpine_thickness(&world);
            let (n, _, tiles, _) = alpine_components(&world);
            let land = land_tiles(&world).max(1);
            thick_mean.push(tmean);
            thick_p95.push(tp95);
            counts.push(n);
            shares.push(100.0 * tiles as f64 / land as f64);
        }
        let round2 = |v: &[f64]| {
            v.iter()
                .map(|x| (x * 100.0).round() / 100.0)
                .collect::<Vec<_>>()
        };
        println!(
            "  {tag}  thickness mean {:.2} {:?}",
            mean(&thick_mean),
            round2(&thick_mean)
        );
        println!(
            "  {tag}  thickness p95  {:.2} {:?}",
            mean(&thick_p95),
            thick_p95
        );
        println!(
            "  {tag}  alpine % land  {:.2} {:?}",
            mean(&shares),
            round2(&shares)
        );
        println!(
            "  {tag}  components     {:.1} {:?}",
            counts.iter().sum::<usize>() as f64 / counts.len() as f64,
            counts
        );
    }

    let pinned = world(TEST_SEED, shipped, presets.clone(), true);
    let (sowable, _, _) = sowable_and_deltas(&pinned);
    let mut land_fracs = Vec::new();
    for seed in SEEDS {
        let w = world(seed, shipped, presets.clone(), false);
        land_fracs.push(land_stats(&w, 256).0);
    }
    println!("  sowable @ {TEST_SEED}: {sowable}");
    println!("  land fraction mean:          {:.3}", mean(&land_fracs));
}

/// **Measurement sweep for the mountain-mask belt levers** — the ones that actually set alpine
/// range WIDTH. Run manually:
/// `cargo test -p core_sim --release --test relief_sweep -- --ignored belt_sweep --nocapture`
#[test]
#[ignore]
fn belt_sweep() {
    let shipped = SimulationConfig::builtin().grid_size;

    // Measure what the sim ACTUALLY ships first, straight off the builtin presets with no
    // patching, so the patched arms below can be trusted to describe the same pipeline.
    measure_belt(
        "SHIPPED (builtin, unpatched)",
        MapPresets::builtin(),
        shipped,
    );

    for belt in [
        BELT_PRE_ARC,
        BELT_SHIPPED,
        BELT_T160,
        BELT_T175,
        BELT_T190,
        BELT_W2,
        BELT_W2_T160,
        BELT_G090,
        BELT_G070,
    ] {
        measure_belt(belt.label, presets_with_belt(belt), shipped);
    }
}
