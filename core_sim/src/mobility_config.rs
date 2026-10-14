//! Data-driven tuning for the Mobility Score.
//!
//! Loaded from `data/mobility_config.json`. The score is a per-faction 0-100 readout of how
//! roaming a people is (see `mobility.rs`) - the counterpart of the Sedentarization Score - a
//! weighted blend of two normalized inputs, the roaming skills a people has learned and the
//! distance its bands walked beside a migrating herd this turn.
//!
//! **The JSON is the sole source of mobility tuning.** Like `demographics_config.rs` there is no
//! hand-written `Default`: every field is required, unknown keys are rejected, and
//! `MobilityConfig::default()` parses [`BUILTIN_MOBILITY_CONFIG`]. The loader is strict to match,
//! through the shared [`crate::config_load`] seam. Per-lever rationale lives in
//! `.claude/rules/core_sim/campaign.md` -> "Mobility" (strict JSON carries no comment keys).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_MOBILITY_CONFIG: &str = include_str!("data/mobility_config.json");

/// Relative weights of the two score inputs. Each normalized input is in `[0, 1]`; the blend is
/// scaled to `[0, 100]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobilityWeights {
    /// The roaming skills (herd lore, travois, drives): the mean of their ledger progress.
    pub skills: f32,
    /// The distance bands walked beside a migrating herd this turn.
    pub distance: f32,
}

/// The input level at which a contribution saturates (normalized value reaches 1.0).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobilityReferences {
    /// Hexes one band must step in a turn, following a herd on a `Migrate` leg, to fully satisfy
    /// the distance input.
    pub hexes_per_turn: f32,
}

/// Root mobility configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobilityConfig {
    /// EMA factor applied to the previous score each turn (`0` = instant, `->1` = very slow).
    pub smoothing: f32,
    pub weights: MobilityWeights,
    pub references: MobilityReferences,
}

/// The **only** `Default`: it parses the builtin JSON, so `default()` and the shipped tuning are
/// the same numbers by construction.
impl Default for MobilityConfig {
    fn default() -> Self {
        Self::from_json_str(BUILTIN_MOBILITY_CONFIG)
            .expect("builtin mobility config should parse and validate")
    }
}

impl MobilityConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Parse **and validate** - every load path (builtin, file, override, staged patch) runs the
    /// range checks here.
    pub fn from_json_str(json: &str) -> Result<Self, MobilityConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate().map_err(MobilityConfigError::Invalid)?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, MobilityConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| MobilityConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    /// Weights finite and non-negative, `smoothing` in `[0, 1)` (a `1` would freeze the score
    /// forever), the distance reference finite and positive (it is a divisor).
    fn validate(&self) -> Result<(), String> {
        for (name, weight) in [
            ("weights.skills", self.weights.skills),
            ("weights.distance", self.weights.distance),
        ] {
            if !(weight.is_finite() && weight >= 0.0) {
                return Err(format!("{name} must be finite and >= 0, got {weight}"));
            }
        }
        if !(self.smoothing.is_finite() && (0.0..1.0).contains(&self.smoothing)) {
            return Err(format!(
                "smoothing must be finite and in [0, 1), got {}",
                self.smoothing
            ));
        }
        let reference = self.references.hexes_per_turn;
        if !(reference.is_finite() && reference > 0.0) {
            return Err(format!(
                "references.hexes_per_turn must be finite and > 0, got {reference}"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum MobilityConfigError {
    #[error("failed to read mobility config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse mobility config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid mobility config: {0}")]
    Invalid(String),
}

impl ConfigLoadError for MobilityConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        match self {
            Self::Read { source, .. } => source.kind() == io::ErrorKind::NotFound,
            Self::Parse(_) | Self::Invalid(_) => false,
        }
    }
}

/// Handle for accessing the mobility configuration.
#[derive(Resource, Debug, Clone)]
pub struct MobilityConfigHandle(pub Arc<MobilityConfig>);

impl MobilityConfigHandle {
    pub fn new(config: Arc<MobilityConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<MobilityConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<MobilityConfig>) {
        self.0 = config;
    }
}

impl Default for MobilityConfigHandle {
    fn default() -> Self {
        Self(MobilityConfig::builtin())
    }
}

/// Metadata about the mobility configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct MobilityConfigMetadata {
    path: Option<PathBuf>,
}

impl MobilityConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load mobility config from environment (`MOBILITY_CONFIG_PATH`) or the default data path. Only
/// an absent *default* path falls back to the builtin - see [`crate::config_load::resolve_config`].
pub fn load_mobility_config_from_env() -> (Arc<MobilityConfig>, MobilityConfigMetadata) {
    let (config, source) = load_config_from_env(
        "MOBILITY_CONFIG_PATH",
        "mobility_config",
        "src/data/mobility_config.json",
        MobilityConfig::builtin,
        MobilityConfig::from_file,
    );
    (config, MobilityConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_patch(patch: &str) -> Result<MobilityConfig, MobilityConfigError> {
        let mut value: serde_json::Value = serde_json::from_str(BUILTIN_MOBILITY_CONFIG).unwrap();
        let patch: serde_json::Value = serde_json::from_str(patch).unwrap();
        for (key, entry) in patch.as_object().unwrap() {
            value[key] = entry.clone();
        }
        MobilityConfig::from_json_str(&value.to_string())
    }

    #[test]
    fn builtin_config_parses_and_is_coherent() {
        let config = MobilityConfig::builtin();
        let w = &config.weights;
        assert!(
            (w.skills + w.distance - 1.0).abs() < 1e-3,
            "weights should sum to ~1.0"
        );
        assert!(config.smoothing >= 0.0 && config.smoothing < 1.0);
        assert!(config.references.hexes_per_turn > 0.0);
    }

    #[test]
    fn the_distance_reference_matches_the_shipped_walking_pace() {
        let labor = crate::labor_config::LaborConfig::builtin();
        assert_eq!(
            MobilityConfig::builtin().references.hexes_per_turn,
            labor.band_move_tiles_per_turn as f32,
            "a band walking every turn at full pace saturates the distance input"
        );
    }

    #[test]
    fn unusable_levers_are_refused() {
        assert!(with_patch(r#"{"smoothing":1.0}"#).is_err());
        assert!(with_patch(r#"{"smoothing":-0.1}"#).is_err());
        assert!(with_patch(r#"{"weights":{"skills":-1.0,"distance":0.5}}"#).is_err());
        assert!(with_patch(r#"{"references":{"hexes_per_turn":0.0}}"#).is_err());
        assert!(with_patch(r#"{"surprise":1}"#).is_err());
    }
}
