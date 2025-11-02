use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
};

use bevy::prelude::*;

use crate::{
    heightfield::ElevationField,
    map_preset::{InlandSeaConfig, IslandConfig, MacroLandConfig, OceanConfig, ShelfConfig},
};

#[derive(Debug, Clone)]
pub struct BandsResult {
    pub terrain: Vec<TerrainBand>,
    pub ocean_distance: Vec<u32>,
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

pub fn build_bands(
    elevation: &ElevationField,
    sea_level: f32,
    macro_cfg: &MacroLandConfig,
    shelf: &ShelfConfig,
    islands: &IslandConfig,
    inland: &InlandSeaConfig,
    ocean_cfg: &OceanConfig,
) -> BandsResult {
    let w = elevation.width as usize;
    let h = elevation.height as usize;
    let idx = |x: usize, y: usize| -> usize { y * w + x };

    let LandMask { mask, land_count } = generate_land_mask(elevation, macro_cfg, sea_level);
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
    place_islands(&mut land, &mut is_ocean, islands, shelf, w, h);
    is_ocean = compute_ocean_mask(&land, w, h);

    // Distance transform and classification
    let ocean_distance = compute_ocean_distance(&land, w, h);
    let terrain = classify_bands(&land, &is_ocean, &ocean_distance, shelf, w, h);

    // Stamp a monotone elevation consistent with bands (rescale original field)
    let mut values = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let i = idx(x, y);
            let deep_value = if ocean_cfg.ridge_density > 0.0
                && ocean_cfg.ridge_amplitude.abs() > f32::EPSILON
            {
                let hash = terrain_hash(x as u32, y as u32);
                let sample = (hash & 0xFFFF) as f32 / 65535.0;
                if sample < ocean_cfg.ridge_density {
                    -0.90 + ocean_cfg.ridge_amplitude * (1.0 - sample / ocean_cfg.ridge_density)
                } else {
                    -0.90
                }
            } else {
                -0.90
            };
            let v = match terrain[i] {
                TerrainBand::Land => 0.10 + 0.40 * 1.0, // positive above 0
                TerrainBand::ContinentalShelf => -0.02 - 0.08 * 1.0,
                TerrainBand::ContinentalSlope => -0.10 - 0.40 * 1.0,
                TerrainBand::DeepOcean => deep_value,
                TerrainBand::InlandSea => -0.06, // shallow lacustrine
            };
            values.push(v);
        }
    }
    let elevation = ElevationField::new(elevation.width, elevation.height, values);

    BandsResult {
        terrain,
        ocean_distance,
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
) {
    // Very lightweight placement: random samples along slope fringe for continental fragments
    // and in abyssal for oceanic islands.
    let idx = |x: usize, y: usize| -> usize { y * w + x };
    let mut seed: u64 = 0xA51C_E55E;
    let mut rng = move || {
        // xorshift64*
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        (seed.wrapping_mul(2685821657736338717u64) >> 32) as u32
    };

    // Continental fragments along slope fringe (distance in [shelf, shelf+slope])
    let fringe_min = shelf.width_tiles as usize;
    let fringe_max = (shelf.width_tiles + shelf.slope_width_tiles) as usize;
    let mut placed_cf = 0u32;
    let target_cf = ((w * h) as f32 * islands.continental_density) as u32;
    for _ in 0..(target_cf * 10).max(100) {
        if placed_cf >= target_cf {
            break;
        }
        let x = (rng() as usize) % w;
        let y = (rng() as usize) % h;
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
            carve_blob_into(land, is_ocean, w, h, x, y, 1 + (rng() % 2) as usize);
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
        let x = (rng() as usize) % w;
        let y = (rng() as usize) % h;
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
                (terrain_hash(x as u32, y as u32) & 0xFFFF) as f32 / 65535.0 * jitter_scale
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
        "mapgen.validate.coastal_bands"
    );
}

fn terrain_hash(x: u32, y: u32) -> u32 {
    let mut n = x;
    n = n.wrapping_mul(0x6C8E_9CF5) ^ y.wrapping_mul(0xB529_7A4D);
    n ^= n >> 13;
    n = n.wrapping_mul(0x68E3_1DA4);
    n ^= n >> 11;
    n = n.wrapping_mul(0x1B56_C4E9);
    n ^ (n >> 16)
}
