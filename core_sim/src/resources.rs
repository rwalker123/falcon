use std::{
    collections::HashMap,
    env, fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use bevy::{math::UVec2, prelude::*};
use serde::Deserialize;
use sim_runtime::{CorruptionLedger, CorruptionSubsystem, FloatRasterState};
use thiserror::Error;

use crate::{
    culture::CultureTensionRecord,
    orders::FactionId,
    scalar::{scalar_from_f32, Scalar},
    start_profile::{FogMode, StartProfileOverrides},
};

#[derive(Debug, Clone, Default)]
pub struct HydrologyOverrides {
    pub river_density: Option<f32>,
    pub river_min_count: Option<usize>,
    pub river_max_count: Option<usize>,
    pub accumulation_threshold_factor: Option<f32>,
    pub source_percentile: Option<f32>,
    pub source_sea_buffer: Option<f32>,
    pub min_length: Option<usize>,
    pub fallback_min_length: Option<usize>,
    pub spacing: Option<f32>,
    pub uphill_gain_pct: Option<f32>,
}

/// Global configuration parameters for the headless simulation prototype.
#[derive(Resource, Debug, Clone)]
pub struct SimulationConfig {
    pub grid_size: UVec2,
    pub map_preset_id: String,
    pub map_seed: u64,
    pub start_profile_id: String,
    pub start_profile_overrides: StartProfileOverrides,
    pub hydrology: HydrologyOverrides,
    pub ambient_temperature: Scalar,
    pub temperature_lerp: Scalar,
    pub logistics_flow_gain: Scalar,
    pub base_link_capacity: Scalar,
    pub mass_bounds: (Scalar, Scalar),
    pub population_growth_rate: Scalar,
    pub temperature_morale_penalty: Scalar,
    pub population_cluster_stride: u32,
    pub population_cap: u32,
    pub power_adjust_rate: Scalar,
    pub max_power_generation: Scalar,
    pub max_power_efficiency: Scalar,
    pub min_power_influence: f32,
    pub max_power_influence: f32,
    pub power_generation_adjust_rate: f32,
    pub power_demand_adjust_rate: f32,
    pub power_storage_stability_bonus: f32,
    pub power_line_capacity: Scalar,
    pub power_storage_efficiency: Scalar,
    pub power_storage_bleed: Scalar,
    pub power_instability_warn: Scalar,
    pub power_instability_critical: Scalar,
    pub mass_flux_epsilon: Scalar,
    pub base_trade_tariff: Scalar,
    pub base_trade_openness: Scalar,
    pub trade_openness_decay: Scalar,
    pub trade_leak_min_ticks: u32,
    pub trade_leak_max_ticks: u32,
    pub trade_leak_exponent: f32,
    pub trade_leak_progress: Scalar,
    pub migration_fragment_scaling: Scalar,
    pub migration_fidelity_floor: Scalar,
    pub corruption_logistics_penalty: Scalar,
    pub corruption_trade_penalty: Scalar,
    pub corruption_military_penalty: Scalar,
    pub snapshot_bind: SocketAddr,
    pub snapshot_flat_bind: SocketAddr,
    pub command_bind: SocketAddr,
    pub log_bind: SocketAddr,
    pub snapshot_history_limit: usize,
    pub crisis_auto_seed: bool,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct MoistureRaster {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

impl MoistureRaster {
    pub fn new(width: u32, height: u32, values: Vec<f32>) -> Self {
        Self {
            width,
            height,
            values,
        }
    }

    pub fn from_state(state: &FloatRasterState) -> Self {
        Self {
            width: state.width,
            height: state.height,
            values: state.samples.clone(),
        }
    }

    pub fn as_state(&self) -> FloatRasterState {
        FloatRasterState {
            width: self.width,
            height: self.height,
            samples: self.values.clone(),
        }
    }
}

pub const BUILTIN_SIMULATION_CONFIG: &str = include_str!("data/simulation_config.json");

impl Default for SimulationConfig {
    fn default() -> Self {
        SimulationConfig::builtin()
    }
}

impl SimulationConfig {
    pub fn builtin() -> Self {
        SimulationConfig::from_json_str(BUILTIN_SIMULATION_CONFIG)
            .expect("builtin simulation config should parse")
    }

    pub fn from_json_str(json: &str) -> Result<Self, SimulationConfigError> {
        let data: SimulationConfigData = serde_json::from_str(json)?;
        data.into_config()
    }

    pub fn from_file(path: &Path) -> Result<Self, SimulationConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SimulationConfigError::ReadFailed {
                path: path.to_path_buf(),
                source,
            })?;
        let config = SimulationConfig::from_json_str(&contents)?;
        Ok(config)
    }
}

#[derive(Debug, Error)]
pub enum SimulationConfigError {
    #[error("failed to parse simulation config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid socket address for `{field}`: {source}")]
    InvalidSocket {
        field: &'static str,
        #[source]
        source: std::net::AddrParseError,
    },
    #[error("failed to read simulation config from {path:?}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Deserialize)]
struct SimulationConfigData {
    grid_size: GridSizeData,
    #[serde(default = "default_map_preset_id")]
    map_preset_id: String,
    #[serde(default)]
    map_seed: u64,
    #[serde(default = "default_start_profile_id")]
    start_profile_id: String,
    #[serde(default)]
    hydrology: Option<HydrologyOverridesData>,
    ambient_temperature: f32,
    temperature_lerp: f32,
    logistics_flow_gain: f32,
    base_link_capacity: f32,
    mass_bounds: MassBoundsData,
    population_growth_rate: f32,
    temperature_morale_penalty: f32,
    population_cluster_stride: u32,
    population_cap: u32,
    power_adjust_rate: f32,
    max_power_generation: f32,
    max_power_efficiency: f32,
    min_power_influence: f32,
    max_power_influence: f32,
    power_generation_adjust_rate: f32,
    power_demand_adjust_rate: f32,
    power_storage_stability_bonus: f32,
    power_line_capacity: f32,
    power_storage_efficiency: f32,
    power_storage_bleed: f32,
    power_instability_warn: f32,
    power_instability_critical: f32,
    mass_flux_epsilon: f32,
    base_trade_tariff: f32,
    base_trade_openness: f32,
    trade_openness_decay: f32,
    trade_leak_min_ticks: u32,
    trade_leak_max_ticks: u32,
    trade_leak_exponent: f32,
    trade_leak_progress: f32,
    migration_fragment_scaling: f32,
    migration_fidelity_floor: f32,
    corruption_logistics_penalty: f32,
    corruption_trade_penalty: f32,
    corruption_military_penalty: f32,
    snapshot_bind: String,
    snapshot_flat_bind: String,
    command_bind: String,
    log_bind: String,
    snapshot_history_limit: usize,
    #[serde(default)]
    crisis_auto_seed: bool,
}

#[derive(Debug, Deserialize)]
struct GridSizeData {
    x: u32,
    y: u32,
}

#[derive(Debug, Deserialize)]
struct MassBoundsData {
    min: f32,
    max: f32,
}

#[derive(Debug, Deserialize, Default)]
struct HydrologyOverridesData {
    river_density: Option<f32>,
    river_min_count: Option<usize>,
    river_max_count: Option<usize>,
    accumulation_threshold_factor: Option<f32>,
    source_percentile: Option<f32>,
    source_sea_buffer: Option<f32>,
    min_length: Option<usize>,
    fallback_min_length: Option<usize>,
    spacing: Option<f32>,
    uphill_gain_pct: Option<f32>,
}

impl HydrologyOverridesData {
    fn into_overrides(self) -> HydrologyOverrides {
        HydrologyOverrides {
            river_density: self.river_density,
            river_min_count: self.river_min_count,
            river_max_count: self.river_max_count,
            accumulation_threshold_factor: self.accumulation_threshold_factor,
            source_percentile: self.source_percentile,
            source_sea_buffer: self.source_sea_buffer,
            min_length: self.min_length,
            fallback_min_length: self.fallback_min_length,
            spacing: self.spacing,
            uphill_gain_pct: self.uphill_gain_pct,
        }
    }
}

impl SimulationConfigData {
    fn into_config(self) -> Result<SimulationConfig, SimulationConfigError> {
        Ok(SimulationConfig {
            grid_size: UVec2::new(self.grid_size.x, self.grid_size.y),
            map_preset_id: self.map_preset_id,
            map_seed: self.map_seed,
            start_profile_id: self.start_profile_id,
            start_profile_overrides: StartProfileOverrides::default(),
            hydrology: self
                .hydrology
                .map(|d| d.into_overrides())
                .unwrap_or_default(),
            ambient_temperature: scalar_from_f32(self.ambient_temperature),
            temperature_lerp: scalar_from_f32(self.temperature_lerp),
            logistics_flow_gain: scalar_from_f32(self.logistics_flow_gain),
            base_link_capacity: scalar_from_f32(self.base_link_capacity),
            mass_bounds: (
                scalar_from_f32(self.mass_bounds.min),
                scalar_from_f32(self.mass_bounds.max),
            ),
            population_growth_rate: scalar_from_f32(self.population_growth_rate),
            temperature_morale_penalty: scalar_from_f32(self.temperature_morale_penalty),
            population_cluster_stride: self.population_cluster_stride,
            population_cap: self.population_cap,
            power_adjust_rate: scalar_from_f32(self.power_adjust_rate),
            max_power_generation: scalar_from_f32(self.max_power_generation),
            max_power_efficiency: scalar_from_f32(self.max_power_efficiency),
            min_power_influence: self.min_power_influence,
            max_power_influence: self.max_power_influence,
            power_generation_adjust_rate: self.power_generation_adjust_rate,
            power_demand_adjust_rate: self.power_demand_adjust_rate,
            power_storage_stability_bonus: self.power_storage_stability_bonus,
            power_line_capacity: scalar_from_f32(self.power_line_capacity),
            power_storage_efficiency: scalar_from_f32(self.power_storage_efficiency),
            power_storage_bleed: scalar_from_f32(self.power_storage_bleed),
            power_instability_warn: scalar_from_f32(self.power_instability_warn),
            power_instability_critical: scalar_from_f32(self.power_instability_critical),
            mass_flux_epsilon: scalar_from_f32(self.mass_flux_epsilon),
            base_trade_tariff: scalar_from_f32(self.base_trade_tariff),
            base_trade_openness: scalar_from_f32(self.base_trade_openness),
            trade_openness_decay: scalar_from_f32(self.trade_openness_decay),
            trade_leak_min_ticks: self.trade_leak_min_ticks,
            trade_leak_max_ticks: self.trade_leak_max_ticks,
            trade_leak_exponent: self.trade_leak_exponent,
            trade_leak_progress: scalar_from_f32(self.trade_leak_progress),
            migration_fragment_scaling: scalar_from_f32(self.migration_fragment_scaling),
            migration_fidelity_floor: scalar_from_f32(self.migration_fidelity_floor),
            corruption_logistics_penalty: scalar_from_f32(self.corruption_logistics_penalty),
            corruption_trade_penalty: scalar_from_f32(self.corruption_trade_penalty),
            corruption_military_penalty: scalar_from_f32(self.corruption_military_penalty),
            snapshot_bind: parse_socket(self.snapshot_bind, "snapshot_bind")?,
            snapshot_flat_bind: parse_socket(self.snapshot_flat_bind, "snapshot_flat_bind")?,
            command_bind: parse_socket(self.command_bind, "command_bind")?,
            log_bind: parse_socket(self.log_bind, "log_bind")?,
            snapshot_history_limit: self.snapshot_history_limit,
            crisis_auto_seed: self.crisis_auto_seed,
        })
    }
}

fn default_map_preset_id() -> String {
    "earthlike".to_string()
}

fn default_start_profile_id() -> String {
    "late_forager_tribe".to_string()
}

fn parse_socket(value: String, field: &'static str) -> Result<SocketAddr, SimulationConfigError> {
    SocketAddr::from_str(&value)
        .map_err(|source| SimulationConfigError::InvalidSocket { field, source })
}

#[derive(Resource, Debug, Clone)]
pub struct SimulationConfigMetadata {
    path: Option<PathBuf>,
    seed_random: bool,
}

impl SimulationConfigMetadata {
    pub fn new(path: Option<PathBuf>, seed_random: bool) -> Self {
        Self { path, seed_random }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }

    pub fn seed_random(&self) -> bool {
        self.seed_random
    }

    pub fn set_seed_random(&mut self, value: bool) {
        self.seed_random = value;
    }
}

pub fn load_simulation_config_from_env() -> (SimulationConfig, SimulationConfigMetadata) {
    let override_path = env::var("SIM_CONFIG_PATH").ok().map(PathBuf::from);

    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/simulation_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match SimulationConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "simulation_config.loaded=file"
                );
                let random_seed = config.map_seed == 0;
                return (
                    config,
                    SimulationConfigMetadata::new(Some(path), random_seed),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "simulation_config.load_failed"
                );
            }
        }
    }

    let config = SimulationConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "simulation_config.loaded=builtin"
    );
    let random_seed = config.map_seed == 0;
    (config, SimulationConfigMetadata::new(None, random_seed))
}

