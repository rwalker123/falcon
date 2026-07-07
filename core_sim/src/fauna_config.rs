//! Data-driven fauna species table + spawn abundance.
//!
//! Loaded from `data/fauna_config.json`. Turns the former hard-coded `HerdSpecies`
//! enum into a table: each species carries a display name, size class, migratory
//! flag, roaming range (route length), group biomass, and the food-module "biomes"
//! it hosts in. `abundance` drives how densely short-range game spawns per biome.
//! Mirrors the `visibility_config.rs` loader pattern (baked-in builtin + optional
//! file/env override).

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use rand::{rngs::SmallRng, Rng};
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_FAUNA_CONFIG: &str = include_str!("data/fauna_config.json");

/// Coarse size band. Drives roaming range + group size; also lets Phase B/C offer
/// the right verbs (big/small game are huntable one-shot; migratory herds follow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    #[default]
    Small,
    Big,
    Migratory,
}

impl SizeClass {
    /// Stable string key (also the snapshot `size_class` field).
    pub fn as_str(&self) -> &'static str {
        match self {
            SizeClass::Small => "small",
            SizeClass::Big => "big",
            SizeClass::Migratory => "migratory",
        }
    }
}

/// One species row in the table.
#[derive(Debug, Clone, Deserialize)]
pub struct SpeciesDef {
    /// Player-facing name; also the snapshot `species` string. Must embed the
    /// client icon keyword (e.g. "deer", "boar") so `FoodIcons.for_herd` resolves.
    pub display_name: String,
    #[serde(default)]
    pub size_class: SizeClass,
    #[serde(default)]
    pub migratory: bool,
    /// Inclusive `[min, max]` route length in tiles = roaming range.
    pub route_len: [u32; 2],
    /// Inclusive `[min, max]` group biomass.
    pub biomass: [f32; 2],
    /// Food-module keys (see `FoodModule::as_str`) this species hosts in.
    #[serde(default)]
    pub host_biomes: Vec<String>,
}

impl SpeciesDef {
    /// Sample a route length within the configured inclusive range (>= 1).
    pub fn sample_route_len(&self, rng: &mut SmallRng) -> u32 {
        let lo = self.route_len[0].max(1);
        let hi = self.route_len[1].max(lo);
        rng.gen_range(lo..=hi)
    }

    /// Sample a group biomass within the configured inclusive range.
    pub fn sample_biomass(&self, rng: &mut SmallRng) -> f32 {
        let lo = self.biomass[0].max(0.0);
        let hi = self.biomass[1].max(lo);
        if hi <= lo {
            lo
        } else {
            rng.gen_range(lo..=hi)
        }
    }

    pub fn hosts_biome(&self, module_key: &str) -> bool {
        self.host_biomes.iter().any(|b| b == module_key)
    }

    /// Per-species carrying capacity biomass regrows toward (= the table max).
    pub fn carrying_capacity(&self) -> f32 {
        self.biomass[1].max(self.biomass[0]).max(0.0)
    }
}

/// Spawn-density tuning. `per_biome` is the per-tile probability of placing a game
/// group, keyed by the tile's food module; abundance is high to start by design.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AbundanceConfig {
    pub per_biome: HashMap<String, f32>,
    pub max_total_game: usize,
    pub min_spacing: u32,
}

impl AbundanceConfig {
    pub fn probability_for(&self, module_key: &str) -> f32 {
        self.per_biome
            .get(module_key)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
}

/// One-shot hunt tuning: how much biomass a hunt takes, how it converts to
/// resources, and the pursuit geometry (band closes to `pursuit_radius` tiles).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HuntConfig {
    pub take_fraction: f32,
    pub min_take: f32,
    pub provisions_per_biomass: f32,
    pub trade_goods_per_biomass: f32,
    pub pursuit_radius: u32,
    pub pursuit_tiles_per_turn: u32,
    pub max_pursuit_turns: u32,
}

impl Default for HuntConfig {
    fn default() -> Self {
        Self {
            take_fraction: 0.30,
            min_take: 40.0,
            provisions_per_biomass: 0.02,
            trade_goods_per_biomass: 0.005,
            pursuit_radius: 1,
            pursuit_tiles_per_turn: 3,
            max_pursuit_turns: 12,
        }
    }
}

impl HuntConfig {
    /// Biomass taken from a group of `biomass`, clamped to `[min_take, biomass]`.
    pub fn take_from(&self, biomass: f32) -> f32 {
        if biomass <= 0.0 {
            return 0.0;
        }
        let fraction_take = (biomass * self.take_fraction).max(self.min_take);
        fraction_take.min(biomass)
    }
}

/// Ecology tuning: per-turn logistic regrowth toward each species' carrying cap.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct EcologyConfig {
    pub regrowth_rate: f32,
}

impl Default for EcologyConfig {
    fn default() -> Self {
        Self {
            regrowth_rate: 0.05,
        }
    }
}

