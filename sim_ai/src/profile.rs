//! **Personality and difficulty** — `data/ai_profiles.json` (`docs/plan_ai_opponents.md` §4, §6;
//! `docs/plan_ai_driver.md` §6).
//!
//! A profile says *who* this people is: an archetype (the stance it starts from), behaviours (whole
//! action classes gated on or off), weights (the budget split and priorities), commitment (the
//! hysteresis term, ⛔ not optional), and the floors its specialists alarm at. A difficulty says
//! *how well it follows through* and is chosen independently. Each key has exactly one consumer,
//! named on its field. **No lever anywhere grants material** — a seat has nowhere to receive it.
//!
//! The file is embedded at build time (`include_str!`, the builtin) and may be replaced whole by
//! `--profiles <path>`; a named file that is missing or broken fails the process rather than
//! quietly loading the builtin (`.claude/rules/core_sim/config-loading.md`'s rule, restated: only
//! an absent *default* falls back, and here the default is the builtin itself). `schemars` derives
//! the schema the shipped file is validated against in a unit test, and `deny_unknown_fields` makes
//! a misspelt key a load error rather than a default.

use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::orchestrator::Stance;
use crate::specialists::{SpecialistId, SPECIALIST_FOOD, SPECIALIST_LAND};

/// The shipped file.
pub const BUILTIN_PROFILES: &str = include_str!("../data/ai_profiles.json");
/// The difficulty a seat plays at when none is named.
pub const DEFAULT_DIFFICULTY: &str = "normal";

/// **A weight key → the specialist it funds.** `contact_seeking` is in the schema with no consumer
/// until `Contact` exists (`plan_ai_driver.md` §4 roster), so it is read, kept, and funds nothing.
pub const WEIGHT_FOOD_SECURITY: &str = "food_security";
pub const WEIGHT_LAND_CLAIM: &str = "land_claim";
pub const WEIGHT_CONTACT_SEEKING: &str = "contact_seeking";
pub const WEIGHT_TO_SPECIALIST: [(&str, SpecialistId); 2] = [
    (WEIGHT_FOOD_SECURITY, SPECIALIST_FOOD),
    (WEIGHT_LAND_CLAIM, SPECIALIST_LAND),
];
/// Every key `weights` may carry.
const KNOWN_WEIGHTS: [&str; 3] = [
    WEIGHT_FOOD_SECURITY,
    WEIGHT_LAND_CLAIM,
    WEIGHT_CONTACT_SEEKING,
];
/// A difficulty's `selection_top_k` at which selection is argmax.
pub const ARGMAX_TOP_K: u32 = 1;
/// A difficulty's `memory_horizon_turns` meaning "never decay".
pub const NO_MEMORY_DECAY: u64 = 0;

/// The goal an archetype pursues — the stance it resolves to (`plan_ai_driver.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Archetype {
    Expand,
    Consolidate,
    Seek,
}

impl Archetype {
    /// Consumed by the orchestrator: the stance it starts from.
    pub fn stance(self) -> Stance {
        match self {
            Archetype::Expand => Stance::Expand,
            Archetype::Consolidate => Stance::Consolidate,
            Archetype::Seek => Stance::Seek,
        }
    }
}

/// Consumed by the arbiter's behaviour gate: intent classes gated off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Behaviors {
    pub will_raid: bool,
    pub will_trade: bool,
}

/// Consumed by `Food`: its alarm threshold.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FoodFloors {
    /// `FoodShort` when the minimum own-band `turns_of_food` is below this.
    pub runway_floor_turns: f32,
    /// A worked row whose realized take per worker has stayed below `poor_yield_fraction` of the
    /// frame's forecast for this many consecutive turns is a **dead row**: its crew is moved and its
    /// source avoided. (A hunt row the sim marks `hunt_useful_workers == 0` is dead at once.)
    pub dead_row_turns: u32,
    /// The share of the forecast a row must realize, per worker, not to count as dead.
    pub poor_yield_fraction: f32,
}

