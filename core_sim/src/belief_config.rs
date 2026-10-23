//! Tuning for belief on a place — how much each source adds to a tile's stock.
//!
//! Loaded from `data/belief_config.json`, on the shared boot seam (`crate::config_load`): an absent
//! default path falls back to the builtin, anything else that is there and wrong is a boot panic.
//! Mirrors `connections_config.rs`.
//!
//! **The numbers, and what they mean in play** (`docs/plan_civilization_steps.md` §"Belief is a
//! property of a place"):
//!
//! - `belief_per_death` **1.0** — belief added to the tile a band stands on, per person who dies
//!   there. At `1.0` the unit of belief *is* the dead-equivalent: a place reading `12` holds twelve
//!   people's worth of ancestors. Later sources (gatherings, the monument) are priced in that unit.
//!
//! - `belief_per_gathering` **1.0** — belief added to the tile a gathering is held on (the larger
//!   band's standing tile), in the same dead-equivalents: one gathering is worth one death's
//!   ancestors. A PLAYTEST DIAL.
//!
//! - `ancestor_pull` **{ secular_devout 0.3, traditionalist_revisionist −0.3 }** — the signed offset
//!   each named culture axis takes at full tie to the ancestors (`s → 1`, `r = 1`); see
//!   `.claude/rules/core_sim/belief.md` → "The ancestor pull". An empty map turns the pull off.
//!
//! - `improvement_belief` **[{ rung "belief:monument", per_turn 0.1 }]** - the belief a held tile
//!   improvement credits to its own tile each turn (`crate::belief::advance_improvement_belief`),
//!   the monument as the third source on the `BeliefRegistry::add` seam. Each `rung` is a
//!   `branch:id` reconciled against the ladder at boot ([`BeliefConfig::validate_improvement_belief`]);
//!   `per_turn` is finite and `> 0`. An empty list turns the payoff off. A PLAYTEST DIAL.
//!
//! There is deliberately **no decay lever**: belief is monotone (`crate::belief`), so an abandoned
//! place keeps its dead.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::culture::{
    culture_axis_from_key, culture_axis_key, CultureTraitAxis, CULTURE_TRAIT_AXES,
};
use bevy::prelude::Resource;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

pub const BUILTIN_BELIEF_CONFIG: &str = include_str!("data/belief_config.json");

/// Root configuration for belief on a place.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BeliefConfig {
    /// Belief added to the tile a band stands on, per person who dies there. Applied to the turn's
    /// **fractional** death total, not the whole-person events, because belief is a continuous
    /// stock — a band losing a third of a person a turn for three turns has buried one.
    #[serde(deserialize_with = "deserialize_non_negative_finite")]
    pub belief_per_death: f32,
    /// Belief added to the tile a gathering is held on, per gathering — two bands meeting after
    /// having no live tie, credited at the larger band's standing tile. In the same
    /// dead-equivalents as `belief_per_death`. PLAYTEST DIAL; finite and `>= 0` at parse.
    #[serde(deserialize_with = "deserialize_non_negative_finite")]
    pub belief_per_gathering: f32,
    /// The signed offset each named culture axis takes at full tie to the ancestors (`s → 1`,
    /// `r = 1`), keyed by the axis snake_case key (`crate::culture::culture_axis_key`). An unknown
    /// key or a non-finite value is a parse error; an axis not named takes no pull, so an empty map
    /// turns the pull off. PLAYTEST DIAL.
    #[serde(deserialize_with = "deserialize_ancestor_pull")]
    pub ancestor_pull: BTreeMap<String, f32>,
    /// **WHAT A HELD TILE IMPROVEMENT ADDS TO ITS OWN TILE'S BELIEF, EACH TURN** (#692) - the
    /// monument as the third source on the `BeliefRegistry::add` seam. One entry per rung; the
    /// engine names none of them, so any tile-kind rung can be a source with a JSON edit. Empty =
    /// no improvement adds belief. Reconciled against the ladder at boot
    /// ([`Self::validate_improvement_belief`]).
    pub improvement_belief: Vec<ImprovementBelief>,
}

