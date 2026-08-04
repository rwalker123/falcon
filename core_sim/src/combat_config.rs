//! Resolver tuning for the combat subsystem (`data/combat_config.json`).
//!
//! This file is **resolver tuning, not creature identity** (`docs/plan_predators.md`): the severity
//! constants [`crate::combat::resolve_fight`] reads. Creature stats live with their creature (animals
//! → [`crate::fauna_config::SpeciesDef`], humans → [`crate::creatures_config`]); this holds only the
//! knobs that shape *how a fight resolves*. Mirrors the `expedition_config.rs` loader convention
//! (baked-in builtin + `COMBAT_CONFIG_PATH` override + [`CombatConfig::validate`] inside
//! `from_json_str`, so a broken override is rejected at **error** level and the builtin used).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::combat::CombatTuning;
use crate::config_load::{load_config_from_env, ConfigLoadError};

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
    /// **The probability one unit's attack lands** — where the resolver's variance lives
    /// (`docs/plan_hunt_through_combat.md` §4.7), drawn per unit so it is *binomial in force size*.
    /// Ships **1.0**, which is an *exact identity*: no draw is made and no randomness consumed, so
    /// the take stays deterministic and the forecast's reported range is a **point**
    /// ([`Self::forecast_range_sigmas`]). Authoring a sub-1 chance is what makes the range real.
    /// Ships finite, `> 0` and `<= 1`.
    pub hit_chance: f32,
    /// **How much of its own `durability` a wounded body knits back per turn out of contact** — the
    /// decay half of [`crate::combat::DamageLedger`]. Ships **0.2** (five quiet turns clear any
    /// wound); finite, `> 0` and `<= 1`.
    pub wound_recovery_rate: f32,
    /// **Damage a hunt does to its own party per ANIMAL ENGAGED**, independent of what the quarry
    /// swings (`docs/plan_hunt_through_combat.md` §4.6) — hunters fall, are trampled in a drive, cut
    /// themselves butchering.
    ///
    /// **A lever, not a per-species field**: the danger is in the activity, not in the rabbit, so it
    /// scales with the *engagement* and lives here beside `expedition_danger_multiplier` — the other
    /// dial in this file that only the hunt adapter reads. Ships **0.15**, finite and `> 0`.
    pub hunt_injury_damage_per_animal: f32,
    /// **How wide the pre-commit forecast's reported range is**, in standard deviations of the take's
    /// own binomials (`docs/plan_hunt_through_combat.md` §6.4).
    ///
    /// A forecast has no event seed — [`crate::fauna::retreat_seed`] is `(map_seed, tick, herd,
    /// party)` and a projection cannot know a future tick — so it draws nothing and reads the
    /// distribution instead: the point estimate is the mean, the reported bounds are this many
    /// sigmas either side. Ships **2.0** (~95% of a normal-approximated binomial), which is the
    /// *"6–11, likely 9"* the design asks for.
    ///
    /// **It is a READOUT width, never a model term** — no resolution path reads it, so widening it
    /// cannot move a single animal. Finite and `> 0`: a `0` would report a point estimate as a
    /// certainty, which is exactly the promise this slice exists to stop making.
    pub forecast_range_sigmas: f32,
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
            hit_chance: self.hit_chance,
            wound_recovery_rate: self.wound_recovery_rate,
            // Config describes a **live** fight; a forecast substitutes its own draw mode at the
            // point of use (`fauna::HuntDraw`), never in the loaded tuning.
            draw: crate::combat::StrikeDraw::Seeded,
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
        // A probability, so the same `(0, 1]` bound. **`0` is rejected**, not treated as "never
        // hits": a fight where no attack ever lands is the whole subsystem silently disabled, which
        // is exactly what the `lethality`/`disengage_fraction` bounds above exist to refuse.
        require_positive_finite("hit_chance", self.hit_chance)?;
        if self.hit_chance > MAX_FRACTION {
            return Err(CombatConfigError::Invalid {
                field: "hit_chance",
                constraint: format!("be at most {MAX_FRACTION}"),
                value: self.hit_chance.to_string(),
            });
        }
        // A share of `durability`, so the same `(0, 1]` bound. **`0` is rejected** on the same
        // reasoning as the dials above: a ledger that never decays is a wound the quarry carries for
        // the rest of the campaign, which is the "never forgets" end the design explicitly refused.
        require_positive_finite("wound_recovery_rate", self.wound_recovery_rate)?;
        if self.wound_recovery_rate > MAX_FRACTION {
            return Err(CombatConfigError::Invalid {
                field: "wound_recovery_rate",
                constraint: format!("be at most {MAX_FRACTION}"),
                value: self.wound_recovery_rate.to_string(),
            });
        }
        // Damage, not a fraction, so only the positive-finite half applies — but `0` is rejected for
        // the same reason every other severity dial here is: it silently deletes the baseline risk
        // rather than tuning it down.
        require_positive_finite(
            "hunt_injury_damage_per_animal",
            self.hunt_injury_damage_per_animal,
        )?;
        // A width in sigmas, so unbounded above but never `0`: a zero-width range reports a
        // distribution as a promise, which is the failure mode the range readout exists to end.
        require_positive_finite("forecast_range_sigmas", self.forecast_range_sigmas)?;
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

impl ConfigLoadError for CombatConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
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

/// Load combat configuration from environment (`COMBAT_CONFIG_PATH`) or the default data path.
/// The file is **validated** before it can reach the sim, and a broken invariant is as fatal as a
/// parse error.
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `COMBAT_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_combat_config_from_env() -> (Arc<CombatConfig>, CombatConfigMetadata) {
    let (config, source) = load_config_from_env(
        "COMBAT_CONFIG_PATH",
        "combat_config",
        "src/data/combat_config.json",
        CombatConfig::builtin,
        CombatConfig::from_file,
    );
    (config, CombatConfigMetadata::new(source))
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
        assert_eq!(config.wound_recovery_rate, 0.2);
        assert_eq!(config.hunt_injury_damage_per_animal, 0.15);
        assert_eq!(config.forecast_range_sigmas, 2.0);
    }

    /// A zero-width range would report the point estimate as a certainty — the exact promise the
    /// range readout exists to stop making.
    #[test]
    fn validate_rejects_a_zero_forecast_range() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.forecast_range_sigmas = 0.0;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "forecast_range_sigmas",
                ..
            })
        ));
    }

    #[test]
    fn validate_rejects_a_wound_recovery_rate_above_one() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.wound_recovery_rate = 1.5;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "wound_recovery_rate",
                ..
            })
        ));
    }

    /// A ledger that never decays is the "never forgets" model the design refused — a party chipping
    /// at a mammoth across fifty turns of unrelated play.
    #[test]
    fn validate_rejects_a_wound_recovery_rate_of_zero() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.wound_recovery_rate = 0.0;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "wound_recovery_rate",
                ..
            })
        ));
    }

    #[test]
    fn validate_rejects_a_non_positive_hunt_injury_damage() {
        let mut config = CombatConfig::builtin().as_ref().clone();
        config.hunt_injury_damage_per_animal = 0.0;
        assert!(matches!(
            config.validate(),
            Err(CombatConfigError::Invalid {
                field: "hunt_injury_damage_per_animal",
                ..
            })
        ));
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
