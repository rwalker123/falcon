use bevy::prelude::*;
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashSet},
    f32::consts::SQRT_2,
};

use crate::{
    components::Tile,
    heightfield::ElevationField,
    map_preset::MapPresetsHandle,
    resources::{SimulationConfig, TileRegistry},
};

use sim_runtime::{TerrainTags, TerrainType};

#[derive(Debug, Clone, Copy)]
pub struct RiverEdge {
    #[allow(dead_code)]
    pub from: UVec2,
    #[allow(dead_code)]
    pub dir: u8,
}

#[derive(Debug, Clone)]
pub struct RiverSegment {
    pub id: u32,
    pub order: u8,
    pub width: u8,
    pub path: Vec<UVec2>,
    pub edges: Vec<RiverEdge>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TerminationClass {
    Ocean,
    Lake,
    Wetland,
    Desert,
    Karst,
    Endorheic,
    None,
}

type RiverTraceResult = (
    Vec<RiverEdge>,
    Vec<UVec2>,
    Vec<(u32, u32, f32, f32)>,
    Option<TerminationClass>,
);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SourceCategory {
    Glacier,
    LakeOutlet,
    Runoff,
    Fallback,
}

fn termination_class_for(terrain: TerrainType, tags: TerrainTags) -> TerminationClass {
    use TerrainType::*;
    match terrain {
        DeepOcean | ContinentalShelf | CoralShelf | HydrothermalVentField => {
            TerminationClass::Ocean
        }
        InlandSea => TerminationClass::Lake,
        RiverDelta | MangroveSwamp | FreshwaterMarsh | TidalFlat | PeatHeath => {
            TerminationClass::Wetland
        }
        HotDesertErg | RockyReg | SemiAridScrub | SaltFlat | OasisBasin => TerminationClass::Desert,
        KarstHighland | KarstCavernMouth | SinkholeField | AquiferCeiling => {
            TerminationClass::Karst
        }
        Glacier | SeasonalSnowfield => TerminationClass::None,
        _ => {
            if tags.contains(TerrainTags::WETLAND) {
                TerminationClass::Wetland
            } else if tags.contains(TerrainTags::FRESHWATER) {
                TerminationClass::Lake
            } else if tags.contains(TerrainTags::ARID) {
                TerminationClass::Desert
            } else if tags.contains(TerrainTags::SUBSURFACE) {
                TerminationClass::Karst
            } else {
                TerminationClass::None
            }
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HydrologyState {
    pub rivers: Vec<RiverSegment>,
}

fn neighbor_dirs() -> &'static [(i32, i32)] {
    // 8-neighborhood: E, NE, N, NW, W, SW, S, SE
    &[
        (1, 0),
        (1, -1),
        (0, -1),
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]
}

struct RiverTraceContext<'a> {
    flow_dir: &'a [u8],
    width: u32,
    height: u32,
    elevation_field: &'a ElevationField,
    sea_level: f32,
    cost: &'a [f32],
    termination_classes: &'a [TerminationClass],
}

fn trace_river_path(
    start: UVec2,
    head_elevation: f32,
    max_allowed_elevation: f32,
    uphill_step_limit: usize,
    ctx: &RiverTraceContext,
) -> RiverTraceResult {
    let mut edges: Vec<RiverEdge> = Vec::new();
    let mut path: Vec<UVec2> = Vec::new();
    let mut samples: Vec<(u32, u32, f32, f32)> = Vec::new();
    let mut termination = None;
    let mut cx = start.x as i32;
    let mut cy = start.y as i32;
    let mut current_elev = head_elevation;
    let mut uphill_steps = 0usize;
    let max_steps = (ctx.width + ctx.height) as usize;
    let mut remaining_steps = max_steps;
    path.push(start);
    let start_idx = (start.y * ctx.width + start.x) as usize;
    samples.push((start.x, start.y, head_elevation, ctx.cost[start_idx]));

    while remaining_steps > 0 {
        remaining_steps -= 1;
        let idx = (cy as u32 * ctx.width + cx as u32) as usize;
        let dir = ctx.flow_dir[idx];
        if dir == 255 {
            termination = Some(TerminationClass::Endorheic);
            break;
        }
        let (dx, dy) = neighbor_dirs()[dir as usize];
        let nx = cx + dx;
        let ny = cy + dy;
        if nx < 0 || ny < 0 || nx >= ctx.width as i32 || ny >= ctx.height as i32 {
            break;
        }
        let next_elev = ctx.elevation_field.sample(nx as u32, ny as u32);
        if next_elev > max_allowed_elevation {
            break;
        }
        if next_elev > current_elev + f32::EPSILON {
            uphill_steps += 1;
            if uphill_steps > uphill_step_limit {
                break;
            }
        } else {
            uphill_steps = uphill_steps.saturating_sub(1);
        }
        let next_idx = (ny as u32 * ctx.width + nx as u32) as usize;
        edges.push(RiverEdge {
            from: UVec2::new(cx as u32, cy as u32),
            dir,
        });
        cx = nx;
        cy = ny;
        current_elev = next_elev;
        let next_pos = UVec2::new(cx as u32, cy as u32);
        path.push(next_pos);
        samples.push((next_pos.x, next_pos.y, next_elev, ctx.cost[next_idx]));
        let class = ctx
            .termination_classes
            .get(next_idx)
            .copied()
            .unwrap_or(TerminationClass::None);
        if next_elev <= ctx.sea_level {
            termination = Some(TerminationClass::Ocean);
            break;
        }
        if matches!(
            class,
            TerminationClass::Lake
                | TerminationClass::Wetland
                | TerminationClass::Desert
                | TerminationClass::Karst
                | TerminationClass::Ocean
        ) {
            termination = Some(class);
            break;
        }
    }

    (edges, path, samples, termination)
}

pub fn generate_hydrology(world: &mut World) {
    let (width, height, preset_opt, elevation_field) = {
        let cfg = world.resource::<SimulationConfig>().clone();
        let width = cfg.grid_size.x;
        let height = cfg.grid_size.y;
        let preset = if let Some(handle) = world.get_resource::<MapPresetsHandle>() {
            handle.get().get(&cfg.map_preset_id).cloned()
        } else {
            None
        };
        let elevation = world
            .get_resource::<ElevationField>()
            .cloned()
            .unwrap_or_else(|| crate::heightfield::build_elevation_field(&cfg, preset.as_ref()));
        (width, height, preset, elevation)
    };

    let sea_level = preset_opt.as_ref().map(|p| p.sea_level).unwrap_or(0.6);
    let river_density = preset_opt
        .as_ref()
        .map(|p| p.river_density)
        .unwrap_or(0.6)
        .clamp(0.1, 2.0);
    let accum_factor = preset_opt
        .as_ref()
        .map(|p| p.river_accum_threshold_factor)
        .unwrap_or(0.35)
        .clamp(0.05, 1.0);
    let min_accum = preset_opt
        .as_ref()
        .map(|p| p.river_min_accum)
        .unwrap_or(6)
        .max(1);
    let min_length = preset_opt
        .as_ref()
        .map(|p| p.river_min_length)
        .unwrap_or(8)
        .max(2);
    let fallback_min_length = preset_opt
        .as_ref()
        .map(|p| p.river_fallback_min_length)
        .unwrap_or(4)
        .max(2);

    let total_tiles_usize = (width * height) as usize;
    let mut flow_dir = vec![255u8; total_tiles_usize];
    let mut flow_accum = vec![0u16; total_tiles_usize];

    let mut termination_classes = vec![TerminationClass::None; total_tiles_usize];
    let mut tile_terrain: Vec<Option<(TerrainType, TerrainTags)>> = vec![None; total_tiles_usize];
    if let Some(registry) = world.get_resource::<TileRegistry>() {
        for (idx, &entity) in registry.tiles.iter().enumerate() {
            if idx >= termination_classes.len() {
                break;
            }
            if let Some(tile) = world.get::<Tile>(entity) {
                termination_classes[idx] = termination_class_for(tile.terrain, tile.terrain_tags);
                tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
            }
        }
    }

    let mut min_elev = 1.0f32;
    let mut max_elev = 0.0f32;
    let mut sum_elev = 0.0f32;
    let mut elev_samples: Vec<f32> = Vec::with_capacity(total_tiles_usize);
    let mut land_tiles = 0u32;
    let mut water_tiles = 0u32;

    let mut seamask = vec![false; total_tiles_usize];
    let mut cost = vec![f32::INFINITY; total_tiles_usize];
    let mut heap = BinaryHeap::new();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let elev = elevation_field.sample(x, y);
            min_elev = min_elev.min(elev);
            max_elev = max_elev.max(elev);
            sum_elev += elev;
            elev_samples.push(elev);
            if elev <= sea_level {
                seamask[idx] = true;
                water_tiles += 1;
                termination_classes[idx] = TerminationClass::Ocean;
                cost[idx] = 0.0;
                heap.push(HeapEntry { cost: 0.0, idx });
            } else {
                land_tiles += 1;
            }
        }
    }

