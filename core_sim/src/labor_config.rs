//! Data-driven labor-allocation tuning (Early-Game Labor, slice 3a).
//!
//! Loaded from `data/labor_config.json`. Drives the source-centric labor pool: the
//! band's work range, the leashed-follow reach for hunting, per-turn band movement,
//! and the flat per-worker throughput tiers for Forage / Hunt / Scout. Mirrors the
//! `fauna_config.rs` loader pattern (baked-in builtin + optional file/env override).
//!
//! No magic numbers: every lever a system reads lives here.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::fauna_config::EcologyConfig;

pub const BUILTIN_LABOR_CONFIG: &str = include_str!("data/labor_config.json");

/// Named-const defaults for the depletable-forage ecology (Intensification §0-ii). All are
/// **tuning dials** (settle live): the per-patch cap, the gather throughput, the biomass→provisions
/// conversion, and the ecology dynamics. `regrowth_rate` is tuned **higher than fauna's 0.05** —
/// patches regrow faster than game. `extinction_floor` is `0.0` because forage patches never
/// despawn (a crashed patch sits at low biomass and recovers via `logistic_regrowth`).
const DEFAULT_FORAGE_CARRYING_CAPACITY: f32 = 120.0;
const DEFAULT_FORAGE_PER_WORKER_BIOMASS_CAPACITY: f32 = 8.0;
const DEFAULT_FORAGE_PROVISIONS_PER_BIOMASS: f32 = 0.05;
const DEFAULT_FORAGE_REGROWTH_RATE: f32 = 0.25;
const DEFAULT_FORAGE_COLLAPSE_FRACTION: f32 = 0.15;
const DEFAULT_FORAGE_COLLAPSE_RATE: f32 = 0.20;
const DEFAULT_FORAGE_STRESSED_FRACTION: f32 = 0.40;
const DEFAULT_FORAGE_EXTINCTION_FLOOR: f32 = 0.0;
/// Reseed standing crop as a fraction of a patch's carrying capacity (Intensification §0-ii). A
/// depleted patch is reseeded up to this floor before regrowth, so a patch driven to exactly `0`
/// (repeated Eradicate + f32 underflow, `take_fraction = 1.0`, or a snapshot carrying `biomass = 0`)
/// still has a seed stock to regrow from — plants reseed from surrounding vegetation, so a crashed
/// patch **always recovers** (the invariant `regrow_patch` promises). Kept small (2% of cap, below
/// `collapse_fraction`) so Eradicate still crashes a patch hard into the Collapsing band — it just
/// can't drive it *permanently* to 0.
const DEFAULT_FORAGE_RESEED_FLOOR_FRACTION: f32 = 0.02;

/// Named-const defaults for the forage **policy axis** (Intensification §0-iii — "forage parity
/// with hunting"). These mirror the fauna `follow`/`market`/`hunt` levers so a gather policy
/// behaves like the matching hunt policy: **Surplus** overdraws the Sustain regrowth skim by
/// `surplus_multiplier` (fauna `follow.surplus_multiplier`), **Market** takes a commercial share
/// `market.take_fraction` of the patch and sells it at `trade_goods_multiplier`× the base
/// `trade_goods_per_biomass` rate (fauna `market.*` + `hunt.trade_goods_per_biomass`), and
/// **Eradicate** strips the patch by `eradicate.take_fraction` (fauna `hunt.take_fraction`).
const DEFAULT_FORAGE_SURPLUS_MULTIPLIER: f32 = 1.6;
const DEFAULT_FORAGE_MARKET_TAKE_FRACTION: f32 = 0.20;
const DEFAULT_FORAGE_MARKET_TRADE_GOODS_MULTIPLIER: f32 = 4.0;
const DEFAULT_FORAGE_MARKET_TRADE_GOODS_PER_BIOMASS: f32 = 0.005;
const DEFAULT_FORAGE_ERADICATE_TAKE_FRACTION: f32 = 0.30;

