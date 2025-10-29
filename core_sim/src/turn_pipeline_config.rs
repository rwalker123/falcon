use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::scalar::{scalar_from_f32, Scalar};

pub const BUILTIN_TURN_PIPELINE_CONFIG: &str = include_str!("data/turn_pipeline_config.json");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TurnPipelineConfig {
    logistics: LogisticsPhaseConfig,
    trade: TradePhaseConfig,
    population: PopulationPhaseConfig,
    power: PowerPhaseConfig,
}

impl TurnPipelineConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_TURN_PIPELINE_CONFIG)
                .expect("builtin turn pipeline config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, TurnPipelineConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| TurnPipelineConfigError::ReadFailed {
                path: path.to_path_buf(),
                source,
            })?;
        let config = TurnPipelineConfig::from_json_str(&contents)?;
        Ok(config)
    }

    pub fn logistics(&self) -> &LogisticsPhaseConfig {
        &self.logistics
    }

    pub fn trade(&self) -> &TradePhaseConfig {
        &self.trade
    }

    pub fn population(&self) -> &PopulationPhaseConfig {
        &self.population
    }

    pub fn power(&self) -> &PowerPhaseConfig {
        &self.power
    }
}

#[derive(Debug, Error)]
pub enum TurnPipelineConfigError {
    #[error("failed to parse turn pipeline config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read turn pipeline config from {path:?}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LogisticsPhaseConfig {
    flow_gain_min: f32,
    flow_gain_max: f32,
    effective_gain_min: f32,
    penalty_min: f32,
    penalty_scalar_min: f32,
    capacity_min: f32,
    attrition_max: f32,
}

impl LogisticsPhaseConfig {
    pub fn flow_gain_min(&self) -> Scalar {
        scalar_from_f32(self.flow_gain_min)
    }

    pub fn flow_gain_max(&self) -> Scalar {
        scalar_from_f32(self.flow_gain_max)
    }

    pub fn effective_gain_min(&self) -> Scalar {
        scalar_from_f32(self.effective_gain_min)
    }

    pub fn penalty_min(&self) -> f32 {
        self.penalty_min
    }

    pub fn penalty_scalar_min(&self) -> f32 {
        self.penalty_scalar_min
    }

    pub fn capacity_min(&self) -> Scalar {
        scalar_from_f32(self.capacity_min)
    }

    pub fn attrition_max(&self) -> f32 {
        self.attrition_max
    }
}

