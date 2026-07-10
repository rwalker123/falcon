//! Data-driven labor-allocation tuning (Early-Game Labor, slice 3a).
//!
//! Loaded from `data/labor_config.json`. Drives the source-centric labor pool: the
//! band's work range, the leashed-follow reach for hunting, per-turn band movement,
//! and the flat per-worker throughput tiers for Forage / Hunt / Scout. Mirrors the
//! `fauna_config.rs` loader pattern (baked-in builtin + optional file/env override).
//!
//! No magic numbers: every lever a system reads lives here.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_LABOR_CONFIG: &str = include_str!("data/labor_config.json");

/// Flat per-worker forage throughput tier (TOE multipliers are a later slice).
#[derive(Debug, Clone, Deserialize)]
pub struct ForageLaborConfig {
    /// Provisions produced per assigned forager per turn at `seasonal_weight` 1.0.
    pub per_worker_yield: f32,
}

/// Flat per-worker hunt throughput tier.
#[derive(Debug, Clone, Deserialize)]
pub struct HuntLaborConfig {
    /// Biomass one hunter can take per turn, capped by the policy ceiling (Sustain =
    /// net regrowth, etc.). The biomass→provisions/trade conversion reuses
    /// `fauna_config`'s `hunt.*_per_biomass` so the ecology stays consistent.
    pub per_worker_biomass_capacity: f32,
}

/// Band-wide scout role tuning: staffed scouts extend the band's live sight range
/// (re-marked Active every turn), scaled by head-count and capped. No resource yield.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoutLaborConfig {
    /// Extra sight-range tiles each staffed scout adds to the band's base sight.
    pub sight_bonus_per_scout: u32,
    /// Upper bound on the scout sight bonus regardless of head-count.
    pub max_sight_bonus: u32,
}

impl ScoutLaborConfig {
    /// The sight-range bonus for a band staffing `scouts` workers on the Scout role:
    /// `min(scouts × sight_bonus_per_scout, max_sight_bonus)`. Zero scouts → zero bonus.
    pub fn sight_bonus(&self, scouts: u32) -> u32 {
        scouts
            .saturating_mul(self.sight_bonus_per_scout)
            .min(self.max_sight_bonus)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LaborConfig {
    /// Chebyshev radius of in-range assignable sources around the band's tile.
    pub band_work_range: u32,
    /// Extra distance beyond `band_work_range` a Hunt assignment reaches (leashed
    /// follow) before it lapses and returns its workers to the pool.
    pub hunt_leash_tiles: u32,
    /// Tiles a `move_band` order advances the band toward its target each turn.
    pub band_move_tiles_per_turn: u32,
    pub forage: ForageLaborConfig,
    pub hunt: HuntLaborConfig,
    pub scout: ScoutLaborConfig,
}

impl LaborConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_LABOR_CONFIG).expect("builtin labor config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, LaborConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| LaborConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(LaborConfig::from_json_str(&contents)?)
    }

    /// Distance (inclusive) at which a Hunt assignment still yields before lapsing.
    pub fn hunt_reach(&self) -> u32 {
        self.band_work_range + self.hunt_leash_tiles
    }
}

#[derive(Debug, Error)]
pub enum LaborConfigError {
    #[error("failed to read labor config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse labor config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the labor configuration.
#[derive(Resource, Debug, Clone)]
pub struct LaborConfigHandle(pub Arc<LaborConfig>);

impl LaborConfigHandle {
    pub fn new(config: Arc<LaborConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<LaborConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<LaborConfig>) {
        self.0 = config;
    }
}

impl Default for LaborConfigHandle {
    fn default() -> Self {
        Self(LaborConfig::builtin())
    }
}

/// Metadata about the labor configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct LaborConfigMetadata {
    path: Option<PathBuf>,
}

impl LaborConfigMetadata {
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

/// Load labor configuration from environment (`LABOR_CONFIG_PATH`) or the default data
/// path, falling back to the baked-in builtin.
pub fn load_labor_config_from_env() -> (Arc<LaborConfig>, LaborConfigMetadata) {
    let override_path = env::var("LABOR_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/labor_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match LaborConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "labor_config.loaded=file"
                );
                return (Arc::new(config), LaborConfigMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "labor_config.load_failed"
                );
            }
        }
    }

    let config = LaborConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "labor_config.loaded=builtin"
    );
    (config, LaborConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses() {
        let config = LaborConfig::builtin();
        assert!(config.band_work_range >= 1);
        assert!(config.hunt_leash_tiles >= 1);
        assert!(config.band_move_tiles_per_turn >= 1);
        assert!(config.forage.per_worker_yield > 0.0);
        assert!(config.hunt.per_worker_biomass_capacity > 0.0);
        assert!(config.scout.sight_bonus_per_scout >= 1);
        assert!(config.scout.max_sight_bonus >= config.scout.sight_bonus_per_scout);
        assert_eq!(
            config.hunt_reach(),
            config.band_work_range + config.hunt_leash_tiles
        );
    }

    #[test]
    fn scout_sight_bonus_scales_with_headcount_and_caps() {
        // The effective band sight range is `base_range + sight_bonus(scouts)`, where the
        // bonus scales linearly per scout and clamps at `max_sight_bonus`.
        let scout = ScoutLaborConfig {
            sight_bonus_per_scout: 1,
            max_sight_bonus: 4,
        };
        const BASE_RANGE: u32 = 6; // visibility_config BandScout base_range.

        // 0 scouts → no bonus → sight is exactly the base range.
        assert_eq!(scout.sight_bonus(0), 0);
        assert_eq!(BASE_RANGE + scout.sight_bonus(0), BASE_RANGE);

        // N scouts below the cap → base_range + N × per-scout.
        assert_eq!(scout.sight_bonus(3), 3);
        assert_eq!(BASE_RANGE + scout.sight_bonus(3), BASE_RANGE + 3);

        // Above the cap → clamped to max_sight_bonus (never grows past it).
        assert_eq!(scout.sight_bonus(4), 4);
        assert_eq!(scout.sight_bonus(99), 4);
        assert_eq!(BASE_RANGE + scout.sight_bonus(99), BASE_RANGE + 4);
    }
}
