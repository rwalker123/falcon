use std::{cmp::Ordering, collections::VecDeque};

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};

use crate::{
    grid_utils::{hex_neighbors_wrapped, neighbors4_wrapped},
    heightfield::ElevationField,
    map_preset::{
        BiomeTransitionConfig, InlandSeaConfig, IslandConfig, MacroLandConfig, OceanConfig,
        ShelfConfig,
    },
};

#[derive(Resource, Debug, Clone, Copy)]
pub struct WorldGenSeed(pub u64);

#[derive(Debug, Clone)]
pub struct BandsResult {
    pub terrain: Vec<TerrainBand>,
    pub ocean_distance: Vec<u32>,
    #[allow(dead_code)]
    pub land_mask: Vec<bool>,
    #[allow(dead_code)]
    pub land_distance: Vec<u32>,
    #[allow(dead_code)]
    pub coastal_land: Vec<bool>,
    pub moisture: Vec<f32>,
    pub mountains: MountainMask,
    pub elevation: ElevationField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainBand {
    Land,
    ContinentalShelf,
    ContinentalSlope,
    DeepOcean,
    InlandSea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountainType {
    Fold,
    Fault,
    Volcanic,
    Dome,
}

impl MountainType {
    fn priority(self) -> u8 {
        match self {
            MountainType::Fold => 4,
            MountainType::Volcanic => 3,
            MountainType::Fault => 2,
            MountainType::Dome => 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MountainCell {
    pub ty: MountainType,
    pub strength: u8,
}

const MIN_RELIEF_SCALE: f32 = 0.35;
const MAX_RELIEF_SCALE: f32 = 2.5;
// --- Belt-strength normalization. FAULT/VOLCANIC spans convert a cell's u8 strength
// into a 0..1 belt ratio; they are used in BOTH the relief pass and restamp_elevation's
// elevation pass and must stay identical, hence shared consts. ---
const FAULT_STRENGTH_SPAN: f32 = 8.0;
const VOLCANIC_STRENGTH_SPAN: f32 = 12.0;
/// Ceiling on a mountain cell's u8 strength (volcanic/plateau paths).
const MAX_CELL_STRENGTH: f32 = 12.0;
/// Fold-belt width grows with mountain_scale: `width * (1 + scale.clamp(0,MAX)*GAIN)`.
const BELT_WIDTH_SCALE_GAIN: f32 = 0.5;
const BELT_WIDTH_SCALE_MAX: f32 = 2.0;
/// Shoreline elevation-blur strength indexed by distance-from-coast (`land_distance`).
const COASTAL_BLUR_WEIGHTS: [f32; 4] = [0.6, 0.45, 0.3, 0.15];
/// Polar microplate formation: minimum polar-cell mass, and cells-per-microplate divisor.
const MIN_POLAR_CELLS: usize = 32;
const POLAR_MICROPLATE_DIVISOR: usize = 12;
/// Polar microplate drift = radial*RADIAL + random*RANDOM + poleward_bias*BIAS.
const POLAR_FLOW_RADIAL: f32 = 0.7;
const POLAR_FLOW_RANDOM: f32 = 0.3;
const POLAR_FLOW_BIAS: f32 = 0.6;
/// ±45° random rotation applied to each plate's radial drift vector.
const PLATE_DRIFT_JITTER: f32 = std::f32::consts::FRAC_PI_2;
// Fault-seam geometry/strength (per-map abundance/length are preset config).
const MAX_FAULT_LINES: u32 = 6;
const MIN_FAULT_PLATE_AREA: usize = 12;
const FAULT_INTERIOR_START_DIST: u32 = 3;
const FAULT_SEAM_STRENGTH: u8 = 6;
const FAULT_BRANCH_SKIP_CHANCE: f32 = 0.55;
const FAULT_FLANK_STRENGTH_DROP: u8 = 2;
// Volcanic chain shape (stochastic texture; per-map volcanic-ness is preset config).
const VOLCANIC_CHAIN_LEN_CAP: u32 = 12;
const VOLCANIC_STRENGTH_SCALE: f32 = 7.0;
const VOLCANIC_CHANCE_CEILING: f32 = 0.8;
const VOLCANIC_FLANK_SPAWN_CHANCE: f32 = 0.5;
const VOLCANIC_FLANK_SIDE_SKIP: f32 = 0.35;
const VOLCANIC_FLANK_DROP_FACTOR: f32 = 0.6;
const VOLCANIC_DIR_CHANGE_CHANCE: f32 = 0.4;
const VOLCANIC_GAP_SKIP_CHANCE: f32 = 0.2;
const VOLCANIC_HUMIDITY_SUPPRESSION: f32 = 0.25;
// Plateau microrelief response curves on `plateau_microrelief_strength`.
const PLATEAU_VARIANCE_FACTOR: f32 = 0.5;
const PLATEAU_INTERIOR_FACTOR: f32 = 0.4;
const PLATEAU_RIM_BOOST_FACTOR: f32 = 4.0;
/// Baseline seed strengths for dome plateaus and polar shear faults.
const DOME_CELL_STRENGTH: u8 = 4;
const POLAR_FAULT_STRENGTH: u8 = 4;
// Land-ratio rebalance coastline-scoring weights.

#[derive(Debug, Clone)]
pub struct MountainMask {
    cells: Vec<Option<MountainCell>>,
    relief_scale: Vec<f32>,
    fold_band_width: u32,
}

impl MountainMask {
    pub(crate) fn new(width: usize, height: usize, fold_band_width: u32) -> Self {
        Self {
            cells: vec![None; width * height],
            relief_scale: vec![1.0; width * height],
            fold_band_width,
        }
    }

    pub fn get(&self, idx: usize) -> Option<MountainCell> {
        self.cells.get(idx).copied().flatten()
    }

    pub fn relief_scale(&self, idx: usize) -> f32 {
        self.relief_scale.get(idx).copied().unwrap_or(1.0)
    }

    fn set(&mut self, idx: usize, cell: MountainCell) {
        match self.cells[idx] {
            Some(existing)
                if existing.ty.priority() > cell.ty.priority()
                    || (existing.ty.priority() == cell.ty.priority()
                        && existing.strength >= cell.strength) => {}
            _ => self.cells[idx] = Some(cell),
        }
    }

    fn enforce_relief_floor(&mut self, idx: usize, floor: f32) -> bool {
        let floor = floor.clamp(MIN_RELIEF_SCALE, MAX_RELIEF_SCALE);
        if let Some(scale) = self.relief_scale.get_mut(idx) {
            if *scale + f32::EPSILON < floor {
                *scale = floor;
                return true;
            }
        }
        false
    }

    fn enforce_relief_cap(&mut self, idx: usize, cap: f32) -> bool {
        let cap = cap.clamp(MIN_RELIEF_SCALE, MAX_RELIEF_SCALE);
        if let Some(scale) = self.relief_scale.get_mut(idx) {
            if *scale - f32::EPSILON > cap {
                *scale = cap;
                return true;
            }
        }
        false
    }

    fn set_relief_scale(&mut self, idx: usize, value: f32) {
        if let Some(scale) = self.relief_scale.get_mut(idx) {
            *scale = value.clamp(MIN_RELIEF_SCALE, MAX_RELIEF_SCALE);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_for_tests(&mut self, idx: usize, cell: MountainCell, relief: f32) {
        self.set(idx, cell);
        if let Some(scale) = self.relief_scale.get_mut(idx) {
            *scale = relief;
        }
    }

    fn iter_counts(&self) -> (usize, usize, usize, usize) {
        let mut fold = 0usize;
        let mut fault = 0usize;
        let mut volcanic = 0usize;
        let mut dome = 0usize;
        for c in self.cells.iter().flatten() {
            match c.ty {
                MountainType::Fold => fold += 1,
                MountainType::Fault => fault += 1,
                MountainType::Volcanic => volcanic += 1,
                MountainType::Dome => dome += 1,
            }
        }
        (fold, fault, volcanic, dome)
    }

    pub fn fold_band_width(&self) -> u32 {
        self.fold_band_width.max(1)
    }
}

type PolarLogEntry = (usize, usize, usize, bool, bool, usize, usize, usize);

#[derive(Debug)]
struct LandMask {
    mask: Vec<bool>,
    land_count: usize,
}

#[allow(clippy::too_many_arguments)]
pub fn build_bands(
    elevation: &ElevationField,
    sea_level: f32,
    macro_cfg: &MacroLandConfig,
    shelf: &ShelfConfig,
    islands: &IslandConfig,
    inland: &InlandSeaConfig,
    ocean_cfg: &OceanConfig,
    moisture_scale: f32,
    biome_cfg: &BiomeTransitionConfig,
    seed: u64,
    mountain_scale: f32,
    mountain_cfg: &crate::map_preset::MountainsConfig,
    wrap_horizontal: bool,
) -> BandsResult {
    let w = elevation.width as usize;
    let h = elevation.height as usize;
    // A private working copy of the field. Every coastline edit below writes HERE and the mask is
    // re-derived — no stage writes `land`/`is_ocean` directly. It is this edited field that flows
    // into `restamp_elevation` and out through `BandsResult.elevation`, so the published bathymetry
    // and the published terrain are the same surface.
    let mut field = elevation.clone();
    let LandMask { mask, land_count } = generate_land_mask(&field, sea_level);
    let mut land = mask;
    let initial_land_ratio = land_count as f32 / (w * h) as f32;
    tracing::debug!(
        target: "shadow_scale::mapgen",
        initial_land_ratio,
        target_land_pct = macro_cfg.target_land_pct,
        continents = macro_cfg.continents,
        min_area = macro_cfg.min_area,
        wrap_horizontal,
        "mapgen.macro_land.initial_ratio"
    );

    let mut is_ocean = compute_ocean_mask_wrapped(&land, w, h, wrap_horizontal);

    // Optionally connect inland seas to ocean via simple strait rule
    if inland.merge_strait_width > 0 {
        connect_inland_seas_via_straits(&mut field, &land, &is_ocean, inland, sea_level, w, h);
        land = generate_land_mask(&field, sea_level).mask;
        is_ocean = compute_ocean_mask_wrapped(&land, w, h, wrap_horizontal);
    }

    // Place islands before classifying so shelves wrap correctly.
    place_islands(&mut field, &is_ocean, islands, shelf, sea_level, w, h, seed);
    land = generate_land_mask(&field, sea_level).mask;
    is_ocean = compute_ocean_mask_wrapped(&land, w, h, wrap_horizontal);

    // `rebalance_land_ratio`/`adjust_land_tiles` are GONE. The contour anchor already holds land% to
    // within a few tenths of `target_land_pct` by construction, so they had nothing to correct — and
    // they corrected it by repainting the mask, which is the defect this arc removes.

    let land_distance = compute_land_distance_wrapped(&land, w, h, wrap_horizontal);
    let coastal_land = compute_coastal_land(&land, &is_ocean, w, h);
    let mountains = derive_mountain_mask(
        &land,
        &is_ocean,
        &land_distance,
        &field,
        mountain_cfg,
        mountain_scale,
        w,
        h,
        seed,
    );

    let elevation = restamp_elevation(
        &land,
        &is_ocean,
        &land_distance,
        &mountains,
        &field,
        mountain_cfg,
        ocean_cfg,
        sea_level,
        seed,
    );

    let moisture = compute_moisture_field(
        &land,
        &coastal_land,
        &land_distance,
        &mountains,
        &elevation,
        w,
        h,
        moisture_scale,
        biome_cfg,
        seed,
    );

    // Distance transform and classification
    let ocean_distance = compute_ocean_distance_wrapped(&land, w, h, wrap_horizontal);
    let terrain = classify_bands(
        &land,
        &is_ocean,
        &ocean_distance,
        shelf,
        &elevation,
        sea_level,
        w,
        h,
        wrap_horizontal,
        seed,
    );

    BandsResult {
        terrain,
        ocean_distance,
        land_mask: land,
        land_distance,
        coastal_land,
        moisture,
        mountains,
        elevation,
    }
}

/// Get 4-connected (cardinal) neighbors without wrapping.
/// Returns neighbors in a specific order: W, E, N, S (for deterministic BFS).
fn neighbors4(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut v = Vec::with_capacity(4);
    if x > 0 {
        v.push((x - 1, y)); // W
    }
    if x + 1 < w {
        v.push((x + 1, y)); // E
    }
    if y > 0 {
        v.push((x, y - 1)); // N
    }
    if y + 1 < h {
        v.push((x, y + 1)); // S
    }
    v.into_iter()
}

/// Get 4-connected neighbors with horizontal wrap support.
/// Uses grid_utils implementation for consistent wrap behavior.
fn neighbors4_with_wrap(
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    wrap_horizontal: bool,
) -> impl Iterator<Item = (usize, usize)> {
    if wrap_horizontal {
        // Use the grid_utils implementation for wrap-aware neighbors
        neighbors4_wrapped(x as u32, y as u32, w as u32, h as u32, true)
            .map(|(nx, ny)| (nx as usize, ny as usize))
            .collect::<Vec<_>>()
            .into_iter()
    } else {
        // Non-wrapping case: use original logic for deterministic output
        neighbors4(x, y, w, h).collect::<Vec<_>>().into_iter()
    }
}

fn neighbor_dirs() -> [(i32, i32); 8] {
    [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ]
}

/// Connect landlocked water bodies to the open ocean by **lowering a corridor of land below sea
/// level**, then re-deriving the mask. It used to flip `land`/`is_ocean` along the corridor, which
/// left the strait's tiles sitting above sea level in the published field — a channel you could see
/// on the map and not in the bathymetry.
fn connect_inland_seas_via_straits(
    field: &mut ElevationField,
    land: &[bool],
    is_ocean: &[bool],
    inland: &InlandSeaConfig,
    sea_level: f32,
    w: usize,
    h: usize,
) {
    let max_width = inland.merge_strait_width as usize;
    // The corridor floor: unambiguously below sea level, so the re-derived mask reads it as water.
    let strait_floor = sea_level - inland.strait_depth_margin.max(0.0);
    // For each inland water tile near ocean within max_width, carve shortest corridor through land.
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    // Collect inland edge tiles
    let mut inland_edges: Vec<(usize, usize)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if land[i] || is_ocean[i] {
                continue;
            }
            // water and not ocean -> inland; check if adjacent to land
            for (nx, ny) in neighbors4(x, y, w, h) {
                if land[idx(nx, ny)] {
                    inland_edges.push((x, y));
                    break;
                }
            }
        }
    }
    // Simple BFS from each inland edge to nearest ocean tile through land only, bounded by max_width
    for &(sx, sy) in &inland_edges {
        let mut q = VecDeque::new();
        let mut dist = vec![u16::MAX; w * h];
        q.push_back((sx, sy));
        dist[idx(sx, sy)] = 0;
        let mut found: Option<(usize, usize)> = None;
        while let Some((x, y)) = q.pop_front() {
            let d = dist[idx(x, y)] as usize;
            if d > max_width {
                continue;
            }
            for (nx, ny) in neighbors4(x, y, w, h) {
                let ni = idx(nx, ny);
                // We allow crossing land to reach ocean
                if is_ocean[ni] {
                    found = Some((nx, ny));
                    break;
                }
                if land[ni] && dist[ni] == u16::MAX {
                    dist[ni] = (d + 1) as u16;
                    q.push_back((nx, ny));
                }
            }
            if found.is_some() {
                break;
            }
        }
        if let Some((tx, ty)) = found {
            // Carve corridor along greedy backtrack by choosing neighbor with decreasing dist
            let mut cx = tx;
            let mut cy = ty;
            loop {
                let i = idx(cx, cy);
                if dist[i] == 0 {
                    break;
                }
                // The one edit: sink the tile. `min` so an already-deeper cell is left alone.
                let values = field.values_mut();
                values[i] = values[i].min(strait_floor).clamp(0.0, 1.0);
                // pick next with minimal distance
                let mut best: Option<(usize, usize, u16)> = None;
                for (nx, ny) in neighbors4(cx, cy, w, h) {
                    let ni = idx(nx, ny);
                    let dv = dist[ni];
                    if dv < u16::MAX && best.map(|b| dv < b.2).unwrap_or(true) {
                        best = Some((nx, ny, dv));
                    }
                }
                if let Some((nx, ny, _)) = best {
                    cx = nx;
                    cy = ny;
                } else {
                    break;
                }
            }
        }
    }
}

/// Island size levers. The old boolean `carve_blob_into` painted a disc of `land` at these radii;
/// they now size a radial elevation blob instead. Kept as named constants rather than moved into
/// `IslandConfig` so island *placement* stays exactly as config-driven as it was — only the
/// mechanism changed.
const ISLAND_CONTINENTAL_RADIUS_MIN: usize = 1;
/// How many discrete radii a continental fragment may take (`MIN`, `MIN + 1`).
const ISLAND_CONTINENTAL_RADIUS_VARIANTS: u32 = 2;
/// Oceanic islands are lone specks — one radius, no variance.
const ISLAND_OCEANIC_RADIUS: usize = 1;
/// The blob's rim height as a fraction of its peak margin. Strictly positive so the whole disc
/// clears sea level (matching the old paint-the-disc behavior) while still reading as a dome
/// rather than a mesa.
const ISLAND_RIM_HEIGHT_FRACTION: f32 = 0.25;
/// The window `place_islands` scans when measuring a candidate tile's distance to the nearest land.
const ISLAND_LAND_SCAN_RADIUS: isize = 8;
/// Placement attempts per island, as a multiple of the target count, plus a floor so a sparse map
/// still gets a fair number of tries.
const ISLAND_CONTINENTAL_ATTEMPT_FACTOR: u32 = 10;
const ISLAND_CONTINENTAL_ATTEMPT_FLOOR: u32 = 100;
const ISLAND_OCEANIC_ATTEMPT_FACTOR: u32 = 20;
const ISLAND_OCEANIC_ATTEMPT_FLOOR: u32 = 200;

/// One island dome's geometry and height, bundled so `raise_island_blob` keeps a readable signature.
struct IslandDome {
    center: (usize, usize),
    radius: usize,
    /// Sea level on the field's normalized scale — the dome's rim is measured from it.
    sea_level: f32,
    /// The dome's centre height. `peak - sea_level` is `IslandConfig::island_peak_margin`.
    peak: f32,
}

/// Seed islands by **raising seamounts above sea level**, then re-deriving the mask. Placement
/// (densities, fringe band, min distance from a continent) is unchanged and still config-driven;
/// only the mechanism moved from painting `land` to editing the field the mask is derived from.
///
/// Land is read **live off `field`** (`elevation > sea_level`) rather than from a pre-call mask, so
/// each iteration sees the islands earlier iterations raised — that is what keeps two fragments from
/// stacking on one tile and keeps oceanic specks honouring `min_distance_from_continent`. Reading
/// the mask live *is* the derived-mask contract.
///
/// `is_ocean` is a different question — **not** `!land`, since an inland sea is water but not ocean,
/// and an oceanic speck must be seeded in open ocean rather than in a lake. It cannot be re-derived
/// from the field by a threshold, so it is *maintained incrementally* instead: raising a dome only
/// ever **removes** tiles from the ocean body (elevation only goes up here, so water never appears),
/// which makes "clear the carved tiles" the complete update. That keeps the candidate filter exact
/// regardless of `shelf.width_tiles` — with a zero-width fringe the live land check alone would let
/// a fragment re-seed on a tile an earlier fragment already raised.
#[allow(clippy::too_many_arguments)]
fn place_islands(
    field: &mut ElevationField,
    is_ocean: &[bool],
    islands: &IslandConfig,
    shelf: &ShelfConfig,
    sea_level: f32,
    w: usize,
    h: usize,
    seed: u64,
) {
    // Very lightweight placement: random samples along slope fringe for continental fragments
    // and in abyssal for oceanic islands.
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut rng = SmallRng::seed_from_u64(seed ^ 0xA51C_E55E);
    let peak = sea_level + islands.island_peak_margin.max(0.0);
    // Maintained across iterations: a tile a dome raised is no longer open ocean.
    let mut is_ocean = is_ocean.to_vec();

    // Continental fragments along slope fringe (distance in [shelf, shelf+slope])
    let fringe_min = shelf.width_tiles as usize;
    let fringe_max = (shelf.width_tiles + shelf.slope_width_tiles) as usize;
    let mut placed_cf = 0u32;
    let target_cf = ((w * h) as f32 * islands.continental_density) as u32;
    for _ in
        0..(target_cf * ISLAND_CONTINENTAL_ATTEMPT_FACTOR).max(ISLAND_CONTINENTAL_ATTEMPT_FLOOR)
    {
        if placed_cf >= target_cf {
            break;
        }
        let x = (rng.gen::<u32>() as usize) % w;
        let y = (rng.gen::<u32>() as usize) % h;
        let i = idx(x, y);
        if !is_ocean[i] {
            continue;
        }
        // approximate distance by scanning for nearest land within small window
        let mut near_dist = usize::MAX;
        for dy in -ISLAND_LAND_SCAN_RADIUS..=ISLAND_LAND_SCAN_RADIUS {
            for dx in -ISLAND_LAND_SCAN_RADIUS..=ISLAND_LAND_SCAN_RADIUS {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                if field.sample(nx as u32, ny as u32) > sea_level {
                    let d = dx.unsigned_abs() + dy.unsigned_abs();
                    near_dist = near_dist.min(d);
                }
            }
        }
        if near_dist >= fringe_min && near_dist <= fringe_max {
            let radius = ISLAND_CONTINENTAL_RADIUS_MIN
                + (rng.gen::<u32>() % ISLAND_CONTINENTAL_RADIUS_VARIANTS) as usize;
            raise_island_blob(
                field,
                w,
                h,
                &IslandDome {
                    center: (x, y),
                    radius,
                    sea_level,
                    peak,
                },
            );
            clear_ocean_under_dome(&mut is_ocean, w, h, (x, y), radius);
            placed_cf += 1;
        }
    }

    // Oceanic islands: far from continents; place in deep ocean
    let mut placed_oi = 0u32;
    let target_oi = ((w * h) as f32 * islands.oceanic_density) as u32;
    for _ in 0..(target_oi * ISLAND_OCEANIC_ATTEMPT_FACTOR).max(ISLAND_OCEANIC_ATTEMPT_FLOOR) {
        if placed_oi >= target_oi {
            break;
        }
        let x = (rng.gen::<u32>() as usize) % w;
        let y = (rng.gen::<u32>() as usize) % h;
        let i = idx(x, y);
        if !is_ocean[i] {
            continue;
        }
        // ensure min distance from land
        let mut ok = true;
        'scan: for dy in -(islands.min_distance_from_continent as isize)
            ..=(islands.min_distance_from_continent as isize)
        {
            for dx in -(islands.min_distance_from_continent as isize)
                ..=(islands.min_distance_from_continent as isize)
            {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                if field.sample(nx as u32, ny as u32) > sea_level {
                    ok = false;
                    break 'scan;
                }
            }
        }
        if !ok {
            continue;
        }
        raise_island_blob(
            field,
            w,
            h,
            &IslandDome {
                center: (x, y),
                radius: ISLAND_OCEANIC_RADIUS,
                sea_level,
                peak,
            },
        );
        clear_ocean_under_dome(&mut is_ocean, w, h, (x, y), ISLAND_OCEANIC_RADIUS);
        placed_oi += 1;
    }
}

/// Raise a radial elevation dome centred on `(cx, cy)`: `peak` at the centre falling linearly to
/// `ISLAND_RIM_HEIGHT_FRACTION * margin` above sea level at `radius`. `max` against the existing
/// field, so an island never *lowers* ground it lands on.
fn raise_island_blob(field: &mut ElevationField, w: usize, h: usize, dome: &IslandDome) {
    let IslandDome {
        center: (cx, cy),
        radius,
        sea_level,
        peak,
    } = *dome;
    let margin = (peak - sea_level).max(0.0);
    let rim = sea_level + margin * ISLAND_RIM_HEIGHT_FRACTION;
    let values = field.values_mut();
    for_each_dome_tile(w, h, (cx, cy), radius, |i, dist| {
        let target = peak + (rim - peak) * dist;
        values[i] = values[i].max(target).clamp(0.0, 1.0);
    });
}

/// The dome's footprint — every in-bounds tile of the disc `dx² + dy² <= radius²`, with its distance
/// from the centre normalized to `[0, 1]`. Shared by `raise_island_blob` and
/// `clear_ocean_under_dome` so the raised tiles and the tiles removed from the ocean body can never
/// disagree.
fn for_each_dome_tile(
    w: usize,
    h: usize,
    center: (usize, usize),
    radius: usize,
    mut visit: impl FnMut(usize, f32),
) {
    let (cx, cy) = center;
    let radius_f = (radius as f32).max(f32::EPSILON);
    for dy in -(radius as isize)..=(radius as isize) {
        for dx in -(radius as isize)..=(radius as isize) {
            let nx = cx as isize + dx;
            let ny = cy as isize + dy;
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let dist = ((dx * dx + dy * dy) as f32).sqrt() / radius_f;
            if dist > 1.0 {
                continue;
            }
            visit(ny as usize * w + nx as usize, dist);
        }
    }
}

/// A tile a dome raised above sea level is land, so it has left the open-ocean body. Elevation only
/// rises in `place_islands`, so removal is the only update the ocean classification can need.
fn clear_ocean_under_dome(
    is_ocean: &mut [bool],
    w: usize,
    h: usize,
    center: (usize, usize),
    radius: usize,
) {
    for_each_dome_tile(w, h, center, radius, |i, _dist| is_ocean[i] = false);
}

/// The land mask, a **pure derived function of the heightfield**: `land = elevation > sea_level`.
///
/// Elevation is the sole authority. Everything that shapes the coastline — continental structure,
/// coastline roughness, erosion, and the contour anchor that puts the `target_land_pct` quantile
/// exactly on `sea_level` — happens in `heightfield::build_elevation_field`, *upstream* of this
/// threshold. That is what makes "a water tile above sea level" unrepresentable rather than merely
/// rare: this function has no way to express it.
///
/// It used to rank tiles by `elevation + jitter` and grow boolean blobs from spaced seeds to fixed
/// per-continent area quotas, which invalidated the anchor's whole justification (a jittered ranking
/// *is* a reordering, so the mask selected a different 38% than the contour the anchor had aligned)
/// and produced both failure modes it was supposed to prevent: sunken land (quota-driven growth
/// swallowed sub-sea-level tiles) and floating high ground (seed-reachable-only growth left tall
/// tiles as ocean). `macro_land.jitter` now lives in the field as
/// `macro_land.coastline_roughness`, and `macro_land.continents` as the continental bias.
fn generate_land_mask(elevation: &ElevationField, sea_level: f32) -> LandMask {
    let total = (elevation.width as usize) * (elevation.height as usize);
    let mut mask = Vec::with_capacity(total);
    let mut land_count = 0usize;
    for y in 0..elevation.height {
        for x in 0..elevation.width {
            let is_land = elevation.sample(x, y) > sea_level;
            land_count += usize::from(is_land);
            mask.push(is_land);
        }
    }
    LandMask { mask, land_count }
}

fn pick_plate_seeds(cells: &[usize], plate_count: usize, w: usize, seed: u64) -> Vec<usize> {
    if plate_count == 0 || cells.is_empty() {
        return Vec::new();
    }
    let mut candidates: Vec<usize> = cells.to_vec();
    candidates.sort_by(|a, b| {
        let ax = (*a % w) as u32;
        let ay = (*a / w) as u32;
        let bx = (*b % w) as u32;
        let by = (*b / w) as u32;
        terrain_hash(seed, ax, ay).cmp(&terrain_hash(seed, bx, by))
    });

    let spacing = ((cells.len() as f32 / plate_count as f32).sqrt() as usize / 2).max(3);
    let mut seeds: Vec<usize> = Vec::with_capacity(plate_count);
    for &candidate in &candidates {
        let cx = candidate % w;
        let cy = candidate / w;
        if seeds.iter().all(|&existing| {
            let ex = existing % w;
            let ey = existing / w;
            let dx = cx.abs_diff(ex);
            let dy = cy.abs_diff(ey);
            dx + dy >= spacing
        }) {
            seeds.push(candidate);
            if seeds.len() == plate_count {
                break;
            }
        }
    }
    if seeds.len() < plate_count {
        for &candidate in &candidates {
            if seeds.contains(&candidate) {
                continue;
            }
            seeds.push(candidate);
            if seeds.len() == plate_count {
                break;
            }
        }
    }
    if seeds.is_empty() {
        seeds.push(cells[0]);
    }
    seeds
}

#[cfg(test)]
fn compute_ocean_mask(land: &[bool], w: usize, h: usize) -> Vec<bool> {
    compute_ocean_mask_wrapped(land, w, h, false)
}

/// Compute ocean mask with optional horizontal wrap support.
///
/// When wrapping horizontally, left/right edges connect (no ocean boundary there).
/// Ocean is seeded only from top/bottom edges.
fn compute_ocean_mask_wrapped(
    land: &[bool],
    w: usize,
    h: usize,
    wrap_horizontal: bool,
) -> Vec<bool> {
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut visited = vec![false; w * h];
    let mut is_ocean = vec![false; w * h];
    let mut q = VecDeque::new();

    // Seed from top and bottom edges (poles - always boundaries)
    for x in 0..w {
        if !land[idx(x, 0)] {
            q.push_back((x, 0));
        }
        if !land[idx(x, h.saturating_sub(1))] {
            q.push_back((x, h.saturating_sub(1)));
        }
    }

    // Seed from left and right edges only if NOT wrapping horizontally
    // When wrapping, these edges connect so ocean doesn't enter from there
    if !wrap_horizontal {
        for y in 0..h {
            if !land[idx(0, y)] {
                q.push_back((0, y));
            }
            if !land[idx(w.saturating_sub(1), y)] {
                q.push_back((w.saturating_sub(1), y));
            }
        }
    }

    while let Some((x, y)) = q.pop_front() {
        let i = idx(x, y);
        if visited[i] || land[i] {
            continue;
        }
        visited[i] = true;
        is_ocean[i] = true;
        for (nx, ny) in neighbors4_with_wrap(x, y, w, h, wrap_horizontal) {
            let ni = idx(nx, ny);
            if !visited[ni] && !land[ni] {
                q.push_back((nx, ny));
            }
        }
    }

    is_ocean
}

/// Compute distance from ocean (water tiles) to each tile, with optional wrap.
fn compute_ocean_distance_wrapped(
    land: &[bool],
    w: usize,
    h: usize,
    wrap_horizontal: bool,
) -> Vec<u32> {
    let mut distance = vec![u32::MAX; w * h];
    let mut dq = VecDeque::new();
    let idx = |x: usize, y: usize| -> usize { y * w + x };

    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if land[i] {
                distance[i] = 0;
                dq.push_back((x, y));
            }
        }
    }

    while let Some((x, y)) = dq.pop_front() {
        let base = distance[idx(x, y)];
        for (nx, ny) in neighbors4_with_wrap(x, y, w, h, wrap_horizontal) {
            let ni = idx(nx, ny);
            if distance[ni] == u32::MAX {
                distance[ni] = base.saturating_add(1);
                dq.push_back((nx, ny));
            }
        }
    }

    distance
}

#[cfg(test)]
fn compute_land_distance(land: &[bool], w: usize, h: usize) -> Vec<u32> {
    compute_land_distance_wrapped(land, w, h, false)
}

/// Compute distance from coast (water-adjacent land) inward, with optional wrap.
fn compute_land_distance_wrapped(
    land: &[bool],
    w: usize,
    h: usize,
    wrap_horizontal: bool,
) -> Vec<u32> {
    let mut distance = vec![u32::MAX; w * h];
    let mut dq = VecDeque::new();
    let idx = |x: usize, y: usize| -> usize { y * w + x };

    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if !land[i] {
                continue;
            }
            let mut adjacent_water = false;
            for (nx, ny) in neighbors4_with_wrap(x, y, w, h, wrap_horizontal) {
                if !land[idx(nx, ny)] {
                    adjacent_water = true;
                    break;
                }
            }
            if adjacent_water {
                distance[i] = 0;
                dq.push_back((x, y));
            }
        }
    }

    while let Some((x, y)) = dq.pop_front() {
        let base = distance[idx(x, y)];
        for (nx, ny) in neighbors4_with_wrap(x, y, w, h, wrap_horizontal) {
            let ni = idx(nx, ny);
            if !land[ni] {
                continue;
            }
            if distance[ni] == u32::MAX {
                distance[ni] = base.saturating_add(1);
                dq.push_back((nx, ny));
            }
        }
    }

    for value in distance.iter_mut() {
        if *value == u32::MAX {
            *value = 0;
        }
    }

    distance
}

fn compute_coastal_land(land: &[bool], is_ocean: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut coastal = vec![false; w * h];
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if !land[i] {
                continue;
            }
            for (nx, ny) in neighbors4(x, y, w, h) {
                let ni = idx(nx, ny);
                if is_ocean[ni] {
                    coastal[i] = true;
                    break;
                }
            }
        }
    }
    coastal
}

