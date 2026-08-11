//! Tuning for the connection primitive — the three clocks a directed tie runs on.
//!
//! Loaded from `data/connections_config.json`, on the shared boot seam
//! (`crate::config_load`): an absent default path falls back to the builtin, anything else that is
//! there and wrong is a boot panic. Mirrors `visibility_config.rs`, the config this one sits
//! closest to — contact is found inside the visibility sweep.
//!
//! **The numbers, and what they mean in play** (`docs/plan_contact_and_logistics.md` §Q6):
//!
//! - `strength.gain_per_contact` **0.25** — four turns of contact reaches a full tie. Meeting
//!   someone is nearly irreversible, so the gain is linear and much larger than the drain.
//! - `strength.decay_per_turn` **0.02** — a full tie bleeds to nothing over 50 turns without
//!   contact. This is the clock that gates gameplay, and the one to tune first.
//! - `forget_turns` **200** — the fact of them. 200 turns after the last contact the edge is gone
//!   and the people are simply forgotten.
//!
//! **The `1.0` ceiling is not here, on purpose.** Strength is a fraction of a full tie by
//! definition, so its ceiling is what the number *means*, not how fast it moves — it is the named
//! constant [`crate::connections::FULL_TIE`].

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_load::{load_config_from_env, ConfigLoadError};
use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_CONNECTIONS_CONFIG: &str = include_str!("data/connections_config.json");

/// Root configuration for the connection primitive.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ConnectionsConfig {
    pub strength: ConnectionStrengthConfig,
    /// Turns after the **last contact** at which the edge is removed outright — clock 3, "the fact
    /// of them". Deliberately far longer than the drain to zero: a parked edge at strength 0 means
    /// *"we know such a people exist and have no current tie"*, which is a different reading from
    /// having forgotten them, and the gap between the two clocks is what keeps it one.
    pub forget_turns: u64,
}

/// Clock 2 — how a tie is raised, and how it drains.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ConnectionStrengthConfig {
    /// Added to a tie on every turn its subject is seen. Linear, and much larger than the drain:
    /// fast to gain, slow to lose.
    pub gain_per_contact: f32,
    /// Subtracted on every turn the subject is **not** seen, down to zero. Zero parks the edge; it
    /// does not delete it (that is `forget_turns` above).
    pub decay_per_turn: f32,
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        Self {
            strength: ConnectionStrengthConfig::default(),
            forget_turns: 200,
        }
    }
}

impl Default for ConnectionStrengthConfig {
    fn default() -> Self {
        Self {
            gain_per_contact: 0.25,
            decay_per_turn: 0.02,
        }
    }
}

impl ConnectionsConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_CONNECTIONS_CONFIG)
                .expect("builtin connections config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, ConnectionsConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ConnectionsConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(ConnectionsConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum ConnectionsConfigError {
    #[error("failed to read connections config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse connections config: {0}")]
    Parse(#[from] serde_json::Error),
}

impl ConfigLoadError for ConnectionsConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the connections configuration.
#[derive(Resource, Debug, Clone)]
pub struct ConnectionsConfigHandle(pub Arc<ConnectionsConfig>);

impl ConnectionsConfigHandle {
    pub fn new(config: Arc<ConnectionsConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<ConnectionsConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<ConnectionsConfig>) {
        self.0 = config;
    }
}

impl Default for ConnectionsConfigHandle {
    fn default() -> Self {
        Self(ConnectionsConfig::builtin())
    }
}

/// Metadata about the connections configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct ConnectionsConfigMetadata {
    path: Option<PathBuf>,
}

impl ConnectionsConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load connections config from environment (`CONNECTIONS_CONFIG_PATH`) or the default data path.
/// Only an absent *default* path falls back to the builtin; see
/// [`crate::config_load::resolve_config`] for the rule and what it panics on.
pub fn load_connections_config_from_env() -> (Arc<ConnectionsConfig>, ConnectionsConfigMetadata) {
    let (config, source) = load_config_from_env(
        "CONNECTIONS_CONFIG_PATH",
        "connections_config",
        "src/data/connections_config.json",
        ConnectionsConfig::builtin,
        ConnectionsConfig::from_file,
    );
    (config, ConnectionsConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_sane() {
        let config = ConnectionsConfig::builtin();
        assert!(
            config.strength.gain_per_contact > config.strength.decay_per_turn,
            "a tie must be faster to gain than to lose, or contact never outruns the drain"
        );
        assert!((0.0..=1.0).contains(&config.strength.gain_per_contact));
        assert!((0.0..=1.0).contains(&config.strength.decay_per_turn));
        assert!(
            config.forget_turns > 0,
            "forgetting on the turn of contact would delete every edge as it formed"
        );
    }
}
