//! Data-driven labor-allocation tuning (Early-Game Labor, slice 3a).
//!
//! Loaded from `data/labor_config.json`. Drives the source-centric labor pool: the
//! band's work range, the leashed-follow reach for hunting, per-turn band movement,
//! and the flat per-worker throughput tiers for Forage / Hunt / Scout. Mirrors the
//! `fauna_config.rs` loader pattern (baked-in builtin + optional file/env override).
//!
//! No magic numbers: every lever a system reads lives here.

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use sim_runtime::TerrainType;
use thiserror::Error;

use crate::fauna_config::EcologyConfig;

pub const BUILTIN_LABOR_CONFIG: &str = include_str!("data/labor_config.json");

/// Named-const defaults for the depletable-forage ecology (Intensification §0-ii). All are
/// **tuning dials** (settle live): the gather throughput, the biomass→provisions conversion, and
/// the ecology dynamics. `regrowth_rate` is tuned **higher than fauna's 0.05** — patches regrow
/// faster than game. `extinction_floor` is `0.0` because forage patches never despawn (a crashed
/// patch sits at low biomass and recovers via `logistic_regrowth`). The per-patch *capacity* is no
/// longer a scalar default: it is [`ForageLaborConfig::capacity_by_biome`], a per-biome table (the
/// human-edible twin of `fauna_config`'s `graze.capacity_by_biome`).
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

/// Extra forage (human-food) capacity a `NavigableRiver` hex carries **on top of** the biome it was
/// cut through (`navigable_forage_capacity`). A giant river is always a fishery — freshwater fish,
/// waterfowl, cattail — so a navigable hex always seeds a forage patch, adding this bonus to
/// `capacity_for(underlying_terrain)` even where the underlying biome is otherwise barren. The old
/// fixed `NavigableRiver` row (130) is now vestigial (the tile reads its underlying biome); this is
/// **additive** on top of that biome, so it starts conservative.
const DEFAULT_NAVIGABLE_RIVER_FORAGE_BONUS: f32 = 80.0;

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
/// a patch worked under the **Cultivate** policy nets forward against the feral decay; and
/// `tended_provisions_per_biomass` is the **tended-harvest** rate — deliberately distinct from the
/// gather `ForageLaborConfig::provisions_per_biomass` (a tended patch is harvested on its full
/// standing biomass without being drawn down).
///
/// **Tuning — a tended patch out-yields the same patch's wild MSY (the intensification incentive).**
/// A tended patch is never drawn down, so its biomass regrows toward its tile's cap `K` and the
/// per-turn tended yield settles at `K × tended_provisions_per_biomass`. The best a *wild* patch can
/// sustainably yield is MSY = regrowth at `K/2` = `regrowth_rate × K/4`, in provisions
/// `regrowth_rate × K/4 × provisions_per_biomass` (the gather rate). **Both are linear in `K`, so the
/// incentive is scale-free** — it holds on every biome in `capacity_by_biome`, which is why the
/// per-biome table can be retuned without re-deriving it. Keep
/// `tended_provisions_per_biomass > regrowth_rate/4 × forage.provisions_per_biomass` so intensifying
/// always pays. At the shipped rates (`regrowth_rate` 0.25, gather 0.05, tended 0.01) tended pays
/// **3.2×** the wild sustainable skim on *any* tile — e.g. on an `AlluvialPlain` (`K` = 195): wild MSY
/// `0.25 × 48.75 × 0.05` = **0.61 prov/turn** vs tended `195 × 0.01` = **1.95 prov/turn**. (The tended
/// *per-biomass* rate is lower than the gather rate, but tended harvests the whole standing crop every
/// turn, not just the regrowth skim.)
const DEFAULT_CULTIVATION_PROGRESS_PER_TURN: f32 = 0.04;
const DEFAULT_CULTIVATION_DECAY_PER_TURN: f32 = 0.01;
const DEFAULT_CULTIVATION_TENDED_PROVISIONS_PER_BIOMASS: f32 = 0.01;
/// **The investment cost of cultivating.** While a patch is being prepared (worked under the
/// `Cultivate` policy, progress < 1.0) the crew is clearing and planting, not gathering: its take
/// ceiling is only this fraction of the patch's **Sustain (MSY)** ceiling. Drawn at a fraction of
/// MSY the take is sustainable, so the patch stays Thriving (which the accrual gate requires) —
/// the cost is a pure **yield dip**, not a depletion.
///
/// **Break-even at the shipped defaults** (`0.25`, `progress_per_turn` 0.04 → 25 turns to prepare):
/// preparing costs ~75% of that patch's Sustain yield for ~25 turns ≈ `0.75 × 0.375 × 25` ≈ **7
/// prov** forgone; a tended patch then out-pays wild Sustain by `1.2 − 0.375` = **0.825 prov/turn**,
/// so the investment is recouped ~8-9 turns after completion. Cultivating is therefore only correct
/// when you intend to stay — the decision the free auto-accrual used to erase.
const DEFAULT_CULTIVATION_CULTIVATING_YIELD_FRACTION: f32 = 0.25;
/// Faction **Cultivation** knowledge earned per turn a band Sustain-forages a Thriving patch
/// (Rung 1b). At `0.05`/turn the knowledge completes (`>= knowledge_completion_threshold`) in ~20
/// Sustain-forage turns — so the knowledge is in hand before a player has any reason to pay the
/// Cultivate dip.
const DEFAULT_CULTIVATION_KNOWLEDGE_PROGRESS_PER_TURN: f32 = 0.05;
/// Ledger progress (`0..=1`) at which the faction **knows** Cultivation and patches may accrue
/// cultivation under the Cultivate policy. `1.0` = the ledger's completion value
/// (`DiscoveryProgressLedger` clamps accrual to `1.0`).
const DEFAULT_CULTIVATION_KNOWLEDGE_COMPLETION_THRESHOLD: f32 = 1.0;

