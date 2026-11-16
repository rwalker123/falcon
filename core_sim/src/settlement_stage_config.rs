//! Data-driven tuning for a band's **settlement-progression stage**.
//!
//! Loaded from `data/settlement_stage_config.json`. The config is the *single source of truth* for
//! the set of stages: an ORDERED list of `SettlementStageDef { id, label, icon, criteria:
//! StageCriteria { min_size } }` — the size threshold is nested under the (extensible) `criteria`
//! record, not a flat field. The sim resolves which stage a band is in with
//! [`resolve_settlement_stage`], which takes a `SettlementStageInputs` record and returns the
//! highest-ordered stage whose `criteria` are all satisfied, then ships the winning stage's
//! presentation tokens on the population snapshot. `label`/`icon` are pass-through strings the sim
//! never interprets (`icon` is an emoji today, a sprite/asset key later).
//!
//! **Adding a settlement stage is a pure config edit** — append a list entry here; no Rust match
//! arm, no schema field, and no client change. Mirrors the `sedentarization_config.rs` /
//! `fauna_config.rs` loader (baked-in builtin + optional file/env override).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    config_load::{load_config_from_env, ConfigLoadError},
    intensification::LadderConfig,
    ladder_ids::RungId,
};

pub const BUILTIN_SETTLEMENT_STAGE_CONFIG: &str = include_str!("data/settlement_stage_config.json");

/// The gate a settlement must clear to be *eligible* for a stage — an extensible record, not a bare
/// threshold. Every declared criterion must be satisfied for the stage to match (see
/// [`resolve_settlement_stage`]). The four inputs are the settlement's people (`min_size`), the
/// highest food-store rung any member holds (`min_store_rung`), the food stored across the members'
/// larders and fixed stores (`min_food`) and the belief on the settlement's anchor tile
/// (`min_belief`). Everything but `min_size` is optional: an absent criterion always holds.
/// Enriching the signal set later is purely additive here — add an `Option` field, populate the
/// matching field on [`SettlementStageInputs`], and add one `&&` line to the predicate.
#[derive(Debug, Clone, Deserialize)]
pub struct StageCriteria {
    /// Minimum head-count summed over the settlement's bands. `0` always matches (the
    /// default/nomadic anchor).
    pub min_size: u32,
    /// The store rung some member's owned fixed store must hold at least (compared by the ladder's
    /// own `order` on the rung's branch).
    #[serde(default)]
    pub min_store_rung: Option<RungId>,
    /// Minimum food stored across the members' larders and owned fixed stores.
    #[serde(default)]
    pub min_food: Option<f32>,
    /// Minimum belief standing on the settlement's anchor tile.
    #[serde(default)]
    pub min_belief: Option<f32>,
}

impl StageCriteria {
    /// A row that asks for people alone.
    pub fn people(min_size: u32) -> Self {
        Self {
            min_size,
            min_store_rung: None,
            min_food: None,
            min_belief: None,
        }
    }
}

/// One settlement stage. `id` is a stable key, `label` a tooltip name, `icon` an opaque
/// presentation token (emoji now, asset key later). `criteria` is the (extensible) eligibility gate
/// — the resolver picks the highest-ordered stage whose criteria are all satisfied.
#[derive(Debug, Clone, Deserialize)]
pub struct SettlementStageDef {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub criteria: StageCriteria,
    /// Whether the first turn a settlement newly reaches this stage is told on the command feed
    /// (`CommandEventKind::SettlementStageReached`). The shipped ladder flags the city row.
    #[serde(default)]
    pub announce: bool,
}

/// Root settlement-stage configuration: the ordered stage list and the stage at which a settlement
/// starts pulling its neighbours. **Provisional — meant to be tuned live.**
#[derive(Debug, Clone, Deserialize)]
pub struct SettlementStageConfig {
    pub stages: Vec<SettlementStageDef>,
    /// The id of the stage from which a tribe's settlement pulls nearby bands into the tribe bank
    /// (`tribe::CloseInputs::settlement`); every stage below it pulls nothing.
    pub pull_from_stage: String,
}

impl Default for SettlementStageConfig {
    fn default() -> Self {
        // The baked-in JSON is the single source of truth; the Default parses it.
        Self::builtin().as_ref().clone()
    }
}

impl SettlementStageConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_SETTLEMENT_STAGE_CONFIG)
                .expect("builtin settlement stage config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, SettlementStageConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| SettlementStageConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        Ok(SettlementStageConfig::from_json_str(&contents)?)
    }
}

/// The per-settlement signals the resolver reads — an extensible record, not a bare `size`.
/// [`crate::settlement`] builds one per settlement (a tribe or a lone band). Adding a signal is a
/// new field here, populated at the one call site.
#[derive(Debug, Clone, Copy, Default)]
pub struct SettlementStageInputs {
    /// Head-count summed over the settlement's bands.
    pub size: u32,
    /// The highest fixed-store rung any member's owned store holds; `None` with no store.
    pub store: Option<RungId>,
    /// Food in the members' larders plus their owned fixed stores.
    pub food: f32,
    /// Belief on the settlement's anchor tile; `0` with no anchor.
    pub belief: f32,
}

