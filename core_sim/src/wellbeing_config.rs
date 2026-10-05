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
//! - `culture` — the Layer-1 "near / far from the ancestors" morale term, read off the band's belief
//!   anchor (`.claude/rules/core_sim/belief.md` → "The culture morale term").
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
/// while content — reserved for a future revolution consequence; Phase 1 only populates it.
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
/// bonus between the two camps (`supply::WalkReach`, also the culture term's reach). Eligible
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

/// Layer 1 — the **culture** morale term: near / far from the ancestors
/// (`docs/plan_civilization_steps.md` §"What belief does, through seams that exist"). With `b` the
/// belief on the band's anchor tile and `s = b / (b + belief_half_saturation)` its saturating weight,
/// the term is `+near_bonus × s` while the band stands within walking reach of the anchor
/// (`supply::WalkReach`, the migration reach) and `−away_drag × s` beyond it; `0` with no anchor.
/// In or out of reach is binary — the drag does not grow with distance. All three are PLAYTEST
/// DIALs.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultureConfig {
    /// Morale per turn a band gains standing within reach of a saturated anchor.
    pub near_bonus: f32,
    /// Morale per turn a band loses standing beyond reach of a saturated anchor.
    pub away_drag: f32,
    /// Belief (dead-equivalents) at which the anchor's weight is one half.
    pub belief_half_saturation: f32,
    /// The least belief (dead-equivalents) a place must hold before a band adopts it as its anchor —
    /// one whole death's worth at `1.0`. Gates adoption only; belief still accrues fractional deaths.
    pub min_anchor_belief: f32,
    /// How much of the reach each hop of kin relays — a band of the same people within walking reach
    /// of a near band is near at this strength, and each further hop multiplies by it again
    /// (`crate::belief_relay`). In `[0, 1]`; `0` turns relaying off.
    pub relay_per_hop: f32,
}

impl Default for CultureConfig {
    fn default() -> Self {
        Self {
            near_bonus: 0.01,
            away_drag: 0.02,
            belief_half_saturation: 10.0,
            min_anchor_belief: 1.0,
            relay_per_hop: 0.5,
        }
    }
}

impl CultureConfig {
    /// The anchor's saturating weight `s = b / (b + belief_half_saturation)`, in `[0, 1)`: a single
    /// death's worth barely registers and a great cemetery approaches the full term.
    pub fn anchor_weight(&self, belief: f32) -> f32 {
        belief / (belief + self.belief_half_saturation)
    }

    /// `near_bonus`, `away_drag` and `min_anchor_belief` must be finite and non-negative,
    /// `relay_per_hop` finite and in `[0, 1]`; `belief_half_saturation` must be
    /// finite and `> 0` (it is the weight's denominator at zero belief).
    pub fn validate(&self) -> Result<(), WellbeingConfigError> {
        require_non_negative_finite("culture.near_bonus", self.near_bonus)?;
        require_non_negative_finite("culture.away_drag", self.away_drag)?;
        require_non_negative_finite("culture.min_anchor_belief", self.min_anchor_belief)?;
        require_non_negative_finite("culture.relay_per_hop", self.relay_per_hop)?;
        if self.relay_per_hop > MAX_RELAY_PER_HOP {
            return Err(WellbeingConfigError::Invalid {
                field: "culture.relay_per_hop",
                constraint: "be at most 1",
                value: self.relay_per_hop.to_string(),
            });
        }
        if !self.belief_half_saturation.is_finite() || self.belief_half_saturation <= 0.0 {
            return Err(WellbeingConfigError::Invalid {
                field: "culture.belief_half_saturation",
                constraint: "be finite and greater than 0",
                value: self.belief_half_saturation.to_string(),
            });
        }
        Ok(())
    }
}

/// The most a hop of kin can relay — a full-strength relay; above it a far band would be nearer than
/// a direct one.
const MAX_RELAY_PER_HOP: f32 = 1.0;

fn require_non_negative_finite(
    field: &'static str,
    value: f32,
) -> Result<(), WellbeingConfigError> {
    if !value.is_finite() || value < 0.0 {
        return Err(WellbeingConfigError::Invalid {
            field,
            constraint: "be finite and at least 0",
            value: value.to_string(),
        });
    }
    Ok(())
}

/// Root wellbeing configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WellbeingConfig {
    pub discontent: DiscontentConfig,
    pub productivity: ProductivityConfig,
    pub migration: MigrationConfig,
    pub culture: CultureConfig,
}

impl WellbeingConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_WELLBEING_CONFIG)
                .expect("builtin wellbeing config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, WellbeingConfigError> {
        let config: WellbeingConfig = serde_json::from_str(json)?;
        config.culture.validate()?;
        Ok(config)
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
    #[error("wellbeing config `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: &'static str,
        value: String,
    },
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
        assert!(config.culture.validate().is_ok());
    }

    #[test]
    fn the_shipped_culture_levers_are_the_documented_defaults() {
        let shipped = &WellbeingConfig::builtin().culture;
        let default = CultureConfig::default();
        assert_eq!(shipped.near_bonus, default.near_bonus);
        assert_eq!(shipped.away_drag, default.away_drag);
        assert_eq!(
            shipped.belief_half_saturation,
            default.belief_half_saturation
        );
        assert_eq!(shipped.min_anchor_belief, default.min_anchor_belief);
        assert_eq!(shipped.relay_per_hop, default.relay_per_hop);
    }

    /// Belief equal to the half-saturation lever weighs exactly one half.
    #[test]
    fn the_anchor_weight_is_one_half_at_the_half_saturation_belief() {
        const ONE_HALF: f32 = 0.5;
        let culture = CultureConfig::default();
        assert_eq!(
            culture.anchor_weight(culture.belief_half_saturation),
            ONE_HALF
        );
    }

    #[test]
    fn a_non_positive_half_saturation_is_refused() {
        let json = r#"{ "culture": { "belief_half_saturation": 0.0 } }"#;
        assert!(matches!(
            WellbeingConfig::from_json_str(json),
            Err(WellbeingConfigError::Invalid {
                field: "culture.belief_half_saturation",
                ..
            })
        ));
    }

    #[test]
    fn a_relay_per_hop_above_one_is_refused() {
        let json = r#"{ "culture": { "relay_per_hop": 1.5 } }"#;
        assert!(matches!(
            WellbeingConfig::from_json_str(json),
            Err(WellbeingConfigError::Invalid {
                field: "culture.relay_per_hop",
                ..
            })
        ));
    }

    #[test]
    fn a_negative_min_anchor_belief_is_refused() {
        let json = r#"{ "culture": { "min_anchor_belief": -1.0 } }"#;
        assert!(matches!(
            WellbeingConfig::from_json_str(json),
            Err(WellbeingConfigError::Invalid {
                field: "culture.min_anchor_belief",
                ..
            })
        ));
    }

    #[test]
    fn a_negative_culture_lever_is_refused() {
        let json = r#"{ "culture": { "away_drag": -0.01 } }"#;
        assert!(matches!(
            WellbeingConfig::from_json_str(json),
            Err(WellbeingConfigError::Invalid {
                field: "culture.away_drag",
                ..
            })
        ));
    }
}
