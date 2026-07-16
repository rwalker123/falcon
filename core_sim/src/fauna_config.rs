//! Data-driven fauna species table + spawn abundance.
//!
//! Loaded from `data/fauna_config.json`. Turns the former hard-coded `HerdSpecies`
//! enum into a table: each species carries a display name, size class, migratory
//! flag, roaming range (route length), group biomass, and the food-module "biomes"
//! it hosts in. `abundance` drives how densely short-range game spawns per biome.
//! Mirrors the `visibility_config.rs` loader pattern (baked-in builtin + optional
//! file/env override).

use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use rand::{rngs::SmallRng, Rng};
use serde::Deserialize;
use sim_runtime::TerrainType;
use thiserror::Error;

pub const BUILTIN_FAUNA_CONFIG: &str = include_str!("data/fauna_config.json");

/// Coarse size band. Drives roaming range + group size; also lets Phase B/C offer
/// the right verbs (big/small game are huntable one-shot; migratory herds follow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    #[default]
    Small,
    Big,
    Migratory,
}

impl SizeClass {
    /// Stable string key (also the snapshot `size_class` field).
    pub fn as_str(&self) -> &'static str {
        match self {
            SizeClass::Small => "small",
            SizeClass::Big => "big",
            SizeClass::Migratory => "migratory",
        }
    }

    /// Parse the stable string key back into a size class (inverse of `as_str`; the rollback
    /// restore path). Unknown/empty strings resolve to the `Default` (`Small`).
    pub fn from_key(key: &str) -> Self {
        match key {
            "big" => SizeClass::Big,
            "migratory" => SizeClass::Migratory,
            _ => SizeClass::Small,
        }
    }
}

/// **How far up the husbandry ladder a species can climb** (Grazing 2d-δ, `docs/plan_grazing_2d.md`
/// §4a). The ladder is a *sequence* (wild → pastoral → pen), so a species' reach is a single ceiling,
/// not two independent flags — which makes the incoherent "pennable but not tameable" state
/// unrepresentable (no `validate()` combination guard needed). `Wild` is hunt-only (domestication never
/// accrues, `tame`/`corral`/`extend_pen` reject); `Pastoral` tames + roams but never pens
/// (`corral`/`extend_pen` reject); `Pen` is the full ladder. **Default `Pen`** preserves the pre-δ
/// universal-full-ladder behaviour for any untagged/future species.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HusbandryCeiling {
    /// Hunt-only. Domestication never accrues.
    Wild,
    /// Reaches the mobile-tamed rung but never the pen.
    Pastoral,
    /// The full ladder — the default.
    #[default]
    Pen,
}

impl HusbandryCeiling {
    /// Stable string key (also the snapshot `husbandry_ceiling` field / the wire `husbandryCeiling`).
    pub fn as_str(&self) -> &'static str {
        match self {
            HusbandryCeiling::Wild => "wild",
            HusbandryCeiling::Pastoral => "pastoral",
            HusbandryCeiling::Pen => "pen",
        }
    }

    /// Parse the stable string key back (inverse of `as_str`; the rollback restore path). Unknown/empty
    /// strings resolve to the `Default` (`Pen`), preserving the full ladder.
    pub fn from_key(key: &str) -> Self {
        match key {
            "wild" => HusbandryCeiling::Wild,
            "pastoral" => HusbandryCeiling::Pastoral,
            _ => HusbandryCeiling::Pen,
        }
    }

    /// Can this species be **tamed** (mobile domestication)? True for `Pastoral` and `Pen`.
    pub fn allows_domestication(&self) -> bool {
        !matches!(self, HusbandryCeiling::Wild)
    }

    /// Can this species be **penned** (corralled)? True only for `Pen`.
    pub fn allows_pen(&self) -> bool {
        matches!(self, HusbandryCeiling::Pen)
    }
}

/// One species row in the table.
#[derive(Debug, Clone, Deserialize)]
pub struct SpeciesDef {
    /// Player-facing name; also the snapshot `species` string. Must embed the
    /// client icon keyword (e.g. "deer", "boar") so `FoodIcons.for_herd` resolves.
    pub display_name: String,
    #[serde(default)]
    pub size_class: SizeClass,
    #[serde(default)]
    pub migratory: bool,
    /// Inclusive `[min, max]` route length in tiles = roaming range.
    pub route_len: [u32; 2],
    /// Inclusive `[min, max]` group biomass.
    pub biomass: [f32; 2],
    /// Food-module keys (see `FoodModule::as_str`) this species hosts in.
    #[serde(default)]
    pub host_biomes: Vec<String>,
    /// Turns the group grazes its current tile before stepping ≤1 hex (the graze-wander cadence,
    /// `advance_herds`). `~1` → effectively half speed, so an equal-speed party can catch it during
    /// a graze turn. Game rows use this; migratory rows use it for the pause between loiter wanders.
    #[serde(default = "default_dwell_turns")]
    pub dwell_turns: u32,
    /// Migratory only: inclusive `[min, max]` turns to loiter (graze-wander near an anchor) before
    /// committing to the next directed migration leg.
    #[serde(default = "default_loiter_turns")]
    pub loiter_turns: [u32; 2],
    /// Migratory only: hex radius of the local graze-wander around a loiter anchor.
    #[serde(default = "default_loiter_radius")]
    pub loiter_radius: u32,
    /// **Fodder one unit of animal biomass demands per turn** (Grazing Phase 2b-i). A herd of
    /// `biomass` draws `fodder_per_biomass × biomass` graze from the tiles in its range each turn
    /// (`fauna::advance_herd_grazing`), the metabolic denominator that turns the land's *grass flow*
    /// into *animals*. Smaller animals run hotter per unit mass, so small game carries the largest
    /// value and migratory megafauna the smallest. Cached onto `Herd` at spawn (mirroring
    /// `carrying_capacity`). Defaults to `0.0` (a non-grazing species) for a config that omits it —
    /// harmless while Phase 2b-i is inert on carrying capacity.
    #[serde(default)]
    pub fodder_per_biomass: f32,
    /// **Per-species logistic regrowth rate** for a *wild* herd (Grazing Phase 2b-ii). Replaces the
    /// single global `fauna.ecology.regrowth_rate` (0.05) that every animal used to breed at — the
    /// artifact that made "small game can't provision an expedition" (PR #117): a rabbit bred at a
    /// mammoth's rate. Fast small game breeds hot (~0.35), slow megafauna cold (~0.04). Cached onto
    /// `Herd` at spawn (mirroring `fodder_per_biomass` / `carrying_capacity`) and folded into the
    /// herd's *wild* ecology by [`crate::fauna::herd_ecology`]; the **pastoral/pen** rungs keep their
    /// own faster `r` (0.25 / 0.90), and the phase bands stay shared. `None` (omitted) falls back to
    /// `fauna.ecology.regrowth_rate`, so an older config stays non-breaking. Validated finite & `> 0`
    /// when present.
    #[serde(default)]
    pub regrowth_rate: Option<f32>,
    /// **How fast this species tames, as a multiple of the `animal:pastoral` rung's own pace**
    /// (intensification ladder slice 3c). The rung owns the *mechanic*; the species scales it —
    /// exactly the split [`SpeciesDef::regrowth_rate`] already uses against `pastoral_gain`/`pen_gain`.
    /// A single dial on the rung would tame a rabbit and a Steppe Runner in the same 25 turns; taming
    /// a small, quick, forgiving animal should be fast, and binding a large migratory herd should be
    /// generational. Roster: rabbit/fowl/crag_goat `1.0` (25 turns) · boar `0.8` (~31) · aurochs `0.5`
    /// (50) · steppe_runner/marsh_grazer `0.2` (125); a `wild`-ceiling species (deer, mammoth) never
    /// tames, so it carries none.
    ///
    /// **It is a TIMESCALE — it scales the rung's `decay_per_turn` as well as its `progress_per_turn`**
    /// (`RungDef::build_accrual` / `build_decay`, the one seam that honors it). Scaling the speed alone
    /// would put a Steppe Runner's `0.04 × 0.2 = 0.008`/turn *below* the rung's `0.01`/turn decay —
    /// literally untameable, and a violation of the ladder's "taming must out-run its decay" bound.
    /// Scaling both keeps the ratio: **slow to tame, slow to forget**.
    ///
    /// Defaults to `1.0` (the rung's own pace) when omitted, so an untagged or future species keeps
    /// today's behaviour. **Playtest dial.** Validated finite & `> 0` (at `0`/negative the species
    /// would silently never tame, or un-tame while worked).
    #[serde(default = "default_taming_rate")]
    pub taming_rate: f32,
    /// **How far up the husbandry ladder this species climbs** (Grazing 2d-δ) — `wild` | `pastoral` |
    /// `pen`. Cached onto `Herd` at spawn (mirroring `fodder_per_biomass` / `regrowth_rate`) and gates
    /// domestication accrual + the `domesticate` / `corral` / `extend_pen` paths. Defaults to `pen`
    /// (the full ladder) when omitted. See [`HusbandryCeiling`].
    #[serde(default)]
    pub husbandry_ceiling: HusbandryCeiling,
}

/// Default graze pause: one turn of grazing between hex steps (≈ half movement speed).
fn default_dwell_turns() -> u32 {
    1
}

/// Default migratory loiter window (turns) at an anchor before the next migration leg.
fn default_loiter_turns() -> [u32; 2] {
    [12, 24]
}

/// **A species that tames at the `animal:pastoral` rung's own pace** — the neutral timescale, so an
/// untagged (or future) species behaves exactly as it did before the dial existed. Also what an
/// unresolvable species name reads as (`FaunaConfig::taming_rate_for`).
pub const DEFAULT_TAMING_RATE: f32 = 1.0;