impl SettlementStageInputs {
    /// The reading an observer can take of a band from outside: its people and nothing else.
    pub fn of_size(size: u32) -> Self {
        Self {
            size,
            ..Self::default()
        }
    }
}

/// Whether `inputs` satisfies *every* criterion `criteria` declares.
fn criteria_match(
    criteria: &StageCriteria,
    inputs: &SettlementStageInputs,
    ladder: &LadderConfig,
) -> bool {
    criteria.min_size <= inputs.size
        && criteria.min_store_rung.is_none_or(|required| {
            inputs
                .store
                .is_some_and(|held| ladder.at_or_above(held, required))
        })
        && criteria.min_food.is_none_or(|min| inputs.food >= min)
        && criteria.min_belief.is_none_or(|min| inputs.belief >= min)
}

/// Resolve which settlement stage `inputs` places a settlement in: the highest-ordered entry in the
/// ordered `stages` list whose criteria are ALL satisfied, falling back to the first (default)
/// stage — which should declare `min_size: 0` so it always matches. Generic over the config list —
/// it never names a specific stage, so **adding a stage is a pure config edit** and this code is
/// untouched. Returns `None` only for an empty list.
pub fn resolve_settlement_stage<'a>(
    inputs: &SettlementStageInputs,
    stages: &'a [SettlementStageDef],
    ladder: &LadderConfig,
) -> Option<&'a SettlementStageDef> {
    stage_index(inputs, stages, ladder).map(|index| &stages[index])
}

/// [`resolve_settlement_stage`] as the winning stage's index in `stages`.
pub fn stage_index(
    inputs: &SettlementStageInputs,
    stages: &[SettlementStageDef],
    ladder: &LadderConfig,
) -> Option<usize> {
    if stages.is_empty() {
        return None;
    }
    Some(
        stages
            .iter()
            .rposition(|stage| criteria_match(&stage.criteria, inputs, ladder))
            .unwrap_or(0),
    )
}

impl StageCriteria {
    /// Whether the row asks for people and nothing else.
    fn asks_people_only(&self) -> bool {
        self.min_store_rung.is_none() && self.min_food.is_none() && self.min_belief.is_none()
    }
}

impl SettlementStageConfig {
    /// **The stage an observer can read off a band from outside**: the highest row that asks for
    /// people alone and that `size` meets. Standing on a ridge shows how many there are, not what
    /// they have stored or buried, so a foreign band's marker reads this (a town or a city is
    /// only ever read from the inside).
    pub fn people_only_index(&self, size: u32) -> Option<usize> {
        self.stages
            .iter()
            .rposition(|stage| stage.criteria.asks_people_only() && stage.criteria.min_size <= size)
    }

    /// The index of the stage a settlement starts pulling from; `stages.len()` (never reached)
    /// when the id names no stage.
    pub fn pull_from_index(&self) -> usize {
        self.stages
            .iter()
            .position(|stage| stage.id == self.pull_from_stage)
            .unwrap_or(self.stages.len())
    }

    /// **The pull a settlement at stage `index` exerts**, in `[0, 1]`: the stage's place on the
    /// ladder (`index / last index`), and `0` below the pull stage. A ladder of one stage pulls
    /// nothing.
    pub fn pull_strength(&self, index: usize) -> f64 {
        let last = self.stages.len().saturating_sub(1);
        if last == 0 || index < self.pull_from_index() {
            return 0.0;
        }
        index.min(last) as f64 / last as f64
    }
}

#[derive(Debug, Error)]
pub enum SettlementStageConfigError {
    #[error("failed to read settlement stage config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse settlement stage config: {0}")]
    Parse(#[from] serde_json::Error),
}

impl ConfigLoadError for SettlementStageConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the settlement-stage configuration.
#[derive(Resource, Debug, Clone)]
pub struct SettlementStageConfigHandle(pub Arc<SettlementStageConfig>);

impl SettlementStageConfigHandle {
    pub fn new(config: Arc<SettlementStageConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<SettlementStageConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<SettlementStageConfig>) {
        self.0 = config;
    }
}

impl Default for SettlementStageConfigHandle {
    fn default() -> Self {
        Self(SettlementStageConfig::builtin())
    }
}

/// Metadata about the settlement-stage configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct SettlementStageConfigMetadata {
    path: Option<PathBuf>,
}

