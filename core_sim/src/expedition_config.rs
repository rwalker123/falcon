//! Data-driven tuning for scouting expeditions (traveling parties).
//!
//! Loaded from `data/expedition_config.json`. An **expedition** is a detached `StartingUnit` band
//! (a `PopulationCohort` tagged `Expedition`, deliberately lacking `ResidentBand`) that a faction
//! outfits with workers + provisions and drives out to explore. This config holds the levers that
//! shape how big a party can be, how far it can report from (communication range), what it observes
//! per turn, and how its carried provisions are drawn and consumed. Mirrors the `sites_config.rs` /
//! `fauna_config.rs` loader pattern (baked-in builtin + optional file/env override), so tuning an
//! expedition is a JSON edit, no code.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_EXPEDITION_CONFIG: &str = include_str!("data/expedition_config.json");

/// Root expedition configuration. Every lever here is a tuning knob read through the handle — no
/// bare literal drives expedition behavior. The builtin JSON carries all fields (it is the single
/// source of the shipped defaults), so no `#[serde(default)]` merge is needed; a malformed or
/// partial override file falls back to the builtin via [`load_expedition_config_from_env`].
#[derive(Debug, Clone, Deserialize)]
pub struct ExpeditionConfig {
    /// Hard cap on the workers a single expedition party can carry (also clamped to the home band's
    /// available workers at launch).
    pub max_party_size: u32,
    /// Base communication range (tiles): the expedition only flushes its observed tiles to the
    /// faction map while within this hex distance of its home band. Early-game default is short so
    /// distant exploration reports back "as a lump on return".
    pub comm_range_tiles: u32,
    /// Multiplier on `comm_range_tiles` reserved for a future movement/comm-tech signal. Stubbed at
    /// 1.0 today — mirrors migration's stubbed movement-tech factor.
    // TODO(phase2): scale by movement/comm tech; mirrors migration's stubbed factor.
    pub comm_range_tech_factor: f32,
    /// The expedition's per-turn line-of-sight observation radius. Default matches the band base
    /// sight range (`visibility_config.json` BandScout `base_range` 6) — an expedition sees as far
    /// as a normal band, it just reports on a comm-range delay.
    pub observe_sight_range: u32,
    /// Provisions drawn from the home band's larder at launch = `party × hex-distance-to-target ×
    /// this`.
    pub provision_draw_per_worker_per_tile: f32,
    /// Provisions the party consumes per turn = `party × this`. Non-fatal at zero in v1
    /// (deterministic success). Scouts only — a hunting party lives off its own kills.
    pub provision_upkeep_per_worker: f32,
    /// Hunting-expedition (PR 2) tuning — how a party follows a herd, harvests, and delivers.
    pub hunt: HuntExpeditionConfig,
    /// Scout opportunistic-replenish (PR 2) tuning — when/where a scout tops up off passing game.
    pub replenish: ReplenishConfig,
}

/// Hunting-expedition levers (`docs/plan_exploration_and_sites.md` §2b). A hunt party follows a
/// migratory herd, takes a **productive** hunt's worth of biomass each turn (`workers ×
/// per_worker_biomass_capacity`, floored per policy — see `advance_expeditions`), accumulates food up
/// to a carry cap, and delivers it. The take **policy** is chosen per-expedition at launch (on the
/// mission), not here.
#[derive(Debug, Clone, Deserialize)]
pub struct HuntExpeditionConfig {
    /// Carry cap = `party_workers × this` (provisions). Tuned so a party fills a cap in ~4–6 active
    /// turns at the productive take rate (`party × per_worker_biomass_capacity × provisions_per_biomass`).
    pub per_worker_carry: f32,
    /// How close (hex distance) the party must be to the herd to take food this turn.
    pub reach_tiles: u32,
    /// When the herd's circuit brings it within this hex distance of the home band, the party may
    /// flip to deliver early — but only with a worthwhile load (see `min_deliver_fraction`).
    pub drop_off_within_tiles: u32,
    /// **Sustain** floor: the party takes the herd down only to `this × carrying_capacity`, leaving
    /// it robust (default ~0.7). Surplus/Market instead floor at the ecology collapse threshold;
    /// Eradicate has no floor (extinction).
    pub sustain_floor_fraction: f32,
    /// Early-delivery gate: with the herd near the band (`drop_off_within_tiles`), only flip to
    /// deliver once `carried ≥ this × cap` (default 0.5) — fixes the empty-larder flip-flop.
    pub min_deliver_fraction: f32,
}

