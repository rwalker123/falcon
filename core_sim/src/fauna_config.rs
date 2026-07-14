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
}

/// Default graze pause: one turn of grazing between hex steps (≈ half movement speed).
fn default_dwell_turns() -> u32 {
    1
}

/// Default migratory loiter window (turns) at an anchor before the next migration leg.
fn default_loiter_turns() -> [u32; 2] {
    [12, 24]
}

/// Default migratory loiter wander radius (hexes) around an anchor.
fn default_loiter_radius() -> u32 {
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
#[derive(Debug, Clone, Deserialize)]
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

/// Husbandry / domestication tuning: a sustained Sustain-follow on a Thriving herd
/// accrues `progress_per_turn` toward taming (1.0 = domesticated); progress that isn't
/// being actively sustained decays by `decay_per_turn`. The explicit `domesticate`
/// command may claim a herd early once progress reaches `claim_threshold`.
///
/// **The husbandry yield ladder is FLOW-BASED — every rung pays MSY**
/// (`docs/plan_corral_managed_population.md`). Management does not buy a licence to eat the standing
/// stock; it buys a **higher growth rate**, because a managed herd is protected from predation,
/// disease and winter kill. The rungs differ *only* in the ecology their MSY is computed against, and
/// in what that ecology costs you:
///
/// | Rung | Ecology | `r` | Costs |
/// |---|---|---|---|
/// | Wild | `fauna.ecology` | 0.05 | a worker |
/// | Mobile domesticated (**pastoral**) | [`PastoralConfig::ecology`] | 0.15 | none — passive |
/// | Penned (**pen**) | [`PenConfig::ecology`] | 0.60 | a worker + **food upkeep** + pinned |
///
/// The managed harvest **draws the herd down** (it takes `sustainable_yield(..)`, exactly as the
/// `Sustain` hunt policy does), which is what makes it sustainable: the herd converges on `K/2` and
/// holds there, paying `r·K/4` forever. The retired flat `provisions_per_biomass` /
/// `corral_provisions_per_biomass` rates paid a share of standing **stock** and never drew the herd
/// down at all — a penned herd parked at capacity and printed food forever (~48× the Sustain
/// baseline).
///
/// **Corral (Rung 1c) levers.** Corralling is an **explicit `Corral` policy with an investment
/// cost**, the animal twin of Cultivate: while the pen is being built (`Herd::corral_progress` < 1.0)
/// the crew takes only `corralling_yield_fraction × the herd's Sustain (MSY) ceiling` — a sustainable
/// draw, so the herd stays healthy — accruing `corral_build_progress_per_turn` each turn; at `1.0` the
/// herd is penned (`corralled_at`) and its keeper harvests the pen's MSY, paying `pen.upkeep_per_biomass`
/// per unit of biomass in feed. `knowledge_progress_per_turn` /
/// `knowledge_completion_threshold` are the earned-**Herding**-knowledge levers (the animal mirror of
/// `CultivationConfig`'s `knowledge_*`): a Sustain-hunt on a Thriving herd teaches the faction Herding
/// (into the `DiscoveryProgressLedger`, discovery `HERDING_DISCOVERY_ID`), the gate the `Corral` policy
/// checks. Note the asymmetry vs. cultivation — mobile *domestication* stays ungated; only corralling
/// needs Herding. `claim_threshold` remains the **`domesticate`** command's early-claim gate on
/// *mobile* taming (unrelated to corralling, which has no early claim).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HusbandryConfig {
    pub progress_per_turn: f32,
    pub decay_per_turn: f32,
    pub claim_threshold: f32,
    /// The **mobile domesticated** (pastoral) rung: the ecology a tamed, roaming herd lives under.
    pub pastoral: PastoralConfig,
    /// The **penned** rung: the ecology a corralled herd lives under, plus what the pen costs to run.
    pub pen: PenConfig,
    /// **The investment cost of corralling** (the animal twin of `cultivating_yield_fraction`): while
    /// the pen is being built, the Hunt take ceiling is this fraction of the herd's **Sustain (MSY)**
    /// ceiling — the crew is building, not hunting. A fraction of MSY is a sustainable draw, so the
    /// herd stays Thriving (which the accrual gate wants). Validated `0 < f < 1`.
    pub corralling_yield_fraction: f32,
    /// Pen construction accrued per turn a band works a domesticated herd it owns under the **Corral**
    /// policy (`Herd::corral_progress`, `1.0` = penned). At `0.04` a pen takes 25 turns to build,
    /// matching the plant side's `cultivation.progress_per_turn`.
    pub corral_build_progress_per_turn: f32,
    /// Rung 1b/1c earned knowledge: faction **Herding** knowledge accrued per turn a band
    /// Sustain-hunts a Thriving herd (into the `DiscoveryProgressLedger`). Herding is *learned by
    /// hunting*, never start-granted; the `corral` command is refused until the faction knows it.
    pub knowledge_progress_per_turn: f32,
    /// Ledger progress (`0..=1`) at which the faction **knows** Herding and may `corral`. `1.0` = the
    /// ledger's completion value (`DiscoveryProgressLedger` clamps accrual to `1.0`).
    pub knowledge_completion_threshold: f32,
}