/// Cultivation tuning (Intensification Phase 1a): a patch worked under the explicit **Cultivate**
/// policy (`FollowPolicy::Cultivate`) — faction knows Cultivation, patch is **Thriving** — accrues
/// `progress_per_turn` toward cultivation (`1.0` = cultivated) while yielding only
/// `cultivating_yield_fraction × its Sustain (MSY) ceiling` (the investment cost). A cultivated patch
/// that isn't tended any given turn goes **feral**, its progress decaying by `decay_per_turn` back
/// below `1.0` (reverting to a wild gather patch). A tended patch pays the band that tends it
/// `biomass × tended_provisions_per_biomass` provisions each turn **without** drawing the patch down
/// (place-local — see `advance_labor_allocation`). The plant mirror of fauna's `HusbandryConfig`.
///
/// There is **no early claim**: a `claim_threshold` that snapped progress to `1.0` would let the
/// player skip the investment, which is the whole decision. The `cultivate` command now *sets the
/// policy* instead.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultivationConfig {
    /// Cultivation gained per turn while a band works the patch under the **Cultivate** policy (and
    /// the faction knows Cultivation and the patch is Thriving).
    pub progress_per_turn: f32,
    /// Cultivation lost per turn on a patch that isn't being actively worked (neither tended nor
    /// under Cultivate) — the feral-reversion rate. An untended tended patch drops below `1.0`
    /// (→ wild) the first turn and fully decays to 0 over ~`1/decay_per_turn` turns.
    pub decay_per_turn: f32,
    /// **The investment cost** (see `DEFAULT_CULTIVATION_CULTIVATING_YIELD_FRACTION`): while
    /// preparing, the patch's take ceiling is this fraction of its Sustain (MSY) ceiling.
    /// Validated `0 < f < 1`.
    pub cultivating_yield_fraction: f32,
    /// **Tended-harvest** rate: a tended patch pays the tending band `biomass × this` provisions/turn
    /// on its full standing crop, without depleting biomass. Tuned so a tended patch out-yields the
    /// same patch's wild MSY skim (see the module-level tuning note). Distinct from the gather
    /// `provisions_per_biomass`.
    pub tended_provisions_per_biomass: f32,
    /// **Rung 1b — earned knowledge.** Faction-level Cultivation knowledge accrued per turn while a
    /// band **Sustain**-forages a Thriving patch (into the `DiscoveryProgressLedger`, discovery
    /// `CULTIVATION_DISCOVERY_ID`). Cultivation is *learned by foraging*, never start-granted; a patch
    /// cannot accrue `cultivation_progress` (under Cultivate) until the faction knows it.
    pub knowledge_progress_per_turn: f32,
    /// Ledger progress (`0..=1`) at which the faction **knows** Cultivation: patches may then accrue
    /// cultivation under the Cultivate policy. `1.0` = the ledger's completion value.
    pub knowledge_completion_threshold: f32,
}