/// Resolve the shelf band width in tiles for a map of the given dimensions, as a
/// (possibly fractional) tile count.
///
/// When `shelf.width_frac` is set, the width scales with the map's shorter
/// dimension as `width_frac * min(w, h)^width_exp` (`width_exp` defaults to 1.0)
/// so the shelf stays a size-invariant fraction of the ocean. Crucially the
/// result is **not** floored to a whole tile: at coarse map resolution Earth's
/// shelf is thinner than one tile, so a sub-1.0 width is meaningful and
/// `classify_bands` renders it as a partial coastal ring (see there). Presets
/// that omit `width_frac` fall back to the fixed integer `width_tiles`
/// (historical behavior).
///
/// The result is clamped to `[0, min(w, h)]`: a shelf can't sensibly be wider
/// than the map, and clamping guards a misconfigured `width_frac`/`width_exp`
/// (huge or non-finite) from overflowing the `u32` band arithmetic in
/// `classify_bands`.
fn effective_shelf_width(shelf: &ShelfConfig, w: usize, h: usize) -> f32 {
    let min_dim = w.min(h) as f32;
    let raw = match shelf.width_frac {
        Some(frac) => {
            let exp = shelf.width_exp.unwrap_or(1.0);
            frac.max(0.0) * min_dim.powf(exp)
        }
        None => shelf.width_tiles as f32,
    };
    if raw.is_finite() {
        // Floor to `min_width_tiles` so a qualifying (gentle) coast gets a continuous
        // ≥1-tile ring instead of the old sub-tile sparse fringe; `width_frac`/`width_exp`
        // still scale it wider on big maps. The `coast_height_threshold` gate in
        // `classify_bands` keeps steep coasts off the shelf, so this floor doesn't blow up
        // the shelf fraction the way a blanket ring on every coast would.
        raw.max(shelf.min_width_tiles).clamp(0.0, min_dim)
    } else {
        0.0
    }
}

/// Map a per-tile hash into a unit `[0, 1)` value for deterministic thresholding.
fn shelf_hash_unit(seed: u64, x: usize, y: usize) -> f32 {
    terrain_hash(seed, x as u32, y as u32) as f32 / (u32::MAX as f32 + 1.0)
}

/// Minimum normalized rise (`elevation.sample − sea_level`) over the land tiles **hex-adjacent**
/// (odd-r 6-neighbour, wrap-aware on x) to ocean tile `(x, y)`. `None` when the tile touches no
/// land hex-neighbour — i.e. it is not on the immediate coastal ring.
///
/// Uses the authoritative odd-r hex adjacency (`grid_utils::hex_neighbors_wrapped`, the same
/// helper gameplay + the client renderer use) rather than 4-connected square neighbours, so
/// "coast-adjacent in worldgen" == "coast-adjacent on screen". This drives BOTH the 1-tile shelf
/// ring's candidacy (`Some` ⇒ the ocean tile touches at least one Land hex-neighbour) and the
/// coast-height gate's min rise, closing the old hex-diagonal gaps where a gentle coast could sit
/// directly against DeepOcean (the 4-cardinal set covers only two of the six hex directions).
// Justified: a leaf worldgen helper whose args are genuinely distinct scalars (land mask, elevation
// field, sea level, tile x/y, grid w/h, wrap flag); bundling them into a struct would only obscure.
#[allow(clippy::too_many_arguments)]
fn min_adjacent_coast_rise(
    land: &[bool],
    elevation: &ElevationField,
    sea_level: f32,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    wrap_horizontal: bool,
) -> Option<f32> {
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut min_rise: Option<f32> = None;
    for (nx, ny) in hex_neighbors_wrapped(x as u32, y as u32, w as u32, h as u32, wrap_horizontal) {
        let (nx, ny) = (nx as usize, ny as usize);
        if land[idx(nx, ny)] {
            let rise = elevation.sample(nx as u32, ny as u32) - sea_level;
            min_rise = Some(min_rise.map_or(rise, |m| m.min(rise)));
        }
    }
    min_rise
}