fn default_taming_rate() -> f32 {
    DEFAULT_TAMING_RATE
}

/// Default migratory loiter wander radius (hexes) around an anchor. Also the fallback grazing-range
/// radius for a migratory herd whose species row can't be resolved (`Herd::graze_range_radius`).
pub(crate) fn default_loiter_radius() -> u32 {
    2
}

impl SpeciesDef {
    /// Sample a route length within the configured inclusive range (>= 1).
    pub fn sample_route_len(&self, rng: &mut SmallRng) -> u32 {
        let lo = self.route_len[0].max(1);
        let hi = self.route_len[1].max(lo);
        rng.gen_range(lo..=hi)
    }

    /// Sample a migratory loiter window (turns) within the configured inclusive range (>= 1).
    pub fn sample_loiter_turns(&self, rng: &mut SmallRng) -> u32 {
        let lo = self.loiter_turns[0].max(1);
        let hi = self.loiter_turns[1].max(lo);
        rng.gen_range(lo..=hi)
    }

    /// Sample a group biomass within the configured inclusive range.
    pub fn sample_biomass(&self, rng: &mut SmallRng) -> f32 {
        let lo = self.biomass[0].max(0.0);
        let hi = self.biomass[1].max(lo);
        if hi <= lo {
            lo
        } else {
            rng.gen_range(lo..=hi)
        }
    }

    pub fn hosts_biome(&self, module_key: &str) -> bool {
        self.host_biomes.iter().any(|b| b == module_key)
    }

    /// Per-species carrying capacity biomass regrows toward (= the table max).
    pub fn carrying_capacity(&self) -> f32 {
        self.biomass[1].max(self.biomass[0]).max(0.0)
    }

    /// The **wild** per-species logistic regrowth rate to cache on a spawned `Herd`, falling back to
    /// the global `fauna.ecology.regrowth_rate` when the row omits its own (Grazing Phase 2b-ii). The
    /// pastoral/pen rungs never read this — they keep their own faster `r` (see
    /// [`crate::fauna::herd_ecology`]).
    pub fn regrowth_rate_or(&self, wild_default: f32) -> f32 {
        self.regrowth_rate.unwrap_or(wild_default)
    }
}

/// Spawn-density tuning. `per_biome` is the per-tile probability of placing a game
/// group, keyed by the tile's food module; abundance is high to start by design.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AbundanceConfig {
    pub per_biome: HashMap<String, f32>,
    pub max_total_game: usize,
    pub min_spacing: u32,
}

impl AbundanceConfig {
    pub fn probability_for(&self, module_key: &str) -> f32 {
        self.per_biome
            .get(module_key)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
}

/// One-shot hunt tuning: how much biomass a hunt takes, how it converts to
/// resources, and the pursuit geometry (band closes to `pursuit_radius` tiles).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HuntConfig {
    pub take_fraction: f32,
    pub min_take: f32,
    pub provisions_per_biomass: f32,
    pub trade_goods_per_biomass: f32,
    pub pursuit_radius: u32,
    pub pursuit_tiles_per_turn: u32,
    pub max_pursuit_turns: u32,
}

impl Default for HuntConfig {
    fn default() -> Self {
        Self {
            take_fraction: 0.30,
            min_take: 40.0,
            provisions_per_biomass: 0.02,
            trade_goods_per_biomass: 0.005,
            pursuit_radius: 1,
            pursuit_tiles_per_turn: 3,
            max_pursuit_turns: 12,
        }
    }
}

impl HuntConfig {
    /// Biomass taken from a group of `biomass`, clamped to `[min_take, biomass]`.
    pub fn take_from(&self, biomass: f32) -> f32 {
        if biomass <= 0.0 {
            return 0.0;
        }
        let fraction_take = (biomass * self.take_fraction).max(self.min_take);
        fraction_take.min(biomass)
    }
}

/// Ecology tuning: per-turn **critical-depensation** biomass dynamics toward each
/// species' carrying cap. Above the Allee threshold (`collapse_fraction * cap`) the
/// group regrows logistically at `regrowth_rate`; below it the group is non-viable and
/// declines by `collapse_rate` of its biomass each turn — an irreversible crash to
/// local extinction even without further hunting (the overhunting point-of-no-return).
/// A collapsing remnant below `extinction_floor * cap` disperses (despawns).
/// `stressed_fraction` is the softer band used only to classify a herd's `EcologyPhase`
/// for the client; it does not affect the growth curve.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct EcologyConfig {
    pub regrowth_rate: f32,
    /// Allee threshold as a fraction of carrying capacity. Below `collapse_fraction *
    /// cap` the group collapses instead of regrowing.
    pub collapse_fraction: f32,
    /// Per-turn fractional decline of a collapsing (sub-threshold) group.
    pub collapse_rate: f32,
    /// Upper edge of the "stressed" (depleted-but-recovering) band, as a fraction of
    /// carrying capacity. Classification only.
    pub stressed_fraction: f32,
    /// Viability floor: a group below `extinction_floor * cap` disperses (local
    /// extinction) so a collapse reaches zero in finite turns.
    pub extinction_floor: f32,
}

impl Default for EcologyConfig {
    fn default() -> Self {
        Self {
            regrowth_rate: 0.05,
            collapse_fraction: 0.15,
            collapse_rate: 0.20,
            stressed_fraction: 0.40,
            extinction_floor: 0.02,
        }
    }
}

/// Immigration tuning: a low per-turn chance to respawn a wild-game group up to the
/// abundance cap so an overhunted map slowly replenishes (early forager play stays
/// game-rich). `max_attempts` bounds the per-turn random tile sampling.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ImmigrationConfig {
    pub chance_per_turn: f32,
    pub max_attempts: u32,
}

impl Default for ImmigrationConfig {
    fn default() -> Self {
        Self {
            chance_per_turn: 0.15,
            max_attempts: 12,
        }
    }
}

/// Follow tuning: policy draw-rates (Sustain = regrowth, Surplus = regrowth ×
/// `surplus_multiplier`, Eradicate reuses the one-shot hunt take) plus the small
/// per-turn non-food tracking benefit (fog reveal pulse + morale).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FollowConfig {
    pub surplus_multiplier: f32,
    pub reveal_radius: u32,
    pub reveal_duration_turns: u64,
    pub morale_gain: f32,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            surplus_multiplier: 1.6,
            reveal_radius: 2,
            reveal_duration_turns: 3,
            morale_gain: 0.01,
        }
    }
}