impl Default for HusbandryConfig {
    fn default() -> Self {
        Self {
            progress_per_turn: 0.04,
            decay_per_turn: 0.01,
            claim_threshold: 0.6,
            pastoral: PastoralConfig::default(),
            pen: PenConfig::default(),
            corralling_yield_fraction: DEFAULT_CORRALLING_YIELD_FRACTION,
            corral_build_progress_per_turn: DEFAULT_CORRAL_BUILD_PROGRESS_PER_TURN,
            knowledge_progress_per_turn: 0.05,
            knowledge_completion_threshold: 1.0,
        }
    }
}

/// The **mobile domesticated (pastoral) rung** of the husbandry ladder: a tamed herd that still roams
/// with the band. It pays its owner the MSY of *this* ecology every turn, passively — no worker, no
/// upkeep (a roaming herd grazes the land for free; that is what roaming *is*).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PastoralConfig {
    /// The ecology a *tamed, mobile* herd lives under. Only `regrowth_rate` differs from the wild
    /// `fauna.ecology` in the shipped config — the phase bands (`collapse_fraction` etc.) are the
    /// shared defaults, so a pastoral herd classifies Thriving/Stressed on the same scale.
    /// [`DEFAULT_PASTORAL_REGROWTH_RATE`] carries the derivation.
    pub ecology: EcologyConfig,
}

/// The **penned (corral) rung**: a confined herd. Highest growth rate on the ladder — and the only
/// rung with a running cost, because a penned herd **cannot graze** and so must be fed.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PenConfig {
    /// The ecology a *penned* herd lives under: shelter, feed and protection buy the ladder's top
    /// growth rate ([`DEFAULT_PEN_REGROWTH_RATE`]). The keeper harvests this ecology's MSY.
    pub ecology: EcologyConfig,
    /// The pen's carrying capacity as a fraction of the herd's own (`K_pen = capacity_fraction ×
    /// carrying_capacity`) — the pen holds a share of what the land held, so it scales per-species
    /// with no new absolute. `1.0` (the shipped value) = the pen is as roomy as the range; lower it to
    /// make penning a *smaller but faster-growing* population. Validated `> 0`.
    pub capacity_fraction: f32,
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
            ecology: EcologyConfig {
                regrowth_rate: DEFAULT_PEN_REGROWTH_RATE,
                ..EcologyConfig::default()
            },
            capacity_fraction: DEFAULT_PEN_CAPACITY_FRACTION,
            upkeep_per_biomass: DEFAULT_PEN_UPKEEP_PER_BIOMASS,
            starve_shrink_rate: DEFAULT_PEN_STARVE_SHRINK_RATE,
        }
    }
}

impl Default for PastoralConfig {
    fn default() -> Self {
        Self {
            ecology: EcologyConfig {
                regrowth_rate: DEFAULT_PASTORAL_REGROWTH_RATE,
                ..EcologyConfig::default()
            },
        }
    }
}

/// **The pastoral growth rate — 5× wild.** Taming a herd protects it from predation, disease and
/// winter kill, so it grows faster; that higher `r` (and *only* that) is what domestication buys.
/// Everything else about the rung is unchanged, and the yield is still the same MSY *flow*
/// (`r·K/4 × hunt.provisions_per_biomass`), so the rungs stay commensurable. At the shipped levers a
/// Red Deer herd (K = 1200) pays `0.25 × 1200 / 4 × 0.02` = **1.50 food/turn** — clearly *above* a
/// ~30-person band's entire demand (~0.79), so taming a herd buys a real **surplus**: savings, an
/// expedition, the settle pull.
///
/// **Retuned from 0.15**, which was measured (a scripted 100-turn campaign on three pinned seeds) to
/// land a freshly-taming band at income **1.275** against consumption **1.294** — a permanent
/// one-day-of-food treadmill with no savings, no affordable expedition, and a `SedentarizationScore`
/// that never reached its soft threshold. The retired stock-share rate, for contrast, paid **12.0**
/// (sixteen bands' entire demand, free, forever). Both are absurd; this sits between them.
const DEFAULT_PASTORAL_REGROWTH_RATE: f32 = 0.25;