/// Follow tuning: policy draw-rates (Sustain = regrowth, Surplus = regrowth ×
/// `surplus_multiplier`, Eradicate reuses the one-shot hunt take) plus the small
/// per-turn non-food tracking benefit (fog reveal pulse + morale).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FollowConfig {
    pub surplus_multiplier: f32,
    pub reveal_radius: u32,
    pub reveal_duration_turns: u64,
    pub morale_gain: f32,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            surplus_multiplier: 1.6,
            reveal_radius: 2,
            reveal_duration_turns: 3,
            morale_gain: 0.01,
        }
    }
}

/// Root fauna configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FaunaConfig {
    pub species: HashMap<String, SpeciesDef>,
    pub abundance: AbundanceConfig,
    pub hunt: HuntConfig,
    pub ecology: EcologyConfig,
    pub follow: FollowConfig,
}

impl FaunaConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_FAUNA_CONFIG).expect("builtin fauna config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, FaunaConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| FaunaConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(FaunaConfig::from_json_str(&contents)?)
    }

    /// `(key, def)` pairs for every migratory species, in a stable key order.
    pub fn migratory_species(&self) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| def.migratory)
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// `(key, def)` pairs for every non-migratory (short-range) game species that
    /// hosts in `module_key`, in a stable key order.
    pub fn game_species_for_biome(&self, module_key: &str) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| !def.migratory && def.hosts_biome(module_key))
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

#[derive(Debug, Error)]
pub enum FaunaConfigError {
    #[error("failed to read fauna config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse fauna config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the fauna configuration.
#[derive(Resource, Debug, Clone)]
pub struct FaunaConfigHandle(pub Arc<FaunaConfig>);

impl FaunaConfigHandle {
    pub fn new(config: Arc<FaunaConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<FaunaConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<FaunaConfig>) {
        self.0 = config;
    }
}

impl Default for FaunaConfigHandle {
    fn default() -> Self {
        Self(FaunaConfig::builtin())
    }
}

/// Metadata about the fauna configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct FaunaConfigMetadata {
    path: Option<PathBuf>,
}

impl FaunaConfigMetadata {
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

/// Load fauna configuration from environment (`FAUNA_CONFIG_PATH`) or the default
/// data path, falling back to the baked-in builtin.
pub fn load_fauna_config_from_env() -> (Arc<FaunaConfig>, FaunaConfigMetadata) {
    let override_path = env::var("FAUNA_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/fauna_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match FaunaConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "fauna_config.loaded=file"
                );
                return (Arc::new(config), FaunaConfigMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "fauna_config.load_failed"
                );
            }
        }
    }

    let config = FaunaConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "fauna_config.loaded=builtin"
    );
    (config, FaunaConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses() {
        let config = FaunaConfig::builtin();
        assert!(config.species.contains_key("deer"));
        assert!(config.species.contains_key("rabbit"));
        assert!(config.species.contains_key("mammoth"));
        // Display names must embed the client icon keyword.
        assert!(config.species["deer"]
            .display_name
            .to_lowercase()
            .contains("deer"));
        assert!(config.species["boar"]
            .display_name
            .to_lowercase()
            .contains("boar"));
    }

    #[test]
    fn migratory_and_game_partitions() {
        let config = FaunaConfig::builtin();
        let migratory = config.migratory_species();
        assert!(migratory.iter().all(|(_, def)| def.migratory));
        assert!(migratory.iter().any(|(k, _)| k.as_str() == "mammoth"));

        // Deer hosts in temperate forest and is short-range game.
        let forest_game = config.game_species_for_biome("temperate_forest");
        assert!(forest_game.iter().any(|(k, _)| k.as_str() == "deer"));
        assert!(forest_game.iter().all(|(_, def)| !def.migratory));
    }

    #[test]
    fn abundance_probability_clamps() {
        let config = FaunaConfig::builtin();
        assert!(config.abundance.probability_for("temperate_forest") > 0.0);
        assert_eq!(config.abundance.probability_for("deep_ocean"), 0.0);
    }

    #[test]
    fn hunt_and_ecology_present() {
        let config = FaunaConfig::builtin();
        assert!(config.hunt.take_fraction > 0.0);
        assert_eq!(config.hunt.pursuit_radius, 1);
        assert!(config.ecology.regrowth_rate > 0.0);
        assert!(config.follow.surplus_multiplier > 1.0);
        assert!(config.follow.reveal_radius >= 1);
        // take clamps to [min_take, biomass].
        assert_eq!(config.hunt.take_from(0.0), 0.0);
        assert_eq!(config.hunt.take_from(10.0), 10.0); // below min_take -> whole group
        let big = config.hunt.take_from(10_000.0);
        assert!(big >= config.hunt.min_take && big <= 10_000.0);
    }

    #[test]
    fn size_class_round_trips() {
        assert_eq!(SizeClass::Big.as_str(), "big");
        assert_eq!(SizeClass::Migratory.as_str(), "migratory");
    }
}