/// Husbandry tuning — **the animal web's own economy**. Taming's own dials are *not* here: the
/// **`Tame` policy**'s build meter (`progress_per_turn` / `decay_per_turn` /
/// `yield_fraction_while_building`) lives on `intensification_ladder.json`'s `animal:pastoral` rung,
/// alongside the pen's on `animal:pen`, so both food webs climb on the same numbers
/// (`crate::intensification`). The retired `claim_threshold` — the `domesticate` command's
/// early-claim — is **gone with the command**: it existed to skip the taming investment, which is
/// the entire decision.
///
/// **The husbandry yield ladder is FLOW-BASED — every rung pays MSY**
/// (`docs/plan_corral_managed_population.md`). Management does not buy a licence to eat the standing
/// stock; it buys a **higher growth rate**, because a managed herd is protected from predation,
/// disease and winter kill. The rungs differ *only* in the ecology their MSY is computed against, and
/// in what that ecology costs you:
///
/// | Rung | Ecology | `r` | Costs |
/// |---|---|---|---|
/// | Wild | `fauna.ecology` | per-species `wild_r` | a worker |
/// | Mobile domesticated (**pastoral**) | [`PastoralConfig::ecology`] | `min(cap, wild_r × pastoral_gain)` | none — passive |
/// | Penned (**pen**) | [`PenConfig::ecology`] | `min(cap, wild_r × pen_gain)` | a worker + **food upkeep** + pinned |
///
/// Since Grazing 2d the managed rungs are **per-species** (`wild_r × gain`, capped) rather than the
/// retired flat `0.25 / 0.90` — a penned rabbit and a penned mammoth are different economies. A penned
/// herd's carrying capacity is its **fenced footprint's** graze flow (`hex_range_tiles(corralled_at,
/// pen_radius)`), so it grazes its own land and the larder only pays what the pasture cannot cover
/// (`pen_upkeep × biomass × (1 − pasture_fraction)`) — `capacity_fraction` is retired.
///
/// The managed harvest **draws the herd down**, which is what makes it sustainable: the herd
/// converges on `K/2` and holds there, paying `r·K/4` forever. Both husbandry rungs take it through
/// the shared helper `fauna::managed_yield_biomass`, which is **constant-*escapement* MSY** —
/// `take = min(peak_regrowth(K), max(0, B − K/2))` — **not** the constant-*catch* `sustainable_yield`
/// a wild `Sustain` hunt takes. The sim regrows in Logistics and harvests in Population, so a
/// constant-catch take is evaluated at the **post**-regrowth biomass; above `K/2` both forms cap at
/// MSY and converge on `K/2`, but **below `K/2`** constant-catch removes `g(B + g(B)) > g(B)` — more
/// than the herd grew — which at the pen's `r` = 0.90 spirals a fully-fed herd to zero. Escapement
/// never takes a herd below `K/2`, so a depleted managed herd **rebuilds** (yielding less while it
/// does) and then pays `r·K/4` forever — stable from both sides. The retired flat
/// `provisions_per_biomass` / `corral_provisions_per_biomass` rates, by contrast, paid a share of
/// standing **stock** and never drew the herd down at all — a penned herd parked at capacity and
/// printed food forever (~48× the Sustain baseline).
///
/// **Corral (Rung 1c) levers.** Corralling is an **explicit `Corral` policy with an investment
/// cost**, the animal twin of Cultivate. Its **build dials moved to the shared ladder**,
/// `data/intensification_ladder.json` → the `animal:pen` rung's `build` block
/// (`crate::intensification`), so both food webs climb on the same numbers: while the pen is being
/// built (`Herd::corral_progress` < 1.0) the crew takes only that rung's
/// `yield_fraction_while_building × the herd's Sustain (MSY) ceiling` — a sustainable draw, so the
/// herd stays healthy — accruing its `progress_per_turn` each turn; at `1.0` the herd is penned
/// (`corralled_at`) and its keeper harvests the pen's MSY, paying `pen.upkeep_per_biomass` per unit
/// of biomass in feed. What stays here is the animal web's own economy. `knowledge_progress_per_turn` /
/// `knowledge_completion_threshold` are the earned-**Herding**-knowledge levers (the animal mirror of
/// `CultivationConfig`'s `knowledge_*`): a Sustain-hunt on a Thriving herd teaches the faction Herding
/// (into the `DiscoveryProgressLedger`, discovery `HERDING_DISCOVERY_ID`), the gate **both** animal
/// investment verbs check today — `Tame` at rung 2 and `Corral` at rung 3. **The cultivation
/// asymmetry is gone:** taming is no longer ungated, and a Sustain hunt no longer tames anything —
/// it only *teaches*, exactly as a Sustain forage only teaches Cultivation.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HusbandryConfig {
    /// The **mobile domesticated** (pastoral) rung: the ecology a tamed, roaming herd lives under.
    pub pastoral: PastoralConfig,
    /// The **penned** rung: the ecology a corralled herd lives under, plus what the pen costs to run.
    pub pen: PenConfig,
    /// **Per-species husbandry growth (Grazing 2d §3).** The mobile-domesticated (pastoral) rung grows
    /// at `min(husbandry_regrowth_cap, wild_r × pastoral_gain)` — a MULTIPLE of the herd's own wild
    /// breeding rate, not a flat rate, so a tamed rabbit and a tamed mammoth are different economies.
    /// `> 1` (management must beat wild growth); `< pen_gain` (the ladder is monotone). Folded into the
    /// pastoral ecology by [`crate::fauna::herd_ecology`]; retires the flat `pastoral.ecology.regrowth_rate`.
    pub pastoral_gain: f32,
    /// The penned rung's growth multiplier: `min(husbandry_regrowth_cap, wild_r × pen_gain)` — the top
    /// of the ladder (`> pastoral_gain`). Retires the flat `pen.ecology.regrowth_rate`.
    pub pen_gain: f32,
    /// The stable-band ceiling on any managed `r`: `pastoral`/`pen` growth is capped here so a fast
    /// breeder (rabbit wild 0.35 × pen_gain 3.0 = 1.05) is held to a logistic rate that does not
    /// overshoot/oscillate. `0.75` keeps the discrete logistic monotone.
    pub husbandry_regrowth_cap: f32,
    /// **The largest fenced-footprint radius a pen may reach** (Grazing 2d-β, the `ExtendPen` command).
    /// Each worked-off ring grows `Herd::pen_radius` by 1; the command refuses once `pen_radius` reaches
    /// this. `2` → up to a 19-tile footprint (`hex_range_tiles` disk `1, 7, 19`). Validated `>= 1`
    /// (a `0` cap would forbid every extension).
    pub pen_radius_max: u32,
}

impl Default for HusbandryConfig {
    fn default() -> Self {
        Self {
            pastoral: PastoralConfig::default(),
            pen: PenConfig::default(),
            pastoral_gain: DEFAULT_PASTORAL_GAIN,
            pen_gain: DEFAULT_PEN_GAIN,
            husbandry_regrowth_cap: DEFAULT_HUSBANDRY_REGROWTH_CAP,
            pen_radius_max: DEFAULT_PEN_RADIUS_MAX,
        }
    }
}

/// The **mobile domesticated (pastoral) rung** of the husbandry ladder: a tamed herd that still roams
/// with the band. It pays its owner the MSY of *this* ecology every turn, passively — no worker, no
/// upkeep (a roaming herd grazes the land for free; that is what roaming *is*).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PastoralConfig {
    /// The ecology a *tamed, mobile* herd lives under — the **phase bands only** now. Since Grazing 2d
    /// the pastoral `regrowth_rate` is **per-species** (`min(husbandry_regrowth_cap, wild_r ×
    /// pastoral_gain)`, folded in by [`crate::fauna::herd_ecology`]); this block's own `regrowth_rate`
    /// is unused (it defaults to the wild rate and only the shared `collapse_fraction`/… bands are read,
    /// so a pastoral herd classifies Thriving/Stressed on the same scale as a wild one).
    pub ecology: EcologyConfig,
}

/// The **penned (corral) rung**: a confined herd. Highest growth rate on the ladder — and the only
/// rung with a running cost, because a penned herd **cannot graze** and so must be fed.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PenConfig {
    /// The ecology a *penned* herd lives under — the **phase bands only** now. Since Grazing 2d the pen
    /// `regrowth_rate` is **per-species** (`min(husbandry_regrowth_cap, wild_r × pen_gain)`, folded in
    /// by [`crate::fauna::herd_ecology`] / `pen_ecology_for`); this block's own `regrowth_rate` is
    /// unused (only the shared phase bands are read). The keeper harvests the per-species pen MSY.
    pub ecology: EcologyConfig,
    /// **Feed.** Food/turn the pen demands per unit of standing biomass, drawn from the keeper band's
    /// larder (`upkeep_per_biomass × biomass`). [`DEFAULT_PEN_UPKEEP_PER_BIOMASS`] carries the
    /// derivation and the net-positive invariant it must satisfy — see
    /// [`FaunaConfig::validate`], which enforces it.
    pub upkeep_per_biomass: f32,
    /// **Starvation.** An underfed pen (`fed_fraction < 1`) shrinks by `starve_shrink_rate × (1 −
    /// fed_fraction) × biomass` each turn, floored at `ecology.extinction_floor × K_pen`: the herd
    /// withers to a remnant and **recovers when fed again** (it does not despawn and does not lose the
    /// pen — a recoverable famine is better play than silently voiding a 25-turn investment).
    /// [`DEFAULT_PEN_STARVE_SHRINK_RATE`] carries the derivation. Validated in `[0, 1]`.
    pub starve_shrink_rate: f32,
}

impl Default for PenConfig {
    fn default() -> Self {
        Self {
            // Phase bands only — the pen `regrowth_rate` is per-species (Grazing 2d), so this defaults
            // to the shared wild bands and its own rate is unread.
            ecology: EcologyConfig::default(),
            upkeep_per_biomass: DEFAULT_PEN_UPKEEP_PER_BIOMASS,
            starve_shrink_rate: DEFAULT_PEN_STARVE_SHRINK_RATE,
        }
    }
}

/// **The pastoral growth multiplier (Grazing 2d §3).** A tamed, mobile herd grows `pastoral_gain ×`
/// its own wild breeding rate (capped at [`DEFAULT_HUSBANDRY_REGROWTH_CAP`]) — protection from
/// predation/disease/winter kill buys a *multiple* of the species' own `r`, not a flat rate, so a
/// tamed rabbit (0.35 → 0.525) and a tamed mammoth (0.04 → 0.06) become different economies. Retires
/// the flat `0.25`. A **playtest lever** — measure and tune (`docs/plan_grazing_2d.md` §3).
const DEFAULT_PASTORAL_GAIN: f32 = 1.5;

/// **The pen growth multiplier (Grazing 2d §3).** The ladder's top: a penned herd grows `pen_gain ×`
/// its wild rate (capped). Resulting pen `r`: rabbit `0.75` (capped, booms) · deer `0.30` · mammoth
/// `0.12` (a long-haul investment). Retires the flat `0.90`. A **playtest lever**.
const DEFAULT_PEN_GAIN: f32 = 3.0;

/// **The stable-band cap on any managed `r`.** `wild_r × gain` is clamped here so a fast breeder cannot
/// be scaled into an unstable/oscillating discrete-logistic rate. `0.75` keeps growth monotone (well
/// below the `r ≥ 1` overshoot regime). A **playtest lever**.
const DEFAULT_HUSBANDRY_REGROWTH_CAP: f32 = 0.75;

/// **The largest fenced-footprint radius a pen may reach** (Grazing 2d-β). `2` → up to a 19-tile
/// footprint; each ring is a 25-turn `ExtendPen` labor investment. A **playtest lever** (higher = pens
/// can grow into larger self-feeding operations at more keeper-turns of cost).
const DEFAULT_PEN_RADIUS_MAX: u32 = 2;

/// **The pen's feed cost per unit of biomass — the running cost the arc exists to add.**
///
/// **Grazing 2d inverts the old "every pen is net-positive" guarantee (§2.4).** With per-species pen
/// `r` and *situational* (pasture-dependent) feed, a static all-species guarantee no longer models the
/// system: a slow-breeder pen (mammoth pen `r ≈ 0.12` → bound `0.0011`) would reject the shipped
/// `0.002`, yet such a pen running at a loss on poor pasture is now a player's **bad placement, not a
/// config error**. So [`FaunaConfig::validate`] enforces only a **best-case sanity floor**: the upkeep
/// dial must leave the **fastest-breeding** species profitable even when *fully larder-fed* (worst
/// pasture) — `u < r_pen · p / (2 + r_pen)` for `r_pen = min(cap, max_wild_r × pen_gain)`. With
/// `r_pen(rabbit) = 0.75`: `0.002 < 0.75 × 0.02 / 2.75 ≈ 0.0055` ✓. Slow breeders and poor pasture may
/// run a pen at a **loss by design** (see [`PEN_ESCAPEMENT_QUARTERS`] for the operating-point
/// derivation the floor uses).
///
/// **Deliberately left alone by the growth-rate retune**: weakening the feed to fix a balance problem
/// would delete the mechanic the arc exists to add.
const DEFAULT_PEN_UPKEEP_PER_BIOMASS: f32 = 0.002;

