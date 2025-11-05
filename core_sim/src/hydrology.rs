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
    mapgen::WorldGenSeed,
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
    termination: TerminationClass,
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

type FlowCandidateMetrics = (u8, f32, f32, f32);
type NeighborCandidateState = (usize, i32, i32, f32, f32, TerminationClass);
type CandidateEntry = (FlowCandidateMetrics, NeighborCandidateState);

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

fn path_meets_length(
    category: SourceCategory,
    path_len: usize,
    min_length: usize,
    fallback_min_length: usize,
) -> bool {
    if path_len >= min_length {
        return true;
    }
    matches!(category, SourceCategory::Fallback) && path_len >= fallback_min_length
}

fn is_water_terrain(terrain: TerrainType) -> bool {
    matches!(
        terrain,
        TerrainType::DeepOcean
            | TerrainType::ContinentalShelf
            | TerrainType::CoralShelf
            | TerrainType::HydrothermalVentField
            | TerrainType::InlandSea
    )
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
    width: u32,
    height: u32,
    elevation_field: &'a ElevationField,
    cost: &'a [f32],
    termination_classes: &'a [TerminationClass],
}

fn trace_river_path(
    start: UVec2,
    head_elevation: f32,
    max_allowed_elevation: f32,
    ctx: &RiverTraceContext,
) -> RiverTraceResult {
    let mut edges: Vec<RiverEdge> = Vec::new();
    let mut path: Vec<UVec2> = Vec::new();
    let mut samples: Vec<(u32, u32, f32, f32)> = Vec::new();
    let mut termination = None;
    let mut best_termination: Option<TerminationClass> = None;
    let mut visited = HashSet::new();
    let mut cx = start.x as i32;
    let mut cy = start.y as i32;
    let max_steps = (ctx.width + ctx.height) as usize;
    let mut remaining_steps = max_steps;
    let head_limit = max_allowed_elevation.max(head_elevation);

    let start_idx = (start.y * ctx.width + start.x) as usize;
    path.push(start);
    samples.push((start.x, start.y, head_elevation, ctx.cost[start_idx]));

    while remaining_steps > 0 {
        remaining_steps -= 1;
        let idx = (cy as u32 * ctx.width + cx as u32) as usize;
        let current_cost = ctx.cost[idx];
        let current_pos = UVec2::new(cx as u32, cy as u32);

        if visited.contains(&idx) {
            termination = best_termination.or(Some(TerminationClass::Endorheic));
            break;
        }
        visited.insert(idx);

        if let Some(class) = ctx.termination_classes.get(idx).copied() {
            match class {
                TerminationClass::Ocean => {
                    termination = Some(TerminationClass::Ocean);
                    break;
                }
                TerminationClass::Lake
                | TerminationClass::Wetland
                | TerminationClass::Desert
                | TerminationClass::Karst => {
                    best_termination.get_or_insert(class);
                }
                _ => {}
            }
        }
        let mut best_candidate: Option<CandidateEntry> = None;
        for (dir_idx, &(dx, dy)) in neighbor_dirs().iter().enumerate() {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx < 0 || ny < 0 || nx >= ctx.width as i32 || ny >= ctx.height as i32 {
                continue;
            }
            let nidx = (ny as u32 * ctx.width + nx as u32) as usize;
            if visited.contains(&nidx) {
                continue;
            }
            let neighbor_cost = ctx.cost[nidx];
            if !neighbor_cost.is_finite() {
                continue;
            }
            let neighbor_elev = ctx.elevation_field.sample(nx as u32, ny as u32);
            if neighbor_elev > head_limit + f32::EPSILON {
                continue;
            }

            let downhill = neighbor_cost + 1e-6 < current_cost;
            let equalish = (neighbor_cost - current_cost).abs() <= 1e-6;
            if !downhill && !equalish {
                // Allow gentle pooling only if still below the head limit.
                if neighbor_elev + f32::EPSILON < head_limit {
                    // permitted small uphill, keep
                } else {
                    continue;
                }
            }

            let class = ctx
                .termination_classes
                .get(nidx)
                .copied()
                .unwrap_or(TerminationClass::None);
            let step_len = if dx == 0 || dy == 0 { 1.0 } else { SQRT_2 };
            let ranking_key = (
                if downhill {
                    0u8
                } else if equalish {
                    1u8
                } else {
                    2u8
                },
                neighbor_cost,
                neighbor_elev,
                step_len,
            );

            if best_candidate
                .as_ref()
                .map(|(best_key, _)| ranking_key < *best_key)
                .unwrap_or(true)
            {
                best_candidate = Some((
                    ranking_key,
                    (dir_idx, nx, ny, neighbor_cost, neighbor_elev, class),
                ));
            }
        }

        let Some((_, (dir_idx, nx, ny, neighbor_cost, neighbor_elev, class))) = best_candidate
        else {
            termination = best_termination.or(Some(TerminationClass::Endorheic));
            break;
        };

        let dir = dir_idx as u8;
        edges.push(RiverEdge {
            from: current_pos,
            dir,
        });

        cx = nx;
        cy = ny;
        let next_pos = UVec2::new(cx as u32, cy as u32);
        path.push(next_pos);
        samples.push((next_pos.x, next_pos.y, neighbor_elev, neighbor_cost));

        if matches!(
            class,
            TerminationClass::Lake
                | TerminationClass::Wetland
                | TerminationClass::Desert
                | TerminationClass::Karst
        ) && best_termination.is_none()
        {
            best_termination = Some(class);
        }
    }

    if termination.is_none() {
        termination = best_termination;
    }

    (edges, path, samples, termination)
}

