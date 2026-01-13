//! Configuration for the Fog of War visibility system.
//!
//! Loaded from `visibility_config.json` with support for environment variable overrides.

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_VISIBILITY_CONFIG: &str = include_str!("data/visibility_config.json");

/// Root configuration for the visibility system.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VisibilityConfig {
    pub decay: DecayConfig,
    pub sight_ranges: HashMap<String, SightRangeConfig>,
    pub elevation: ElevationConfig,
    pub line_of_sight: LineOfSightConfig,
    pub terrain_modifiers: TerrainModifierConfig,
}

impl Default for VisibilityConfig {
    fn default() -> Self {
        Self {
            decay: DecayConfig::default(),
            sight_ranges: default_sight_ranges(),
            elevation: ElevationConfig::default(),
            line_of_sight: LineOfSightConfig::default(),
            terrain_modifiers: TerrainModifierConfig::default(),
        }
    }
}

fn default_sight_ranges() -> HashMap<String, SightRangeConfig> {
    let mut ranges = HashMap::new();
    ranges.insert(
        "BandScout".to_string(),
        SightRangeConfig {
            base_range: 6,
            elevation_bonus_factor: 1.5,
        },
    );
    ranges.insert(
        "BandHunter".to_string(),
        SightRangeConfig {
            base_range: 4,
            elevation_bonus_factor: 1.2,
        },
    );
    ranges.insert(
        "BandGuardian".to_string(),
        SightRangeConfig {
            base_range: 3,
            elevation_bonus_factor: 1.0,
        },
    );
    ranges.insert(
        "BandCrafter".to_string(),
        SightRangeConfig {
            base_range: 2,
            elevation_bonus_factor: 0.8,
        },
    );
    ranges.insert(
        "TownCenter".to_string(),
        SightRangeConfig {
            base_range: 5,
            elevation_bonus_factor: 1.0,
        },
    );
    ranges.insert(
        "Camp".to_string(),
        SightRangeConfig {
            base_range: 3,
            elevation_bonus_factor: 0.5,
        },
    );
    ranges
}

impl VisibilityConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_VISIBILITY_CONFIG)
                .expect("builtin visibility config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, VisibilityConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| VisibilityConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let config = VisibilityConfig::from_json_str(&contents)?;
        Ok(config)
    }

    /// Get sight range config for a unit type, with fallback to default.
    pub fn sight_range_for(&self, unit_kind: &str) -> SightRangeConfig {
        self.sight_ranges
            .get(unit_kind)
            .cloned()
            .unwrap_or(SightRangeConfig::default())
    }

    /// Get the default sight range for unknown unit types.
    pub fn default_sight_range(&self) -> u32 {
        3
    }
}

/// Configuration for visibility decay.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DecayConfig {
    pub enabled: bool,
    pub threshold_turns: u64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_turns: 12,
        }
    }
}

/// Sight range configuration for a unit type.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SightRangeConfig {
    pub base_range: u32,
    pub elevation_bonus_factor: f32,
}

impl Default for SightRangeConfig {
    fn default() -> Self {
        Self {
            base_range: 3,
            elevation_bonus_factor: 1.0,
        }
    }
}

/// Configuration for elevation-based sight bonuses.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ElevationConfig {
    pub enabled: bool,
    pub bonus_per_100m: u32,
    pub max_bonus: u32,
}

impl Default for ElevationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bonus_per_100m: 1,
            max_bonus: 4,
        }
    }
}

/// Configuration for line-of-sight blocking.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LineOfSightConfig {
    pub enabled: bool,
    pub blocking_terrain_tags: Vec<String>,
}

impl Default for LineOfSightConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            blocking_terrain_tags: vec!["HIGHLAND".to_string(), "VOLCANIC".to_string()],
        }
    }
}

/// Terrain-based sight modifiers.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TerrainModifierConfig {
    pub forest_penalty: i32,
    pub water_bonus: i32,
}

impl Default for TerrainModifierConfig {
    fn default() -> Self {
        Self {
            forest_penalty: -2,
            water_bonus: 1,
        }
    }
}

#[derive(Debug, Error)]
pub enum VisibilityConfigError {
    #[error("failed to parse visibility config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("failed to read visibility config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Handle for accessing the visibility configuration.
#[derive(Resource, Debug, Clone)]
pub struct VisibilityConfigHandle(pub Arc<VisibilityConfig>);

impl VisibilityConfigHandle {
    pub fn new(config: Arc<VisibilityConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<VisibilityConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<VisibilityConfig>) {
        self.0 = config;
    }
}

/// Metadata about the visibility configuration source.
#[derive(Resource, Debug, Clone)]
pub struct VisibilityConfigMetadata {
    path: Option<PathBuf>,
}

impl VisibilityConfigMetadata {
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

/// Load visibility configuration from environment or default path.
pub fn load_visibility_config_from_env() -> (Arc<VisibilityConfig>, VisibilityConfigMetadata) {
    let override_path = env::var("VISIBILITY_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/visibility_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match VisibilityConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "visibility_config.loaded=file"
                );
                return (Arc::new(config), VisibilityConfigMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "visibility_config.load_failed"
                );
            }
        }
    }

    let config = VisibilityConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "visibility_config.loaded=builtin"
    );
    (config, VisibilityConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let config = VisibilityConfig::default();
        assert!(config.decay.enabled);
        assert_eq!(config.decay.threshold_turns, 12);
        assert!(config.sight_ranges.contains_key("BandScout"));
    }

    #[test]
    fn builtin_config_parses() {
        let _config = VisibilityConfig::builtin();
    }

    #[test]
    fn sight_range_lookup() {
        let config = VisibilityConfig::default();
        let scout = config.sight_range_for("BandScout");
        assert_eq!(scout.base_range, 6);
        assert_eq!(scout.elevation_bonus_factor, 1.5);

        let unknown = config.sight_range_for("UnknownUnit");
        assert_eq!(unknown.base_range, 3); // default
    }
}
