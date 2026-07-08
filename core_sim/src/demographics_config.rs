//! Data-driven tuning for the demographic population model.
//!
//! Loaded from `data/demographics_config.json`. Each `PopulationCohort` carries three age
//! brackets (children / working-age / elders) plus a local food larder; `simulate_population`
//! (see `systems.rs`) draws per-capita food each turn, then resolves scarcity/cold deaths,
//! births, maturation, aging, and elder mortality from these rates. Mirrors the
//! `sedentarization_config.rs` / `fauna_config.rs` loader (baked-in builtin + optional
//! file/env override).

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_DEMOGRAPHICS_CONFIG: &str = include_str!("data/demographics_config.json");

/// Fractions (summing to ~1.0) that split a freshly spawned cohort's head-count into the three
/// age brackets.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsDistribution {
    pub children: f32,
    pub working: f32,
    pub elders: f32,
}

impl Default for DemographicsDistribution {
    fn default() -> Self {
        Self {
            children: 0.30,
            working: 0.55,
            elders: 0.15,
        }
    }
}

/// Per-turn food draw. `demand = per_capita_draw × (children·child_factor + working·working_factor
/// + elders·elder_factor)`; the per-bracket factors let dependents eat less than a working adult.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsConsumption {
    pub per_capita_draw: f32,
    pub child_factor: f32,
    pub working_factor: f32,
    pub elder_factor: f32,
}

impl Default for DemographicsConsumption {
    fn default() -> Self {
        Self {
            per_capita_draw: 0.5,
            child_factor: 0.6,
            working_factor: 1.0,
            elder_factor: 0.8,
        }
    }
}

/// Birth tuning. `births = birth_rate × working × fed_ratio × morale_signal ×
/// (1 + surplus_bonus × surplus_ratio)`, added to children. `morale_floor` is the morale below
/// which no births occur.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsBirths {
    pub birth_rate: f32,
    pub surplus_bonus: f32,
    pub morale_floor: f32,
}

impl Default for DemographicsBirths {
    fn default() -> Self {
        Self {
            birth_rate: 0.03,
            surplus_bonus: 0.5,
            morale_floor: 0.2,
        }
    }
}

/// Starvation tuning. When food demand outruns the larder, each bracket loses
/// `deficit_fraction × starvation_mortality × <bracket>_vulnerability` of its head-count
/// (dependents typically more vulnerable than working-age).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsScarcity {
    pub starvation_mortality: f32,
    pub child_vulnerability: f32,
    pub working_vulnerability: f32,
    pub elder_vulnerability: f32,
}

impl Default for DemographicsScarcity {
    fn default() -> Self {
        Self {
            starvation_mortality: 0.5,
            child_vulnerability: 1.5,
            working_vulnerability: 1.0,
            elder_vulnerability: 1.5,
        }
    }
}

/// Cold-death tuning. Temperature deviation beyond `temp_tolerance` (°, absolute) kills
/// `min(max_mortality, excess × mortality_scale)` of every bracket per turn.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsCold {
    pub temp_tolerance: f32,
    pub mortality_scale: f32,
    pub max_mortality: f32,
}

impl Default for DemographicsCold {
    fn default() -> Self {
        Self {
            temp_tolerance: 12.0,
            mortality_scale: 0.02,
            max_mortality: 0.1,
        }
    }
}

/// Root demographic configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsConfig {
    pub initial_distribution: DemographicsDistribution,
    pub consumption: DemographicsConsumption,
    pub births: DemographicsBirths,
    /// Fraction of children that mature into the working bracket each turn.
    pub maturation_rate: f32,
    /// Fraction of working-age that age into the elder bracket each turn.
    pub aging_rate: f32,
    /// Fraction of elders that die of old age each turn.
    pub elder_mortality_rate: f32,
    pub scarcity: DemographicsScarcity,
    pub cold: DemographicsCold,
}

impl Default for DemographicsConfig {
    fn default() -> Self {
        Self {
            initial_distribution: DemographicsDistribution::default(),
            consumption: DemographicsConsumption::default(),
            births: DemographicsBirths::default(),
            maturation_rate: 0.05,
            aging_rate: 0.025,
            elder_mortality_rate: 0.06,
            scarcity: DemographicsScarcity::default(),
            cold: DemographicsCold::default(),
        }
    }
}

impl DemographicsConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG)
                .expect("builtin demographics config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, DemographicsConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| DemographicsConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(DemographicsConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum DemographicsConfigError {
    #[error("failed to read demographics config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse demographics config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the demographic configuration.
#[derive(Resource, Debug, Clone)]
pub struct DemographicsConfigHandle(pub Arc<DemographicsConfig>);

impl DemographicsConfigHandle {
    pub fn new(config: Arc<DemographicsConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<DemographicsConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<DemographicsConfig>) {
        self.0 = config;
    }
}

impl Default for DemographicsConfigHandle {
    fn default() -> Self {
        Self(DemographicsConfig::builtin())
    }
}

/// Metadata about the demographic configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct DemographicsConfigMetadata {
    path: Option<PathBuf>,
}

impl DemographicsConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load demographic config from environment (`DEMOGRAPHICS_CONFIG_PATH`) or the default data
/// path, falling back to the baked-in builtin.
pub fn load_demographics_config_from_env() -> (Arc<DemographicsConfig>, DemographicsConfigMetadata)
{
    let override_path = env::var("DEMOGRAPHICS_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/demographics_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match DemographicsConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "demographics_config.loaded=file"
                );
                return (
                    Arc::new(config),
                    DemographicsConfigMetadata::new(Some(path)),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "demographics_config.load_failed"
                );
            }
        }
    }

    let config = DemographicsConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "demographics_config.loaded=builtin"
    );
    (config, DemographicsConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_sane() {
        let config = DemographicsConfig::builtin();
        // Initial distribution is a probability split.
        let dist = &config.initial_distribution;
        let sum = dist.children + dist.working + dist.elders;
        assert!(
            (sum - 1.0).abs() < 1e-3,
            "initial distribution should sum to ~1.0, got {sum}"
        );
        assert!(
            dist.working > 0.0,
            "there must be a working (labor) bracket"
        );
        // Rates are valid per-turn fractions.
        for rate in [
            config.maturation_rate,
            config.aging_rate,
            config.elder_mortality_rate,
            config.births.birth_rate,
            config.consumption.per_capita_draw,
        ] {
            assert!(rate >= 0.0, "rates must be non-negative");
        }
        assert!(config.cold.max_mortality <= 1.0);
        assert!(config.scarcity.starvation_mortality >= 0.0);
    }
}
