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
//!
//! The **loader** is strict to match, via the shared [`crate::config_load`] seam: a
//! present-but-broken file, or a `DEMOGRAPHICS_CONFIG_PATH` that is missing or broken, is fatal at
//! boot. Only an *absent default path* falls back to the builtin — otherwise the strict schema
//! would merely move the silent substitution from one key out to the whole file.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};

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
/// `net_ratio = (steady_income − demand) / demand`, the negation of the same net drain the
/// player-facing `turnsOfFood` runway divides by (a pen eats grass and hay, so neither term counts
/// livestock). See
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

/// One **tail** of the temperature-death model — the cold one below its `onset_temp`, the heat one
/// above. Every degree a tile sits past that onset kills
/// `min(excess × mortality_scale, max_mortality) × <bracket>_vulnerability` of the bracket per turn,
/// food or no food.
///
/// ⛔ **The two tails differ in ALL THREE parameters — threshold, slope and ceiling — which is why
/// the per-tail split was necessary rather than cosmetic.** No single symmetric deviation from an
/// ambient can express that: a tolerance forced the onsets to mirror each other about
/// `ambient_temperature`, putting heat death at 30 °, a warm summer day.
///
/// **Extreme heat is markedly less lethal than extreme cold, and that asymmetry is deliberate.**
/// Heat is survivable with shade and water; −57 ° demands shelter, fire and clothing. So the heat
/// ceiling is 3 % against cold's 10 % — the deadliest heat costs a band roughly a third of what the
/// deadliest cold does (~1.2 against ~3.0 people per turn on a band of 23). Do not "restore
/// symmetry" here: the gap is the model, not an oversight.
///
/// **The rates are calibrated to the temperatures the map should REACH, not the ones it currently
/// produces.** Each tail rises from zero at its onset to `max_mortality` at its own target extreme —
/// −57 ° for cold (57 ° of runway past the 0 ° onset, `0.10 ÷ 57 ≈ 0.00175`) and +57 ° for heat
/// (17 ° past the 40 ° onset, `0.03 ÷ 17 ≈ 0.00176`). Different runways *and* different ceilings, so
/// no shared scale could have served both.
///
/// **The cold onset is 0 °, which is also `climate.polar_max_temp`** — so Boreal and Temperate
/// ground is survivable end to end and only Polar ground kills. That agreement is a *consequence* of
/// two independently-set config values, not something code enforces: move either and the tile card's
/// climate label and its survivability verdict drift apart again (issue #614).
///
/// Today's generator spans −18.5 ° to +31.0 °, so the coldest reachable tile costs a worker
/// 3.2 %/turn, the ceiling is out of reach at both ends, and the heat tail is **entirely dormant** —
/// nothing comes within nine degrees of its onset. That is intended. Calibrating to today's narrow
/// range would have to be redone the moment the range widens, and would make every tile below
/// −18.5 ° kill at an identical rate once such tiles exist. Issue #622 widens the range; these four
/// numbers are the ones it must be checked against.
///
/// The vulnerabilities deliberately differ from [`DemographicsScarcity`]'s: temperature takes the
/// **old first**, then children, and is gentlest on working-age, where starvation weights children
/// and elders alike. Separate levers because the two do not hurt the same people equally. They are
/// ratios, so unlike the rates they say nothing about how wide the map's temperature range is.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsTemperatureTail {
    /// The tile temperature (°) at which this tail starts costing lives — below it for `cold`,
    /// above it for `heat`.
    pub onset_temp: f32,
    pub mortality_scale: f32,
    /// ⛔ **The ceiling on the TILE's base rate — not the most any bracket can suffer.** It is
    /// applied *before* the per-bracket multiplier, so a bracket's actual ceiling is
    /// `max_mortality × <bracket>_vulnerability`: 10 % for workers, 12.5 % for children, 15 % for
    /// elders at the shipped tuning.
    ///
    /// Clamping *after* the multiplier was tried and rejected. It reads like the safer ordering — a
    /// true per-bracket ceiling nobody can exceed — but it makes the brackets converge: elders would
    /// saturate at −38.1 ° and children at −45.7 °, so from −45.7 ° down every bracket sits on exactly
    /// the same number and the age ordering vanishes in the range where cold is *most* severe, which
    /// is the one place it matters most. Capping the base rate keeps "the cold takes the old first"
    /// true all the way down.
    pub max_mortality: f32,
    pub child_vulnerability: f32,
    pub working_vulnerability: f32,
    pub elder_vulnerability: f32,
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
    /// The tail below `cold.onset_temp`.
    pub cold: DemographicsTemperatureTail,
    /// The tail above `heat.onset_temp`. **Dormant on today's maps** — worldgen tops out near 31 °
    /// and the onset is 40 ° — but calibrated for the range issue #622 opens up, not for today's.
    pub heat: DemographicsTemperatureTail,
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