impl Default for CultivationConfig {
    fn default() -> Self {
        Self {
            progress_per_turn: DEFAULT_CULTIVATION_PROGRESS_PER_TURN,
            decay_per_turn: DEFAULT_CULTIVATION_DECAY_PER_TURN,
            cultivating_yield_fraction: DEFAULT_CULTIVATION_CULTIVATING_YIELD_FRACTION,
            tended_provisions_per_biomass: DEFAULT_CULTIVATION_TENDED_PROVISIONS_PER_BIOMASS,
            knowledge_progress_per_turn: DEFAULT_CULTIVATION_KNOWLEDGE_PROGRESS_PER_TURN,
            knowledge_completion_threshold: DEFAULT_CULTIVATION_KNOWLEDGE_COMPLETION_THRESHOLD,
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

/// A biome on which **nothing human-edible grows** (open water outside the shelf, glacier, lava,
/// salt flat). Named rather than bare so a `0.0` in the table reads as *"deliberately barren"* and a
/// `0.0` in code reads as *"the same thing"*, not as a fallback that lost its lookup. A
/// `FoodModuleTag` tile whose biome reads `NO_FORAGE_CAPACITY` is **not seeded a patch at all**
/// (`spawn_initial_forage`), exactly as a zero-graze tile holds no `GrazePatch`.
pub const NO_FORAGE_CAPACITY: f32 = 0.0;

/// Depletable-forage tuning (Intensification §0-ii). A worked `FoodModuleTag` tile carries a
/// mutable per-patch `biomass`/`carrying_capacity` (`ForageRegistry`) that foraging draws down and
/// that regrows logistically toward `carrying_capacity` — the herd biomass model transposed onto
/// plants. Supersedes the retired flat `per_worker_yield` lever.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ForageLaborConfig {
    /// **The human food web, by biome** — human-edible biomass (seeds, nuts, tubers, fruit, shellfish,
    /// inshore fish) a tile of each biome carries at capacity. Each seeded patch starts full at this
    /// value; a `FoodModuleTag` tile whose biome reads [`NO_FORAGE_CAPACITY`] carries no patch at all.
    ///
    /// **A pure data table, not a formula**, and the exact mirror of `fauna_config`'s
    /// `graze.capacity_by_biome` — the *animal* food web. The two tables are per-**biome** (not
    /// per-`FoodModule`) precisely so they are directly comparable tile-for-tile and can **disagree
    /// within a module**: that disagreement *is* the agropastoral decision (`docs/plan_grazing_foundation.md`
    /// §1). Every one of the 37 [`TerrainType`]s must appear (enforced by [`LaborConfig::validate`]:
    /// a missing biome would silently read as an invisible zero-forage dead zone — **zero must be
    /// stated, never defaulted**).
    ///
    /// The `FoodModuleTag` model is untouched: the module still decides *what kind* of gathering a
    /// tile offers and its `seasonal_weight`. This table decides *how much* is there.
    pub capacity_by_biome: HashMap<TerrainType, f32>,
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
    /// The **river fishing bonus** added to a `NavigableRiver` hex's seeded forage capacity, on top of
    /// the biome it was cut through — a navigable river is always a fishery. See
    /// [`ForageLaborConfig::navigable_forage_capacity`] and
    /// [`DEFAULT_NAVIGABLE_RIVER_FORAGE_BONUS`].
    pub navigable_river_forage_bonus: f32,
}

impl ForageLaborConfig {
    /// Human-edible biomass a `terrain` tile carries at capacity. An **unknown** biome reads
    /// [`NO_FORAGE_CAPACITY`], but [`LaborConfig::validate`] guarantees the table is total over
    /// [`TerrainType::VALUES`], so on any loaded config this is a real lookup, never a silent
    /// default. Mirrors `GrazeConfig::capacity_for`.
    pub fn capacity_for(&self, terrain: TerrainType) -> f32 {
        self.capacity_by_biome
            .get(&terrain)
            .copied()
            .unwrap_or(NO_FORAGE_CAPACITY)
    }

    /// Forage capacity of a **navigable river** hex: the biome it was cut through
    /// (`capacity_for(underlying)`) **plus** the river fishing bonus. A navigable river is always a
    /// fishery, so this is always `>= navigable_river_forage_bonus > 0` — a navigable hex always
    /// seeds a patch, even over an otherwise-barren biome. THE single source of "navigable forage
    /// capacity", shared by the seeding path (`spawn_initial_forage`) and the wire path
    /// (`snapshot::tile_state`) so the two cannot drift.
    pub fn navigable_forage_capacity(&self, underlying: TerrainType) -> f32 {
        self.capacity_for(underlying) + self.navigable_river_forage_bonus
    }
}

impl Default for ForageLaborConfig {
    fn default() -> Self {
        Self {
            // Deliberately **empty**, mirroring `GrazeConfig::default`. The 37-row table is *data*,
            // and its single authoritative copy is `labor_config.json` — duplicating it here would
            // guarantee the two drift. A config whose `forage` block omits (or under-fills) the table
            // is *rejected* by [`LaborConfig::validate`] and the builtin — which has it — is used, so
            // an incomplete table can never quietly produce a map with no food on it.
            capacity_by_biome: HashMap::new(),
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
            navigable_river_forage_bonus: DEFAULT_NAVIGABLE_RIVER_FORAGE_BONUS,
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
            LaborConfig::from_json_str(BUILTIN_LABOR_CONFIG)
                .expect("builtin labor config should parse and validate"),
        )
    }

    /// Parse **and validate** (the `fauna_config.rs` convention, so *every* load path — builtin,
    /// default file, `LABOR_CONFIG_PATH` override — is covered and an invalid config can never be
    /// silently accepted).
    pub fn from_json_str(json: &str) -> Result<Self, LaborConfigError> {
        let config: LaborConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, LaborConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| LaborConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        LaborConfig::from_json_str(&contents)
    }

    /// Invariants a labor config must satisfy to be usable. Mirrors `FaunaConfig::validate` (which
    /// guards the *animal* food web's `graze.capacity_by_biome`) — the human food web's table gets
    /// the same discipline, because it fails the same way: silently, and invisibly.
    pub fn validate(&self) -> Result<(), LaborConfigError> {
        validate_forage_capacity_table(&self.forage)
    }

    /// Distance (inclusive) at which a Hunt assignment still yields before lapsing.
    pub fn hunt_reach(&self) -> u32 {
        self.band_work_range + self.hunt_leash_tiles
    }
}

/// The **human** food web's per-biome table must be *total* over every `TerrainType`
/// (`TerrainType::VALUES`), finite, non-negative, and not everywhere zero — the exact invariants
/// `validate_graze` enforces on the animal one:
/// - a **missing** biome silently reads `NO_FORAGE_CAPACITY` (`capacity_for`'s `unwrap_or`), i.e. an
///   invisible zero-forage dead zone nothing on the map would ever explain. **Zero must be stated.**
/// - an **all-zero** table parses perfectly and leaves the map with no gatherable food anywhere.
fn validate_forage_capacity_table(forage: &ForageLaborConfig) -> Result<(), LaborConfigError> {
    let mut positive_rows = 0usize;
    for terrain in TerrainType::VALUES {
        let Some(&capacity) = forage.capacity_by_biome.get(&terrain) else {
            return Err(LaborConfigError::Invalid {
                field: "forage.capacity_by_biome",
                constraint: format!(
                    "name every one of the {} biomes (missing {terrain:?}); an absent biome silently \
                     reads as zero forage",
                    TerrainType::VALUES.len()
                ),
                value: format!("{} rows", forage.capacity_by_biome.len()),
            });
        };
        if !capacity.is_finite() || capacity < NO_FORAGE_CAPACITY {
            return Err(LaborConfigError::Invalid {
                field: "forage.capacity_by_biome",
                constraint: format!("be finite and at least {NO_FORAGE_CAPACITY} for every biome"),
                value: format!("{terrain:?} = {capacity}"),
            });
        }
        if capacity > NO_FORAGE_CAPACITY {
            positive_rows += 1;
        }
    }
    if positive_rows == 0 {
        return Err(LaborConfigError::Invalid {
            field: "forage.capacity_by_biome",
            constraint:
                "give at least one biome a positive capacity, or there is nothing to gather \
                         anywhere on any map"
                    .to_string(),
            value: "every biome is 0".to_string(),
        });
    }
    Ok(())
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
    #[error("invalid labor config: {field} must {constraint} (was {value})")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
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
            // A *broken invariant* is louder than a missing file: the config parsed, so it looks
            // fine, and silently falling back to the builtin would hide a table the operator
            // believes is live (the `fauna_config.invalid_rejected` convention).
            Err(err @ LaborConfigError::Invalid { .. }) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "labor_config.invalid_rejected"
                );
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
        // Cultivation (Phase 1a): progress outruns decay so a patch under Cultivate nets forward,
        // the preparing yield is a strict *dip* (a positive fraction of MSY, but less than it), and
        // the steady tended-yield is positive.
        assert!(
            config.forage.cultivation.progress_per_turn > config.forage.cultivation.decay_per_turn
        );
        assert!(config.forage.cultivation.cultivating_yield_fraction > 0.0);
        assert!(config.forage.cultivation.cultivating_yield_fraction < 1.0);
        assert!(config.forage.cultivation.tended_provisions_per_biomass > 0.0);
        // Rung 1b (earned knowledge): positive accrual, completion threshold in (0, 1].
        assert!(config.forage.cultivation.knowledge_progress_per_turn > 0.0);
        assert!(config.forage.cultivation.knowledge_completion_threshold > 0.0);
        assert!(config.forage.cultivation.knowledge_completion_threshold <= 1.0);
        // A tended patch (harvested on its full standing biomass, ~cap) out-yields the same patch's
        // wild MSY skim (regrowth at K/2 = regrowth_rate × K/4, × the gather provisions rate) — the
        // intensification incentive. Compare per-biomass factors: tended pays on ~K, wild MSY on
        // regrowth_rate·K/4, so tended wins iff tended_rate > regrowth_rate/4 × gather_rate.
        let forage = &config.forage;
        let wild_msy_rate = forage.ecology.regrowth_rate / 4.0 * forage.provisions_per_biomass;
        assert!(
            forage.cultivation.tended_provisions_per_biomass > wild_msy_rate,
            "tended patch must out-yield its wild MSY: {} vs {}",
            forage.cultivation.tended_provisions_per_biomass,
            wild_msy_rate
        );
        assert!(config.hunt.per_worker_biomass_capacity > 0.0);
        assert!(config.scout.vantage_distance_base >= 1);
        assert!(config.scout.vantage_distance_max >= config.scout.vantage_distance_base);
        assert!(config.scout.vantage_range >= 1);
        assert_eq!(
            config.hunt_reach(),
            config.band_work_range + config.hunt_leash_tiles
        );
    }

