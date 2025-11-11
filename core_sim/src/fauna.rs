use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{rngs::SmallRng, Rng, SeedableRng};
use sim_runtime::TerrainTags;
use tracing::info;

use crate::{
    components::Tile,
    mapgen::WorldGenSeed,
    resources::{SimulationConfig, StartLocation, TileRegistry},
};

pub const HERD_DENSITY_REFERENCE_BIOMASS: f32 = 8_000.0;

#[derive(Debug, Clone)]
pub struct Herd {
    pub id: String,
    pub label: String,
    pub species: HerdSpecies,
    pub route: Vec<UVec2>,
    pub step_index: usize,
    pub biomass: f32,
}

impl Herd {
    pub fn new(id: String, species: HerdSpecies, route: Vec<UVec2>, biomass: f32) -> Self {
        let label = format!("{} ({})", species.display_label(), id);
        Self {
            id,
            label,
            species,
            route,
            step_index: 0,
            biomass,
        }
    }

    pub fn position(&self) -> UVec2 {
        self.route
            .get(self.step_index)
            .copied()
            .unwrap_or_else(|| UVec2::new(0, 0))
    }

    pub fn advance(&mut self) {
        if self.route.is_empty() {
            return;
        }
        self.step_index = (self.step_index + 1) % self.route.len();
    }

    pub fn route_length(&self) -> usize {
        self.route.len()
    }

