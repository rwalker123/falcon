//! The personality catalog - the cultural lean a STARTING faction is born with.
//!
//! A personality is a small offset on a few culture axes. Every faction that exists at world build
//! (the human seat, the majors, the minors) draws one from this catalog, weighted and seeded from
//! the map seed and its faction id, and its starting bands carry the lean on their culture
//! MODIFIER (`culture::StartingCultureLean`). A faction born mid-game (fission, independence)
//! draws nothing: it inherits whatever its source band already is.
//!
//! Loaded from `data/personalities.json` on the shared boot seam ([`crate::config_load`]), with a
//! `PERSONALITIES_PATH` override - the faction-name catalog's shape exactly.
//! The rationale is `.claude/rules/core_sim/factions.md` -> "Personalities".

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::culture::{culture_axis_from_key, CULTURE_TRAIT_AXES};
use crate::hashing::splitmix64;
use crate::orders::FactionId;
use crate::scalar::{scalar_from_f32, Scalar};

pub const BUILTIN_PERSONALITIES: &str = include_str!("data/personalities.json");

/// XOR sub-seed salt for a faction's personality draw (`map_seed ^ salt ^ faction_id`), on the
/// repo's domain-subseed convention (cf. [`crate::faction_names::FACTION_NAME_SALT`]): a fixed,
/// arbitrary non-zero constant so the draw cannot correlate with any other made from the same map
/// seed.
pub const PERSONALITY_SALT: u64 = 0x9E25_01A1_17E5_C0DE;

/// 2^64 as an `f64`: maps a 64-bit hash onto `[0, 1)`.
const HASH_RANGE: f64 = 18_446_744_073_709_551_616.0;

/// One entry of the catalog.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personality {
    pub id: String,
    /// Culture axis key (see [`crate::culture::culture_axis_key`]) -> offset added to the axis. The
    /// sign follows the axis name: `a_b` runs from the `a` pole (negative) to the `b` pole
    /// (positive).
    #[serde(default)]
    pub culture_lean: BTreeMap<String, f32>,
    /// Relative chance of being drawn. Validated `> 0`.
    pub weight: f32,
}

