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
    /// **The free pooling reach of a people that knows `messengers`** (`docs/plan_far_kin.md` §3),
    /// in hex steps - the VALUE the stat takes once the people knows the lesson, not a multiplier.
    /// Read ONLY by [`crate::supply::balance_supply_networks`]' link test for a pair of that people
    /// (links are same-people gated already). [`crate::supply::WalkReach`], the work party's local
    /// range and the road bonus (`free_pooling_reach_tiles - reach_tiles`) all keep `reach_tiles`,
    /// so learning messengers never shrinks what a road does for a walker. Validated
    /// `reach_tiles <= this` ([`SupplyNetworkConfig::validate`]) and `this <` the trail rung's
    /// `holds_link_to_tiles` ([`SupplyNetworkConfig::validate_against_trail`], checked at boot where
    /// the ladder is in scope): at or past a trail's reach messengers would link every far pair for
    /// free and the founding-walk trail would add nothing.
    pub reach_tiles_with_messengers: u32,
    /// Max quantity of a single commodity a node may send or receive per turn (how *fast* a
    /// network equalizes; reach and the tie decide *who* is linked).
    pub throughput_per_turn: f32,
    /// Fraction of each transfer lost in transit (`0` = frictionless, `1` = nothing arrives).
    pub friction: f32,
    /// ⛔ **THE DEAD-BAND, AS A FRACTION OF THE NODE'S OWN PER-CAPITA FAIR SHARE** — never an
    /// absolute quantity, and never a fraction of what the *network* holds. A node whose move is
    /// smaller than `this × its fair share` is skipped so a balanced network doesn't churn.
    ///
    /// **It is relative because one balancer serves commodities three orders of magnitude apart.**
    /// The lever was an absolute `0.5` from the food-only original, and materials then inherited it
    /// unexamined: food is held in the hundreds, where `0.5` is the noise floor it was chosen to be,
    /// while a material is held in single units **and pools per `(material, rating)`** — bone alone
    /// lands in eight distinct rating piles across the shipped fauna roster. On a pile of `0.75` a
    /// `0.5` floor is not a dead-band, it is a wall: the balancer computed the move and dropped it,
    /// every turn, silently. A neighbour's bone could not reach a band that had none.
    ///
    /// **The fair share is the denominator because it is what the move is measured against.** A
    /// node ships or asks for the gap between its store and its per-capita share, so scaling the
    /// threshold to `Σ stores` would let it grow with member count while the gap it gates does not
    /// — twelve camps at the stock two camps hold per head would have to clear a bar six times
    /// higher for the same shortfall. Against the fair share the lever is **member-count free by
    /// construction**: twelve equal camps and two equal camps at the same per-capita stock resolve
    /// the identical dead-band.
    ///
    /// **`0.0025` is chosen to leave food where it is**, not as a fresh tuning judgement: it
    /// reproduces the retired absolute `0.5` exactly on the case that floor was calibrated against
    /// — two camps holding `400` food between them have a fair share of `200`, and
    /// `200 × 0.0025 = 0.5`. On a sub-unit pile it follows the pile down: a `0.75` bone stock split
    /// two ways is a `0.375` fair share and a `0.00094` threshold, some **400× smaller than the
    /// `0.375` move it would have to block**, so it cannot wall one — and it rises again with the
    /// pile, which is the point.
    pub min_transfer_fraction: f32,
}

impl Default for SupplyNetworkConfig {
    fn default() -> Self {
        Self {
            reach_tiles: 3,
            reach_tiles_with_messengers: 4,
            throughput_per_turn: 50.0,
            friction: 0.05,
            min_transfer_fraction: 0.0025,
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
        let config = SupplyNetworkConfig::from_json_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    /// **The free pooling reach for a pair of a people that does (or does not) know `messengers`** -
    /// the value the stat takes, per the field docs above.
    pub fn pooling_reach(&self, knows_messengers: bool) -> u32 {
        if knows_messengers {
            self.reach_tiles_with_messengers
        } else {
            self.reach_tiles
        }
    }

    /// Messengers only ever reaches further: `reach_tiles <= reach_tiles_with_messengers`.
    pub fn validate(&self) -> Result<(), SupplyNetworkConfigError> {
        if self.reach_tiles_with_messengers < self.reach_tiles {
            return Err(SupplyNetworkConfigError::Invalid(format!(
                "reach_tiles_with_messengers ({}) must be >= reach_tiles ({}): a lesson that \
                 shrank the free reach would be a penalty",
                self.reach_tiles_with_messengers, self.reach_tiles
            )));
        }
        Ok(())
    }

    /// **Messengers' reach stays below a trail's**: `reach_tiles_with_messengers <
    /// trail_reach` (the free floor's top rung's `holds_link_to_tiles`,
    /// [`crate::routes::trail_reach_tiles`]). The cross-config half, run at boot where both configs
    /// are in scope (`lib.rs`) - a supply config holds no ladder.
    pub fn validate_against_trail(&self, trail_reach: u32) -> Result<(), SupplyNetworkConfigError> {
        if self.reach_tiles_with_messengers >= trail_reach {
            return Err(SupplyNetworkConfigError::Invalid(format!(
                "reach_tiles_with_messengers ({}) must be < the trail's holds_link_to_tiles ({}): \
                 otherwise messengers links every far pair for free and the founding-walk trail \
                 adds nothing",
                self.reach_tiles_with_messengers, trail_reach
            )));
        }
        Ok(())
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
    #[error("invalid supply network config: {0}")]
    Invalid(String),
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
        config.validate().expect("the shipped reaches are ordered");
        assert_eq!(config.pooling_reach(false), config.reach_tiles);
        assert_eq!(
            config.pooling_reach(true),
            config.reach_tiles_with_messengers
        );
        assert!(
            (0.0..=1.0).contains(&config.friction),
            "friction is a fraction in [0, 1]"
        );
        assert!(
            (0.0..=1.0).contains(&config.min_transfer_fraction),
            "the dead-band is a fraction of the node's own fair share, not a quantity"
        );
    }

    #[test]
    fn messengers_reach_may_not_be_below_the_free_reach() {
        let mut config = SupplyNetworkConfig::default();
        config.reach_tiles_with_messengers = config.reach_tiles - 1;
        assert!(config.validate().is_err());
        config.reach_tiles_with_messengers = config.reach_tiles;
        assert!(config.validate().is_ok(), "equal is allowed: it is a no-op");
    }

    /// The two shipped files are never loaded together by a single loader, so the cross-file rule is
    /// pinned on the shipped data (and enforced at boot in `lib.rs`).
    #[test]
    fn the_shipped_messengers_reach_is_below_the_shipped_trails() {
        let config = SupplyNetworkConfig::builtin();
        let ladder = crate::intensification::LadderConfig::builtin();
        let trail = crate::routes::trail_reach_tiles(&ladder);
        config
            .validate_against_trail(trail)
            .expect("messengers reach stays below the trail's");
        assert!(config.reach_tiles_with_messengers < trail);
        assert!(config
            .validate_against_trail(config.reach_tiles_with_messengers)
            .is_err());
    }
}
