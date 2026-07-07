use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use sim_runtime::TerrainTags;
use tracing::info;

use crate::{
    components::Tile,
    fauna_config::{EcologyConfig, FaunaConfig, FaunaConfigHandle, SizeClass},
    food::{classify_food_module, FoodModule},
    mapgen::WorldGenSeed,
    orders::FactionId,
    resources::{FactionInventory, SimulationConfig, SimulationTick, StartLocation, TileRegistry},
};

/// RNG salt for per-turn immigration, kept distinct from the initial-spawn salt so the
/// two streams don't correlate.
const IMMIGRATION_SEED_SALT: u64 = 0xFA1A_B0B0;

/// Id prefix marking a short-range wild-game group (migratory herds use `herd_`). The
/// `abundance.max_total_game` cap applies to these groups only — both at initial spawn
/// (`placed.len()`) and per-turn immigration.
const GAME_ID_PREFIX: &str = "game_";

pub const HERD_DENSITY_REFERENCE_BIOMASS: f32 = 8_000.0;

/// Coarse ecological health band derived from a group's biomass vs its carrying
/// capacity (thresholds in `EcologyConfig`). Surfaced to the client as an early
/// overhunting warning, and the seam the later domestication / industrialized-hunting
/// arc keys off (e.g. a long Sustain-follow on a `Thriving` herd → husbandry progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EcologyPhase {
    /// At or above the stressed band — a healthy, self-sustaining group.
    #[default]
    Thriving,
    /// Depleted but above the collapse threshold — still able to recover if left alone.
    Stressed,
    /// Below the Allee threshold — non-viable and crashing to local extinction
    /// regardless of whether hunting continues (the point of no return).
    Collapsing,
}

impl EcologyPhase {
    /// Stable string key (also the snapshot `ecologyPhase` field).
    pub fn as_str(self) -> &'static str {
        match self {
            EcologyPhase::Thriving => "thriving",
            EcologyPhase::Stressed => "stressed",
            EcologyPhase::Collapsing => "collapsing",
        }
    }
}