/// Tracks total simulation ticks elapsed.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationTick(pub u64);

#[derive(Resource, Debug, Clone, Copy)]
pub struct StartLocation {
    position: Option<UVec2>,
    survey_radius: Option<u32>,
    fog_mode: FogMode,
}

impl Default for StartLocation {
    fn default() -> Self {
        Self {
            position: None,
            survey_radius: None,
            fog_mode: FogMode::Standard,
        }
    }
}

impl StartLocation {
    pub fn new(position: Option<UVec2>) -> Self {
        Self {
            position,
            survey_radius: None,
            fog_mode: FogMode::Standard,
        }
    }

    pub fn from_profile(position: Option<UVec2>, overrides: &StartProfileOverrides) -> Self {
        Self {
            position,
            survey_radius: overrides.survey_radius,
            fog_mode: overrides.fog_mode.unwrap_or(FogMode::Standard),
        }
    }

    pub fn position(&self) -> Option<UVec2> {
        self.position
    }

    pub fn survey_radius(&self) -> Option<u32> {
        self.survey_radius
    }

    pub fn fog_mode(&self) -> FogMode {
        self.fog_mode
    }

    pub fn relocate(&mut self, position: UVec2) {
        self.position = Some(position);
    }

    pub fn set_survey_radius(&mut self, radius: Option<u32>) {
        self.survey_radius = radius;
    }

