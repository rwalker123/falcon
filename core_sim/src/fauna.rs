use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use sim_runtime::TerrainTags;
use tracing::info;

use crate::{
    components::Tile,
    fauna_config::{FaunaConfig, FaunaConfigHandle, SizeClass},
    food::{classify_food_module, FoodModule},
    mapgen::WorldGenSeed,
    resources::{SimulationConfig, StartLocation, TileRegistry},
};

pub const HERD_DENSITY_REFERENCE_BIOMASS: f32 = 8_000.0;

#[derive(Debug, Clone)]
pub struct Herd {
    pub id: String,
    pub label: String,
    /// Species display name (also the snapshot `species` string; drives the client
    /// icon via keyword match). Sourced from the data-driven `fauna_config.json`.
    pub species: String,
    /// Coarse size band (snapshot `size_class`); lets the client offer the right verbs.
    pub size_class: SizeClass,
    pub route: Vec<UVec2>,
    pub step_index: usize,
    pub biomass: f32,
    /// Per-species carrying capacity (= table biomass max) that biomass regrows toward.
    pub carrying_capacity: f32,
}

impl Herd {
    pub fn new(
        id: String,
        species_display: String,
        size_class: SizeClass,
        route: Vec<UVec2>,
        biomass: f32,
        carrying_capacity: f32,
    ) -> Self {
        let label = format!("{} ({})", species_display, id);
        Self {
            id,
            label,
            species: species_display,
            size_class,
            route,
            step_index: 0,
            biomass,
            carrying_capacity,
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

#[derive(Debug, Clone, Default)]
pub struct HerdTelemetryEntry {
    pub id: String,
    pub label: String,
    pub species: String,
    pub size_class: String,
    pub huntable: bool,
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
    fauna_config: Res<FaunaConfigHandle>,
) {
    if !registry.herds.is_empty() {
        telemetry.entries = registry.herds.iter().map(to_entry).collect();
        density.rebuild(config.grid_size, &registry);
        return;
    }

    let fauna = fauna_config.get();
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

    let mut herds = Vec::new();
    // 1. Long-range migratory herds — start-anchored, species/biomass from config.
    spawn_migratory_herds(
        &fauna,
        base,
        width,
        height,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );
    // 2. Short-range wild game — biome-density placement across the whole map.
    spawn_short_range_game(
        &fauna,
        width,
        height,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );

    registry.herds = herds;
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

fn log_herd_spawn(herd: &Herd) {
    let position = herd.position();
    info!(
        target: "shadow_scale::analytics",
        event = "herd_spawn",
        herd = %herd.id,
        label = %herd.label,
        species = %herd.species,
        x = position.x,
        y = position.y,
        biomass = herd.biomass,
        route_length = herd.route_length(),
    );
}

/// Long-range migratory herds: a handful of cross-region walkers anchored on the
/// start area, one per `determine_herd_count`, species drawn from the config's
/// migratory rows.
#[allow(clippy::too_many_arguments)]
fn spawn_migratory_herds(
    fauna: &FaunaConfig,
    base: UVec2,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
) {
    let migratory = fauna.migratory_species();
    if migratory.is_empty() {
        return;
    }
    let herd_target = determine_herd_count(width, height);
    for idx in 0..herd_target {
        let (key, def) = migratory[rng.gen_range(0..migratory.len())];
        let steps = def.sample_route_len(rng);
        let Some(route) = build_route(base, width, height, tile_registry, tiles, rng, steps) else {
            continue;
        };
        let biomass = def.sample_biomass(rng);
        let carrying_capacity = def.carrying_capacity();
        let id = format!("herd_{key}_{idx:02}");
        let herd = Herd::new(
            id,
            def.display_name.clone(),
            def.size_class,
            route,
            biomass,
            carrying_capacity,
        );
        log_herd_spawn(&herd);
        herds.push(herd);
    }
}

/// Short-range wild game (big + small): iterate land tiles, roll the per-biome
/// abundance, then greedily place bounded, spaced-out groups from a shuffled pool
/// so placement is spread across the map rather than clustered by scan order.
fn spawn_short_range_game(
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
) {
    // Collect every tile where the abundance roll succeeds (map-wide).
    let mut winners: Vec<(UVec2, &'static str)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            let Some(module) = module_at(pos, tile_registry, tiles) else {
                continue;
            };
            let module_key = module.as_str();
            let prob = fauna.abundance.probability_for(module_key);
            if prob <= 0.0 {
                continue;
            }
            if rng.gen::<f32>() < prob {
                winners.push((pos, module_key));
            }
        }
    }
    // Shuffle so the cap + spacing thin the pool uniformly, not top-to-bottom.
    winners.shuffle(rng);

    let max_total = fauna.abundance.max_total_game;
    let min_spacing = fauna.abundance.min_spacing;
    let mut placed: Vec<UVec2> = Vec::new();
    let mut game_idx = 0u32;
    for (pos, module_key) in winners {
        if placed.len() >= max_total {
            break;
        }
        if placed
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < min_spacing)
        {
            continue;
        }
        let candidates = fauna.game_species_for_biome(module_key);
        if candidates.is_empty() {
            continue;
        }
        let (key, def) = candidates[rng.gen_range(0..candidates.len())];
        let steps = def.sample_route_len(rng);
        let Some(route) = build_short_route(pos, steps, width, height, tile_registry, tiles, rng)
        else {
            continue;
        };
        let biomass = def.sample_biomass(rng);
        let carrying_capacity = def.carrying_capacity();
        let id = format!("game_{key}_{game_idx:02}");
        game_idx += 1;
        let herd = Herd::new(
            id,
            def.display_name.clone(),
            def.size_class,
            route,
            biomass,
            carrying_capacity,
        );
        log_herd_spawn(&herd);
        placed.push(pos);
        herds.push(herd);
    }
}

pub fn advance_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    if registry.herds.is_empty() {
        telemetry.entries.clear();
        density.width = 0;
        density.height = 0;
        density.samples.clear();
        return;
    }
    let regrowth_rate = fauna_config.get().ecology.regrowth_rate;
    for herd in registry.herds.iter_mut() {
        herd.advance();
        regrow_biomass(herd, regrowth_rate);
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
    // Local extinction: a group hunted to zero disperses and despawns.
    registry.herds.retain(|herd| herd.biomass > 0.0);
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

/// Logistic regrowth toward the herd's per-species carrying capacity. A group at or
/// below zero is left for the caller to despawn (it does not regrow from extinction).
fn regrow_biomass(herd: &mut Herd, regrowth_rate: f32) {
    let cap = herd.carrying_capacity;
    if cap <= 0.0 || herd.biomass <= 0.0 {
        return;
    }
    herd.biomass =
        (herd.biomass + regrowth_rate * herd.biomass * (1.0 - herd.biomass / cap)).clamp(0.0, cap);
}

fn to_entry(herd: &Herd) -> HerdTelemetryEntry {
    HerdTelemetryEntry {
        id: herd.id.clone(),
        label: herd.label.clone(),
        species: herd.species.clone(),
        size_class: herd.size_class.as_str().to_string(),
        // All fauna are huntable in Phase B; Phase C/D may differentiate.
        huntable: true,
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

/// Long migratory route: a jittered spiral of `steps` waypoints around `origin`,
/// keeping only land tiles. Returns `None` if fewer than 3 distinct points land.
fn build_route(
    origin: UVec2,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    steps: u32,
) -> Option<Vec<UVec2>> {
    let mut points = Vec::new();
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

/// Short roaming route for wild game: `steps` waypoints within a small radius of
/// `origin` (radius grows with route length). `steps == 1` yields a single-tile,
/// stationary group (which the client draws with no trail). Returns `None` only if
/// `origin` itself is not land.
fn build_short_route(
    origin: UVec2,
    steps: u32,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Vec<UVec2>> {
    if !is_land_tile(origin, registry, tiles) {
        return None;
    }
    let mut points = vec![origin];
    let target = steps.max(1) as usize;
    if target <= 1 {
        return Some(points);
    }
    // Wander radius scales with route length (big game ~2-3 tiles, small ~1).
    let radius = target.saturating_sub(1).max(1) as i32;
    let max_attempts = target * 4;
    let mut attempts = 0;
    while points.len() < target && attempts < max_attempts {
        attempts += 1;
        let dx = rng.gen_range(-radius..=radius);
        let dy = rng.gen_range(-radius..=radius);
        let Some(pos) = clamp_to_grid(origin.x as i32 + dx, origin.y as i32 + dy, width, height)
        else {
            continue;
        };
        if is_land_tile(pos, registry, tiles) && !points.contains(&pos) {
            points.push(pos);
        }
    }
    Some(points)
}

/// Food module for a tile position, or `None` for water / unclassified tiles.
fn module_at(position: UVec2, registry: &TileRegistry, tiles: &Query<&Tile>) -> Option<FoodModule> {
    let entity = registry.index(position.x, position.y)?;
    let tile = tiles.get(entity).ok()?;
    classify_food_module(tile)
}

fn chebyshev_distance(a: UVec2, b: UVec2) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    dx.max(dy)
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
