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
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use sim_runtime::TerrainType;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::fauna_config::EcologyConfig;

pub const BUILTIN_LABOR_CONFIG: &str = include_str!("data/labor_config.json");

/// Named-const defaults for the depletable-forage ecology (Intensification §0-ii). All are
/// **tuning dials** (settle live): the gather throughput, the biomass→provisions conversion, and
/// the ecology dynamics. `regrowth_rate` is tuned **higher than fauna's 0.05** — patches regrow
/// faster than game. `extinction_floor` is `0.0` because forage patches never despawn (a crashed
/// patch sits at low biomass and recovers via `logistic_regrowth`). The per-patch *capacity* is no
/// longer a scalar default: it is [`ForageLaborConfig::capacity_by_biome`], a per-biome table (the
/// human-edible twin of `fauna_config`'s `graze.capacity_by_biome`).
const DEFAULT_FORAGE_PER_WORKER_BIOMASS_CAPACITY: f32 = 1.6;
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

/// **The tended rung's growth multiplier** — folded into a committed patch's `r` by
/// [`crate::forage::patch_ecology`], the plant mirror of `fauna::herd_ecology` and the one seam any
/// consumer resolves a patch's ecology through.
///
/// **Neutral at `1.0` since Flora Roster S2** (`docs/plan_flora_roster.md` §4.3). It began as the plant
/// twin of `fauna_config`'s `pastoral_gain` — a tended stand "grows faster toward its own ceiling,
/// freed from competitors". But S1 made that competitor-removal **explicit** as a composition term, so
/// a growth-rate boost **double-counts** it. S2 retired the boost, and #433 paid the debt it left:
/// tending now pays through **weeding** (the favored crop's share rises within the tile's basket) plus
/// **conversion** ([`DEFAULT_CULTIVATION_TENDED_CONVERSION_GAIN`] on that crop's own yield vector), and
/// the rung-2 "wild < tended" guarantee lives in the roster's own bar
/// (`core_sim/tests/flora_roster.rs`). At `1.0` a tended stand regrows exactly as fast as wild.
///
/// **The lever stays** (it is not deleted): `1.0` is neutral, and a small boost can be dialed back in
/// for playtest if the roster ever wants tending to also quicken regrowth. `validate()` forbids only a
/// gain **below** `1.0` — tending making a stand grow *slower* than wild is incoherent whatever the
/// crop. A **playtest dial**.
const DEFAULT_CULTIVATION_TENDED_REGROWTH_GAIN: f32 = 1.0;

/// The **Field**-harvest rate (the plant ladder's rung 3, slice 5): a sown Field pays its workers
/// `biomass × this` provisions/turn on its full standing crop, without being drawn down — **the one
/// rung on the plant web that is a managed rate rather than a curve**, because at rung 3 the source
/// is *yours*: you control its reproduction, so there is no wild stock left to over-skim and the
/// policy axis honestly collapses (the animal mirror is the pen's `managed_yield_biomass`).
///
/// **It must exceed what the same patch pays as a *tended* patch, or rung 3 is pointless.** A Field is
/// never drawn down, so its biomass settles at `K` and it pays `K × this`; a tended patch pays its
/// boosted MSY, `tended_regrowth_gain × regrowth_rate × K/4 × provisions_per_biomass`. Both are linear
/// in `K`, so the comparison is **scale-free** across every biome — `validate()` states it once as a
/// per-biomass inequality. Shipped at **0.02**: on an `AlluvialPlain` (`K` = 195) a Field *produces*
/// `195 × 0.02` = **3.9 prov/turn** against a tended patch's 0.91 and a wild Sustain skim's 0.61.
///
/// **Production, not take** (slice 7): the crew still has to carry it home, so the Field's *actual*
/// yield is `min(production, workers × per-worker throughput)` — a rich Field genuinely needs many
/// hands, and understaffing it wastes the difference. A **playtest dial**.
const DEFAULT_CULTIVATION_FIELD_PROVISIONS_PER_BIOMASS: f32 = 0.02;

