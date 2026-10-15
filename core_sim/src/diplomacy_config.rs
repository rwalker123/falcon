//! Tuning for early diplomacy — offers, goodwill, treaties (`docs/plan_early_diplomacy.md` §5).
//!
//! Loaded from `data/diplomacy_config.json`, on the shared boot seam (`crate::config_load`): an
//! absent default path falls back to the builtin, anything else that is there and wrong is a boot
//! panic. Env override `DIPLOMACY_CONFIG_PATH`. No hot reload.
//!
//! **Every §5 key is loaded and validated now**, though the offer slice reads only
//! the two `offers.*` keys: the goodwill, roads and ai blocks belong to the slices that follow,
//! and a file that is wrong should stop the boot on the day it is edited rather than the day a
//! later slice first reads it. Every key is required — a missing one is a parse failure, never a
//! number nobody chose — and every table is closed (`deny_unknown_fields`); only the root accepts a
//! `_comment`.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::{intensification::RungKey, routes::FIRST_BUILT_RUNG};
use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_DIPLOMACY_CONFIG: &str = include_str!("data/diplomacy_config.json");

/// The lowest `offers.offer_wait_turns` that means anything: an offer that expired the turn it was
/// posted would be refused before its receiver could answer.
const MIN_OFFER_WAIT_TURNS: u32 = 1;
/// Goodwill runs `-1..=+1`; the word cuts and the AI's treaty line are measured on that scale.
const GOODWILL_FLOOR: f32 = -1.0;
const GOODWILL_CEILING: f32 = 1.0;

/// Root configuration for early diplomacy.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiplomacyConfig {
    #[serde(default, rename = "_comment")]
    _comment: Option<String>,
    pub offers: OffersConfig,
    pub goodwill: GoodwillConfig,
    pub roads: DiplomacyRoadsConfig,
    pub ai: DiplomacyAiConfig,
}

/// How long a pending dealing waits.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OffersConfig {
    /// Turns a pending dealing waits for an answer before silence refuses it.
    pub offer_wait_turns: u32,
    /// The lowest built road rung (a route-ladder wire key such as `route:dirt_road`) every tile
    /// between two camps must carry, keeping met, for the link between them to stand
    /// ([`crate::routes::standing_link`]). Validated to name a rung at or above the first built one.
    pub standing_link_min_rung: String,
}

impl OffersConfig {
    /// The rung `standing_link_min_rung` names. Validation guarantees it resolves.
    pub fn standing_link_floor(&self) -> RungKey {
        RungKey::from_wire_key(&self.standing_link_min_rung)
            .expect("validated: standing_link_min_rung names a route rung")
    }
}

/// What one people thinks of another (read by the goodwill slice).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodwillConfig {
    /// Proportional fade toward zero each turn, a fraction in `0..=1`.
    pub fade_per_turn: f32,
    /// Goodwill per unit of worth, for gifts and exchanges.
    pub per_load: f32,
    /// Goodwill per stationed person per turn.
    pub per_person_stationed: f32,
    /// Goodwill per standing link per turn.
    pub per_link: f32,
    /// Goodwill per treaty term in force per turn.
    pub per_term: f32,
    /// The drop a breach costs, a fraction in `0..=1`.
    pub broken_treaty: f32,
    /// Goodwill below this reads *wary*.
    pub wary_below: f32,
    /// Goodwill above this reads *friendly*.
    pub friendly_above: f32,
}

/// What a foreigner's walk over a built road is worth (read by the treaty slice).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiplomacyRoadsConfig {
    /// A foreigner's share of a built road's payoff, a fraction in `0..=1`.
    pub foreign_road_share: f32,
}

/// How an AI people answers (read by the AI slice).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiplomacyAiConfig {
    /// The goodwill an AI needs toward a proposer to accept a treaty, on the `-1..=+1` scale.
    pub ai_treaty_goodwill: f32,
}