/// **How fast an unfed pen wastes away**: a fully-unfed herd loses 10% of its biomass per turn. Slow
/// enough that a bad winter is survivable and visibly recoverable (the player sees the herd shrink and
/// can act), fast enough that neglecting the feed for a decade of turns really does reduce the pen to
/// a remnant.
const DEFAULT_PEN_STARVE_SHRINK_RATE: f32 = 0.10;

/// Market-hunting tuning: the commercial Follow policy over-harvests a large fixed share
/// of biomass each turn (`take_fraction`) and sells it, yielding `trade_goods_multiplier`×
/// the normal trade-goods rate. The heavy take drives the group past the Allee threshold
/// into the depensation collapse (no separate depletion state — pure ecology reuse).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MarketConfig {
    pub take_fraction: f32,
    pub trade_goods_multiplier: f32,
}

impl Default for MarketConfig {
    fn default() -> Self {
        Self {
            take_fraction: 0.20,
            trade_goods_multiplier: 4.0,
        }
    }
}

/// **The graze (pasture) layer** — the land's *animal-edible* vegetal stock (grass, browse, forbs),
/// distinct from the human-edible `ForagePatch.biomass` (seeds/nuts/tubers) on food-module tiles.
/// Authoritative design: `docs/plan_grazing_foundation.md`. It lives on **any vegetated land tile**,
/// with a capacity set by that tile's biome — a temperate forest is rich in nuts and poor in graze
/// (the canopy shades out ground cover); a prairie steppe is the reverse.
///
/// **Homed in `fauna_config.json`, not a file of its own**, because graze is the *substrate of the
/// fauna model*: every consumer of it (herd carrying capacity, competition, overgrazing, migration,
/// spawn placement — Phase 2b/2c) is a fauna system, and no labor/human system may ever read it. That
/// also lets it reuse [`FaunaConfig::validate`] and its `validate_ecology` helper verbatim rather
/// than forking a second loader, env override and error enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GrazeConfig {
    /// Grazeable biomass a tile of each biome carries at capacity. **A pure data table, not a
    /// formula** — every [`TerrainType`] (`TerrainType::VALUES`) must appear (enforced by
    /// [`FaunaConfig::validate`]: a missing biome would silently read as zero graze, i.e. an
    /// invisible dead zone). `0.0` is the *deliberate* reading for water, glacier, bare rock and lava.
    /// The absolute scale is a free parameter — only the *ratios* matter until Phase 2b's
    /// `fodder_per_biomass` denominates it into animals.
    pub capacity_by_biome: HashMap<TerrainType, f32>,
    /// Graze regrowth + the Thriving/Stressed/Collapsing phase bands. **Grass has no Allee
    /// depensation** — `advance_graze_regrowth` runs pure `logistic_regrowth`, never
    /// `net_biomass_delta`'s collapse branch — so `collapse_rate` here is *inert* (it is read by no
    /// graze code path; the shared [`EcologyConfig`] simply carries it, exactly as `labor_config`'s
    /// forage ecology does). `regrowth_rate` is tuned **well above** forage's 0.25 and fauna's 0.05:
    /// see [`DEFAULT_GRAZE_REGROWTH_RATE`].
    pub ecology: EcologyConfig,
    /// The reseed standing crop, as a fraction of the tile's capacity, that a depleted patch is
    /// lifted to before regrowth each turn — the exact mirror of `forage.reseed_floor_fraction`.
    /// Grass reseeds from surrounding ground, so **graze is never permanently dead**: an eaten-out
    /// tile recovers from this seed stock via the normal logistic curve instead of sticking at `0`
    /// (`logistic_regrowth(0, ..) == 0`). Kept below `ecology.collapse_fraction` so a stripped pasture
    /// still reads Collapsing — the floor stops permanent death, it does not hide overgrazing.
    pub reseed_floor_fraction: f32,
    /// **The overgrazing escapement floor** (Grazing Phase 2b-ii), as a fraction of a tile's capacity:
    /// grazing (`fauna::advance_herd_grazing`) can draw a patch down to this biomass but **no lower**
    /// in a turn. This is the constant-*escapement* discipline the coupled herd↔graze system needs to
    /// converge (`docs/plan_grazing_2b.md` §2.2, the same lesson the corral learned): the herd's demand
    /// is a constant-*catch* draw on the graze, and a catch that strips a patch past the point where its
    /// regrowth can refill the offtake collapses the range into a permanently-stripped attractor at the
    /// reseed floor (the herd surviving as a stunted remnant on dead ground). Holding the draw above
    /// this fraction bounds `K` below at `graze_sustainable_flow(escapement·cap)/fodder`, so an
    /// **overgrazed range recovers to a stable smaller herd** instead of crashing. Set **above**
    /// `reseed_floor_fraction` (so it is a real escapement, not just the reseed lift) and **below**
    /// `MSY_BIOMASS_FRACTION` (0.5, the graze's own MSY point — so overgrazing below the productive
    /// intensity is still *possible and visible*, just not unbounded). A **starting anchor** — deeper
    /// (lower) allows more dramatic overgrazing at more crash risk; measure and retune (§9.5).
    pub overgraze_escapement_fraction: f32,
}

/// Graze regrows **fast** — it is the quickest-renewing vegetal stock in the model, and that is the
/// whole economic premise of herding: a pasture eaten to the ground is back within a few seasons,
/// where a nut grove is not.
///
/// Ordering (each rung is a claim about the biology, not a knob): wild fauna `0.05` ≪ forage
/// `0.25` (`labor_config.json`) < **graze `0.40`** ≪ a fed pen `0.90` (a hyper-managed system, not a
/// wild one). At `r = 0.40` a tile's sustainable flow is `r·K/4 = 0.10·K` per turn and a stripped
/// pasture climbs back to ~90% of capacity in ~20 turns (vs ~35 at forage's `0.25`).
const DEFAULT_GRAZE_REGROWTH_RATE: f32 = 0.40;

/// Mirrors `forage.reseed_floor_fraction` (0.02) — see [`GrazeConfig::reseed_floor_fraction`].
const DEFAULT_GRAZE_RESEED_FLOOR_FRACTION: f32 = 0.02;

/// The overgrazing escapement floor (Grazing 2b-ii) — grazing cannot draw a patch below this fraction
/// of capacity, the constant-escapement discipline that keeps the herd↔graze loop convergent. Measured
/// (`core_sim/tests/grazing_2b_convergence.rs`): at `0.25` an overgrazed range settles on degraded
/// ground (graze ~0.25–0.5·cap, `K` ≥ ~0.84·`K_max`) and **recovers**, where the bare reseed floor
/// (0.02) locks it into a stripped remnant. See [`GrazeConfig::overgraze_escapement_fraction`].
const DEFAULT_GRAZE_OVERGRAZE_ESCAPEMENT_FRACTION: f32 = 0.25;

impl Default for GrazeConfig {
    fn default() -> Self {
        Self {
            // Deliberately **empty**. The per-`TerrainType` table is *data*, and its single authoritative copy is
            // `fauna_config.json` — duplicating it here would guarantee the two drift. A config whose
            // `graze` block omits (or under-fills) the table is *rejected* by [`FaunaConfig::validate`]
            // and the builtin — which has it — is used, so an incomplete table can never quietly
            // produce a map with no pasture on it.
            capacity_by_biome: HashMap::new(),
            ecology: EcologyConfig {
                regrowth_rate: DEFAULT_GRAZE_REGROWTH_RATE,
                ..EcologyConfig::default()
            },
            reseed_floor_fraction: DEFAULT_GRAZE_RESEED_FLOOR_FRACTION,
            overgraze_escapement_fraction: DEFAULT_GRAZE_OVERGRAZE_ESCAPEMENT_FRACTION,
        }
    }
}

/// Root fauna configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FaunaConfig {
    pub species: HashMap<String, SpeciesDef>,
    pub abundance: AbundanceConfig,
    pub hunt: HuntConfig,
    pub ecology: EcologyConfig,
    pub follow: FollowConfig,
    pub immigration: ImmigrationConfig,
    pub husbandry: HusbandryConfig,
    pub market: MarketConfig,
    /// The per-biome graze (pasture) layer — see [`GrazeConfig`].
    pub graze: GrazeConfig,
}

impl GrazeConfig {
    /// Grazeable biomass a `terrain` tile carries at capacity. An **unknown** biome reads `0.0`, but
    /// [`FaunaConfig::validate`] guarantees the table is total over [`TerrainType::VALUES`], so on any
    /// loaded config this is a real lookup, never a silent default.
    pub fn capacity_for(&self, terrain: TerrainType) -> f32 {
        self.capacity_by_biome
            .get(&terrain)
            .copied()
            .unwrap_or(NO_GRAZE_CAPACITY)
    }
}

/// A biome that carries no animal-edible vegetation at all (open water, glacier, bare rock, lava,
/// salt flat). Named rather than bare so a `0.0` in the table reads as *"deliberately barren"* and a
/// `0.0` in code reads as *"the same thing"*, not as a fallback that lost its lookup.
pub const NO_GRAZE_CAPACITY: f32 = 0.0;

