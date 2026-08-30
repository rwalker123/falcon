//! Data-driven tuning for the supply network — the per-faction logistics layer that auto-balances
//! each band's local goods store with nearby bands every turn.
//!
//! Loaded from `data/supply_network_config.json`. Bands (and, later, populated tiles / storage
//! pits) within `reach_tiles` hex steps of each other form a connected supply network that redistributes
//! stored goods toward a per-capita balance, moving at most `throughput_per_turn` per node and
//! losing `friction` of each transfer in transit. Early game this is tiny-reach, near-free sharing
//! between neighbors; the same knobs scale to settlements/cities later. Mirrors the
//! `sedentarization_config.rs` / `fauna_config.rs` loader (baked-in builtin + optional file/env
//! override).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_load::{load_config_from_env, ConfigLoadError};
use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_SUPPLY_NETWORK_CONFIG: &str = include_str!("data/supply_network_config.json");

/// Root supply-network configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SupplyNetworkConfig {
    /// **The distance at which a logistics link holds itself for free**, in **hex steps**
    /// ([`crate::grid_utils::hex_distance_wrapped`], the metric every other radius in the sim uses).
    /// Two *connected* nodes (a live tie in [`crate::connections::ConnectionLedger`], either
    /// direction) are linked when within it; beyond it a link needs a route to hold it open, and
    /// goods move by a shipment.
    /// Whether a link's two ends then pool for free is a separate, same-faction policy —
    /// [`crate::supply::balance_supply_networks`].
    pub reach_tiles: u32,
    /// Max quantity of a single commodity a node may send or receive per turn (how *fast* a
    /// network equalizes; reach and the tie decide *who* is linked).
    pub throughput_per_turn: f32,
    /// Fraction of each transfer lost in transit (`0` = frictionless, `1` = nothing arrives).
    pub friction: f32,
    /// Dead-band: transfers smaller than this are skipped so a balanced network doesn't churn.
    pub min_transfer: f32,
}

impl Default for SupplyNetworkConfig {
    fn default() -> Self {
        Self {
            reach_tiles: 3,
            throughput_per_turn: 50.0,
            friction: 0.05,
            min_transfer: 0.5,
        }
    }
}

impl SupplyNetworkConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_SUPPLY_NETWORK_CONFIG)
                .expect("builtin supply network config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, SupplyNetworkConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SupplyNetworkConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(SupplyNetworkConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum SupplyNetworkConfigError {
    #[error("failed to read supply network config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse supply network config: {0}")]
    Parse(#[from] serde_json::Error),
}

impl ConfigLoadError for SupplyNetworkConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the supply-network configuration.
#[derive(Resource, Debug, Clone)]
pub struct SupplyNetworkConfigHandle(pub Arc<SupplyNetworkConfig>);

impl SupplyNetworkConfigHandle {
    pub fn new(config: Arc<SupplyNetworkConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<SupplyNetworkConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<SupplyNetworkConfig>) {
        self.0 = config;
    }
}

impl Default for SupplyNetworkConfigHandle {
    fn default() -> Self {
        Self(SupplyNetworkConfig::builtin())
    }
}

/// Metadata about the supply-network configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct SupplyNetworkConfigMetadata {
    path: Option<PathBuf>,
}

impl SupplyNetworkConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load supply-network config from environment (`SUPPLY_NETWORK_CONFIG_PATH`) or the default data
/// path. Only an absent *default* path falls back to the builtin; see
/// [`crate::config_load::resolve_config`] for the rule and what it panics on.
pub fn load_supply_network_config_from_env(
) -> (Arc<SupplyNetworkConfig>, SupplyNetworkConfigMetadata) {
    let (config, source) = load_config_from_env(
        "SUPPLY_NETWORK_CONFIG_PATH",
        "supply_network_config",
        "src/data/supply_network_config.json",
        SupplyNetworkConfig::builtin,
        SupplyNetworkConfig::from_file,
    );
    (config, SupplyNetworkConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_sane() {
        let config = SupplyNetworkConfig::builtin();
        assert!(
            config.reach_tiles >= 1,
            "reach must connect at least neighbors"
        );
        assert!(config.throughput_per_turn > 0.0);
        assert!(
            (0.0..=1.0).contains(&config.friction),
            "friction is a fraction in [0, 1]"
        );
        assert!(config.min_transfer >= 0.0);
    }
}