/// **ONE IMPROVEMENT SOURCE** - the belief a tile gains each turn a rung is held on it. Strict: every
/// key required and no other accepted, the `Preservation` row's shape.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImprovementBelief {
    /// The rung that has to be HELD, `branch:id`.
    pub rung: crate::ladder_ids::RungId,
    /// Belief credited to the improvement's own tile per turn the rung is held, in dead-equivalents.
    /// Finite and `> 0`.
    #[serde(deserialize_with = "deserialize_positive_finite")]
    pub per_turn: f32,
}

/// Parse a per-turn belief rate, refusing a value that is not finite or is not above zero - a rate of
/// zero is a source that parses and credits nothing.
fn deserialize_positive_finite<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let value = f32::deserialize(deserializer)?;
    if !value.is_finite() || value <= 0.0 {
        return Err(D::Error::custom(format!(
            "improvement_belief.per_turn must be finite and > 0, got {value}"
        )));
    }
    Ok(value)
}

/// Shipped belief per gathering: one gathering is worth one death's ancestors.
const DEFAULT_BELIEF_PER_GATHERING: f32 = 1.0;
/// Shipped belief per death: the unit of belief is the dead-equivalent.
const DEFAULT_BELIEF_PER_DEATH: f32 = 1.0;

/// Parse a belief-per-source lever, refusing a value that is not finite or is negative.
fn deserialize_non_negative_finite<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let value = f32::deserialize(deserializer)?;
    if !value.is_finite() || value < 0.0 {
        return Err(D::Error::custom(format!(
            "belief lever must be finite and >= 0, got {value}"
        )));
    }
    Ok(value)
}

/// Shipped pull on the Devout axis (positive is devout) at full tie.
const DEFAULT_PULL_SECULAR_DEVOUT: f32 = 0.3;
/// Shipped pull on the Traditionalist axis (negative is traditionalist) at full tie.
const DEFAULT_PULL_TRADITIONALIST_REVISIONIST: f32 = -0.3;
/// The pull on an axis the map does not name.
const NO_PULL: f32 = 0.0;

/// Parse `ancestor_pull`, refusing a key that names no culture axis and a value that is not finite.
fn deserialize_ancestor_pull<'de, D>(deserializer: D) -> Result<BTreeMap<String, f32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let map = BTreeMap::<String, f32>::deserialize(deserializer)?;
    for (key, value) in &map {
        if culture_axis_from_key(key).is_none() {
            return Err(D::Error::custom(format!(
                "ancestor_pull names `{key}`, which is not a culture axis key"
            )));
        }
        if !value.is_finite() {
            return Err(D::Error::custom(format!(
                "ancestor_pull.{key} must be finite, got {value}"
            )));
        }
    }
    Ok(map)
}

impl Default for BeliefConfig {
    fn default() -> Self {
        Self {
            belief_per_death: DEFAULT_BELIEF_PER_DEATH,
            belief_per_gathering: DEFAULT_BELIEF_PER_GATHERING,
            ancestor_pull: BTreeMap::from([
                (
                    culture_axis_key(CultureTraitAxis::SecularDevout).to_string(),
                    DEFAULT_PULL_SECULAR_DEVOUT,
                ),
                (
                    culture_axis_key(CultureTraitAxis::TraditionalistRevisionist).to_string(),
                    DEFAULT_PULL_TRADITIONALIST_REVISIONIST,
                ),
            ]),
            // **Empty, because the engine names no rung.** The shipped monument entry lives in
            // `belief_config.json` alone; a partial file that omits the list turns the payoff off.
            improvement_belief: Vec::new(),
        }
    }
}