impl SettlementStageConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load settlement-stage config from environment (`SETTLEMENT_STAGE_CONFIG_PATH`) or the default
/// data path.
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `SETTLEMENT_STAGE_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_settlement_stage_config_from_env(
) -> (Arc<SettlementStageConfig>, SettlementStageConfigMetadata) {
    let (config, source) = load_config_from_env(
        "SETTLEMENT_STAGE_CONFIG_PATH",
        "settlement_stage_config",
        "src/data/settlement_stage_config.json",
        SettlementStageConfig::builtin,
        SettlementStageConfig::from_file,
    );
    (config, SettlementStageConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ladder() -> std::sync::Arc<LadderConfig> {
        LadderConfig::builtin()
    }

    fn stage_id(inputs: SettlementStageInputs) -> String {
        let config = SettlementStageConfig::builtin();
        resolve_settlement_stage(&inputs, &config.stages, &ladder())
            .unwrap()
            .id
            .clone()
    }

    fn rung(id: &str) -> RungId {
        id.parse().expect("a rung id")
    }

    /// Inputs that satisfy every non-people criterion of the shipped city row.
    fn city_grade(size: u32) -> SettlementStageInputs {
        SettlementStageInputs {
            size,
            store: Some(rung("food_stores:granary")),
            food: 1.0e6,
            belief: 1.0e6,
        }
    }

    #[test]
    fn builtin_config_parses_and_is_ordered() {
        let config = SettlementStageConfig::builtin();
        assert!(!config.stages.is_empty(), "config must define stages");
        assert_eq!(config.stages.first().unwrap().criteria.min_size, 0);
        for pair in config.stages.windows(2) {
            assert!(
                pair[1].criteria.min_size > pair[0].criteria.min_size,
                "stage thresholds must strictly increase: {} then {}",
                pair[0].criteria.min_size,
                pair[1].criteria.min_size
            );
        }
        for stage in &config.stages {
            assert!(!stage.id.is_empty());
            assert!(!stage.label.is_empty());
            assert!(!stage.icon.is_empty());
        }
        assert!(
            config.pull_from_index() < config.stages.len(),
            "pull_from_stage must name a stage"
        );
    }

    #[test]
    fn the_shipped_ladder_runs_nomadic_camp_village_town_city() {
        let config = SettlementStageConfig::builtin();
        let ids: Vec<&str> = config.stages.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["nomadic", "camp", "village", "town", "city"]);
        let announced: Vec<&str> = config
            .stages
            .iter()
            .filter(|s| s.announce)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(announced, ["city"]);
    }

    #[test]
    fn people_alone_resolve_up_to_village() {
        assert_eq!(stage_id(SettlementStageInputs::of_size(30)), "nomadic");
        assert_eq!(stage_id(SettlementStageInputs::of_size(0)), "nomadic");
        assert_eq!(stage_id(SettlementStageInputs::of_size(50)), "camp");
        assert_eq!(stage_id(SettlementStageInputs::of_size(99)), "camp");
        assert_eq!(stage_id(SettlementStageInputs::of_size(100)), "village");
        // Without a store, a crowd is still only a village.
        assert_eq!(stage_id(SettlementStageInputs::of_size(10_000)), "village");
    }

    #[test]
    fn a_store_rung_gates_town_and_city_by_order() {
        let config = SettlementStageConfig::builtin();
        let town = config.stages.iter().find(|s| s.id == "town").unwrap();
        let size = town.criteria.min_size;
        let with_store = |store: Option<RungId>| {
            stage_id(SettlementStageInputs {
                size,
                store,
                ..SettlementStageInputs::default()
            })
        };
        assert_eq!(with_store(None), "village");
        assert_eq!(with_store(Some(rung("food_stores:unstored"))), "village");
        assert_eq!(with_store(Some(rung("food_stores:storage_pit"))), "town");
        // The granary outranks the pit on the branch, so it satisfies the town row too.
        assert_eq!(with_store(Some(rung("food_stores:granary"))), "town");
    }

    #[test]
    fn the_city_row_needs_a_granary_food_and_belief() {
        let config = SettlementStageConfig::builtin();
        let city = config.stages.iter().find(|s| s.id == "city").unwrap();
        let size = city.criteria.min_size;
        assert_eq!(stage_id(city_grade(size)), "city");
        assert_eq!(stage_id(city_grade(size - 1)), "town");
        let pit = Some(rung("food_stores:storage_pit"));
        assert_eq!(
            stage_id(SettlementStageInputs {
                store: pit,
                ..city_grade(size)
            }),
            "town",
            "a pit is not a granary"
        );
        assert_eq!(
            stage_id(SettlementStageInputs {
                food: city.criteria.min_food.unwrap() - 1.0,
                ..city_grade(size)
            }),
            "town",
            "too little stored food"
        );
        assert_eq!(
            stage_id(SettlementStageInputs {
                belief: city.criteria.min_belief.unwrap() - 0.5,
                ..city_grade(size)
            }),
            "town",
            "too little belief on the anchor"
        );
    }

    #[test]
    fn the_pull_is_zero_below_the_pull_stage_and_grows_to_one_at_the_top() {
        let config = SettlementStageConfig::builtin();
        let village = config.pull_from_index();
        assert!(config.pull_strength(village - 1).abs() < f64::EPSILON);
        let last = config.stages.len() - 1;
        assert!((config.pull_strength(last) - 1.0).abs() < f64::EPSILON);
        assert!(config.pull_strength(village) > 0.0);
        assert!(config.pull_strength(village) < config.pull_strength(last));
    }

    #[test]
    fn empty_stage_list_resolves_to_none() {
        assert!(
            resolve_settlement_stage(&SettlementStageInputs::of_size(42), &[], &ladder()).is_none()
        );
    }
}