/// The largest a fraction-valued lever may be (`[0, 1]` / `(0, 1]` bounds in [`FaunaConfig::validate`]).
const MAX_FRACTION: f32 = 1.0;

/// The pen's **escapement point**, expressed in quarters of `K` — the managed harvest never takes the
/// herd below `K/2` (`fauna::managed_yield_biomass`), so `K/2 = 2/4 · K` is where a settled pen sits.
/// Not a tuning value: it is the MSY point of the logistic curve. It appears in the pen's
/// net-positive bound (below), whose derivation is:
///
/// At the settled operating point the herd stands at `K/2` **after** the keeper's take. The feed,
/// however, is charged on the biomass standing **before** it — `K/2 + r·K/4`, i.e. after that turn's
/// regrowth: you feed every animal in the pen, including the ones you are about to harvest. So
///
/// ```text
/// yield = r·K/4 · p            feed = u · (K/2 + r·K/4) = u · K·(2 + r)/4
/// net > 0  ⟺  u < r·p / (2 + r)
/// ```
///
/// (The idealised `u < r·p/2` ignores that the feed is charged post-regrowth, and is therefore a hair
/// *too loose* — it would admit a narrow band of upkeep values that are in fact a net loss.)
///
/// Since Grazing 2d the `r` in that bound is the **fastest** species' pen rate (§2.4) — the floor is a
/// best-case sanity check, not an every-species guarantee.
const PEN_ESCAPEMENT_QUARTERS: f32 = 2.0;

impl FaunaConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_FAUNA_CONFIG)
                .expect("builtin fauna config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, FaunaConfigError> {
        let config: FaunaConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, FaunaConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| FaunaConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        FaunaConfig::from_json_str(&contents)
    }

    /// Enforce the invariants that, if broken, would make the fauna model **silently incoherent**
    /// rather than merely differently-tuned. Runs inside [`FaunaConfig::from_json_str`], so **every**
    /// load path (builtin, default file, `FAUNA_CONFIG_PATH` override) is covered — the
    /// `expedition_config.rs` / `crisis_config.rs` convention. A broken invariant is logged at
    /// **error** level by [`load_fauna_config_from_env`] and the known-good builtin is used instead.
    ///
    /// The load-bearing one is **the pen's net-positive bound**: a pen whose feed costs more than its
    /// harvest yields is a *trap* — the player pays a 25-turn build and a permanent keeper to make
    /// their food situation strictly worse, with nothing in the UI to explain it. See
    /// [`DEFAULT_PEN_UPKEEP_PER_BIOMASS`].
    pub fn validate(&self) -> Result<(), FaunaConfigError> {
        // --- Hunt: the biomass→provisions rate the WHOLE ladder is denominated in. At `0` every rung
        // (wild, pastoral, pen) pays nothing and the food economy silently stops.
        require_positive_finite(
            "hunt.provisions_per_biomass",
            self.hunt.provisions_per_biomass,
        )?;
        require_positive_finite("hunt.take_fraction", self.hunt.take_fraction)?;

        // --- The three ecologies. `regrowth_rate` at `0` is a dead resource (no MSY, no regrowth);
        // the phase fractions must be ordered `extinction_floor < collapse < stressed < 1` or the
        // Thriving/Stressed/Collapsing classification is nonsense.
        validate_ecology("ecology", &self.ecology)?;
        validate_ecology(
            "husbandry.pastoral.ecology",
            &self.husbandry.pastoral.ecology,
        )?;
        validate_ecology("husbandry.pen.ecology", &self.husbandry.pen.ecology)?;

        // --- Per-species levers (Grazing Phase 2b-ii). A `regrowth_rate` present but non-positive is a
        // dead wild herd (no MSY, never grows); a negative/NaN `fodder_per_biomass` would make the
        // range draw-down and the range-derived `K` nonsense. Both are `#[serde(default)]`, so an older
        // config that omits them stays valid (fodder → 0.0 = non-grazing; regrowth → the global wild
        // rate). Iterated in stable key order so the error names a deterministic species.
        let mut species: Vec<(&String, &SpeciesDef)> = self.species.iter().collect();
        species.sort_by(|a, b| a.0.cmp(b.0));
        for (key, def) in species {
            // `"species.<key>.<leaf>"`, leaked to a `&'static str` like [`field`] (a fixed handful,
            // one per species per load — the config is loaded a bounded number of times).
            let species_field = |leaf: &str| -> &'static str {
                Box::leak(format!("species.{key}.{leaf}").into_boxed_str())
            };
            require_non_negative_finite(
                species_field("fodder_per_biomass"),
                def.fodder_per_biomass,
            )?;
            if let Some(regrowth_rate) = def.regrowth_rate {
                require_positive_finite(species_field("regrowth_rate"), regrowth_rate)?;
            }
            // The taming timescale (slice 3c). **Positive is the whole bound**: the multiplier dilates
            // the `animal:pastoral` rung's `progress_per_turn` AND its `decay_per_turn` together, so the
            // ladder's own "taming must out-run its decay" check (`LadderConfig::validate`) already
            // covers every species — the ratio is invariant under a positive scale. At `0` the species
            // would silently never tame while reading as tameable; negative would *un*-tame a herd the
            // crew is working, and (via the same decay) push its progress up while it is abandoned.
            require_positive_finite(species_field("taming_rate"), def.taming_rate)?;
        }

        // --- The ladder is MONOTONE, now as GAINS (Grazing 2d §3): management buys a *multiple* of the
        // species' own wild `r`, so each rung grows faster than the one below it for **every** species.
        // Invert this and penning a herd would *lower* its yield. `pastoral_gain > 1` (management must
        // beat wild growth); `pen_gain > pastoral_gain` (the pen tops the ladder); the cap is a live
        // positive rate (the stable-band ceiling the gains clamp to).
        require_greater_than(
            "husbandry.pastoral_gain",
            self.husbandry.pastoral_gain,
            "1.0 (management must beat wild growth)",
            MAX_FRACTION,
        )?;
        require_greater_than(
            "husbandry.pen_gain",
            self.husbandry.pen_gain,
            "husbandry.pastoral_gain",
            self.husbandry.pastoral_gain,
        )?;
        require_positive_finite(
            "husbandry.husbandry_regrowth_cap",
            self.husbandry.husbandry_regrowth_cap,
        )?;
        // `pen_radius_max` at `0` would forbid every `ExtendPen` (2d-β) — the command could never grow a
        // pen past its single tile, silently disabling the mechanic.
        if self.husbandry.pen_radius_max < 1 {
            return Err(FaunaConfigError::Invalid {
                field: "husbandry.pen_radius_max",
                constraint: "be at least 1 (a 0 cap forbids every pen extension)".to_string(),
                value: self.husbandry.pen_radius_max.to_string(),
            });
        }

        // --- The pen's feed. A shrink rate above 1 would drive an underfed herd's biomass *negative* in
        // one turn; below 0 it would *grow* a starving herd.
        require_in_unit_range(
            "husbandry.pen.starve_shrink_rate",
            self.husbandry.pen.starve_shrink_rate,
        )?;
        require_non_negative_finite(
            "husbandry.pen.upkeep_per_biomass",
            self.husbandry.pen.upkeep_per_biomass,
        )?;
        // **THE PEN MUST NOT BE A TRAP — a BEST-CASE floor (Grazing 2d §2.4).** With per-species pen `r`
        // and pasture-dependent feed, the old "every pen nets positive" guarantee no longer models the
        // system (it would reject slow-breeder worlds outright), and a slow breeder on poor pasture
        // running at a loss is now a player's bad placement, **not** a config error. So we require only
        // that the **fastest-breeding** species stays net-positive even when *fully larder-fed* (worst
        // pasture): at the operating point a pen yields `r·K/4 · p` and eats `u · K·(2 + r)/4`, so it
        // nets positive iff `u < r_pen · p / (2 + r_pen)` for `r_pen = min(cap, max_wild_r × pen_gain)`
        // (see [`PEN_ESCAPEMENT_QUARTERS`]). Shipped: `0.002 < 0.75 × 0.02 / 2.75 ≈ 0.0055` ✓. A
        // violating override would make **even the best pen** a permanent net food LOSS.
        let fastest_pen_r = (self.max_wild_regrowth_rate() * self.husbandry.pen_gain)
            .min(self.husbandry.husbandry_regrowth_cap);
        let net_positive_bound = fastest_pen_r * self.hunt.provisions_per_biomass
            / (PEN_ESCAPEMENT_QUARTERS + fastest_pen_r);
        if self.husbandry.pen.upkeep_per_biomass >= net_positive_bound {
            return Err(FaunaConfigError::Invalid {
                field: "husbandry.pen.upkeep_per_biomass",
                constraint: format!(
                    "be less than r_pen × hunt.provisions_per_biomass / (2 + r_pen) (= \
                     {net_positive_bound}), where r_pen is the FASTEST species' pen rate \
                     min(husbandry_regrowth_cap, max_wild_r × pen_gain) — otherwise even the best pen \
                     costs more feed than its harvest yields"
                ),
                value: self.husbandry.pen.upkeep_per_biomass.to_string(),
            });
        }

        // --- (Husbandry's *build* dials — the pen's rate and its investment dip — are bounded by
        // `LadderConfig::validate`, which owns the `animal:pen` rung's `build` block; so are the
        // **earned-knowledge** dials as of slice 4, which moved to the ladder's `knowledge` block
        // when the earn path became one rung-driven seam. Both bounds now hold for BOTH webs from a
        // single statement, instead of each web restating its own copy.)

        // --- Follow / market / immigration (ported from the builtin-only unit assertions).
        require_greater_than(
            "follow.surplus_multiplier",
            self.follow.surplus_multiplier,
            "the Sustain baseline",
            MAX_FRACTION,
        )?;
        require_open_unit_fraction("market.take_fraction", self.market.take_fraction)?;
        require_greater_than(
            "market.trade_goods_multiplier",
            self.market.trade_goods_multiplier,
            "the base trade rate",
            MAX_FRACTION,
        )?;
        require_in_unit_range(
            "immigration.chance_per_turn",
            self.immigration.chance_per_turn,
        )?;

        // --- The graze (pasture) layer. Same ecology invariants as every other rung; plus the two
        // that make the *table* trustworthy.
        validate_ecology("graze.ecology", &self.graze.ecology)?;
        validate_graze(&self.graze)?;

        Ok(())
    }

    /// The **fastest wild breeding rate** across the species table — each species' own `regrowth_rate`
    /// (or the global wild rate for a row that omits it), folded with `f32::max` and seeded from the
    /// global rate so an empty table falls back to it. The best-case input to the pen's net-positive
    /// floor (Grazing 2d §2.4): the fastest species is the one that must stay profitable.
    fn max_wild_regrowth_rate(&self) -> f32 {
        self.species
            .values()
            .map(|def| def.regrowth_rate_or(self.ecology.regrowth_rate))
            .fold(self.ecology.regrowth_rate, f32::max)
    }

    /// `(key, def)` pairs for every migratory species, in a stable key order.
    pub fn migratory_species(&self) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| def.migratory)
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// Resolve a species row by its `display_name` (the value a `Herd` stores in `species`), so
    /// `advance_herds` can read the herd's movement cadence levers. Display names are unique.
    pub fn species_by_display(&self, display: &str) -> Option<&SpeciesDef> {
        self.species
            .values()
            .find(|def| def.display_name == display)
    }

    /// **The species' taming timescale** ([`SpeciesDef::taming_rate`]), resolved by the display name a
    /// `Herd` carries — the same live-resolution path the movement cadence levers take
    /// (`fauna::advance_herds` → [`FaunaConfig::species_by_display`]), so retuning the dial takes
    /// effect on herds already on the map instead of freezing at spawn. A species the table cannot
    /// resolve (an isolated test fixture) reads [`DEFAULT_TAMING_RATE`] — the rung's own pace, i.e.
    /// exactly the pre-dial behaviour.
    pub fn taming_rate_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(DEFAULT_TAMING_RATE, |def| def.taming_rate)
    }

    /// `(key, def)` pairs for every non-migratory (short-range) game species that
    /// hosts in `module_key`, in a stable key order.
    pub fn game_species_for_biome(&self, module_key: &str) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| !def.migratory && def.hosts_biome(module_key))
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