impl DiplomacyConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_DIPLOMACY_CONFIG)
                .expect("builtin diplomacy config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, DiplomacyConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, DiplomacyConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| DiplomacyConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&contents)
    }

    fn validate(&self) -> Result<(), DiplomacyConfigError> {
        if self.offers.offer_wait_turns < MIN_OFFER_WAIT_TURNS {
            return Err(DiplomacyConfigError::Invalid {
                field: "offers.offer_wait_turns".to_string(),
                reason: format!("must be at least {MIN_OFFER_WAIT_TURNS}"),
            });
        }
        match RungKey::from_wire_key(&self.offers.standing_link_min_rung) {
            Some(rung) if rung.is_at_or_above(FIRST_BUILT_RUNG) => {}
            _ => {
                return Err(DiplomacyConfigError::Invalid {
                    field: "offers.standing_link_min_rung".to_string(),
                    reason: format!(
                        "must name a built route rung (a wire key at or above {}; was {:?})",
                        FIRST_BUILT_RUNG.wire_key(),
                        self.offers.standing_link_min_rung
                    ),
                });
            }
        }
        let goodwill = &self.goodwill;
        for (field, value) in [
            ("goodwill.fade_per_turn", goodwill.fade_per_turn),
            ("goodwill.broken_treaty", goodwill.broken_treaty),
            ("roads.foreign_road_share", self.roads.foreign_road_share),
        ] {
            require_fraction(field, value)?;
        }
        for (field, value) in [
            ("goodwill.per_load", goodwill.per_load),
            (
                "goodwill.per_person_stationed",
                goodwill.per_person_stationed,
            ),
            ("goodwill.per_link", goodwill.per_link),
            ("goodwill.per_term", goodwill.per_term),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(DiplomacyConfigError::Invalid {
                    field: field.to_string(),
                    reason: format!("must be finite and non-negative (was {value})"),
                });
            }
        }
        for (field, value) in [
            ("goodwill.wary_below", goodwill.wary_below),
            ("goodwill.friendly_above", goodwill.friendly_above),
            ("ai.ai_treaty_goodwill", self.ai.ai_treaty_goodwill),
        ] {
            if !value.is_finite() || !(GOODWILL_FLOOR..=GOODWILL_CEILING).contains(&value) {
                return Err(DiplomacyConfigError::Invalid {
                    field: field.to_string(),
                    reason: format!(
                        "must be finite and within {GOODWILL_FLOOR}..={GOODWILL_CEILING} \
                         (was {value})"
                    ),
                });
            }
        }
        if goodwill.wary_below > goodwill.friendly_above {
            return Err(DiplomacyConfigError::Invalid {
                field: "goodwill.wary_below".to_string(),
                reason: format!(
                    "must not exceed goodwill.friendly_above ({} > {})",
                    goodwill.wary_below, goodwill.friendly_above
                ),
            });
        }
        Ok(())
    }
}