/// **How hard the TENDED rung WEEDS a committed species up through the tile's basket** (#433,
/// `docs/plan_flora_roster.md` §4.3). Tending does not change how much the tile produces — **the land
/// owns `K`, and no rung below 4 raises it or lowers it** — it changes what that production is *made
/// of*: the favored crop's share rises to `min(1.0, share × gain)` and the increase is taken from the
/// least abundant remaining species first.
///
/// **The cap at 1.0 is the model, not a safety rail.** A basket is a whole; weeding can only move
/// share *within* it, which is why the rung's payoff has to come from **conversion**
/// ([`DEFAULT_CULTIVATION_TENDED_CONVERSION_GAIN`]) compounding with a bigger share, never from the
/// share alone.
///
/// Shipped at **1.5** (the value the retired `tended_concentration_gain` carried) — a plant that is
/// already about two-thirds of its tile's basket fills it once tended; a marginal one does not,
/// without the inputs that are rung 4. A **playtest dial**.
const DEFAULT_CULTIVATION_TENDED_WEEDING_GAIN: f32 = 1.5;

/// **The TENDED rung's CONVERSION gain** (#433) — the multiplier on the **favored species' whole
/// yield vector** (food, fodder and trade alike, no `role` branch) once the patch is a completed
/// Tended Patch.
///
/// **It applies to the favored term only, and that is the whole point.** Tending is knowing *your*
/// crop; the volunteers still standing beside it are still wild. A blanket multiplier on the basket
/// would make every commitment pay ~`gain` regardless of what you favored, erasing the crop choice.
/// On the favored term it *compounds* with weeding, so favoring a dominant plant pays and favoring a
/// marginal one barely moves.
///
/// Shipped at **2.0** — it is what makes a 25-turn Cultivate pay back in the teens of turns rather
/// than the eighties, and it is the payoff Flora Roster S2 left owing when it retired
/// `tended_regrowth_gain` to a neutral 1.0 with nothing in its place. A **playtest dial**.
const DEFAULT_CULTIVATION_TENDED_CONVERSION_GAIN: f32 = 2.0;

/// Cultivation tuning (Intensification Phase 1a) — **the levers that are NOT the build meter's**.
/// The plant rung-2 build dials (how fast a patch is prepared, how fast it goes feral, and the
/// investment dip it pays while preparing) moved to the shared ladder,
/// `data/intensification_ladder.json` → the `plant:tended` rung's `build` block
/// (`crate::intensification`), because plants and animals must climb on the *same* numbers — and, as
/// of slice 4, so did the **earned-knowledge levers** (`knowledge_progress_per_turn`, since split
/// into the ladder's `learn_rate` + per-knowledge `lesson_costs`, /
/// `knowledge_completion_threshold` → the ladder's `knowledge` block): once the earn path became one
/// rung-driven seam, a per-web copy of "20 turns to learn a rung" was pure duplication. What stays
/// here is the plant web's own economy: **the two rungs' payoffs** — rung 2's growth gain and rung 3's
/// managed rate. They stay here for the same reason `pastoral_gain`/`pen_gain` stay in `fauna_config`:
/// a rung's *payoff* is its web's economy, where its *build* is the ladder's grammar.
///
/// A patch worked with the **Cultivate** improvement in flight ([`crate::components::Improvement::Cultivate`])
/// — faction knows Cultivation, patch is **Thriving** — accrues the `plant:tended` rung's
/// work units toward the `plant:tended` rung's `work_cost` while yielding only that rung's
/// `yield_fraction_while_building ×` the assignment's own stance ceiling (the investment cost). A cultivated patch that isn't tended any given
/// turn goes **feral**, its progress decaying by the rung's `decay_per_turn` back below `1.0`
/// (reverting to a wild gather patch). A tended patch is **still a wild stand** — the tending buys it
/// a faster curve (`tended_regrowth_gain`), and the band gathers it under the full policy axis,
/// drawing it down, exactly as a *pastoral* herd is hunted on its boosted `r`. The plant mirror of
/// fauna's `HusbandryConfig`.
///
/// There is **no early claim**: a `claim_threshold` that snapped progress to `1.0` would let the
/// player skip the investment, which is the whole decision. The `cultivate` command now *sets the
/// policy* instead.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CultivationConfig {
    /// **The tended rung's growth multiplier** — a tended patch's stock regrows `this ×` as fast as
    /// the same patch would wild; folded in by [`crate::forage::patch_ecology`]. **Neutral at `1.0`
    /// since Flora Roster S2**: tending pays through weeding + conversion, not this boost, so a
    /// gain of `1.0` (regrows as fast as wild) is valid — only a gain *below* `1.0` is rejected (it
    /// would make tending grow a stand *slower* than wild). See
    /// [`DEFAULT_CULTIVATION_TENDED_REGROWTH_GAIN`].
    pub tended_regrowth_gain: f32,
    /// **Field-harvest** rate (rung 3): a sown Field *produces* `biomass × this` provisions/turn on
    /// its full standing crop, without depleting biomass — the one managed rate on the plant web,
    /// because at rung 3 the source is yours. Must out-produce the tended rung's saturated MSY (see
    /// [`DEFAULT_CULTIVATION_FIELD_PROVISIONS_PER_BIOMASS`]), or climbing to rung 3 would buy nothing.
    pub field_provisions_per_biomass: f32,
    /// **The tended rung's WEEDING gain** — how far tending pushes the favored species' share of the
    /// tile's basket, `min(1.0, share × this)`. See [`DEFAULT_CULTIVATION_TENDED_WEEDING_GAIN`]: it
    /// moves share *within* the basket and never touches `K`, because **the land owns `K`**.
    pub tended_weeding_gain: f32,
    /// **The tended rung's CONVERSION gain** — the multiplier on the favored crop's whole yield
    /// vector once the patch is tended. See [`DEFAULT_CULTIVATION_TENDED_CONVERSION_GAIN`].
    ///
    /// **There is no `field_conversion_gain` twin**, and no surviving `field_*_gain` at all: a Field
    /// forces the favored share to `1.0` (nothing left to weed) and converts at its own dial,
    /// `field_provisions_per_biomass`.
    pub tended_conversion_gain: f32,
}