/// Classify a group's ecological phase from its biomass fraction of carrying capacity.
pub(crate) fn classify_ecology_phase(
    biomass: f32,
    cap: f32,
    ecology: &EcologyConfig,
) -> EcologyPhase {
    if cap <= 0.0 {
        return EcologyPhase::Collapsing;
    }
    let frac = biomass / cap;
    if frac < ecology.collapse_fraction {
        EcologyPhase::Collapsing
    } else if frac < ecology.stressed_fraction {
        EcologyPhase::Stressed
    } else {
        EcologyPhase::Thriving
    }
}

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
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from
    /// biomass vs `carrying_capacity`. Surfaced to the client and the domestication hook.
    pub ecology_phase: EcologyPhase,
    /// Husbandry progress in `[0.0, 1.0]`; `1.0` = domesticated. Accrues while a band
    /// Sustain-follows this (Thriving) group and decays otherwise (see `advance_husbandry`).
    pub domestication_progress: f32,
    /// Faction tending/owning this group (`Some` iff `domestication_progress > 0`).
    pub owner: Option<FactionId>,
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
            // Refreshed against the ecology config at spawn/each turn; Thriving until then.
            ecology_phase: EcologyPhase::Thriving,
            domestication_progress: 0.0,
            owner: None,
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }

    /// A fully-tamed (managed livestock) group: yields provisions each turn and is
    /// immune to the overhunting collapse.
    pub fn is_domesticated(&self) -> bool {
        self.domestication_progress >= 1.0
    }

    /// Accrue husbandry progress for `faction` (the tending band). Sets ownership on the
    /// first accrual; only the owner makes progress. Clamped to 1.0 (auto-domestication).
    pub(crate) fn accrue_domestication(&mut self, faction: FactionId, amount: f32) {
        if self.is_domesticated() {
            return;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner == Some(faction) {
            self.domestication_progress = (self.domestication_progress + amount).min(1.0);
        }
    }

    /// Decay husbandry progress toward zero when the group isn't being actively tended;
    /// ownership lapses once progress reaches zero. A domesticated group is left alone.
    pub(crate) fn decay_domestication(&mut self, amount: f32) {
        if self.is_domesticated() {
            return;
        }
        self.domestication_progress = (self.domestication_progress - amount).max(0.0);
        // Reconcile the `owner is Some ⟺ progress > 0` invariant unconditionally, so a
        // group that reaches (or somehow sits at) zero progress never keeps a stale owner
        // — which would otherwise block another faction from ever tending it.
        if self.domestication_progress <= 0.0 {
            self.owner = None;
        }
    }

    /// Finalize domestication (the `domesticate` command's early claim): snap progress to
    /// 1.0 so `is_domesticated()` latches.
    pub fn claim_domestication(&mut self) {
        self.domestication_progress = 1.0;
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
    /// Ecological health band string (see `EcologyPhase::as_str`).
    pub ecology_phase: String,
    /// Husbandry progress in `[0.0, 1.0]` (`1.0` = domesticated).
    pub domestication: f32,
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

    /// Number of domesticated groups owned by `faction`. The seam the future
    /// `SedentarizationScore` reads for its "domestication progress" input (`TASKS.md`).
    pub fn domesticated_count(&self, faction: FactionId) -> usize {
        self.herds
            .iter()
            .filter(|herd| herd.is_domesticated() && herd.owner == Some(faction))
            .count()
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
        let mut herd = Herd::new(
            id,
            def.display_name.clone(),
            def.size_class,
            route,
            biomass,
            carrying_capacity,
        );
        herd.refresh_ecology_phase(&fauna.ecology);
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
        let Some(herd) = spawn_game_group_at(
            pos,
            module_key,
            game_idx,
            fauna,
            width,
            height,
            tile_registry,
            tiles,
            rng,
        ) else {
            continue;
        };
        game_idx += 1;
        log_herd_spawn(&herd);
        placed.push(pos);
        herds.push(herd);
    }
}

/// Build a single short-range game group at `pos`: pick a species hosting `module_key`,
/// roll its route/biomass, and stamp its initial `ecology_phase`. Returns `None` if no
/// species hosts the biome or the origin is not land. Shared by initial spawn and
/// per-turn immigration.
// Placement needs the config, grid bounds, both tile resources, and the RNG; grouping
// them into a struct would just move the noise without improving clarity.
#[allow(clippy::too_many_arguments)]
fn spawn_game_group_at(
    pos: UVec2,
    module_key: &str,
    game_idx: u32,
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Herd> {
    let candidates = fauna.game_species_for_biome(module_key);
    if candidates.is_empty() {
        return None;
    }
    let (key, def) = candidates[rng.gen_range(0..candidates.len())];
    let steps = def.sample_route_len(rng);
    let route = build_short_route(pos, steps, width, height, tile_registry, tiles, rng)?;
    let biomass = def.sample_biomass(rng);
    let carrying_capacity = def.carrying_capacity();
    let id = format!("{GAME_ID_PREFIX}{key}_{game_idx:02}");
    let mut herd = Herd::new(
        id,
        def.display_name.clone(),
        def.size_class,
        route,
        biomass,
        carrying_capacity,
    );
    herd.refresh_ecology_phase(&fauna.ecology);
    Some(herd)
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
    let fauna = fauna_config.get();
    let ecology = &fauna.ecology;
    for herd in registry.herds.iter_mut() {
        herd.advance();
        regrow_biomass(herd, ecology);
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
            ecology_phase = herd.ecology_phase.as_str(),
        );
    }
    // Local extinction: a group hunted to zero, or a collapsing remnant that has fallen
    // below the viability floor, disperses and despawns.
    registry
        .herds
        .retain(|herd| herd.biomass > ecology.extinction_floor * herd.carrying_capacity);
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

/// Per-turn immigration: with probability `immigration.chance_per_turn`, respawn one
/// short-range game group up to the abundance cap so an overhunted map slowly
/// replenishes (early forager play stays game-rich). Samples up to
/// `immigration.max_attempts` random land tiles hosting game, respecting `min_spacing`
/// from existing groups. Runs in `TurnStage::Logistics` right after `advance_herds`.
// Bevy system signature: each param is a distinct resource/query the immigration roll
// needs (registry + telemetry/density outputs, config, tick+seed for the RNG, tiles);
// they can't be collapsed without a container resource that adds no clarity.
#[allow(clippy::too_many_arguments)]
pub fn repopulate_fauna(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
    tick: Res<SimulationTick>,
    world_seed: Option<Res<WorldGenSeed>>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
) {
    let fauna = fauna_config.get();
    let imm = &fauna.immigration;
    // `max_total_game` caps short-range game groups only (matching spawn's `placed`
    // counter); migratory `herd_*` are spawned separately and don't count against it.
    let game_count = registry
        .herds
        .iter()
        .filter(|herd| herd.id.starts_with(GAME_ID_PREFIX))
        .count();
    if imm.chance_per_turn <= 0.0 || game_count >= fauna.abundance.max_total_game {
        return;
    }

    let width = config.grid_size.x.max(4);
    let height = config.grid_size.y.max(4);
    let seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed);
    let mut rng = SmallRng::seed_from_u64(seed ^ tick.0 ^ IMMIGRATION_SEED_SALT);

    // Roll the per-turn immigration chance.
    if rng.gen::<f32>() >= imm.chance_per_turn {
        return;
    }

    // Ids past the initial cap + tick keep immigrants from colliding with spawn ids
    // (only one group immigrates per turn, so `tick` disambiguates across turns).
    let idx = fauna.abundance.max_total_game as u32 + tick.0 as u32;
    let min_spacing = fauna.abundance.min_spacing;
    let existing: Vec<UVec2> = registry.herds.iter().map(|herd| herd.position()).collect();

    for _ in 0..imm.max_attempts {
        let pos = UVec2::new(rng.gen_range(0..width), rng.gen_range(0..height));
        let Some(module) = module_at(pos, &tile_registry, &tiles) else {
            continue;
        };
        let module_key = module.as_str();
        if fauna.abundance.probability_for(module_key) <= 0.0 {
            continue;
        }
        if existing
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < min_spacing)
        {
            continue;
        }
        if let Some(herd) = spawn_game_group_at(
            pos,
            module_key,
            idx,
            &fauna,
            width,
            height,
            &tile_registry,
            &tiles,
            &mut rng,
        ) {
            info!(
                target: "shadow_scale::analytics",
                event = "immigration",
                herd = %herd.id,
                species = %herd.species,
                x = pos.x,
                y = pos.y,
                biomass = herd.biomass,
            );
            registry.herds.push(herd);
            telemetry.entries = registry.snapshot_entries();
            density.rebuild(config.grid_size, &registry);
            return;
        }
    }
}

