use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
};

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};

use crate::{
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

#[derive(Clone, Copy)]
struct QueueItem {
    priority: f32,
    idx: usize,
    continent: usize,
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
            && self.continent == other.continent
            && self.priority.to_bits() == other.priority.to_bits()
    }
}

impl Eq for QueueItem {}

impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.priority.total_cmp(&other.priority) {
            Ordering::Equal => match self.idx.cmp(&other.idx) {
                Ordering::Equal => self.continent.cmp(&other.continent),
                ordering => ordering,
            },
            ordering => ordering,
        }
    }
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
) -> BandsResult {
    let w = elevation.width as usize;
    let h = elevation.height as usize;
    let LandMask { mask, land_count } = generate_land_mask(elevation, macro_cfg, sea_level, seed);
    let mut land = mask;
    let initial_land_ratio = land_count as f32 / (w * h) as f32;
    tracing::debug!(
        target: "shadow_scale::mapgen",
        initial_land_ratio,
        target_land_pct = macro_cfg.target_land_pct,
        continents = macro_cfg.continents,
        min_area = macro_cfg.min_area,
        jitter = macro_cfg.jitter,
        "mapgen.macro_land.initial_ratio"
    );

    let mut is_ocean = compute_ocean_mask(&land, w, h);

    // Optionally connect inland seas to ocean via simple strait rule
    if inland.merge_strait_width > 0 {
        connect_inland_seas_via_straits(
            &mut land,
            &mut is_ocean,
            inland.merge_strait_width as usize,
            w,
            h,
        );
        is_ocean = compute_ocean_mask(&land, w, h);
    }

    // Place islands before classifying so shelves wrap correctly.
    place_islands(&mut land, &mut is_ocean, islands, shelf, w, h, seed);
    is_ocean = compute_ocean_mask(&land, w, h);

    rebalance_land_ratio(
        &mut land,
        &mut is_ocean,
        elevation,
        macro_cfg.target_land_pct,
        0.015,
        w,
        h,
        seed,
    );
    is_ocean = compute_ocean_mask(&land, w, h);

    let land_distance = compute_land_distance(&land, w, h);
    let coastal_land = compute_coastal_land(&land, &is_ocean, w, h);
    let mountains = derive_mountain_mask(
        &land,
        &is_ocean,
        &land_distance,
        elevation,
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
        elevation,
        mountain_cfg,
        mountain_scale,
        ocean_cfg,
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
    let ocean_distance = compute_ocean_distance(&land, w, h);
    let terrain = classify_bands(&land, &is_ocean, &ocean_distance, shelf, w, h);

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

fn neighbors4(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    let mut v = Vec::with_capacity(4);
    if x > 0 {
        v.push((x - 1, y));
    }
    if x + 1 < w {
        v.push((x + 1, y));
    }
    if y > 0 {
        v.push((x, y - 1));
    }
    if y + 1 < h {
        v.push((x, y + 1));
    }
    v.into_iter()
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

fn connect_inland_seas_via_straits(
    land: &mut [bool],
    is_ocean: &mut [bool],
    max_width: usize,
    w: usize,
    h: usize,
) {
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
                land[i] = false;
                is_ocean[i] = true;
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

fn place_islands(
    land: &mut [bool],
    is_ocean: &mut [bool],
    islands: &IslandConfig,
    shelf: &ShelfConfig,
    w: usize,
    h: usize,
    seed: u64,
) {
    // Very lightweight placement: random samples along slope fringe for continental fragments
    // and in abyssal for oceanic islands.
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut rng = SmallRng::seed_from_u64(seed ^ 0xA51C_E55E);

    // Continental fragments along slope fringe (distance in [shelf, shelf+slope])
    let fringe_min = shelf.width_tiles as usize;
    let fringe_max = (shelf.width_tiles + shelf.slope_width_tiles) as usize;
    let mut placed_cf = 0u32;
    let target_cf = ((w * h) as f32 * islands.continental_density) as u32;
    for _ in 0..(target_cf * 10).max(100) {
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
        for dy in -8..=8 {
            for dx in -8..=8 {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                    continue;
                }
                let ni = idx(nx as usize, ny as usize);
                if land[ni] {
                    let d = dx.unsigned_abs() + dy.unsigned_abs();
                    near_dist = near_dist.min(d);
                }
            }
        }
        if near_dist >= fringe_min && near_dist <= fringe_max {
            let radius = 1 + (rng.gen::<u32>() % 2) as usize;
            carve_blob_into(land, is_ocean, w, h, x, y, radius);
            placed_cf += 1;
        }
    }

    // Oceanic islands: far from continents; place in deep ocean
    let mut placed_oi = 0u32;
    let target_oi = ((w * h) as f32 * islands.oceanic_density) as u32;
    for _ in 0..(target_oi * 20).max(200) {
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
                if land[idx(nx as usize, ny as usize)] {
                    ok = false;
                    break 'scan;
                }
            }
        }
        if !ok {
            continue;
        }
        carve_blob_into(land, is_ocean, w, h, x, y, 1);
        placed_oi += 1;
    }
}

fn carve_blob_into(
    land: &mut [bool],
    is_ocean: &mut [bool],
    w: usize,
    h: usize,
    cx: usize,
    cy: usize,
    radius: usize,
) {
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    for dy in -(radius as isize)..=(radius as isize) {
        for dx in -(radius as isize)..=(radius as isize) {
            let nx = cx as isize + dx;
            let ny = cy as isize + dy;
            if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                continue;
            }
            let dist2 = (dx * dx + dy * dy) as usize;
            if dist2 as f32 <= (radius as f32 * radius as f32) {
                let i = idx(nx as usize, ny as usize);
                land[i] = true;
                is_ocean[i] = false;
            }
        }
    }
}