/// Consumed by `Land`: its floors and reach.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LandFloors {
    /// *Blind*: fewer known tiles than this within `horizon_tiles` of a band posts scouts.
    pub known_tiles_floor: u32,
    /// *Room*: a band larger than this on owned ground splits, under `Expand`.
    pub split_size: u32,
    /// How far from a band *blind* counts and *better ground* looks, in hex steps.
    pub horizon_tiles: u32,
    /// How many scouts *blind* posts (`assign_labor … scout <n>`).
    pub scout_workers: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AiProfile {
    pub archetype: Archetype,
    pub behaviors: Behaviors,
    /// Consumed by the orchestrator: normalised over the enabled specialists as budgets, and as
    /// priorities un-normalised.
    pub weights: BTreeMap<String, f32>,
    /// Consumed twice, as `plan_ai_driver.md` §5 says: the orchestrator's stance switch margin and
    /// the arbiter's intent bonus (`score *= 1 + commitment` on an intent chosen last turn).
    pub commitment: f32,
    pub food: FoodFloors,
    pub land: LandFloors,
}

impl AiProfile {
    /// The weight under `key`, or zero: a profile that omits a weight funds nothing with it.
    pub fn weight(&self, key: &str) -> f32 {
        self.weights.get(key).copied().unwrap_or_default()
    }
}

/// How well a seat follows through (`plan_ai_opponents.md` §6): three levers on one brain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Difficulty {
    /// Consumed by the arbiter: argmax at [`ARGMAX_TOP_K`], a uniform sample among the top k above.
    pub selection_top_k: u32,
    /// Consumed by the orchestrator: how many turns a plan stands before it is re-made.
    pub goal_cadence_turns: u64,
    /// Consumed by `SeatMemory`: a tile last seen longer ago than this is unknown again;
    /// [`NO_MEMORY_DECAY`] never forgets.
    pub memory_horizon_turns: u64,
}

/// Global levers with one consumer each.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tuning {
    #[serde(default, rename = "_comment_alarm_budget_shift", skip_serializing)]
    _comment_alarm_budget_shift: Option<String>,
    /// Consumed by `ConstantStance`: the worker share moved to an alarming specialist for one
    /// cadence.
    pub alarm_budget_shift: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AiProfiles {
    #[serde(default, rename = "_comment", skip_serializing)]
    _comment: Option<String>,
    pub profiles: BTreeMap<String, AiProfile>,
    pub difficulties: BTreeMap<String, Difficulty>,
    pub tuning: Tuning,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("could not read the profiles file {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("the profiles file {path} does not parse: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("the profiles file is incoherent: {0}")]
    Invalid(String),
    #[error("no profile named `{0}`; the file has {1:?}")]
    NoSuchProfile(String, Vec<String>),
    #[error("no difficulty named `{0}`; the file has {1:?}")]
    NoSuchDifficulty(String, Vec<String>),
}

impl AiProfiles {
    /// The embedded file, validated. A builtin that fails here is a build defect, so it panics.
    pub fn builtin() -> Self {
        Self::parse(BUILTIN_PROFILES, "<builtin>").expect("the shipped ai_profiles.json is valid")
    }