// Justified: a leaf worldgen helper whose args are genuinely distinct inputs (land/ocean masks,
// ocean-distance grid, shelf config, elevation field, sea level, grid w/h, wrap flag, seed);
// bundling them into a context struct would only obscure the coast-band computation.
#[allow(clippy::too_many_arguments)]
fn classify_bands(
    land: &[bool],
    is_ocean: &[bool],
    ocean_distance: &[u32],
    shelf: &ShelfConfig,
    elevation: &ElevationField,
    sea_level: f32,
    w: usize,
    h: usize,
    wrap_horizontal: bool,
    seed: u64,
) -> Vec<TerrainBand> {
    let mut terrain = vec![TerrainBand::DeepOcean; w * h];
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    // Shelf width: `full` whole rings around the coast are shelf candidates; the next
    // ring (`full + 1`) is a candidate on only `frac` of its tiles (deterministic hash).
    // With the `min_width_tiles` floor (default 1.0) `full == 1`/`frac == 0`, so the
    // default shelf is the immediate coastal ring — determined HEX-exactly below (hex-adjacent
    // to land), not via the square `ocean_distance == 1`, so it has no hex-diagonal gaps. The
    // outer (`full > 1`) rings still ride the square-connected `ocean_distance`. Slope collapses
    // to DeepOcean downstream, so its exact extent is cosmetic — only the shelf boundary matters
    // for the ocean composition.
    let shelf_width = effective_shelf_width(shelf, w, h);
    let full = shelf_width.floor();
    let frac = shelf_width - full;
    let full = full as u32;
    let coast_height_threshold = shelf.coast_height_threshold;

    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if land[i] {
                terrain[i] = TerrainBand::Land;
                continue;
            }
            if !is_ocean[i] {
                terrain[i] = TerrainBand::InlandSea;
                continue;
            }
            let d = ocean_distance[i];
            // Hex-exact min rise over the tile's LAND hex-neighbours (`None` ⇒ touches no land).
            // Authoritative odd-r 6-neighbour adjacency, so the immediate coastal ring matches
            // what the client renders — no hex-diagonal gaps.
            let coast_rise =
                min_adjacent_coast_rise(land, elevation, sea_level, x, y, w, h, wrap_horizontal);
            // Immediate coastal ring — HEX-exact. An ocean tile is on the default 1-tile shelf
            // ring iff it is hex-adjacent to at least one Land tile (`coast_rise.is_some()`),
            // covering all six odd-r directions so a gentle coast never falls through to
            // slope→DeepOcean on a hex-diagonal. Coast-height gate: the MIN rise over its LAND
            // hex-neighbours must be gentle (< threshold); steep/cliff coasts stay off the shelf.
            let immediate_ring_shelf = coast_rise.is_some_and(|rise| rise < coast_height_threshold);
            // Outer rings (only when a preset widens the shelf past the `min_width_tiles` floor,
            // i.e. `full > 1`) still follow the pre-existing SQUARE-connected ocean-distance
            // transform. Only the immediate ring above is hex-exact; a full hex distance-transform
            // for wide shelves is the follow-up. Outer-ring tiles touch no land, so the
            // coast-height gate passes them unfiltered (`None → true`), matching prior behaviour.
            let outer_ring_candidate = d >= 2
                && (d <= full
                    || (d == full + 1 && frac > 0.0 && shelf_hash_unit(seed, x, y) < frac));
            let outer_ring_shelf =
                outer_ring_candidate && coast_rise.is_none_or(|rise| rise < coast_height_threshold);
            let is_shelf = immediate_ring_shelf || outer_ring_shelf;
            if is_shelf {
                terrain[i] = TerrainBand::ContinentalShelf;
            } else if d <= full + shelf.slope_width_tiles {
                terrain[i] = TerrainBand::ContinentalSlope;
            } else {
                terrain[i] = TerrainBand::DeepOcean;
            }
        }
    }

    terrain
}

#[allow(clippy::too_many_arguments, clippy::manual_is_multiple_of)]
fn derive_mountain_mask(
    land: &[bool],
    is_ocean: &[bool],
    land_distance: &[u32],
    elevation: &ElevationField,
    cfg: &crate::map_preset::MountainsConfig,
    mountain_scale: f32,
    w: usize,
    h: usize,
    seed: u64,
) -> MountainMask {
    let total = w * h;
    let belt_width_base = cfg.belt_width_tiles.max(1);
    let belt_width = ((belt_width_base as f32)
        * (1.0 + mountain_scale.clamp(0.0, BELT_WIDTH_SCALE_MAX) * BELT_WIDTH_SCALE_GAIN))
        .round()
        .max(1.0) as u32;
    let mut mask = MountainMask::new(w, h, belt_width);
    let mut plateau_cells: Vec<usize> = Vec::new();
    if total == 0 {
        return mask;
    }

    let mut component_ids = vec![-1i32; total];
    let mut components: Vec<Vec<usize>> = Vec::new();
    let mut queue = VecDeque::new();

    let cardinal = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    for idx in 0..total {
        if !land[idx] || component_ids[idx] != -1 {
            continue;
        }
        let mut cells = Vec::new();
        component_ids[idx] = components.len() as i32;
        queue.push_back(idx);
        while let Some(current) = queue.pop_front() {
            cells.push(current);
            let x = current % w;
            let y = current / w;
            for (dx, dy) in cardinal {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                let nidx = ny as usize * w + nx as usize;
                if land[nidx] && component_ids[nidx] == -1 {
                    component_ids[nidx] = component_ids[idx];
                    queue.push_back(nidx);
                }
            }
        }
        components.push(cells);
    }

    if components.is_empty() {
        return mask;
    }

    let mut rng = SmallRng::seed_from_u64(seed ^ 0xD1FFE77E);
    let mut plate_ids = vec![-1i32; total];
    let mut plate_flows: Vec<(f32, f32)> = Vec::new();
    let mut global_plate_index = 0usize;

    for (comp_idx, cells) in components.iter().enumerate() {
        let comp_area = cells.len();
        if comp_area == 0 {
            continue;
        }

        let mut centroid_x = 0.0f32;
        let mut centroid_y = 0.0f32;
        for &cell in cells {
            centroid_x += (cell % w) as f32;
            centroid_y += (cell / w) as f32;
        }
        centroid_x /= comp_area as f32;
        centroid_y /= comp_area as f32;

        let mut plate_count = if comp_area < cfg.plate_area_bucket_2 as usize {
            1
        } else if comp_area < cfg.plate_area_bucket_3 as usize {
            2
        } else if comp_area < cfg.plate_area_bucket_4 as usize {
            3
        } else {
            4
        };
        if plate_count <= 1 && comp_area >= cfg.plate_area_bump as usize {
            plate_count = 2;
        }
        plate_count = plate_count.min(comp_area.max(1));

        let seed_offset = seed ^ ((comp_idx as u64 + 1) * 0x7F4A_7C15);
        let mut seeds = pick_plate_seeds(cells, plate_count, w, seed_offset);
        if seeds.is_empty() {
            seeds.push(cells[0]);
        }

        for &seed_cell in &seeds {
            let sx = (seed_cell % w) as f32;
            let sy = (seed_cell / w) as f32;
            let mut vx = sx - centroid_x;
            let mut vy = sy - centroid_y;
            let len = (vx * vx + vy * vy).sqrt();
            if len <= 0.5 {
                let angle = rng.gen::<f32>() * std::f32::consts::TAU;
                vx = angle.cos();
                vy = angle.sin();
            } else {
                vx /= len;
                vy /= len;
                let jitter = (rng.gen::<f32>() - 0.5) * PLATE_DRIFT_JITTER;
                let (sin_j, cos_j) = jitter.sin_cos();
                let rx = vx * cos_j - vy * sin_j;
                let ry = vx * sin_j + vy * cos_j;
                vx = rx;
                vy = ry;
            }
            let norm = (vx * vx + vy * vy).sqrt().max(1e-3);
            plate_flows.push((vx / norm, vy / norm));
        }

        let mut queue = VecDeque::new();
        for (local_idx, &seed_cell) in seeds.iter().enumerate() {
            let plate_id = (global_plate_index + local_idx) as i32;
            plate_ids[seed_cell] = plate_id;
            queue.push_back(seed_cell);
        }
        while let Some(cell) = queue.pop_front() {
            let plate_id = plate_ids[cell];
            let x = cell % w;
            let y = cell / w;
            for &(dx, dy) in &cardinal {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                let nidx = ny as usize * w + nx as usize;
                if plate_ids[nidx] != -1 {
                    continue;
                }
                if component_ids[nidx] != comp_idx as i32 {
                    continue;
                }
                plate_ids[nidx] = plate_id;
                queue.push_back(nidx);
            }
        }
        global_plate_index += seeds.len();
    }

    let mut boundary_cells: Vec<(usize, usize)> = Vec::new();
    let neighbor_offsets = neighbor_dirs();
    for idx in 0..total {
        let plate_id = plate_ids[idx];
        if plate_id < 0 {
            continue;
        }
        let mut is_boundary = false;
        for &(dx, dy) in &neighbor_offsets {
            let x = idx % w;
            let y = idx / w;
            let nx = x as isize + dx as isize;
            let ny = y as isize + dy as isize;
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let nidx = ny as usize * w + nx as usize;
            let other_plate = plate_ids[nidx];
            if other_plate < 0 || other_plate == plate_id {
                continue;
            }
            let flow_a = plate_flows
                .get(plate_id as usize)
                .copied()
                .unwrap_or((0.0, 0.0));
            let flow_b = plate_flows
                .get(other_plate as usize)
                .copied()
                .unwrap_or((0.0, 0.0));
            let dot = flow_a.0 * flow_b.0 + flow_a.1 * flow_b.1;
            if dot <= cfg.belt_convergence {
                if !is_boundary {
                    boundary_cells.push((idx, plate_id as usize));
                    is_boundary = true;
                }
                boundary_cells.push((nidx, other_plate as usize));
            }
        }
        if is_boundary {
            mask.set(
                idx,
                MountainCell {
                    ty: MountainType::Fold,
                    strength: (belt_width + 1) as u8,
                },
            );
        }
    }

    if !boundary_cells.is_empty() {
        let mut visited = vec![u32::MAX; total];
        let mut belt_queue: VecDeque<(usize, usize, u32)> = VecDeque::new();
        for (cell, comp) in boundary_cells {
            belt_queue.push_back((cell, comp, 0));
            visited[cell] = 0;
        }
        while let Some((cell, comp, dist)) = belt_queue.pop_front() {
            if dist > belt_width {
                continue;
            }
            if plate_ids[cell] < 0 || plate_ids[cell] as usize != comp {
                continue;
            }
            let strength = ((belt_width + 1).saturating_sub(dist)) as u8;
            mask.set(
                cell,
                MountainCell {
                    ty: MountainType::Fold,
                    strength,
                },
            );
            if dist == belt_width {
                continue;
            }
            let x = cell % w;
            let y = cell / w;
            for &(dx, dy) in &neighbor_offsets {
                let nx = x as isize + dx as isize;
                let ny = y as isize + dy as isize;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                let nidx = ny as usize * w + nx as usize;
                if plate_ids[nidx] < 0 || plate_ids[nidx] as usize != comp {
                    continue;
                }
                if visited[nidx] <= dist + 1 {
                    continue;
                }
                visited[nidx] = dist + 1;
                belt_queue.push_back((nidx, comp, dist + 1));
            }
        }
    }

    let mut polar_band = (cfg.polar_latitude_fraction.clamp(0.0, 0.5) * h as f32).round() as usize;
    if cfg.polar_microplate_density > f32::EPSILON && total > 0 {
        if cfg.polar_latitude_fraction > 0.0 && polar_band == 0 {
            polar_band = 1;
        }
        let max_band = (h / 2).max(1);
        polar_band = polar_band.min(max_band);
        if polar_band > 0 {
            let south_band_start = h.saturating_sub(polar_band);
            let mut polar_microplate_ids = vec![-1i32; total];
            let mut microplate_flows: Vec<(f32, f32)> = Vec::new();
            let mut polar_logs: Vec<PolarLogEntry> = Vec::new();
            let convergence_threshold = cfg.polar_convergence;
            let divergence_threshold = cfg.polar_divergence;

            for (comp_idx, cells) in components.iter().enumerate() {
                let mut polar_cells = Vec::new();
                for &idx in cells.iter() {
                    let y = idx / w;
                    if y < polar_band || y >= south_band_start {
                        polar_cells.push(idx);
                    }
                }

                if polar_cells.len() < MIN_POLAR_CELLS {
                    continue;
                }

                let touches_north = polar_cells.iter().any(|&idx| idx / w < polar_band);
                let touches_south = polar_cells.iter().any(|&idx| idx / w >= south_band_start);

                let mut centroid_x = 0.0f32;
                let mut centroid_y = 0.0f32;
                for &idx in &polar_cells {
                    centroid_x += (idx % w) as f32;
                    centroid_y += (idx / w) as f32;
                }
                let inv = 1.0 / polar_cells.len() as f32;
                centroid_x *= inv;
                centroid_y *= inv;

                let desired = (polar_cells.len() as f32) * cfg.polar_microplate_density.max(0.0);
                let mut microplate_count = desired.ceil() as usize;
                let upper_bound = (polar_cells.len() / POLAR_MICROPLATE_DIVISOR).max(2);
                if microplate_count < 2 {
                    microplate_count = 2;
                }
                microplate_count = microplate_count.min(upper_bound);
                if microplate_count < 2 {
                    continue;
                }

                let mut micro_rng =
                    SmallRng::seed_from_u64(seed ^ ((comp_idx as u64 + 1) * 0xC19F_D743));
                let mut seeds: Vec<usize> = Vec::with_capacity(microplate_count);
                let mut north_candidates = Vec::new();
                let mut south_candidates = Vec::new();
                for &idx in &polar_cells {
                    let y = idx / w;
                    if y < polar_band {
                        north_candidates.push(idx);
                    }
                    if y >= south_band_start {
                        south_candidates.push(idx);
                    }
                }
                if !north_candidates.is_empty() && seeds.len() < microplate_count {
                    let pick = north_candidates[micro_rng.gen_range(0..north_candidates.len())];
                    seeds.push(pick);
                }
                if !south_candidates.is_empty() && seeds.len() < microplate_count {
                    let pick = south_candidates[micro_rng.gen_range(0..south_candidates.len())];
                    if !seeds.contains(&pick) {
                        seeds.push(pick);
                    }
                }
                let mut shuffled = polar_cells.clone();
                shuffled.shuffle(&mut micro_rng);
                for cell in shuffled {
                    if seeds.len() >= microplate_count {
                        break;
                    }
                    if !seeds.contains(&cell) {
                        seeds.push(cell);
                    }
                }
                if seeds.len() < 2 {
                    continue;
                }

                let bias = if touches_north && !touches_south {
                    (0.0, 1.0)
                } else if touches_south && !touches_north {
                    (0.0, -1.0)
                } else {
                    (0.0, 0.0)
                };
                let global_start = microplate_flows.len();
                for &seed_cell in &seeds {
                    let sx = (seed_cell % w) as f32;
                    let sy = (seed_cell / w) as f32;
                    let mut vx = sx - centroid_x;
                    let mut vy = sy - centroid_y;
                    let len = (vx * vx + vy * vy).sqrt();
                    if len > 0.25 {
                        vx /= len;
                        vy /= len;
                    } else {
                        let theta = micro_rng.gen::<f32>() * std::f32::consts::TAU;
                        vx = theta.cos();
                        vy = theta.sin();
                    }
                    let theta = micro_rng.gen::<f32>() * std::f32::consts::TAU;
                    let rand_vec = (theta.cos(), theta.sin());
                    vx = vx * POLAR_FLOW_RADIAL
                        + rand_vec.0 * POLAR_FLOW_RANDOM
                        + bias.0 * POLAR_FLOW_BIAS;
                    vy = vy * POLAR_FLOW_RADIAL
                        + rand_vec.1 * POLAR_FLOW_RANDOM
                        + bias.1 * POLAR_FLOW_BIAS;
                    let norm = (vx * vx + vy * vy).sqrt().max(1e-3);
                    microplate_flows.push((vx / norm, vy / norm));
                }

                let uplift_floor = cfg.polar_uplift_scale.max(1.0);
                let relief_cap = cfg.polar_low_relief_scale.clamp(MIN_RELIEF_SCALE, 1.0);

                let mut queue = VecDeque::new();
                for (local_idx, &seed_cell) in seeds.iter().enumerate() {
                    let id = (global_start + local_idx) as i32;
                    polar_microplate_ids[seed_cell] = id;
                    queue.push_back(seed_cell);
                }
                while let Some(cell) = queue.pop_front() {
                    let id = polar_microplate_ids[cell];
                    let x = cell % w;
                    let y = cell / w;
                    for &(dx, dy) in &cardinal {
                        let nx = x as isize + dx;
                        let ny = y as isize + dy;
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let nidx = ny as usize * w + nx as usize;
                        if polar_microplate_ids[nidx] != -1 {
                            continue;
                        }
                        if component_ids[nidx] != comp_idx as i32 {
                            continue;
                        }
                        let ny_idx = nidx / w;
                        if ny_idx < polar_band || ny_idx >= south_band_start {
                            polar_microplate_ids[nidx] = id;
                            queue.push_back(nidx);
                        }
                    }
                }

                for &cell in &polar_cells {
                    if polar_microplate_ids[cell] != -1 {
                        continue;
                    }
                    let theta = micro_rng.gen::<f32>() * std::f32::consts::TAU;
                    microplate_flows.push((theta.cos(), theta.sin()));
                    let new_id = (microplate_flows.len() - 1) as i32;
                    polar_microplate_ids[cell] = new_id;
                    queue.push_back(cell);
                    while let Some(current) = queue.pop_front() {
                        let x = current % w;
                        let y = current / w;
                        for &(dx, dy) in &cardinal {
                            let nx = x as isize + dx;
                            let ny = y as isize + dy;
                            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                                continue;
                            }
                            let nidx = ny as usize * w + nx as usize;
                            if polar_microplate_ids[nidx] != -1 {
                                continue;
                            }
                            if component_ids[nidx] != comp_idx as i32 {
                                continue;
                            }
                            let ny_idx = nidx / w;
                            if ny_idx < polar_band || ny_idx >= south_band_start {
                                polar_microplate_ids[nidx] = new_id;
                                queue.push_back(nidx);
                            }
                        }
                    }
                }

                let mut comp_uplift = 0usize;
                let mut comp_relief = 0usize;
                let mut comp_fault = 0usize;

                for &cell in &polar_cells {
                    let id = polar_microplate_ids[cell];
                    if id < 0 {
                        continue;
                    }
                    let x = cell % w;
                    let y = cell / w;
                    for &(dx, dy) in &neighbor_offsets {
                        let nx = x as isize + dx as isize;
                        let ny = y as isize + dy as isize;
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let nidx = ny as usize * w + nx as usize;
                        let other_id = polar_microplate_ids[nidx];
                        if other_id < 0 || other_id == id || id > other_id {
                            continue;
                        }
                        let flow_a = microplate_flows
                            .get(id as usize)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        let flow_b = microplate_flows
                            .get(other_id as usize)
                            .copied()
                            .unwrap_or((0.0, 0.0));
                        let dot = flow_a.0 * flow_b.0 + flow_a.1 * flow_b.1;
                        if dot <= convergence_threshold {
                            let base_strength = (belt_width + 1) as u8;
                            let uplift_strength = ((base_strength as f32 * uplift_floor).round()
                                as u8)
                                .max(base_strength);
                            mask.set(
                                cell,
                                MountainCell {
                                    ty: MountainType::Fold,
                                    strength: uplift_strength,
                                },
                            );
                            mask.set(
                                nidx,
                                MountainCell {
                                    ty: MountainType::Fold,
                                    strength: uplift_strength,
                                },
                            );
                            if mask.enforce_relief_floor(cell, uplift_floor) {
                                comp_uplift += 1;
                            }
                            if mask.enforce_relief_floor(nidx, uplift_floor) {
                                comp_uplift += 1;
                            }
                        } else if dot >= divergence_threshold {
                            if mask.enforce_relief_cap(cell, relief_cap) {
                                comp_relief += 1;
                            }
                            if mask.enforce_relief_cap(nidx, relief_cap) {
                                comp_relief += 1;
                            }
                        } else if micro_rng.gen::<f32>() < 0.4 {
                            mask.set(
                                cell,
                                MountainCell {
                                    ty: MountainType::Fault,
                                    strength: POLAR_FAULT_STRENGTH,
                                },
                            );
                            mask.set(
                                nidx,
                                MountainCell {
                                    ty: MountainType::Fault,
                                    strength: POLAR_FAULT_STRENGTH,
                                },
                            );
                            comp_fault += 2;
                        }
                    }
                }

                if comp_uplift > 0 || comp_relief > 0 || comp_fault > 0 {
                    polar_logs.push((
                        comp_idx,
                        polar_cells.len(),
                        seeds.len(),
                        touches_north,
                        touches_south,
                        comp_uplift,
                        comp_relief,
                        comp_fault,
                    ));
                }
            }

            for (
                component,
                polar_cells,
                plates,
                touches_north,
                touches_south,
                uplift_cells,
                relief_cells,
                fault_cells,
            ) in polar_logs
            {
                tracing::info!(
                    target: "shadow_scale::mapgen",
                    component,
                    polar_cells,
                    microplates = plates,
                    touches_north,
                    touches_south,
                    uplift_cells,
                    relief_cells,
                    shear_fault_cells = fault_cells,
                    "mapgen.tectonics.polar_microplates"
                );
            }
        }
    }

    let fault_line_count = cfg.fault_line_count.min(MAX_FAULT_LINES);
    let fault_dirs: &[(isize, isize)] = &[
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];

    for (comp_idx, cells) in components.iter().enumerate() {
        if cells.len() < MIN_FAULT_PLATE_AREA {
            continue;
        }
        let mut comp_rng = SmallRng::seed_from_u64(seed ^ ((comp_idx as u64 + 1) * 0x9E37C15D));

        let interior_cells: Vec<usize> = cells
            .iter()
            .copied()
            .filter(|&idx| land_distance[idx] >= FAULT_INTERIOR_START_DIST)
            .collect();
        let fault_start_pool = if !interior_cells.is_empty() {
            &interior_cells
        } else {
            cells
        };

        let mut local_faults = fault_line_count.max(1);
        if cells.len() > cfg.fault_area_bonus_2 as usize {
            local_faults += 1;
        }
        if cells.len() > cfg.fault_area_bonus_3 as usize {
            local_faults += 1;
        }

        for _ in 0..local_faults {
            let start = fault_start_pool[comp_rng.gen_range(0..fault_start_pool.len())];
            let dir = fault_dirs[comp_rng.gen_range(0..fault_dirs.len())];
            let mut current = start;
            let mut length =
                (cells.len() as f32 * cfg.fault_length_fraction * comp_rng.gen::<f32>()).round()
                    as usize;
            length = length.clamp(4, (cells.len() / 2).max(4));
            let mut step = 0usize;
            let strength = FAULT_SEAM_STRENGTH;
            let mut block_phase = comp_rng.gen_range(2..5);
            while step < length {
                if step % block_phase != 0 {
                    mask.set(
                        current,
                        MountainCell {
                            ty: MountainType::Fault,
                            strength,
                        },
                    );

                    let x = current % w;
                    let y = current / w;
                    let perpendicular = (-dir.1, dir.0);
                    for &(px, py) in [perpendicular, (-perpendicular.0, -perpendicular.1)].iter() {
                        if comp_rng.gen::<f32>() > FAULT_BRANCH_SKIP_CHANCE {
                            continue;
                        }
                        let nx = x as isize + px;
                        let ny = y as isize + py;
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let nidx = ny as usize * w + nx as usize;
                        if component_ids[nidx] == comp_idx as i32 {
                            mask.set(
                                nidx,
                                MountainCell {
                                    ty: MountainType::Fault,
                                    strength: strength.saturating_sub(FAULT_FLANK_STRENGTH_DROP),
                                },
                            );
                        }
                    }
                }
                let x = current % w;
                let y = current / w;
                let nx = x as isize + dir.0;
                let ny = y as isize + dir.1;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    break;
                }
                let next_idx = ny as usize * w + nx as usize;
                if component_ids[next_idx] != comp_idx as i32 {
                    break;
                }
                current = next_idx;
                step += 1;
                if step % block_phase == 0 {
                    block_phase = comp_rng.gen_range(2..5);
                }
            }
        }

        let mut coastal: Vec<usize> = cells
            .iter()
            .copied()
            .filter(|&idx| {
                neighbor_offsets.iter().any(|&(dx, dy)| {
                    let x = idx % w;
                    let y = idx / w;
                    let nx = x as isize + dx as isize;
                    let ny = y as isize + dy as isize;
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        false
                    } else {
                        !land[ny as usize * w + nx as usize]
                            || is_ocean[ny as usize * w + nx as usize]
                    }
                })
            })
            .collect();

        let coastal_fraction = (coastal.len() as f32 / cells.len() as f32).clamp(0.0, 1.0);
        let volcanic_weight = (cells.len() as f32 / cfg.volcanic_area_norm).clamp(0.3, 1.4)
            * (cfg.volcanic_coastal_base + cfg.volcanic_coastal_gain * coastal_fraction);
        let volcanic_chance =
            (cfg.volcanic_arc_chance * volcanic_weight).clamp(0.0, VOLCANIC_CHANCE_CEILING);
        let max_chains = cfg.max_volcanic_chains_per_plate.max(1) as usize;
        let mut chains_spawned = 0usize;
        let mut attempts = max_chains * 3;
        let strength_drop = cfg.volcanic_strength_drop.max(0.8);
        let component_cap = ((cells.len() as f32) * cfg.volcanic_tile_fraction)
            .clamp(6.0, cfg.volcanic_tile_cap_per_plate as f32)
            as usize;
        let tile_cap = component_cap.max(4);
        let mut volcanic_tiles_used = 0usize;

        while attempts > 0 && chains_spawned < max_chains {
            attempts -= 1;
            if coastal.is_empty() {
                break;
            }
            if comp_rng.gen::<f32>() >= volcanic_chance {
                continue;
            }
            let start_idx = comp_rng.gen_range(0..coastal.len());
            let mut start = coastal.swap_remove(start_idx);
            let mut chain_dir = fault_dirs[comp_rng.gen_range(0..fault_dirs.len())];
            let base_length = cfg.volcanic_chain_length.clamp(1, VOLCANIC_CHAIN_LEN_CAP) as usize;
            let max_chain_len = (tile_cap - volcanic_tiles_used).max(1);
            let chain_len = base_length.min(max_chain_len);
            let mut chain_strength =
                (cfg.volcanic_strength * VOLCANIC_STRENGTH_SCALE).clamp(2.5, 9.0);
            let mut chain_step = 0usize;
            let mut chain_gap = comp_rng.gen_range(2..5);

            while chain_step < chain_len && chain_strength > 1.0 {
                if volcanic_tiles_used >= tile_cap {
                    break;
                }
                let primary_strength = chain_strength.round().clamp(1.0, MAX_CELL_STRENGTH) as u8;
                mask.set(
                    start,
                    MountainCell {
                        ty: MountainType::Volcanic,
                        strength: primary_strength,
                    },
                );
                volcanic_tiles_used += 1;

                if comp_rng.gen::<f32>() < VOLCANIC_FLANK_SPAWN_CHANCE {
                    let x = start % w;
                    let y = start / w;
                    let perpendicular = (-chain_dir.1, chain_dir.0);
                    for &(px, py) in [perpendicular, (-perpendicular.0, -perpendicular.1)].iter() {
                        if comp_rng.gen::<f32>() > VOLCANIC_FLANK_SIDE_SKIP {
                            continue;
                        }
                        let nx = x as isize + px;
                        let ny = y as isize + py;
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let nidx = ny as usize * w + nx as usize;
                        if component_ids[nidx] == comp_idx as i32 {
                            let flank_strength =
                                (chain_strength - strength_drop * VOLCANIC_FLANK_DROP_FACTOR)
                                    .round()
                                    .clamp(1.0, 9.0) as u8;
                            mask.set(
                                nidx,
                                MountainCell {
                                    ty: MountainType::Volcanic,
                                    strength: flank_strength,
                                },
                            );
                            volcanic_tiles_used += 1;
                            if volcanic_tiles_used >= tile_cap {
                                break;
                            }
                        }
                    }
                }

                let x = start % w;
                let y = start / w;
                let nx = x as isize + chain_dir.0;
                let ny = y as isize + chain_dir.1;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    break;
                }
                let next_idx = ny as usize * w + nx as usize;
                if component_ids[next_idx] != comp_idx as i32 {
                    break;
                }
                start = next_idx;
                chain_step += 1;
                chain_strength = (chain_strength - strength_drop).max(1.0);
                if chain_step % chain_gap == 0 {
                    chain_gap = comp_rng.gen_range(2..5);
                    chain_dir = if comp_rng.gen::<f32>() < VOLCANIC_DIR_CHANGE_CHANCE {
                        *(fault_dirs
                            .get(comp_rng.gen_range(0..fault_dirs.len()))
                            .unwrap_or(&(1, 0)))
                    } else {
                        chain_dir
                    };
                    if comp_rng.gen::<f32>() < VOLCANIC_GAP_SKIP_CHANCE {
                        chain_step += 1;
                    }
                }
            }

            chains_spawned += 1;
        }

        tracing::debug!(
            target: "shadow_scale::mapgen",
            plate = comp_idx,
            area = cells.len(),
            volcanic_tiles = volcanic_tiles_used,
            volcanic_tile_cap = tile_cap,
            chains_spawned,
            "tectonics.volcanic_budget",
        );

        let plateau_fraction = cfg.plateau_density.clamp(0.0, 0.2);
        let plateau_count =
            ((cells.len() as f32 * plateau_fraction).round() as usize).min(cells.len());
        if plateau_count > 0 {
            let mut ranked: Vec<(usize, f32)> = cells
                .iter()
                .copied()
                .map(|idx| (idx, elevation.sample((idx % w) as u32, (idx / w) as u32)))
                .collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
            for &(cell, _) in ranked.iter().take(plateau_count) {
                mask.set(
                    cell,
                    MountainCell {
                        ty: MountainType::Dome,
                        strength: DOME_CELL_STRENGTH,
                    },
                );
                if matches!(
                    mask.get(cell),
                    Some(MountainCell {
                        ty: MountainType::Dome,
                        ..
                    })
                ) {
                    plateau_cells.push(cell);
                }
            }
        }
    }

    apply_belt_relief(&mut mask, w, h, cfg);
    apply_plateau_microrelief(&mut mask, &plateau_cells, w, h, cfg, seed);

    let (fold, fault, volcanic, dome) = mask.iter_counts();
    tracing::info!(
        target: "shadow_scale::mapgen",
        fold,
        fault,
        volcanic,
        dome,
        "mapgen.tectonics.summary"
    );

    mask
}