impl Personality {
    /// This personality's lean as a per-axis offset vector (zero on every axis it does not name).
    pub fn lean_vector(&self) -> [Scalar; CULTURE_TRAIT_AXES] {
        let mut lean = [Scalar::zero(); CULTURE_TRAIT_AXES];
        for (key, offset) in &self.culture_lean {
            if let Some(axis) = culture_axis_from_key(key) {
                lean[axis.index()] = scalar_from_f32(*offset);
            }
        }
        lean
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersonalitiesData {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    personalities: Vec<Personality>,
}

/// The validated catalog, in file order.
#[derive(Debug, Clone)]
pub struct PersonalityCatalog {
    personalities: Vec<Personality>,
}

impl PersonalityCatalog {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_PERSONALITIES)
                .expect("builtin personalities should be valid"),
        )
    }

    /// Parse **and validate**: at least one entry, unique ids, known culture axis keys, finite
    /// offsets, and a finite weight above zero.
    pub fn from_json_str(json: &str) -> Result<Self, PersonalitiesError> {
        let data: PersonalitiesData = serde_json::from_str(json)?;
        if data.personalities.is_empty() {
            return Err(PersonalitiesError::Empty);
        }
        for (index, personality) in data.personalities.iter().enumerate() {
            if data.personalities[..index]
                .iter()
                .any(|seen| seen.id == personality.id)
            {
                return Err(PersonalitiesError::DuplicateId(personality.id.clone()));
            }
            if !personality.weight.is_finite() || personality.weight <= 0.0 {
                return Err(PersonalitiesError::BadWeight {
                    id: personality.id.clone(),
                    weight: personality.weight,
                });
            }
            for (key, offset) in &personality.culture_lean {
                if culture_axis_from_key(key).is_none() {
                    return Err(PersonalitiesError::UnknownAxis {
                        id: personality.id.clone(),
                        key: key.clone(),
                    });
                }
                if !offset.is_finite() {
                    return Err(PersonalitiesError::NonFiniteOffset {
                        id: personality.id.clone(),
                        key: key.clone(),
                    });
                }
            }
        }
        Ok(Self {
            personalities: data.personalities,
        })
    }

    pub fn from_file(path: &Path) -> Result<Self, PersonalitiesError> {
        let contents = fs::read_to_string(path).map_err(|source| PersonalitiesError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    pub fn personalities(&self) -> &[Personality] {
        &self.personalities
    }

    pub fn len(&self) -> usize {
        self.personalities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.personalities.is_empty()
    }

    /// **The personality `faction` is born with in the world generated from `map_seed`** - a pure
    /// function of the two: a weighted draw over the catalog from one splitmix64 of
    /// `map_seed ^ PERSONALITY_SALT ^ faction`.
    pub fn draw(&self, faction: FactionId, map_seed: u64) -> &Personality {
        let hash = splitmix64(map_seed ^ PERSONALITY_SALT ^ u64::from(faction.0));
        let total: f64 = self
            .personalities
            .iter()
            .map(|personality| f64::from(personality.weight))
            .sum();
        let mut target = (hash as f64 / HASH_RANGE) * total;
        for personality in &self.personalities {
            target -= f64::from(personality.weight);
            if target < 0.0 {
                return personality;
            }
        }
        // Rounding can leave `target` at the very end of the range; the last entry owns it.
        self.personalities
            .last()
            .expect("validated non-empty at load")
    }
}

#[derive(Debug, Error)]
pub enum PersonalitiesError {
    #[error("failed to read personalities from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse personalities: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("personality catalog is empty; no faction could be drawn a lean")]
    Empty,
    #[error("personality catalog repeats id `{0}`")]
    DuplicateId(String),
    #[error("personality `{id}` has weight {weight}: a weight must be finite and above zero")]
    BadWeight { id: String, weight: f32 },
    #[error("personality `{id}` leans on `{key}`, which is not a culture axis key")]
    UnknownAxis { id: String, key: String },
    #[error("personality `{id}` has a non-finite offset on `{key}`")]
    NonFiniteOffset { id: String, key: String },
}

impl ConfigLoadError for PersonalitiesError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the personality catalog.
#[derive(Resource, Debug, Clone)]
pub struct PersonalityCatalogHandle(pub Arc<PersonalityCatalog>);

impl PersonalityCatalogHandle {
    pub fn new(catalog: Arc<PersonalityCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<PersonalityCatalog> {
        Arc::clone(&self.0)
    }
}

impl Default for PersonalityCatalogHandle {
    fn default() -> Self {
        Self(PersonalityCatalog::builtin())
    }
}

/// Metadata about the personality catalog source.
#[derive(Resource, Debug, Clone, Default)]
pub struct PersonalityCatalogMetadata {
    path: Option<PathBuf>,
}

impl PersonalityCatalogMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the catalog from the environment (`PERSONALITIES_PATH`) or the default data path. Only an
/// absent *default* path falls back to the builtin.
pub fn load_personalities_from_env() -> (Arc<PersonalityCatalog>, PersonalityCatalogMetadata) {
    let (catalog, source) = load_config_from_env(
        "PERSONALITIES_PATH",
        "personalities",
        "src/data/personalities.json",
        PersonalityCatalog::builtin,
        PersonalityCatalog::from_file,
    );
    (catalog, PersonalityCatalogMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::culture::CultureTraitAxis;
    use serde_json::{json, Value};

    /// A sample of seeds wide enough that a six-entry catalog shows every entry.
    const SAMPLE_SEEDS: u64 = 64;

    fn builtin_json() -> Value {
        serde_json::from_str(BUILTIN_PERSONALITIES).expect("the builtin parses as json")
    }

    #[test]
    fn the_builtin_catalog_ships_the_six_opening_personalities() {
        let catalog = PersonalityCatalog::builtin();
        let ids: Vec<&str> = catalog
            .personalities()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec![
                "purist",
                "syncretic",
                "warlike",
                "wanderers",
                "homebodies",
                "steady"
            ]
        );
    }

    /// The sign convention: the axis `a_b` runs from `a` (negative) to `b` (positive).
    #[test]
    fn the_shipped_leans_point_at_the_named_pole() {
        let catalog = PersonalityCatalog::builtin();
        let lean_of = |id: &str, axis: CultureTraitAxis| -> f32 {
            let personality = catalog
                .personalities()
                .iter()
                .find(|p| p.id == id)
                .expect("shipped personality");
            personality.lean_vector()[axis.index()].to_f32()
        };
        assert!(lean_of("purist", CultureTraitAxis::SyncreticPurist) > 0.0);
        assert!(lean_of("syncretic", CultureTraitAxis::SyncreticPurist) < 0.0);
        assert!(lean_of("warlike", CultureTraitAxis::PassiveAggressive) > 0.0);
        assert!(lean_of("wanderers", CultureTraitAxis::ExpansionistInsular) < 0.0);
        assert!(lean_of("homebodies", CultureTraitAxis::ExpansionistInsular) > 0.0);
        let steady = catalog
            .personalities()
            .iter()
            .find(|p| p.id == "steady")
            .expect("shipped personality");
        assert!(steady.lean_vector().iter().all(|v| *v == Scalar::zero()));
    }

    #[test]
    fn the_draw_is_deterministic_for_a_seed_and_a_faction() {
        let catalog = PersonalityCatalog::builtin();
        for seed in 0..SAMPLE_SEEDS {
            for faction in 0..4 {
                assert_eq!(
                    catalog.draw(FactionId(faction), seed).id,
                    catalog.draw(FactionId(faction), seed).id
                );
            }
        }
    }

    /// Over a sample the draw reaches every entry and differs across factions of one seed.
    #[test]
    fn the_draw_varies_across_factions_and_reaches_every_entry() {
        let catalog = PersonalityCatalog::builtin();
        let mut seen = std::collections::BTreeSet::new();
        let mut differing_seeds = 0;
        for seed in 0..SAMPLE_SEEDS {
            let drawn: Vec<&str> = (0..4)
                .map(|faction| catalog.draw(FactionId(faction), seed).id.as_str())
                .collect();
            seen.extend(drawn.iter().copied());
            if drawn.iter().any(|id| *id != drawn[0]) {
                differing_seeds += 1;
            }
        }
        assert_eq!(seen.len(), catalog.len(), "every personality is reachable");
        assert!(
            differing_seeds > 0,
            "factions of one world must not all draw alike"
        );
    }

    #[test]
    fn a_heavier_weight_is_drawn_more_often() {
        let catalog = PersonalityCatalog::from_json_str(
            &json!({"personalities": [
                {"id": "common", "weight": 9.0},
                {"id": "rare", "weight": 1.0}
            ]})
            .to_string(),
        )
        .expect("valid");
        let common = (0..SAMPLE_SEEDS * 4)
            .filter(|seed| catalog.draw(FactionId(0), *seed).id == "common")
            .count() as u64;
        assert!(common > SAMPLE_SEEDS * 2, "common drew {common}");
    }

    #[test]
    fn an_empty_catalog_is_rejected() {
        assert!(matches!(
            PersonalityCatalog::from_json_str(&json!({"personalities": []}).to_string()),
            Err(PersonalitiesError::Empty)
        ));
    }

    #[test]
    fn a_duplicate_id_is_rejected() {
        let mut value = builtin_json();
        value["personalities"][1]["id"] = Value::from("purist");
        assert!(matches!(
            PersonalityCatalog::from_json_str(&value.to_string()),
            Err(PersonalitiesError::DuplicateId(_))
        ));
    }

    #[test]
    fn a_non_positive_weight_is_rejected() {
        for bad in [0.0, -1.0] {
            let mut value = builtin_json();
            value["personalities"][0]["weight"] = Value::from(bad);
            assert!(matches!(
                PersonalityCatalog::from_json_str(&value.to_string()),
                Err(PersonalitiesError::BadWeight { .. })
            ));
        }
    }

    #[test]
    fn an_unknown_axis_key_is_rejected() {
        let mut value = builtin_json();
        value["personalities"][0]["culture_lean"] = json!({"not_an_axis": 0.5});
        assert!(matches!(
            PersonalityCatalog::from_json_str(&value.to_string()),
            Err(PersonalitiesError::UnknownAxis { .. })
        ));
    }
}