    while let Some(HeapEntry {
        cost: current_cost,
        idx,
    }) = heap.pop()
    {
        if current_cost > cost[idx] {
            continue;
        }
        let cx = (idx as u32) % width;
        let cy = (idx as u32) / width;
        let elev_here = elevation_field.sample(cx, cy);
        for &(dx, dy) in neighbor_dirs() {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            let elev_next = elevation_field.sample(nx as u32, ny as u32);
            let slope_penalty = (elev_next - elev_here).max(0.0);
            let step_len = if dx == 0 || dy == 0 { 1.0 } else { SQRT_2 };
            let step_cost = slope_penalty + 0.01 * step_len;
            let new_cost = current_cost + step_cost;
            if new_cost + f32::EPSILON < cost[nidx] {
                cost[nidx] = new_cost;
                heap.push(HeapEntry {
                    cost: new_cost,
                    idx: nidx,
                });
            }
        }
    }

    let land_unreachable = cost
        .iter()
        .enumerate()
        .filter(|(idx, c)| !seamask[*idx] && !c.is_finite())
        .count();

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if seamask[idx] {
                flow_dir[idx] = 255;
                continue;
            }
            let mut best_dir: u8 = 255;
            let mut best_cost = cost[idx];
            for (d, &(dx, dy)) in neighbor_dirs().iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let nidx = (ny as u32 * width + nx as u32) as usize;
                if cost[nidx] < best_cost {
                    best_cost = cost[nidx];
                    best_dir = d as u8;
                }
            }
            if best_dir != 255 {
                flow_dir[idx] = best_dir;
                continue;
            }

            // Fallback to local downhill heuristic if cost map failed to provide direction.
            let elev = elevation_field.sample(x, y);
            let mut downhill_land_dir: u8 = 255;
            let mut downhill_land_elev = elev;
            let mut downhill_any_dir: u8 = 255;
            let mut downhill_any_elev = elev;
            let mut fallback_dir: u8 = 255;
            let mut fallback_elev = elev;

            for (d, &(dx, dy)) in neighbor_dirs().iter().enumerate() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                    continue;
                }
                let nelev = elevation_field.sample(nx as u32, ny as u32);

                if nelev < fallback_elev || fallback_dir == 255 {
                    fallback_elev = nelev;
                    fallback_dir = d as u8;
                }

                if nelev < downhill_any_elev || downhill_any_dir == 255 {
                    downhill_any_elev = nelev;
                    downhill_any_dir = d as u8;
                }

                if nelev > sea_level && (nelev < downhill_land_elev || downhill_land_dir == 255) {
                    downhill_land_elev = nelev;
                    downhill_land_dir = d as u8;
                }
            }

            let chosen_dir = if downhill_land_dir != 255 && downhill_land_elev < elev {
                downhill_land_dir
            } else if downhill_any_dir != 255 && downhill_any_elev < elev {
                downhill_any_dir
            } else if downhill_land_dir != 255 {
                downhill_land_dir
            } else if downhill_any_dir != 255 {
                downhill_any_dir
            } else {
                fallback_dir
            };

            flow_dir[idx] = chosen_dir;
        }
    }

    // Compute downstream mapping and upstream adjacency.
    let mut downstream: Vec<usize> = vec![usize::MAX; total_tiles_usize];
    let mut upstream: Vec<Vec<usize>> = vec![Vec::new(); total_tiles_usize];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let dir = flow_dir[idx];
            if dir == 255 {
                continue;
            }
            let (dx, dy) = neighbor_dirs()[dir as usize];
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            downstream[idx] = nidx;
            upstream[nidx].push(idx);
        }
    }

    let invalid_downstream = downstream
        .iter()
        .enumerate()
        .filter(|(idx, &d)| d == usize::MAX && !seamask[*idx])
        .count();

    let mut order: Vec<usize> = (0..total_tiles_usize).collect();
    order.sort_by(|a, b| cost[*b].partial_cmp(&cost[*a]).unwrap_or(Ordering::Equal));

    for accum in flow_accum.iter_mut().take(total_tiles_usize) {
        *accum = 1;
    }

    let mut orphan_tiles = 0usize;
    for idx in order {
        let downstream_idx = downstream[idx];
        if downstream_idx != usize::MAX {
            flow_accum[downstream_idx] = flow_accum[downstream_idx].saturating_add(flow_accum[idx]);
        } else if !seamask[idx] {
            orphan_tiles += 1;
        }
    }

    // Trace a handful of rivers from high-accum/high-elevation sources.
    let mut rivers: Vec<RiverSegment> = Vec::new();
    let trace_ctx = RiverTraceContext {
        flow_dir: &flow_dir,
        width,
        height,
        elevation_field: &elevation_field,
        sea_level,
        cost: &cost,
        termination_classes: &termination_classes,
    };
    let river_land_ratio = preset_opt
        .as_ref()
        .map(|p| p.river_land_ratio)
        .unwrap_or(300.0)
        .clamp(1.0, 10_000.0);
    let river_min_count = preset_opt
        .as_ref()
        .map(|p| p.river_min_count)
        .unwrap_or(2)
        .max(1);
    let river_max_count = preset_opt
        .as_ref()
        .map(|p| p.river_max_count)
        .unwrap_or(128)
        .max(river_min_count);
    let land_tile_count = land_tiles.max(1) as f32;
    let base_target = (land_tile_count / river_land_ratio).max(river_min_count as f32);
    let mut target_rivers = ((base_target * river_density).round() as usize)
        .max(river_min_count)
        .min(river_max_count);
    if target_rivers == 0 {
        target_rivers = river_min_count;
    }
    let source_percentile = preset_opt
        .as_ref()
        .map(|p| p.river_source_percentile)
        .unwrap_or(0.7)
        .clamp(0.0, 1.0);
    let sea_buffer = preset_opt
        .as_ref()
        .map(|p| p.river_source_sea_buffer)
        .unwrap_or(0.08)
        .max(0.0);
    let mut accum_sorted = flow_accum.clone();
    accum_sorted.sort_unstable();
    let accum_percentile = preset_opt
        .as_ref()
        .map(|p| p.river_accum_percentile)
        .unwrap_or(0.0)
        .clamp(0.0, 0.999);
    let percentile_threshold = if accum_percentile > 0.0 {
        quantile_u16(&accum_sorted, accum_percentile).round() as u16
    } else {
        0
    };
    let overall_max_accum = flow_accum.iter().copied().max().unwrap_or(0);
    let mut accumulation_threshold = if percentile_threshold > 0 {
        percentile_threshold
    } else {
        ((overall_max_accum as f32) * accum_factor).round() as u16
    };
    accumulation_threshold = accumulation_threshold
        .max(min_accum)
        .min(overall_max_accum.max(1))
        .max(1);

    let percentile_elev = quantile(&elev_samples, source_percentile);
    let headwater_threshold = percentile_elev.max(sea_level + sea_buffer);
    let fallback_threshold = sea_level + 0.05;

    let climb_headwater = |start_idx: usize, threshold: f32| -> usize {
        let mut stack = vec![start_idx];
        let mut visited = vec![false; total_tiles_usize];
        let mut best_idx = start_idx;
        let mut best_elev = {
            let x = start_idx as u32 % width;
            let y = start_idx as u32 / width;
            elevation_field.sample(x, y)
        };
        while let Some(idx) = stack.pop() {
            if visited[idx] {
                continue;
            }
            visited[idx] = true;
            let x = idx as u32 % width;
            let y = idx as u32 / width;
            let elev = elevation_field.sample(x, y);
            if elev >= threshold {
                return idx;
            }
            if elev > best_elev {
                best_idx = idx;
                best_elev = elev;
            }
            for &u in &upstream[idx] {
                stack.push(u);
            }
        }
        if best_elev >= fallback_threshold {
            best_idx
        } else {
            start_idx
        }
    };

    let mut glacier_heads: Vec<usize> = Vec::new();
    let mut lake_heads: Vec<usize> = Vec::new();
    let mut runoff_heads: Vec<usize> = Vec::new();
    let mut seen_heads: HashSet<usize> = HashSet::new();

    // Classify land tiles by terrain/tags for headwater prioritisation
    for idx in 0..total_tiles_usize {
        if seamask[idx] {
            continue;
        }
        let x = (idx as u32) % width;
        let y = (idx as u32) / width;
        let elev = elevation_field.sample(x, y);
        if let Some((terrain, tags)) = tile_terrain[idx] {
            let is_highland = tags.contains(TerrainTags::HIGHLAND);
            let is_glacial = matches!(
                terrain,
                TerrainType::Glacier
                    | TerrainType::SeasonalSnowfield
                    | TerrainType::AlpineMountain
                    | TerrainType::HighPlateau
                    | TerrainType::KarstHighland
            );
            if is_glacial || (is_highland && elev >= headwater_threshold) {
                if seen_heads.insert(idx) {
                    glacier_heads.push(idx);
                }
                continue;
            }
        }
    }

    // Lake outlets: land tiles adjacent to lake water tiles
    let mut lake_border: HashSet<usize> = HashSet::new();
    for (idx, term_class) in termination_classes
        .iter()
        .enumerate()
        .take(total_tiles_usize)
    {
        if *term_class != TerminationClass::Lake {
            continue;
        }
        let cx = (idx as u32) % width;
        let cy = (idx as u32) / width;
        for &(dx, dy) in neighbor_dirs() {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let nidx = (ny as u32 * width + nx as u32) as usize;
            if seamask[nidx] {
                continue;
            }
            if seen_heads.contains(&nidx) || lake_border.contains(&nidx) {
                continue;
            }
            lake_border.insert(nidx);
            seen_heads.insert(nidx);
            lake_heads.push(nidx);
        }
    }

    // High-slope runoff tiles
    let slope_threshold = 0.04f32;
    for (idx, &is_sea) in seamask.iter().enumerate().take(total_tiles_usize) {
        if is_sea {
            continue;
        }
        if seen_heads.contains(&idx) {
            continue;
        }
        let x = (idx as u32) % width;
        let y = (idx as u32) / width;
        let elev = elevation_field.sample(x, y);
        if elev < headwater_threshold {
            continue;
        }
        let mut max_drop = 0.0f32;
        for &(dx, dy) in neighbor_dirs() {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let neighbor_elev = elevation_field.sample(nx as u32, ny as u32);
            max_drop = max_drop.max(elev - neighbor_elev);
        }
        if max_drop >= slope_threshold && seen_heads.insert(idx) {
            runoff_heads.push(idx);
        }
    }

    // Fallback candidates ordered by accumulation
    let mut fallback_heads: Vec<usize> = (0..total_tiles_usize)
        .filter(|idx| !seamask[*idx] && !seen_heads.contains(idx))
        .collect();
    fallback_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));

    glacier_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));
    lake_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));
    runoff_heads.sort_unstable_by(|a, b| flow_accum[*b].cmp(&flow_accum[*a]));

    let glacier_sources: Vec<(usize, usize)> = glacier_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, headwater_threshold);
            (idx, head_idx)
        })
        .collect();
    let lake_sources: Vec<(usize, usize)> = lake_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, headwater_threshold);
            (idx, head_idx)
        })
        .collect();
    let runoff_sources: Vec<(usize, usize)> = runoff_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, headwater_threshold);
            (idx, head_idx)
        })
        .collect();
    let fallback_sources: Vec<(usize, usize)> = fallback_heads
        .iter()
        .map(|&idx| {
            let head_idx = climb_headwater(idx, fallback_threshold);
            (idx, head_idx)
        })
        .collect();

    let candidate_total =
        glacier_sources.len() + lake_sources.len() + runoff_sources.len() + fallback_sources.len();

    let max_accum = glacier_sources
        .iter()
        .chain(lake_sources.iter())
        .chain(runoff_sources.iter())
        .chain(fallback_sources.iter())
        .map(|(idx, _)| flow_accum[*idx])
        .max()
        .unwrap_or(0);

    let fallback_sources_clone = fallback_sources.clone();
    let source_groups = vec![
        (SourceCategory::Glacier, glacier_sources),
        (SourceCategory::LakeOutlet, lake_sources),
        (SourceCategory::Runoff, runoff_sources),
        (SourceCategory::Fallback, fallback_sources),
    ];
    let sink_tiles = flow_dir
        .iter()
        .enumerate()
        .filter(|(idx, &dir)| dir == 255 && !seamask[*idx])
        .count();
    let mut taken = 0;
    let total_tiles = (width * height) as f32;
    let mean_elev = if total_tiles == 0.0 {
        0.0
    } else {
        sum_elev / total_tiles
    };
    elev_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_elev = quantile(&elev_samples, 0.5);

    let max_flow_total = flow_accum.iter().copied().max().unwrap_or(0);
    let tiles_over_one = flow_accum.iter().filter(|&&v| v > 1).count();
    tracing::debug!(
        target: "shadow_scale::mapgen",
        candidates = candidate_total,
        accumulation_threshold,
        max_accum,
        max_flow_total,
        tiles_over_one,
        target_rivers,
        headwater_threshold,
        source_percentile = source_percentile,
        sea_buffer,
        sink_tiles,
        invalid_downstream,
        orphan_tiles,
        land_unreachable,
        land_tiles,
        water_tiles,
        elev_mean = mean_elev,
        elev_median = median_elev,
        accum_p25 = quantile_u16(&accum_sorted, 0.25),
        accum_p50 = quantile_u16(&accum_sorted, 0.5),
        accum_p75 = quantile_u16(&accum_sorted, 0.75),
        accum_p90 = quantile_u16(&accum_sorted, 0.9),
        percentile_threshold,
        accum_percentile
    );

    let mut sample_paths_logged = 0usize;
    let mut accepted_heads = HashSet::new();
    let mut spacing_sq = preset_opt
        .as_ref()
        .map(|p| p.river_min_spacing)
        .unwrap_or(12.0)
        .max(0.0);
    spacing_sq *= spacing_sq;
    let mut pass = 0;
    let uphill_step_limit = preset_opt
        .as_ref()
        .map(|p| p.river_uphill_step_limit as usize)
        .unwrap_or(2);
    let uphill_gain_pct = preset_opt
        .as_ref()
        .map(|p| p.river_uphill_gain_pct)
        .unwrap_or(0.05)
        .max(0.0);

    while pass < 3 && taken < target_rivers {
        for (category, sources) in &source_groups {
            for &(base_idx, head_idx) in sources {
                if taken >= target_rivers {
                    break;
                }
                if accepted_heads.contains(&head_idx) {
                    continue;
                }
                let acc = flow_accum[base_idx];
                if *category == SourceCategory::Fallback && acc < accumulation_threshold {
                    continue;
                }
                let sx = head_idx as u32 % width;
                let sy = head_idx as u32 / width;
                if spacing_sq > 0.0 {
                    let mut too_close = false;
                    for r in rivers.iter() {
                        if let Some(&p) = r.path.first() {
                            let dx = p.x as i32 - sx as i32;
                            let dy = p.y as i32 - sy as i32;
                            if (dx * dx + dy * dy) as f32 <= spacing_sq {
                                too_close = true;
                                break;
                            }
                        }
                    }
                    if too_close {
                        continue;
                    }
                }

                let head_pos = UVec2::new(sx, sy);
                let head_elev = elevation_field.sample(sx, sy);
                let max_allowed_elev = head_elev * (1.0 + uphill_gain_pct);
                let (edges, path, samples, termination) = trace_river_path(
                    head_pos,
                    head_elev,
                    max_allowed_elev,
                    uphill_step_limit,
                    &trace_ctx,
                );
                let path_len = path.len();
                if path_len == 0 {
                    continue;
                }
                if sample_paths_logged < 2 {
                    tracing::debug!(
                        target: "shadow_scale::mapgen",
                        category = ?category,
                        sx,
                        sy,
                        acc,
                        path = ?samples,
                        "hydrology.sample_path"
                    );
                    sample_paths_logged += 1;
                }
                tracing::info!(
                    target: "shadow_scale::mapgen",
                    category = ?category,
                    acc,
                    sx,
                    sy,
                    path_len,
                    termination = ?termination,
                    threshold = accumulation_threshold,
                    "hydrology.candidate_trace"
                );
                if path_len >= min_length || path_len >= fallback_min_length {
                    taken += 1;
                    accepted_heads.insert(head_idx);
                    rivers.push(RiverSegment {
                        id: taken as u32,
                        order: 1,
                        width: 1,
                        path,
                        edges,
                    });
                }
            }
        }
        if spacing_sq == 0.0 {
            break;
        }
        spacing_sq *= 0.5;
        if spacing_sq < 1.0 {
            spacing_sq = 0.0;
        }
        pass += 1;
    }
    if rivers.is_empty() {
        if let Some(&(base_idx, head_idx)) = fallback_sources_clone.first() {
            let acc = flow_accum[base_idx];
            if acc >= 1 {
                let sx = head_idx as u32 % width;
                let sy = head_idx as u32 / width;
                let head_pos = UVec2::new(sx, sy);
                let head_elev = elevation_field.sample(sx, sy);
                let max_allowed_elev = head_elev * (1.0 + uphill_gain_pct);
                let (edges, path, samples, termination) = trace_river_path(
                    head_pos,
                    head_elev,
                    max_allowed_elev,
                    uphill_step_limit,
                    &trace_ctx,
                );
                let path_len = path.len();
                tracing::debug!(
                    target: "shadow_scale::mapgen",
                    category = ?SourceCategory::Fallback,
                    sx,
                    sy,
                    acc,
                    path = ?samples,
                    "hydrology.sample_path_fallback"
                );
                tracing::info!(
                    target: "shadow_scale::mapgen",
                    category = ?SourceCategory::Fallback,
                    acc,
                    sx,
                    sy,
                    path_len,
                    termination = ?termination,
                    fallback_min_length,
                    "hydrology.fallback_trace"
                );
                if path_len >= fallback_min_length {
                    taken += 1;
                    rivers.push(RiverSegment {
                        id: taken as u32,
                        order: 1,
                        width: 1,
                        path,
                        edges,
                    });
                }
            }
        }
    }

    let mut state = world
        .remove_resource::<HydrologyState>()
        .unwrap_or_default();
    state.rivers = rivers;
    let river_count = state.rivers.len();
    let total_edges: usize = state.rivers.iter().map(|r| r.edges.len()).sum();
    world.insert_resource(state);

    tracing::info!(
        target: "shadow_scale::mapgen",
        rivers = river_count,
        candidates = candidate_total,
        max_accum,
        accumulation_threshold,
        total_edges,
        "hydrology.generated"
    );
}

fn quantile(values: &[f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    values[idx]
}

fn quantile_u16(values: &[u16], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f32 * q.clamp(0.0, 1.0)).round() as usize;
    values[idx] as f32
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HydrologyState>();
};

#[derive(Copy, Clone, Debug)]
struct HeapEntry {
    cost: f32,
    idx: usize,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.idx == other.idx
    }
}

impl Eq for HeapEntry {}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
            .then_with(|| self.idx.cmp(&other.idx))
    }
}
