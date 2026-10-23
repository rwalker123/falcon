//! The pool a band leader's name is drawn from.
//!
//! **A leader is a named person**, and the sim owns the name for the reason a band's name is owned
//! there (`crate::band_names`): it is minted once, published on the wire and carried through the
//! checkpoint, never derived on the client. This module is the band catalog's twin, sharing its
//! two primitives -- [`validate_name_pool`] (a pool that is empty or repeats a name is refused) and
//! [`permuted_name`] (the one salted shuffle and the cycle suffix) -- so there is one
//! implementation of each.
//!
//! Loaded from `data/leader_names.json` on the shared boot seam ([`crate::config_load`]): an absent
//! default path falls back to the builtin, anything else that is there and wrong is a boot panic.
//! **The list is CONTENT, not tuning.**
//!
//! **How a band's leader is named.** The permutation is seeded by the map seed and the leader
//! salt; the *index* into it is the band's id. A band's leader therefore never depends on how many
//! leaders have emerged or in what order, and (the shuffle being a permutation) two bands of one
//! world never share a name until the pool is exhausted, after which the cycle suffix keeps them
//! apart. Rationale: `.claude/rules/core_sim/leadership.md`.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::band_names::{permuted_name, validate_name_pool, NamePoolFault};
use crate::components::BandId;
use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::hashing::splitmix64;

pub const BUILTIN_LEADER_NAMES: &str = include_str!("data/leader_names.json");

/// XOR sub-seed salt for the leader-name permutation (`map_seed ^ LEADER_NAME_SALT`), following the
/// repo's domain-subseed convention (cf. `BAND_NAME_SALT`). A fixed, arbitrary non-zero constant,
/// distinct from every other salt, so leader naming cannot correlate with any other draw made from
/// the same map seed -- and in particular not with the band-name permutation.
pub const LEADER_NAME_SALT: u64 = 0x1EAD_E4A3_9B27_C6D1;

/// A second XOR salt, on top of [`LEADER_NAME_SALT`], for the tribe-leader permutation: tribe
/// leaders draw from the same pool but never share a shuffle with a band's leader. A fixed,
/// arbitrary non-zero constant.
pub const TRIBE_LEADER_NAME_SALT: u64 = 0x7121_BE5A_C0DE_4F19;

/// The curated pool of leader names, in file order. File order is an index space, not a ranking.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct LeaderNameCatalog {
    names: Vec<String>,
}