    pub fn set_fog_mode(&mut self, mode: FogMode) {
        self.fog_mode = mode;
    }
}

#[derive(Debug, Clone)]
pub struct FogReveal {
    pub center: UVec2,
    pub radius: u32,
    pub expires_at: u64,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct FogRevealLedger {
    reveals: Vec<FogReveal>,
}

impl FogRevealLedger {
    pub fn queue(&mut self, center: UVec2, radius: u32, expires_at: u64) {
        let radius = radius.max(1);
        self.reveals.push(FogReveal {
            center,
            radius,
            expires_at,
        });
    }

    pub fn iter_active(&self, tick: u64) -> impl Iterator<Item = &FogReveal> {
        self.reveals
            .iter()
            .filter(move |reveal| reveal.expires_at >= tick)
    }

    pub fn prune_expired(&mut self, tick: u64) {
        self.reveals.retain(|reveal| reveal.expires_at >= tick);
    }

    pub fn is_empty(&self) -> bool {
        self.reveals.is_empty()
    }
}

/// Authoritative sentiment axis bias values applied across factions.
///
/// Sentiment is composed of three categories of forces:
/// - **Policy levers** (`policy`): long-lived adjustments driven by enacted reforms or manual tweaks.
/// - **Incident deltas** (`incidents`): short-lived shocks produced by exposed scandals, crises, etc.
/// - **Influencer output** (`influencer`): procedurally generated contributions from the influencer roster.
#[derive(Resource, Debug, Clone)]
pub struct SentimentAxisBias {
    policy: [Scalar; 4],
    incidents: [Scalar; 4],
    influencer: [Scalar; 4],
}

impl Default for SentimentAxisBias {
    fn default() -> Self {
        Self {
            policy: [Scalar::zero(); 4],
            incidents: [Scalar::zero(); 4],
            influencer: [Scalar::zero(); 4],
        }
    }
}

impl SentimentAxisBias {
    pub fn set_policy_axis(&mut self, axis: usize, value: Scalar) {
        if let Some(slot) = self.policy.get_mut(axis) {
            *slot = value;
        }
    }

