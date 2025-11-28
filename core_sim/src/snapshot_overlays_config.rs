use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    food::FoodModule,
    scalar::{scalar_from_f32, Scalar},
};

pub const BUILTIN_SNAPSHOT_OVERLAYS_CONFIG: &str =
    include_str!("data/snapshot_overlays_config.json");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SnapshotOverlaysConfig {
    corruption: CorruptionOverlayConfig,
    culture: CultureOverlayConfig,
    military: MilitaryOverlayConfig,
    fog: FogOverlayConfig,
    food: FoodOverlayConfig,
}

impl SnapshotOverlaysConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_SNAPSHOT_OVERLAYS_CONFIG)
                .expect("builtin snapshot overlays config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, SnapshotOverlaysConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SnapshotOverlaysConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        let config = SnapshotOverlaysConfig::from_json_str(&contents)?;
        Ok(config)
    }

    pub fn corruption(&self) -> &CorruptionOverlayConfig {
        &self.corruption
    }

    pub fn culture(&self) -> &CultureOverlayConfig {
        &self.culture
    }

    pub fn military(&self) -> &MilitaryOverlayConfig {
        &self.military
    }

    pub fn fog(&self) -> &FogOverlayConfig {
        &self.fog
    }

    pub fn food(&self) -> &FoodOverlayConfig {
        &self.food
    }
}

#[derive(Debug, Error)]
pub enum SnapshotOverlaysConfigError {
    #[error("failed to parse snapshot overlays config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read snapshot overlays config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CorruptionOverlayConfig {
    logistics_weight: f32,
    trade_weight: f32,
    military_weight: f32,
    governance_weight: f32,
    logistics_spike_multiplier: f32,
    trade_spike_multiplier: f32,
    military_spike_multiplier: f32,
    governance_spike_multiplier: f32,
}

impl CorruptionOverlayConfig {
    pub fn logistics_weight(&self) -> Scalar {
        scalar_from_f32(self.logistics_weight)
    }

    pub fn trade_weight(&self) -> Scalar {
        scalar_from_f32(self.trade_weight)
    }

    pub fn military_weight(&self) -> Scalar {
        scalar_from_f32(self.military_weight)
    }

    pub fn governance_weight(&self) -> Scalar {
        scalar_from_f32(self.governance_weight)
    }

    pub fn logistics_spike_multiplier(&self) -> f32 {
        self.logistics_spike_multiplier
    }

    pub fn trade_spike_multiplier(&self) -> f32 {
        self.trade_spike_multiplier
    }

    pub fn military_spike_multiplier(&self) -> f32 {
        self.military_spike_multiplier
    }

    pub fn governance_spike_multiplier(&self) -> f32 {
        self.governance_spike_multiplier
    }
}

