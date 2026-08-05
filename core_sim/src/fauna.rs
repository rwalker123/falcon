use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use sim_runtime::TerrainTags;
use tracing::info;

use std::hash::{Hash, Hasher};

use crate::{
    combat::{
        self, CombatStats, CombatTuning, Contingent, ContingentId, DamageLedger, FightPayload,
        Force, ForceId, Posture,
    },
    components::{
        floor_overdraws, Improvement, PopulationCohort, ResidentBand, SourceYield, Tile, YieldRange,
    },
    fauna_config::{
        default_loiter_radius, Diet, EcologyConfig, FaunaConfig, FaunaConfigHandle, GrazeConfig,
        HuntYield, HusbandryCeiling, SizeClass, SpeciesDef, YieldAccounts, YieldAxis,
        DEFAULT_HUSBANDRY_DENSITY, NO_GRAZE_CAPACITY,
    },
    food::{classify_food_module, FoodModule},
    graze::GrazeRegistry,
    grid_utils::{
        hex_distance_wrapped, hex_neighbor, hex_neighbors_wrapped, hex_range_tiles,
        HEX_DIRECTION_COUNT,
    },
    hashing::FnvHasher,
    intensification::{
        source_crew_needed, BuildDips, LadderConfig, LadderConfigHandle, RungBranch, RungDef,
        RungKey, RungMovement, NEGLECT_NONE, NO_NEGLECT_GRACE,
    },
    mapgen::WorldGenSeed,
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, SimulationConfig, SimulationTick,
        StartLocation, TileRegistry,
    },
    systems::workers_needed_for_take,
};

/// RNG salt for per-turn immigration, kept distinct from the initial-spawn salt so the
/// two streams don't correlate.
const IMMIGRATION_SEED_SALT: u64 = 0xFA1A_B0B0;

/// RNG salt for per-turn herd graze-wander / loiter movement, distinct from the immigration
/// stream. Combined with `map_seed ^ tick ^ hash(herd.id)` so each herd's wander is deterministic
/// under rollback (mirrors `repopulate_fauna`'s seeding).
const HERD_MOVEMENT_SEED_SALT: u64 = 0x4D0E_9A17_C0FF_EE21;

/// XOR sub-seed salt for the **retreat** draw (`docs/plan_hunt_through_combat.md` §6.2), following
/// the repo's domain-subseed convention (cf. [`HERD_MOVEMENT_SEED_SALT`], `PALETTE_SEED_SALT`).
///
/// **The draw is seeded per EVENT, never taken from a shared stream**, and that is a hard
/// requirement rather than a style choice: a shared stream makes every draw order-dependent, so
/// adding or reordering one hunt shifts every downstream result and rollback stops reproducing.
/// Composed from `(map_seed, tick, herd, party)` by [`retreat_seed`], which is order-independent.
const RETREAT_SEED_SALT: u64 = 0x5CA7_7E12_D00D_1E55;

/// RNG salt for the per-turn neglect-escape shed jitter (`docs/plan_fauna_neglect_escape.md` §3.1),
/// kept distinct from the movement/immigration streams so the shed's ±band doesn't correlate with a
/// herd's wander. Combined with `map_seed ^ tick ^ hash(herd.id)`, exactly like the movement RNG, so
/// the shed is deterministic under rollback (never a wall-clock `rand`).
const ESCAPE_SEED_SALT: u64 = 0x5CA9_E5CA_9EFE_A100;

/// **The whole-animal convergence floor** (`docs/plan_fauna_neglect_escape.md` §3.3): when an
/// under-contained herd's overage is `>= 1` animal, **at least one** animal leaves — otherwise a
/// shrinking overage rounds down to zero and the herd stalls one or two head over its labor capacity
/// forever. Named per the no-magic-numbers rule.
const MIN_ESCAPE_ANIMALS: f32 = 1.0;

/// Id prefix marking a **feral** wild-game group spawned by the neglect-escape shed
/// (`docs/plan_fauna_neglect_escape.md` §2.3). Deliberately **not** [`GAME_ID_PREFIX`]: player-caused
/// ferals must not count against `abundance.max_total_game` (which would both suppress the shed itself
/// and throttle later immigration — §5 item 2), so they carry their own prefix the immigration cap
/// scan skips.
const FERAL_ID_PREFIX: &str = "feral_";

/// Id prefix marking a short-range wild-game group (migratory herds use `herd_`). The
/// `abundance.max_total_game` cap applies to these groups only — both at initial spawn
/// (`placed.len()`) and per-turn immigration.
const GAME_ID_PREFIX: &str = "game_";

/// Id prefix marking a **predator pack** (Predators Phase 1a), distinct from both the short-range
/// `game_` groups and the migratory `herd_` walkers. Predators are seeded by their own
/// [`spawn_predators`] pass, so they do **not** count against `abundance.max_total_game` (which filters
/// on [`GAME_ID_PREFIX`]) and telemetry/tests can find them by this prefix.
const PREDATOR_ID_PREFIX: &str = "pred_";

pub const HERD_DENSITY_REFERENCE_BIOMASS: f32 = 8_000.0;

/// Discovery id for the faction-level **Herding** knowledge — the animal ladder's **rung-2** gate
/// (`docs/plan_intensification_ladder.md` §2a/§4.3), and the mirror of
/// `forage::CULTIVATION_DISCOVERY_ID`. Knowledge is **earned by doing**: working a **wild** herd under
/// a stewardship policy teaches it (`RungDef::knowledge_earned`, driven by the `animal:wild` rung's
/// `earns_knowledge`) — you learn to herd by managing wild herds. It gates the **`Tame`** verb.
/// Declared as a start-profile knowledge tag (`herding` → this id in
/// `data/start_profile_knowledge_tags.json`) purely so it is mappable; it is deliberately **not**
/// listed in any start profile's `starting_knowledge_tags`, so no faction starts knowing it.
///
/// **Herding no longer gates `Corral`** (the §4.3 reshuffle, slice 4): one knowledge per transition,
/// so rung 3 moved onto its own [`PENNING_DISCOVERY_ID`] and this one is `Tame`'s gate alone. The old
/// "mobile domestication stays ungated" asymmetry vs. Cultivation is likewise gone — both webs now
/// gate rung 2 on the knowledge rung 1 teaches. Next free id after `cultivation` (2003).
pub const HERDING_DISCOVERY_ID: u32 = 2004;

/// Discovery id for the faction-level **Penning** knowledge — the animal ladder's **rung-3** gate
/// (`docs/plan_intensification_ladder.md` §2a/§4.3), and the twin of
/// `forage::SEED_SELECTION_DISCOVERY_ID`.
///
/// **Earned by practising rung 2**: working a *pastoral* (tamed) herd under a stewardship policy
/// teaches it (`RungDef::knowledge_earned`, driven by the `animal:pastoral` rung's `earns_knowledge`)
/// — §4's rule exactly, *"you learn herding by managing wild herds; penning by managing tamed ones"*.
/// It gates the **`Corral`** verb (and, riding the same `animal:pen` rung, `ExtendPen`), which
/// **Herding** used to gate. Declared as a start-profile knowledge tag (`penning` → this id in
/// `data/start_profile_knowledge_tags.json`) purely so it is mappable, and deliberately **not**
/// listed in any start profile's `starting_knowledge_tags` — nothing on the ladder is start-granted.
///
/// **Knowledge is general; the husbandry ceiling is per-species** (§4.2): taming a `pastoral`-ceiling
/// Steppe Runner still teaches Penning — you just spend it on a boar, since the runner itself can
/// never be fenced. Next free id after `seed_selection` (2005).
pub const PENNING_DISCOVERY_ID: u32 = 2006;

/// Discovery id for the faction-level **Foddering** knowledge — the pen's *fodder-draw* unlock (Flora
/// Roster F3, `docs/plan_flora_roster.md` §5.2). It is **NOT a new ladder rung**: a pen already
/// exists, and Foddering only unlocks a penned herd's ability to draw the band's `FODDER` store as
/// delivered graze-flow (the feed term, `advance_labor_allocation`, and the `K_pen` fodder term,
/// `ecological_carrying_capacity`). Until a faction knows it a pen is byte-identical to its pre-F3
/// footprint-only self.
///
/// **Earned by running a pen**: the `animal:pen` rung's `earns_knowledge` (`null` pre-F3) is now
/// `foddering`, so working a *penned* herd under a stewardship policy teaches it via the existing
/// `RungDef::knowledge_earned` seam — *you learn to hay a herd by keeping one*. Declared as a
/// start-profile knowledge tag (`foddering` → this id in `data/start_profile_knowledge_tags.json`)
/// purely so it is mappable, and deliberately **not** listed in any start profile's
/// `starting_knowledge_tags` — nothing on the ladder is start-granted. Next free id after `penning`
/// (2006).
pub const FODDERING_DISCOVERY_ID: u32 = 2007;

/// Coarse ecological health band derived from a group's biomass vs its carrying
/// capacity (thresholds in `EcologyConfig`). Surfaced to the client as an early
/// overhunting warning, and the seam the later domestication / industrialized-hunting
/// arc keys off (e.g. a long Sustain-follow on a `Thriving` herd → husbandry progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EcologyPhase {
    /// At or above the stressed band — a healthy, self-sustaining group.
    #[default]
    Thriving,
    /// Depleted but above the collapse threshold — still able to recover if left alone.
    Stressed,
    /// Below the Allee threshold — non-viable and crashing to local extinction
    /// regardless of whether hunting continues (the point of no return).
    Collapsing,
}

impl EcologyPhase {
    /// Stable string key (also the snapshot `ecologyPhase` field).
    pub fn as_str(self) -> &'static str {
        match self {
            EcologyPhase::Thriving => "thriving",
            EcologyPhase::Stressed => "stressed",
            EcologyPhase::Collapsing => "collapsing",
        }
    }

    /// Parse the stable string key back into a phase (inverse of `as_str`; the rollback restore
    /// path). Unknown/empty strings resolve to the `Default` (`Thriving`).
    pub fn from_key(key: &str) -> Self {
        match key {
            "stressed" => EcologyPhase::Stressed,
            "collapsing" => EcologyPhase::Collapsing,
            _ => EcologyPhase::Thriving,
        }
    }
}

/// Classify a group's ecological phase from its biomass fraction of carrying capacity.
pub(crate) fn classify_ecology_phase(
    biomass: f32,
    cap: f32,
    ecology: &EcologyConfig,
) -> EcologyPhase {
    if cap <= 0.0 {
        return EcologyPhase::Collapsing;
    }
    let frac = biomass / cap;
    if frac < ecology.collapse_fraction {
        EcologyPhase::Collapsing
    } else if frac < ecology.stressed_fraction {
        EcologyPhase::Stressed
    } else {
        EcologyPhase::Thriving
    }
}

/// A herd's per-turn movement mode (graze-wander + loiter-then-migrate, `advance_herds`).
/// Game groups graze-wander their local cluster forever; migratory groups alternate loitering near
/// a route anchor and a directed 1-hex/turn migration to the next anchor. See
/// `docs/plan_wildlife_hunting_overlay.md` "Herd Movement".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoamState {
    /// Wild game (`Big`/`Small`): permanent graze-wander toward the current cluster waypoint.
    GrazeWander,
    /// Migratory: loitering near the current anchor for `turns_left` more turns.
    Loiter { turns_left: u32 },
    /// Migratory: a directed leg toward the next anchor at 1 hex/turn, no grazing pause.
    Migrate,
}

/// Stable string keys for `RoamState`, paired with [`RoamState::from_mode`] so the mapping between
/// the live enum and its string spelling lives in one place.
const ROAM_MODE_GRAZE_WANDER: &str = "graze_wander";
const ROAM_MODE_LOITER: &str = "loiter";
const ROAM_MODE_MIGRATE: &str = "migrate";

impl RoamState {
    /// Stable string key for the movement mode (inverse of [`RoamState::from_mode`]).
    pub fn mode_key(self) -> &'static str {
        match self {
            RoamState::GrazeWander => ROAM_MODE_GRAZE_WANDER,
            RoamState::Loiter { .. } => ROAM_MODE_LOITER,
            RoamState::Migrate => ROAM_MODE_MIGRATE,
        }
    }

    /// The loiter countdown (`0` for graze-wander / migrate).
    pub fn loiter_turns_left(self) -> u32 {
        match self {
            RoamState::Loiter { turns_left } => turns_left,
            _ => 0,
        }
    }

    /// Reconstruct from the stable string key + loiter countdown (rollback restore; inverse of
    /// `mode_key` + `loiter_turns_left`). Unknown/empty keys resolve to `GrazeWander`.
    pub fn from_mode(mode: &str, loiter_turns_left: u32) -> Self {
        match mode {
            ROAM_MODE_LOITER => RoamState::Loiter {
                turns_left: loiter_turns_left,
            },
            ROAM_MODE_MIGRATE => RoamState::Migrate,
            _ => RoamState::GrazeWander,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Herd {
    pub id: String,
    pub label: String,
    /// Species display name (also the snapshot `species` string; drives the client
    /// icon via keyword match). Sourced from the data-driven `fauna_config.json`.
    pub species: String,
    /// Coarse size band (snapshot `size_class`); lets the client offer the right verbs.
    pub size_class: SizeClass,
    /// Sparse anchor list (was a dense per-turn path). Game: the small local cluster it wanders;
    /// migratory: the loiter anchors a migration cycles through. `step_index` is the current one.
    pub route: Vec<UVec2>,
    pub step_index: usize,
    /// Live position — walked one hex per move by `advance_herds` (no longer `route[step_index]`).
    pub current_pos: UVec2,
    /// Grazing pause countdown (graze-wander cadence); moves only when this hits 0.
    pub dwell_remaining: u32,
    /// Current movement mode (graze-wander for game, loiter/migrate for migratory).
    pub roam: RoamState,
    /// Next intended hex (client heading arrow): the tile a `Migrate` leg heads to next, else `None`
    /// (loitering/grazing herds show no arrow).
    pub next_pos: Option<UVec2>,
    pub biomass: f32,
    /// Per-species carrying capacity (= table biomass max) that biomass regrows toward.
    pub carrying_capacity: f32,
    /// Per-species **fodder demand per unit biomass** (Grazing Phase 2b-i), cached from the
    /// `SpeciesDef` at spawn exactly as `carrying_capacity` is. Each turn a mobile herd draws
    /// `fodder_per_biomass × biomass` graze from the tiles in its range (`advance_herd_grazing`).
    /// `0.0` for a non-grazing species. **Inert on carrying capacity this slice** — the eating only
    /// draws the graze layer down (visible on the pasture overlay); `K` is still the species constant.
    pub fodder_per_biomass: f32,
    /// Per-species **wild logistic regrowth rate** (Grazing Phase 2b-ii), cached from the `SpeciesDef`
    /// at spawn (mirroring `fodder_per_biomass`), resolved via `SpeciesDef::regrowth_rate_or` so a row
    /// that omits it falls back to `fauna.ecology.regrowth_rate`. [`herd_ecology`] folds it into the
    /// herd's **wild** ecology (fast small game breeds hot, slow megafauna cold); a domesticated
    /// (pastoral) or penned herd ignores it and keeps its rung's own faster `r`. Rewound by rollback with
    /// the rest of the cloned registry (sim-side only — not on the client wire).
    pub regrowth_rate: f32,
    /// **The biomass of ONE animal** ([`crate::fauna_config::SpeciesDef::body_mass`]), cached from the
    /// `SpeciesDef` at spawn exactly as `regrowth_rate` / `fodder_per_biomass` are. The quantum every
    /// hunt take is floored to ([`quantise_animal_take`]): a herd holds `biomass / body_mass` animals,
    /// **derived on demand, never stored** — biomass stays the authoritative stock.
    ///
    /// Rewound by rollback with the rest of the cloned registry (sim-side only — not on the client
    /// wire) so a restored herd keeps its quantum rather than reading `0` and being stripped whole in
    /// one turn.
    pub body_mass: f32,
    /// **The kill-credit accumulator** — biomass a hunting **party** has *earned toward its next
    /// whole animal* but not yet spent, in `[0, biomass]`.
    ///
    /// **THE RESIDENT BAND NO LONGER BANKS** (`docs/plan_harvest_floor.md` §1). Its ceiling is
    /// [`hunt_escapement_ceiling`], a **stock** — the biomass standing above the floor — and banking a
    /// stock compounds it: the herd would offer its whole surplus every turn *plus* everything it had
    /// already handed over. The accumulator that role needed is the herd's own standing biomass, which
    /// crosses one `body_mass` on exactly the cadence the bank used to meter.
    ///
    /// Its one remaining writer is the hunting **expedition** (`systems::expedition_take_biomass`),
    /// where it banks a different quantity: the *party's* per-turn processing throughput, metering
    /// **when** the next whole animal is ready for a body heavier than one turn's work. That bank is
    /// capped at the herd's standing surplus, so it can never fund a kill below the raid's floor.
    ///
    /// Authoritative sim state — rewound by rollback with the cloned registry (sim-side only, not on
    /// the client wire), so a rollback rewinds a herd's progress toward its next kill rather than
    /// resetting the wait.
    pub hunt_credit: f32,
    /// **How far up the husbandry ladder this herd's species can climb** (Grazing 2d-δ), cached from
    /// the `SpeciesDef` at spawn (mirroring `regrowth_rate` / `fodder_per_biomass`). Gates the three
    /// husbandry seams without re-resolving config: taming accrual + the `tame` command (a `Wild` herd
    /// never tames), and the `corral` / `extend_pen` paths (only a `Pen` herd pens).
    /// Rewound by rollback with the cloned registry and exported as `husbandryCeiling`.
    pub husbandry_ceiling: HusbandryCeiling,
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from
    /// biomass vs `carrying_capacity`. Surfaced to the client and the domestication hook.
    pub ecology_phase: EcologyPhase,
    /// Husbandry progress in `[0.0, 1.0]`; `1.0` = domesticated. Accrues while a band
    /// Sustain-follows this (Thriving) group and decays otherwise (see `advance_husbandry`).
    pub domestication_progress: f32,
    /// Faction tending/owning this group (`Some` iff `domestication_progress > 0`).
    pub owner: Option<FactionId>,
    /// Corral (Rung 1c): the tile a **penned** herd is fixed at, or `None` for a mobile herd.
    /// `Some` = the herd does NOT roam (`advance_herds` skips its movement — it stays put) and is
    /// paid its keeper **place-local** at the higher corral rate (via the tending Hunt assignment in
    /// `advance_labor_allocation`), not the mobile even-split husbandry yield. Only a *domesticated*
    /// herd whose owner knows **Penning** can be corralled (`corral` command). Authoritative sim state —
    /// rewound by rollback with the cloned registry. The animal mirror of a cultivated patch being a
    /// fixed tended patch;
    /// contrast the deliberate asymmetry — an *un*corralled domesticated herd stays mobile
    /// (pastoralism travels with the band).
    pub corralled_at: Option<UVec2>,
    /// Pen-construction progress in `[0.0, 1.0]`; `1.0` = the pen is built (and `corralled_at` is set
    /// that same turn). Accrues **only** while a band works this herd with the
    /// [`crate::components::Improvement::Corral`] verb in flight (faction knows **Penning** + owns the
    /// *domesticated* herd), at
    /// `husbandry.corral_build_progress_per_turn`. The animal mirror of
    /// `ForagePatch::cultivation_progress`, and the investment the `corralling_yield_fraction` dip
    /// buys. Authoritative sim state — rewound by rollback with the cloned registry, so a rollback
    /// rewinds a half-built pen rather than losing it. Unlike cultivation it does **not** decay
    /// gradually — but the two ends of its life differ: a **mid-build** gate lapse *keeps* progress
    /// (materials on the ground, not a field growing back over), while a **completed pen that
    /// escapes** (`advance_husbandry`) resets it to `0.0` — the pen is lost along with the herd that
    /// roamed off it, so re-penning pays the full investment again.
    pub corral_progress: f32,
    /// **The pen's footprint radius** (Grazing 2d) — the hex range, centred on `corralled_at`, of the
    /// *fenced land* a penned herd grazes and derives its `K` over (`hex_range_tiles(corralled_at,
    /// pen_radius)`). `0` = today's single tile; each ring the `ExtendPen` command (2d-β) works off
    /// raises it. Read by **all** the pen-footprint logic (K, grazing, the larder offset, the wire
    /// count) so β only has to grow it. Authoritative sim state — rewound by rollback with the cloned
    /// registry.
    pub pen_radius: u32,
    /// Pen-**extension** build progress `[0.0, 1.0]` for the in-flight ring (the `ExtendPen` labor
    /// ladder, 2d-β), accrued each turn the keeper tends an *extending* pen at
    /// `husbandry.corral_build_progress_per_turn`; at `1.0` the ring completes (`pen_radius += 1`, this
    /// resets to `0.0`, `pen_extending` clears). Exported as `penExtendProgress` for a "Fencing N%"
    /// badge. Authoritative sim state, alongside `pen_radius`.
    pub pen_extend_progress: f32,
    /// **The `ExtendPen` "extending" state** (2d-β): `true` while a keeper is fencing the next ring
    /// (`pen_extend_progress` accruing, the harvest dipped to `corralling_yield_fraction`), the animal
    /// mirror of a herd's under-construction `corral_progress`. Set by the `ExtendPen` command, cleared
    /// when the ring completes. A rollback rewinds an in-flight extension rather than stranding a
    /// half-progress meter that never completes.
    pub pen_extending: bool,
    /// **The herd's biomass at the START of this turn, before Logistics regrowth** — captured at the
    /// top of [`regrow_biomass`].
    ///
    /// **It is the basis every `sustainable_yield` PROJECTION is taken on**, never a take: the take
    /// runs *after* regrowth, so evaluating a constant-*catch* rate at the current (grown) biomass
    /// takes slightly more than the herd actually grew (`regen(B_post) > regen(B_pre)`) — a slow leak
    /// that drifts a below-`K/2` herd *down* instead of letting it recover. Its readers are
    /// [`hunt_forecast`]'s rung payoffs (`managed_yield` / `pastoral_yield`) and the forward
    /// projections' Sustain-rate basis.
    ///
    /// **The take path stopped reading it** when the harvest floor made every stance constant
    /// escapement ([`hunt_escapement_ceiling`]): `B − floor·K` is the stock standing above the floor
    /// whenever it is measured, so there is no leak for a pre-regrowth basis to correct.
    ///
    /// Re-stamped every turn at the top of `regrow_biomass`, so it is never more than one turn old;
    /// sim-side only — not on the client wire. Defaults to `biomass` at construction so a herd that has
    /// never regrown reads a sane pre-regrowth value.
    pub biomass_before_regrowth: f32,
    /// Transient per-turn scratch: the graze biomass this herd actually drew from its footprint this
    /// turn (`advance_herd_grazing`, Logistics), read the same turn by the pen larder-offset in
    /// `advance_labor_allocation` (Population). For a penned herd it is what the fenced footprint fed
    /// the pen; the larder pays only the remainder. Recomputed each turn; sim-side only — not on the
    /// client wire.
    pub footprint_intake: f32,
    /// Transient per-turn scratch: the share of a penned herd's feed its footprint covered last FEED
    /// (`footprint_intake / (fodder_per_biomass × biomass)`, clamped `[0, 1]`; Grazing 2d §2.3). `1.0`
    /// = the pasture feeds the pen for free; `0.0` = a barren footprint pays the full larder bill.
    /// Exported as `penPastureFraction`. `0.0` for an unpenned herd.
    pub pen_pasture_fraction: f32,
    /// Transient per-turn scratch: the hay this pen drew from its keeper band's `FODDER` store last
    /// FEED (Flora Roster F3, §5.2), in fodder units. Written by the corral-tend branch of
    /// `advance_labor_allocation` (Population); `0.0` for an unpenned herd, a keeper who does not know
    /// Foddering, or a pen whose footprint already fed it. Exported as `fodderDraw` so the client can
    /// show "fed by hay" beside the `penUpkeep` "fed by bread". Recomputed each turn — it records what
    /// was drawn, while the hay itself lives in the keeper's `LocalStore`.
    pub fodder_draw: f32,
    /// Transient per-turn scratch: the **net** food/turn this pen's keeper hauls from the `FOOD` larder,
    /// *after* the footprint's pasture and any drawn hay have paid their share (Flora Roster F3) — the
    /// corral-tend branch's own `demand` local (`gross pen_upkeep × (1 − land_hay_fraction)`), in
    /// **food** units and the exact number the branch bills. `0.0` when pasture and hay fully feed the
    /// pen, or for an unpenned herd. Exported as `penLarderBill` — the render-ready larder term of the
    /// "pasture NN% · hay X.X · larder Y.Y" feed split, so the client sums nothing. Recomputed each
    /// turn, like `fodder_draw`/`pen_pasture_fraction`.
    pub pen_larder_bill: f32,
    /// Transient per-turn scratch: hay's contribution to this pen's feed, converted to
    /// **food-equivalent** units — the food it *displaced* from the larder (`gross pen_upkeep ×
    /// fodder_draw / grass_demand`, Flora Roster F3). [`Self::fodder_draw`] itself is in grass units
    /// (~25× the food scale) and cannot share a row with the food-unit pasture/larder terms; this can.
    /// `0.0` when no hay was drawn, the keeper lacks Foddering, or the herd is unpenned. Exported as
    /// `penHayFood` — the hay term of the feed split. Written beside `fodder_draw`, and recomputed each
    /// turn with it. The three terms partition the gross bill:
    /// `gross × pen_pasture_fraction + pen_hay_food + pen_larder_bill == gross` (± f32 epsilon).
    pub pen_hay_food: f32,
    /// Transient per-turn scratch: the **sustained fodder inflow** in range of this pen's keeper band
    /// — the per-turn hay output of the band's fodder Fields (Flora Roster F3, §5.3). Written *after*
    /// the assignment loop in `advance_labor_allocation` (Population) and read the **next** turn by
    /// `ecological_carrying_capacity` (Logistics) as the `K_pen` fodder-flow term. It is the **flow**,
    /// deliberately NOT the store's **stock**: raising `K` off a built-up buffer would spike K → the
    /// herd grows → the store empties → K collapses → starvation, an oscillation. The flow is what the
    /// farming sustainably delivers, so the loop settles. `0.0` for an unpenned herd or one no hay
    /// reaches; sim-side only — not on the client wire (the *draw* it produces rides `fodderDraw`).
    pub fodder_delivery_rate: f32,
    /// Transient per-turn flag: a Hunt assignment tended this corralled herd this turn (set in
    /// `advance_labor_allocation`, Population). `advance_husbandry` (Logistics, the *next* turn —
    /// Logistics runs before Population) reads it: a corralled herd tended this turn is spared, an
    /// untended one **escapes** (reverts to mobile). Mirrors `ForagePatch::tended_this_turn`. **Not**
    /// on the client wire (derived), but it **does survive a rollback**: the checkpoint clones the
    /// whole `HerdRegistry` (`SimState::herds`), so a restored pen resumes with exactly the tended flag
    /// it was captured with. That is what keeps the first post-restore Logistics escape pass — which
    /// runs before the labor arm can re-mark a pen its keeper is tending — from escaping a pen a keeper
    /// tends every turn (which would clear `corralled_at`/`pen_radius` and throw away the whole
    /// rebuild).
    pub corralled_tended_this_turn: bool,
    /// Transient per-turn flag: the fraction of the pen's **feed** demand its keeper actually paid last
    /// turn (`paid / demand ∈ [0, 1]`; `1.0` = fully fed, and the value when nothing was demanded).
    /// Written by the corral-tend branch of `advance_labor_allocation` (Population) and read one turn
    /// later by `advance_husbandry` (Logistics), which **starves** an underfed pen — the same
    /// deliberate one-turn lag as `corralled_tended_this_turn`, and reset to `1.0` after reading.
    /// Exported as `penFedFraction`.
    pub pen_fed_fraction: f32,
    /// Transient per-turn signal: how well this managed herd was **staffed** last turn — `min(1,
    /// assigned / herders_needed)`, written by the Hunt arm of `advance_labor_allocation` (Population)
    /// and read one turn later by `advance_husbandry` (Logistics), the same deliberate lag as
    /// [`Herd::pen_fed_fraction`], whose shape this mirrors exactly.
    ///
    /// **Understaffing degrades proportionally — it never triggers an escape** (see
    /// [`herded_fraction`]). `1.0` = fully herded (and the value for a herd nobody needs to herd).
    /// Exported as `herdedFraction`.
    pub herded_fraction: f32,
    /// Transient edge-gate for the starving-pen feed line: `true` while the herd is *already known* to
    /// be starving, so `advance_husbandry` announces the famine **once** on the turn it starts rather
    /// than every turn it continues. Cleared when the pen is fed again (so a *second* famine is
    /// announced afresh). Off the client wire — the *notice* is what the player sees, not the gate —
    /// and a rollback rewinds the gate rather than re-announcing a famine already reported.
    pub pen_starving: bool,
    /// **Was this herd actively TAMED this turn** — set by the `Tame` arm of
    /// `advance_labor_allocation`, read by `advance_husbandry` to spare it from `decay_domestication`.
    /// The animal twin of `ForagePatch::tended_this_turn`, with the same **deliberate one-turn lag**
    /// (Logistics reads what Population wrote last turn) and the same rule: a herd under active taming
    /// neither goes feral nor bleeds its partial progress, so the investment accrues at the **full**
    /// `progress_per_turn` rather than net-of-decay. It is set even when a gate lapses mid-run
    /// (mirroring the plant side) — a crew that showed up and worked keeps the herd from reverting.
    ///
    /// Distinct from a plain hunt at any other policy: a Sustain hunt *harvests* a herd, it does not
    /// tame it, so it must not hold the taming meter up.
    ///
    /// Cleared every turn in `advance_husbandry` after it is read, so it can never go stale; sim-side
    /// only — not on the client wire.
    pub tamed_this_turn: bool,
    /// **The hysteresis-stabilized herder requirement** — the remembered `herders_needed` for a
    /// *managed* herd (`0` for a wild one). The raw `ceil(animals / animals_per_herder)` flickers ±1
    /// every turn when a Sustain-hunted herd's biomass breathes across an `animals_per_herder`
    /// multiple (the lumpy whole-animal kill), trapping the player in a "staff all 1 / staff all 2"
    /// churn that costs them their tameness. This field breaks the flicker with an **asymmetric
    /// deadband** ([`Herd::stabilize_herders_needed`], run every turn in [`advance_husbandry`]):
    /// **up immediately** (under-herding is harmful), **down only once the herd has clearly shrunk**
    /// below the lower rung's ceiling by more than `animals_per_herder ×
    /// husbandry.herders_hysteresis_fraction`. So a herd bumped to 2 holds at 2 across a one-animal
    /// dip and drops only on a genuine multi-band fall.
    ///
    /// It is **the source every consumer reads** ([`herd_herders_needed`] → the `herded_fraction`
    /// decay, [`crate::intensification::source_crew_needed`], the `herdersNeeded` snapshot field).
    /// **Authoritative sim state**
    /// (like `corral_progress`), so a rollback restores the remembered requirement rather than
    /// re-flickering for a turn. `0` also means "not yet stabilized" — a
    /// freshly-tamed or newly-spawned managed herd, for which [`herd_herders_needed`]
    /// falls back to the raw ceil until the next `advance_husbandry` seeds this.
    pub herders_needed: u32,
    /// **Edge-gate for the under-herded feed line** (neglect-escape slice 2,
    /// `docs/plan_fauna_neglect_escape.md` §4 item 1): `true` while the herd is *already known* to be
    /// under-contained — too few herders to hold all its animals, so it is shedding this turn. Set on
    /// the `false → true` transition (`advance_husbandry` fires the notice **once**), cleared the turn
    /// it recovers (fully staffed / no overage) so a later relapse re-announces. The herder-shortfall
    /// twin of `pen_starving`, and like it a rollback rewinds the edge rather than re-firing the
    /// notice.
    pub under_herded: bool,
    /// **How many consecutive turns this managed herd's keepers have failed to hold it** — the
    /// neglect counter the shed is gated on, and the exact twin of `ForagePatch::neglect_turns`.
    /// Reset to [`NEGLECT_NONE`] on any turn the herd's upkeep requirement is met (its assigned
    /// herders reach `herders_needed`, i.e. `herded_fraction == FULLY_HERDED`) **and on any turn it is
    /// not a managed herd at all** — a wild herd is nobody's to neglect. Incremented on every other
    /// turn.
    ///
    /// **Damage the engaged animals are carrying from earlier turns of an ongoing hunt**
    /// (`docs/plan_hunt_through_combat.md` §4.2) — the cross-turn accumulator that makes the fight's
    /// gate **steep instead of absolute**: twenty hunters with weak spears wear a mammoth down over
    /// several turns rather than bouncing off it forever, and a party of 62 (one short of the
    /// stateless threshold) is not condemned to take casualties for nothing on every turn of the
    /// campaign.
    ///
    /// **A herd-level fact, not a party-level one** — the animal does not care who wounded it, so two
    /// bands working the same herd wear it down together. It heals in [`advance_herds`] on any turn
    /// nobody is in contact ([`crate::combat::DamageLedger::recover`]).
    ///
    /// **This is not `hunt_credit` coming back.** That bank was deleted because the escapement
    /// ceiling is a *stock* and accumulating a stock compounds it; damage is a **flow**, and an
    /// accumulator is the correct integral of a rate — see [`crate::combat::DamageLedger`], which
    /// carries the long form because it is the objection every reader will raise.
    ///
    /// Authoritative sim state — rewound by rollback with the cloned registry (sim-side only, not on
    /// the client wire), so a restored herd resumes the hunt exactly as wounded as it was.
    pub wounds: DamageLedger,
    /// Animals leave only while this **exceeds** the herd's current rung's
    /// [`RungDef::neglect_grace_turns`] — `animal:pastoral`'s for a tamed herd, `animal:pen`'s for a
    /// penned one, which is why the grace is per-rung: the fence holds a flock without a keeper for
    /// far longer than habit holds an unfenced one. The under-herded *notice* is deliberately **not**
    /// gated on it (see `advance_husbandry`): the grace is exactly when the player can still act.
    ///
    /// Rides the checkpoint with the rest of the registry, so a rollback rewinds a spent grace rather
    /// than handing the herd a fresh one.
    pub neglect_turns: u16,
}

impl Herd {
    // A constructor that mirrors the herd's identity + spawn-state fields (id/species/size/route/
    // biomass/K/fodder/regrowth/body_mass) — bundling them into a struct would just move the noise.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        species_display: String,
        size_class: SizeClass,
        route: Vec<UVec2>,
        biomass: f32,
        carrying_capacity: f32,
        fodder_per_biomass: f32,
        regrowth_rate: f32,
        body_mass: f32,
    ) -> Self {
        let label = format!("{} ({})", species_display, id);
        let current_pos = route.first().copied().unwrap_or_else(|| UVec2::new(0, 0));
        // Migratory groups start loitering at their spawn anchor (the caller samples the real
        // `turns_left` from the species' `loiter_turns`); game groups graze-wander their cluster.
        let roam = if size_class == SizeClass::Migratory {
            RoamState::Loiter { turns_left: 0 }
        } else {
            RoamState::GrazeWander
        };
        Self {
            id,
            label,
            species: species_display,
            size_class,
            route,
            step_index: 0,
            current_pos,
            dwell_remaining: 0,
            roam,
            next_pos: None,
            biomass,
            carrying_capacity,
            fodder_per_biomass,
            regrowth_rate,
            body_mass,
            // A fresh herd has earned no kill-credit yet (slice 8b).
            hunt_credit: 0.0,
            // No regrowth has run yet — pre-regrowth == current (slice 8b).
            biomass_before_regrowth: biomass,
            // Full ladder by default; the real spawn resolves the species' ceiling from its `SpeciesDef`
            // right after construction (`spawn_short_range_game` / the migratory spawn). A test-built
            // herd keeps the default `Pen` = the pre-2d-δ universal-full-ladder behaviour.
            husbandry_ceiling: HusbandryCeiling::default(),
            // Refreshed against the ecology config at spawn/each turn; Thriving until then.
            ecology_phase: EcologyPhase::Thriving,
            domestication_progress: 0.0,
            owner: None,
            corralled_at: None,
            corral_progress: 0.0,
            pen_radius: 0,
            pen_extend_progress: 0.0,
            pen_extending: false,
            footprint_intake: 0.0,
            pen_pasture_fraction: 0.0,
            fodder_draw: 0.0,
            pen_larder_bill: 0.0,
            pen_hay_food: 0.0,
            fodder_delivery_rate: 0.0,
            corralled_tended_this_turn: false,
            pen_fed_fraction: PEN_FULLY_FED,
            herded_fraction: FULLY_HERDED,
            pen_starving: false,
            tamed_this_turn: false,
            // A fresh herd is wild (no owner, no pen), so it needs no keepers; `stabilize_herders_needed`
            // seeds the real requirement the first turn it becomes managed. `0` = "not yet stabilized".
            herders_needed: 0,
            // A fresh herd is fully contained (nothing to hold yet).
            under_herded: false,
            // Nobody has laid a hand on it yet.
            wounds: DamageLedger::default(),
            neglect_turns: NEGLECT_NONE,
        }
    }

    /// Recompute `ecology_phase` from the current biomass against **the ecology this herd actually
    /// lives under** ([`herd_ecology`]) and **the capacity that actually bounds it**
    /// ([`herd_capacity`]) — never the raw wild ecology, or a penned herd would be classified against
    /// a curve it does not follow.
    pub fn refresh_ecology_phase(&mut self, fauna: &FaunaConfig) {
        self.ecology_phase = classify_ecology_phase(
            self.biomass,
            herd_capacity(self, fauna),
            &herd_ecology(self, fauna),
        );
    }

    /// A fully-tamed (managed livestock) group: yields provisions each turn and is
    /// immune to the overhunting collapse.
    pub fn is_domesticated(&self) -> bool {
        self.domestication_progress >= 1.0
    }

    /// **Can this herd be tamed** (Grazing 2d-δ)? Gated by the species' `husbandry_ceiling` — a `Wild`
    /// species is hunt-only, so domestication accrual and the `tame` verb no-op / reject.
    pub fn can_domesticate(&self) -> bool {
        self.husbandry_ceiling.allows_domestication()
    }

    /// **Can this herd be penned** (Grazing 2d-δ)? Only a `Pen`-ceiling species; the `corral` /
    /// `extend_pen` paths and the `Corral` policy accrual reject a `Wild` or `Pastoral` species.
    pub fn can_pen(&self) -> bool {
        self.husbandry_ceiling.allows_pen()
    }

    /// Accrue taming progress for `faction` (the band working this herd with
    /// [`crate::components::Improvement::Tame`] in flight).
    /// Sets ownership on the first accrual; only the owner makes progress. Clamped to 1.0
    /// (auto-domestication at [`crate::intensification::RUNG_COMPLETE`]). Mirrors
    /// `ForagePatch::accrue_cultivation`.
    ///
    /// **A `Wild`-ceiling species never accrues** (Grazing 2d-δ) — self-guarded here so the "hunt-only"
    /// invariant holds regardless of the call site (and no wild herd ever picks up an `owner`).
    ///
    /// **`pub` so tests can build a tamed herd** by running the *real* path to completion
    /// (`accrue_domestication(f, RUNG_COMPLETE)`). It replaces the retired `claim_domestication`,
    /// which snapped progress to `1.0` for the `domesticate` early-claim: with that command gone the
    /// primitive had no production caller, and a "skip the investment" method left lying in the API
    /// is precisely what the ladder exists to delete. Going through the accrual instead means a test
    /// fixture obeys the husbandry ceiling like everything else — you cannot fabricate a
    /// domesticated `wild` herd.
    ///
    /// **Returns `true` only when THIS call finished the rung**, matching [`Herd::accrue_corral`] and
    /// `ForagePatch::accrue_cultivation`: `handle_tame` sets the verb on every band hunting the herd,
    /// so a post-hoc `is_domesticated()` test would push one "Tamed the …" feed line per band.
    pub fn accrue_domestication(&mut self, faction: FactionId, amount: f32) -> bool {
        if self.is_domesticated() || !self.can_domesticate() {
            return false;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        self.domestication_progress = (self.domestication_progress + amount).min(1.0);
        self.is_domesticated()
    }

    // `decay_domestication` is DELETED (`docs/plan_fauna_neglect_escape.md` §2.1). Its only caller was
    // the retired `decay_under_herded` tameness-bleed; `domestication_progress` is now monotone-up
    // (earned via `Tame`, never lost to neglect), and ownership clears only when a managed herd sheds
    // to zero animals (`advance_husbandry`), not when progress reaches zero.

    /// A **corralled** (penned) herd: fixed at `corralled_at`, doesn't roam, and is paid its keeper
    /// place-local at the higher corral rate. The animal mirror of `ForagePatch::is_cultivated`
    /// gating the tended-patch behaviour.
    pub fn is_corralled(&self) -> bool {
        self.corralled_at.is_some()
    }

    /// Pen the herd at `tile` — called when `corral_progress` reaches `1.0` (the pen is finished).
    /// Fixes its position and grants a one-turn "tended" grace (`corralled_tended_this_turn = true`)
    /// so the first `advance_husbandry` pass after penning spares it — the keeper's Hunt assignment
    /// then re-marks it tended each Population stage to keep it penned.
    ///
    /// **Returns `false` and pens NOTHING if the species' `husbandry_ceiling` forbids a pen**
    /// (Grazing 2d-δ). The ceiling was enforced at the *commands* (`validate_labor_policy`,
    /// `handle_extend_pen`) and at the `Corral` accrual, but **not here — at the state mutation** —
    /// so this method would happily pen a **mammoth**: `wild`-ceiling, so `accrue_domestication`
    /// early-returns and it can never be tamed or owned, yet `corralled_at`/`corral_progress` were
    /// set unconditionally and the tend branch only checks `is_corralled()`. That produced a **wild,
    /// unowned, penned herd** — a state the real sim cannot reach, which a test fixture nonetheless
    /// stood on for real. This is the same hole slice 3a found when the `domesticate` early-claim let
    /// you claim a mammoth, and it is closed the same way: **`accrue_domestication` self-guards on
    /// `can_domesticate()`, so `corral_at` self-guards on `can_pen()`** — the invariant is structural
    /// and holds regardless of call site.
    ///
    /// **Loud in debug, honest in release**, the `hunt_expedition_ceiling` convention: every shipped
    /// path already gates on `can_pen()`, so reaching here with a non-`pen` species is a bug and a
    /// debug build screams; release refuses rather than fabricating the impossible state. It returns
    /// `bool` rather than no-op'ing silently — a caller left believing it penned something is worse
    /// than a loud failure.
    #[must_use = "a pen may be refused by the species' husbandry ceiling — do not assume it was built"]
    pub fn corral_at(&mut self, tile: UVec2) -> bool {
        if !self.can_pen() {
            debug_assert!(
                false,
                "{} cannot be penned (husbandry_ceiling = {}) — every corral path must gate on \
                 `can_pen()` before reaching `corral_at`",
                self.species,
                self.husbandry_ceiling.as_str()
            );
            return false;
        }
        self.corralled_at = Some(tile);
        self.current_pos = tile;
        self.next_pos = None;
        self.corral_progress = 1.0;
        self.corralled_tended_this_turn = true;
        true
    }

    /// Accrue pen-construction progress for `faction` (the keeper band, working the herd with
    /// [`crate::components::Improvement::Corral`] in flight); at `1.0` the pen is finished and the
    /// herd is penned at `tile`. Only
    /// the herd's owner builds (a domesticated herd always has one). Returns `true` on the turn the
    /// pen completes, so the caller can announce it. The animal mirror of
    /// `ForagePatch::accrue_cultivation` (which latches via `is_cultivated`); called **after** the
    /// turn's take so the pre-commit forecast can't lie about which yield this turn pays.
    pub(crate) fn accrue_corral(&mut self, faction: FactionId, amount: f32, tile: UVec2) -> bool {
        if self.is_corralled() || self.owner != Some(faction) {
            return false;
        }
        self.corral_progress = (self.corral_progress + amount).min(1.0);
        if self.corral_progress >= 1.0 {
            // The ceiling is already gated upstream (the `Corral` policy accrual + the commands), so
            // this can only refuse on a bug — and then the pen is genuinely not built, so say so.
            return self.corral_at(tile);
        }
        false
    }

    /// Begin an `ExtendPen` extension (Grazing 2d-β): enter the "extending" state with a fresh ring
    /// meter. Requires a **built pen with room to grow** (`is_corralled()` and `pen_radius <
    /// radius_max`) and **no extension already in flight** — returns `false` (a no-op) otherwise, so the
    /// command handler's validation and this guard can never disagree. The animal mirror of the `Corral`
    /// policy's under-construction state, but on an *already-penned* herd.
    pub fn begin_pen_extension(&mut self, radius_max: u32) -> bool {
        if !self.is_corralled() || self.pen_extending || self.pen_radius >= radius_max {
            return false;
        }
        self.pen_extending = true;
        self.pen_extend_progress = 0.0;
        true
    }

    /// Accrue one turn of pen-**extension** progress (2d-β), the twin of [`accrue_corral`] on an
    /// already-penned herd: while `pen_extending`, add `amount` to `pen_extend_progress`; at `1.0` the
    /// ring completes — `pen_radius += 1` (saturating at `radius_max`), the meter resets and the
    /// extending state clears. Returns `true` on the completion turn so the caller can announce it.
    /// Called **after** the turn's (dipped) take, mirroring `accrue_corral`.
    pub(crate) fn accrue_pen_extension(&mut self, amount: f32, radius_max: u32) -> bool {
        if !self.pen_extending {
            return false;
        }
        self.pen_extend_progress = (self.pen_extend_progress + amount).min(1.0);
        if self.pen_extend_progress >= 1.0 {
            self.pen_radius = (self.pen_radius + 1).min(radius_max);
            self.pen_extend_progress = 0.0;
            self.pen_extending = false;
            return true;
        }
        false
    }

    /// **Update the hysteresis-stabilized [`herders_needed`] for this herd** and return it — run once
    /// per turn for every herd in [`advance_husbandry`]. `band` is the deadband in **animals**
    /// (`animals_per_herder × husbandry.herders_hysteresis_fraction`).
    ///
    /// A **wild** herd isn't yours to maintain, so it stays `0` (the `herd_herders_needed` wild gate).
    /// A **managed** herd's requirement moves **asymmetrically**:
    /// - **up immediately** when the raw need rises — under-herding is harmful, respond at once;
    /// - **down only when the herd has clearly shrunk** — drop below `current` only once
    ///   `animals ≤ (current − 1) × animals_per_herder − band`, i.e. genuinely past the lower rung's
    ///   ceiling by more than the deadband, so a ±1-animal oscillation across a head-count multiple
    ///   can't churn the requirement;
    /// - a not-yet-stabilized managed herd (`herders_needed == 0`, e.g. the turn it was tamed) seeds
    ///   straight to the raw ceil so it is correct from its first stabilized turn.
    ///
    /// Never below `1` for a managed herd that still has animals (the raw `ceil` floor); an emptied
    /// managed herd reads `0`.
    pub fn stabilize_herders_needed(&mut self, animals_per_herder: f32, band: f32) -> u32 {
        if !(self.is_corralled() || self.owner.is_some()) {
            self.herders_needed = 0;
            return 0;
        }
        let raw = herders_needed(self.biomass, self.body_mass, animals_per_herder);
        let current = self.herders_needed;
        let animals = if self.body_mass > 0.0 {
            self.biomass / self.body_mass
        } else {
            0.0
        };
        let next = if current == 0 || raw > current {
            // First stabilized turn, or a rise: respond at once.
            raw
        } else if animals <= (current - 1) as f32 * animals_per_herder - band {
            // A genuine fall well below the lower rung's ceiling — step down to the raw need.
            raw
        } else {
            // Breathing across the boundary: hold.
            current
        };
        self.herders_needed = next;
        next
    }

    /// The **grazing range radius** (hex distance from `current_pos`) the herd eats each turn
    /// (Grazing Phase 2b-i). It is the footprint the herd already *occupies*, keyed off `size_class`:
    /// - **Small** game (a warren, `route_len == 1`) sits on its one tile → `R = 0`.
    /// - **Big** game roams a couple of tiles → `R = 1` (its tile + the 6 neighbours).
    /// - **Migratory** herds graze their whole current loiter cluster → `R = loiter_radius` (the same
    ///   radius their loiter-wander is confined to, so the range they eat is exactly the range they
    ///   roam — not the whole baked route, which they only pass through).
    ///
    /// Resolving from `size_class` (rather than adding a new lever) keeps the range tied to the
    /// existing footprint the design §4 identified as *already* the grazing range. `def` supplies the
    /// migratory `loiter_radius`; a `None` (unresolved species) falls back to the same default the
    /// loiter-wander uses.
    pub fn graze_range_radius(&self, def: Option<&SpeciesDef>) -> u32 {
        match self.size_class {
            SizeClass::Small => 0,
            SizeClass::Big => 1,
            SizeClass::Migratory => def
                .map(|d| d.loiter_radius)
                .unwrap_or(default_loiter_radius()),
        }
    }

    /// The herd's live tile — walked one hex per move by `advance_herds` (graze-wander /
    /// loiter-migrate), no longer a teleport to `route[step_index]`.
    pub fn position(&self) -> UVec2 {
        self.current_pos
    }

    pub fn route_length(&self) -> usize {
        self.route.len()
    }

    /// The herd's next intended hex — the client heading arrow. `Some` only during a `Migrate` leg
    /// (one hex toward the target anchor); `None` while loitering/grazing (no misleading arrow).
    pub fn next_position(&self) -> Option<UVec2> {
        self.next_pos
    }
}

/// A fully-fed pen (`paid == demand`, or nothing demanded). The neutral value of
/// `Herd::pen_fed_fraction`, so an un-penned or newly-spawned herd never starves.
pub(crate) const PEN_FULLY_FED: f32 = 1.0;

/// A pen with **no keeper at all** — unfed (`docs/plan_fauna_neglect_escape.md` §2.4). `advance_husbandry`
/// stamps this on an abandoned pen so next turn's `regrow_biomass` scales its growth to zero: without it
/// a fast breeder's regrowth (`r` up to 1.0) cancels the ~10%/turn shed and the pen leaks strays forever
/// instead of shedding to zero and losing the pen. It is the "nobody is bringing food" reading, distinct
/// from a *keeper who cannot pay* (which flows through `starve_underfed_pen`, unchanged).
pub(crate) const PEN_NOT_FED: f32 = 0.0;

/// **THE ecology a herd actually lives under** — the one place the husbandry ladder's
/// rung → growth-rate mapping lives (`docs/plan_corral_managed_population.md` §3). Management buys a
/// *growth rate*, and nothing else:
///
/// - **wild** (`fauna.ecology`, `r` = 0.05) — hunted, predated, winter-killed;
/// - **pastoral** (`husbandry.pastoral.ecology`, `r` = 0.25) — tamed but still roaming;
/// - **pen** (`husbandry.pen.ecology`, `r` = 0.90) — corralled: sheltered, guarded, and **fed**.
///
/// Every consumer of a herd's ecology — regrowth, the MSY/policy ceilings, the phase classification,
/// the forecast, the expedition — resolves it *here*. **No call site may re-derive it**: a second copy
/// of this mapping is exactly how a forecast starts promising a number the take won't pay.
/// Returns an **owned** `EcologyConfig` (cheap — five `f32`s, `Copy`) rather than a borrow, because a
/// **wild** herd's curve now runs at the herd's own **per-species `regrowth_rate`** (Grazing Phase
/// 2b-ii): the wild ecology with only its `regrowth_rate` swapped for `herd.regrowth_rate`, leaving the
/// shared phase bands (`collapse_fraction`/`stressed_fraction`/`extinction_floor`) intact. The
/// pastoral/pen rungs keep their own faster `r` verbatim. This stays THE single seam — every consumer
/// (regrowth, MSY/policy ceilings, phase classification, forecast, expedition) reads the folded rate
/// here and nowhere re-derives it, so a wild rabbit and a wild mammoth breed at different rates without
/// a second copy of the mapping.
pub fn herd_ecology(herd: &Herd, fauna: &FaunaConfig) -> EcologyConfig {
    if herd.is_corralled() {
        pen_ecology_for(herd, fauna)
    } else if herd.is_domesticated() {
        pastoral_ecology_for(herd, fauna)
    } else {
        EcologyConfig {
            regrowth_rate: herd.regrowth_rate,
            ..fauna.ecology
        }
    }
}

/// The **pastoral** ecology a herd would live under: its per-species managed rate
/// (`min(husbandry_regrowth_cap, wild_r × pastoral_gain)`, Grazing 2d §3) folded into the pastoral
/// rung's shared phase bands. Retires the flat `pastoral.ecology.regrowth_rate`.
fn pastoral_ecology_for(herd: &Herd, fauna: &FaunaConfig) -> EcologyConfig {
    EcologyConfig {
        regrowth_rate: managed_regrowth_rate(
            herd.regrowth_rate,
            fauna.husbandry.pastoral_gain,
            fauna,
        ),
        ..fauna.husbandry.pastoral.ecology
    }
}

/// The **pen** ecology a herd would live under *if penned* — its per-species managed rate
/// (`min(husbandry_regrowth_cap, wild_r × pen_gain)`) folded into the pen rung's phase bands. Shared by
/// [`herd_ecology`] (a live penned herd) **and** [`pen_yield_biomass`] (the forecast's "what would this
/// pay once penned?" projection for a herd that is not penned yet), so the two never disagree.
fn pen_ecology_for(herd: &Herd, fauna: &FaunaConfig) -> EcologyConfig {
    EcologyConfig {
        regrowth_rate: managed_regrowth_rate(herd.regrowth_rate, fauna.husbandry.pen_gain, fauna),
        ..fauna.husbandry.pen.ecology
    }
}

/// A managed rung's per-species growth rate (Grazing 2d §3): the herd's own wild `r` scaled by the
/// rung's `gain`, clamped to the stable-band cap so a fast breeder cannot be pushed into an
/// oscillating discrete-logistic rate. The one place the `wild_r × gain → capped r` mapping lives.
fn managed_regrowth_rate(wild_r: f32, gain: f32, fauna: &FaunaConfig) -> f32 {
    (wild_r * gain).min(fauna.husbandry.husbandry_regrowth_cap)
}

/// **THE capacity that actually bounds a herd** — its cached `carrying_capacity`. For a **mobile** herd
/// that is the range's ecological `K` (Grazing 2b-ii); for a **penned** herd it is the fenced
/// footprint's `K` (Grazing 2d — `capacity_fraction` is retired, a penned herd is no longer scaled off
/// the range). The twin of [`herd_ecology`] — same rule: no call site re-derives it.
pub fn herd_capacity(herd: &Herd, _fauna: &FaunaConfig) -> f32 {
    herd.carrying_capacity
}

/// **The per-species density (K) multiplier for a herd's CURRENT husbandry rung** — domestication makes
/// the land hold *more* animals, non-linearly by species (the density ladder, orthogonal to the r-gains
/// `herd_ecology` folds in). A **corralled** herd multiplies its footprint `K` by the species'
/// [`SpeciesDef::pen_density`], a **mobile-tamed** herd by its [`SpeciesDef::pastoral_density`], and a
/// **wild** herd by [`DEFAULT_HUSBANDRY_DENSITY`] (`1.0`, so its `K` is byte-identical). Mirrors
/// `herd_ecology`'s rung dispatch exactly.
///
/// Resolved **live** by display name (`pen_density_for` / `pastoral_density_for`, the `taming_rate_for`
/// path), never cached on the `Herd`, so a config retune reaches herds already on the map. Applied at
/// the single K seam [`ecological_carrying_capacity`] (the one place `herd.carrying_capacity` is
/// written), covering both the graze-derived and the fallback constant K.
pub fn herd_density_gain(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    if herd.is_corralled() {
        fauna.pen_density_for(&herd.species)
    } else if herd.is_domesticated() {
        fauna.pastoral_density_for(&herd.species)
    } else {
        DEFAULT_HUSBANDRY_DENSITY
    }
}

/// **The feed a pen demands — or WOULD demand once built** — at the herd's current biomass:
/// `upkeep_per_biomass × biomass`, drawn from the keeper band's larder. A penned herd cannot graze;
/// this is the physical price of the thing that makes a pen a pen, and the tether that gives "the pen
/// pins the band" its teeth.
///
/// **Answered for EVERY herd, penned or not** — a *projection* for an unpenned one, the *live* demand
/// for a penned one — on the **same biomass basis** [`corral_yield`] (`hunt_forecast`'s
/// `managed_yield`) already uses to answer "what would this pay once penned?". The two are a **matched
/// pair the client subtracts**: quoting the payoff while hiding the running cost, at the one moment the
/// running cost should drive the decision (the pre-commit `Corral` row, on a herd that is by definition
/// *not yet penned*), is the same defect as advertising the gross yield — a preview quoting a number
/// the player will never bank.
///
/// **Demanded, not paid.** A starving pen demands more than it is paid; `Herd::pen_fed_fraction` is
/// that ratio, and the band's *actual* ledger debit is the per-band
/// `PopulationCohortState::pen_feed_upkeep` (the real `LocalStore::take` amount) — which does **not**
/// read this. So no consumer needs a "0 when unpenned" reading, and one field with one meaning beats
/// two that must be kept in lockstep.
pub fn pen_upkeep(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    (fauna.husbandry.pen.upkeep_per_biomass * herd.biomass).max(0.0)
}

/// **THE managed (husbanded) harvest**, in biomass — the one helper both husbandry rungs take their
/// yield from (`advance_husbandry`'s pastoral even-split and the corral-tend branch of
/// `advance_labor_allocation`), so the pen and the pastoral herd can never disagree about what a
/// managed harvest *is*.
///
/// It is the **maximum sustainable yield, taken as constant *escapement***: harvest the biomass
/// standing above the MSY point (`K/2`).
///
/// ```text
/// take = max(0, B − K/2)
/// ```
///
/// **This is now literally [`hunt_escapement_ceiling`]`(Sustain, …)` — ONE RULE FOR EVERY TAKE IN
/// THE GAME**, since `docs/plan_harvest_floor.md` slice 1 gave both food webs the pen's shape. A managed harvest and a wild Sustain hunt are the same act against
/// different curves, which is what the husbandry ladder always claimed and now actually is.
///
/// ## The `min(peak_regrowth(K), …)` cap was REMOVED in slice 8 — do not restore it
///
/// It used to read `min(peak_regrowth(K), max(0, B − K/2))`, capping the take at one turn's peak
/// regrowth (`r·K/4`) so that the yield at capacity equalled the yield at the operating point. That
/// cap **made whole-animal harvesting impossible**, and not subtly:
///
/// A quantised take can only pulse if the herd's spare biomass **accumulates** across the turns it
/// waits. The cap bounds the ceiling by the constant `r·K/4` *however long the herd is left alone* —
/// so `floor(ceiling / body_mass)` is `0` on turn 1 and still `0` on turn 500 whenever
/// `r·K/4 < body_mass`. The herd grows to `K` and sits there while the keeper pays feed **forever and
/// collects nothing**. Measured at a radius-0 pen (the default) on the best pasture in the game: the
/// **boar** (`peak_regrowth` 30 vs body 50) and the **aurochs** (32.4 vs 80) — *the two species the
/// grazing-2d arc added for penning* — yielded zero permanently. That is not a pulse, it is a silent
/// trap on a 25-turn investment.
///
/// Removing it costs nothing at equilibrium: at the settled operating point `B* = K/2` the spare
/// biomass **is** one turn's regrowth, so the cap was already inactive and the pen's net-positive
/// bound (`upkeep < r_pen × provisions / (2 + r_pen)`, `FaunaConfig::validate`) is untouched. All the
/// cap ever suppressed was the *fresh-pen burst* — which is exactly the behaviour the wild rungs
/// have, and honest: a pen holding twice what its footprint sustains should hand over the surplus.
///
/// **Why escapement, and not constant catch** (the `sustainable_yield` a wild `Sustain` hunt *used*
/// to take — before slice 8 made every rung escapement for this very reason).
/// The sim regrows in Logistics and harvests in Population, so a constant-catch MSY take is evaluated
/// at the *post*-regrowth biomass. Above `K/2` that is harmless (the take is capped at MSY either
/// way, and both converge on `K/2` paying `r·K/4`). **Below `K/2` it takes `g(B + g(B))`, which is
/// strictly more than the `g(B)` the herd actually grew** — so the herd bleeds a little every turn and
/// the `K/2` equilibrium is stable only from *above*. At the wild `r` = 0.05 that leak is a rounding
/// error; at the pen's `r` = 0.60 it is fatal — a **fully fed** pen knocked below `K/2` (by a famine,
/// or by a band hunting it) spirals to zero in ~12 turns and can never recover. Escapement removes the
/// leak by construction: it never takes a herd below `K/2`, so a depleted managed herd **rebuilds**
/// (yielding less, or nothing, while it does) and then pays `r·K/4` forever. Identical yield at
/// capacity and at the operating point; the difference is only that this one is stable from *both*
/// sides — which is exactly why real fisheries use escapement and not constant catch.
///
/// A managed harvest therefore **never overdraws** (`actual == sustainable`, no ⚠), and a starved pen's
/// yield falls with its herd instead of finishing it off.
///
/// ## The pen is quantised too — and reads steady *emergently*, not by stipulation
///
/// Slice 8's [`quantise_animal_take`] applies here exactly as it does to a wild hunt: **you cannot
/// slaughter half a cow any more than you can half-kill a mammoth.** This helper is unchanged by that
/// — it still hands back the *biomass the pen can spare*, and the corral-tend branch rounds it to
/// animals.
///
/// **The pen nonetheless has no wait turns, and that is a consequence of its ecology rather than a
/// rule.** A pen runs at `r_pen = min(husbandry_regrowth_cap, wild_r × pen_gain)` — up to **3× the
/// wild rate** — so its MSY clears one body's worth of meat *every turn* for every pennable species
/// (measured in `grazing_2d_pen::the_pen_slaughters_whole_animals_every_turn`). A herd that breeds
/// fast enough to slaughter from continuously simply never has to wait. **That is the real-world
/// reason a pen reads steady where a hunt pulses**, and it is why the `peak_regrowth` cap above needs
/// no slice-8 change: `floor(peak_regrowth / body_mass) >= 1` is all quantisation asks of it.
///
/// So rung 3's payoffs are the ones it actually earns — a faster `r`, no chasing (the herd is at your
/// fence), a self-feeding footprint, and a `K` you control — **not** an exemption from butchery.
/// A pen on poor enough range *will* pulse (the aurochs is the closest: pen MSY ≈ `0.0675 × K` against
/// body mass 80, so it waits below `K ≈ 1185`), and that is honest rather than a bug.
///
/// Takes the raw `(biomass, capacity, ecology)` rather than a `&Herd` because the forecast must also
/// answer it for a herd that is **not penned yet** ("what will this pay once the pen is built?").
pub(crate) fn managed_yield_biomass(biomass: f32, capacity: f32, _ecology: &EcologyConfig) -> f32 {
    // The pen harvests on **Sustain's floor** — the same `MSY_BIOMASS_FRACTION` the hunt uses, so the
    // keeper and the hunter can never disagree about what "leave the productive stock standing" means.
    (biomass - capacity * MSY_BIOMASS_FRACTION).max(0.0)
}

/// **The herders a managed herd demands, every turn** (intensification ladder slice 8):
/// `ceil((biomass / body_mass) / animals_per_herder)`, at least 1 for any herd that still has an
/// animal in it.
///
/// *Just because you aren't killing an animal doesn't mean you aren't tending them, making sure they
/// don't run off, repairing fences.* Before this, a pen of 2 and a pen of 200 needed the same single
/// keeper — only the **feed** scaled with the herd.
///
/// # ONE need, not two — the herders mind the herd AND slaughter it
///
/// A managed rung has **no separate harvest-side worker need**. The crew standing in the pen is the
/// crew that butchers, so `workers_needed` on a pastoral/penned source *is* this number, and the
/// standing cost is owed on **wait turns** too (a herd that cannot spare an animal still has to be
/// fed, watched and fenced). Contrast [`quantise_animal_take`]'s `carryable`, which is a *hunt*
/// concept.
///
/// # Wild hunting has NO maintenance, and that asymmetry is deliberate — do not "unify" them
///
/// A wild herd isn't yours: you don't mind it, you *find* it. So a wild hunt pays no standing cost —
/// and keeps its **carry cap** (`workers × per_worker_biomass_capacity`), because you must haul the
/// kill home across the range. A managed herd is the mirror: you pay every turn to keep it, and the
/// meat is already standing where your people are. **The models differ because the products differ:**
///
/// ```text
/// hunt    = reach + carry
/// harvest = maintain + take
/// ```
///
/// Collapsing them would either charge a hunter rent on an animal they do not own, or hand a rancher
/// their herd for free.
///
/// # Herding is HEADS, not tonnes
///
/// The denominator is per-**animal** ([`SpeciesDef::animals_per_herder`]), never per-biomass: a
/// shepherd minds ~300 sheep and a cowherd ~80 cattle, because you watch individuals and a heavier
/// beast is not proportionally more work. See that field for the unit error this replaced.
///
/// A herd with no `body_mass` (impossible — `validate()` requires it positive) or no animals left
/// needs nobody; the `max(1)` floor otherwise means *some* crew is always on the books, so a herd can
/// never be fully staffed by zero people.
pub fn herders_needed(biomass: f32, body_mass: f32, animals_per_herder: f32) -> u32 {
    // NaN-safe by construction: every guard is a positive test, so a NaN input falls through to `0`
    // (nobody needed) rather than sneaking past a negated comparison.
    let sane = biomass > 0.0 && body_mass > 0.0 && animals_per_herder > 0.0;
    if !sane {
        return 0;
    }
    let animals = biomass / body_mass;
    ((animals / animals_per_herder).ceil() as u32).max(1)
}

/// [`herders_needed`] for a herd, resolving its species' `animals_per_herder` live off the config (the
/// `taming_rate_for` path — a retune reaches herds already on the map). `0` for a herd that is not on
/// a managed rung: a **wild** herd has no keepers, by design (see [`herders_needed`]).
///
/// # "Managed" is `is_corralled() || owner.is_some()` — a herd you have STARTED to tame, not only a
/// finished one
///
/// **Herders set how large a tame flock you can HOLD; `Tame` is what earns the tameness.** Scoping the
/// requirement to `is_domesticated()` (progress *exactly* `1.0`) instead looks right and is a trap:
/// the flag is a **threshold**, so a herd still mid-taming (an `owner`, but progress `< 1.0`) would owe
/// no keepers and never shed — yet it is just as much your herd to hold. A managed herd whose herders
/// cannot hold all its animals **sheds the overage to the wild web** (`shed_uncontained_animals`); it
/// does **not** bleed tameness — tameness is permanent, it leaves only *with the animals that leave*
/// (neglect sheds animals, not the meter). Full abandonment sheds the whole flock and the empty herd
/// despawns. Pinned by `fauna_husbandry::neglect_never_un_tames_a_herd` and
/// `the_shed_is_bounded_by_the_true_overage_near_a_ceil_boundary`.
///
/// `owner.is_some()` is exactly "somebody's herd" — a **wild** herd (no owner, no pen) reads `0` and is
/// untouched. `corral_at` does **not** require domestication (it gates on `can_pen()` only), so the
/// `is_corralled()` half keeps a penned-but-untamed fixture staffed.
pub fn herd_herders_needed(herd: &Herd, fauna: &FaunaConfig) -> u32 {
    if !(herd.is_corralled() || herd.owner.is_some()) {
        return 0;
    }
    // **The hysteresis-stabilized requirement is the source of truth** (`Herd::herders_needed`,
    // seeded every turn by `stabilize_herders_needed` in `advance_husbandry`). A `> 0` value is a
    // real, deadband-stabilized count — return it so the requirement doesn't flicker ±1 as the herd
    // breathes across an `animals_per_herder` multiple.
    if herd.herders_needed > 0 {
        return herd.herders_needed;
    }
    // `0` = not yet stabilized: a herd the turn it becomes managed (before the next `advance_husbandry`
    // seeds it) or a test-built managed herd. Fall back to the raw ceil so it is never wrong for a turn.
    herders_needed(
        herd.biomass,
        herd.body_mass,
        fauna.animals_per_herder_for(&herd.species),
    )
}

/// **The crew this herd WOULD owe if it were managed** — ownership-INDEPENDENT (fauna neglect-escape,
/// the taming-startup-lag fix). Identical to [`herd_herders_needed`] **except the gate**: it returns `0`
/// only for a species that can never be tamed (`!can_domesticate()` — a `Wild` husbandry ceiling), where
/// `herd_herders_needed` returns `0` for any herd not *yet* owned/corralled.
///
/// On the turn a **Tame/Corral** (investment) assignment starts, ownership has not been set yet
/// (`accrue_domestication` runs later, in Population), so the ownership-gated count reads `0` and the crew
/// collapses to the take-side hauler count — the herd reads "1 of N working" on a full crew for a turn. An
/// investment assignment *means* the herd is being managed, so its herder requirement is the
/// biomass-derived crew regardless of whether ownership is recorded yet. Call this for an investment
/// policy, [`herd_herders_needed`] for an extractive one (a wild Sustain-hunted herd must stay
/// ownership-gated to `0`, or its `herded_fraction` would drop below `1` and it would falsely read
/// under-herded and shed). Both the labor arm and the `herdersNeededIfManaged` wire field resolve through
/// this — one definition.
///
/// **Prefers the hysteresis-stabilized `Herd::herders_needed`** exactly as `herd_herders_needed` does, so
/// an *already-managed* herd (owner set — e.g. a corralled herd under `Corral`) returns the stabilized
/// count and does not re-flicker ±1; only a not-yet-owned tameable herd (field `0`) falls back to the raw
/// ceil. So for every managed herd this equals `herd_herders_needed` — they diverge only where the
/// ownership gate does.
pub fn would_be_herders_needed(herd: &Herd, fauna: &FaunaConfig) -> u32 {
    if !herd.can_domesticate() {
        return 0;
    }
    if herd.herders_needed > 0 {
        return herd.herders_needed;
    }
    herders_needed(
        herd.biomass,
        herd.body_mass,
        fauna.animals_per_herder_for(&herd.species),
    )
}

/// **How well a managed herd is staffed this turn** — `min(1, assigned / needed)`, the herding twin of
/// `Herd::pen_fed_fraction`.
///
/// **Understaffing degrades PROPORTIONALLY; it never triggers an escape.** A binary threshold would
/// destroy a 25-turn investment on *rounding*, as a herd's biomass breathes across a herder boundary
/// — that is an accident, not a decision. So this scales the damage exactly the way an underfed pen's
/// `pen_fed_fraction` scales `pen.starve_shrink_rate`: half the herders you need, half the tending,
/// and the rung's meter bleeds proportionally — floored, and recoverable the moment you staff it
/// again. **Binary escape survives for total abandonment only** (zero herders — nobody is minding the
/// gate), which is the one case where "it broke out" is the honest model.
///
/// A herd that needs nobody (wild, or empty) is trivially fully herded.
pub fn herded_fraction(assigned: u32, needed: u32) -> f32 {
    if needed == 0 {
        return FULLY_HERDED;
    }
    (assigned as f32 / needed as f32).clamp(0.0, FULLY_HERDED)
}

/// A fully-staffed managed herd — the neutral value of [`herded_fraction`], and what a herd with no
/// herder demand reads. Mirrors [`PEN_FULLY_FED`].
pub const FULLY_HERDED: f32 = 1.0;

/// **A managed herd nobody worked** — what `advance_husbandry` resets a domesticated herd's
/// `herded_fraction` to each turn, so a herd whose keeper never showed up reads "unherded" rather than
/// inheriting last turn's staffing. The *wild* rungs reset to [`FULLY_HERDED`] instead: they demand no
/// herders, so "unstaffed" would be a lie that decays them for free.
pub const NOT_HERDED: f32 = 0.0;

/// The **gross managed harvest a PEN yields**, in biomass: [`managed_yield_biomass`] against the herd's
/// per-species pen ecology ([`pen_ecology_for`]) and the pen's capacity (the herd's
/// `carrying_capacity`, which for a penned herd is its fenced footprint's `K` — Grazing 2d). Takes the
/// `&Herd` (not raw scalars) because the per-species pen `r` needs the herd's own wild rate. This is
/// the pen's **actual** constant-escapement take, so it drives the corral-tend payout
/// (`systems::labor`) and the penned-herd early-return of [`hunt_forecast`] (forecast == actual). The
/// forecast's *un-penned* "what would this pay once penned?" projection is instead the pen's sustained
/// MSY (`sustainable_yield` at the pen `r`) — the long-run rate that shows the ladder, not this
/// one-turn escapement.
pub(crate) fn pen_yield_biomass(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    managed_yield_biomass(
        herd.biomass,
        herd.carrying_capacity,
        &pen_ecology_for(herd, fauna),
    )
}

#[derive(Debug, Clone, Default)]
pub struct HerdTelemetryEntry {
    pub id: String,
    pub label: String,
    pub species: String,
    pub size_class: String,
    pub huntable: bool,
    /// Ecological health band string (see `EcologyPhase::as_str`).
    pub ecology_phase: String,
    /// Husbandry progress in `[0.0, 1.0]` (`1.0` = domesticated).
    pub domestication: f32,
    /// Rung 1c corral state: `true` iff the herd is penned (`Herd::is_corralled`). Client shows a
    /// place-bound corral indicator distinct from a mobile domesticated herd.
    pub corralled: bool,
    /// Pen-construction progress in `[0.0, 1.0]` (`Herd::corral_progress`) — the client's "pen
    /// building N%" meter while a keeper works the herd with the `Corral` improvement in flight.
    pub corral_progress: f32,
    pub position: UVec2,
    pub biomass: f32,
    pub route_length: u32,
    pub next_position: Option<UVec2>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdRegistry {
    pub herds: Vec<Herd>,
}

impl HerdRegistry {
    pub fn clear(&mut self) {
        self.herds.clear();
    }

    pub fn find(&self, id: &str) -> Option<&Herd> {
        self.herds.iter().find(|herd| herd.id == id)
    }

    pub fn entries(&self) -> &[Herd] {
        &self.herds
    }

    pub fn snapshot_entries(&self) -> Vec<HerdTelemetryEntry> {
        self.herds.iter().map(to_entry).collect()
    }

    /// Number of domesticated groups owned by `faction`. The seam the future
    /// `SedentarizationScore` reads for its "domestication progress" input.
    pub fn domesticated_count(&self, faction: FactionId) -> usize {
        self.herds
            .iter()
            .filter(|herd| herd.is_domesticated() && herd.owner == Some(faction))
            .count()
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdTelemetry {
    pub entries: Vec<HerdTelemetryEntry>,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct HerdDensityMap {
    pub width: u32,
    pub height: u32,
    samples: Vec<f32>,
}

impl HerdDensityMap {
    pub fn rebuild(&mut self, grid_size: UVec2, registry: &HerdRegistry) {
        let samples: Vec<(UVec2, f32)> = registry
            .herds
            .iter()
            .map(|herd| (herd.position(), herd.biomass))
            .collect();
        self.rebuild_from_samples(grid_size, &samples);
    }

    pub fn rebuild_from_samples(&mut self, grid_size: UVec2, herds: &[(UVec2, f32)]) {
        let width = grid_size.x.max(1);
        let height = grid_size.y.max(1);
        let total = width.saturating_mul(height).max(1);
        if self.width != width || self.height != height || self.samples.len() != total as usize {
            self.width = width;
            self.height = height;
            self.samples = vec![0.0; total as usize];
        } else {
            self.samples.fill(0.0);
        }

        for (pos, biomass) in herds {
            if pos.x >= self.width || pos.y >= self.height {
                continue;
            }
            let idx = (pos.y as usize) * self.width as usize + pos.x as usize;
            self.samples[idx] += *biomass;
        }
    }

    pub fn density_at(&self, pos: UVec2) -> f32 {
        if self.samples.is_empty() || pos.x >= self.width || pos.y >= self.height {
            return 0.0;
        }
        let idx = (pos.y as usize) * self.width as usize + pos.x as usize;
        self.samples.get(idx).copied().unwrap_or(0.0)
    }

    pub fn normalized_density_at(&self, pos: UVec2) -> f32 {
        normalize_density(self.density_at(pos))
    }

    pub fn normalized_pair_average(&self, a: UVec2, b: UVec2) -> f32 {
        let avg = 0.5 * (self.density_at(a) + self.density_at(b));
        normalize_density(avg)
    }

    pub fn normalized_average(&self) -> f32 {
        normalize_density(self.average_density())
    }

    pub fn average_density(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let total: f32 = self.samples.iter().copied().sum();
        total / (self.samples.len() as f32)
    }

    pub fn max_density(&self) -> f32 {
        self.samples
            .iter()
            .copied()
            .fold(0.0f32, |acc, value| acc.max(value))
    }
}

fn normalize_density(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else {
        (value / HERD_DENSITY_REFERENCE_BIOMASS).clamp(0.0, 1.0)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_initial_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    start_location: Res<StartLocation>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    world_seed: Option<Res<WorldGenSeed>>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    if !registry.herds.is_empty() {
        telemetry.entries = registry.herds.iter().map(to_entry).collect();
        density.rebuild(config.grid_size, &registry);
        return;
    }

    let fauna = fauna_config.get();
    let seed = world_seed
        .map(|seed| seed.0)
        .unwrap_or_else(|| config.map_seed);
    let mut rng = if seed == 0 {
        SmallRng::from_entropy()
    } else {
        SmallRng::seed_from_u64(seed ^ 0xFA1A_FEED)
    };

    let width = config.grid_size.x.max(4);
    let height = config.grid_size.y.max(4);
    let wrap = config.map_topology.wrap_horizontal;
    let base = start_location
        .position()
        .unwrap_or(UVec2::new(width / 2, height / 2));

    let mut herds = Vec::new();
    // 1. Long-range migratory herds — anchored on host-biome-suitable tiles across the map, with
    //    each species drawn from the config's migratory rows.
    spawn_migratory_herds(
        &fauna,
        base,
        width,
        height,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
        wrap,
    );
    // 2. Short-range wild game — biome-density placement across the whole map.
    spawn_short_range_game(
        &fauna,
        width,
        height,
        wrap,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );
    // 3. Predator packs — a dedicated, capped, well-spaced pass (Predators Phase 1a) so predators are
    //    rare and do NOT consume the `max_total_game` prey budget. Runs AFTER the herbivore pool so it
    //    does not perturb its RNG draws (a carnivore-free config seats nothing here).
    spawn_predators(
        &fauna,
        width,
        height,
        wrap,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );

    registry.herds = herds;
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

fn log_herd_spawn(herd: &Herd) {
    let position = herd.position();
    info!(
        target: "shadow_scale::analytics",
        event = "herd_spawn",
        herd = %herd.id,
        label = %herd.label,
        species = %herd.species,
        x = position.x,
        y = position.y,
        biomass = herd.biomass,
        route_length = herd.route_length(),
    );
}

/// Long-range migratory herds: a handful of cross-region walkers, as many as
/// `abundance.migratory` budgets for the map size, species drawn from the config's migratory rows.
///
/// **`host_biomes` is LIVE for migratory species** (it was previously ignored): a herd's loiter
/// anchors sit on tiles suitable for its species (`module_at ∈ host_biomes`), drawn from a regional
/// home range, and the migration legs cross whatever less-suitable ground lies between — so a herd
/// lives in its biome range across the map rather than clustered at the player start. A species whose
/// host biomes the map lacks falls back to the start-anchored spiral (`build_migratory_route`), so it
/// still spawns somewhere.
#[allow(clippy::too_many_arguments)]
fn spawn_migratory_herds(
    fauna: &FaunaConfig,
    base: UVec2,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
    wrap: bool,
) {
    let migratory = fauna.migratory_species();
    if migratory.is_empty() {
        return;
    }
    // Bucket every land tile by its host food module once (deterministic (y, x) scan order → ordered
    // Vecs), so each herd's per-species suitable-tile slice is a cheap concat instead of an O(w·h)
    // rescan. A tile has exactly one module, so buckets are disjoint (no cross-bucket duplicates).
    let mut suitable_by_module: BTreeMap<FoodModule, Vec<UVec2>> = BTreeMap::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            if let Some(module) = module_at(pos, tile_registry, tiles) {
                suitable_by_module.entry(module).or_default().push(pos);
            }
        }
    }
    let herd_target = fauna.abundance.migratory.herds_for_map(width, height);
    // `idx` is the *seated* herd count, not the attempt count: a `build_migratory_route` failure used
    // to `continue` the `0..herd_target` loop, which consumed the slot and left the map one migratory
    // herd short (measured: 1 map in 120). The budget names how many herds a map should HOLD, so a
    // failure re-draws instead — bounded by `MIGRATORY_ROUTE_ATTEMPTS_PER_HERD` so a map that can seat
    // none (no land, or every migratory row's route unbuildable) still terminates rather than spinning.
    let mut idx = 0u32;
    let mut attempts_left = herd_target.saturating_mul(MIGRATORY_ROUTE_ATTEMPTS_PER_HERD);
    while idx < herd_target && attempts_left > 0 {
        attempts_left -= 1;
        let (key, def) = migratory[rng.gen_range(0..migratory.len())];
        let steps = def.sample_route_len(rng);
        let suitable = suitable_tiles_for(def, &suitable_by_module);
        let Some(route) = build_migratory_route(
            base,
            width,
            height,
            tile_registry,
            tiles,
            &fauna.graze,
            &suitable,
            rng,
            steps,
            wrap,
        ) else {
            continue;
        };
        let biomass = def.sample_biomass(rng);
        let carrying_capacity = def.carrying_capacity();
        let id = format!("herd_{key}_{idx:02}");
        let mut herd = Herd::new(
            id,
            def.display_name.clone(),
            def.size_class,
            route,
            biomass,
            carrying_capacity,
            def.fodder_per_biomass,
            def.regrowth_rate_or(fauna.ecology.regrowth_rate),
            def.body_mass,
        );
        // Start loitering at the spawn anchor for a randomized window (rather than migrating off
        // immediately from `Loiter { turns_left: 0 }`).
        herd.roam = RoamState::Loiter {
            turns_left: def.sample_loiter_turns(rng),
        };
        // Cache the species' husbandry ceiling (Grazing 2d-δ) so the gates read a herd field.
        herd.husbandry_ceiling = def.husbandry_ceiling;
        herd.refresh_ecology_phase(fauna);
        log_herd_spawn(&herd);
        herds.push(herd);
        idx += 1;
    }
    if idx < herd_target {
        // UNDER-FILL AND REPORT, never relax the budget — the same contract the tag solver's
        // `mapgen.tag_solver.under_filled_climate_gated` states. Every migratory row's route was
        // unbuildable often enough to exhaust the retries, which means the map genuinely cannot seat
        // the budget (no land, or no host biome anywhere). Silence here is what made the retired
        // slot-eating `continue` cost a herd per ~120 maps with nothing to read.
        info!(
            target: "shadow_scale::fauna",
            shortfall = herd_target - idx,
            target = herd_target,
            seated = idx,
            "fauna.migratory.under_filled"
        );
    }
}

/// How many `build_migratory_route` attempts each migratory slot is allowed before the slot is given
/// up (`spawn_migratory_herds`). A route failure re-draws rather than eating the slot, so this is the
/// loop's termination bound: on a map where no migratory row can build a route (no land at all) the
/// pass must still finish. Small — a failure is rare (1 map in 120 measured), so the retries are
/// nearly free, and a slot that fails this many times in a row is genuinely unseatable.
const MIGRATORY_ROUTE_ATTEMPTS_PER_HERD: u32 = 8;

/// Short-range wild game (big + small): iterate land tiles, roll the per-biome
/// abundance, then greedily place bounded, spaced-out groups from a shuffled pool
/// so placement is spread across the map rather than clustered by scan order.
#[allow(clippy::too_many_arguments)] // Bevy resources + grid bounds + topology; a struct would only move the noise
fn spawn_short_range_game(
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
) {
    // Collect every tile where the abundance roll succeeds (map-wide).
    let mut winners: Vec<(UVec2, &'static str)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            let Some(module) = module_at(pos, tile_registry, tiles) else {
                continue;
            };
            let module_key = module.as_str();
            let prob = fauna.abundance.probability_for(module_key);
            if prob <= 0.0 {
                continue;
            }
            if rng.gen::<f32>() < prob {
                winners.push((pos, module_key));
            }
        }
    }
    // Shuffle so the cap + spacing thin the pool uniformly, not top-to-bottom.
    winners.shuffle(rng);

    let max_total = fauna.abundance.max_total_game;
    let min_spacing = fauna.abundance.min_spacing;
    let mut placed: Vec<UVec2> = Vec::new();
    let mut game_idx = 0u32;
    for (pos, module_key) in winners {
        if placed.len() >= max_total {
            break;
        }
        if placed
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < min_spacing)
        {
            continue;
        }
        let Some(herd) = spawn_game_group_at(
            pos,
            module_key,
            game_idx,
            fauna,
            width,
            height,
            wrap,
            tile_registry,
            tiles,
            rng,
        ) else {
            continue;
        };
        game_idx += 1;
        log_herd_spawn(&herd);
        placed.push(pos);
        herds.push(herd);
    }
}

/// Build a single short-range game group at `pos`: pick a species hosting `module_key`,
/// roll its route/biomass, and stamp its initial `ecology_phase`. Returns `None` if no
/// species hosts the biome or the origin is not land. Shared by initial spawn and
/// per-turn immigration.
// Placement needs the config, grid bounds, both tile resources, and the RNG; grouping
// them into a struct would just move the noise without improving clarity.
#[allow(clippy::too_many_arguments)]
fn spawn_game_group_at(
    pos: UVec2,
    module_key: &str,
    game_idx: u32,
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Herd> {
    // **Site-filter the candidate list BEFORE the pick.** A species carrying a site rule may only
    // be drawn where the ground satisfies it, so a cold *inland* tile whose only candidate is a
    // marine forager correctly spawns nothing rather than seating a seal on the tundra. Candidates
    // may ask for *different kinds* of water, so the neighbour scan still runs once for the tile and
    // each candidate is tested against its own requirement; the draw stays exactly one `gen_range`
    // — only its bound changes.
    let mut candidates = fauna.game_species_for_biome(module_key);
    if candidates
        .iter()
        .any(|(_, def)| def.adjacent_water.is_required())
    {
        let (has_salt, has_fresh) =
            adjacent_water_kinds(pos, width, height, wrap, tile_registry, tiles);
        candidates.retain(|(_, def)| def.adjacent_water.satisfied_by(has_salt, has_fresh));
    }
    if candidates.is_empty() {
        return None;
    }
    let (key, def) = candidates[rng.gen_range(0..candidates.len())];
    let steps = def.sample_route_len(rng);
    let route = build_short_route(pos, steps, width, height, tile_registry, tiles, rng)?;
    let biomass = def.sample_biomass(rng);
    let carrying_capacity = def.carrying_capacity();
    let id = format!("{GAME_ID_PREFIX}{key}_{game_idx:02}");
    let mut herd = Herd::new(
        id,
        def.display_name.clone(),
        def.size_class,
        route,
        biomass,
        carrying_capacity,
        def.fodder_per_biomass,
        def.regrowth_rate_or(fauna.ecology.regrowth_rate),
        def.body_mass,
    );
    // Cache the species' husbandry ceiling (Grazing 2d-δ) so the gates read a herd field.
    herd.husbandry_ceiling = def.husbandry_ceiling;
    herd.refresh_ecology_phase(fauna);
    Some(herd)
}

/// **The dedicated predator spawn pass** (Predators Phase 1a, `docs/plan_predators.md`) — same
/// winner-collection → shuffle → greedy-spaced placement as [`spawn_short_range_game`], drawing **only
/// carnivore** species, so predators are rare and do not consume the `abundance.max_total_game` prey
/// budget. Called from [`spawn_initial_herds`] **after** both prey passes (migratory + short-range), so
/// the full prey base is present when the target is counted; it seeds predators **once** (a collapsed
/// prey base dies out and does not respawn — there is no predator immigration path, by design).
///
/// **The cap is prey-derived, not an absolute.** Instead of a single `predators.max_packs`, each
/// carnivore species carries a **target** = `round(eligible_prey_herds × prey_ratio)` (its own prey set
/// × its own ratio — a predator population is *defined by* its prey base). A winning tile seats one of
/// the carnivore species hosting its biome **whose per-species target is not yet met** (uniformly among
/// them, as before), and the loop ends when every species' target is met or the winners are exhausted.
/// For the single shipped carnivore this is exactly "place up to `target` packs", but it generalizes to
/// N predators.
// Predators carry no shore/site rule (there is no wrap-aware neighbour scan to run), but the **prey
// gate** (Predators Phase 1a) measures each candidate's prey-derived `K` over a wrap-aware sensing disk,
// so this pass now needs `wrap` for that distance test.
#[allow(clippy::too_many_arguments)] // mirrors `spawn_short_range_game`'s Bevy-resource plumbing
fn spawn_predators(
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
    herds: &mut Vec<Herd>,
) {
    let predators = &fauna.predators;
    // **Count the prey base now** (both prey passes have run) and derive each carnivore species' pack
    // target. Keyed by display name (the value a `Herd` stores in `species`), so the placement loop can
    // read a built herd's target back without a second lookup.
    let prey_index = build_prey_index(herds, fauna);
    let targets: BTreeMap<String, usize> = fauna
        .carnivore_species()
        .into_iter()
        .map(|(_, def)| (def.display_name.clone(), predator_target(def, &prey_index)))
        .collect();
    if targets.values().all(|&t| t == 0) {
        return;
    }

    // Every tile where the predator abundance roll succeeds (map-wide).
    let mut winners: Vec<(UVec2, &'static str)> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pos = UVec2::new(x, y);
            let Some(module) = module_at(pos, tile_registry, tiles) else {
                continue;
            };
            let module_key = module.as_str();
            let prob = predators.probability_for(module_key);
            if prob <= 0.0 {
                continue;
            }
            if rng.gen::<f32>() < prob {
                winners.push((pos, module_key));
            }
        }
    }
    // Shuffle so the targets + spacing thin the pool uniformly, not top-to-bottom (the game-pass idiom).
    winners.shuffle(rng);

    let mut placed: Vec<UVec2> = Vec::new();
    let mut placed_by_species: BTreeMap<String, usize> = BTreeMap::new();
    let mut pack_idx = 0u32;
    let all_targets_met = |placed_by_species: &BTreeMap<String, usize>| {
        targets.iter().all(|(species, target)| {
            placed_by_species.get(species).copied().unwrap_or(0) >= *target
        })
    };
    for (pos, module_key) in winners {
        if all_targets_met(&placed_by_species) {
            break;
        }
        if placed
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < predators.min_spacing)
        {
            continue;
        }
        let Some(herd) = spawn_predator_group_at(
            pos,
            module_key,
            pack_idx,
            fauna,
            width,
            height,
            wrap,
            tile_registry,
            tiles,
            &targets,
            &placed_by_species,
            &prey_index,
            rng,
        ) else {
            continue;
        };
        pack_idx += 1;
        *placed_by_species.entry(herd.species.clone()).or_default() += 1;
        log_herd_spawn(&herd);
        placed.push(pos);
        herds.push(herd);
    }
}

/// Build a single predator pack at `pos`: pick a **carnivore** species hosting `module_key` **whose
/// per-species target is not yet met** *and* whose **prey-derived `K` at this tile reaches its minimum
/// spawn biomass**, roll its route/biomass, and stamp its initial `ecology_phase` — the carnivore twin
/// of [`spawn_game_group_at`], with a distinct [`PREDATOR_ID_PREFIX`]. Returns `None` if no such species
/// qualifies (all hosting species are at target, none host the biome, or none has enough prey in reach)
/// or the origin is not land. Predators carry no shore rule, so there is no site filter here.
///
/// **The prey gate** (Predators Phase 1a): a pack must land where the local prey base can sustain at
/// least its smallest form. Placing on prey-sparse ground gives a stranded pack `K → 0` that despawns
/// almost immediately (idea 6 applied at spawn), so a species is a candidate only when
/// [`carnivore_k_at`]`(pos, …) >= def.min_spawn_biomass()` — the *same* prey-derived K formula the live
/// per-turn K reads, so the spawn gate and the running K never disagree. On prey-sparse maps this can
/// place fewer than the derived target; a viable pack near game beats a stillborn one.
#[allow(clippy::too_many_arguments)] // mirrors `spawn_game_group_at`'s Bevy-resource plumbing
fn spawn_predator_group_at(
    pos: UVec2,
    module_key: &str,
    pack_idx: u32,
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    // The per-species prey-derived targets + the running placed counts (both keyed by display name), so
    // a winning tile only seats a species that still has room under its target.
    targets: &BTreeMap<String, usize>,
    placed_by_species: &BTreeMap<String, usize>,
    // The map-wide prey index (start-of-spawn herbivore herds) the prey gate measures `K` against.
    prey_index: &[PreyDatum],
    rng: &mut SmallRng,
) -> Option<Herd> {
    let radius = fauna.predators.prey_sense_radius;
    let candidates: Vec<(&String, &SpeciesDef)> = fauna
        .carnivore_species_for_biome(module_key)
        .into_iter()
        .filter(|(_, def)| {
            let target = targets.get(&def.display_name).copied().unwrap_or(0);
            let placed = placed_by_species
                .get(&def.display_name)
                .copied()
                .unwrap_or(0);
            if placed >= target {
                return false;
            }
            // The prey gate: enough prey in reach to sustain at least the pack's smallest size.
            carnivore_k_at(
                pos,
                def.combat.attack,
                def.prey_per_biomass,
                prey_index,
                radius,
                width,
                wrap,
            ) >= def.min_spawn_biomass()
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let (key, def) = candidates[rng.gen_range(0..candidates.len())];
    let steps = def.sample_route_len(rng);
    let route = build_short_route(pos, steps, width, height, tile_registry, tiles, rng)?;
    let biomass = def.sample_biomass(rng);
    let carrying_capacity = def.carrying_capacity();
    let id = format!("{PREDATOR_ID_PREFIX}{key}_{pack_idx:02}");
    let mut herd = Herd::new(
        id,
        def.display_name.clone(),
        def.size_class,
        route,
        biomass,
        carrying_capacity,
        def.fodder_per_biomass,
        def.regrowth_rate_or(fauna.ecology.regrowth_rate),
        def.body_mass,
    );
    herd.husbandry_ceiling = def.husbandry_ceiling;
    herd.refresh_ecology_phase(fauna);
    Some(herd)
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_herds(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
    // **The wound-recovery rate** (`CombatTuning::wound_recovery_rate`) — a herd nobody is fighting
    // knits its accumulated damage back together here, in the same Logistics pass that regrows it.
    combat_config: Res<crate::combat_config::CombatConfigHandle>,
    // The ladder decides **how a herd moves**: each herd's rung declares its `behavior.movement`
    // primitive (§3's proximity spine `roam` → `drift_to_owner` → `fixed`), and this system is the
    // first consumer of the behavior schema (slice 3b).
    ladder_config: Res<LadderConfigHandle>,
    tick: Res<SimulationTick>,
    world_seed: Option<Res<WorldGenSeed>>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    // The camps a `drift_to_owner` herd drifts toward. `With<ResidentBand>`: the herd seeks its
    // owner's *settled* bands, not a detached expedition party that happens to be passing (the
    // positive-marker isolation convention — see `components::ResidentBand`).
    bands: Query<&PopulationCohort, With<ResidentBand>>,
    // Optional so the many hand-built fauna test harnesses that run `advance_herds` in isolation
    // don't have to stand up a graze layer; a `None`/empty registry falls back to plain land movement
    // (the pre-2b-i behaviour). The live app always carries a seeded `GrazeRegistry`.
    graze: Option<Res<GrazeRegistry>>,
) {
    if registry.herds.is_empty() {
        telemetry.entries.clear();
        density.width = 0;
        density.height = 0;
        density.samples.clear();
        return;
    }
    let fauna = fauna_config.get();
    let ladder = ladder_config.get();
    let combat_tuning = combat_config.get().tuning();
    let width = config.grid_size.x.max(1);
    let height = config.grid_size.y.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    let base_seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed) ^ tick.0;
    // A `None`/empty graze layer → plain land movement (pre-2b-i); a seeded one → graze-aware roam.
    let empty_graze = GrazeRegistry::default();
    let graze = graze.as_deref().unwrap_or(&empty_graze);
    let owner_camps = owner_camp_tiles(&bands, &tiles);
    // **The prey index** (Predators Phase 1a) — a start-of-turn snapshot of every herbivore herd, built
    // in this immutable pass *before* the mutable loop below so a carnivore's `K` (computed inside that
    // loop, which cannot read the other live herds) can read it. It reads start-of-turn prey biomass,
    // the same one-turn lag a herbivore's graze `K` has. Byte-identical to before on a carnivore-free
    // map: nothing reads it unless a herd's species is a carnivore.
    let prey_index = build_prey_index(&registry.herds, &fauna);
    for herd in registry.herds.iter_mut() {
        // Deterministic per-herd, per-turn RNG (rollback-stable): map_seed ^ tick ^ salt ^ id-hash.
        let mut hasher = FnvHasher::new();
        herd.id.hash(&mut hasher);
        let mut rng =
            SmallRng::seed_from_u64(base_seed ^ HERD_MOVEMENT_SEED_SALT ^ hasher.finish());
        // Movement cadence levers for this species (fall back to a slow game default if unresolved).
        let def = fauna.species_by_display(&herd.species);
        // **A herd nobody fought last turn knits back together** (`docs/plan_hunt_through_combat.md`
        // §4.2). Logistics runs before Population, so this reads the contact the *previous* turn's
        // take recorded: a party that keeps hunting never lets a turn of healing through, and one
        // that breaks off gets a single turn of grace before the ledger starts draining. An
        // unresolvable species falls back to the neutral body, whose `durability` is what the ledger
        // was banked against anyway.
        let quarry_body = def.map_or_else(CombatStats::default, |d| d.combat);
        herd.wounds
            .recover(combat_tuning.wound_recovery_rate, &quarry_body);
        // **The movement primitive comes from the herd's RUNG (diet-adjusted), not from
        // `is_domesticated()`** — the ladder's `behavior.movement` is config (§5), and this is the
        // first place the engine reads it. §3's proximity spine falls out of the shipped records: wild
        // `roam` → pastoral `drift_to_owner` → pen `fixed`; `movement_primitive` overlays the one
        // diet-resolved case, a wild carnivore's `pursue`.
        match movement_primitive(herd, def, &ladder) {
            // A `fixed` source does not roam — today's penned herd, pinned at `corralled_at` (Rung
            // 1c). It still grazes/regrows (ecology is independent of movement); only its wander is
            // skipped.
            RungMovement::Fixed => herd.next_pos = None,
            RungMovement::Roam => advance_herd_roam(
                herd,
                def,
                None, // no attractor: a wild herbivore roams its own full range
                &tile_registry,
                &tiles,
                graze,
                &mut rng,
                width,
                height,
                wrap,
            ),
            // `drift_to_owner`: the herd biases its step toward its owner's nearest camp. No owner,
            // or an owner with no bands → `None`, i.e. a plain roam (the fallback is the `Option`).
            RungMovement::DriftToOwner => {
                let camps = herd.owner.and_then(|owner| owner_camps.get(&owner));
                advance_herd_roam(
                    herd,
                    def,
                    camps.map(|c| c.as_slice()),
                    &tile_registry,
                    &tiles,
                    graze,
                    &mut rng,
                    width,
                    height,
                    wrap,
                )
            }
            // `pursue`: a wild carnivore steps toward the nearest **clearable prey in pursuit range**.
            // Prey come from the start-of-turn `prey_index` + the single `attack_clears_defense` rule
            // (NOT the `HerdDensityMap`, which counts every herd — uneatable mammoths, other predators
            // — a *second* prey definition). One prey rule shared with carnivore-`K` and predation, so
            // a wolf chases only prey it can actually eat. Positions are start-of-turn (the same
            // one-turn lag carnivore-`K` reads: a herbivore processed later this turn hasn't moved yet
            // from the index's view — consistent and deterministic). No prey in range → `None` → plain
            // graze-roam (re-acquire next turn), so a carnivore-free-of-prey map is byte-identical to
            // today's roam. The target list is tie-broken downstream by `resource_step_order`, so its
            // build order can't leak into the step.
            RungMovement::Pursue => {
                let pred_attack = def.map(|d| d.combat.attack).unwrap_or(0.0);
                let pursuit_radius = fauna.predators.pursuit_radius;
                let targets: Vec<UVec2> = prey_index
                    .iter()
                    .filter(|p| {
                        attack_clears_defense(pred_attack, p.defense)
                            && hex_distance_wrapped(herd.current_pos, p.pos, width, wrap)
                                <= pursuit_radius
                    })
                    .map(|p| p.pos)
                    .collect();
                advance_herd_roam(
                    herd,
                    def,
                    (!targets.is_empty()).then_some(targets.as_slice()),
                    &tile_registry,
                    &tiles,
                    graze,
                    &mut rng,
                    width,
                    height,
                    wrap,
                )
            }
        }
        // **K is ecological — for a MOBILE herd its roam range, for a PENNED herd its fenced footprint**
        // (Grazing 2b-ii + 2d §2.1). Recomputed each turn (penned herds are no longer frozen) from the
        // graze the footprint yields, so nothing downstream changes: `herd_capacity` still reads this
        // cached field. Computed AFTER movement (K reflects where the herd now stands / its fence) and
        // BEFORE `regrow_biomass` (the herd grows toward this K), over the SAME tiles
        // `advance_herd_grazing` then eats.
        //
        // **A penned herd on a WHOLLY-BARREN footprint keeps its frozen K and is fully larder-fed** —
        // §2.3's "today's behaviour, preserved as the worst case". `ecological_carrying_capacity`
        // returns `Some(0.0)` for a zero-graze footprint, which would crush the pen to zero; a rock pen
        // instead holds its herd on the granary. A grazeable footprint (`k > 0`) gives the pen its
        // ecological K and it self-feeds. (A *mobile* herd keeps the 2b-ii behaviour — it shrinks toward
        // `Some(0)` on barren ground, which its graze-aware roam is meant to keep it off of.)
        if let Some(k) =
            ecological_carrying_capacity(herd, def, graze, &prey_index, &fauna, width, height, wrap)
        {
            if !(herd.is_corralled() && k <= 0.0) {
                herd.carrying_capacity = k;
            }
        }
        regrow_biomass(herd, &fauna);
        let position = herd.position();
        info!(
            target: "shadow_scale::analytics",
            event = "herd_migrate",
            herd = %herd.id,
            label = %herd.label,
            x = position.x,
            y = position.y,
            step_index = herd.step_index,
            route_length = herd.route_length(),
            biomass = herd.biomass,
            ecology_phase = herd.ecology_phase.as_str(),
        );
    }
    // Local extinction: a group hunted to zero, or a collapsing remnant that has fallen below the
    // viability floor, **disperses** and despawns — measured against the ecology/capacity the herd
    // actually lives under (`herd_ecology`/`herd_capacity`), never the raw wild pair.
    //
    // A **penned** herd is exempt: dispersal is the mechanism of local extinction, and a corralled
    // herd is confined — it cannot disperse. A starved pen instead withers to a remnant at its
    // extinction floor (`advance_husbandry`) and **recovers when fed again**, keeping the pen. That is
    // deliberate: a recoverable famine the player can watch and fix is better play than silently
    // voiding a 25-turn investment, and it keeps starvation out of this despawn path entirely.
    //
    // An **owned** (managed) herd is likewise exempt (`docs/plan_fauna_neglect_escape.md` §2.4): a
    // managed herd does not *disperse* — it is held by its keepers, and the only way it leaves play is
    // the neglect-escape **bleed-out**, which `advance_husbandry` resolves by shedding it into the wild
    // web and despawning the empty entity itself. Without this exemption a fully-abandoned pastoral herd
    // would be despawned here the moment it dipped below the extinction floor, stranding ~a floor's
    // worth of biomass instead of shedding it out — the animals would vanish rather than go feral.
    registry.herds.retain(|herd| {
        herd.is_corralled()
            || herd.owner.is_some()
            || herd.biomass
                > herd_ecology(herd, &fauna).extinction_floor * herd_capacity(herd, &fauna)
    });
    telemetry.entries = registry.snapshot_entries();
    density.rebuild(config.grid_size, &registry);
}

/// The **graze's sustainable flow** at biomass `G` (Grazing Phase 2b-ii) — one turn's regrowth at the
/// MSY-clamped biomass (`min(G, cap/2)`), **pure logistic, without the Allee cutoff**. This is the
/// graze counterpart of [`sustainable_yield`], but deliberately *not* that helper: `sustainable_yield`
/// runs through `net_biomass_delta`, which zeroes the flow below `collapse_fraction` (the animal Allee
/// crash) — yet **grass has no depensation** (`advance_graze_regrowth` runs pure logistic, and the
/// design promises a pasture always recovers). Using `sustainable_yield` here would make a heavily-but-
/// recoverably grazed tile read `K = 0` and crash its herd to zero on ground that in fact regrows — the
/// exact "crash on recoverable ground" the convergence gate forbids. This flow peaks at
/// `r_graze·cap/4` for `G ≥ cap/2` (so `K` is flat while the range holds above its MSY point) and
/// declines smoothly to `0` as `G → 0` (so overgrazing lowers `K` continuously, no cliff).
/// **The tiles a herd grazes / derives its `K` over** (Grazing 2d §2.1) — a single seam so the K
/// computation, the graze draw-down and the wire's footprint count all read one definition. Returns
/// the `(anchor, radius)` for `hex_range_tiles`:
/// - a **penned** herd → its **fenced footprint** `(corralled_at, pen_radius)` (a pen is a piece of
///   fenced land; it does not roam);
/// - a **mobile** herd → its **roam range** `(current_pos, graze_range_radius)` (Grazing 2b-i).
///
/// `pen_radius = 0` (today) is the single corralled tile; the `ExtendPen` command (2d-β) grows it.
fn herd_footprint(herd: &Herd, def: Option<&SpeciesDef>) -> (UVec2, u32) {
    match herd.corralled_at {
        Some(pen) => (pen, herd.pen_radius),
        None => (herd.current_pos, herd.graze_range_radius(def)),
    }
}

pub(crate) fn graze_sustainable_flow(biomass: f32, cap: f32, graze_eco: &EcologyConfig) -> f32 {
    logistic_regrowth(
        biomass.min(cap * MSY_BIOMASS_FRACTION),
        cap,
        graze_eco.regrowth_rate,
    )
}

/// **A prey herd's sustainable meat flow** (Predators Phase 1a) — the carnivore counterpart of
/// [`graze_sustainable_flow`], and the term a carnivore's prey-limited `K` sums over its prey-sensing
/// disk (`K_pred = Σ_prey prey_sustainable_flow / prey_per_biomass`). Exactly the graze flow's shape —
/// one turn's **pure logistic** regrowth at the MSY-clamped biomass (`min(B, cap/2)`), *without* the
/// Allee cutoff — but computed against the prey herd's **own** per-species `regrowth_rate` (each prey
/// herd carries its own `r`, fast small game vs slow megafauna). Reads the prey's **current** (drawn-
/// down) biomass, so a thinned prey base yields less flow → lowers `K_pred` (the coupled feedback that
/// makes a predator decline as it eats its prey out).
pub(crate) fn prey_sustainable_flow(biomass: f32, cap: f32, regrowth_rate: f32) -> f32 {
    logistic_regrowth(biomass.min(cap * MSY_BIOMASS_FRACTION), cap, regrowth_rate)
}

/// **A snapshot of one prey herd** (Predators Phase 1a) — the immutable prey datum a carnivore's `K`
/// reads. `advance_herds` builds a `Vec<PreyDatum>` over every **herbivore** herd in one pass *before*
/// its `iter_mut` loop, resolving the cross-herd borrow: `ecological_carrying_capacity` runs inside
/// that loop and so cannot read the other live herds, but it can read this start-of-turn snapshot (the
/// same one-turn lag a herbivore's graze `K` has). Carnivores are excluded here so a lone seeded pack
/// can never count itself (or another predator) as prey.
#[derive(Debug, Clone, Copy)]
pub struct PreyDatum {
    /// The prey herd's live tile (the centre the predator measures its sensing-disk distance to).
    pub pos: UVec2,
    /// Start-of-turn biomass — the drawn-down stock, so a thinned prey base lowers `K_pred`.
    pub biomass: f32,
    /// The prey herd's carrying capacity (the `cap` its MSY-clamped flow is computed against).
    pub carrying_capacity: f32,
    /// The prey herd's **own** per-species wild regrowth rate.
    pub regrowth_rate: f32,
    /// The prey species' `combat.defense` — a predator counts it as prey only if its `attack` clears
    /// this (idea 7: wolves can't crack a mammoth's defense).
    pub defense: f32,
}

/// Build the [`PreyDatum`] index over every **herbivore** herd in `herds`, resolving each herd's
/// species via `fauna.species_by_display`. A herd whose species resolves to a **carnivore** is not
/// prey and is skipped; an unresolved species (an isolated test fixture) is treated as a herbivore
/// with the default `combat.defense`. Called once at the top of [`advance_herds`], before the mutable
/// herd loop.
pub fn build_prey_index(herds: &[Herd], fauna: &FaunaConfig) -> Vec<PreyDatum> {
    let default_defense = crate::combat::CombatStats::default().defense;
    herds
        .iter()
        .filter_map(|herd| {
            let def = fauna.species_by_display(&herd.species);
            if def.map(|d| d.diet) == Some(Diet::Carnivore) {
                return None;
            }
            Some(PreyDatum {
                pos: herd.current_pos,
                biomass: herd.biomass,
                carrying_capacity: herd.carrying_capacity,
                regrowth_rate: herd.regrowth_rate,
                defense: def.map_or(default_defense, |d| d.combat.defense),
            })
        })
        .collect()
}

/// **The clearance half of the prey rule** (Predators Phase 1a, idea 7): a predator counts a herbivore
/// herd as prey only if its `attack` reaches the herd's `defense`. This is the single spelling of that
/// comparison — shared by the carnivore `K` ([`carnivore_carrying_capacity`]), the predation draw
/// ([`advance_predation`]) and the prey-derived spawn count ([`eligible_prey_herds`]) — so "prey" has
/// one definition (herbivore, via [`build_prey_index`]/[`PreyDatum`], + this clearance) and never a
/// second, divergent predicate.
#[inline]
fn attack_clears_defense(attack: f32, defense: f32) -> bool {
    defense <= attack
}

/// **The map-wide count of a predator's prey** (Predators Phase 1a) — every herbivore herd this
/// predator's `attack` clears, across the whole map (no sensing-disk filter: the disk sizes a *live*
/// pack's `K`, this sizes the *population*). Counting the full prey set including small game is
/// intended — a wolf's `attack 3` clears rabbit/fowl at the default `defense 1`, consistent with the
/// model's `attack ≥ defense` rule. Reads the same [`PreyDatum`] index the carnivore `K` does, so the
/// "prey" definition is not duplicated.
fn eligible_prey_herds(predator_attack: f32, prey: &[PreyDatum]) -> usize {
    prey.iter()
        .filter(|p| attack_clears_defense(predator_attack, p.defense))
        .count()
}

/// **A carnivore species' prey-derived pack target** (Predators Phase 1a) —
/// `round(eligible_prey_herds × species.prey_ratio)`. A predator population is *defined by* its prey
/// base, so the pack count is derived from the prey the species can take, not a fixed cap. `prey_ratio`
/// is guaranteed finite `> 0` for a carnivore by [`FaunaConfig::validate`].
fn predator_target(species: &SpeciesDef, prey: &[PreyDatum]) -> usize {
    let eligible = eligible_prey_herds(species.combat.attack, prey);
    (eligible as f32 * species.prey_ratio).round() as usize
}

/// **The ecological carrying capacity** (Grazing Phase 2b-ii, `docs/plan_grazing_2b.md` §2/§3): the
/// number of animals the sustainable graze flow on a herd's range can feed. Sum the graze flow
/// ([`graze_sustainable_flow`], at each tile's **current — drawn-down —** biomass) over the herd's
/// range tiles ([`hex_range_tiles`], the SAME tiles [`advance_herd_grazing`] eats), then denominate
/// into animals by the herd's per-species `fodder_per_biomass`:
///
/// ```text
/// K = Σ_range graze_sustainable_flow(G_tile, G_cap_tile, graze.ecology) / fodder_per_biomass
/// ```
///
/// Reading the graze's **current** biomass is the whole feedback loop (§2.1): a range grazed below its
/// MSY point yields less flow, so `K` falls and the herd shrinks (the emergent overgrazing spiral); a
/// range at/above its MSY point yields the full flow, so `K` is maximal and a herd at `K` eats exactly
/// that flow, holding the pasture at the most productive grazing intensity — carrying capacity falls
/// out of the loop, it is not a number anyone set.
///
/// Returns `None` (→ the caller keeps the herd's frozen constant `K`) for a **non-grazing** herd
/// (`fodder_per_biomass <= 0`, e.g. a legacy config or a species that omits it) or when the graze
/// layer is **absent/empty** (the isolated fauna test harnesses run `advance_herds` without a graze
/// registry) — nothing regresses. A genuinely barren/overgrazed range yields `Some(small)` down toward
/// `Some(0.0)`; the herd shrinks toward it (movement, §4.1, keeps herds off zero-graze ground so this
/// is the overgrazing tail, not a stranding).
// The herd + its resolved def + the two food layers (graze registry, prey index) + grid bounds; the
// carnivore branch adds the prey index. Bundling into a struct would only move the noise.
#[allow(clippy::too_many_arguments)]
fn ecological_carrying_capacity(
    herd: &Herd,
    def: Option<&SpeciesDef>,
    graze: &GrazeRegistry,
    prey: &[PreyDatum],
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
) -> Option<f32> {
    // **Diet branches the ONE K seam** (Predators Phase 1a). A carnivore's food layer is *other herds*
    // (the prey index), not the per-tile `GrazeRegistry` — so it ignores graze / `fodder_per_biomass`
    // / `fodder_delivery_rate` entirely and sums prey flow in its sensing disk. Only the *layer* and
    // the *denominator* differ; both branches share `prey_/graze_sustainable_flow`'s logistic shape.
    if def.map(|d| d.diet) == Some(Diet::Carnivore) {
        return carnivore_carrying_capacity(herd, def.unwrap(), prey, fauna, width, wrap);
    }
    if herd.fodder_per_biomass <= 0.0 || graze.is_empty() {
        return None;
    }
    let (anchor, radius) = herd_footprint(herd, def);
    let range = hex_range_tiles(anchor, radius, width, height, wrap);
    let mut flow = 0.0;
    for tile in range {
        if let Some(patch) = graze.patch(tile) {
            flow += graze_sustainable_flow(
                patch.biomass,
                patch.carrying_capacity,
                &fauna.graze.ecology,
            );
        }
    }
    // **The fodder-flow term** (Flora Roster F3, §5.3): delivered hay enters `K` at exactly the point
    // graze does, because hay *is* feed. `fodder_delivery_rate` is the SUSTAINED inflow of the keeper
    // band's hay Fields (written by `advance_labor_allocation`), NOT the store's stock — reading a
    // built-up buffer would spike K and oscillate the coupled loop, while the flow is what the farming
    // sustainably delivers, so it settles. It is `0.0` unless the keeper knows Foddering AND the herd
    // is penned (the labor arm gates and stamps it), so a wild/unfoddered herd's `K` is byte-identical
    // to its footprint-only self. Added to the graze flow BEFORE the density gain and the
    // fodder-per-biomass division, so `K_pen = (footprint_flow + fodder_flow) / fodder_per_biomass`,
    // which relaxes "a dead tile cannot hold a pen" into an honest feedlot: a barren footprint
    // (`flow = 0`) carried entirely by delivered hay.
    flow += herd.fodder_delivery_rate.max(0.0);
    // **The per-species husbandry DENSITY gain** (the density ladder): domestication makes the land
    // hold *more* animals, non-linearly by species — a corralled goat's fenced pasture supports
    // `pen_density ×` the animals a wild goat's would. Orthogonal to the r-gains `herd_ecology` folds
    // in (which scale the *rate*, not the *ceiling*). Applied to the FINAL range-derived K, so a wild
    // herd's `×1.0` leaves this byte-identical; recomputed fresh each turn from `flow`, so it is
    // idempotent (never a compounding read of the already-scaled field).
    Some(flow / herd.fodder_per_biomass * herd_density_gain(herd, fauna))
}

/// **A carnivore's prey-limited carrying capacity** (Predators Phase 1a) — the trophic transpose of
/// the herbivore path above: `K_pred = Σ_prey prey_sustainable_flow(prey) / def.prey_per_biomass` over
/// the prey herds inside the predator's **prey-sensing disk**
/// (`fauna.predators.prey_sense_radius`, wider than a graze footprint because prey are sparse points).
/// A herd counts as prey only if it is a herbivore (already true of every [`PreyDatum`]) **and** its
/// `defense <= the predator's attack` (idea 7 — a wolf's `attack 3` never counts a mammoth's
/// `defense 12`). Graze / fodder / `herd_density_gain` are ignored (a wild predator's density gain is
/// `×1.0` anyway; the branch keeps it clean). `prey_per_biomass > 0` is guaranteed for a carnivore by
/// `FaunaConfig::validate`, so the division is safe; a thinned or absent prey base yields
/// `Some(small)`→`Some(0.0)`, which drives the pack down (and, past its extinction floor, despawns it).
fn carnivore_carrying_capacity(
    herd: &Herd,
    def: &SpeciesDef,
    prey: &[PreyDatum],
    fauna: &FaunaConfig,
    width: u32,
    wrap: bool,
) -> Option<f32> {
    Some(carnivore_k_at(
        herd.current_pos,
        def.combat.attack,
        def.prey_per_biomass,
        prey,
        fauna.predators.prey_sense_radius,
        width,
        wrap,
    ))
}

/// **The position-parameterized prey-derived `K`** (Predators Phase 1a) — the one formula for a
/// carnivore's prey-limited carrying capacity, shared by the live per-turn K
/// ([`carnivore_carrying_capacity`]) and the prey-gated spawn ([`spawn_predator_group_at`]) so the two
/// can never diverge (DRY): `K = Σ_prey prey_sustainable_flow(prey) / prey_per_biomass` over the prey
/// herds inside `radius` of `pos` whose `defense` the predator's `attack` clears
/// ([`attack_clears_defense`], the single prey rule). `prey_per_biomass > 0` is guaranteed for a
/// carnivore by [`FaunaConfig::validate`], so the division is safe.
pub fn carnivore_k_at(
    pos: UVec2,
    attack: f32,
    prey_per_biomass: f32,
    prey: &[PreyDatum],
    radius: u32,
    width: u32,
    wrap: bool,
) -> f32 {
    let flow: f32 = prey
        .iter()
        .filter(|p| {
            attack_clears_defense(attack, p.defense)
                && hex_distance_wrapped(pos, p.pos, width, wrap) <= radius
        })
        .map(|p| prey_sustainable_flow(p.biomass, p.carrying_capacity, p.regrowth_rate))
        .sum();
    flow / prey_per_biomass
}

/// **The graze draw-down** (Grazing Phase 2b-i, `docs/plan_grazing_2b.md` §3). Each **mobile,
/// non-corralled** herd eats the graze on the tiles in its range, lowering the `GrazeRegistry` — the
/// animal-edible mirror of `forage::forage_take`. A corralled herd is fed from its keeper's larder
/// (`pen_upkeep`), not from the land, so it is skipped.
///
/// Per herd: enumerate its **range** = [`hex_range_tiles`]`(current_pos, graze_range_radius)`, demand
/// `fodder_per_biomass × biomass` fodder, and draw it from the range's patches ([`graze_take`]),
/// **proportional to each tile's available graze** and floored at the **overgrazing escapement floor**
/// (never below `overgraze_escapement_fraction × capacity` — 2b-ii's convergence discipline; a barren
/// tile with no patch contributes nothing).
///
/// **Deterministic under rollback.** Herds are drawn **sequentially in `HerdRegistry` order** — that
/// Vec is itself rollback-persisted in a fixed order (the checkpoint clones the registry whole), and
/// `advance_herds`' `retain` / immigration's `push` both preserve it — so two herds sharing a tile
/// always draw in the same order, and the eaten state is reproducible.
///
/// **This is one half of the coupled model (2b-ii).** The draw-down lowers the range's graze, which is
/// what [`ecological_carrying_capacity`] reads next turn to size the herd — so eating a range down
/// *lowers `K`* (the overgrazing feedback), and the escapement floor is what stops that feedback from
/// running away. (In 2b-i this was inert on `K`; 2b-ii activates it.)
///
/// Turn order: registered **after `advance_herds`** (herds have roamed to their new tile *and* had `K`
/// recomputed + grown toward it) and **before `advance_graze_regrowth`** (so the eaten state is what
/// regrows — a herd can't eat grass that regrew the same turn).
pub fn advance_herd_grazing(
    mut herds: ResMut<HerdRegistry>,
    mut graze: ResMut<GrazeRegistry>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    if herds.herds.is_empty() || graze.is_empty() {
        return;
    }
    let fauna = fauna_config.get();
    let width = config.grid_size.x.max(1);
    let height = config.grid_size.y.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    // Grazing draws down to the **overgrazing escapement floor** (2b-ii), not the reseed floor: the
    // constant-escapement discipline that keeps the herd↔graze loop convergent (validated `>` the
    // reseed floor, so it is the binding one). Below it a range collapses into a stripped remnant.
    let escapement_floor_fraction = fauna.graze.overgraze_escapement_fraction;
    for herd in herds.herds.iter_mut() {
        // **Penned herds graze too now (Grazing 2d §2.2)** — a pen is a piece of fenced *land*, and the
        // herd draws it down over its footprint exactly like a wild herd (escapement-floored). The grass
        // it draws (`footprint_intake`) offsets its keeper's larder bill this turn (§2.3, read in
        // `advance_labor_allocation`). `herd_footprint` picks the fenced footprint for a penned herd,
        // the roam range for a mobile one.
        let demand = (herd.fodder_per_biomass * herd.biomass).max(0.0);
        if demand <= 0.0 {
            herd.footprint_intake = 0.0;
            continue;
        }
        let def = fauna.species_by_display(&herd.species);
        let (anchor, radius) = herd_footprint(herd, def);
        let range = hex_range_tiles(anchor, radius, width, height, wrap);
        herd.footprint_intake = graze_take(&mut graze, &range, demand, escapement_floor_fraction);
    }
}

/// Draw `demand` fodder from the graze patches on `range`, **proportional to each tile's available
/// graze** (biomass above `floor_fraction × capacity`) and clamped so no patch drops below that floor.
/// The animal-edible counterpart of `forage::forage_take`'s subtract-and-clamp discipline.
///
/// `floor_fraction` is the **overgrazing escapement floor** (2b-ii, `graze.overgraze_escapement_fraction`)
/// — grazing may draw a patch down to it but no further, the constant-escapement discipline that keeps
/// the coupled herd↔graze loop convergent (a deeper draw would let a range collapse into a stripped
/// remnant it cannot climb back out of; `docs/plan_grazing_2b.md` §2.2). It sits *above* the reseed
/// lift, so it is the binding floor.
///
/// Proportional distribution (not an even split) is order-independent within a single herd's take and
/// spreads the pressure toward the richer tiles in the range; a tile with no patch (barren) simply
/// isn't in the sum and contributes nothing. If the whole range's available graze is below `demand`
/// the herd eats all of it (down to the floors) and no further — the range is grazed out for the turn.
/// The `ecology_phase` is left stale here on purpose: `advance_graze_regrowth` (the very next system)
/// regrows every patch and refreshes its phase, exactly as `forage_take` defers to `regrow_patch`.
///
/// **Returns the biomass actually drawn** (`min(demand, total_available)`), which the pen larder-offset
/// (Grazing 2d §2.3) reads as the herd's `footprint_intake` — the share the footprint fed the pen.
fn graze_take(graze: &mut GrazeRegistry, range: &[UVec2], demand: f32, floor_fraction: f32) -> f32 {
    // Total graze available across the range (each tile's biomass above the escapement floor).
    let mut total_available = 0.0;
    for &tile in range {
        if let Some(patch) = graze.patch(tile) {
            let floor = floor_fraction * patch.carrying_capacity;
            total_available += (patch.biomass - floor).max(0.0);
        }
    }
    if total_available <= 0.0 {
        return 0.0;
    }
    let taken_fraction = (demand / total_available).min(1.0);
    for &tile in range {
        if let Some(patch) = graze.patch_mut(tile) {
            let floor = floor_fraction * patch.carrying_capacity;
            let available = (patch.biomass - floor).max(0.0);
            patch.biomass -= available * taken_fraction;
        }
    }
    (taken_fraction * total_available).max(0.0)
}

/// **The predation draw-down** (Predators Phase 1a, `docs/plan_predators.md`) — the trophic transpose
/// of [`advance_herd_grazing`]: each turn every **carnivore** herd eats prey biomass from the
/// herbivore herds inside its prey-sensing disk, exactly as an herbivore draws its range's graze down.
/// The "layer" is the prey herds themselves, so this operates on the `HerdRegistry` in place.
///
/// Per predator: demand `prey_per_biomass × biomass`, drawn from the prey herds in
/// `predators.prey_sense_radius` whose `combat.defense <= the predator's combat.attack`, **proportional
/// to each prey herd's available biomass** (biomass above the functional-response floor
/// `predators.predation_escapement_fraction × prey.carrying_capacity`) and clamped so no prey herd
/// drops below that floor — the taper that makes the pack take less as prey thins and stop before zero
/// (the discrete analog of a Lotka–Volterra oscillation). Predation credits **no food to anyone** this
/// phase (a wolf's dinner is abstracted biomass, not a player yield) and sets no `footprint_intake`.
///
/// **Index-based over the Vec** (predator `i` mutates prey `j`, always distinct — a carnivore is never
/// its own prey) to satisfy the borrow checker, and **deterministic in `HerdRegistry` order**:
/// predators are drawn sequentially, and the proportional split within one predator's take is
/// order-independent (the exact discipline `advance_herd_grazing` uses for shared graze). Registered in
/// Logistics **after `advance_herd_grazing`** and **before `advance_graze_regrowth`**, so the eaten
/// prey state is what regrows next turn's `advance_herds` (symmetric to grazing).
pub fn advance_predation(
    mut herds: ResMut<HerdRegistry>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
) {
    if herds.herds.is_empty() {
        return;
    }
    let fauna = fauna_config.get();
    let width = config.grid_size.x.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    let escapement_floor_fraction = fauna.predators.predation_escapement_fraction;
    let radius = fauna.predators.prey_sense_radius;
    let default_defense = crate::combat::CombatStats::default().defense;
    let len = herds.herds.len();
    for i in 0..len {
        // Read predator `i`: resolve its species; a non-carnivore (or unresolved) herd is not a hunter.
        let (pred_pos, attack, demand) = {
            let pred = &herds.herds[i];
            let Some(def) = fauna.species_by_display(&pred.species) else {
                continue;
            };
            if def.diet != Diet::Carnivore {
                continue;
            }
            let demand = (def.prey_per_biomass * pred.biomass).max(0.0);
            (pred.current_pos, def.combat.attack, demand)
        };
        if demand <= 0.0 {
            continue;
        }
        // Collect the prey herds in range whose defense this predator's attack clears, and their total
        // available biomass (above the escapement floor). `j != i` by construction (a carnivore is not
        // a herbivore), but the diet check below also excludes any *other* carnivore.
        let mut prey_indices: Vec<usize> = Vec::new();
        let mut total_available = 0.0;
        for j in 0..len {
            if j == i {
                continue;
            }
            let prey = &herds.herds[j];
            let pdef = fauna.species_by_display(&prey.species);
            if pdef.map(|d| d.diet) == Some(Diet::Carnivore) {
                continue;
            }
            let defense = pdef.map_or(default_defense, |d| d.combat.defense);
            if !attack_clears_defense(attack, defense) {
                continue;
            }
            if hex_distance_wrapped(pred_pos, prey.current_pos, width, wrap) > radius {
                continue;
            }
            let floor = escapement_floor_fraction * prey.carrying_capacity;
            let available = (prey.biomass - floor).max(0.0);
            if available <= 0.0 {
                continue;
            }
            prey_indices.push(j);
            total_available += available;
        }
        if total_available <= 0.0 {
            continue;
        }
        let drawn_fraction = (demand / total_available).min(1.0);
        for j in prey_indices {
            let prey = &mut herds.herds[j];
            let floor = escapement_floor_fraction * prey.carrying_capacity;
            let available = (prey.biomass - floor).max(0.0);
            prey.biomass -= available * drawn_fraction;
        }
    }
}

/// One turn of graze-wander / loiter-migrate movement (`docs/plan_wildlife_hunting_overlay.md`
/// "Herd Movement"). Deterministic under the per-turn seeded `rng`. Mutates the herd's
/// `current_pos` / `dwell_remaining` / `roam` / `step_index` / `next_pos`. `def` supplies the
/// species' cadence levers (`None` → a slow game default). Movement is ≤ 1 hex/turn and land-clamped;
/// it never touches `biomass` (ecology stays independent — a loitering herd still grazes/regrows).
///
/// **`attractor`** carries the herd's resource-seeking primitive — the ONE attractor path both
/// `drift_to_owner` (pastoral → owner camps) and `pursue` (wild carnivore → clearable prey tiles)
/// ride: `Some(tiles)` = this herd's rung/diet says it steers toward the nearest of `tiles`. The two
/// are mutually exclusive (a pastoral herbivore drifts, a wild carnivore pursues, a wild herbivore
/// does neither), so the *same* block serves both — the only difference (which tiles, and any range
/// gate) is built in `advance_herds`. It **composes with, and does not replace, the wild roam**: the
/// pre-empt only takes a turn it can genuinely get *closer* to an attractor. Once the herd is as near
/// as it can get — at the tile, or hemmed in — the turn falls through to the normal state machine, so
/// a tamed herd grazes around its people (and a pack around its prey) instead of freezing on the tile.
/// `None` (a wild herbivore, an unowned pastoral herd, or a carnivore with no prey in range) is
/// exactly today's roam.
// Args are the herd + its cadence levers + the grid/tile context needed to land-clamp a hex step;
// bundling them adds noise without clarity (matches the other fauna spawn/movement helpers).
#[allow(clippy::too_many_arguments)]
fn advance_herd_roam(
    herd: &mut Herd,
    def: Option<&SpeciesDef>,
    attractor: Option<&[UVec2]>,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    rng: &mut SmallRng,
    width: u32,
    height: u32,
    wrap: bool,
) {
    let dwell_turns = def.map(|d| d.dwell_turns).unwrap_or(1);
    let loiter_radius = def.map(|d| d.loiter_radius).unwrap_or(2);
    herd.next_pos = None;

    // **The attractor pre-empt** — a herd whose rung/diet seeks a resource (a tamed herd its owner's
    // camp, a wild carnivore its prey) steers toward the nearest attractor tile *before* the roam
    // state machine, and takes the turn when it can close the distance. The species' own `dwell_turns`
    // cadence still applies: taming an animal — and pursuing prey — makes it *near*, not *faster* (a
    // wolf is not quicker than a deer).
    if let Some(targets) = attractor.filter(|targets| !targets.is_empty()) {
        if herd.dwell_remaining > 0 {
            herd.dwell_remaining -= 1;
            return;
        }
        if relocate_toward_resource(herd, targets, registry, tiles, graze, width, height, wrap) {
            herd.dwell_remaining = dwell_turns;
            return;
        }
        // Already at the target (or no acceptable step gets nearer) → fall through to the normal roam.
    }

    match herd.roam {
        RoamState::GrazeWander => {
            // Wild game: graze `dwell_turns`, then step one hex toward the current cluster
            // waypoint, advancing to the next when reached (a route_len==1 group stays put).
            if herd.dwell_remaining > 0 {
                herd.dwell_remaining -= 1;
                return;
            }
            let target = herd
                .route
                .get(herd.step_index)
                .copied()
                .unwrap_or(herd.current_pos);
            if herd.current_pos == target && !herd.route.is_empty() {
                herd.step_index = (herd.step_index + 1) % herd.route.len();
            }
            let target = herd
                .route
                .get(herd.step_index)
                .copied()
                .unwrap_or(herd.current_pos);
            step_herd_toward(herd, target, registry, tiles, graze, width, height, wrap);
            herd.dwell_remaining = dwell_turns;
        }
        RoamState::Loiter { turns_left } => {
            if turns_left == 0 {
                // Loiter expired — commit to migrating to the next anchor (starts next turn).
                herd.roam = RoamState::Migrate;
                return;
            }
            let anchor = herd
                .route
                .get(herd.step_index)
                .copied()
                .unwrap_or(herd.current_pos);
            // Graze-wander confined to `loiter_radius` of the anchor: dwell, then a ≤1-hex nudge.
            if herd.dwell_remaining > 0 {
                herd.dwell_remaining -= 1;
            } else {
                wander_near_anchor(
                    herd,
                    anchor,
                    loiter_radius,
                    registry,
                    tiles,
                    graze,
                    rng,
                    width,
                    height,
                    wrap,
                );
                herd.dwell_remaining = dwell_turns;
            }
            herd.roam = RoamState::Loiter {
                turns_left: turns_left - 1,
            };
        }
        RoamState::Migrate => {
            // Directed leg to the next anchor at 1 hex/turn, no grazing pause.
            let next_index = if herd.route.is_empty() {
                0
            } else {
                (herd.step_index + 1) % herd.route.len()
            };
            let target = herd
                .route
                .get(next_index)
                .copied()
                .unwrap_or(herd.current_pos);
            let moved = step_herd_toward(herd, target, registry, tiles, graze, width, height, wrap);
            if herd.current_pos == target || !moved {
                // Arrived (or hemmed in) → loiter at the new anchor for a fresh window.
                herd.step_index = next_index;
                let turns = def.map(|d| d.sample_loiter_turns(rng)).unwrap_or(16);
                herd.roam = RoamState::Loiter { turns_left: turns };
                herd.dwell_remaining = 0;
            } else {
                // Heading arrow: where it will step next turn.
                herd.next_pos = best_land_neighbor_toward(
                    herd.current_pos,
                    target,
                    registry,
                    tiles,
                    graze,
                    width,
                    height,
                    wrap,
                );
            }
        }
    }
}

/// Step the herd one hex toward `target`, choosing the land neighbour that most reduces hex
/// distance (deterministic tie-break by direction order). Returns whether it moved (`false` = no
/// land neighbour gets closer, so it stays — avoids marching into water / off the map).
#[allow(clippy::too_many_arguments)]
fn step_herd_toward(
    herd: &mut Herd,
    target: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    width: u32,
    height: u32,
    wrap: bool,
) -> bool {
    if herd.current_pos == target {
        return false;
    }
    match best_land_neighbor_toward(
        herd.current_pos,
        target,
        registry,
        tiles,
        graze,
        width,
        height,
        wrap,
    ) {
        Some(next) => {
            herd.current_pos = next;
            true
        }
        None => false,
    }
}

/// The land neighbour of `from` that best steps toward `target` — **graze-aware** (Grazing 2b-i
/// §4.1). A candidate must be land, **grazeable** (a `GrazeRegistry` patch with positive capacity —
/// never barren glacier / rock / desert, where a grazer would starve on ground it should never cross),
/// and strictly closer to `target` than `from` (so a herd never oscillates, backtracks, or wanders
/// away from its anchor). Among those, the closest wins; **ties break toward the richer pasture**
/// (higher graze capacity) so a herd drifts along fertile ground, and direction order breaks the rest.
/// `None` = no grazeable step gets closer, so the herd stays put — a herd hemmed in by barren does not
/// cross it.
#[allow(clippy::too_many_arguments)]
fn best_land_neighbor_toward(
    from: UVec2,
    target: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    width: u32,
    height: u32,
    wrap: bool,
) -> Option<UVec2> {
    let cur_dist = hex_distance_wrapped(from, target, width, wrap);
    // (pos, hex distance to target, graze capacity) — closest-then-richest.
    let mut best: Option<(UVec2, u32, f32)> = None;
    for (np, cap) in acceptable_steps(from, registry, tiles, graze, width, height, wrap) {
        let d = hex_distance_wrapped(np, target, width, wrap);
        if d >= cur_dist {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, best_dist, best_cap)) => d < best_dist || (d == best_dist && cap > best_cap),
        };
        if better {
            best = Some((np, d, cap));
        }
    }
    best.map(|(pos, _, _)| pos)
}

/// **The steps a herd may take at all** — the acceptance filter *every* movement primitive orders
/// within (Grazing 2b-i §4.1), so `roam` and `drift_to_owner` can never disagree about what ground is
/// crossable. A candidate must be a hex neighbour, **land**, and — when a graze layer is seeded —
/// **not barren** (no patch, or zero capacity: dead ground a grazer would starve on and must never
/// cross). Returned in hex-direction order, each paired with its tile's graze **capacity** — the
/// land's stable fertility, which is the preference the primitives order by (never the live biomass;
/// chasing *receding* grass is the deferred 2c dynamic).
///
/// With no seeded graze layer (the isolated fauna test harnesses / pre-graze worldgen) every land
/// neighbour is acceptable at capacity `0` — the pre-2b-i behaviour, unchanged.
fn acceptable_steps(
    from: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    width: u32,
    height: u32,
    wrap: bool,
) -> Vec<(UVec2, f32)> {
    let graze_aware = !graze.is_empty();
    let mut steps = Vec::new();
    for dir in 0..HEX_DIRECTION_COUNT {
        let Some((nx, ny)) = hex_neighbor(from.x, from.y, dir, width, height, wrap) else {
            continue;
        };
        let np = UVec2::new(nx, ny);
        if !is_land_tile(np, registry, tiles) {
            continue;
        }
        let cap = tile_graze_capacity(graze, np);
        if graze_aware && cap <= NO_GRAZE_CAPACITY {
            continue;
        }
        steps.push((np, cap));
    }
    steps
}

/// **The shared prey-/owner-seeking primitive** (`relocate_toward_resource`) — one step toward the
/// **nearest** of `targets`. It is the mechanism the design calls the trophic transpose: `drift_to_owner`
/// hands it its owner's camps, `pursue` hands it the clearable prey tiles, and the body is identical.
/// Returns whether the herd moved — `false` = it is already as near as it can get (standing on a target,
/// or no acceptable step closes the distance), and the caller falls through to the herd's normal roam,
/// so a tamed herd grazes *around* its people (and a pack around its prey) rather than freezing on the
/// tile.
///
/// **It composes with the roam, it does not replace it.** The candidates are exactly
/// [`acceptable_steps`] — the roam's own land + barren-avoidance filter — and the roam's fertility
/// preference survives as the second sort key. What the primitive changes is only the herd's
/// *attractor*: the target set instead of its wild route anchor.
///
/// **The order is TOTAL and hasher-independent**: `(target distance ASC, graze capacity DESC, y ASC,
/// x ASC)`. The last two keys exist because the first two can genuinely tie (two neighbours the same
/// distance out on the same biome) and a tie broken by anything incidental is a flake waiting to
/// happen — the lesson of `GrazeRegistry::richest_patch`'s `HashMap`-order tie.
/// [`nearest_target_distance`] mins over the targets, so the target list's order cannot leak in either.
///
/// **There is no attractor-strength lever, deliberately** — this is a preference *ordering*, not a
/// weight: one step per turn toward the resource, and nothing to tune.
#[allow(clippy::too_many_arguments)]
fn relocate_toward_resource(
    herd: &mut Herd,
    targets: &[UVec2],
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    width: u32,
    height: u32,
    wrap: bool,
) -> bool {
    let current = nearest_target_distance(herd.current_pos, targets, width, wrap);
    if current == 0 {
        // Standing on a target tile: nothing to close, so the normal roam takes the turn.
        return false;
    }
    let mut best: Option<(u32, f32, UVec2)> = None;
    for (np, cap) in acceptable_steps(
        herd.current_pos,
        registry,
        tiles,
        graze,
        width,
        height,
        wrap,
    ) {
        let d = nearest_target_distance(np, targets, width, wrap);
        // Only a step that genuinely closes the distance counts; anything else is roaming, and
        // roaming is the fall-through's job.
        if d >= current {
            continue;
        }
        let candidate = (d, cap, np);
        let better = best.is_none_or(|best| resource_step_order(candidate, best) == Ordering::Less);
        if better {
            best = Some(candidate);
        }
    }
    match best {
        Some((_, _, pos)) => {
            herd.current_pos = pos;
            true
        }
        None => false,
    }
}

/// The attractor step's **total** candidate order: nearest the target, then the richer pasture, then a
/// deterministic `(y, x)` tie-break. Shared by drift and pursue. See [`relocate_toward_resource`] on
/// why the last key is not optional.
fn resource_step_order(a: (u32, f32, UVec2), b: (u32, f32, UVec2)) -> Ordering {
    a.0.cmp(&b.0)
        .then_with(|| b.1.total_cmp(&a.1))
        .then_with(|| a.2.y.cmp(&b.2.y))
        .then_with(|| a.2.x.cmp(&b.2.x))
}

/// Hex distance from `from` to the **nearest** of `targets` (wrap-aware). A `min` over the list, so the
/// list's order cannot change the answer — the primitive's determinism rests on this. An empty list
/// never reaches here (the caller filters it into a plain roam).
fn nearest_target_distance(from: UVec2, targets: &[UVec2], width: u32, wrap: bool) -> u32 {
    targets
        .iter()
        .map(|&target| hex_distance_wrapped(from, target, width, wrap))
        .min()
        .unwrap_or(u32::MAX)
}

/// Every faction's **camps** — the tiles its resident bands stand on this turn, which is what a
/// `drift_to_owner` herd steers by. Keyed by faction for an O(1) lookup per herd; each faction's Vec
/// order is irrelevant by construction ([`camp_distance`] mins over it).
fn owner_camp_tiles(
    bands: &Query<&PopulationCohort, With<ResidentBand>>,
    tiles: &Query<&Tile>,
) -> BTreeMap<FactionId, Vec<UVec2>> {
    let mut camps: BTreeMap<FactionId, Vec<UVec2>> = BTreeMap::new();
    for cohort in bands.iter() {
        if let Ok(tile) = tiles.get(cohort.current_tile) {
            camps.entry(cohort.faction).or_default().push(tile.position);
        }
    }
    camps
}

/// **The rung a herd stands on** — the animal ladder resolved for one herd: penned → `animal:pen`,
/// tamed → `animal:pastoral`, else `animal:wild`. THE seam between herd state and the ladder config:
/// a system asks for the rung and reads its declared primitives, instead of re-deriving behaviour
/// from `is_domesticated()` at the call site (which is how the ladder stops being data).
///
/// Two systems resolve a herd through it: `advance_herds` (which movement primitive to run) and the
/// Hunt arm of `advance_labor_allocation` (**which knowledge this herd's rung teaches** —
/// `RungDef::knowledge_earned`, slice 4). The plant twin is `forage::patch_rung`.
pub(crate) fn herd_rung<'a>(herd: &Herd, ladder: &'a LadderConfig) -> &'a RungDef {
    ladder.rung(if herd.is_corralled() {
        RungKey::AnimalPen
    } else if herd.is_domesticated() {
        RungKey::AnimalPastoral
    } else {
        RungKey::AnimalWild
    })
}

/// **The movement primitive a herd actually runs** — the herd's rung movement ([`herd_rung`]),
/// EXCEPT a **wild carnivore**, which [`RungMovement::Pursue`]s the nearest prey it can eat instead of
/// roaming toward grass. Resolved **diet-aware here**, not from a rung record, because the husbandry
/// rungs are diet-orthogonal: `animal:wild` is one rung shared by a deer and a wolf, so a carnivore's
/// food-seeking movement cannot be a rung-record field today. Only a *wild* carnivore pursues — a
/// future tamed wolf→dog would keep its rung's `drift_to_owner`; all shipped carnivores are
/// `wild`-ceiling, so this is always `Pursue` for them. An unresolved/`None` `def` is not a carnivore,
/// so it falls to its rung's movement.
fn movement_primitive(
    herd: &Herd,
    def: Option<&SpeciesDef>,
    ladder: &LadderConfig,
) -> RungMovement {
    // "Wild" = neither tamed nor penned — exactly `herd_rung`'s `animal:wild` branch.
    let is_wild = !herd.is_corralled() && !herd.is_domesticated();
    if is_wild && def.map(|d| d.diet) == Some(Diet::Carnivore) {
        return RungMovement::Pursue;
    }
    herd_rung(herd, ladder).behavior.movement
}

/// A tile's graze **capacity** (the land's stable fertility, not its live biomass) — `0` where no
/// patch exists (barren biome). 2b-i's movement keys off capacity, not the eaten-down live biomass,
/// on purpose: chasing *receding* grass (leaving a cluster because it was grazed out) is the emergent
/// 2c dynamic, deliberately deferred. Here herds only *avoid barren* and *prefer fertile* ground.
fn tile_graze_capacity(graze: &GrazeRegistry, tile: UVec2) -> f32 {
    graze
        .patch(tile)
        .map(|patch| patch.carrying_capacity)
        .unwrap_or(NO_GRAZE_CAPACITY)
}

/// Nudge the herd ≤1 hex within `loiter_radius` of `anchor` — **graze-aware** (Grazing 2b-i §4.1).
/// Candidates must be land, within the loiter radius, **and grazeable** (a positive-capacity patch);
/// the herd never wanders onto barren ground and, if hemmed in by it, stays put. The step is chosen
/// **weighted by graze capacity** (richer pasture more likely), folding graze into the *existing*
/// per-turn seeded `rng` (one draw — no second RNG), so it stays deterministic under rollback.
#[allow(clippy::too_many_arguments)]
fn wander_near_anchor(
    herd: &mut Herd,
    anchor: UVec2,
    loiter_radius: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    rng: &mut SmallRng,
    width: u32,
    height: u32,
    wrap: bool,
) {
    // With no seeded graze layer (isolated test harnesses) fall back to plain land movement.
    let graze_aware = !graze.is_empty();
    // (tile, graze capacity) for each acceptable step inside the loiter radius.
    let mut options: Vec<(UVec2, f32)> = Vec::new();
    let mut total_capacity = 0.0;
    for (np, cap) in acceptable_steps(
        herd.current_pos,
        registry,
        tiles,
        graze,
        width,
        height,
        wrap,
    ) {
        if hex_distance_wrapped(np, anchor, width, wrap) > loiter_radius {
            continue;
        }
        options.push((np, cap));
        total_capacity += cap;
    }
    if options.is_empty() {
        return;
    }
    if !graze_aware {
        // Pre-2b-i behaviour: a uniform random land neighbour (same RNG draw as before).
        herd.current_pos = options[rng.gen_range(0..options.len())].0;
        return;
    }
    // Capacity-weighted pick over the one existing RNG draw (all-positive weights, so this always
    // lands on an option; the final fallback covers f32 rounding at the top of the range).
    let mut threshold = rng.gen::<f32>() * total_capacity;
    for (tile, cap) in &options {
        threshold -= cap;
        if threshold <= 0.0 {
            herd.current_pos = *tile;
            return;
        }
    }
    herd.current_pos = options[options.len() - 1].0;
}

/// Per-turn immigration: with probability `immigration.chance_per_turn`, respawn one
/// short-range game group up to the abundance cap so an overhunted map slowly
/// replenishes (early forager play stays game-rich). Samples up to
/// `immigration.max_attempts` random land tiles hosting game, respecting `min_spacing`
/// from existing groups. Runs in `TurnStage::Logistics` right after `advance_herds`.
// Bevy system signature: each param is a distinct resource/query the immigration roll
// needs (registry + telemetry/density outputs, config, tick+seed for the RNG, tiles);
// they can't be collapsed without a container resource that adds no clarity.
#[allow(clippy::too_many_arguments)]
pub fn repopulate_fauna(
    mut registry: ResMut<HerdRegistry>,
    mut telemetry: ResMut<HerdTelemetry>,
    mut density: ResMut<HerdDensityMap>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
    tick: Res<SimulationTick>,
    world_seed: Option<Res<WorldGenSeed>>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
) {
    let fauna = fauna_config.get();
    let imm = &fauna.immigration;
    // `max_total_game` caps short-range game groups only (matching spawn's `placed`
    // counter); migratory `herd_*` are spawned separately and don't count against it.
    let game_count = registry
        .herds
        .iter()
        .filter(|herd| herd.id.starts_with(GAME_ID_PREFIX))
        .count();
    if imm.chance_per_turn <= 0.0 || game_count >= fauna.abundance.max_total_game {
        return;
    }

    let width = config.grid_size.x.max(4);
    let height = config.grid_size.y.max(4);
    let seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed);
    let mut rng = SmallRng::seed_from_u64(seed ^ tick.0 ^ IMMIGRATION_SEED_SALT);

    // Roll the per-turn immigration chance.
    if rng.gen::<f32>() >= imm.chance_per_turn {
        return;
    }

    // Ids past the initial cap + tick keep immigrants from colliding with spawn ids
    // (only one group immigrates per turn, so `tick` disambiguates across turns).
    let idx = fauna.abundance.max_total_game as u32 + tick.0 as u32;
    let min_spacing = fauna.abundance.min_spacing;
    let existing: Vec<UVec2> = registry.herds.iter().map(|herd| herd.position()).collect();

    for _ in 0..imm.max_attempts {
        let pos = UVec2::new(rng.gen_range(0..width), rng.gen_range(0..height));
        let Some(module) = module_at(pos, &tile_registry, &tiles) else {
            continue;
        };
        let module_key = module.as_str();
        if fauna.abundance.probability_for(module_key) <= 0.0 {
            continue;
        }
        if existing
            .iter()
            .any(|p| chebyshev_distance(*p, pos) < min_spacing)
        {
            continue;
        }
        if let Some(herd) = spawn_game_group_at(
            pos,
            module_key,
            idx,
            &fauna,
            width,
            height,
            config.map_topology.wrap_horizontal,
            &tile_registry,
            &tiles,
            &mut rng,
        ) {
            info!(
                target: "shadow_scale::analytics",
                event = "immigration",
                herd = %herd.id,
                species = %herd.species,
                x = pos.x,
                y = pos.y,
                biomass = herd.biomass,
            );
            registry.herds.push(herd);
            telemetry.entries = registry.snapshot_entries();
            density.rebuild(config.grid_size, &registry);
            return;
        }
    }
}

/// Per-turn husbandry (`TurnStage::Logistics`, after `advance_herds`): the pen **feed** check and the
/// shared **shed** mechanic for every managed (pastoral or penned) herd. Runs before the same turn's
/// `Tame` accrual in `advance_labor_allocation` (`Population`), the deliberate one-turn lag.
///
/// **NEGLECT SHEDS ANIMALS, IT DOES NOT UN-TAME THEM** (`docs/plan_fauna_neglect_escape.md`). This
/// pass **replaced both** the tameness-bleed (`decay_under_herded`) and the binary corral escape with
/// **one** mechanic: an under-contained managed herd sheds whole animals over its labor capacity into a
/// nearby **wild** herd of the same species ([`shed_uncontained_animals`] → [`place_shed_animals`]),
/// and `domestication_progress` is **never** decayed by neglect (it is monotone-up, earned via `Tame`).
/// Total abandonment falls out as the `herded_fraction == 0` limit: the whole flock **bleeds out** to
/// the wild web over turns, and when the herd can no longer shed a whole animal the empty managed entity
/// is **despawned** (Phase 3; the pen, if any, is announced lost via [`announce_pen_lost`] first, §2.4).
/// The pen's **feed** (`starve_underfed_pen`) is orthogonal and unchanged (§2.5).
///
/// **PASSIVE-FREE PASTORAL IS RETIRED — this pass pays NOTHING** (slice 3b,
/// `docs/plan_intensification_ladder.md` §3: *every* rung is worker-driven). A tamed herd used to pay
/// its owner its pastoral MSY here with **no worker at all**, split across the owner's bands. It no
/// longer does: a pastoral herd yields **only** through a normal `Hunt` assignment, exactly like a
/// wild one. The taming payoff is **yield per worker**, delivered for free by the existing
/// [`herd_ecology`] seam — a tamed herd lives on the pastoral ecology (`r` = wild × `pastoral_gain`
/// 1.5), so the *same* hunters take ~1.5× the sustainable food from the same `K`. That is the "buy
/// freedom" thesis delivered granularly (surplus workers are freed for other tasks) instead of as a
/// binary "pastoral = zero workers", and it is what keeps the pen's **investment dip** a real cost:
/// with no passive rung there is no second payment for the same animals to stack on it, so the
/// `worked_this_turn` no-double-pay flag is gone with the payout it guarded.
///
/// **Corral (Rung 1c).** A **corralled** herd's keeper harvests the *pen's* MSY place-locally
/// (`advance_labor_allocation`); this pass runs its feed check and the shared shed. Logistics runs
/// before Population, so both flags were written **last** turn (the deliberate one-turn lag, mirroring
/// `ForagePatch::tended_this_turn`):
/// - **Under-contained → shed, ONCE THE GRACE IS SPENT** (§2.2). Too few keepers (or none) sheds
///   animals over the labor capacity into the wild web, per-rung rate (`pen_escape_fraction` <
///   `pastoral_escape_fraction` — the fence buys time). The pen is lost only on shed-to-zero, not on
///   the first untended turn.
///
///   **The shed no longer bites on the first under-herded turn.** [`Herd::neglect_turns`] counts
///   consecutive turns the herd's keepers failed to hold it, and animals leave only while that
///   exceeds the herd's rung's `grace_turns` ([`RungDef::neglect_grace_turns`] — `animal:pen`'s for a
///   penned herd, `animal:pastoral`'s otherwise). The plant twin is the same counter gating the feral
///   bleed in [`crate::forage::advance_cultivation`]: one trigger, two penalties.
///
///   **The under-herded NOTICE is deliberately not gated on the grace** — it fires on the turn the
///   herd genuinely becomes under-contained, which is precisely the window in which the player can
///   still send hands and lose nothing. Warning only once the animals are already leaving would spend
///   the grace on silence.
/// - **A keeper who cannot pay the feed → starvation** (unchanged, §2.5). An underfed pen
///   (`pen_fed_fraction < 1`) **shrinks** by `pen.starve_shrink_rate × (1 − fed) × biomass`, floored at
///   `pen.ecology.extinction_floor × K_pen`. It does **not** despawn and does **not** lose the pen: the
///   herd withers to a remnant and **recovers when fed again** — a recoverable famine (edge-gated feed
///   line on the first starving turn). Starving your animals to feed your people is a *decision*.
///
/// The animal mirror of `forage::advance_cultivation`'s feral pass.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_husbandry(
    mut registry: ResMut<HerdRegistry>,
    config: Res<SimulationConfig>,
    fauna_config: Res<FaunaConfigHandle>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    // The shed jitter draws from the world seed stream (deterministic under rollback), so this pass
    // now needs the seed + grid geometry + the tiles, exactly like `advance_herds`.
    world_seed: Option<Res<WorldGenSeed>>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    // The neglect grace is a ladder dial (the rung's `build.grace_turns`), read here for the same
    // reason `advance_cultivation` reads it: the penalty differs per web, the trigger does not.
    ladder_config: Res<LadderConfigHandle>,
) {
    let fauna = fauna_config.get();
    let ladder = ladder_config.get();
    let width = config.grid_size.x.max(1);
    let height = config.grid_size.y.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    // Mirrors `advance_herds`' seeding: `map_seed ^ tick`, per-herd salted below.
    let base_seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed) ^ tick.0;

    // **Phase 1 — shed.** While we hold `&mut` on each managed herd we reduce its biomass in place and
    // record where the escapees came from; we cannot mutate/add OTHER herds mid-iteration, so the
    // placement of that biomass into the wild web is a second pass (Phase 2) over the whole registry.
    let mut shed_events: Vec<ShedEvent> = Vec::new();
    // Herds that bled out entirely this turn (fully abandoned, shed below one animal) — despawned in
    // Phase 3, after placement, so the Phase-2 source indices stay valid.
    let mut despawn_ids: Vec<String> = Vec::new();
    for (source_index, herd) in registry.herds.iter_mut().enumerate() {
        // **Stabilize the herder requirement** (slice: herder hysteresis) — once per turn, before the
        // labor arm (Population, next stage) reads it via `herd_herders_needed`. A wild herd stays `0`;
        // a managed one moves up immediately but down only past a deadband, so a Sustain-hunted herd
        // breathing ±1 animal across an `animals_per_herder` multiple doesn't flicker the requirement
        // (and with it the `herded_fraction` / tameness). See `Herd::stabilize_herders_needed`.
        let animals_per_herder = fauna.animals_per_herder_for(&herd.species);
        herd.stabilize_herders_needed(
            animals_per_herder,
            animals_per_herder * fauna.husbandry.herders_hysteresis_fraction,
        );
        // The `tamed_this_turn` flag is still cleared each turn so it can never go stale — but its one
        // consumer, the retired tameness decay, is GONE (`docs/plan_fauna_neglect_escape.md` §2.1):
        // `domestication_progress` is monotone-up now, never bled by neglect.
        herd.tamed_this_turn = false;
        // **How well the herd was STAFFED last turn** (slice 8) — the same Population→Logistics lag as
        // `pen_fed_fraction`, read here and reset so it can never go stale. A herd nobody worked reads
        // the `0.0` its keeper never wrote, which is exactly right: no crew, no herding.
        let herded_last_turn = herd.herded_fraction;
        herd.herded_fraction = if herd.is_corralled() || herd.owner.is_some() {
            NOT_HERDED
        } else {
            FULLY_HERDED
        };
        // **Only a MANAGED herd sheds / feeds.** A wild herd is nobody's to keep, so it neither pays
        // a larder bill nor loses animals to under-containment — it simply roams. (Same scope the
        // retired tameness decay used: `is_corralled() || owner.is_some()`, never `is_domesticated()`,
        // which is a `>= 1.0` threshold that would drop a herd out of the managed set the moment it
        // dipped.)
        if !(herd.is_corralled() || herd.owner.is_some()) {
            continue;
        }

        // **FEED (§2.5).** A penned herd whose keeper tended it last turn pays — or fails to pay — its
        // larder bill. This is orthogonal to *herding*: a pen can be fully fed AND under-contained in
        // the same turn (a keeper who can pay, but with too few hands to hold the whole flock).
        // `starve_underfed_pen`, `herders_needed`, and the hysteresis are untouched by this arc.
        if herd.is_corralled() {
            if herd.corralled_tended_this_turn {
                herd.corralled_tended_this_turn = false;
                starve_underfed_pen(herd, &fauna, &mut event_log, tick.0);
            } else {
                // **No keeper → unfed** (§2.4). Stamping `NOT_FED` zeroes next turn's `regrow_biomass`,
                // which is what lets the shed actually drive an abandoned pen to zero: a fast breeder's
                // regrowth would otherwise cancel the ~10%/turn shed and the pen would leak strays
                // forever. Distinct from a keeper who cannot pay (that runs `starve_underfed_pen`,
                // unchanged) — nobody is bringing food at all.
                herd.pen_fed_fraction = PEN_NOT_FED;
            }
        }

        // **SHED (the arc, §2.2).** An under-contained managed herd sheds whole animals over its labor
        // capacity into the wild web — the one mechanic that replaced BOTH the tameness-bleed and the
        // binary corral escape. The jitter draws from a per-herd seeded RNG (`map_seed ^ tick ^ salt ^
        // fnv(id)`, the `advance_herds` recipe) so it is deterministic under rollback.
        let mut rng = {
            let mut hasher = FnvHasher::new();
            herd.id.hash(&mut hasher);
            SmallRng::seed_from_u64(base_seed ^ ESCAPE_SEED_SALT ^ hasher.finish())
        };
        //
        // **The overage is measured first and the shed is gated second**, because the two answer
        // different questions: *is this herd under-contained* (which drives the notice, and must be
        // true during the grace — that is when the player can still fix it) versus *do animals leave
        // this turn* (which the grace suppresses).
        let under_contained = uncontained_overage(herd, herded_last_turn, &fauna).is_some();
        // **The neglect counter** — the animal twin of `ForagePatch::neglect_turns`. A herd whose
        // keepers can hold it is forgiven outright, so the grace measures *consecutive* neglect.
        if under_contained {
            herd.neglect_turns = herd.neglect_turns.saturating_add(1);
        } else {
            herd.neglect_turns = NEGLECT_NONE;
        }
        // The rung whose keeping obligation this herd is under, through the one seam the wire's
        // countdown reads too ([`herd_keeping_rung`]).
        let grace = herd_keeping_rung(herd, &ladder)
            .map_or(NO_NEGLECT_GRACE, |rung| rung.neglect_grace_turns());
        if u32::from(herd.neglect_turns) > grace {
            if let Some(event) =
                shed_uncontained_animals(herd, source_index, herded_last_turn, &fauna, &mut rng)
            {
                shed_events.push(event);
            }
        }

        // **UNDER-HERDED EDGE NOTICE (slice 2, `docs/plan_fauna_neglect_escape.md` §4 item 1).** Fire a
        // command-feed line the turn a managed herd **becomes** under-contained — edge-gated on the
        // persisted `under_herded` bool so it fires **once** on the `false → true` transition, not every
        // turn it stays under-contained, and re-fires after a recovery + relapse. Cleared the turn the
        // herd is no longer shedding (fully staffed / within capacity). Distinct from the pen-*lost*
        // (`announce_pen_lost`) and pen-*starving* (`starve_underfed_pen`) edges — this is the
        // herder-shortfall edge, and it fires for pastoral herds too. The precedent is the pen-starving
        // edge gate (`Herd::pen_starving`); like it, this bool rewinds with a rollback rather than
        // re-firing the notice.
        if under_contained {
            if !herd.under_herded {
                herd.under_herded = true;
                if let Some(owner) = herd.owner {
                    let pos = herd.position();
                    event_log.push(CommandEventEntry::new(
                        tick.0,
                        CommandEventKind::HerdUnderHerded,
                        owner,
                        format!(
                            "The {} has too few herders — animals are drifting off",
                            herd.species
                        ),
                        Some(format!(
                            "status=under_herded herded={:.2} needed={} herd={} x={} y={}",
                            herded_last_turn, herd.herders_needed, herd.id, pos.x, pos.y
                        )),
                    ));
                }
            }
        } else {
            herd.under_herded = false;
        }

        // **BLEED-OUT ON TOTAL ABANDONMENT (§2.4).** A herd with ZERO herders last turn
        // (`herded_fraction == NOT_HERDED`) keeps shedding — regrowth already suppressed
        // (`regrow_biomass`), and for a pen unfed above — until it can no longer shed a whole animal
        // (`biomass < body_mass`). At that point it has **bled out entirely** into the wild web, so the
        // managed entity is **despawned** (Phase 3, after placement), NOT left as an ownerless-but-tame
        // husk. Ownership and the pen/fence state are **never cleared at a floor**: clearing `owner`
        // would drop the herd out of the managed set (`is_corralled() || owner.is_some()`) and stop the
        // shed, stranding exactly the husk this eliminates — so it stays owned/corralled and bleeds all
        // the way down, then vanishes with its (now-empty) fence. Tameness is never reset — it leaves
        // with the animals (each shed batch becomes a wild herd at domestication 0). Distinct from FEED
        // starvation, which floors a *fed* pen and keeps it (`starve_underfed_pen`, §2.5): a starving
        // pen has a keeper (`herded_fraction > 0`), so it never reaches this branch — only animals
        // *leaving* empty a herd.
        let body_mass = herd.body_mass;
        if herded_last_turn <= NOT_HERDED && body_mass > 0.0 && herd.biomass < body_mass {
            if herd.is_corralled() {
                // The pen dies with the entity — no fence reset needed (it despawns too). Announce it so
                // pen destruction is never silent.
                announce_pen_lost(herd, &mut event_log, tick.0);
            }
            despawn_ids.push(herd.id.clone());
        }
    }

    // **Phase 2 — place the shed biomass into the WILD web (§2.3).** Merge over the tile + adjacent
    // ring into a same-species wild herd, else spawn a fresh wild herd on an adjacent land tile.
    place_shed_animals(
        &mut registry,
        &shed_events,
        &fauna,
        base_seed,
        tick.0,
        width,
        height,
        wrap,
        &tile_registry,
        &tiles,
    );

    // **Phase 3 — despawn the herds that bled out (§2.4).** Their biomass is already in the wild web
    // via the shed, so the empty managed entity is removed. After placement so the Phase-2 source
    // indices stayed valid.
    if !despawn_ids.is_empty() {
        registry
            .herds
            .retain(|herd| !despawn_ids.contains(&herd.id));
    }
}

/// The escapees recorded by [`shed_uncontained_animals`] during Phase 1 of `advance_husbandry`, placed
/// into the wild web in Phase 2 ([`place_shed_animals`]). Carries the source species' cached traits so
/// the wild herd it merges into / spawns is byte-identical to a naturally-spawned one of that species.
struct ShedEvent {
    species: String,
    size_class: SizeClass,
    /// The managed herd's own tile — the merge/spawn centres on it and its adjacent ring.
    from_pos: UVec2,
    escaped_biomass: f32,
    fodder_per_biomass: f32,
    regrowth_rate: f32,
    body_mass: f32,
    husbandry_ceiling: HusbandryCeiling,
    /// Index of the herd that shed, excluded from being its own merge target (a herd that shed to zero
    /// and had its owner cleared is now a sub-viability wild residual at `from_pos` — merging the
    /// escapees back into it would defeat the drift-out).
    source_index: usize,
}

/// **The shared "animals leave" mechanic** (`docs/plan_fauna_neglect_escape.md` §2.2/§3.2) — an
/// under-contained managed herd sheds whole animals over its labor capacity into the wild web. It
/// replaced BOTH `decay_under_herded` (the tameness-bleed) and the binary corral escape: neglect now
/// costs the **visible** axis (herd size), never the invisible one (`domestication_progress`, which is
/// monotone-up and never touched here).
///
/// The overage is the herd's **actual** count over what its keepers can hold, reconstructed from the
/// real staffing: `capacity_animals = herded_fraction × herders_needed × animals_per_herder` (the
/// `herded_fraction × needed` product recovers `assigned` exactly, since `herded_fraction =
/// min(1, assigned/needed)`), and `overage_animals = max(0, current − capacity)`. **NOT** the
/// `(1 − herded_fraction) × current` shorthand (a review-caught spec bug): that over-estimates near a
/// `ceil` boundary because it assumes `current ≈ needed × animals_per_herder`, which is false when
/// `herders_needed = ceil(animals/aph)` rounds up hard — 101 animals @ aph 50 staffed at 2 has a true
/// overage of **1** (`101 − 2×50`), but the shorthand reads `0.333 × 101 = 33.7` and sheds ~8/turn.
/// A **fraction of the OVERAGE** leaves, not of the total, so as the herd shrinks toward its capacity
/// fewer leave and it **stops exactly** at `overage < 1` — no overshoot below the real labor capacity,
/// and none to zero unless capacity is `0` (total abandonment, the `herders_needed`-and-`herded == 0`
/// limit). The count is in **whole animals**, with a **min-1 floor** when the overage is `≥ 1` so a
/// small overage clears instead of asymptoting one or two over forever.
///
/// `herders_needed` is read through [`herd_herders_needed`] (the stabilized field, falling back to the
/// raw `ceil` for a not-yet-stabilized managed herd) so a `0` can never collapse capacity to zero and
/// shed a fully-staffed fresh herd. `stabilize_herders_needed` runs earlier in `advance_husbandry`, so
/// for a managed herd the stabilized value is already `> 0` by the time the shed reads it.
///
/// The rate is **per-rung**: `pen_escape_fraction` for a corralled herd (slower — the fence),
/// `pastoral_escape_fraction` otherwise, each `× (1 + jitter)` from the caller's seeded RNG. Reduces
/// this herd's biomass and returns the placement event, or `None` when nothing leaves this turn.
fn shed_uncontained_animals(
    herd: &mut Herd,
    source_index: usize,
    herded_last_turn: f32,
    fauna: &FaunaConfig,
    rng: &mut SmallRng,
) -> Option<ShedEvent> {
    let body_mass = herd.body_mass;
    let overage_animals = uncontained_overage(herd, herded_last_turn, fauna)?;
    let husbandry = &fauna.husbandry;
    let rate = if herd.is_corralled() {
        husbandry.pen_escape_fraction
    } else {
        husbandry.pastoral_escape_fraction
    };
    let jitter_band = husbandry.escape_fraction_jitter;
    let jitter = if jitter_band > 0.0 {
        rng.gen_range(-jitter_band..=jitter_band)
    } else {
        0.0
    };
    let jittered = (rate * (1.0 + jitter)).max(0.0);
    // Whole animals, min-1 floor (the overage is `>= 1` here, so at least one head always clears).
    let leaving = (jittered * overage_animals).floor().max(MIN_ESCAPE_ANIMALS);
    let escaped_biomass = (leaving * body_mass).min(herd.biomass);
    if escaped_biomass <= 0.0 {
        return None;
    }
    herd.biomass = (herd.biomass - escaped_biomass).max(0.0);
    herd.refresh_ecology_phase(fauna);
    Some(ShedEvent {
        species: herd.species.clone(),
        size_class: herd.size_class,
        from_pos: herd.position(),
        escaped_biomass,
        fodder_per_biomass: herd.fodder_per_biomass,
        regrowth_rate: herd.regrowth_rate,
        body_mass,
        husbandry_ceiling: herd.husbandry_ceiling,
        source_index,
    })
}

/// **Which rung's keeping obligation this herd is under** — `animal:pen` for a penned herd,
/// `animal:pastoral` for any other managed one, `None` for a wild herd (nobody's to keep, so it never
/// sheds and has no grace to spend).
///
/// **Deliberately not [`herd_rung`]**, which answers which rung the herd has *completed*: a half-tamed
/// herd is already owned and already sheds, and reading `animal:wild` there — a rung with no build and
/// therefore no `grace_turns` — would hand the herd in the middle of a 25-turn investment the least
/// forgiveness on the whole ladder.
///
/// **One seam, two readers**, the twin of `forage::patch_unwinding_rung`: `advance_husbandry` gates
/// the shed on this rung's grace and the snapshot publishes *that* rung's countdown.
pub fn herd_keeping_rung<'a>(herd: &Herd, ladder: &'a LadderConfig) -> Option<&'a RungDef> {
    (herd.is_corralled() || herd.owner.is_some()).then(|| {
        ladder.rung(if herd.is_corralled() {
            RungKey::AnimalPen
        } else {
            RungKey::AnimalPastoral
        })
    })
}

/// **Turns of neglect this herd can still absorb before its keepers start losing animals** — the wire's
/// countdown, resolved through [`herd_keeping_rung`] so it always describes the rung
/// [`advance_husbandry`] actually gates the shed on. `None` = a wild herd, with nothing at risk.
pub fn herd_neglect_grace_remaining(herd: &Herd, ladder: &LadderConfig) -> Option<u32> {
    herd_keeping_rung(herd, ladder).map(|rung| {
        crate::intensification::neglect_grace_remaining(
            herd.neglect_turns,
            rung.neglect_grace_turns(),
        )
    })
}

/// **How many whole animals this herd's keepers cannot hold** — the measurement half of
/// [`shed_uncontained_animals`], split out because *being* under-contained and *losing animals for it*
/// are two questions with two answers once a neglect grace sits between them: the under-herded feed
/// notice fires on the first, the shed only on the second.
///
/// `None` means the herd fits its labor capacity (or is within one animal of it) — the self-limiting
/// attractor — or has no measurable stock at all. See [`shed_uncontained_animals`] for why the
/// capacity is reconstructed from `herded_fraction × herders_needed × animals_per_herder` rather than
/// the `(1 − herded_fraction) × current` shorthand.
fn uncontained_overage(herd: &Herd, herded_last_turn: f32, fauna: &FaunaConfig) -> Option<f32> {
    let body_mass = herd.body_mass;
    if body_mass <= 0.0 || herd.biomass <= 0.0 {
        return None;
    }
    let current_animals = herd.biomass / body_mass;
    // **Reconstruct the real labor capacity from actual staffing** (review fix — see the doc comment):
    // `capacity = assigned × animals_per_herder`, with `assigned = herded_fraction × herders_needed`.
    // A fully-staffed herd reads `herded == 1` ⇒ `capacity = needed × aph ≥ current` ⇒ no shed; a herd
    // nobody worked reads `herded == 0` ⇒ `capacity = 0` ⇒ the whole flock is overage.
    let animals_per_herder = fauna.animals_per_herder_for(&herd.species);
    let needed = herd_herders_needed(herd, fauna) as f32;
    let capacity_animals = herded_last_turn.max(0.0) * needed * animals_per_herder;
    let overage_animals = (current_animals - capacity_animals).max(0.0);
    (overage_animals >= MIN_ESCAPE_ANIMALS).then_some(overage_animals)
}

/// **Announce a lost pen** (`docs/plan_fauna_neglect_escape.md` §2.4). A fully-abandoned pen has shed
/// its last animal and the managed entity is about to be despawned (its biomass is already in the wild
/// web via the shed), so there is **no fence state to reset** — it goes with the entity. Pushes the same
/// `CommandEventKind::Corral` line the pen's *completion* and old escape pushed — one kind for the pen's
/// whole life — so pen destruction is **never silent**. Reads `&Herd` (the caller despawns it next).
fn announce_pen_lost(herd: &Herd, event_log: &mut CommandEventLog, tick: u64) {
    info!(
        target: "shadow_scale::analytics",
        event = "corral_escape",
        herd = %herd.id,
        faction = herd.owner.map(|f| f.0).unwrap_or_default(),
    );
    if let Some(owner) = herd.owner {
        let (pen_x, pen_y) = herd.corralled_at.map(|t| (t.x, t.y)).unwrap_or_default();
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Corral,
            owner,
            format!(
                "The {} herd has drifted off — untended, the pen is lost",
                herd.species
            ),
            Some(format!(
                "status=escaped reason=untended action=corral herd={} x={} y={}",
                herd.id, pen_x, pen_y
            )),
        ));
    }
}

/// **Phase 2 of the shed** (`docs/plan_fauna_neglect_escape.md` §2.3): resolve each [`ShedEvent`] into
/// the wild web, over the full registry (which Phase 1 could not touch while iterating it mutably).
/// Merge over the tile + adjacent ring into a same-species wild herd, else spawn a fresh wild herd on
/// an adjacent land tile, falling back to the source tile only if hemmed in.
#[allow(clippy::too_many_arguments)]
fn place_shed_animals(
    registry: &mut HerdRegistry,
    events: &[ShedEvent],
    fauna: &FaunaConfig,
    base_seed: u64,
    tick: u64,
    width: u32,
    height: u32,
    wrap: bool,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
) {
    if events.is_empty() {
        return;
    }
    // Every herd that shed is excluded from being a merge target: a still-managed source won't match
    // the wild filter anyway, and a shed-to-zero source is a dying residual we must not merge back into.
    let source_indices: HashSet<usize> = events.iter().map(|e| e.source_index).collect();
    for (seq, event) in events.iter().enumerate() {
        // 1. MERGE over the tile + adjacent ring — reinforce an existing wild population of the same
        //    species instead of proliferating herds (which also sidesteps `abundance.max_total_game`).
        if let Some(target) =
            nearest_wild_merge_target(registry, event, &source_indices, width, wrap)
        {
            registry.herds[target].biomass += event.escaped_biomass;
            registry.herds[target].refresh_ecology_phase(fauna);
            continue;
        }
        // 2/3. ELSE spawn a fresh wild herd on an adjacent land tile (fall back to the source tile only
        //      if every neighbour is water/off-map). Seeded per event so the drift-out is reproducible.
        let mut rng = {
            let mut hasher = FnvHasher::new();
            event.species.hash(&mut hasher);
            event.from_pos.x.hash(&mut hasher);
            event.from_pos.y.hash(&mut hasher);
            SmallRng::seed_from_u64(base_seed ^ ESCAPE_SEED_SALT ^ hasher.finish() ^ seq as u64)
        };
        let spawn_pos = pick_adjacent_land(
            event.from_pos,
            width,
            height,
            wrap,
            tile_registry,
            tiles,
            &mut rng,
        )
        .unwrap_or(event.from_pos);
        if let Some(herd) = spawn_feral_group(
            event,
            spawn_pos,
            fauna,
            tick,
            seq,
            width,
            height,
            tile_registry,
            tiles,
            &mut rng,
        ) {
            registry.herds.push(herd);
        }
    }
}

/// The nearest **wild** herd of the event's species on its tile or the adjacent ring — the merge
/// target for the shed (`docs/plan_fauna_neglect_escape.md` §2.3 step 1). "Wild" = unowned,
/// uncorralled, zero domestication; the source herd (and any other shedder) is excluded. Ordered
/// `(hex distance ASC, y ASC, x ASC, index ASC)` so the pick is total and hasher-independent.
fn nearest_wild_merge_target(
    registry: &HerdRegistry,
    event: &ShedEvent,
    source_indices: &HashSet<usize>,
    width: u32,
    wrap: bool,
) -> Option<usize> {
    registry
        .herds
        .iter()
        .enumerate()
        .filter(|(i, h)| {
            !source_indices.contains(i)
                && h.owner.is_none()
                && !h.is_corralled()
                && h.domestication_progress == 0.0
                && h.species == event.species
                && hex_distance_wrapped(h.position(), event.from_pos, width, wrap) <= 1
        })
        .min_by(|(ai, ah), (bi, bh)| {
            let da = hex_distance_wrapped(ah.position(), event.from_pos, width, wrap);
            let db = hex_distance_wrapped(bh.position(), event.from_pos, width, wrap);
            da.cmp(&db)
                .then_with(|| ah.position().y.cmp(&bh.position().y))
                .then_with(|| ah.position().x.cmp(&bh.position().x))
                .then_with(|| ai.cmp(bi))
        })
        .map(|(i, _)| i)
}

/// Pick an adjacent **land** tile to drift the escapees onto (`docs/plan_fauna_neglect_escape.md` §2.3
/// step 2), wrap-aware. Deterministic: land neighbours are sorted `(y, x)` then a seeded draw picks
/// one, so the drift-out is reproducible under rollback. `None` = hemmed in (every neighbour
/// water/off-map), which the caller turns into the same-tile fallback (step 3).
fn pick_adjacent_land(
    from: UVec2,
    width: u32,
    height: u32,
    wrap: bool,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<UVec2> {
    let mut candidates: Vec<UVec2> = hex_neighbors_wrapped(from.x, from.y, width, height, wrap)
        .map(|(x, y)| UVec2::new(x, y))
        .filter(|p| is_land_tile(*p, tile_registry, tiles))
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by_key(|p| (p.y, p.x));
    Some(candidates[rng.gen_range(0..candidates.len())])
}

/// Construct the fresh **wild** herd a shed spawns (`docs/plan_fauna_neglect_escape.md` §2.3 step 2),
/// carrying the escapees' biomass at `owner = None` / `domestication_progress = 0` / `corralled_at =
/// None` — a fresh wild group whatever its origin stock. Reuses the source species' cached traits and
/// the same `build_short_route` land-neighbour path `spawn_game_group_at` uses, so it is byte-identical
/// to a naturally-spawned herd of that species. **Exempt from `abundance.max_total_game`** (§5 item 2)
/// and carries the [`FERAL_ID_PREFIX`] the immigration cap scan skips.
#[allow(clippy::too_many_arguments)]
fn spawn_feral_group(
    event: &ShedEvent,
    spawn_pos: UVec2,
    fauna: &FaunaConfig,
    tick: u64,
    seq: usize,
    width: u32,
    height: u32,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Herd> {
    let def = fauna.species_by_display(&event.species);
    let steps = def.map(|d| d.sample_route_len(rng)).unwrap_or(1);
    let route = build_short_route(spawn_pos, steps, width, height, tile_registry, tiles, rng)
        .unwrap_or_else(|| vec![spawn_pos]);
    // A wild herd's `K` is recomputed from its range by `advance_herds` next turn; seed it with the
    // species' own wild carrying capacity so the interim readout is sane.
    let carrying_capacity = def
        .map(|d| d.carrying_capacity())
        .unwrap_or(event.escaped_biomass.max(1.0));
    let id = format!("{FERAL_ID_PREFIX}{tick}_{seq}");
    let mut herd = Herd::new(
        id,
        event.species.clone(),
        event.size_class,
        route,
        event.escaped_biomass,
        carrying_capacity,
        event.fodder_per_biomass,
        event.regrowth_rate,
        event.body_mass,
    );
    herd.husbandry_ceiling = event.husbandry_ceiling;
    herd.refresh_ecology_phase(fauna);
    Some(herd)
}

/// **A keeper who cannot pay the feed starves the herd.** Reads the `pen_fed_fraction` its keeper
/// wrote last turn (Population → Logistics, the deliberate one-turn lag) and, if the pen went hungry,
/// shrinks it by `starve_shrink_rate × (1 − fed) × biomass` — floored at
/// `pen.ecology.extinction_floor × K_pen`.
///
/// The herd **withers to a remnant and recovers when fed again**: it does not despawn (a penned herd
/// cannot disperse — see `advance_herds`' retention) and it does not lose the pen. Deliberate:
/// recoverable starvation is better play than silently voiding a 25-turn investment, and it keeps this
/// out of the escape/despawn paths entirely. The famine is announced **once**, on the turn it starts
/// (`pen_starving` edge-gates the feed line), so it is never silent and never spam.
///
/// Resets `pen_fed_fraction` to [`PEN_FULLY_FED`] after reading — the flag is a one-turn signal, so a
/// pen whose keeper walks off is handled by the *escape* branch, not by a stale starvation value.
fn starve_underfed_pen(
    herd: &mut Herd,
    fauna: &FaunaConfig,
    event_log: &mut CommandEventLog,
    tick: u64,
) {
    let fed = herd.pen_fed_fraction.clamp(0.0, PEN_FULLY_FED);
    herd.pen_fed_fraction = PEN_FULLY_FED;
    if fed >= PEN_FULLY_FED {
        // Fed again → a later famine is announced afresh.
        herd.pen_starving = false;
        return;
    }
    let pen = &fauna.husbandry.pen;
    let floor = pen.ecology.extinction_floor * herd_capacity(herd, fauna);
    let shrink = pen.starve_shrink_rate * (PEN_FULLY_FED - fed) * herd.biomass;
    herd.biomass = (herd.biomass - shrink).max(floor);
    herd.refresh_ecology_phase(fauna);
    info!(
        target: "shadow_scale::analytics",
        event = "pen_starving",
        herd = %herd.id,
        faction = herd.owner.map(|f| f.0).unwrap_or_default(),
        fed_fraction = fed,
        biomass = herd.biomass,
    );
    // Edge-gated: announce the famine on the turn it starts, not every turn it continues. A shrinking
    // herd whose yield is quietly falling must never be a mystery.
    if herd.pen_starving {
        return;
    }
    herd.pen_starving = true;
    if let Some(owner) = herd.owner {
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Corral,
            owner,
            format!(
                "The {} herd is starving — the pen has no feed",
                herd.species
            ),
            Some(format!(
                "status=starving fed={fed:.2} action=corral herd={}",
                herd.id
            )),
        ));
    }
}

/// Pre-commit **yield forecast** for one worked source (a herd or a forage patch), as the client
/// needs it to show "Expected yield: +X.XX /turn" and cap its worker stepper *while the player is
/// composing an assignment* — before anything is committed (the `SourceYield` telemetry is
/// post-hoc). Every field is **provisions (food) per turn** at the source's CURRENT biomass, with
/// the caller's `output_multiplier` already folded in (the snapshot exports it at `1.0`, so the
/// client scales by the band's `outputMultiplier` — a linear factor on every field, which leaves
/// `max_useful_workers` invariant).
///
/// The consumer composes:
/// - `expected(workers, policy) = min(workers × per_worker_yield, ceiling(policy))`
/// - `max_useful_workers(policy) = ceil(ceiling(policy) / per_worker_yield)`
///
/// Each `ceiling_*` is the stance's **pre-clamp** offer and [`SourceYieldForecast::stock_cap`] is
/// the standing stock it cannot exceed; [`SourceYieldForecast::ceiling_for`] applies the clamp, so
/// what a reader gets IS the take the sim pays. **The clamp is a lookup, not a stored row, because
/// the DIP has to land inside it** — see [`SourceYieldForecast::ceiling_under`].
/// **Forecast == actual is an invariant**: the forecast and
/// the take path (`hunt_take` / `forage::forage_take`) share the same ceiling + conversion helpers
/// ([`hunt_escapement_ceiling`] × the species' `HuntYield`,
/// `forage::forage_escapement_ceiling`/`forage_provisions`) — never duplicate the formulas, or the UI
/// will lie.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceYieldForecast {
    /// **Every field is a [`YieldAccounts`] — food AND trade goods per turn, never a food scalar**
    /// (`docs/plan_hunt_yield_model.md`, issue #337).
    ///
    /// **Why vectorised rather than sibling `*_trade` scalars.** A wolf's food ceilings are all `0`,
    /// so a food-denominated forecast cannot express its yield *at all* — the client would read
    /// "0/turn" on every rung and the forecast would be **false**, not merely incomplete. Sibling
    /// scalars would double the surface and let the two halves drift apart under a retune; one pair
    /// per rung cannot, because `ceiling_for` hands both components to every reader at once.
    ///
    /// **The forage side fills `.trade_goods = 0.0` throughout** — see `forage::forage_forecast`.
    /// That is a known gap (the plant web's Deplete gather *does* sell), not a regression: the
    /// forecast carried no trade at all before this arc. The client renders a trade line **only when
    /// `trade_goods > 0`** — flora's cash-crop rule — so a plant shows no trade line rather than a
    /// false "0".
    ///
    /// Food/turn one worker contributes at this source (throughput → provisions), before the policy
    /// ceiling binds. `0.0` means no worker can extract anything this turn (e.g. a zero seasonal
    /// weight) — consumers must not divide by it; ask [`SourceYieldForecast::ratio_axis`] instead.
    pub per_worker_yield: YieldAccounts,
    /// **The source's standing biomass**, and [`SourceYieldForecast::carrying_capacity`] beside it —
    /// the two terms an escapement ceiling is made of. Ignored on a managed source.
    ///
    /// **The forecast stores the TERMS, not a fixed set of rows, because the floor is continuous.**
    /// Four `ceiling_sustain`/`surplus`/`deplete`/`eradicate` fields could answer four questions; a
    /// player dragging a floor asks a different one every frame, and a row-per-answer surface can
    /// only be extended by adding rows that then have to be kept in step with the take. One
    /// computation ([`SourceYieldForecast::ceiling_at`]) answers every floor and cannot drift from
    /// itself.
    pub biomass: f32,
    /// The source's carrying capacity — the `K` a floor is a fraction *of*. Ignored on a managed
    /// source.
    pub carrying_capacity: f32,
    /// **What ONE UNIT of this source's biomass is worth, in every account** — the patch's
    /// basket-averaged `provisions_per_biomass`, or the herd's [`crate::fauna_config::HuntYield`]
    /// vector, with the caller's `output_multiplier` already folded in exactly as it is into every
    /// other field here.
    ///
    /// It is the conversion half of `ceiling = room × rate`, kept as a *rate* rather than baked into
    /// pre-multiplied rows so that a ceiling at an arbitrary floor is one multiplication rather than
    /// a lookup that does not exist.
    pub per_biomass_yield: YieldAccounts,
    /// **`Some` on a rung-3 MANAGED source** (a Pen or a Field) — the production it hands over,
    /// which is what makes the floor axis honestly collapse there: the source is *yours*, you
    /// control its reproduction, and there is no wild stock left to stop short of. `None` on every
    /// drawn-down source, whose ceiling is the escapement room above the caller's floor.
    ///
    /// It replaced `stock_cap: Option<..>`, which carried the same discriminator (`None` = managed)
    /// alongside a clamp that an escapement ceiling can no longer need: `B − floor·K ≤ B` for any
    /// floor `≥ 0`, so the standing stock cannot bind. The clamp survives inside
    /// [`SourceYieldForecast::ceiling_at`] as belt-and-braces, derived from `biomass` rather than
    /// stored beside it, so there is no second statement of the same number to fall out of step.
    pub managed_production: Option<YieldAccounts>,
    /// **The two build dips of this source's web** — each rung's `yield_fraction_while_building`,
    /// carried here so a reader can price a build without holding the ladder
    /// ([`SourceYieldForecast::ceiling_under`]).
    ///
    /// **This replaced the three flat `ceiling_prepare` / `ceiling_tame` / `ceiling_sow` fields**
    /// (issue #442). Each of those was a *fifth ceiling row* — the rung's fraction applied to the
    /// **Sustain** ceiling and nothing else — which was only expressible because a build verb *was*
    /// the policy, so a builder could be in no other stance. With the axes split the dip is a factor
    /// on whichever stance the player holds, so it is stored as the factor and applied to the four
    /// rows that remain. Two rungs still keep two independently tunable numbers, which is the reason
    /// `ceiling_tame` and `ceiling_sow` were split out in the first place.
    pub build_dips: BuildDips,
    /// Food/turn the source pays **once the improvement completes** — the tended-patch harvest
    /// (`tended_provisions`), or, for an **un-penned** herd, the pen's **sustained MSY** projected on
    /// the pen ecology (`sustainable_yield` at the pen `r`, the long-run rate that shows the ladder).
    /// A **penned** herd instead reads its actual constant-escapement corral take (`corral_yield`,
    /// via the `is_corralled()` early-return in `hunt_forecast`), so forecast == actual there. Lets
    /// the client show the payoff ("preparing X → then Y") *before* the player commits to the dip.
    /// Crosses the wire as `ForagePatchState.tendedYield` / `HerdTelemetryState.corralYield`.
    pub managed_yield: YieldAccounts,
    /// Food/turn a herd pays **once tamed** — the **Tame rung's payoff**: the pastoral **sustained
    /// MSY** at the herd's current biomass (`sustainable_yield` at the pastoral `r`). The pastoral
    /// analog of [`SourceYieldForecast::managed_yield`]/`corralYield`: `ceiling_under(stance, Tame)`
    /// is Tame's *during-building dip*, this is what a Sustain hunt pays *after* the herd is tamed — so the
    /// client can render Tame as `→ +Y` (like Cultivate/Sow/Corral) instead of quoting only the dip.
    /// `0` on a source that never offers Tame: a forage patch (hunt-only verb), or a herd already
    /// penned or forage-tended. Crosses the wire as `HerdTelemetryState.pastoralYield`.
    pub pastoral_yield: YieldAccounts,
    /// **One animal's worth of yield** — `body_mass` through the same species vector every other
    /// field here uses — or **[`YieldAccounts::ZERO`] for a source that does not quantise**
    /// (intensification ladder slice 8).
    ///
    /// It is what makes the *preview* lumpy in the same places the take is: [`forecast_expected_take`]
    /// runs [`quantise_animal_take`] against it, so the client's "Expected yield" shows the same
    /// pulse (and the same 0 on a waiting turn) the sim will pay.
    ///
    /// **"Does this source quantise?" is now `!body_mass_yield.is_zero()`, not
    /// `body_mass_yield.provisions > 0`.** It read the provisions component until #337, and that test
    /// is wrong for a wolf: an inedible species' food quantum is `0`, so the old test would call a
    /// pack of wolves *continuous* and hand back a smooth fraction of a wolf. Whole animals are a
    /// property of the animal, never of what it happens to be worth to you. Every source that existed
    /// before this arc reads identically (plants are `ZERO` in both components; edible game is
    /// positive on provisions), so this is a widening, not a change.
    ///
    /// The animal COUNT is taken on [`SourceYieldForecast::ratio_axis`] — the first component with a
    /// positive rate — which is the operational form of `quantise_animal_take`'s
    /// **never-divide-by-a-food-number-you-have-not-established-is-positive** rule.
    ///
    /// **`ZERO` = continuous, deliberately, and it is the plant web's value.** You harvest grain by
    /// the handful; you cannot half-kill a deer. Quantisation is animal-only because *the products
    /// differ* — the same reason seed travels and a herd doesn't. Do not "fix" this into a plant body
    /// mass.
    pub body_mass_yield: YieldAccounts,
    /// **The species' engagement throughput** ([`SpeciesDef::engage_rate`]) — how many animals one
    /// hunter can bring into contact per turn, so the forecast bounds the take the same way
    /// `hunt_take` does (`docs/plan_hunt_through_combat.md` §2). Without it a preview would promise a
    /// take the party could never reach, which is exactly the forecast-vs-actual split
    /// `.claude/rules/core_sim/yield-forecast.md` forbids.
    ///
    /// **[`f32::INFINITY`] on a continuous source and on a pen** — a plant is not stalked and a penned
    /// animal is not either, so there is no engagement stage for either. Same shape as
    /// `body_mass_yield`, which is likewise inert on the plant web.
    pub engage_rate: f32,
    /// **The fight the take resolves through** (`docs/plan_hunt_through_combat.md` §4) — the party's
    /// per-hunter profile and the quarry's body, so the preview brings down exactly the animals the
    /// take will. Without it a forecast would promise a mammoth to a bare-handed party, which is the
    /// forecast-vs-actual split `.claude/rules/core_sim/yield-forecast.md` forbids.
    ///
    /// **`None` = there is no fight stage at all** — every plant source, and a pen (a penned animal is
    /// slaughtered, not stalked). The same statement `engage_rate: f32::INFINITY` makes beside it.
    pub fight: Option<(HuntingParty, QuarryFight)>,
}

/// [`SourceYieldForecast::pastoral_yield`] for a source that never offers the `Tame` verb — a forage
/// patch, or a herd already penned/forage-tended. `0` = *no Tame payoff to advertise*, the pastoral
/// twin of `PLANTS_DO_NOT_QUANTISE`.
pub(crate) const NO_PASTORAL_YIELD: YieldAccounts = YieldAccounts::ZERO;

/// The biomass a **per-unit rate** is the yield of — `1.0`, so `HuntYield::apply(ONE_UNIT_OF_BIOMASS)`
/// reads as *"what is one unit of this stock worth"* rather than as an unexplained `1.0` argument to
/// a function whose other callers pass a real take.
pub(crate) const ONE_UNIT_OF_BIOMASS: f32 = 1.0;

/// [`SourceYieldForecast::biomass`] / `carrying_capacity` for a **rung-3 managed** source, whose
/// harvest is a production rate rather than a draw on a standing stock. Named rather than a bare `0`
/// because the value is not a measurement — it is the statement that *there is no stock to stop
/// short of here*, which is exactly why `ceiling_at` ignores the floor on such a source.
pub(crate) const NO_STANDING_STOCK_TO_DRAW_DOWN: f32 = 0.0;

impl SourceYieldForecast {
    /// A **rung-3 managed source** — a corralled herd (a Pen) or a sown Field. The source is *yours*:
    /// you control its reproduction, so there is no wild stock left to over-skim and **the policy axis
    /// honestly collapses** — every ceiling is the one managed yield `production` it hands over.
    ///
    /// **`per_worker_yield` is the crew's REAL throughput, not the yield** (slice 7). It used to be
    /// `production` itself, which encoded "one worker collects everything the land offers": the
    /// client's `max_useful_workers = ceil(ceiling / per_worker_yield)` then fell out as a hardcoded
    /// `1` however rich the source, and `forecast_expected_take`'s `min` could never bind. Passing the
    /// throughput restores the **collection** half of production-vs-collection at the top rung —
    /// `min(workers × per_worker_yield, production)` — so a rich Field genuinely needs more hands and
    /// says how many. **The policy axis collapses at rung 3; the worker cap never does — you always
    /// have to carry the harvest home.**
    pub(crate) fn managed(
        production: YieldAccounts,
        per_worker_yield: YieldAccounts,
        body_mass_yield: YieldAccounts,
    ) -> Self {
        Self {
            per_worker_yield,
            body_mass_yield,
            // **A managed source is never drawn down**, so it has no escapement room to compute and
            // no floor to compute it at: `production` is both what it offers and all it offers,
            // whatever the player's dial says. The biomass terms are inert here and stated as such.
            biomass: NO_STANDING_STOCK_TO_DRAW_DOWN,
            carrying_capacity: NO_STANDING_STOCK_TO_DRAW_DOWN,
            per_biomass_yield: YieldAccounts::ZERO,
            managed_production: Some(production),
            // Nothing is left to build on a rung-3 source, so there is no dip to price — every
            // ceiling, dipped or not, is the managed yield it pays now. Honest *here*, unlike on a
            // rung-2 source, because there is genuinely nothing left to build: the plant web's
            // rung-2 patch takes the policy-live path instead (`forage::forage_forecast`).
            //
            // **`NOTHING_LEFT_TO_BUILD`, not the identity `1.0`** (PR #448 review): the two are the
            // same multiplier and *different facts*, and the wire has to be able to say the second
            // one. See `intensification::NO_BUILD_REMAINING_FRACTION`.
            build_dips: BuildDips::NOTHING_LEFT_TO_BUILD,
            // A penned animal is not stalked — no engagement stage, and so no fight either.
            engage_rate: f32::INFINITY,
            fight: None,
            managed_yield: production,
            // A rung-3 managed source (a Pen or a Field) is past taming — a penned herd never offers
            // the Tame verb — so it advertises no pastoral payoff.
            pastoral_yield: NO_PASTORAL_YIELD,
        }
    }

    /// **THE yield/turn cap this source pays at `floor`** — the one computation every reader of this
    /// type goes through, and the exact twin of the take path's `hunt_escapement_ceiling` /
    /// `forage_escapement_ceiling`:
    ///
    /// ```text
    /// max(0, B − floor·K) × per_biomass_yield
    /// ```
    ///
    /// **It answers ANY floor, which is why the four stance rows became a function.** The player
    /// drags a continuous dial; a forecast made of fixed rows can only answer the floors someone
    /// thought to store, and every row added is a second place the ceiling is computed. There is one
    /// place now, and `forecast == actual` is a property of the arithmetic rather than of keeping
    /// two lists in step.
    ///
    /// **A rung-3 MANAGED source ignores the floor entirely** and pays its `managed_production`: it
    /// is yours, so there is no wild stock to stop short of and the axis honestly collapses. That is
    /// a fact about the rung, not a special case in the caller — which is why the branch lives here
    /// and no take path repeats it.
    ///
    /// **IT TAKES NO `improvement`, and that is what makes the client able to draw the curve**
    /// (`docs/plan_harvest_floor.md` §3.1). The build dip moved onto crew throughput, so a ceiling is
    /// now purely the stock above the floor at the source's own per-biomass rates — linear, exact,
    /// and composable from the terms already on the wire. The dip still ships (`BuildDips`, the
    /// `*BuildFraction` fields); it multiplies `workers × perWorkerYield` instead, which is where
    /// [`forecast_expected_take`] applies it.
    ///
    /// The standing-stock clamp is belt-and-braces (`B − floor·K ≤ B` for any floor `≥ 0`) and kept
    /// because a future ceiling that *could* exceed the stock must not silently over-report.
    pub fn ceiling_at(&self, floor: f32) -> YieldAccounts {
        if let Some(production) = self.managed_production {
            return production;
        }
        let room = escapement_ceiling(floor, self.biomass, self.carrying_capacity);
        self.per_biomass_yield
            .scale(room)
            .min(self.per_biomass_yield.scale(self.biomass.max(0.0)))
    }

    /// **Does this source pay in whole animals?** [`SourceYieldForecast::body_mass_yield`] carries the
    /// quantum; a source that pays it in *neither* currency is continuous (every plant patch/Field).
    pub fn quantises(&self) -> bool {
        !self.body_mass_yield.is_zero()
    }

    /// **The component every RATIO in this forecast is counted on** — the animal count
    /// (`ceiling / one animal`) and the staffing inversions (`take / per-worker`) alike.
    ///
    /// Ratios are unit-free, so any component with a **positive** per-biomass rate gives the same
    /// answer; a component whose rate is `0` gives `0/0`. `Provisions` is preferred so every edible
    /// species divides exactly the numbers it divided before this arc — bit-identical, not merely
    /// equivalent — and a wolf falls through to `TradeGoods`. A source with no positive component at
    /// all yields nothing and has nothing to count.
    ///
    /// Resolved off the quantum first (it is the divisor that actually appears in `floor`), then the
    /// per-worker rate for a continuous source.
    pub fn ratio_axis(&self) -> Option<YieldAxis> {
        self.body_mass_yield
            .ratio_axis()
            .or_else(|| self.per_worker_yield.ratio_axis())
    }
}

/// **The expected take**: the food/turn `workers` will produce at this source under `policy`, and the
/// exact composition the take path pays — the client's "Expected yield" row, the assign-time telemetry
/// seed (`SourceYield`), and the forecast==actual tests all call it. The one place the formula lives.
///
/// **Two shapes, because the two food webs' products differ** (slice 8):
/// - **Continuous** (`body_mass_yield == 0` — every plant source): `min(collection, ceiling)`, the
///   composition `forage_take` pays. You gather grain by the handful.
/// - **Quantised** (an animal source): [`quantise_animal_take`]'s `carried`, the composition
///   `systems::hunt_take` / `expedition_take_biomass` / the pen's corral-tend branch pay. You take
///   whole animals — so the preview shows the same **pulse**, including the honest `0.00` on a turn
///   the herd cannot spare one, rather than a smooth average the sim never actually hands over.
///
/// `collection` is **`workers × per_worker_yield × build_dip(improvement)`** — the build fraction
/// rides the CREW, not the ceiling (`docs/plan_harvest_floor.md` §3.1). So the whole expression is
///
/// ```text
/// min(workers × per_worker_yield × <rung>BuildFraction, ceiling_at(floor))
/// ```
///
/// which is also the composition the client draws its curve from: every term ships. A crew big
/// enough to saturate the source's standing stock therefore pays **no** dip — a build costs yield
/// only while hands are the scarce thing, which is both the fix for §0.3 (no floor can dodge a
/// factor that does not touch the floor's term) and the legible reading of it (at 25% carry it takes
/// four times the people to clear the same standing surplus).
///
/// Both branches are in **provisions**-space; the biomass→provisions conversion is linear and
/// positive, so it factors out of every `min`/`floor` and the quantised branch counts exactly the
/// animals the biomass-space take kills.
///
/// **This is the range's MIDDLE, not a promise** (`docs/plan_hunt_through_combat.md` §6.4) — see
/// [`forecast_take_range`] for what the readout reports around it, and for the restated
/// `forecast == actual`. Where nothing is stochastic (the shipped roster) the two are the same
/// number bit-for-bit, which is why every existing caller keeps this entry point unchanged.
pub fn forecast_expected_take(
    forecast: &SourceYieldForecast,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
) -> YieldAccounts {
    forecast_production_and_take_at(forecast, workers, floor, improvement, HuntDraw::EXPECTED).1
}

/// **The pre-commit take as a DISTRIBUTION** — *"6–11, likely 9"*
/// (`docs/plan_hunt_through_combat.md` §6.4).
///
/// # `forecast == actual`, restated
///
/// The invariant it replaces read *"the forecast is the number the sim will pay"*, and that cannot
/// survive a stochastic take: [`retreat_seed`] is `(map_seed, tick, herd, party)` and a projection
/// cannot know a future tick, so a preview physically cannot draw the roll the live take will draw
/// (see [`HuntDraw`]). The honest form is:
///
/// > **[`Self::likely`] is the take's EXPECTATION over the seed, and the take the sim pays lies
/// > within `[low, high]`.** Where no stage is stochastic the distribution is degenerate and
/// > `low == likely == high == the take`, **bit-for-bit**.
///
/// **Both sentences are now live.** Slice 7 authored a non-zero `wariness` on every species
/// (`docs/plan_hunt_through_combat.md` §3.1), so the animal web's band is a real one — the first
/// sentence — while the **plant** web has no retreat and no fight at all and `hit_chance` is still
/// `1.0`, so a gather stays a point. That degeneracy is not a coincidence to be re-derived at each
/// site: it falls out of [`animals_that_stay`] and [`crate::combat::attacks_landed_at`] sharing the
/// live path's own early returns, which is what keeps a wariness-`0` species (config-only now) and
/// the whole plant web bit-for-bit exact.
///
/// # Why three evaluations of the take rather than a spread applied to one
///
/// The take is `min(engagement, floor, carry, fight)` put through [`quantise_animal_take`]'s
/// `floor()`, so it is **not** linear in the stages that vary: a ±20% band on the animals brought
/// down is not a ±20% band on the food, and on a slow breeder it is frequently *no* band at all
/// (both bounds land on the same whole animal). Evaluating the identical arithmetic at three
/// quantiles reports the lumpiness honestly, and — because every arm is monotone non-decreasing in
/// the draw — gives `low <= likely <= high` without a clamp.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TakeRange {
    /// The pessimistic bound — `−sigmas` on both stochastic stages.
    pub low: YieldAccounts,
    /// **The expectation**, and the number every non-range reader still quotes
    /// ([`forecast_expected_take`]).
    pub likely: YieldAccounts,
    /// The optimistic bound — `+sigmas`.
    ///
    /// **A range is a POINT exactly when `low == likely == high`**, and that is a reading a consumer
    /// makes rather than a stored flag: the sim publishes the three numbers and *"say 9, not 6–11"*
    /// falls out of comparing them. It is the **shipped** case — `wariness 0` and `hit_chance 1.0`
    /// make every quantile the same identity — so a reader who does not know that renders a
    /// zero-width range on every source in the game.
    pub high: YieldAccounts,
}

/// [`TakeRange`] for one assignment — the same arithmetic [`forecast_expected_take`] runs, evaluated
/// at `−sigmas`, the mean, and `+sigmas`.
///
/// `sigmas` is `combat_config.forecast_range_sigmas`, a **readout width**: no resolution path reads
/// it, so widening the reported band cannot move a single animal.
pub fn forecast_take_range(
    forecast: &SourceYieldForecast,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    sigmas: f32,
) -> TakeRange {
    let at = |sigmas: f32| {
        forecast_production_and_take_at(
            forecast,
            workers,
            floor,
            improvement,
            HuntDraw::Quantile { sigmas },
        )
        .1
    };
    TakeRange {
        low: at(-sigmas.abs()),
        likely: at(combat::EXPECTED_STRIKES),
        high: at(sigmas.abs()),
    }
}

/// **The negligible-take floor (in biomass) that ends a `realized` forward projection.** Below this a
/// source is treated as *spent* — its herd extinct or its patch stripped to nothing — so the loop
/// stops and the average divides only by the turns that actually delivered. It is deliberately far
/// below any live source's one-turn take (the slowest wild MSY is ~`r·K/4` ≫ this on every species),
/// so a healthy source never trips it and a dead one always does.
///
/// **Biomass-space only** — that is what the argument above measures. The plant web's projection
/// breaks on an already-converted *provisions* take, so it carries its own sibling constant
/// (`forage::REALIZED_PROJECTION_PROVISIONS_EPSILON`) justified on the provisions scale, rather than
/// borrowing this one across a unit boundary its doc does not cover.
const REALIZED_PROJECTION_TAKE_EPSILON: f32 = 1e-4;

/// **The steady `realized` yield for a hunt source — a FORWARD PROJECTION.** The average food/turn the
/// herd delivers over the next `horizon` turns, computed by simulating it forward from its CURRENT
/// state under `policy` + `workers`, mirroring the real turn order (Logistics regrow → Population take)
/// exactly as [`crate::systems::expeditions::hunt_trip_forecast`] does. It is a **pure function of the
/// passed herd state** — no history, no persistence — so the assign-time seed and the resolved row
/// compute the identical number (exact forecast == actual, the true no-jump).
///
/// **Simulated UNQUANTISED**: whole-animal rounding decides *when* the food arrives, never the N-turn
/// total, so simulating the smooth escapement take ([`hunt_escapement_ceiling`]) gives the smooth
/// average directly — which is the whole point, since the lumpy quantised take is what `actual`
/// already reports. A Sustain herd converges on `K/2` and reads flat at ~MSY; a Surplus/Deplete herd
/// declines to its floor within the horizon and the average honestly reflects it; a corralled herd
/// projects its managed pen yield (already smooth). Reuses the shared model helpers
/// ([`regrow_biomass`], [`hunt_escapement_ceiling`], [`pen_yield_biomass`], [`HuntYield::apply`],
/// [`herd_ecology`]/[`herd_capacity`]) — no second copy of the ecology or take math.
///
/// # Unquantised is NOT unbounded — the engagement cap still binds here
///
/// Dropping the *quantiser* is sound because rounding is a timing effect. Dropping the **engagement
/// bound** ([`animals_engaged`]) would not be: it is a hard per-turn cap on how many animals a party
/// can reach at all, so it genuinely lowers the N-turn total. Two hunters on a rabbit warren can
/// never take more than `2 × engage_rate × body_mass` biomass in a turn however much room stands
/// above the floor, and a `realized` that ignored that would over-quote the steady food rate several
/// times over. So the per-turn take here is capped by `engaged × body_mass` alongside the crew's
/// carry and the standing stock — in biomass, unrounded, which is exactly the smooth reading of the
/// same cap the quantised path applies to whole animals.
// The projection needs the full take context (source, both configs, throughput, multiplier, crew,
// policy, horizon) — the same shape `hunt_source_yield_preview` already carries.
#[allow(clippy::too_many_arguments)]
pub fn project_realized_hunt(
    herd: &Herd,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    // The party doing the hunting — the fight is a per-turn bound on the projected take exactly as
    // the engagement is (see the doc above).
    party: &HuntingParty,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    horizon: u32,
) -> YieldAccounts {
    if horizon == 0 {
        // `LaborConfig::validate` pins `horizon > 0`; belt-and-braces against /0.
        return YieldAccounts::ZERO;
    }
    // The projection runs on a private copy — the caller's live herd is never touched. Ecology and
    // capacity cannot change under the projected take (the quarry is never tamed/penned mid-run), so
    // resolve them once, exactly as `hunt_trip_forecast` does.
    let mut quarry = herd.clone();
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = herd_capacity(&quarry, fauna);
    // The species' yield vector — resolved once; the quarry is never re-speciated mid-projection.
    let hunt_yield = herd_hunt_yield(&quarry, fauna);
    let corralled = quarry.is_corralled();
    // **The build dip rides the CREW** (`docs/plan_harvest_floor.md` §3.1) — the same term
    // `systems::hunt_take` applies, so the projection and the take stay one model.
    let collection = workers as f32 * per_worker_biomass_capacity * ladder.build_dip(improvement);
    // **The engagement bound, in animals** — how many the party can bring into contact each projected
    // turn (`docs/plan_hunt_through_combat.md` §2). Constant for the run: the crew does not change
    // size mid-projection and the quarry is never re-speciated. A **pen** has no engagement stage at
    // all (a penned animal is not stalked), so it is unbounded there — the same exemption
    // `project_arrivals_hunt` states by passing `f32::INFINITY` to the quantiser on its corral branch.
    // **Engagement, then the retreat's EXPECTATION** — a projection cannot draw the retreat the take
    // will draw (see [`HuntDraw`]), so it reads the same binomial's mean. At the shipped
    // `wariness 0` that is an exact identity and this line is inert.
    let engaged = if corralled {
        f32::INFINITY
    } else {
        animals_that_stay(
            animals_engaged(
                workers,
                fauna.engage_rate_for(&quarry.species),
                ladder.build_dip(improvement),
            ),
            fauna.wariness_for(&quarry.species),
            HuntDraw::EXPECTED,
        )
    };
    // **The FIGHT is resolved INSIDE the loop, and it is the wounds that force that** (§4.2). It used
    // to be hoisted out as a constant, which was right for a stateless resolver and is now wrong: a
    // sub-threshold party brings down nothing for several turns and then a whole animal, and a
    // projection that froze the first turn's answer would quote **zero forever** for exactly the
    // parties the accumulator exists to serve. The quarry's body is constant; only its wounds move.
    let mut quarry_fight = herd_quarry_fight(&quarry, fauna);
    let mut total = YieldAccounts::ZERO;
    // The number of turns actually simulated. A self-terminating policy (Eradicate strips the herd in
    // ~1 turn, Deplete drives it extinct) breaks early, and the average divides by THIS — not the full
    // `horizon` — so the reported rate is what the player gets *while the source lasts*, never diluted
    // by dead turns after the herd is gone. Sustain never terminates, so it runs the full horizon.
    let mut turns = 0u32;
    for _ in 0..horizon {
        // Logistics: regrow first (sets `quarry.biomass_before_regrowth`, then grows `quarry.biomass`).
        regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            break; // `advance_herds` would despawn it here — the herd is gone.
        }
        // Population: the SMOOTH per-turn take (unquantised), capped by the crew's throughput, by
        // what the party can reach, and by the standing stock. A pen pays its managed escapement MSY;
        // a wild/pastoral herd pays the stock standing above its stance's floor, at the CURRENT
        // biomass — what `hunt_take` reads. Unquantised, but **not** unbounded: see the doc above for
        // why the engagement cap belongs here and the rounding does not.
        let rate = if corralled {
            pen_yield_biomass(&quarry, fauna)
        } else {
            hunt_escapement_ceiling(floor, quarry.biomass, capacity)
        };
        // Dropping the *quantiser* here is sound because rounding is a timing effect; dropping the
        // fight would not be, for exactly the reason the engagement bound belongs here — a
        // bare-handed party brings down **nothing** from a mammoth herd however much room stands
        // above the floor, and a `realized` that ignored that would quote a steady food rate the
        // party can never collect.
        let engagement_biomass = if corralled {
            f32::INFINITY
        } else {
            let fight = resolve_hunt_fight(
                engaged,
                workers as f32 * ladder.build_dip(improvement),
                party,
                &quarry_fight,
                HuntDraw::EXPECTED,
            );
            quarry_fight = quarry_fight.with_wounds(fight.wounds);
            fight.brought_down * quarry.body_mass
        };
        // **The SOURCE-side offer is what decides whether the run is over** — the stock standing
        // above the floor. A zero take is no longer proof the source is spent: since damage carries
        // between turns a party can grind for several turns and *then* land a body (§4.2), and
        // breaking on the first of those would report **zero forever** for exactly the parties the
        // accumulator exists to serve. So the wait turns stay in, counted in the denominator like the
        // `0.0` slots the arrivals schedule already publishes, and only a spent source breaks.
        let offered = rate.min(quarry.biomass).max(0.0);
        if offered <= REALIZED_PROJECTION_TAKE_EPSILON {
            break; // the source is spent — stop before diluting the average with dead turns.
        }
        let take = offered.min(collection).min(engagement_biomass).max(0.0);
        quarry.biomass -= take;
        // **Both products are projected from the same simulated take**, so the steady trade headline
        // can never drift from the steady food one (`docs/plan_hunt_yield_model.md` §9).
        total = total.plus(hunt_yield.apply(take, output_multiplier));
        turns += 1;
    }
    if turns > 0 {
        total.scale(1.0 / turns as f32)
    } else {
        YieldAccounts::ZERO
    }
}

/// **WHEN the food lands for a hunt source — a FORWARD PROJECTION *with* the whole-animal
/// quantisation.** The discrete sibling of [`project_realized_hunt`]: the same forward simulation,
/// from the same herd state, under the same policy and crew — but run through
/// [`quantise_animal_take`] as the real take path is, recording what is delivered on each projected
/// turn.
///
/// Returns exactly `horizon` entries: **index `i` is the food delivered `i + 1` turns from now**, and
/// `0.0` is an honest *wait* turn (the herd could not yet spare a whole body), not a missing reading.
/// A **continuous** source — a pen, or fast game whose escapement clears a body every turn — simply
/// has a positive value in every slot, which the client draws as a solid run.
///
/// # Why this and `realized` are two functions, not one
///
/// Rounding to whole animals decides *when* one lands, never *how much* lands over the window — so
/// `realized` omits it to get the smooth average directly, and this keeps it to get the timing. Their
/// totals agree: `Σ arrivals ≈ realized × horizon`, up to the partial body still standing at the end.
///
/// # Where it starts
///
/// From the herd's **real current biomass**, which since the harvest floor *is* the accumulator: the
/// wait between kills is the stock climbing back over one `body_mass` above the floor, so a herd six
/// turns into rebuilding a mammoth's worth of room delivers on turn 1, not turn 7. Callers pass the
/// **post-take** state so slot 0 is the *next* delivery rather than the one this turn already paid.
///
/// Simulated on a private clone (the caller's herd is never touched) through the same shared helpers
/// the take path uses ([`regrow_biomass`], [`hunt_escapement_ceiling`], [`quantise_animal_take`],
/// [`pen_yield_biomass`], [`HuntYield::apply`], [`herd_ecology`]/[`herd_capacity`]) — **no second copy
/// of the take math**, so the schedule is what the sim will really pay.
// Same shape as its `realized` sibling — the projection needs the full take context.
#[allow(clippy::too_many_arguments)]
pub fn project_arrivals_hunt(
    herd: &Herd,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    // The party doing the hunting — the schedule runs the same fight the take does.
    party: &HuntingParty,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    horizon: u32,
) -> Vec<f32> {
    // `LaborConfig::validate` pins `horizon > 0`; a zero horizon yields an empty schedule, which the
    // client reads as "no data" exactly like an unprojected row.
    let mut schedule = vec![0.0_f32; horizon as usize];
    // The projection runs on a private copy. Ecology and capacity cannot change under the projected
    // take (the quarry is never tamed/penned mid-run), so resolve them once — as `project_realized_hunt`
    // and `hunt_trip_forecast` both do.
    let mut quarry = herd.clone();
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = herd_capacity(&quarry, fauna);
    // The species' yield vector — resolved once; the quarry is never re-speciated mid-projection.
    let hunt_yield = herd_hunt_yield(&quarry, fauna);
    // How many the party can bring into contact each projected turn — constant for the run, since
    // the crew does not change size mid-projection and the quarry is never re-speciated.
    // Engagement, then the retreat's **expectation** — see the twin in `project_realized_hunt`.
    let engaged = animals_that_stay(
        animals_engaged(
            workers,
            fauna.engage_rate_for(&quarry.species),
            ladder.build_dip(improvement),
        ),
        fauna.wariness_for(&quarry.species),
        HuntDraw::EXPECTED,
    );
    // **The fight is resolved PER TURN, not once for the run**, because its wounds accumulate
    // (§4.2): a sub-threshold party lands nothing for several turns and then a whole animal, and that
    // pulse is exactly what this schedule exists to draw. Only the quarry's *body* is constant.
    // Inert to the seed at the shipped `hit_chance` (see [`FORECAST_FIGHT_SEED`]).
    let mut quarry_fight = herd_quarry_fight(&quarry, fauna);
    let corralled = quarry.is_corralled();
    // **The build dip rides the CREW** (`docs/plan_harvest_floor.md` §3.1) — the same term
    // `systems::hunt_take` applies, so the projection and the take stay one model.
    let collection = workers as f32 * per_worker_biomass_capacity * ladder.build_dip(improvement);
    for slot in schedule.iter_mut() {
        // Logistics: regrow first (sets `quarry.biomass_before_regrowth`, then grows `quarry.biomass`).
        regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            break; // `advance_herds` would despawn it here — the herd is gone, nothing more arrives.
        }
        // Population: the real take path, quantised to whole animals.
        //
        // **The completion test is the extinction floor ALONE — deliberately NOT the realized
        // projection's `take <= EPSILON` break.** There, a zero take means the source is spent and
        // would dilute an average; here a zero take is a *wait* turn, which is the entire mechanic
        // this schedule exists to show. Breaking on it would truncate every big-game schedule at its
        // first gap.
        let carried = if corralled {
            // A pen is a managed harvest: no bank, no policy axis — the keeper butchers whole animals
            // out of the pen's own escapement MSY, exactly as the corral-tend branch of
            // `advance_labor_allocation` does.
            let production = pen_yield_biomass(&quarry, fauna);
            // A penned animal is not stalked: no engagement bound.
            let take = quantise_animal_take(
                production,
                collection,
                quarry.body_mass,
                f32::INFINITY,
                EngagementStop::WhenPackFull,
            );
            quarry.biomass -= take.killed_biomass();
            take.carried
        } else {
            // A wild/pastoral herd hands over the stock standing above its stance's floor, rounded to
            // whole animals — the `systems::hunt_take` sequence, helper for helper.
            let ceiling = hunt_escapement_ceiling(floor, quarry.biomass, capacity);
            let fight = resolve_hunt_fight(
                engaged,
                workers as f32 * ladder.build_dip(improvement),
                party,
                &quarry_fight,
                HuntDraw::EXPECTED,
            );
            quarry_fight = quarry_fight.with_wounds(fight.wounds);
            let take = quantise_animal_take(
                ceiling,
                collection,
                quarry.body_mass,
                fight.brought_down,
                EngagementStop::WhenPackFull,
            );
            quarry.biomass -= take.killed_biomass();
            take.carried
        };
        *slot = hunt_yield.apply(carried, output_multiplier).provisions;
    }
    schedule
}

/// **What the source hands over, and what of it the crew keeps** — `(production, actual)`, the pair
/// [`forecast_expected_take`] and [`forecast_source_yield`] both need, resolved once so the take and
/// the waste can never be computed against different productions.
///
/// **`production` is what the source ACTUALLY GIVES UP this turn, not what it could offer** — and on a
/// quantised source those differ, which is the whole point of splitting this out (slice 8):
/// - **Continuous**: `production` is the policy ceiling. What the crew can't gather stays standing.
/// - **Quantised**: `production` is the **biomass of the animals killed**, not the escapement the herd
///   *could* have spared. An animal you didn't kill was never produced — it is still alive. So a lone
///   hunter on a fresh mammoth herd reports `production = 800` (one mammoth) and wastes 760, **not**
///   `production = 6000` (the whole escapement) wasting 5960, which would be a nonsense reading of a
///   herd standing peacefully on the range.
///
/// That keeps `wasted = production − actual` — slice 7's one formula — meaning exactly one thing at
/// every rung: *food this source gave up that the crew did not bring home*. On the drawn-down plant
/// rungs it stays in the stock and regrows; on an animal rung it is meat left to rot.
///
/// **`draw` decides WHICH reading of the two stochastic stages this is** — the take's expectation
/// ([`HuntDraw::EXPECTED`]) or a bound of the reported range (§6.4). Every arm below is monotone
/// non-decreasing in it, which is what makes `low <= likely <= high` a property of the arithmetic
/// rather than a clamp applied afterwards.
fn forecast_production_and_take_at(
    forecast: &SourceYieldForecast,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    draw: HuntDraw,
) -> (YieldAccounts, YieldAccounts) {
    // **The build dip rides the CREW** (`docs/plan_harvest_floor.md` §3.1): a crew clearing ground or
    // gentling a herd carries `yield_fraction_while_building ×` what a harvesting crew carries. It is
    // floor-independent by construction — the dip no longer touches the floor's term at all — which
    // is what stops a deep floor from building for free.
    let collection = forecast
        .per_worker_yield
        .scale(workers as f32 * forecast.build_dips.of(improvement));
    // The assignment's ceiling at its floor. Undipped: the source offers what stands above the floor
    // whether the crew is harvesting it or building on it.
    let ceiling = forecast.ceiling_at(floor);
    // **The animal count is taken on ONE axis and then valued in both.** `quantise_animal_take`
    // divides by the quantum, so it must run on a component whose per-biomass rate is positive —
    // `Provisions` for every edible species (bit-identical to the pre-#337 arithmetic), `TradeGoods`
    // for a wolf, whose food quantum is `0` and would make `floor(x / 0)` an infinity. The count is
    // the same number on either axis (a ratio is unit-free), so `rescaled_to` carries it back into
    // the other currency without re-running the quantiser or re-deriving the mix.
    match forecast
        .quantises()
        .then(|| forecast.ratio_axis())
        .flatten()
    {
        Some(axis) => {
            let quantum = forecast.body_mass_yield;
            let engaged = animals_engaged(
                workers,
                forecast.engage_rate,
                forecast.build_dips.of(improvement),
            );
            // **Engagement, then retreat, then the fight** — the same three stages in the same order
            // `systems::hunt_take` runs (`docs/plan_hunt_through_combat.md` §1), through the same
            // helpers. The forecast cannot *draw* the retreat or the attack rolls, so it reads them
            // at `draw`'s quantile instead of guessing a seed (see [`HuntDraw`]); the wariness is the
            // quarry's own, off the fight the forecast already carries.
            //
            // **No fight stage means no retreat stage either** — a pen and the plant web, whose
            // `engage_rate` is already `f32::INFINITY`.
            let brought_down = match forecast.fight {
                Some((party, quarry)) => {
                    let stayed = animals_that_stay(engaged, quarry.profile.wariness, draw);
                    resolve_hunt_fight(
                        stayed,
                        workers as f32 * forecast.build_dips.of(improvement),
                        &party,
                        &quarry,
                        draw,
                    )
                    .brought_down
                }
                None => engaged,
            };
            let take = quantise_animal_take(
                ceiling.component(axis),
                collection.component(axis),
                quantum.component(axis),
                brought_down,
                EngagementStop::WhenPackFull,
            );
            (
                quantum.rescaled_to(axis, take.killed_biomass()),
                quantum.rescaled_to(axis, take.carried),
            )
        }
        // Continuous (every plant source): component-wise, because both operands are the same biomass
        // through the same rates, so the two components agree on which side binds.
        None => (ceiling, collection.min(ceiling)),
    }
}

/// Compose the **seeded** `SourceYield` telemetry row for a source from its pre-commit forecast —
/// what the source *will* pay next turn under this staffing/policy, written at assign time so the
/// map annotation and the band panel never show `+0.00` for an assignment that has simply not been
/// resolved yet. Mirrors the rows `advance_labor_allocation` writes:
/// - `actual` = [`forecast_expected_take`],
/// - `sustainable` = the caller's MSY-based sustainable rate (`sustainable_yield × provisions ×
///   output_multiplier`, the same value the resolution path records) — except a **`managed`** source
///   (a rung-3 Field / Pen), whose harvest never draws the stock down, so `sustainable == actual`
///   (no ⚠), exactly as the Field/corral arms record it,
/// - `wasted` = the uncollected signal ([`forecast_production_and_take_at`]): the production the crew
///   could not carry home,
/// - `workers_needed` = the whole crew ([`source_crew_needed`]): the caller's `standing_crew` floored
///   against the take-side count, itself the expected take inverted by the per-worker throughput **as
///   the dip leaves it** (a ratio, so provisions-space matches the resolution path's biomass-space
///   result),
/// - `overdraws` = whether this policy draws the stock below what it sustains — the ⚠ ([`SourceYield`]).
///
/// **`standing_crew` is what the source is owed whether or not it pays this turn** — a herd's
/// `herders_needed` on the animal web, the building rung's [`LadderConfig::build_crew`] on the plant
/// one. Both webs pass it through the *same* [`source_crew_needed`] the resolved arms of
/// `advance_labor_allocation` use, which is what keeps the assign-time seed and the post-turn row
/// reporting the same number: the plant half used to be omitted here, so a freshly-composed
/// `Cultivate` read "only 1 of 2 working" (the dipped take, inverted) until the next turn overwrote
/// it with the build crew.
///
/// **`managed` is rung 3 only** (slice 7): it marks "this source's harvest cannot overdraw", and only
/// the rungs you own qualify. A *tended* patch is still a wild stand on a better curve — it draws
/// down and can be over-farmed — so it takes the ordinary branch, ⚠ and all.
///
/// Every signal is computed for **every** rung, from the one expected take: the rung-kinds differ in
/// what their ceilings mean, never in whether the crew has to carry the food home.
#[allow(clippy::too_many_arguments)] // one row's worth of yield context — bundling it would just move the noise
pub(crate) fn forecast_source_yield(
    forecast: &SourceYieldForecast,
    sustainable: f32,
    managed: bool,
    standing_crew: u32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    realized: f32,
    realized_trade: f32,
    arrivals: Vec<f32>,
    // How wide a band to report around the expected take (`combat_config.forecast_range_sigmas`) —
    // a **readout width**: nothing the sim resolves reads it, so it cannot move an animal.
    range_sigmas: f32,
) -> SourceYield {
    // **The row's scalars are the range's MIDDLE.** A telemetry row states one figure; the
    // distribution it sits in rides beside it as `range` (`docs/plan_hunt_through_combat.md` §6.4),
    // and on the shipped roster the three readings are the same number bit-for-bit.
    let range = forecast_take_range(forecast, workers, floor, improvement, range_sigmas);
    let (production, actual) =
        forecast_production_and_take_at(forecast, workers, floor, improvement, HuntDraw::EXPECTED);
    // What ONE worker on this assignment actually moves — the crew's rate *after* the build dip,
    // since §3.1 put the dip on the crew. Every staffing count below divides by this, never by the
    // undipped `per_worker_yield`: the take it is inverting was paid at this rate.
    let dipped_per_worker = forecast
        .per_worker_yield
        .scale(forecast.build_dips.of(improvement));
    SourceYield {
        actual: actual.provisions,
        // **Trade is telemetry, not larder income** — it never enters `food_income` (see
        // `SourceYield::trade`), so it rides beside `actual` rather than being summed into it.
        trade: actual.trade_goods,
        // The band the two scalars above sit in the middle of. Built from the SAME
        // `forecast_production_and_take_at`, three quantiles apart, so `low <= actual <= high` is a
        // property of the arithmetic rather than a clamp.
        range: YieldRange {
            low: range.low.provisions,
            high: range.high.provisions,
            trade_low: range.low.trade_goods,
            trade_high: range.high.trade_goods,
        },
        sustainable: if managed {
            actual.provisions
        } else {
            sustainable
        },
        // The discrete twin of `realized`, from the same forward simulation run with the kill-credit
        // bank (`project_arrivals_hunt` / `project_arrivals_forage`) — also the caller's, for the same
        // reason: a pure function of the source state, so seed and resolved row agree.
        arrivals,
        // The steady headline: the forward-projected average food/turn over the next horizon turns
        // (`project_realized_hunt` / `project_realized_forage`), computed by the caller from the same
        // source state — a pure function of state, so the seed and the resolved row agree exactly.
        realized,
        realized_trade,
        wasted: (production.provisions - actual.provisions).max(0.0),
        // **Every source reports its whole CREW: [`source_crew_needed`] = `max(standing, take)`** — the
        // SAME shape both resolved arms of `advance_labor_allocation` record, so the assign-time seed
        // and the post-turn row agree (no "1 of N" on a pending Tame or Cultivate). The `standing_crew`
        // is the caller's: a herd's `herders_needed` on the animal web, the building rung's
        // `LadderConfig::build_crew` on the plant one. The animal caller sizes the herder term
        // ownership-INDEPENDENTLY while an improvement is in flight (`would_be_herders_needed`), so a
        // not-yet-owned Tame source seeds its real crew; a wild herd's is `0`, collapsing the `max` to
        // the take side.
        //
        // The take side differs by whether the source is lumpy. A **whole-animal** (hunt) source uses
        // [`hunt_take_workers`] — the peak-drop crew that can both **reach** and **carry** the drop
        // the assignment's own escapement ceiling allows, which equals the client's
        // `_max_useful_workers`: `actual` is the quantised take — `0` on a wait turn for a slow
        // breeder whose room is lighter than one body — so inverting it collapses the count and
        // contradicts `wasted`. A **continuous** source (every plant patch/Field,
        // `body_mass_yield == 0`) is un-lumpy, so it keeps the ordinary overstaffing inversion.
        //
        // Both branches count on [`SourceYieldForecast::ratio_axis`] rather than on provisions: a
        // staffing count is a RATIO, and dividing a wolf's zero food take by its zero per-worker food
        // rate is the `0/0` the vector model exists to make impossible. Every edible species divides
        // exactly the numbers it divided before #337.
        //
        // **And BOTH branches divide by the DIPPED throughput** — `dipped_per_worker`, the exact term
        // [`forecast_production_and_take_at`] scales the crew by. Dividing the undipped rate into a take
        // that was paid the dip is a unit mismatch that lands on both webs at once: the plant branch
        // reported `workers × dip` hands working out of `workers` assigned (advice that, followed,
        // halves the take), and the animal branch sized the haul crew as if the party were harvesting
        // when it is gentling. Seed and resolved row agreed with each other and both disagreed with
        // the take, which is why a seed==resolved test cannot see it.
        workers_needed: match forecast.ratio_axis() {
            Some(axis) if forecast.quantises() => source_crew_needed(
                standing_crew,
                hunt_take_workers(
                    forecast.ceiling_at(floor).component(axis),
                    forecast.body_mass_yield.component(axis),
                    dipped_per_worker.component(axis),
                    // **The engagement term is UNDIPPED here and dipped by the argument beside it** —
                    // `hunt_engage_workers` multiplies the two exactly as `animals_engaged` does, so
                    // the crew inverts the bound the take was actually paid. A pen forecasts
                    // `f32::INFINITY` and contributes no engagement crew at all.
                    forecast.engage_rate,
                    forecast.build_dips.of(improvement),
                ),
            ),
            Some(axis) => source_crew_needed(
                standing_crew,
                workers_needed_for_take(
                    actual.component(axis),
                    dipped_per_worker.component(axis),
                    workers,
                ),
            ),
            // A source that yields nothing in either currency still has to be kept: a build crew (or a
            // herd's keepers) is owed whether or not the source pays this turn.
            None => standing_crew,
        },
        overdraws: !managed && floor_overdraws(floor),
    }
}

/// The assign-time yield telemetry seed for a **Hunt** source: what staffing `herd` with `workers`
/// hunters under `policy` will pay next turn, in the same shape the Hunt arm of
/// `advance_labor_allocation` records after the take. Reuses `hunt_forecast` (hence `hunt_take`'s own
/// ceiling/conversion helpers) and the shared MSY `sustainable_yield`, so the seed is exactly the
/// number the turn then produces — no jump. The plant mirror is `forage::forage_source_yield_preview`.
// The seed composes the whole telemetry row, so it carries the full take context (see the sibling
// `project_realized_hunt`).
#[allow(clippy::too_many_arguments)]
pub fn hunt_source_yield_preview(
    herd: &Herd,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    // The band's own party — kit and tuning — so the seed resolves the fight the turn will.
    party: &HuntingParty,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    realized_horizon: u32,
    arrivals_horizon: u32,
    // `combat_config.forecast_range_sigmas` — how wide a band the seeded row reports around its
    // expected take (`docs/plan_hunt_through_combat.md` §6.4). Last, matching the forage twin.
    range_sigmas: f32,
) -> SourceYield {
    let forecast = hunt_forecast(
        herd,
        fauna,
        ladder,
        per_worker_biomass_capacity,
        party,
        output_multiplier,
    );
    let sustainable = herd_hunt_yield(herd, fauna)
        .apply(
            sustainable_yield(
                herd.biomass,
                herd_capacity(herd, fauna),
                &herd_ecology(herd, fauna),
            ),
            output_multiplier,
        )
        .provisions;
    // The steady headline is the forward projection from THIS herd state — the same computation the
    // resolved Hunt arm runs, so seed == first resolved value exactly.
    let realized = project_realized_hunt(
        herd,
        fauna,
        ladder,
        per_worker_biomass_capacity,
        party,
        output_multiplier,
        workers,
        floor,
        improvement,
        realized_horizon,
    );
    // The discrete twin, from the same herd state: when each of the next `arrivals_horizon` deliveries
    // lands, bank and all.
    let arrivals = project_arrivals_hunt(
        herd,
        fauna,
        ladder,
        per_worker_biomass_capacity,
        party,
        output_multiplier,
        workers,
        floor,
        improvement,
        arrivals_horizon,
    );
    // **The herder term, ownership-INDEPENDENT while an improvement is in flight** (taming-startup-lag
    // fix): a Tame/Corral compose means the herd is being managed, but ownership is set only later in
    // Population, so `herd_herders_needed` would read `0` on the compose turn and the seed's
    // `workers_needed` would collapse to the haul crew ("1 of N" on a pending Tame).
    // `would_be_herders_needed` is the real crew regardless of recorded ownership; a pure harvest stays
    // ownership-gated (wild = 0). This is the SAME rule the resolved Hunt arm applies at its
    // `source_crew_needed`, so seed == resolved. **The question is now "is a build running", asked of
    // the improvement axis** — it used to be `policy.is_investment()`, which the split retires.
    let herders_needed = if improvement.is_some() {
        would_be_herders_needed(herd, fauna)
    } else {
        herd_herders_needed(herd, fauna)
    };
    forecast_source_yield(
        &forecast,
        sustainable,
        herd.is_corralled(),
        herders_needed,
        workers,
        floor,
        improvement,
        realized.provisions,
        realized.trade_goods,
        arrivals,
        range_sigmas,
    )
}

/// **THE biomass a hunt may take from a herd this turn** — **constant escapement**. The animal web's
/// half of `docs/plan_harvest_floor.md` §1, and the exact twin of
/// `forage::forage_escapement_ceiling`:
///
/// ```text
/// max(0, B − floor·K)
/// ```
///
/// | floor | herd |
/// |---|---|
/// | `0.50` = [`MSY_BIOMASS_FRACTION`] | settles ON `K/2`, the most productive biomass |
/// | `0.30` | drawn down, still above the Allee brink |
/// | `0.15` = `ecology.collapse_fraction` | drawn to the brink; depensation finishes it |
/// | `0` | the whole stock — under `extinction_floor`, and gone |
///
/// **THE BUILD DIP IS NOT HERE — it multiplies the CREW** (`docs/plan_harvest_floor.md` §3.1). See
/// `forage::forage_escapement_ceiling` for why: dipping the ceiling made a deep floor build for
/// free, because a fraction of a bigger stock still filled the crew's baskets.
///
/// # It is a STOCK, not a rate — which is why the kill-credit bank left this path
///
/// The four stances used to be four ascending **multiples of MSY** banked into
/// [`Herd::hunt_credit`] until the bank cleared one `body_mass`. A ceiling that is already a *stock*
/// must not be banked: adding it to an accumulator would offer the herd's whole surplus **plus
/// everything it had already handed over**, compounding a quantity that was never a flow. So
/// [`systems::hunt_take`] neither reads nor advances `hunt_credit`, and the accumulator the bank
/// provided is the herd's **own standing biomass**: a mammoth held at floor `0.5` regrows ~120/turn
/// against an 800 body mass, so `B − 0.5·K` crosses one body after ~7 turns and
/// [`quantise_animal_take`] pays exactly the wait-then-one pulse the bank used to produce. Same
/// cadence, one fewer piece of state.
///
/// **There are no arms at all any more.** "Take everything" was once its own branch, because it was a
/// stock while the other three stances were rates; it is now simply `floor = 0` of the one
/// expression, which is what let the stance axis be replaced by the number it always stood for.
///
/// # `r`-INDEPENDENT — no ecology, no `FaunaConfig`, and that is structural
///
/// The escapement room does not depend on how fast the herd breeds ([`escapement_ceiling`]), so this
/// function cannot reach the growth curve at all: dropping the `ecology` + `fauna` parameters is what
/// makes *"the take never depends on `r`"* a property of the signature rather than a rule to
/// remember. [`sustainable_yield`] survives for **telemetry only** — the overdraw ⚠ and the
/// investment-payoff projections — and no take path may call it.
///
/// # It reads the CURRENT biomass
///
/// Not [`Herd::biomass_before_regrowth`]. That subtlety existed because a constant *catch* evaluated
/// after Logistics regrowth takes more than the stock grew, leaking a below-`K/2` herd down;
/// constant escapement has no such leak — `B_now − floor·K` is exactly the stock standing above the
/// floor, whenever it is measured.
///
/// `carrying_capacity` is **the herd's own**, resolved by [`herd_capacity`] — never the caller
/// reaching for `herd.carrying_capacity` past the seam.
///
/// `floor` is the **assignment's**, a fraction of `K` in `0.0..=1.0`
/// ([`crate::components::floor_is_valid`], enforced at the command boundary). It is not a stance and
/// not a lookup: the whole of what the player decides about pressure is this one number.
pub fn hunt_escapement_ceiling(floor: f32, biomass: f32, carrying_capacity: f32) -> f32 {
    escapement_ceiling(floor, biomass, carrying_capacity)
}

/// **One turn's whole-animal hunt take** — the result of [`quantise_animal_take`].
///
/// The herd loses [`AnimalTake::killed_biomass`] (you cannot un-kill an animal you could not carry);
/// the party banks `carried`; `wasted` is the difference, and it is a **real loss** — meat left to rot
/// on the range, not stock left standing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AnimalTake {
    /// Whole animals killed this turn. `0` = the herd could not spare one and the hunt **waited**.
    pub killed: u32,
    /// Biomass carried home — what the larder is paid for.
    pub carried: f32,
    /// Biomass killed but **not** carried: the party could not haul the whole animal. The player's
    /// call, never hidden — it is what `SourceYield.wasted` reports on a hunt.
    pub wasted: f32,
}

impl AnimalTake {
    /// The biomass the **herd** loses: every animal killed, carried home or not.
    pub fn killed_biomass(&self) -> f32 {
        self.carried + self.wasted
    }
}

/// **Does the party's pack stop it engaging?** — the single line of behaviour that separates a
/// denial raid from a hunt (`docs/plan_denial_raid.md` §1), carried as a type so the difference is
/// stated once and read by [`quantise_animal_take`] and [`hunt_take_bound`] together.
///
/// ```text
/// hunt:    carried = min(killed × body_mass, carry_capacity, carry_room)
///          …and the party stops engaging once its pack is full
///
/// denial:  carried = min(killed × body_mass, carry_capacity, carry_room)   // identical
///          …and the party never stops engaging
/// ```
///
/// **It bounds the KILL, never the carry.** `carried` is the same expression under both, which is
/// why a raid still banks whatever it can haul on the way home — a rounding error against what it
/// killed, and the point of the mission. What denial removes is the clause that made
/// *"hunters do not kill what they cannot use"* true, because killing what you have no intention of
/// using is the entire premise.
///
/// **This is what `floor = 0` could never be** (§0). The escapement floor is a *number*; the pack is
/// a *bound*, and no value of the number reaches it — which is why denial is a mission
/// ([`crate::ExpeditionMission::Deny`]) rather than a preset on the assign dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngagementStop {
    /// **A hunt** — the pack is a bound on the kill as well as on the haul, so a nearly-full party
    /// kills fewer animals rather than slaughtering one it has no room for
    /// (`systems::hunt_take`'s original rule, and the right model of subsistence hunting).
    WhenPackFull,
    /// **A denial raid** — the pack bounds only what comes home. The party engages for as long as
    /// the herd can spare an animal and the fight can put it down.
    Never,
}

/// **Is this herd past the point of no return?** — biomass under `ecology.collapse_fraction × K`,
/// where [`net_biomass_delta`] zeroes the growth flow and the herd instead declines irreversibly at
/// `collapse_rate` with no further pressure on it (`docs/plan_denial_raid.md` §1.1).
///
/// **It is the denial raid's success condition, and that is why it is not "zero".** A raid's goal is
/// to push the herd under this line and walk away, not to kill every animal — which is what lets a
/// small party erase a large placid herd, and why ordinary subsistence hunting never does it by
/// accident: any escapement floor above `collapse_fraction` stops the take long before.
///
/// Read against the **same** comparison [`classify_ecology_phase`] makes ([`EcologyPhase::Collapsing`]),
/// so the raid's completion and the ecology band a client renders cannot disagree about where the
/// line is. A herd with no capacity is already non-viable.
pub fn herd_past_recovery(biomass: f32, carrying_capacity: f32, ecology: &EcologyConfig) -> bool {
    classify_ecology_phase(biomass, carrying_capacity, ecology) == EcologyPhase::Collapsing
}

/// **How many animals a party can bring into contact this turn** — the engagement stage of
/// `docs/plan_hunt_through_combat.md` §2, and the single definition of it, so no two hunters of a
/// herd can disagree about how many they could reach.
///
/// `workers × engage_rate`, floored to whole animals — **but never below one for a party that
/// exists**. A fractional engagement means a small band cannot corner the quarry *efficiently*, not
/// that it cannot walk up to it: three hunters do reach a mammoth, and then fail at the *fight*,
/// which is where the gate lives. Flooring to zero would put a headcount threshold in front of the
/// attack-vs-defense one and hide the reason.
///
/// A party of no workers engages nothing, which is not the same statement.
///
/// # The build dip multiplies THIS too, and leaving it out re-opens a closed defect
///
/// `build_dip` is the rung's `yield_fraction_while_building` — hands spent gentling a herd are hands
/// not hunting it, and engagement is *crew throughput* exactly as carry is
/// (`docs/plan_harvest_floor.md` §3.1). Applying the dip only to carry looks harmless until the
/// engagement bound is the binding one, at which point a building crew and a harvesting crew take
/// **the same number of animals** and the build is free — which is §0.3 of that same doc
/// ("the harshest stance builds free") returning through a new door.
///
/// Pass [`crate::intensification::NO_BUILD_UNDERWAY_DIP`] where nothing is being built — the
/// identity multiplier, and exactly what [`crate::intensification::BuildDips::of`] answers for
/// `None`. **Not `NO_BUILD_REMAINING_FRACTION`**, which is `0.0` and a *wire* value: passing it here
/// would floor the engagement to `0` and the `max(1.0)` would silently cap the whole party at one
/// animal per turn.
pub fn animals_engaged(workers: u32, engage_rate: f32, build_dip: f32) -> f32 {
    if workers == 0 {
        return 0.0;
    }
    (workers as f32 * engage_rate.max(0.0) * build_dip.max(0.0))
        .floor()
        .max(1.0)
}

/// **The per-event seed for a retreat draw** — `(map_seed, tick, herd, party)`, order-independent by
/// construction (`docs/plan_hunt_through_combat.md` §6.2). Two runs that resolve the same hunts in a
/// different order must produce identical outcomes, which a shared RNG stream cannot promise.
///
/// **The herd id is folded in with [`crate::hashing::FnvHasher`], never `DefaultHasher`** — the same
/// rule every other seed site in the sim follows. `std`'s hasher is SipHash-1-3 with an output the
/// library documents as *unspecified across releases*, so a checkpoint replayed on a different
/// toolchain would derive a different seed for the same `(map_seed, tick, herd, party)` and stop
/// reproducing the retreat — the exact property this function exists to guarantee.
pub fn retreat_seed(map_seed: u64, tick: u64, herd_id: &str, workers: u32) -> u64 {
    let mut hasher = FnvHasher::new();
    herd_id.hash(&mut hasher);
    map_seed ^ tick ^ RETREAT_SEED_SALT ^ hasher.finish() ^ (workers as u64)
}

/// **How many of the engaged animals stay to be fought** — the retreat stage
/// (`docs/plan_hunt_through_combat.md` §3), between engagement and the fight.
///
/// Each engaged animal independently breaks off with probability `wariness`. **Escaped animals are
/// not dead**, so the herd loses nothing for them: a wary herd costs the party *hunter-turns*, never
/// herd biomass, and that pressure falls out with no extra rule.
///
/// # `wariness == 0` is an EXACT identity, and that is load-bearing
///
/// No draw is made and no randomness is consumed — not a `gen_bool(0.0)` that returns `false` while
/// advancing the stream. That is what lets the field ship inert across the whole roster and leaves
/// every existing yield test pinning the numbers it pins today, and it is asserted directly rather
/// than assumed.
///
/// A non-finite `engaged` (a pen, a plant — no engagement stage at all) is returned unchanged: there
/// is nothing to retreat from, and iterating it would not terminate.
pub fn animals_that_stay(engaged: f32, wariness: f32, draw: HuntDraw) -> f32 {
    if wariness <= 0.0 || !engaged.is_finite() || engaged <= 0.0 {
        return engaged;
    }
    let stayers = engaged.floor();
    match draw {
        HuntDraw::Seeded(seed) => {
            let odds = f64::from(wariness.min(1.0));
            let mut rng = SmallRng::seed_from_u64(seed);
            (0..stayers as u32).filter(|_| !rng.gen_bool(odds)).count() as f32
        }
        // **A forecast makes no draw** — it reads the same binomial analytically (see [`HuntDraw`]).
        HuntDraw::Quantile { sigmas } => {
            let stay_chance = 1.0 - wariness.min(1.0);
            let mean = stayers * stay_chance;
            let deviation = (stayers * stay_chance * (1.0 - stay_chance)).sqrt();
            (mean + sigmas * deviation).clamp(0.0, stayers)
        }
    }
}

/// **How a hunt resolves its two stochastic stages** — the retreat draw ([`animals_that_stay`]) and
/// the fight's per-unit attack rolls ([`crate::combat::StrikeDraw`]) — carried as one value so a
/// take path states its mode once and every stage downstream obeys it.
///
/// # A forecast cannot draw, and that is a fact about time rather than a limitation
///
/// [`retreat_seed`] is composed from `(map_seed, tick, herd, party)`; a projection is projecting into
/// ticks that have not happened, so **there is no tick for it to name**. Passing a stand-in (as the
/// retired `forecast_retreat_seed` did) does not make the preview reproduce the take — it makes it
/// draw a *different* sample with the same confidence, which is exactly the promise
/// `docs/plan_hunt_through_combat.md` §6.4 stops the forecast making.
///
/// So a forecast asks for a **quantile of the distribution the take will draw from** instead:
/// `sigmas = `[`crate::combat::EXPECTED_STRIKES`] is its point estimate, and `±forecast_range_sigmas`
/// are the bounds it reports as a range. Where there is no randomness — the plant web, a pen, or a
/// species held at `wariness 0`, all at the shipped `hit_chance 1.0` — every stage takes its exact
/// identity whatever `sigmas` says, so low, likely and high collapse onto the one number the take
/// pays, **bit-for-bit**. On the animal web they no longer do: slice 7's authored `wariness` (§3.1)
/// is what makes the reported band real.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HuntDraw {
    /// **A live hunt** — draw both stages from this per-event seed (§6.2), never a shared stream, so
    /// resolving the same hunts in a different order gives identical outcomes.
    Seeded(u64),
    /// **A forecast** — draw nothing; read both stages `sigmas` standard deviations from their means.
    Quantile { sigmas: f32 },
}

impl HuntDraw {
    /// The forecast's **point estimate** — the mean of every stage, and what `forecast == actual`
    /// now claims the take's expectation equals.
    pub const EXPECTED: Self = Self::Quantile {
        sigmas: combat::EXPECTED_STRIKES,
    };

    /// How the *fight* half resolves under this mode — the value a [`CombatTuning`] carries into
    /// [`combat::resolve_fight`], so the retreat and the fight cannot end up in different modes.
    pub fn strike(self) -> combat::StrikeDraw {
        match self {
            HuntDraw::Seeded(_) => combat::StrikeDraw::Seeded,
            HuntDraw::Quantile { sigmas } => combat::StrikeDraw::Quantile { sigmas },
        }
    }

    /// The stream seed a live fight draws from. A forecast makes no draw, so it hands over
    /// [`FORECAST_FIGHT_SEED`] and nothing reads it.
    pub fn seed(self) -> u64 {
        match self {
            HuntDraw::Seeded(seed) => seed,
            HuntDraw::Quantile { .. } => FORECAST_FIGHT_SEED,
        }
    }
}

/// **The quarry's side of a hunt fight** — the species dials the resolver needs, resolved off
/// [`SpeciesDef`] by [`FaunaConfig::quarry_fight_for`] so no take path reaches past that seam.
///
/// `ferocity` is **not** part of [`CombatStats`] because it is not a body: it is *"does it fight back
/// or flee"*, and it composes into the animal's attack at the adapter (`docs/plan_predators.md`'s
/// strength-vs-behaviour split, §4.4 here). Everything else — `defense`, `durability`, the intrinsic
/// `attack` it scales — is the neutral combat body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuarryFight {
    /// The species' intrinsic combat body: `defense` (the gate), `durability` (the attrition
    /// denominator) and the `attack` `ferocity` scales.
    pub profile: CombatStats,
    /// **Does it fight back, or flee** — `0` makes the engagement one-sided (§4.4/§4.6): the animal
    /// side contributes no attack at all, so nobody is hurt and no battle report is emitted.
    pub ferocity: f32,
    /// **What earlier turns of this hunt already did to it** ([`Herd::wounds`]) — the cross-turn
    /// accumulator, carried on the quarry because it is a fact about the animal rather than about the
    /// party (§4.2).
    ///
    /// Defaults empty, which is the un-hunted animal and the exact pre-accumulation behaviour; a live
    /// take path fills it from the herd with [`herd_quarry_fight`] and stores
    /// [`HuntFight::wounds`] back afterwards.
    pub wounds: DamageLedger,
}

impl QuarryFight {
    /// The same quarry, carrying the wounds a herd has already taken — the one seam between
    /// [`Herd::wounds`] and the fight.
    pub fn with_wounds(self, wounds: DamageLedger) -> Self {
        Self { wounds, ..self }
    }

    /// **What the animal actually swings** — `attack × ferocity`, the one composition
    /// (`docs/plan_predators.md`: *danger = strength × behaviour*). A fleeing deer barely scratches
    /// the party; a cornered mammoth is a real fight.
    pub fn effective_attack(&self) -> f32 {
        self.profile.attack * self.ferocity
    }

    /// The animal's profile **as it fights** — its body with the ferocity-scaled attack.
    pub fn fighting_profile(&self) -> CombatStats {
        CombatStats {
            attack: self.effective_attack(),
            ..self.profile
        }
    }
}

/// **The party's side of a hunt fight, resolved once per band/party** — the per-hunter profile with
/// the hunting kit composed in ([`crate::equipment_config::EquipmentConfig::hunter_profile`]) and the
/// resolver tuning it fights at.
///
/// **Every take and forecast path takes this same struct**, which is the point: `hunt_take`, the
/// expedition raid, the arrivals schedule, the steady projection and the pre-commit preview all
/// resolve the *identical* fight, so `forecast == actual` per component cannot drift into two
/// answers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntingParty {
    /// One hunter's combat profile — intrinsic ⊕ hunting kit. `attack 1` bare-handed, `20` speared.
    pub hunter: CombatStats,
    /// The resolver severity dials this party fights at. An expedition passes the
    /// `expedition_danger_multiplier`-scaled lethality; a resident band the base tuning.
    pub tuning: CombatTuning,
    /// **The hunt's own hazard, per animal engaged**
    /// ([`crate::combat_config::CombatConfig::hunt_injury_damage_per_animal`]) — damage the *activity*
    /// does to the party whatever the quarry swings (§4.6).
    ///
    /// Hunters fall, break bones, are trampled in a drive, cut themselves butchering. Without it only
    /// mammoth, aurochs and wolf could hurt anyone on the shipped roster and a boar cost nothing,
    /// contradicting §4.2's own *"survives by ferocity alone — frail, still costs you people"*.
    ///
    /// It rides the **party** rather than the quarry because the danger is in the activity, not in
    /// the rabbit; it scales with the *engagement* at the point of use, so more animals worked means
    /// more chances to get hurt.
    pub injury_damage_per_animal: f32,
}

impl HuntingParty {
    /// **The shipped, fully-kitted party at the base tuning** — the `person` row's intrinsic profile
    /// with the hunting kit's `attack` tier composed in.
    ///
    /// **It reads the BUILTIN configs, so a `*_CONFIG_PATH` override or a staged tuning patch does
    /// not reach it.** That makes it a fixture helper — the party a test means when it says "an
    /// ordinary band" — and **not** something a production path may call: every one of those has a
    /// live handle and a band whose kit may be worn out, and must resolve
    /// [`crate::equipment_config::EquipmentConfig::hunter_profile`] against it.
    pub fn builtin_equipped() -> Self {
        let combat = crate::combat_config::CombatConfig::builtin();
        let equipment = crate::equipment_config::EquipmentConfig::builtin();
        Self {
            hunter: equipment.hunter_profile(
                crate::creatures_config::CreaturesConfig::builtin().person(),
                true,
            ),
            tuning: combat.tuning(),
            injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
        }
    }

    /// The same party **with its spears gone** — the unequipped tier, which is the `person` row's
    /// intrinsic `attack 1`. The other side of §4.8's cliff, and what a test asserting the gate wants.
    pub fn builtin_unequipped() -> Self {
        let combat = crate::combat_config::CombatConfig::builtin();
        Self {
            hunter: crate::creatures_config::CreaturesConfig::builtin().person(),
            tuning: combat.tuning(),
            injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
        }
    }
}

/// **What the fight cost the party** — the band-side casualties of one hunt, in people.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FightCasualties {
    /// Permanent losses, applied at the population `death_fraction` seam.
    pub killed: f32,
    /// Recoverable losses — surfaced, mechanically inert until the recovery slice.
    pub wounded: f32,
}

/// The `killed` a `HuntDanger` line needs before it is worth pushing — see the gate in
/// `systems::labor`'s Hunt arm for why it is a death rather than [`FightCasualties::any`].
pub const NO_DEATHS_TO_REPORT: f32 = 0.0;

impl FightCasualties {
    /// Did anyone go down at all — killed **or** wounded?
    ///
    /// **Not the `HuntDanger` gate any more.** The hunt's baseline injury risk (§4.6) makes this true
    /// on every engagement, so the feed line is gated on [`NO_DEATHS_TO_REPORT`] instead; this stays
    /// the honest "did the fight cost anything" question a test or a future readout wants.
    pub fn any(&self) -> bool {
        self.killed + self.wounded > 0.0
    }
}

/// **One turn's fight** — how many animals the party brought down, and what it cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntFight {
    /// **Whole animals brought down** — the bound [`quantise_animal_take`] takes as its fight arm.
    /// [`f32::INFINITY`] for a source with no fight stage at all (a pen).
    pub brought_down: f32,
    /// What the party lost.
    pub casualties: FightCasualties,
    /// **Was this a real fight** — `false` for the one-sided engagement of §4.5 (the animal side
    /// contributes no attack), which resolves without ceremony, cost or a battle report.
    ///
    /// **It is not the gate on casualties any more.** The hunt's own baseline injury risk fires on a
    /// harmless quarry too (§4.6), so a `false` here means *no battle report*, not *no cost*.
    pub fought: bool,
    /// **The quarry's wound ledger AFTER this turn's damage** — what a live take path stores back on
    /// [`Herd::wounds`], and what a forward projection threads into its next simulated turn.
    ///
    /// Returned by value rather than mutated through a `&mut` so [`resolve_hunt_fight`] stays a pure
    /// function of its inputs, exactly as [`crate::combat::resolve_fight`] is: a forecast can resolve
    /// the same fight the take will and simply drop the ledger.
    pub wounds: DamageLedger,
}

/// **The fight stage** (`docs/plan_hunt_through_combat.md` §4) — the party engages `stayed` animals
/// and the combat subsystem decides who dies, on both sides.
///
/// This replaced a bespoke hunt formula. Before it, one event was resolved **twice**: what happened to
/// the hunters went through [`crate::combat::resolve_fight`] while what happened to the animals came
/// out of the party's *carrying capacity*, and nothing reconciled them (§0.1). Now the take's kill arm
/// **is** the resolver's enemy losses, so a party cannot succeed on one path while the other says the
/// mammoth routed them — and `plan_predators.md` §7's *"casualties resolve through a first-class
/// combat subsystem, never a bespoke hunt formula"* holds for both sides of the same fight.
///
/// # A herd is a `Force` (§4.1)
///
/// The take already quantised to whole animals, so the herd already knew its animal count. `stayed`
/// animals map to one [`Contingent`] carrying the species' [`CombatStats`], the party to one
/// `"person"` contingent at the kit-composed profile, and the enemy losses map back to whole animals
/// on the way out. [`AnimalTake`] needed no new field.
///
/// # Brought down, not killed — the split is about a force that persists
///
/// The animal side's losses are read as **`killed + wounded`**. The kill/wound split exists to model
/// *recoverable* losses for a force that fights again; a hunting party finishes what it brings down,
/// so an animal on the ground is meat either way. Reading only `killed` would apply a silent
/// few-percent haircut that varies with party size — and would break §4.6's ceiling arithmetic, which
/// is `min(engage_rate, (attack − defense)/durability) × body_mass` exactly.
///
/// # The one-sided fast path (§4.5)
///
/// **Snaring rabbits is not a war.** When the animal contributes no attack, the party's casualties are
/// *structurally* zero — there is no damage to take — so the payload, the party's `Force` and the
/// battle report are all skipped and `fought` reports `false`. It is a genuine short-circuit and **not
/// a second model**: it composes the very same [`crate::combat::strike_damage`] /
/// [`crate::combat::units_brought_down`] primitives [`crate::combat::resolve_fight`] does, so the kill
/// count is identical either way. A second *formula* for small game would recreate exactly the
/// parallel-model problem §0 exists to delete.
///
/// # The fight CARRIES OVER — a wounded animal stays wounded (§4.2)
///
/// Damage that does not finish a body this turn is banked on the quarry ([`QuarryFight::wounds`] in,
/// [`HuntFight::wounds`] out), so twenty hunters with weak spears wear a mammoth down over several
/// turns instead of bouncing off it forever. **Without it the gate is absolute rather than steep**:
/// `ceil(durability / (attack − defense))` would be a hard threshold — 63 hunters for a mammoth at
/// the shipped spear — and a party of 62 would take casualties every turn and never kill anything, on
/// any horizon.
///
/// **The gate itself is untouched.** Below it [`crate::combat::strike_damage`] is exactly `0`, and no
/// length of horizon accumulates zero into a kill: eight hundred bare-handed people still cannot take
/// a mammoth. Why banking a *flow* is right where banking the escapement *stock* was not is recorded
/// on [`DamageLedger`], because that is the objection this design invites.
///
/// # The hunt itself injures people (§4.6)
///
/// On top of whatever the quarry does, the hunt carries a **baseline injury risk** —
/// [`HuntingParty::injury_damage_per_animal`] × the animals engaged, run through the resolver's own
/// severity arithmetic ([`crate::combat::casualties_from_damage`]) and **added to** the fight's
/// casualties rather than reported beside them. Hunters fall, are trampled in a drive, cut themselves
/// butchering; a harmless animal is not a risk-free day out. It scales with the **engagement**, not
/// with the quarry, so it is one lever rather than a per-species field.
///
/// A non-finite `stayed` (a pen — a penned animal is not stalked, not fought, not wary and not
/// dangerous) is returned unchanged with no fight and no injuries at all.
pub fn resolve_hunt_fight(
    stayed: f32,
    // **The party's EFFECTIVE strength, build dip included** — `workers × build_dip`, the same term
    // [`animals_engaged`] is handed (`docs/plan_harvest_floor.md` §3.1: the dip multiplies the crew,
    // never the ceiling). Hands spent gentling a herd are hands not fighting it, so a crew mid-build
    // brings down proportionally less; passing the raw head count would let a build fight for free
    // and reopen §0.3's "the harshest stance builds free" through a new door.
    hunters: f32,
    party: &HuntingParty,
    quarry: &QuarryFight,
    // **Live or forecast** — whether the attack rolls are drawn from a per-event seed or read off
    // their own binomial (`docs/plan_hunt_through_combat.md` §6.4). The forecast paths hand over a
    // [`HuntDraw::Quantile`], which is what lets the preview resolve *this* function rather than a
    // second copy of it.
    draw: HuntDraw,
) -> HuntFight {
    // The tuning this fight resolves at, with the caller's draw mode folded in — one substitution,
    // so the one-sided arm and `combat::resolve_fight` cannot disagree about whether to roll.
    let tuning = CombatTuning {
        draw: draw.strike(),
        ..party.tuning
    };
    if !stayed.is_finite() || stayed <= 0.0 || hunters <= 0.0 {
        return HuntFight {
            brought_down: stayed,
            casualties: FightCasualties::default(),
            fought: false,
            wounds: quarry.wounds,
        };
    }
    // **What the activity costs whoever shows up** (§4.6) — resolved before the quarry is even asked
    // whether it fights back, because it does not depend on the answer.
    let injuries = hunt_injuries(stayed, hunters, party);
    let mut wounds = quarry.wounds;
    if quarry.effective_attack() <= 0.0 {
        // **The one-sided engagement.** The animal cannot hurt anyone, so the fight itself costs
        // nothing and the kill is all the resolver would have computed.
        let landed = combat::landed_strikes_seeded(hunters, &tuning, draw.seed());
        let damage = landed * combat::strike_damage(party.hunter.attack, quarry.profile.defense);
        return HuntFight {
            brought_down: wounds.strike(damage * tuning.lethality, &quarry.profile, stayed),
            casualties: injuries,
            fought: false,
            wounds,
        };
    }
    let payload = FightPayload {
        sides: vec![
            Force {
                id: HUNTING_PARTY_FORCE,
                posture: Posture::Aggressor,
                contingents: vec![Contingent {
                    kind: ContingentId::from(HUNTER_CONTINGENT),
                    count: hunters,
                    profile: party.hunter,
                }],
            },
            Force {
                id: QUARRY_FORCE,
                posture: Posture::Defender,
                contingents: vec![Contingent {
                    kind: ContingentId::from(QUARRY_CONTINGENT),
                    count: stayed,
                    profile: quarry.fighting_profile(),
                }],
            },
        ],
        // The engagement's hex is the herd's, and the placeholder resolver ignores it; a hunt has no
        // second tile to name.
        terrain: Vec::new(),
        seed: draw.seed(),
    };
    let outcome = combat::resolve_fight(&payload, &tuning);
    let mut quarry_damage = 0.0_f32;
    let mut casualties = injuries;
    for result in &outcome.results {
        if result.force == QUARRY_FORCE {
            // **The DAMAGE, not the bodies.** The resolver's own `killed + wounded` has already been
            // divided by `durability` and clamped, so it cannot be banked; the raw flow can, and the
            // ledger below is what turns it into whole animals — this turn's and every earlier
            // turn's together.
            quarry_damage += result.damage_dealt;
        } else {
            casualties.killed += result.killed;
            casualties.wounded += result.wounded;
        }
    }
    HuntFight {
        // **Whole animals** — the same rule `quantise_animal_take` exists for. A fractional kill left
        // un-floored would let `killed_biomass` and the reported `killed` count disagree, so the
        // ledger hands back only completed bodies and keeps the remainder.
        brought_down: wounds.strike(quarry_damage, &quarry.profile, stayed),
        casualties,
        fought: true,
        wounds,
    }
}

/// **The hunt's own hazard, resolved into people** (`docs/plan_hunt_through_combat.md` §4.6) —
/// [`HuntingParty::injury_damage_per_animal`] × the animals engaged, put through
/// [`crate::combat::units_brought_down`] — the very primitive that turns an enemy's damage into
/// bodies — and **added into** the fight's own [`FightCasualties`] rather than reported beside them.
/// The party's `lethality` scales it, so a detached raid's `expedition_danger_multiplier` reaches it
/// like every other blow.
///
/// **It is not gated**: a ravine does not have to beat your `defense` to hurt you, so
/// [`crate::combat::strike_damage`] is deliberately absent here.
///
/// # Always WOUNDED, never killed — and that is the gate doing its job, not an exemption
///
/// What makes a blow lethal is an attacker landing it past your defense; a hunt's hazards have no
/// attacker, so they land as `wounded` — the resolver's own name for a loss a force takes and keeps.
/// It is also what keeps this *texture* instead of a second combat model:
/// [`crate::components::available_workers`] **floors** a cohort's working scalar, so **any** fatality,
/// however fractional, costs a whole worker of throughput on the spot. A four-hunter raid that lost a
/// quarter of its capacity the first time it engaged a rabbit would be a balance change wearing a
/// flavour note's clothes.
fn hunt_injuries(stayed: f32, hunters: f32, party: &HuntingParty) -> FightCasualties {
    let hazard = party.injury_damage_per_animal.max(0.0) * stayed * party.tuning.lethality;
    FightCasualties {
        killed: NO_FATAL_HUNTING_ACCIDENTS,
        wounded: combat::units_brought_down(hazard, &party.hunter, hunters),
    }
}

/// The baseline hunting hazard's fatal share — see [`hunt_injuries`] for why it is `0` rather than
/// the resolver's severity split.
const NO_FATAL_HUNTING_ACCIDENTS: f32 = 0.0;

/// The hunting party's side of a hunt fight — the aggressor.
const HUNTING_PARTY_FORCE: ForceId = ForceId(0);
/// The herd's side of a hunt fight — the defender.
const QUARRY_FORCE: ForceId = ForceId(1);
/// The party's one contingent key, matching the `person` row of the creatures roster.
const HUNTER_CONTINGENT: &str = "person";
/// The herd's one contingent key. The species name is *not* used: it would make the key vary by
/// quarry for no consumer, and nothing downstream reads it.
const QUARRY_CONTINGENT: &str = "quarry";

/// **The stream seed a forecast's fight hands to [`crate::combat::resolve_fight`], and never reads.**
///
/// A projection cannot know the tick it is projecting, so it cannot compose the per-event seed the
/// live take will use — and it no longer tries: a forecast resolves at [`HuntDraw::Quantile`], which
/// makes **no draw at all**, so the payload's `seed` is inert by construction rather than by luck.
/// It survives because `FightPayload` requires the field.
///
/// It used to be the constant a forecast *did* draw with, on the reasoning that the shipped
/// `hit_chance` of `1.0` made it unobservable. True, and the wrong shape: it would have started
/// reporting an arbitrary sample as the answer the moment a sub-1 chance was authored. See
/// [`HuntDraw`].
pub const FORECAST_FIGHT_SEED: u64 = 0;

/// **THE whole-animal quantiser** (intensification ladder slice 8) — the one place a take is rounded
/// to animals, shared by **every rung**: `systems::hunt_take` (the resident band + the scout's
/// replenish), `systems::expedition_take_biomass` (the hunting party + its forward-simulated
/// forecast), the **pen**'s corral-tend branch of `advance_labor_allocation`, and
/// [`forecast_expected_take`] (the pre-commit preview) — so no two hunters or keepers of a herd can
/// disagree about what a kill is.
///
/// **Whole animals everywhere — you cannot slaughter half a cow either.** The pen quantises for the
/// exact reason the hunt does; see [`managed_yield_biomass`] for why it nonetheless *reads* steady.
///
/// **A herd is not a fluid.** You kill *whole animals*, and a big animal is a lot of food at once:
/// ```text
/// affordable = floor(policy_ceiling / body_mass)   // whole animals the herd can spare
/// carryable  = floor(collection    / body_mass)    // whole animals the party can haul
/// killed     = min(affordable, max(1, carryable))  IF affordable >= 1, else 0
/// carried    = min(killed × body_mass, collection)
/// wasted     = killed × body_mass − carried
/// ```
///
/// Two clauses carry the whole design:
/// - **`max(1, carryable)` — you cannot half-kill a mammoth.** A party that cannot carry a whole
///   animal may still take one and **waste most of it**. That is not forbidden and not hidden: it is
///   what makes party size mean *"how much of the kill do you keep"* (one hunter keeps 80% of a boar,
///   33% of a steppe runner, **5%** of a mammoth; ~20 hunters bring a whole mammoth home).
/// - **`affordable < 1` ⇒ take **nothing** and WAIT.** When the herd cannot yet spare a whole animal
///   the hunt pauses while it regrows — constant escapement, discretised. The rhythm falls straight
///   out as `body_mass / MSY` turns per animal at the operating point: small game every turn,
///   boar/deer ~2, mammoth ~7 — *then you eat for a week*.
///
/// `body_mass <= 0` is impossible (`FaunaConfig::validate` requires it finite & positive on every
/// species) and would mean a herd of infinitely many animals; it takes **nothing** and screams in a
/// debug build, rather than letting `floor(x / 0) = inf` strip the whole stock in one turn.
/// **QUANTISATION STAYS IN BIOMASS SPACE — never derive an animal count from a food number.**
///
/// This *used* to be a precision note, on the premise that flooring in provisions-space and in
/// biomass-space give the same animal count "because the conversion is a positive linear factor and a
/// positive linear factor cancels". Since the per-species yield vector
/// ([`crate::fauna_config::HuntYield`], `docs/plan_hunt_yield_model.md`) that premise is **false**:
/// an **inedible** species has `provisions_per_biomass == 0`, which is not a *positive* factor, so
/// `floor(food_ceiling / food_per_animal)` is `0/0` — `NaN`, not 3 wolves. Counting animals off food
/// is therefore not a rounding hazard but a **category error**, and every quantiser
/// ([`quantise_animal_take`], [`whole_animals`], [`hunt_haul_workers`]) takes biomass.
///
/// The epsilon survives because the *precision* problem it solved is still real wherever a biomass
/// ratio is compared against a separately-computed one: `0.02` is stored as `0.019999999552965164`,
/// so 60 hunters on a mammoth read `2400 / 800 = 3.0` exactly one way and `2.9999998` the other.
/// `floor` then says **3 animals** vs **2**, and the preview under-quotes the take by a whole mammoth
/// (caught by `exported_snapshot_fields_reproduce_band_hunt_take`).
///
/// `f32` carries ~1.2e-7 relative precision and each side takes a couple of roundings, so `1e-6`
/// clears the error by an order of magnitude while staying far below any *real* gap: a take genuinely
/// this close to a whole animal would need `body_mass` tuned to seven significant figures. And it is
/// self-correcting either way — `carried` is clamped to `collection` regardless, so at worst one more
/// animal is counted killed and its meat reported wasted.
const ANIMAL_COUNT_EPSILON: f32 = 1e-6;

/// **How many whole animals the source can spare** — [`whole_animals`] of a policy ceiling, exposed
/// so a take path can bound its **engagement** by it before the fight
/// (`docs/plan_hunt_through_combat.md` §1: the escapement floor bounds `engaged`, not `killed`).
///
/// That ordering is what makes **restraint free**. Bounding the kill instead would have a party at
/// its floor engage normally, take casualties, wear its kit, and then decline to kill what it had
/// already fought — and killing without taking is denial, not restraint.
///
/// **It is deliberately not applied on the forecast paths, and that costs nothing**: the quantiser
/// clamps the kill by this same `affordable` whatever the engagement was, so trimming the engagement
/// cannot move the *take* by a single animal — only the casualties and the kit wear, neither of which
/// a yield forecast models. `forecast == actual` is unaffected.
///
/// A non-finite `body_mass` (never reachable — `FaunaConfig::validate` pins it finite-positive) is
/// answered `0`, matching [`quantise_animal_take`]'s own guard.
pub fn animals_affordable(policy_ceiling: f32, body_mass: f32) -> f32 {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        return 0.0;
    }
    whole_animals(policy_ceiling.max(0.0), body_mass)
}

/// Whole animals in `available` biomass, at `body_mass` each — `floor`, with
/// [`ANIMAL_COUNT_EPSILON`] of relative slop so the same take counts the same animals whether it is
/// quantised in biomass or in provisions.
fn whole_animals(available: f32, body_mass: f32) -> f32 {
    let ratio = available / body_mass;
    (ratio * (1.0 + ANIMAL_COUNT_EPSILON)).floor()
}

pub fn quantise_animal_take(
    policy_ceiling: f32,
    collection: f32,
    body_mass: f32,
    brought_down: f32,
    stop: EngagementStop,
) -> AnimalTake {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        debug_assert!(
            false,
            "body_mass must be finite and positive (FaunaConfig::validate enforces it); got {body_mass}"
        );
        return AnimalTake::default();
    }
    let ceiling = policy_ceiling.max(0.0);
    let collection = collection.max(0.0);
    let affordable = whole_animals(ceiling, body_mass);
    if affordable < 1.0 {
        // The herd cannot spare a whole animal — wait for it to regrow. THE mechanic.
        return AnimalTake::default();
    }
    let carryable = whole_animals(collection, body_mass);
    // `max(1.0)`: a party that can't carry one still takes one — and wastes the rest.
    //
    // **`brought_down` is the THIRD bound, and it is THE FIGHT** (`docs/plan_hunt_through_combat.md`
    // §4): the animals the party actually put on the ground, which the engagement bound (§2) already
    // caps from above because you cannot bring down an animal you never reached. It arrives already
    // whole — [`resolve_hunt_fight`] floors it — so `killed` below stays integral and
    // `killed_biomass` cannot disagree with the reported count. A pen passes [`f32::INFINITY`]: a
    // penned animal is not stalked and not fought.
    //
    // **The carry arm is the ONE thing a denial raid drops** ([`EngagementStop`],
    // `docs/plan_denial_raid.md` §1): a hunting party stops engaging once its pack is full, a denial
    // party never stops. The `carried` line below is untouched by the choice, so a raid still banks
    // whatever it can haul and the rest becomes [`AnimalTake::wasted`].
    let killed = match stop {
        EngagementStop::WhenPackFull => affordable
            .min(carryable.max(1.0))
            .min(brought_down.max(0.0)),
        EngagementStop::Never => affordable.min(brought_down.max(0.0)),
    };
    let killed_biomass = killed * body_mass;
    let carried = killed_biomass.min(collection);
    AnimalTake {
        // `killed` is bounded by `ceiling / body_mass` and both are finite, so the cast is safe.
        killed: killed as u32,
        carried,
        wasted: (killed_biomass - carried).max(0.0),
    }
}

/// **WHICH of the take's four bounds actually stopped this hunt**
/// (`docs/plan_hunt_through_combat.md` §6.6) — a fact the resolution produces, carried on the hunt
/// report so a player can tell *"there was nothing left to take"* from *"we could not reach them"*.
///
/// It exists because §11's first open question has a specific failure mode: for most species the
/// escapement floor binds long before engagement does, so an `engage_rate` authored too low becomes a
/// **second, invisible floor**. Naming the bound is what makes that visible instead of mysterious.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HuntTakeBound {
    /// **Reach** — the party brought down everything that stayed, and could have used more hands
    /// (§2). This is the bound an under-authored `engage_rate` hides behind.
    Engagement,
    /// **The escapement floor** — the herd could not spare another whole animal above where the crew
    /// stops. Also the bound of a turn that takes nothing at all and waits for regrowth.
    Floor,
    /// **Carry** — more went down than the party could haul, so the remainder is
    /// [`AnimalTake::wasted`].
    Carry,
    /// **The fight** — the party reached more animals than it could put on the ground (§4). At the
    /// gate this is the *"hunters die, nothing is killed"* outcome §6.5 warns about before launch.
    Fight,
}

impl HuntTakeBound {
    /// Stable wire/detail key, the `as_str` convention every wire enum in this crate uses.
    pub fn as_str(self) -> &'static str {
        match self {
            HuntTakeBound::Engagement => "engagement",
            HuntTakeBound::Floor => "floor",
            HuntTakeBound::Carry => "carry",
            HuntTakeBound::Fight => "fight",
        }
    }
}

/// **Which bound [`quantise_animal_take`] actually hit**, read off the same terms and through the
/// same [`whole_animals`] helper — so the reported bound and the paid take cannot disagree about
/// what "affordable" or "carryable" mean.
///
/// **Precedence on a tie is `Floor → Carry → Fight/Engagement`, and it is stated rather than
/// incidental.** Ties are common (a crew sized exactly to its ceiling), and the first arm is the one
/// that is true of the *source*: when the herd has nothing more to spare, that is the fact the player
/// needs whatever else was also tight.
///
/// The last two arms split one `min`: `brought_down` is capped by `stayed` from above, so bringing
/// down **everything that stayed** means reach was the limit, and bringing down less means the fight
/// was.
pub fn hunt_take_bound(
    policy_ceiling: f32,
    collection: f32,
    body_mass: f32,
    stayed: f32,
    brought_down: f32,
    stop: EngagementStop,
) -> HuntTakeBound {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        return HuntTakeBound::Floor;
    }
    let affordable = whole_animals(policy_ceiling.max(0.0), body_mass);
    // The same `max(1.0)` the quantiser applies: a party that cannot carry a whole animal still takes
    // one, so carry does not *bind* below one body — it produces waste instead.
    //
    // **A denial raid can never be carry-bound**, because the quantiser's carry arm is exactly what
    // [`EngagementStop::Never`] drops: the pack still decides what comes home, but it stops no kill,
    // so reporting `Carry` here would name a bound the take did not hit.
    let carryable = match stop {
        EngagementStop::WhenPackFull => whole_animals(collection.max(0.0), body_mass).max(1.0),
        EngagementStop::Never => f32::INFINITY,
    };
    let brought_down = brought_down.max(0.0);
    if affordable <= carryable.min(brought_down) {
        HuntTakeBound::Floor
    } else if carryable <= brought_down {
        HuntTakeBound::Carry
    } else if brought_down < stayed.max(0.0) {
        HuntTakeBound::Fight
    } else {
        HuntTakeBound::Engagement
    }
}

/// **The carry crew for a whole-animal (hunt) source** — the number of haulers a hunt needs to carry
/// home the *peak animal drop its ceiling allows* without waste, and the biomass-space mirror of the
/// client's compose-panel `_max_useful_workers`. This is `SourceYield.workers_needed`'s haul component
/// for every whole-animal source (wild hunt, pastoral herd, pen), and it is deliberately **not** the
/// lumpy [`AnimalTake::carried`] of any single turn.
///
/// # Why not the this-turn take
///
/// A slow breeder whose one-turn regrowth is lighter than one body (a Wild Aurochs, `r ≈ 0.09`, body
/// 80) drops **0** animals on a wait turn while the room above its floor rebuilds — so inverting
/// `carried` collapses `workers_needed` to `0` (and, for a managed herd, to the bare herder count via
/// [`crate::intensification::source_crew_needed`]).
/// That contradicts the *same row's* `wasted_yield`, which correctly reports the waste an understaffed
/// crew leaves standing: the panel then says `workersNeeded: 1` beside a 50%-`wastedYield` at 1 worker
/// — *drop workers* and *add workers* on one row. Sizing the crew off the **ceiling** instead makes
/// the two agree, and makes the band panel's overstaff note equal the compose panel's stepper cap.
///
/// # The peak drop
///
/// The most whole animals `ceiling` can drop is `floor(ceiling / body) + 1`: the whole animals the
/// room already covers, plus the one its partial body becomes on the turn regrowth tips it over (the
/// `+1` — the same `floor(ceiling/body)+1` the client counts). Carrying that peak needs
/// `ceil(peak_biomass / per_worker)` haulers:
///
/// ```text
/// peak_animals = floor(ceiling / body) + 1
/// peak_biomass = peak_animals × body
/// crew         = ceil(peak_biomass / per_worker)
/// ```
///
/// `ceiling` is the take's own bound — the stance's [`hunt_escapement_ceiling`], the number
/// [`quantise_animal_take`] divides — so the crew and the waste can never disagree about what the
/// source offered. **On a full herd that is the crew which would clear it to the floor in ONE turn,
/// which is a big number, and it is the honest one** (`docs/plan_harvest_floor.md` §7.6): it is what
/// makes *"this crew cannot draw the herd that low"* expressible instead of silently true. Do not
/// clamp it.
///
/// **`per_worker` is the crew's EFFECTIVE throughput, dip included** — a crew gentling a herd hauls
/// [`crate::intensification::LadderConfig::build_dip`]`×` what a hunting one does (§3.1 put the dip on
/// the crew), so it takes proportionally more of them to clear the same room. Every caller passes
/// `per_worker_biomass_capacity × build_dip`, which is also what the client's
/// `SourceForecast.max_useful_workers` divides by; passing the undipped rate sizes a harvesting crew
/// and then pays it the building take.
///
/// Units are free — pass all three in biomass, or all three in provisions (the ratios
/// are scale-invariant, so the provisions-space call in [`forecast_source_yield`] and the biomass-space
/// calls in the labor arm agree). Naturally `>= 1` for any finite-positive `body`/`per_worker` (since
/// `peak_animals >= 1`); a degenerate `body`/`per_worker` (≤ 0 — unreachable, `FaunaConfig::validate`
/// pins `body_mass` positive and the per-worker levers are positive config) yields `0`.
pub fn hunt_haul_workers(ceiling: f32, body: f32, per_worker: f32) -> u32 {
    if !body.is_finite() || body <= 0.0 || !per_worker.is_finite() || per_worker <= 0.0 {
        return 0;
    }
    let peak_biomass = peak_animal_drop(ceiling, body) * body;
    (peak_biomass / per_worker).ceil() as u32
}

/// **The most whole animals a `ceiling` can drop in one turn** — `floor(ceiling / body) + 1`, the one
/// definition of the peak drop, shared by [`hunt_haul_workers`] and [`hunt_engage_workers`] so the
/// two crew terms can never be sized against different drops.
///
/// The `+1` is the partial body still standing: on the turn regrowth tips it over the room covers one
/// more animal than `floor` sees. `body` is assumed finite-positive — every caller checks it first.
fn peak_animal_drop(ceiling: f32, body: f32) -> f32 {
    (ceiling.max(0.0) / body).floor() + 1.0
}

/// **The ENGAGEMENT crew for a whole-animal (hunt) source** — how many hunters it takes to bring the
/// ceiling's peak animal drop *into contact* in one turn, the third unit in `workers_needed`'s
/// `max()` (`docs/plan_hunt_through_combat.md` §2).
///
/// It is the exact inverse of [`animals_engaged`]: that floors `workers × engage_rate × build_dip` to
/// whole animals, so the crew reaching `n` of them is `ceil(n / (engage_rate × build_dip))`.
///
/// ```text
/// crew = ceil(peak_animal_drop(ceiling, body) / (engage_rate × build_dip))
/// ```
///
/// # Why it cannot be folded into the haul crew
///
/// The two terms scale on **different units** — hauling is per *biomass* (one hauler carries 40),
/// engaging is per *animal* (one hunter reaches 10 fowl or 0.05 mammoths) — so neither dominates
/// across the roster, exactly as the herder term does not. A Wild Fowl herd with ~470 head above its
/// floor is 61 biomass: **two** haulers clear it and **47** hunters are needed to reach it, so sizing
/// the crew on carry alone told the player *"more hands would be idle"* about the very hands the take
/// was short of. The mammoth inverts it (one hunter reaches the peak drop; twenty are needed to carry
/// it home).
///
/// # The dip rides the crew here too
///
/// `build_dip` is the rung's `yield_fraction_while_building`, the same term [`animals_engaged`] and
/// [`hunt_haul_workers`] apply (`docs/plan_harvest_floor.md` §3.1): hands spent gentling a herd are
/// hands not stalking it, so it takes proportionally more of them to corner the same drop. Pass
/// [`crate::intensification::NO_BUILD_UNDERWAY_DIP`] where nothing is being built.
///
/// # A source with no engagement stage reports no engagement crew
///
/// `0` for a **pen** and for the plant web, whose `engage_rate` is [`f32::INFINITY`]
/// ([`FaunaConfig::engage_rate_for`], [`SourceYieldForecast::managed`]) — a penned animal is not
/// stalked and a plant is not either — so the `max()` collapses to the haul term and neither web
/// regresses. Same for a degenerate `body`/rate.
///
/// Units on `ceiling`/`body` are free, exactly as they are for [`hunt_haul_workers`]: an animal count
/// is a ratio, so a provisions-space call and a biomass-space one give the same crew.
pub fn hunt_engage_workers(ceiling: f32, body: f32, engage_rate: f32, build_dip: f32) -> u32 {
    if !body.is_finite() || body <= 0.0 {
        return 0;
    }
    let reach = engage_rate.max(0.0) * build_dip.max(0.0);
    if !reach.is_finite() || reach <= 0.0 {
        // No engagement stage (a pen, a plant) — or a dip of zero, which is not a crew size but the
        // absence of one. Either way this term has nothing to say and the `max()` keeps the others.
        return 0;
    }
    (peak_animal_drop(ceiling, body) / reach).ceil() as u32
}

/// **THE take-side crew for a whole-animal (hunt) source** — `max(`[`hunt_haul_workers`]`,
/// `[`hunt_engage_workers`]`)`, and the single seam every `workers_needed` on the animal web sizes
/// its take half with (the assign-time seed in [`forecast_source_yield`] and the resolved Hunt arm of
/// `advance_labor_allocation`), so the two cannot answer differently.
///
/// **Two jobs, one crew, two units** — reach the animals, then carry them home. It is the take-side
/// half of [`crate::intensification::source_crew_needed`]'s `max(standing, take)`, which adds the
/// third: the herders who mind a managed herd whether or not it is killed from this turn.
/// `max()`, never `+`: one crew covering its busiest job.
pub fn hunt_take_workers(
    ceiling: f32,
    body: f32,
    per_worker: f32,
    engage_rate: f32,
    build_dip: f32,
) -> u32 {
    hunt_haul_workers(ceiling, body, per_worker).max(hunt_engage_workers(
        ceiling,
        body,
        engage_rate,
        build_dip,
    ))
}

/// **The most animals a herd puts back per turn on the way down** — what a denial raid has to
/// out-kill, in *animals*, at **every** point of the path from the stock it stands at now to the
/// point of no return (`docs/plan_denial_raid.md` §3.1).
///
/// # It is NOT the regrowth at the herd's current biomass, and that is the whole reason it is a
/// function
///
/// The logistic curve peaks at the food peak, so a herd standing **above** `K/2` regrows *faster*
/// as the raid draws it down: a party sized on the stock it starts against would out-kill the herd
/// for a few turns and then stall on the way past `K/2`, forever. Below `K/2` the reverse holds —
/// regrowth falls as the raid works — so there the current stock is the binding reading and the
/// raid accelerates.
///
/// Both cases are `sustainable_yield(B, K)` = `net_biomass_delta(min(B, K/2), K)`, which is exactly
/// *"the highest per-turn growth on the interval this raid traverses"*. Reading it through that seam
/// keeps the requirement on the **same** curve [`regrow_biomass`] advances the herd with, rather
/// than opening a second copy of the model beside it.
///
/// `0` for a herd already **past** its Allee point (its growth flow is negative — there is nothing
/// left to outpace) and for a degenerate `body_mass`.
pub fn herd_replacement_animals(
    biomass: f32,
    carrying_capacity: f32,
    body_mass: f32,
    ecology: &EcologyConfig,
) -> f32 {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        return 0.0;
    }
    sustainable_yield(biomass, carrying_capacity, ecology) / body_mass
}

/// **The one hunter that turns a tie into a decline.** A party whose kills exactly *equal* the
/// herd's replacement drives it nowhere, so the requirement is the smallest integer **strictly**
/// past the quotient — `floor(x) + 1`, never `ceil(x)`, which is wrong by one at precisely the
/// round value a tuner is most likely to author. Same idiom, and the same reason, as
/// [`peak_animal_drop`]'s `+ 1`.
const OUTPACE_BY_ONE_HUNTER: u32 = 1;

/// **The smallest party whose kills outpace a herd's replacement** — the denial raid's requirement,
/// and the number its launch sheet opens on (`docs/plan_denial_raid.md` §3.1).
///
/// ```text
/// per_hunter = engage_rate × (1 − wariness)             // animals one hunter kills a turn
/// party      = floor(replacement_animals / per_hunter) + 1
/// ```
///
/// # It rounds UP, always, and a floor here is the defect it exists to prevent
///
/// `8.3` hunters is **9**. Rounding the other way hands the player a party that provably does not
/// work — it ties or loses against the regrowth every turn — while the sheet presents it as the
/// answer. `floor(x) + 1` is the up-rounding *and* the strictness in one operator: see
/// [`OUTPACE_BY_ONE_HUNTER`].
///
/// # `None` means NO party can, which is a different statement from a large number
///
/// A quarry with `wariness >= 1` (every animal breaks off before contact) or `engage_rate <= 0`
/// (nothing is ever brought into contact) cannot be denied by any number of hunters, and the honest
/// answer is the absence of one rather than [`u32::MAX`].
///
/// # It is a BOUND, not the verdict
///
/// It is linear in the party, so it sees neither the whole-animal quantiser, nor the fight, nor
/// [`animals_engaged`]'s `max(1)` floor (which lets a lone hunter reach one mammoth where this
/// arithmetic reads `0.05`). Those live in the raid's own forward simulation, which is what the
/// launch table reports; this sizes how far that table has to run. See
/// `snapshot::subsistence::denial_estimate_entries`.
pub fn denial_party_needed(
    replacement_animals: f32,
    engage_rate: f32,
    wariness: f32,
) -> Option<u32> {
    let per_hunter = engage_rate.max(0.0) * (1.0 - wariness).clamp(0.0, 1.0);
    // The NaN test is written out rather than left to `<=`, which a NaN passes. `FaunaConfig`
    // validates both dials finite, so this is belt-and-braces — but the arm it would otherwise fall
    // into answers `Some(1)`, i.e. *"one hunter is enough"*, which is the most dangerous possible
    // reading of an unreadable quarry. An **infinite** `per_hunter` is a different thing entirely and
    // must NOT come here: it is the "no engagement stage" reading (a pen, the plant web), where the
    // quotient is `0` and one hunter genuinely does suffice.
    if per_hunter.is_nan() || per_hunter <= 0.0 {
        return None;
    }
    // `f32 as u32` saturates rather than wrapping, so an absurd quotient answers the largest party
    // the type can state instead of a tiny one.
    let quotient = replacement_animals.max(0.0) / per_hunter;
    Some((quotient.floor() as u32).saturating_add(OUTPACE_BY_ONE_HUNTER))
}

/// **RETIRED: `hunt_provisions(biomass, &FaunaConfig, mult)`** — the single global biomass→provisions
/// conversion, `take × hunt.provisions_per_biomass × output_multiplier`.
///
/// It routed *one* product off a *global* rate, so it could not express a species whose take is not
/// food (a wolf) nor pay a second product at all. Replaced by
/// [`FaunaConfig::hunt_yield_for`]`(&herd.species)`[`.apply(take, mult)`](HuntYield::apply), which
/// yields **both** components in one call — you cannot convert the meat and forget the pelt.
///
/// It stays a doc gravestone rather than a deprecated shim because the species is *always* in scope
/// at every former call site: keeping a global-rate escape hatch is exactly how a wolf would start
/// paying venison on the one path that missed the sweep.
///
/// The properties it carried are unchanged and now live on [`HuntYield::apply`]: shared by
/// `systems::hunt_take` (which quantises the result onto the larder's `Scalar` grid) and the
/// pre-commit forecast so the two can't drift, and FOOD income stays fully fractional — a few hunters
/// may yield `< 1` provision per turn.
///
/// The single **per-species** conversion for a hunt take. Sugar over
/// `fauna.hunt_yield_for(&herd.species).apply(..)` for the many sites that hold a `&Herd`.
pub fn herd_hunt_yield(herd: &Herd, fauna: &FaunaConfig) -> HuntYield {
    fauna.hunt_yield_for(&herd.species)
}

/// **The quarry a live herd presents to a fight** — the species' body from
/// [`FaunaConfig::quarry_fight_for`] carrying **this herd's** accumulated [`Herd::wounds`].
///
/// **THE seam between the herd's damage ledger and the fight**, and the sugar every path holding a
/// `&Herd` must use: `quarry_fight_for` alone hands back an *un-hunted* animal, so a take path that
/// skipped this would silently restart the mammoth's wounds every turn — the stateless behaviour the
/// accumulator exists to replace, failing quietly rather than loudly.
pub fn herd_quarry_fight(herd: &Herd, fauna: &FaunaConfig) -> QuarryFight {
    fauna
        .quarry_fight_for(&herd.species)
        .with_wounds(herd.wounds)
}

/// The **gross** managed yield a **penned** herd hands its keeper each turn: the pen's MSY
/// ([`pen_yield_biomass`]) through the herd's **own species vector** — a corralled wolf pays pelts,
/// exactly as a wild one does, because the pen changes the *intensity* (a managed rate on a herd you
/// own), never the *product*. Gross, deliberately — the pen's feed ([`pen_upkeep`]) is a *separate*
/// debit on the keeper's larder, so the player can see both halves of the trade instead of one netted
/// number.
///
/// Shared by the corral-tend branch of `advance_labor_allocation` (the payout) and [`hunt_forecast`]
/// (the forecast + the "what will this herd pay once penned?" projection), so forecast == actual.
pub(crate) fn corral_yield(
    herd: &Herd,
    fauna: &FaunaConfig,
    output_multiplier: f32,
) -> YieldAccounts {
    herd_hunt_yield(herd, fauna).apply(pen_yield_biomass(herd, fauna), output_multiplier)
}

/// **May this species ONLY be worked at floor `0`?** — the ONE seam the order validator
/// (`assign_labor` / the `tame`/`corral` command paths) and the snapshot's raid-estimate table both
/// read, so what the client is offered and what the sim accepts cannot drift into two rules.
///
/// **The yield flags gate the PRODUCTS, not the dial.** A wolf (`edible == false`,
/// `tradeable == true`) may be worked at any floor and is paid in pelts, because every floor is a
/// meaningful depth at which to collect pelts — that is the whole product/intensity split
/// (`docs/plan_hunt_yield_model.md`). So this is `false` for a wolf, and it is not a general
/// per-species filter.
///
/// **The only rule:** a [`HuntYield::yields_nothing`] species — a pure pest, worth neither meat nor
/// pelt — can only be worked at floor `0`. Every other floor would be a depth at which to collect
/// nothing; the one coherent instruction left is *make it stop*. No shipped species hits this branch
/// today (the wolf trades), so it is pinned by a synthetic-config unit test rather than a roster one.
///
/// It replaced `hunt_policies_for`, which returned the *list of stances* this predicate was really
/// expressing — a list that could not survive the stances, and that a continuous floor cannot be
/// checked against anyway (`0.42` is in no list).
pub fn species_requires_denial(hunt_yield: HuntYield) -> bool {
    hunt_yield.yields_nothing()
}

/// Pre-commit yield forecast for hunting `herd` with `per_worker_biomass_capacity` biomass/hunter
/// (`labor_config.json` `hunt.per_worker_biomass_capacity`). Mirrors `systems::hunt_take` exactly:
/// same resolved ecology/capacity ([`herd_ecology`] / [`herd_capacity`]), same per-policy ceilings,
/// same biomass clamp, same biomass→provisions conversion. A **corralled** herd forecasts its corral
/// yield with one worker (see `SourceYieldForecast::tended`). The band Hunt labor has no carry limit
/// (it passes `carry_room_biomass = f32::INFINITY` to `hunt_take`), so the forecast models no carry
/// clamp either — a hunting *expedition*'s carry cap is out of scope.
pub(crate) fn hunt_forecast(
    herd: &Herd,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    // The party that would work this herd — its per-hunter profile (kit composed in) and the resolver
    // tuning it fights at, so the preview resolves the SAME fight the take will
    // (`docs/plan_hunt_through_combat.md` §4).
    party: &HuntingParty,
    output_multiplier: f32,
) -> SourceYieldForecast {
    // The pen's yield is **gross** — its feed is debited separately (wire: `penUpkeep`).
    //
    // A pen collapses the *policy* axis (the herd is yours) but **not** the worker cap: the keeper
    // still has to carry the meat home, so `managed` gets the same real per-hunter throughput a wild
    // hunt is capped by, and the pen's take is `min(pen MSY, hunters × throughput)` (slice 7 — the
    // Field's twin, and the same fix: the old `::tended` claimed one keeper collected the whole pen).
    //
    // **The species' yield vector, resolved ONCE for the whole forecast** — the product half of
    // `yield = product × intensity` (`docs/plan_hunt_yield_model.md`). Every ceiling below is a
    // biomass rate put through it, so a wolf's food ceilings are honestly `0` on every rung *and* its
    // trade ceilings carry the real number — which is exactly why the forecast is a `YieldAccounts` and
    // not a food scalar with a sibling.
    let hunt_yield = herd_hunt_yield(herd, fauna);
    if herd.is_corralled() {
        return SourceYieldForecast::managed(
            corral_yield(herd, fauna, output_multiplier),
            hunt_yield.apply(per_worker_biomass_capacity.max(0.0), output_multiplier),
            // A pen is butchered in whole animals like everything else (slice 8) — it just breeds fast
            // enough (`pen_gain` ×3) that its MSY clears a body every turn, so it reads steady without
            // being exempt. See `managed_yield_biomass`.
            hunt_yield.apply(herd.body_mass, output_multiplier),
        );
    }
    SourceYieldForecast {
        per_worker_yield: hunt_yield.apply(per_worker_biomass_capacity.max(0.0), output_multiplier),
        // The quantum that makes this preview pulse exactly as the take does (slice 8).
        body_mass_yield: hunt_yield.apply(herd.body_mass, output_multiplier),
        // The engagement throughput the take is bounded by, so preview and take agree on how many
        // animals the party can even reach.
        engage_rate: fauna.engage_rate_for(&herd.species),
        // ...and the fight that decides how many of those actually go down (§4).
        // ...carrying **this herd's** accumulated wounds, so a single-turn preview says "this is the
        // turn it finally goes down" on the turn it does (`herd_quarry_fight`, §4.2).
        fight: Some((*party, herd_quarry_fight(herd, fauna))),
        // **The TERMS of the take, not a set of answers.** `ceiling_at(floor, improvement)` composes
        // them into exactly what `hunt_take` computes — the herd's own `K` (`herd_capacity`, never
        // the raw field) and its CURRENT biomass, so the forecast and the take read the same stock.
        biomass: herd.biomass,
        carrying_capacity: herd_capacity(herd, fauna),
        // What one unit of this herd's biomass is worth, in both currencies — the species' vector,
        // resolved once for the whole forecast.
        per_biomass_yield: hunt_yield.apply(ONE_UNIT_OF_BIOMASS, output_multiplier),
        // A wild/pastoral herd IS drawn down — there is a standing stock to stop short of, which is
        // the whole of what a floor decides.
        managed_production: None,
        // The animal web's two build dips (`Tame`, then `Corral`), as the FACTORS they are: the
        // ceiling a builder pays is `room × dip`, applied inside `ceiling_at` so no caller can apply
        // it in the wrong order (§2.2).
        build_dips: BuildDips::for_branch(ladder, RungBranch::Animal),
        // The Corral rung's PAYOFF (`corralYield`) projected for a still-un-penned herd: the pen's
        // **sustained MSY** on the improved (pen) ecology — the long-run rate that shows the
        // Sustain < Tame < Corral ladder, NOT the one-turn constant-escapement take. Same
        // `biomass_before_regrowth` basis and `carrying_capacity` the wild `ceiling` closure uses, so
        // the ONLY difference from Sustain is the pen ecology's boosted `r`. The **actual** pen take
        // stays constant-escapement (`corral_yield`) — see the `is_corralled()` early-return.
        managed_yield: hunt_yield.apply(
            sustainable_yield(
                herd.biomass_before_regrowth,
                herd.carrying_capacity,
                &pen_ecology_for(herd, fauna),
            ),
            output_multiplier,
        ),
        // The Tame rung's PAYOFF (the pastoral analog of `managed_yield` above): the pastoral
        // **sustained MSY** — what a Sustain hunt pays once this herd is tamed — projected for a
        // still-wild herd on the same basis as Sustain, so the only difference is the pastoral `r`.
        // `ceiling_tame` is the during-building dip; this is the `→ +Y` the client renders. A wild
        // herd whose species never tames (`wild` ceiling) reads its wild MSY here, which is fine — the
        // client only surfaces it on the Tame affordance, hidden on a non-tameable herd.
        pastoral_yield: hunt_yield.apply(
            sustainable_yield(
                herd.biomass_before_regrowth,
                herd.carrying_capacity,
                &pastoral_ecology_for(herd, fauna),
            ),
            output_multiplier,
        ),
    }
}

/// One turn's positive logistic regrowth increment (>= 0) for a group of `biomass`
/// toward `cap`. The healthy branch of `net_biomass_delta`. Also the forage patch's
/// regrowth curve (`forage::regrow_patch`) — plants have no Allee crash, so a depleted
/// patch always recovers via this branch (see `forage.rs`).
pub(crate) fn logistic_regrowth(biomass: f32, cap: f32, regrowth_rate: f32) -> f32 {
    if cap <= 0.0 || biomass <= 0.0 {
        return 0.0;
    }
    (regrowth_rate * biomass * (1.0 - biomass / cap)).max(0.0)
}

/// One turn of **reseeding pure-logistic regrowth**: the new biomass a plant stock at `biomass`
/// reaches, growing toward `cap` at `regrowth_rate`, after first being lifted to a **reseed floor**
/// (`reseed_floor_fraction × cap`).
///
/// The single source of the plant regrowth curve, shared by `forage::regrow_patch` (the human-edible
/// stock) and `graze::regrow_graze_patch` (the animal-edible one). Plants have **no Allee crash**
/// (that is `net_biomass_delta`, the animal curve), so a depleted patch always recovers. The floor is
/// what makes "always recovers" true rather than merely intended: `logistic_regrowth` returns `0` at
/// `biomass == 0`, so a stock driven to exactly `0` would otherwise stick there forever. The lift is a
/// `max()`, so a healthy stock is untouched; and the floor is kept below `collapse_fraction`, so a
/// stripped patch still reads Collapsing — it just cannot be held at `0`.
pub(crate) fn reseeding_logistic_regrowth(
    biomass: f32,
    cap: f32,
    regrowth_rate: f32,
    reseed_floor_fraction: f32,
) -> f32 {
    let reseeded = biomass.max(reseed_floor_fraction * cap);
    let delta = logistic_regrowth(reseeded, cap, regrowth_rate);
    (reseeded + delta).clamp(0.0, cap)
}

/// Net per-turn biomass change with **critical depensation**. Above the Allee
/// threshold (`collapse_fraction * cap`) the group regrows logistically; below it the
/// group is non-viable and declines by `collapse_rate` of its biomass each turn — an
/// irreversible crash to local extinction even without further hunting (the overhunting
/// point of no return). Also sizes a Sustain/Surplus follow's take (via `.max(0.0)`):
/// a collapsing group yields no surplus.
pub(crate) fn net_biomass_delta(biomass: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    if cap <= 0.0 || biomass <= 0.0 {
        return 0.0;
    }
    let allee = ecology.collapse_fraction * cap;
    if biomass < allee {
        -(ecology.collapse_rate * biomass)
    } else {
        logistic_regrowth(biomass, cap, ecology.regrowth_rate)
    }
}

/// The most-productive biomass for logistic regrowth is K/2 (the Maximum Sustainable
/// Yield point), where `r·B·(1−B/K)` peaks.
///
/// **Public since slice 8, because it is now the sim's OPERATING POINT and not merely an interior
/// detail of the regrowth curve.** Sustain's escapement point **is** `K · MSY_BIOMASS_FRACTION`
/// ([`sustain_ceiling`]) — so a harvested herd *lives* here (Sustain settles it at `K/2`), and any
/// test that wants to measure a rung at the point a running herd actually stands has to seat against
/// this number. Exporting it is what stops those fixtures from spelling `0.5` by hand and silently
/// drifting if the curve's peak ever moves.
pub const MSY_BIOMASS_FRACTION: f32 = 0.5;

/// **CONSTANT ESCAPEMENT** — the biomass standing above a floor, and therefore the whole take
/// ceiling on **both** food webs (`docs/plan_harvest_floor.md` §1): a take is
/// `min(crew throughput, escapement_ceiling(floor, B, K))`, and the floor is the only thing a stance
/// (later, a labor assignment) contributes to it.
///
/// **`r`-INDEPENDENT, which is the property that makes it the right shape** — and is why this
/// function takes no [`EcologyConfig`]. Unlike MSY (`r·K/4`, [`peak_regrowth`]) the answer does not
/// depend on how fast the stock breeds, so a take can no longer be a *rate* that outruns the
/// standing stock, and *"where do I stop"* stops being a question about the growth curve. The sim
/// already harvests a penned herd exactly this way ([`pen_yield_biomass`], floor
/// [`MSY_BIOMASS_FRACTION`]); this is that rule generalised to a floor the caller names.
///
/// The answer is `≤ biomass` for any `floor_fraction ≥ 0`, so a caller's standing-stock clamp can
/// never bind — keep such clamps as belt-and-braces, not as load-bearing terms.
pub fn escapement_ceiling(floor_fraction: f32, biomass: f32, carrying_capacity: f32) -> f32 {
    (biomass - floor_fraction * carrying_capacity).max(0.0)
}

/// Max Sustainable Yield ceiling: regrowth evaluated at the most-productive biomass (K/2),
/// so a resource AT carrying capacity still has a positive sustainable harvest (Sustain draws it
/// down to K/2 and holds it there). Below the Allee threshold this is 0 (don't harvest a
/// collapsing resource — inherited from net_biomass_delta's negative branch, clamped). Distinct
/// from net_biomass_delta, which stays the ACTUAL per-turn biomass change used by regrow_biomass.
pub(crate) fn sustainable_yield(biomass: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    net_biomass_delta(biomass.min(cap * MSY_BIOMASS_FRACTION), cap, ecology).max(0.0)
}

/// The **most biomass a group can add in one turn**, whatever its current state: the logistic curve
/// evaluated at its peak (K/2, the MSY point — the same curve `regrow_biomass` applies, so no second
/// copy of the model). A group above or below K/2 regrows *less*, and a sub-Allee one *loses*
/// biomass, so this bounds every herd's per-turn growth from above.
///
/// `pub(crate)` for the hunt-trip forecast's O(1) "this party cannot possibly fill its pack"
/// short-circuit (`systems::hunt_trip_provisions_bound`), which needs a **true upper bound** on the
/// biomass a herd can hand a party over the forecast horizon without simulating it turn by turn.
pub(crate) fn peak_regrowth(cap: f32, ecology: &EcologyConfig) -> f32 {
    logistic_regrowth(cap * MSY_BIOMASS_FRACTION, cap, ecology.regrowth_rate)
}

/// Apply one turn of critical-depensation dynamics toward the herd's carrying capacity
/// and refresh its `ecology_phase`. A sub-threshold group declines instead of regrowing;
/// the caller despawns it once it falls below the viability floor.
///
/// `pub(crate)` because the hunt-trip forecast (`systems::hunt_trip_forecast`) runs a herd forward
/// turn by turn on a **clone** and must apply the *same* regrowth the live `advance_herds` does —
/// re-deriving the curve there would let the pre-launch estimate drift from the sim.
pub(crate) fn regrow_biomass(herd: &mut Herd, fauna: &FaunaConfig) {
    // Capture the pre-regrowth biomass so the Population-stage Sustain take can size its rate against
    // what the herd *was*, not what it grew to this turn (slice 8b — `Herd::biomass_before_regrowth`).
    herd.biomass_before_regrowth = herd.biomass;
    // The herd's OWN ecology + capacity (`herd_ecology` / `herd_capacity`): wild `r` is now
    // **per-species** (fast small game ~0.35, slow megafauna ~0.04), pastoral 0.25, penned 0.90 — the
    // whole husbandry ladder is just this curve run at a different rate.
    let ecology = herd_ecology(herd, fauna);
    let cap = herd_capacity(herd, fauna);
    // A domesticated (managed) group is immune to the overhunting collapse: it always
    // regrows logistically toward capacity and never crosses into the depensation crash.
    let delta = if herd.is_domesticated() {
        logistic_regrowth(herd.biomass, cap, ecology.regrowth_rate)
    } else {
        net_biomass_delta(herd.biomass, cap, &ecology)
    };
    // **The pen's growth is what the FEED buys.** A penned herd cannot graze, so an unfed one does not
    // grow at all (`docs/plan_corral_managed_population.md` §3.1: *fed → regrow; underfed → shrink*) —
    // its growth scales with the fraction of last turn's feed its keeper actually paid, and
    // `advance_husbandry` then applies the wasting on top. Without this the pen's own `r` = 0.60
    // out-runs the 10%/turn starvation four times over: an "unfed" herd would keep growing, park at
    // `K/2`, and quietly pay its keeper a yield for feed they never bought.
    // `pen_fed_fraction` is 1.0 for every herd that is not penned, so this is inert elsewhere.
    let delta = delta * herd.pen_fed_fraction.clamp(0.0, PEN_FULLY_FED);
    // **A TOTALLY-ABANDONED pastoral herd does not regrow** (`docs/plan_fauna_neglect_escape.md` §2.4,
    // option B). An owned, unfenced herd with ZERO herders last turn (`herded_fraction == NOT_HERDED` —
    // the same one-turn-lag signal the pen reads, written by the labor arm and reset by
    // `advance_husbandry`) is suppressed to zero growth, so the shed drives it to the extinction floor
    // and it goes **fully feral** (ownership clears on shed-to-zero) instead of persisting at a leaky
    // ~0.6·K equilibrium. This is a **binary abandonment gate, not a scaling**: PARTIAL neglect
    // (`herded_fraction > 0` — understaffed but still herded) keeps normal regrowth and settles at its
    // labor-supported capacity as a stable smaller TAME herd, mirroring the pen's untended/tended split.
    // A corralled herd is handled by `pen_fed_fraction` above (`!is_corralled()` here), and a wild herd
    // has no owner, so this is inert for both.
    let abandoned_pastoral =
        herd.owner.is_some() && !herd.is_corralled() && herd.herded_fraction <= NOT_HERDED;
    let delta = if abandoned_pastoral { 0.0 } else { delta };
    herd.biomass = (herd.biomass + delta).clamp(0.0, cap);
    herd.refresh_ecology_phase(fauna);
}

fn to_entry(herd: &Herd) -> HerdTelemetryEntry {
    HerdTelemetryEntry {
        id: herd.id.clone(),
        label: herd.label.clone(),
        species: herd.species.clone(),
        size_class: herd.size_class.as_str().to_string(),
        // All fauna are huntable in Phase B; Phase C/D may differentiate.
        huntable: true,
        ecology_phase: herd.ecology_phase.as_str().to_string(),
        domestication: herd.domestication_progress,
        corralled: herd.is_corralled(),
        corral_progress: herd.corral_progress,
        position: herd.position(),
        biomass: herd.biomass,
        route_length: herd.route_length() as u32,
        next_position: herd.next_position(),
    }
}

/// Radius (hexes) of the neighbourhood `build_route` searches to pull a migratory anchor onto the
/// most fertile nearby ground (Grazing 2b-i §4.1). Small — a local nudge that shifts the anchor onto
/// grass without redrawing the spiral's shape.
const ANCHOR_FERTILITY_SCAN_RADIUS: u32 = 1;

/// Hex radius around a seed suitable-tile within which a migratory herd's loiter anchors are drawn —
/// its regional **home range** (`build_migratory_route`). Big enough to give the herd a real
/// migration circuit across a biome patch, small enough to keep it *in* one region rather than
/// scattered map-wide.
const MIGRATORY_HOME_RANGE_RADIUS: u32 = 12;

/// Minimum hex spacing between chosen migratory anchors, so they are distinct loiter patches rather
/// than adjacent tiles (`build_migratory_route`).
const MIGRATORY_ANCHOR_MIN_SPACING: u32 = 3;

/// The fewest distinct anchors a migratory route may have (mirrors `build_route`'s own `< 3` floor):
/// below this the herd falls back to the seed-centred spiral, which guarantees ≥3 or `None`.
const MIGRATORY_MIN_ANCHORS: usize = 3;

/// Long migratory route: a jittered spiral of `steps` waypoints around `origin`, keeping only land
/// tiles and **biasing each anchor onto fertile ground** so the route connects pasture (2b-i §4.1).
/// Returns `None` if fewer than 3 distinct points land.
///
/// Fertility is read **directly from `graze_config.capacity_by_biome`** for each tile's terrain, NOT
/// from the live `GrazeRegistry`: `build_route` runs inside `spawn_initial_herds`, which is ordered
/// **before** `spawn_initial_graze` in the Startup chain, so no graze patches exist yet. The bias is
/// deterministic (a pure argmax over the neighbourhood — no extra RNG draw).
#[allow(clippy::too_many_arguments)]
fn build_route(
    origin: UVec2,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze_config: &GrazeConfig,
    rng: &mut SmallRng,
    steps: u32,
) -> Option<Vec<UVec2>> {
    let mut points = Vec::new();
    let radius = rng.gen_range(4..=12) as f32;
    let mut angle = rng.gen_range(0.0..TAU);
    for _ in 0..steps {
        let dx = angle.cos() * radius;
        let dy = angle.sin() * radius;
        angle = (angle + rng.gen_range(0.4..=1.2)) % TAU;
        let candidate = clamp_to_grid(
            origin.x as i32 + dx.round() as i32,
            origin.y as i32 + dy.round() as i32,
            width,
            height,
        );
        if let Some(pos) = candidate {
            // Shift the spiral point onto the richest pasture in its immediate neighbourhood, so a
            // migratory herd loiters where the grass is.
            if let Some(anchor) =
                most_fertile_land_near(pos, registry, tiles, graze_config, width, height)
            {
                if points.last().copied() != Some(anchor) {
                    points.push(anchor);
                }
            }
        }
    }
    if points.len() < 3 {
        None
    } else {
        Some(points)
    }
}

/// The host-biome-suitable tiles for `def`: the union of the pre-bucketed [`FoodModule`] lists for
/// each of its `host_biomes` keys, restored to global (y, x) scan order (each bucket is already in
/// scan order, but concatenating disjoint buckets is not). Empty when the map hosts none of the
/// species' biomes — the graceful-fallback case `build_migratory_route` handles.
fn suitable_tiles_for(
    def: &SpeciesDef,
    suitable_by_module: &BTreeMap<FoodModule, Vec<UVec2>>,
) -> Vec<UVec2> {
    let mut out: Vec<UVec2> = Vec::new();
    for module in FoodModule::VARIANTS {
        if def.hosts_biome(module.as_str()) {
            if let Some(bucket) = suitable_by_module.get(&module) {
                out.extend_from_slice(bucket);
            }
        }
    }
    out.sort_by_key(|p| (p.y, p.x));
    out
}

/// Build a migratory herd's route so its loiter **anchors** sit on tiles suitable for its species
/// (`module_at ∈ host_biomes`), drawn from a regional home range, with the migration legs crossing
/// whatever less-suitable ground lies between. The sim's Loiter↔Migrate machine steps through the
/// anchors in order, so placing them on suitable patches is the whole mechanic — no movement-code
/// change. Guaranteed to return a route of ≥3 anchors, or `None` only where `build_route` would.
///
/// `suitable` is the precomputed host-biome tile slice for THIS species (see [`suitable_tiles_for`]).
/// Determinism: only the passed seeded `rng`, no `HashMap`/`HashSet` iteration, every tie broken by
/// an explicit `(y, x)` key — mirroring `build_route` / `spawn_short_range_game`.
#[allow(clippy::too_many_arguments)]
fn build_migratory_route(
    base: UVec2,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze_config: &GrazeConfig,
    suitable: &[UVec2],
    rng: &mut SmallRng,
    steps: u32,
    wrap: bool,
) -> Option<Vec<UVec2>> {
    // 1. No host-biome tiles on this map: fall back to the start-anchored spiral so the species
    //    still spawns somewhere rather than vanishing.
    if suitable.is_empty() {
        return build_route(
            base,
            width,
            height,
            registry,
            tiles,
            graze_config,
            rng,
            steps,
        );
    }
    // 2. Seed the home range on a random suitable tile.
    let seed = suitable[rng.gen_range(0..suitable.len())];
    // 3. The regional home range: suitable tiles within `MIGRATORY_HOME_RANGE_RADIUS` of the seed
    //    (which is always in the pool, its own distance being 0).
    let pool: Vec<UVec2> = suitable
        .iter()
        .copied()
        .filter(|&tile| {
            hex_distance_wrapped(seed, tile, width, wrap) <= MIGRATORY_HOME_RANGE_RADIUS
        })
        .collect();
    // 4. Too few suitable patches nearby to form a circuit: a biome-blind spiral, but centred on the
    //    in-biome seed so the herd is at least in/near its range.
    if pool.len() < MIGRATORY_MIN_ANCHORS {
        return build_route(
            seed,
            width,
            height,
            registry,
            tiles,
            graze_config,
            rng,
            steps,
        );
    }
    // 5. Greedily pick spaced-out anchors from the shuffled pool, `seed` first (so it is anchor #0).
    let mut shuffled = pool;
    shuffled.shuffle(rng);
    let mut accepted = vec![seed];
    for tile in shuffled {
        if accepted.len() as u32 >= steps {
            break;
        }
        if accepted
            .iter()
            .all(|&a| hex_distance_wrapped(a, tile, width, wrap) >= MIGRATORY_ANCHOR_MIN_SPACING)
        {
            accepted.push(tile);
        }
    }
    if accepted.len() < MIGRATORY_MIN_ANCHORS {
        return build_route(
            seed,
            width,
            height,
            registry,
            tiles,
            graze_config,
            rng,
            steps,
        );
    }
    // 6. Order the anchors into a walkable circuit by nearest-neighbour chaining from `seed`, so
    //    consecutive migration legs stay short rather than teleporting across the home range.
    Some(order_anchors_nearest_neighbor(accepted, seed, width, wrap))
}

/// Order `anchors` into a walkable circuit by nearest-neighbour chaining starting from `start`:
/// repeatedly append the nearest not-yet-chained anchor (wrap-aware hex distance; ties by `(y, x)`
/// ascending for determinism). `start` is guaranteed to be present (it is anchor #0).
fn order_anchors_nearest_neighbor(
    anchors: Vec<UVec2>,
    start: UVec2,
    width: u32,
    wrap: bool,
) -> Vec<UVec2> {
    let mut remaining = anchors;
    let start_idx = remaining.iter().position(|&a| a == start).unwrap_or(0);
    let mut ordered = Vec::with_capacity(remaining.len());
    ordered.push(remaining.remove(start_idx));
    while !remaining.is_empty() {
        let current = *ordered.last().expect("ordered is seeded with start");
        let mut best_idx = 0usize;
        let mut best_key = (u32::MAX, u32::MAX, u32::MAX);
        for (i, &cand) in remaining.iter().enumerate() {
            let key = (
                hex_distance_wrapped(current, cand, width, wrap),
                cand.y,
                cand.x,
            );
            if key < best_key {
                best_key = key;
                best_idx = i;
            }
        }
        ordered.push(remaining.remove(best_idx));
    }
    ordered
}

/// The land tile of the highest **graze capacity** (from the config table) within
/// [`ANCHOR_FERTILITY_SCAN_RADIUS`] of `center` — the fertile-anchor argmax `build_route` uses. Ties
/// resolve by `hex_range_tiles` scan order (deterministic). `None` only when no tile in the
/// neighbourhood is land. Uses `wrap = false` to match `build_route`'s clamp-based spiral geometry.
fn most_fertile_land_near(
    center: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze_config: &GrazeConfig,
    width: u32,
    height: u32,
) -> Option<UVec2> {
    let mut best: Option<(UVec2, f32)> = None;
    for tile in hex_range_tiles(center, ANCHOR_FERTILITY_SCAN_RADIUS, width, height, false) {
        if !is_land_tile(tile, registry, tiles) {
            continue;
        }
        let capacity = tile_terrain(tile, registry, tiles)
            .map(|terrain| graze_config.capacity_for(terrain))
            .unwrap_or(NO_GRAZE_CAPACITY);
        if best
            .map(|(_, best_cap)| capacity > best_cap)
            .unwrap_or(true)
        {
            best = Some((tile, capacity));
        }
    }
    best.map(|(pos, _)| pos)
}

/// The tile's `TerrainType` at `pos`, or `None` off-map. Used to read a tile's graze capacity from
/// the config table at spawn (before the `GrazeRegistry` exists).
fn tile_terrain(
    pos: UVec2,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
) -> Option<sim_runtime::TerrainType> {
    registry
        .index(pos.x, pos.y)
        .and_then(|entity| tiles.get(entity).ok())
        .map(|tile| tile.terrain)
}

/// Short roaming route for wild game: `steps` waypoints within a small radius of
/// `origin` (radius grows with route length). `steps == 1` yields a single-tile,
/// stationary group (which the client draws with no trail). Returns `None` only if
/// `origin` itself is not land.
fn build_short_route(
    origin: UVec2,
    steps: u32,
    width: u32,
    height: u32,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Vec<UVec2>> {
    if !is_land_tile(origin, registry, tiles) {
        return None;
    }
    let mut points = vec![origin];
    let target = steps.max(1) as usize;
    if target <= 1 {
        return Some(points);
    }
    // Wander radius scales with route length (big game ~2-3 tiles, small ~1).
    let radius = target.saturating_sub(1).max(1) as i32;
    let max_attempts = target * 4;
    let mut attempts = 0;
    while points.len() < target && attempts < max_attempts {
        attempts += 1;
        let dx = rng.gen_range(-radius..=radius);
        let dy = rng.gen_range(-radius..=radius);
        let Some(pos) = clamp_to_grid(origin.x as i32 + dx, origin.y as i32 + dy, width, height)
        else {
            continue;
        };
        if is_land_tile(pos, registry, tiles) && !points.contains(&pos) {
            points.push(pos);
        }
    }
    Some(points)
}

/// Food module for a tile position, or `None` for water / unclassified tiles.
fn module_at(position: UVec2, registry: &TileRegistry, tiles: &Query<&Tile>) -> Option<FoodModule> {
    let entity = registry.index(position.x, position.y)?;
    let tile = tiles.get(entity).ok()?;
    classify_food_module(tile)
}

fn chebyshev_distance(a: UVec2, b: UVec2) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    dx.max(dy)
}

fn clamp_to_grid(x: i32, y: i32, width: u32, height: u32) -> Option<UVec2> {
    let max_x = width as i32 - 1;
    let max_y = height as i32 - 1;
    if max_x < 0 || max_y < 0 {
        return None;
    }
    let clamped_x = x.clamp(0, max_x) as u32;
    let clamped_y = y.clamp(0, max_y) as u32;
    Some(UVec2::new(clamped_x, clamped_y))
}

/// Is this tile **salt** water, and is it **fresh** water? The shore predicate's vocabulary, stated
/// once.
///
/// Salt = `WATER` **without** `FRESHWATER`; fresh = `WATER` **with** it. The salt half is exactly
/// the rule `TileWorld::is_ocean` states in `hydrology.rs` (the drainage code's "only the ocean is a
/// sink"), in the same tag vocabulary — a landlocked lake, an inland sea and a navigable river are
/// all `WATER | FRESHWATER` and are therefore *not* ocean.
fn water_kind(tags: TerrainTags) -> (bool, bool) {
    if !tags.contains(TerrainTags::WATER) {
        return (false, false);
    }
    let fresh = tags.contains(TerrainTags::FRESHWATER);
    (!fresh, fresh)
}

/// What kinds of open water `position` borders across its six hex sides, as `(has_salt, has_fresh)`
/// — the **shore predicate** a site rule is tested against
/// ([`crate::fauna_config::SpeciesDef::adjacent_water`]).
///
/// It **reads** the real coastline geometry (the terrain tags worldgen stamped) and never edits
/// terrain, so the sole authority on where the water is stays worldgen's.
fn adjacent_water_kinds(
    position: UVec2,
    width: u32,
    height: u32,
    wrap: bool,
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
) -> (bool, bool) {
    (0..HEX_DIRECTION_COUNT).fold((false, false), |(salt, fresh), dir| {
        let tags = hex_neighbor(position.x, position.y, dir, width, height, wrap)
            .and_then(|(nx, ny)| registry.index(nx, ny))
            .and_then(|entity| tiles.get(entity).ok())
            .map(|tile| tile.terrain_tags)
            .unwrap_or_else(TerrainTags::empty);
        let (neighbor_salt, neighbor_fresh) = water_kind(tags);
        (salt || neighbor_salt, fresh || neighbor_fresh)
    })
}

fn is_land_tile(position: UVec2, registry: &TileRegistry, tiles: &Query<&Tile>) -> bool {
    registry
        .index(position.x, position.y)
        .and_then(|entity| tiles.get(entity).ok())
        .map(|tile| !tile.terrain_tags.contains(TerrainTags::WATER))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::NO_IMPROVEMENT_UNDERWAY;
    use crate::fauna_config::ShoreRequirement;
    use crate::intensification::{NO_BUILD_UNDERWAY_DIP, RUNG_COMPLETE};
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};
    use crate::terrain::terrain_definition;
    use sim_runtime::TerrainType;

    // ---- The shore predicate: salt vs fresh --------------------------------------------------

    /// A 3x1 strip `[water, land, land]` whose only water neighbour carries `tags`, plus the
    /// `Query<&Tile>` machinery `adjacent_water_kinds` wants. Returns `(has_salt, has_fresh)` for
    /// the land tile at `(1, 0)`.
    fn adjacent_water_kinds_beside(water: TerrainType) -> (bool, bool) {
        use bevy::ecs::system::SystemState;

        const WIDTH: u32 = 3;
        const HEIGHT: u32 = 1;

        let mut world = World::new();
        let mut tiles = Vec::new();
        for x in 0..WIDTH {
            let terrain = if x == 0 { water } else { TerrainType::Tundra };
            let entity = world
                .spawn(Tile {
                    position: UVec2::new(x, 0),
                    terrain,
                    terrain_tags: terrain_definition(terrain).tags,
                    ..Default::default()
                })
                .id();
            tiles.push(entity);
        }
        let registry = TileRegistry {
            tiles,
            width: WIDTH,
            height: HEIGHT,
        };

        let mut state: SystemState<Query<&Tile>> = SystemState::new(&mut world);
        let query = state.get(&world);
        adjacent_water_kinds(UVec2::new(1, 0), WIDTH, HEIGHT, false, &registry, &query)
    }

    /// **A lake is not a coast.** `InlandSea` is `WATER | FRESHWATER`, so a land tile beside one
    /// reads fresh-but-not-salt and cannot satisfy a `Salt` site rule — the bug that let a Grey Seal
    /// colony haul out on a one-hex freshwater lake. `ContinentalShelf` is `WATER` without
    /// `FRESHWATER` (the ocean) and does satisfy it.
    #[test]
    fn a_salt_shore_rule_rejects_a_lake_and_accepts_the_ocean() {
        let (lake_salt, lake_fresh) = adjacent_water_kinds_beside(TerrainType::InlandSea);
        assert!(!lake_salt, "an inland sea is fresh water, never salt");
        assert!(lake_fresh, "an inland sea is fresh water");
        assert!(
            !ShoreRequirement::Salt.satisfied_by(lake_salt, lake_fresh),
            "a marine forager must not seat itself beside a landlocked freshwater lake"
        );

        let (shelf_salt, shelf_fresh) = adjacent_water_kinds_beside(TerrainType::ContinentalShelf);
        assert!(
            shelf_salt,
            "the continental shelf is the ocean — salt water"
        );
        assert!(!shelf_fresh, "the ocean is not fresh water");
        assert!(
            ShoreRequirement::Salt.satisfied_by(shelf_salt, shelf_fresh),
            "an ocean shore is exactly what a seal colony hauls out on"
        );
    }

    /// Drive the **real** short-range spawn path (`spawn_game_group_at`) `DRAWS` times on a
    /// `boreal_arctic` land tile whose single water neighbour carries `water`, and report which
    /// species it seated.
    ///
    /// A sweep over generated maps cannot prove this: measured over the six standard seeds, a
    /// seal-hosting land tile bordering **only** fresh water is 0–10 tiles against 59–100 salt-shore
    /// ones, and the map-wide game cap seats just 2–3 colonies per map — so the buggy case is a few
    /// percent per seed and the sweep passes on the old predicate by luck. The fixture removes the
    /// luck: here fresh-only shore is the **only** ground on offer.
    fn species_spawned_beside(water: TerrainType) -> std::collections::BTreeSet<String> {
        use bevy::ecs::system::SystemState;

        /// Wide enough that `build_short_route` always finds land for a multi-anchor species, so a
        /// missing species means the site rule dropped it and never that routing failed.
        const SIZE: u32 = 7;
        /// Enough draws that a candidate surviving the filter is seen with overwhelming probability
        /// (the pick is uniform over a handful of candidates).
        const DRAWS: u64 = 200;
        /// The one water hex, orthogonally adjacent to the target below.
        const WATER_TILE: UVec2 = UVec2::new(3, 2);
        /// The land hex under test — its only water neighbour is [`WATER_TILE`].
        const TARGET: UVec2 = UVec2::new(3, 3);

        let mut world = World::new();
        let mut tiles = Vec::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let pos = UVec2::new(x, y);
                // Tundra is an explicit `boreal_arctic` arm of `classify_food_module`.
                let terrain = if pos == WATER_TILE {
                    water
                } else {
                    TerrainType::Tundra
                };
                let entity = world
                    .spawn(Tile {
                        position: pos,
                        terrain,
                        terrain_tags: terrain_definition(terrain).tags,
                        ..Default::default()
                    })
                    .id();
                tiles.push(entity);
            }
        }
        let registry = TileRegistry {
            tiles,
            width: SIZE,
            height: SIZE,
        };
        let fauna = FaunaConfig::builtin();

        let mut state: SystemState<Query<&Tile>> = SystemState::new(&mut world);
        let query = state.get(&world);
        (0..DRAWS)
            .filter_map(|draw| {
                let mut rng = SmallRng::seed_from_u64(draw);
                spawn_game_group_at(
                    TARGET,
                    FoodModule::BorealArctic.as_str(),
                    0,
                    &fauna,
                    SIZE,
                    SIZE,
                    false,
                    &registry,
                    &query,
                    &mut rng,
                )
                .map(|herd| herd.species)
            })
            .collect()
    }

    /// **The regression: no seal colony on a lakeshore.** Driven through the real spawn path, on a
    /// `boreal_arctic` tile whose only water is an `InlandSea`. Under the pre-fix any-`WATER`
    /// predicate the seal survives the filter and this fails; the `ContinentalShelf` arm is what
    /// keeps it from passing vacuously (the same fixture, salt water, **must** seat seals).
    #[test]
    fn the_spawn_path_seats_seals_on_an_ocean_shore_and_never_on_a_lakeshore() {
        const SEAL: &str = "Grey Seals";

        let beside_ocean = species_spawned_beside(TerrainType::ContinentalShelf);
        assert!(
            beside_ocean.iter().any(|species| species == SEAL),
            "the fixture must be able to seat a seal at all, or the lakeshore assertion proves \
             nothing — got {beside_ocean:?}"
        );

        let beside_lake = species_spawned_beside(TerrainType::InlandSea);
        assert!(
            !beside_lake.iter().any(|species| species == SEAL),
            "a seal colony hauled out beside a landlocked freshwater lake — the shore rule must \
             read SALT water, not any water; got {beside_lake:?}"
        );
        assert!(
            !beside_lake.is_empty(),
            "the lakeshore tile must still host the biome's other game, or the filter is dropping \
             everything rather than just the marine forager"
        );
    }

    /// The freshwater species are **unaffected** by the lake case — the Silt Catfish rides `Any` and
    /// must keep its pre-split behaviour exactly, and a hypothetical `Fresh` species wants the lake.
    #[test]
    fn fresh_and_any_shore_rules_are_satisfied_by_a_lake() {
        let (salt, fresh) = adjacent_water_kinds_beside(TerrainType::InlandSea);
        assert!(ShoreRequirement::Any.satisfied_by(salt, fresh));
        assert!(ShoreRequirement::Fresh.satisfied_by(salt, fresh));
        assert!(ShoreRequirement::None.satisfied_by(salt, fresh));
    }

    /// A tile with no water at all satisfies nothing but `None` — including `Any`, which is the
    /// state the pre-split `requires_adjacent_water: true` expressed.
    #[test]
    fn a_dry_site_satisfies_only_the_absent_rule() {
        let (salt, fresh) = adjacent_water_kinds_beside(TerrainType::Tundra);
        assert!(!salt);
        assert!(!fresh);
        assert!(ShoreRequirement::None.satisfied_by(salt, fresh));
        for requirement in [
            ShoreRequirement::Any,
            ShoreRequirement::Salt,
            ShoreRequirement::Fresh,
        ] {
            assert!(
                !requirement.satisfied_by(salt, fresh),
                "{} must not be satisfied by dry ground",
                requirement.as_str()
            );
        }
    }

    #[test]
    fn ecology_phase_string_roundtrips() {
        for phase in [
            EcologyPhase::Thriving,
            EcologyPhase::Stressed,
            EcologyPhase::Collapsing,
        ] {
            assert_eq!(EcologyPhase::from_key(phase.as_str()), phase);
        }
    }

    #[test]
    fn ecology_phase_from_unknown_key_defaults_thriving() {
        assert_eq!(EcologyPhase::from_key(""), EcologyPhase::Thriving);
        assert_eq!(EcologyPhase::from_key("bogus"), EcologyPhase::Thriving);
    }

    #[test]
    fn roam_state_string_roundtrips() {
        for roam in [
            RoamState::GrazeWander,
            RoamState::Loiter { turns_left: 7 },
            RoamState::Migrate,
        ] {
            let restored = RoamState::from_mode(roam.mode_key(), roam.loiter_turns_left());
            assert_eq!(restored, roam);
        }
    }

    #[test]
    fn size_class_string_roundtrips() {
        for size in [SizeClass::Small, SizeClass::Big, SizeClass::Migratory] {
            assert_eq!(SizeClass::from_key(size.as_str()), size);
        }
    }

    // ---- Herder-requirement hysteresis -----------------------------------------------------

    /// A managed herd fixture with `body_mass == 1` (so `biomass == head count`) and an owner, so
    /// `stabilize_herders_needed` treats it as managed.
    fn managed_herd_with_heads(heads: f32) -> Herd {
        let mut herd = Herd::new(
            "aurochs_test".to_string(),
            "Wild Aurochs".to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            heads,
            10_000.0,
            0.05,
            0.09,
            1.0, // body_mass: one animal per unit biomass
        );
        herd.owner = Some(FactionId(1));
        herd
    }

    /// The core anti-flicker property: a managed herd whose head count breathes ±1 across an
    /// `animals_per_herder` boundary reports a STABLE `herders_needed` once bumped up — it does not
    /// drop back on a one-animal dip. A Wild Aurochs (`animals_per_herder = 12`) near 12 head.
    #[test]
    fn herder_requirement_is_stable_across_a_one_animal_oscillation() {
        const APH: f32 = 12.0;
        const BAND: f32 = APH * 0.25; // the shipped default deadband, in animals
        let mut herd = managed_herd_with_heads(13.0);
        // First stabilized turn seeds the raw ceil: ceil(13 / 12) = 2.
        assert_eq!(herd.stabilize_herders_needed(APH, BAND), 2);
        // Now oscillate 13 → 11 → 12 → 13 (the lumpy Sustain kill): it must HOLD at 2 the whole way,
        // never flickering back to 1.
        for heads in [11.0_f32, 12.0, 13.0, 11.0, 13.0] {
            herd.biomass = heads;
            assert_eq!(
                herd.stabilize_herders_needed(APH, BAND),
                2,
                "held at 2 through the ±1 oscillation at {heads} head",
            );
        }
    }

    /// A herd that genuinely GROWS past the boundary bumps the requirement up **immediately** —
    /// under-herding is harmful, so it responds at once.
    #[test]
    fn herder_requirement_rises_immediately_on_real_growth() {
        const APH: f32 = 12.0;
        const BAND: f32 = APH * 0.25;
        let mut herd = managed_herd_with_heads(12.0);
        assert_eq!(herd.stabilize_herders_needed(APH, BAND), 1); // ceil(12/12) = 1
        herd.biomass = 25.0; // clearly a third herder's worth (ceil(25/12) = 3)
        assert_eq!(herd.stabilize_herders_needed(APH, BAND), 3);
    }

    /// The requirement drops only after a CLEAR fall — past the lower rung's ceiling by more than the
    /// deadband — not on a one-animal dip.
    #[test]
    fn herder_requirement_drops_only_after_a_clear_fall() {
        const APH: f32 = 12.0;
        const BAND: f32 = APH * 0.25; // 3 animals
        let mut herd = managed_herd_with_heads(20.0);
        assert_eq!(herd.stabilize_herders_needed(APH, BAND), 2); // ceil(20/12) = 2
                                                                 // Just below the 1-herder ceiling (12) but within the deadband: 10 > 12 − 3 = 9 → HOLD at 2.
        herd.biomass = 10.0;
        assert_eq!(herd.stabilize_herders_needed(APH, BAND), 2);
        // Below the deadband floor (≤ 9): a genuine drop → step down to ceil(8/12) = 1.
        herd.biomass = 8.0;
        assert_eq!(herd.stabilize_herders_needed(APH, BAND), 1);
    }

    /// A wild herd isn't yours to maintain — it stays `0`, and `herd_herders_needed` reads `0`.
    #[test]
    fn a_wild_herd_needs_no_herders() {
        const APH: f32 = 12.0;
        let mut herd = managed_herd_with_heads(50.0);
        herd.owner = None; // wild again
        assert_eq!(herd.stabilize_herders_needed(APH, APH * 0.25), 0);
        assert_eq!(herd.herders_needed, 0);
    }

    /// **`wariness 0` is an EXACT identity that consumes no randomness** — the property the whole
    /// slice rests on, asserted rather than assumed. Observationally: the answer cannot depend on the
    /// seed, because no draw is made. If someone "simplifies" the early return into a
    /// `gen_bool(0.0)`, the values still match but the stream advances, and every downstream draw in
    /// the turn shifts — a corruption no yield assertion would catch.
    #[test]
    fn zero_wariness_never_draws_and_never_changes_the_take() {
        for engaged in [1.0, 2.0, 17.0, 300.0] {
            for seed in [0, 1, u64::MAX, 0x5EED_1234_5678_9ABC] {
                assert_eq!(
                    animals_that_stay(engaged, 0.0, HuntDraw::Seeded(seed)),
                    engaged,
                    "wariness 0 must return the engaged count untouched, for any seed"
                );
            }
        }
    }

    /// **A pen and a plant have no engagement stage, so they have no retreat either.** An infinite
    /// engagement passes through at any wariness — and must, since iterating it would not terminate.
    #[test]
    fn an_unbounded_engagement_has_nothing_to_retreat_from() {
        for wariness in [0.0, 0.5, 1.0] {
            assert_eq!(
                animals_that_stay(f32::INFINITY, wariness, HuntDraw::Seeded(7)),
                f32::INFINITY,
                "a source with no engagement stage cannot retreat"
            );
        }
    }

    /// **The draw is deterministic in its seed and bounded by the engagement** — the two halves of
    /// "unpredictable to the player, reproducible for the sim". Paired with a liveness assertion,
    /// because a retreat stage that always returned `engaged` would also satisfy the bound.
    #[test]
    fn the_retreat_draw_is_seeded_bounded_and_actually_bites() {
        const ENGAGED: f32 = 200.0;
        for seed in [1_u64, 99, 4242] {
            let once = animals_that_stay(ENGAGED, 0.5, HuntDraw::Seeded(seed));
            assert_eq!(
                once,
                animals_that_stay(ENGAGED, 0.5, HuntDraw::Seeded(seed)),
                "the same seed must reproduce the same retreat"
            );
            assert!(
                (0.0..=ENGAGED).contains(&once),
                "the retreat cannot invent or destroy engaged animals: {once}"
            );
        }
        // Liveness: at wariness 1 every engaged animal breaks off, so the stage is genuinely wired.
        assert_eq!(animals_that_stay(ENGAGED, 1.0, HuntDraw::Seeded(3)), 0.0);
        // ...and it is not merely all-or-nothing at the midpoint.
        let half = animals_that_stay(ENGAGED, 0.5, HuntDraw::Seeded(11));
        assert!(
            half > 0.0 && half < ENGAGED,
            "a mid wariness must leave some and take some: {half}"
        );
    }

    /// **`wariness 0` has no spread to quantile either** — the retreat's half of the degenerate
    /// identity (`docs/plan_hunt_through_combat.md` §6.4). At every quantile the forecast reads the
    /// engaged count *untouched*, which is what makes the shipped range a point and
    /// `forecast == actual` an exact identity rather than an approximation.
    ///
    /// **This is the sensitive guard.** The end-to-end wire test reads the take after the
    /// whole-animal quantiser, which absorbs a small perturbation; nothing rounds here.
    #[test]
    fn zero_wariness_has_no_spread_for_the_forecast_to_report() {
        for engaged in [1.0, 2.0, 17.0, 300.0] {
            for sigmas in [-3.0_f32, -2.0, 0.0, 2.0, 3.0] {
                assert_eq!(
                    animals_that_stay(engaged, 0.0, HuntDraw::Quantile { sigmas }),
                    engaged,
                    "wariness 0 must return the engaged count untouched at every quantile"
                );
            }
        }
    }

    /// **The retreat's quantile IS the binomial the live draw samples** — its mean matches the
    /// draw's mean over many seeds, and the band widens with `sigmas` and never leaves the support.
    /// Paired with the liveness half, because a quantile stuck at `engaged` would satisfy the
    /// ordering alone.
    #[test]
    fn the_retreat_quantile_tracks_the_draw_it_replaces() {
        const ENGAGED: f32 = 400.0;
        const WARINESS: f32 = 0.25;
        const SEEDS: u32 = 200;

        let expected = animals_that_stay(
            ENGAGED,
            WARINESS,
            HuntDraw::Quantile {
                sigmas: combat::EXPECTED_STRIKES,
            },
        );
        assert!(
            expected > 0.0 && expected < ENGAGED,
            "liveness: a mid wariness must leave some and take some: {expected}"
        );
        let drawn: f32 = (0..SEEDS)
            .map(|seed| animals_that_stay(ENGAGED, WARINESS, HuntDraw::Seeded(u64::from(seed))))
            .sum::<f32>()
            / SEEDS as f32;
        // Within a tenth of a standard deviation of the mean at this sample size — a band on the
        // *distribution*, not a hand-picked tolerance.
        let deviation = (ENGAGED * WARINESS * (1.0 - WARINESS)).sqrt();
        assert!(
            (drawn - expected).abs() < deviation,
            "the quantile's mean {expected} must track the draw's mean {drawn} (σ {deviation})"
        );

        let narrow = animals_that_stay(ENGAGED, WARINESS, HuntDraw::Quantile { sigmas: 1.0 })
            - animals_that_stay(ENGAGED, WARINESS, HuntDraw::Quantile { sigmas: -1.0 });
        let wide = animals_that_stay(ENGAGED, WARINESS, HuntDraw::Quantile { sigmas: 3.0 })
            - animals_that_stay(ENGAGED, WARINESS, HuntDraw::Quantile { sigmas: -3.0 });
        assert!(
            wide > narrow && narrow > 0.0,
            "the band must widen with sigmas: 1σ {narrow} vs 3σ {wide}"
        );
        assert_eq!(
            animals_that_stay(ENGAGED, WARINESS, HuntDraw::Quantile { sigmas: -99.0 }),
            0.0,
            "the band cannot leave the support below"
        );
        assert_eq!(
            animals_that_stay(ENGAGED, WARINESS, HuntDraw::Quantile { sigmas: 99.0 }),
            ENGAGED,
            "the band cannot leave the support above"
        );
    }

    /// **The named bound is the one that actually bound** — asserted as a *relation*, not against a
    /// literal: relaxing the named term alone must raise the take, and relaxing any other must not
    /// (`docs/plan_hunt_through_combat.md` §6.6).
    ///
    /// The engagement arm is the one this exists for. §11's first open question is that an
    /// `engage_rate` authored too low silently becomes a **second floor**; `bound=engagement` is what
    /// makes that legible, and a bound that quietly reported `floor` there would hide it.
    #[test]
    fn the_take_bound_names_the_term_that_actually_bound() {
        /// A rabbit-ish body, so every bound below is expressible in a small number of animals.
        const BODY: f32 = 2.0;
        /// Any of the three quantiser terms, made large enough not to bind.
        const SLACK: f32 = 1_000.0;

        // Each case: (ceiling, collection, stayed, brought_down) with exactly one term tight.
        let cases = [
            (4.0 * BODY, SLACK, SLACK, SLACK, HuntTakeBound::Floor),
            (SLACK, 4.0 * BODY, SLACK, SLACK, HuntTakeBound::Carry),
            // Reached ten, put four on the ground — the fight is the shortfall.
            (SLACK, SLACK, 10.0, 4.0, HuntTakeBound::Fight),
            // Reached four and killed all four — reach is the shortfall.
            (SLACK, SLACK, 4.0, 4.0, HuntTakeBound::Engagement),
        ];
        for (ceiling, collection, stayed, brought_down, expected) in cases {
            assert_eq!(
                hunt_take_bound(
                    ceiling,
                    collection,
                    BODY,
                    stayed,
                    brought_down,
                    EngagementStop::WhenPackFull
                ),
                expected,
                "({ceiling}, {collection}, {stayed}, {brought_down}) must name {expected:?}"
            );
            let tight = quantise_animal_take(
                ceiling,
                collection,
                BODY,
                brought_down,
                EngagementStop::WhenPackFull,
            );
            // **Liveness / the relation**: relaxing the named term raises the take. Without this the
            // table above would only be asserting against itself.
            let relaxed = match expected {
                HuntTakeBound::Floor => quantise_animal_take(
                    ceiling * 2.0,
                    collection,
                    BODY,
                    brought_down,
                    EngagementStop::WhenPackFull,
                ),
                HuntTakeBound::Carry => quantise_animal_take(
                    ceiling,
                    collection * 2.0,
                    BODY,
                    brought_down,
                    EngagementStop::WhenPackFull,
                ),
                HuntTakeBound::Fight | HuntTakeBound::Engagement => quantise_animal_take(
                    ceiling,
                    collection,
                    BODY,
                    brought_down * 2.0,
                    EngagementStop::WhenPackFull,
                ),
            };
            assert!(
                relaxed.killed > tight.killed,
                "relaxing {expected:?} must raise the take: {} -> {}",
                tight.killed,
                relaxed.killed
            );
        }
    }

    /// **A turn that takes nothing reports the FLOOR** — the herd could not spare a whole body, which
    /// is the wait turn the whole-animal quantiser exists to produce. It is the bound a player needs
    /// to see, because the alternative reading ("we could not reach them") is a different problem
    /// with a different remedy.
    #[test]
    fn a_wait_turn_reports_the_floor_that_caused_it() {
        const BODY: f32 = 800.0;
        let take = quantise_animal_take(
            BODY / 2.0,
            f32::INFINITY,
            BODY,
            f32::INFINITY,
            EngagementStop::WhenPackFull,
        );
        assert_eq!(take.killed, 0, "half a body cannot be taken");
        assert_eq!(
            hunt_take_bound(
                BODY / 2.0,
                f32::INFINITY,
                BODY,
                f32::INFINITY,
                f32::INFINITY,
                EngagementStop::WhenPackFull,
            ),
            HuntTakeBound::Floor
        );
    }

    /// **The seed is composed per event, so it cannot depend on hunt ORDER.** Same
    /// `(map_seed, tick, herd, party)` ⇒ same seed, and a different herd or party ⇒ a different one.
    /// This is what makes rollback reproduce regardless of the order hunts resolve in; a shared RNG
    /// stream could not promise it.
    #[test]
    fn the_retreat_seed_is_per_event_not_per_stream() {
        let a = retreat_seed(7, 3, "game_deer_1", 5);
        assert_eq!(
            a,
            retreat_seed(7, 3, "game_deer_1", 5),
            "pure in its inputs"
        );
        assert_ne!(a, retreat_seed(7, 3, "game_deer_2", 5), "herd must matter");
        assert_ne!(a, retreat_seed(7, 4, "game_deer_1", 5), "tick must matter");
        assert_ne!(a, retreat_seed(7, 3, "game_deer_1", 6), "party must matter");
    }

    // ---- The engagement bound ------------------------------------------------------------------

    /// A quarry a whole hunter can only partly corner in a turn — `workers × rate < 1` for any small
    /// party, which is the case the `max(1.0)` floor exists for.
    const HARD_TO_CORNER_ENGAGE_RATE: f32 = 0.25;
    /// A quarry a hunter reaches two of per turn — the linear-scaling fixture.
    const EASY_ENGAGE_RATE: f32 = 2.0;
    /// The `animal:pastoral` rung's shipped `yield_fraction_while_building`: half the crew's
    /// throughput goes into gentling the herd instead of hunting it.
    const HALF_CREW_BUILD_DIP: f32 = 0.5;

    /// **A fractional engagement reaches one animal, not zero**
    /// (`docs/plan_hunt_through_combat.md` §10). A small band cannot corner the quarry *efficiently*;
    /// it can still walk up to it, and the gate on whether it survives the meeting is the fight, not
    /// a headcount threshold in front of it.
    #[test]
    fn a_party_too_small_to_corner_one_animal_still_engages_one() {
        for workers in 1..=3u32 {
            let engaged =
                animals_engaged(workers, HARD_TO_CORNER_ENGAGE_RATE, NO_BUILD_UNDERWAY_DIP);
            assert!(
                (workers as f32 * HARD_TO_CORNER_ENGAGE_RATE) < 1.0,
                "fixture must actually be fractional for {workers} hunters"
            );
            assert_eq!(
                engaged, 1.0,
                "{workers} hunters must reach one animal, never zero"
            );
        }
        // The floor is the *fraction's* floor, not a blanket one: a party whose reach clears whole
        // animals is not pinned at 1.
        assert_eq!(
            animals_engaged(8, HARD_TO_CORNER_ENGAGE_RATE, NO_BUILD_UNDERWAY_DIP),
            2.0
        );
    }

    /// **No workers engage NOTHING** — a different statement from the fractional floor above, and
    /// pinned separately: the `max(1.0)` must not manufacture a hunter out of an unstaffed row.
    #[test]
    fn a_party_of_no_workers_engages_nothing() {
        for rate in [HARD_TO_CORNER_ENGAGE_RATE, EASY_ENGAGE_RATE, f32::INFINITY] {
            for dip in [NO_BUILD_UNDERWAY_DIP, HALF_CREW_BUILD_DIP] {
                assert_eq!(
                    animals_engaged(0, rate, dip),
                    0.0,
                    "an unstaffed row reaches no animals (rate {rate}, dip {dip})"
                );
            }
        }
    }

    /// **The build dip multiplies ENGAGEMENT, not just carry** — hands gentling a herd are hands not
    /// hunting it. Leaving it out is what this function's own doc calls re-opening a closed defect:
    /// a building crew and a harvesting crew would reach the same animals and the build would be free
    /// wherever engagement is the binding term. Asserted where the count is `>= 2`, so the `max(1.0)`
    /// floor cannot mask the difference.
    #[test]
    fn the_build_dip_multiplies_engagement() {
        const CREW: u32 = 8;
        let harvesting = animals_engaged(CREW, EASY_ENGAGE_RATE, NO_BUILD_UNDERWAY_DIP);
        let building = animals_engaged(CREW, EASY_ENGAGE_RATE, HALF_CREW_BUILD_DIP);
        // Liveness: both crews genuinely reach several animals, so neither reading is the floor.
        assert_eq!(harvesting, CREW as f32 * EASY_ENGAGE_RATE);
        assert_eq!(
            building,
            CREW as f32 * EASY_ENGAGE_RATE * HALF_CREW_BUILD_DIP
        );
        assert!(
            building >= 2.0 && building < harvesting,
            "a dipped crew must reach strictly fewer: {building} vs {harvesting}"
        );
    }

    /// **Engagement is linear in party size** — twice the hunters reach twice the animals, which is
    /// what makes party size the lever the take responds to.
    #[test]
    fn engagement_scales_linearly_with_the_party() {
        for workers in 1..=6u32 {
            assert_eq!(
                animals_engaged(workers, EASY_ENGAGE_RATE, NO_BUILD_UNDERWAY_DIP),
                workers as f32 * EASY_ENGAGE_RATE,
                "engagement must scale with the party at {workers} hunters"
            );
        }
    }

    /// **The bound actually reaches a take.** The other four tests pin the helper; this one pins that
    /// [`quantise_animal_take`] is genuinely bounded by it — an engagement-bound party kills strictly
    /// fewer than the same party's carry would allow, so deleting the third bound fails here rather
    /// than passing quietly. Paired with the unbounded reading as the liveness half: a take that
    /// collapsed to zero would also satisfy "fewer".
    #[test]
    fn the_engagement_bound_reaches_the_take() {
        // A Red Deer-shaped fixture: a light body, a herd with plenty standing above the floor, and a
        // party whose packs could haul far more animals than it can get near.
        const BODY_MASS: f32 = 15.0;
        const HUNTERS: u32 = 5;
        const PER_WORKER_CARRY: f32 = 40.0;
        const ONE_ANIMAL_PER_HUNTER: f32 = 1.0;
        // Room for far more animals than either bound allows, so the herd is never what binds.
        const AMPLE_CEILING: f32 = BODY_MASS * 100.0;
        let collection = HUNTERS as f32 * PER_WORKER_CARRY;

        let engaged = animals_engaged(HUNTERS, ONE_ANIMAL_PER_HUNTER, NO_BUILD_UNDERWAY_DIP);
        let bounded = quantise_animal_take(
            AMPLE_CEILING,
            collection,
            BODY_MASS,
            engaged,
            EngagementStop::WhenPackFull,
        );
        // `f32::INFINITY` is what a pen passes — no engagement stage — so this is the take as it was
        // before the bound existed.
        let carry_bound = quantise_animal_take(
            AMPLE_CEILING,
            collection,
            BODY_MASS,
            f32::INFINITY,
            EngagementStop::WhenPackFull,
        );

        assert_eq!(bounded.killed, HUNTERS, "the party kills what it can reach");
        assert_eq!(
            carry_bound.killed,
            (collection / BODY_MASS) as u32,
            "unbounded, the party kills what it can carry"
        );
        assert!(
            bounded.killed < carry_bound.killed,
            "the engagement bound must bite: {} vs {}",
            bounded.killed,
            carry_bound.killed
        );
        // Liveness: the bound reduces the take, it does not switch it off.
        assert!(bounded.killed > 0 && bounded.carried > 0.0);
    }

    // ---- The engagement CREW (the third term of `workers_needed`) -------------------------------

    /// A ceiling with room for many whole bodies, so the peak drop is a real number rather than the
    /// `+1` a nearly-empty room degenerates to.
    const ROOMY_CEILING: f32 = 1_000.0;
    /// A light body against [`ROOMY_CEILING`] — many animals, little biomass, which is the regime
    /// where reach binds and carry does not.
    const LIGHT_BODY: f32 = 1.0;
    /// The shipped `hunt.per_worker_biomass_capacity`.
    const HUNTER_CARRY: f32 = 40.0;

    /// **The engagement crew is the exact inverse of [`animals_engaged`]** — the smallest party that
    /// reaches the whole peak drop, and one hunter short of it does not. That exactness is the
    /// property: a crew count that merely correlated with reach would let the panel name a number the
    /// stepper's own take does not clear.
    #[test]
    fn the_engagement_crew_is_the_smallest_party_that_reaches_the_peak_drop() {
        for rate in [HARD_TO_CORNER_ENGAGE_RATE, EASY_ENGAGE_RATE, 10.0] {
            for dip in [NO_BUILD_UNDERWAY_DIP, HALF_CREW_BUILD_DIP] {
                let peak = peak_animal_drop(ROOMY_CEILING, LIGHT_BODY);
                let crew = hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, rate, dip);
                assert!(
                    crew > 1,
                    "rate {rate} dip {dip}: fixture must need a real crew"
                );
                assert!(
                    animals_engaged(crew, rate, dip) >= peak,
                    "rate {rate} dip {dip}: {crew} hunters must reach the whole {peak}-animal drop, \
                     they reach {}",
                    animals_engaged(crew, rate, dip)
                );
                assert!(
                    animals_engaged(crew - 1, rate, dip) < peak,
                    "rate {rate} dip {dip}: …and {} must not — the count has to be the SMALLEST such \
                     crew, not merely a sufficient one",
                    crew - 1
                );
            }
        }
    }

    /// **The dip raises the engagement crew, exactly as it raises the haul crew** (§3.1) — hands
    /// gentling a herd are hands not stalking it, so it takes proportionally more of them to corner
    /// the same drop. Without this the *crew* would price a harvesting party while the *take* pays
    /// the building one, which is the unit mismatch the haul term already had once.
    #[test]
    fn a_building_crew_needs_more_hands_to_reach_the_same_drop() {
        let harvesting = hunt_engage_workers(
            ROOMY_CEILING,
            LIGHT_BODY,
            EASY_ENGAGE_RATE,
            NO_BUILD_UNDERWAY_DIP,
        );
        let building = hunt_engage_workers(
            ROOMY_CEILING,
            LIGHT_BODY,
            EASY_ENGAGE_RATE,
            HALF_CREW_BUILD_DIP,
        );
        assert!(
            harvesting > 0,
            "liveness: the harvesting crew is a real count"
        );
        assert!(
            building > harvesting,
            "a gentling crew must be the larger: {building} vs {harvesting}"
        );
        // The sharp form: the crew sized for HARVESTING cannot reach the drop once it is dipped —
        // which is precisely the advice a dip-blind count would give.
        let peak = peak_animal_drop(ROOMY_CEILING, LIGHT_BODY);
        assert!(
            animals_engaged(harvesting, EASY_ENGAGE_RATE, HALF_CREW_BUILD_DIP) < peak,
            "the undipped count must be too small once the crew is building"
        );
        assert!(
            animals_engaged(building, EASY_ENGAGE_RATE, HALF_CREW_BUILD_DIP) >= peak,
            "…and the dipped count must clear it"
        );
    }

    /// **A source with NO ENGAGEMENT STAGE reports no engagement crew** — a pen and the plant web
    /// both forecast `f32::INFINITY` ([`SourceYieldForecast::managed`],
    /// [`FaunaConfig::engage_rate_for`]), so the `max()` collapses to the haul term and neither
    /// regresses. This is the no-regress half of the pair below.
    #[test]
    fn a_source_with_no_engagement_stage_keeps_exactly_its_haul_crew() {
        let haul = hunt_haul_workers(ROOMY_CEILING, LIGHT_BODY, HUNTER_CARRY);
        assert!(haul > 0, "liveness: the haul crew is a real count");
        for dip in [NO_BUILD_UNDERWAY_DIP, HALF_CREW_BUILD_DIP] {
            assert_eq!(
                hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, f32::INFINITY, dip),
                0,
                "an unstalked source owes no engagement crew (dip {dip})"
            );
        }
        assert_eq!(
            hunt_take_workers(
                ROOMY_CEILING,
                LIGHT_BODY,
                HUNTER_CARRY,
                f32::INFINITY,
                NO_BUILD_UNDERWAY_DIP
            ),
            haul,
            "…so the take crew is the haul crew, unchanged"
        );
    }

    /// **The take crew takes whichever of REACH and CARRY binds — and both directions are live.**
    /// The two scale on different units (animals reachable vs biomass carried), so a `max()` that
    /// had quietly collapsed to one of them would still look plausible; this pins that each side
    /// wins somewhere.
    #[test]
    fn the_take_crew_is_bound_by_reach_on_light_game_and_by_carry_on_heavy() {
        // Light body, slow reach: many animals to get near, almost nothing to carry.
        const SLOW_REACH: f32 = 0.5;
        let reach_bound = hunt_take_workers(
            ROOMY_CEILING,
            LIGHT_BODY,
            HUNTER_CARRY,
            SLOW_REACH,
            NO_BUILD_UNDERWAY_DIP,
        );
        let reach_haul = hunt_haul_workers(ROOMY_CEILING, LIGHT_BODY, HUNTER_CARRY);
        assert!(
            reach_bound > reach_haul && reach_haul > 0,
            "light game is reach-bound: take crew {reach_bound} must exceed the live haul crew \
             {reach_haul}"
        );

        // Heavy body, fast reach: one hunter gets near the whole drop and twenty carry it home.
        const HEAVY_BODY: f32 = 800.0;
        const FAST_REACH: f32 = 100.0;
        let carry_bound = hunt_take_workers(
            ROOMY_CEILING,
            HEAVY_BODY,
            HUNTER_CARRY,
            FAST_REACH,
            NO_BUILD_UNDERWAY_DIP,
        );
        let carry_haul = hunt_haul_workers(ROOMY_CEILING, HEAVY_BODY, HUNTER_CARRY);
        let carry_reach =
            hunt_engage_workers(ROOMY_CEILING, HEAVY_BODY, FAST_REACH, NO_BUILD_UNDERWAY_DIP);
        assert!(
            carry_reach > 0,
            "liveness: the engagement term is computed here too, it is simply the smaller"
        );
        assert!(
            carry_bound == carry_haul && carry_haul > carry_reach,
            "heavy game is carry-bound: take crew {carry_bound} is the haul crew {carry_haul}, \
             above the reach crew {carry_reach}"
        );
    }

    /// A zero deadband restores the raw stateless behaviour (the flicker) — the lever genuinely
    /// controls the hysteresis.
    #[test]
    fn a_zero_deadband_restores_the_raw_flicker() {
        const APH: f32 = 12.0;
        let mut herd = managed_herd_with_heads(13.0);
        assert_eq!(herd.stabilize_herders_needed(APH, 0.0), 2);
        herd.biomass = 12.0; // 12 ≤ (2−1)·12 − 0 = 12 → drops immediately with no band
        assert_eq!(herd.stabilize_herders_needed(APH, 0.0), 1);
    }

    // `the_build_dip_is_applied_inside_the_standing_stock_clamp` was deleted with its subject: the
    // dip is no longer a term of `ceiling_at` at all (`docs/plan_harvest_floor.md` §3.1 moved it onto
    // crew throughput), so there is no ordering left between it and the standing-stock clamp.

    /// The food peak, which these forecast-shape tests use wherever the floor is not what varies.
    const PEAK_FLOOR: f32 = MSY_BIOMASS_FRACTION;

    /// **A crew whose own THROUGHPUT is the binding term** — carry or engagement, not the herd's
    /// standing stock. Since `docs/plan_harvest_floor.md` §3.1 the build dip multiplies crew
    /// throughput rather than the ceiling, so it is invisible at a staffing the source's stock binds:
    /// a build only ever costs yield while hands are the scarce thing.
    ///
    /// **It must clear the ENGAGEMENT bound too, which is why it is no longer 5**
    /// (`docs/plan_hunt_through_combat.md` §2). Engagement is the third bound and it rounds up to one
    /// animal, so at five hunters on the aurochs fixture a dipped and an undipped crew both engage
    /// exactly **1** — the dip is real but unobservable, because a whole-animal floor swallows it.
    /// Twelve puts engagement at 2 against 1, where the dip has room to show. A crew that cannot see
    /// the dip proves nothing about whether the dip is applied.
    const DIP_VISIBLE_CREW: u32 = 12;

    /// **Every legal floor's ceiling is the take path's own arithmetic**, on both webs — the
    /// property that replaced four stored rows with one function
    /// (`docs/plan_harvest_floor.md` §5). A row-per-stance surface could only answer the floors
    /// someone thought to store; this asserts the accessor against `escapement_ceiling` itself
    /// across the whole legal range, including the ends.
    #[test]
    fn ceiling_at_is_the_escapement_room_at_every_legal_floor() {
        const STOCK: f32 = 800.0;
        const CAPACITY: f32 = 1000.0;
        const RATE: f32 = 0.02;
        let forecast = SourceYieldForecast {
            biomass: STOCK,
            carrying_capacity: CAPACITY,
            per_biomass_yield: plant_food_only(RATE),
            managed_production: None,
            ..Default::default()
        };
        let mut saw_room = false;
        for step in 0..=100 {
            let floor = step as f32 / 100.0;
            assert!(crate::components::floor_is_valid(floor));
            let expected = escapement_ceiling(floor, STOCK, CAPACITY) * RATE;
            let actual = forecast.ceiling_at(floor).provisions;
            assert!(
                (actual - expected).abs() < 1e-4,
                "floor {floor}: {actual} vs {expected}"
            );
            saw_room |= actual > 0.0;
        }
        assert!(
            saw_room,
            "the sweep must find real room somewhere, or it is ordering zeros"
        );
    }

    /// A rung-3 managed source **ignores the floor entirely and has no dips left to offer** — every
    /// floor, dipped or not, reads the one managed production. The `managed_production: Some(..)`
    /// arm of [`SourceYieldForecast::ceiling_at`], and the reason `managed` can carry
    /// [`BuildDips::NOTHING_LEFT_TO_BUILD`] without paying its crew zero.
    #[test]
    fn a_managed_source_ignores_the_floor_on_every_rung() {
        const PRODUCTION: f32 = 7.0;
        let forecast = SourceYieldForecast::managed(
            plant_food_only(PRODUCTION),
            plant_food_only(1.0),
            YieldAccounts::ZERO,
        );
        for step in 0..=10 {
            let floor = step as f32 / 10.0;
            for improvement in [
                None,
                Some(Improvement::Cultivate),
                Some(Improvement::Sow),
                Some(Improvement::Tame),
                Some(Improvement::Corral),
            ] {
                assert_eq!(
                    forecast.ceiling_at(floor).provisions,
                    PRODUCTION,
                    "a finished source pays its managed yield at floor {floor} + {improvement:?}"
                );
            }
        }
        assert_eq!(
            forecast.build_dips,
            BuildDips::NOTHING_LEFT_TO_BUILD,
            "and it says so on the wire, rather than publishing the identity as if a build were on \
             offer"
        );
    }

    /// `plant_food_only`'s local twin — a food-only account, so these forecast-shape tests read as
    /// arithmetic rather than as a species' yield vector.
    fn plant_food_only(provisions: f32) -> YieldAccounts {
        YieldAccounts {
            provisions,
            ..YieldAccounts::ZERO
        }
    }

    // ---- Grazing Phase 2b-i ----------------------------------------------------------------

    use crate::graze::{GrazePatch, GrazeRegistry};

    /// Wild per-species regrowth rate for the 2b-i grazing harnesses (inert on `K`, so any live rate
    /// works); the global wild default the retired single ecology used.
    const WILD_TEST_REGROWTH_RATE: f32 = 0.05;

    /// One test beast (slice 8). Small relative to these fixtures' capacities, so a take quantises
    /// without the *fixture* becoming a study of the pulse — the rhythm has its own dedicated tests.
    const TEST_BODY_MASS: f32 = 1.0;

    fn herd_of_size(size: SizeClass, biomass: f32, cap: f32, fodder: f32) -> Herd {
        Herd::new(
            "game_test".to_string(),
            "Test Beast".to_string(),
            size,
            vec![UVec2::new(1, 1)],
            biomass,
            cap,
            fodder,
            WILD_TEST_REGROWTH_RATE,
            TEST_BODY_MASS,
        )
    }

    /// The **Tame rung's payoff** (`pastoral_yield`) is what a Sustain hunt pays *once the herd is
    /// tamed* — the pastoral analog of `managed_yield`/`corralYield`. It exists so the client can quote
    /// Tame's `→ +Y` instead of only its during-building dip (`ceiling_under(stance, Tame)`), which
    /// reads *below* the undipped stance.
    ///
    /// **Both payoffs are the SUSTAINED MSY on each rung's own ecology** — the long-run rate, which is
    /// `r`-dependent and so orders the ladder strictly: `pastoral_yield` **<** `managed_yield`, a
    /// strict step because only the ecology's `r` differs (pastoral `r×2` < pen `r×4`, MSY-capped).
    ///
    /// **The ladder is NOT visible against a stance ceiling, and cannot be** (`docs/plan_harvest_floor.md`
    /// §1, and the same note husbandry.md already carries for the pen). A stance ceiling is
    /// **constant escapement** — `B − floor·K`, a *stock* — so `r` cancels out of it entirely and a
    /// full herd's one-turn Sustain ceiling is `K/2` on **every** rung. Comparing it against a
    /// long-run rate compares a stock to a flow; that the stock is the larger number at `B = K` says
    /// nothing about the ladder. What the rungs buy is that the next animal comes sooner.
    #[test]
    fn the_tame_rung_advertises_its_payoff_above_the_dip_and_wild_sustain() {
        let fauna = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        // A healthy Wild Boar herd at capacity — a pennable species (`husbandry_ceiling == pen`).
        let mut herd = herd_of_size(SizeClass::Big, 1000.0, 1000.0, 0.06);
        herd.species = "Wild Boar".to_string();
        herd.regrowth_rate = 0.10;
        herd.husbandry_ceiling = HusbandryCeiling::Pen;
        herd.body_mass = 50.0;
        let forecast = hunt_forecast(
            &herd,
            &fauna,
            &ladder,
            40.0,
            &HuntingParty::builtin_equipped(),
            1.0,
        );

        // **The dip is read off the CREW, not the ceiling** (`docs/plan_harvest_floor.md` §3.1), so
        // it is visible only at a staffing the crew's carry binds — `DIP_VISIBLE_CREW` hunters carry
        // less than the herd's escapement offers, which is the regime a real Tame build lives in.
        let tame_dip = forecast_expected_take(
            &forecast,
            DIP_VISIBLE_CREW,
            PEAK_FLOOR,
            Some(Improvement::Tame),
        )
        .provisions;
        let wild_sustain = forecast_expected_take(
            &forecast,
            DIP_VISIBLE_CREW,
            PEAK_FLOOR,
            NO_IMPROVEMENT_UNDERWAY,
        )
        .provisions;
        assert!(
            tame_dip < wild_sustain,
            "the during-building dip reads below wild Sustain — the defect pastoral_yield fixes: \
             dip {tame_dip} vs sustain {wild_sustain}"
        );
        // The ladder, on the axis that can express it: the two rung PAYOFFS, each a sustained MSY on
        // its own ecology (r-dependent). Measured ≈ 0.75 < 1.5.
        assert!(
            forecast.pastoral_yield.provisions < forecast.managed_yield.provisions,
            "the pen's payoff out-yields taming's (Tame < Corral): tame {} vs corral {}",
            forecast.pastoral_yield.provisions,
            forecast.managed_yield.provisions,
        );
        // And the stance ceiling is the `r`-free escapement stock — the reason it is NOT on that
        // ladder. Pinned so nobody "fixes" the ordering above by putting a growth rate back into a
        // take ceiling.
        assert!(
            (forecast.ceiling_at(PEAK_FLOOR).provisions
                - herd.biomass * MSY_BIOMASS_FRACTION * fauna.hunt.provisions_per_biomass)
                .abs()
                < 1e-4,
            "a full herd's Sustain ceiling is exactly `B - K/2`, whatever its `r`: {}",
            forecast.ceiling_at(PEAK_FLOOR).provisions,
        );
    }

    /// **The forecast ceilings are the escapement stock the take actually pays, and they stay
    /// ORDERED** — pinned on a slow-breeder herd (Wild-Aurochs-shaped, `r ≈ 0.09`, one turn's regrowth
    /// lighter than one body) carrying a stale `hunt_credit`, which the resident path must now ignore
    /// completely (`Herd::hunt_credit` — the bank left with the rates it metered).
    ///
    /// What must hold at a single turn: `Sustain < Surplus < Deplete` (deeper floor ⇒ more standing
    /// stock takeable), and each build **dip** below the undipped stance it rides — the "Preparing +X"
    /// half of the client's row.
    ///
    /// **What is deliberately NOT asserted is dip-versus-payoff.** A dipped stance ceiling is a stock
    /// and a rung payoff is a long-run rate; ordering them was only meaningful while the stance was
    /// itself a rate. See `the_tame_rung_advertises_its_payoff_above_the_dip_and_wild_sustain`.
    #[test]
    fn the_forecast_ceilings_are_the_escapement_stock_and_stay_ordered() {
        let fauna = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        // A healthy slow breeder at capacity (Wild-Aurochs-shaped): MSY < body_mass, so it banks credit.
        let mut herd = herd_of_size(SizeClass::Big, 1000.0, 1000.0, 0.06);
        herd.species = "Wild Aurochs".to_string();
        herd.regrowth_rate = 0.09;
        herd.husbandry_ceiling = HusbandryCeiling::Pen;
        herd.body_mass = 50.0;
        herd.biomass_before_regrowth = herd.biomass;
        // Fill the kill-credit bank to a whole animal — the state that inflated the OLD ceiling. The
        // steady forecast must ignore it.
        herd.hunt_credit = herd.body_mass;

        let forecast = hunt_forecast(
            &herd,
            &fauna,
            &ladder,
            40.0,
            &HuntingParty::builtin_equipped(),
            1.0,
        );

        // Extractive ladder — deeper floor, more stock standing above it, unperturbed by the stale bank.
        assert!(
            forecast.ceiling_at(PEAK_FLOOR).provisions < forecast.ceiling_at(0.3).provisions
                && forecast.ceiling_at(0.3).provisions < forecast.ceiling_at(0.15).provisions,
            "extractive ceilings must ascend with the floor, and must not read the stale bank: \
             sustain {} surplus {} deplete {}",
            forecast.ceiling_at(PEAK_FLOOR).provisions,
            forecast.ceiling_at(0.3).provisions,
            forecast.ceiling_at(0.15).provisions,
        );
        // **A CEILING NO LONGER CARRIES A DIP AT ALL** (`docs/plan_harvest_floor.md` §3.1): the
        // build fraction moved onto crew throughput, which is what makes `ceiling_at` linear in the
        // terms already on the wire and therefore composable by the client. Pinned positively —
        // every rung reads the identical ceiling at a given floor — because "the dip is gone from
        // here" is the property that would silently regress if someone put it back.
        let expected = |crew, floor, improvement| {
            forecast_expected_take(&forecast, crew, floor, improvement).provisions
        };
        for floor in [PEAK_FLOOR, 0.3, 0.15] {
            assert!(
                (forecast.ceiling_at(floor).provisions
                    - escapement_ceiling(floor, herd.biomass, herd.carrying_capacity)
                        * fauna.hunt.provisions_per_biomass)
                    .abs()
                    < 1e-4,
                "the ceiling at {floor} is `max(0, B − floor·K) × rate` and nothing else — the \
                 expression the client composes: {}",
                forecast.ceiling_at(floor).provisions
            );
        }
        // The dip shows up where it now lives — in what a *crew* brings home. Asked at a staffing
        // the carry binds, since a crew the escapement binds pays no dip by construction.
        let tame_dip = expected(DIP_VISIBLE_CREW, PEAK_FLOOR, Some(Improvement::Tame));
        let corral_dip = expected(DIP_VISIBLE_CREW, PEAK_FLOOR, Some(Improvement::Corral));
        let undipped = expected(DIP_VISIBLE_CREW, PEAK_FLOOR, NO_IMPROVEMENT_UNDERWAY);
        assert!(
            tame_dip < undipped,
            "the Tame dip must read below the take it rides: dip {tame_dip} vs sustain {undipped}"
        );
        assert!(
            corral_dip < undipped,
            "the Corral dip must read below the take it rides: dip {corral_dip} vs sustain \
             {undipped}"
        );
        // **A deeper floor still takes more now** — the pressure axis is untouched by the build,
        // which is exactly the separation §3.1 bought: the dip cannot be dodged by choosing a floor,
        // and choosing a floor is not made cheaper by building.
        assert!(
            expected(DIP_VISIBLE_CREW, 0.15, Some(Improvement::Tame)) >= tame_dip,
            "a deeper floor never takes less while taming"
        );
        // The rung PAYOFFS still climb — the axis on which the ladder is expressible at a single turn.
        assert!(
            forecast.pastoral_yield.provisions < forecast.managed_yield.provisions,
            "the payoff ladder must climb Tame < Corral: tame {} corral {}",
            forecast.pastoral_yield.provisions,
            forecast.managed_yield.provisions,
        );
        // The stale bank changed nothing: the ceiling is exactly the escapement stock.
        assert!(
            (forecast.ceiling_at(PEAK_FLOOR).provisions
                - herd.biomass * MSY_BIOMASS_FRACTION * fauna.hunt.provisions_per_biomass)
                .abs()
                < 1e-4,
            "the Sustain ceiling is `B - K/2`, not `B - K/2 + credit`: {}",
            forecast.ceiling_at(PEAK_FLOOR).provisions,
        );
    }

    /// A **forage patch** and a **penned herd** never offer the `Tame` verb, so their forecast
    /// advertises no pastoral payoff (`pastoral_yield == 0`) — the mirror of `ceiling_tame == 0` on the
    /// plant side and of `managed()` collapsing the axis on a rung-3 source.
    #[test]
    fn a_penned_herd_advertises_no_tame_payoff() {
        let fauna = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        let mut herd = herd_of_size(SizeClass::Big, 1000.0, 1000.0, 0.06);
        herd.species = "Wild Boar".to_string();
        herd.husbandry_ceiling = HusbandryCeiling::Pen;
        herd.corralled_at = Some(UVec2::new(1, 1));
        let forecast = hunt_forecast(
            &herd,
            &fauna,
            &ladder,
            40.0,
            &HuntingParty::builtin_equipped(),
            1.0,
        );
        assert_eq!(
            forecast.pastoral_yield, NO_PASTORAL_YIELD,
            "a penned herd is past taming — no Tame payoff to advertise",
        );
    }

    /// Grazing 2d-δ: a `Wild`-ceiling herd never accrues domestication (and never picks up an owner),
    /// a `Pastoral` one tames but cannot be penned, and a `Pen` one climbs the whole ladder.
    #[test]
    fn husbandry_ceiling_gates_taming_and_penning() {
        let faction = FactionId(7);

        let mut wild = herd_of_size(SizeClass::Big, 600.0, 1200.0, 0.05);
        wild.husbandry_ceiling = HusbandryCeiling::Wild;
        assert!(!wild.can_domesticate() && !wild.can_pen());
        wild.accrue_domestication(faction, 1.0);
        assert_eq!(wild.domestication_progress, 0.0, "a wild herd never tames");
        assert_eq!(wild.owner, None, "and never picks up an owner");

        let mut pastoral = herd_of_size(SizeClass::Migratory, 4000.0, 9000.0, 0.05);
        pastoral.husbandry_ceiling = HusbandryCeiling::Pastoral;
        assert!(pastoral.can_domesticate() && !pastoral.can_pen());
        pastoral.accrue_domestication(faction, 1.0);
        assert!(
            pastoral.is_domesticated() && pastoral.owner == Some(faction),
            "a pastoral herd tames fine"
        );

        let mut pen = herd_of_size(SizeClass::Small, 100.0, 200.0, 0.10);
        pen.husbandry_ceiling = HusbandryCeiling::Pen;
        assert!(
            pen.can_domesticate() && pen.can_pen(),
            "a pen herd climbs the full ladder"
        );
    }

    #[test]
    fn graze_range_radius_maps_from_size_class() {
        let fauna = FaunaConfig::builtin();
        // Small game sits on its one tile; big game roams a 1-hex footprint.
        assert_eq!(
            herd_of_size(SizeClass::Small, 100.0, 200.0, 0.1).graze_range_radius(None),
            0
        );
        assert_eq!(
            herd_of_size(SizeClass::Big, 800.0, 1200.0, 0.05).graze_range_radius(None),
            1
        );
        // Migratory grazes its whole loiter cluster = the species' loiter_radius.
        let mammoth = fauna.species_by_display("Thunder Mammoths");
        assert_eq!(
            herd_of_size(SizeClass::Migratory, 9000.0, 12000.0, 0.011).graze_range_radius(mammoth),
            mammoth.map(|d| d.loiter_radius).unwrap()
        );
        // With no resolvable species row, a migratory herd falls back to the loiter default.
        assert_eq!(
            herd_of_size(SizeClass::Migratory, 9000.0, 12000.0, 0.011).graze_range_radius(None),
            default_loiter_radius()
        );
    }

    fn full_patch(x: u32, cap: f32) -> GrazePatch {
        GrazePatch::new(UVec2::new(x, 0), cap)
    }

    #[test]
    fn graze_take_draws_down_proportionally_and_respects_the_reseed_floor() {
        const CAP: f32 = 240.0;
        const FLOOR_FRACTION: f32 = 0.02;
        let mut graze = GrazeRegistry::default();
        // Two full tiles in range + one absent (barren) tile that must contribute nothing.
        graze.patches.insert(UVec2::new(0, 0), full_patch(0, CAP));
        graze.patches.insert(UVec2::new(1, 0), full_patch(1, CAP));
        let range = [UVec2::new(0, 0), UVec2::new(1, 0), UVec2::new(2, 0)];

        // A modest demand is split proportionally (both patches equal → equal draw), never below floor.
        graze_take(&mut graze, &range, 48.0, FLOOR_FRACTION);
        let a = graze.patch(UVec2::new(0, 0)).unwrap().biomass;
        let b = graze.patch(UVec2::new(1, 0)).unwrap().biomass;
        assert!(
            (a - b).abs() < 1e-4,
            "equal patches drawn equally: {a} vs {b}"
        );
        assert!(
            (a - (CAP - 24.0)).abs() < 1e-3,
            "each of two tiles paid half of 48: {a}"
        );
        assert!(
            graze.patch(UVec2::new(2, 0)).is_none(),
            "barren tile stays absent"
        );

        // An enormous demand cannot drive a patch below its reseed floor.
        graze_take(&mut graze, &range, 1e9, FLOOR_FRACTION);
        let floor = FLOOR_FRACTION * CAP;
        for x in [0u32, 1] {
            let biomass = graze.patch(UVec2::new(x, 0)).unwrap().biomass;
            assert!(
                (biomass - floor).abs() < 1e-3,
                "an overgrazed tile floors at the reseed floor, not 0: {biomass} vs {floor}"
            );
        }
    }

    /// Grazing draws a patch down, and once the herd stops eating it the patch **recovers** toward
    /// capacity via the shared reseeding regrowth curve — overgrazing is never permanent (the reseed
    /// floor + logistic climb). This pins the draw-down + recovery loop at the helper level.
    #[test]
    fn a_grazed_patch_recovers_after_the_herd_moves_on() {
        const CAP: f32 = 240.0;
        const FLOOR_FRACTION: f32 = 0.02;
        let regrowth_rate = FaunaConfig::builtin().graze.ecology.regrowth_rate;
        let mut graze = GrazeRegistry::default();
        graze.patches.insert(UVec2::new(0, 0), full_patch(0, CAP));
        let range = [UVec2::new(0, 0)];

        // Herd present: eat hard for several turns → the tile is drawn well down.
        for _ in 0..8 {
            graze_take(&mut graze, &range, 60.0, FLOOR_FRACTION);
        }
        let grazed = graze.patch(UVec2::new(0, 0)).unwrap().biomass;
        assert!(
            grazed < 0.6 * CAP,
            "sustained grazing draws the range down: {grazed}"
        );

        // Herd moves on: no more grazing, only regrowth (the very next system each turn). It climbs back.
        let patch = graze.patch_mut(UVec2::new(0, 0)).unwrap();
        for _ in 0..40 {
            patch.biomass = reseeding_logistic_regrowth(
                patch.biomass,
                patch.carrying_capacity,
                regrowth_rate,
                FLOOR_FRACTION,
            );
        }
        assert!(
            patch.biomass > 0.9 * CAP,
            "an ungrazed patch recovers toward capacity: {}",
            patch.biomass
        );
    }

    // A tiny hand-built world to exercise the graze-aware roam directly through `advance_herds`.
    fn roam_world(barren_gap: bool) -> bevy::prelude::World {
        use sim_runtime::TerrainType;

        let mut world = bevy::prelude::World::default();
        let mut config = SimulationConfig::builtin();
        config.grid_size = UVec2::new(5, 1);
        config.map_topology.wrap_horizontal = false;
        config.map_seed = 42;
        world.insert_resource(config);
        world.insert_resource(FaunaConfigHandle::default());
        world.insert_resource(LadderConfigHandle::default());
        world.insert_resource(crate::combat_config::CombatConfigHandle::default());
        world.insert_resource(SimulationTick::default());
        world.insert_resource(HerdTelemetry::default());
        world.insert_resource(HerdDensityMap::default());

        // A 5×1 strip of land; graze patches on every tile EXCEPT x=2 when `barren_gap` (that tile is
        // then "barren" — land with no pasture, the case a grazer must refuse to cross).
        let tiles: Vec<_> = (0..5)
            .map(|x| {
                world
                    .spawn(Tile {
                        position: UVec2::new(x, 0),
                        terrain: TerrainType::PrairieSteppe,
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        world.insert_resource(TileRegistry {
            tiles,
            width: 5,
            height: 1,
        });
        let mut graze = GrazeRegistry::default();
        for x in 0..5 {
            if barren_gap && x == 2 {
                continue;
            }
            graze.patches.insert(UVec2::new(x, 0), full_patch(x, 240.0));
        }
        world.insert_resource(graze);

        // A big-game herd at x=1 whose next anchor is x=4 — its path east runs straight through x=2.
        let mut herd = herd_of_size(SizeClass::Big, 240.0, 240.0, 0.0);
        herd.route = vec![UVec2::new(1, 0), UVec2::new(4, 0)];
        herd.current_pos = UVec2::new(1, 0);
        herd.step_index = 0;
        herd.dwell_remaining = 0;
        herd.roam = RoamState::GrazeWander;
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd);
        world.insert_resource(registry);
        world
    }

    #[test]
    fn roam_never_steps_onto_a_barren_tile_it_could_avoid() {
        use bevy::ecs::system::RunSystemOnce;
        // Positive control: with pasture all the way, the herd steps east onto x=2.
        let mut open = roam_world(false);
        open.run_system_once(advance_herds);
        let pos = open.resource::<HerdRegistry>().herds[0].current_pos;
        assert_eq!(
            pos,
            UVec2::new(2, 0),
            "with grass everywhere the herd advances east"
        );

        // With x=2 barren, the only distance-reducing step is dead ground → the herd stays put rather
        // than crossing it. It never ends the turn on the zero-graze tile.
        let mut gapped = roam_world(true);
        gapped.run_system_once(advance_herds);
        let pos = gapped.resource::<HerdRegistry>().herds[0].current_pos;
        assert_eq!(
            pos,
            UVec2::new(1, 0),
            "the herd refuses to cross barren ground"
        );
        assert_ne!(pos, UVec2::new(2, 0));
    }

    // --- `drift_to_owner` (Intensification ladder slice 3b, §3 dial 4) ------------------------------

    /// Tame the strip world's herd for `faction` — the real accrual (never a fabricated flag), so the
    /// husbandry ceiling still has its say.
    fn tame_the_herd(world: &mut bevy::prelude::World, faction: FactionId) {
        let mut registry = world.resource_mut::<HerdRegistry>();
        registry.herds[0].accrue_domestication(faction, RUNG_COMPLETE);
        assert!(registry.herds[0].is_domesticated());
    }

    /// Plant a resident band of `faction` on the strip's tile `x` — the camp a tamed herd drifts to.
    fn camp_at(world: &mut bevy::prelude::World, x: u32, faction: FactionId) {
        let tile = world.resource::<TileRegistry>().index(x, 0).expect("tile");
        world.spawn((
            ResidentBand,
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(30.0),
                elders: scalar_zero(),
                stores: crate::components::LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: crate::components::MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0,
                faction,
                knowledge: Vec::new(),
                migration: None,
            },
        ));
    }

    fn herd_x(world: &bevy::prelude::World) -> u32 {
        world.resource::<HerdRegistry>().herds[0].current_pos.x
    }

    fn run_roam_turns(world: &mut bevy::prelude::World, turns: u32) {
        use bevy::ecs::system::RunSystemOnce;
        for _ in 0..turns {
            world.run_system_once(advance_herds);
        }
    }

    /// **A tamed herd drifts toward its owner's camp** — the `drift_to_owner` primitive, wired from
    /// the `animal:pastoral` rung's `behavior.movement` (§3's proximity spine: wild roams its range,
    /// tamed stays near its people, penned is fixed). The strip's herd starts at x=4 with its wild
    /// route anchor *also* east; taming it makes the camp at x=0 the attractor instead, and it walks
    /// there one hex at a time (never teleporting).
    #[test]
    fn a_tamed_herd_drifts_toward_its_owners_camp() {
        let faction = FactionId(0);
        let mut world = roam_world(false);
        world.resource_mut::<HerdRegistry>().herds[0].current_pos = UVec2::new(4, 0);
        tame_the_herd(&mut world, faction);
        camp_at(&mut world, 0, faction);

        // One hex per step, and the species' own dwell cadence between steps — taming makes an animal
        // near, not fast. Four hexes at ≤1/turn cannot be crossed in fewer than four turns.
        let mut track = Vec::new();
        for _ in 0..12 {
            run_roam_turns(&mut world, 1);
            track.push(herd_x(&world));
        }
        assert!(
            track.windows(2).all(|w| w[0].abs_diff(w[1]) <= 1),
            "the drift moves at most one hex per turn: {track:?}"
        );
        assert!(
            track.contains(&0),
            "a tamed herd reaches its owner's camp: {track:?}"
        );
        assert_eq!(
            herd_x(&world),
            0,
            "and stays with its people once there (the strip's only pasture is its camp's tile)"
        );
    }

    /// **No owner band → the plain roam, unchanged.** The drift is a preference over the *same*
    /// candidates, so with nobody to drift to a tamed herd must move exactly as it did before it was
    /// tamed — asserted against a wild control run of the same seeded world, not against a hand-copied
    /// expectation.
    #[test]
    fn a_tamed_herd_with_no_owner_band_roams_exactly_like_a_wild_one() {
        let mut wild = roam_world(false);
        let mut tamed = roam_world(false);
        tame_the_herd(&mut tamed, FactionId(0));
        // Deliberately no `camp_at`: the owning faction has no bands at all.

        for _ in 0..10 {
            run_roam_turns(&mut wild, 1);
            run_roam_turns(&mut tamed, 1);
            assert_eq!(
                herd_x(&tamed),
                herd_x(&wild),
                "with no owner band the tamed herd falls back to the wild roam"
            );
        }
    }

    /// **The drift is a preference among ACCEPTABLE steps — it never crosses barren ground.** The
    /// camp sits at x=0 with dead ground at x=2 between it and the herd: the pull is real, but the
    /// 2b-i barren-avoidance still binds, so the herd stops at the edge of the gap rather than
    /// starving its way across it. (Composition, not replacement.)
    #[test]
    fn drift_never_steps_onto_barren_ground() {
        let faction = FactionId(0);
        let mut world = roam_world(true);
        world.resource_mut::<HerdRegistry>().herds[0].current_pos = UVec2::new(4, 0);
        tame_the_herd(&mut world, faction);
        camp_at(&mut world, 0, faction);

        for _ in 0..12 {
            run_roam_turns(&mut world, 1);
            assert_ne!(
                herd_x(&world),
                2,
                "the drift must never put the herd on the barren tile"
            );
        }
        assert_eq!(
            herd_x(&world),
            3,
            "it drifts as near the camp as the pasture allows, and stops at the gap"
        );
    }

    /// **The inert invariant.** `advance_herd_grazing` moves only the graze layer — it must not touch
    /// any herd's biomass or carrying capacity, and `K` stays the species constant (not graze-derived)
    /// this slice, so a hunt forecast is byte-identical before and after a grazing turn.
    #[test]
    fn grazing_is_inert_on_carrying_capacity_and_hunt_yield() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = roam_world(false);
        // Give the herd a real appetite so grazing actually draws the layer down.
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].fodder_per_biomass = 0.10;
            registry.herds[0].biomass = 200.0;
        }
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let before = world.resource::<HerdRegistry>().herds[0].clone();
        let forecast_before = hunt_forecast(
            &before,
            &fauna,
            &LadderConfig::builtin(),
            40.0,
            &HuntingParty::builtin_equipped(),
            1.0,
        );

        world.run_system_once(advance_herd_grazing);

        let after = &world.resource::<HerdRegistry>().herds[0];
        assert_eq!(
            after.biomass, before.biomass,
            "grazing does not touch herd biomass"
        );
        assert_eq!(
            after.carrying_capacity, before.carrying_capacity,
            "K is untouched by grazing"
        );
        // K is still the species constant, not a graze-derived value.
        assert_eq!(herd_capacity(after, &fauna), after.carrying_capacity);
        let forecast_after = hunt_forecast(
            after,
            &fauna,
            &LadderConfig::builtin(),
            40.0,
            &HuntingParty::builtin_equipped(),
            1.0,
        );
        assert_eq!(
            forecast_before.ceiling_at(PEAK_FLOOR),
            forecast_after.ceiling_at(PEAK_FLOOR),
            "the Sustain hunt ceiling is unchanged by grazing (inert on the hunting economy)"
        );

        // And the grazing genuinely happened — the herd's tile was drawn down.
        let grazed = world
            .resource::<GrazeRegistry>()
            .patch(UVec2::new(1, 0))
            .unwrap()
            .biomass;
        assert!(grazed < 240.0, "the herd's tile was grazed: {grazed}");
    }

    // ---- The DENIAL requirement (`docs/plan_denial_raid.md` §3.1) --------------------------------

    /// The reported case's own numbers: a Red Deer's `engage_rate 1` against its `wariness 0.65`,
    /// and a herd replacing itself at 2.91 animals a turn. `2.91 / 0.35 = 8.32`.
    const REPORTED_REPLACEMENT_ANIMALS: f32 = 2.9142854;
    const RED_DEER_ENGAGE_RATE: f32 = 1.0;
    const RED_DEER_WARINESS: f32 = 0.65;

    /// **8.3 hunters is NINE, and this is the assertion the defect is shaped like.** A floor — or a
    /// `ceil` on a quotient that lands exactly on an integer — hands back a party that ties with the
    /// herd's regrowth every turn and therefore declines nothing, while the sheet presents it as the
    /// answer.
    ///
    /// The exact-integer case is asserted beside it because it is where `ceil` and `floor + 1`
    /// disagree, and it is the value a tuner is most likely to author.
    #[test]
    fn a_requirement_of_eight_point_three_hunters_is_nine_and_a_tie_is_never_enough() {
        assert_eq!(
            denial_party_needed(
                REPORTED_REPLACEMENT_ANIMALS,
                RED_DEER_ENGAGE_RATE,
                RED_DEER_WARINESS
            ),
            Some(9),
            "2.91 / 0.35 = 8.3 hunters, so nine — rounding down is the reported bug"
        );
        // **An exactly-integral quotient**, stated in binary-exact terms so it really is one: a
        // hunter at `1 × (1 − 0.5)` kills 0.5 animals, so eight of them kill exactly the 4.0 the
        // herd replaces. A tie drives the herd nowhere, so the answer is NINE — `ceil` answers 8.
        assert_eq!(
            denial_party_needed(4.0, 1.0, 0.5),
            Some(9),
            "a party that exactly matches the regrowth declines nothing — `ceil` would answer 8"
        );
        // Liveness, so the two above cannot pass by the function answering 9 to everything.
        assert_eq!(
            denial_party_needed(0.0, RED_DEER_ENGAGE_RATE, RED_DEER_WARINESS),
            Some(1),
            "a herd replacing nothing needs the smallest party that exists, not none and not nine"
        );
    }

    /// **A quarry no number of hunters can reach answers `None`, not a huge number.** `wariness 1`
    /// means every animal breaks off before contact and `engage_rate 0` means none is ever reached;
    /// either way the honest answer is the absence of a party, which is what the wire's `0` sentinel
    /// carries.
    #[test]
    fn a_quarry_nothing_brings_into_contact_names_no_party_at_all() {
        assert_eq!(
            denial_party_needed(REPORTED_REPLACEMENT_ANIMALS, RED_DEER_ENGAGE_RATE, 1.0),
            None
        );
        assert_eq!(
            denial_party_needed(REPORTED_REPLACEMENT_ANIMALS, 0.0, RED_DEER_WARINESS),
            None
        );
        // A source with NO ENGAGEMENT STAGE is the opposite reading and must not be confused with
        // it: `f32::INFINITY` reaches everything, so one hunter suffices.
        assert_eq!(
            denial_party_needed(REPORTED_REPLACEMENT_ANIMALS, f32::INFINITY, 0.0),
            Some(1)
        );
    }

    /// **The replacement a raid must outpace is the PEAK on the path down, not the rate at the
    /// herd's current stock** — the difference decides whether a party sized on it stalls at the
    /// food peak forever.
    ///
    /// A **full** herd's instantaneous regrowth is `0`, so a current-stock reading would say one
    /// hunter suffices; the raid would then drive it to `K/2`, where the curve peaks, and sit there.
    /// Above `K/2` the answer must therefore be the peak; below it, the current stock, because from
    /// there the raid only ever makes the herd grow slower.
    #[test]
    fn the_replacement_to_outpace_is_the_peak_on_the_path_not_the_rate_where_the_herd_stands() {
        let ecology = FaunaConfig::builtin().ecology;
        const CAP: f32 = 1_000.0;
        const BODY: f32 = 10.0;
        let peak = herd_replacement_animals(CAP * MSY_BIOMASS_FRACTION, CAP, BODY, &ecology);
        assert!(peak > 0.0, "liveness: the food peak replaces real animals");
        assert_eq!(
            herd_replacement_animals(CAP, CAP, BODY, &ecology),
            peak,
            "a FULL herd regrows nothing this turn, but a raid on it must still pass through the \
             food peak — sizing on the instantaneous rate would answer one hunter"
        );
        let below = herd_replacement_animals(CAP * 0.25, CAP, BODY, &ecology);
        assert!(
            below < peak && below > 0.0,
            "below the peak the raid only slows the herd further, so the current stock binds \
             ({below} vs {peak})"
        );
        assert_eq!(
            herd_replacement_animals(CAP * ecology.collapse_fraction * 0.5, CAP, BODY, &ecology),
            0.0,
            "a herd already past its Allee point replaces nothing — there is nothing to outpace"
        );
    }
}