impl BeliefConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_BELIEF_CONFIG)
                .expect("builtin belief config should parse"),
        )
    }

    /// The pull as a per-axis target offset at full tie, in axis order; `0` on an axis not named.
    pub fn ancestor_pull_vector(&self) -> [f32; CULTURE_TRAIT_AXES] {
        CultureTraitAxis::ALL.map(|axis| {
            self.ancestor_pull
                .get(culture_axis_key(axis))
                .copied()
                .unwrap_or(NO_PULL)
        })
    }

    /// **EVERY IMPROVEMENT SOURCE NAMES A RUNG THE LADDER DEFINES, ON A TILE BRANCH** - the
    /// cross-config half, run at boot where both configs are in scope (`validate` cannot hold the
    /// ladder). A rung nobody defines, one on a branch whose kind is not `tile`, a root nothing
    /// raises (every tile would pay it) or a rung listed twice would parse and never apply, or apply
    /// wrongly.
    pub fn validate_improvement_belief(
        &self,
        ladder: &crate::intensification::LadderConfig,
    ) -> Result<(), String> {
        use crate::intensification::SourceKind;
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.improvement_belief {
            let Some(rung) = ladder.rung_by_id(entry.rung) else {
                return Err(format!(
                    "improvement_belief names unknown rung '{}'",
                    entry.rung
                ));
            };
            if ladder.source_kind(rung.branch) != Some(SourceKind::Tile) {
                return Err(format!(
                    "improvement_belief names '{}', which is not on a tile branch - only an \
                     improvement standing on one tile can add belief to that tile",
                    entry.rung
                ));
            }
            if rung.build.is_none() {
                return Err(format!(
                    "improvement_belief names '{}', a rung nothing raises - every tile stands on \
                     it, so it would add belief everywhere",
                    entry.rung
                ));
            }
            if !seen.insert(entry.rung) {
                return Err(format!("improvement_belief states '{}' twice", entry.rung));
            }
        }
        Ok(())
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, BeliefConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| BeliefConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(BeliefConfig::from_json_str(&contents)?)
    }
}

#[derive(Debug, Error)]
pub enum BeliefConfigError {
    #[error("failed to read belief config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse belief config: {0}")]
    Parse(#[from] serde_json::Error),
}

impl ConfigLoadError for BeliefConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the belief configuration.
#[derive(Resource, Debug, Clone)]
pub struct BeliefConfigHandle(pub Arc<BeliefConfig>);

impl BeliefConfigHandle {
    pub fn new(config: Arc<BeliefConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<BeliefConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<BeliefConfig>) {
        self.0 = config;
    }
}

impl Default for BeliefConfigHandle {
    fn default() -> Self {
        Self(BeliefConfig::builtin())
    }
}

/// Metadata about the belief configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct BeliefConfigMetadata {
    path: Option<PathBuf>,
}