/// Per-turn husbandry upkeep (`TurnStage::Logistics`, after `advance_herds`): pay each
/// domesticated group's owner a steady provisions yield — proportional to biomass and
/// **without** depleting the herd (sustainable managed harvest) — and decay husbandry
/// progress on any not-yet-tamed group. Runs before the same turn's accrual in
/// `advance_fauna_pursuits` (`Population`), so a Sustain-followed group nets
/// `progress_per_turn - decay_per_turn` while an untended one only decays.
pub fn advance_husbandry(
    mut registry: ResMut<HerdRegistry>,
    mut inventory: ResMut<FactionInventory>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    let fauna = fauna_config.get();
    let husbandry = &fauna.husbandry;
    for herd in registry.herds.iter_mut() {
        if herd.is_domesticated() {
            let Some(owner) = herd.owner else {
                continue;
            };
            let provisions = (herd.biomass * husbandry.provisions_per_biomass).round() as i64;
            if provisions > 0 {
                inventory.add_stockpile(owner, "provisions", provisions);
                info!(
                    target: "shadow_scale::analytics",
                    event = "husbandry_yield",
                    herd = %herd.id,
                    faction = owner.0,
                    provisions,
                );
            }
        } else {
            herd.decay_domestication(husbandry.decay_per_turn);
        }
    }
}

/// One turn's positive logistic regrowth increment (>= 0) for a group of `biomass`
/// toward `cap`. The healthy branch of `net_biomass_delta`.
fn logistic_regrowth(biomass: f32, cap: f32, regrowth_rate: f32) -> f32 {
    if cap <= 0.0 || biomass <= 0.0 {
        return 0.0;
    }
    (regrowth_rate * biomass * (1.0 - biomass / cap)).max(0.0)
}

/// Net per-turn biomass change with **critical depensation**. Above the Allee
/// threshold (`collapse_fraction * cap`) the group regrows logistically; below it the
/// group is non-viable and declines by `collapse_rate` of its biomass each turn — an
/// irreversible crash to local extinction even without further hunting (the overhunting
/// point of no return). Also sizes a Sustain/Surplus follow's take (via `.max(0.0)`):
/// a collapsing group yields no surplus.
pub(crate) fn net_biomass_delta(biomass: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    if cap <= 0.0 || biomass <= 0.0 {
        return 0.0;
    }
    let allee = ecology.collapse_fraction * cap;
    if biomass < allee {
        -(ecology.collapse_rate * biomass)
    } else {
        logistic_regrowth(biomass, cap, ecology.regrowth_rate)
    }
}

/// Apply one turn of critical-depensation dynamics toward the herd's carrying capacity
/// and refresh its `ecology_phase`. A sub-threshold group declines instead of regrowing;
/// the caller despawns it once it falls below the viability floor.
fn regrow_biomass(herd: &mut Herd, ecology: &EcologyConfig) {
    let cap = herd.carrying_capacity;
    // A domesticated (managed) group is immune to the overhunting collapse: it always
    // regrows logistically toward capacity and never crosses into the depensation crash.
    let delta = if herd.is_domesticated() {
        logistic_regrowth(herd.biomass, cap, ecology.regrowth_rate)
    } else {
        net_biomass_delta(herd.biomass, cap, ecology)
    };
    herd.biomass = (herd.biomass + delta).clamp(0.0, cap);
    herd.refresh_ecology_phase(ecology);
}

fn to_entry(herd: &Herd) -> HerdTelemetryEntry {
    HerdTelemetryEntry {
        id: herd.id.clone(),
        label: herd.label.clone(),
        species: herd.species.clone(),
        size_class: herd.size_class.as_str().to_string(),
        // All fauna are huntable in Phase B; Phase C/D may differentiate.
        huntable: true,
        ecology_phase: herd.ecology_phase.as_str().to_string(),
        domestication: herd.domestication_progress,
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