impl Default for CultivationConfig {
    fn default() -> Self {
        Self {
            tended_regrowth_gain: DEFAULT_CULTIVATION_TENDED_REGROWTH_GAIN,
            field_provisions_per_biomass: DEFAULT_CULTIVATION_FIELD_PROVISIONS_PER_BIOMASS,
            tended_weeding_gain: DEFAULT_CULTIVATION_TENDED_WEEDING_GAIN,
            tended_conversion_gain: DEFAULT_CULTIVATION_TENDED_CONVERSION_GAIN,
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
    /// **The NO-EQUIPMENT baseline** a *bare-handed* forager gathers per turn (× `seasonal_weight`),
    /// capped by the policy ceiling and the patch's remaining biomass — the forage counterpart of
    /// `hunt.per_worker_biomass_capacity`.
    ///
    /// > **THE NUMBER MOVED; THE KEY DID NOT.** This was the *basketed* `8.0` for as long as the
    /// > basket's own payload had nowhere else to live. Quality tiers gave it one: the baskets'
    /// > `flint` tier declares `forage_carry equipped 8.0`, and what stands here is the `1.6` the
    /// > item used to declare as its `unequipped` side. The key's **role** changed and its name did
    /// > not, because every caller hands it to
    /// > [`crate::equipment_config::EquipmentConfig::forage_per_worker_biomass_capacity`], whose
    /// > argument is the fallback either way.
    /// >
    /// > **A readout with no band to resolve a tier against must NOT quote this as "what a gatherer
    /// > collects"** — that is
    /// > [`crate::equipment_config::EquipmentConfig::equipped_reference`], which answers `8.0` off
    /// > the item table.
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
            cultivation: CultivationConfig::default(),
            navigable_river_forage_bonus: DEFAULT_NAVIGABLE_RIVER_FORAGE_BONUS,
        }
    }
}