/// Scales belt-tile relief by belt strength so cores tower (clearing the AlpineMountain
/// threshold) and edges taper to plateaus/hills. Raise-only (`enforce_relief_floor`), so
/// it never lowers the plateau relief tuning applied during mask construction.
///
/// Polar rows are intentionally skipped: those tiles have their own uplift/low-relief
/// basin tuning (`polar_*` config), they become SeasonalSnowfield rather than Alpine
/// anyway, and boosting them would flatten the deliberate polar contrast. Domes are also
/// skipped — they are handled as low-relief plateaus by microrelief.
fn apply_belt_relief(
    mask: &mut MountainMask,
    w: usize,
    h: usize,
    cfg: &crate::map_preset::MountainsConfig,
) {
    let gain = cfg.relief_belt_gain.max(0.0);
    if gain <= f32::EPSILON {
        return;
    }
    let fold_band_width = mask.fold_band_width() as f32 + 1.0;
    // Only skip polar rows when a polar band is actually configured. `= 0.0` means
    // "no polar band" and must skip nothing, matching the polar_band idiom used during
    // microplate seeding (see `polar_latitude_fraction > 0.0` guard above).
    // Only skip polar rows when a polar band is actually configured. `= 0.0` means
    // "no polar band" and must skip nothing. Match `derive_mountain_mask`: the band is at
    // most half the map, so clamp the fraction to 0.5 and cap the skipped rows to h/2 —
    // otherwise a `fraction > 0.5` would overlap the top/bottom bands and skip every row.
    let polar_rows = if cfg.polar_latitude_fraction > 0.0 {
        ((h as f32) * cfg.polar_latitude_fraction.clamp(0.0, 0.5))
            .ceil()
            .clamp(1.0, (h / 2).max(1) as f32) as usize
    } else {
        0
    };
    for y in 0..h {
        if y < polar_rows || y >= h.saturating_sub(polar_rows) {
            continue;
        }
        for x in 0..w {
            let idx = y * w + x;
            if let Some(cell) = mask.get(idx) {
                let ratio = match cell.ty {
                    MountainType::Fold => (cell.strength as f32 / fold_band_width).clamp(0.0, 1.0),
                    MountainType::Fault => {
                        (cell.strength as f32 / FAULT_STRENGTH_SPAN).clamp(0.0, 1.0)
                    }
                    MountainType::Volcanic => {
                        (cell.strength as f32 / VOLCANIC_STRENGTH_SPAN).clamp(0.0, 1.0)
                    }
                    MountainType::Dome => continue,
                };
                mask.enforce_relief_floor(idx, 1.0 + ratio * gain);
            }
        }
    }
}

fn apply_plateau_microrelief(
    mask: &mut MountainMask,
    plateau_cells: &[usize],
    w: usize,
    h: usize,
    cfg: &crate::map_preset::MountainsConfig,
    seed: u64,
) {
    if plateau_cells.is_empty() || cfg.plateau_microrelief_strength <= f32::EPSILON {
        return;
    }
    let micro_strength = cfg.plateau_microrelief_strength.clamp(0.0, 2.0);
    let rim_width = cfg.plateau_rim_width.max(1) as usize;
    let variance = cfg.plateau_terrace_variance.clamp(0.0, 1.0);
    let mut is_plateau = vec![false; w * h];
    for &idx in plateau_cells {
        if matches!(
            mask.get(idx),
            Some(MountainCell {
                ty: MountainType::Dome,
                ..
            })
        ) {
            is_plateau[idx] = true;
        }
    }
    let mut visited = vec![false; w * h];
    let mut distance = vec![u16::MAX; w * h];
    let mut cluster_count = 0usize;
    let mut rim_cells = 0usize;
    let mut terrace_cells = 0usize;
    let mut cluster_q = VecDeque::new();
    let mut rim_q = VecDeque::new();

    for &start in plateau_cells {
        if !is_plateau[start] || visited[start] {
            continue;
        }
        cluster_count += 1;
        cluster_q.push_back(start);
        visited[start] = true;
        let mut cluster_members = Vec::new();
        while let Some(idx) = cluster_q.pop_front() {
            cluster_members.push(idx);
            let x = idx % w;
            let y = idx / w;
            for (nx, ny) in neighbors4(x, y, w, h) {
                let nidx = ny * w + nx;
                if is_plateau[nidx] && !visited[nidx] {
                    visited[nidx] = true;
                    cluster_q.push_back(nidx);
                }
            }
        }

        rim_q.clear();
        for &idx in &cluster_members {
            let x = idx % w;
            let y = idx / w;
            let mut edge = false;
            for (nx, ny) in neighbors4(x, y, w, h) {
                if !is_plateau[ny * w + nx] {
                    edge = true;
                    break;
                }
            }
            if edge {
                distance[idx] = 0;
                rim_q.push_back(idx);
            }
        }

        while let Some(idx) = rim_q.pop_front() {
            let current_dist = distance[idx];
            if current_dist as usize >= rim_width {
                continue;
            }
            let x = idx % w;
            let y = idx / w;
            for (nx, ny) in neighbors4(x, y, w, h) {
                let nidx = ny * w + nx;
                if !is_plateau[nidx] {
                    continue;
                }
                if distance[nidx] > current_dist + 1 {
                    distance[nidx] = current_dist + 1;
                    rim_q.push_back(nidx);
                }
            }
        }
    }

    let rim_width_f = rim_width as f32;
    let variance_scale = (variance * micro_strength * PLATEAU_VARIANCE_FACTOR).clamp(0.0, 1.0);
    let base_interior =
        (1.0 - micro_strength * PLATEAU_INTERIOR_FACTOR).clamp(MIN_RELIEF_SCALE, 1.0);
    let noise_seed = seed ^ 0xA99D_13E7_9925u64;

    for &idx in plateau_cells {
        if !is_plateau[idx] {
            continue;
        }
        let d = distance[idx];
        let x = idx % w;
        let y = idx / w;
        let Some(cell) = mask.get(idx) else {
            continue;
        };
        if d != u16::MAX && d as usize <= rim_width {
            let factor = 1.0 - (d as f32 / (rim_width_f + 0.5));
            let rim_relief =
                (1.0 + micro_strength * factor).clamp(MIN_RELIEF_SCALE, MAX_RELIEF_SCALE);
            mask.set_relief_scale(idx, rim_relief);
            let boosted = ((cell.strength as f32)
                + micro_strength * PLATEAU_RIM_BOOST_FACTOR * factor)
                .round()
                .clamp(cell.strength as f32, MAX_CELL_STRENGTH) as u8;
            mask.set(
                idx,
                MountainCell {
                    ty: MountainType::Dome,
                    strength: boosted.max(cell.strength),
                },
            );
            rim_cells += 1;
        } else {
            let noise = terrain_hash(noise_seed, x as u32, y as u32);
            let sample = (noise & 0xFFFF) as f32 / 65535.0;
            let variation = (sample - 0.5) * 2.0 * variance_scale;
            let terrace_relief =
                (base_interior + variation).clamp(MIN_RELIEF_SCALE, MAX_RELIEF_SCALE);
            mask.set_relief_scale(idx, terrace_relief);
            terrace_cells += 1;
        }
    }

    tracing::info!(
        target: "shadow_scale::mapgen",
        plateau_clusters = cluster_count,
        plateau_cells = plateau_cells.len(),
        rim_cells,
        terrace_cells,
        rim_width,
        microrelief_strength = micro_strength,
        variance,
        "mapgen.tectonics.plateau_microrelief"
    );
}