    /// Parse the builtin with `mutate` applied to its JSON, expecting a **rejection** — the
    /// `fauna_config::tests::reject` idiom.
    fn reject(mutate: impl FnOnce(&mut serde_json::Value)) -> LaborConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_LABOR_CONFIG).expect("builtin parses");
        mutate(&mut json);
        LaborConfig::from_json_str(&json.to_string()).expect_err("config should be rejected")
    }

    fn assert_rejects_field(err: LaborConfigError, expected: &str) {
        match err {
            LaborConfigError::Invalid { field, .. } => assert_eq!(field, expected),
            other => panic!("expected an Invalid rejection on {expected}, got {other:?}"),
        }
    }

    /// The forage table must be **total** over the 37 biomes. A missing row would silently read as
    /// zero forage — an invisible dead zone in the human food web that nothing would ever explain.
    /// The exact discipline `FaunaConfig::validate` applies to the graze (animal) table.
    #[test]
    fn validate_rejects_a_partial_forage_biome_table() {
        let err = reject(|json| {
            json["forage"]["capacity_by_biome"]
                .as_object_mut()
                .expect("table")
                .remove("AlluvialPlain");
        });
        assert_rejects_field(err, "forage.capacity_by_biome");
    }

    /// An all-zero table parses perfectly and leaves every map with nothing to gather anywhere.
    #[test]
    fn validate_rejects_an_all_zero_forage_table() {
        let err = reject(|json| {
            let table = json["forage"]["capacity_by_biome"]
                .as_object_mut()
                .expect("table");
            for value in table.values_mut() {
                *value = (0.0).into();
            }
        });
        assert_rejects_field(err, "forage.capacity_by_biome");
    }

    #[test]
    fn validate_rejects_a_negative_forage_capacity() {
        let err =
            reject(|json| json["forage"]["capacity_by_biome"]["AlluvialPlain"] = (-1.0).into());
        assert_rejects_field(err, "forage.capacity_by_biome");
    }

    /// **The two food webs must actually disagree.** This is the model claim the whole two-table
    /// split exists to make (`docs/plan_grazing_foundation.md` §1) — if it ever inverts, "your best
    /// farm is not your best pasture" has quietly become false and the agropastoral decision has
    /// evaporated. Asserted per-tile against the *graze* table, the only place the two can be
    /// compared.
    #[test]
    fn the_two_food_webs_disagree_farm_is_not_pasture() {
        let forage = &LaborConfig::builtin().forage;
        let graze = &crate::fauna_config::FaunaConfig::builtin().graze;

        // Total table (the validator's job, restated as a model claim).
        assert_eq!(forage.capacity_by_biome.len(), TerrainType::VALUES.len());

        // The flagship inversion: a closed-canopy woodland is the best human ground and among the
        // worst pasture; a prairie steppe is exactly the reverse.
        let woodland = TerrainType::MixedWoodland;
        let prairie = TerrainType::PrairieSteppe;
        assert!(forage.capacity_for(woodland) > forage.capacity_for(prairie));
        assert!(graze.capacity_for(woodland) < graze.capacity_for(prairie));

        // The silt lowlands are THE FARM, not the pasture: they beat prairie for humans and lose to
        // it for animals.
        for farm in [
            TerrainType::AlluvialPlain,
            TerrainType::Floodplain,
            TerrainType::RiverDelta,
        ] {
            assert!(
                forage.capacity_for(farm) > forage.capacity_for(prairie),
                "{farm:?} must out-farm prairie"
            );
            assert!(
                graze.capacity_for(farm) < graze.capacity_for(prairie),
                "{farm:?} must not out-pasture prairie"
            );
        }

        // Nothing human-edible grows on ice or a salt pan — a *stated* zero, not a defaulted one.
        for barren in [
            TerrainType::Glacier,
            TerrainType::SaltFlat,
            TerrainType::BasalticLavaField,
            TerrainType::DeepOcean,
        ] {
            assert_eq!(
                forage.capacity_for(barren),
                NO_FORAGE_CAPACITY,
                "{barren:?}"
            );
        }

        // The shelf is the coastal larder — rich in human food and (being water) zero pasture. The
        // sharpest divergence on the map, and the reason `water = 0 forage` would have been wrong:
        // shelf / inland-sea / coral tiles carry real `FoodModuleTag` fisheries.
        for marine in [
            TerrainType::ContinentalShelf,
            TerrainType::InlandSea,
            TerrainType::CoralShelf,
        ] {
            assert!(forage.capacity_for(marine) > 0.0, "{marine:?} is a fishery");
            assert_eq!(graze.capacity_for(marine), 0.0, "{marine:?} is not pasture");
        }
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