fn generate_land_mask(
    elevation: &ElevationField,
    macro_cfg: &MacroLandConfig,
    _sea_level: f32,
    seed: u64,
) -> LandMask {
    let w = elevation.width as usize;
    let h = elevation.height as usize;
    let total = w * h;
    let idx = |x: usize, y: usize| -> usize { y * w + x };

    if total == 0 {
        return LandMask {
            mask: Vec::new(),
            land_count: 0,
        };
    }

    let target_pct = macro_cfg.target_land_pct.clamp(0.01, 0.99);
    let mut desired_land = ((total as f32) * target_pct).round() as usize;
    desired_land = desired_land.clamp(1, total);

    let mut tile_scores = vec![0.0f32; total];
    for y in 0..h {
        for x in 0..w {
            let e = elevation.sample(x as u32, y as u32);
            let jitter_scale = macro_cfg.jitter.max(0.0);
            let jitter_noise = if jitter_scale > 0.0 {
                (terrain_hash(seed, x as u32, y as u32) & 0xFFFF) as f32 / 65535.0 * jitter_scale
            } else {
                0.0
            };
            tile_scores[idx(x, y)] = e + jitter_noise;
        }
    }

    let mut sorted_indices: Vec<usize> = (0..total).collect();
    sorted_indices.sort_unstable_by(|a, b| tile_scores[*b].total_cmp(&tile_scores[*a]));

    let mut continents = macro_cfg.continents.max(1) as usize;
    continents = continents.min(desired_land.max(1));

    let raw_min_area = macro_cfg.min_area.max(1) as usize;
    let per_continent_cap = (desired_land / continents.max(1)).max(1);
    let effective_min_area = raw_min_area.min(per_continent_cap);

    let mut seeds: Vec<usize> = Vec::new();
    let spacing = ((total as f32 / continents as f32).sqrt() as usize).max(3);

    for &candidate in &sorted_indices {
        if seeds.len() >= continents {
            break;
        }
        let cx = candidate % w;
        let cy = candidate / w;
        if seeds.iter().all(|&existing| {
            let ex = existing % w;
            let ey = existing / w;
            let dist = cx.abs_diff(ex) + cy.abs_diff(ey);
            dist >= spacing
        }) {
            seeds.push(candidate);
        }
    }

    if seeds.len() < continents {
        for &candidate in &sorted_indices {
            if seeds.len() >= continents {
                break;
            }
            if !seeds.contains(&candidate) {
                seeds.push(candidate);
            }
        }
    }

    let mut targets = vec![0usize; continents];
    let mut areas = vec![0usize; continents];
    let base_target = desired_land / continents;
    let mut remainder = desired_land % continents;
    for target in targets.iter_mut().take(continents) {
        let mut t = base_target;
        if remainder > 0 {
            remainder -= 1;
            t += 1;
        }
        *target = t.max(effective_min_area);
    }
    if targets.iter().sum::<usize>() > desired_land {
        let mut excess = targets.iter().sum::<usize>() - desired_land;
        for target in targets.iter_mut().rev() {
            if excess == 0 {
                break;
            }
            let min_allowed = effective_min_area;
            if *target > min_allowed {
                let reducible = (*target - min_allowed).min(excess);
                *target -= reducible;
                excess -= reducible;
            }
        }
    }

    let mut land = vec![false; total];
    let mut assignment = vec![None::<usize>; total];
    let mut heap = BinaryHeap::new();
    let mut overflow = BinaryHeap::new();
    let mut total_land = 0usize;

    for (id, &seed_idx) in seeds.iter().enumerate() {
        if assignment[seed_idx].is_some() {
            continue;
        }
        assignment[seed_idx] = Some(id);
        land[seed_idx] = true;
        areas[id] += 1;
        total_land += 1;
        push_neighbors(seed_idx, id, w, h, &assignment, &tile_scores, &mut heap);
    }

    let mut pending_targets = targets
        .iter()
        .enumerate()
        .filter(|(i, t)| areas[*i] < **t)
        .count();

    while pending_targets > 0 && total_land < desired_land {
        if let Some(item) = heap.pop() {
            if assignment[item.idx].is_some() {
                continue;
            }
            let continent = item.continent;
            if areas[continent] >= targets[continent] {
                overflow.push(item);
                continue;
            }
            assignment[item.idx] = Some(continent);
            land[item.idx] = true;
            areas[continent] += 1;
            total_land += 1;
            if areas[continent] >= targets[continent] {
                pending_targets = pending_targets.saturating_sub(1);
            }
            push_neighbors(
                item.idx,
                continent,
                w,
                h,
                &assignment,
                &tile_scores,
                &mut heap,
            );
        } else {
            break;
        }
    }

    heap.append(&mut overflow);

    while total_land < desired_land {
        if let Some(item) = heap.pop() {
            if assignment[item.idx].is_some() {
                continue;
            }
            let continent = item.continent.min(areas.len().saturating_sub(1));
            assignment[item.idx] = Some(continent);
            land[item.idx] = true;
            areas[continent] = areas[continent].saturating_add(1);
            total_land += 1;
            push_neighbors(
                item.idx,
                continent,
                w,
                h,
                &assignment,
                &tile_scores,
                &mut heap,
            );
        } else {
            break;
        }
    }

    if total_land < desired_land {
        let mut remaining = desired_land - total_land;
        for &candidate in &sorted_indices {
            if remaining == 0 {
                break;
            }
            if !land[candidate] {
                land[candidate] = true;
                remaining -= 1;
            }
        }
        total_land = desired_land - remaining;
    }

    LandMask {
        mask: land,
        land_count: total_land,
    }
}

