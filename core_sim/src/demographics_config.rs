//! Data-driven tuning for the demographic population model.
//!
//! Loaded from `data/demographics_config.json`. Each `PopulationCohort` carries three age
//! brackets (children / working-age / elders) plus a local food larder; `simulate_population`
//! (see `systems.rs`) draws per-capita food each turn, then resolves scarcity/cold deaths,
//! births, maturation, aging, and elder mortality from these rates. Mirrors the
//! `sedentarization_config.rs` / `fauna_config.rs` loader (baked-in builtin + optional
//! file/env override).
//!
//! **The JSON is the sole source of demographics tuning** (issue #350). There are no hand-written
//! Rust defaults to drift out of sync with it: every field is required, unknown keys are rejected,
//! and `DemographicsConfig::default()` parses [`BUILTIN_DEMOGRAPHICS_CONFIG`]. A missing or
//! misspelled key is a loud parse error, not a silent fallback to a second set of numbers.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_DEMOGRAPHICS_CONFIG: &str = include_str!("data/demographics_config.json");

/// Fractions (summing to ~1.0) that split a freshly spawned cohort's head-count into the three
/// age brackets.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsDistribution {
    pub children: f32,
    pub working: f32,
    pub elders: f32,
}

/// Per-turn food draw. `demand = per_capita_draw × (children·child_factor + working·working_factor
/// + elders·elder_factor)`; the per-bracket factors let dependents eat less than a working adult.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsConsumption {
    pub per_capita_draw: f32,
    pub child_factor: f32,
    pub working_factor: f32,
    pub elder_factor: f32,
}

/// Campaign-start seeding. Each freshly spawned band starts with `food_reserve_days` turns of
/// its own food demand carried in its larder (food is band-local from day one — no faction pool)
/// and a `well_fed_morale_bonus` for opening the game provisioned.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsStartup {
    pub food_reserve_days: f32,
    pub well_fed_morale_bonus: f32,
}

/// The **stock** fertility factor: how deep is the larder. `reserve = 1 + bonus ×
/// min(reserve_turns / saturation_turns, 1)`, where `reserve_turns` is the post-meal larder
/// measured in turns of demand. `saturation_turns = 1.0` reproduces the retired hardcoded
/// behaviour (a two-turn buffer read as maximum surplus); the shipped 10.0 means a band must bank
/// roughly a season to earn the full bonus. See `docs/plan_population_growth_model.md`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsReserve {
    /// Maximum fertility bonus from a full larder (the retired `births.surplus_bonus`).
    pub bonus: f32,
    /// Turns of banked demand that earn the full `bonus`.
    pub saturation_turns: f32,
}

/// The **flow** fertility factor: is the larder growing or shrinking. Two-sided and centred at 1.0
/// — net-positive food raises fertility, net-negative lowers it — driven by
/// `net_ratio = (steady_income − demand − pen_feed_upkeep) / demand`, the negation of the same net
/// drain the player-facing `turnsOfFood` runway divides by. See
/// `docs/plan_population_growth_model.md`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsTrend {
    /// Maximum fertility bonus from net-positive food.
    pub surplus_gain: f32,
    /// Net surplus (as a multiple of demand) that earns the full `surplus_gain`.
    pub surplus_saturation: f32,
    /// Maximum fertility penalty from net-negative food, and **the damp-vs-stop lever**: `0.75`
    /// leaves a fully-collapsed band breeding at 25% of base (starvation mortality stays the real
    /// consequence of a deficit), while **`1.0` lets negative flow stop growth outright**. See
    /// `docs/plan_population_growth_model.md` §2.4.
    pub deficit_penalty: f32,
    /// Net deficit (as a multiple of demand) that reaches the full `deficit_penalty`. `1.0` means
    /// the penalty maxes out when the net flow is a full turn's demand in the red — that is at zero
    /// income for a band with no pens, and *sooner* for one whose pens also eat from the larder.
    pub deficit_saturation: f32,
}