fn prevailing_wind_for_row(y: usize, height: usize, cfg: &BiomeTransitionConfig, seed: u64) -> i32 {
    if height == 0 {
        return 1;
    }
    let lat = if height <= 1 {
        0.5
    } else {
        y as f32 / (height.saturating_sub(1) as f32)
    };
    let dist_equator = (lat - 0.5).abs();
    let mut dir = if dist_equator < cfg.trade_wind_band {
        -1
    } else {
        1
    };
    let hash = terrain_hash(seed ^ 0xACED_D00Du64, y as u32, height as u32);
    let roll = (hash & 0xFFFF) as f32 / 65535.0;
    if roll < cfg.prevailing_wind_flip_chance.clamp(0.0, 1.0) {
        dir *= -1;
    }
    if dir == 0 {
        1
    } else {
        dir
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_moisture_field(
    land: &[bool],
    coastal_land: &[bool],
    land_distance: &[u32],
    mountains: &MountainMask,
    elevation: &ElevationField,
    width: usize,
    height: usize,
    moisture_scale: f32,
    cfg: &BiomeTransitionConfig,
    seed: u64,
) -> Vec<f32> {
    let mut values = vec![0.0f32; width * height];
    if width == 0 || height == 0 {
        return values;
    }
    for y in 0..height {
        let direction = prevailing_wind_for_row(y, height, cfg, seed);
        let lat = if height <= 1 {
            0.5
        } else {
            y as f32 / (height.saturating_sub(1) as f32)
        };
        let dist_equator = (lat - 0.5).abs();
        let latitude_bonus = (1.0 - (dist_equator * cfg.latitude_dryness_falloff).min(1.0))
            * cfg.latitude_humidity_weight.clamp(0.0, 1.0);

        let iter: Box<dyn Iterator<Item = usize>> = if direction >= 0 {
            Box::new(0..width)
        } else {
            Box::new((0..width).rev())
        };
        let mut shadow = 0.0f32;
        let mut carry = 0.0f32;
        for x in iter {
            let idx = y * width + x;
            if !land.get(idx).copied().unwrap_or(false) {
                values[idx] = 1.0;
                shadow = 0.0;
                carry = 1.0;
                continue;
            }

            let distance = land_distance.get(idx).copied().unwrap_or(0) as f32;
            let coastal_flag = coastal_land.get(idx).copied().unwrap_or(false);
            let base_coastal = if coastal_flag {
                cfg.coastal_bonus_scale
            } else {
                (-distance / cfg.coastal_rainfall_decay.max(0.1)).exp() * cfg.coastal_bonus_scale
            };

            let base_humidity = cfg.base_humidity_weight + latitude_bonus + base_coastal + carry;
            let mut humidity = base_humidity - shadow;

            shadow *= 1.0 - cfg.rain_shadow_decay.clamp(0.0, 0.95);
            if shadow < 1e-4 {
                shadow = 0.0;
            }

            if let Some(cell) = mountains.get(idx) {
                let relief = mountains.relief_scale(idx).max(0.0);
                humidity += cfg.windward_moisture_bonus * relief;
                let added_shadow = cfg.rain_shadow_strength.max(0.0) * relief;
                shadow = (shadow + added_shadow).clamp(0.0, 2.0);
                if matches!(cell.ty, MountainType::Volcanic) {
                    humidity -= added_shadow * VOLCANIC_HUMIDITY_SUPPRESSION;
                }
                carry = (carry + cfg.windward_moisture_bonus * 0.5).clamp(0.0, 1.2);
            }

            let interior_penalty = cfg.interior_aridity_strength
                * (distance / (distance + cfg.interior_aridity_distance)).min(1.0)
                * (1.0 - latitude_bonus.clamp(0.0, 1.0));
            humidity -= interior_penalty;

            let elev = elevation.sample(x as u32, y as u32);
            humidity += (elev - 0.5) * cfg.elevation_humidity_weight;

            humidity = humidity * cfg.humidity_scale + cfg.humidity_bias;
            humidity = (humidity * moisture_scale).clamp(0.0, 1.0);
            if humidity.is_nan() {
                humidity = 0.0;
            }
            values[idx] = humidity;

            carry = (carry + base_coastal * 0.25).clamp(0.0, 1.2);
            carry *= 1.0 - 0.25 * cfg.interior_aridity_strength.clamp(0.0, 0.95);
            if carry < 1e-4 {
                carry = 0.0;
            }
        }
    }

    values
}

#[allow(clippy::too_many_arguments)]
fn restamp_elevation(
    land: &[bool],
    is_ocean: &[bool],
    land_distance: &[u32],
    mountains: &MountainMask,
    base_elevation: &ElevationField,
    cfg: &crate::map_preset::MountainsConfig,
    ocean_cfg: &OceanConfig,
    sea_level: f32,
    seed: u64,
) -> ElevationField {
    let w = base_elevation.width as usize;
    let h = base_elevation.height as usize;
    let idx = |x: usize, y: usize| -> usize { y * w + x };

    let fold_band_width = mountains.fold_band_width() as f32 + 1.0;

    // Tie the elevation field to the (relief-based) biome so mountains are genuinely
    // tall: lowlands compress into [sea_level, elevation_base] and every mountain tile
    // is floored above elevation_base, ordered by relief and per-type prominence.
    let sea_level = sea_level.clamp(0.0, 0.999);
    let elevation_base = cfg.elevation_base.clamp(sea_level, 1.0);
    let relief_span = (MAX_RELIEF_SCALE - MIN_RELIEF_SCALE).max(f32::EPSILON);

    // A mountain tile's elevation is a monotonic function of its relief (the same
    // signal that picks its biome), so the field and the biome always agree. Kept as a
    // closure because it is re-applied after coastal smoothing (which would otherwise
    // drag coast-adjacent peaks down toward the ocean).
    let mountain_floor = |ty: MountainType, relief: f32| -> f32 {
        let prominence = match ty {
            MountainType::Fold => cfg.fold_prominence,
            MountainType::Fault => cfg.fault_prominence,
            MountainType::Volcanic => cfg.volcanic_prominence,
            MountainType::Dome => cfg.dome_prominence,
        };
        let relief_norm = ((relief - MIN_RELIEF_SCALE) / relief_span).clamp(0.0, 1.0);
        // Clamp to 1.0 so the invariant floor ∈ [elevation_base, 1.0] holds even when a
        // preset sets a *_prominence > 1.0 — the post-smoothing re-floor uses this floor
        // directly (unclamped .max), so an out-of-range floor would leak elevation
        // samples outside the normalized 0..1 contract.
        (elevation_base + relief_norm * (1.0 - elevation_base) * prominence.max(0.0)).min(1.0)
    };

    let mut values = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let base = base_elevation.sample(x as u32, y as u32);
            let i = idx(x, y);
            let mut v = base;

            if land[i] {
                if let Some(cell) = mountains.get(i) {
                    let relief = mountains.relief_scale(i);
                    // Belt position adds a small texture (peaks slightly taller at the
                    // range spine) that is bounded so it can never lift a low-relief
                    // tile above a higher-relief one — relief still determines ordering.
                    let belt_ratio = match cell.ty {
                        MountainType::Fold => {
                            (cell.strength as f32 / fold_band_width).clamp(0.0, 1.0)
                        }
                        MountainType::Fault => {
                            (cell.strength as f32 / FAULT_STRENGTH_SPAN).clamp(0.0, 1.0)
                        }
                        MountainType::Volcanic => {
                            (cell.strength as f32 / VOLCANIC_STRENGTH_SPAN).clamp(0.0, 1.0)
                        }
                        // Domes have no belt spine, so they get no spine
                        // texture — they read as flat plateaus floored at
                        // dome_prominence, matching apply_belt_relief which
                        // also skips domes.
                        MountainType::Dome => 0.0,
                    };
                    let floor = mountain_floor(cell.ty, relief);
                    v = (floor + belt_ratio * cfg.belt_texture.max(0.0)).clamp(0.0, 1.0);
                } else {
                    // Non-mountain land: compress into [sea_level, elevation_base] so
                    // lowlands (plains, etc.) never out-top the mountains.
                    let above_sea = ((v - sea_level) / (1.0 - sea_level)).clamp(0.0, 1.0);
                    v = sea_level + above_sea * (elevation_base - sea_level);
                }
            } else if is_ocean[i]
                && ocean_cfg.ridge_density > 0.0
                && ocean_cfg.ridge_amplitude.abs() > f32::EPSILON
            {
                let hash = terrain_hash(seed, x as u32, y as u32);
                let sample = (hash & 0xFFFF) as f32 / 65535.0;
                if sample < ocean_cfg.ridge_density {
                    // Capped AT sea level: a mid-ocean ridge is bathymetry, not an island. A
                    // positive `ridge_amplitude` on a shallow cell would otherwise lift an
                    // `is_ocean` tile above sea level — the exact state this arc makes
                    // unrepresentable in the mask, reintroduced one stage later in the field.
                    v = (v + ocean_cfg.ridge_amplitude * (1.0 - sample / ocean_cfg.ridge_density))
                        .min(sea_level);
                }
            }

            values.push(v.clamp(0.0, 1.0));
        }
    }

    apply_coastal_smoothing(&mut values, w, h, land, land_distance);

    // Coastal smoothing blends land toward its (ocean-inclusive) neighborhood, which
    // drags coast-adjacent mountains down toward the sea and lifts mountain-adjacent
    // plains up. Re-assert the hard band boundary so mountains stay at/above
    // elevation_base and lowlands stay at/below it, with no overlap.
    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            if !land[i] {
                continue;
            }
            if let Some(cell) = mountains.get(i) {
                let floor = mountain_floor(cell.ty, mountains.relief_scale(i));
                values[i] = values[i].max(floor);
            } else {
                // Floored AT sea level as well as capped at `elevation_base`. `apply_coastal_smoothing`
                // blends a coastal land tile toward an ocean-inclusive 3x3 mean, which can drag it
                // *below* sea level — the other half of the original defect (land under water). The
                // floor is `sea_level` itself, not `sea_level + eps`: a tile sitting exactly on the
                // coastline contour is a real, meaningful value, and inventing an epsilon offset just
                // to satisfy a strict inequality would be a magic number standing in for a fact.
                values[i] = values[i].clamp(sea_level, elevation_base);
            }
        }
    }

    // Propagate the input field's sea level: a restamp reshapes the surface, it does not redefine
    // where the sea is. Without this the restamped field silently reverts to `DEFAULT_SEA_LEVEL`,
    // and any consumer that inserts it as a resource without re-attaching (there was one, in
    // `spawn_initial_world`) publishes a coastline at the wrong height.
    ElevationField::new(base_elevation.width, base_elevation.height, values)
        .with_sea_level(base_elevation.sea_level)
}

fn apply_coastal_smoothing(
    values: &mut [f32],
    width: usize,
    height: usize,
    land: &[bool],
    land_distance: &[u32],
) {
    if values.is_empty() {
        return;
    }

    let mut blurred = vec![0.0f32; values.len()];
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0f32;
            let mut count = 0usize;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                        continue;
                    }
                    let idx = ny as usize * width + nx as usize;
                    sum += values[idx];
                    count += 1;
                }
            }
            let idx = y * width + x;
            blurred[idx] = if count > 0 {
                sum / count as f32
            } else {
                values[idx]
            };
        }
    }

    for idx in 0..values.len() {
        if !land[idx] {
            continue;
        }
        let distance = land_distance[idx] as usize;
        let weight = COASTAL_BLUR_WEIGHTS.get(distance).copied().unwrap_or(0.0);
        if weight <= 0.0 {
            continue;
        }
        let blended = values[idx] * (1.0 - weight) + blurred[idx] * weight;
        values[idx] = blended.clamp(0.0, 1.0);
    }
}

pub fn validate_bands(bands: &BandsResult, grid: UVec2) {
    let w = grid.x as usize;
    let h = grid.y as usize;
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut ok_shelf_distance = true;
    let mut ok_inland_shelf = true;
    let mut detached_shelf = 0u32;
    let mut c_land = 0usize;
    let mut c_shelf = 0usize;
    let mut c_slope = 0usize;
    let mut c_abyss = 0usize;
    let mut c_inland = 0usize;
    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            match bands.terrain[i] {
                TerrainBand::ContinentalShelf => {
                    if bands.ocean_distance[i] == u32::MAX || bands.ocean_distance[i] == 0 {
                        ok_shelf_distance = false;
                    }
                    let mut near_land = false;
                    for (nx, ny) in neighbors4(x, y, w, h) {
                        if matches!(bands.terrain[idx(nx, ny)], TerrainBand::Land) {
                            near_land = true;
                            break;
                        }
                    }
                    if !near_land {
                        detached_shelf += 1;
                    }
                }
                TerrainBand::InlandSea => {
                    c_inland += 1;
                    for (nx, ny) in neighbors4(x, y, w, h) {
                        if matches!(bands.terrain[idx(nx, ny)], TerrainBand::DeepOcean) {
                            ok_inland_shelf = false;
                        }
                    }
                }
                TerrainBand::Land => {
                    c_land += 1;
                }
                TerrainBand::ContinentalSlope => {
                    c_slope += 1;
                }
                TerrainBand::DeepOcean => {
                    c_abyss += 1;
                }
            }
            if matches!(bands.terrain[i], TerrainBand::ContinentalShelf) {
                c_shelf += 1;
            }
        }
    }
    let (fold_count, fault_count, volcanic_count, dome_count) = bands.mountains.iter_counts();
    tracing::info!(
        target: "shadow_scale::mapgen",
        ok_shelf_distance,
        ok_inland_shelf,
        detached_shelf,
        land = c_land,
        shelf = c_shelf,
        slope = c_slope,
        abyss = c_abyss,
        inland = c_inland,
        land_ratio = (c_land as f32) / ((w * h) as f32),
        fold_mountains = fold_count,
        fault_mountains = fault_count,
        volcanic_mountains = volcanic_count,
        dome_mountains = dome_count,
        "mapgen.validate.coastal_bands"
    );
}