impl Default for CorruptionOverlayConfig {
    fn default() -> Self {
        Self {
            logistics_weight: 0.35,
            trade_weight: 0.25,
            military_weight: 0.2,
            governance_weight: 0.2,
            logistics_spike_multiplier: 2.0,
            trade_spike_multiplier: 2.0,
            military_spike_multiplier: 1.0,
            governance_spike_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureOverlayConfig {
    hard_tick_bonus_step: f32,
    hard_tick_bonus_cap: f32,
    soft_tick_bonus_step: f32,
    soft_tick_bonus_cap: f32,
}

impl CultureOverlayConfig {
    pub fn hard_tick_bonus_step(&self) -> f32 {
        self.hard_tick_bonus_step
    }

    pub fn hard_tick_bonus_cap(&self) -> f32 {
        self.hard_tick_bonus_cap
    }

    pub fn soft_tick_bonus_step(&self) -> f32 {
        self.soft_tick_bonus_step
    }

    pub fn soft_tick_bonus_cap(&self) -> f32 {
        self.soft_tick_bonus_cap
    }
}

impl Default for CultureOverlayConfig {
    fn default() -> Self {
        Self {
            hard_tick_bonus_step: 0.05,
            hard_tick_bonus_cap: 0.5,
            soft_tick_bonus_step: 0.03,
            soft_tick_bonus_cap: 0.3,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MilitaryOverlayConfig {
    size_factor_denominator: f32,
    presence_clamp_max: f32,
    heavy_size_threshold: u32,
    heavy_size_bonus: f32,
    support_clamp_max: f32,
    power_margin_max: f32,
    presence_weight: f32,
    support_weight: f32,
    combined_clamp_max: f32,
}

impl MilitaryOverlayConfig {
    pub fn size_factor_denominator(&self) -> f32 {
        self.size_factor_denominator.max(f32::EPSILON)
    }

    pub fn presence_clamp_max(&self) -> Scalar {
        scalar_from_f32(self.presence_clamp_max)
    }

    pub fn heavy_size_threshold(&self) -> u32 {
        self.heavy_size_threshold
    }

    pub fn heavy_size_bonus(&self) -> Scalar {
        scalar_from_f32(self.heavy_size_bonus)
    }

    pub fn support_clamp_max(&self) -> Scalar {
        scalar_from_f32(self.support_clamp_max)
    }

    pub fn power_margin_max(&self) -> Scalar {
        scalar_from_f32(self.power_margin_max)
    }

    pub fn presence_weight(&self) -> Scalar {
        scalar_from_f32(self.presence_weight)
    }

    pub fn support_weight(&self) -> Scalar {
        scalar_from_f32(self.support_weight)
    }

    pub fn combined_clamp_max(&self) -> Scalar {
        scalar_from_f32(self.combined_clamp_max)
    }
}

impl Default for MilitaryOverlayConfig {
    fn default() -> Self {
        Self {
            size_factor_denominator: 1_500.0,
            presence_clamp_max: 5.0,
            heavy_size_threshold: 2_500,
            heavy_size_bonus: 0.1,
            support_clamp_max: 5.0,
            power_margin_max: 5.0,
            presence_weight: 0.6,
            support_weight: 0.4,
            combined_clamp_max: 5.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FogOverlayConfig {
    global_local_blend: f32,
}

impl FogOverlayConfig {
    pub fn global_local_blend(&self) -> Scalar {
        scalar_from_f32(self.global_local_blend)
    }
}

impl Default for FogOverlayConfig {
    fn default() -> Self {
        Self {
            global_local_blend: 0.5,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FoodOverlayConfig {
    max_total_sites: usize,
    default_radius: u32,
    radius_padding: u32,
    min_site_spacing: u32,
    provisions_per_weight: f32,
    trade_goods_per_weight: f32,
    trade_bonus_modules: HashMap<String, f32>,
    wild_game_modules: Vec<FoodModule>,
    wild_game_probability: f32,
    wild_game_weight_scale: f32,
    wild_game_max_per_module: usize,
    wild_game_max_total: usize,
    wild_game_radius: u32,
}

impl FoodOverlayConfig {
    pub fn max_total_sites(&self) -> usize {
        self.max_total_sites.max(1)
    }

    pub fn default_radius(&self) -> u32 {
        self.default_radius
    }

    pub fn radius_padding(&self) -> u32 {
        self.radius_padding
    }

    pub fn provisions_per_weight(&self) -> f32 {
        self.provisions_per_weight.max(0.0)
    }

    pub fn trade_goods_per_weight(&self) -> f32 {
        self.trade_goods_per_weight.max(0.0)
    }

    pub fn trade_bonus_for(&self, module: &FoodModule) -> f32 {
        self.trade_bonus_modules
            .get(module.as_str())
            .copied()
            .unwrap_or(0.0)
    }

    pub fn wild_game_modules(&self) -> &[FoodModule] {
        &self.wild_game_modules
    }

    pub fn wild_game_probability(&self) -> f32 {
        self.wild_game_probability.clamp(0.0, 1.0)
    }

    pub fn wild_game_weight_scale(&self) -> f32 {
        if self.wild_game_weight_scale <= 0.0 {
            1.0
        } else {
            self.wild_game_weight_scale
        }
    }

    pub fn wild_game_max_per_module(&self) -> usize {
        self.wild_game_max_per_module
    }

    pub fn wild_game_max_total(&self) -> usize {
        self.wild_game_max_total
    }

    pub fn wild_game_radius(&self) -> u32 {
        self.wild_game_radius
    }

    pub fn min_site_spacing(&self) -> u32 {
        self.min_site_spacing.max(1)
    }
}

impl Default for FoodOverlayConfig {
    fn default() -> Self {
        Self {
            max_total_sites: 40,
            default_radius: 6,
            radius_padding: 2,
            min_site_spacing: 4,
            provisions_per_weight: 120.0,
            trade_goods_per_weight: 35.0,
            trade_bonus_modules: HashMap::from([
                ("coastal_littoral".to_string(), 25.0),
                ("riverine_delta".to_string(), 15.0),
                ("coastal_upwelling".to_string(), 30.0),
            ]),
            wild_game_modules: vec![
                FoodModule::TemperateForest,
                FoodModule::MixedWoodland,
                FoodModule::SavannaGrassland,
                FoodModule::MontaneHighland,
                FoodModule::WetlandSwamp,
            ],
            wild_game_probability: 0.35,
            wild_game_weight_scale: 0.65,
            wild_game_max_per_module: 3,
            wild_game_max_total: 12,
            wild_game_radius: 8,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SnapshotOverlaysConfigHandle(pub Arc<SnapshotOverlaysConfig>);

impl SnapshotOverlaysConfigHandle {
    pub fn new(config: Arc<SnapshotOverlaysConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<SnapshotOverlaysConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<SnapshotOverlaysConfig>) {
        self.0 = config;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct SnapshotOverlaysConfigMetadata {
    path: Option<PathBuf>,
}

impl SnapshotOverlaysConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

pub fn load_snapshot_overlays_config_from_env(
) -> (Arc<SnapshotOverlaysConfig>, SnapshotOverlaysConfigMetadata) {
    let override_path = env::var("SNAPSHOT_OVERLAYS_CONFIG_PATH")
        .ok()
        .map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/snapshot_overlays_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match SnapshotOverlaysConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "snapshot_overlays_config.loaded=file"
                );
                return (
                    Arc::new(config),
                    SnapshotOverlaysConfigMetadata::new(Some(path)),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "snapshot_overlays_config.load_failed"
                );
            }
        }
    }

    let config = SnapshotOverlaysConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "snapshot_overlays_config.loaded=builtin"
    );
    (config, SnapshotOverlaysConfigMetadata::new(None))
}