/// **The pen's growth rate — 18× wild, 3.6× pastoral.** A penned herd is sheltered, fed and guarded:
/// the top of the ladder, and the reason the pen is worth a 25-turn build plus a permanent keeper.
/// At the shipped levers Red Deer (K = 1200) grosses `0.90 × 1200 / 4 × 0.02` = **5.40 food/turn**,
/// and at its settled operating point nets **≈ 3.66** of that after feed — **12× wild Sustain** and
/// **≈ 2.4× the free pastoral rung below it**, so the ladder stays monotone and the pen still earns
/// its worker + feed + being pinned.
const DEFAULT_PEN_REGROWTH_RATE: f32 = 0.90;

/// The pen holds exactly what the range held (`K_pen = K`). A v1 anchor: it makes the penned rung a
/// pure *growth-rate* upgrade, so nothing about the model turns on a second, arbitrary scale.
const DEFAULT_PEN_CAPACITY_FRACTION: f32 = 1.0;

/// **The pen's feed cost per unit of biomass — the running cost that is the whole point of the arc.**
/// Chosen against the pen's own operating point so the pen is always a net gain and never a trap:
/// it nets positive iff **`u < r · p / (2 + r)`** (see [`PEN_ESCAPEMENT_QUARTERS`] for the
/// derivation) = `0.90 × 0.02 / 2.90` ≈ `0.0062`. The shipped `0.002` sits a **~3.1× margin** inside
/// that bound — a real cost (Red Deer: ≈ 1.74 food/turn at `B*`, roughly a third of the 5.40 gross)
/// that still leaves the pen the ladder's best rung. [`FaunaConfig::validate`] **enforces** the bound,
/// so an override cannot silently turn corralling into a permanent net food loss.
///
/// **Deliberately left alone by the growth-rate retune**: weakening the feed to fix a balance problem
/// would delete the mechanic the arc exists to add.
const DEFAULT_PEN_UPKEEP_PER_BIOMASS: f32 = 0.002;

/// **How fast an unfed pen wastes away**: a fully-unfed herd loses 10% of its biomass per turn. Slow
/// enough that a bad winter is survivable and visibly recoverable (the player sees the herd shrink and
/// can act), fast enough that neglecting the feed for a decade of turns really does reduce the pen to
/// a remnant.
const DEFAULT_PEN_STARVE_SHRINK_RATE: f32 = 0.10;

/// **The investment cost of corralling**: while the pen is being built, a Corral hunt takes only this
/// fraction of the herd's Sustain (MSY) ceiling — and, because the passive pastoral rung is skipped for
/// a herd a band is working (`Herd::worked_this_turn`), that dip is the builder's *whole* income from
/// the animal. At `0.50` a Red Deer build pays **0.75/turn against the 1.50** of walking away — ~19
/// provisions forgone over the 25 turns, recouped ~9 turns after the pen opens.
///
/// **Retuned from 0.25** (the plant side's `labor_config::DEFAULT_CULTIVATION_CULTIVATING_YIELD_FRACTION`):
/// measured, that dip forced the band to fund the build out of a *famine* and crashed its population
/// ~50% before the pen completed. The cost must be paid from a **surplus**, not a starvation.
const DEFAULT_CORRALLING_YIELD_FRACTION: f32 = 0.50;
/// Pen construction per turn under the Corral policy → 25 turns to build, matching the plant side's
/// `cultivation.progress_per_turn`. A dedicated lever (not the taming `progress_per_turn`) so pen
/// speed and tame speed can be tuned independently.
const DEFAULT_CORRAL_BUILD_PROGRESS_PER_TURN: f32 = 0.04;

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
    /// formula** — every one of the 37 [`TerrainType`]s must appear (enforced by
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

