//! Tuning for belief on a place — how much each source adds to a tile's stock.
//!
//! Loaded from `data/belief_config.json`, on the shared boot seam (`crate::config_load`): an absent
//! default path falls back to the builtin, anything else that is there and wrong is a boot panic.
//! Mirrors `connections_config.rs`.
//!
//! **The numbers, and what they mean in play** (`docs/plan_civilization_steps.md` §"Belief is a
//! property of a place"):
//!
//! - `belief_per_death` **1.0** — belief added to the tile a band stands on, per person who dies
//!   there. At `1.0` the unit of belief *is* the dead-equivalent: a place reading `12` holds twelve
//!   people's worth of ancestors. Later sources (gatherings, the monument) are priced in that unit.
//!
//! There is deliberately **no decay lever**: belief is monotone (`crate::belief`), so an abandoned
//! place keeps its dead.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_load::{load_config_from_env, ConfigLoadError};
use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_BELIEF_CONFIG: &str = include_str!("data/belief_config.json");

/// Root configuration for belief on a place.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BeliefConfig {
    /// Belief added to the tile a band stands on, per person who dies there. Applied to the turn's
    /// **fractional** death total, not the whole-person events, because belief is a continuous
    /// stock — a band losing a third of a person a turn for three turns has buried one.
    pub belief_per_death: f32,
}

impl Default for BeliefConfig {
    fn default() -> Self {
        Self {
            belief_per_death: 1.0,
        }
    }
}

impl BeliefConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_BELIEF_CONFIG)
                .expect("builtin belief config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, BeliefConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| BeliefConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(BeliefConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum BeliefConfigError {
    #[error("failed to read belief config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse belief config: {0}")]
    Parse(#[from] serde_json::Error),
}

impl ConfigLoadError for BeliefConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the belief configuration.
#[derive(Resource, Debug, Clone)]
pub struct BeliefConfigHandle(pub Arc<BeliefConfig>);

impl BeliefConfigHandle {
    pub fn new(config: Arc<BeliefConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<BeliefConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<BeliefConfig>) {
        self.0 = config;
    }
}

impl Default for BeliefConfigHandle {
    fn default() -> Self {
        Self(BeliefConfig::builtin())
    }
}

/// Metadata about the belief configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct BeliefConfigMetadata {
    path: Option<PathBuf>,
}

impl BeliefConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load belief config from environment (`BELIEF_CONFIG_PATH`) or the default data path. Only an
/// absent *default* path falls back to the builtin; see [`crate::config_load::resolve_config`] for
/// the rule and what it panics on.
pub fn load_belief_config_from_env() -> (Arc<BeliefConfig>, BeliefConfigMetadata) {
    let (config, source) = load_config_from_env(
        "BELIEF_CONFIG_PATH",
        "belief_config",
        "src/data/belief_config.json",
        BeliefConfig::builtin,
        BeliefConfig::from_file,
    );
    (config, BeliefConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_matches_the_default() {
        let builtin = BeliefConfig::builtin();
        assert!(
            builtin.belief_per_death > 0.0,
            "a death that adds no belief would leave the cemetery source switched off"
        );
        assert_eq!(
            builtin.belief_per_death,
            BeliefConfig::default().belief_per_death,
            "the shipped JSON and the serde default must agree, or a partial file reads differently"
        );
    }
}