    /// A file named by the operator: missing or broken is an error, never the builtin.
    pub fn load(path: &Path) -> Result<Self, ProfileError> {
        let text = std::fs::read_to_string(path).map_err(|source| ProfileError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    pub fn parse(text: &str, path: &str) -> Result<Self, ProfileError> {
        let profiles: Self = serde_json::from_str(text).map_err(|source| ProfileError::Parse {
            path: path.to_owned(),
            source,
        })?;
        profiles.validate()?;
        Ok(profiles)
    }

    /// The first profile in key order — what a seat plays when none is named.
    pub fn default_profile_id(&self) -> &str {
        self.profiles
            .keys()
            .next()
            .map(String::as_str)
            .expect("validate() refuses an empty profile set")
    }

    pub fn profile(&self, id: &str) -> Result<&AiProfile, ProfileError> {
        self.profiles.get(id).ok_or_else(|| {
            ProfileError::NoSuchProfile(id.to_owned(), self.profiles.keys().cloned().collect())
        })
    }

    pub fn difficulty(&self, id: &str) -> Result<Difficulty, ProfileError> {
        self.difficulties.get(id).copied().ok_or_else(|| {
            ProfileError::NoSuchDifficulty(
                id.to_owned(),
                self.difficulties.keys().cloned().collect(),
            )
        })
    }

    /// Parsed-but-incoherent counts as broken (`config-loading.md`).
    fn validate(&self) -> Result<(), ProfileError> {
        let invalid = |detail: String| Err(ProfileError::Invalid(detail));
        if self.profiles.is_empty() {
            return invalid("no profiles".to_owned());
        }
        if !self.difficulties.contains_key(DEFAULT_DIFFICULTY) {
            return invalid(format!("no `{DEFAULT_DIFFICULTY}` difficulty"));
        }
        for (id, profile) in &self.profiles {
            for (key, weight) in &profile.weights {
                if !KNOWN_WEIGHTS.contains(&key.as_str()) {
                    return invalid(format!("profile `{id}`: unknown weight `{key}`"));
                }
                if !weight.is_finite() || *weight < 0.0 {
                    return invalid(format!("profile `{id}`: weight `{key}` = {weight}"));
                }
            }
            if !profile.commitment.is_finite() || profile.commitment < 0.0 {
                return invalid(format!("profile `{id}`: commitment {}", profile.commitment));
            }
            if !profile.food.runway_floor_turns.is_finite() || profile.food.runway_floor_turns < 0.0
            {
                return invalid(format!("profile `{id}`: food.runway_floor_turns"));
            }
            if profile.land.scout_workers == 0 {
                return invalid(format!("profile `{id}`: land.scout_workers is 0"));
            }
            if profile.food.dead_row_turns == 0 {
                return invalid(format!("profile `{id}`: food.dead_row_turns is 0"));
            }
            let fraction = profile.food.poor_yield_fraction;
            if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
                return invalid(format!(
                    "profile `{id}`: food.poor_yield_fraction = {fraction}"
                ));
            }
        }
        for (id, difficulty) in &self.difficulties {
            if difficulty.selection_top_k < ARGMAX_TOP_K {
                return invalid(format!("difficulty `{id}`: selection_top_k is 0"));
            }
            if difficulty.goal_cadence_turns == 0 {
                return invalid(format!("difficulty `{id}`: goal_cadence_turns is 0"));
            }
        }
        let shift = self.tuning.alarm_budget_shift;
        if !shift.is_finite() || !(0.0..=1.0).contains(&shift) {
            return invalid(format!(
                "tuning.alarm_budget_shift = {shift} is not a share"
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_file_parses_validates_and_matches_its_own_schema() {
        let profiles = AiProfiles::builtin();
        assert!(
            profiles.profiles.len() >= 2,
            "two shipped profiles that differ"
        );
        assert_ne!(
            profiles.profile("forager").unwrap().weights,
            profiles.profile("rover").unwrap().weights
        );
        profiles.difficulty(DEFAULT_DIFFICULTY).unwrap();
        let schema = serde_json::to_value(schemars::schema_for!(AiProfiles)).unwrap();
        let compiled = jsonschema::JSONSchema::compile(&schema).expect("the schema compiles");
        let document: serde_json::Value = serde_json::from_str(BUILTIN_PROFILES).unwrap();
        let outcome = compiled
            .validate(&document)
            .map_err(|errors| errors.map(|error| error.to_string()).collect::<Vec<_>>());
        if let Err(errors) = outcome {
            panic!("ai_profiles.json does not match its schema: {errors:?}");
        }
    }

    #[test]
    fn every_shipped_weight_names_a_known_key_and_a_stranger_is_refused() {
        let mut document: serde_json::Value = serde_json::from_str(BUILTIN_PROFILES).unwrap();
        document["profiles"]["forager"]["weights"]["warlike"] = serde_json::json!(0.5);
        let err = AiProfiles::parse(&document.to_string(), "<test>").unwrap_err();
        assert!(matches!(err, ProfileError::Invalid(_)), "{err}");
    }

    #[test]
    fn an_unknown_key_is_a_parse_error_not_a_default() {
        let mut document: serde_json::Value = serde_json::from_str(BUILTIN_PROFILES).unwrap();
        document["profiles"]["forager"]["scout_bias"] = serde_json::json!(0.2);
        assert!(matches!(
            AiProfiles::parse(&document.to_string(), "<test>"),
            Err(ProfileError::Parse { .. })
        ));
    }

    #[test]
    fn an_unknown_profile_or_difficulty_is_named_with_the_choices() {
        let profiles = AiProfiles::builtin();
        assert!(matches!(
            profiles.profile("warlord"),
            Err(ProfileError::NoSuchProfile(_, _))
        ));
        assert!(matches!(
            profiles.difficulty("brutal"),
            Err(ProfileError::NoSuchDifficulty(_, _))
        ));
        assert_eq!(profiles.default_profile_id(), "forager");
    }
}
