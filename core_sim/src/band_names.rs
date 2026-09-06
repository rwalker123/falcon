//! The pool a band's name is drawn from, and the per-faction permutation that turns a slot number
//! into one of those names.
//!
//! **A band's name is identity, and the sim owns it.** Before this module a band had no name at
//! all and the client fabricated one from a row number, which meant two screens counting different
//! rows disagreed about what a band was called, and a band dying renamed every band after it. The
//! cure is that the name is minted once at founding, published on the wire, and carried through the
//! checkpoint — never derived from a position in a list.
//!
//! Loaded from `data/band_names.json` on the shared boot seam ([`crate::config_load`]): an absent
//! default path falls back to the builtin, anything else that is there and wrong is a boot panic.
//! Mirrors `connections_config.rs`, the smallest config on that seam.
//!
//! **The list is CONTENT, not tuning** — curated words, not a syllable-masher — so its one
//! structural rule is the one the uniqueness guarantee rests on: [`BandNameCatalog::from_json_str`]
//! rejects a duplicate entry rather than silently deduplicating it. See
//! [`crate::resources::BandNameAllocator`] for how a slot becomes a name.

use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::hashing::splitmix64;
use crate::orders::FactionId;

pub const BUILTIN_BAND_NAMES: &str = include_str!("data/band_names.json");

/// XOR sub-seed salt for a faction's band-name permutation
/// (`map_seed ^ BAND_NAME_SALT ^ faction`), following the repo's domain-subseed convention (cf.
/// `PALETTE_SEED_SALT`, `HERD_MOVEMENT_SEED_SALT`). A fixed, arbitrary non-zero constant so band
/// naming cannot correlate with any other draw made from the same map seed.
pub const BAND_NAME_SALT: u64 = 0xB4ED_4A3E_5C07_11F5;

/// The largest number the roman-numeral suffix can spell. Above it a cycle falls back to a decimal
/// suffix — see [`cycle_suffix`].
const ROMAN_MAX: u32 = 3999;