/// The graze table's own invariants — the ones that decide whether the **land layer** is trustworthy.
///
/// - **Totality.** The table must name every `TerrainType` (`TerrainType::VALUES`). A missing row silently reads
///   `0.0` ([`NO_GRAZE_CAPACITY`]) — an invisible dead zone in the pasture layer that no error, no
///   log line and no overlay would ever explain. Zero must be *stated*, never *defaulted*.
/// - **At least one positive row.** An all-zero table disables the entire layer (no herd could be
///   fed anywhere) while parsing perfectly — exactly the class of "silently turns a feature off"
///   lever this validation exists to catch.
/// - **`reseed_floor_fraction` below `collapse_fraction`.** The floor exists to stop *permanent*
///   death, not to hide overgrazing: at or above the collapse band a stripped pasture would be lifted
///   straight back into a healthier phase every turn, and the ecology phase (and the client's
///   overgrazing warning) would never be able to read Collapsing.
fn validate_graze(graze: &GrazeConfig) -> Result<(), FaunaConfigError> {
    let mut positive_rows = 0usize;
    for terrain in TerrainType::VALUES {
        let Some(&capacity) = graze.capacity_by_biome.get(&terrain) else {
            return Err(FaunaConfigError::Invalid {
                field: "graze.capacity_by_biome",
                constraint: format!(
                    "name every one of the {} biomes (missing {terrain:?}); an absent biome silently \
                     reads as zero graze",
                    TerrainType::VALUES.len()
                ),
                value: format!("{} rows", graze.capacity_by_biome.len()),
            });
        };
        if !capacity.is_finite() || capacity < NO_GRAZE_CAPACITY {
            return Err(FaunaConfigError::Invalid {
                field: "graze.capacity_by_biome",
                constraint: format!("be finite and at least {NO_GRAZE_CAPACITY} for every biome"),
                value: format!("{terrain:?} = {capacity}"),
            });
        }
        if capacity > NO_GRAZE_CAPACITY {
            positive_rows += 1;
        }
    }
    if positive_rows == 0 {
        return Err(FaunaConfigError::Invalid {
            field: "graze.capacity_by_biome",
            constraint: "give at least one biome a positive capacity, or there is no pasture \
                         anywhere on any map"
                .to_string(),
            value: "every biome is 0".to_string(),
        });
    }

    require_in_unit_range("graze.reseed_floor_fraction", graze.reseed_floor_fraction)?;
    require_greater_than(
        "graze.ecology.collapse_fraction",
        graze.ecology.collapse_fraction,
        "graze.reseed_floor_fraction",
        graze.reseed_floor_fraction,
    )?;

    // The overgrazing escapement floor (2b-ii): a real escapement above the reseed lift, and below the
    // graze MSY point (0.5·cap) so overgrazing is still possible/visible. Outside this band it is either
    // useless (≤ reseed floor → the crash-prevention it exists for is gone) or degenerate (≥ 0.5 → no
    // overgrazing can ever happen; a range is pinned at its most-productive intensity forever).
    require_in_unit_range(
        "graze.overgraze_escapement_fraction",
        graze.overgraze_escapement_fraction,
    )?;
    require_greater_than(
        "graze.overgraze_escapement_fraction",
        graze.overgraze_escapement_fraction,
        "graze.reseed_floor_fraction",
        graze.reseed_floor_fraction,
    )?;
    require_greater_than(
        "the graze MSY point (0.5)",
        GRAZE_MSY_BIOMASS_FRACTION,
        "graze.overgraze_escapement_fraction",
        graze.overgraze_escapement_fraction,
    )?;
    Ok(())
}

/// The graze's MSY biomass fraction (`cap/2`) — mirrors `fauna::MSY_BIOMASS_FRACTION` (the logistic
/// peak), named here so the escapement-floor bound reads against the concept, not a bare `0.5`.
const GRAZE_MSY_BIOMASS_FRACTION: f32 = 0.5;

/// Every ecology block (wild / pastoral / pen — and each is a full [`EcologyConfig`]) shares the same
/// invariants: a live growth rate, and phase thresholds ordered `extinction_floor < collapse_fraction
/// < stressed_fraction < 1` so `classify_ecology_phase` can actually separate the three bands.
fn validate_ecology(prefix: &'static str, ecology: &EcologyConfig) -> Result<(), FaunaConfigError> {
    // A `0` regrowth rate is a dead resource: MSY is 0, so every rung of the ladder that reads this
    // ecology silently pays nothing forever.
    require_positive_finite(field(prefix, "regrowth_rate"), ecology.regrowth_rate)?;
    require_positive_finite(field(prefix, "collapse_rate"), ecology.collapse_rate)?;
    require_in_unit_range(field(prefix, "extinction_floor"), ecology.extinction_floor)?;
    require_in_unit_range(
        field(prefix, "collapse_fraction"),
        ecology.collapse_fraction,
    )?;
    require_in_unit_range(
        field(prefix, "stressed_fraction"),
        ecology.stressed_fraction,
    )?;
    require_greater_than(
        field(prefix, "collapse_fraction"),
        ecology.collapse_fraction,
        field(prefix, "extinction_floor"),
        ecology.extinction_floor,
    )?;
    require_greater_than(
        field(prefix, "stressed_fraction"),
        ecology.stressed_fraction,
        field(prefix, "collapse_fraction"),
        ecology.collapse_fraction,
    )?;
    require_greater_than(
        "1.0 (a resource cannot be 'stressed' at capacity)",
        MAX_FRACTION,
        field(prefix, "stressed_fraction"),
        ecology.stressed_fraction,
    )?;
    Ok(())
}

/// `"<prefix>.<leaf>"` as a `&'static str` — the ecology checks are run over three different blocks,
/// so the error must name *which* one. Leaked deliberately: there are a fixed handful of these, they
/// live for the process, and it keeps [`FaunaConfigError::Invalid`]'s `field` a cheap `&'static str`
/// (matching the `expedition_config.rs` convention) instead of forcing a `String` on every call site.
fn field(prefix: &'static str, leaf: &'static str) -> &'static str {
    Box::leak(format!("{prefix}.{leaf}").into_boxed_str())
}

fn require_positive_finite(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: "be finite and greater than 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

fn require_non_negative_finite(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value < 0.0 {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: "be finite and at least 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

/// `[0, 1]` — a fraction that may legitimately be zero (an off switch) or whole.
fn require_in_unit_range(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || !(0.0..=MAX_FRACTION).contains(&value) {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!("be finite and in [0, {MAX_FRACTION}]"),
            value: value.to_string(),
        });
    }
    Ok(())
}

// NB: `require_fraction` — the `(0, 1]` bound — went with the earned-knowledge dials it was this
// config's only caller of (slice 4). It lives on as `intensification::validate_knowledge`'s
// `completion_threshold` check, which now states the bound once for both food webs.

/// `(0, 1)` — a strict fraction: neither end is coherent (`0` = the lever does nothing, `1` = it does
/// everything, and in both cases the mechanic it gates disappears).
fn require_open_unit_fraction(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value <= 0.0 || value >= MAX_FRACTION {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!("be finite and in (0, {MAX_FRACTION})"),
            value: value.to_string(),
        });
    }
    Ok(())
}

