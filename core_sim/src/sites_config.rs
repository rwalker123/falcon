//! Data-driven Wondrous Sites catalog + placement rules.
//!
//! Loaded from `data/sites_config.json`. A **site** is a notable map feature a tile can
//! hold (a peak, a fertile basin, later riches/ruins/tribes), hidden under fog until a
//! faction's vision reveals it. The catalog is a table: each entry carries a category,
//! display name, glyph, the `placement_rule` that decides where worldgen stamps it, and a
//! per-category `discovery_reward`. `placement` holds the tuning for each named rule.
//! Mirrors the `fauna_config.rs` loader pattern (baked-in builtin + optional file/env
//! override) so adding "Salt Flats" or "Ancient Ruin" is a new JSON row, no code.

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_SITES_CONFIG: &str = include_str!("data/sites_config.json");

/// Per-category discovery payoff. v1 has a single lever (a one-shot morale bonus applied to
/// the discovering faction's bands); it is a struct rather than a bare field so future
/// per-category rewards (resource flags, diplomacy contacts, culture/naming) slot in without
/// touching the discovery system's call site.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct DiscoveryReward {
    /// One-shot morale bonus (0..1 scale) granted to each of the discovering faction's bands.
    pub morale_bonus: f32,
}

/// One site row in the catalog. `placement_rule` keys into [`SitesConfig::placement`].
#[derive(Debug, Clone, Deserialize)]
pub struct SiteDef {
    /// Coarse category (`landmark`, `settle_site`, later `riches`, `tribe`); drives the
    /// reward semantics and the client marker style. Free-form string — new categories need
    /// no schema change.
    pub category: String,
    /// Player-facing name; also the snapshot `display_name`.
    pub display_name: String,
    /// Client marker glyph.
    pub glyph: String,
    /// Key into [`SitesConfig::placement`] selecting the worldgen placement rule.
    pub placement_rule: String,
    #[serde(default)]
    pub discovery_reward: DiscoveryReward,
}

/// Tuning for one named placement rule. The fields are the **union** of what the v1 rules
/// need; a rule only reads the ones that apply to it (`Option` = "this rule ignores it").
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PlacementRuleCfg {
    /// Hard cap on how many sites of a rule are stamped map-wide.
    pub max_sites: u32,
    /// Minimum Chebyshev spacing between two sites of the same rule (anti-cluster).
    pub min_spacing: u32,
    /// `prominent_mountain`: minimum tile relief to qualify.
    pub min_relief: Option<f32>,
    /// `fertile_settle`: maximum `tile_morale_pressure` total (habitability) to qualify.
    pub max_habitability_pressure: Option<f32>,
    /// `fertile_settle`: minimum food-module `seasonal_weight` to qualify.
    pub min_food_weight: Option<f32>,
}

/// Root sites configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct SitesConfig {
    /// Site catalog keyed by `site_id`.
    pub catalog: HashMap<String, SiteDef>,
    /// Placement rules keyed by `placement_rule`.
    pub placement: HashMap<String, PlacementRuleCfg>,
}

impl SitesConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_SITES_CONFIG).expect("builtin sites config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, SitesConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| SitesConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(SitesConfig::from_json_str(&contents)?)
    }

    /// Resolve the [`SiteDef`] for a `site_id`, if present.
    pub fn site(&self, site_id: &str) -> Option<&SiteDef> {
        self.catalog.get(site_id)
    }

    /// `(site_id, def, rule)` triples for every catalog entry whose `placement_rule` resolves,
    /// in stable `site_id` order so worldgen placement is deterministic.
    pub fn placeable_sites(&self) -> Vec<(&String, &SiteDef, &PlacementRuleCfg)> {
        let mut out: Vec<_> = self
            .catalog
            .iter()
            .filter_map(|(id, def)| {
                self.placement
                    .get(&def.placement_rule)
                    .map(|rule| (id, def, rule))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

#[derive(Debug, Error)]
pub enum SitesConfigError {
    #[error("failed to read sites config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse sites config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the sites configuration.
#[derive(Resource, Debug, Clone)]
pub struct SitesConfigHandle(pub Arc<SitesConfig>);

impl SitesConfigHandle {
    pub fn new(config: Arc<SitesConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<SitesConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<SitesConfig>) {
        self.0 = config;
    }
}

impl Default for SitesConfigHandle {
    fn default() -> Self {
        Self(SitesConfig::builtin())
    }
}

/// Metadata about the sites configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct SitesConfigMetadata {
    path: Option<PathBuf>,
}

impl SitesConfigMetadata {
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

/// Load sites configuration from environment (`SITES_CONFIG_PATH`) or the default data path,
/// falling back to the baked-in builtin.
pub fn load_sites_config_from_env() -> (Arc<SitesConfig>, SitesConfigMetadata) {
    let override_path = env::var("SITES_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/sites_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match SitesConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "sites_config.loaded=file"
                );
                return (Arc::new(config), SitesConfigMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "sites_config.load_failed"
                );
            }
        }
    }

    let config = SitesConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "sites_config.loaded=builtin"
    );
    (config, SitesConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses() {
        let config = SitesConfig::builtin();
        assert!(config.catalog.contains_key("great_peak"));
        assert!(config.catalog.contains_key("verdant_basin"));
        assert_eq!(config.catalog["great_peak"].category, "landmark");
        assert_eq!(config.catalog["verdant_basin"].category, "settle_site");
        // Every catalog entry's placement_rule resolves.
        for def in config.catalog.values() {
            assert!(config.placement.contains_key(&def.placement_rule));
        }
    }

    #[test]
    fn placement_rules_present() {
        let config = SitesConfig::builtin();
        let mountain = &config.placement["prominent_mountain"];
        assert_eq!(mountain.max_sites, 5);
        assert!(mountain.min_relief.unwrap() > 0.0);
        let fertile = &config.placement["fertile_settle"];
        assert!(fertile.max_habitability_pressure.unwrap() > 0.0);
        assert!(fertile.min_food_weight.unwrap() > 0.0);
    }

    #[test]
    fn placeable_sites_sorted_and_resolved() {
        let config = SitesConfig::builtin();
        let placeable = config.placeable_sites();
        assert_eq!(placeable.len(), 2);
        // Stable site_id order.
        assert_eq!(placeable[0].0, "great_peak");
        assert_eq!(placeable[1].0, "verdant_basin");
    }

    #[test]
    fn discovery_reward_defaults() {
        let config = SitesConfig::builtin();
        assert!(config.catalog["great_peak"].discovery_reward.morale_bonus > 0.0);
    }
}
