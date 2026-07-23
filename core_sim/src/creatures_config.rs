//! The **creatures roster** (`data/creatures.json`) — intrinsic `CombatStats` for non-fauna units.
//!
//! A combatant = **an intrinsic creature ⊕ an equipment loadout** (`docs/plan_predators.md`). Animals
//! keep their intrinsic stats on [`crate::fauna_config::SpeciesDef`]; **humans and future non-fauna
//! units live here** — a human is not wildlife (so not `fauna_config.json`) and its stats are not
//! resolver tuning (so not `combat_config.json`). Today it is a 1-row roster holding the base
//! `"person"`. Mirrors the `expedition_config.rs` loader convention (baked-in builtin +
//! `CREATURES_CONFIG_PATH` override + [`CreaturesConfig::validate`] inside `from_json_str`, so a
//! broken override is rejected at **error** level and the builtin used).

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::combat::CombatStats;

pub const BUILTIN_CREATURES_CONFIG: &str = include_str!("data/creatures.json");

/// The roster id of the base human — the contingent kind every hunt party fields today.
pub const PERSON_ID: &str = "person";

/// One creature row: its intrinsic combat body. `combat` carries the same [`CombatStats`] a species
/// does, so combat composes a human and a wolf through the *same* neutral type.
#[derive(Debug, Clone, Deserialize)]
pub struct CreatureDef {
    /// Intrinsic per-unit combat profile (bare, before any equipment loadout).
    #[serde(default)]
    pub combat: CombatStats,
}

/// Root creatures configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CreaturesConfig {
    /// Creature rows keyed by id (`"person"` today).
    pub creatures: HashMap<String, CreatureDef>,
}

impl CreaturesConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_CREATURES_CONFIG)
                .expect("builtin creatures config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, CreaturesConfigError> {
        let config: CreaturesConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, CreaturesConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| CreaturesConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        CreaturesConfig::from_json_str(&contents)
    }

    /// Resolve a creature row by id.
    pub fn by_id(&self, id: &str) -> Option<&CreatureDef> {
        self.creatures.get(id)
    }

    /// The base human's combat stats — the profile every hunt party's contingent uses today.
    pub fn person(&self) -> CombatStats {
        self.by_id(PERSON_ID)
            .map(|def| def.combat)
            .unwrap_or_default()
    }

    /// The base `"person"` row must exist (the hunt adapter reads it) and every row's `combat` must be
    /// a legal profile: `attack >= 0` finite, `defense > 0` finite (it is a denominator in the
    /// kill/wound split).
    pub fn validate(&self) -> Result<(), CreaturesConfigError> {
        if !self.creatures.contains_key(PERSON_ID) {
            return Err(CreaturesConfigError::Invalid {
                field: "creatures.person",
                constraint: "be present (the base human every hunt party fields)".to_string(),
                value: "<missing>".to_string(),
            });
        }
        // Iterated in stable key order so the error names a deterministic creature.
        let mut rows: Vec<(&String, &CreatureDef)> = self.creatures.iter().collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        for (id, def) in rows {
            let attack_field: &'static str =
                Box::leak(format!("creatures.{id}.combat.attack").into_boxed_str());
            let defense_field: &'static str =
                Box::leak(format!("creatures.{id}.combat.defense").into_boxed_str());
            if !def.combat.attack.is_finite() || def.combat.attack < 0.0 {
                return Err(CreaturesConfigError::Invalid {
                    field: attack_field,
                    constraint: "be finite and at least 0".to_string(),
                    value: def.combat.attack.to_string(),
                });
            }
            if !def.combat.defense.is_finite() || def.combat.defense <= 0.0 {
                return Err(CreaturesConfigError::Invalid {
                    field: defense_field,
                    constraint: "be finite and greater than 0 (it is a denominator)".to_string(),
                    value: def.combat.defense.to_string(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CreaturesConfigError {
    #[error("failed to read creatures config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse creatures config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid creatures config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
}

/// Handle for accessing the creatures configuration.
#[derive(Resource, Debug, Clone)]
pub struct CreaturesConfigHandle(pub Arc<CreaturesConfig>);

impl CreaturesConfigHandle {
    pub fn new(config: Arc<CreaturesConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<CreaturesConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<CreaturesConfig>) {
        self.0 = config;
    }
}

impl Default for CreaturesConfigHandle {
    fn default() -> Self {
        Self(CreaturesConfig::builtin())
    }
}

/// Metadata about the creatures configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct CreaturesConfigMetadata {
    path: Option<PathBuf>,
}

impl CreaturesConfigMetadata {
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

/// Load creatures configuration from environment (`CREATURES_CONFIG_PATH`) or the default data path,
/// falling back to the baked-in builtin. Every candidate is **validated**; a broken override is
/// logged at **error** level and rejected in favour of the builtin.
pub fn load_creatures_config_from_env() -> (Arc<CreaturesConfig>, CreaturesConfigMetadata) {
    let override_path = env::var("CREATURES_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/creatures.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match CreaturesConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "creatures_config.loaded=file"
                );
                return (Arc::new(config), CreaturesConfigMetadata::new(Some(path)));
            }
            Err(err @ CreaturesConfigError::Invalid { .. }) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "creatures_config.invalid_rejected"
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "creatures_config.load_failed"
                );
            }
        }
    }

    let config = CreaturesConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "creatures_config.loaded=builtin"
    );
    (config, CreaturesConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::RangeBand;

    #[test]
    fn builtin_config_has_the_base_person() {
        let config = CreaturesConfig::builtin();
        let person = config.person();
        assert_eq!(person.attack, 1.0);
        assert_eq!(person.defense, 1.0);
        assert_eq!(person.range, RangeBand::Melee);
    }

    #[test]
    fn validate_rejects_a_missing_person() {
        let json =
            r#"{ "creatures": { "wolf": { "combat": { "attack": 3.0, "defense": 2.0 } } } }"#;
        assert!(matches!(
            CreaturesConfig::from_json_str(json),
            Err(CreaturesConfigError::Invalid {
                field: "creatures.person",
                ..
            })
        ));
    }

    #[test]
    fn validate_rejects_a_non_positive_defense() {
        let json =
            r#"{ "creatures": { "person": { "combat": { "attack": 1.0, "defense": 0.0 } } } }"#;
        assert!(matches!(
            CreaturesConfig::from_json_str(json),
            Err(CreaturesConfigError::Invalid { .. })
        ));
    }
}
