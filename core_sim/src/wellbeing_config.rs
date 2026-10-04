//! Data-driven tuning for the **Civilization Wellbeing** subsystem (`docs/plan_civ_wellbeing.md`).
//!
//! Loaded from `data/wellbeing_config.json`. Wellbeing is the three-layer spine
//! **factors → morale → discontent → consequences**:
//! - `discontent` — how morale maps to the share of a band that is unhappy (working-weighted),
//!   plus the `grievance` accumulator (severity × duration, reserved for a future revolution
//!   consequence — Phase 1 only feeds it).
//! - `productivity` — the discontent entry of the output **modifier stack** (`output = base ×
//!   Π(modifiers)`); future education/tech/government modifiers slot in alongside it.
//! - `migration` — tech-gated relocation: discontented people move to a better reachable
//!   same-faction band or stay (population conserved within the faction).
//!
//! Mirrors the `demographics_config.rs` / `sedentarization_config.rs` loader (baked-in builtin +
//! optional file/env override).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_WELLBEING_CONFIG: &str = include_str!("data/wellbeing_config.json");

/// Layer 2 — discontent tuning. `discontent_fraction = clamp((content_morale − morale) /
/// (content_morale − floor_morale), 0, 1)`: 0 at/above `content_morale`, rising to 1.0 at/below
/// `floor_morale`. This drives **productivity only** (0.6 onset). The `grievance` accumulator gains
/// `grievance_gain × discontent_fraction` per turn (× `trapped_multiplier` when the band is *trapped*
/// — below the migration threshold with no reachable destination) and decays by `grievance_decay`
/// while content. Its one consequence is independence: a cut-off group whose people-weighted grievance
/// reaches `independence.grievance_threshold` breaks away (`systems::independence`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DiscontentConfig {
    pub content_morale: f32,
    pub floor_morale: f32,
    pub grievance_gain: f32,
    pub grievance_decay: f32,
    pub trapped_multiplier: f32,
}

impl Default for DiscontentConfig {
    fn default() -> Self {
        Self {
            content_morale: 0.6,
            floor_morale: 0.1,
            grievance_gain: 0.05,
            grievance_decay: 0.1,
            trapped_multiplier: 1.5,
        }
    }
}

/// Layer 3a — productivity modifier stack tuning. The discontent modifier is
/// `max(floor_mult, 1 − discontent_fraction × discontent_weight)`; `floor_mult` is the worst-case
/// output a fully-discontented band still produces (people work, just poorly — morale never
/// zeroes output).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ProductivityConfig {
    pub floor_mult: f32,
    pub discontent_weight: f32,
}

impl Default for ProductivityConfig {
    fn default() -> Self {
        Self {
            floor_mult: 0.5,
            discontent_weight: 1.0,
        }
    }
}

/// Layer 3b — migration tuning. **Decoupled from `discontent_fraction`** (which is productivity-only):
/// migration has its own morale-scaled onset at `morale_threshold` (0.25). Each turn the band sheds
/// `size × move_fraction` people, where
/// `move_fraction = max_rate × clamp((morale_threshold − morale) / morale_threshold, 0, 1)` — 0 at
/// morale ≥ `morale_threshold`, ramping to `max_rate` at rock-bottom morale (e.g. 0.075 at 0.125,
/// 0.15 at 0). Leavers are composed mostly of working-age: the total is split across brackets
/// proportional to `bracket_size × weight` (working = 1.0, dependents = `dependent_weight` 0.4), so
/// the headline fraction stays exact while workers dominate. They seek the highest-morale eligible
/// band within reach — their own people's first — where reach is `base_reach` hex steps less the road
/// bonus between the two camps (`advance_population_migration`; the work party's walk seam). Eligible
/// = `morale ≥ attractive_morale` AND
/// `morale > source_morale + min_morale_gap`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MigrationConfig {
    pub morale_threshold: f32,
    pub max_rate: f32,
    pub base_reach: f32,
    pub attractive_morale: f32,
    pub min_morale_gap: f32,
    pub dependent_weight: f32,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            morale_threshold: 0.25,
            max_rate: 0.15,
            base_reach: 4.0,
            attractive_morale: 0.5,
            min_morale_gap: 0.05,
            dependent_weight: 0.4,
        }
    }
}

/// **Independence** — when a cut-off group of a people's bands becomes a people of its own
/// (`docs/plan_band_fission.md` §Independence, `systems::independence`).
///
/// The clock is the contact tie's own bleed (`connections_config.json` → `strength.*`) and the
/// pressure is the discontent block's `grievance`; this block adds the ONE number that joins them:
/// the people-weighted mean grievance at which a group no longer tied to its people's heart breaks
/// away.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IndependenceConfig {
    /// People-weighted mean `grievance` at which a cut-off group becomes its own people. At the
    /// shipped `discontent` dials a mildly unhappy far band takes ~40 turns to reach `1.0`, a starving
    /// trapped one ~17, and a content one never does. Validated finite and non-negative; `0` means
    /// any cut-off group leaves the turn it is cut off.
    pub grievance_threshold: f32,
}

