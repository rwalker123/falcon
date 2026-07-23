//! Resolver tuning for the combat subsystem (`data/combat_config.json`).
//!
//! This file is **resolver tuning, not creature identity** (`docs/plan_predators.md`): the severity
//! constants [`crate::combat::resolve_fight`] reads. Creature stats live with their creature (animals
//! → [`crate::fauna_config::SpeciesDef`], humans → [`crate::creatures_config`]); this holds only the
//! knobs that shape *how a fight resolves*. Mirrors the `expedition_config.rs` loader convention
//! (baked-in builtin + `COMBAT_CONFIG_PATH` override + [`CombatConfig::validate`] inside
//! `from_json_str`, so a broken override is rejected at **error** level and the builtin used).

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::combat::CombatTuning;

pub const BUILTIN_COMBAT_CONFIG: &str = include_str!("data/combat_config.json");

/// Root combat-resolver configuration. Every lever is read through the handle into a
/// [`CombatTuning`] passed to the pure resolver — no bare literal drives the fight math.
#[derive(Debug, Clone, Deserialize)]
pub struct CombatConfig {
    /// Scales every side's total losses (`docs/plan_predators.md`). Ships **1.0**.
    pub lethality: f32,
    /// A loser whose losses exceed this fraction of its headcount is driven off (`disengaged`) rather
    /// than annihilated. Ships **0.5**.
    pub disengage_fraction: f32,
    /// **How much bloodier a hunt is when a detached expedition fights it** — a multiplier on
    /// `lethality` applied only in the expedition-hunt adapter (`advance_expeditions`), never the
    /// resident-band path. A hunting party is far from home, unsupported and tired, so the same beast
    /// costs it more. Ships **1.5**. A deferred general combat-modifiers layer (proximity / fatigue /
    /// supply, plus a *home-advantage* discount for local hunts) will supersede this flat dial. Ships
    /// finite & `> 0`.
    pub expedition_danger_multiplier: f32,
}

impl CombatConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_COMBAT_CONFIG)
                .expect("builtin combat config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, CombatConfigError> {
        let config: CombatConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, CombatConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| CombatConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        CombatConfig::from_json_str(&contents)
    }

    /// The [`CombatTuning`] the pure resolver reads. Keeping the two types separate lets combat depend
    /// on nothing from the config layer.
    pub fn tuning(&self) -> CombatTuning {
        CombatTuning {
            lethality: self.lethality,
            disengage_fraction: self.disengage_fraction,
        }
    }

    /// Both severity dials must be finite and `> 0` (at `0` a fight is bloodless — the whole
    /// subsystem is silently disabled), and `disengage_fraction <= 1` (above a full headcount no loser
    /// could ever be flagged as merely driven off).
    pub fn validate(&self) -> Result<(), CombatConfigError> {
        require_positive_finite("lethality", self.lethality)?;
        require_positive_finite(
            "expedition_danger_multiplier",
            self.expedition_danger_multiplier,
        )?;
        require_positive_finite("disengage_fraction", self.disengage_fraction)?;
        if self.disengage_fraction > MAX_FRACTION {
            return Err(CombatConfigError::Invalid {
                field: "disengage_fraction",
                constraint: format!("be at most {MAX_FRACTION}"),
                value: self.disengage_fraction.to_string(),
            });
        }
        Ok(())
    }
}

/// The largest a fraction-valued lever may be.
const MAX_FRACTION: f32 = 1.0;

fn require_positive_finite(field: &'static str, value: f32) -> Result<(), CombatConfigError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(CombatConfigError::Invalid {
            field,
            constraint: "be finite and greater than 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum CombatConfigError {
    #[error("failed to read combat config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse combat config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid combat config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
}

/// Handle for accessing the combat configuration.
#[derive(Resource, Debug, Clone)]
pub struct CombatConfigHandle(pub Arc<CombatConfig>);

impl CombatConfigHandle {
    pub fn new(config: Arc<CombatConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<CombatConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<CombatConfig>) {
        self.0 = config;
    }
}

impl Default for CombatConfigHandle {
    fn default() -> Self {
        Self(CombatConfig::builtin())
    }
}

/// Metadata about the combat configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct CombatConfigMetadata {
    path: Option<PathBuf>,
}

impl CombatConfigMetadata {
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

/// Load combat configuration from environment (`COMBAT_CONFIG_PATH`) or the default data path,
/// falling back to the baked-in builtin. Every candidate is **validated** before it can reach the
/// sim; a broken override is logged at **error** level and rejected in favour of the builtin.
pub fn load_combat_config_from_env() -> (Arc<CombatConfig>, CombatConfigMetadata) {
    let override_path = env::var("COMBAT_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/combat_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match CombatConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "combat_config.loaded=file"
                );
                return (Arc::new(config), CombatConfigMetadata::new(Some(path)));
            }
            Err(err @ CombatConfigError::Invalid { .. }) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "combat_config.invalid_rejected"
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "combat_config.load_failed"
                );
            }
        }
    }

    let config = CombatConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "combat_config.loaded=builtin"
    );
    (config, CombatConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_matches_the_shipped_defaults() {
        let config = CombatConfig::builtin();
        assert_eq!(config.lethality, 1.0);
        assert_eq!(config.disengage_fraction, 0.5);
        assert_eq!(config.expedition_danger_multiplier, 1.5);
    }

    #[test]
    fn validate_rejects_a_non_positive_expedition_danger_multiplier() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.expedition_danger_multiplier = 0.0;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "expedition_danger_multiplier",
                ..
            })
        ));
    }

    #[test]
    fn validate_rejects_a_non_positive_lethality() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.lethality = 0.0;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "lethality",
                ..
            })
        ));
    }

    #[test]
    fn validate_rejects_a_disengage_fraction_above_one() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.disengage_fraction = 1.5;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "disengage_fraction",
                ..
            })
        ));
    }
}
