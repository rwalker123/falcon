// Guards the earthlike continental-shelf MODEL (core_sim/CLAUDE.md, World Generation
// → "Continental shelf width"): a continuous ≥1-tile ContinentalShelf ring forms off
// GENTLE (passive-margin) coasts, while STEEP mountain/cliff coasts show deep water
// right at the edge. Four properties are asserted:
//   (a) a sanity band on the shelf fraction of ocean — the shelf is a real, non-trivial
//       slice of the ocean but never covers all of it. It is NO LONGER the old tight,
//       size-invariant 5-8% band: with a 1-tile floor the fraction now varies with
//       coastline steepness and *shrinks* as the open ocean grows on larger maps.
//   (b) the FINAL-map invariant (the point of `reconcile_coastal_shelf`): NO DeepOcean tile
//       is odd-r hex-adjacent to a gentle (rise < threshold) land tile — closing the residual
//       that hydrology deltas/marshes + the tag solver's polar tundra create AFTER the shelf is
//       classified. Steep coasts are still checked to keep DeepOcean at their edge.
//   (c) delta/marsh coasts have shelf (not deep water) seaward — a targeted case of (b).
//   (d) the model itself — coast land abutting shelf tiles rises LESS than coast land
//       abutting deep-water-at-the-edge tiles (the coast-height gate).
// Deterministic: fixed seeds. Hex adjacency uses the sim's own `wrap_horizontal` flag so the
// checks match the map the sim actually generated.
mod common;

use bevy::math::UVec2;
use core_sim::grid_utils::hex_neighbors_wrapped;
use core_sim::{build_headless_app, SimulationConfig, SimulationConfigMetadata, SnapshotHistory};
use sim_runtime::{TerrainType, WorldSnapshot};

/// Earthlike coast-height gate (normalized rise above sea level). Matches
/// `map_presets.json` earthlike `shelf.coast_height_threshold` / the `ShelfConfig` default —
/// the same gate `classify_bands` + `reconcile_coastal_shelf` use to split gentle (→ shelf)
/// from steep (→ deep water) coasts. Kept in sync with core_sim; see core_sim/CLAUDE.md
/// "Continental shelf width".
const EARTHLIKE_COAST_HEIGHT_THRESHOLD: f64 = 0.10;

/// Per-map sanity band for the shelf fraction of ocean. Non-trivial (the shelf is a real
/// slice) but well short of covering all ocean. **Re-measured after the border-ring bathymetry
/// fix** (`classify_terrain`'s legacy map-frame edge rings no longer drown ~250 band-`Land` tiles
/// per 80x52 map, so the coastline is real and the orphaned offshore shelf those drowned tiles
/// used to strand is gone): across 80x52..256x192 earthlike maps the player-facing fraction (slope
/// folds into deep water) now runs ~14%..33% (was ~17%..35% with the bug) — it still includes the
/// gentle coasts the final `reconcile_coastal_shelf` pass stamps after `classify_bands` (deltas,
/// marshes, polar tundra), and still shrinks as the open ocean grows, so this band leaves
/// comfortable margin on both ends (min sample 14.0% vs. a 6% floor). The
/// band-level hex-diagonal-gap fix is guarded by
/// `mapgen::tests::earthlike_bands_have_no_gentle_coast_shelf_gap`, and the strict FINAL-map
/// invariant by `earthlike_no_deep_ocean_touches_gentle_land_on_final_map` below (the snapshot is
/// post-hydrology/-tag-solver/-reconcile: the earlier stages stamp deltas/marsh/polar land against
/// ocean independently of the shelf ring, and the final reconcile pass then stamps the shelf that
/// closes those gaps — which is exactly why the strict snapshot-level zero-gap invariant now holds).
const SAMPLE_MIN: f64 = 0.06;
const SAMPLE_MAX: f64 = 0.50;

