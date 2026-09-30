//! The pool a faction's name is drawn from, and the one per-world permutation that turns a faction
//! id into one of those names.
//!
//! **A faction's name is identity, and the sim owns it** — the rule band names already follow
//! (`.claude/rules/core_sim/band-names.md`). A faction used to be an id and nothing else, and the
//! client fabricated a name for it; now the world mints one per faction when it is generated,
//! publishes every faction's name on the wire, and carries the set through the checkpoint.
//!
//! **One permutation per WORLD, not per faction.** A band's name comes from its own faction's
//! permutation, because two bands of different peoples may share a name. Two factions of one world
//! may not, so the pool is shuffled once from `map_seed ^ FACTION_NAME_SALT` and faction `N` takes
//! position `N` — injective in the id by construction, with the band pool's cycle suffix past the
//! end. The shuffle itself is [`crate::band_names::permuted_name`], the band pool's own: one
//! implementation, two salted seeds.
//!
//! Loaded from `data/faction_names.json` on the shared boot seam ([`crate::config_load`]), with a
//! `FACTION_NAMES_PATH` override — the band catalog's shape exactly.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::band_names::{permuted_name, validate_name_pool, NamePoolFault};
use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::hashing::splitmix64;
use crate::orders::FactionId;

pub const BUILTIN_FACTION_NAMES: &str = include_str!("data/faction_names.json");

/// XOR sub-seed salt for the world's faction-name permutation (`map_seed ^ FACTION_NAME_SALT`), on
/// the repo's domain-subseed convention (cf. [`crate::band_names::BAND_NAME_SALT`]). A fixed,
/// arbitrary non-zero constant so faction naming cannot correlate with any other draw made from the
/// same map seed — the band permutations included.
pub const FACTION_NAME_SALT: u64 = 0xFAC7_10A5_3E2D_9B41;

/// The curated pool of faction names, in file order. **File order is an index space, not a
/// ranking** — see the module note.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct FactionNameCatalog {
    names: Vec<String>,
}