/// Named-const defaults for **cultivation** (Intensification Phase 1a — the plant analog of fauna
/// husbandry, `fauna_config::HusbandryConfig`). `progress_per_turn` must exceed `decay_per_turn` so
/// a Sustain-foraged Thriving patch nets forward; `claim_threshold` is the early-claim gate; and
/// `provisions_per_biomass` is the **STEADY tended-yield** rate — deliberately distinct from the
/// gather `ForageLaborConfig::provisions_per_biomass` (a cultivated patch yields without being drawn
/// down).
const DEFAULT_CULTIVATION_PROGRESS_PER_TURN: f32 = 0.04;
const DEFAULT_CULTIVATION_DECAY_PER_TURN: f32 = 0.01;
const DEFAULT_CULTIVATION_CLAIM_THRESHOLD: f32 = 0.6;
const DEFAULT_CULTIVATION_PROVISIONS_PER_BIOMASS: f32 = 0.01;

/// Cultivation tuning (Intensification Phase 1a): a sustained **Sustain** forage on a **Thriving**
/// patch accrues `progress_per_turn` toward cultivation (`1.0` = cultivated); progress that isn't
/// being actively sustained decays by `decay_per_turn`. The explicit `cultivate` command may claim
/// a patch early once progress reaches `claim_threshold`. A cultivated patch yields
/// `biomass × provisions_per_biomass` provisions to its owner each turn **without** drawing the
/// patch down. The plant mirror of fauna's `HusbandryConfig`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultivationConfig {
    /// Cultivation gained per turn while a band Sustain-forages a Thriving patch.
    pub progress_per_turn: f32,
    /// Cultivation lost per turn on a patch that isn't being actively tended.
    pub decay_per_turn: f32,
    /// Progress at which the `cultivate` command may claim the patch early (snaps to 1.0).
    pub claim_threshold: f32,
    /// **STEADY** tended-yield rate: a cultivated patch pays `biomass × this` provisions/turn to its
    /// owner without depleting biomass. Distinct from the gather `provisions_per_biomass`.
    pub provisions_per_biomass: f32,
}

impl Default for CultivationConfig {
    fn default() -> Self {
        Self {
            progress_per_turn: DEFAULT_CULTIVATION_PROGRESS_PER_TURN,
            decay_per_turn: DEFAULT_CULTIVATION_DECAY_PER_TURN,
            claim_threshold: DEFAULT_CULTIVATION_CLAIM_THRESHOLD,
            provisions_per_biomass: DEFAULT_CULTIVATION_PROVISIONS_PER_BIOMASS,
        }
    }
}

/// Forage **Market** policy tuning (Intensification §0-iii): a commercial gather that takes
/// `take_fraction` of the patch's biomass each turn and sells it — the raw take yields
/// `take × trade_goods_per_biomass × trade_goods_multiplier` trade goods (the plant mirror of
/// fauna's `market` block + `hunt.trade_goods_per_biomass`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ForageMarketConfig {
    /// Fraction of the patch's remaining biomass a Market gather targets each turn (the ceiling
    /// before the throughput/biomass clamps).
    pub take_fraction: f32,
    /// Multiplier applied to the base trade-goods rate for gathered-for-sale goods.
    pub trade_goods_multiplier: f32,
    /// Base trade goods yielded per unit of biomass taken.
    pub trade_goods_per_biomass: f32,
}

impl Default for ForageMarketConfig {
    fn default() -> Self {
        Self {
            take_fraction: DEFAULT_FORAGE_MARKET_TAKE_FRACTION,
            trade_goods_multiplier: DEFAULT_FORAGE_MARKET_TRADE_GOODS_MULTIPLIER,
            trade_goods_per_biomass: DEFAULT_FORAGE_MARKET_TRADE_GOODS_PER_BIOMASS,
        }
    }
}

/// Forage **Eradicate** policy tuning (Intensification §0-iii): an aggressive strip that takes
/// `take_fraction` of the patch's biomass with no floor (the plant mirror of fauna's
/// `hunt.take_fraction`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ForageEradicateConfig {
    /// Fraction of the patch's remaining biomass an Eradicate gather targets each turn.
    pub take_fraction: f32,
}

impl Default for ForageEradicateConfig {
    fn default() -> Self {
        Self {
            take_fraction: DEFAULT_FORAGE_ERADICATE_TAKE_FRACTION,
        }
    }
}