fn terrain_hash(seed: u64, x: u32, y: u32) -> u32 {
    let seed_low = seed as u32;
    let seed_high = (seed >> 32) as u32;
    let mut n = x.wrapping_add(seed_low.rotate_left(7));
    n = n.wrapping_mul(0x6C8E_9CF5) ^ y.wrapping_mul(0xB529_7A4D) ^ seed_high;
    n ^= n >> 13;
    n = n.wrapping_mul(0x68E3_1DA4 ^ seed_low.rotate_left(11));
    n ^= n >> 11;
    n = n.wrapping_mul(0x1B56_C4E9 ^ seed_high.rotate_left(3));
    n ^ (n >> 16)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::{
        heightfield::build_elevation_field,
        map_preset::{MapPreset, MapPresets},
        resources::SimulationConfig,
    };
    use bevy::math::UVec2;

    #[derive(Debug)]
    struct RegressionMetrics {
        land_ratio: f32,
        fold: usize,
        fault: usize,
        volcanic: usize,
        dome: usize,
        polar_fold: usize,
        polar_fault: usize,
        polar_uplift_cells: usize,
        polar_relief_cells: usize,
    }

    fn preset_seed(preset: &MapPreset, override_seed: Option<u64>) -> u64 {
        override_seed
            .or(preset.map_seed)
            .unwrap_or(0xC0DE_5EED_CAFEu64)
    }

    fn regression_metrics_for_preset(id: &str, override_seed: Option<u64>) -> RegressionMetrics {
        let presets = MapPresets::builtin();
        let preset = presets
            .get(id)
            .unwrap_or_else(|| panic!("missing preset {id}"));

        let seed = preset_seed(preset, override_seed);

        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(preset.dimensions.width, preset.dimensions.height);
        config.map_seed = seed;
        config.map_preset_id = preset.id.clone();

        let elevation = build_elevation_field(&config, Some(preset), seed);
        let bands = build_bands(
            &elevation,
            preset.sea_level,
            &preset.macro_land,
            &preset.shelf,
            &preset.islands,
            &preset.inland_sea,
            &preset.ocean,
            preset.moisture_scale,
            &preset.biomes,
            seed,
            preset.mountain_scale,
            &preset.mountains,
            false, // wrap_horizontal - test without wrap
        );

        compute_metrics(preset, &bands)
    }

    fn compute_metrics(preset: &MapPreset, bands: &BandsResult) -> RegressionMetrics {
        let total = (preset.dimensions.width * preset.dimensions.height) as usize;
        let land_tiles = bands.land_mask.iter().copied().filter(|&v| v).count();
        let (fold, fault, volcanic, dome) = bands.mountains.iter_counts();

        let width = preset.dimensions.width as usize;
        let height = preset.dimensions.height as usize;
        let polar_rows = ((preset.dimensions.height as f32)
            * preset.mountains.polar_latitude_fraction)
            .ceil()
            .clamp(1.0, preset.dimensions.height as f32) as usize;
        let uplift_floor = preset.mountains.polar_uplift_scale;
        let relief_cap = preset.mountains.polar_low_relief_scale;

        let mut polar_fold = 0usize;
        let mut polar_fault = 0usize;
        let mut polar_uplift_cells = 0usize;
        let mut polar_relief_cells = 0usize;

        for y in 0..height {
            if !(y < polar_rows || y >= height.saturating_sub(polar_rows)) {
                continue;
            }
            for x in 0..width {
                let idx = y * width + x;
                if let Some(cell) = bands.mountains.get(idx) {
                    match cell.ty {
                        MountainType::Fold => polar_fold += 1,
                        MountainType::Fault => polar_fault += 1,
                        MountainType::Volcanic | MountainType::Dome => {}
                    }
                    let relief = bands.mountains.relief_scale(idx);
                    if relief + 1e-4 >= uplift_floor {
                        polar_uplift_cells += 1;
                    }
                    if relief <= relief_cap + 1e-4 {
                        polar_relief_cells += 1;
                    }
                }
            }
        }

        RegressionMetrics {
            land_ratio: land_tiles as f32 / total as f32,
            fold,
            fault,
            volcanic,
            dome,
            polar_fold,
            polar_fault,
            polar_uplift_cells,
            polar_relief_cells,
        }
    }

    fn dummy_land_mask(width: usize, height: usize, fill: bool) -> Vec<bool> {
        vec![fill; width * height]
    }

    /// A shelf-width below 1.0 tile is floored up to a continuous 1-tile ring by
    /// `min_width_tiles` (default), replacing the old sub-tile sparse fringe.
    #[test]
    fn effective_shelf_width_floors_to_min_width_tiles() {
        // earthlike-style sub-tile width: 0.040 * 80^0.4 ≈ 0.23 tiles pre-floor.
        let shelf = ShelfConfig {
            width_frac: Some(0.040),
            width_exp: Some(0.4),
            min_width_tiles: 1.0,
            ..ShelfConfig::default()
        };
        let w = effective_shelf_width(&shelf, 128, 80);
        assert!(
            (w - 1.0).abs() < 1e-6,
            "sub-tile shelf width should floor to min_width_tiles (1.0), got {w}"
        );
        // A preset that bumps the coefficient past the floor still scales wider.
        let wide = ShelfConfig {
            width_frac: Some(0.5),
            width_exp: Some(1.0),
            min_width_tiles: 1.0,
            ..ShelfConfig::default()
        };
        assert!(
            effective_shelf_width(&wide, 128, 80) > 1.0,
            "a coefficient above the floor should still scale the shelf wider"
        );
    }

    /// The coast-height gate: on a 1-tile shelf, an ocean tile abutting a gently-rising
    /// (lowland) coast becomes ContinentalShelf, while one abutting a steep (mountain/
    /// cliff) coast does not — it collapses to slope/deep water at the edge.
    #[test]
    fn classify_bands_gates_shelf_on_coast_height() {
        // A 5-wide, 3-tall strip: column 0 is land (the coast), columns 1..5 ocean.
        // Row 0's coast land rises gently, row 2's coast land is a cliff; row 1 is a
        // no-land control row so the coast rows are isolated.
        let w = 5usize;
        let h = 3usize;
        let mut land = vec![false; w * h];
        land[0] = true; // (0,0) gentle coast
        land[2 * w] = true; // (0,2) cliff coast
        let is_ocean = land.iter().map(|&l| !l).collect::<Vec<_>>();
        let sea_level = 0.6f32;
        let mut elev = vec![0.0f32; w * h]; // ocean tiles at 0.0
        elev[0] = sea_level + 0.02; // gentle: rise 0.02 < threshold
        elev[2 * w] = sea_level + 0.30; // cliff: rise 0.30 >= threshold
        let elevation = ElevationField::new(w as u32, h as u32, elev);
        let ocean_distance = compute_ocean_distance_wrapped(&land, w, h, false);
        let shelf = ShelfConfig {
            width_frac: None,
            width_tiles: 0, // rely purely on the min_width_tiles floor → 1-tile ring
            slope_width_tiles: 3,
            min_width_tiles: 1.0,
            coast_height_threshold: 0.10,
            ..ShelfConfig::default()
        };
        let terrain = classify_bands(
            &land,
            &is_ocean,
            &ocean_distance,
            &shelf,
            &elevation,
            sea_level,
            w,
            h,
            false,
            0,
        );
        let at = |x: usize, y: usize| terrain[y * w + x];
        // Gentle coast (row 0): the immediately-adjacent ocean tile is shelf.
        assert_eq!(
            at(1, 0),
            TerrainBand::ContinentalShelf,
            "ocean abutting a gently-rising coast should be ContinentalShelf"
        );
        // Cliff coast (row 2): the immediately-adjacent ocean tile is NOT shelf.
        assert_ne!(
            at(1, 2),
            TerrainBand::ContinentalShelf,
            "ocean abutting a steep/cliff coast should not be ContinentalShelf"
        );
    }

    /// The shelf's coast-adjacency is HEX-exact (odd-r 6-neighbour), so an ocean tile whose ONLY
    /// coast contact is a hex-DIAGONAL land tile (never one of its 4 cardinal neighbours) still
    /// forms a shelf off a gentle coast — and stays deep off a steep one. The old 4-cardinal
    /// adjacency missed these diagonals, leaving DeepOcean directly against gentle land. Covers
    /// both row parities, since odd-r diagonal offsets differ by parity.
    #[test]
    fn classify_bands_shelf_covers_hex_diagonal_coast() {
        // A single land tile in an otherwise all-ocean grid, placed at a pure hex-DIAGONAL of the
        // probe ocean tile (verified NOT a 4-cardinal neighbour below), so only hex adjacency
        // links them. Returns the probe tile's band. `rise` is the land's normalized rise.
        fn probe(
            w: usize,
            h: usize,
            land_xy: (usize, usize),
            target_xy: (usize, usize),
            rise: f32,
        ) -> TerrainBand {
            let (lx, ly) = land_xy;
            let (tx, ty) = target_xy;
            // The land tile must be a hex-diagonal — reachable via hex adjacency but NOT among the
            // probe tile's 4 cardinal (square) neighbours; that is exactly what this test exercises.
            let cardinals = [
                (tx.wrapping_add(1), ty),
                (tx.wrapping_sub(1), ty),
                (tx, ty.wrapping_add(1)),
                (tx, ty.wrapping_sub(1)),
            ];
            assert!(
                !cardinals.contains(&land_xy),
                "land tile must be a hex-diagonal, not a 4-cardinal neighbour of the probe"
            );
            let mut land = vec![false; w * h];
            land[ly * w + lx] = true;
            let is_ocean = land.iter().map(|&l| !l).collect::<Vec<_>>();
            let sea_level = 0.6f32;
            let mut elev = vec![0.0f32; w * h];
            elev[ly * w + lx] = sea_level + rise;
            let elevation = ElevationField::new(w as u32, h as u32, elev);
            let ocean_distance = compute_ocean_distance_wrapped(&land, w, h, false);
            let shelf = ShelfConfig {
                width_frac: None,
                width_tiles: 0, // rely purely on the min_width_tiles floor → 1-tile ring
                slope_width_tiles: 3,
                min_width_tiles: 1.0,
                coast_height_threshold: 0.10,
                ..ShelfConfig::default()
            };
            let terrain = classify_bands(
                &land,
                &is_ocean,
                &ocean_distance,
                &shelf,
                &elevation,
                sea_level,
                w,
                h,
                false,
                0,
            );
            terrain[ty * w + tx]
        }

        let gentle = 0.02f32; // rise < coast_height_threshold
        let steep = 0.30f32; // rise >= coast_height_threshold

        // Even probe row (y = 2): NW hex-diagonal is (x-1, y-1) = (1, 1).
        assert_eq!(
            probe(6, 6, (1, 1), (2, 2), gentle),
            TerrainBand::ContinentalShelf,
            "even-row ocean touching a gentle coast only on a hex-diagonal should be shelf"
        );
        assert_ne!(
            probe(6, 6, (1, 1), (2, 2), steep),
            TerrainBand::ContinentalShelf,
            "even-row ocean touching only a steep hex-diagonal coast should not be shelf"
        );

        // Odd probe row (y = 3): NE hex-diagonal is (x+1, y-1) = (3, 2).
        assert_eq!(
            probe(6, 6, (3, 2), (2, 3), gentle),
            TerrainBand::ContinentalShelf,
            "odd-row ocean touching a gentle coast only on a hex-diagonal should be shelf"
        );
        assert_ne!(
            probe(6, 6, (3, 2), (2, 3), steep),
            TerrainBand::ContinentalShelf,
            "odd-row ocean touching only a steep hex-diagonal coast should not be shelf"
        );
    }

    /// Authoritative full-earthlike guard for the fix: over a REAL generated coastline,
    /// `classify_bands` leaves NO DeepOcean tile hex-adjacent (odd-r 6-neighbour, wrap-aware) to a
    /// GENTLE (below-threshold-rise) Land tile — every gentle coast carries a shelf on all six
    /// seaward hex-neighbours. Before the hex-aware fix the hex-diagonal coast directions fell
    /// through the 4-cardinal shelf ring and left deep water directly against gentle land. This is
    /// checked at the BAND level (on `classify_bands`' own `land_mask` + restamped elevation)
    /// because the post-worldgen snapshot additionally stamps river deltas / marsh / polar land
    /// against ocean in later, out-of-scope stages (hydrology + tag-budget solver), independently
    /// of the shelf ring — so the band level is where this shelf fix is provable.
    #[test]
    fn earthlike_bands_have_no_gentle_coast_shelf_gap() {
        let seeds = [0x0FA1_C0DEu64, 0x5EED_F00D, 0x0000_BEEF];
        let dims = [(80u32, 52u32), (128u32, 96u32)];
        let presets = MapPresets::builtin();
        let preset = presets.get("earthlike").expect("earthlike preset");
        let threshold = preset.shelf.coast_height_threshold;
        for &(w, h) in &dims {
            for &seed in &seeds {
                let mut config = SimulationConfig::builtin();
                config.grid_size = UVec2::new(w, h);
                config.map_seed = seed;
                config.map_preset_id = preset.id.clone();
                let elevation = build_elevation_field(&config, Some(preset), seed);
                // wrap_horizontal = true to mirror production earthlike (map_topology wraps on x),
                // so the seam is handled exactly as the shipped map + client render it.
                let bands = build_bands(
                    &elevation,
                    preset.sea_level,
                    &preset.macro_land,
                    &preset.shelf,
                    &preset.islands,
                    &preset.inland_sea,
                    &preset.ocean,
                    preset.moisture_scale,
                    &preset.biomes,
                    seed,
                    preset.mountain_scale,
                    &preset.mountains,
                    true,
                );
                let (wu, hu) = (w as usize, h as usize);
                let mut gaps = 0usize;
                for y in 0..hu {
                    for x in 0..wu {
                        if !matches!(bands.terrain[y * wu + x], TerrainBand::DeepOcean) {
                            continue;
                        }
                        for (nx, ny) in hex_neighbors_wrapped(x as u32, y as u32, w, h, true) {
                            if bands.land_mask[ny as usize * wu + nx as usize] {
                                let rise = bands.elevation.sample(nx, ny) - preset.sea_level;
                                if (0.0..threshold).contains(&rise) {
                                    gaps += 1;
                                }
                            }
                        }
                    }
                }
                assert_eq!(
                    gaps, 0,
                    "{w}x{h} seed={seed:016x}: {gaps} DeepOcean tiles are hex-adjacent to a gentle \
                     (rise < {threshold}) coast — the shelf coast-adjacency ring left a \
                     hex-diagonal gap"
                );
            }
        }
    }

    #[test]
    fn mountain_mask_counts_match_expectations() {
        let width = 8usize;
        let height = 8usize;
        let land = dummy_land_mask(width, height, true);
        let is_ocean = vec![false; width * height];
        let land_distance = vec![2u32; width * height];
        let elevation = ElevationField::new(width as u32, height as u32, vec![0.5; width * height]);
        let cfg = crate::map_preset::MountainsConfig {
            fault_line_count: 1,
            ..Default::default()
        };
        let mask = derive_mountain_mask(
            &land,
            &is_ocean,
            &land_distance,
            &elevation,
            &cfg,
            1.0,
            width,
            height,
            42,
        );
        let (fold, fault, volcanic, _) = mask.iter_counts();
        let total = fold + fault + volcanic;
        assert!(total > 0, "expected mountain features to be generated");
    }

    // Guards the elevation<->biome coupling: mountain-mask tiles (which become the
    // mountain biomes) must read clearly higher on the elevation field than
    // non-mountain lowland tiles. Regression test for the historical decoupling where
    // AlpineMountain tiles could sit near the field minimum while plains hit the max.
    #[test]
    fn mountain_tiles_out_top_lowland_tiles() {
        use bevy::math::UVec2;

        let presets = MapPresets::builtin();
        let preset = presets.get("earthlike").expect("earthlike preset");
        let seed = preset_seed(preset, None);

        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(preset.dimensions.width, preset.dimensions.height);
        config.map_seed = seed;
        config.map_preset_id = preset.id.clone();

        let elevation = build_elevation_field(&config, Some(preset), seed);
        let bands = build_bands(
            &elevation,
            preset.sea_level,
            &preset.macro_land,
            &preset.shelf,
            &preset.islands,
            &preset.inland_sea,
            &preset.ocean,
            preset.moisture_scale,
            &preset.biomes,
            seed,
            preset.mountain_scale,
            &preset.mountains,
            false, // wrap_horizontal - test without wrap
        );

        let width = preset.dimensions.width as usize;
        let height = preset.dimensions.height as usize;
        let (mut mountain_sum, mut mountain_n) = (0.0f64, 0usize);
        let (mut lowland_sum, mut lowland_n) = (0.0f64, 0usize);
        let mut mountain_min = f64::MAX;
        let mut lowland_max = f64::MIN;
        // Track mountains that border ocean specifically — the coast case that used to
        // get dragged to ~0 by coastal smoothing before the post-smoothing re-floor.
        let mut coastal_mountain_min = f64::MAX;
        let mut coastal_mountain_n = 0usize;
        let is_ocean = |x: usize, y: usize| bands.terrain[y * width + x] != TerrainBand::Land;
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if bands.terrain[idx] != TerrainBand::Land {
                    continue;
                }
                let elev = bands.elevation.sample(x as u32, y as u32) as f64;
                if bands.mountains.get(idx).is_some() {
                    mountain_sum += elev;
                    mountain_n += 1;
                    mountain_min = mountain_min.min(elev);
                    let borders_ocean =
                        [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                            .iter()
                            .any(|&(dx, dy)| {
                                let nx = x as i32 + dx;
                                let ny = y as i32 + dy;
                                nx >= 0
                                    && ny >= 0
                                    && (nx as usize) < width
                                    && (ny as usize) < height
                                    && is_ocean(nx as usize, ny as usize)
                            });
                    if borders_ocean {
                        coastal_mountain_min = coastal_mountain_min.min(elev);
                        coastal_mountain_n += 1;
                    }
                } else {
                    lowland_sum += elev;
                    lowland_n += 1;
                    lowland_max = lowland_max.max(elev);
                }
            }
        }

        assert!(
            mountain_n > 0 && lowland_n > 0,
            "expected both mountain and lowland land tiles (mountain={mountain_n}, lowland={lowland_n})"
        );
        let mountain_mean = mountain_sum / mountain_n as f64;
        let lowland_mean = lowland_sum / lowland_n as f64;
        let elevation_base = preset.mountains.elevation_base as f64;

        assert!(
            mountain_mean > lowland_mean + 0.15,
            "mountain mean elevation {mountain_mean:.3} should clearly exceed lowland mean {lowland_mean:.3}"
        );
        // The post-smoothing re-floor guarantees EVERY mountain tile sits at/above
        // elevation_base, and lowland compression keeps every plain at/below it — a hard
        // separation with no overlap. (Small epsilon for f32→f64 rounding.)
        let eps = 1e-4;
        assert!(
            mountain_min >= elevation_base - eps,
            "lowest mountain tile {mountain_min:.3} must stay at/above elevation_base {elevation_base:.3}"
        );
        assert!(
            lowland_max <= elevation_base + eps,
            "highest lowland tile {lowland_max:.3} must stay at/below elevation_base {elevation_base:.3}"
        );
        // The reported regression: mountains next to water must not collapse to ~0.
        assert!(
            coastal_mountain_n > 0,
            "expected some ocean-bordering mountain tiles to exercise the coastal case"
        );
        assert!(
            coastal_mountain_min >= elevation_base - eps,
            "lowest coast-bordering mountain {coastal_mountain_min:.3} must stay at/above elevation_base {elevation_base:.3}"
        );
    }

    // Guards against the base classifier re-introducing mask-less "mountains": every
    // tile whose FINAL biome is AlpineMountain must sit on genuinely high ground (it can
    // only come from a mountain-mask cell with relief >= 1.45, floored well above
    // elevation_base). Before the classify_terrain fix these could sit near sea level.
    #[test]
    fn alpine_biome_tiles_are_tall() {
        use crate::terrain::terrain_for_position_with_classifier;
        use bevy::math::UVec2;
        use sim_runtime::TerrainType;

        let presets = MapPresets::builtin();
        let preset = presets.get("earthlike").expect("earthlike preset");
        let seed = preset_seed(preset, None);

        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(preset.dimensions.width, preset.dimensions.height);
        config.map_seed = seed;
        config.map_preset_id = preset.id.clone();

        let elevation = build_elevation_field(&config, Some(preset), seed);
        let bands = build_bands(
            &elevation,
            preset.sea_level,
            &preset.macro_land,
            &preset.shelf,
            &preset.islands,
            &preset.inland_sea,
            &preset.ocean,
            preset.moisture_scale,
            &preset.biomes,
            seed,
            preset.mountain_scale,
            &preset.mountains,
            false,
        );

        let width = preset.dimensions.width as usize;
        let height = preset.dimensions.height as usize;
        let grid = UVec2::new(preset.dimensions.width, preset.dimensions.height);
        // These biomes are produced ONLY by the tectonic mountain mask
        // (select_mountain_terrain), never by the base climate classifier, so every one
        // must sit on floored (tall) ground. Before the fix the base classifier's fake
        // noise-elevation stamped them on flat lowland tiles.
        let mask_only_peaks = [
            TerrainType::AlpineMountain,
            TerrainType::HighPlateau,
            TerrainType::KarstHighland,
        ];
        let (mut peak_min, mut peak_n) = (f64::MAX, 0usize);
        let mut alpine_n = 0usize;
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if bands.terrain[idx] != TerrainBand::Land {
                    continue;
                }
                let position = UVec2::new(x as u32, y as u32);
                let relief = bands.mountains.relief_scale(idx);
                let (terrain, _tags) = terrain_for_position_with_classifier(
                    position,
                    grid,
                    bands.moisture.get(idx).copied(),
                    Some(bands.elevation.sample(position.x, position.y)),
                    bands.mountains.get(idx).map(|cell| (cell.ty, relief)),
                    &preset.terrain_classifier,
                    // This fixture isolates the RELIEF branch: a cold band would divert tall
                    // tiles to Glacier/SeasonalSnowfield before the alpine branch is reached,
                    // which is a climate assertion, not the mask assertion under test here.
                    crate::climate::ClimateBand::Temperate,
                );
                if terrain == TerrainType::AlpineMountain {
                    alpine_n += 1;
                }
                if mask_only_peaks.contains(&terrain) {
                    peak_min = peak_min.min(bands.elevation.sample(x as u32, y as u32) as f64);
                    peak_n += 1;
                }
            }
        }

        assert!(peak_n > 0, "expected some mask-based peak biomes");
        // The belt-relief boost (relief_belt_gain) must lift belt cores past the Alpine
        // threshold, so AlpineMountain genuinely appears (not fabricated by noise).
        assert!(
            alpine_n > 0,
            "expected AlpineMountain tiles from high-relief belt cores"
        );
        let elevation_base = preset.mountains.elevation_base as f64;
        assert!(
            peak_min >= elevation_base - 1e-4,
            "lowest mask-peak biome tile {peak_min:.3} should sit at/above elevation_base {elevation_base:.3} (no mask-less mountains from the base classifier)"
        );
    }

    #[test]
    fn moisture_field_respects_orographic_shadow() {
        let width = 5usize;
        let height = 3usize;
        let total = width * height;
        let mut land = vec![true; total];
        // leave a single ocean column on the eastern edge (wind blows east -> west)
        for y in 0..height {
            land[y * width + (width - 1)] = false;
        }
        let is_ocean = compute_ocean_mask(&land, width, height);
        let land_distance = compute_land_distance(&land, width, height);
        let coastal_land = compute_coastal_land(&land, &is_ocean, width, height);

        let mut mask = MountainMask::new(width, height, 3);
        let mid_idx = width + 2;
        mask.set_for_tests(
            mid_idx,
            MountainCell {
                ty: MountainType::Fold,
                strength: 9,
            },
            1.5,
        );

        let elevation = ElevationField::new(width as u32, height as u32, vec![0.75; total]);
        let biome_cfg = crate::map_preset::BiomeTransitionConfig {
            prevailing_wind_flip_chance: 0.0,
            base_humidity_weight: 0.35,
            latitude_humidity_weight: 0.2,
            windward_moisture_bonus: 0.3,
            rain_shadow_strength: 0.4,
            rain_shadow_decay: 0.15,
            coastal_bonus_scale: 0.6,
            humidity_scale: 0.9,
            ..Default::default()
        };

        let seed = 0xC0FFEE;
        let moisture = compute_moisture_field(
            &land,
            &coastal_land,
            &land_distance,
            &mask,
            &elevation,
            width,
            height,
            0.85,
            &biome_cfg,
            seed,
        );

        let wind_dir = prevailing_wind_for_row(1, height, &biome_cfg, seed);
        let upwind_idx = if wind_dir >= 0 {
            mid_idx.saturating_sub(1)
        } else {
            (mid_idx + 1).min(total - 1)
        };
        let downwind_idx = if wind_dir >= 0 {
            (mid_idx + 1).min(total - 1)
        } else {
            mid_idx.saturating_sub(1)
        };

        assert!(moisture[mid_idx] >= 0.0 && moisture[mid_idx] <= 1.0);
        assert!(moisture[upwind_idx] >= 0.0 && moisture[upwind_idx] <= 1.0);
        assert!(moisture[downwind_idx] >= 0.0 && moisture[downwind_idx] <= 1.0);
        assert!(moisture[mid_idx] + 0.01 >= moisture[upwind_idx]);
        assert!(moisture[downwind_idx] + 0.02 < moisture[mid_idx]);
        assert!(moisture[downwind_idx] + 0.02 < moisture[upwind_idx]);
    }

    #[test]
    fn polar_microplate_smoke_test() {
        let width = 12usize;
        let height = 12usize;
        let mut land = vec![false; width * height];
        for y in 0..height {
            for x in 0..width {
                if y < 3 || y >= height - 3 {
                    land[y * width + x] = true;
                }
            }
        }
        let is_ocean = vec![false; width * height];
        let land_distance = vec![1u32; width * height];
        let elevation = ElevationField::new(width as u32, height as u32, vec![0.6; width * height]);
        let cfg = crate::map_preset::MountainsConfig {
            polar_microplate_density: 0.01,
            polar_latitude_fraction: 0.25,
            fault_line_count: 1,
            ..Default::default()
        };

        let mask = derive_mountain_mask(
            &land,
            &is_ocean,
            &land_distance,
            &elevation,
            &cfg,
            1.0,
            width,
            height,
            123,
        );
        let (fold, _, _, _) = mask.iter_counts();
        assert!(
            fold >= 6,
            "expected fold belts from polar microplates (got {fold})"
        );
    }

    /// Grid the `polar_contrast` band fixture runs at — small next to a shipped 256x192 preset,
    /// but large enough to hold two landmasses that each clear the plate-splitting area threshold.
    const POLAR_FIXTURE_WIDTH: usize = 64;
    const POLAR_FIXTURE_HEIGHT: usize = 44;
    /// Ocean floor the fixture starts from, well below `polar_contrast`'s 0.6 sea level, so every
    /// tile is water until a peak lifts it.
    const POLAR_FIXTURE_BASE_DEPTH: f32 = 0.2;
    /// Elevation added at a peak's centre. `BASE_DEPTH + PEAK_HEIGHT` is the fixture's summit.
    const POLAR_FIXTURE_PEAK_HEIGHT: f32 = 0.8;
    /// Radius, in tiles, over which a peak falls linearly back to the ocean floor. Only the inner
    /// half of the cone clears sea level, so this yields a ~609-tile landmass per peak.
    const POLAR_FIXTURE_PEAK_RADIUS: f32 = 28.0;
    /// Centres of the fixture's two landmasses, separated so their above-sea discs stay disjoint
    /// with open ocean between them.
    const POLAR_FIXTURE_PEAK_CENTERS: [(usize, usize); 2] = [(15, 22), (49, 22)];

    /// Synthetic elevation for the `polar_contrast` band test: an all-ocean plane with two
    /// well-separated radial cones poking above sea level.
    ///
    /// The structure here is load-bearing, not decorative. `derive_mountain_mask` derives tectonic
    /// plates from the land mask's *connected components*, splitting each component into 1-4 plates
    /// purely by area (`mountains.plate_area_bucket_*`), and fold belts form only along a boundary
    /// between two plates whose drift converges. So a fixture earns `fold > 0` only if it contains
    /// a landmass big enough to be split into **two or more plates** — under
    /// `plate_area_bucket_2` (192 tiles for this preset) every component is a single plate, there
    /// are no plate boundaries anywhere on the map, and no fold belt can exist.
    ///
    /// The previous fixture was a pure linear diagonal plane (`((x + y) / (w + h)).fract()`, which
    /// never wraps at 48x32), yielding exactly one small triangular corner of land. It passed only
    /// because stray island placement occasionally coined an extra component. Both peaks here clear
    /// the threshold with wide margin, and the mountain counts stay positive with island placement
    /// disabled — so the assertions below are structural rather than an island artifact.
    ///
    /// If you retune this field, keep the landmasses above the plate-splitting threshold.
    fn polar_fixture_elevation() -> ElevationField {
        let mut values = Vec::with_capacity(POLAR_FIXTURE_WIDTH * POLAR_FIXTURE_HEIGHT);
        for y in 0..POLAR_FIXTURE_HEIGHT {
            for x in 0..POLAR_FIXTURE_WIDTH {
                let mut value = POLAR_FIXTURE_BASE_DEPTH;
                for (center_x, center_y) in POLAR_FIXTURE_PEAK_CENTERS {
                    let dx = x as f32 - center_x as f32;
                    let dy = y as f32 - center_y as f32;
                    let distance = dx.hypot(dy);
                    if distance < POLAR_FIXTURE_PEAK_RADIUS {
                        let falloff = 1.0 - distance / POLAR_FIXTURE_PEAK_RADIUS;
                        value = value
                            .max(POLAR_FIXTURE_BASE_DEPTH + POLAR_FIXTURE_PEAK_HEIGHT * falloff);
                    }
                }
                values.push(value);
            }
        }
        ElevationField::new(
            POLAR_FIXTURE_WIDTH as u32,
            POLAR_FIXTURE_HEIGHT as u32,
            values,
        )
    }

    #[test]
    fn polar_contrast_preset_builds_bands() {
        let presets = crate::map_preset::MapPresets::builtin();
        let preset = presets
            .get("polar_contrast")
            .expect("polar_contrast preset");
        let width = POLAR_FIXTURE_WIDTH;
        let height = POLAR_FIXTURE_HEIGHT;
        let elevation = polar_fixture_elevation();
        let seed = preset.map_seed.unwrap_or(99);
        let bands = build_bands(
            &elevation,
            preset.sea_level,
            &preset.macro_land,
            &preset.shelf,
            &preset.islands,
            &preset.inland_sea,
            &preset.ocean,
            preset.moisture_scale,
            &preset.biomes,
            seed,
            preset.mountain_scale,
            &preset.mountains,
            false, // wrap_horizontal - test without wrap
        );
        assert_eq!(bands.terrain.len(), width * height);
        assert!(bands.land_mask.iter().any(|&cell| cell));
        let (fold, fault, volcanic, _) = bands.mountains.iter_counts();
        assert!(fold > 0, "expected fold mountains");
        assert!(fault > 0, "expected fault mountains");
        assert!(volcanic > 0, "expected volcanic terrain");
    }

    /// Prints every regression metric for both shipped presets, so a deliberate re-pin can read all
    /// the new centres at once. The `*_regression_metrics_stable` assertions stop at the **first**
    /// drift, which otherwise turns a re-pin into a one-metric-per-run loop.
    #[test]
    #[ignore]
    fn print_regression_metrics() {
        println!(
            "earthlike: {:?}",
            regression_metrics_for_preset("earthlike", Some(0xE47E_51DE_2024u64))
        );
        println!(
            "polar_contrast: {:?}",
            regression_metrics_for_preset("polar_contrast", None)
        );
    }

    #[test]
    fn earthlike_regression_metrics_stable() {
        let metrics = regression_metrics_for_preset("earthlike", Some(0xE47E_51DE_2024u64));
        assert!(
            (metrics.land_ratio - 0.403).abs() <= 0.01,
            "earthlike land ratio drift: {}",
            metrics.land_ratio
        );
        assert!(
            (metrics.fold as isize - 2326).abs() <= 32,
            "earthlike fold count drift: {}",
            metrics.fold
        );
        assert!(
            (metrics.fault as isize - 254).abs() <= 16,
            "earthlike fault count drift: {}",
            metrics.fault
        );
        assert!(
            (metrics.volcanic as isize - 24).abs() <= 6,
            "earthlike volcanic count drift: {}",
            metrics.volcanic
        );
        assert!(
            (metrics.dome as isize - 707).abs() <= 32,
            "earthlike dome count drift: {}",
            metrics.dome
        );
        assert!(
            (metrics.polar_fold as isize - 613).abs() <= 32,
            "earthlike polar fold drift: {}",
            metrics.polar_fold
        );
        assert!(
            (metrics.polar_fault as isize - 157).abs() <= 16,
            "earthlike polar fault drift: {}",
            metrics.polar_fault
        );
        assert!(
            (metrics.polar_uplift_cells as isize - 136).abs() <= 20,
            "earthlike polar uplift cells drift: {}",
            metrics.polar_uplift_cells
        );
        assert!(
            (metrics.polar_relief_cells as isize - 12).abs() <= 10,
            "earthlike polar relief cap drift: {}",
            metrics.polar_relief_cells
        );
    }

    /// **Why a relief term can collapse a preset's fold belts** — the measurement that explained
    /// `polar_contrast`'s fold count falling 3556 -> 544 under the tilt (run with
    /// `-- --ignored --nocapture`).
    ///
    /// Fold belts form only at boundaries **between plates inside one land component**, and
    /// [`derive_mountain_mask`] buckets plate count by component area with a **cap of 4**. So the
    /// fold count tracks *how many multi-plate landmasses there are*, not how much land there is:
    /// five separate 1.5k-9k-tile bodies carry ~14 plates and a long boundary network, while one
    /// fused 18k-tile supercontinent carries **4 plates** and almost none.
    ///
    /// Measured, that is exactly what happened: the tilt fused `polar_contrast`'s five multi-plate
    /// components into **two** (largest 9053 -> 18313 tiles, i.e. 85% of all land in one body) and
    /// the folds went with them. It is the same land-bridging failure the tilt window was added to
    /// prevent, surviving the window on this preset.
    ///
    /// It also **refutes the obvious suspect**: `polar_contrast`'s much stricter
    /// `belt_convergence` (-0.1 vs earthlike's 0.25) is *not* the sensitivity. Under the dome the
    /// two gates give 3556 vs 3782 — 6% apart. The gate was never the mechanism; landmass fusion
    /// was.
    #[test]
    #[ignore]
    fn polar_contrast_fold_investigation() {
        /// (label, warp, tilt, spine)
        const RELIEFS: [(&str, f64, f64, f64); 5] = [
            ("dome", 0.0, 0.0, 0.0),
            ("warp-only", 0.18, 0.0, 0.0),
            ("spine-only", 0.0, 0.0, 0.35),
            ("tilt-on", 0.18, 2.0, 0.35),
            ("tilt-off (SHIPPED)", 0.18, 0.0, 0.35),
        ];

        fn plate_structure(
            land: &[bool],
            w: usize,
            h: usize,
            cfg: &crate::map_preset::MountainsConfig,
        ) -> String {
            let total = w * h;
            let mut comp = vec![usize::MAX; total];
            let mut sizes: Vec<usize> = Vec::new();
            for start in 0..total {
                if !land[start] || comp[start] != usize::MAX {
                    continue;
                }
                let id = sizes.len();
                let mut size = 0usize;
                let mut stack = vec![start];
                comp[start] = id;
                while let Some(idx) = stack.pop() {
                    size += 1;
                    let (x, y) = (idx % w, idx / w);
                    for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let n = ny as usize * w + nx as usize;
                        if land[n] && comp[n] == usize::MAX {
                            comp[n] = id;
                            stack.push(n);
                        }
                    }
                }
                sizes.push(size);
            }
            let plates_for = |area: usize| -> usize {
                let mut n = if area < cfg.plate_area_bucket_2 as usize {
                    1
                } else if area < cfg.plate_area_bucket_3 as usize {
                    2
                } else if area < cfg.plate_area_bucket_4 as usize {
                    3
                } else {
                    4
                };
                if n <= 1 && area >= cfg.plate_area_bump as usize {
                    n = 2;
                }
                n.min(area.max(1))
            };
            let plates: usize = sizes.iter().map(|&a| plates_for(a)).sum();
            let multi: usize = sizes.iter().filter(|&&a| plates_for(a) > 1).count();
            let in_multi: usize = sizes.iter().filter(|&&a| plates_for(a) > 1).sum();
            sizes.sort_unstable_by(|a, b| b.cmp(a));
            format!(
                "comps {:>4} | plates {:>4} | multi-plate comps {:>3} (land in them {:>6}) | largest {:?}",
                sizes.len(),
                plates,
                multi,
                in_multi,
                &sizes[..sizes.len().min(6)]
            )
        }

        fn report(id: &str, relief: (&str, f64, f64, f64), convergence: Option<f64>) {
            let mut file: serde_json::Value =
                serde_json::from_str(crate::map_preset::BUILTIN_MAP_PRESETS).expect("presets");
            for preset in file["presets"].as_array_mut().expect("presets").iter_mut() {
                let macro_land = preset["macro_land"].as_object_mut().expect("macro_land");
                macro_land.insert("continental_warp_amplitude".into(), relief.1.into());
                macro_land.insert("continental_tilt_strength".into(), relief.2.into());
                macro_land.insert("continental_spine_amplitude".into(), relief.3.into());
                if let Some(c) = convergence {
                    preset["mountains"]
                        .as_object_mut()
                        .expect("mountains")
                        .insert("belt_convergence".into(), c.into());
                }
            }
            let presets = MapPresets::from_json_str(&file.to_string()).expect("patched presets");
            let preset = presets.get(id).expect("preset");
            let seed = preset_seed(preset, None);

            let mut config = SimulationConfig::builtin();
            config.grid_size = UVec2::new(preset.dimensions.width, preset.dimensions.height);
            config.map_seed = seed;
            config.map_preset_id = preset.id.clone();

            let elevation = build_elevation_field(&config, Some(preset), seed);
            let bands = build_bands(
                &elevation,
                preset.sea_level,
                &preset.macro_land,
                &preset.shelf,
                &preset.islands,
                &preset.inland_sea,
                &preset.ocean,
                preset.moisture_scale,
                &preset.biomes,
                seed,
                preset.mountain_scale,
                &preset.mountains,
                false,
            );
            let (fold, fault, volcanic, dome) = bands.mountains.iter_counts();
            let land = bands.land_mask.iter().copied().filter(|&v| v).count();
            let (w, h) = (
                preset.dimensions.width as usize,
                preset.dimensions.height as usize,
            );
            println!(
                "  {:<20} gate {:>5} | fold {:>5} fault {:>4} volc {:>3} dome {:>5} | land {:.3}",
                relief.0,
                preset.mountains.belt_convergence,
                fold,
                fault,
                volcanic,
                dome,
                land as f64 / (w * h) as f64
            );
            println!(
                "      {}",
                plate_structure(&bands.land_mask, w, h, &preset.mountains)
            );
        }

        println!("=== polar_contrast @ shipped gate (belt_convergence -0.1) ===");
        for relief in RELIEFS {
            report("polar_contrast", relief, None);
        }
        println!("=== polar_contrast @ earthlike's gate (belt_convergence 0.25) ===");
        for relief in [RELIEFS[0], RELIEFS[4]] {
            report("polar_contrast", relief, Some(0.25));
        }
        println!("=== earthlike @ its own gate (0.25), for reference ===");
        for relief in [RELIEFS[0], RELIEFS[4]] {
            report("earthlike", relief, None);
        }
    }

    #[test]
    fn polar_contrast_regression_metrics_stable() {
        let metrics = regression_metrics_for_preset("polar_contrast", None);
        assert!(
            (metrics.land_ratio - 0.440).abs() <= 0.01,
            "polar_contrast land ratio drift: {}",
            metrics.land_ratio
        );
        assert!(
            (metrics.fold as isize - 3004).abs() <= 40,
            "polar_contrast fold count drift: {}",
            metrics.fold
        );
        assert!(
            (metrics.fault as isize - 702).abs() <= 24,
            "polar_contrast fault count drift: {}",
            metrics.fault
        );
        assert!(
            (metrics.volcanic as isize - 52).abs() <= 10,
            "polar_contrast volcanic count drift: {}",
            metrics.volcanic
        );
        assert!(
            (metrics.dome as isize - 981).abs() <= 40,
            "polar_contrast dome count drift: {}",
            metrics.dome
        );
        assert!(
            (metrics.polar_fold as isize - 1471).abs() <= 36,
            "polar_contrast polar fold drift: {}",
            metrics.polar_fold
        );
        assert!(
            (metrics.polar_fault as isize - 244).abs() <= 18,
            "polar_contrast polar fault drift: {}",
            metrics.polar_fault
        );
        assert!(
            (metrics.polar_uplift_cells as isize - 233).abs() <= 14,
            "polar_contrast polar uplift cells drift: {}",
            metrics.polar_uplift_cells
        );
        assert!(
            (metrics.polar_relief_cells as isize - 368).abs() <= 18,
            "polar_contrast polar relief cap drift: {}",
            metrics.polar_relief_cells
        );
    }

    // -------------------------------------------------------------------------------------
    // Bathymetry invariants (see core_sim/CLAUDE.md → World Generation Pipeline).
    //
    // These run the REAL full pipeline (`build_headless_app` Startup chain: bands →
    // biomes → hydrology → tag solver → palette clamp → `reconcile_coastal_shelf`) and
    // then compare the FINAL map against the band raster the very same seed produces.
    // They guard the legacy map-border "edge ring" bug: `classify_terrain`'s three
    // `edge < coastal_*_edge` rings proxy a coastline only in the preset-less world; under
    // a preset they read the MAP FRAME, and used to coin-flip hundreds of band-`Land`
    // tiles per map into water biomes along the border — deleting the land out from under
    // legitimate shelf rings (orphaned shelf) and pinching off isolated deep pockets.
    // -------------------------------------------------------------------------------------

    /// Full-pipeline map plus the band raster / restamped elevation for the same seed.
    struct GeneratedWorld {
        snapshot: sim_schema::WorldSnapshot,
        bands: BandsResult,
        width: usize,
        height: usize,
        wrap_horizontal: bool,
        shelf: ShelfConfig,
    }

    impl GeneratedWorld {
        fn idx(&self, x: usize, y: usize) -> usize {
            y * self.width + x
        }

        fn terrain(&self, x: usize, y: usize) -> sim_runtime::TerrainType {
            self.snapshot.terrain.samples[self.idx(x, y)].terrain
        }

        fn is_water(&self, x: usize, y: usize) -> bool {
            crate::terrain::terrain_definition(self.terrain(x, y))
                .tags
                .contains(sim_runtime::TerrainTags::WATER)
        }

        fn neighbors(&self, x: usize, y: usize) -> Vec<(usize, usize)> {
            crate::grid_utils::hex_neighbors_wrapped(
                x as u32,
                y as u32,
                self.width as u32,
                self.height as u32,
                self.wrap_horizontal,
            )
            .map(|(nx, ny)| (nx as usize, ny as usize))
            .collect()
        }
    }

    /// Runs the full Startup pipeline for `earthlike` at the given size/seed and rebuilds the
    /// band raster from the same resolved inputs (deterministic, so the two agree tile-for-tile).
    fn generate_earthlike_world(width: u32, height: u32, seed: u64) -> GeneratedWorld {
        let mut app = crate::build_headless_app();
        if let Some(mut md) = app
            .world
            .get_resource_mut::<crate::resources::SimulationConfigMetadata>()
        {
            md.set_seed_random(false);
        }
        {
            let mut cfg = app.world.resource_mut::<SimulationConfig>();
            cfg.map_preset_id = "earthlike".to_string();
            cfg.grid_size = UVec2::new(width, height);
            cfg.map_seed = seed;
        }
        app.update();

        let config = app.world.resource::<SimulationConfig>().clone();
        let presets = app
            .world
            .resource::<crate::map_preset::MapPresetsHandle>()
            .get();
        let preset = presets
            .get(&config.map_preset_id)
            .expect("earthlike preset");
        let world_seed = config.map_seed;

        let elevation = build_elevation_field(&config, Some(preset), world_seed);
        let bands = build_bands(
            &elevation,
            preset.sea_level,
            &preset.macro_land,
            &preset.shelf,
            &preset.islands,
            &preset.inland_sea,
            &preset.ocean,
            preset.moisture_scale,
            &preset.biomes,
            world_seed,
            preset.mountain_scale,
            &preset.mountains,
            config.map_topology.wrap_horizontal,
        );

        let snapshot = app
            .world
            .resource::<crate::SnapshotHistory>()
            .last_snapshot
            .as_ref()
            .map(|s| (**s).clone())
            .expect("snapshot after worldgen");

        GeneratedWorld {
            snapshot,
            bands,
            width: width as usize,
            height: height as usize,
            wrap_horizontal: config.map_topology.wrap_horizontal,
            shelf: preset.shelf.clone(),
        }
    }

    #[test]
    fn earthlike_band_land_never_ends_water_tagged() {
        // THE core invariant Part 1 of the border-ring fix establishes: `classify_terrain` is
        // only ever called for tiles the band raster declared `Land`, so no such tile may come
        // back WATER-tagged on the final map. Before the fix the legacy map-border edge rings
        // coin-flipped 248-295 band-`Land` tiles per 80x52 map (~16-19% of all land) into
        // DeepOcean/shelf/marsh biomes hugging the map frame.
        for (w, h, seed) in [
            (80u32, 52u32, 0x0FA1_C0DEu64),
            (80, 52, 0x5EED_F00D),
            (128, 96, 0x0000_BEEF),
        ] {
            let world = generate_earthlike_world(w, h, seed);
            let mut drowned: Vec<String> = Vec::new();
            for y in 0..world.height {
                for x in 0..world.width {
                    if world.bands.terrain[world.idx(x, y)] != TerrainBand::Land {
                        continue;
                    }
                    // A NavigableRiver is the one legitimate way a band-Land tile ends WATER-tagged:
                    // the hydrology pass deliberately carves a big river's tail into a waterway
                    // through the land. This invariant guards against the *classifier* drowning land
                    // by noise (the border-ring bug), not against hydrology doing its job.
                    if world.terrain(x, y) == sim_runtime::TerrainType::NavigableRiver {
                        continue;
                    }
                    if world.is_water(x, y) {
                        drowned.push(format!("({x},{y})={:?}", world.terrain(x, y)));
                    }
                }
            }
            assert!(
                drowned.is_empty(),
                "{w}x{h} seed={seed:#x}: {} band-Land tiles ended WATER-tagged: {}",
                drowned.len(),
                drowned
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    #[test]
    fn earthlike_shelf_is_never_orphaned() {
        // The shelf is a coastal fringe: every `ContinentalShelf` tile must sit within the
        // effective shelf width of land (1 tile at every shipped size — `min_width_tiles` floors
        // the sub-tile earthlike width), i.e. it has >= 1 land hex-neighbour. The border-ring bug
        // deleted the land out from under legitimate shelf rings, stranding 118-153 shelf tiles
        // per 80x52 map with NO land hex-neighbour, 3-7 hexes out to sea.
        for (w, h, seed) in [
            (80u32, 52u32, 0x0FA1_C0DEu64),
            (80, 52, 0x5EED_F00D),
            (128, 96, 0x0000_BEEF),
        ] {
            let world = generate_earthlike_world(w, h, seed);
            let width_tiles = effective_shelf_width(&world.shelf, world.width, world.height);
            assert!(
                width_tiles <= 1.0,
                "this test asserts the d=1 fringe; earthlike {w}x{h} widened the shelf to \
                 {width_tiles} tiles - extend it to a hex distance transform"
            );

            let mut orphans: Vec<String> = Vec::new();
            for y in 0..world.height {
                for x in 0..world.width {
                    if world.terrain(x, y) != sim_runtime::TerrainType::ContinentalShelf {
                        continue;
                    }
                    let has_land_neighbour = world
                        .neighbors(x, y)
                        .into_iter()
                        .any(|(nx, ny)| !world.is_water(nx, ny));
                    if !has_land_neighbour {
                        orphans.push(format!("({x},{y})"));
                    }
                }
            }
            assert!(
                orphans.is_empty(),
                "{w}x{h} seed={seed:#x}: {} ContinentalShelf tiles have no land hex-neighbour: {}",
                orphans.len(),
                orphans
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    /// The largest radius a continental fragment can roll — how far its tiles reach past its
    /// centre. Derived from the placement constants rather than restated, so a retune moves it.
    const ISLAND_CONTINENTAL_RADIUS_MAX: usize =
        ISLAND_CONTINENTAL_RADIUS_MIN + ISLAND_CONTINENTAL_RADIUS_VARIANTS as usize - 1;
    /// Tiles in a max-radius continental dome — `raise_island_blob` fills the disc
    /// `dx² + dy² <= r²`, which at `r = 2` is 13 tiles. The most land one fragment can cover alone.
    const ISLAND_CONTINENTAL_DOME_MAX_TILES: usize = 13;

    /// Sea level used by the island-placement tests, on the field's normalized 0..1 scale.
    const ISLAND_TEST_SEA_LEVEL: f32 = 0.5;
    /// How far above `ISLAND_TEST_SEA_LEVEL` the test continent's tiles sit.
    const ISLAND_TEST_CONTINENT_HEIGHT: f32 = 0.3;

    /// Connected components (4-neighbour) of the tiles selected by `member`, as tile-index sets.
    fn island_components(
        member: &dyn Fn(usize) -> bool,
        w: usize,
        h: usize,
    ) -> Vec<std::collections::HashSet<usize>> {
        let mut seen = vec![false; w * h];
        let mut components = Vec::new();
        for start in 0..w * h {
            if seen[start] || !member(start) {
                continue;
            }
            let mut component = std::collections::HashSet::new();
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                component.insert(i);
                let (x, y) = ((i % w) as isize, (i / w) as isize);
                for (dx, dy) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                        continue;
                    }
                    let ni = ny as usize * w + nx as usize;
                    if !seen[ni] && member(ni) {
                        seen[ni] = true;
                        stack.push(ni);
                    }
                }
            }
            components.push(component);
        }
        components
    }

    /// Minimum Manhattan distance between two tile-index sets — the same metric `place_islands`
    /// uses when it measures a candidate's distance to the nearest land.
    fn min_manhattan_between(
        a: &std::collections::HashSet<usize>,
        b: &std::collections::HashSet<usize>,
        w: usize,
    ) -> usize {
        let mut best = usize::MAX;
        for &i in a {
            for &j in b {
                let (ax, ay) = ((i % w) as isize, (i / w) as isize);
                let (bx, by) = ((j % w) as isize, (j / w) as isize);
                best = best.min((ax - bx).unsigned_abs() + (ay - by).unsigned_abs());
            }
        }
        best
    }

    /// Oceanic islands must honour `min_distance_from_continent` **against each other**, not just
    /// against the pre-existing continents: each placement raises land, and the next candidate's
    /// scan reads that land live off the field. Before the live read this scan consulted a stale
    /// pre-call snapshot, so specks could stack on one tile or cluster inside the exclusion radius.
    #[test]
    fn oceanic_islands_never_stack_and_respect_min_distance() {
        let w = 96usize;
        let h = 96usize;
        // Open ocean everywhere: no continent, so every rejection must come from another island.
        let mut field = ElevationField::new(w as u32, h as u32, vec![0.0f32; w * h]);
        let is_ocean = vec![true; w * h];
        let islands = IslandConfig {
            continental_density: 0.0,
            oceanic_density: 0.002,
            ..IslandConfig::default()
        };
        let shelf = ShelfConfig::default();
        let min_distance = islands.min_distance_from_continent as usize;

        place_islands(
            &mut field,
            &is_ocean,
            &islands,
            &shelf,
            ISLAND_TEST_SEA_LEVEL,
            w,
            h,
            0xDEAD_BEEF,
        );

        let is_land =
            |i: usize| field.sample((i % w) as u32, (i / w) as u32) > ISLAND_TEST_SEA_LEVEL;
        let components = island_components(&is_land, w, h);
        assert!(
            components.len() > 1,
            "test is vacuous: only {} island(s) placed",
            components.len()
        );

        // Non-overlap: every island the loop placed must be its own connected component. Two specks
        // stacked on one tile (or merged into one blob) would show up as a shortfall here. The
        // target is the same expression `place_islands` uses to size its run.
        let target_oi = ((w * h) as f32 * islands.oceanic_density) as usize;
        assert_eq!(
            components.len(),
            target_oi,
            "oceanic islands stacked or merged: {} distinct islands for {target_oi} placements",
            components.len()
        );

        // Spacing: the placement scan rejects any candidate with land inside the exclusion square,
        // so distinct islands end up strictly beyond `min_distance_from_continent`.
        for (a_idx, a) in components.iter().enumerate() {
            for b in components.iter().skip(a_idx + 1) {
                let d = min_manhattan_between(a, b, w);
                assert!(
                    d > min_distance,
                    "two oceanic islands are {d} tiles apart, inside min_distance_from_continent \
                     ({min_distance})"
                );
            }
        }
    }

    /// The continental-fringe path has the same contract: an island raised on one iteration is land
    /// for the next, so later fragments must sit at least `shelf.width_tiles` away from it rather
    /// than only from the original continent.
    #[test]
    fn continental_fringe_islands_stay_off_previously_placed_islands() {
        let w = 96usize;
        let h = 96usize;
        // A 2-column continent on the west edge gives the fringe scan something to measure from.
        const CONTINENT_WIDTH: usize = 2;
        let mut elev = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..CONTINENT_WIDTH {
                elev[y * w + x] = ISLAND_TEST_SEA_LEVEL + ISLAND_TEST_CONTINENT_HEIGHT;
            }
        }
        let mut field = ElevationField::new(w as u32, h as u32, elev);
        let is_ocean = (0..w * h)
            .map(|i| i % w >= CONTINENT_WIDTH)
            .collect::<Vec<_>>();
        let islands = IslandConfig {
            continental_density: 0.002,
            oceanic_density: 0.0,
            ..IslandConfig::default()
        };
        // A wide fringe band well clear of the max continental dome radius, so the surviving
        // separation below is comfortably positive.
        let shelf = ShelfConfig {
            width_tiles: 6,
            slope_width_tiles: 6,
            ..ShelfConfig::default()
        };
        let fringe_min = shelf.width_tiles as usize;

        place_islands(
            &mut field,
            &is_ocean,
            &islands,
            &shelf,
            ISLAND_TEST_SEA_LEVEL,
            w,
            h,
            0x1234_5678,
        );

        // Exclude the seed continent; only compare the fragments against one another.
        let is_island = |i: usize| {
            i % w >= CONTINENT_WIDTH
                && field.sample((i % w) as u32, (i / w) as u32) > ISLAND_TEST_SEA_LEVEL
        };
        let components = island_components(&is_island, w, h);
        assert!(
            components.len() > 1,
            "test is vacuous: only {} fragment(s) placed",
            components.len()
        );

        // Non-overlap. The loop may legitimately place fewer than its target (each fragment eats
        // into the fringe band, so later attempts find fewer valid candidates), so the count is not
        // the invariant — the *size* is: no single fragment may exceed one max-radius dome. Two
        // stacked or touching fragments would merge into a component larger than that.
        for component in &components {
            assert!(
                component.len() <= ISLAND_CONTINENTAL_DOME_MAX_TILES,
                "a continental fragment covers {} tiles, more than one radius-\
                 {ISLAND_CONTINENTAL_RADIUS_MAX} dome ({ISLAND_CONTINENTAL_DOME_MAX_TILES}) — \
                 fragments overlapped",
                component.len()
            );
        }

        // Spacing. The fringe scan measures a *candidate centre* against existing land, so a new
        // centre lands `>= fringe_min` (Manhattan) from the nearest tile of every earlier fragment.
        // Its own dome then reaches out by up to `ISLAND_CONTINENTAL_RADIUS_MAX`, so the surviving
        // tile-to-tile separation is that band minus one dome radius.
        let min_separation = fringe_min - ISLAND_CONTINENTAL_RADIUS_MAX;
        for (a_idx, a) in components.iter().enumerate() {
            for b in components.iter().skip(a_idx + 1) {
                let d = min_manhattan_between(a, b, w);
                assert!(
                    d >= min_separation,
                    "two continental fragments are {d} tiles apart, closer than the fringe band \
                     minimum ({fringe_min}) allows for a dome of radius \
                     {ISLAND_CONTINENTAL_RADIUS_MAX} ({min_separation})"
                );
            }
        }
    }
}