/// Birth tuning. `births = birth_rate × working × hunger × reserve × trend`, added to children — a
/// **product of named factors** mirroring `output_multiplier`'s modifier stack, so adding a future
/// fertility driver is one entry rather than a rewrite of the birth path.
///
/// `hunger` (the food the band actually ate over what it wanted) is a **gate**: it alone reaches 0,
/// so an empty larder yields zero births. `reserve` ∈ `[1, 1+bonus]` and `trend` ∈
/// `[1−deficit_penalty, 1+surplus_gain]` are **modifiers** around 1.0 — neither can zero the
/// product on its own, which is why the stack needs no separate floor lever.
///
/// Births are **morale-independent** (Civilization Wellbeing model, `docs/plan_civ_wellbeing.md`):
/// contentment doesn't change procreation — low morale relocates people or drags output, never
/// suppresses births.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsBirths {
    pub birth_rate: f32,
    pub reserve: DemographicsReserve,
    pub trend: DemographicsTrend,
}

/// Starvation tuning. When food demand outruns the larder, each bracket loses
/// `deficit_fraction × starvation_mortality × <bracket>_vulnerability` of its head-count per turn
/// (dependents typically more vulnerable than working-age) — but never more than the deficit
/// itself, so a 10% food shortfall impacts at most 10% of a bracket. Keep `starvation_mortality`
/// well below 1 so shortfalls bleed the population down over several turns rather than in one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsScarcity {
    pub starvation_mortality: f32,
    pub child_vulnerability: f32,
    pub working_vulnerability: f32,
    pub elder_vulnerability: f32,
}

/// Cold-death tuning. Temperature deviation beyond `temp_tolerance` (°, absolute) kills
/// `min(max_mortality, excess × mortality_scale)` of every bracket per turn.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsCold {
    pub temp_tolerance: f32,
    pub mortality_scale: f32,
    pub max_mortality: f32,
}

/// Root demographic configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsConfig {
    pub initial_distribution: DemographicsDistribution,
    pub consumption: DemographicsConsumption,
    pub startup: DemographicsStartup,
    pub births: DemographicsBirths,
    /// Fraction of children that mature into the working bracket each turn.
    pub maturation_rate: f32,
    /// Fraction of working-age that age into the elder bracket each turn.
    pub aging_rate: f32,
    /// Fraction of elders that die of old age each turn.
    pub elder_mortality_rate: f32,
    pub scarcity: DemographicsScarcity,
    pub cold: DemographicsCold,
}

/// The **only** `Default` in this module: it parses the builtin JSON, so `default()` and the
/// shipped tuning are the same numbers by construction and cannot drift.
///
/// This is non-recursive **only because no struct here carries a container-level
/// `#[serde(default)]`** — that attribute would make deserialization call back into this impl.
/// Do not re-add it; every field is deliberately required (`deny_unknown_fields` on top), so a
/// missing or unknown key fails loudly instead of falling back to a second set of numbers.
impl Default for DemographicsConfig {
    fn default() -> Self {
        serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG)
            .expect("builtin demographics config should parse")
    }
}