/// Depletable-forage tuning (Intensification §0-ii). A worked `FoodModuleTag` tile carries a
/// mutable per-patch `biomass`/`carrying_capacity` (`ForageRegistry`) that foraging draws down and
/// that regrows logistically toward `carrying_capacity` — the herd biomass model transposed onto
/// plants. Supersedes the retired flat `per_worker_yield` lever.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ForageLaborConfig {
    /// Per-patch carrying cap that patch biomass regrows toward (a flat default; a per-`FoodModule`
    /// table is a later refinement). Each seeded patch starts full at this value.
    pub carrying_capacity: f32,
    /// Biomass one forager can gather per turn (× `seasonal_weight`), capped by the policy ceiling
    /// (Sustain = one turn's net regrowth) and the patch's remaining biomass — the forage
    /// counterpart of `hunt.per_worker_biomass_capacity`.
    pub per_worker_biomass_capacity: f32,
    /// Biomass→provisions conversion for a gather take (the forage counterpart of
    /// `fauna.hunt.provisions_per_biomass`).
    pub provisions_per_biomass: f32,
    /// Depletion/regrowth dynamics (reuses fauna's `EcologyConfig`; forage regrows *faster* than
    /// game via a higher `regrowth_rate`). `collapse_fraction`/`stressed_fraction` classify the
    /// patch's ecology phase with the same ordering invariant. This config feeds `sustainable_yield`
    /// (the MSY-based Sustain ceiling, regrowth evaluated at the most-productive biomass K/2) — patch
    /// *regrowth* itself is pure logistic (plants have no critical-depensation crash), so a depleted
    /// patch recovers.
    pub ecology: EcologyConfig,
    /// Reseed standing crop, as a **fraction of `carrying_capacity`**, that a depleted patch is
    /// lifted to *before* logistic regrowth each turn (`regrow_patch`). This models plants
    /// reseeding from surrounding vegetation, so a patch driven to exactly `0` still has a seed
    /// stock and recovers via normal regrowth — the "a feral patch always recovers" invariant.
    /// Only affects patches below the floor (a healthy patch is untouched); kept small (below
    /// `collapse_fraction`) so Eradicate still crashes a patch hard, just never permanently to 0.
    pub reseed_floor_fraction: f32,
    /// **Surplus** policy multiplier on the Sustain (net-regrowth) ceiling (§0-iii). `> 1.0` so a
    /// Surplus gather overdraws a healthy patch — the plant mirror of `follow.surplus_multiplier`.
    pub surplus_multiplier: f32,
    /// **Market** policy tuning (§0-iii): a commercial gather share + the gathered-goods trade-goods
    /// conversion.
    pub market: ForageMarketConfig,
    /// **Eradicate** policy tuning (§0-iii): the aggressive strip-the-patch share.
    pub eradicate: ForageEradicateConfig,
    /// **Cultivation** tuning (Phase 1a): the plant analog of fauna husbandry — Sustain-forage
    /// accrual, decay, early-claim gate, and the steady tended-yield rate.
    pub cultivation: CultivationConfig,
}

impl Default for ForageLaborConfig {
    fn default() -> Self {
        Self {
            carrying_capacity: DEFAULT_FORAGE_CARRYING_CAPACITY,
            per_worker_biomass_capacity: DEFAULT_FORAGE_PER_WORKER_BIOMASS_CAPACITY,
            provisions_per_biomass: DEFAULT_FORAGE_PROVISIONS_PER_BIOMASS,
            ecology: EcologyConfig {
                regrowth_rate: DEFAULT_FORAGE_REGROWTH_RATE,
                collapse_fraction: DEFAULT_FORAGE_COLLAPSE_FRACTION,
                collapse_rate: DEFAULT_FORAGE_COLLAPSE_RATE,
                stressed_fraction: DEFAULT_FORAGE_STRESSED_FRACTION,
                extinction_floor: DEFAULT_FORAGE_EXTINCTION_FLOOR,
            },
            reseed_floor_fraction: DEFAULT_FORAGE_RESEED_FLOOR_FRACTION,
            surplus_multiplier: DEFAULT_FORAGE_SURPLUS_MULTIPLIER,
            market: ForageMarketConfig::default(),
            eradicate: ForageEradicateConfig::default(),
            cultivation: CultivationConfig::default(),
        }
    }
}

