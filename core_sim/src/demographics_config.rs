//! Data-driven tuning for the demographic population model.
//!
//! Loaded from `data/demographics_config.json`. Each `PopulationCohort` carries three age
//! brackets (children / working-age / elders) plus a local food larder; `simulate_population`
//! (see `systems.rs`) draws per-capita food each turn, then resolves scarcity/cold deaths,
//! births, maturation, aging, and elder mortality from these rates. Mirrors the
//! `sedentarization_config.rs` / `fauna_config.rs` loader (baked-in builtin + optional
//! file/env override).

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
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsDistribution {
    pub children: f32,
    pub working: f32,
    pub elders: f32,
}

impl Default for DemographicsDistribution {
    fn default() -> Self {
        Self {
            children: 0.30,
            working: 0.55,
            elders: 0.15,
        }
    }
}

/// Per-turn food draw. `demand = per_capita_draw × (children·child_factor + working·working_factor
/// + elders·elder_factor)`; the per-bracket factors let dependents eat less than a working adult.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsConsumption {
    pub per_capita_draw: f32,
    pub child_factor: f32,
    pub working_factor: f32,
    pub elder_factor: f32,
}

impl Default for DemographicsConsumption {
    fn default() -> Self {
        Self {
            per_capita_draw: 0.03,
            child_factor: 0.6,
            working_factor: 1.0,
            elder_factor: 0.8,
        }
    }
}

/// Campaign-start seeding. Each freshly spawned band starts with `food_reserve_days` turns of
/// its own food demand carried in its larder (food is band-local from day one — no faction pool)
/// and a `well_fed_morale_bonus` for opening the game provisioned.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsStartup {
    pub food_reserve_days: f32,
    pub well_fed_morale_bonus: f32,
}

impl Default for DemographicsStartup {
    fn default() -> Self {
        Self {
            food_reserve_days: 20.0,
            well_fed_morale_bonus: 0.2,
        }
    }
}

/// The **stock** fertility factor: how deep is the larder. `reserve = 1 + bonus ×
/// min(reserve_turns / saturation_turns, 1)`, where `reserve_turns` is the post-meal larder
/// measured in turns of demand. `saturation_turns = 1.0` reproduces the retired hardcoded
/// behaviour (a two-turn buffer read as maximum surplus); the shipped 10.0 means a band must bank
/// roughly a season to earn the full bonus. See `docs/plan_population_growth_model.md`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsReserve {
    /// Maximum fertility bonus from a full larder (the retired `births.surplus_bonus`).
    pub bonus: f32,
    /// Turns of banked demand that earn the full `bonus`.
    pub saturation_turns: f32,
}

impl Default for DemographicsReserve {
    fn default() -> Self {
        Self {
            bonus: 0.5,
            saturation_turns: 10.0,
        }
    }
}

/// The **flow** fertility factor: is the larder growing or shrinking. Two-sided and centred at 1.0
/// — net-positive food raises fertility, net-negative lowers it — driven by
/// `net_ratio = (steady_income − demand − pen_feed_upkeep) / demand`, the negation of the same net
/// drain the player-facing `turnsOfFood` runway divides by. See
/// `docs/plan_population_growth_model.md`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
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

impl Default for DemographicsTrend {
    fn default() -> Self {
        Self {
            surplus_gain: 0.25,
            surplus_saturation: 0.5,
            deficit_penalty: 0.75,
            deficit_saturation: 1.0,
        }
    }
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
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsBirths {
    pub birth_rate: f32,
    pub reserve: DemographicsReserve,
    pub trend: DemographicsTrend,
}

impl Default for DemographicsBirths {
    fn default() -> Self {
        Self {
            birth_rate: 0.03,
            reserve: DemographicsReserve::default(),
            trend: DemographicsTrend::default(),
        }
    }
}

/// Starvation tuning. When food demand outruns the larder, each bracket loses
/// `deficit_fraction × starvation_mortality × <bracket>_vulnerability` of its head-count per turn
/// (dependents typically more vulnerable than working-age) — but never more than the deficit
/// itself, so a 10% food shortfall impacts at most 10% of a bracket. Keep `starvation_mortality`
/// well below 1 so shortfalls bleed the population down over several turns rather than in one.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsScarcity {
    pub starvation_mortality: f32,
    pub child_vulnerability: f32,
    pub working_vulnerability: f32,
    pub elder_vulnerability: f32,
}

impl Default for DemographicsScarcity {
    fn default() -> Self {
        Self {
            starvation_mortality: 0.2,
            child_vulnerability: 1.5,
            working_vulnerability: 1.0,
            elder_vulnerability: 1.5,
        }
    }
}

/// Cold-death tuning. Temperature deviation beyond `temp_tolerance` (°, absolute) kills
/// `min(max_mortality, excess × mortality_scale)` of every bracket per turn.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DemographicsCold {
    pub temp_tolerance: f32,
    pub mortality_scale: f32,
    pub max_mortality: f32,
}

impl Default for DemographicsCold {
    fn default() -> Self {
        Self {
            temp_tolerance: 12.0,
            mortality_scale: 0.02,
            max_mortality: 0.1,
        }
    }
}

/// Root demographic configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
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

impl Default for DemographicsConfig {
    fn default() -> Self {
        Self {
            initial_distribution: DemographicsDistribution::default(),
            consumption: DemographicsConsumption::default(),
            startup: DemographicsStartup::default(),
            births: DemographicsBirths::default(),
            maturation_rate: 0.05,
            aging_rate: 0.025,
            elder_mortality_rate: 0.06,
            scarcity: DemographicsScarcity::default(),
            cold: DemographicsCold::default(),
        }
    }
}

impl DemographicsConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG)
                .expect("builtin demographics config should parse"),
        )
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
}