/// Flat per-worker hunt throughput tier.
#[derive(Debug, Clone, Deserialize)]
pub struct HuntLaborConfig {
    /// **The NO-EQUIPMENT baseline** a *sledless* hunter hauls per turn, capped by the policy
    /// ceiling (Sustain = net regrowth, etc.). The biomass→provisions/trade conversion reuses
    /// `fauna_config`'s `hunt.*_per_biomass` so the ecology stays consistent.
    ///
    /// > **THE NUMBER MOVED; THE KEY DID NOT.** This was the *sledded* `40.0` for as long as the
    /// > sled's own payload had nowhere else to live. Quality tiers gave it one: the sled's `flint`
    /// > tier declares `hunt_carry equipped 40.0`, and what stands here is the `12.0` the item used
    /// > to declare as its `unequipped` side. The key's **role** changed and its name did not,
    /// > because every caller hands it to
    /// > [`crate::equipment_config::EquipmentConfig::hunt_per_worker_biomass_capacity`] (or
    /// > `::pen_per_worker_biomass_capacity`), whose argument is the fallback either way.
    /// >
    /// > **A herd row, a patch row or a Field's managed collection cap must NOT quote this as "what
    /// > a hunter hauls"** — none of them has a band to resolve a tier against, and the answer they
    /// > want is [`crate::equipment_config::EquipmentConfig::equipped_reference`], which reads
    /// > `40.0` off the item table. **A pen still shares this rate**, resolved off the sled's tier
    /// > through [`crate::equipment_config::EquipmentStat::shares_equipped_rate_with`], so it keeps
    /// > exactly one home.
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
    /// **The forward-projection horizon for a source's steady `realized` yield**, in turns. Each
    /// source's `SourceYield::realized` is the *average food/turn it will deliver over the next N
    /// turns*, computed by simulating the herd/patch forward N turns from its CURRENT state under the
    /// assignment's policy + worker count (the smooth policy RATE, not the lumpy kill-credit bank).
    /// It is a **pure function of state** — no history, no cold-start — so the assign-time seed and
    /// the resolved row compute the identical number (exact forecast == actual). A larger horizon
    /// smooths a settled Sustain herd to flat ≈ MSY and lets a Surplus/Deplete projection see the
    /// herd's decline within the window. Its own lever, distinct from the expedition
    /// `forecast_horizon_turns` (a raid-length horizon, a different question). Validated `> 0`.
    pub yield_average_horizon_turns: u32,
    /// **The forward-projection horizon for a source's ARRIVAL SCHEDULE**, in turns. Each source's
    /// `SourceYield::arrivals` is *what lands on each of the next N turns* — the same forward
    /// simulation `yield_average_horizon_turns` drives, but run **WITH** the kill-credit bank, so it
    /// answers the opposite question: not *how much per turn on average* but *on which turns does the
    /// food actually arrive*. That is why it is its **own** lever and deliberately shorter: a schedule
    /// is read turn-by-turn on a chart, so the horizon is a display span (how far ahead the player can
    /// plan their larder), where the average's horizon is a smoothing window. Validated `> 0`.
    pub arrivals_horizon_turns: u32,
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
        validate_forage_capacity_table(&self.forage)?;
        // The realized-yield forward projection needs a positive horizon: it averages provisions over
        // `1..=horizon` simulated turns, and `horizon == 0` would divide by zero (an empty projection
        // has no average to report).
        if self.yield_average_horizon_turns == 0 {
            return Err(LaborConfigError::Invalid {
                field: "yield_average_horizon_turns",
                constraint:
                    "be at least 1 (the realized-yield forward-projection horizon in turns)"
                        .to_string(),
                value: self.yield_average_horizon_turns.to_string(),
            });
        }
        // The arrival schedule is a `Vec` of exactly this length — at `0` the sim would publish an
        // empty schedule for every source and the client's chart would silently render nothing.
        if self.arrivals_horizon_turns == 0 {
            return Err(LaborConfigError::Invalid {
                field: "arrivals_horizon_turns",
                constraint:
                    "be at least 1 (the arrival-schedule forward-projection horizon in turns)"
                        .to_string(),
                value: self.arrivals_horizon_turns.to_string(),
            });
        }
        validate_plant_ladder_payoffs(&self.forage)
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

/// **The wild rung's growth multiplier** — a wild patch grows at exactly its ecology's `regrowth_rate`,
/// so it is the identity, and it is the floor `cultivation.tended_regrowth_gain` may not fall below.
/// Named rather than a bare `1.0` because it states *which* rung the comparison is against.
const WILD_REGROWTH_GAIN: f32 = 1.0;

/// **The plant ladder must be monotone, or climbing it buys nothing** — the payoff twin of
/// `FaunaConfig::validate`'s `pen_gain > pastoral_gain > 1` check, and enforced on **every** load path
/// (builtin, default file, `LABOR_CONFIG_PATH` override) for the reason that check is: a rung whose
/// payoff sits at or below the rung beneath it is not a design choice, it is a config that has
/// silently deleted a rung.
///
/// Two checks:
/// - **`tended_regrowth_gain >= 1.0`** — this is a *coherence* floor, **not** the "wild < tended"
///   guarantee it used to be (Flora Roster S2, `docs/plan_flora_roster.md` §4.3). Since S1 made
///   competitor-removal explicit as a *composition* term, a tended patch's payoff is
///   `weeding × conversion`, not this regrowth boost — so a neutral gain of `1.0` still pays. The
///   scale-free "wild < tended" invariant is retired by design; whether a *committed* crop is worth
///   tending is now guaranteed by the roster's own bar (`core_sim/tests/flora_roster.rs`, "pays on
///   its best country, less on its worst"), which sees the basket and the conversion gain where this
///   config check — blind to both — cannot. All this check forbids is the **incoherent** case: a gain
///   *below* `1.0` would make tending a stand grow **slower** than wild, which is nonsense whatever
///   the crop.
/// - **tended < field** — a Field is never drawn down, so it settles at `K` and produces
///   `K × field_provisions_per_biomass`; a tended patch pays its MSY,
///   `regrowth_gain × (r·K/4) × basket_rate × conversion_gain`. Divide both by `K`. This is
///   **scale-free** (linear in the tile's `K`, so it holds on every biome in `capacity_by_biome` at
///   once). The `r·K/4` factor comes from the **shared** [`peak_regrowth`] curve at unit capacity —
///   never a second copy of the model.
///
///   **Evaluated at tending's SATURATED best case** (#433): with weeding pushed all the way, the
///   tended basket is the favored crop alone, so its rate is that crop's own — which then cancels
///   against the same crop's rate inside the Field's `patch_species_quality`. What is left on the
///   tended side is `provisions_per_biomass × tended_conversion_gain`, and the check stays both
///   scale-free in `K` *and* independent of which species it is asked about. Anything less than
///   saturated only widens the margin.
fn validate_plant_ladder_payoffs(forage: &ForageLaborConfig) -> Result<(), LaborConfigError> {
    let cultivation = &forage.cultivation;
    if !cultivation.tended_regrowth_gain.is_finite()
        || cultivation.tended_regrowth_gain < WILD_REGROWTH_GAIN
    {
        return Err(LaborConfigError::Invalid {
            field: "forage.cultivation.tended_regrowth_gain",
            constraint: format!(
                "be finite and at least {WILD_REGROWTH_GAIN} (the wild curve) — a gain BELOW it \
                 would make tending a stand grow SLOWER than leaving it wild, which is incoherent. \
                 A neutral gain of exactly {WILD_REGROWTH_GAIN} is valid: since Flora Roster S1 \
                 (docs/plan_flora_roster.md §4.3) tending pays through weeding + conversion, \
                 not this boost, and whether a committed crop is worth tending is guaranteed by the \
                 roster's own bar (core_sim/tests/flora_roster.rs), not by this scale-free check"
            ),
            value: cultivation.tended_regrowth_gain.to_string(),
        });
    }
    if !cultivation.field_provisions_per_biomass.is_finite()
        || cultivation.field_provisions_per_biomass <= 0.0
    {
        return Err(LaborConfigError::Invalid {
            field: "forage.cultivation.field_provisions_per_biomass",
            constraint: "be finite and positive — a Field that pays nothing is not a rung"
                .to_string(),
            value: cultivation.field_provisions_per_biomass.to_string(),
        });
    }
    // The tended rung's MSY per unit of the tile's `K`, in provisions, at its SATURATED best case:
    // the shared peak-regrowth curve at unit capacity (`r/4`), on the tended rung's `r`
    // (`× tended_regrowth_gain`), through the gather conversion, times the rung's conversion gain on
    // the favored crop (whose own rate cancels against the Field's — see the doc comment).
    let tended_rate = cultivation.tended_regrowth_gain
        * peak_regrowth_per_capacity(&forage.ecology)
        * forage.provisions_per_biomass
        * cultivation.tended_conversion_gain;
    if cultivation.field_provisions_per_biomass <= tended_rate {
        return Err(LaborConfigError::Invalid {
            field: "forage.cultivation.field_provisions_per_biomass",
            constraint: format!(
                "exceed what the same patch pays one rung down — the tended rung's MSY of \
                 {tended_rate} per unit of the tile's carrying capacity — or sowing a Field buys \
                 nothing"
            ),
            value: cultivation.field_provisions_per_biomass.to_string(),
        });
    }
    // **The tended rung's two gains** (#433). Below `1.0` each is incoherent rather than merely
    // unbalanced: a weeding gain under 1 would weed *against* the crop it just committed to, and a
    // conversion gain under 1 would make knowing your crop convert it worse than not knowing it.
    // Neither needs an upper bound — weeding saturates at the whole basket, and a runaway conversion
    // gain is caught by the monotonicity check above, which reads it.
    for (field, gain) in [
        (
            "forage.cultivation.tended_weeding_gain",
            cultivation.tended_weeding_gain,
        ),
        (
            "forage.cultivation.tended_conversion_gain",
            cultivation.tended_conversion_gain,
        ),
    ] {
        if !gain.is_finite() || gain < NO_TENDED_GAIN {
            return Err(LaborConfigError::Invalid {
                field,
                constraint: format!(
                    "be finite and at least {NO_TENDED_GAIN} — below it the tended rung would make \
                     the crop it just committed to *worse off* than leaving the stand alone, which \
                     is incoherent"
                ),
                value: gain.to_string(),
            });
        }
    }
    Ok(())
}

/// **The tended gain that changes nothing** — a tended patch would hold exactly the crop's own share
/// of the basket and convert it at exactly the basket's own rate. The floor both tended gains must
/// clear, named rather than a bare `1.0` because it states *what* the number means.
const NO_TENDED_GAIN: f32 = 1.0;

/// One turn's **peak (MSY) regrowth per unit of carrying capacity** — `r/4` — read off the *shared*
/// logistic curve at unit capacity rather than re-spelled as a formula, so the plant ladder's tuning
/// bounds and the yields they bound can never disagree about what MSY is.
fn peak_regrowth_per_capacity(ecology: &EcologyConfig) -> f32 {
    const UNIT_CAPACITY: f32 = 1.0;
    crate::fauna::peak_regrowth(UNIT_CAPACITY, ecology)
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

impl ConfigLoadError for LaborConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
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

/// Load labor configuration from environment (`LABOR_CONFIG_PATH`) or the default data path. The
/// config is **validated** on load, and a broken invariant is as fatal as a parse error.
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `LABOR_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_labor_config_from_env() -> (Arc<LaborConfig>, LaborConfigMetadata) {
    let (config, source) = load_config_from_env(
        "LABOR_CONFIG_PATH",
        "labor_config",
        "src/data/labor_config.json",
        LaborConfig::builtin,
        LaborConfig::from_file,
    );
    (config, LaborConfigMetadata::new(source))
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
        // Forage policy axis (§0-iii): Surplus overdraws the Sustain skim, Deplete/Eradicate take a
        // fractional commercial/strip share, and Deplete marks the basket's own trade rate up (the
        // species-blind flat `trade_goods_per_biomass` is retired — #433).
        // Cultivation (Phase 1a): the plant ladder's two payoffs are sane and monotone. (The plant
        // rungs' *build* dials — progress vs decay, and the preparing dip — moved to the ladder,
        // where `LadderConfig::validate` bounds them on every load path; the payoffs' own
        // monotonicity now rides `LaborConfig::validate`, asserted directly below so the *builtin*
        // is pinned to the shipped shape rather than merely to the bound.)
        // S2: the tended regrowth boost is retired to a NEUTRAL 1.0 (tending pays through
        // weeding + conversion, not this gain); `>= 1.0` is the coherence floor, not `> 1.0`.
        assert!(config.forage.cultivation.tended_regrowth_gain >= 1.0);
        assert!(config.forage.cultivation.field_provisions_per_biomass > 0.0);
        assert!(config.validate().is_ok());
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

    /// **A tended patch that grows SLOWER than the wild stand is incoherent** (Flora Roster S2). The
    /// gain is neutral at `1.0` — since S1 made competitor-removal a composition term, tending pays
    /// through weeding + conversion, not this boost — so `1.0` is valid (see
    /// [`validate_accepts_a_neutral_tended_regrowth_gain`]) and only a gain *below* `1.0` is rejected:
    /// tending making a stand grow slower than leaving it wild is nonsense whatever the crop.
    /// (Non-finite gains are guarded in code but not exercised here — JSON cannot express NaN or
    /// infinity, so a config file can never carry one; `serde` rejects those spellings first.)
    #[test]
    fn validate_rejects_a_tended_gain_below_the_wild_curve() {
        for gain in [0.9, 0.5, -1.0] {
            let err =
                reject(|json| json["forage"]["cultivation"]["tended_regrowth_gain"] = gain.into());
            assert_rejects_field(err, "forage.cultivation.tended_regrowth_gain");
        }
    }

    /// **A tended rung that makes its own crop worse off is incoherent** (#433). Below `1.0` the
    /// weeding gain would weed *against* the crop the commitment just chose, and the conversion gain
    /// would make knowing your crop convert it worse than not knowing it. Neither is a balance
    /// question, which is why both are refused at load rather than left to the roster's bar.
    #[test]
    fn validate_rejects_a_tended_gain_that_would_hurt_the_committed_crop() {
        for key in ["tended_weeding_gain", "tended_conversion_gain"] {
            for gain in [0.9, 0.5, -1.0] {
                let err = reject(|json| json["forage"]["cultivation"][key] = gain.into());
                assert_rejects_field(err, &format!("forage.cultivation.{key}"));
            }
        }
    }

    /// **A neutral tended regrowth gain of exactly `1.0` is valid** (Flora Roster S2). The old check
    /// rejected `<= 1.0` on the now-retired "wild < tended" invariant; S2 moved that guarantee to the
    /// roster's own bar (`core_sim/tests/flora_roster.rs`), which sees the basket and the gains.
    /// A tended stand that regrows exactly as fast as wild still pays — through the crop it commits to.
    #[test]
    fn validate_accepts_a_neutral_tended_regrowth_gain() {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_LABOR_CONFIG).expect("builtin parses");
        json["forage"]["cultivation"]["tended_regrowth_gain"] = (1.0).into();
        assert!(
            LaborConfig::from_json_str(&json.to_string()).is_ok(),
            "a neutral tended_regrowth_gain of 1.0 must be accepted"
        );
    }

    /// **The plant ladder must be monotone.** A Field that out-produces nothing is a rung the player
    /// pays Seed Selection + 25 turns to reach and is *worse off* for — the failure the tended-gain
    /// check above guards one rung down.
    #[test]
    fn validate_rejects_a_field_that_does_not_beat_the_tended_patch_below_it() {
        // **Derived from the loaded config, never a baked literal** — the tended rung's SATURATED MSY
        // per unit K (`regrowth_gain × r/4 × provisions_per_biomass × conversion_gain`) is exactly the
        // bar the Field must clear, and the validator computes it the same way, so the boundary can
        // never silently drift from the check.
        let forage = &LaborConfig::builtin().forage;
        let tended_rate = f64::from(
            forage.cultivation.tended_regrowth_gain
                * peak_regrowth_per_capacity(&forage.ecology)
                * forage.provisions_per_biomass
                * forage.cultivation.tended_conversion_gain,
        );
        // `== tended_rate` (Field ties tended), a lower rate, and a negative rate are all rejected.
        for rate in [tended_rate, tended_rate * 0.5, -0.02] {
            let err = reject(|json| {
                json["forage"]["cultivation"]["field_provisions_per_biomass"] = rate.into()
            });
            assert_rejects_field(err, "forage.cultivation.field_provisions_per_biomass");
        }
    }

    /// **The plant ladder is scale-free — it reads the same on a delta and on a steppe.** Every rung's
    /// payoff is linear in the tile's `K`, so the monotonicity `validate` enforces per-biomass must
    /// hold at *every* capacity in the shipped table at once. That is what lets the per-biome table be
    /// retuned without re-deriving the ladder.
    ///
    /// **The tended term is its SATURATED best case** (#433): weeding pushed all the way, so the
    /// basket is the favored crop alone and that crop's own rate cancels against the same crop's rate
    /// inside the Field's quality factor. What is left on the tended side is the wild rate times the
    /// rung's conversion gain, which keeps the check both scale-free in `K` and independent of which
    /// species it is asked about — and makes the `tended < field` step the *tightest* form of itself.
    #[test]
    fn the_plant_ladder_is_monotone_on_every_biome() {
        let forage = &LaborConfig::builtin().forage;
        let cultivation = &forage.cultivation;
        for terrain in TerrainType::VALUES {
            let capacity = forage.capacity_for(terrain);
            if capacity <= NO_FORAGE_CAPACITY {
                continue;
            }
            // Wild and tended are both gathered off an MSY curve; the Field is a managed rate on the
            // standing crop it settles at (`K`).
            let wild_msy = peak_regrowth_per_capacity(&forage.ecology)
                * capacity
                * forage.provisions_per_biomass;
            let tended_msy =
                cultivation.tended_regrowth_gain * cultivation.tended_conversion_gain * wild_msy;
            let field = capacity * cultivation.field_provisions_per_biomass;
            assert!(
                wild_msy < tended_msy && tended_msy < field,
                "the ladder must climb on {terrain:?} (K = {capacity}): wild {wild_msy} → tended \
                 {tended_msy} → field {field}"
            );
        }
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