impl LeaderNameCatalog {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_LEADER_NAMES)
                .expect("builtin leader names should be valid"),
        )
    }

    /// Parse **and validate**: an empty pool, or one that repeats a name, is refused.
    pub fn from_json_str(json: &str) -> Result<Self, LeaderNamesError> {
        let catalog: Self = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn from_file(path: &Path) -> Result<Self, LeaderNamesError> {
        let contents = fs::read_to_string(path).map_err(|source| LeaderNamesError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    fn validate(&self) -> Result<(), LeaderNamesError> {
        validate_name_pool(&self.names).map_err(|fault| match fault {
            NamePoolFault::Empty => LeaderNamesError::Empty,
            NamePoolFault::Duplicate(name) => LeaderNamesError::Duplicate(name),
        })
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

    /// The name of the leader `band` gets under `map_seed`.
    ///
    /// **Deterministic**: the same map seed and band always spell the same name, so a save, a
    /// reload and a replay agree. **Injective in the band id** up to the pool's length, and past it
    /// by the cycle suffix, for the reason [`permuted_name`] gives.
    pub fn name_for_band(&self, map_seed: u64, band: BandId) -> String {
        let seed = splitmix64(map_seed ^ LEADER_NAME_SALT);
        permuted_name(&self.names, seed, band.0 as usize)
    }

    /// The name of the leader tribe number `tribe` forms under `map_seed`: the same pool and
    /// shuffle, under the tribe salt on top, so it is deterministic, unique among a world's tribes
    /// until the pool is exhausted, and independent of every band leader's name.
    pub fn name_for_tribe(&self, map_seed: u64, tribe: u64) -> String {
        let seed = splitmix64(map_seed ^ LEADER_NAME_SALT ^ TRIBE_LEADER_NAME_SALT);
        permuted_name(&self.names, seed, tribe as usize)
    }
}

#[derive(Debug, Error)]
pub enum LeaderNamesError {
    #[error("failed to read leader names from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse leader names: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("leader name catalog is empty; every leader would emerge nameless")]
    Empty,
    #[error("leader name catalog repeats {0:?}; two bands' leaders would share a name")]
    Duplicate(String),
}

impl ConfigLoadError for LeaderNamesError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the leader name catalog.
#[derive(Resource, Debug, Clone)]
pub struct LeaderNameCatalogHandle(pub Arc<LeaderNameCatalog>);

impl LeaderNameCatalogHandle {
    pub fn new(catalog: Arc<LeaderNameCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<LeaderNameCatalog> {
        Arc::clone(&self.0)
    }

    pub fn catalog(&self) -> &LeaderNameCatalog {
        &self.0
    }
}

impl Default for LeaderNameCatalogHandle {
    fn default() -> Self {
        Self(LeaderNameCatalog::builtin())
    }
}

/// Metadata about the leader name catalog source.
#[derive(Resource, Debug, Clone, Default)]
pub struct LeaderNameCatalogMetadata {
    path: Option<PathBuf>,
}

impl LeaderNameCatalogMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the leader name catalog from the environment (`LEADER_NAMES_PATH`) or the default data
/// path. Only an absent *default* path falls back to the builtin; see
/// [`crate::config_load::resolve_config`].
pub fn load_leader_names_from_env() -> (Arc<LeaderNameCatalog>, LeaderNameCatalogMetadata) {
    let (catalog, source) = load_config_from_env(
        "LEADER_NAMES_PATH",
        "leader_names",
        "src/data/leader_names.json",
        LeaderNameCatalog::builtin,
        LeaderNameCatalog::from_file,
    );
    (catalog, LeaderNameCatalogMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const TEST_MAP_SEED: u64 = 0x1234_5678_9ABC_DEF0;

    #[test]
    fn builtin_catalog_loads_and_is_distinct() {
        let catalog = LeaderNameCatalog::builtin();
        assert!(!catalog.is_empty());
        let unique: HashSet<&str> = catalog.names().iter().map(String::as_str).collect();
        assert_eq!(
            unique.len(),
            catalog.len(),
            "the shipped pool repeats a name"
        );
        for name in catalog.names() {
            assert!(!name.is_empty());
            assert!(
                !name.chars().any(|c| c.is_ascii_digit()),
                "{name:?} holds a digit, which would collide with a decimal cycle suffix"
            );
        }
    }

    #[test]
    fn duplicate_and_empty_pools_are_refused() {
        assert!(matches!(
            LeaderNameCatalog::from_json_str(r#"{"names": ["Orrin", "Hale", "Orrin"]}"#),
            Err(LeaderNamesError::Duplicate(name)) if name == "Orrin"
        ));
        assert!(matches!(
            LeaderNameCatalog::from_json_str(r#"{"names": []}"#),
            Err(LeaderNamesError::Empty)
        ));
    }

    #[test]
    fn a_band_resolves_the_same_name_every_time_and_bands_differ() {
        let catalog = LeaderNameCatalog::builtin();
        let first = catalog.name_for_band(TEST_MAP_SEED, BandId(7));
        assert_eq!(first, catalog.name_for_band(TEST_MAP_SEED, BandId(7)));
        let names: HashSet<String> = (0..catalog.len() as u64)
            .map(|band| catalog.name_for_band(TEST_MAP_SEED, BandId(band)))
            .collect();
        assert_eq!(
            names.len(),
            catalog.len(),
            "the first pass over the pool is a permutation"
        );
    }

    #[test]
    fn a_different_map_seed_permutes_differently() {
        let catalog = LeaderNameCatalog::builtin();
        let a: Vec<String> = (0..catalog.len() as u64)
            .map(|band| catalog.name_for_band(TEST_MAP_SEED, BandId(band)))
            .collect();
        let b: Vec<String> = (0..catalog.len() as u64)
            .map(|band| catalog.name_for_band(TEST_MAP_SEED ^ 1, BandId(band)))
            .collect();
        assert_ne!(a, b);
    }
}
