//! Data-driven tuning for the Sedentarization Score.
//!
//! Loaded from `data/sedentarization_config.json`. The score is a per-faction 0–100
//! "pressure to root in place" (see `sedentarization.rs`), a weighted blend of normalized
//! inputs (domestication, surplus, resource density, population) crossing a `soft_threshold`
//! (~40, "establish a seasonal base?") and a `hard_threshold` (~70, "settle?"). Mirrors the
//! `fauna_config.rs` loader (baked-in builtin + optional file/env override).

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_SEDENTARIZATION_CONFIG: &str = include_str!("data/sedentarization_config.json");

/// Relative weights of the score inputs. Normalized inputs (each in `[0, 1]`) are blended by
/// these weights (which should sum to ~1.0) and scaled to `[0, 100]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SedentarizationWeights {
    pub domestication: f32,
    pub surplus: f32,
    pub resource_density: f32,
    pub population: f32,
}

impl Default for SedentarizationWeights {
    fn default() -> Self {
        Self {
            domestication: 0.35,
            surplus: 0.30,
            resource_density: 0.20,
            population: 0.15,
        }
    }
}

/// The input level at which each contribution saturates (normalized value reaches 1.0).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SedentarizationReferences {
    /// Domesticated groups owned by the faction that fully satisfy the domestication input.
    pub domesticated_herds: u32,
    /// Provisions stockpile that fully satisfies the surplus input.
    pub surplus: f32,
    /// Total population that fully satisfies the population input.
    pub population: f32,
}

impl Default for SedentarizationReferences {
    fn default() -> Self {
        Self {
            domesticated_herds: 3,
            surplus: 300.0,
            population: 300.0,
        }
    }
}

/// Root sedentarization configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SedentarizationConfig {
    /// Soft prompt ("establish a seasonal base?").
    pub soft_threshold: f32,
    /// Hard prompt ("invest in storehouses and settle?").
    pub hard_threshold: f32,
    /// EMA factor applied to the previous score each turn (`0` = instant, `→1` = very slow).
    pub smoothing: f32,
    pub weights: SedentarizationWeights,
    pub references: SedentarizationReferences,
}

impl Default for SedentarizationConfig {
    fn default() -> Self {
        Self {
            soft_threshold: 40.0,
            hard_threshold: 70.0,
            smoothing: 0.5,
            weights: SedentarizationWeights::default(),
            references: SedentarizationReferences::default(),
        }
    }
}

impl SedentarizationConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_SEDENTARIZATION_CONFIG)
                .expect("builtin sedentarization config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, SedentarizationConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SedentarizationConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(SedentarizationConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum SedentarizationConfigError {
    #[error("failed to read sedentarization config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse sedentarization config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the sedentarization configuration.
#[derive(Resource, Debug, Clone)]
pub struct SedentarizationConfigHandle(pub Arc<SedentarizationConfig>);

impl SedentarizationConfigHandle {
    pub fn new(config: Arc<SedentarizationConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<SedentarizationConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<SedentarizationConfig>) {
        self.0 = config;
    }
}

impl Default for SedentarizationConfigHandle {
    fn default() -> Self {
        Self(SedentarizationConfig::builtin())
    }
}

/// Metadata about the sedentarization configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct SedentarizationConfigMetadata {
    path: Option<PathBuf>,
}

impl SedentarizationConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load sedentarization config from environment (`SEDENTARIZATION_CONFIG_PATH`) or the default
/// data path, falling back to the baked-in builtin.
pub fn load_sedentarization_config_from_env(
) -> (Arc<SedentarizationConfig>, SedentarizationConfigMetadata) {
    let override_path = env::var("SEDENTARIZATION_CONFIG_PATH")
        .ok()
        .map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/sedentarization_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match SedentarizationConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "sedentarization_config.loaded=file"
                );
                return (
                    Arc::new(config),
                    SedentarizationConfigMetadata::new(Some(path)),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "sedentarization_config.load_failed"
                );
            }
        }
    }

    let config = SedentarizationConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "sedentarization_config.loaded=builtin"
    );
    (config, SedentarizationConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_ordered() {
        let config = SedentarizationConfig::builtin();
        // Thresholds are ordered within (0, 100).
        assert!(config.soft_threshold > 0.0);
        assert!(config.soft_threshold < config.hard_threshold);
        assert!(config.hard_threshold < 100.0);
        // Smoothing is a valid EMA factor.
        assert!(config.smoothing >= 0.0 && config.smoothing < 1.0);
        // Weights blend to ~1.0 so the score scales cleanly to [0, 100].
        let w = &config.weights;
        let sum = w.domestication + w.surplus + w.resource_density + w.population;
        assert!(
            (sum - 1.0).abs() < 1e-3,
            "weights should sum to ~1.0, got {sum}"
        );
        // References are positive.
        assert!(config.references.domesticated_herds >= 1);
        assert!(config.references.surplus > 0.0);
        assert!(config.references.population > 0.0);
    }
}