fn push_neighbors(
    idx_tile: usize,
    continent_id: usize,
    w: usize,
    h: usize,
    assignment: &[Option<usize>],
    tile_scores: &[f32],
    heap: &mut BinaryHeap<QueueItem>,
) {
    let x = idx_tile % w;
    let y = idx_tile / w;
    for (nx, ny) in neighbors4(x, y, w, h) {
        let nidx = ny * w + nx;
        if assignment[nidx].is_none() {
            heap.push(QueueItem {
                priority: tile_scores[nidx],
                idx: nidx,
                continent: continent_id,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rebalance_land_ratio(
    land: &mut [bool],
    is_ocean: &mut [bool],
    elevation: &ElevationField,
    target_pct: f32,
    tolerance: f32,
    w: usize,
    h: usize,
    seed: u64,
) {
    let total_tiles = land.len();
    if total_tiles == 0 {
        return;
    }
    let target_pct = target_pct.clamp(0.01, 0.99);
    let target_tiles = ((total_tiles as f32) * target_pct).round() as isize;
    let tolerance_tiles = ((total_tiles as f32) * tolerance.clamp(0.0, 0.2)).round() as isize;
    let lower_bound = (target_tiles - tolerance_tiles).max(0);
    let upper_bound = (target_tiles + tolerance_tiles).min(total_tiles as isize);

    let mut land_count = land.iter().filter(|&&is_land| is_land).count() as isize;
    if land_count >= lower_bound && land_count <= upper_bound {
        return;
    }

    if land_count < target_tiles {
        let needed = (target_tiles - land_count) as usize;
        if needed > 0 {
            adjust_land_tiles(
                land,
                is_ocean,
                elevation,
                w,
                h,
                seed,
                needed,
                true,
                &mut land_count,
            );
        }
    } else if land_count > target_tiles {
        let surplus = (land_count - target_tiles) as usize;
        if surplus > 0 {
            adjust_land_tiles(
                land,
                is_ocean,
                elevation,
                w,
                h,
                seed,
                surplus,
                false,
                &mut land_count,
            );
        }
    }
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

#[allow(clippy::too_many_arguments)]
fn adjust_land_tiles(
    land: &mut [bool],
    is_ocean: &mut [bool],
    elevation: &ElevationField,
    w: usize,
    h: usize,
    seed: u64,
    count: usize,
    grow: bool,
    land_count: &mut isize,
) {
    if count == 0 {
        return;
    }
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut candidates: Vec<(usize, f32)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let tile_idx = idx(x, y);
            if grow == land[tile_idx] {
                continue;
            }
            let mut score = elevation.sample(x as u32, y as u32);
            let mut adjacent = false;
            for (nx, ny) in neighbors4(x, y, w, h) {
                if land[idx(nx, ny)] {
                    adjacent = true;
                    break;
                }
            }
            if grow {
                if adjacent {
                    score += 0.35;
                } else {
                    score -= 0.15;
                }
            } else if adjacent {
                score -= 0.25;
            } else {
                score += 0.1;
            }
            let noise = terrain_hash(seed ^ 0xA962_4D3B, x as u32, y as u32);
            let jitter = ((noise & 0xFFFF) as f32 / 65535.0 - 0.5) * 0.05;
            score += jitter;
            candidates.push((tile_idx, score));
        }
    }
    if candidates.is_empty() {
        return;
    }
    if grow {
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    } else {
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
    }

    let mut remaining = count;
    for (tile_idx, _) in candidates.into_iter() {
        if remaining == 0 {
            break;
        }
        if grow {
            if land[tile_idx] {
                continue;
            }
            land[tile_idx] = true;
            is_ocean[tile_idx] = false;
            *land_count += 1;
        } else {
            if !land[tile_idx] {
                continue;
            }
            land[tile_idx] = false;
            *land_count -= 1;
        }
        remaining -= 1;
    }
}

fn compute_ocean_mask(land: &[bool], w: usize, h: usize) -> Vec<bool> {
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut visited = vec![false; w * h];
    let mut is_ocean = vec![false; w * h];
    let mut q = VecDeque::new();

    for x in 0..w {
        if !land[idx(x, 0)] {
            q.push_back((x, 0));
        }
        if !land[idx(x, h.saturating_sub(1))] {
            q.push_back((x, h.saturating_sub(1)));
        }
    }
    for y in 0..h {
        if !land[idx(0, y)] {
            q.push_back((0, y));
        }
        if !land[idx(w.saturating_sub(1), y)] {
            q.push_back((w.saturating_sub(1), y));
        }
    }

    while let Some((x, y)) = q.pop_front() {
        let i = idx(x, y);
        if visited[i] || land[i] {
            continue;
        }
        visited[i] = true;
        is_ocean[i] = true;
        for (nx, ny) in neighbors4(x, y, w, h) {
            let ni = idx(nx, ny);
            if !visited[ni] && !land[ni] {
                q.push_back((nx, ny));
            }
        }
    }

    is_ocean
}

fn compute_ocean_distance(land: &[bool], w: usize, h: usize) -> Vec<u32> {
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
        for (nx, ny) in neighbors4(x, y, w, h) {
            let ni = idx(nx, ny);
            if distance[ni] == u32::MAX {
                distance[ni] = base.saturating_add(1);
                dq.push_back((nx, ny));
            }
        }
    }

    distance
}

fn compute_land_distance(land: &[bool], w: usize, h: usize) -> Vec<u32> {
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
            for (nx, ny) in neighbors4(x, y, w, h) {
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
        for (nx, ny) in neighbors4(x, y, w, h) {
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

fn classify_bands(
    land: &[bool],
    is_ocean: &[bool],
    ocean_distance: &[u32],
    shelf: &ShelfConfig,
    w: usize,
    h: usize,
) -> Vec<TerrainBand> {
    let mut terrain = vec![TerrainBand::DeepOcean; w * h];
    let idx = |x: usize, y: usize| -> usize { y * w + x };

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
            if d <= shelf.width_tiles {
                terrain[i] = TerrainBand::ContinentalShelf;
            } else if d <= shelf.width_tiles + shelf.slope_width_tiles {
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
    let belt_width = ((belt_width_base as f32) * (1.0 + mountain_scale.clamp(0.0, 2.0) * 0.5))
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

        let mut plate_count = if comp_area < 192 {
            1
        } else if comp_area < 640 {
            2
        } else if comp_area < 1500 {
            3
        } else {
            4
        };
        if plate_count <= 1 && comp_area >= 256 {
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
                let jitter = (rng.gen::<f32>() - 0.5) * std::f32::consts::FRAC_PI_2;
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
            if dot <= -0.1 {
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
            let convergence_threshold = -0.2f32;
            let divergence_threshold = 0.45f32;

            for (comp_idx, cells) in components.iter().enumerate() {
                let mut polar_cells = Vec::new();
                for &idx in cells.iter() {
                    let y = idx / w;
                    if y < polar_band || y >= south_band_start {
                        polar_cells.push(idx);
                    }
                }

                if polar_cells.len() < 32 {
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
                let upper_bound = (polar_cells.len() / 12).max(2);
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
                    vx = vx * 0.7 + rand_vec.0 * 0.3 + bias.0 * 0.6;
                    vy = vy * 0.7 + rand_vec.1 * 0.3 + bias.1 * 0.6;
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
                                    strength: 4,
                                },
                            );
                            mask.set(
                                nidx,
                                MountainCell {
                                    ty: MountainType::Fault,
                                    strength: 4,
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

    let fault_line_count = cfg.fault_line_count.min(6);
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
        if cells.len() < 12 {
            continue;
        }
        let mut comp_rng = SmallRng::seed_from_u64(seed ^ ((comp_idx as u64 + 1) * 0x9E37C15D));

        let interior_cells: Vec<usize> = cells
            .iter()
            .copied()
            .filter(|&idx| land_distance[idx] >= 3)
            .collect();
        let fault_start_pool = if !interior_cells.is_empty() {
            &interior_cells
        } else {
            cells
        };

        let mut local_faults = fault_line_count.max(1);
        if cells.len() > 600 {
            local_faults += 1;
        }
        if cells.len() > 1400 {
            local_faults += 1;
        }

        for _ in 0..local_faults {
            let start = fault_start_pool[comp_rng.gen_range(0..fault_start_pool.len())];
            let dir = fault_dirs[comp_rng.gen_range(0..fault_dirs.len())];
            let mut current = start;
            let mut length = (cells.len() as f32 * 0.1 * comp_rng.gen::<f32>()).round() as usize;
            length = length.clamp(4, (cells.len() / 2).max(4));
            let mut step = 0usize;
            let strength = 6u8;
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
                        if comp_rng.gen::<f32>() > 0.55 {
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
                                    strength: strength.saturating_sub(2),
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
        let volcanic_weight =
            (cells.len() as f32 / 800.0).clamp(0.3, 1.4) * (0.55 + 0.7 * coastal_fraction);
        let volcanic_chance = (cfg.volcanic_arc_chance * volcanic_weight).clamp(0.0, 0.8);
        let max_chains = cfg.max_volcanic_chains_per_plate.max(1) as usize;
        let mut chains_spawned = 0usize;
        let mut attempts = max_chains * 3;
        let strength_drop = cfg.volcanic_strength_drop.max(0.8);
        let component_cap = ((cells.len() as f32) * 0.012)
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
            let base_length = cfg.volcanic_chain_length.clamp(1, 12) as usize;
            let max_chain_len = (tile_cap - volcanic_tiles_used).max(1);
            let chain_len = base_length.min(max_chain_len);
            let mut chain_strength = (cfg.volcanic_strength * 7.0).clamp(2.5, 9.0);
            let mut chain_step = 0usize;
            let mut chain_gap = comp_rng.gen_range(2..5);

            while chain_step < chain_len && chain_strength > 1.0 {
                if volcanic_tiles_used >= tile_cap {
                    break;
                }
                let primary_strength = chain_strength.round().clamp(1.0, 12.0) as u8;
                mask.set(
                    start,
                    MountainCell {
                        ty: MountainType::Volcanic,
                        strength: primary_strength,
                    },
                );
                volcanic_tiles_used += 1;

                if comp_rng.gen::<f32>() < 0.5 {
                    let x = start % w;
                    let y = start / w;
                    let perpendicular = (-chain_dir.1, chain_dir.0);
                    for &(px, py) in [perpendicular, (-perpendicular.0, -perpendicular.1)].iter() {
                        if comp_rng.gen::<f32>() > 0.35 {
                            continue;
                        }
                        let nx = x as isize + px;
                        let ny = y as isize + py;
                        if nx < 0 || ny < 0 || nx as usize >= w || ny as usize >= h {
                            continue;
                        }
                        let nidx = ny as usize * w + nx as usize;
                        if component_ids[nidx] == comp_idx as i32 {
                            let flank_strength = (chain_strength - strength_drop * 0.6)
                                .round()
                                .clamp(1.0, 9.0)
                                as u8;
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
                    chain_dir = if comp_rng.gen::<f32>() < 0.4 {
                        *(fault_dirs
                            .get(comp_rng.gen_range(0..fault_dirs.len()))
                            .unwrap_or(&(1, 0)))
                    } else {
                        chain_dir
                    };
                    if comp_rng.gen::<f32>() < 0.2 {
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
                        strength: 4,
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
    let variance_scale = (variance * micro_strength * 0.5).clamp(0.0, 1.0);
    let base_interior = (1.0 - micro_strength * 0.4).clamp(MIN_RELIEF_SCALE, 1.0);
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
            let boosted = ((cell.strength as f32) + micro_strength * 4.0 * factor)
                .round()
                .clamp(cell.strength as f32, 12.0) as u8;
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
    let mut dir = if dist_equator < 0.18 { -1 } else { 1 };
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
        let latitude_bonus =
            (1.0 - (dist_equator * 1.8).min(1.0)) * cfg.latitude_humidity_weight.clamp(0.0, 1.0);

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
                    humidity -= added_shadow * 0.25;
                }
                carry = (carry + cfg.windward_moisture_bonus * 0.5).clamp(0.0, 1.2);
            }

            let interior_penalty = cfg.interior_aridity_strength
                * (distance / (distance + 3.5)).min(1.0)
                * (1.0 - latitude_bonus.clamp(0.0, 1.0));
            humidity -= interior_penalty;

            let elev = elevation.sample(x as u32, y as u32);
            humidity += (elev - 0.5) * 0.08;

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
    mountain_scale: f32,
    ocean_cfg: &OceanConfig,
    seed: u64,
) -> ElevationField {
    let w = base_elevation.width as usize;
    let h = base_elevation.height as usize;
    let idx = |x: usize, y: usize| -> usize { y * w + x };

    let fold_band_width = mountains.fold_band_width() as f32 + 1.0;
    let fold_strength = cfg.fold_strength.max(0.0);
    let fault_strength_cfg = cfg.fault_strength.max(0.0);
    let volcanic_strength_cfg = cfg.volcanic_strength.max(0.0);
    let dome_strength_cfg = (cfg.plateau_density * 0.8).clamp(0.0, 0.4) + mountain_scale * 0.05;

    let mut values = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let base = base_elevation.sample(x as u32, y as u32);
            let i = idx(x, y);
            let mut v = base;

            if land[i] {
                if let Some(cell) = mountains.get(i) {
                    let relief = mountains.relief_scale(i);
                    match cell.ty {
                        MountainType::Fold => {
                            let ratio = (cell.strength as f32 / fold_band_width).clamp(0.0, 1.0);
                            v += fold_strength * ratio * relief;
                        }
                        MountainType::Fault => {
                            let ratio = (cell.strength as f32 / 8.0).clamp(0.0, 1.0);
                            v += fault_strength_cfg * ratio * relief;
                        }
                        MountainType::Volcanic => {
                            let ratio = (cell.strength as f32 / 12.0).clamp(0.0, 1.0);
                            v += volcanic_strength_cfg * ratio * relief;
                        }
                        MountainType::Dome => {
                            v += dome_strength_cfg * relief;
                        }
                    }
                }
            } else if is_ocean[i]
                && ocean_cfg.ridge_density > 0.0
                && ocean_cfg.ridge_amplitude.abs() > f32::EPSILON
            {
                let hash = terrain_hash(seed, x as u32, y as u32);
                let sample = (hash & 0xFFFF) as f32 / 65535.0;
                if sample < ocean_cfg.ridge_density {
                    v += ocean_cfg.ridge_amplitude * (1.0 - sample / ocean_cfg.ridge_density);
                }
            }

            values.push(v.clamp(0.0, 1.0));
        }
    }

    apply_coastal_smoothing(&mut values, w, h, land, land_distance);

    ElevationField::new(base_elevation.width, base_elevation.height, values)
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
        let weight = match distance {
            0 => 0.6,
            1 => 0.45,
            2 => 0.3,
            3 => 0.15,
            _ => 0.0,
        };
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

    #[test]
    fn polar_contrast_preset_builds_bands() {
        let presets = crate::map_preset::MapPresets::builtin();
        let preset = presets
            .get("polar_contrast")
            .expect("polar_contrast preset");
        let width = 48usize;
        let height = 32usize;
        let mut values = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let base = ((x + y) as f32) / ((width + height) as f32);
                values.push(base.fract());
            }
        }
        let elevation = ElevationField::new(width as u32, height as u32, values);
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
        );
        assert_eq!(bands.terrain.len(), width * height);
        assert!(bands.land_mask.iter().any(|&cell| cell));
        let (fold, fault, volcanic, _) = bands.mountains.iter_counts();
        assert!(fold > 0, "expected fold mountains");
        assert!(fault > 0, "expected fault mountains");
        assert!(volcanic > 0, "expected volcanic terrain");
    }

    #[test]
    fn earthlike_regression_metrics_stable() {
        let metrics = regression_metrics_for_preset("earthlike", Some(0xE47E_51DE_2024u64));
        assert!(
            (metrics.land_ratio - 0.392).abs() <= 0.01,
            "earthlike land ratio drift: {}",
            metrics.land_ratio
        );
        assert!(
            (metrics.fold as isize - 1122).abs() <= 32,
            "earthlike fold count drift: {}",
            metrics.fold
        );
        assert!(
            (metrics.fault as isize - 129).abs() <= 16,
            "earthlike fault count drift: {}",
            metrics.fault
        );
        assert!(
            (metrics.volcanic as isize - 16).abs() <= 6,
            "earthlike volcanic count drift: {}",
            metrics.volcanic
        );
        assert!(
            (metrics.dome as isize - 885).abs() <= 32,
            "earthlike dome count drift: {}",
            metrics.dome
        );
        assert!(
            (metrics.polar_fold as isize - 656).abs() <= 32,
            "earthlike polar fold drift: {}",
            metrics.polar_fold
        );
        assert!(
            (metrics.polar_fault as isize - 69).abs() <= 16,
            "earthlike polar fault drift: {}",
            metrics.polar_fault
        );
        assert!(
            (metrics.polar_uplift_cells as isize - 109).abs() <= 20,
            "earthlike polar uplift cells drift: {}",
            metrics.polar_uplift_cells
        );
        assert!(
            (metrics.polar_relief_cells as isize - 27).abs() <= 10,
            "earthlike polar relief cap drift: {}",
            metrics.polar_relief_cells
        );
    }

    #[test]
    fn polar_contrast_regression_metrics_stable() {
        let metrics = regression_metrics_for_preset("polar_contrast", None);
        assert!(
            (metrics.land_ratio - 0.414).abs() <= 0.01,
            "polar_contrast land ratio drift: {}",
            metrics.land_ratio
        );
        assert!(
            (metrics.fold as isize - 2048).abs() <= 40,
            "polar_contrast fold count drift: {}",
            metrics.fold
        );
        assert!(
            (metrics.fault as isize - 438).abs() <= 24,
            "polar_contrast fault count drift: {}",
            metrics.fault
        );
        assert!(
            (metrics.volcanic as isize - 62).abs() <= 10,
            "polar_contrast volcanic count drift: {}",
            metrics.volcanic
        );
        assert!(
            (metrics.dome as isize - 1041).abs() <= 40,
            "polar_contrast dome count drift: {}",
            metrics.dome
        );
        assert!(
            (metrics.polar_fold as isize - 823).abs() <= 36,
            "polar_contrast polar fold drift: {}",
            metrics.polar_fold
        );
        assert!(
            (metrics.polar_fault as isize - 187).abs() <= 18,
            "polar_contrast polar fault drift: {}",
            metrics.polar_fault
        );
        assert!(
            (metrics.polar_uplift_cells as isize - 163).abs() <= 14,
            "polar_contrast polar uplift cells drift: {}",
            metrics.polar_uplift_cells
        );
        assert!(
            (metrics.polar_relief_cells as isize - 121).abs() <= 18,
            "polar_contrast polar relief cap drift: {}",
            metrics.polar_relief_cells
        );
    }
}