/// Generates an earthlike map and returns its snapshot plus the effective
/// `map_topology.wrap_horizontal` flag the sim ran with, so hex-adjacency checks in the tests
/// match the sim exactly (the test fixture config may leave the map non-wrapping).
fn generate(width: u32, height: u32, seed: u64) -> (WorldSnapshot, bool) {
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
    let wrap_horizontal = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot
        .as_ref()
        .map(|s| (**s).clone())
        .expect("snapshot after worldgen");
    (snapshot, wrap_horizontal)
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

/// Is this terrain a land tile (anything not open ocean / inland water)?
fn is_land(t: TerrainType) -> bool {
    !matches!(
        t,
        TerrainType::ContinentalShelf
            | TerrainType::DeepOcean
            | TerrainType::InlandSea
            // A navigable river is WATER-tagged (it mirrors InlandSea), so it is not land and must
            // never attract a continental shelf.
            | TerrainType::NavigableRiver
    )
}

/// Mean elevation-overlay sample of the coast land directly adjacent (4-neighbour,
/// wrap-aware on x) to shelf tiles vs. deep-water-at-the-edge tiles. Uses each coastal
/// ocean tile's MIN adjacent-land sample (mirroring the gate's min-rise logic). The
/// elevation-overlay samples are monotonic in real elevation, so comparing raw samples
/// is sufficient to prove shelves sit off lower/gentler coasts. `wrap_horizontal` is the
/// flag `generate()` returned, so x-neighbours wrap exactly when the sim's map does (and
/// are otherwise skipped at the edges), matching the sibling assertion helpers.
fn coast_land_elev_by_shelf(
    snap: &WorldSnapshot,
    wrap_horizontal: bool,
) -> (Option<f64>, Option<f64>) {
    let w = snap.terrain.width as usize;
    let h = snap.terrain.height as usize;
    let terr = &snap.terrain.samples;
    let elev = &snap.elevation_overlay.samples;
    assert_eq!(terr.len(), w * h, "terrain sample count matches dimensions");
    assert_eq!(
        elev.len(),
        w * h,
        "elevation sample count matches dimensions"
    );
    let idx = |x: usize, y: usize| y * w + x;

    let mut shelf_sum = 0.0f64;
    let mut shelf_n = 0usize;
    let mut deep_sum = 0.0f64;
    let mut deep_n = 0usize;

    for y in 0..h {
        for x in 0..w {
            let t = terr[idx(x, y)].terrain;
            let is_shelf = matches!(t, TerrainType::ContinentalShelf);
            let is_deep = matches!(t, TerrainType::DeepOcean);
            if !(is_shelf || is_deep) {
                continue;
            }
            // MIN adjacent-land elevation sample: x wraps only when the sim's map does
            // (else out-of-range x-neighbours are skipped), y always clamps at the poles.
            let mut min_land: Option<u16> = None;
            let west = if x > 0 {
                Some(x - 1)
            } else if wrap_horizontal {
                Some(w - 1)
            } else {
                None
            };
            let east = if x + 1 < w {
                Some(x + 1)
            } else if wrap_horizontal {
                Some(0)
            } else {
                None
            };
            let neighbours = [
                west.map(|nx| (nx, y)),
                east.map(|nx| (nx, y)),
                Some((x, y.wrapping_sub(1))),
                Some((x, y + 1)),
            ];
            for (nx, ny) in neighbours.into_iter().flatten() {
                if ny >= h {
                    continue;
                }
                if is_land(terr[idx(nx, ny)].terrain) {
                    let e = elev[idx(nx, ny)];
                    min_land = Some(min_land.map_or(e, |m| m.min(e)));
                }
            }
            let Some(land_e) = min_land else { continue };
            if is_shelf {
                shelf_sum += land_e as f64;
                shelf_n += 1;
            } else {
                deep_sum += land_e as f64;
                deep_n += 1;
            }
        }
    }

    let shelf_mean = (shelf_n > 0).then(|| shelf_sum / shelf_n as f64);
    let deep_mean = (deep_n > 0).then(|| deep_sum / deep_n as f64);
    (shelf_mean, deep_mean)
}

#[test]
fn earthlike_shelf_fraction_is_a_non_trivial_slice_across_sizes() {
    // Sizes span the small "Standard" play size up to the preset's native 256x192.
    // Native size uses a single seed to bound CI time (slowest to generate).
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

    let mut out_of_band: Vec<String> = Vec::new();
    println!("\n=== earthlike shelf % of ocean ===");
    for (w, h, seed) in samples {
        let (snap, _wrap) = generate(w, h, seed);
        let (frac, shelf, ocean) = shelf_fraction_of_ocean(&snap);
        println!(
            "{:>4}x{:<4} seed={:016x}: shelf={:>6} ocean={:>7} -> {:>5.1}% of ocean",
            w,
            h,
            seed,
            shelf,
            ocean,
            frac * 100.0
        );
        if !(SAMPLE_MIN..=SAMPLE_MAX).contains(&frac) {
            out_of_band.push(format!(
                "{w}x{h} seed={seed:016x}: {:.1}% outside sanity band [{:.0}%, {:.0}%]",
                frac * 100.0,
                SAMPLE_MIN * 100.0,
                SAMPLE_MAX * 100.0
            ));
        }
    }

    assert!(
        out_of_band.is_empty(),
        "some maps left the shelf-fraction sanity band:\n{}",
        out_of_band.join("\n")
    );
}

/// A WATER-tagged terrain (mirrors the sim's `TerrainTags::WATER` set — the 5 ocean/inland
/// water biomes). The `reconcile_coastal_shelf` pass treats everything else as land, so the
/// invariant test uses the same partition (deltas/marshes/tundra all count as land).
fn is_water_terrain(t: TerrainType) -> bool {
    matches!(
        t,
        TerrainType::DeepOcean
            | TerrainType::ContinentalShelf
            | TerrainType::InlandSea
            | TerrainType::CoralShelf
            | TerrainType::HydrothermalVentField
            | TerrainType::NavigableRiver
    )
}

/// Normalized rise above sea level reconstructed from the elevation overlay. The overlay stores
/// `sample_u16 = round((value − min)/(max − min) · 65535)` and `sea_level = (field.sea_level −
/// min)/(max − min)` (both on the `[min, max]` scale), so
/// `rise = (sample/65535 − sea_level) · (max − min)` recovers `field.sample − field.sea_level` —
/// the exact quantity the sim's coast-height gate compares against `coast_height_threshold`.
fn overlay_rise(snap: &WorldSnapshot, x: usize, y: usize, w: usize) -> f64 {
    let overlay = &snap.elevation_overlay;
    let decoded = overlay.samples[y * w + x] as f64 / 65535.0;
    (decoded - overlay.sea_level as f64) * (overlay.max_value - overlay.min_value) as f64
}

#[test]
fn earthlike_no_deep_ocean_touches_gentle_land_on_final_map() {
    // The authoritative final-map invariant: after the FULL pipeline (hydrology deltas/marshes +
    // tag-solver polar tundra + palette clamp + `reconcile_coastal_shelf`), NO DeepOcean tile is
    // odd-r hex-adjacent to a GENTLE (rise < threshold) land tile. Uses `grid_utils` hex adjacency
    // — the player's view. This is the residual `classify_bands` can't close (those later stages
    // repaint coasts after the shelf is decided); the post-pass makes it hold on the live map.
    // Also proves STEEP coasts are UNAFFECTED: at least one DeepOcean tile stays hex-adjacent to a
    // steep (rise >= threshold) land tile — cliffs keep deep water at the edge (we didn't just
    // convert all coastal ocean to shelf).
    let samples: [(u32, u32, u64); 5] = [
        (80, 52, 0x0FA1_C0DE),
        (80, 52, 0x5EED_F00D),
        (80, 52, 0x0000_BEEF),
        (128, 96, 0x0FA1_C0DE),
        (192, 128, 0x5EED_F00D),
    ];

    let mut violations: Vec<String> = Vec::new();
    let mut steep_coast_deep_total = 0usize;
    println!("\n=== final-map DeepOcean-vs-gentle-land hex adjacencies (must be 0) ===");
    for (w, h, seed) in samples {
        let (snap, wrap) = generate(w, h, seed);
        let tw = snap.terrain.width as usize;
        let th = snap.terrain.height as usize;
        let terr = &snap.terrain.samples;
        assert_eq!(
            terr.len(),
            tw * th,
            "terrain sample count matches dimensions"
        );

        let mut gentle_gaps = 0usize;
        let mut steep_deep = 0usize;
        for y in 0..th {
            for x in 0..tw {
                if terr[y * tw + x].terrain != TerrainType::DeepOcean {
                    continue;
                }
                for (nx, ny) in
                    hex_neighbors_wrapped(x as u32, y as u32, tw as u32, th as u32, wrap)
                {
                    let nt = terr[ny as usize * tw + nx as usize].terrain;
                    if is_water_terrain(nt) {
                        continue;
                    }
                    let rise = overlay_rise(&snap, nx as usize, ny as usize, tw);
                    if rise < EARTHLIKE_COAST_HEIGHT_THRESHOLD {
                        gentle_gaps += 1;
                    } else {
                        steep_deep += 1;
                    }
                }
            }
        }
        steep_coast_deep_total += steep_deep;
        println!(
            "{:>4}x{:<4} seed={:016x}: gentle-gaps={:>3}  steep-coast-deep={:>4}",
            w, h, seed, gentle_gaps, steep_deep
        );
        if gentle_gaps > 0 {
            violations.push(format!(
                "{w}x{h} seed={seed:016x}: {gentle_gaps} DeepOcean tiles hex-adjacent to gentle land"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "reconcile_coastal_shelf left gentle-coast-vs-DeepOcean gaps on the final map:\n{}",
        violations.join("\n")
    );
    assert!(
        steep_coast_deep_total > 0,
        "expected some steep (rise >= threshold) coast to keep DeepOcean at its edge — the pass \
         should not convert every coastal ocean tile to shelf"
    );
}

#[test]
fn earthlike_delta_and_marsh_coasts_have_shelf_not_deep_water() {
    // Deltas/marshes are gentle land, so the invariant guarantees their seaward ocean is shelf,
    // never DeepOcean. Confirm directly: no RiverDelta/FreshwaterMarsh/Floodplain tile is
    // hex-adjacent to DeepOcean on the final map (any ocean it touches is ContinentalShelf).
    let samples: [(u32, u32, u64); 3] = [
        (80, 52, 0x0FA1_C0DE),
        (128, 96, 0x0FA1_C0DE),
        (192, 128, 0x5EED_F00D),
    ];

    let mut violations: Vec<String> = Vec::new();
    for (w, h, seed) in samples {
        let (snap, wrap) = generate(w, h, seed);
        let tw = snap.terrain.width as usize;
        let th = snap.terrain.height as usize;
        let terr = &snap.terrain.samples;
        for y in 0..th {
            for x in 0..tw {
                let t = terr[y * tw + x].terrain;
                if !matches!(
                    t,
                    TerrainType::RiverDelta
                        | TerrainType::FreshwaterMarsh
                        | TerrainType::Floodplain
                ) {
                    continue;
                }
                for (nx, ny) in
                    hex_neighbors_wrapped(x as u32, y as u32, tw as u32, th as u32, wrap)
                {
                    if terr[ny as usize * tw + nx as usize].terrain == TerrainType::DeepOcean {
                        violations.push(format!(
                            "{w}x{h} seed={seed:016x}: {t:?} at ({x},{y}) touches DeepOcean"
                        ));
                    }
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "delta/marsh coasts should have a shelf seaward, not deep water:\n{}",
        violations.join("\n")
    );
}

#[test]
fn shelf_forms_off_gentler_coasts_than_deep_water() {
    // The model: shelves sit off GENTLE (low, passive-margin) coasts; deep water sits
    // against STEEP (high, active-margin) coasts. So coast land next to shelf tiles must
    // be, on average, LOWER than coast land next to deep-water-at-the-edge tiles.
    // Aggregate across a few fixed maps to be robust to per-map coastline variance.
    let samples: [(u32, u32, u64); 4] = [
        (80, 52, 0x0FA1_C0DE),
        (80, 52, 0x5EED_F00D),
        (128, 96, 0x0FA1_C0DE),
        (192, 128, 0x0FA1_C0DE),
    ];

    println!("\n=== coast-land elevation: shelf vs deep-water-at-edge ===");
    for (w, h, seed) in samples {
        let (snap, wrap) = generate(w, h, seed);
        let (shelf_mean, deep_mean) = coast_land_elev_by_shelf(&snap, wrap);
        let shelf_mean = shelf_mean.expect("map has shelf tiles with adjacent coast land");
        let deep_mean =
            deep_mean.expect("map has deep-water-at-edge tiles with adjacent coast land");
        println!(
            "{:>4}x{:<4} seed={:016x}: shelf-coast={:>8.1}  deep-coast={:>8.1}",
            w, h, seed, shelf_mean, deep_mean
        );
        assert!(
            shelf_mean < deep_mean,
            "{w}x{h} seed={seed:016x}: coast land next to shelf ({shelf_mean:.1}) should be \
             lower than coast land next to deep water ({deep_mean:.1}) — the coast-height gate"
        );
    }
}