impl Default for LogisticsPhaseConfig {
    fn default() -> Self {
        Self {
            flow_gain_min: 0.02,
            flow_gain_max: 0.5,
            effective_gain_min: 0.005,
            penalty_min: 0.05,
            penalty_scalar_min: 0.1,
            capacity_min: 0.05,
            attrition_max: 0.95,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TradePhaseConfig {
    tariff_min: f32,
    tariff_max_scalar: f32,
}

impl TradePhaseConfig {
    pub fn tariff_min(&self) -> Scalar {
        scalar_from_f32(self.tariff_min)
    }

    pub fn tariff_max_scalar(&self) -> Scalar {
        scalar_from_f32(self.tariff_max_scalar)
    }
}

impl Default for TradePhaseConfig {
    fn default() -> Self {
        Self {
            tariff_min: 0.0,
            tariff_max_scalar: 1.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PopulationPhaseConfig {
    attrition_penalty_scale: f32,
    hardness_penalty_scale: f32,
    temperature_growth_penalty: f32,
    morale_influence_scale: f32,
    culture_bias_scale: f32,
    attrition_morale_scale: f32,
    growth_clamp: f32,
    migration_morale_threshold: f32,
    migration_eta_ticks: u16,
}

impl PopulationPhaseConfig {
    pub fn attrition_penalty_scale(&self) -> Scalar {
        scalar_from_f32(self.attrition_penalty_scale)
    }

    pub fn hardness_penalty_scale(&self) -> Scalar {
        scalar_from_f32(self.hardness_penalty_scale)
    }

    pub fn temperature_growth_penalty(&self) -> Scalar {
        scalar_from_f32(self.temperature_growth_penalty)
    }

    pub fn morale_influence_scale(&self) -> Scalar {
        scalar_from_f32(self.morale_influence_scale)
    }

    pub fn culture_bias_scale(&self) -> Scalar {
        scalar_from_f32(self.culture_bias_scale)
    }

    pub fn attrition_morale_scale(&self) -> Scalar {
        scalar_from_f32(self.attrition_morale_scale)
    }

    pub fn growth_clamp(&self) -> Scalar {
        scalar_from_f32(self.growth_clamp.abs())
    }

    pub fn migration_morale_threshold(&self) -> Scalar {
        scalar_from_f32(self.migration_morale_threshold)
    }

    pub fn migration_eta_ticks(&self) -> u16 {
        self.migration_eta_ticks
    }
}

impl Default for PopulationPhaseConfig {
    fn default() -> Self {
        Self {
            attrition_penalty_scale: 0.2,
            hardness_penalty_scale: 0.05,
            temperature_growth_penalty: 0.0005,
            morale_influence_scale: 0.4,
            culture_bias_scale: 0.5,
            attrition_morale_scale: 0.5,
            growth_clamp: 0.06,
            migration_morale_threshold: 0.78,
            migration_eta_ticks: 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PowerPhaseConfig {
    efficiency_adjust_scale: f32,
    efficiency_floor: f32,
    influence_demand_reduction: f32,
    storage_efficiency_min: f32,
    storage_efficiency_max: f32,
    storage_bleed_max: f32,
}

impl PowerPhaseConfig {
    pub fn efficiency_adjust_scale(&self) -> Scalar {
        scalar_from_f32(self.efficiency_adjust_scale)
    }

    pub fn efficiency_floor(&self) -> Scalar {
        scalar_from_f32(self.efficiency_floor)
    }

    pub fn influence_demand_reduction(&self) -> Scalar {
        scalar_from_f32(self.influence_demand_reduction)
    }

    pub fn storage_efficiency_min(&self) -> Scalar {
        scalar_from_f32(self.storage_efficiency_min)
    }

    pub fn storage_efficiency_max(&self) -> Scalar {
        scalar_from_f32(self.storage_efficiency_max)
    }

    pub fn storage_bleed_max(&self) -> Scalar {
        scalar_from_f32(self.storage_bleed_max)
    }
}

impl Default for PowerPhaseConfig {
    fn default() -> Self {
        Self {
            efficiency_adjust_scale: 0.01,
            efficiency_floor: 0.5,
            influence_demand_reduction: 0.25,
            storage_efficiency_min: 0.1,
            storage_efficiency_max: 1.0,
            storage_bleed_max: 0.25,
        }
    }
}

#[derive(Resource, Debug, Clone)]
pub struct TurnPipelineConfigHandle(pub Arc<TurnPipelineConfig>);

impl TurnPipelineConfigHandle {
    pub fn new(config: Arc<TurnPipelineConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<TurnPipelineConfig> {
        Arc::clone(&self.0)
    }

    pub fn config(&self) -> &TurnPipelineConfig {
        &self.0
    }

    pub fn replace(&mut self, config: Arc<TurnPipelineConfig>) {
        self.0 = config;
    }
}

#[derive(Resource, Debug, Clone)]
pub struct TurnPipelineConfigMetadata {
    path: Option<PathBuf>,
}

impl TurnPipelineConfigMetadata {
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

pub fn load_turn_pipeline_config_from_env() -> (Arc<TurnPipelineConfig>, TurnPipelineConfigMetadata)
{
    let override_path = env::var("TURN_PIPELINE_CONFIG_PATH")
        .ok()
        .map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/turn_pipeline_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match TurnPipelineConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "turn_pipeline_config.loaded=file"
                );
                return (
                    Arc::new(config),
                    TurnPipelineConfigMetadata::new(Some(path)),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "turn_pipeline_config.load_failed"
                );
            }
        }
    }

    let config = TurnPipelineConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "turn_pipeline_config.loaded=builtin"
    );
    (config, TurnPipelineConfigMetadata::new(None))
}