impl Default for IndependenceConfig {
    fn default() -> Self {
        Self {
            grievance_threshold: 1.0,
        }
    }
}

/// Root wellbeing configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WellbeingConfig {
    pub discontent: DiscontentConfig,
    pub productivity: ProductivityConfig,
    pub migration: MigrationConfig,
    pub independence: IndependenceConfig,
}

impl WellbeingConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_WELLBEING_CONFIG)
                .expect("builtin wellbeing config should parse"),
        )
    }

    /// Parse **and validate** — every load path (boot, the staged tuning override) funnels through
    /// here, so a value the sim cannot honour is refused at the edit rather than read in play.
    pub fn from_json_str(json: &str) -> Result<Self, WellbeingConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    /// The invariants a parsed config must hold. Only the independence threshold is checked here:
    /// it is the one lever a negative or non-finite value would turn into a nonsense comparison
    /// (every band's grievance is `>= 0`, so a negative threshold would break every cut-off group
    /// away on its first turn while reading like a strict setting).
    pub fn validate(&self) -> Result<(), WellbeingConfigError> {
        let threshold = self.independence.grievance_threshold;
        if !threshold.is_finite() || threshold < 0.0 {
            return Err(WellbeingConfigError::Invalid(format!(
                "independence.grievance_threshold must be finite and >= 0, got {threshold}"
            )));
        }
        Ok(())
    }

    pub fn from_file(path: &Path) -> Result<Self, WellbeingConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| WellbeingConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        WellbeingConfig::from_json_str(&contents)
    }
}

#[derive(Debug, Error)]
pub enum WellbeingConfigError {
    #[error("failed to read wellbeing config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse wellbeing config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid wellbeing config: {0}")]
    Invalid(String),
}

impl ConfigLoadError for WellbeingConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the wellbeing configuration.
#[derive(Resource, Debug, Clone)]
pub struct WellbeingConfigHandle(pub Arc<WellbeingConfig>);

impl WellbeingConfigHandle {
    pub fn new(config: Arc<WellbeingConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<WellbeingConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<WellbeingConfig>) {
        self.0 = config;
    }
}

impl Default for WellbeingConfigHandle {
    fn default() -> Self {
        Self(WellbeingConfig::builtin())
    }
}

/// Metadata about the wellbeing configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct WellbeingConfigMetadata {
    path: Option<PathBuf>,
}

impl WellbeingConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load wellbeing config from environment (`WELLBEING_CONFIG_PATH`) or the default data path.
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `WELLBEING_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_wellbeing_config_from_env() -> (Arc<WellbeingConfig>, WellbeingConfigMetadata) {
    let (config, source) = load_config_from_env(
        "WELLBEING_CONFIG_PATH",
        "wellbeing_config",
        "src/data/wellbeing_config.json",
        WellbeingConfig::builtin,
        WellbeingConfig::from_file,
    );
    (config, WellbeingConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_sane() {
        let config = WellbeingConfig::builtin();
        let d = &config.discontent;
        assert!(
            d.content_morale > d.floor_morale,
            "content_morale must exceed floor_morale for a valid discontent span"
        );
        assert!(d.grievance_gain >= 0.0 && d.grievance_decay >= 0.0);
        assert!(d.trapped_multiplier >= 1.0);
        let p = &config.productivity;
        assert!(
            (0.0..=1.0).contains(&p.floor_mult),
            "floor_mult is a multiplier in [0, 1]"
        );
        assert!(p.discontent_weight >= 0.0);
        let m = &config.migration;
        assert!(m.max_rate >= 0.0 && m.base_reach >= 0.0);
        assert!((0.0..=1.0).contains(&m.morale_threshold));
        assert!((0.0..=1.0).contains(&m.dependent_weight));
        assert!((0.0..=1.0).contains(&m.attractive_morale));
        assert!(config.independence.grievance_threshold >= 0.0);
    }

    /// A negative or non-finite independence threshold is refused at parse, never read in play.
    #[test]
    fn a_negative_independence_threshold_is_rejected() {
        let rejected =
            WellbeingConfig::from_json_str(r#"{"independence": {"grievance_threshold": -0.5}}"#);
        assert!(matches!(rejected, Err(WellbeingConfigError::Invalid(_))));
        let accepted =
            WellbeingConfig::from_json_str(r#"{"independence": {"grievance_threshold": 0.0}}"#)
                .expect("zero is a real setting: leave the turn you are cut off");
        assert_eq!(accepted.independence.grievance_threshold, 0.0);
    }

    /// An absent block takes the shipped default, like every other wellbeing block.
    #[test]
    fn an_absent_independence_block_takes_the_default() {
        let config = WellbeingConfig::from_json_str("{}").expect("an empty object parses");
        assert_eq!(
            config.independence.grievance_threshold,
            IndependenceConfig::default().grievance_threshold
        );
    }
}