impl ConfigLoadError for DemographicsConfigError {
    /// The file simply **is not there**, as distinct from being unreadable (permissions, a
    /// directory in the way) or invalid. This is the one distinction the loader's fallback rule
    /// turns on — see [`crate::config_load::resolve_config`].
    fn is_not_found(&self) -> bool {
        match self {
            Self::Read { source, .. } => source.kind() == io::ErrorKind::NotFound,
            Self::Parse(_) => false,
        }
    }
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

/// Load demographic config from environment (`DEMOGRAPHICS_CONFIG_PATH`) or the default data path.
///
/// **Panics** when the named/default file exists but cannot be read or parsed, or when an explicit
/// `DEMOGRAPHICS_CONFIG_PATH` names a file that is not there — see
/// [`crate::config_load::resolve_config`] for why only an absent *default* path is benign. Without
/// that, the strict schema (#350) would merely move the silent substitution one layer out: a
/// fat-fingered key would stop being a silent serde default and start being a silent whole-file
/// fallback.
pub fn load_demographics_config_from_env() -> (Arc<DemographicsConfig>, DemographicsConfigMetadata)
{
    let (config, source) = load_config_from_env(
        "DEMOGRAPHICS_CONFIG_PATH",
        "demographics_config",
        "src/data/demographics_config.json",
        DemographicsConfig::builtin,
        DemographicsConfig::from_file,
    );
    (config, DemographicsConfigMetadata::new(source))
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
        assert!(config.scarcity.starvation_mortality >= 0.0);
        // Both tails weight the brackets the way the model orders them: elders worst, then
        // children, workers least. Asserted as an ORDERING rather than as literals so a re-tune
        // moves the numbers freely but cannot silently invert who the temperature takes first.
        for (name, tail) in [("cold", &config.cold), ("heat", &config.heat)] {
            // The cap bounds the BASE rate, so what has to stay a fraction is the cap times the
            // steepest bracket weight — that product is the real per-bracket ceiling, and
            // `death_fraction` relies on it being ≤ 1 to do without a clamp of its own.
            let steepest = tail
                .elder_vulnerability
                .max(tail.child_vulnerability)
                .max(tail.working_vulnerability);
            assert!(
                tail.max_mortality * steepest <= 1.0,
                "{name}'s worst bracket would lose {} of itself per turn, which is not a fraction",
                tail.max_mortality * steepest
            );
            assert!(
                tail.elder_vulnerability > tail.child_vulnerability
                    && tail.child_vulnerability > tail.working_vulnerability,
                "{name} must weight elders > children > working, got {} / {} / {}",
                tail.elder_vulnerability,
                tail.child_vulnerability,
                tail.working_vulnerability
            );
            assert!(
                tail.working_vulnerability > 0.0 && tail.mortality_scale >= 0.0,
                "{name} vulnerabilities are positive multipliers and the scale is non-negative"
            );
        }
        assert!(
            config.cold.onset_temp < config.heat.onset_temp,
            "the survivable band runs cold onset -> heat onset, got {} -> {}",
            config.cold.onset_temp,
            config.heat.onset_temp
        );
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