/// Flat per-worker hunt throughput tier.
#[derive(Debug, Clone, Deserialize)]
pub struct HuntLaborConfig {
    /// Biomass one hunter can take per turn, capped by the policy ceiling (Sustain =
    /// net regrowth, etc.). The biomass→provisions/trade conversion reuses
    /// `fauna_config`'s `hunt.*_per_biomass` so the ecology stays consistent.
    pub per_worker_biomass_capacity: f32,
}

/// Band-wide scout role tuning: staffed scouts act as **forward observers**. Instead of
/// bumping the band's sight radius, they post vantage points out from the band in all six
/// hex directions and compute line-of-sight from each, so scouting reveals *around*
/// obstacles (ridges/forest), not just farther. No resource yield.
#[derive(Debug, Clone, Deserialize)]
pub struct ScoutLaborConfig {
    /// Base distance (tiles) a vantage is posted out from the band with ≥1 scout.
    pub vantage_distance_base: u32,
    /// Additional vantage distance per staffed scout (more scouts → ring farther out).
    pub vantage_distance_per_scout: u32,
    /// Upper bound on how far a vantage is posted regardless of head-count.
    pub vantage_distance_max: u32,
    /// Sight range (tiles) each posted vantage reveals via the band's normal LOS.
    pub vantage_range: u32,
}