impl FactionNameCatalog {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_FACTION_NAMES)
                .expect("builtin faction names should be valid"),
        )
    }

    /// Parse **and validate** — empty or repeating pools are refused, on the band pool's rule
    /// ([`crate::band_names::validate_name_pool`]): two factions sharing a name would be a
    /// collision no probe could see.
    pub fn from_json_str(json: &str) -> Result<Self, FactionNamesError> {
        let catalog: Self = serde_json::from_str(json)?;
        validate_name_pool(&catalog.names).map_err(|fault| match fault {
            NamePoolFault::Empty => FactionNamesError::Empty,
            NamePoolFault::Duplicate(name) => FactionNamesError::Duplicate(name),
        })?;
        Ok(catalog)
    }

    pub fn from_file(path: &Path) -> Result<Self, FactionNamesError> {
        let contents = fs::read_to_string(path).map_err(|source| FactionNamesError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// **The name faction `faction` goes by in the world generated from `map_seed`** — a pure
    /// function of the two. One permutation per world, so two ids of one world never collide.
    pub fn name_for(&self, faction: FactionId, map_seed: u64) -> String {
        permuted_name(
            &self.names,
            splitmix64(map_seed ^ FACTION_NAME_SALT),
            faction.0 as usize,
        )
    }
}

#[derive(Debug, Error)]
pub enum FactionNamesError {
    #[error("failed to read faction names from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse faction names: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("faction name catalog is empty; every faction would be nameless")]
    Empty,
    #[error("faction name catalog repeats {0:?}; two factions of one world could share a name")]
    Duplicate(String),
}

impl ConfigLoadError for FactionNamesError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the faction name catalog.
#[derive(Resource, Debug, Clone)]
pub struct FactionNameCatalogHandle(pub Arc<FactionNameCatalog>);

impl FactionNameCatalogHandle {
    pub fn new(catalog: Arc<FactionNameCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<FactionNameCatalog> {
        Arc::clone(&self.0)
    }
}

impl Default for FactionNameCatalogHandle {
    fn default() -> Self {
        Self(FactionNameCatalog::builtin())
    }
}

/// Metadata about the faction name catalog source.
#[derive(Resource, Debug, Clone, Default)]
pub struct FactionNameCatalogMetadata {
    path: Option<PathBuf>,
}

impl FactionNameCatalogMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the faction name catalog from the environment (`FACTION_NAMES_PATH`) or the default data
/// path. Only an absent *default* path falls back to the builtin.
pub fn load_faction_names_from_env() -> (Arc<FactionNameCatalog>, FactionNameCatalogMetadata) {
    let (catalog, source) = load_config_from_env(
        "FACTION_NAMES_PATH",
        "faction_names",
        "src/data/faction_names.json",
        FactionNameCatalog::builtin,
        FactionNameCatalog::from_file,
    );
    (catalog, FactionNameCatalogMetadata::new(source))
}

/// **Every faction's name in this world** — minted by worldgen from the roster and the map seed,
/// published world-visible, and **checkpointed** (`SimState::faction_names`).
///
/// **The save wins**, like the roster itself: a pool edit after a game was saved must not rename
/// that game's factions, so a load restores these strings rather than re-deriving them.
///
/// A `BTreeMap` so the wire section iterates in faction order and diffs out when nothing moved.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionNames(BTreeMap<FactionId, String>);

impl FactionNames {
    /// Mint every faction on `factions` from `catalog` under `map_seed`.
    pub fn mint(factions: &[FactionId], map_seed: u64, catalog: &FactionNameCatalog) -> Self {
        Self(
            factions
                .iter()
                .map(|faction| (*faction, catalog.name_for(*faction, map_seed)))
                .collect(),
        )
    }

    /// This faction's name, or `None` for a faction the world never minted.
    pub fn name(&self, faction: FactionId) -> Option<&str> {
        self.0.get(&faction).map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (FactionId, &str)> {
        self.0
            .iter()
            .map(|(faction, name)| (*faction, name.as_str()))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const TEST_MAP_SEED: u64 = 0x0F0E_0D0C_0B0A_0908;

    #[test]
    fn builtin_catalog_loads_and_is_distinct_and_digit_free() {
        let catalog = FactionNameCatalog::builtin();
        assert!(!catalog.is_empty());
        let unique: HashSet<&str> = catalog.names().iter().map(String::as_str).collect();
        assert_eq!(
            unique.len(),
            catalog.len(),
            "the shipped pool holds no repeat"
        );
        for name in catalog.names() {
            assert!(!name.is_empty());
            assert!(
                !name
                    .chars()
                    .any(|c| c.is_ascii_digit() || c.is_whitespace()),
                "{name:?} would be confusable with a cycle suffix"
            );
        }
    }

    #[test]
    fn a_duplicate_or_an_empty_pool_is_refused() {
        let dup = FactionNameCatalog::from_json_str(r#"{"names": ["Ashkin", "Ashkin"]}"#)
            .expect_err("a repeated name must not load");
        assert!(matches!(&dup, FactionNamesError::Duplicate(n) if n == "Ashkin"));
        let empty = FactionNameCatalog::from_json_str(r#"{"names": []}"#)
            .expect_err("an empty pool must not load");
        assert!(matches!(empty, FactionNamesError::Empty));
    }

    #[test]
    fn a_roster_past_the_pool_is_still_distinct() {
        let catalog = FactionNameCatalog::builtin();
        // Two full cycles and a remainder: the suffix has to carry the distinction past the pool.
        let roster: Vec<FactionId> = (0..(catalog.len() * 2 + 3) as u32).map(FactionId).collect();
        let names = FactionNames::mint(&roster, TEST_MAP_SEED, &catalog);
        let unique: HashSet<&str> = names.iter().map(|(_, n)| n).collect();
        assert_eq!(unique.len(), roster.len());
    }

    #[test]
    fn the_map_seed_moves_the_names() {
        let catalog = FactionNameCatalog::builtin();
        let roster: Vec<FactionId> = (0..catalog.len() as u32).map(FactionId).collect();
        assert_eq!(
            FactionNames::mint(&roster, TEST_MAP_SEED, &catalog),
            FactionNames::mint(&roster, TEST_MAP_SEED, &catalog),
            "a pure function of (seed, faction)"
        );
        assert_ne!(
            FactionNames::mint(&roster, TEST_MAP_SEED, &catalog),
            FactionNames::mint(&roster, TEST_MAP_SEED ^ 1, &catalog),
            "and the seed decides the permutation"
        );
    }
}
