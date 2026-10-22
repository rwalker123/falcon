//! Tuning for how a band comes to have a leader (`crate::leadership`).
//!
//! Loaded from `data/leadership_config.json` on the shared boot seam ([`crate::config_load`]): an
//! absent default path falls back to the builtin, anything else that is there and wrong is a boot
//! panic. The JSON is the sole source of the numbers: every field is required and unknown keys are
//! rejected, so a fat-fingered lever fails the load rather than running the default. Per-lever
//! rationale lives in `.claude/rules/core_sim/leadership.md`.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_LEADERSHIP_CONFIG: &str = include_str!("data/leadership_config.json");

/// Root leadership configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeadershipConfig {
    /// The practice a band must bank before it accepts a leader.
    pub practice_threshold: f64,
    /// Practice one finished Cultivate, Sow, Tame or Corral build adds (`RungDef::directed_work`).
    pub practice_per_directed_build: f64,
    /// Practice a commanded band split adds to EACH of the two bands.
    pub practice_per_split: f64,
    /// Practice added per hex a resident band steps while following a herd.
    pub practice_per_herd_hex: f64,
    /// Practice added the first time a band reaches each settlement stage above the first.
    pub practice_per_stage_reached: f64,
}

/// The **only** `Default`: it parses the builtin JSON, so `default()` and the shipped tuning are
/// the same numbers by construction.
impl Default for LeadershipConfig {
    fn default() -> Self {
        Self::from_json_str(BUILTIN_LEADERSHIP_CONFIG)
            .expect("builtin leadership config should parse and validate")
    }
}

impl LeadershipConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Parse **and validate** - every load path (builtin, file, override, staged patch) runs the
    /// range checks here.
    pub fn from_json_str(json: &str) -> Result<Self, LeadershipConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate().map_err(LeadershipConfigError::Invalid)?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, LeadershipConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| LeadershipConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    /// Every lever finite and `> 0`: a zero quantum would make a source count for nothing, and a
    /// zero threshold would crown a leader before any work had been done.
    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("practice_threshold", self.practice_threshold),
            (
                "practice_per_directed_build",
                self.practice_per_directed_build,
            ),
            ("practice_per_split", self.practice_per_split),
            ("practice_per_herd_hex", self.practice_per_herd_hex),
            (
                "practice_per_stage_reached",
                self.practice_per_stage_reached,
            ),
        ] {
            if !(value.is_finite() && value > 0.0) {
                return Err(format!("{name} must be finite and > 0, got {value}"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum LeadershipConfigError {
    #[error("failed to read leadership config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse leadership config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid leadership config: {0}")]
    Invalid(String),
}

impl ConfigLoadError for LeadershipConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        match self {
            Self::Read { source, .. } => source.kind() == io::ErrorKind::NotFound,
            Self::Parse(_) | Self::Invalid(_) => false,
        }
    }
}

/// Handle for accessing the leadership configuration.
#[derive(Resource, Debug, Clone)]
pub struct LeadershipConfigHandle(pub Arc<LeadershipConfig>);

impl LeadershipConfigHandle {
    pub fn new(config: Arc<LeadershipConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<LeadershipConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<LeadershipConfig>) {
        self.0 = config;
    }
}

impl Default for LeadershipConfigHandle {
    fn default() -> Self {
        Self(LeadershipConfig::builtin())
    }
}

/// Metadata about the leadership configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct LeadershipConfigMetadata {
    path: Option<PathBuf>,
}

impl LeadershipConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the leadership config from the environment (`LEADERSHIP_CONFIG_PATH`) or the default data
/// path. Only an absent *default* path falls back to the builtin - see
/// [`crate::config_load::resolve_config`].
pub fn load_leadership_config_from_env() -> (Arc<LeadershipConfig>, LeadershipConfigMetadata) {
    let (config, source) = load_config_from_env(
        "LEADERSHIP_CONFIG_PATH",
        "leadership_config",
        "src/data/leadership_config.json",
        LeadershipConfig::builtin,
        LeadershipConfig::from_file,
    );
    (config, LeadershipConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_loads_with_the_shipped_levers() {
        let config = LeadershipConfig::default();
        for lever in [
            config.practice_threshold,
            config.practice_per_directed_build,
            config.practice_per_split,
            config.practice_per_herd_hex,
            config.practice_per_stage_reached,
        ] {
            assert!(lever > 0.0);
        }
    }

    /// The shipped JSON with `key` replaced by `value`.
    fn with(key: &str, value: f64) -> String {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_LEADERSHIP_CONFIG).expect("builtin parses");
        json[key] = serde_json::json!(value);
        json.to_string()
    }

    #[test]
    fn a_zero_or_negative_lever_is_refused() {
        for key in [
            "practice_threshold",
            "practice_per_directed_build",
            "practice_per_split",
            "practice_per_herd_hex",
            "practice_per_stage_reached",
        ] {
            for bad in [0.0, -1.0] {
                assert!(
                    matches!(
                        LeadershipConfig::from_json_str(&with(key, bad)),
                        Err(LeadershipConfigError::Invalid(_))
                    ),
                    "{key} = {bad} must be refused"
                );
            }
        }
    }

    #[test]
    fn an_unknown_or_missing_key_is_refused() {
        assert!(LeadershipConfig::from_json_str(r#"{"practice_threshold": 3.0}"#).is_err());
        let mut json: serde_json::Value = serde_json::from_str(BUILTIN_LEADERSHIP_CONFIG).unwrap();
        json["extra"] = serde_json::json!(1);
        assert!(LeadershipConfig::from_json_str(&json.to_string()).is_err());
    }
}