    pub fn set_policy_axes(&mut self, values: [Scalar; 4]) {
        self.policy = values;
    }

    pub fn policy_values(&self) -> [Scalar; 4] {
        self.policy
    }

    pub fn set_influencer(&mut self, deltas: [Scalar; 4]) {
        self.influencer = deltas;
    }

    pub fn influencer_values(&self) -> [Scalar; 4] {
        self.influencer
    }

    pub fn incident_values(&self) -> [Scalar; 4] {
        self.incidents
    }

    pub fn apply_incident_delta(&mut self, axis: usize, delta: Scalar) {
        if let Some(slot) = self.incidents.get_mut(axis) {
            *slot = (*slot + delta).clamp(Scalar::from_f32(-2.0), Scalar::from_f32(2.0));
        }
    }

    pub fn reset_incidents(&mut self) {
        self.incidents = [Scalar::zero(); 4];
    }

    pub fn manual_environment(&self) -> [Scalar; 4] {
        let mut result = self.policy;
        for (idx, incident) in self.incidents.iter().enumerate() {
            result[idx] += *incident;
        }
        result
    }

    pub fn combined(&self) -> [Scalar; 4] {
        let mut result = self.manual_environment();
        for (idx, delta) in self.influencer.iter().enumerate() {
            result[idx] += *delta;
        }
        result
    }

