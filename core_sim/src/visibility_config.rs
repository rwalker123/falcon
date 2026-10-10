//! Configuration for the Fog of War visibility system.
//!
//! Loaded from `visibility_config.json` with support for environment variable overrides.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_VISIBILITY_CONFIG: &str = include_str!("data/visibility_config.json");

/// Root configuration for the visibility system.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct VisibilityConfig {
    pub decay: DecayConfig,
    pub sight_ranges: HashMap<String, SightRangeConfig>,
    pub elevation: ElevationConfig,
    pub line_of_sight: LineOfSightConfig,
    pub terrain_detection: TerrainDetectionConfig,
    pub size_sight_bonus: SizeSightBonusConfig,
    pub movement: MovementConfig,
}

impl Default for VisibilityConfig {
    fn default() -> Self {
        Self {
            decay: DecayConfig::default(),
            sight_ranges: default_sight_ranges(),
            elevation: ElevationConfig::default(),
            line_of_sight: LineOfSightConfig::default(),
            terrain_detection: TerrainDetectionConfig::default(),
            size_sight_bonus: SizeSightBonusConfig::default(),
            movement: MovementConfig::default(),
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
            Self::from_json_str(BUILTIN_VISIBILITY_CONFIG)
                .expect("builtin visibility config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, VisibilityConfigError> {
        let config: VisibilityConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    /// The terrain scale must be finite and non-negative; the size bonus needs a finite non-negative
    /// threshold and a strictly positive step (it is a divisor).
    pub fn validate(&self) -> Result<(), VisibilityConfigError> {
        let scale = self.terrain_detection.tiles_per_unit;
        if !scale.is_finite() || scale < 0.0 {
            return Err(VisibilityConfigError::Invalid {
                field: "terrain_detection.tiles_per_unit",
                constraint: "be finite and at least 0",
                value: scale.to_string(),
            });
        }
        let threshold = self.size_sight_bonus.threshold_people;
        if !threshold.is_finite() || threshold < 0.0 {
            return Err(VisibilityConfigError::Invalid {
                field: "size_sight_bonus.threshold_people",
                constraint: "be finite and at least 0",
                value: threshold.to_string(),
            });
        }
        let step = self.size_sight_bonus.people_per_tile;
        if !step.is_finite() || step <= 0.0 {
            return Err(VisibilityConfigError::Invalid {
                field: "size_sight_bonus.people_per_tile",
                constraint: "be finite and greater than 0",
                value: step.to_string(),
            });
        }
        Ok(())
    }

    pub fn from_file(path: &Path) -> Result<Self, VisibilityConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| VisibilityConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        VisibilityConfig::from_json_str(&contents)
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
        // Permanent memory: a tile that leaves sight becomes Discovered (cloudy) and
        // stays that way — it never decays back to Unexplored (black). Set
        // enabled = true in visibility_config.json to re-enable the final decay step
        // (Discovered -> Unexplored after threshold_turns unseen).
        Self {
            enabled: false,
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

/// Configuration for how unit movement drives visibility.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MovementConfig {
    /// Upper bound on how many tiles a single-turn move may sweep for visibility.
    /// When a unit moves, `calculate_visibility` reveals the corridor between its
    /// previous and current tile; a span larger than this is treated as spurious
    /// (wrap-seam artifact, interpolation glitch, or an implausible jump) and only
    /// the endpoint is revealed. Keep this comfortably above the real maximum
    /// per-turn move distance (derived from movement speed) so genuine moves are
    /// always swept in full.
    pub max_sweep_tiles: u32,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self { max_sweep_tiles: 8 }
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

/// How a tile's authored `detection_modifier` (`terrain.rs`, roughly -0.25..+0.20) becomes whole
/// tiles of sight range into that tile. The one terrain sight term.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TerrainDetectionConfig {
    /// Tiles of range per 1.0 of `detection_modifier`; the product is truncated toward zero.
    pub tiles_per_unit: f32,
}

impl Default for TerrainDetectionConfig {
    fn default() -> Self {
        Self {
            tiles_per_unit: 10.0,
        }
    }
}

/// A large group is visible from further away: a per-TARGET-tile bonus on sight range into the tile
/// it stands on (faction-blind; a property of the target, not of the observer).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SizeSightBonusConfig {
    /// People standing on a tile below which there is no bonus (50 = the settlement "camp" stage
    /// `min_size`; a 30-person start band gets 0).
    pub threshold_people: f32,
    /// Each full step of this many people above the threshold adds one tile of range.
    pub people_per_tile: f32,
    /// Ceiling on the bonus, in tiles.
    pub max_tiles: u32,
}

impl Default for SizeSightBonusConfig {
    fn default() -> Self {
        Self {
            threshold_people: 50.0,
            people_per_tile: 25.0,
            max_tiles: 4,
        }
    }
}

#[derive(Debug, Error)]
pub enum VisibilityConfigError {
    #[error("failed to parse visibility config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid visibility config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: &'static str,
        value: String,
    },
    #[error("failed to read visibility config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl ConfigLoadError for VisibilityConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
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

/// Load visibility configuration from environment (`VISIBILITY_CONFIG_PATH`) or the default data
/// path.
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `VISIBILITY_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_visibility_config_from_env() -> (Arc<VisibilityConfig>, VisibilityConfigMetadata) {
    let (config, source) = load_config_from_env(
        "VISIBILITY_CONFIG_PATH",
        "visibility_config",
        "src/data/visibility_config.json",
        VisibilityConfig::builtin,
        VisibilityConfig::from_file,
    );
    (config, VisibilityConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let config = VisibilityConfig::default();
        // Permanent-memory default: Discovered tiles do not decay to Unexplored.
        assert!(!config.decay.enabled);
        assert_eq!(config.decay.threshold_turns, 12);
        assert!(config.sight_ranges.contains_key("BandScout"));
    }

    #[test]
    fn validate_rejects_bad_levers() {
        let bad_scale = r#"{"terrain_detection":{"tiles_per_unit":-1.0}}"#;
        assert!(VisibilityConfig::from_json_str(bad_scale).is_err());
        let bad_step = r#"{"size_sight_bonus":{"people_per_tile":0.0}}"#;
        assert!(VisibilityConfig::from_json_str(bad_step).is_err());
        let bad_threshold = r#"{"size_sight_bonus":{"threshold_people":-5.0}}"#;
        assert!(VisibilityConfig::from_json_str(bad_threshold).is_err());
        assert!(VisibilityConfig::from_json_str("{}").is_ok());
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