impl Default for GrazeConfig {
    fn default() -> Self {
        Self {
            // Deliberately **empty**. The 37-row table is *data*, and its single authoritative copy is
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

        // --- The ladder is MONOTONE: management buys a growth rate, so each rung must grow faster
        // than the one below it. Invert this and penning a herd would *lower* its yield — the player
        // pays a build + a keeper + feed to earn less.
        require_greater_than(
            "husbandry.pen.ecology.regrowth_rate",
            self.husbandry.pen.ecology.regrowth_rate,
            "husbandry.pastoral.ecology.regrowth_rate",
            self.husbandry.pastoral.ecology.regrowth_rate,
        )?;
        require_greater_than(
            "husbandry.pastoral.ecology.regrowth_rate",
            self.husbandry.pastoral.ecology.regrowth_rate,
            "ecology.regrowth_rate",
            self.ecology.regrowth_rate,
        )?;

        // --- The pen. `capacity_fraction` at `0` gives `K_pen = 0` → the MSY is 0 and the herd is
        // instantly below every phase threshold: penning would delete the herd's yield outright.
        require_positive_finite(
            "husbandry.pen.capacity_fraction",
            self.husbandry.pen.capacity_fraction,
        )?;
        // A shrink rate above 1 would drive an underfed herd's biomass *negative* in one turn; below 0
        // it would *grow* a starving herd.
        require_in_unit_range(
            "husbandry.pen.starve_shrink_rate",
            self.husbandry.pen.starve_shrink_rate,
        )?;
        require_non_negative_finite(
            "husbandry.pen.upkeep_per_biomass",
            self.husbandry.pen.upkeep_per_biomass,
        )?;
        // **THE PEN MUST NOT BE A TRAP.** At its settled operating point the pen yields `r·K/4 · p`
        // and eats `u · K·(2 + r)/4`, so it nets positive iff `u < r·p / (2 + r)` (see
        // [`PEN_ESCAPEMENT_QUARTERS`] for the derivation). Shipped:
        // `0.002 < 0.60 × 0.02 / 2.6 ≈ 0.00462` ✓ (a ~2.3× margin). A violating override would make
        // corralling a permanent, silent net food LOSS — the player pays a 25-turn build and a
        // permanent keeper to make their food situation strictly worse.
        let pen_regrowth = self.husbandry.pen.ecology.regrowth_rate;
        let net_positive_bound = pen_regrowth * self.hunt.provisions_per_biomass
            / (PEN_ESCAPEMENT_QUARTERS + pen_regrowth);
        if self.husbandry.pen.upkeep_per_biomass >= net_positive_bound {
            return Err(FaunaConfigError::Invalid {
                field: "husbandry.pen.upkeep_per_biomass",
                constraint: format!(
                    "be less than pen.ecology.regrowth_rate × hunt.provisions_per_biomass / \
                     (2 + pen.ecology.regrowth_rate) (= {net_positive_bound}), or the pen costs more \
                     feed than its harvest yields food"
                ),
                value: self.husbandry.pen.upkeep_per_biomass.to_string(),
            });
        }

        // --- Corral / husbandry accrual. A `0` build rate never finishes a pen; a `0` (or `1`)
        // yield fraction makes the "investment dip" either total or free.
        require_open_unit_fraction(
            "husbandry.corralling_yield_fraction",
            self.husbandry.corralling_yield_fraction,
        )?;
        require_positive_finite(
            "husbandry.corral_build_progress_per_turn",
            self.husbandry.corral_build_progress_per_turn,
        )?;
        require_positive_finite(
            "husbandry.knowledge_progress_per_turn",
            self.husbandry.knowledge_progress_per_turn,
        )?;
        // `0` would mean Herding is known before it is learned; `> 1` is unreachable (the ledger
        // clamps accrual to 1.0), so the gate could never open.
        require_fraction(
            "husbandry.knowledge_completion_threshold",
            self.husbandry.knowledge_completion_threshold,
        )?;
        // Taming must out-run its own decay, or no herd can ever be domesticated by sustained work.
        require_greater_than(
            "husbandry.progress_per_turn",
            self.husbandry.progress_per_turn,
            "husbandry.decay_per_turn",
            self.husbandry.decay_per_turn,
        )?;
        require_non_negative_finite("husbandry.decay_per_turn", self.husbandry.decay_per_turn)?;
        // The `domesticate` command's early-claim gate: at `0` every herd is claimable instantly, at
        // `>= 1` the "early" claim is never early.
        require_open_unit_fraction("husbandry.claim_threshold", self.husbandry.claim_threshold)?;

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
/// - **Totality.** The table must name every one of the 37 biomes. A missing row silently reads
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
    Ok(())
}

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

/// `(0, 1]` — a fraction that must do *something* but may be whole.
fn require_fraction(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value <= 0.0 || value > MAX_FRACTION {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!("be finite and in (0, {MAX_FRACTION}]"),
            value: value.to_string(),
        });
    }
    Ok(())
}

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
        let wild = config.ecology.regrowth_rate;
        let pastoral = config.husbandry.pastoral.ecology.regrowth_rate;
        let pen = config.husbandry.pen.ecology.regrowth_rate;
        assert!(
            pen > pastoral && pastoral > wild,
            "{wild} < {pastoral} < {pen}"
        );
        // net > 0 at the settled operating point ⟺ upkeep < r·p / (2 + r).
        let bound = pen * config.hunt.provisions_per_biomass / (PEN_ESCAPEMENT_QUARTERS + pen);
        assert!(
            config.husbandry.pen.upkeep_per_biomass < bound,
            "the shipped pen must net positive: {} < {bound}",
            config.husbandry.pen.upkeep_per_biomass
        );
        assert!(config.husbandry.pen.capacity_fraction > 0.0);
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
        // Bound = r·p / (2 + r) = 0.90 × 0.02 / 2.90 ≈ 0.0062; at or above it the pen is a net loss at
        // its settled operating point.
        let err = reject(|json| json["husbandry"]["pen"]["upkeep_per_biomass"] = (0.0065).into());
        assert_rejects_field(err, "husbandry.pen.upkeep_per_biomass");
        // The *idealised* bound `r·p/2` (= 0.009) ignores that the feed is charged on the
        // post-regrowth biomass, so it is a hair too loose: a config in that band costs more feed than
        // its harvest yields food and must still be refused.
        let err = reject(|json| json["husbandry"]["pen"]["upkeep_per_biomass"] = (0.008).into());
        assert_rejects_field(err, "husbandry.pen.upkeep_per_biomass");
        // The shipped value has ample room inside the bound.
        assert!(FaunaConfig::builtin().validate().is_ok());
    }

    /// The ladder must be monotone in `r`: a pen that grows no faster than the pastoral rung would
    /// pay *less* than it (it also carries feed), inverting the whole intensification incentive.
    #[test]
    fn validate_rejects_an_inverted_husbandry_ladder() {
        let err = reject(|json| {
            json["husbandry"]["pen"]["ecology"]["regrowth_rate"] = (0.10).into();
        });
        assert_rejects_field(err, "husbandry.pen.ecology.regrowth_rate");

        let err = reject(|json| {
            json["husbandry"]["pastoral"]["ecology"]["regrowth_rate"] = (0.05).into();
        });
        assert_rejects_field(err, "husbandry.pastoral.ecology.regrowth_rate");
    }

    #[test]
    fn validate_rejects_a_dead_ecology() {
        let err = reject(|json| json["ecology"]["regrowth_rate"] = (0.0).into());
        assert_rejects_field(err, "ecology.regrowth_rate");
        let err =
            reject(|json| json["husbandry"]["pen"]["ecology"]["regrowth_rate"] = (0.0).into());
        // A `0` pen rate trips the monotone check first — either way it cannot load.
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
    fn validate_rejects_a_pen_with_no_room() {
        let err = reject(|json| json["husbandry"]["pen"]["capacity_fraction"] = (0.0).into());
        assert_rejects_field(err, "husbandry.pen.capacity_fraction");
    }

    #[test]
    fn validate_rejects_an_out_of_range_starve_rate() {
        let err = reject(|json| json["husbandry"]["pen"]["starve_shrink_rate"] = (1.5).into());
        assert_rejects_field(err, "husbandry.pen.starve_shrink_rate");
    }

    #[test]
    fn validate_rejects_a_broken_corral_investment() {
        let err = reject(|json| json["husbandry"]["corralling_yield_fraction"] = (1.0).into());
        assert_rejects_field(err, "husbandry.corralling_yield_fraction");
        let err = reject(|json| json["husbandry"]["corral_build_progress_per_turn"] = (0.0).into());
        assert_rejects_field(err, "husbandry.corral_build_progress_per_turn");
    }

    #[test]
    fn validate_rejects_an_unlearnable_or_pre_learned_herding_gate() {
        let err = reject(|json| json["husbandry"]["knowledge_progress_per_turn"] = (0.0).into());
        assert_rejects_field(err, "husbandry.knowledge_progress_per_turn");
        let err = reject(|json| json["husbandry"]["knowledge_completion_threshold"] = (0.0).into());
        assert_rejects_field(err, "husbandry.knowledge_completion_threshold");
    }

    #[test]
    fn validate_rejects_taming_that_cannot_outrun_its_decay() {
        let err = reject(|json| json["husbandry"]["decay_per_turn"] = (0.04).into());
        assert_rejects_field(err, "husbandry.progress_per_turn");
    }

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

    /// The graze table must be **total** over the 37 biomes. A missing row would silently read as
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