impl DemographicsConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, DemographicsConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| DemographicsConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(DemographicsConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum DemographicsConfigError {
    #[error("failed to read demographics config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse demographics config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the demographic configuration.
#[derive(Resource, Debug, Clone)]
pub struct DemographicsConfigHandle(pub Arc<DemographicsConfig>);

impl DemographicsConfigHandle {
    pub fn new(config: Arc<DemographicsConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<DemographicsConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<DemographicsConfig>) {
        self.0 = config;
    }
}

impl Default for DemographicsConfigHandle {
    fn default() -> Self {
        Self(DemographicsConfig::builtin())
    }
}

/// Metadata about the demographic configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct DemographicsConfigMetadata {
    path: Option<PathBuf>,
}

impl DemographicsConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load demographic config from environment (`DEMOGRAPHICS_CONFIG_PATH`) or the default data
/// path, falling back to the baked-in builtin.
pub fn load_demographics_config_from_env() -> (Arc<DemographicsConfig>, DemographicsConfigMetadata)
{
    let override_path = env::var("DEMOGRAPHICS_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/demographics_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match DemographicsConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "demographics_config.loaded=file"
                );
                return (
                    Arc::new(config),
                    DemographicsConfigMetadata::new(Some(path)),
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "demographics_config.load_failed"
                );
            }
        }
    }

    let config = DemographicsConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "demographics_config.loaded=builtin"
    );
    (config, DemographicsConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_sane() {
        let config = DemographicsConfig::builtin();
        // Initial distribution is a probability split.
        let dist = &config.initial_distribution;
        let sum = dist.children + dist.working + dist.elders;
        assert!(
            (sum - 1.0).abs() < 1e-3,
            "initial distribution should sum to ~1.0, got {sum}"
        );
        assert!(
            dist.working > 0.0,
            "there must be a working (labor) bracket"
        );
        // Rates are valid per-turn fractions.
        for rate in [
            config.maturation_rate,
            config.aging_rate,
            config.elder_mortality_rate,
            config.births.birth_rate,
            config.consumption.per_capita_draw,
        ] {
            assert!(rate >= 0.0, "rates must be non-negative");
        }
        assert!(config.cold.max_mortality <= 1.0);
        assert!(config.scarcity.starvation_mortality >= 0.0);
        // Bands must open the game with a positive food reserve.
        assert!(config.startup.food_reserve_days > 0.0);

        // Fertility factors (`docs/plan_population_growth_model.md`). Both saturations divide, so a
        // zero would be a degenerate curve; `deficit_penalty > 1` would drive `trend` negative were
        // it not clamped, and is never what a tuner means.
        let births = &config.births;
        assert!(
            births.reserve.saturation_turns > 0.0,
            "reserve saturation must be a positive number of turns"
        );
        assert!(births.reserve.bonus >= 0.0);
        assert!(
            births.trend.surplus_saturation > 0.0 && births.trend.deficit_saturation > 0.0,
            "trend saturations must be positive — they are divisors"
        );
        assert!(
            (0.0..=1.0).contains(&births.trend.deficit_penalty),
            "deficit_penalty is a fraction of base fertility (1.0 = flow stops growth), got {}",
            births.trend.deficit_penalty
        );
        assert!(births.trend.surplus_gain >= 0.0);
    }

    /// The JSON is the sole source (#350): `default()` **is** the shipped tuning, so a test that
    /// takes a `DemographicsConfig::default()` exercises the numbers the campaign runs on. The
    /// comparison is deliberately **total** rather than a sample of fields — re-introducing a
    /// hand-written `Default` anywhere in the tree fails here the moment any one of its literals
    /// disagrees with the data file, which is precisely the drift this change removed.
    #[test]
    fn default_is_the_shipped_tuning() {
        let shipped: DemographicsConfig =
            serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG).expect("builtin should parse");
        assert_eq!(
            DemographicsConfig::default(),
            shipped,
            "default() must be the shipped JSON, field for field"
        );
    }

    /// Parse the builtin into a `Value` so these tests describe a *shape* rule, not a tuning value —
    /// splicing the shipped literal into a string edit would silently no-op after any re-tune.
    fn builtin_value() -> serde_json::Value {
        serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG).expect("builtin should parse")
    }

    /// A missing key is a **loud parse error**, not a silent fallback — that fallback is exactly how
    /// the Rust and JSON `per_capita_draw` values drifted 5.3× apart.
    #[test]
    fn a_missing_key_is_rejected() {
        let mut value = builtin_value();
        value["consumption"]
            .as_object_mut()
            .expect("consumption is an object")
            .remove("per_capita_draw")
            .expect("builtin should carry per_capita_draw");
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "a config missing per_capita_draw must fail to parse"
        );
    }

    /// So is a key no Rust field reads — a retired lever left behind would otherwise look live.
    #[test]
    fn an_unknown_key_is_rejected() {
        let mut value = builtin_value();
        value["consumption"]
            .as_object_mut()
            .expect("consumption is an object")
            .insert("retired_lever".to_string(), serde_json::json!(1.0));
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "a retired/misspelled key must fail to parse"
        );
    }
}
