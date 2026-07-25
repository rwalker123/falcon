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

use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_SETTLEMENT_STAGE_CONFIG: &str = include_str!("data/settlement_stage_config.json");

/// The gate a band must clear to be *eligible* for a stage — an extensible record, not a bare
/// threshold. Every declared criterion must be satisfied for the stage to match (see
/// [`resolve_settlement_stage`]). Today the only criterion is `min_size`; enriching the signal set
/// later is purely additive here (e.g. `min_structures: Option<u32>`, `min_sedentarization:
/// Option<f32>`) — add an `Option` field, populate the matching field on [`SettlementStageInputs`],
/// and add one `&&` line to the resolver predicate. No config-iteration, builder, schema, or client
/// change.
#[derive(Debug, Clone, Deserialize)]
pub struct StageCriteria {
    /// Minimum band head-count for this stage. `0` always matches (the default/nomadic anchor).
    pub min_size: u32,
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
}

/// Root settlement-stage configuration: the ordered stage list. Defaults ship three stages whose
/// thresholds are calibrated to the early-game `size` scale — a starting band (`DEFAULT_STARTING_
/// BAND_SIZE` = 30, see `start_profile.rs`) resolves to `nomadic`; sustained growth reaches `camp`
/// (50) then `village` (100). **Provisional — meant to be tuned live.**
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SettlementStageConfig {
    pub stages: Vec<SettlementStageDef>,
}

impl Default for SettlementStageConfig {
    fn default() -> Self {
        // Keep in sync with `data/settlement_stage_config.json` (that file is authoritative when
        // present; this is the baked-in fallback).
        Self {
            stages: vec![
                SettlementStageDef {
                    id: "nomadic".to_string(),
                    label: "Nomadic band".to_string(),
                    icon: "⛺".to_string(),
                    criteria: StageCriteria { min_size: 0 },
                },
                SettlementStageDef {
                    id: "camp".to_string(),
                    label: "Seasonal camp".to_string(),
                    icon: "🛖".to_string(),
                    criteria: StageCriteria { min_size: 50 },
                },
                SettlementStageDef {
                    id: "village".to_string(),
                    label: "Village".to_string(),
                    icon: "🏘️".to_string(),
                    criteria: StageCriteria { min_size: 100 },
                },
            ],
        }
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

/// The per-band signals the resolver reads — an extensible record, not a bare `size`. The snapshot
/// builder constructs one per band. Adding a future signal (structure count, sedentarization,
/// tether, …) is a new field here, populated at the one call site; the resolver signature and its
/// callers stay unchanged.
///
/// INTERIM INPUT: the only signal today is band head-count (`size`). This size-only rule is a
/// deliberate proxy — Phase 3/4 enrich the inputs *inside* this record and the resolver predicate
/// without changing the snapshot field or the client.
#[derive(Debug, Clone, Copy)]
pub struct SettlementStageInputs {
    pub size: u32,
}

/// Whether `inputs` satisfies *every* criterion `criteria` declares. Extending the signal set is a
/// one-line addition here (e.g. `&& criteria.min_structures.is_none_or(|m| inputs.structures >= m)`).
fn criteria_match(criteria: &StageCriteria, inputs: &SettlementStageInputs) -> bool {
    criteria.min_size <= inputs.size
}

/// Resolve which settlement stage `inputs` places a band in: the highest-ordered entry in the
/// ordered `stages` list whose criteria are ALL satisfied, falling back to the first (default)
/// stage — which should declare `min_size: 0` so it always matches. Generic over the config list —
/// it never names a specific stage, so **adding a stage is a pure config edit** and this code is
/// untouched. Returns `None` only for an empty list.
pub fn resolve_settlement_stage<'a>(
    inputs: &SettlementStageInputs,
    stages: &'a [SettlementStageDef],
) -> Option<&'a SettlementStageDef> {
    stages
        .iter()
        .rev()
        .find(|stage| criteria_match(&stage.criteria, inputs))
        .or_else(|| stages.first())
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

    #[test]
    fn builtin_config_parses_and_is_ordered() {
        let config = SettlementStageConfig::builtin();
        assert!(!config.stages.is_empty(), "config must define stages");
        // A well-formed config anchors the low end at `min_size: 0` so every band resolves.
        assert_eq!(config.stages.first().unwrap().criteria.min_size, 0);
        // Thresholds are strictly increasing so the ordered list reads as a progression.
        for pair in config.stages.windows(2) {
            assert!(
                pair[1].criteria.min_size > pair[0].criteria.min_size,
                "stage thresholds must strictly increase: {} then {}",
                pair[0].criteria.min_size,
                pair[1].criteria.min_size
            );
        }
        // Presentation tokens are non-empty.
        for stage in &config.stages {
            assert!(!stage.id.is_empty());
            assert!(!stage.label.is_empty());
            assert!(!stage.icon.is_empty());
        }
    }

    #[test]
    fn builtin_matches_default() {
        // The baked-in JSON and the `Default` fallback must agree (they are two copies of the
        // same source of truth).
        let from_json = SettlementStageConfig::builtin();
        let from_default = SettlementStageConfig::default();
        assert_eq!(from_json.stages.len(), from_default.stages.len());
        for (a, b) in from_json.stages.iter().zip(from_default.stages.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.label, b.label);
            assert_eq!(a.criteria.min_size, b.criteria.min_size);
            assert_eq!(a.icon, b.icon);
        }
    }

    #[test]
    fn resolves_highest_eligible_stage() {
        let config = SettlementStageConfig::builtin();
        let stage_id = |size: u32| {
            resolve_settlement_stage(&SettlementStageInputs { size }, &config.stages)
                .unwrap()
                .id
                .clone()
        };
        // A starting band (DEFAULT_STARTING_BAND_SIZE = 30) is nomadic.
        assert_eq!(stage_id(30), "nomadic");
        assert_eq!(stage_id(0), "nomadic");
        assert_eq!(stage_id(50), "camp");
        assert_eq!(stage_id(99), "camp");
        assert_eq!(stage_id(100), "village");
        assert_eq!(stage_id(10_000), "village");
    }

    #[test]
    fn empty_stage_list_resolves_to_none() {
        assert!(resolve_settlement_stage(&SettlementStageInputs { size: 42 }, &[]).is_none());
    }
}