    pub fn as_raw(&self) -> [i64; 4] {
        self.combined().map(Scalar::raw)
    }

    pub fn reset_to_state(&mut self, policy: [Scalar; 4], incidents: [Scalar; 4]) {
        self.policy = policy;
        self.incidents = incidents;
        self.influencer = [Scalar::zero(); 4];
    }
}

/// Index of tile entities for reuse by other systems.
#[derive(Resource, Debug, Clone)]
pub struct TileRegistry {
    pub tiles: Vec<Entity>,
    pub width: u32,
    pub height: u32,
}

impl TileRegistry {
    pub fn index(&self, x: u32, y: u32) -> Option<Entity> {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.tiles.get(idx).cloned()
        } else {
            None
        }
    }
}

/// Tracks corruption intensity across subsystems for snapshot export.
#[derive(Resource, Debug, Clone, Default)]
pub struct CorruptionLedgers {
    ledger: CorruptionLedger,
}

impl CorruptionLedgers {
    pub fn ledger(&self) -> &CorruptionLedger {
        &self.ledger
    }

    pub fn ledger_mut(&mut self) -> &mut CorruptionLedger {
        &mut self.ledger
    }

    pub fn total_intensity(&self, subsystem: CorruptionSubsystem) -> i64 {
        self.ledger
            .entries
            .iter()
            .filter(|entry| entry.subsystem == subsystem)
            .map(|entry| entry.intensity.max(0))
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorruptionExposureRecord {
    pub incident_id: u64,
    pub subsystem: CorruptionSubsystem,
    pub intensity: i64,
    pub trust_delta: i64,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CorruptionTelemetry {
    pub active_incidents: usize,
    pub exposures_this_turn: Vec<CorruptionExposureRecord>,
    pub exposures_total: u64,
}

impl CorruptionTelemetry {
    pub fn reset_turn(&mut self) {
        self.exposures_this_turn.clear();
    }

    pub fn record_exposure(&mut self, record: CorruptionExposureRecord) {
        self.exposures_this_turn.push(record);
        self.exposures_total += 1;
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DiplomacyLeverage {
    pub recent: Vec<CorruptionExposureRecord>,
    pub max_entries: usize,
    pub culture_signals: Vec<CultureTensionRecord>,
    pub great_discoveries: Vec<(FactionId, u16)>,
}

impl DiplomacyLeverage {
    pub fn push(&mut self, record: CorruptionExposureRecord) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.recent.push(record);
        if self.recent.len() > self.max_entries {
            let overflow = self.recent.len() - self.max_entries;
            self.recent.drain(0..overflow);
        }
    }

    pub fn push_culture_signal(&mut self, record: CultureTensionRecord) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.culture_signals.push(record);
        if self.culture_signals.len() > self.max_entries {
            let overflow = self.culture_signals.len() - self.max_entries;
            self.culture_signals.drain(0..overflow);
        }
    }

    pub fn push_great_discovery(&mut self, faction: FactionId, discovery_id: u16) {
        if self.max_entries == 0 {
            self.max_entries = 16;
        }
        self.great_discoveries.push((faction, discovery_id));
        if self.great_discoveries.len() > self.max_entries {
            let overflow = self.great_discoveries.len() - self.max_entries;
            self.great_discoveries.drain(0..overflow);
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct PendingCrisisSeeds {
    pub seeds: Vec<(FactionId, u16)>,
}

impl PendingCrisisSeeds {
    pub fn push(&mut self, faction: FactionId, discovery_id: u16) {
        self.seeds.push((faction, discovery_id));
    }

    pub fn drain(&mut self) -> Vec<(FactionId, u16)> {
        std::mem::take(&mut self.seeds)
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct PendingCrisisSpawns {
    pub spawns: Vec<(FactionId, String)>,
}

impl PendingCrisisSpawns {
    pub fn push<S: Into<String>>(&mut self, faction: FactionId, archetype_id: S) {
        self.spawns.push((faction, archetype_id.into()));
    }

    pub fn drain(&mut self) -> Vec<(FactionId, String)> {
        std::mem::take(&mut self.spawns)
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct DiscoveryProgressLedger {
    pub progress: HashMap<FactionId, HashMap<u32, Scalar>>,
}

impl DiscoveryProgressLedger {
    pub fn add_progress(&mut self, faction: FactionId, discovery_id: u32, delta: Scalar) -> Scalar {
        let faction_entry = self.progress.entry(faction).or_default();
        let entry = faction_entry
            .entry(discovery_id)
            .or_insert_with(Scalar::zero);
        *entry = (*entry + delta).clamp(Scalar::zero(), Scalar::one());
        *entry
    }

    pub fn get_progress(&self, faction: FactionId, discovery_id: u32) -> Scalar {
        self.progress
            .get(&faction)
            .and_then(|map| map.get(&discovery_id))
            .copied()
            .unwrap_or_else(Scalar::zero)
    }
}

#[derive(Debug, Clone)]
pub struct TradeDiffusionRecord {
    pub tick: u64,
    pub from: FactionId,
    pub to: FactionId,
    pub discovery_id: u32,
    pub delta: Scalar,
    pub via_migration: bool,
    pub herd_density: f32,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct TradeTelemetry {
    pub tech_diffusion_applied: u32,
    pub migration_transfers: u32,
    pub records: Vec<TradeDiffusionRecord>,
}

impl TradeTelemetry {
    pub fn reset_turn(&mut self) {
        self.tech_diffusion_applied = 0;
        self.migration_transfers = 0;
        self.records.clear();
    }

    pub fn push_record(&mut self, record: TradeDiffusionRecord) {
        self.records.push(record);
    }
}

/// Per-faction stockpile of abstracted inventory items granted by start profiles.
#[derive(Resource, Debug, Clone, Default)]
pub struct FactionInventory {
    stockpiles: HashMap<FactionId, HashMap<String, i64>>,
}

impl FactionInventory {
    pub fn add_stockpile<S: Into<String>>(&mut self, faction: FactionId, item: S, quantity: i64) {
        if quantity == 0 {
            return;
        }
        let entry = self.stockpiles.entry(faction).or_default();
        *entry.entry(item.into()).or_insert(0) += quantity;
    }

    pub fn take_stockpile(&mut self, faction: FactionId, item: &str, quantity: i64) -> i64 {
        if quantity <= 0 {
            return 0;
        }
        let Some(entry) = self.stockpiles.get_mut(&faction) else {
            return 0;
        };
        let (removable, cleanup_faction) = {
            let Some(slot) = entry.get_mut(item) else {
                return 0;
            };
            let removable = (*slot).min(quantity);
            *slot -= removable;
            if *slot == 0 {
                entry.remove(item);
            }
            (removable, entry.is_empty())
        };
        if cleanup_faction {
            self.stockpiles.remove(&faction);
        }
        removable
    }

    pub fn stockpile(&self, faction: FactionId) -> Option<&HashMap<String, i64>> {
        self.stockpiles.get(&faction)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&FactionId, &HashMap<String, i64>)> {
        self.stockpiles.iter()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandEventKind {
    Scout,
    FollowHerd,
    FoundCamp,
    Forage,
    Hunt,
}

impl CommandEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandEventKind::Scout => "scout",
            CommandEventKind::FollowHerd => "follow_herd",
            CommandEventKind::FoundCamp => "found_camp",
            CommandEventKind::Forage => "forage",
            CommandEventKind::Hunt => "hunt",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandEventEntry {
    pub tick: u64,
    pub kind: CommandEventKind,
    pub faction: FactionId,
    pub label: String,
    pub detail: Option<String>,
}

impl CommandEventEntry {
    pub fn new<S: Into<String>>(
        tick: u64,
        kind: CommandEventKind,
        faction: FactionId,
        label: S,
        detail: Option<String>,
    ) -> Self {
        Self {
            tick,
            kind,
            faction,
            label: label.into(),
            detail,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct CommandEventLog {
    entries: Vec<CommandEventEntry>,
    max_entries: usize,
}

impl Default for CommandEventLog {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 32,
        }
    }
}

impl CommandEventLog {
    pub fn push(&mut self, entry: CommandEventEntry) {
        if self.entries.len() >= self.max_entries {
            let overflow = self.entries.len() + 1 - self.max_entries;
            self.entries.drain(0..overflow);
        }
        self.entries.push(entry);
    }

    pub fn iter(&self) -> impl Iterator<Item = &CommandEventEntry> {
        self.entries.iter()
    }
}