pub fn generate_hydrology(world: &mut World) {
    let cfg = world.resource::<SimulationConfig>().clone();
    let (width, height, preset_opt, elevation_field) = {
        let width = cfg.grid_size.x;
        let height = cfg.grid_size.y;
        let preset = if let Some(handle) = world.get_resource::<MapPresetsHandle>() {
            handle.get().get(&cfg.map_preset_id).cloned()
        } else {
            None
        };
        let seed = world
            .get_resource::<WorldGenSeed>()
            .map(|s| s.0)
            .unwrap_or(0);
        let elevation = world
            .get_resource::<ElevationField>()
            .cloned()
            .unwrap_or_else(|| {
                crate::heightfield::build_elevation_field(&cfg, preset.as_ref(), seed)
            });
        (width, height, preset, elevation)
    };

    let sea_level = preset_opt.as_ref().map(|p| p.sea_level).unwrap_or(0.6);
    let overrides = cfg.hydrology.clone();
    let base_river_density = preset_opt.as_ref().map(|p| p.river_density).unwrap_or(0.6);
    let river_density = overrides
        .river_density
        .unwrap_or(base_river_density)
        .clamp(0.1, 5.0);
    let base_accum_factor = preset_opt
        .as_ref()
        .map(|p| p.river_accum_threshold_factor)
        .unwrap_or(0.35);
    let accum_factor = overrides
        .accumulation_threshold_factor
        .unwrap_or(base_accum_factor)
        .clamp(0.05, 2.0);
    let base_min_accum = preset_opt
        .as_ref()
        .map(|p| p.river_min_accum)
        .unwrap_or(6)
        .max(1);
    let min_accum = base_min_accum;
    let base_min_length = preset_opt
        .as_ref()
        .map(|p| p.river_min_length)
        .unwrap_or(8)
        .max(2);
    let min_length = overrides.min_length.unwrap_or(base_min_length).max(2);
    let base_fallback_min_length = preset_opt
        .as_ref()
        .map(|p| p.river_fallback_min_length)
        .unwrap_or(4)
        .max(2);
    let fallback_min_length = overrides
        .fallback_min_length
        .unwrap_or(base_fallback_min_length)
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
            let mut treat_as_water = elev <= sea_level;
            if let Some((terrain, _)) = tile_terrain[idx] {
                if !is_water_terrain(terrain) {
                    treat_as_water = false;
                }
            }

            if treat_as_water {
                seamask[idx] = true;
                water_tiles += 1;
                termination_classes[idx] = TerminationClass::Ocean;
                cost[idx] = 0.0;
                heap.push(HeapEntry { cost: 0.0, idx });
            } else {
                land_tiles += 1;
                if termination_classes[idx] == TerminationClass::Ocean {
                    termination_classes[idx] = tile_terrain[idx]
                        .map(|(terrain, tags)| termination_class_for(terrain, tags))
                        .unwrap_or(TerminationClass::None);
                }
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
    let mut river_tiles: HashSet<usize> = HashSet::new();
    let trace_ctx = RiverTraceContext {
        width,
        height,
        elevation_field: &elevation_field,
        cost: &cost,
        termination_classes: &termination_classes,
    };
    let river_land_ratio = preset_opt
        .as_ref()
        .map(|p| p.river_land_ratio)
        .unwrap_or(300.0)
        .clamp(1.0, 10_000.0);
    let base_river_min_count = preset_opt
        .as_ref()
        .map(|p| p.river_min_count)
        .unwrap_or(2)
        .max(1);
    let base_river_max_count = preset_opt
        .as_ref()
        .map(|p| p.river_max_count)
        .unwrap_or(128)
        .max(base_river_min_count);
    let river_min_count = overrides.river_min_count.unwrap_or(base_river_min_count);
    let river_max_count = overrides
        .river_max_count
        .unwrap_or(base_river_max_count)
        .max(river_min_count);
    let land_tile_count = land_tiles.max(1) as f32;
    let base_target = (land_tile_count / river_land_ratio).max(river_min_count as f32);
    let mut target_rivers = ((base_target * river_density).round() as usize)
        .max(river_min_count)
        .min(river_max_count);
    if target_rivers == 0 {
        target_rivers = river_min_count;
    }
    elev_samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let base_source_percentile = preset_opt
        .as_ref()
        .map(|p| p.river_source_percentile)
        .unwrap_or(0.7);
    let source_percentile = overrides
        .source_percentile
        .unwrap_or(base_source_percentile)
        .clamp(0.0, 1.0);
    let base_sea_buffer = preset_opt
        .as_ref()
        .map(|p| p.river_source_sea_buffer)
        .unwrap_or(0.08)
        .max(0.0);
    let sea_buffer = overrides
        .source_sea_buffer
        .unwrap_or(base_sea_buffer)
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
    let base_spacing = preset_opt
        .as_ref()
        .map(|p| p.river_min_spacing)
        .unwrap_or(12.0)
        .max(0.0);
    let mut spacing_sq = overrides.spacing.unwrap_or(base_spacing).max(0.0);
    spacing_sq *= spacing_sq;
    let mut pass = 0;
    let uphill_gain_pct = overrides
        .uphill_gain_pct
        .or_else(|| preset_opt.as_ref().map(|p| p.river_uphill_gain_pct))
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
                let (edges, path, samples, termination) =
                    trace_river_path(head_pos, head_elev, max_allowed_elev, &trace_ctx);
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
                let connects_existing = path.iter().skip(1).any(|pos| {
                    let idx = (pos.y * width + pos.x) as usize;
                    river_tiles.contains(&idx)
                });

                let allow_seed_short = rivers.is_empty();
                let allow_short = matches!(category, SourceCategory::Fallback)
                    || connects_existing
                    || allow_seed_short;

                let acceptable =
                    path_meets_length(*category, path_len, min_length, fallback_min_length)
                        || (allow_short && path_len >= fallback_min_length);

                if acceptable {
                    taken += 1;
                    accepted_heads.insert(head_idx);
                    rivers.push(RiverSegment {
                        id: taken as u32,
                        order: 1,
                        width: 1,
                        path,
                        edges,
                        termination: termination.unwrap_or(TerminationClass::None),
                    });
                    for pos in rivers.last().unwrap().path.iter() {
                        let idx = (pos.y * width + pos.x) as usize;
                        river_tiles.insert(idx);
                    }
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
                let (edges, path, samples, termination) =
                    trace_river_path(head_pos, head_elev, max_allowed_elev, &trace_ctx);
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
                let connects_existing = path.iter().skip(1).any(|pos| {
                    let idx = (pos.y * width + pos.x) as usize;
                    river_tiles.contains(&idx)
                });

                let allow_seed_short = rivers.is_empty();
                let allow_short = connects_existing || allow_seed_short;

                let acceptable = path_meets_length(
                    SourceCategory::Fallback,
                    path_len,
                    min_length,
                    fallback_min_length,
                ) || (allow_short && path_len >= fallback_min_length);

                if acceptable {
                    taken += 1;
                    rivers.push(RiverSegment {
                        id: taken as u32,
                        order: 1,
                        width: 1,
                        path,
                        edges,
                        termination: termination.unwrap_or(TerminationClass::None),
                    });
                    for pos in rivers.last().unwrap().path.iter() {
                        let idx = (pos.y * width + pos.x) as usize;
                        river_tiles.insert(idx);
                    }
                }
            }
        }
    }

    // Compute per-tile Strahler orders to classify tributary strength.
    let mut topo: Vec<usize> = (0..total_tiles_usize).collect();
    topo.sort_unstable_by(|a, b| cost[*b].partial_cmp(&cost[*a]).unwrap_or(Ordering::Equal));
    let mut tile_orders: Vec<u8> = vec![0; total_tiles_usize];
    for idx in topo {
        let parents = &upstream[idx];
        if parents.is_empty() {
            tile_orders[idx] = 1;
            continue;
        }
        let mut max_order = 0u8;
        let mut duplicate_max = 0u8;
        for &p in parents {
            let order = tile_orders[p].max(1);
            if order > max_order {
                max_order = order;
                duplicate_max = 1;
            } else if order == max_order {
                duplicate_max = duplicate_max.saturating_add(1);
            }
        }
        let mut order_here = if duplicate_max >= 2 {
            max_order.saturating_add(1)
        } else {
            max_order
        };
        if order_here == 0 {
            order_here = 1;
        }
        tile_orders[idx] = order_here;
    }

    let mut total_length = 0usize;
    let mut max_order_seg = 0u8;
    let mut tributary_segments = 0usize;
    let mut delta_segment_count = 0usize;
    let mut delta_candidates: Vec<usize> = Vec::new();
    for segment in rivers.iter_mut() {
        total_length += segment.path.len();
        let mut seg_order = 1u8;
        let mut max_acc = 0u16;
        for pos in &segment.path {
            let idx = (pos.y * width + pos.x) as usize;
            seg_order = seg_order.max(tile_orders.get(idx).copied().unwrap_or(1));
            max_acc = max_acc.max(flow_accum[idx]);
        }
        segment.order = seg_order.max(1);
        if segment.order > 1 {
            tributary_segments += 1;
        }
        max_order_seg = max_order_seg.max(segment.order);
        let width_val = ((max_acc.max(1) as f32).log2().floor() as i32 + 1).max(1);
        segment.width = width_val.clamp(1, u8::MAX as i32) as u8;

        if matches!(segment.termination, TerminationClass::Ocean) {
            delta_segment_count += 1;
            if let Some(delta_idx) = segment.path.iter().rev().find_map(|pos| {
                let idx = (pos.y * width + pos.x) as usize;
                if !seamask[idx] {
                    Some(idx)
                } else {
                    None
                }
            }) {
                delta_candidates.push(delta_idx);
            }
        }
    }

    let mut delta_tiles_applied = 0usize;
    let updates: Vec<(usize, Entity)> = if let Some(registry) = world.get_resource::<TileRegistry>()
    {
        let mut unique = HashSet::new();
        let mut collected = Vec::new();
        for &idx in &delta_candidates {
            if unique.insert(idx) {
                if let Some(&entity) = registry.tiles.get(idx) {
                    collected.push((idx, entity));
                }
            }
        }
        collected
    } else {
        Vec::new()
    };

    for (idx, entity) in updates {
        if let Some(mut tile) = world.get_mut::<Tile>(entity) {
            if tile.terrain != TerrainType::RiverDelta {
                tile.terrain = TerrainType::RiverDelta;
                tile.terrain_tags |= TerrainTags::WETLAND;
                tile.terrain_tags |= TerrainTags::FRESHWATER;
                delta_tiles_applied += 1;
                tile_terrain[idx] = Some((tile.terrain, tile.terrain_tags));
            }
        }
    }

    let river_count = rivers.len();
    let total_edges: usize = rivers.iter().map(|r| r.edges.len()).sum();
    let avg_length = if river_count == 0 {
        0.0
    } else {
        total_length as f32 / river_count as f32
    };

    let mut state = world
        .remove_resource::<HydrologyState>()
        .unwrap_or_default();
    state.rivers = rivers;
    world.insert_resource(state);

    tracing::info!(
        target: "shadow_scale::mapgen",
        rivers = river_count,
        candidates = candidate_total,
        max_accum,
        avg_length,
        max_order = max_order_seg,
        tributaries = tributary_segments,
        delta_segments = delta_segment_count,
        delta_tiles = delta_tiles_applied,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn idx(width: u32, x: u32, y: u32) -> usize {
        (y * width + x) as usize
    }

    #[test]
    fn river_traces_through_wetland_until_ocean() {
        let width = 5u32;
        let height = 5u32;
        let mut elevations = vec![0.9f32; (width * height) as usize];
        elevations[idx(width, 2, 4)] = 0.95;
        elevations[idx(width, 2, 3)] = 0.85;
        elevations[idx(width, 2, 2)] = 0.72;
        elevations[idx(width, 2, 1)] = 0.65;
        elevations[idx(width, 2, 0)] = 0.4;
        for x in 0..width {
            elevations[idx(width, x, 0)] = 0.4;
        }

        let elevation_field = ElevationField::new(width, height, elevations);
        let mut cost = vec![f32::INFINITY; (width * height) as usize];
        cost[idx(width, 2, 4)] = 10.0;
        cost[idx(width, 2, 3)] = 8.0;
        cost[idx(width, 2, 2)] = 6.0;
        cost[idx(width, 2, 1)] = 3.0;
        cost[idx(width, 2, 0)] = 0.0;

        let mut termination_classes = vec![TerminationClass::None; (width * height) as usize];
        termination_classes[idx(width, 2, 2)] = TerminationClass::Wetland;
        termination_classes[idx(width, 2, 0)] = TerminationClass::Ocean;
        for x in 0..width {
            termination_classes[idx(width, x, 0)] = TerminationClass::Ocean;
        }

        let ctx = RiverTraceContext {
            width,
            height,
            elevation_field: &elevation_field,
            cost: &cost,
            termination_classes: &termination_classes,
        };

        let start = UVec2::new(2, 4);
        let head_elev = ctx.elevation_field.sample(start.x, start.y);
        let max_allowed_elevation = head_elev * 1.05;

        let (_edges, path, _samples, termination) =
            trace_river_path(start, head_elev, max_allowed_elevation, &ctx);

        assert!(
            path.contains(&UVec2::new(2, 2)),
            "river never entered wetland tile"
        );
        assert_eq!(path.last(), Some(&UVec2::new(2, 0)));
        assert!(
            path.len() >= 5,
            "expected at least 5 points, got {}",
            path.len()
        );
        assert_eq!(termination, Some(TerminationClass::Ocean));
    }

    #[test]
    fn generates_river_to_ocean_on_small_grid() {
        use crate::{
            components::{ElementKind, Tile},
            mapgen::WorldGenSeed,
            resources::{SimulationConfig, TileRegistry},
            scalar::scalar_zero,
        };

        let width = 6u32;
        let height = 6u32;

        let mut world = World::new();
        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(width, height);
        config.map_preset_id = "debug".to_string();
        config.hydrology.min_length = Some(4);
        config.hydrology.fallback_min_length = Some(3);
        config.hydrology.river_density = Some(1.0);
        world.insert_resource(config);
        world.insert_resource(WorldGenSeed(0));

        let mut elevations = vec![0.7f32; (width * height) as usize];
        for x in 0..width {
            elevations[idx(width, x, 0)] = 0.2; // ocean row
            elevations[idx(width, x, 1)] = 0.55; // coast lowland
        }
        // carve a valley leading from (3,5) to the ocean with a wetland in the middle
        elevations[idx(width, 3, 5)] = 0.9;
        elevations[idx(width, 3, 4)] = 0.82;
        elevations[idx(width, 3, 3)] = 0.74;
        elevations[idx(width, 3, 2)] = 0.65;
        elevations[idx(width, 3, 1)] = 0.55;

        world.insert_resource(ElevationField::new(width, height, elevations));

        let mut tiles = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let terrain = if y == 0 {
                    TerrainType::DeepOcean
                } else if y == 1 {
                    TerrainType::TidalFlat
                } else if x == 3 && y == 3 {
                    TerrainType::FreshwaterMarsh
                } else {
                    TerrainType::MixedWoodland
                };
                let tags = match terrain {
                    TerrainType::FreshwaterMarsh | TerrainType::TidalFlat => TerrainTags::WETLAND,
                    TerrainType::DeepOcean => TerrainTags::empty(),
                    _ => TerrainTags::empty(),
                };
                let entity = world
                    .spawn(Tile {
                        position: UVec2::new(x, y),
                        element: ElementKind::Ferrite,
                        mass: scalar_zero(),
                        temperature: scalar_zero(),
                        terrain,
                        terrain_tags: tags,
                        mountain: None,
                    })
                    .id();
                tiles.push(entity);
            }
        }
        world.insert_resource(TileRegistry {
            tiles,
            width,
            height,
        });

        generate_hydrology(&mut world);

        let hydro = world.resource::<HydrologyState>();
        assert!(!hydro.rivers.is_empty(), "expected at least one river");

        let river = hydro
            .rivers
            .iter()
            .max_by_key(|r| r.path.len())
            .expect("river list should not be empty");
        let last = river
            .path
            .last()
            .expect("river should contain at least one point");

        assert!(
            river.path.len() >= 3,
            "expected river to have length >= 3 but was {} (path = {:?})",
            river.path.len(),
            river.path
        );

        assert_eq!(
            last.y, 0,
            "river should reach ocean row, path = {:?}",
            river.path
        );

        let (termination_terrain, termination_tags) = match (last.x, last.y) {
            (_, 0) => (TerrainType::DeepOcean, TerrainTags::empty()),
            (_, 1) => (TerrainType::TidalFlat, TerrainTags::WETLAND),
            (3, 3) => (TerrainType::FreshwaterMarsh, TerrainTags::WETLAND),
            _ => (TerrainType::MixedWoodland, TerrainTags::empty()),
        };

        let termination = termination_class_for(termination_terrain, termination_tags);

        assert_eq!(
            termination,
            TerminationClass::Ocean,
            "river termination = {:?}",
            termination
        );
    }
    #[test]
    fn river_crosses_inland_lake_before_ocean() {
        let width = 6u32;
        let height = 6u32;
        let mut elevations = vec![0.8f32; (width * height) as usize];
        elevations[idx(width, 3, 5)] = 0.95;
        elevations[idx(width, 3, 4)] = 0.88;
        elevations[idx(width, 3, 3)] = 0.68; // lake basin (still below head limit)
        elevations[idx(width, 3, 2)] = 0.6;
        elevations[idx(width, 3, 1)] = 0.5;
        elevations[idx(width, 3, 0)] = 0.3;
        for x in 0..width {
            elevations[idx(width, x, 0)] = 0.3;
        }

        let elevation_field = ElevationField::new(width, height, elevations);
        let mut cost = vec![f32::INFINITY; (width * height) as usize];
        cost[idx(width, 3, 5)] = 10.0;
        cost[idx(width, 3, 4)] = 7.0;
        cost[idx(width, 3, 3)] = 4.0;
        cost[idx(width, 3, 2)] = 2.0;
        cost[idx(width, 3, 1)] = 1.0;
        cost[idx(width, 3, 0)] = 0.0;
        for x in 0..width {
            cost[idx(width, x, 0)] = 0.0;
        }

        let mut termination_classes = vec![TerminationClass::None; (width * height) as usize];
        termination_classes[idx(width, 3, 3)] = TerminationClass::Lake;
        for x in 0..width {
            termination_classes[idx(width, x, 0)] = TerminationClass::Ocean;
        }

        let ctx = RiverTraceContext {
            width,
            height,
            elevation_field: &elevation_field,
            cost: &cost,
            termination_classes: &termination_classes,
        };

        let start = UVec2::new(3, 5);
        let head_elev = ctx.elevation_field.sample(start.x, start.y);
        let max_allowed_elevation = head_elev * 1.05;
        let (_edges, path, _samples, termination) =
            trace_river_path(start, head_elev, max_allowed_elevation, &ctx);

        assert!(
            path.iter().any(|p| *p == UVec2::new(3, 3)),
            "river never crossed lake tile: {:?}",
            path
        );
        assert_eq!(path.last(), Some(&UVec2::new(3, 0)));
        assert_eq!(termination, Some(TerminationClass::Ocean));
    }

    #[test]
    fn river_prefers_delta_when_available() {
        let width = 5u32;
        let height = 5u32;
        let mut elevations = vec![0.75f32; (width * height) as usize];
        for x in 0..width {
            elevations[idx(width, x, 0)] = 0.2;
        }
        elevations[idx(width, 2, 4)] = 0.95;
        elevations[idx(width, 2, 3)] = 0.68;
        elevations[idx(width, 2, 2)] = 0.55;
        elevations[idx(width, 2, 1)] = 0.28;

        let elevation_field = ElevationField::new(width, height, elevations);

        let mut cost = vec![f32::INFINITY; (width * height) as usize];
        cost[idx(width, 2, 4)] = 5.0;
        cost[idx(width, 2, 3)] = 3.0;
        cost[idx(width, 2, 2)] = 1.5;
        cost[idx(width, 2, 1)] = 0.5;
        for x in 0..width {
            cost[idx(width, x, 0)] = 0.0;
        }

        let mut termination_classes = vec![TerminationClass::None; (width * height) as usize];
        termination_classes[idx(width, 2, 1)] = TerminationClass::Wetland; // delta tile
        for x in 0..width {
            termination_classes[idx(width, x, 0)] = TerminationClass::Ocean;
        }

        let ctx = RiverTraceContext {
            width,
            height,
            elevation_field: &elevation_field,
            cost: &cost,
            termination_classes: &termination_classes,
        };

        let start = UVec2::new(2, 4);
        let head_elev = elevation_field.sample(start.x, start.y);
        let max_allowed_elevation = head_elev * 1.05;
        let (_edges, path, _samples, termination) =
            trace_river_path(start, head_elev, max_allowed_elevation, &ctx);

        assert!(
            path.contains(&UVec2::new(2, 1)),
            "river skipped delta tile: {:?}",
            path
        );
        assert!(matches!(
            termination,
            Some(TerminationClass::Wetland | TerminationClass::Ocean)
        ));
    }

    #[test]
    fn tributary_paths_share_downstream_segment() {
        let width = 7u32;
        let height = 7u32;
        let mut elevations = vec![0.0f32; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                elevations[idx(width, x, y)] =
                    0.2 + (y as f32) * 0.08 - ((x as f32 - 3.0).abs() * 0.01);
            }
        }
        let elevation_field = ElevationField::new(width, height, elevations);

        let mut cost = vec![f32::INFINITY; (width * height) as usize];
        for y in 0..height {
            for x in 0..width {
                let lateral = (x as i32 - 3).abs() as f32;
                cost[idx(width, x, y)] = (y as f32) * 1.2 + lateral;
            }
        }
        for x in 0..width {
            cost[idx(width, x, 0)] = 0.0;
        }

        let mut termination_classes = vec![TerminationClass::None; (width * height) as usize];
        for x in 0..width {
            termination_classes[idx(width, x, 0)] = TerminationClass::Ocean;
        }

        let ctx = RiverTraceContext {
            width,
            height,
            elevation_field: &elevation_field,
            cost: &cost,
            termination_classes: &termination_classes,
        };

        let main_start = UVec2::new(3, 6);
        let west_start = UVec2::new(1, 6);
        let east_start = UVec2::new(5, 6);

        let trace = |start: UVec2| {
            let head = elevation_field.sample(start.x, start.y);
            trace_river_path(start, head, head * 1.05, &ctx)
        };

        let (_main_edges, main_path, _, _) = trace(main_start);
        let (_west_edges, west_path, _, _) = trace(west_start);
        let (_east_edges, east_path, _, _) = trace(east_start);

        assert!(main_path.len() >= 6, "main path too short: {:?}", main_path);
        assert!(west_path.len() >= 4, "west path too short: {:?}", west_path);
        assert!(east_path.len() >= 4, "east path too short: {:?}", east_path);

        let main_set: HashSet<_> = main_path.iter().copied().collect();
        let west_set: HashSet<_> = west_path.iter().copied().collect();
        let east_set: HashSet<_> = east_path.iter().copied().collect();
        let west_shared: Vec<_> = main_set.intersection(&west_set).copied().collect();
        let east_shared: Vec<_> = main_set.intersection(&east_set).copied().collect();

        assert!(
            west_shared.iter().any(|p| p.y <= 2),
            "west tributary never merged with main downstream: {:?} vs {:?}",
            west_path,
            main_path
        );
        assert!(
            east_shared.iter().any(|p| p.y <= 2),
            "east tributary never merged with main downstream: {:?} vs {:?}",
            east_path,
            main_path
        );
        assert_eq!(main_path.last().map(|p| p.y), Some(0));
    }

    #[test]
    fn non_fallback_requires_min_length() {
        assert!(!path_meets_length(SourceCategory::Glacier, 5, 8, 4));
        assert!(path_meets_length(SourceCategory::Glacier, 8, 8, 4));
    }

    #[test]
    fn fallback_allows_shorter_length() {
        assert!(path_meets_length(SourceCategory::Fallback, 5, 8, 4));
        assert!(!path_meets_length(SourceCategory::Fallback, 3, 8, 4));
    }
}