    pub fn next_position(&self) -> Option<UVec2> {
        if self.route.is_empty() {
            return None;
        }
        let next_index = (self.step_index + 1) % self.route.len();
        self.route.get(next_index).copied()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HerdSpecies {
    Mammoth,
    SteppeRunner,
    MarshGrazer,
}

impl HerdSpecies {
    pub fn display_label(&self) -> &'static str {
        match self {
            HerdSpecies::Mammoth => "Thunder Mammoths",
            HerdSpecies::SteppeRunner => "Steppe Runners",
            HerdSpecies::MarshGrazer => "Marsh Grazers",
        }
    }

    pub fn sample(rng: &mut SmallRng) -> Self {
        match rng.gen_range(0..=2) {
            0 => HerdSpecies::Mammoth,
            1 => HerdSpecies::SteppeRunner,
            _ => HerdSpecies::MarshGrazer,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct HerdTelemetryEntry {
    pub id: String,
    pub label: String,
    pub species: String,
    pub position: UVec2,
    pub biomass: f32,
    pub route_length: u32,
    pub next_position: Option<UVec2>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdRegistry {
    pub herds: Vec<Herd>,
}

impl HerdRegistry {
    pub fn clear(&mut self) {
        self.herds.clear();
    }

    pub fn find(&self, id: &str) -> Option<&Herd> {
        self.herds.iter().find(|herd| herd.id == id)
    }

    pub fn entries(&self) -> &[Herd] {
        &self.herds
    }

    pub fn snapshot_entries(&self) -> Vec<HerdTelemetryEntry> {
        self.herds.iter().map(to_entry).collect()
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdTelemetry {
    pub entries: Vec<HerdTelemetryEntry>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdDensityMap {
    pub width: u32,
    pub height: u32,
    samples: Vec<f32>,
}

impl HerdDensityMap {
    pub fn rebuild(&mut self, grid_size: UVec2, registry: &HerdRegistry) {
        let samples: Vec<(UVec2, f32)> = registry
            .herds
            .iter()
            .map(|herd| (herd.position(), herd.biomass))
            .collect();
        self.rebuild_from_samples(grid_size, &samples);
    }

    pub fn rebuild_from_samples(&mut self, grid_size: UVec2, herds: &[(UVec2, f32)]) {
        let width = grid_size.x.max(1);
        let height = grid_size.y.max(1);
        let total = width.saturating_mul(height).max(1);
        if self.width != width || self.height != height || self.samples.len() != total as usize {
            self.width = width;
            self.height = height;
            self.samples = vec![0.0; total as usize];
        } else {
            self.samples.fill(0.0);
        }

        for (pos, biomass) in herds {
            if pos.x >= self.width || pos.y >= self.height {
                continue;
            }
            let idx = (pos.y as usize) * self.width as usize + pos.x as usize;
            self.samples[idx] += *biomass;
        }
    }

    pub fn density_at(&self, pos: UVec2) -> f32 {
        if self.samples.is_empty() || pos.x >= self.width || pos.y >= self.height {
            return 0.0;
        }
        let idx = (pos.y as usize) * self.width as usize + pos.x as usize;
        self.samples.get(idx).copied().unwrap_or(0.0)
    }

    pub fn normalized_density_at(&self, pos: UVec2) -> f32 {
        normalize_density(self.density_at(pos))
    }

    pub fn normalized_pair_average(&self, a: UVec2, b: UVec2) -> f32 {
        let avg = 0.5 * (self.density_at(a) + self.density_at(b));
        normalize_density(avg)
    }

    pub fn normalized_average(&self) -> f32 {
        normalize_density(self.average_density())
    }

    pub fn average_density(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let total: f32 = self.samples.iter().copied().sum();
        total / (self.samples.len() as f32)
    }

    pub fn max_density(&self) -> f32 {
        self.samples
            .iter()
            .copied()
            .fold(0.0f32, |acc, value| acc.max(value))
    }
}

fn normalize_density(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else {
        (value / HERD_DENSITY_REFERENCE_BIOMASS).clamp(0.0, 1.0)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_initial_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    start_location: Res<StartLocation>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    world_seed: Option<Res<WorldGenSeed>>,
) {
    if !registry.herds.is_empty() {
        telemetry.entries = registry.herds.iter().map(to_entry).collect();
        density.rebuild(config.grid_size, &registry);
        return;
    }

    let seed = world_seed
        .map(|seed| seed.0)
        .unwrap_or_else(|| config.map_seed);
    let mut rng = if seed == 0 {
        SmallRng::from_entropy()
    } else {
        SmallRng::seed_from_u64(seed ^ 0xFA1A_FEED)
    };

    let width = config.grid_size.x.max(4);
    let height = config.grid_size.y.max(4);
    let base = start_location
        .position()
        .unwrap_or(UVec2::new(width / 2, height / 2));

    let herd_target = determine_herd_count(width, height);
    let mut herds = Vec::with_capacity(herd_target as usize);
    for idx in 0..herd_target {
        if let Some(route) = build_route(base, width, height, &tile_registry, &tiles, &mut rng) {
            let species = HerdSpecies::sample(&mut rng);
            let id = format!("trail_herd_{:02}", idx);
            let biomass = rng.gen_range(4000.0..12000.0);
            let herd = Herd::new(id, species, route, biomass);
            let position = herd.position();
            info!(
                target: "shadow_scale::analytics",
                event = "herd_spawn",
                herd = %herd.id,
                label = %herd.label,
                species = %herd.species.display_label(),
                x = position.x,
                y = position.y,
                biomass = herd.biomass,
                route_length = herd.route_length(),
            );
            herds.push(herd);
        }
    }
    registry.herds = herds;
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

pub fn advance_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
) {
    if registry.herds.is_empty() {
        telemetry.entries.clear();
        density.width = 0;
        density.height = 0;
        density.samples.clear();
        return;
    }
    for herd in registry.herds.iter_mut() {
        herd.advance();
        let position = herd.position();
        info!(
            target: "shadow_scale::analytics",
            event = "herd_migrate",
            herd = %herd.id,
            label = %herd.label,
            x = position.x,
            y = position.y,
            step_index = herd.step_index,
            route_length = herd.route_length(),
            biomass = herd.biomass,
        );
    }
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

fn to_entry(herd: &Herd) -> HerdTelemetryEntry {
    HerdTelemetryEntry {
        id: herd.id.clone(),
        label: herd.label.clone(),
        species: herd.species.display_label().to_string(),
        position: herd.position(),
        biomass: herd.biomass,
        route_length: herd.route_length() as u32,
        next_position: herd.next_position(),
    }
}

fn determine_herd_count(width: u32, height: u32) -> u32 {
    let area = width.saturating_mul(height).max(1);
    let baseline = area / 3000;
    baseline.clamp(2, 6)
}

fn build_route(
    origin: UVec2,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Vec<UVec2>> {
    let mut points = Vec::new();
    let steps = rng.gen_range(6..=12);
    let radius = rng.gen_range(4..=12) as f32;
    let mut angle = rng.gen_range(0.0..TAU);
    for _ in 0..steps {
        let dx = angle.cos() * radius;
        let dy = angle.sin() * radius;
        angle = (angle + rng.gen_range(0.4..=1.2)) % TAU;
        let candidate = clamp_to_grid(
            origin.x as i32 + dx.round() as i32,
            origin.y as i32 + dy.round() as i32,
            width,
            height,
        );
        if let Some(pos) = candidate {
            if is_land_tile(pos, registry, tiles) && points.last().copied() != Some(pos) {
                points.push(pos);
            }
        }
    }
    if points.len() < 3 {
        None
    } else {
        Some(points)
    }
}

fn clamp_to_grid(x: i32, y: i32, width: u32, height: u32) -> Option<UVec2> {
    let max_x = width as i32 - 1;
    let max_y = height as i32 - 1;
    if max_x < 0 || max_y < 0 {
        return None;
    }
    let clamped_x = x.clamp(0, max_x) as u32;
    let clamped_y = y.clamp(0, max_y) as u32;
    Some(UVec2::new(clamped_x, clamped_y))
}

fn is_land_tile(position: UVec2, registry: &TileRegistry, tiles: &Query<&Tile>) -> bool {
    registry
        .index(position.x, position.y)
        .and_then(|entity| tiles.get(entity).ok())
        .map(|tile| !tile.terrain_tags.contains(TerrainTags::WATER))
        .unwrap_or(false)
}