/// A strict cross-field ordering (`value > other`) — the shape most of this config's real invariants
/// take (the monotone ladder, the ordered phase bands, accrual out-running decay).
fn require_greater_than(
    field: &'static str,
    value: f32,
    other_field: &'static str,
    other: f32,
) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value <= other {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!("be finite and greater than {other_field} (= {other})"),
            value: value.to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum FaunaConfigError {
    #[error("failed to read fauna config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse fauna config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid fauna config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
}

/// Handle for accessing the fauna configuration.
#[derive(Resource, Debug, Clone)]
pub struct FaunaConfigHandle(pub Arc<FaunaConfig>);

impl FaunaConfigHandle {
    pub fn new(config: Arc<FaunaConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<FaunaConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<FaunaConfig>) {
        self.0 = config;
    }
}

impl Default for FaunaConfigHandle {
    fn default() -> Self {
        Self(FaunaConfig::builtin())
    }
}

/// Metadata about the fauna configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct FaunaConfigMetadata {
    path: Option<PathBuf>,
}

impl FaunaConfigMetadata {
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

/// Load fauna configuration from environment (`FAUNA_CONFIG_PATH`) or the default
/// data path, falling back to the baked-in builtin.
///
/// Every candidate goes through [`FaunaConfig::from_json_str`], so it is **validated** before it can
/// reach the sim: an override that would silently break the model (a pen that eats more than it
/// yields, an inverted husbandry ladder, an unreachable knowledge gate, …) is rejected and logged at
/// **error** level naming the broken invariant, and the known-good builtin is used instead.
pub fn load_fauna_config_from_env() -> (Arc<FaunaConfig>, FaunaConfigMetadata) {
    let override_path = env::var("FAUNA_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/fauna_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match FaunaConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "fauna_config.loaded=file"
                );
                return (Arc::new(config), FaunaConfigMetadata::new(Some(path)));
            }
            // A broken invariant is an operator error, not a missing file: the config that *was*
            // found says something incoherent. Shout about it.
            Err(err @ FaunaConfigError::Invalid { .. }) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "fauna_config.invalid_rejected"
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "fauna_config.load_failed"
                );
            }
        }
    }

    let config = FaunaConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "fauna_config.loaded=builtin"
    );
    (config, FaunaConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intensification::{LadderConfig, RungKey, RUNG_COMPLETE};

    #[test]
    fn builtin_config_parses() {
        let config = FaunaConfig::builtin();
        assert!(config.species.contains_key("deer"));
        assert!(config.species.contains_key("rabbit"));
        assert!(config.species.contains_key("mammoth"));
        // Display names must embed the client icon keyword.
        assert!(config.species["deer"]
            .display_name
            .to_lowercase()
            .contains("deer"));
        assert!(config.species["boar"]
            .display_name
            .to_lowercase()
            .contains("boar"));
    }

    /// Grazing 2d-δ: the shipped roster's husbandry ceilings, and the `pen` default for an omitted one.
    #[test]
    fn builtin_husbandry_ceilings_match_the_roster() {
        let config = FaunaConfig::builtin();
        use HusbandryCeiling::*;
        for (key, expected) in [
            ("mammoth", Wild),
            ("deer", Wild),
            ("steppe_runner", Pastoral),
            ("marsh_grazer", Pastoral),
            ("boar", Pen),
            ("rabbit", Pen),
            ("fowl", Pen),
        ] {
            assert_eq!(
                config.species[key].husbandry_ceiling, expected,
                "{key} husbandry_ceiling"
            );
        }
        // An omitted field defaults to `pen` (the full ladder), preserving pre-δ behaviour.
        let def: SpeciesDef =
            serde_json::from_str(r#"{"display_name":"X","route_len":[1,1],"biomass":[1,1]}"#)
                .unwrap();
        assert_eq!(def.husbandry_ceiling, HusbandryCeiling::Pen);
    }

    /// Slice 3c: the shipped taming timescales, and the `1.0` default for an omitted one. The
    /// **turns-to-tame** each implies is what the roster is really claiming, so assert that — a dial
    /// read back as a number nobody can interpret is not a guard.
    #[test]
    fn builtin_taming_rates_match_the_roster() {
        let config = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        let progress_per_turn = ladder
            .rung(RungKey::AnimalPastoral)
            .build
            .as_ref()
            .expect("the pastoral rung builds")
            .progress_per_turn;

        for (key, rate, turns_to_tame) in [
            ("rabbit", 1.0_f32, 25.0_f32),
            ("fowl", 1.0, 25.0),
            ("crag_goat", 1.0, 25.0),
            ("boar", 0.8, 31.25),
            ("aurochs", 0.5, 50.0),
            ("steppe_runner", 0.2, 125.0),
            ("marsh_grazer", 0.2, 125.0),
        ] {
            let def = &config.species[key];
            assert_eq!(def.taming_rate, rate, "{key} taming_rate");
            assert!(
                (RUNG_COMPLETE / (progress_per_turn * def.taming_rate) - turns_to_tame).abs()
                    < 0.01,
                "{key} should tame in ~{turns_to_tame} turns"
            );
        }
        // A `wild`-ceiling species never tames at all, so it states no rate (and reads the default).
        for key in ["deer", "mammoth"] {
            assert_eq!(
                config.species[key].husbandry_ceiling,
                HusbandryCeiling::Wild
            );
            assert_eq!(config.species[key].taming_rate, DEFAULT_TAMING_RATE);
        }
        // An omitted field taming at the rung's own pace is what keeps an untagged/future species on
        // today's 25 turns.
        let def: SpeciesDef =
            serde_json::from_str(r#"{"display_name":"X","route_len":[1,1],"biomass":[1,1]}"#)
                .unwrap();
        assert_eq!(def.taming_rate, DEFAULT_TAMING_RATE);
        // And an unresolvable species reads the same, so a fixture herd can never tame at `0`/turn.
        assert_eq!(config.taming_rate_for("No Such Beast"), DEFAULT_TAMING_RATE);
    }

    /// A `taming_rate` of `0` reads as "tameable" everywhere (the ceiling still says `pastoral`) while
    /// the meter never moves — the silent-disable failure mode config validation exists to catch. A
    /// negative one would *un*-tame a herd its crew is working.
    #[test]
    fn validate_rejects_a_non_positive_taming_rate() {
        for bad in [0.0, -0.2] {
            let err = reject(|json| json["species"]["rabbit"]["taming_rate"] = (bad).into());
            assert_rejects_field(err, "species.rabbit.taming_rate");
        }
    }

    #[test]
    fn migratory_and_game_partitions() {
        let config = FaunaConfig::builtin();
        let migratory = config.migratory_species();
        assert!(migratory.iter().all(|(_, def)| def.migratory));
        assert!(migratory.iter().any(|(k, _)| k.as_str() == "mammoth"));

        // Deer hosts in temperate forest and is short-range game.
        let forest_game = config.game_species_for_biome("temperate_forest");
        assert!(forest_game.iter().any(|(k, _)| k.as_str() == "deer"));
        assert!(forest_game.iter().all(|(_, def)| !def.migratory));
    }

    #[test]
    fn abundance_probability_clamps() {
        let config = FaunaConfig::builtin();
        assert!(config.abundance.probability_for("temperate_forest") > 0.0);
        assert_eq!(config.abundance.probability_for("deep_ocean"), 0.0);
    }

    /// The levers `validate()` deliberately does NOT bound (they have coherent meanings at their
    /// extremes) plus the `take_from` clamp — everything else moved into the validator, which every
    /// load path now runs (`builtin()` would panic below if the shipped config broke one).
    #[test]
    fn hunt_and_ecology_present() {
        let config = FaunaConfig::builtin();
        assert_eq!(config.hunt.pursuit_radius, 1);
        assert!(config.immigration.max_attempts >= 1);
        assert!(config.follow.reveal_radius >= 1);
        // take clamps to [min_take, biomass].
        assert_eq!(config.hunt.take_from(0.0), 0.0);
        assert_eq!(config.hunt.take_from(10.0), 10.0); // below min_take -> whole group
        let big = config.hunt.take_from(10_000.0);
        assert!(big >= config.hunt.min_take && big <= 10_000.0);
    }

    /// The shipped ladder is monotone (management buys a growth rate) and the pen nets positive at its
    /// operating point — the two invariants the whole arc rests on, asserted on the *shipped* numbers.
    #[test]
    fn builtin_husbandry_ladder_is_monotone_and_the_pen_pays() {
        let config = FaunaConfig::builtin();
        // The ladder is monotone as GAINS now (Grazing 2d): pastoral beats wild, pen tops pastoral.
        assert!(
            config.husbandry.pen_gain > config.husbandry.pastoral_gain
                && config.husbandry.pastoral_gain > 1.0,
            "1.0 < {} < {}",
            config.husbandry.pastoral_gain,
            config.husbandry.pen_gain
        );
        // Best-case floor: the FASTEST species' pen rate must still net positive when fully larder-fed.
        let fastest_pen_r = (config.max_wild_regrowth_rate() * config.husbandry.pen_gain)
            .min(config.husbandry.husbandry_regrowth_cap);
        let bound = fastest_pen_r * config.hunt.provisions_per_biomass
            / (PEN_ESCAPEMENT_QUARTERS + fastest_pen_r);
        assert!(
            config.husbandry.pen.upkeep_per_biomass < bound,
            "the shipped pen must net positive for the fastest breeder: {} < {bound}",
            config.husbandry.pen.upkeep_per_biomass
        );
    }

    /// Mutate the builtin, re-serialize, and re-load it through `from_json_str` — the *only* entry
    /// point, so this exercises the same validation every load path (builtin/file/env override) runs.
    fn reject(mutate: impl FnOnce(&mut serde_json::Value)) -> FaunaConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_FAUNA_CONFIG).expect("builtin parses");
        mutate(&mut json);
        FaunaConfig::from_json_str(&json.to_string())
            .expect_err("a broken invariant must be rejected")
    }

    fn assert_rejects_field(err: FaunaConfigError, expected: &str) {
        match err {
            FaunaConfigError::Invalid { field, .. } => assert_eq!(field, expected),
            other => panic!("expected an Invalid error for {expected}, got {other:?}"),
        }
    }

    /// **The load-bearing one.** A pen whose feed costs more than its harvest yields is a trap: the
    /// player pays a 25-turn build + a permanent keeper to make their food situation strictly worse.
    #[test]
    fn validate_rejects_a_pen_that_eats_more_than_it_yields() {
        // Best-case floor (Grazing 2d §2.4): r_pen(fastest) = min(0.75, 0.35 × 3.0) = 0.75, so the
        // bound is 0.75 × 0.02 / 2.75 ≈ 0.0055; at or above it EVEN THE BEST pen is a net loss.
        let err = reject(|json| json["husbandry"]["pen"]["upkeep_per_biomass"] = (0.0065).into());
        assert_rejects_field(err, "husbandry.pen.upkeep_per_biomass");
        let err = reject(|json| json["husbandry"]["pen"]["upkeep_per_biomass"] = (0.008).into());
        assert_rejects_field(err, "husbandry.pen.upkeep_per_biomass");
        // The shipped value has ample room inside the bound.
        assert!(FaunaConfig::builtin().validate().is_ok());
    }

    /// The ladder must be monotone in `r`: a pen that grows no faster than the pastoral rung would
    /// pay *less* than it (it also carries feed), inverting the whole intensification incentive.
    #[test]
    fn validate_rejects_an_inverted_husbandry_ladder() {
        // The ladder is monotone as GAINS now (Grazing 2d): a pen that grows no faster than the
        // pastoral rung inverts the incentive.
        let err = reject(|json| json["husbandry"]["pen_gain"] = (1.2).into());
        assert_rejects_field(err, "husbandry.pen_gain");
        // Management must beat wild growth, or taming is a downgrade.
        let err = reject(|json| json["husbandry"]["pastoral_gain"] = (0.9).into());
        assert_rejects_field(err, "husbandry.pastoral_gain");
    }

    #[test]
    fn validate_rejects_a_dead_ecology() {
        let err = reject(|json| json["ecology"]["regrowth_rate"] = (0.0).into());
        assert_rejects_field(err, "ecology.regrowth_rate");
        let err =
            reject(|json| json["husbandry"]["pen"]["ecology"]["regrowth_rate"] = (0.0).into());
        // The pen ecology block still carries the shared phase bands, so a `0` regrowth trips
        // `validate_ecology` (its `regrowth_rate` must be a live rate, even though the *managed* growth
        // rate is now per-species and does not read it).
        assert!(matches!(err, FaunaConfigError::Invalid { .. }));
    }

    #[test]
    fn validate_rejects_unordered_ecology_phase_bands() {
        let err = reject(|json| json["ecology"]["stressed_fraction"] = (0.10).into());
        assert_rejects_field(err, "ecology.stressed_fraction");
        let err = reject(|json| json["ecology"]["extinction_floor"] = (0.50).into());
        assert_rejects_field(err, "ecology.collapse_fraction");
    }

    #[test]
    fn validate_rejects_an_out_of_range_starve_rate() {
        let err = reject(|json| json["husbandry"]["pen"]["starve_shrink_rate"] = (1.5).into());
        assert_rejects_field(err, "husbandry.pen.starve_shrink_rate");
    }

    // The pen's *build* dials moved to the ladder — their rejection tests moved with them, to
    // `crate::intensification`'s `rejects_a_free_investment` / `rejects_a_starving_investment` /
    // `rejects_a_non_building_progress_rate`.

    // NB: the earned-knowledge dials moved to the ladder in slice 4 (both webs' copies were
    // identical once the earn path became one rung-driven seam), and so did this rejection test —
    // `intensification::tests::rejects_a_ladder_nobody_could_ever_learn` /
    // `rejects_a_knowledge_gate_that_is_open_or_shut_from_the_start` now assert the bound **once**,
    // for both food webs, instead of each web guarding its own copy.

    // NB: "taming must out-run its own decay" is still guarded — it moved to
    // `intensification::tests::rejects_taming_that_cannot_outrun_its_decay` along with the dials
    // themselves (the `animal:pastoral` rung's `build` block), where `LadderConfig::validate` now
    // owns the bound for *every* rung of *both* food webs rather than each web re-asserting it.

    #[test]
    fn validate_rejects_a_zero_provisions_rate() {
        // The rate the WHOLE ladder is denominated in: at `0` every rung silently pays nothing.
        let err = reject(|json| json["hunt"]["provisions_per_biomass"] = (0.0).into());
        assert_rejects_field(err, "hunt.provisions_per_biomass");
    }

    /// A rejected override must fall back to the **known-good builtin**, never disable the model.
    #[test]
    fn an_invalid_override_falls_back_to_the_builtin() {
        let dir = std::env::temp_dir().join("shadow_scale_fauna_config_validate");
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("trap_pen.json");
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_FAUNA_CONFIG).expect("builtin parses");
        json["husbandry"]["pen"]["upkeep_per_biomass"] = (10.0).into();
        fs::write(&path, json.to_string()).expect("write override");

        assert!(
            FaunaConfig::from_file(&path).is_err(),
            "the trap pen is refused"
        );
        // The builtin is still loadable and sane — the sim keeps running on it.
        let builtin = FaunaConfig::builtin();
        assert!(builtin.validate().is_ok());
    }

    #[test]
    fn size_class_round_trips() {
        assert_eq!(SizeClass::Big.as_str(), "big");
        assert_eq!(SizeClass::Migratory.as_str(), "migratory");
    }

    /// The graze table must be **total** over every `TerrainType`. A missing row would silently read as
    /// zero graze — an invisible dead zone in the pasture layer that nothing would ever explain.
    #[test]
    fn validate_rejects_a_partial_graze_biome_table() {
        let err = reject(|json| {
            json["graze"]["capacity_by_biome"]
                .as_object_mut()
                .expect("table")
                .remove("PrairieSteppe");
        });
        assert_rejects_field(err, "graze.capacity_by_biome");
    }

    /// An all-zero table parses perfectly and disables the entire layer — no pasture anywhere, on any
    /// map. Exactly the "silently turns a feature off" class of lever validation exists to catch.
    #[test]
    fn validate_rejects_an_all_zero_graze_table() {
        let err = reject(|json| {
            let table = json["graze"]["capacity_by_biome"]
                .as_object_mut()
                .expect("table");
            for value in table.values_mut() {
                *value = (0.0).into();
            }
        });
        assert_rejects_field(err, "graze.capacity_by_biome");
    }

    #[test]
    fn validate_rejects_a_negative_graze_capacity() {
        let err =
            reject(|json| json["graze"]["capacity_by_biome"]["PrairieSteppe"] = (-1.0).into());
        assert_rejects_field(err, "graze.capacity_by_biome");
    }

    /// A dead graze ecology (`r = 0`) means grass never regrows — every pasture is a one-shot stock
    /// and, from Phase 2b, every herd starves.
    #[test]
    fn validate_rejects_a_dead_graze_ecology() {
        let err = reject(|json| json["graze"]["ecology"]["regrowth_rate"] = (0.0).into());
        assert_rejects_field(err, "graze.ecology.regrowth_rate");
    }

    /// The reseed floor stops *permanent* death; it must not hide overgrazing. At or above
    /// `collapse_fraction` a stripped pasture is lifted back into a healthier band every turn and the
    /// Collapsing phase (and the client's overgrazing warning) becomes unreachable.
    #[test]
    fn validate_rejects_a_reseed_floor_that_hides_overgrazing() {
        let err = reject(|json| json["graze"]["reseed_floor_fraction"] = (0.5).into());
        assert_rejects_field(err, "graze.ecology.collapse_fraction");
    }

    /// The shipped table's model claims, asserted rather than assumed: open grassland is pasture,
    /// closed-canopy forest is not, and water/ice/rock carry nothing at all.
    #[test]
    fn builtin_graze_table_is_total_and_sane() {
        let config = FaunaConfig::builtin();
        let graze = &config.graze;
        assert_eq!(graze.capacity_by_biome.len(), TerrainType::VALUES.len());
        let prairie = graze.capacity_for(TerrainType::PrairieSteppe);
        assert!(prairie > 0.0);
        assert!(prairie > graze.capacity_for(TerrainType::MixedWoodland));
        assert!(prairie > graze.capacity_for(TerrainType::Tundra));
        assert_eq!(
            graze.capacity_for(TerrainType::DeepOcean),
            NO_GRAZE_CAPACITY
        );
        assert_eq!(graze.capacity_for(TerrainType::Glacier), NO_GRAZE_CAPACITY);
        assert!(graze.reseed_floor_fraction < graze.ecology.collapse_fraction);
    }
}