impl BeliefConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load belief config from environment (`BELIEF_CONFIG_PATH`) or the default data path. Only an
/// absent *default* path falls back to the builtin; see [`crate::config_load::resolve_config`] for
/// the rule and what it panics on.
pub fn load_belief_config_from_env() -> (Arc<BeliefConfig>, BeliefConfigMetadata) {
    let (config, source) = load_config_from_env(
        "BELIEF_CONFIG_PATH",
        "belief_config",
        "src/data/belief_config.json",
        BeliefConfig::builtin,
        BeliefConfig::from_file,
    );
    (config, BeliefConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_matches_the_default() {
        let builtin = BeliefConfig::builtin();
        assert!(
            builtin.belief_per_death > 0.0,
            "a death that adds no belief would leave the cemetery source switched off"
        );
        assert_eq!(
            builtin.belief_per_death,
            BeliefConfig::default().belief_per_death,
            "the shipped JSON and the serde default must agree, or a partial file reads differently"
        );
        assert_eq!(
            builtin.ancestor_pull,
            BeliefConfig::default().ancestor_pull,
            "the shipped pull and the serde default must agree"
        );
    }

    #[test]
    fn belief_per_gathering_must_be_finite_and_non_negative() {
        assert!(BeliefConfig::from_json_str(r#"{ "belief_per_gathering": -0.5 }"#).is_err());
        assert!(BeliefConfig::from_json_str(r#"{ "belief_per_gathering": 1e60 }"#).is_err());
        let off = BeliefConfig::from_json_str(r#"{ "belief_per_gathering": 0 }"#).expect("parses");
        assert_eq!(off.belief_per_gathering, 0.0);
        assert_eq!(
            BeliefConfig::builtin().belief_per_gathering,
            BeliefConfig::default().belief_per_gathering
        );
    }

    #[test]
    fn improvement_belief_is_strict_and_positive() {
        const GOOD: &str = r#"{ "improvement_belief": [ { "rung": "a:b", "per_turn": 0.1 } ] }"#;
        assert!(BeliefConfig::from_json_str(GOOD).is_ok());
        for bad in [
            r#"{ "improvement_belief": [ { "rung": "a:b", "per_turn": 0 } ] }"#,
            r#"{ "improvement_belief": [ { "rung": "a:b", "per_turn": -1 } ] }"#,
            r#"{ "improvement_belief": [ { "rung": "a:b", "per_turn": 1e60 } ] }"#,
            r#"{ "improvement_belief": [ { "rung": "a:b" } ] }"#,
            r#"{ "improvement_belief": [ { "per_turn": 0.1 } ] }"#,
            r#"{ "improvement_belief": [ { "rung": "ab", "per_turn": 0.1 } ] }"#,
            r#"{ "improvement_belief": [ { "rung": "a:b", "per_turn": 0.1, "extra": 1 } ] }"#,
        ] {
            assert!(BeliefConfig::from_json_str(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn improvement_belief_must_name_a_tile_rung_the_ladder_defines() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let check = |rung: &str| {
            BeliefConfig::from_json_str(&format!(
                r#"{{ "improvement_belief": [ {{ "rung": "{rung}", "per_turn": 0.1 }} ] }}"#
            ))
            .expect("parses")
            .validate_improvement_belief(&ladder)
        };
        assert!(check("belief:no_such_rung").is_err(), "unknown rung");
        assert!(check("plant:tended").is_err(), "not a tile branch");
        assert!(
            check("belief:gathering_ground").is_err(),
            "a root nothing raises"
        );
        assert!(check("belief:monument").is_ok());
        let twice = BeliefConfig::from_json_str(
            r#"{ "improvement_belief": [
                { "rung": "belief:monument", "per_turn": 0.1 },
                { "rung": "belief:monument", "per_turn": 0.2 } ] }"#,
        )
        .expect("parses");
        assert!(twice.validate_improvement_belief(&ladder).is_err());
    }

    #[test]
    fn an_unknown_ancestor_pull_axis_is_a_parse_error() {
        assert!(
            BeliefConfig::from_json_str(r#"{ "ancestor_pull": { "no_such_axis": 0.1 } }"#).is_err()
        );
    }

    #[test]
    fn a_non_finite_ancestor_pull_is_a_parse_error() {
        // JSON has no NaN literal; a float that overflows `f32` parses to infinity.
        assert!(
            BeliefConfig::from_json_str(r#"{ "ancestor_pull": { "secular_devout": 1e60 } }"#)
                .is_err()
        );
    }

    #[test]
    fn the_pull_vector_places_each_value_on_its_axis_and_an_empty_map_is_zero() {
        let pull = BeliefConfig::default().ancestor_pull_vector();
        assert_eq!(
            pull[CultureTraitAxis::SecularDevout.index()],
            DEFAULT_PULL_SECULAR_DEVOUT
        );
        assert_eq!(
            pull[CultureTraitAxis::TraditionalistRevisionist.index()],
            DEFAULT_PULL_TRADITIONALIST_REVISIONIST
        );
        let off = BeliefConfig::from_json_str(r#"{ "ancestor_pull": {} }"#).expect("parses");
        assert!(off.ancestor_pull_vector().iter().all(|v| *v == NO_PULL));
    }
}