fn require_fraction(field: &str, value: f32) -> Result<(), DiplomacyConfigError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(DiplomacyConfigError::Invalid {
            field: field.to_string(),
            reason: format!("must be a finite fraction within 0..=1 (was {value})"),
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum DiplomacyConfigError {
    #[error("failed to read diplomacy config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse diplomacy config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid diplomacy config: {field} {reason}")]
    Invalid { field: String, reason: String },
}

impl ConfigLoadError for DiplomacyConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the diplomacy configuration.
#[derive(Resource, Debug, Clone)]
pub struct DiplomacyConfigHandle(pub Arc<DiplomacyConfig>);

impl DiplomacyConfigHandle {
    pub fn new(config: Arc<DiplomacyConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<DiplomacyConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<DiplomacyConfig>) {
        self.0 = config;
    }
}

impl Default for DiplomacyConfigHandle {
    fn default() -> Self {
        Self(DiplomacyConfig::builtin())
    }
}

/// Metadata about the diplomacy configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct DiplomacyConfigMetadata {
    path: Option<PathBuf>,
}

impl DiplomacyConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load the diplomacy config from environment (`DIPLOMACY_CONFIG_PATH`) or the default data path.
/// Only an absent *default* path falls back to the builtin; see
/// [`crate::config_load::resolve_config`] for the rule and what it panics on.
pub fn load_diplomacy_config_from_env() -> (Arc<DiplomacyConfig>, DiplomacyConfigMetadata) {
    let (config, source) = load_config_from_env(
        "DIPLOMACY_CONFIG_PATH",
        "diplomacy_config",
        "src/data/diplomacy_config.json",
        DiplomacyConfig::builtin,
        DiplomacyConfig::from_file,
    );
    (config, DiplomacyConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(edit: impl FnOnce(&mut serde_json::Value)) -> Result<DiplomacyConfig, String> {
        let mut value: serde_json::Value =
            serde_json::from_str(BUILTIN_DIPLOMACY_CONFIG).expect("the builtin is JSON");
        edit(&mut value);
        DiplomacyConfig::from_json_str(&value.to_string()).map_err(|err| err.to_string())
    }

    #[test]
    fn the_builtin_parses_with_the_spec_opening_values() {
        let config = DiplomacyConfig::builtin();
        assert_eq!(config.offers.offer_wait_turns, 1);
        assert_eq!(config.offers.standing_link_floor(), FIRST_BUILT_RUNG);
        assert_eq!(config.goodwill.fade_per_turn, 0.02);
        assert_eq!(config.goodwill.broken_treaty, 0.5);
        assert_eq!(config.goodwill.wary_below, -0.2);
        assert_eq!(config.goodwill.friendly_above, 0.2);
        assert_eq!(config.roads.foreign_road_share, 0.9);
        assert_eq!(config.ai.ai_treaty_goodwill, 0.2);
    }

    #[test]
    fn a_missing_key_is_a_parse_failure_not_a_default() {
        let err = with(|v| {
            v["goodwill"].as_object_mut().unwrap().remove("per_link");
        })
        .expect_err("a missing lever must not parse");
        assert!(err.contains("per_link"), "{err}");
    }

    #[test]
    fn an_unknown_key_is_refused() {
        assert!(with(|v| {
            v["offers"]["offer_wait_turn"] = serde_json::json!(2);
        })
        .is_err());
    }

    #[test]
    fn the_standing_link_floor_must_name_a_built_route_rung() {
        for bad in ["route:trail", "plant:tended", "route:nonsense", ""] {
            let err = with(|v| v["offers"]["standing_link_min_rung"] = serde_json::json!(bad))
                .expect_err("a floor that is no built route rung must not load");
            assert!(err.contains("offers.standing_link_min_rung"), "{err}");
        }
        let paved =
            with(|v| v["offers"]["standing_link_min_rung"] = serde_json::json!("route:paved_road"))
                .expect("a higher built rung is a legal floor");
        assert_eq!(paved.offers.standing_link_floor(), RungKey::RoutePavedRoad);
    }

    #[test]
    fn an_offer_that_expires_before_it_can_be_answered_is_refused() {
        let err = with(|v| v["offers"]["offer_wait_turns"] = serde_json::json!(0))
            .expect_err("a zero wait refuses every offer unseen");
        assert!(err.contains("offers.offer_wait_turns"), "{err}");
    }

    #[test]
    fn a_fraction_outside_zero_to_one_is_refused() {
        for (block, key) in [
            ("goodwill", "fade_per_turn"),
            ("goodwill", "broken_treaty"),
            ("roads", "foreign_road_share"),
        ] {
            assert!(
                with(|v| v[block][key] = serde_json::json!(1.5)).is_err(),
                "{block}.{key}"
            );
            assert!(
                with(|v| v[block][key] = serde_json::json!(-0.1)).is_err(),
                "{block}.{key}"
            );
        }
    }

    #[test]
    fn the_word_cuts_must_stay_on_the_goodwill_scale_and_in_order() {
        assert!(with(|v| v["goodwill"]["wary_below"] = serde_json::json!(-2.0)).is_err());
        assert!(with(|v| v["ai"]["ai_treaty_goodwill"] = serde_json::json!(1.5)).is_err());
        assert!(with(|v| v["goodwill"]["wary_below"] = serde_json::json!(0.5)).is_err());
    }

    #[test]
    fn a_negative_credit_is_refused() {
        assert!(with(|v| v["goodwill"]["per_load"] = serde_json::json!(-0.01)).is_err());
    }
}