/// Scout opportunistic-replenish levers: the scout's own use of the shared `hunt_take` primitive.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplenishConfig {
    /// Top up when remaining provisions are below `party_workers × provision_upkeep_per_worker ×
    /// this` (i.e. fewer than this many turns of upkeep remain).
    pub low_turns: u32,
    /// The scout must be within this hex distance of a huntable herd to top up.
    pub reach_tiles: u32,
}

impl ExpeditionConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_EXPEDITION_CONFIG)
                .expect("builtin expedition config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, ExpeditionConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ExpeditionConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(ExpeditionConfig::from_json_str(&contents)?)
    }

    /// Effective communication range in tiles (`comm_range_tiles × comm_range_tech_factor`, rounded).
    pub fn effective_comm_range(&self) -> u32 {
        (self.comm_range_tiles as f32 * self.comm_range_tech_factor).round() as u32
    }
}

#[derive(Debug, Error)]
pub enum ExpeditionConfigError {
    #[error("failed to read expedition config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse expedition config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the expedition configuration.
#[derive(Resource, Debug, Clone)]
pub struct ExpeditionConfigHandle(pub Arc<ExpeditionConfig>);

impl ExpeditionConfigHandle {
    pub fn new(config: Arc<ExpeditionConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<ExpeditionConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<ExpeditionConfig>) {
        self.0 = config;
    }
}

impl Default for ExpeditionConfigHandle {
    fn default() -> Self {
        Self(ExpeditionConfig::builtin())
    }
}

/// Metadata about the expedition configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct ExpeditionConfigMetadata {
    path: Option<PathBuf>,
}

impl ExpeditionConfigMetadata {
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

/// Load expedition configuration from environment (`EXPEDITION_CONFIG_PATH`) or the default data
/// path, falling back to the baked-in builtin. Not wired into the `reload_config` hot-reload path
/// (mirrors `sites_config.json` / `fauna_config.json`).
pub fn load_expedition_config_from_env() -> (Arc<ExpeditionConfig>, ExpeditionConfigMetadata) {
    let override_path = env::var("EXPEDITION_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/expedition_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match ExpeditionConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "expedition_config.loaded=file"
                );
                return (Arc::new(config), ExpeditionConfigMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "expedition_config.load_failed"
                );
            }
        }
    }

    let config = ExpeditionConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "expedition_config.loaded=builtin"
    );
    (config, ExpeditionConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses() {
        let config = ExpeditionConfig::builtin();
        assert_eq!(config.max_party_size, 8);
        assert_eq!(config.comm_range_tiles, 2);
        assert_eq!(config.observe_sight_range, 6);
        assert!(config.provision_draw_per_worker_per_tile > 0.0);
        assert!(config.provision_upkeep_per_worker > 0.0);
        assert!(config.hunt.per_worker_carry > 0.0);
        assert!(config.hunt.reach_tiles >= 1);
        // Sustain floor must be a sensible fraction; min-deliver gate in (0, 1].
        assert!(
            config.hunt.sustain_floor_fraction > 0.0 && config.hunt.sustain_floor_fraction < 1.0
        );
        assert!(config.hunt.min_deliver_fraction > 0.0 && config.hunt.min_deliver_fraction <= 1.0);
        assert!(config.replenish.low_turns >= 1);
    }

    #[test]
    fn effective_comm_range_applies_factor() {
        let config = ExpeditionConfig::builtin();
        assert_eq!(config.effective_comm_range(), config.comm_range_tiles);
    }
}
