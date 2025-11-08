use std::collections::VecDeque;

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use sim_runtime::TerrainTags;

pub type ProvinceId = u32;

#[derive(Resource, Debug, Clone)]
pub struct ProvinceMap {
    width: u32,
    height: u32,
    assignments: Vec<Option<ProvinceId>>,
    land_tiles: usize,
    province_count: ProvinceId,
}

impl ProvinceMap {
    pub fn generate(width: u32, height: u32, tags: &[TerrainTags], seed: u64) -> Self {
        let total = (width as usize).saturating_mul(height as usize);
        let mut assignments = vec![None; total];
        let mut visited = vec![false; total];
        let mut is_land = vec![false; total];
        let mut land_tiles = 0usize;
        for (idx, tag) in tags.iter().enumerate().take(total) {
            if !tag.contains(TerrainTags::WATER) {
                is_land[idx] = true;
                land_tiles += 1;
            }
        }

        let mut next_province: ProvinceId = 1;
        let mut rng = SmallRng::seed_from_u64(seed ^ 0x4b5f_d2c3);
        let width_i32 = width as i32;
        let height_i32 = height as i32;
        const NEIGHBORS: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
        const TARGET_SIZE: usize = 96;

        for start in 0..total {
            if visited[start] || !is_land[start] {
                continue;
            }
            let mut component = Vec::new();
            let mut queue = VecDeque::new();
            queue.push_back(start);
            visited[start] = true;
            while let Some(idx) = queue.pop_front() {
                component.push(idx);
                let x = (idx % width as usize) as i32;
                let y = (idx / width as usize) as i32;
                for (dx, dy) in NEIGHBORS {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width_i32 || ny >= height_i32 {
                        continue;
                    }
                    let nidx = (ny as usize) * width as usize + nx as usize;
                    if visited[nidx] || !is_land[nidx] {
                        continue;
                    }
                    visited[nidx] = true;
                    queue.push_back(nidx);
                }
            }
            if component.is_empty() {
                continue;
            }
            let mut provinces_needed = component.len().div_ceil(TARGET_SIZE);
            provinces_needed = provinces_needed.clamp(1, component.len());
            let mut seeds: Vec<usize> = component
                .choose_multiple(&mut rng, provinces_needed)
                .cloned()
                .collect();
            if seeds.is_empty() {
                seeds.push(component[0]);
            }
            let mut growth = VecDeque::new();
            for seed_idx in seeds {
                let province_id = next_province;
                next_province = next_province.wrapping_add(1).max(1);
                assignments[seed_idx] = Some(province_id);
                growth.push_back((seed_idx, province_id));
            }
            while let Some((idx, province_id)) = growth.pop_front() {
                let x = (idx % width as usize) as i32;
                let y = (idx / width as usize) as i32;
                for (dx, dy) in NEIGHBORS {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width_i32 || ny >= height_i32 {
                        continue;
                    }
                    let nidx = (ny as usize) * width as usize + nx as usize;
                    if !is_land[nidx] || assignments[nidx].is_some() {
                        continue;
                    }
                    assignments[nidx] = Some(province_id);
                    growth.push_back((nidx, province_id));
                }
            }
        }

        let province_count = next_province.saturating_sub(1);
        ProvinceMap {
            width,
            height,
            assignments,
            land_tiles,
            province_count,
        }
    }

    pub fn province_at_index(&self, idx: usize) -> Option<ProvinceId> {
        self.assignments.get(idx).copied().flatten()
    }

    pub fn province_at(&self, x: u32, y: u32) -> Option<ProvinceId> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y * self.width + x) as usize;
        self.province_at_index(idx)
    }

    pub fn province_count(&self) -> ProvinceId {
        self.province_count
    }

    pub fn land_tiles(&self) -> usize {
        self.land_tiles
    }
}