impl ScoutLaborConfig {
    /// How far vantages are posted out from the band for a cohort staffing `scouts`
    /// workers: `min(vantage_distance_base + scouts × vantage_distance_per_scout,
    /// vantage_distance_max)`. Zero scouts → `0` (no vantages posted).
    pub fn vantage_distance(&self, scouts: u32) -> u32 {
        if scouts == 0 {
            return 0;
        }
        self.vantage_distance_base
            .saturating_add(scouts.saturating_mul(self.vantage_distance_per_scout))
            .min(self.vantage_distance_max)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LaborConfig {
    /// True odd-r **hex-distance** radius (`grid_utils::hex_distance_wrapped`, wrap-aware)
    /// of in-range assignable sources around the band's tile.
    pub band_work_range: u32,
    /// Sight range (tiles) each worked source tile (a Forage tile or a Hunt herd's current
    /// tile) reveals via the band's normal LOS in `calculate_visibility`. Workers stand at
    /// the sources they exploit, so those spots provide fog reveal like the band center and
    /// scout vantages do.
    pub worked_source_sight_range: u32,
    /// Extra distance beyond `band_work_range` a Hunt assignment reaches (leashed
    /// follow) before it lapses and returns its workers to the pool.
    pub hunt_leash_tiles: u32,
    /// Tiles a `move_band` order advances the band toward its target each turn.
    pub band_move_tiles_per_turn: u32,
    pub forage: ForageLaborConfig,
    pub hunt: HuntLaborConfig,
    pub scout: ScoutLaborConfig,
}

impl LaborConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            serde_json::from_str(BUILTIN_LABOR_CONFIG).expect("builtin labor config should parse"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn from_file(path: &Path) -> Result<Self, LaborConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| LaborConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(LaborConfig::from_json_str(&contents)?)
    }

    /// Distance (inclusive) at which a Hunt assignment still yields before lapsing.
    pub fn hunt_reach(&self) -> u32 {
        self.band_work_range + self.hunt_leash_tiles
    }
}

#[derive(Debug, Error)]
pub enum LaborConfigError {
    #[error("failed to read labor config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse labor config: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Handle for accessing the labor configuration.
#[derive(Resource, Debug, Clone)]
pub struct LaborConfigHandle(pub Arc<LaborConfig>);

impl LaborConfigHandle {
    pub fn new(config: Arc<LaborConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<LaborConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<LaborConfig>) {
        self.0 = config;
    }
}

impl Default for LaborConfigHandle {
    fn default() -> Self {
        Self(LaborConfig::builtin())
    }
}

/// Metadata about the labor configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct LaborConfigMetadata {
    path: Option<PathBuf>,
}

impl LaborConfigMetadata {
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

/// Load labor configuration from environment (`LABOR_CONFIG_PATH`) or the default data
/// path, falling back to the baked-in builtin.
pub fn load_labor_config_from_env() -> (Arc<LaborConfig>, LaborConfigMetadata) {
    let override_path = env::var("LABOR_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/labor_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match LaborConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "labor_config.loaded=file"
                );
                return (Arc::new(config), LaborConfigMetadata::new(Some(path)));
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "labor_config.load_failed"
                );
            }
        }
    }

    let config = LaborConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "labor_config.loaded=builtin"
    );
    (config, LaborConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses() {
        let config = LaborConfig::builtin();
        assert!(config.band_work_range >= 1);
        assert!(config.worked_source_sight_range >= 1);
        assert!(config.hunt_leash_tiles >= 1);
        assert!(config.band_move_tiles_per_turn >= 1);
        // Depletable-forage levers (Intensification §0-ii).
        assert!(config.forage.carrying_capacity > 0.0);
        assert!(config.forage.per_worker_biomass_capacity > 0.0);
        assert!(config.forage.provisions_per_biomass > 0.0);
        assert!(config.forage.ecology.regrowth_rate > 0.0);
        // Ecology-phase ordering invariant (collapse band below stressed band).
        assert!(config.forage.ecology.collapse_fraction < config.forage.ecology.stressed_fraction);
        // Reseed floor is a small positive standing crop below the collapse band, so a crashed
        // patch recovers from it while Eradicate still bottoms the patch out in Collapsing.
        assert!(config.forage.reseed_floor_fraction > 0.0);
        assert!(config.forage.reseed_floor_fraction < config.forage.ecology.collapse_fraction);
        // Forage policy axis (§0-iii): Surplus overdraws the Sustain skim, Market/Eradicate take a
        // fractional commercial/strip share, Market sells at a boosted trade-goods rate.
        assert!(config.forage.surplus_multiplier > 1.0);
        assert!(config.forage.market.take_fraction > 0.0);
        assert!(config.forage.market.take_fraction < 1.0);
        assert!(config.forage.market.trade_goods_multiplier > 1.0);
        assert!(config.forage.market.trade_goods_per_biomass > 0.0);
        assert!(config.forage.eradicate.take_fraction > 0.0);
        assert!(config.forage.eradicate.take_fraction <= 1.0);
        // Cultivation (Phase 1a): progress outruns decay so a tended patch nets forward, the claim
        // gate sits strictly in (0, 1), and the steady tended-yield is positive.
        assert!(
            config.forage.cultivation.progress_per_turn > config.forage.cultivation.decay_per_turn
        );
        assert!(config.forage.cultivation.claim_threshold > 0.0);
        assert!(config.forage.cultivation.claim_threshold < 1.0);
        assert!(config.forage.cultivation.provisions_per_biomass > 0.0);
        assert!(config.hunt.per_worker_biomass_capacity > 0.0);
        assert!(config.scout.vantage_distance_base >= 1);
        assert!(config.scout.vantage_distance_max >= config.scout.vantage_distance_base);
        assert!(config.scout.vantage_range >= 1);
        assert_eq!(
            config.hunt_reach(),
            config.band_work_range + config.hunt_leash_tiles
        );
    }

    #[test]
    fn scout_vantage_distance_scales_with_headcount_and_caps() {
        // Vantages are posted `vantage_distance(scouts)` tiles out from the band, scaling
        // linearly per scout and clamping at `vantage_distance_max`.
        let scout = ScoutLaborConfig {
            vantage_distance_base: 2,
            vantage_distance_per_scout: 1,
            vantage_distance_max: 6,
            vantage_range: 2,
        };

        // 0 scouts → no vantages posted at all.
        assert_eq!(scout.vantage_distance(0), 0);

        // N scouts below the cap → base + N × per-scout.
        assert_eq!(scout.vantage_distance(1), 3);
        assert_eq!(scout.vantage_distance(3), 5);

        // Above the cap → clamped to vantage_distance_max (never grows past it).
        assert_eq!(scout.vantage_distance(4), 6);
        assert_eq!(scout.vantage_distance(99), 6);
    }
}