/// The subtractive roman-numeral table, largest value first. Every value in `1..=ROMAN_MAX` is
/// spellable by greedily consuming this table.
const ROMAN_TABLE: [(u32, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

/// The curated pool of band names, in file order.
///
/// **File order is an index space, not a ranking.** The allocator shuffles `0..len` per faction
/// from the map seed, so nothing about the order is player-visible; what it does mean is that
/// editing the list changes which name a given slot resolves to, and a world generated before the
/// edit reads differently after it.
#[derive(Resource, Debug, Clone, Deserialize)]
pub struct BandNameCatalog {
    names: Vec<String>,
}

impl BandNameCatalog {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_BAND_NAMES).expect("builtin band names should be valid"),
        )
    }

    /// Parse **and validate**. A catalog that is empty, or that repeats a name, is refused: the
    /// "unique within a faction" guarantee is `(cycle, permutation[k % len])` being injective in
    /// `k`, and that is only a guarantee about *names* if no two indices spell the same word.
    /// Silently deduplicating would make a list that looks like it holds 200 names hand out 199,
    /// with the collision invisible until two living bands shared one.
    pub fn from_json_str(json: &str) -> Result<Self, BandNamesError> {
        let catalog: Self = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn from_file(path: &Path) -> Result<Self, BandNamesError> {
        let contents = fs::read_to_string(path).map_err(|source| BandNamesError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    fn validate(&self) -> Result<(), BandNamesError> {
        if self.names.is_empty() {
            return Err(BandNamesError::Empty);
        }
        let mut seen: HashSet<&str> = HashSet::with_capacity(self.names.len());
        for name in &self.names {
            if !seen.insert(name.as_str()) {
                return Err(BandNamesError::Duplicate(name.clone()));
            }
        }
        Ok(())
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

    /// Resolve a faction's `slot`th name.
    ///
    /// **Deterministic**: the permutation is `SmallRng::seed_from_u64(splitmix64(map_seed ^
    /// BAND_NAME_SALT ^ faction))` shuffled over `0..len`, so the same map seed, faction and
    /// founding order always spell the same names — a save, a reload and a replay agree.
    ///
    /// **Injective in `slot`**: `cycle = slot / len` and `permutation[slot % len]` together
    /// recover `slot`, so two different slots of one faction never resolve to the same string. Past
    /// the end of the pool the name repeats with a cycle suffix (`Ashfell II`), which is why no
    /// probing and no collision check is needed anywhere.
    ///
    /// The permutation is rebuilt per call rather than cached: minting happens a handful of times
    /// per game, and a cache would be checkpoint state earning nothing.
    pub fn name_for_slot(&self, faction: FactionId, map_seed: u64, slot: u32) -> String {
        // Only reachable from a hand-built catalog: the loader refuses an empty list. An empty name
        // is what the client renders as its `Band #<id>` fallback, which beats dividing by zero.
        if self.names.is_empty() {
            return String::new();
        }
        let len = self.names.len();
        let mut order: Vec<usize> = (0..len).collect();
        let seed = splitmix64(map_seed ^ BAND_NAME_SALT ^ u64::from(faction.0));
        order.shuffle(&mut SmallRng::seed_from_u64(seed));

        let slot = slot as usize;
        let cycle = slot / len;
        let name = &self.names[order[slot % len]];
        match cycle_suffix(cycle) {
            Some(suffix) => format!("{name} {suffix}"),
            None => name.clone(),
        }
    }
}

/// The suffix that distinguishes the `cycle`th pass over the pool. `None` for the first pass — the
/// common case is a bare name.
///
/// A cycle is spelled in roman numerals up to [`ROMAN_MAX`] and in decimal above it. Both remain
/// injective in `cycle` and neither can collide with a bare catalog entry, because a catalog name
/// carries no digits and no trailing roman group.
fn cycle_suffix(cycle: usize) -> Option<String> {
    if cycle == 0 {
        return None;
    }
    // The first repeat is `II`, not `I`: the bare name is the first band to hold it.
    let ordinal = u32::try_from(cycle.saturating_add(1)).unwrap_or(u32::MAX);
    Some(roman(ordinal).unwrap_or_else(|| ordinal.to_string()))
}

/// Spell `value` in roman numerals, or `None` outside `1..=ROMAN_MAX` (roman has no zero and no
/// standard spelling above 3999).
fn roman(value: u32) -> Option<String> {
    if value == 0 || value > ROMAN_MAX {
        return None;
    }
    let mut remaining = value;
    let mut out = String::new();
    for (amount, symbol) in ROMAN_TABLE {
        while remaining >= amount {
            out.push_str(symbol);
            remaining -= amount;
        }
    }
    Some(out)
}

#[derive(Debug, Error)]
pub enum BandNamesError {
    #[error("failed to read band names from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse band names: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("band name catalog is empty; every band would be founded nameless")]
    Empty,
    #[error("band name catalog repeats {0:?}; two bands of one faction would share a name")]
    Duplicate(String),
}

impl ConfigLoadError for BandNamesError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the band name catalog.
#[derive(Resource, Debug, Clone)]
pub struct BandNameCatalogHandle(pub Arc<BandNameCatalog>);

impl BandNameCatalogHandle {
    pub fn new(catalog: Arc<BandNameCatalog>) -> Self {
        Self(catalog)
    }

    pub fn get(&self) -> Arc<BandNameCatalog> {
        Arc::clone(&self.0)
    }

    pub fn catalog(&self) -> &BandNameCatalog {
        &self.0
    }
}

impl Default for BandNameCatalogHandle {
    fn default() -> Self {
        Self(BandNameCatalog::builtin())
    }
}

/// Metadata about the band name catalog source.
#[derive(Resource, Debug, Clone, Default)]
pub struct BandNameCatalogMetadata {
    path: Option<PathBuf>,
}

impl BandNameCatalogMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the band name catalog from the environment (`BAND_NAMES_PATH`) or the default data path.
/// Only an absent *default* path falls back to the builtin; see
/// [`crate::config_load::resolve_config`] for the rule and what it panics on.
pub fn load_band_names_from_env() -> (Arc<BandNameCatalog>, BandNameCatalogMetadata) {
    let (catalog, source) = load_config_from_env(
        "BAND_NAMES_PATH",
        "band_names",
        "src/data/band_names.json",
        BandNameCatalog::builtin,
        BandNameCatalog::from_file,
    );
    (catalog, BandNameCatalogMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MAP_SEED: u64 = 0x1234_5678_9ABC_DEF0;

    #[test]
    fn builtin_catalog_loads_and_is_distinct() {
        let catalog = BandNameCatalog::builtin();
        assert!(
            !catalog.is_empty(),
            "an empty pool would found every band nameless"
        );
        let unique: HashSet<&str> = catalog.names().iter().map(String::as_str).collect();
        assert_eq!(
            unique.len(),
            catalog.len(),
            "the shipped pool must hold no duplicate, or two bands of one faction share a name"
        );
        for name in catalog.names() {
            assert!(
                !name.is_empty(),
                "a blank entry publishes as the client's `Band #<id>` fallback"
            );
            assert!(
                !name.chars().any(|c| c.is_ascii_digit()),
                "{name:?} holds a digit, which would collide with a decimal cycle suffix"
            );
        }
    }

    #[test]
    fn duplicate_entry_is_refused_rather_than_deduplicated() {
        let err =
            BandNameCatalog::from_json_str(r#"{"names": ["Ashfell", "Brackwater", "Ashfell"]}"#)
                .expect_err("a repeated name must not load");
        assert!(
            matches!(&err, BandNamesError::Duplicate(name) if name == "Ashfell"),
            "expected a duplicate error naming the repeat, got {err}"
        );
    }

    #[test]
    fn empty_catalog_is_refused() {
        let err = BandNameCatalog::from_json_str(r#"{"names": []}"#)
            .expect_err("an empty pool must not load");
        assert!(matches!(err, BandNamesError::Empty), "got {err}");
    }

    #[test]
    fn roman_spells_the_cycle_suffixes() {
        assert_eq!(cycle_suffix(0), None, "the first pass takes the bare name");
        assert_eq!(cycle_suffix(1).as_deref(), Some("II"));
        assert_eq!(cycle_suffix(2).as_deref(), Some("III"));
        assert_eq!(cycle_suffix(3).as_deref(), Some("IV"));
        assert_eq!(roman(1).as_deref(), Some("I"));
        assert_eq!(roman(1994).as_deref(), Some("MCMXCIV"));
        assert_eq!(roman(ROMAN_MAX).as_deref(), Some("MMMCMXCIX"));
        assert_eq!(roman(0), None);
        assert_eq!(
            roman(ROMAN_MAX + 1),
            None,
            "above 3999 falls back to decimal"
        );
    }

    #[test]
    fn every_slot_of_the_first_cycle_is_a_distinct_catalog_entry() {
        let catalog = BandNameCatalog::builtin();
        let faction = FactionId(0);
        let names: HashSet<String> = (0..catalog.len() as u32)
            .map(|slot| catalog.name_for_slot(faction, TEST_MAP_SEED, slot))
            .collect();
        assert_eq!(
            names.len(),
            catalog.len(),
            "the first pass must be a permutation of the pool"
        );
        let pool: HashSet<&str> = catalog.names().iter().map(String::as_str).collect();
        for name in &names {
            assert!(pool.contains(name.as_str()), "{name:?} is not in the pool");
        }
    }

    #[test]
    fn a_different_map_seed_permutes_differently() {
        let catalog = BandNameCatalog::builtin();
        let faction = FactionId(0);
        let a: Vec<String> = (0..catalog.len() as u32)
            .map(|slot| catalog.name_for_slot(faction, TEST_MAP_SEED, slot))
            .collect();
        let b: Vec<String> = (0..catalog.len() as u32)
            .map(|slot| catalog.name_for_slot(faction, TEST_MAP_SEED ^ 1, slot))
            .collect();
        assert_ne!(a, b, "the permutation must depend on the map seed");
    }

    #[test]
    fn two_factions_permute_the_same_pool_differently() {
        let catalog = BandNameCatalog::builtin();
        let a: Vec<String> = (0..catalog.len() as u32)
            .map(|slot| catalog.name_for_slot(FactionId(0), TEST_MAP_SEED, slot))
            .collect();
        let b: Vec<String> = (0..catalog.len() as u32)
            .map(|slot| catalog.name_for_slot(FactionId(1), TEST_MAP_SEED, slot))
            .collect();
        assert_ne!(a, b, "a faction's permutation must depend on its id");
    }
}
