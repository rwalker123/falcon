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
    components::{take_overdraws, PopulationCohort, ResidentBand, SourceYield, Tile, YieldRange},
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
        interpolate, LadderConfig, LadderConfigHandle, RungBranch, RungDef, RungKey, RungMovement,
        RungStanding, NEGLECT_NONE, NO_BUILD_GEAR, NO_CREW_ON_THIS_ACTIVITY, NO_NEGLECT_GRACE,
        NO_RUNG_WORK_BANKED, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED, RUNG_UNSTARTED,
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

/// XOR sub-seed salt for the **fight's** strike draws, applied by [`HuntDraw::seed`] on top of the
/// retreat's own seed — so the two stochastic stages of one hunt draw from **independent** streams.
///
/// # Why one value cannot feed both stages
///
/// Every draw site here reseeds a *fresh* `SmallRng` from a `u64`; nothing hands one advancing stream
/// down the pipeline. So [`animals_that_stay`] and [`crate::combat::landed_strikes`] handed the *same*
/// `u64` do not take turns on one stream — they replay the **same underlying uniforms in the same
/// order**. The k-th retreat Bernoulli and the k-th strike Bernoulli then compare one uniform against
/// two thresholds, which makes *"animal k stayed"* and *"hunter k landed"* nested events rather than
/// independent ones, and the two counts become rank-correlated for every draw.
///
/// `docs/plan_hunt_through_combat.md` §4.7 asks for variance **binomial in force size**, which is a
/// statement about two independent stages: a party that reaches few animals *and* misses them is a
/// different distribution from one where the two coincide by construction. The defect is invisible at
/// the shipped `hit_chance` of `1.0` — [`crate::combat::attacks_landed`] short-circuits before it
/// draws — and authoring a sub-1 chance is exactly what `combat_config.json` calls the next tuning
/// step, so it is salted apart now rather than after it starts biasing takes.
const FIGHT_SEED_SALT: u64 = 0xF16E_5EED_C0DE_1A75;

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
    /// **THE HERD'S ONE POSITION ON THE ANIMAL BRANCH**, in work units, cumulative across every rung
    /// (`docs/plan_standing_upkeep.md` §2.8/§4.10) — the animal twin of
    /// [`crate::forage::ForagePatch::ladder_position`].
    ///
    /// # It replaced TWO unconnected meters, and that is what let the rung interpolate
    ///
    /// A herd used to carry `domestication_progress`/`domestication_cost` and
    /// `corral_progress`/`corral_cost` as four independent numbers. Nothing related them, so every
    /// payout had to ask a **boolean** — `is_corralled()`, else `is_domesticated()`, else wild — and a
    /// herd one work unit short of tame paid exactly what a wild one paid. §4.10 landed the
    /// one-position model on the plant web and skipped this one; this is that restructure, arriving
    /// late.
    ///
    /// **Read through [`Self::standing`], never raw.** The position is a coordinate; the standing is
    /// what it *means*, and it is stamped on the same write so no call site re-derives it.
    ladder_position: f32,
    /// **WHAT THE POSITION MEANS** — held rung, rung being raised, and how far into it
    /// ([`RungStanding`]), re-derived on every [`Self::set_ladder_position`] so the pair can never be
    /// half-written. The `credit` term is already zeroed for an `on_completion` rung, which is what
    /// keeps `animal:pen` a step while `animal:pastoral` slides.
    standing: RungStanding,
    /// **WHAT TAMING THIS SPECIES COSTS, AS A MULTIPLE OF THE RUNG'S OWN `work_cost`** — the species'
    /// `taming_cost_multiplier`, stamped at spawn beside [`Self::body_mass`] and
    /// [`Self::husbandry_ceiling`] and for the same reason: the herd must be able to resolve its own
    /// [`Self::standing`] from a ladder alone, and a per-species price is not in the ladder.
    ///
    /// A Steppe Runner's `animal:pastoral` boundary sits at 250 work units where an aurochs' sits at
    /// 50, so a ladder-wide reading would put every herd's rung edges in the wrong place.
    pub taming_cost_multiplier: f32,
    /// Faction tending/owning this group — `Some` from the **first** `Tame` accrual
    /// ([`Self::accrue_domestication`] records it before any work is banked), and cleared only
    /// when a managed herd sheds its last animal. So it is *not* a reading of
    /// [`Self::ladder_position`]: a herd one work unit up the pastoral rung already has an owner,
    /// and an owner is what puts it in the managed set (`is_corralled() || owner.is_some()`) that
    /// pays keeping and sheds.
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
    /// **The pen's footprint radius** (Grazing 2d) — the hex range, centred on `corralled_at`, of the
    /// *fenced land* a penned herd grazes and derives its `K` over (`hex_range_tiles(corralled_at,
    /// pen_radius)`). `0` = today's single tile; each ring the `ExtendPen` command (2d-β) works off
    /// raises it. Read by **all** the pen-footprint logic (K, grazing, the larder offset, the wire
    /// count) so β only has to grow it. Authoritative sim state — rewound by rollback with the cloned
    /// registry.
    pub pen_radius: u32,
    /// Pen-**extension** build progress `[0.0, 1.0]` for the in-flight ring (the `ExtendPen` labor
    /// ladder, 2d-β), accrued each turn the keeper tends an *extending* pen at that crew's own work
    /// output; at [`Self::pen_extend_cost`] the ring completes (`pen_radius += 1`, this meter and its
    /// cost reset, `pen_extending` clears). Exported as `penExtendProgress` for a "Fencing N%"
    /// badge. Authoritative sim state, alongside `pen_radius`.
    ///
    /// **In absolute work units**, completing at [`Self::pen_extend_cost`] — a ring rides the *same*
    /// `animal:pen` rung as the pen it widens, so it cannot drift from the initial build.
    pub pen_extend_progress: f32,
    /// **What the in-flight fence ring costs, in work units** — the `animal:pen` rung's own
    /// `work_cost`, stamped when the ring is worked. Reset with the meter when a ring completes.
    pub pen_extend_cost: f32,
    /// **How many more turns a build on this herd needs, at the crew, floor and kit that worked it
    /// this turn** — stamped by the labor arm (Tame or Corral) and published as
    /// `HerdTelemetryState.buildTurnsRemaining`.
    ///
    /// **It is a PROJECTION when nothing is being built** — the exact rule
    /// [`Self::pen_fed_fraction`]'s neighbour `penUpkeep` already follows, and for the same reason: the
    /// pre-commit Corral row is by definition looking at a herd nobody is penning yet. With a verb in
    /// flight it is [`crate::intensification::build_turns_remaining`] on the running meter; with none
    /// it is [`crate::intensification::LadderConfig::projected_build_turns`] on the rung this herd
    /// would climb next.
    ///
    /// `None` = **no estimate**, and it means there is genuinely no answer: the herd is penned (the
    /// top of the animal ladder), the next rung's own ceiling/knowledge/ownership gates refuse it for
    /// this faction, or the crew produced nothing and a running build is stalled. The client cannot
    /// compute any of it (it holds neither the crew's output, nor the floor multiplier, nor the kit),
    /// so the sim answers — the `penFeedUpkeep` discipline. Transient per-turn scratch on
    /// `tamed_this_turn`'s cycle: written in Population, cleared by `advance_husbandry` the next turn.
    /// `None` is the wire's [`sim_schema::NO_BUILD_TURNS_ESTIMATE`],
    /// [`crate::intensification::BuildTurns::Holding`] its [`sim_schema::BUILD_METER_HOLDS`] and
    /// [`crate::intensification::BuildTurns::Rotting`] its [`sim_schema::BUILD_METER_ROTS`] — see
    /// `ForagePatch::build_turns_remaining` for why the three are separate answers.
    pub build_turns_remaining: Option<crate::intensification::BuildTurns>,
    /// **What the keepers' TOOLS ADD to this herd's running build, per turn**, in work units —
    /// [`crate::intensification::gear_work_supply`] over the pool, published as
    /// `HerdTelemetryState.buildWorkFromGear`.
    ///
    /// **It is an ADDEND on the pool's output, never a deduction from the job**
    /// (`docs/plan_standing_upkeep.md` §4.8) — a `Tame` costs its species' whole
    /// `work_cost × taming_cost_multiplier` with handling gear and without.
    ///
    /// [`crate::intensification::NO_BUILD_GEAR`] when no build is in flight or the crew left the
    /// handling gear at camp. Transient per-turn scratch on [`Self::build_turns_remaining`]'s cycle,
    /// and for its reason: the kit is re-read every turn, so no state may record *"this build was
    /// geared"*.
    pub build_work_from_gear: f32,
    /// **WHERE THIS SOURCE SITS IN THE WINNING BAND'S BUILD QUEUE** — 0-based, and
    /// [`crate::intensification::NOT_IN_ANY_BUILD_QUEUE`] (`-1`) when no band has queued it
    /// (`docs/plan_standing_upkeep.md` §4.6b).
    ///
    /// **It rides the same winner as [`Self::build_turns_remaining`] and
    /// [`Self::build_work_from_gear`]** — the three are read as one set, so a date from one band's
    /// queue beside another band's position would be two answers pretending to be one.
    ///
    /// **Without it a chained date is a number with no explanation.** The whole builders pool goes
    /// on the head of a queue, so an entry's turns are everything above it plus its own span — and
    /// the player cannot tell forty turns of work from eight turns of work behind four other jobs.
    ///
    /// Transient per-turn scratch on [`Self::build_turns_remaining`]'s cycle, and for its reason.
    pub build_queue_position: i32,
    /// **WHY THE BAND'S BUILDERS ARE STUCK ON THIS HERD** — the plant twin's rationale in full is on
    /// [`crate::forage::ForagePatch::build_blocked_reason`]. The conjunct of the rung's own gate that
    /// refused ([`crate::intensification::BuildGate`]), or
    /// [`crate::intensification::BuildGate::Open`] (wire key `""`) when this herd is not a blocked
    /// build.
    ///
    /// **The animal web is where the sentinel bites hardest**: an unkept flock's suppressed regrowth
    /// pins it at the hunters' floor, so the `Tame`'s escapement gate never reopens and nothing on
    /// the build line can move it (`.claude/rules/core_sim/husbandry.md` → "THE REGROWTH SUPPRESSION
    /// CLOSES A LOOP").
    ///
    /// Transient per-turn scratch on [`Self::build_turns_remaining`]'s cycle, and for its reason.
    pub build_blocked_reason: crate::intensification::BuildGate,
    /// **WHERE THE QUEUED ENTRY IS TAKING THIS SOURCE** — the destination rung, `None` when no band
    /// has queued it (`docs/plan_standing_upkeep.md` §2.8). The entry retires when the source reaches
    /// this rung's **top**, not when an intermediate rung fills.
    ///
    /// Transient per-turn scratch on [`Self::build_turns_remaining`]'s cycle, and for its reason.
    pub build_destination: Option<crate::intensification::RungKey>,
    /// **THE LEGS THE ENTRY STILL HAS TO LAY**, in climb order, first-incomplete first, each with its
    /// own **chained** turns ([`crate::intensification::BuildLeg`] plus the chain the publish pass
    /// stamps). Empty when the source is not queued, or has already arrived.
    ///
    /// Transient per-turn scratch on [`Self::build_turns_remaining`]'s cycle, and for its reason.
    pub build_legs: Vec<crate::intensification::PublishedBuildLeg>,

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
    /// crew output rather than net-of-decay. It is set even when a gate lapses mid-run
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
    /// [`RungDef::upkeep_grace_turns`] — `animal:pastoral`'s for a tamed herd, `animal:pen`'s for a
    /// penned one, which is why the grace is per-rung: the fence holds a flock without a keeper for
    /// far longer than habit holds an unfenced one. The under-herded *notice* is deliberately **not**
    /// gated on it (see `advance_husbandry`): the grace is exactly when the player can still act.
    ///
    /// Rides the checkpoint with the rest of the registry, so a rollback rewinds a spent grace rather
    /// than handing the herd a fresh one.
    pub neglect_turns: u16,
    /// **HOW FRAYED THIS HERD'S ATTACHMENT HAS BECOME** — the herd's *condition*, and the meter the
    /// escape rate accelerates on. **Not** a count of turns, and deliberately not
    /// [`Self::neglect_turns`].
    ///
    /// # ⛔ THE THREE QUANTITIES ARE DIFFERENT AND ONLY THIS ONE DECAYS
    ///
    /// - **Damage** — animals lost, rung lost — is *gone*, and keeping never repairs it. Only work
    ///   does: re-breeding, re-taming, re-queueing the build.
    /// - **The grace** ([`Self::neglect_turns`]) is a *forgiveness window* — how long before the
    ///   penalty starts — and it **resets outright** when the bill is met. You tended, you earned the
    ///   window back. Unchanged, on both webs.
    /// - **This** is what makes the shed accelerate, and it **decays slowly rather than resetting**.
    ///
    /// If the acceleration keyed off the grace, one tended turn in every N would erase it completely
    /// and a herd could be held for ever on token attention — measured before this meter existed, a
    /// herd survived indefinitely on **one tended turn in fourteen**, at *above* its starting size.
    ///
    /// **Rises by the SHORTFALL FRACTION**, so half-staffed keeping frays it at half speed — the
    /// *"I tend it, but not enough"* case, and the same proportionality the shed and the plant rot
    /// already use. **Falls by `husbandry.neglect_recovery_rate`** on any turn the bill is met, and
    /// deliberately slower than it rose: that asymmetry **is** the cost of neglect.
    ///
    /// Sim-side only; floored at zero and never negative.
    pub neglect_pressure: f32,
    /// **WHAT THE AT-RISK METER'S OWN CREW SUPPLIED THIS TURN**, in work units — the **keepers**
    /// once that rung is built and the **builders** while it is not
    /// ([`herd_upkeep_supply`], `docs/plan_standing_upkeep.md` §2.4), summed over every band working
    /// the herd.
    ///
    /// **IT IS THE ONE STORED FACT OF THE KEEPING**, and it carries what the retired
    /// `herded_fraction` did: the published ratio ([`herd_herded_fraction`]), the shortfall
    /// ([`herd_upkeep_shortfall`]) and the animals nobody can hold ([`uncontained_overage`]) are all
    /// derived from it, so no two of them can describe different staffings. *"Zero keepers last
    /// turn"* — the total-abandonment gate `regrow_biomass` and the bleed-out read — is simply
    /// `upkeep_supplied <= 0`.
    ///
    /// **Transient per-turn scratch on [`Self::build_turns_remaining`]'s cycle, and for its reason**:
    /// it describes *this* turn's crew, so a herd the player took the hands off must stop publishing
    /// a figure a crew that is no longer there paid. `advance_husbandry` clears it once per turn
    /// after everything downstream has read it, and the labor arm re-stamps it in Population.
    pub upkeep_supplied: f32,
    /// **THE BILL THIS HERD'S KEEPERS WERE HANDED** — the demand [`herd_upkeep_demand`] answered when
    /// the band's keeping pool was split, stamped by the labor arm and cleared each turn by
    /// `advance_husbandry`. `None` = no band answered for this herd this turn.
    ///
    /// # ⛔ IT EXISTS BECAUSE THE DEMAND MOVED WITHIN THE TURN
    ///
    /// The animal twin of `ForagePatch::upkeep_demanded`, and it arrived with the same mechanic: the
    /// keeping demand **interpolates on the position** now, and the build accrual raises that position
    /// *after* `maintenance_shares` has already split the pool against it. Judging the lagged supply
    /// against a demand that has since risen makes a fully-staffed keeping read permanently short —
    /// on the turn a Tame banks its first work, most sharply of all, where the share is struck at a
    /// demand of `0` and the capture reads a live one.
    ///
    /// **First write wins**, for the plant twin's reason: several bands may work one herd, the shares
    /// were all struck at the pre-accrual position, so the bill has to be too.
    pub upkeep_demanded: Option<f32>,
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
            ladder_position: RUNG_UNSTARTED,
            standing: RungStanding::unstarted(RungBranch::Animal),
            taming_cost_multiplier: RUNG_COST_UNSCALED,
            owner: None,
            corralled_at: None,
            pen_radius: 0,
            pen_extend_progress: RUNG_UNSTARTED,
            pen_extend_cost: RUNG_UNSTARTED,
            build_turns_remaining: None,
            build_work_from_gear: NO_BUILD_GEAR,
            build_queue_position: crate::intensification::NOT_IN_ANY_BUILD_QUEUE,
            build_blocked_reason: crate::intensification::BuildGate::Open,
            build_destination: None,
            build_legs: Vec::new(),
            pen_extending: false,
            footprint_intake: 0.0,
            pen_pasture_fraction: 0.0,
            fodder_draw: 0.0,
            pen_larder_bill: 0.0,
            pen_hay_food: 0.0,
            fodder_delivery_rate: 0.0,
            corralled_tended_this_turn: false,
            pen_fed_fraction: PEN_FULLY_FED,
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
            neglect_pressure: NO_NEGLECT_PRESSURE,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            upkeep_demanded: None,
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

    /// **WHAT THIS HERD GREW THIS TURN**, in biomass — `biomass − biomass_before_regrowth`, floored
    /// at zero.
    ///
    /// **It is realized, not projected.** Logistics regrows before Population takes, so by the time
    /// any take or build gate asks, the growth has already happened and this is a measurement rather
    /// than a forecast. `biomass_before_regrowth` is re-stamped at the top of every `regrow_biomass`,
    /// so it is never more than one turn old.
    ///
    /// Floored because a herd that **shrank** (a shed, a raid, a predator) grew nothing to share —
    /// and [`growth_share`] must never hand a take a negative bound.
    pub fn growth_this_turn(&self) -> f32 {
        (self.biomass - self.biomass_before_regrowth).max(0.0)
    }

    /// **WHAT HAS BEEN SUNK INTO THIS HERD**, in work units — the animal twin of
    /// `ForagePatch::ladder_position`. `RUNG_UNSTARTED` for a wild herd.
    pub fn ladder_position(&self) -> f32 {
        self.ladder_position
    }

    /// **WHERE THIS HERD STANDS ON THE ANIMAL BRANCH** — held rung, rung being raised, credit into
    /// it. Every payout and every cost reads this (through
    /// [`crate::intensification::interpolate`]); nothing re-derives it from a meter.
    pub fn standing(&self) -> RungStanding {
        self.standing
    }

    /// **⛔ THE ONE MUTATOR — it writes the position AND re-derives the standing, together.** The
    /// animal twin of `ForagePatch::set_ladder_position`, and separate for the same reason: the
    /// standing is what every ladder-free reader sees, and a stale one is a herd paying a rung its
    /// position no longer reaches.
    ///
    /// Floored at [`RUNG_UNSTARTED`]. **Ownership is NOT reconciled here**, unlike the plant twin: a
    /// herd's owner lapses when it sheds its last animal (`advance_husbandry`), never when its
    /// position moves, because taming is monotone-up — neglect sheds *animals*, not tameness.
    pub fn set_ladder_position(&mut self, position: f32, ladder: &LadderConfig) {
        self.ladder_position = position.max(RUNG_UNSTARTED);
        self.standing = self.standing_at(self.ladder_position, ladder);
    }

    /// [`RungStanding::at`] with **this herd's own prices** — the species' taming multiplier on the
    /// pastoral rung, [`RUNG_COST_UNSCALED`] on the pen (a fence is a fence, whatever it holds).
    fn standing_at(&self, position: f32, ladder: &LadderConfig) -> RungStanding {
        let multiplier = self.taming_cost_multiplier;
        RungStanding::at(ladder, RungBranch::Animal, position, |rung| {
            ladder.rung(rung).build_cost(match rung {
                RungKey::AnimalPastoral => multiplier,
                _ => RUNG_COST_UNSCALED,
            })
        })
    }

    /// **WHAT THIS HERD'S `rung` METER READS**, in work units — the position clamped into that rung's
    /// own span, the animal twin of `forage::patch_rung_work_done`. This is what the two retired
    /// `*_progress` fields stored, now derived from the one position so they cannot drift apart.
    pub fn rung_work_done(&self, rung: RungKey, ladder: &LadderConfig) -> f32 {
        let (base, width) = self.rung_span(rung, ladder);
        (self.ladder_position - base).clamp(RUNG_UNSTARTED, width)
    }

    /// **WHAT THIS HERD'S `rung` COSTS**, in work units — its width on this herd's own price list.
    /// The animal twin of the plant web's live `build_cost` reads, and what the two retired `*_cost`
    /// fields stored.
    pub fn rung_cost(&self, rung: RungKey, ladder: &LadderConfig) -> f32 {
        self.rung_span(rung, ladder).1
    }

    /// `(base, width)` of `rung` on this herd's own price list.
    fn rung_span(&self, rung: RungKey, ladder: &LadderConfig) -> (f32, f32) {
        let multiplier = self.taming_cost_multiplier;
        crate::intensification::rung_span(rung, &|key| {
            ladder.rung(key).build_cost(match key {
                RungKey::AnimalPastoral => multiplier,
                _ => RUNG_COST_UNSCALED,
            })
        })
    }

    /// **HOW FAR UP `rung` THIS HERD IS, AS A FRACTION** — `1.0` once it is held, the standing's own
    /// `credit` while it is being raised, `0` before. **Ladder-free**, because it reads the standing
    /// rather than a pair of work numbers: the wire and the client's meters want a fraction, and this
    /// is the one place the division happens.
    ///
    /// It honours `partial_credit` for free — `animal:pen` is `on_completion`, so its credit is
    /// already zero and this reads `0` until the fence closes and `1` after.
    pub fn rung_fraction(&self, rung: RungKey) -> f32 {
        if self.standing.held.is_at_or_above(rung) {
            WHOLE_RUNG
        } else if self.standing.raising == Some(rung) {
            self.standing.credit
        } else {
            RUNG_UNSTARTED
        }
    }

    /// **HOW FAR UP ITS OWN LADDER THIS HERD STANDS, `0.0..=1.0`** — the position over the total
    /// cost of the whole animal branch **at this herd's prices**.
    ///
    /// # ⛔ RAW POSITIONS ARE NOT COMPARABLE ACROSS SPECIES, AND THAT IS WHAT THIS IS FOR
    ///
    /// `taming_cost_multiplier` scales `animal:pastoral` and nothing else ([`Herd::standing_at`]
    /// passes `RUNG_COST_UNSCALED` for the pen), so a **fully penned rabbit** sits at `50 + 75 =
    /// 125` work units while a **merely tamed Steppe Runner** sits at `250`. Any ordering on the raw
    /// position therefore ranks the un-penned steppe runner above the finished rabbit pen — which is
    /// the one case a species-picking reader (`telling::nouns::most_domesticated_species`) ever
    /// sees. Dividing by the branch's own total makes the comparison apples-to-apples: the rabbit
    /// reads `1.0`, the steppe runner `0.77`.
    ///
    /// The branch top is walked from the root through [`RungKey::above`] rather than named, so
    /// appending a rung moves this with the ladder.
    pub fn ladder_fraction(&self, ladder: &LadderConfig) -> f32 {
        let mut top = RungBranch::Animal.root_rung();
        while let Some(next) = top.above() {
            top = next;
        }
        let (base, width) = self.rung_span(top, ladder);
        let whole_ladder = base + width;
        if whole_ladder <= RUNG_UNSTARTED {
            // A branch that costs nothing to climb is wholly climbed by standing on it — there is no
            // ratio to take, and `0/0` is not an answer.
            return WHOLE_RUNG;
        }
        (self.ladder_position / whole_ladder).clamp(RUNG_UNSTARTED, WHOLE_RUNG)
    }

    /// A fully-tamed (managed livestock) group: yields provisions each turn and is
    /// immune to the overhunting collapse.
    ///
    /// **`cost > RUNG_UNSTARTED` is load-bearing**: a wild herd carries `0` in both fields, and
    /// `0 >= 0` would read every animal on the map as tame.
    pub fn is_domesticated(&self) -> bool {
        self.standing.held.is_at_or_above(RungKey::AnimalPastoral)
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
    /// (auto-domestication at the job's own `cost`). Mirrors
    /// `ForagePatch::accrue_cultivation`.
    ///
    /// **A `Wild`-ceiling species never accrues** (Grazing 2d-δ) — self-guarded here so the "hunt-only"
    /// invariant holds regardless of the call site (and no wild herd ever picks up an `owner`).
    ///
    /// **`pub` so tests can build a tamed herd** by running the *real* path to completion
    /// ([`Self::tame_outright`]). It replaces the retired `claim_domestication`,
    /// which snapped progress to `1.0` for the `domesticate` early-claim: with that command gone the
    /// primitive had no production caller, and a "skip the investment" method left lying in the API
    /// is precisely what the ladder exists to delete. Going through the accrual instead means a test
    /// fixture obeys the husbandry ceiling like everything else — you cannot fabricate a
    /// domesticated `wild` herd.
    ///
    /// **Returns `true` only when THIS call finished the rung**, matching [`Herd::accrue_corral`] and
    /// `ForagePatch::accrue_cultivation`: `handle_tame` sets the verb on every band hunting the herd,
    /// so a post-hoc `is_domesticated()` test would push one "Tamed the …" feed line per band.
    pub fn accrue_domestication(
        &mut self,
        faction: FactionId,
        amount: f32,
        cost_multiplier: f32,
        ladder: &LadderConfig,
    ) -> bool {
        if self.is_domesticated() || !self.can_domesticate() {
            return false;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        // **The cost is the species' own price for this rung**, and it is stamped on the herd rather
        // than re-derived per call, because `standing_at` needs it to place every rung boundary.
        self.taming_cost_multiplier = cost_multiplier;
        // **BANK ONTO THE ONE POSITION, CAPPED AT THE PASTORAL RUNG'S OWN TOP** — the animal twin of
        // `ForagePatch::accrue_toward`. Capping here is what stops a Tame overspilling into the pen's
        // span and fabricating a fence nobody built.
        let (base, width) = self.rung_span(RungKey::AnimalPastoral, ladder);
        let capped = (self.ladder_position + amount).min(base + width);
        self.set_ladder_position(capped, ladder);
        self.is_domesticated()
    }

    /// **A fixture's already-tamed herd.** Runs the *real* accrual against
    /// [`FABRICATED_BUILD_COST`], so the husbandry ceiling and the owner-lock still apply — you
    /// cannot fabricate a domesticated `wild` herd. It replaces the `accrue_domestication(f,
    /// RUNG_COMPLETE)` spelling, which stopped meaning anything the moment a job had a size.
    pub fn tame_outright(&mut self, faction: FactionId, ladder: &LadderConfig) -> bool {
        // Enough work to clear the pastoral rung's whole span at this species' own price, whatever
        // that price is — the position is capped at the rung's top, so overshooting is free and a
        // fabricated job cannot spill into the pen.
        let span = self.rung_span(RungKey::AnimalPastoral, ladder);
        self.accrue_domestication(
            faction,
            span.0 + span.1,
            self.taming_cost_multiplier,
            ladder,
        )
    }

    // `decay_domestication` is DELETED (`docs/plan_fauna_neglect_escape.md` §2.1). Its only caller was
    // the retired `decay_under_herded` tameness-bleed; [`Self::ladder_position`] is monotone-up
    // (earned via `Tame`, never lost to neglect), and ownership clears only when a managed herd sheds
    // to zero animals (`advance_husbandry`), not when the position falls.

    /// **IS THE PEN METER FULL?** — `corral_progress >= corral_cost`, the *building vs maintaining*
    /// state test (`docs/plan_standing_upkeep.md` §2.4) and **not** the same question as
    /// [`Self::is_corralled`], which is the stored fence flag.
    ///
    /// The two agree on every herd the sim can reach today (a pen is raised by filling this meter and
    /// nothing bleeds it), and they are still separate because they answer different questions: the
    /// meter says *who supplies the maintenance rate*, the flag says *is this herd penned*.
    ///
    /// `cost > RUNG_UNSTARTED` is load-bearing for `is_domesticated`'s reason: a wild herd carries
    /// `0` in both fields and `0 >= 0` would read it as finished.
    pub fn corral_meter_full(&self) -> bool {
        self.standing.held.is_at_or_above(RungKey::AnimalPen)
    }

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
    pub fn corral_at(&mut self, tile: UVec2, ladder: &LadderConfig) -> bool {
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
        // **The pen is finished, so the position stands at the top of the branch.** A fixture that
        // pens a herd outright never banked the work, so it is placed here — one write, and the
        // standing follows it, where the retired pair of `corral_*` fields had to be kept in step.
        let (base, width) = self.rung_span(RungKey::AnimalPen, ladder);
        self.set_ladder_position(base + width, ladder);
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
    pub(crate) fn accrue_corral(
        &mut self,
        faction: FactionId,
        amount: f32,
        ladder: &LadderConfig,
        tile: UVec2,
    ) -> bool {
        if self.is_corralled() || self.owner != Some(faction) {
            return false;
        }
        // **ONE POSITION, CAPPED AT THE BRANCH'S TOP.** A `Corral` on a herd that is not yet tame
        // therefore lays the pastoral leg first — the same two-leg climb a `Sow` on untended ground
        // makes on the plant web.
        let (base, width) = self.rung_span(RungKey::AnimalPen, ladder);
        let top = base + width;
        self.set_ladder_position((self.ladder_position + amount).min(top), ladder);
        if self.corral_meter_full() {
            // The ceiling is already gated upstream (the `Corral` policy accrual + the commands), so
            // this can only refuse on a bug — and then the pen is genuinely not built, so say so.
            return self.corral_at(tile, ladder);
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
        self.pen_extend_progress = RUNG_UNSTARTED;
        self.pen_extend_cost = RUNG_UNSTARTED;
        true
    }

    /// Accrue one turn of pen-**extension** progress (2d-β), the twin of [`accrue_corral`] on an
    /// already-penned herd: while `pen_extending`, add `amount` to `pen_extend_progress`; at `1.0` the
    /// ring completes — `pen_radius += 1` (saturating at `radius_max`), the meter resets and the
    /// extending state clears. Returns `true` on the completion turn so the caller can announce it.
    /// Called **after** the turn's (dipped) take, mirroring `accrue_corral`.
    pub(crate) fn accrue_pen_extension(&mut self, amount: f32, cost: f32, radius_max: u32) -> bool {
        if !self.pen_extending {
            return false;
        }
        self.pen_extend_cost = cost;
        self.pen_extend_progress =
            crate::forage::banked_up_to_cost(self.pen_extend_progress + amount, cost);
        if self.pen_extend_progress >= cost {
            self.pen_radius = (self.pen_radius + 1).min(radius_max);
            self.pen_extend_progress = RUNG_UNSTARTED;
            self.pen_extend_cost = RUNG_UNSTARTED;
            self.pen_extending = false;
            return true;
        }
        false
    }

    /// **STOP AN UNFINISHED RING AND CLEAR ITS METER** — the state a ring leaves behind when its
    /// queue entry goes ([`crate::fauna::cancel_dropped_rings`]).
    ///
    /// # THE BANKED PROGRESS IS DISCARDED, AND THAT IS THE HONEST STATE
    ///
    /// `unqueue`'s contract is that it leaves the source's meter alone, which argues for keeping
    /// `pen_extend_progress`. But [`Self::begin_pen_extension`] **resets that meter to
    /// [`RUNG_UNSTARTED`] on every start**, so a preserved ring meter could never be resumed by any
    /// path the game has — it would be a number nothing can read. Clearing both says what is true:
    /// the ring stopped, and the next one starts from nothing.
    ///
    /// Idempotent, so the completion path — which clears the same three fields itself — may pass
    /// through it without a second rule.
    pub fn cancel_pen_extension(&mut self) {
        self.pen_extending = false;
        self.pen_extend_progress = RUNG_UNSTARTED;
        self.pen_extend_cost = RUNG_UNSTARTED;
    }

    /// **Update the hysteresis-stabilized keeper requirement for this herd** and return it — run once
    /// per turn for every herd in [`advance_husbandry`]. `band` is the deadband in **animals**
    /// (`animals_per_herder × husbandry.herders_hysteresis_fraction`).
    ///
    /// **`raw` is passed in rather than recomputed**, because the keeper count has exactly one
    /// definition since it became an upkeep ([`raw_herders_needed`], the rung's own
    /// `upkeep_crew_needed` at this herd's keeper load). A second `ceil` here would be a copy that
    /// could drift from the ladder the moment a rung's rate moved.
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
    pub fn stabilize_herders_needed(
        &mut self,
        raw: u32,
        animals_per_herder: f32,
        band: f32,
    ) -> u32 {
        if !(self.is_corralled() || self.owner.is_some()) {
            self.herders_needed = 0;
            return 0;
        }
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
    // **THE RATE INTERPOLATES; THE PHASE BANDS STEP.** `r` is a *payout* — how fast the flock
    // breeds — so a half-tamed herd breeds part-way between wild and pastoral, exactly as a
    // half-raised Field grows part-way between tended and Field (`forage::patch_ecology`). The
    // **bands** (`collapse_fraction` / `stressed_fraction` / `extinction_floor`) are the phase
    // classifier's cut points, not a payout: they come from the rung the herd **holds**, because
    // "Collapsing" is a word about a state and blending two definitions of it would invent a third.
    EcologyConfig {
        regrowth_rate: interpolate(&herd.standing(), |rung| {
            rung_regrowth_rate(rung, herd, fauna)
        }),
        ..rung_ecology_bands(herd.standing().held, fauna)
    }
}

/// **THE BREEDING RATE A RUNG BUYS**, asked about a rung the herd may not stand on — the animal twin
/// of `forage::rung_regrowth_gain`, and the seam [`herd_ecology`] interpolates over.
fn rung_regrowth_rate(rung: RungKey, herd: &Herd, fauna: &FaunaConfig) -> f32 {
    match rung {
        RungKey::AnimalPen => {
            managed_regrowth_rate(herd.regrowth_rate, fauna.husbandry.pen_gain, fauna)
        }
        RungKey::AnimalPastoral => {
            managed_regrowth_rate(herd.regrowth_rate, fauna.husbandry.pastoral_gain, fauna)
        }
        _ => herd.regrowth_rate,
    }
}

/// **THE PHASE BANDS A RUNG CLASSIFIES AGAINST** — the ecology block whose cut points a herd holding
/// `rung` is judged by. Stepped rather than interpolated: see [`herd_ecology`].
fn rung_ecology_bands(rung: RungKey, fauna: &FaunaConfig) -> EcologyConfig {
    match rung {
        RungKey::AnimalPen => fauna.husbandry.pen.ecology,
        RungKey::AnimalPastoral => fauna.husbandry.pastoral.ecology,
        _ => fauna.ecology,
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

/// **The per-species density (K) multiplier a husbandry STANDING buys** — domestication makes the land
/// hold *more* animals, non-linearly by species (the density ladder, orthogonal to the r-gains
/// `herd_ecology` folds in). A **corralled** herd multiplies its footprint `K` by the species'
/// [`SpeciesDef::pen_density`], a **mobile-tamed** herd by its [`SpeciesDef::pastoral_density`], and a
/// **wild** herd by [`DEFAULT_HUSBANDRY_DENSITY`] (`1.0`, so its `K` is byte-identical). Mirrors
/// `herd_ecology`'s rung dispatch exactly.
///
/// **The standing is a PARAMETER, and that is what keeps the two readings one expression.** The live
/// `K` passes the herd's own ([`CapacityStanding::live`]); the destination quote passes the standing
/// the build is climbing toward ([`herd_destination_capacity`]). A destination assembled out of a
/// second formula would agree with the sim only until one of them was retuned.
///
/// Resolved **live** by display name (`pen_density_for` / `pastoral_density_for`, the `taming_cost_multiplier_for`
/// path), never cached on the `Herd`, so a config retune reaches herds already on the map. Applied at
/// the single K seam [`ecological_carrying_capacity`] (the one place `herd.carrying_capacity` is
/// written), covering both the graze-derived and the fallback constant K.
pub fn herd_density_gain(standing: &RungStanding, herd: &Herd, fauna: &FaunaConfig) -> f32 {
    interpolate(standing, |rung| rung_density_gain(rung, herd, fauna))
}

/// **THE DENSITY GAIN A RUNG BUYS**, asked about a rung the herd may not stand on — the animal twin
/// of `forage::rung_capacity_gain`, and the per-rung seam [`herd_density_gain`] interpolates over.
fn rung_density_gain(rung: RungKey, herd: &Herd, fauna: &FaunaConfig) -> f32 {
    match rung {
        RungKey::AnimalPen => fauna.pen_density_for(&herd.species),
        RungKey::AnimalPastoral => fauna.pastoral_density_for(&herd.species),
        _ => DEFAULT_HUSBANDRY_DENSITY,
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

// **RETIRED: `managed_yield_biomass`** — `(biomass − capacity × MSY_BIOMASS_FRACTION)`, the
// escapement ceiling with the floor nailed to Sustain and the ecology argument unused. See
// `pen_yield_biomass`'s gravestone: it is the whole reason the pen's re-expression is exact.

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
/// presents no load at all.
///
/// # It is a MEASURE, not a demand — the ladder turns it into one
///
/// **The keeper count is an `upkeep` now** (`docs/plan_standing_upkeep.md` §2.4): the rung declares
/// `work_per_turn: 1.0, scaled_by: source_load`, so `upkeep_demand = loads` work units and
/// `upkeep_crew_needed = ceil(loads)` — which is the `ceil((biomass/body_mass)/animals_per_herder)`
/// this used to compute directly, hand for hand on every species. The arithmetic did not move; the
/// *authority* did, so a herd's standing cost is quoted in the same work units as everything else on
/// the ladder and the `max(1)` floor is gone as redundant (any positive load ceils to at least one).
pub fn herd_keeper_loads(biomass: f32, body_mass: f32, animals_per_herder: f32) -> f32 {
    // NaN-safe by construction: every guard is a positive test, so a NaN input falls through to `0`
    // (no load) rather than sneaking past a negated comparison.
    let sane = biomass > 0.0 && body_mass > 0.0 && animals_per_herder > 0.0;
    if !sane {
        return NO_KEEPER_LOAD;
    }
    (biomass / body_mass) / animals_per_herder
}

/// **AN UNFRAYED HERD** — the floor of [`Herd::neglect_pressure`], and the reading of a herd whose
/// keeping has been met long enough to work off every turn of neglect. Named rather than a bare `0.0`
/// because "no pressure" is a state the recovery rate is *aimed* at, not merely an initial value.
pub const NO_NEGLECT_PRESSURE: f32 = 0.0;

/// **THE WHOLE HERD** — the ceiling on a shed fraction: you cannot lose more animals than are
/// standing there. Named because the accelerating rate reaches it quickly by design, so the clamp is
/// a reachable state rather than a defensive rail.
const WHOLE_HERD: f32 = 1.0;

/// **A herd that presents nothing to mind** — an empty herd, or one whose species declares no
/// `animals_per_herder`. Named because a bare `0.0` in a load position reads as a missing value
/// rather than the deliberate *"there is nothing here to keep"* it is.
pub const NO_KEEPER_LOAD: f32 = 0.0;

/// **ONE keeper-load** — the measure at which a rung's `upkeep.work_per_turn` is quoted, so
/// `rung.upkeep_demand(ONE_KEEPER_LOAD)` reads back *"the work one keeper-load costs"*. It is the
/// animal branch's twin of [`crate::forage::ONE_TENDER_LOAD`], and the divisor that turns a
/// shortfall in **work** back into a shortfall in **loads**.
pub const ONE_KEEPER_LOAD: f32 = 1.0;

/// [`herd_keeper_loads`] for a herd, resolving its species' `animals_per_herder` live off the config.
pub fn herd_keeper_load(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    herd_keeper_loads(
        herd.biomass,
        herd.body_mass,
        fauna.animals_per_herder_for(&herd.species),
    )
}

/// [`herders_needed`] for a herd, resolving its species' `animals_per_herder` live off the config (the
/// `taming_cost_multiplier_for` path — a retune reaches herds already on the map). `0` for a herd that is not on
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
///
/// # ⛔ IT DOES NOT INTERPOLATE, AND IT IS NOT THE KEEPING BILL'S CREW
///
/// This is the **standing requirement**: how many keepers a flock of this species and this size wants,
/// read off the rung's own rate at the herd's keeper load. It is a function of *head count*, not of how
/// far up a rung the herd has been worked — deliberately, because everything downstream of it needs it
/// that way. The hysteresis ([`Herd::stabilize_herders_needed`], seeded from [`raw_herders_needed`])
/// exists to damp the **head count** breathing across an `animals_per_herder` multiple, and a term
/// sliding with a build meter underneath it would be a second, undamped source of the same flicker.
/// [`would_be_herders_needed`] must state a crew for
/// a herd at position **zero**, where an interpolated answer is `0` — the whole startup lag it was
/// written to close. And the wire's `herdersNeeded` is read by the client as *"is this a managed herd
/// that owes keepers at all"*, with `herdersNeededIfManaged == herdersNeeded` pinned on every managed
/// herd; interpolating here would break that on every herd mid-`Tame`.
///
/// **The hands the bill takes is [`herd_upkeep_workers_needed`]** (`ceil` of [`herd_keeping_basis`]),
/// which is what `upkeepWorkersNeeded` publishes and what the compose sheet's `KEEPERS` row quotes.
/// The two agree at the top of a rung and diverge below it.
pub fn herd_herders_needed(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> u32 {
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
    // seeds it) or a test-built managed herd. Fall back to the raw count so it is never wrong for a
    // turn — **through the ladder**, which owns the definition since the demand became an upkeep.
    raw_herders_needed(herd, fauna, ladder)
}

/// **THE unstabilized keeper count** — the rung's own `upkeep_crew_needed` at this herd's keeper
/// load, and the single definition of *how many hands a herd wants*
/// (`docs/plan_standing_upkeep.md` §2.4). [`herd_herders_needed`] prefers the hysteresis-stabilized
/// field and falls back to this; [`Herd::stabilize_herders_needed`] is *seeded* from it.
///
/// `0` for a herd standing on no keeping rung, and for one whose rung declares no upkeep.
pub fn raw_herders_needed(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> u32 {
    herd_keeping_rung(herd, ladder).map_or(0, |rung| {
        rung.upkeep_crew_needed(herd_keeper_load(herd, fauna))
    })
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
pub fn would_be_herders_needed(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> u32 {
    if !herd.can_domesticate() {
        return 0;
    }
    if herd.herders_needed > 0 {
        return herd.herders_needed;
    }
    // **The rung it WOULD stand on**, since it may not stand on one yet: an unowned tameable herd
    // has no keeping rung, so asking `raw_herders_needed` would answer `0` — the very startup lag
    // this function exists to close.
    ladder
        .rung(if herd.is_corralled() {
            RungKey::AnimalPen
        } else {
            RungKey::AnimalPastoral
        })
        .upkeep_crew_needed(herd_keeper_load(herd, fauna))
}

/// A fully-kept managed herd — the neutral value of [`herd_herded_fraction`], and what a herd with no
/// keeper demand reads. Mirrors [`PEN_FULLY_FED`].
pub const FULLY_HERDED: f32 = 1.0;

/// **A managed herd nobody kept** — the floor of [`herd_herded_fraction`], and the reading a herd
/// whose keepers never showed up gets. A *wild* herd reads [`FULLY_HERDED`] instead: it demands no
/// keepers, so "unstaffed" would be a lie that sheds it for free.
pub const NOT_HERDED: f32 = 0.0;

// **RETIRED: `pen_yield_biomass`** — the pen's own managed production. Its whole body was
// `managed_yield_biomass`, i.e. the escapement ceiling at a **hardcoded** `MSY_BIOMASS_FRACTION`
// floor, which is precisely why re-expressing it as the ordinary floor-live draw changed the settled
// yield by **nothing at Sustain** and everything at every other floor: the pen was already taking an
// escapement ceiling, it just refused to read the player's dial.

#[derive(Debug, Clone, Default)]
pub struct HerdTelemetryEntry {
    pub id: String,
    pub label: String,
    pub species: String,
    pub size_class: String,
    pub huntable: bool,
    /// Ecological health band string (see `EcologyPhase::as_str`).
    pub ecology_phase: String,
    /// Husbandry progress as a `[0.0, 1.0]` **fraction** of this herd's own taming job
    /// ([`crate::intensification::build_fraction`]) — the meter itself is in absolute work units, and
    /// the sim divides here so the wire keeps the range every shipped readout already renders.
    pub domestication: f32,
    /// Rung 1c corral state: `true` iff the herd is penned (`Herd::is_corralled`). Client shows a
    /// place-bound corral indicator distinct from a mobile domesticated herd.
    pub corralled: bool,
    /// Pen-construction progress as a `[0.0, 1.0]` **fraction** of this herd's own pen job — the
    /// client's "pen building N%" meter while a keeper works the herd with the `Corral` improvement
    /// in flight. Divided at capture, like `domestication` above.
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
        herd.carrying_capacity = settled_capacity(
            ecological_carrying_capacity(
                herd,
                def,
                graze,
                &prey_index,
                &fauna,
                width,
                height,
                wrap,
                CapacityStanding::live(herd),
            ),
            herd.is_corralled(),
            herd.carrying_capacity,
        );
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
    herd_footprint_at(herd, def, herd.corralled_at.is_some())
}

/// [`herd_footprint`] asked about a fencing the herd may not have yet — the **destination** reading's
/// half of the footprint question ([`herd_destination_capacity`]).
///
/// A herd climbing toward a pen has no `corralled_at` yet, and `corral_at` anchors the pen **where
/// the herd is standing** when the Corral lands, so *"here, at `pen_radius`"* is the projection —
/// the same "today's position, tomorrow's rung" rule every other destination term follows. It
/// matters because a pen's footprint is a fraction of a roam range: quoting a Corral's `K` over the
/// range the herd walks today would overstate it by the whole ratio between them, which
/// `pen_density` only partly gives back.
fn herd_footprint_at(herd: &Herd, def: Option<&SpeciesDef>, penned: bool) -> (UVec2, u32) {
    match (penned, herd.corralled_at) {
        (true, pen) => (pen.unwrap_or(herd.current_pos), herd.pen_radius),
        (false, _) => (herd.current_pos, herd.graze_range_radius(def)),
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
    at: CapacityStanding,
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
    let (anchor, radius) = herd_footprint_at(herd, def, at.penned);
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
    Some(flow / herd.fodder_per_biomass * herd_density_gain(&at.standing, herd, fauna))
}

/// **THE RUNG A CAPACITY READING IS STRUCK AT** — the two things [`ecological_carrying_capacity`]
/// takes off the husbandry ladder rather than off the land: *where does this herd stand* (which
/// [`herd_density_gain`]) and *is it behind a fence* (which [`herd_footprint_at`]).
///
/// It exists so the **live** `K` and the **destination** `K` are the same call with a different
/// standing. The pair is carried together because they are not independent — a herd at
/// `animal:pen` is by construction fenced — but they are not derivable from each other either: a
/// fixture may seat a position without anchoring a pen, and the live reading must keep answering
/// off `corralled_at` the way it always has.
#[derive(Debug, Clone, Copy)]
struct CapacityStanding {
    /// The ladder standing the per-rung gain is interpolated on.
    standing: RungStanding,
    /// Whether the reading is taken over a **fenced footprint** rather than a roam range.
    penned: bool,
}

impl CapacityStanding {
    /// **WHERE THE HERD STANDS NOW** — the live reading `advance_herds` writes. `penned` is
    /// `corralled_at`, which is what the footprint seam has always branched on.
    fn live(herd: &Herd) -> Self {
        Self {
            standing: herd.standing(),
            penned: herd.corralled_at.is_some(),
        }
    }

    /// **WHERE THE HERD WILL STAND ONCE IT ARRIVES AT `rung`.** A pen already built stays a pen (a
    /// destination cannot un-fence a herd), so the fencing is *either* — which is also what keeps an
    /// `extend_pen` entry, whose destination is the rung it already holds, reading over its fence.
    fn arrived_at(rung: RungKey, herd: &Herd) -> Self {
        Self {
            standing: RungStanding::arrived_at(rung),
            penned: herd.corralled_at.is_some() || rung.is_at_or_above(RungKey::AnimalPen),
        }
    }
}

/// **THE RULE THAT TURNS A COMPUTED `K` INTO THE ONE A HERD ACTUALLY CARRIES**, stated once so the
/// live write and the destination quote cannot apply different ones:
///
/// - the seam answering `None` — a **non-grazing** species or an absent graze layer — keeps the
///   herd's frozen constant `K`, and a rung therefore buys it nothing;
/// - a **penned** herd on a wholly-barren footprint (`Some(0)`) likewise keeps it, Grazing 2d §2.3's
///   "a rock pen holds its herd on the granary" — crushing a pen to zero is the state that guard
///   exists to prevent.
///
/// `standing_capacity` is what the herd is carrying today, which is what both arms fall back to.
fn settled_capacity(computed: Option<f32>, penned: bool, standing_capacity: f32) -> f32 {
    match computed {
        Some(k) if !(penned && k <= 0.0) => k,
        _ => standing_capacity,
    }
}

/// **THE CAPACITY THIS HERD WILL CARRY AT THE RUNG ITS BUILD IS HEADING FOR** — `None` when no band
/// has queued it, because then there is **no destination to quote**, which is a different statement
/// from a capacity of zero (a barren range, which is a real reading a real herd can have).
///
/// # ⛔ IT IS THE ONE `K` SEAM AT A SECOND STANDING — never a second formula
///
/// [`ecological_carrying_capacity`] is the only place `herd.carrying_capacity` is written, and this
/// calls **it**, with [`CapacityStanding::arrived_at`] in place of the live standing and
/// [`settled_capacity`] applying the same write rule. So the number advertised while the build runs
/// is the number the herd is handed when it lands.
///
/// **The rung moves; the land does not.** The flow is summed over the graze as it stands **today**,
/// exactly as the live `K` is — a projection of a future range would be inventing pasture. So this
/// answers *"what would this herd hold if it stood on that rung right now"*, which is the only
/// question with an exact answer; the delivered figure differs by however much the land itself moved
/// in the meantime, the same way the live `K` moves turn to turn.
///
/// **The pen leg projects its FOOTPRINT too** ([`herd_footprint_at`]), because a Corral does not
/// merely multiply the range's `K` by `pen_density` — it swaps a roam range for a fenced one.
#[allow(clippy::too_many_arguments)]
pub fn herd_destination_capacity(
    herd: &Herd,
    def: Option<&SpeciesDef>,
    graze: &GrazeRegistry,
    prey: &[PreyDatum],
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
) -> Option<f32> {
    let destination = herd.build_destination?;
    let at = CapacityStanding::arrived_at(destination, herd);
    Some(settled_capacity(
        ecological_carrying_capacity(herd, def, graze, prey, fauna, width, height, wrap, at),
        at.penned,
        herd.carrying_capacity,
    ))
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
    ladder.rung(herd_rung_key(herd))
}

/// **[`herd_rung`] without the ladder** — the same reading answered as the key, for the callers that
/// walk the ladder from it ([`RungKey::above`]) rather than read a record's dials. The plant twin is
/// `forage::patch_rung_key`, and it exists for the same reason: the "penned → pen, tamed → pastoral,
/// else wild" test has exactly one home.
///
/// **RETIRED beside it: `herd_head_count`** — `biomass / body_mass`, briefly the scale term the
/// animal rungs' upkeep was quoted against. **A per-HEAD rate is the measurement error
/// `animals_per_herder` exists to prevent**, one level up: it says *"one keeper per 100 fowl but one
/// per 2 boar"* and invents a 45-herder steppe megaherd that is a pure artifact of the unit. The
/// rungs quote per **keeper-load** instead ([`herd_keeper_loads`] — `head count /
/// animals_per_herder`), which folds the species' own ratio in before the ladder sees it, so one rate
/// covers a shepherd's 300 sheep and a cowherd's 80 cattle.
pub(crate) fn herd_rung_key(herd: &Herd) -> RungKey {
    if herd.is_corralled() {
        RungKey::AnimalPen
    } else if herd.is_domesticated() {
        RungKey::AnimalPastoral
    } else {
        RungKey::AnimalWild
    }
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
/// and [`Herd::ladder_position`] is **never** decayed by neglect (it is monotone-up, earned via
/// `Tame`, and no animal rung declares a `meter_decay`).
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
///   exceeds the herd's rung's `upkeep.grace_turns` ([`RungDef::upkeep_grace_turns`] — `animal:pen`'s for a
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
        // **The raw count is the LADDER's** (`raw_herders_needed` — the keeping rung's
        // `upkeep_crew_needed` at this herd's keeper load), passed in so the hysteresis stabilizes
        // one definition rather than keeping a second `ceil` of its own.
        let raw = raw_herders_needed(herd, &fauna, &ladder);
        herd.stabilize_herders_needed(
            raw,
            animals_per_herder,
            animals_per_herder * fauna.husbandry.herders_hysteresis_fraction,
        );
        // The `tamed_this_turn` flag is still cleared each turn so it can never go stale — but its one
        // consumer, the retired tameness decay, is GONE (`docs/plan_fauna_neglect_escape.md` §2.1):
        // the herd's ladder position is monotone-up now, never bled by neglect.
        herd.tamed_this_turn = false;
        // **And the turns estimate with it**, on the same one-turn cycle: a build the keeper walked
        // away from must stop publishing a finish date, and the labor arm re-stamps it this turn if a
        // crew is still on it (Logistics runs before Population).
        herd.build_turns_remaining = None;
        herd.build_work_from_gear = NO_BUILD_GEAR;
        herd.build_queue_position = crate::intensification::NOT_IN_ANY_BUILD_QUEUE;
        herd.build_blocked_reason = crate::intensification::BuildGate::Open;
        herd.build_destination = None;
        herd.build_legs = Vec::new();
        // **HOW WELL THE HERD WAS KEPT LAST TURN** — the same Population→Logistics lag
        // `pen_fed_fraction` runs on. Everything downstream of the staffing is resolved into locals
        // **here, before the field is cleared**, so the whole turn judges one reading: what went
        // unmet, how many animals that leaves uncontained, and whether anybody was on it at all. A
        // herd nobody worked reads the `0` its keeper never wrote, which is exactly right.
        //
        // It is derived from the one stored fact rather than kept in a second field beside it
        // (`herd_herded_fraction` for the published ratio), so the number the wire showed and the
        // shed the sim applies can never describe different staffings.
        let supplied_last_turn = herd.upkeep_supplied;
        let shortfall_last_turn = herd_upkeep_shortfall(herd, &fauna, &ladder);
        let overage_last_turn = uncontained_overage(herd, &fauna, &ladder);
        // **HOW SHORT THE KEEPING FELL, CAPTURED BEFORE THE FIELDS THAT SAY SO ARE CLEARED.** The
        // neglect pressure below rides this, and `upkeep_supplied` / `upkeep_demanded` are both wiped
        // a few lines down — reading it after would score every turn as wholly unkept and the
        // pressure could never fall.
        let shortfall_fraction_last_turn = crate::intensification::upkeep_shortfall_fraction(
            herd_keeping_basis(herd, &fauna, &ladder),
            herd.upkeep_supplied,
        );
        // **And the field is cleared now**, on the one-turn cycle the plant twin runs: it describes
        // the keepers that held the herd, so a herd whose keepers have gone must stop reporting what
        // they paid. Clearing it is what re-arms the shed — next turn's shortfall is the whole demand
        // again unless somebody restates it. (`advance_herds`' `regrow_biomass` reads it earlier in
        // the same Logistics stage, so its abandonment gate still sees last turn's value.)
        herd.upkeep_supplied = NO_UPKEEP_DEMAND;
        // …and the bill it was judged against, so "already stamped" always means *this* turn.
        herd.upkeep_demanded = None;
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
        let under_contained = herd_is_neglected(overage_last_turn);
        // **The neglect counter** — the animal twin of `ForagePatch::neglect_turns`. A herd whose
        // keepers can hold it is forgiven outright, so the grace measures *consecutive* neglect.
        if under_contained {
            herd.neglect_turns = herd.neglect_turns.saturating_add(1);
        } else {
            herd.neglect_turns = NEGLECT_NONE;
        }
        // **THE PRESSURE — a different quantity from the grace above, and the only one that decays.**
        // The grace is *forgiveness* and resets outright; this is the herd's **condition**, and it is
        // what the escape rate accelerates on. Keying the acceleration off the grace would let one
        // tended turn in every N erase it completely — measured, a herd survived indefinitely on one
        // tended turn in fourteen, at above its starting size.
        //
        // **It rises by the SHORTFALL FRACTION**, so half-staffed keeping frays it at half speed (the
        // *"I tend it, but not enough"* case), and **falls by the recovery rate** — slower than it
        // rose, which is the cost of neglect — on any turn the bill is met. It never resets on one
        // good turn.
        //
        // # ⛔ IT RISES ON THE SAME PREDICATE THE GRACE DOES, AND THAT IS WHAT BOUNDS IT
        //
        // It rose on `shortfall_fraction > 0` while the grace counted [`herd_is_neglected`], and the
        // two are not the same test: a **3-head herd kept at 90%** has an overage of `0.3` of an
        // animal, so the grace resets every turn and nothing ever sheds — while the pressure climbed
        // `+0.1` a turn, for ever, with nothing to spend it. It is the exponent in
        // `rate × (1 + escape_acceleration)^pressure`, so the turn that herd finally grew past the
        // one-animal gate the first shed fired at a rate clamped to the **whole flock**: a herd its
        // keeper had held at 90% for three hundred turns, and which had never once been
        // under-contained, lost everything in a single turn.
        //
        // Sharing the predicate is the whole fix, and no cap is wanted on top of it: while the herd
        // *is* neglected the counter climbs too, so the shed fires the moment the grace runs out and
        // spends the pressure it has accumulated. A herd cannot bank pressure it is not shedding
        // against.
        herd.neglect_pressure = if under_contained {
            herd.neglect_pressure + shortfall_fraction_last_turn
        } else {
            (herd.neglect_pressure - fauna.husbandry.neglect_recovery_rate.max(0.0))
                .max(NO_NEGLECT_PRESSURE)
        };
        // The rung whose keeping obligation this herd is under, through the one seam the wire's
        // countdown reads too ([`herd_keeping_rung`]).
        let grace = herd_keeping_rung(herd, &ladder)
            .map_or(NO_NEGLECT_GRACE, |rung| rung.upkeep_grace_turns());
        if u32::from(herd.neglect_turns) > grace {
            if let Some(overage) = overage_last_turn {
                if let Some(event) = shed_uncontained_animals(
                    herd,
                    source_index,
                    overage,
                    herd.neglect_pressure,
                    &fauna,
                    &mut rng,
                ) {
                    shed_events.push(event);
                }
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
                            "status=under_herded short={:.2} needed={} herd={} x={} y={}",
                            shortfall_last_turn, herd.herders_needed, herd.id, pos.x, pos.y
                        )),
                    ));
                }
            }
        } else {
            herd.under_herded = false;
        }

        // **BLEED-OUT ON TOTAL ABANDONMENT (§2.4).** A herd with ZERO keepers last turn keeps
        // shedding — regrowth already suppressed
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
        // **"Zero keepers last turn"** is `upkeep_supplied == 0` — the same reading the retired
        // `herded_fraction == NOT_HERDED` was, off the one field that now carries it.
        let body_mass = herd.body_mass;
        if supplied_last_turn <= NO_UPKEEP_DEMAND && body_mass > 0.0 && herd.biomass < body_mass {
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
/// costs the **visible** axis (herd size), never the invisible one ([`Herd::ladder_position`], which
/// is monotone-up and never touched here).
///
/// **THE SHED IS THE SHORTFALL PENALTY, and it is continuous in it** (`docs/plan_standing_upkeep.md`
/// §2.4): the animals nobody has hands for are the ones that drift off, so half the keepers a herd
/// wants sheds at half rate. It no longer gates on `herded_fraction < FULLY_HERDED`, which was a
/// threshold answering *whether* a herd was under-contained rather than *by how much* — the same
/// step the plant web's binary `tended_this_turn` flag took.
///
/// The overage is [`uncontained_overage`] — the upkeep shortfall converted back into animals. A
/// **fraction of the OVERAGE** leaves, not of the total, so as the herd shrinks toward its capacity
/// fewer leave and it **stops exactly** at `overage < 1` — no overshoot below the real labor capacity,
/// and none to zero unless capacity is `0` (total abandonment). The count is in **whole animals**,
/// with a **min-1 floor** when the overage is `≥ 1` so a small overage clears instead of asymptoting
/// one or two over forever.
///
/// The rate is **per-rung**: `pen_escape_fraction` for a corralled herd (slower — the fence),
/// `pastoral_escape_fraction` otherwise, each `× (1 + jitter)` from the caller's seeded RNG. It reads
/// `is_corralled()` rather than the keeping rung, deliberately: a half-raised pen has no fence yet,
/// so it leaks at the open-range rate while it owes the pen rung's longer grace. Reduces this herd's
/// biomass and returns the placement event, or `None` when nothing leaves this turn.
fn shed_uncontained_animals(
    herd: &mut Herd,
    source_index: usize,
    overage_animals: f32,
    // **The herd's accumulated neglect pressure** ([`Herd::neglect_pressure`]) — its *condition*, not
    // a count of turns. The rate compounds on this, which is what makes an abandoned herd terminate
    // rather than settle, and what one tended turn cannot erase.
    pressure: f32,
    fauna: &FaunaConfig,
    rng: &mut SmallRng,
) -> Option<ShedEvent> {
    let body_mass = herd.body_mass;
    let husbandry = &fauna.husbandry;
    let rate = if herd.is_corralled() {
        husbandry.pen_escape_fraction
    } else {
        husbandry.pastoral_escape_fraction
    };
    // **⛔ THE RATE ACCELERATES, AND WITHOUT THAT THE HERD NEVER LEAVES.** A constant fraction of the
    // overage balances against the growth curve — measured, a wholly unkept pastoral aurochs herd
    // settled at **64% of `K`, still owned, for ever**. The design is that it terminates: *"if no
    // herders are present, eventually, the entire herd leaves and you are left with nothing. The
    // longer you don't tend it, the quicker the remaining herd leaves, meaning it isn't linear."*
    //
    // **Compounding rather than a linear ramp**, because the ruling is about the *rate*: the overage
    // this multiplies is itself shrinking with the herd, so a linear ramp leaves a shallow tail that
    // a fast breeder can still out-run. `(1 + accel)^turns` cannot be out-run by any `r`.
    //
    // **The fence still buys time** — a pen starts from its own slower `pen_escape_fraction` and
    // accelerates from there, so it arrives at nothing *later*, not never.
    let rate = rate * (1.0 + husbandry.escape_acceleration.max(0.0)).powf(pressure.max(0.0));
    let jitter_band = husbandry.escape_fraction_jitter;
    let jitter = if jitter_band > 0.0 {
        rng.gen_range(-jitter_band..=jitter_band)
    } else {
        0.0
    };
    // **Clamped at the whole herd**: you cannot shed a larger share than is standing there, and an
    // accelerating rate reaches `1.0` quickly by design.
    //
    // **It is belt-and-braces, not load-bearing** — `escaped_biomass` below already takes
    // `.min(herd.biomass)`, so removing this changes no observable number and no test fails. It is
    // kept because an unclamped *fraction* is a nonsense reading for anything that later reads the
    // rate, and because the downstream `min` is a clamp on the wrong quantity to rely on.
    let jittered = (rate * (1.0 + jitter)).clamp(0.0, WHOLE_HERD);
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
///
/// **IT IS THE RUNG THE POSITION IS UNWINDING**, the twin of `forage::patch_unwinding_rung`: the
/// **newest** rung carrying work — the pen while it has any, then the pastoral rung under it —
/// and `None` on a herd nobody has worked, which is nobody's to neglect.
///
/// It reads the standing rather than a pair of meters, so it cannot fall a turn behind the payment
/// side the way the retired hand-written progress copy did.
pub fn herd_keeping_rung<'a>(herd: &Herd, ladder: &'a LadderConfig) -> Option<&'a RungDef> {
    let standing = herd.standing();
    if herd.ladder_position() <= RUNG_UNSTARTED {
        return None;
    }
    // The rung being raised owns the risk while it carries work; otherwise the held rung does.
    let key = standing
        .raising
        .filter(|_| standing.credit > NO_RUNG_CREDIT_HELD);
    Some(ladder.rung(key.unwrap_or(standing.held)))
}

/// **THE WHOLE OF A RUNG** — the fraction [`Herd::rung_fraction`] reports for a rung the herd holds.
/// Named rather than a bare `1.0` because it is a *completeness*, not a multiplier.
const WHOLE_RUNG: f32 = 1.0;

/// **A RUNG BEING RAISED THAT CARRIES NOTHING YET** — the credit below which the *held* rung is the
/// one at risk. Named rather than a bare `0.0` because an `on_completion` rung reads exactly this
/// while its meter fills, which is what keeps a herd fencing its range on the **pastoral** grace.
const NO_RUNG_CREDIT_HELD: f32 = 0.0;

/// **WHICH RUNG THIS HERD IS BUILDING** — the animal twin of `forage::patch_build_verb`, with the
/// same rule: the player declares for a meter at zero, and a meter carrying progress declares for
/// itself.
///
/// # BOTH ANIMAL METERS ARE MONOTONE, AND THAT READS CORRECTLY
///
/// [`Herd::ladder_position`] is monotone-up on both animal rungs (the neglect-escape arc retired the
/// taming bleed, and no animal rung declares a `meter_decay`), so a rung derived as *building* stays
/// that way **until it completes** — a half-tamed herd is permanently building. That is the honest reading rather than a
/// loop: it means the `Tame` is still in flight, which it is, and the herd accrues the moment
/// builders are staffed. Nothing re-declares anything, because nothing is written.
///
/// The consequence the plant web has — a completed rung eroding back into the building state — is
/// therefore unreachable here, which is why no animal rung declares a `meter_decay`.
pub fn herd_build_verb(
    herd: &Herd,
    declared: Option<crate::components::Improvement>,
) -> Option<crate::components::Improvement> {
    use crate::components::Improvement;
    // **The rung the position is actually raising declares for itself**, which is the one-position
    // form of the retired newest-meter-first walk: a pen with work on it governs, so a `Tame`
    // declared on a herd already fencing is dead rather than stalled.
    //
    // # ⛔ IT IS THE WORK BANKED, NOT THE CREDIT — an `on_completion` rung has none
    //
    // The pen arm asked `standing.credit > 0`, and `animal:pen` is `partial_credit: on_completion`,
    // which `RungStanding::at` pins to `NO_RUNG_CREDIT` at **every** position short of full. So the
    // arm could never fire: a herd with real work banked on its fence and no live declaration (what
    // `banking.declared_on` answers once the queue entry is gone) fell through and derived **no
    // verb**, where the retired two-meter walk answered `Corral`. `RungStanding::banked` is what a
    // meter carries rather than what it is worth, so it is positive from the first work banked on
    // either rung — which is also why one arm now serves both.
    let standing = herd.standing();
    if standing.banked > NO_RUNG_WORK_BANKED {
        if let Some(verb) = standing.raising.and_then(RungKey::builder_verb) {
            return Some(verb);
        }
    }
    // Nothing is part-way up a rung here, so a declaration is the player starting one.
    match declared {
        Some(Improvement::Corral) if !herd.corral_meter_full() => Some(Improvement::Corral),
        Some(Improvement::Tame) if !herd.is_domesticated() => Some(Improvement::Tame),
        _ => None,
    }
}

/// **IS THE RUNG THIS QUEUE ENTRY DECLARED ALREADY STANDING?** — the animal twin of
/// `forage::patch_rung_already_built`, and the test that retires a **dead** entry
/// (`docs/plan_standing_upkeep.md` §2.5).
///
/// # IT IS THE METER'S OWN FULLNESS, NOT THE RETAIN BAR
///
/// It asks exactly what [`herd_build_verb`] asks — `is_domesticated()` / `corral_meter_full()` —
/// because the two must never disagree about whether there is work left. A *retain-bar* test
/// (`is_corralled()`, which is what `validate_corral` asks the **player**) would answer *"already
/// built"* for a meter that has eroded below its cost but not past its bar, i.e. for a rung the
/// builders are legitimately repairing.
///
/// A rung the other web owns is never already built here — a `Cultivate` cannot stand on a herd —
/// and the entry's own web decides which arm it reaches.
pub fn herd_rung_already_built(herd: &Herd, declared: crate::components::Improvement) -> bool {
    use crate::components::Improvement;
    match declared {
        Improvement::Tame => herd.is_domesticated(),
        Improvement::Corral => herd.corral_meter_full(),
        Improvement::Cultivate | Improvement::Sow => false,
    }
}

/// **CANCEL EVERY RING NAMED BY AN ENTRY THAT JUST LEFT A BUILD QUEUE.**
///
/// `extend_pen` sets `Herd::pen_extending` **before** it queues, and only completion cleared it — so
/// an entry dropped mid-ring left the flag set with nothing left to fund the ring, and
/// [`Herd::begin_pen_extension`] refuses while it is set. That is a **permanent** dead end on that
/// pen, one `✕` click away.
///
/// Handed the entries a drop produced (`LaborAllocation::prune_build_queue` returns them; the two
/// command seams below read theirs before dropping), so every exit passes through one rule rather
/// than each remembering it.
pub fn cancel_dropped_rings(
    herds: &mut HerdRegistry,
    dropped: &[crate::components::BuildQueueEntry],
) {
    for entry in dropped {
        let (crate::components::BuildJob::ExtendPen, crate::components::BuildSource::Herd(id)) =
            (&entry.declared, &entry.source)
        else {
            continue;
        };
        if let Some(herd) = herds.herds.iter_mut().find(|herd| &herd.id == id) {
            herd.cancel_pen_extension();
        }
    }
}

/// **WITHDRAW A DECLARATION, AND STOP THE RING IT WAS FUNDING** — the `unqueue` command's whole
/// effect on one band. Returns whether an entry was there.
///
/// A `World`-level seam rather than a method on [`crate::components::LaborAllocation`] because the
/// ring lives on the **herd**, and an allocation holds no registry. It is what `handle_unqueue`
/// calls, so the command and the guard cannot come apart.
pub fn unqueue_build_and_cancel_ring(
    world: &mut World,
    band: Entity,
    source: &crate::components::BuildSource,
) -> bool {
    let Some(mut allocation) = world.get_mut::<crate::components::LaborAllocation>(band) else {
        return false;
    };
    let entry = allocation.build_queue_entry(source).cloned();
    if !allocation.unqueue_build(source) {
        return false;
    }
    cancel_dropped_rings(&mut world.resource_mut::<HerdRegistry>(), entry.as_slice());
    true
}

/// **PUT A HOLDING DOWN, AND STOP THE RING IT WAS FUNDING** — the `abandon` command's whole effect on
/// one band, and [`unqueue_build_and_cancel_ring`]'s twin. Returns whether the band held the source.
///
/// The row's entry goes with it (`LaborAllocation::drop_source_row` prunes on the same edge), so the
/// ring is read off the queue **before** the drop.
pub fn drop_holding_and_cancel_ring(
    world: &mut World,
    band: Entity,
    target: &crate::components::LaborTarget,
) -> bool {
    let Some(mut allocation) = world.get_mut::<crate::components::LaborAllocation>(band) else {
        return false;
    };
    let entry = crate::components::BuildSource::of(target)
        .and_then(|source| allocation.build_queue_entry(&source).cloned());
    if !allocation.drop_source_row(target) {
        return false;
    }
    cancel_dropped_rings(&mut world.resource_mut::<HerdRegistry>(), entry.as_slice());
    true
}

// **RETIRED: `herd_is_maintaining`** — the animal twin of the retired `forage::patch_is_maintaining`,
// and retired for the same reason (`docs/plan_standing_upkeep.md` §4.6a). The meter's **fullness**
// decided who supplied the maintenance rate; nothing about how full a meter is decides who pays. The
// band's `husbandry` pool owes the rate for every meter carrying work, at any fullness, and a build
// crew supplies nothing toward it.

/// **WHAT HAS BEEN SUNK INTO THE METER AT RISK** — the animal twin of `forage::patch_at_risk_cost`,
/// and the ordering key of [`crate::intensification::UpkeepFundMode::Priority`]: a band short of
/// keepers holds its pen before its unfenced flock.
pub fn herd_at_risk_cost(herd: &Herd) -> f32 {
    herd.ladder_position()
}

/// **THE WORK THE AT-RISK METER WAS OWED THIS TURN, AND THE KEEPING POOL OWES ALL OF IT** — the
/// animal twin of `forage::patch_upkeep_supply` (`docs/plan_standing_upkeep.md` §2.4/§4.6a).
///
/// **A meter carrying work is billed to the band's `husbandry` pool at any fullness** — from the
/// first work banked until the last — and a build crew supplies nothing toward it. A `Tame` in
/// flight owes what a tamed herd owes, to the same hands.
///
/// `keeping_share` is this herd's slice of that pool; [`NO_UPKEEP_DEMAND`] where there is no meter
/// to hold and none being started.
pub fn herd_upkeep_supply(
    herd: &Herd,
    improvement: Option<crate::components::Improvement>,
    keeping_share: f32,
) -> f32 {
    if herd_claims_keeping(herd, improvement) {
        keeping_share
    } else {
        NO_UPKEEP_DEMAND
    }
}

/// **DOES THIS HERD DRAW ON THE BAND'S KEEPING POOL AT ALL?** — the animal twin of
/// `forage::patch_claims_keeping`: there is work on the ladder to hold, **or** a verb in flight that
/// is about to bank some.
///
/// # ⛔ THE VERB TERM IS THE ONE-TURN CARRY, AND IT IS ALL THAT SURVIVES OF `herd_keeping_meter`
///
/// `maintenance_shares` runs **before** the turn's build accrual and the capture reads the herd
/// **after** it. On the turn a Tame banks its first work, a claim resolved on the position alone
/// reads zero, the share comes back zero, and the capture then publishes `supplied 0` against a live
/// demand on a **staffed** `husbandry` role. That is the defect the retired meter's `by_verb` term
/// was added for, and it survives here in the only form the interpolated demand still needs.
///
/// **Exhaustive on the verb, on purpose** — a new animal verb falling through to `false` would leave
/// its first turn unclaimed, which is precisely that bug.
pub fn herd_claims_keeping(
    herd: &Herd,
    improvement: Option<crate::components::Improvement>,
) -> bool {
    let by_position = herd.ladder_position() > RUNG_UNSTARTED;
    let by_verb = improvement.is_some_and(|verb| {
        use crate::components::Improvement;
        match verb {
            Improvement::Tame | Improvement::Corral => true,
            Improvement::Cultivate | Improvement::Sow => false,
        }
    });
    by_position || by_verb
}

// **RETIRED: `herd_keeping_meter`** — *"which of the two meters this turn's keeping answers for"*,
// and the animal twin of the plant web's `patch_keeping_meter`, retired for the same reason
// (`docs/plan_standing_upkeep.md` §2.8/§4.10). It existed because the demand **stepped**: a herd was
// billed the whole pastoral rate the instant `accrue_domestication` recorded an owner, so the claim
// side and the payment side — split across the Population→Logistics carry — had to agree which side
// of that step they were on, and a verb term was the only thing that could tell them.
//
// **The demand interpolates now, so there is no step to straddle.** Its two remaining jobs split
// exactly as the plant web's did: *how much* is `herd_upkeep_demand`, which reads the standing and
// needs no verb, and *does this claim at all* is `herd_claims_keeping`, which keeps the verb term for
// the one-turn carry.
//
// **The pen's step did not need it either.** `animal:pen` is `partial_credit: on_completion`, so
// `RungStanding::credit` is already `0` for a pen in flight — a herd fencing its range interpolates
// between wild and *pastoral* and reaches the pen's rate only when the fence closes. That is the
// same "half a fence is not half a pen" rule the meter's old comment stated, now enforced by the
// standing rather than by a branch.

/// **WHAT THIS HERD'S AT-RISK METER WILL LOSE ON THE NEXT DECAY PASS**, in work units — the plant twin's
/// shape (`forage::patch_meter_rot`), through the same [`RungDef::meter_rot`] seam.
///
/// **IT IS ALWAYS `0` ON THE SHIPPED LADDER, AND THAT IS NOT AN OMISSION.** Neither animal rung
/// declares a `meter_decay`: an under-kept flock **sheds animals** at `fauna_config`'s own escape
/// fractions, which are already the rate, so a second one on the rung would be two numbers for one
/// mechanic. Nothing eats an animal build — a `Tame` or `Corral` with any crew on it publishes a real
/// finish date however short its keeping is, and the price of that shortfall is paid in animals
/// rather than in meter. The seam exists so the countdown and the wire read one number on both webs;
/// do not "fix" the missing red.
pub fn herd_meter_rot(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    herd_keeping_rung(herd, ladder).map_or(NO_UPKEEP_DECAY, |rung| {
        rung.meter_rot(
            herd_keeper_load(herd, fauna),
            herd.upkeep_supplied,
            herd.neglect_turns,
        )
    })
}

/// **WHAT IT COSTS TO HOLD THIS HERD THIS TURN**, in work units — **interpolated on the herd's own
/// standing** at this herd's keeper load, and [`NO_UPKEEP_DEMAND`] on a wild herd, which is nobody's
/// to keep.
///
/// **THE one definition**, reached by the shed, the labor arm's stamp and the snapshot alike.
///
/// # ⛔ IT USED TO CHARGE 100% OF THE COST FROM DAY ONE FOR 0% OF THE BENEFIT
///
/// `Herd::accrue_domestication` sets `owner` on its **first** call, and the retired
/// `herd_keeping_meter` read *"owned ⇒ pastoral"* — so from the first turn of a 50-unit Tame the herd
/// owed the **whole** pastoral rate and went on owing exactly that until the meter filled, while
/// every payout waited for `is_domesticated()`. That is §2.8's asymmetry inverted: the cost arrives
/// whole and the benefit arrives last. Both now ride [`interpolate`] over the same standing, so a
/// herd 10% into a Tame owes 10% of the rate and breeds 10% of the way to pastoral.
///
/// **AND THE VERB TERM IS GONE WITH THE STEP.** It existed because the claim side
/// (`maintenance_shares`, before the accrual) and the payment side (the capture, after it) had to
/// agree which meter they meant across a **discontinuity** — a herd that was wild when the shares
/// were split and owned when the bill was read. There is no step left to straddle: the demand is a
/// continuous function of the position, and at a position of zero it is `NO_UPKEEP_DEMAND` on both
/// sides of the accrual. This is the same deletion `forage::patch_upkeep_demand` made when the plant
/// demand began interpolating. What the verb still decides — *does this source claim a share at all
/// before it has banked anything* — is [`herd_claims_keeping`].
pub fn herd_upkeep_demand(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    let load = herd_keeper_load(herd, fauna);
    interpolate(&herd.standing(), |rung| {
        ladder.rung(rung).upkeep_demand(load)
    })
}

/// **WHAT WENT UNMET THIS TURN**, in work units — [`herd_upkeep_demand`] less what the meter's own
/// crew supplied, floored at zero.
///
/// **Derived, never stored**, for the reason the plant twin is: the labor arm only visits herds some
/// band is assigned to, so a stored shortfall would read a tidy `0` on exactly the abandoned herds
/// that are shedding.
pub fn herd_upkeep_shortfall(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    crate::intensification::upkeep_shortfall(
        herd_keeping_basis(herd, fauna, ladder),
        herd.upkeep_supplied,
    )
}

/// **THE BILL THE KEEPING IS JUDGED AGAINST** — [`Herd::upkeep_demanded`] where a band answered for
/// this herd, and the live [`herd_upkeep_demand`] where none did. The animal twin of
/// `forage::patch_keeping_basis`.
///
/// **One function, three readers** — the shed, the published shortfall and the published demand — so
/// what the sim sheds against, what the wire bills and what the player sees cannot be three numbers.
/// The `None` arm is what keeps an **abandoned** herd honest: nobody was handed a bill, so the whole
/// of the live one went unmet.
pub fn herd_keeping_basis(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    herd.upkeep_demanded
        .unwrap_or_else(|| herd_upkeep_demand(herd, fauna, ladder))
}

/// **HANDS TO MEET THIS HERD'S KEEPING BILL** — `ceil` of [`herd_keeping_basis`], and the animal twin
/// of [`crate::forage::patch_upkeep_workers_needed`], seam for seam.
///
/// # ⛔ IT IS NOT [`herd_herders_needed`], AND THE TWO ANSWER DIFFERENT QUESTIONS
///
/// This one asks *"how many hands does the bill this herd was handed take?"* — so it **moves with the
/// herd's ladder position**, because the bill does ([`herd_upkeep_demand`] interpolates on the
/// standing). [`herd_herders_needed`] asks *"how many keepers does a flock of this species and this
/// size want?"* — the head-count requirement at the rung's own rate, which is position-**independent**
/// by construction and is what the hysteresis stabilizes and what a pre-commit quote
/// ([`would_be_herders_needed`]) has to state before any position exists.
///
/// **Publishing the second one as the first is what this exists to stop.** The wire states the
/// identity `upkeepWorkersNeeded == ceil(upkeepDemand / PER_WORKER_OUTPUT)` and tells the client to do
/// no arithmetic of its own; `herd_herders_needed` reads the rung's **bare** rate, so a herd a tenth of
/// the way up a Tame was billed `0.185` work and told to staff **two** keepers — the card asking for a
/// crew twice the size of the bill for the whole middle of a Tame, and the player staffing it, because
/// the panel said so. The two agree again at the top of the rung, where the interpolation reaches the
/// rate; they diverge everywhere below it.
///
/// The `ceil` rounds **up**, so any live bill at all asks for at least one keeper: you cannot send a
/// fiftieth of a person. [`NO_CREW_ON_THIS_ACTIVITY`] on a herd that owes nothing — wild, or standing
/// on a rung that declares no upkeep.
pub fn herd_upkeep_workers_needed(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> u32 {
    let demand = herd_keeping_basis(herd, fauna, ladder);
    if demand <= NO_UPKEEP_DEMAND {
        return NO_CREW_ON_THIS_ACTIVITY;
    }
    (demand / crate::intensification::PER_WORKER_OUTPUT).ceil() as u32
}

/// **HOW WELL THIS HERD IS KEPT** — `min(1, supplied / demand)`, the ratio the wire publishes and the
/// regrowth's abandonment gate used to read off a stored field.
///
/// Derived from the one stored fact ([`Herd::upkeep_supplied`]) rather than written beside it, so the
/// published ratio and the shed can never disagree about the same turn's staffing. A herd that owes
/// nothing — wild, or empty — is trivially [`FULLY_HERDED`].
pub fn herd_herded_fraction(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> f32 {
    let demand = herd_keeping_basis(herd, fauna, ladder);
    if demand <= NO_UPKEEP_DEMAND {
        return FULLY_HERDED;
    }
    (herd.upkeep_supplied / demand).clamp(NOT_HERDED, FULLY_HERDED)
}

/// **Turns of neglect this herd can still absorb before its keepers start losing animals** — the wire's
/// countdown, resolved through [`herd_keeping_rung`] so it always describes the rung
/// [`advance_husbandry`] actually gates the shed on. `None` = a wild herd, with nothing at risk.
///
/// It reads the **upkeep's** grace ([`RungDef::upkeep_grace_turns`]), not the build's, exactly as
/// [`crate::forage::patch_neglect_grace_remaining`] does: on both webs the neglect trigger is an unmet
/// standing demand rather than an un-worked build, so every buildable rung declares
/// `build.grace_turns: null` and the live number lives in its `upkeep` block. Reading the build's
/// gave [`crate::intensification::NO_NEGLECT_GRACE`] for every herd, so a **fully kept** herd
/// published *"sheds in 1 turn"* forever while [`advance_husbandry`] gated the shed on a grace of 2.
pub fn herd_neglect_grace_remaining(herd: &Herd, ladder: &LadderConfig) -> Option<u32> {
    herd_keeping_rung(herd, ladder).map(|rung| {
        crate::intensification::neglect_grace_remaining(
            herd.neglect_turns,
            rung.upkeep_grace_turns(),
        )
    })
}

/// **How many whole animals this herd's keepers cannot hold** — the measurement half of
/// [`shed_uncontained_animals`], split out because *being* under-contained and *losing animals for it*
/// are two questions with two answers once a neglect grace sits between them: the under-herded feed
/// notice fires on the first, the shed only on the second.
///
/// `None` means the herd fits its labor capacity (or is within one animal of it) — the self-limiting
/// attractor — or has no measurable stock at all.
///
/// # It IS the upkeep shortfall, converted into animals
///
/// **The overage is `shortfall_in_loads × animals_per_herder`** (`docs/plan_standing_upkeep.md`
/// §2.4), which is the same number the retired `herded_fraction × herders_needed ×
/// animals_per_herder` capacity reconstruction produced and is now read off the one seam the whole
/// term goes through. It is continuous in the staffing by construction — half the keepers a herd
/// wants leaves half its animals uncontained, where the retired `herded_fraction < FULLY_HERDED`
/// gate was a threshold that said only *whether* a herd was under-contained.
///
/// **`MIN_ESCAPE_ANIMALS` is the animal branch's quantum, and it is why the counter can differ from
/// the plant web's.** A plant meter is continuous, so any shortfall bleeds; a herd loses **whole
/// animals**, so a shortfall of less than one animal is not under-containment at all — the herd is
/// within a head of its keepers' capacity and nothing can leave. That is the same whole-animal
/// discipline `quantise_animal_take` imposes on the take.
/// **⛔ IS THIS HERD NEGLECTED THIS TURN? — THE ONE PREDICATE, SHARED BY THE GRACE AND THE PRESSURE.**
///
/// A managed herd is neglected on a turn its keeping shortfall leaves at least one **whole animal**
/// uncontained ([`uncontained_overage`], whose [`MIN_ESCAPE_ANIMALS`] quantum is the animal branch's
/// own: a herd loses whole beasts, so a shortfall too small to free one is not under-containment).
///
/// **It exists because two things that must move together were driven by two tests.**
/// `Herd::neglect_turns` counted this one; `Herd::neglect_pressure` rose on any positive shortfall
/// fraction at all. A 3-head herd kept at 90% therefore reset its grace every turn — nothing ever
/// shed — while the pressure climbed for ever, and the turn the flock grew past the whole-animal gate
/// the first shed fired at a rate the accumulated exponent had clamped to the entire herd. One
/// function, called twice, is what makes that unrepresentable rather than merely fixed.
///
/// **It takes the overage rather than the herd**, because `advance_husbandry` captures that reading
/// *before* it clears `upkeep_supplied` and `upkeep_demanded` for the turn — re-resolving it
/// afterwards would score every herd as wholly unkept.
fn herd_is_neglected(overage_last_turn: Option<f32>) -> bool {
    overage_last_turn.is_some()
}

fn uncontained_overage(herd: &Herd, fauna: &FaunaConfig, ladder: &LadderConfig) -> Option<f32> {
    let body_mass = herd.body_mass;
    if body_mass <= 0.0 || herd.biomass <= 0.0 {
        return None;
    }
    // **THE SHORTFALL FRACTION, STRAIGHT ONTO THE HEAD COUNT.** `shortfall_in_loads ×
    // animals_per_herder` *is* `shortfall_fraction × head count` — the loads cancel — so reading the
    // fraction says the same thing without reconstructing a per-load rate, and it keeps working when
    // the supplier is a **build crew** rather than the keeping pool (a herd mid-`Tame` is owed the
    // same rate, from different hands).
    let fraction = crate::intensification::upkeep_shortfall_fraction(
        // **THE BILL THE KEEPERS WERE HANDED**, not the one the turn's own build has since raised —
        // see `Herd::upkeep_demanded`.
        herd_keeping_basis(herd, fauna, ladder),
        herd.upkeep_supplied,
    );
    let head_count = herd.biomass / body_mass;
    let overage_animals = (fraction * head_count).max(0.0);
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
                && h.ladder_position() == RUNG_UNSTARTED
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
/// carrying the escapees' biomass at `owner = None` / `ladder_position = RUNG_UNSTARTED` /
/// `corralled_at = None` — a fresh wild group whatever its origin stock. Reuses the source species' cached traits and
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
// **Clone, not `Copy`** — it carries a [`HuntingParty`], which carries the party's crews. A
// partly-equipped party is a list, so nothing holding one can be a bit-copy any more.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceYieldForecast {
    /// **Every field is a [`YieldAccounts`] — every scalar account per turn, never a food scalar**
    /// (`docs/plan_hunt_yield_model.md`, issue #337).
    ///
    /// **Why vectorised rather than sibling per-account scalars.** Sibling scalars double the
    /// surface and let the halves drift apart under a retune; one vector per rung cannot, because
    /// `ceiling_at` hands every component to every reader at once.
    ///
    /// **The forecast is scalar-only, and a wolf therefore forecasts `0`** (arc #527). What an
    /// inedible species pays is **materials**, which are batches carrying a characteristic vector
    /// each and cannot be added, scaled or `min`'d the way this type's components are. A wolf's
    /// forecast reading `0 food` is honest — it is not food — but it is also *silent* about the
    /// pelts the take really banks. Projecting materials is its own arc.
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
    /// **THE GROWTH THE TAKE THIS FORECAST PRICES WILL SEE**, in biomass — the third term of
    /// [`take_room`], and the reason this type can hold the take's own backstop rather than a
    /// narrower reading of it.
    ///
    /// # ⛔ IT IS NEXT TURN'S GROWTH, NOT [`Herd::growth_this_turn`] READ WHERE THE FORECAST STANDS
    ///
    /// Every consumer of this type resolves it **after** the Population take — the capture publishes
    /// a row in the Snapshot stage, the assign-time seed answers a client between turns — and
    /// `growth_this_turn` is `biomass − biomass_before_regrowth`, with the take subtracted from
    /// `biomass` *after* `regrow_biomass` stamped the pair. On a source harvested at or above its
    /// growth that field reads **zero**, so a forecast reading it there would switch off precisely
    /// the backstop that exists to pay a source sitting on its floor.
    ///
    /// So both producers fill it from the **regrow-first projection** the crew curve already runs
    /// ([`next_turns_quarry`], `forage::next_turns_stand`) — the same rule
    /// [`hunt_crew_take_curve`]'s doc states at length: *a forecast regrows first, because the take
    /// it prices runs after Logistics.*
    ///
    /// `0.0` on a source that is not growing, which pays nothing through the backstop and leaves the
    /// escapement room to answer alone — the reading every forecast had before the backstop existed.
    pub growth: f32,
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
    /// **The BIOMASS [`Self::managed_yield`] and [`Self::pastoral_yield`] are the conversion of** —
    /// the two investment rungs' harvests before any rate is applied.
    ///
    /// **The MATERIAL account needs them and the scalar accounts do not**, which is the same
    /// asymmetry `forage::field_harvest_biomass` already states one web over: a material's
    /// `per_biomass` is a rate on the *carcass*, and an inedible species' currency components are
    /// all `0`, so there is no currency to scale off. Without these a wolf's Tame and Corral rungs
    /// could quote nothing at all — which is exactly what the retired `pastoralTrade`/`corralTrade`
    /// used to say and what their material replacements say now.
    ///
    /// `0.0` on a source that offers neither rung (a forage patch, a herd already penned), matching
    /// the `NO_PASTORAL_YIELD` convention beside it.
    pub managed_yield_biomass: f32,
    /// See [`Self::managed_yield_biomass`].
    pub pastoral_yield_biomass: f32,
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

/// **The biomass an investment rung a source does not offer would harvest: none.** The biomass twin
/// of [`NO_PASTORAL_YIELD`], named rather than a bare `0.0` because it is the statement *"there is no
/// such rung here"* rather than a measurement of an empty one.
pub(crate) const NO_INVESTMENT_RUNG_BIOMASS: f32 = 0.0;

/// [`SourceYieldForecast::pastoral_yield`] for a source that never offers the `Tame` verb — a forage
/// patch, or a herd already penned/forage-tended. `0` = *no Tame payoff to advertise*, the pastoral
/// twin of `PLANTS_DO_NOT_QUANTISE`.
pub(crate) const NO_PASTORAL_YIELD: YieldAccounts = YieldAccounts::ZERO;

/// **The retreat term of a source that has no retreat stage** — every plant patch and every pen,
/// which [`SourceYieldForecast::fight`] answers `None` for (a plant does not bolt, a penned animal is
/// slaughtered rather than stalked).
///
/// `1.0` is the identity, and it is safe *here specifically* because those same sources forecast
/// `engage_rate: f32::INFINITY`: their engagement crew is already `0` whatever this term says, so the
/// value cannot reach an answer. **It is not a stand-in for an unresolved party** — a hunting source
/// must pass its own [`HuntingParty::stay_fraction`], or the crew stops matching the take.
pub(crate) const NO_RETREAT_STAGE_STAY: f32 = 1.0;

/// The biomass a **per-unit rate** is the yield of — `1.0`, so `HuntYield::apply(ONE_UNIT_OF_BIOMASS)`
/// reads as *"what is one unit of this stock worth"* rather than as an unexplained `1.0` argument to
/// a function whose other callers pass a real take.
pub(crate) const ONE_UNIT_OF_BIOMASS: f32 = 1.0;

impl SourceYieldForecast {
    // **RETIRED: `NO_STANDING_STOCK_TO_DRAW_DOWN`** — the sentinel biomass a managed forecast
    // carried because it had no stock to stop short of. Every rung has one now.

    // **RETIRED: `SourceYieldForecast::managed`** — the constructor for a source whose take ignored
    // the floor. Both webs' rung-3 sources are drawn down now, so nothing builds one and
    // `managed_production` is always `None`.

    /// **THE yield/turn cap this source pays at `floor`** — the one computation every reader of this
    /// type goes through, and the exact twin of the take path's [`herd_take_room`] /
    /// `forage::patch_take_room`:
    ///
    /// ```text
    /// take_room(floor, B, K, growth) × per_biomass_yield
    /// ```
    ///
    /// # ⛔ IT IS [`take_room`], NOT [`escapement_ceiling`] — THE TAKE'S BOUND, WHOLE
    ///
    /// The room *or* the share of the turn's growth the floor leaves takeable, whichever is larger,
    /// through the very function `hunt_take`/`forage::forage_take` are bounded by. It read the bare
    /// escapement room while the take paths carried the backstop, and on a source at or below a
    /// floor its own build raised the two then said different things: the take handed over
    /// `growth × (1 − floor)` while the row published `actual 0`, `range 0` and no useful crew at
    /// all. The growth term rides on [`Self::growth`] precisely so this stays one function rather
    /// than a reimplementation that agrees today.
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
        let room = take_room(floor, self.biomass, self.carrying_capacity, self.growth);
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
/// `collection` is **`workers × per_worker_yield`** and `workers` is the **take** crew, so the whole
/// expression is
///
/// ```text
/// min(workers × per_worker_yield, ceiling_at(floor))
/// ```
///
/// which is also the composition the client draws its curve from: every term ships.
///
/// **THERE IS NO BUILD TERM, and that is why the function takes no `improvement`**
/// (`docs/plan_standing_upkeep.md` §2.2). A rung's `yield_fraction_while_building` is retired on both
/// webs: a build is staffed in its own right, so what these hands carry cannot depend on what the
/// builders beside them are doing. `forecast == actual` therefore holds through a build for the
/// stronger reason that neither side has a build term to get wrong.
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
) -> YieldAccounts {
    forecast_production_and_take_at(forecast, workers, floor, HuntDraw::EXPECTED).1
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
    sigmas: f32,
) -> TakeRange {
    let at = |sigmas: f32| {
        forecast_production_and_take_at(forecast, workers, floor, HuntDraw::Quantile { sigmas }).1
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
    per_worker_biomass_capacity: f32,
    // The party doing the hunting — the fight is a per-turn bound on the projected take exactly as
    // the engagement is (see the doc above).
    party: &HuntingParty,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
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
    let _corralled = quarry.is_corralled();
    // **`workers` IS THE TAKE CREW** (`docs/plan_standing_upkeep.md` §2.2) — the same term
    // `systems::hunt_take` is paid at, so the projection and the take stay one model. A build on
    // this herd is its own allocation with its own hands and scales nothing here.
    let collection = workers as f32 * per_worker_biomass_capacity;
    // **The party's REACH, in animals** — how many it can bring into contact each projected turn
    // (`docs/plan_hunt_through_combat.md` §2). Constant for the run: the crew does not change size
    // mid-projection and the quarry is never re-speciated. What the herd can *spare* is not constant,
    // so the escapement clamp and the retreat that follows it live inside the loop below. A **pen**
    // has no engagement stage at all (a penned animal is not stalked), so it is unbounded there — the
    // same exemption `project_arrivals_hunt` states by passing `f32::INFINITY` to the quantiser on
    // its corral branch.
    let reach = animals_engaged(workers, fauna.engage_rate_for(&quarry.species));
    let wariness = fauna.wariness_for(&quarry.species);
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
        // what the party can reach, and by the standing stock. **Every rung pays the stock standing
        // above its stance's floor, at the CURRENT biomass** — what `hunt_take` reads. The pen's
        // separate managed arm is retired: a rung may change production, no rung changes the draw.
        // Unquantised, but **not** unbounded: see the doc above for why the engagement cap belongs
        // here and the rounding does not.
        // **THE SAME BOUND THE TAKE USES** ([`hunt_take_room`]) — `regrow_biomass` above has just
        // stamped this turn's growth, so the backstop's term is a measurement here exactly as it is
        // in the live arm. A forecast on the pure escapement room would quote `0` for every turn a
        // herd spends below its own climbing floor while the sim pays the growth share.
        let rate = hunt_take_room(floor, quarry.biomass, capacity, quarry.growth_this_turn());
        // Dropping the *quantiser* here is sound because rounding is a timing effect; dropping the
        // fight would not be, for exactly the reason the engagement bound belongs here — a
        // bare-handed party brings down **nothing** from a mammoth herd however much room stands
        // above the floor, and a `realized` that ignored that would quote a steady food rate the
        // party can never collect.
        let engagement_biomass = {
            // **Engagement, then the retreat's EXPECTATION, then the fight** — the take's three
            // stages in the take's order. The reach is clamped by what the herd can spare *before*
            // the retreat ([`animals_affordable`]), because the retreat keeps a fraction of whatever
            // it is handed: clamping afterwards would retreat a bigger party than the take does and
            // over-quote every turn the escapement room binds. A projection cannot *draw* the
            // retreat the take will draw (see [`HuntDraw`]), so it reads the same binomial's mean.
            let engaged = party.stayers(
                reach.min(animals_affordable(rate, quarry.body_mass)),
                wariness,
                HuntDraw::EXPECTED,
            );
            let fight = resolve_hunt_fight(
                engaged,
                workers as f32,
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
    per_worker_biomass_capacity: f32,
    // The party doing the hunting — the schedule runs the same fight the take does.
    party: &HuntingParty,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
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
    // The party's **reach** — constant for the run, since the crew does not change size
    // mid-projection and the quarry is never re-speciated. The escapement clamp and the retreat that
    // follows it are not constant, so they sit in the loop below — see the twin in
    // `project_realized_hunt`.
    // **`workers` IS THE TAKE CREW** (`docs/plan_standing_upkeep.md` §2.2) — the same term
    // `systems::hunt_take` is paid at, so the projection and the take stay one model. A build on
    // this herd is its own allocation with its own hands and scales nothing here.
    let reach = animals_engaged(workers, fauna.engage_rate_for(&quarry.species));
    let wariness = fauna.wariness_for(&quarry.species);
    // **The fight is resolved PER TURN, not once for the run**, because its wounds accumulate
    // (§4.2): a sub-threshold party lands nothing for several turns and then a whole animal, and that
    // pulse is exactly what this schedule exists to draw. Only the quarry's *body* is constant.
    // Inert to the seed at the shipped `hit_chance` (see [`FORECAST_FIGHT_SEED`]).
    let mut quarry_fight = herd_quarry_fight(&quarry, fauna);
    let _corralled = quarry.is_corralled();
    let collection = workers as f32 * per_worker_biomass_capacity;
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
        // **ONE PATH AT EVERY RUNG.** The pen's separate managed arm — no bank, no policy axis, no
        // engagement bound — is retired with the model: a rung may change production, no rung changes
        // the draw.
        let carried = {
            // A wild/pastoral herd hands over the stock standing above its stance's floor, rounded to
            // whole animals — the `systems::hunt_take` sequence, helper for helper.
            let ceiling =
                hunt_take_room(floor, quarry.biomass, capacity, quarry.growth_this_turn());
            // Engagement clamped by what the herd can spare, **then** the retreat's expectation,
            // then the fight — the take's order, because the retreat keeps a fraction of whatever
            // it is handed and clamping after it would quote a take off a bigger party than the
            // one the sim sends ([`animals_affordable`]).
            let engaged = party.stayers(
                reach.min(animals_affordable(ceiling, quarry.body_mass)),
                wariness,
                HuntDraw::EXPECTED,
            );
            let fight = resolve_hunt_fight(
                engaged,
                workers as f32,
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
    draw: HuntDraw,
) -> (YieldAccounts, YieldAccounts) {
    // **`workers` IS THE TAKE CREW, and it is the only crew here** (`docs/plan_standing_upkeep.md`
    // §2.2): a build on this same source is its own allocation with its own hands, so a verb in
    // flight scales nothing about what these hunters carry. The `improvement` axis survives on this
    // signature because the *rung* still decides which ceiling the source offers, not because it
    // prices the crew.
    let collection = forecast.per_worker_yield.scale(workers as f32);
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
            let engaged = animals_engaged(workers, forecast.engage_rate)
                // **Restraint is free, and the forecast has to say so too** — the escapement floor
                // bounds what the party *goes after*, exactly as `systems::hunt_take` bounds it
                // (`docs/plan_hunt_through_combat.md` §1). Clamping here rather than leaving it to the
                // quantiser is what keeps the retreat below running on the same population the take
                // retreats; see [`animals_affordable`]. Both terms are read on `axis`, and an animal
                // count is a ratio, so the currency cancels.
                .min(animals_affordable(
                    ceiling.component(axis),
                    quantum.component(axis),
                ));
            // **Engagement, then retreat, then the fight** — the same three stages in the same order
            // `systems::hunt_take` runs (`docs/plan_hunt_through_combat.md` §1), through the same
            // helpers. The forecast cannot *draw* the retreat or the attack rolls, so it reads them
            // at `draw`'s quantile instead of guessing a seed (see [`HuntDraw`]); the wariness is the
            // quarry's own, off the fight the forecast already carries.
            //
            // **No fight stage means no retreat stage either** — a pen and the plant web, whose
            // `engage_rate` is already `f32::INFINITY`.
            let brought_down = match &forecast.fight {
                Some((party, quarry)) => {
                    let stayed = party.stayers(engaged, quarry.profile.wariness, draw);
                    resolve_hunt_fight(stayed, workers as f32, party, quarry, draw).brought_down
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
/// - `workers_needed` = **the TAKE activity's own crew** — hands to haul the offer, the expected
///   take inverted by the per-worker throughput (a ratio, so provisions-space matches the resolution
///   path's biomass-space result). It was `source_crew_needed(standing, take)`, a `max` blending a
///   herding headcount with a hauling one and a build's staffing floor, because one crew did every
///   job on a source and the row had one number to give. **Each activity is staffed on its own row
///   now** — the take on the source, the keeping and the building on the band
///   (`docs/plan_standing_upkeep.md` §2.5) — so each answers in its own unit,
/// - `overdraws` = whether this policy draws the stock below what it sustains — the ⚠
///   ([`SourceYield`]). **The caller hands it in already answered**, because the answer needs the
///   source's own growth curve and the two webs' curves are different functions: the animal web
///   answers through [`hunt_take_overdraws`], the plant web through
///   `forage::forage_take_overdraws`, and both of those are one call to
///   [`crate::components::take_overdraws`]. This function only applies the `managed` veto below.
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
    workers: u32,
    floor: f32,
    realized: f32,
    arrivals: Vec<f32>,
    // **THE ⚠, already answered** by the source's own web through
    // [`crate::components::take_overdraws`] — intent *and* the crew's ability to get there. This
    // function cannot re-derive it: the ability half reads a growth curve, and there are two of them.
    overdraws: bool,
    // How wide a band to report around the expected take (`combat_config.forecast_range_sigmas`) —
    // a **readout width**: nothing the sim resolves reads it, so it cannot move an animal.
    range_sigmas: f32,
) -> SourceYield {
    // **The row's scalars are the range's MIDDLE.** A telemetry row states one figure; the
    // distribution it sits in rides beside it as `range` (`docs/plan_hunt_through_combat.md` §6.4),
    // and on the shipped roster the three readings are the same number bit-for-bit.
    let range = forecast_take_range(forecast, workers, floor, range_sigmas);
    let (production, actual) =
        forecast_production_and_take_at(forecast, workers, floor, HuntDraw::EXPECTED);
    // What ONE worker on this assignment moves — the whole `per_worker_yield`, because the take
    // crew is the only crew in the take. It was scaled by the retired build dip (and then by the
    // retired work budget's share); with the build staffed in its own right there is nothing left to
    // scale it by.
    SourceYield {
        actual: actual.provisions,
        // **The FEED currency, taken off the same take vector** (issue #449) — never a second
        // derivation. It is `0` today on both webs and for two different reasons: no animal pays
        // fodder at all, and the plant web's forecast is deliberately food-only
        // (`forage::plant_food_only`, the gap `PLANT_FODDER_FORECAST_NOT_YET_PROJECTED` names),
        // so a pre-commit row quotes no fodder until that projection lands. Reading the component
        // rather than writing a literal means it starts telling the truth the moment it does.
        fodder: actual.fodder,
        // **A pre-commit row quotes NO material, and that is a stated gap rather than a claim of
        // zero** (arc #527). Projecting materials needs the take in *biomass* — `credit_material_yield`
        // is paid off `take.carried`, and this path resolves the take in currency space, where an
        // inedible species has no positive axis to count on. The **resolved** row does carry it
        // (`systems::labor` hands over exactly what the credit deposited), and the number a player
        // decides on rides the herd row's own `material_per_biomass` / `per_worker_material`, which
        // are rates and need no take at all.
        materials: Vec::new(),
        // The band `actual` sits in the middle of. Built from the SAME
        // `forecast_production_and_take_at`, three quantiles apart, so `low <= actual <= high` is a
        // property of the arithmetic rather than a clamp.
        range: YieldRange {
            low: range.low.provisions,
            high: range.high.provisions,
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
        wasted: (production.provisions - actual.provisions).max(0.0),
        // **`workers_needed` IS THE TAKE ACTIVITY'S OWN COUNT — hands to haul the offer**
        // (`docs/plan_standing_upkeep.md` §2.2). It used to be `source_crew_needed(standing, take)`,
        // a `max` that blended a herding headcount, a hauling one and a build's staffing floor into
        // one number — because one crew did every job on a source and the row had one number to
        // give. **Each activity is staffed on its own row now** (`docs/plan_standing_upkeep.md`
        // §2.5), so each answers in its own unit: this one in haulers, the keeping's in
        // `upkeepWorkersNeeded`, and the building's on the band's own `builders` row. A `max` across
        // units was always the compromise a single allocation forced.
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
        workers_needed: match forecast.ratio_axis() {
            Some(axis) if forecast.quantises() => hunt_take_workers(
                forecast.ceiling_at(floor).component(axis),
                forecast.body_mass_yield.component(axis),
                forecast.per_worker_yield.component(axis),
                // **The engagement term is the third unit** — `hunt_engage_workers` reads it exactly
                // as `animals_engaged` does, so the crew inverts the bound the take was actually
                // paid. A pen forecasts `f32::INFINITY` and contributes no engagement crew at all.
                forecast.engage_rate,
                // **The retreat, off THIS forecast's own party and quarry** — the same
                // `stay_fraction` the take above was priced with, so the crew and the take can
                // never be resolved at two different dispersions.
                forecast
                    .fight
                    .as_ref()
                    .map_or(NO_RETREAT_STAGE_STAY, |(party, quarry)| {
                        party.stay_fraction(quarry.profile.wariness)
                    }),
            ),
            Some(axis) => workers_needed_for_take(
                actual.component(axis),
                forecast.per_worker_yield.component(axis),
                workers,
            ),
            // A source that yields nothing in either currency asks for no haulers. What it costs to
            // KEEP is a different question, answered per activity on the source's own row.
            None => NO_CREW_ON_THIS_ACTIVITY,
        },
        // A **managed** source (rung 3 — a Field, a pen) takes at most its escapement MSY, so it
        // cannot overdraw whatever the dial and the crew say.
        overdraws: !managed && overdraws,
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
    per_worker_biomass_capacity: f32,
    // The band's own party — kit and tuning — so the seed resolves the fight the turn will.
    party: &HuntingParty,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    realized_horizon: u32,
    arrivals_horizon: u32,
    // `combat_config.forecast_range_sigmas` — how wide a band the seeded row reports around its
    // expected take (`docs/plan_hunt_through_combat.md` §6.4). Last, matching the forage twin.
    range_sigmas: f32,
) -> SourceYield {
    let forecast = hunt_forecast(
        herd,
        fauna,
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
        per_worker_biomass_capacity,
        party,
        output_multiplier,
        workers,
        floor,
        realized_horizon,
    );
    // The discrete twin, from the same herd state: when each of the next `arrivals_horizon` deliveries
    // lands, bank and all.
    let arrivals = project_arrivals_hunt(
        herd,
        fauna,
        per_worker_biomass_capacity,
        party,
        output_multiplier,
        workers,
        floor,
        arrivals_horizon,
    );
    // **The herder term no longer rides `workers_needed`** — a keeping crew and a hauling crew are
    // different jobs in different units, and each activity states its own count now
    // (`docs/plan_standing_upkeep.md` §2.2). `herders_needed` keeps its own wire field
    // (`HerdTelemetryState::herdersNeeded`, with the ownership-independent
    // `herdersNeededIfManaged` beside it) and is untouched by this row.
    forecast_source_yield(
        &forecast,
        sustainable,
        herd.is_corralled(),
        workers,
        floor,
        realized.provisions,
        arrivals,
        // The ⚠, off this herd's own curve and this party's own reach — see [`hunt_take_overdraws`].
        hunt_take_overdraws(
            herd,
            fauna,
            herd.biomass,
            per_worker_biomass_capacity,
            party,
            workers,
            floor,
        ),
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

/// **WHAT A HUNTING PARTY MAY ACTUALLY TAKE FROM THIS HERD THIS TURN** — the animal web's
/// [`take_room`]: the escapement room, or the share of the turn's growth this floor leaves takeable,
/// whichever is larger.
///
/// # ⛔ THIS IS THE TAKE'S BOUND *AND* THE BUILD'S GATE — [`hunt_escapement_ceiling`] IS NEITHER
///
/// The two are deliberately separate functions rather than one with a flag, because they answer
/// different questions and exactly one of them may be widened:
/// - **This one** bounds `hunt_take` and answers *"is there anything here to work with"* for the
///   `Tame` build's eligibility. A herd pushed below its own floor by the `K` its taming raised is
///   still a herd, and it is still growing.
/// - **[`hunt_escapement_ceiling`]** stays the pure escapement room, and is what the **lesson** is
///   gated on. `intensification::learn_multiplier`'s doc makes that load-bearing: *"watching teaches
///   nothing"* at `floor = 1.0` is the self-limit that stops a near-`1.0` floor farming knowledge at
///   ×2 for free, and it self-limits **precisely because** the source must stand above the floor for
///   a take to exist. Widening the lesson's gate would open that; widening the build's does not
///   touch it.
pub fn hunt_take_room(floor: f32, biomass: f32, carrying_capacity: f32, growth: f32) -> f32 {
    take_room(floor, biomass, carrying_capacity, growth)
}

/// [`hunt_take_room`] for a herd, reading its own realized growth and its own resolved capacity —
/// the form every caller that holds a `&Herd` should use, so none of them re-derives either term.
pub fn herd_take_room(herd: &Herd, floor: f32, fauna: &FaunaConfig) -> f32 {
    hunt_take_room(
        floor,
        herd.biomass,
        herd_capacity(herd, fauna),
        herd.growth_this_turn(),
    )
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
/// # `workers` IS THE TAKE CREW — no build term, and no dip
///
/// It took a `build_dip` factor while one crew did every job on a source, on the reasoning that
/// hands spent gentling a herd are hands not hunting it. **The player allocates the two crews
/// separately now** (`docs/plan_standing_upkeep.md` §2.2), so the hunters here are only ever
/// hunters and the reach is simply theirs. The defect that factor guarded against
/// (`docs/plan_harvest_floor.md` §0.3, *"the harshest stance builds free"*) cannot recur, because
/// there is no shared crew for a build to ride for free.
pub fn animals_engaged(workers: u32, engage_rate: f32) -> f32 {
    if workers == 0 {
        return 0.0;
    }
    (workers as f32 * engage_rate.max(0.0)).floor().max(1.0)
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

/// **The retreat's one composition** — `clamp(wariness × dispersion, 0, 1)`, the probability an
/// engaged animal breaks off, with the kit's noise folded into the quarry's own flight response
/// ([`crate::equipment_config::EquipmentStat::Dispersion`]).
///
/// **Clamped into `0..=1` because both factors are authored**: a species may ship `wariness 0.85`
/// and a future kit a dispersion above `1.0` (a noisy drive), and a probability above one is not a
/// probability. `0` is the identity the retreat has always had — no draw is made at all.
///
/// Stated once so the drawn form ([`HuntingParty::stayers`]) and the closed form
/// ([`stay_fraction`], which [`per_hunter_take_biomass`] prices a kit with) cannot disagree about
/// how loud a party is.
pub fn effective_wariness(wariness: f32, dispersion: f32) -> f32 {
    (wariness * dispersion.max(0.0)).clamp(0.0, 1.0)
}

/// **The share of an engagement that stays**, `1 − effective_wariness` — the retreat as a *term*
/// rather than as a draw, for the closed forms that price a party without resolving a hunt
/// ([`per_hunter_take_biomass`], and the wire's `HerdTelemetryState::stay_fraction`, which is this
/// at the neutral `dispersion 1`).
///
/// # It prices the TAKE and the CREW, on the same rate
///
/// A hunter's real throughput is `engage_rate × stay` — what they reach times what stands — so this
/// term divides into a crew count ([`hunt_engage_workers`]) exactly as it multiplies into a biomass
/// rate ([`per_hunter_take_biomass`]). **A party that keeps one animal in
/// four needs four times the hands to draw the same stock down.** Any surface that sizes a hunting
/// crew reads it here, through the party's own [`HuntingParty::stay_fraction`], so a crew and the
/// take beside it cannot be resolved at different dispersions.
pub fn stay_fraction(wariness: f32, dispersion: f32) -> f32 {
    1.0 - effective_wariness(wariness, dispersion)
}

/// **How many of the engaged animals stay to be fought** — the retreat stage
/// (`docs/plan_hunt_through_combat.md` §3), between engagement and the fight.
///
/// Each engaged animal independently breaks off with probability `wariness`. **Escaped animals are
/// not dead**, so the herd loses nothing for them: a wary herd costs the party *hunter-turns*, never
/// herd biomass, and that pressure falls out with no extra rule.
///
/// Those hunter-turns are **priced**, not merely implied: the closed form of this draw
/// ([`stay_fraction`]) is a factor in the per-hunter rate, so a wary quarry raises the crew a
/// `workers_needed` names ([`hunt_engage_workers`]) by exactly the reciprocal of what stands.
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

/// **[`animals_that_stay`] ON A FRACTIONAL ENGAGEMENT** — the retreat as a rate
/// ([`EngagementQuantum::Rate`]), with the whole-animal floor removed.
///
/// The binomial's mean and standard deviation, `n·p` and `√(n·p·(1−p))`, are defined for a
/// fractional `n` and are the continuous extension of the same distribution — so this is the *same*
/// reading [`animals_that_stay`] takes, asked of an engagement that has not been rounded to bodies.
/// **`n` here is a rate, not a count**, which is the whole of why the floor is wrong for it: a party
/// engaging four fifths of an animal a turn keeps `0.8 × stay` of one a turn.
///
/// **A [`HuntDraw::Seeded`] draw falls back to the whole-animal form**, and that is not a shortcut:
/// a live take resolves *bodies* — you cannot roll a fraction of an animal breaking off — so a rate
/// has nothing to draw. No caller reaches it (only the curve asks for a rate, and a curve never
/// draws); the arm exists so the function is total rather than panicking on an unreachable state.
///
/// The `wariness <= 0` and non-finite guards are [`animals_that_stay`]'s, unchanged: a calm quarry
/// and a source with no engagement stage at all answer their exact identities on both forms.
pub fn animals_that_stay_at_rate(engaged: f32, wariness: f32, draw: HuntDraw) -> f32 {
    if wariness <= 0.0 || !engaged.is_finite() || engaged <= 0.0 {
        return engaged;
    }
    match draw {
        HuntDraw::Seeded(_) => animals_that_stay(engaged, wariness, draw),
        HuntDraw::Quantile { sigmas } => {
            let stay_chance = 1.0 - wariness.min(1.0);
            let mean = engaged * stay_chance;
            let deviation = (engaged * stay_chance * (1.0 - stay_chance)).sqrt();
            (mean + sigmas * deviation).clamp(0.0, engaged)
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

    /// The stream seed a live fight draws from — **the event's seed salted apart from the retreat's**
    /// ([`FIGHT_SEED_SALT`]). A forecast makes no draw, so it hands over [`FORECAST_FIGHT_SEED`] and
    /// nothing reads it.
    ///
    /// **The salt is what makes the hunt's two stages independent.** Both stages reseed a fresh
    /// `SmallRng` from a `u64` rather than sharing one advancing stream, so handing the retreat's seed
    /// straight to the fight would replay the same uniforms in the same order and lock *"animal k
    /// stayed"* to *"hunter k landed"*. See [`FIGHT_SEED_SALT`] for why that matters the moment
    /// `hit_chance` drops below `1.0`.
    pub fn seed(self) -> u64 {
        match self {
            HuntDraw::Seeded(seed) => seed ^ FIGHT_SEED_SALT,
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
/// **A partly-equipped party is SEVERAL parties**, which is why the profile is a list: see
/// [`HuntCrew`], and [`crate::equipment_config::EquipmentConfig::coverage`] for how the gear divides
/// the people.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntingParty {
    /// **The runs of hunters holding the same gear**, best-equipped first, and `Σ share == 1`.
    /// Never empty for a party that exists — a wholly bare party is one crew at the intrinsic tier.
    pub crews: Vec<HuntCrew>,
    /// The resolver severity dials this party fights at. An expedition passes the
    /// `expedition_danger_multiplier`-scaled lethality; a resident band the base tuning.
    pub tuning: CombatTuning,
    /// **Multiplies the quarry's own `wariness` at the retreat**
    /// ([`crate::equipment_config::EquipmentStat::Dispersion`]), neutral at `1.0`.
    ///
    /// A multiplier rather than a subtraction so the **species** decides how much a noisy approach
    /// costs: at `wariness 0.85` a gazelle loses almost everything to one, at `0.10` a mammoth barely
    /// notices. That is what lets a single spear line scatter a warren and contain a mammoth with no
    /// per-target authoring — and why a targets/size-class axis on equipment was not needed.
    ///
    /// `0` (a trap) means **nothing breaks off**, which is the `wariness 0` identity the retreat has
    /// always had rather than a new branch.
    ///
    /// # It is PARTY-WIDE, and it is the MAX across the crews
    ///
    /// The same clause [`crate::equipment_config::KitChoice::multiplier`] resolves an item's
    /// multipliers with, one level up: if part of your party is running up and throwing spears, the
    /// herd is scared — the trap line the other half set does not un-scare it. So a party that could
    /// only arm half its people with the stand-off device is *loud*, and buying the quiet approach
    /// means buying enough of it.
    pub dispersion: f32,
}

/// **One run of hunters holding the same gear** — the party's own
/// [`crate::equipment_config::Crew`], resolved into the two numbers a fight reads.
///
/// **`share` is a FRACTION of the party, never a head count**, which is what keeps
/// [`HuntingParty`] scale-free exactly as the single-profile struct was: every caller still passes
/// its own `hunters` count, and a sub-party that engages (`hunt_engage_workers` returning fewer than
/// were assigned) carries the same mix rather than an arbitrary prefix of it.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntCrew {
    /// This run's share of the party, in `0..=1`. The crews sum to `1`.
    pub share: f32,
    /// **What this run is holding** — the party's kit narrowed to this crew
    /// ([`crate::equipment_config::Crew::kit`]), so the fight can charge the weapon that swung
    /// without a wear site having to pair the outcome back up with the coverage that produced it.
    ///
    /// **`None` for a party built through [`HuntingParty::uniform`]** — a fixture, a reference quote
    /// or a launch forecast, none of which owns a ledger to charge. It is an absent *charge*, not an
    /// empty kit: an empty kit would be a claim that the crew holds nothing.
    pub kit: Option<crate::equipment_config::KitChoice>,
    /// One hunter's combat profile in this run — intrinsic ⊕ what *this* run is holding. `attack 1`
    /// bare-handed, `20` speared.
    pub hunter: CombatStats,
    /// **The hunt's own hazard, per animal engaged**
    /// ([`crate::combat_config::CombatConfig::hunt_injury_damage_per_animal`]) — damage the *activity*
    /// does to the party whatever the quarry swings (§4.6).
    ///
    /// Hunters fall, break bones, are trampled in a drive, cut themselves butchering. Without it only
    /// mammoth, aurochs and wolf could hurt anyone on the shipped roster and a boar cost nothing,
    /// contradicting §4.2's own *"survives by ferocity alone — frail, still costs you people"*.
    ///
    /// It rides the **crew** rather than the quarry because the danger is in the activity, not in
    /// the rabbit; it scales with the *engagement* at the point of use, so more animals worked means
    /// more chances to get hurt. **Per crew, because `exposure` is a kit's** — the half of the party
    /// that got the stand-off device is the half that does not get hurt.
    pub injury_damage_per_animal: f32,
}

/// **Everything a live party is resolved from, minus the quarry** — bundled because the six travel
/// together through every take and forecast path, and because the quarry is the one axis that has to
/// stay a *late* argument: a mass-bounded weapon is only a weapon against animals it can hold, so the
/// attack tier cannot be resolved before the target is known.
///
/// That is the `party_for = |body_mass| …` factory idiom `advance_labor_allocation` and
/// `advance_expeditions` already used, given a name so the coverage behind it is resolved **once**
/// per party per turn rather than once per quarry.
pub struct PartyResolution<'a> {
    /// The item table every tier is resolved through.
    pub equipment: &'a crate::equipment_config::EquipmentConfig,
    /// **How this party's gear divides its people** — resolved once, off the party's own kit, head
    /// count and ledger ([`crate::equipment_config::EquipmentConfig::coverage`]).
    pub coverage: &'a crate::equipment_config::KitCoverage,
    /// The band's live ledger — what each crew's kit is masked against.
    pub wear: &'a crate::components::BandEquipment,
    /// The `person` roster row: what a hunter is before any gear.
    pub intrinsic: CombatStats,
    /// The resolver severity dials — see [`HuntingParty::tuning`].
    pub tuning: CombatTuning,
    /// [`crate::combat_config::CombatConfig::hunt_injury_damage_per_animal`], **before** a crew's own
    /// `exposure` scales it.
    pub hunt_injury_damage_per_animal: f32,
}

impl PartyResolution<'_> {
    /// **The party as it fights THIS quarry** — one [`HuntCrew`] per covered run.
    ///
    /// The `quarry` argument is the two named resolvers' own distinction, kept visible here rather
    /// than folded away: a **take or forecast** passes [`crate::equipment_config::Quarry::Mass`], a
    /// **display** surface with no target passes `Quarry::Any`. Handing `Any` to a take would give a
    /// trapping party its small-game attack against a mammoth, which is the bug the bound exists to
    /// prevent.
    pub fn party_against(&self, quarry: crate::equipment_config::Quarry) -> HuntingParty {
        let crews: Vec<HuntCrew> = self
            .coverage
            .crews()
            .iter()
            .map(|crew| HuntCrew {
                share: crew.workers / self.total_workers(),
                kit: Some(crew.kit.clone()),
                hunter: match quarry {
                    crate::equipment_config::Quarry::Mass(body_mass) => self
                        .equipment
                        .hunter_profile_against(self.intrinsic, &crew.kit, self.wear, body_mass),
                    crate::equipment_config::Quarry::Any => self
                        .equipment
                        .hunter_profile_unbounded(self.intrinsic, &crew.kit, self.wear),
                },
                injury_damage_per_animal: self.hunt_injury_damage_per_animal
                    * self.equipment.exposure(&crew.kit, self.wear),
            })
            .collect();
        // **A party with nobody in it is still ONE crew**, holding the kit it was sent with — the
        // same answer this seam gave before coverage existed, and what keeps every consumer free of
        // an "empty party" branch.
        if crews.is_empty() {
            return HuntingParty::uniform(
                match quarry {
                    crate::equipment_config::Quarry::Mass(body_mass) => {
                        self.equipment.hunter_profile_against(
                            self.intrinsic,
                            self.coverage.kit(),
                            self.wear,
                            body_mass,
                        )
                    }
                    crate::equipment_config::Quarry::Any => self
                        .equipment
                        .hunter_profile_unbounded(self.intrinsic, self.coverage.kit(), self.wear),
                },
                self.tuning,
                self.hunt_injury_damage_per_animal
                    * self.equipment.exposure(self.coverage.kit(), self.wear),
                self.equipment.dispersion(self.coverage.kit(), self.wear),
            );
        }
        HuntingParty {
            crews,
            tuning: self.tuning,
            // **The MAX across the crews** — see [`HuntingParty::dispersion`]. Resolved from each
            // crew's own kit rather than the party's, so a party that could not arm everybody with
            // the quiet instrument is priced at the loud one.
            dispersion: self
                .coverage
                .crews()
                .iter()
                .map(|crew| self.equipment.dispersion(&crew.kit, self.wear))
                .fold(f32::NEG_INFINITY, f32::max),
        }
    }

    /// The head count the shares are taken against — the coverage's own
    /// ([`crate::equipment_config::KitCoverage::workers`]), not a sum of the crews, so the shares
    /// divide by the very number the partition was cut from. Positive whenever there is a crew,
    /// because `coverage` never emits one for a party of nobody.
    fn total_workers(&self) -> f32 {
        self.coverage.workers()
    }
}

impl HuntingParty {
    /// **A party where everybody is holding the same thing** — one crew at `share 1.0`.
    ///
    /// The shape every party had before the partly-equipped one existed, and what a fixture, a
    /// reference quote or a fully-covered band still resolves to.
    pub fn uniform(
        hunter: CombatStats,
        tuning: CombatTuning,
        injury_damage_per_animal: f32,
        dispersion: f32,
    ) -> Self {
        Self {
            crews: vec![HuntCrew {
                share: 1.0,
                kit: None,
                hunter,
                injury_damage_per_animal,
            }],
            tuning,
            dispersion,
        }
    }

    /// **The best-equipped crew's hunter** — what a party *can* do at its best, for a readout with
    /// no room for a mix and for a test naming the tier it composed.
    ///
    /// **Not a take seam.** A take resolves every crew (`resolve_hunt_fight`), because the whole
    /// point of the mix is that the rest of the party is not this.
    pub fn best_equipped_hunter(&self) -> CombatStats {
        self.crews
            .first()
            .map(|crew| crew.hunter)
            .unwrap_or_default()
    }

    /// **How many of the animals it reached stay to be fought** — [`animals_that_stay`] with the
    /// kit's `dispersion` applied to the quarry's own `wariness`.
    ///
    /// The product is **clamped into `0..=1`** because both factors are authored: a species may ship
    /// `wariness 0.85` and a future kit a dispersion above `1.0` (a noisy drive), and a probability
    /// above one is not a probability. `0` is the identity the retreat has always had — no draw is
    /// made at all — so a trap line lands in a tested regime rather than a new branch.
    pub fn stayers(&self, engaged: f32, wariness: f32, draw: HuntDraw) -> f32 {
        animals_that_stay(engaged, effective_wariness(wariness, self.dispersion), draw)
    }

    /// **[`stayers`](Self::stayers) WITHOUT THE WHOLE-ANIMAL FLOOR** — the retreat applied to a
    /// fractional engagement, for [`EngagementQuantum::Rate`].
    ///
    /// **The retreat re-floors what the room clamp already floored, one stage later**, and that is
    /// why un-flooring [`animals_sparable`] alone changes nothing: `animals_that_stay` opens with
    /// `let stayers = engaged.floor()`, so an engagement of `0.45` is handed to the binomial as `0`
    /// and the whole curve is zero however the room was measured. Both floors have to go, or neither
    /// does.
    pub fn stayers_at_rate(&self, engaged: f32, wariness: f32, draw: HuntDraw) -> f32 {
        animals_that_stay_at_rate(engaged, effective_wariness(wariness, self.dispersion), draw)
    }

    /// **The closed form of [`stayers`](Self::stayers)** — the share of an engagement this party
    /// keeps against a quarry of `wariness`, with its kit's `dispersion` folded in.
    ///
    /// The one seam every *rate* reads the retreat through: [`per_hunter_take_biomass`] prices a kit
    /// with it and [`hunt_engage_workers`] sizes a crew with it, so a compose sheet cannot quote a
    /// take at one dispersion beside a crew at another.
    pub fn stay_fraction(&self, wariness: f32) -> f32 {
        stay_fraction(wariness, self.dispersion)
    }

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
        // The **hunt job's default kit** against a band with no wear — "an ordinary band", resolved
        // through the same seams a live one uses rather than by asserting which items that kit holds.
        let kit = equipment.default_kit(crate::equipment_config::KitJob::Hunt);
        let fresh = crate::components::BandEquipment::start_stocked(&equipment);
        Self::uniform(
            equipment.hunter_profile_unbounded(
                crate::creatures_config::CreaturesConfig::builtin().person(),
                &kit,
                &fresh,
            ),
            combat.tuning(),
            combat.hunt_injury_damage_per_animal * equipment.exposure(&kit, &fresh),
            equipment.dispersion(&kit, &fresh),
        )
    }

    /// The same party **with its spears gone** — the unequipped tier, which is the `person` row's
    /// intrinsic `attack 1`. The other side of §4.8's cliff, and what a test asserting the gate wants.
    pub fn builtin_unequipped() -> Self {
        let combat = crate::combat_config::CombatConfig::builtin();
        // **The empty kit, not a hand-built profile.** Every unequipped tier and every neutral
        // multiplier then comes from the same resolution a live bare-handed party runs, so this
        // fixture cannot drift from the game if an item's unequipped side is retuned.
        let equipment = crate::equipment_config::EquipmentConfig::builtin();
        let kit = equipment.no_kit();
        let fresh = crate::components::BandEquipment::start_stocked(&equipment);
        Self::uniform(
            crate::creatures_config::CreaturesConfig::builtin().person(),
            combat.tuning(),
            combat.hunt_injury_damage_per_animal * equipment.exposure(&kit, &fresh),
            equipment.dispersion(&kit, &fresh),
        )
    }
}

/// **§4.6's per-hunter-turn take, in biomass** — what one hunter carrying `kit` brings down off this
/// species in one turn:
///
/// ```text
/// min(engage_rate, (attack − defense) / durability) × (1 − clamp(wariness × dispersion, 0, 1)) × body_mass
/// ```
///
/// The three bounds a hunt actually has, in order: what one hunter **reaches**, what one hunter can
/// **bring down**, and what **stays** to be brought down. Composed from the resolver's own
/// primitives ([`combat::strike_damage`], [`combat::units_brought_down`]) and the retreat's own
/// helper ([`HuntingParty::stay_fraction`]) rather than re-spelled, so it cannot become a second
/// take model: the same `min(reach, damage/durability)` the fight pays, read as a rate.
///
/// **It sees no carry tier, no crew size, no escapement floor and no quantiser** — it is the
/// *ceiling*, which is exactly what makes it a fair comparison between two kits against one quarry:
/// every term it drops is a property of the band or the herd rather than of the kit.
///
/// `0` for a quarry the kit cannot hurt at all (`attack ≤ defense`) — the gate refusing the hunt,
/// which is what makes a trapping party score zero on a Red Deer.
///
/// **It takes the two terms a KIT moves** — the kit-composed hunter profile and the kit's
/// `dispersion` — rather than a whole [`HuntingParty`], because the party's other two fields
/// (resolver tuning, the injury hazard) describe what the hunt *costs*, not what it *takes*, and
/// requiring them would make every caller hold a [`crate::combat_config::CombatConfig`] to price a
/// comparison neither field enters.
pub fn per_hunter_take_biomass(hunter: CombatStats, dispersion: f32, species: &SpeciesDef) -> f32 {
    let quarry = species.combat;
    let damage = combat::strike_damage(hunter.attack, quarry.defense);
    let brought_down = combat::units_brought_down(damage, &quarry, species.engage_rate);
    brought_down * stay_fraction(quarry.wariness, dispersion) * species.body_mass.max(0.0)
}

/// **Which kit THIS QUARRY wants** — the hunt roster scored against one species, at the **fresh**
/// tier, returning the kit a compose sheet should open on and the one `assign_labor … hunt` resolves
/// when the player names none.
///
/// # Derived, never authored
///
/// A config predicate (`mass < X && wariness > Y`) would be a **third** copy of facts that already
/// exist twice — the trap declares `max_body_mass 1.0` and `dispersion 0`, the species declares
/// `body_mass`, `wariness` and `defense` — and would drift from both on the first retune, exactly as
/// a `size_class` or a "jumpy" flag would. It would also be silently wrong on `defense`, which is
/// what actually zeroes a trap party on Marsh Grazers. So the answer is *measured* with
/// [`per_hunter_take_biomass`], through the same `hunter_profile_against` / `dispersion` seams a
/// live take resolves.
///
/// # Wear does NOT enter
///
/// Every kit is scored against [`crate::components::BandEquipment::start_stocked`] — one unworn
/// unit of everything — so a
/// herd's default is a property of **quarry × roster**, a per-world constant, and cannot reshuffle
/// under the player as their spears wear down. The same rule the picker's greying follows.
///
/// # A near-tie keeps the job default
///
/// The winner replaces [`crate::equipment_config::DefaultKitsConfig::hunt`] only when it scores more
/// than `(1 + quarry_default_kit_margin) ×` the default's own score. A default that flips on a
/// trivial retune moves under the player for reasons they cannot see. **A default that scores `0`
/// is beaten by anything positive** — "better than nothing" needs no margin, and a margin cannot be
/// expressed against zero anyway.
///
/// Ties resolve to the **earliest roster entry** (the fold keeps only a strictly greater score), so
/// two kits that price identically answer by file order rather than by iteration order.
pub fn quarry_default_hunt_kit(
    equipment: &crate::equipment_config::EquipmentConfig,
    person: CombatStats,
    species: &SpeciesDef,
) -> crate::equipment_config::KitChoice {
    let job = crate::equipment_config::KitJob::Hunt;
    let fresh = crate::components::BandEquipment::start_stocked(equipment);
    let score = |kit: &crate::equipment_config::KitChoice| {
        per_hunter_take_biomass(
            // **Against THIS animal**, so a mass-bounded weapon is scored on quarry it can actually
            // hold — the whole reason a bounded kit may win here and may not be the job default.
            equipment.hunter_profile_against(person, kit, &fresh, species.body_mass),
            equipment.dispersion(kit, &fresh),
            species,
        )
    };
    let default = equipment.default_kit(job);
    let threshold = score(&default) * (1.0 + equipment.quarry_default_kit_margin.max(0.0));
    let mut best: Option<(crate::equipment_config::KitChoice, f32)> = None;
    for kit in equipment.kits_for_job(job) {
        let value = score(&kit);
        // Strictly greater on BOTH tests: above the margin at all, and above whatever is already
        // holding the slot — which is what makes file order the tie-break.
        if value > threshold && best.as_ref().is_none_or(|(_, held)| value > *held) {
            best = Some((kit, value));
        }
    }
    best.map(|(kit, _)| kit).unwrap_or(default)
}

/// **Which kit THIS HERD wants** — the one seam every surface resolves a herd's no-kit-named default
/// through: the wire's `HerdTelemetryState::default_kit_id`, `assign_labor … hunt` and both raiding
/// verbs.
///
/// # A pen is a SOURCE AXIS, not a score
///
/// A corralled herd is collected at [`crate::equipment_config::EquipmentStat::PenCarry`], and the
/// only kit that supplies it is the handling gear — so *"which kit does a pen want"* has a
/// **structural** answer, and [`quarry_default_hunt_kit`] structurally cannot give it: a pen has no
/// fight stage, so [`per_hunter_take_biomass`] scores every kit on a quarry the party never stalks
/// and hands back the *range* winner. That is how a corralled Rabbit Warren came to publish
/// `trapping` — a kit whose contribution at a pen is nil.
///
/// It is the same source-axis rule the client's picker greys on and `KitRoster.priced_source`
/// prices on, asked of the **roster** rather than answered with an id
/// ([`crate::equipment_config::EquipmentConfig::kit_supplying`]).
///
/// **A roster with no `PenCarry` hunt kit falls through to the score**, which is the honest answer:
/// nothing can work a pen properly, so the herd keeps whatever the range comparison chose rather
/// than publishing an empty selection.
pub fn herd_default_hunt_kit(
    equipment: &crate::equipment_config::EquipmentConfig,
    person: CombatStats,
    species: &SpeciesDef,
    corralled: bool,
) -> crate::equipment_config::KitChoice {
    let job = crate::equipment_config::KitJob::Hunt;
    if corralled {
        if let Some(kit) =
            equipment.kit_supplying(job, crate::equipment_config::EquipmentStat::PenCarry)
        {
            return kit;
        }
    }
    quarry_default_hunt_kit(equipment, person, species)
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

/// **What one crew's gear did work for, this fight** — the crew's own narrowed kit and the strikes
/// it is charged for ([`crate::equipment_config::WearQuantum::Strike`]).
///
/// **The kit rides with the number**, so a wear site charges what actually swung without holding the
/// party beside the outcome and zipping two lists by index — the alignment that would silently
/// mis-bill a crew the day a party's crews and its charges were built in different orders.
#[derive(Debug, Clone, PartialEq)]
pub struct StrikeCharge {
    /// The crew's kit, narrowed to what it holds — so an item the crew does not carry is never
    /// charged, which is what makes a bare-handed run free.
    pub kit: crate::equipment_config::KitChoice,
    /// Strikes landed, scaled by the share of the party's damage the bodies could absorb.
    pub strikes: f32,
}

/// **One turn's fight** — how many animals the party brought down, and what it cost.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntFight {
    /// **Whole animals brought down** — the bound [`quantise_animal_take`] takes as its fight arm.
    /// [`f32::INFINITY`] for a source with no fight stage at all (a pen).
    pub brought_down: f32,
    /// **Animals a turn, UN-floored** — [`Self::brought_down`]'s rate
    /// ([`crate::combat::StruckBlow::expected_units_down`]), and what every *per-turn* readout must
    /// publish.
    ///
    /// `brought_down` is whole bodies completed **this** turn, so a party below one body per turn
    /// reads `0` on most turns and `1` on the rest — a Wild Aurochs crew of eight reads `0` forever
    /// while genuinely taking ~0.75 a turn. Because the ledger keeps every remainder, this is
    /// exactly the average `brought_down` converges to under a stationary fight, which is what makes
    /// it the honest answer to *"how many next turn"*.
    ///
    /// It carries the same clamps `brought_down` does — the retreat's `stayed` and, through it, the
    /// escapement room — because both are computed from the same absorbed blow; only the whole-animal
    /// quantiser is missing, and that is a timing effect (`project_realized_hunt`'s doc).
    pub expected_brought_down: f32,
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
    /// **What the party's weapons are charged for** — one entry per crew that was holding something,
    /// in crew order. Empty for a pen (no fight), for a fixture party built through
    /// [`HuntingParty::uniform`] (no kit to charge), and for a forecast, which resolves the fight and
    /// drops the charge exactly as it drops the ledger.
    pub strike_charges: Vec<StrikeCharge>,
}

impl HuntFight {
    /// **Charge every crew's own kit for the blows it landed** — the one seam every take path's
    /// weapon wear goes through.
    ///
    /// A site says *"this party just fought"* and each crew pays for its own swing: the run that
    /// could not clear the quarry's defence landed nothing and is charged nothing, and a run holding
    /// no weapon has nothing in its kit to charge either. The **carry** kits are charged separately
    /// on their own quanta — this is only what was swung.
    pub fn charge_strike_wear(
        &self,
        wear: &mut crate::components::BandEquipment,
        config: &crate::equipment_config::EquipmentConfig,
    ) {
        for charge in &self.strike_charges {
            wear.wear_kit(
                config,
                &charge.kit,
                crate::equipment_config::WearQuantum::Strike,
                charge.strikes,
            );
        }
    }
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
    // **The party's strength — the TAKE crew's head count, and nothing else** (the same term
    // [`animals_engaged`] is handed, `docs/plan_standing_upkeep.md` §2.2). The retired build dip
    // scaled it, on the reasoning that hands gentling a herd are hands not fighting it; that is a
    // statement about a *shared* crew, and the player states the two crews separately now, so the
    // hunters here are only ever hunters.
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
            // No fight ran, so the rate is the bound itself: [`f32::INFINITY`] for a pen (no fight
            // stage at all) and `<= 0` for an empty engagement. Quantising nothing and rating
            // nothing are the same answer here.
            expected_brought_down: stayed,
            casualties: FightCasualties::default(),
            fought: false,
            wounds: quarry.wounds,
            // **Nobody swung.** A pen, an empty engagement or a party of nobody wears no weapon —
            // this is `docs/plan_denial_raid.md` §1.2's "never for turns elapsed", as a `Vec` that
            // is simply empty.
            strike_charges: Vec::new(),
        };
    }
    // **What the activity costs whoever shows up** (§4.6) — resolved before the quarry is even asked
    // whether it fights back, because it does not depend on the answer.
    let injuries = hunt_injuries(stayed, hunters, party);
    let mut wounds = quarry.wounds;
    if quarry.effective_attack() <= 0.0 {
        // **The one-sided engagement.** The animal cannot hurt anyone, so the fight itself costs
        // nothing and the kill is all the resolver would have computed.
        //
        // **Summed over the crews, exactly as the resolver sums its contingents** — a crew whose
        // attack the quarry's defense swallows contributes a hard zero here for the same reason it
        // does there ([`combat::strike_damage`]), so the short-circuit stays a short-circuit and
        // not a second model.
        let landed_per_crew: Vec<f32> = party
            .crews
            .iter()
            .map(|crew| {
                // A crew whose attack the quarry's defence swallows lands nothing, exactly as the
                // resolver's gate would have it — so it is charged nothing below.
                if combat::strike_damage(crew.hunter.attack, quarry.profile.defense) <= 0.0 {
                    return 0.0;
                }
                combat::landed_strikes_seeded(hunters * crew.share, &tuning, draw.seed())
            })
            .collect();
        let damage: f32 = party
            .crews
            .iter()
            .zip(&landed_per_crew)
            .map(|(crew, landed)| {
                landed * combat::strike_damage(crew.hunter.attack, quarry.profile.defense)
            })
            .sum();
        let blow = wounds.strike_blow(damage * tuning.lethality, &quarry.profile, stayed);
        return HuntFight {
            brought_down: blow.units_down,
            expected_brought_down: blow.expected_units_down,
            casualties: injuries,
            fought: false,
            wounds,
            strike_charges: strike_charges(
                party,
                &landed_per_crew,
                absorbed_share(blow.absorbed, damage * tuning.lethality),
            ),
        };
    }
    let payload = FightPayload {
        sides: vec![
            Force {
                id: HUNTING_PARTY_FORCE,
                posture: Posture::Aggressor,
                // **ONE CONTINGENT PER CREW**, because the resolver gates every attacker/target pair
                // on that attacker's own `attack` ([`combat::resolve_fight`]): a bare-handed run
                // inside a speared party lands *exactly zero* on a defence it cannot clear, where
                // one averaged profile would have let it borrow the spears' attack. Nothing about
                // the resolver changes — the party is simply described honestly.
                contingents: party
                    .crews
                    .iter()
                    .enumerate()
                    .map(|(index, crew)| Contingent {
                        kind: hunter_contingent_id(index),
                        count: hunters * crew.share,
                        profile: crew.hunter,
                    })
                    .collect(),
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
    // **What each crew landed**, in crew order — the party force's rows come back in the order its
    // contingents were built, which is the crew order.
    let mut landed_per_crew: Vec<f32> = Vec::with_capacity(party.crews.len());
    for result in &outcome.results {
        if result.force == QUARRY_FORCE {
            // **The DAMAGE, not the bodies.** The resolver's own `killed + wounded` has already been
            // divided by `durability` and clamped, so it cannot be banked; the raw flow can, and the
            // ledger below is what turns it into whole animals — this turn's and every earlier
            // turn's together.
            quarry_damage += result.damage_dealt;
        } else {
            // **Every non-quarry result**, which is now one per crew rather than one per party —
            // the loop already summed them and still does, so a party's losses are the whole
            // party's however its gear divided it.
            casualties.killed += result.killed;
            casualties.wounded += result.wounded;
            landed_per_crew.push(result.strikes_landed);
        }
    }
    let blow = wounds.strike_blow(quarry_damage, &quarry.profile, stayed);
    HuntFight {
        // **Whole animals** — the same rule `quantise_animal_take` exists for. A fractional kill left
        // un-floored would let `killed_biomass` and the reported `killed` count disagree, so the
        // ledger hands back only completed bodies and keeps the remainder.
        brought_down: blow.units_down,
        // …and the same blow before the floor, for the readouts that must answer *per turn* — see
        // the field.
        expected_brought_down: blow.expected_units_down,
        casualties,
        fought: true,
        wounds,
        strike_charges: strike_charges(
            party,
            &landed_per_crew,
            absorbed_share(blow.absorbed, quarry_damage),
        ),
    }
}

/// **How much of a party's blow the bodies could take, as a fraction** — the scale a strike charge
/// is billed at (`.claude/rules/core_sim/equipment.md` → "Wear follows the work actually done").
///
/// Ten hunters deal enough damage for five deer with two standing: two-fifths of the swing went into
/// a body, so two-fifths of the party's spears did work. **`1.0` when nothing was dealt** — there is
/// nothing to scale, and the strike count it multiplies is zero anyway; answering `0/0` as `0` would
/// read the same but hides which of the two is the reason.
fn absorbed_share(absorbed: f32, dealt: f32) -> f32 {
    if dealt > 0.0 {
        (absorbed / dealt).clamp(0.0, 1.0)
    } else {
        WHOLE_BLOW_LANDED
    }
}

/// The absorbed share of a blow that dealt nothing — see [`absorbed_share`].
const WHOLE_BLOW_LANDED: f32 = 1.0;

/// **One [`StrikeCharge`] per crew that is holding something** — its landed strikes, scaled by the
/// share of the party's damage the bodies could absorb.
///
/// A crew with **no kit** ([`HuntCrew::kit`] `None`) is skipped rather than charged zero: a fixture
/// or reference party has no ledger behind it, which is a different statement from *"this crew swung
/// and wore nothing out"*.
fn strike_charges(
    party: &HuntingParty,
    landed_per_crew: &[f32],
    absorbed_share: f32,
) -> Vec<StrikeCharge> {
    party
        .crews
        .iter()
        .zip(landed_per_crew)
        .filter_map(|(crew, landed)| {
            crew.kit.as_ref().map(|kit| StrikeCharge {
                kit: kit.clone(),
                strikes: landed * absorbed_share,
            })
        })
        .collect()
}

/// **The hunt's own hazard, resolved into people** (`docs/plan_hunt_through_combat.md` §4.6) —
/// [`HuntCrew::injury_damage_per_animal`] × the animals engaged, put through
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
///
/// # PER CREW, because `exposure` is a kit's
///
/// Each crew works its own share of the engagement (`stayed × share`) and is hurt at its own
/// `injury_damage_per_animal`, so the half of a party carrying the stand-off device takes the
/// stand-off's hazard and the half that closed with the animal takes the full one. Both the hazard
/// and the crew it is capped against scale by the same `share`, so a **uniformly** exposed party is
/// arithmetically identical to the single-profile form this replaced.
fn hunt_injuries(stayed: f32, hunters: f32, party: &HuntingParty) -> FightCasualties {
    let wounded = party
        .crews
        .iter()
        .map(|crew| {
            let hazard = crew.injury_damage_per_animal.max(0.0)
                * stayed
                * crew.share
                * party.tuning.lethality;
            combat::units_brought_down(hazard, &crew.hunter, hunters * crew.share)
        })
        .sum();
    FightCasualties {
        killed: NO_FATAL_HUNTING_ACCIDENTS,
        wounded,
    }
}

/// The baseline hunting hazard's fatal share — see [`hunt_injuries`] for why it is `0` rather than
/// the resolver's severity split.
const NO_FATAL_HUNTING_ACCIDENTS: f32 = 0.0;

/// The hunting party's side of a hunt fight — the aggressor.
const HUNTING_PARTY_FORCE: ForceId = ForceId(0);
/// The herd's side of a hunt fight — the defender.
const QUARRY_FORCE: ForceId = ForceId(1);
/// **One crew's contingent key** — the `person` row of the creatures roster, suffixed with the
/// crew's index so a partly-equipped party's runs are distinct contingents rather than one id
/// repeated.
///
/// Nothing downstream branches on it (`resolve_hunt_fight` sums every non-quarry result), so the
/// suffix is for legibility in a `FightOutcome` rather than for a consumer — but two contingents
/// sharing a key is the kind of thing a future report groups by, and it would silently merge two
/// crews that are deliberately not the same.
fn hunter_contingent_id(index: usize) -> ContingentId {
    ContingentId(format!("{HUNTER_CONTINGENT}#{index}"))
}

/// The party's contingent key stem, matching the `person` row of the creatures roster.
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
/// **EVERY path applies it, forecast paths included, and the retreat is why.** It used to be
/// live-only, on the argument that the quantiser re-clamps the kill by this same `affordable`
/// whatever the engagement was — true while [`animals_that_stay`] was the identity at `wariness 0`,
/// and **false the moment the roster authored a real wariness** (`docs/plan_hunt_through_combat.md`
/// §3.1). The retreat is a *fraction* of whatever it is handed, so a forecast that retreats the
/// party's full reach and clamps afterwards runs the retreat on a **different population** than the
/// take does, and over-quotes whenever the escapement room binds below that reach — the steady state
/// of an ordinary hunt. 20 hunters on a Rabbit Warren with room for 37: the take engages 37, keeps
/// ~9 and kills ~9; a forecast engaging 200 keeps 50 and quotes the whole 37. So the clamp belongs
/// **before** the retreat on the forecast paths too, which is exactly where the take applies it.
///
/// A non-finite `body_mass` (never reachable — `FaunaConfig::validate` pins it finite-positive) is
/// answered `0`, matching [`quantise_animal_take`]'s own guard.
pub fn animals_affordable(policy_ceiling: f32, body_mass: f32) -> f32 {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        return 0.0;
    }
    whole_animals(policy_ceiling.max(0.0), body_mass)
}

/// **[`animals_affordable`] AS A RATE — the same room, unrounded.** `ceiling ÷ body_mass`, with no
/// whole-animal floor under it.
///
/// # Why the floored form is wrong for a curve, and only for a curve
///
/// A take puts **bodies** on the ground, so `animals_affordable` is right wherever a turn is being
/// resolved. A [`hunt_crew_take_curve`] row is a **rate**, and the whole-animal quantum on this web
/// is a *timing* effect the herd's own biomass integrates — `SpeciesDef::body_mass`'s config note
/// says it outright: *"when the herd cannot yet spare a whole animal the hunt PAUSES and the herd
/// regrows; that wait is constant escapement, discretised, and the herd's own biomass is the
/// accumulator (there is no credit meter)."* Flooring a rate against that quantum therefore reports
/// a **cadence as a never**.
///
/// It is the same correction [`HuntFight::expected_brought_down`] already makes one stage later, for
/// the same reason and in the same words — the fight arm was floored to whole animals too, and a
/// curve of zeroes was published for crews genuinely taking `0.75` a turn. The room arm simply did
/// not get the treatment at the time.
///
/// Reported from play on a **Wild Aurochs** (`body_mass 120`, wild `r 0.09`) standing on its 50%
/// floor at 1200 of 2400 biomass: next turn's room is `0.09 × 1200 × 0.5` = **54 biomass — 0.45 of
/// one body** — which floors to **zero animals**, so every crew size read `0`, the sheet said the
/// hunters bring down nothing, and the stepper offered no crew to assign. The herd pays one aurochs
/// about every two and a half turns.
///
/// **The floor is untouched on every take path**, which is what makes this safe: only
/// [`EngagementQuantum::Rate`] reaches this function.
pub fn animals_sparable(policy_ceiling: f32, body_mass: f32) -> f32 {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        return 0.0;
    }
    policy_ceiling.max(0.0) / body_mass
}

/// **IS AN ENGAGEMENT COUNTED IN BODIES, OR AS A RATE?** — carried as a type rather than inferred
/// from [`HuntDraw`], because *whether to roll* and *what unit the answer is in* are independent and
/// the one caller that wants a rate is a forecast that could just as easily have wanted bodies.
///
/// The whole-animal quantum belongs to a **turn**, not to the model: the herd's own biomass carries
/// the remainder between turns, so a source sparing four fifths of a body a turn genuinely pays one
/// body every five turns. A reading that describes one turn must round; a reading that describes a
/// *rate* must not, or it reports that cadence as a never.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngagementQuantum {
    /// **A turn** — the room and the retreat are floored to whole animals, because that is what
    /// hits the ground. Every take path and every per-turn forecast.
    WholeAnimals,
    /// **A rate** — neither is floored. [`hunt_crew_take_curve`] alone, whose rows are documented as
    /// a per-turn rate and are already un-floored at the fight stage
    /// ([`HuntFight::expected_brought_down`]).
    Rate,
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

/// **WHICH of the take's five bounds actually stopped this hunt**
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
    /// **The party's own throughput** — the herd could spare another animal and the crew simply could
    /// not process one yet, because its kill-credit bank (`systems::expedition_take_biomass`) has not
    /// banked a whole body.
    ///
    /// **It is split out of [`Self::Floor`] because the two have opposite remedies**, and conflating
    /// them was a real defect: an 8-hunter raid on a full Thunder Mammoth herd banks toward one
    /// 800-unit body for several turns while fifteen mammoths stand there, and reporting *"the herd
    /// could not spare another whole animal"* there is simply false. `Floor` says *leave, there is
    /// nothing here*; `Throughput` says *bring more hands*.
    ///
    /// Only a **detached party** can report it — a resident band's ceiling is the escapement stock
    /// itself, with no bank between the herd and the crew (`systems::hunt_take`).
    Throughput,
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
            HuntTakeBound::Throughput => "throughput",
            HuntTakeBound::Carry => "carry",
            HuntTakeBound::Fight => "fight",
        }
    }
}

/// **Which bound [`quantise_animal_take`] actually hit**, read off the same terms and through the
/// same [`whole_animals`] helper — so the reported bound and the paid take cannot disagree about
/// what "affordable" or "carryable" mean.
///
/// **Precedence on a tie is `Floor/Throughput → Carry → Fight/Engagement`, and it is stated rather
/// than incidental.** Ties are common (a crew sized exactly to its ceiling), and the first arm is the
/// one that is true of the *source*: when the herd has nothing more to spare, that is the fact the
/// player needs whatever else was also tight.
///
/// The last two arms split one `min`: `brought_down` is capped by `stayed` from above, so bringing
/// down **everything that stayed** means reach was the limit, and bringing down less means the fight
/// was.
///
/// # The first arm splits too, on `escapement_room`
///
/// `take_ceiling` is what the quantiser was handed, and on a **detached party** that is not the herd's
/// escapement room: it is the party's kill-credit bank clamped to that room
/// (`systems::expedition_take_biomass`). Reading the ceiling alone therefore cannot tell *"the herd
/// has nothing left"* from *"the crew has not banked a body yet"* — it called both `Floor`, on a herd
/// with fifteen animals standing in it. `escapement_room` is the herd-side number the ceiling was
/// clamped against, and comparing the two **in whole animals** is what splits them: the bank only
/// costs the party a kill when it holds back a *whole* body, which is the same granularity the take
/// itself pays at.
pub fn hunt_take_bound(
    // What [`quantise_animal_take`] was handed — the same term, so the report cannot name a bound the
    // take did not hit.
    take_ceiling: f32,
    // **The herd's own room above the floor**, which `take_ceiling` may be smaller than. A resident
    // band passes its ceiling here unchanged: its ceiling *is* the escapement stock, so `Throughput`
    // is unreachable for it by construction rather than by a flag.
    escapement_room: f32,
    collection: f32,
    body_mass: f32,
    stayed: f32,
    brought_down: f32,
    stop: EngagementStop,
) -> HuntTakeBound {
    if !body_mass.is_finite() || body_mass <= 0.0 {
        return HuntTakeBound::Floor;
    }
    let affordable = whole_animals(take_ceiling.max(0.0), body_mass);
    // What the HERD could have spared. `<= affordable` (a band, or a raid whose bank has caught up
    // with the surplus) leaves the first arm reading `Floor` exactly as it did.
    let sparable = whole_animals(escapement_room.max(0.0), body_mass);
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
        // The ceiling bound the take — but WHOSE ceiling? The bank held back a whole animal the herd
        // could have spared, or it did not and the herd is genuinely the limit.
        if affordable < sparable {
            HuntTakeBound::Throughput
        } else {
            HuntTakeBound::Floor
        }
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
/// **`per_worker` is the crew's resolved carry tier and nothing else** — the pen's or the range's,
/// at the kit the crew was sent out with. The retired build dip used to multiply it, on the reasoning
/// that a crew gentling a herd hauls less than a hunting one; a build has its own hands now
/// (`docs/plan_standing_upkeep.md` §2.2), so the haulers are only ever haulers and the same rate
/// sizes the crew here and divides the client's `SourceForecast.max_useful_workers`.
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

/// **The ENGAGEMENT crew for a whole-animal (hunt) source** — how many hunters it takes to put the
/// ceiling's peak animal drop *on the ground* in one turn, the third unit in `workers_needed`'s
/// `max()` (`docs/plan_hunt_through_combat.md` §2).
///
/// ```text
/// crew = ceil(peak_animal_drop(ceiling, body) / (engage_rate × stay))
/// ```
///
/// # The retreat prices the CREW as well as the take
///
/// The divisor is what one hunter actually brings down in a turn — the animals they **reach**
/// (`engage_rate`) times the share of those that **stand** ([`stay_fraction`]) — and not the raw
/// reach. **A party that keeps one animal in four needs four times the hands to draw the same
/// stock down**, so the retreat belongs in every sizing of a hunting crew exactly as it already
/// belongs in the pricing of a hunting take ([`per_hunter_take_biomass`], which divides the same
/// three terms into a biomass rate).
///
/// It is the crew inverse of the closed-form per-hunter rate, *not* of [`animals_engaged`]: that
/// answers how many animals the party gets near, which is strictly more than it kills the moment a
/// species has any wariness at all. Sizing on the reach alone made the panel name a crew whose take
/// left the herd short — and, because the client's *clear it now* target divides the room by the
/// retreat-aware rate while the stepper cap divided by the raw one, name two different crews on the
/// same sheet.
///
/// **Pass the PARTY'S own stay, never the species' bare `1 − wariness`.** A kit moves the retreat
/// through [`crate::equipment_config::EquipmentStat::Dispersion`], so the crew and the take must be
/// resolved through the one [`stay_fraction`] seam or a sheet can size a crew at one dispersion
/// beside a take priced at another.
///
/// # Why it cannot be folded into the haul crew
///
/// The two terms scale on **different units** — hauling is per *biomass* (one hauler carries 40),
/// engaging is per *animal* (one hunter reaches 10 fowl or 0.05 mammoths) — so neither dominates
/// across the roster, exactly as the herder term does not. A Wild Fowl herd with ~470 head above its
/// floor is 61 biomass: **two** haulers clear it and dozens of hunters are needed to reach it, so
/// sizing the crew on carry alone told the player *"more hands would be idle"* about the very hands
/// the take was short of. The mammoth inverts it (one hunter reaches the peak drop; twenty are needed
/// to carry it home).
///
/// # There is no build term in the divisor, on this seam or its two siblings
///
/// The rung's `yield_fraction_while_building` used to sit between the reach and the stay here, in
/// [`animals_engaged`] and in [`hunt_haul_workers`] — one statement about a crew doing two jobs at
/// once. The player states the two crews separately now (`docs/plan_standing_upkeep.md` §2.2), so a
/// hunter is only ever a hunter and this function takes no rung.
///
/// # A source with no engagement stage reports no engagement crew
///
/// `0` for a **pen** and for the plant web, whose `engage_rate` is [`f32::INFINITY`]
/// ([`FaunaConfig::engage_rate_for`], [`SourceYieldForecast::managed`]) — a penned animal is not
/// stalked and a plant is not either — so the `max()` collapses to the haul term and neither web
/// regresses. Same for a degenerate `body`/rate.
///
/// # `stay == 0` reports no crew, and that is the ANSWER rather than a fudge
///
/// At a stay of zero nothing the party reaches ever stands: the take is identically zero at *every*
/// party size, so the crew needed to achieve it is none. `0` is therefore the honest count, not a
/// sentinel — and emphatically not [`u32::MAX`], which would read as *"staff it harder"* about a
/// source no crew can take anything from.
///
/// Units on `ceiling`/`body` are free, exactly as they are for [`hunt_haul_workers`]: an animal count
/// is a ratio, so a provisions-space call and a biomass-space one give the same crew.
pub fn hunt_engage_workers(ceiling: f32, body: f32, engage_rate: f32, stay: f32) -> u32 {
    if !body.is_finite() || body <= 0.0 {
        return 0;
    }
    // What one hunter puts on the ground in a turn: reached × stayed. The `build_dip` factor that
    // used to sit between them retired with the shared crew — a build is staffed in its own right
    // now, so a hunter is only ever a hunter.
    let brought_down = engage_rate.max(0.0) * stay.clamp(0.0, 1.0);
    if !brought_down.is_finite() || brought_down <= 0.0 {
        // No engagement stage (a pen, a plant) or a quarry that never stands — neither of which is a
        // crew size. Either way this term has nothing to say and the `max()` keeps the others.
        return 0;
    }
    (peak_animal_drop(ceiling, body) / brought_down).ceil() as u32
}

/// **THE take-side crew for a whole-animal (hunt) source** — `max(`[`hunt_haul_workers`]`,
/// `[`hunt_engage_workers`]`)`, and the single seam every `workers_needed` on the animal web sizes
/// its take half with (the assign-time seed in [`forecast_source_yield`] and the resolved Hunt arm of
/// `advance_labor_allocation`), so the two cannot answer differently.
///
/// **Two jobs, one crew, two units** — bring the animals down, then carry them home. It is the
/// take-side half of [`crate::intensification::source_crew_needed`]'s `max(standing, take)`, which
/// adds the third: the herders who mind a managed herd whether or not it is killed from this turn.
/// `max()`, never `+`: one crew covering its busiest job.
///
/// `stay` is the party's own retreat term ([`HuntingParty::stay_fraction`]) — see
/// [`hunt_engage_workers`] for why a crew is sized on what stands rather than on what is reached.
pub fn hunt_take_workers(
    ceiling: f32,
    body: f32,
    per_worker: f32,
    engage_rate: f32,
    stay: f32,
) -> u32 {
    hunt_haul_workers(ceiling, body, per_worker).max(hunt_engage_workers(
        ceiling,
        body,
        engage_rate,
        stay,
    ))
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
/// **HOW MUCH OF THIS HERD A CREW CAN REACH AND HANDLE IN A TURN** — the engagement bound, at the
/// rung the herd actually stands on.
///
/// # ⛔ EVERY RUNG HAS ONE, INCLUDING THE PEN
///
/// A penned herd used to pass `f32::INFINITY` here, on the reading that a fenced animal is not
/// stalked. **An infinite bound is not "no stalking", it is no bound** — and it is what let the pen's
/// take escape every check the wild path applies. A keeper genuinely handles far more animals per
/// turn than a hunter, because they are standing still rather than running away; that is a
/// **multiplier** (`husbandry.pen_engage_gain`), not the absence of a number.
///
/// **It STEPS at the fence**, like everything else penning buys: `is_corralled()` is a stored fact
/// set at completion, and half a fence is no fence — the animals are still roaming and nothing about
/// handling them has changed. That is the deliberate difference from the plant web's Field, where a
/// half-sown field genuinely has half a crop in the ground.
pub fn herd_engage_rate(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    let wild = fauna.engage_rate_for(&herd.species);
    if herd.is_corralled() {
        wild * fauna.husbandry.pen_engage_gain
    } else {
        wild
    }
}

/// **THE seam between the herd's damage ledger and the fight**, and the sugar every path holding a
/// `&Herd` must use: `quarry_fight_for` alone hands back an *un-hunted* animal, so a take path that
/// skipped this would silently restart the mammoth's wounds every turn — the stateless behaviour the
/// accumulator exists to replace, failing quietly rather than loudly.
pub fn herd_quarry_fight(herd: &Herd, fauna: &FaunaConfig) -> QuarryFight {
    fauna
        .quarry_fight_for(&herd.species)
        .with_wounds(herd.wounds)
}

/// **The take's first three stages, resolved together** — engagement, retreat, fight — and the ONE
/// definition of them.
///
/// [`crate::systems::hunt_take`] is this plus the quantiser and the herd mutation; the crew-take
/// query's curve is this and nothing else. They call the same function because a curve that answers
/// a *different* three stages from the ones the turn runs is precisely the defect the curve exists
/// to close: the client used to compose `animals_that_stay(animals_engaged(..))` itself, which is
/// stages one and two with **the fight missing**, and quoted a Wild Aurochs party 2.3× what the sim
/// pays.
///
/// # The order is load-bearing, and so is where the room clamp sits
///
/// ```text
/// ceiling = herd_take_room(floor)                                   // the escapement room
/// engaged = animals_engaged(workers).min(animals_affordable(ceiling))
/// stayed  = party.stayers(engaged, wariness)                        // the retreat
/// fight   = resolve_hunt_fight(stayed, workers, party, quarry)      // damage over durability
/// ```
///
/// **The room clamps the engagement, not the outcome.** Restraint is free
/// (`docs/plan_hunt_through_combat.md` §1): the floor bounds what the party *goes after*, and since
/// the retreat keeps a fraction of whatever it is handed, clamping afterwards would retreat a bigger
/// party than the take does and over-quote every turn the room binds. That is why a curve answered
/// at one floor cannot be reused at another.
///
/// # It does not touch the herd
///
/// The wound ledger arrives on [`HuntFight::wounds`] and the caller decides whether to store it —
/// which is what lets a query resolve exactly the fight the turn will and simply drop it, the same
/// contract [`resolve_hunt_fight`] already keeps.
pub fn resolve_hunt_engagement(
    herd: &Herd,
    fauna: &FaunaConfig,
    party: &HuntingParty,
    workers: u32,
    floor: f32,
    // **Live or forecast** — a live take draws both stochastic stages from its per-event seed; a
    // curve reads their quantiles. See [`HuntDraw`].
    draw: HuntDraw,
    // **Bodies, or a rate?** — see [`EngagementQuantum`]. Orthogonal to `draw`: *whether to roll* and
    // *what unit the answer is in* are different questions, and the curve is the one caller that
    // wants a rate.
    quantum: EngagementQuantum,
) -> HuntEngagement {
    let ceiling = herd_take_room(herd, floor, fauna);
    let reach = animals_engaged(workers, fauna.engage_rate_for(&herd.species));
    let wariness = fauna.wariness_for(&herd.species);
    let (engaged, stayed) = match quantum {
        EngagementQuantum::WholeAnimals => {
            let engaged = reach.min(animals_affordable(ceiling, herd.body_mass));
            (engaged, party.stayers(engaged, wariness, draw))
        }
        EngagementQuantum::Rate => {
            let engaged = reach.min(animals_sparable(ceiling, herd.body_mass));
            (engaged, party.stayers_at_rate(engaged, wariness, draw))
        }
    };
    let fight = resolve_hunt_fight(
        stayed,
        workers as f32,
        party,
        &herd_quarry_fight(herd, fauna),
        draw,
    );
    HuntEngagement {
        ceiling,
        engaged,
        stayed,
        fight,
    }
}

/// What [`resolve_hunt_engagement`] worked out — every intermediate the take reports on, so a caller
/// reads them rather than recomputing any one of them.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntEngagement {
    /// The escapement room at the caller's floor, in **biomass** ([`herd_take_room`]).
    pub ceiling: f32,
    /// Animals the party brought into contact — the reach, already clamped by what the herd can
    /// spare.
    pub engaged: f32,
    /// Of those, how many stayed to be fought.
    pub stayed: f32,
    /// The fight. [`HuntFight::brought_down`] is **whole animals on the ground this turn** and is
    /// the third arm [`quantise_animal_take`] `min`s — the term a pre-commit reading cannot derive
    /// and must be told. A readout answering *"per turn"* wants
    /// [`HuntFight::expected_brought_down`] instead, which is the same fight without the
    /// whole-animal quantiser.
    pub fight: HuntFight,
}

/// **One crew size's whole-crew take**, in animals a turn, with the fight already resolved — one row
/// of [`hunt_crew_take_curve`].
///
/// It is [`HuntFight::expected_brought_down`] and never [`HuntFight::brought_down`]: the rows are a
/// **rate**, not the bodies that hit the ground next turn. See [`hunt_crew_take_curve`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntCrewTake {
    /// The crew this row prices — **the whole party**, not a marginal hunter. The take is not linear
    /// in the crew (the fight arm is, the engagement arm is a staircase), so a per-hunter reading of
    /// this number is wrong by up to the width of a tread.
    pub workers: u32,
    /// The pessimistic bound, at `combat_config.forecast_range_sigmas` below the mean.
    pub low: f32,
    /// The point estimate ([`crate::combat::EXPECTED_STRIKES`]) — the only quantile
    /// [`hunt_useful_crew`] reads.
    pub likely: f32,
    /// The optimistic bound, the same width above.
    pub high: f32,
}

/// Everything one crew-take curve is resolved from, gathered so the **query** and the **capture**
/// hand the producer identical inputs rather than each assembling a party its own way.
pub struct HuntCrewCurveInputs<'a> {
    /// The quarry, live — its stock, its wounds and the rung it stands on all enter the take.
    pub herd: &'a Herd,
    pub fauna: &'a FaunaConfig,
    /// The item table every crew's tier resolves through.
    pub equipment: &'a crate::equipment_config::EquipmentConfig,
    /// The kit this crew works under, already **resolved** to a roster entry — a curve priced at the
    /// job default for a band carrying traps answers a question nobody asked.
    pub kit: &'a crate::equipment_config::KitChoice,
    /// The band's live wear ledger. Coverage is re-resolved per crew size against it, which is a
    /// term of the curve rather than an accident: five spears stretch differently over four hunters
    /// than over twelve.
    pub wear: &'a crate::components::BandEquipment,
    /// The `person` roster row — what a hunter is before any gear.
    pub intrinsic: CombatStats,
    /// The severity dials the fight resolves at. **A resident band hunting its own range passes the
    /// base [`crate::combat_config::CombatConfig::tuning`]**; only a detached raid passes
    /// `expedition_tuning`, and the two differ by half again in the fight term.
    pub tuning: CombatTuning,
    /// [`crate::combat_config::CombatConfig::hunt_injury_damage_per_animal`].
    pub hunt_injury_damage_per_animal: f32,
    /// `combat_config.forecast_range_sigmas` — the reported band's half-width, a readout lever.
    pub range_sigmas: f32,
    /// The floor this crew stops at. **A curve answered at one floor cannot be reused at another**:
    /// the room clamps the engagement *before* the retreat, so it moves every row.
    pub floor: f32,
    /// `labor_config.hunt.per_worker_biomass_capacity` — what a **bare-handed** worker carries, the
    /// baseline both animal carry tiers are resolved against
    /// ([`crate::equipment_config::EquipmentConfig::pen_per_worker_biomass_capacity`]).
    ///
    /// **The stalking rows do not read it** — they are a *kill* rate, and the haul is a separate
    /// bound the take applies afterwards. **The PEN rows do**, because a pen has no fight and its
    /// crew term is the collection: what stops a penned row rising is the keepers running out of
    /// hands to bring animals out with.
    pub baseline_haul_rate: f32,
    /// The largest crew the curve is asked about — the source's own crew pool (the hands on it plus
    /// the band's idle ones), which is every crew the stepper beside it can reach.
    pub max_workers: u32,
}

/// **THE QUARRY AS NEXT TURN'S TAKE WILL FIND IT** — a clone with one Logistics regrowth applied,
/// which is the state every forecast on this seam is really being asked about. Nothing here touches
/// the caller's herd.
///
/// **`pub` because a HARNESS has to advance the same turn the forecast projected**: a fixture that
/// quotes a pre-commit row and then resolves the take must run this in between, or it compares two
/// turns and reads the growth as a drift.
///
/// It is [`project_realized_hunt`]'s loop body stopped after its first step (`regrow` → read the
/// room), and it exists so that *"a forecast regrows first"* is **one expression** rather than a
/// rule each forecast path remembers. [`hunt_crew_take_curve`]'s doc carries the measurement that
/// forced it.
///
/// **It does not despawn, shed, starve or graze**, and that is deliberate rather than an omission:
/// those are `advance_herds` / `advance_husbandry`'s business and they need a world. The projection
/// makes exactly the same simplification, so the curve and the steady rate the work board publishes
/// stay two readings of one model.
///
/// **"Regrown" is not "larger".** Below the Allee threshold [`regrow_biomass`] takes the depensation
/// branch and the clone comes back **smaller** — which is the honest forecast for a collapsing herd,
/// since next turn's take really will draw on less stock. A guard asserting the clone never shrinks
/// looked obviously true and fired on the first thin-herd fixture it met.
pub fn next_turns_quarry(herd: &Herd, fauna: &FaunaConfig) -> Herd {
    let mut quarry = herd.clone();
    regrow_biomass(&mut quarry, fauna);
    quarry
}

/// **THE HUNT TAKE CURVE — the one producer.** One row per crew size, `1..=max_workers`, each row
/// the *whole* crew's expected animals a turn with the engagement, the retreat and **the fight** all
/// resolved.
///
/// # Why it is a seam and not a body inside the query
///
/// Because the answer travels on **two transports** and there may only be one arithmetic behind
/// them:
///
/// - `forecast_query::answer_hunt_crew_take` ships the rows themselves, for a compose sheet asking
///   about a crew it has not committed yet;
/// - the snapshot ships [`hunt_useful_crew`] of these rows on an **assigned** row's
///   [`sim_schema::state::LaborAssignmentState::hunt_useful_workers`], because the Work board
///   renders many rows a frame and cannot round-trip for each of them.
///
/// The board's `+` gate used to divide the room by a *fightless* per-worker reach — the engagement
/// and the retreat with no attack, no defense and no durability — and so quoted a different ceiling
/// from the compose sheet for the same herd. Both now read this.
///
/// # The rows are a RATE, and that is not a rounding preference
///
/// [`HuntFight::expected_brought_down`], never `brought_down`. A Wild Aurochs (`defense 6`,
/// `durability 150`, `engage_rate 0.17`) is engaged one animal at a time by every crew from 1 to 11,
/// so the blow is capped well under a `150`-durability body and `floor(damage / durability)` is `0`
/// for every one of them — a curve of zeroes for crews genuinely taking `0.75` a turn. The wound
/// ledger the sim carries between turns is what makes the un-floored rate the honest answer, and a
/// curve is one frozen turn by construction.
///
/// # It resolves the take's own three stages, and does not mutate
///
/// Each row is [`resolve_hunt_engagement`] — literally the function `systems::hunt_take` runs — with
/// the wound ledger it hands back dropped. Nothing here touches the caller's herd.
///
/// # It asks about NEXT TURN, so it regrows first — the take it predicts does
///
/// Every caller resolves this **after** the Population take: the query answers a client between
/// turns and the capture publishes [`hunt_useful_crew`] in the Snapshot stage. The take it is
/// predicting runs after the *next* Logistics regrowth. Reading the herd as it stands is therefore
/// reading it a whole turn early, and the error is not small — it is the entire take, because both
/// terms the room is made of are written by the take that just happened:
///
/// - [`escapement_ceiling`] reads `biomass`, which the take has just drawn back down toward the
///   floor. A crew holding a herd at its floor leaves a room of approximately nothing.
/// - the [`growth_share`] backstop reads [`Herd::growth_this_turn`], which is
///   `biomass − biomass_before_regrowth` — and the take is subtracted from `biomass` after
///   `regrow_biomass` stamps the pair. On a source harvested at or above its growth that field is
///   **zero**, so the backstop that exists to pay a source sitting at its floor is switched off by
///   precisely the harvesting that puts it there.
///
/// Reported from play on a Rabbit Warren (`K 10`, floor `0.5`, one trapper): the row's own
/// `actualYield` was `0.0216` — four rabbits — and its `arrivalSchedule` was positive in all twenty
/// slots, while this curve read **zero at every crew size**, the sheet said *"these hunters bring
/// down ≈0 Rabbit Warren/turn"*, and `huntUsefulWorkers` published `0` for a row that was feeding
/// the band. The stock the take saw was `5.914`; the stock this read was `5.039`.
///
/// So the quarry is a **private regrown clone**, which is [`project_realized_hunt`]'s loop
/// (`regrow` → read the room → take) stopped after its first turn — and that is why the work board,
/// which reads that projection, was right about this herd for the whole life of the discrepancy.
/// The clone is what keeps *"nothing here touches the herd"* true.
pub fn hunt_crew_take_curve(inputs: &HuntCrewCurveInputs<'_>) -> Vec<HuntCrewTake> {
    let quarry = next_turns_quarry(inputs.herd, inputs.fauna);
    // **A PEN IS COLLECTED, NOT STALKED — so it gets the curve of the take it actually resolves.**
    // See [`pen_crew_take_curve`]; the branch is here, at the one producer, so neither transport and
    // no client has to know which rung a row stands on to trust the number.
    if quarry.is_corralled() {
        return pen_crew_take_curve(&quarry, inputs);
    }
    let sigmas = inputs.range_sigmas.abs();
    (1..=inputs.max_workers)
        .map(|workers| {
            let coverage = inputs
                .equipment
                .coverage(inputs.kit, workers as f32, inputs.wear);
            let party = PartyResolution {
                equipment: inputs.equipment,
                coverage: &coverage,
                wear: inputs.wear,
                intrinsic: inputs.intrinsic,
                tuning: inputs.tuning,
                hunt_injury_damage_per_animal: inputs.hunt_injury_damage_per_animal,
            }
            .party_against(crate::equipment_config::Quarry::Mass(quarry.body_mass));
            let take_rate = |draw_sigmas: f32| {
                resolve_hunt_engagement(
                    &quarry,
                    inputs.fauna,
                    &party,
                    workers,
                    inputs.floor,
                    HuntDraw::Quantile {
                        sigmas: draw_sigmas,
                    },
                    // **THE ONE CALLER THAT WANTS A RATE** — these rows are documented as a per-turn
                    // rate, and are already un-floored at the fight stage. See
                    // [`EngagementQuantum`].
                    EngagementQuantum::Rate,
                )
                .fight
                .expected_brought_down
            };
            HuntCrewTake {
                workers,
                // Monotone non-decreasing in the quantile at every stage, so `low <= likely <= high`
                // is a property of the arithmetic rather than a clamp applied afterwards — the same
                // invariant [`forecast_take_range`] holds.
                low: take_rate(-sigmas),
                likely: take_rate(crate::combat::EXPECTED_STRIKES),
                high: take_rate(sigmas),
            }
        })
        .collect()
}

/// **THE PEN COLLECTION CURVE — the same question as [`hunt_crew_take_curve`], asked of a rung with
/// no engagement stage.** One row per crew size, `1..=max_workers`, in animals a turn.
///
/// # A penned row HAS a useful-crew ceiling, and it is not the stalking one
///
/// A corralled herd never reaches `systems::hunt_take`: the Hunt arm's tend branch `continue`s
/// before it and resolves the slaughter itself. There is no engagement, no retreat and **no fight**
/// — a penned beast is walked out and killed — so a stalking curve over a pen answers a question the
/// sim never asks, and answers it with the quarry's `defense` and the crew's *hunting* kit. A pen
/// whose defense bare hands cannot clear published [`NO_USEFUL_CREW`] and shut the Work board's `+`
/// gate on a row whose keepers were collecting perfectly well.
///
/// What actually bounds a pen's take, and therefore what this curve is
/// (`systems::labor`'s tend branch, term for term):
///
/// ```text
/// production = herd_take_room(herd, floor)            // crew-INDEPENDENT: the stock above the floor
/// collection = workers × pen_per_worker_biomass       // the HUSBANDRY tier, coverage-weighted
/// handling   = herd_engage_rate(herd) × workers       // the species' rate × the pen's handling gain
/// row        = quantise_animal_take(production, collection, body_mass, handling, WhenPackFull).killed
/// ```
///
/// So the ceiling is *the crew at which the keepers stop being the binding term* — the pen's honest
/// answer to *"would another pair of hands buy me more"*, which is exactly what the field means on
/// every other row.
///
/// # The three quantiles are one number, and that is a fact rather than a shortcut
///
/// The spread on a stalking row is the **fight**'s (`combat_config.forecast_range_sigmas` about
/// [`crate::combat::EXPECTED_STRIKES`]). A slaughter has no fight, so there is nothing to be
/// uncertain about: `low == likely == high`, and a reader drawing a band around a pen row would be
/// drawing one around a certainty.
///
/// # It is *animals put on the ground*, matching the stalking rows' `expected_brought_down` — AND
/// # IT IS A RATE, FOR THE SAME REASON THEY ARE
///
/// It published [`quantise_animal_take`]'s `killed`, which is `0` for any crew whose room affords
/// less than one whole body — so a penned **Wild Aurochs** (`body_mass 120`) whose next-turn room is
/// 54 biomass read `0` at *every* crew size, [`hunt_useful_crew`] answered [`NO_USEFUL_CREW`], and
/// the Work board's `+` shut on a pen that collects one beast about every two and a half turns.
/// That is the *"cadence reported as a never"* the stalking rows were fixed for
/// ([`EngagementQuantum::Rate`]); the pen path is the same statement about the same quantum, and the
/// herd's own biomass is the accumulator on both.
///
/// The carry bound stays (a curve asks *"would another pair of hands buy me more"*, and on a pen the
/// keepers' haul is what answers), and it keeps its own [`ONE_WHOLE_ANIMAL`] floor, which is not a
/// rounding but the indivisibility of the animal.
///
/// # `quarry` is the REGROWN clone, not `inputs.herd`
///
/// A pen is drawn through the same escapement room a stalked herd is, so it carries the same
/// one-turn skew and takes the same cure — see [`hunt_crew_take_curve`], which does the regrowing
/// and hands the result down. Reading `inputs.herd` here would leave the pen rows a turn behind the
/// stalking ones for no reason anybody could state.
fn pen_crew_take_curve(quarry: &Herd, inputs: &HuntCrewCurveInputs<'_>) -> Vec<HuntCrewTake> {
    // **Crew-independent, so it is resolved once** — the stock standing above this assignment's own
    // floor, through the very seam the tend branch draws from.
    let production = herd_take_room(quarry, inputs.floor, inputs.fauna);
    // The species' own handling rate with the pen's gain already folded in — a keeper handles far
    // more animals a turn than a hunter because they are standing still rather than running away.
    let handling_per_worker = herd_engage_rate(quarry, inputs.fauna);
    (1..=inputs.max_workers)
        .map(|workers| {
            // **Coverage re-resolved per crew size**, exactly as the stalking rows do it: a band with
            // five sets of handling gear stretches them differently over four keepers than over
            // twelve, and that curvature is a term rather than an accident.
            let coverage = inputs
                .equipment
                .coverage(inputs.kit, workers as f32, inputs.wear);
            let carry_per_worker = coverage.weighted_rate(|kit| {
                inputs.equipment.pen_per_worker_biomass_capacity(
                    inputs.baseline_haul_rate,
                    kit,
                    inputs.wear,
                )
            });
            // **A RATE, NOT A TURN'S BODIES** — [`animals_sparable`], and the carry arm un-floored
            // beside it, for the reason [`EngagementQuantum::Rate`] states one branch over. This is
            // the same three bounds [`quantise_animal_take`] applies (`killed`), with the two
            // whole-animal floors taken off: the room a crew may spare, what its keepers can carry
            // out, and the species' handling.
            let take = animals_sparable(production, quarry.body_mass)
                .min(
                    (workers as f32 * carry_per_worker / quarry.body_mass)
                        // **The carry arm still cannot bind below one body**, exactly as
                        // `quantise_animal_take`'s `carryable.max(1.0)` does not: a keeper who
                        // cannot haul a whole beast still walks one out and wastes the rest. That
                        // is a fact about the animal, not a rounding, so it survives the rate.
                        .max(ONE_WHOLE_ANIMAL),
                )
                .min(handling_per_worker * workers as f32)
                .max(0.0);
            HuntCrewTake {
                workers,
                low: take,
                likely: take,
                high: take,
            }
        })
        .collect()
}

/// **THE SMALLEST TAKE A CREW THAT TAKES ANYTHING PUTS ON THE GROUND** — one animal, because an
/// animal is indivisible even when the hands that killed it cannot carry it home. It is
/// [`quantise_animal_take`]'s own `carryable.max(1.0)`, named where [`pen_crew_take_curve`] applies
/// it as a rate: the collection bound may not fall below one body, though every other bound may.
const ONE_WHOLE_ANIMAL: f32 = 1.0;

/// **NO CREW IS USEFUL HERE** — what [`hunt_useful_crew`] answers when no crew in the curve brings
/// anything down at all: a bare-handed party against a `defense` it cannot clear lands exactly zero
/// however many people it sends, and *"one worker is useful"* would be a false floor.
///
/// It is also what an **empty** curve reads, which is the same statement about a crew pool of
/// nobody. The wire's [`sim_schema::state::LaborAssignmentState::hunt_useful_workers`] carries it as
/// `0`.
pub const NO_USEFUL_CREW: u32 = 0;

/// **HOW CLOSE COUNTS AS THE SAME TAKE.** The curve's rows are `min(fight, stayed)` where `stayed`
/// is itself clamped by the room, so adjacent crews on a bound tread agree to within float noise
/// rather than bit-for-bit; without an epsilon a wobble in the last mantissa bits would read as a
/// crew that buys more take. **Relative, not absolute**, because the rows span a Wild Fowl's
/// hundreds of animals a turn and a mammoth's hundredths.
///
/// The client's `SourceForecast.CREW_TAKE_REACH_TOLERANCE` is the same number for the same reason —
/// it walks the *published rows* of this same curve — so the two readings of one curve cannot
/// disagree about where it stopped rising.
const CREW_TAKE_RISE_TOLERANCE: f32 = 0.001;

/// **WHERE THE CURVE STOPS RISING** — the crew beyond which more hands add nothing, *fight
/// included*. This is what *"max N workers useful here"* means, and it is the same answer the
/// crew-take curve plateaus at because it **is** that plateau: the snapshot publishes this, the
/// query publishes the rows, and both come out of [`hunt_crew_take_curve`].
///
/// # It is the LAST RISE, not the first flat
///
/// The engagement is a staircase — `floor(w × engage_rate)` is flat across whole runs of crew sizes
/// and steps at integer boundaries — so a scan that stopped at the first crew whose take equalled
/// its predecessor's would report the bottom of a tread as the top of the stairs. On the shipped
/// Wild Boar (`engage_rate 0.33`) crews one through six all bring the same single animal to bay and
/// the seventh brings two.
///
/// # A curve still rising at its last row plateaus AT that row
///
/// The curve is asked about a bounded pool, so *"still climbing when the rows ran out"* is the
/// honest answer *every hand this band has is still buying take* — not a licence to invent crews it
/// cannot field. A reader that needs to tell the two apart compares the answer with the curve's
/// length, which is what the client's `crew_take_curve_settled` does.
///
/// [`NO_USEFUL_CREW`] when nothing in the curve brings anything down.
pub fn hunt_useful_crew(curve: &[HuntCrewTake]) -> u32 {
    let mut plateau = NO_USEFUL_CREW;
    let mut best = 0.0f32;
    for row in curve {
        // A non-finite row is not a bigger take, it is an unpriceable one — the reading a source
        // with no engagement stage at all produces — so it never counts as a rise.
        if row.likely.is_finite() && row.likely > best * (1.0 + CREW_TAKE_RISE_TOLERANCE) {
            best = row.likely;
            plateau = row.workers;
        }
    }
    plateau
}

// **RETIRED: `corral_yield`** — the gross managed yield a penned herd handed its keeper each turn.
// It was `pen_yield_biomass` through the species vector, with no floor term, no drawdown and no
// engagement bound. A pen takes the ordinary escapement draw now: **a rung may change production, no
// rung changes the draw.**

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
    // **RETIRED: the pen's MANAGED early return.** A penned herd used to short-circuit into a flat
    // production with no drawdown, no escapement floor, no engagement bound and
    // `sustainable == actual`. **Production and draw are separate concerns; a rung may change
    // production, no rung changes the draw** — so a pen takes the ordinary path below, and what
    // penning buys is its `r` gain, its density gain on `K`, its slower escape and its handling gain.
    let hunt_yield = herd_hunt_yield(herd, fauna);
    // **THE HERD AS NEXT TURN'S TAKE WILL FIND IT** — one Logistics regrowth applied to a private
    // clone, for the reason spelled out on the two stock terms below.
    let quarry = next_turns_quarry(herd, fauna);
    // **The two investment rungs' harvests, in BIOMASS, resolved once.** Both quotes below are a
    // rate applied to one of these, and the material account needs the biomass itself — see
    // `SourceYieldForecast::managed_yield_biomass`. Same `biomass_before_regrowth` basis and
    // `carrying_capacity` the wild ceiling uses, so the ONLY difference from Sustain is the rung's
    // boosted `r`.
    let pen_msy_biomass = sustainable_yield(
        herd.biomass_before_regrowth,
        herd.carrying_capacity,
        &pen_ecology_for(herd, fauna),
    );
    let pastoral_msy_biomass = sustainable_yield(
        herd.biomass_before_regrowth,
        herd.carrying_capacity,
        &pastoral_ecology_for(herd, fauna),
    );
    SourceYieldForecast {
        per_worker_yield: hunt_yield.apply(per_worker_biomass_capacity.max(0.0), output_multiplier),
        // The quantum that makes this preview pulse exactly as the take does (slice 8).
        body_mass_yield: hunt_yield.apply(herd.body_mass, output_multiplier),
        // The engagement throughput the take is bounded by, so preview and take agree on how many
        // animals the party can even reach.
        engage_rate: herd_engage_rate(herd, fauna),
        // ...and the fight that decides how many of those actually go down (§4).
        // ...carrying **this herd's** accumulated wounds, so a single-turn preview says "this is the
        // turn it finally goes down" on the turn it does (`herd_quarry_fight`, §4.2).
        fight: Some((party.clone(), herd_quarry_fight(herd, fauna))),
        // **The TERMS of the take, not a set of answers.** `ceiling_at(floor, improvement)` composes
        // them into exactly what `hunt_take` computes — the herd's own `K` (`herd_capacity`, never
        // the raw field) and the stock the take will find, so the forecast and the take read one
        // herd.
        //
        // # ⛔ BOTH STOCK TERMS ARE NEXT TURN'S, BECAUSE THE TAKE THEY PRICE RUNS AFTER LOGISTICS
        //
        // Every caller resolves this **after** the Population take — the capture publishes a row in
        // the Snapshot stage, the assign-time seed answers a client between turns — so the raw herd
        // is a whole turn stale in *both* terms `take_room` is made of: `biomass` has just been
        // drawn back toward the floor, and `growth_this_turn` is `biomass − biomass_before_regrowth`
        // with the take subtracted after `regrow_biomass` stamped the pair, which on a worked source
        // reads **zero** and switches off the very backstop that pays a herd sitting on its floor.
        // So the pair comes off [`next_turns_quarry`] — the same private regrown clone
        // [`hunt_crew_take_curve`] resolves against, and the first step of the loop
        // [`project_realized_hunt`] runs — and the row's `actual`, its `realized` headline and the
        // crew curve beside it are three readings of one turn rather than two frames.
        biomass: quarry.biomass,
        carrying_capacity: herd_capacity(herd, fauna),
        growth: quarry.growth_this_turn(),
        // What one unit of this herd's biomass is worth, in both currencies — the species' vector,
        // resolved once for the whole forecast.
        per_biomass_yield: hunt_yield.apply(ONE_UNIT_OF_BIOMASS, output_multiplier),
        // **EVERY rung is drawn down** — there is a standing stock to stop short of at all three,
        // which is the whole of what a floor decides. The pen's exemption is retired.
        managed_production: None,
        // The Corral rung's PAYOFF (`corralYield`) projected for a still-un-penned herd: the pen's
        // **sustained MSY** on the improved (pen) ecology — the long-run rate that shows the
        // Sustain < Tame < Corral ladder. Same `biomass_before_regrowth` basis and
        // `carrying_capacity` the wild `ceiling` closure uses, so the ONLY difference from Sustain is
        // the pen ecology's boosted `r`. **The actual pen take is now the ordinary escapement draw**,
        // which at the settled operating point is exactly this — quote and payout are one number
        // again rather than two shapes that happened to coincide.
        managed_yield: hunt_yield.apply(pen_msy_biomass, output_multiplier),
        // The Tame rung's PAYOFF (the pastoral analog of `managed_yield` above): the pastoral
        // **sustained MSY** — what a Sustain hunt pays once this herd is tamed — projected for a
        // still-wild herd on the same basis as Sustain, so the only difference is the pastoral `r`.
        // `ceiling_tame` is the during-building dip; this is the `→ +Y` the client renders. A wild
        // herd whose species never tames (`wild` ceiling) reads its wild MSY here, which is fine — the
        // client only surfaces it on the Tame affordance, hidden on a non-tameable herd.
        // **A herd past the pastoral rung advertises NO Tame payoff.** This is an *affordance*
        // readout, not a payout — which rung the client may still offer — so it survives the managed
        // harvest's retirement unchanged: a penned herd cannot be tamed, and quoting what taming
        // would have paid on ground you have already fenced is an offer that is not on the table.
        pastoral_yield: if herd.is_corralled() {
            NO_PASTORAL_YIELD
        } else {
            hunt_yield.apply(pastoral_msy_biomass, output_multiplier)
        },
        // **The biomass both quotes above are the conversion of** — stated once and handed over, so
        // the material account (which has no currency to scale off on an inedible species) reads the
        // *same* harvest the food quote does rather than a second `sustainable_yield` call.
        managed_yield_biomass: pen_msy_biomass,
        pastoral_yield_biomass: pastoral_msy_biomass,
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

/// **WHAT A CREW MAY ACTUALLY TAKE THIS TURN** — [`escapement_ceiling`], or the share of this turn's
/// growth the player's own floor says they were willing to take, **whichever is larger**.
///
/// # ⛔ WHY THE ESCAPEMENT ROOM ALONE WAS NOT ENOUGH
///
/// The floor is a fraction of `K`, and **a rung raises `K`** — a tamed herd's `pastoral_density`, a
/// sown field's `field_capacity_gain`. So `floor · K` climbs while the stock does not, and a source
/// standing *exactly* on its floor when a build starts is pushed **below** it by its own
/// improvement. Measured on an aurochs tame begun on the floor (`stance_probe`'s
/// `probe_the_tame_floor_squeeze`): the room reached zero on turn 6 at one herder, turn 3 at four and
/// turn 2 at eight — **more hands made it worse** — and because the build's own gate reads this same
/// room, the tame then stalled and **never completed at any crew size**.
///
/// # THE BACKSTOP IS THE FLOOR ITSELF, NOT A NEW DIAL
///
/// `growth × (1 − floor)`: **you keep the share of the growth you were willing to take.** The dial
/// the player already set governs it, so there is nothing new to tune and nothing new to validate
/// beyond [`crate::components::floor_is_valid`]'s existing bound.
///
/// It is what makes the two degenerate ends survive **without a special case**:
/// - `floor = 1.0` → `× 0` → the backstop pays **nothing**, so *"leave the whole source standing"*
///   keeps meaning exactly that — at the take **and** at the build gate, continuously rather than by
///   a branch. This is the property `a_full_floor_takes_nothing_and_refuses_the_build` pins.
/// - `floor = 0` → the room is the whole stock and the `max` never selects the backstop.
///
/// **It is NOT a second definition of the take.** The build's eligibility reads this same number
/// (`systems::labor`'s `source_is_workable`), which is what makes *"a legal build target that yields
/// nothing"* unrepresentable rather than merely avoided — and keeps it that way if either term is
/// retuned.
///
/// `growth` is the source's **realized** growth this turn, not a projection: Logistics regrows
/// before Population takes, so by take time it has already happened
/// ([`Herd::growth_this_turn`]). A negative or absent growth contributes nothing.
pub fn take_room(floor_fraction: f32, biomass: f32, carrying_capacity: f32, growth: f32) -> f32 {
    escapement_ceiling(floor_fraction, biomass, carrying_capacity)
        .max(growth_share(floor_fraction, growth))
}

/// **THE SHARE OF THIS TURN'S GROWTH THE FLOOR LEAVES TAKEABLE** — `growth × (1 − floor)`, floored at
/// zero on both terms. Split out from [`take_room`] so the one sentence the model is
/// (*"you keep the share of the growth you were willing to take"*) has a name, and so the
/// `floor = 1.0 → 0` identity is testable on its own.
pub fn growth_share(floor_fraction: f32, growth: f32) -> f32 {
    growth.max(0.0) * (WHOLE_STOCK - floor_fraction.clamp(0.0, WHOLE_STOCK))
}

/// **A floor that leaves the WHOLE stock standing** — the top of the floor's range, and the value at
/// which [`growth_share`] pays nothing. Named rather than a bare `1.0` because it is the *meaning* of
/// the bound `floor_is_valid` enforces, not an arbitrary clamp.
const WHOLE_STOCK: f32 = 1.0;

/// Max Sustainable Yield ceiling: regrowth evaluated at the most-productive biomass (K/2),
/// so a resource AT carrying capacity still has a positive sustainable harvest (Sustain draws it
/// down to K/2 and holds it there). Below the Allee threshold this is 0 (don't harvest a
/// collapsing resource — inherited from net_biomass_delta's negative branch, clamped). Distinct
/// from net_biomass_delta, which stays the ACTUAL per-turn biomass change used by regrow_biomass.
pub(crate) fn sustainable_yield(biomass: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    net_biomass_delta(biomass.min(cap * MSY_BIOMASS_FRACTION), cap, ecology).max(0.0)
}

// **RETIRED: `peak_regrowth`** — the shared MSY-at-unit-capacity helper, whose last caller was the
// plant ladder's config check (`labor_config::peak_regrowth_per_capacity`). That comparison no longer
// needs it: see the gravestone there.

/// **The biggest one-turn regrowth anywhere in a band of standing stock** — the rate a crew has to
/// out-take to descend *through* that band, and the ability half of
/// [`crate::components::take_overdraws`].
///
/// **It is exact, not sampled.** Both webs' curves are logistic above their low-stock branch, so the
/// only interior maximum either can have is the food peak at `MSY_BIOMASS_FRACTION × cap`; every
/// other piece is monotone (a herd's depensation decline falls with biomass, a patch's reseed lift
/// falls with biomass, the logistic rises below the peak), so its maximum sits on an endpoint.
/// Evaluating the three candidates therefore finds the true peak, without the interpolation error the
/// wire's [`crate::snapshot::REGROWTH_CURVE_SAMPLES`] curve carries — that one is a *display*
/// resolution and must not be what a verdict turns on.
///
/// `regrowth_at` is the source's own one-turn biomass delta at a given standing stock, which is why
/// this takes a closure rather than an [`EcologyConfig`]: the plant curve and the animal curve are
/// two different functions, exactly as `snapshot::patch_regrowth_samples` / `herd_regrowth_samples`
/// are.
pub(crate) fn peak_regrowth_between(
    cap: f32,
    low: f32,
    high: f32,
    regrowth_at: impl Fn(f32) -> f32,
) -> f32 {
    if !cap.is_finite() || cap <= 0.0 {
        return 0.0;
    }
    let lo = low.min(high).clamp(0.0, cap);
    let hi = low.max(high).clamp(0.0, cap);
    let peak_stock = MSY_BIOMASS_FRACTION * cap;
    let mut peak = regrowth_at(lo).max(regrowth_at(hi));
    if (lo..=hi).contains(&peak_stock) {
        peak = peak.max(regrowth_at(peak_stock));
    }
    peak
}

/// **The band a take has to cross to reach its floor** — `floor·K` up to the stock standing today,
/// as `(low, high)` for [`peak_regrowth_between`].
///
/// **Anchored at the floor, never below it.** A source already sitting under its floor is handing
/// over nothing; what decides whether the crew will *hold* it there once it grows back is the
/// regrowth at the floor itself, so the band collapses onto `floor·K` rather than reaching down to a
/// stock the crew is not taking from.
pub(crate) fn floor_reach_band(floor: f32, biomass: f32, cap: f32) -> (f32, f32) {
    let floor_stock = floor * cap;
    (floor_stock, biomass.max(floor_stock))
}

/// **THE CURVE THIS HERD IS ON, AT ANY STOCK** — the one place the choice between the two growth
/// models is made, so a reader and the pass that applies it cannot sample different curves.
///
/// **A managed group is immune to the overhunting collapse**: it regrows logistically toward
/// capacity at every stock and never crosses into the depensation crash, where a wild one below its
/// `collapse_fraction` is *losing* biomass. Anything sampling the wild curve for a tamed or penned
/// herd reads a negative where the real answer is positive — which is how the overdraw ⚠ came to fire
/// on a crew that cannot draw its herd down.
///
/// **The per-turn modifiers are NOT here** — [`regrow_biomass`] applies the pen's feed fraction and
/// the abandoned-pastoral gate on top, because those describe *this turn's* keeping rather than the
/// shape of the curve. A forecast sampling the curve at a stock the herd is not standing on has no
/// business assuming either.
pub fn regrowth_delta_at(herd: &Herd, stock: f32, cap: f32, ecology: &EcologyConfig) -> f32 {
    if herd.is_domesticated() {
        logistic_regrowth(stock, cap, ecology.regrowth_rate)
    } else {
        net_biomass_delta(stock, cap, ecology)
    }
}

/// **Can a crew of `workers` hunters draw THIS herd to `floor`, and is that floor below the food
/// peak?** — the animal web's producer of [`SourceYield::overdraws`], and the only thing the Hunt
/// arms (resolved and seeded) publish that flag through.
///
/// `biomass` is the herd's **pre-take** standing stock — what this turn's crew is facing, the same
/// term `sustainable` on the row is computed at.
///
/// **The crew's throughput is `min(carry, reach)`**, the two bounds `hunt_take` itself pays: what the
/// party can haul home ([`hunt_take_workers`]'s haul half) and what it can bring into contact in a
/// turn ([`animals_engaged`] × the retreat's [`HuntingParty::stay_fraction`] × a body). Sizing on
/// carry alone would call a two-hunter party capable of drawing down a herd of fowl it can barely
/// touch — the same error [`hunt_engage_workers`] exists to keep out of the crew counts.
///
/// **The FIGHT is deliberately not a fourth bound.** Its damage accumulates across turns
/// (`project_realized_hunt` resolves it *inside* the loop for exactly that reason), so it has no
/// per-turn rate to compare a regrowth against. Leaving it out can only make the crew look *more*
/// capable, which leaves the ⚠ lit in the cases a fight would have blocked — the same subtractive
/// direction the rest of this predicate runs in.
pub fn hunt_take_overdraws(
    herd: &Herd,
    fauna: &FaunaConfig,
    biomass: f32,
    per_worker_biomass_capacity: f32,
    party: &HuntingParty,
    workers: u32,
    floor: f32,
) -> bool {
    let cap = herd_capacity(herd, fauna);
    let ecology = herd_ecology(herd, fauna);
    let carry = workers as f32 * per_worker_biomass_capacity.max(0.0);
    // What the party puts on the ground in a turn: reached, less what breaks off, in biomass.
    let reach = animals_engaged(workers, fauna.engage_rate_for(&herd.species))
        * party.stay_fraction(fauna.wariness_for(&herd.species))
        * herd.body_mass.max(0.0);
    let (low, high) = floor_reach_band(floor, biomass, cap);
    take_overdraws(
        floor,
        carry.min(reach),
        // **THE HERD'S OWN CURVE** ([`regrowth_delta_at`]), never the wild one by default: a tamed
        // or penned herd below its `collapse_fraction` regrows where a wild one crashes, so the wild
        // sample under-reports the peak and lights the ⚠ on a crew that cannot draw it down.
        peak_regrowth_between(cap, low, high, |stock| {
            regrowth_delta_at(herd, stock, cap, &ecology)
        }),
    )
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
    // **Through [`regrowth_delta_at`]**, which every reader of the curve shares — the overdraw
    // predicate kept its own copy of this choice and lost the domesticated arm out of it.
    let delta = regrowth_delta_at(herd, herd.biomass, cap, &ecology);
    // **The pen's growth is what the FEED buys.** A penned herd cannot graze, so an unfed one does not
    // grow at all (`docs/plan_corral_managed_population.md` §3.1: *fed → regrow; underfed → shrink*) —
    // its growth scales with the fraction of last turn's feed its keeper actually paid, and
    // `advance_husbandry` then applies the wasting on top. Without this the pen's own `r` = 0.60
    // out-runs the 10%/turn starvation four times over: an "unfed" herd would keep growing, park at
    // `K/2`, and quietly pay its keeper a yield for feed they never bought.
    // `pen_fed_fraction` is 1.0 for every herd that is not penned, so this is inert elsewhere.
    let delta = delta * herd.pen_fed_fraction.clamp(0.0, PEN_FULLY_FED);
    // **RETIRED: the totally-abandoned-pastoral growth freeze.** An owned, unfenced herd whose
    // keeping went wholly unmet last turn used to have its growth zeroed outright, so the shed could
    // drive it to the extinction floor.
    //
    // **A HERD'S GROWTH IS A FACT ABOUT THE LAND IT STANDS ON, NOT ABOUT WHO IS WATCHING IT.**
    // Animals eat and breed whether or not anyone is paid to mind them; the price of not keeping a
    // herd is that it **leaves** (`shed_uncontained_animals`), and that is the whole of it. A second
    // penalty on the same trigger made neglect cost twice and made the two impossible to tune apart.
    // (A feed term may scale this later — a fed pen already does, one line above — but that is a
    // model about fodder, not about labour.)
    //
    // **Going feral survives the deletion, and it is the shed that does it.** A wholly unkept herd is
    // short by its whole demand, so the shed takes `pastoral_escape_fraction` of the herd every turn
    // past the grace — which outruns the pastoral growth curve at every biomass, so the herd runs
    // down to nothing and `advance_husbandry` clears its ownership on shed-to-zero. There is no leaky
    // equilibrium for the freeze to have been protecting against; the numbers are in
    // `tests/fauna_husbandry.rs`'s `probe_the_abandoned_herds_fate`.
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
        // **The wire keeps the 0..1 fraction; the source keeps ONE position.** Read off the
        // standing (`Herd::rung_fraction`), so a tamed herd reads exactly `1.0` beside an
        // `is_domesticated()` that is already true — and the pen reads `0` until its fence closes,
        // because `animal:pen` is `on_completion` and its credit is zero while it builds.
        domestication: herd.rung_fraction(RungKey::AnimalPastoral),
        corralled: herd.is_corralled(),
        corral_progress: herd.rung_fraction(RungKey::AnimalPen),
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
    use crate::fauna_config::ShoreRequirement;
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

    /// **The raw keeper count the ladder would answer** for this herd at `animals_per_herder` — the
    /// one definition (`raw_herders_needed`), reached without an ECS world so the hysteresis tests
    /// can exercise the real number rather than a second `ceil` of their own.
    fn raw_for(herd: &Herd, animals_per_herder: f32) -> u32 {
        LadderConfig::builtin()
            .rung(RungKey::AnimalPastoral)
            .upkeep_crew_needed(herd_keeper_loads(
                herd.biomass,
                herd.body_mass,
                animals_per_herder,
            ))
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
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
            2
        );
        // Now oscillate 13 → 11 → 12 → 13 (the lumpy Sustain kill): it must HOLD at 2 the whole way,
        // never flickering back to 1.
        for heads in [11.0_f32, 12.0, 13.0, 11.0, 13.0] {
            herd.biomass = heads;
            assert_eq!(
                herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
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
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
            1
        ); // ceil(12/12) = 1
        herd.biomass = 25.0; // clearly a third herder's worth (ceil(25/12) = 3)
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
            3
        );
    }

    /// The requirement drops only after a CLEAR fall — past the lower rung's ceiling by more than the
    /// deadband — not on a one-animal dip.
    #[test]
    fn herder_requirement_drops_only_after_a_clear_fall() {
        const APH: f32 = 12.0;
        const BAND: f32 = APH * 0.25; // 3 animals
        let mut herd = managed_herd_with_heads(20.0);
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
            2
        ); // ceil(20/12) = 2
           // Just below the 1-herder ceiling (12) but within the deadband: 10 > 12 − 3 = 9 → HOLD at 2.
        herd.biomass = 10.0;
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
            2
        );
        // Below the deadband floor (≤ 9): a genuine drop → step down to ceil(8/12) = 1.
        herd.biomass = 8.0;
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, BAND),
            1
        );
    }

    /// A wild herd isn't yours to maintain — it stays `0`, and `herd_herders_needed` reads `0`.
    #[test]
    fn a_wild_herd_needs_no_herders() {
        const APH: f32 = 12.0;
        let mut herd = managed_herd_with_heads(50.0);
        herd.owner = None; // wild again
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, APH * 0.25),
            0
        );
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

        // Each case: (ceiling, escapement room, collection, stayed, brought_down) with exactly one
        // term tight. **The first two differ only in the room**, which is the whole point of the
        // split: the same four-animal ceiling is the herd's limit when the herd has no more to spare
        // and the party's when it has.
        let cases = [
            (
                4.0 * BODY,
                4.0 * BODY,
                SLACK,
                SLACK,
                SLACK,
                HuntTakeBound::Floor,
            ),
            (
                4.0 * BODY,
                SLACK,
                SLACK,
                SLACK,
                SLACK,
                HuntTakeBound::Throughput,
            ),
            (SLACK, SLACK, 4.0 * BODY, SLACK, SLACK, HuntTakeBound::Carry),
            // Reached ten, put four on the ground — the fight is the shortfall.
            (SLACK, SLACK, SLACK, 10.0, 4.0, HuntTakeBound::Fight),
            // Reached four and killed all four — reach is the shortfall.
            (SLACK, SLACK, SLACK, 4.0, 4.0, HuntTakeBound::Engagement),
        ];
        for (ceiling, room, collection, stayed, brought_down, expected) in cases {
            assert_eq!(
                hunt_take_bound(
                    ceiling,
                    room,
                    collection,
                    BODY,
                    stayed,
                    brought_down,
                    EngagementStop::WhenPackFull
                ),
                expected,
                "({ceiling}, {room}, {collection}, {stayed}, {brought_down}) must name {expected:?}"
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
                // Both ceiling-side bounds relax the same way — they differ in *whose* ceiling it is,
                // not in which term the quantiser read.
                HuntTakeBound::Floor | HuntTakeBound::Throughput => quantise_animal_take(
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
                // The herd's room IS the ceiling here — a resident band's wait turn, where half a
                // body standing above the floor is genuinely all there is.
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

    /// **The retreat draw and the fight's strike draws must be INDEPENDENT** — §4.7's variance is
    /// binomial in force size, which is a claim about two stages that do not know about each other.
    ///
    /// # Why this is measured rather than asserted
    ///
    /// Every draw site reseeds a *fresh* `SmallRng` from a `u64`; nothing passes one advancing stream
    /// along. So a fight handed the retreat's own seed replays the **same uniforms in the same
    /// order**, and `gen_bool(wariness)` / `gen_bool(hit_chance)` become two thresholds on one
    /// uniform — nested events, not independent ones. "They use different code paths" is exactly the
    /// reasoning that would miss it, so this test computes the Pearson correlation of the two stages'
    /// counts across many events and requires it to be near zero.
    ///
    /// **The unsalted arm is the liveness half, and it is not decoration**: it re-runs the identical
    /// experiment with the retreat's raw seed handed to the fight — what the code did before
    /// [`FIGHT_SEED_SALT`] — and requires a *large* correlation. Without it a bug that made either
    /// stage constant would sail through the first assertion with `r ≈ 0`.
    ///
    /// **Measured**: the shared-stream arm reads `r = −1.000` — with `wariness == hit_chance` the
    /// two indicators are *exact complements* of one uniform, so `landed == engaged − stayed` every
    /// event — and the salted arm reads `r = +0.002`. The coupling is total, not marginal.
    ///
    /// It runs at `hit_chance 0.5`, deliberately **not** the shipped `1.0`: at `1.0`
    /// [`crate::combat::attacks_landed`] returns before it draws, so the whole defect is inert and
    /// unmeasurable — which is precisely why it survived to be found by review.
    #[test]
    fn the_retreat_and_the_fight_draw_from_independent_streams() {
        /// Enough events for the sample correlation's own noise (`≈ 1/√n`) to sit an order of
        /// magnitude under [`INDEPENDENT_CORRELATION`].
        const EVENTS: u64 = 4_000;
        /// The one map every event is drawn on; the tick is what moves.
        const MAP_SEED: u64 = 0x51A5_11ED_1234_5678;
        /// Both stages take the same *count* so their draws line up one-for-one — the alignment that
        /// makes a shared stream maximally correlated, i.e. the worst case rather than a lucky one.
        const ENGAGED: f32 = 24.0;
        /// Mid-range on both dials: a Bernoulli's variance peaks at `0.5`, so any coupling shows.
        const WARINESS: f32 = 0.5;
        const HIT_CHANCE: f32 = 0.5;
        /// What "decorrelated" is allowed to mean for a sample of [`EVENTS`] draws.
        const INDEPENDENT_CORRELATION: f64 = 0.1;
        /// What the shared-stream arm must exceed for this test to have teeth.
        const COUPLED_CORRELATION: f64 = 0.5;

        let tuning = combat::CombatTuning {
            hit_chance: HIT_CHANCE,
            draw: combat::StrikeDraw::Seeded,
            ..Default::default()
        };
        // Pearson's r over the (stayed, landed) pairs the two stages produce for one seed each.
        let correlation = |fight_seed: fn(HuntDraw) -> u64| {
            let samples: Vec<(f64, f64)> = (0..EVENTS)
                .map(|event| {
                    // One map, one party, one turn per event — `retreat_seed` XORs its map seed and
                    // its tick, so varying *both* together would cancel and hold the seed constant.
                    let draw = HuntDraw::Seeded(retreat_seed(MAP_SEED, event, "game_deer_1", 12));
                    let stayed = animals_that_stay(ENGAGED, WARINESS, draw);
                    let landed = combat::landed_strikes_seeded(ENGAGED, &tuning, fight_seed(draw));
                    (f64::from(stayed), f64::from(landed))
                })
                .collect();
            let n = samples.len() as f64;
            let mean = |pick: fn(&(f64, f64)) -> f64| samples.iter().map(pick).sum::<f64>() / n;
            let (mean_x, mean_y) = (mean(|s| s.0), mean(|s| s.1));
            let mut covariance = 0.0;
            let (mut var_x, mut var_y) = (0.0, 0.0);
            for (x, y) in &samples {
                covariance += (x - mean_x) * (y - mean_y);
                var_x += (x - mean_x).powi(2);
                var_y += (y - mean_y).powi(2);
            }
            assert!(
                var_x > 0.0 && var_y > 0.0,
                "both stages must actually vary, or a correlation of 0 would prove nothing \
                 (var_stayed={var_x}, var_landed={var_y})"
            );
            covariance / (var_x * var_y).sqrt()
        };

        // The bug: the retreat's own seed handed straight to the fight, as `HuntDraw::seed` did.
        let coupled = correlation(|draw| match draw {
            HuntDraw::Seeded(seed) => seed,
            HuntDraw::Quantile { .. } => FORECAST_FIGHT_SEED,
        });
        // The fix: the same event, salted apart.
        let salted = correlation(HuntDraw::seed);

        assert!(
            coupled.abs() > COUPLED_CORRELATION,
            "the shared-stream arm must stay strongly correlated or this test proves nothing; \
             measured r={coupled}"
        );
        assert!(
            salted.abs() < INDEPENDENT_CORRELATION,
            "the retreat and the fight must decorrelate once salted apart; measured r={salted} \
             (shared-stream arm r={coupled})"
        );
    }

    // ---- The engagement bound ------------------------------------------------------------------

    /// A quarry a whole hunter can only partly corner in a turn — `workers × rate < 1` for any small
    /// party, which is the case the `max(1.0)` floor exists for.
    const HARD_TO_CORNER_ENGAGE_RATE: f32 = 0.25;
    /// A quarry a hunter reaches two of per turn — the linear-scaling fixture.
    const EASY_ENGAGE_RATE: f32 = 2.0;
    /// **Half a party** — the fixture that used to be a build dip and then a production share. It
    /// is now simply *a smaller crew*, which is the whole of what the model has to say about a band
    /// splitting its hands between hunting and building.
    fn half_of(crew: u32) -> u32 {
        crew / 2
    }

    /// **A fractional engagement reaches one animal, not zero**
    /// (`docs/plan_hunt_through_combat.md` §10). A small band cannot corner the quarry *efficiently*;
    /// it can still walk up to it, and the gate on whether it survives the meeting is the fight, not
    /// a headcount threshold in front of it.
    #[test]
    fn a_party_too_small_to_corner_one_animal_still_engages_one() {
        for workers in 1..=3u32 {
            let engaged = animals_engaged(workers, HARD_TO_CORNER_ENGAGE_RATE);
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
        assert_eq!(animals_engaged(8, HARD_TO_CORNER_ENGAGE_RATE), 2.0);
    }

    /// **No workers engage NOTHING** — a different statement from the fractional floor above, and
    /// pinned separately: the `max(1.0)` must not manufacture a hunter out of an unstaffed row.
    #[test]
    fn a_party_of_no_workers_engages_nothing() {
        for rate in [HARD_TO_CORNER_ENGAGE_RATE, EASY_ENGAGE_RATE, f32::INFINITY] {
            assert_eq!(
                animals_engaged(0, rate),
                0.0,
                "an unstaffed row reaches no animals (rate {rate})"
            );
        }
    }

    /// **ENGAGEMENT SCALES WITH THE CREW, linearly** — half the hunters reach half the animals.
    /// That is what makes *"put hands on the build instead"* a real cost with nothing else in the
    /// arithmetic: the retired `build_dip` factor said the same thing about a *shared* crew, and the
    /// defect it guarded (a build riding a hunting party for free) cannot recur when the two crews
    /// are separate numbers. Asserted where the count is `>= 2`, so the `max(1.0)` floor cannot mask
    /// the difference.
    #[test]
    fn engagement_scales_with_the_hunting_crew() {
        const CREW: u32 = 8;
        let whole_party = animals_engaged(CREW, EASY_ENGAGE_RATE);
        let half_party = animals_engaged(half_of(CREW), EASY_ENGAGE_RATE);
        // Liveness: both crews genuinely reach several animals, so neither reading is the floor.
        assert_eq!(whole_party, CREW as f32 * EASY_ENGAGE_RATE);
        assert_eq!(half_party, half_of(CREW) as f32 * EASY_ENGAGE_RATE);
        assert!(
            half_party >= 2.0 && half_party < whole_party,
            "half the hunters must reach strictly fewer: {half_party} vs {whole_party}"
        );
    }

    /// **Engagement is linear in party size** — twice the hunters reach twice the animals, which is
    /// what makes party size the lever the take responds to.
    #[test]
    fn engagement_scales_linearly_with_the_party() {
        for workers in 1..=6u32 {
            assert_eq!(
                animals_engaged(workers, EASY_ENGAGE_RATE),
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

        let engaged = animals_engaged(HUNTERS, ONE_ANIMAL_PER_HUNTER);
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
    /// A quarry that never breaks off — the retreat's identity, so a case exercising only reach and
    /// dip reads exactly as it did before the retreat entered the crew.
    const NOTHING_BREAKS_OFF: f32 = 1.0;
    /// A quarry that keeps three animals in four — the shipped Wild Boar's `wariness 0.25` at the
    /// neutral spear dispersion.
    const BOAR_STAY: f32 = 0.75;

    /// **The engagement crew is the exact inverse of the per-hunter BRING-DOWN rate** — the smallest
    /// party whose `engage_rate × dip × stay` covers the whole peak drop, and one hunter short of it
    /// does not. That exactness is the property: a crew count that merely correlated with the rate
    /// would let the panel name a number the stepper's own take does not clear.
    ///
    /// The retreat is one of the three factors, so the inverse is of the **closed-form** rate rather
    /// than of [`animals_engaged`] — which answers how many the party gets *near*, strictly more than
    /// it kills wherever a species has any wariness. The engagement itself is still asserted to cover
    /// the drop, because you cannot bring down what you never reached.
    #[test]
    fn the_engagement_crew_is_the_smallest_party_that_brings_down_the_peak_drop() {
        for rate in [HARD_TO_CORNER_ENGAGE_RATE, EASY_ENGAGE_RATE, 10.0] {
            {
                for stay in [NOTHING_BREAKS_OFF, BOAR_STAY] {
                    let peak = peak_animal_drop(ROOMY_CEILING, LIGHT_BODY);
                    let crew = hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, rate, stay);
                    let brought_down = |hands: u32| hands as f32 * rate * stay;
                    assert!(
                        crew > 1,
                        "rate {rate} stay {stay}: fixture must need a real crew"
                    );
                    assert!(
                        brought_down(crew) >= peak,
                        "rate {rate} stay {stay}: {crew} hunters must put the whole \
                         {peak}-animal drop down, they put down {}",
                        brought_down(crew)
                    );
                    assert!(
                        brought_down(crew - 1) < peak,
                        "rate {rate} stay {stay}: …and {} must not — the count has to be \
                         the SMALLEST such crew, not merely a sufficient one",
                        crew - 1
                    );
                    assert!(
                        animals_engaged(crew, rate) >= peak,
                        "rate {rate} stay {stay}: …and it must REACH the drop too — you \
                         cannot bring down what you never got near"
                    );
                }
            }
        }
    }

    // **RETIRED: `a_building_crew_needs_more_hands_to_reach_the_same_drop`** — the engagement crew's
    // half of the build dip, which said a gentling party needed proportionally more hands to corner
    // the same drop because its throughput was scaled down.
    //
    // The dip is gone and so is the shared crew: a build is staffed in its own right
    // (`docs/plan_standing_upkeep.md` §2.2), so the hunters this count sizes are only ever hunters
    // and there is no second reading of "the crew" for it to disagree with. What survives of the
    // claim is `engagement_scales_with_the_hunting_crew` above — the linearity the count inverts.

    /// **A source with NO ENGAGEMENT STAGE reports no engagement crew** — a pen and the plant web
    /// both forecast `f32::INFINITY` ([`SourceYieldForecast::managed`],
    /// [`FaunaConfig::engage_rate_for`]), so the `max()` collapses to the haul term and neither
    /// regresses. This is the no-regress half of the pair below.
    ///
    /// **Byte-identical whatever the retreat says**, which is what makes
    /// [`NO_RETREAT_STAGE_STAY`]'s neutral safe on the `fight: None` branch: an infinite reach times
    /// any stay is still not a finite rate, so the term cannot start speaking on a source that has
    /// no engagement stage to speak about.
    #[test]
    fn a_source_with_no_engagement_stage_keeps_exactly_its_haul_crew() {
        let haul = hunt_haul_workers(ROOMY_CEILING, LIGHT_BODY, HUNTER_CARRY);
        assert!(haul > 0, "liveness: the haul crew is a real count");
        {
            for stay in [NO_RETREAT_STAGE_STAY, BOAR_STAY, NOTHING_STANDS] {
                assert_eq!(
                    hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, f32::INFINITY, stay),
                    0,
                    "an unstalked source owes no engagement crew (stay {stay})"
                );
                assert_eq!(
                    hunt_take_workers(ROOMY_CEILING, LIGHT_BODY, HUNTER_CARRY, f32::INFINITY, stay),
                    haul,
                    "…so the take crew is the haul crew, unchanged (stay {stay})"
                );
            }
        }
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
            NOTHING_BREAKS_OFF,
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
            NOTHING_BREAKS_OFF,
        );
        let carry_haul = hunt_haul_workers(ROOMY_CEILING, HEAVY_BODY, HUNTER_CARRY);
        let carry_reach =
            hunt_engage_workers(ROOMY_CEILING, HEAVY_BODY, FAST_REACH, NOTHING_BREAKS_OFF);
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

    // ---- The retreat prices the crew (`docs/plan_hunt_through_combat.md` §3) ---------------------

    /// A Wild Boar's shipped body mass, in the whole numbers the case is stated in.
    const BOAR_BODY: f32 = 12.0;
    /// A Wild Boar's shipped `engage_rate` — one hunter gets near a third of a boar per turn.
    const BOAR_ENGAGE_RATE: f32 = 0.33;
    /// A room of 27 whole boar (`27 × 12`), so the peak drop is the 28 that room's last partial body
    /// covers — `floor(324 / 12) + 1`.
    const BOAR_ROOM: f32 = 27.0 * BOAR_BODY;
    /// A quarry nothing stands against — [`stay_fraction`] at `wariness 1`, or a kit so loud it
    /// scatters everything.
    const NOTHING_STANDS: f32 = 0.0;

    /// **THE WILD BOAR CASE — the crew is sized on what the party puts DOWN, not on what it gets
    /// near.** One hunter reaches `0.33` boar a turn and keeps three in four of them, so they land
    /// `0.33 × 0.75 = 0.2475` boar. Clearing a 28-animal peak drop therefore takes
    /// `ceil(28 / 0.2475) = 114` hunters, and the retreat-blind reading (`ceil(28 / 0.33) = 85`) is
    /// short by a third.
    ///
    /// This is the contradiction the change exists to remove: the compose sheet's *clear it now*
    /// target already divided the room by the retreat-aware rate while the stepper cap beside it
    /// divided by the raw reach, so the sheet named a crew the panel refused to let the player
    /// assign. The sharp half is asserted rather than the arithmetic alone — the raw-reach crew
    /// demonstrably leaves the herd short.
    #[test]
    fn a_wary_boar_herd_needs_the_hands_the_retreat_costs() {
        const PEAK_DROP: f32 = 28.0;
        const RETREAT_AWARE_CREW: u32 = 114;
        const RAW_REACH_CREW: u32 = 85;

        assert_eq!(
            peak_animal_drop(BOAR_ROOM, BOAR_BODY),
            PEAK_DROP,
            "the fixture's room must be the 28-animal drop the numbers below are derived from"
        );
        let crew = hunt_engage_workers(BOAR_ROOM, BOAR_BODY, BOAR_ENGAGE_RATE, BOAR_STAY);
        assert_eq!(
            crew, RETREAT_AWARE_CREW,
            "28 boar at 0.2475 down per hunter is {RETREAT_AWARE_CREW} hands, not {crew}"
        );
        assert_eq!(
            hunt_engage_workers(BOAR_ROOM, BOAR_BODY, BOAR_ENGAGE_RATE, NOTHING_BREAKS_OFF,),
            RAW_REACH_CREW,
            "…and the raw reach is what the crew used to be sized on"
        );
        // The sharp form: the retreat-blind crew cannot draw the room down, and the sized one can.
        let brought_down = |hands: u32| hands as f32 * BOAR_ENGAGE_RATE * BOAR_STAY;
        assert!(
            brought_down(RAW_REACH_CREW) < PEAK_DROP,
            "{RAW_REACH_CREW} hunters put {} boar down against a {PEAK_DROP}-animal drop — the herd \
             is left short, which is why the old count was not merely a different convention",
            brought_down(RAW_REACH_CREW)
        );
        assert!(
            brought_down(RETREAT_AWARE_CREW) >= PEAK_DROP,
            "…and {RETREAT_AWARE_CREW} clears it"
        );
    }

    /// **A quarry that never stands owes NO crew, and `0` is the answer rather than a fudge.** At a
    /// stay of zero the take is identically zero at every party size — no number of hands changes it
    /// — so the crew *needed to achieve the take* is none. Asserted as that reasoning: the take at
    /// the largest crew the function could have named is still nothing.
    #[test]
    fn a_quarry_that_never_stands_needs_no_crew_because_no_crew_can_take_it() {
        for rate in [HARD_TO_CORNER_ENGAGE_RATE, EASY_ENGAGE_RATE] {
            assert_eq!(
                hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, rate, NOTHING_STANDS),
                0,
                "rate {rate}: nothing stands, so no crew achieves the take"
            );
        }
        // The reasoning, not merely the return: a huge party still brings down nothing, so there is
        // no crew size the count could honestly have named.
        const AN_ABSURDLY_LARGE_PARTY: f32 = 1_000_000.0;
        assert_eq!(
            AN_ABSURDLY_LARGE_PARTY * EASY_ENGAGE_RATE * NOTHING_STANDS,
            0.0,
            "the premise: at stay 0 the take is zero however many hands are sent"
        );
        // …and the haul term still speaks, so the `max()` has not been switched off.
        assert_eq!(
            hunt_take_workers(
                ROOMY_CEILING,
                LIGHT_BODY,
                HUNTER_CARRY,
                EASY_ENGAGE_RATE,
                NOTHING_STANDS,
            ),
            hunt_haul_workers(ROOMY_CEILING, LIGHT_BODY, HUNTER_CARRY),
            "the take crew collapses to the haul term, it does not collapse to zero"
        );
    }

    /// **Monotone in the retreat: a wary quarry never needs FEWER hands than a calm one** at the same
    /// reach. Without this the crew could be *any* function of the stay and still pass the boar case;
    /// the direction is what makes it a model rather than a fitted number.
    #[test]
    fn a_warier_quarry_never_needs_fewer_hands() {
        // Descending stay = ascending wariness. `NOTHING_STANDS` is excluded deliberately: it is the
        // one rung where the crew drops to `0`, and it does so because the take drops to `0` with it.
        const DESCENDING_STAY: [f32; 5] = [1.0, 0.9, 0.75, 0.4, 0.15];
        for rate in [HARD_TO_CORNER_ENGAGE_RATE, EASY_ENGAGE_RATE] {
            let mut previous = 0;
            for stay in DESCENDING_STAY {
                let crew = hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, rate, stay);
                assert!(
                    crew >= previous,
                    "rate {rate}: a quarry keeping {stay} needs {crew} hands, fewer than the calmer \
                     quarry's {previous}"
                );
                previous = crew;
            }
            // Liveness: the sweep must actually climb, or a constant would pass it.
            assert!(
                previous > hunt_engage_workers(ROOMY_CEILING, LIGHT_BODY, rate, NOTHING_BREAKS_OFF),
                "rate {rate}: the wariest rung must cost strictly more than the calmest"
            );
        }
    }

    /// A zero deadband restores the raw stateless behaviour (the flicker) — the lever genuinely
    /// controls the hysteresis.
    #[test]
    fn a_zero_deadband_restores_the_raw_flicker() {
        const APH: f32 = 12.0;
        let mut herd = managed_herd_with_heads(13.0);
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, 0.0),
            2
        );
        herd.biomass = 12.0; // 12 ≤ (2−1)·12 − 0 = 12 → drops immediately with no band
        assert_eq!(
            herd.stabilize_herders_needed(raw_for(&herd, APH), APH, 0.0),
            1
        );
    }

    // `the_build_dip_is_applied_inside_the_standing_stock_clamp` was deleted with its subject: the
    // dip is no longer a term of `ceiling_at` at all (`docs/plan_harvest_floor.md` §3.1 moved it onto
    // crew throughput), so there is no ordering left between it and the standing-stock clamp.

    /// The food peak, which these forecast-shape tests use wherever the floor is not what varies.
    const PEAK_FLOOR: f32 = MSY_BIOMASS_FRACTION;

    // **RETIRED: `A_KEEPER`** — the single keeper the retired managed-source test staffed. It
    // existed only to show that a verb handed to a finished source cost its take crew nothing, and
    // that test went with the managed source itself.

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

    /// **EVERY RUNG IS DRAWN DOWN, INCLUDING RUNG 3 — the "managed source" is retired.**
    ///
    /// This asserted the opposite: a rung-3 source ignored the floor entirely and paid one managed
    /// production at every stance. **Production and draw are separate concerns; a rung may change
    /// production, no rung changes the draw** — so a pen and a Field are hunted and gathered through
    /// the same escapement path as the rungs beneath them, and `managed_production` is now always
    /// `None`.
    ///
    /// What replaced it is the pair below: the floor is **live** at rung 3, and it is live in the
    /// direction that matters — a higher escapement floor holds more back, so the ceiling falls.
    #[test]
    fn a_rung_three_source_is_drawn_down_like_every_other_rung() {
        const STOCK: f32 = 800.0;
        const CAPACITY: f32 = 1000.0;
        const RATE: f32 = 0.02;
        let forecast = SourceYieldForecast {
            managed_production: None,
            biomass: STOCK,
            carrying_capacity: CAPACITY,
            per_biomass_yield: plant_food_only(RATE),
            ..Default::default()
        };
        let mut previous: Option<f32> = None;
        for step in 0..=10 {
            let floor = step as f32 / 10.0;
            let ceiling = forecast.ceiling_at(floor).provisions;
            if let Some(previous) = previous {
                assert!(
                    ceiling <= previous,
                    "a rung-3 source's ceiling must fall as the floor rises: floor {floor} offered \
                     {ceiling} against the floor below it, which offered {previous}"
                );
            }
            previous = Some(ceiling);
        }
        // The liveness half: a ceiling that ignored the floor would satisfy the ordering above by
        // being constant, which is exactly the retired model.
        assert!(
            previous.is_some_and(|last| last < forecast.ceiling_at(0.0).provisions),
            "…and it must actually move across the sweep, or the floor is being ignored after all"
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

    // ---------------------------------------------------------------------------------------------
    // THE ANIMAL WEB'S ONE-POSITION LADDER (`docs/plan_standing_upkeep.md` §2.8/§4.10)
    // ---------------------------------------------------------------------------------------------

    /// **A herd part-way up a rung, seated exactly.** The fixture every interpolation test below
    /// stands on: it seats a real position through the one mutator, so the standing, the payouts and
    /// the bill all describe the same place — which is the whole point of collapsing the two meters.
    fn herd_at(
        fraction_of_pastoral: f32,
    ) -> (
        Herd,
        std::sync::Arc<FaunaConfig>,
        std::sync::Arc<LadderConfig>,
    ) {
        let fauna = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        let mut herd = herd_of_size(SizeClass::Big, 600.0, 1200.0, 0.05);
        herd.species = AUROCHS.to_string();
        herd.husbandry_ceiling = HusbandryCeiling::Pen;
        herd.taming_cost_multiplier = fauna.taming_cost_multiplier_for(&herd.species);
        let cost = herd.rung_cost(RungKey::AnimalPastoral, &ladder);
        herd.set_ladder_position(cost * fraction_of_pastoral, &ladder);
        herd.owner = Some(FactionId(0));
        (herd, fauna, ladder)
    }

    /// The species the playtest is quoted on — `pastoral_density 2.0`, wild `r` 0.09.
    const AUROCHS: &str = "Wild Aurochs";
    /// A tenth of the way up the pastoral rung — the position the cost claim is stated at.
    const A_TENTH_TAMED: f32 = 0.1;
    /// Half-way up it, where wild and pastoral are both a real distance away.
    const HALF_TAMED: f32 = 0.5;
    /// Nothing banked at all.
    const WILD: f32 = 0.0;
    /// The whole rung.
    const WHOLLY_TAMED: f32 = 1.0;

    /// **⛔ THE COST NO LONGER ARRIVES WHOLE ON DAY ONE.** `accrue_domestication` records an owner on
    /// its **first** call, and the retired `herd_keeping_meter` read *"owned ⇒ pastoral"* — so a herd
    /// one work unit into a 100-unit Tame owed the **entire** pastoral rate, and went on owing exactly
    /// that until the meter filled. 100% of the cost for 0% of the benefit, which is
    /// `docs/plan_standing_upkeep.md` §2.8's asymmetry inverted, and it was in the shipped game.
    #[test]
    fn a_tenth_tamed_herd_owes_about_a_tenth_of_the_pastoral_rate() {
        let (herd, fauna, ladder) = herd_at(A_TENTH_TAMED);

        // **THE PRECONDITION** — the position really is a tenth of the way up, so a pass cannot come
        // from the fixture having seated nothing (or everything).
        let standing = herd.standing();
        assert_eq!(standing.held, RungKey::AnimalWild);
        assert_eq!(standing.raising, Some(RungKey::AnimalPastoral));
        assert!(
            (standing.credit - A_TENTH_TAMED).abs() < 1e-4,
            "fixture: the herd must stand a tenth up the pastoral rung, not {}",
            standing.credit
        );

        let whole_rung = ladder
            .rung(RungKey::AnimalPastoral)
            .upkeep_demand(herd_keeper_load(&herd, &fauna));
        assert!(
            whole_rung > NO_UPKEEP_DEMAND,
            "fixture: the pastoral rung must cost something to hold, or the ratio is 0/0"
        );

        let owed = herd_upkeep_demand(&herd, &fauna, &ladder);
        assert!(
            (owed - whole_rung * A_TENTH_TAMED).abs() < 1e-4,
            "a herd a tenth into a Tame owes a tenth of the rate: {owed} against {}",
            whole_rung * A_TENTH_TAMED
        );
        assert!(
            (owed - whole_rung).abs() > 1e-3,
            "…and emphatically NOT the whole rate, which is what it owed before ({owed} against \
             {whole_rung})"
        );
    }

    /// **THE PAYOUTS SLIDE WITH THE POSITION** — the ceiling a herd's land holds and the rate it
    /// breeds at, both part-way between wild and pastoral at half-tamed. They stepped on
    /// `is_domesticated()` before, so a herd one unit short of tame bred and held exactly what a wild
    /// one did.
    #[test]
    fn a_half_tamed_herds_ceiling_and_breeding_sit_between_wild_and_pastoral() {
        let (wild, fauna, ladder) = herd_at(WILD);
        let (half, _, _) = herd_at(HALF_TAMED);
        let (tame, _, _) = herd_at(WHOLLY_TAMED);
        let _ = &ladder;

        // **THE PRECONDITION** — wild and pastoral must differ on both axes, or "between" is a
        // statement about one number and the test passes when the ladder collapses.
        let (wild_density, tame_density) = (
            herd_density_gain(&wild.standing(), &wild, &fauna),
            herd_density_gain(&tame.standing(), &tame, &fauna),
        );
        let (wild_r, tame_r) = (
            herd_ecology(&wild, &fauna).regrowth_rate,
            herd_ecology(&tame, &fauna).regrowth_rate,
        );
        assert!(
            tame_density > wild_density,
            "fixture: taming must raise the ceiling, or the density claim is vacuous ({tame_density} \
             against {wild_density})"
        );
        assert!(
            tame_r > wild_r,
            "fixture: taming must raise the breeding rate, or the ecology claim is vacuous ({tame_r} \
             against {wild_r})"
        );

        let half_density = herd_density_gain(&half.standing(), &half, &fauna);
        assert!(
            half_density > wild_density && half_density < tame_density,
            "a half-tamed herd's ceiling sits between the two: {half_density} in \
             ({wild_density}, {tame_density})"
        );
        let half_r = herd_ecology(&half, &fauna).regrowth_rate;
        assert!(
            half_r > wild_r && half_r < tame_r,
            "…and so does its breeding rate: {half_r} in ({wild_r}, {tame_r})"
        );
    }

    /// **THE PEN STILL SNAPS, and `partial_credit: on_completion` is what does it** — half a fence is
    /// not half a pen, so the animals are still roaming and nothing about them has changed.
    ///
    /// Asserted at **99% and at 100%** of the pen's own span, because a rung that had silently become
    /// continuous would differ from the step only in the last percent.
    #[test]
    fn the_pen_still_snaps_at_the_fence() {
        let fauna = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        let seat = |fraction_of_pen: f32| -> Herd {
            let (mut herd, _, _) = herd_at(WHOLLY_TAMED);
            let (base, width) = herd.rung_span(RungKey::AnimalPen, &ladder);
            herd.set_ladder_position(base + width * fraction_of_pen, &ladder);
            herd
        };

        const ALL_BUT_CLOSED: f32 = 0.99;
        let nearly = seat(ALL_BUT_CLOSED);
        let closed = seat(WHOLLY_TAMED);

        // **THE PRECONDITION** — the two fixtures really are on either side of the fence.
        assert!(
            !nearly.corral_meter_full() && closed.corral_meter_full(),
            "fixture: the pair must straddle the fence"
        );
        assert!(
            (nearly.ladder_position() - closed.ladder_position()).abs() > 0.0,
            "fixture: and they must be different positions"
        );

        let wholly_tamed = herd_at(WHOLLY_TAMED).0;
        let pastoral_density = herd_density_gain(&wholly_tamed.standing(), &wholly_tamed, &fauna);
        assert!(
            (herd_density_gain(&nearly.standing(), &nearly, &fauna) - pastoral_density).abs()
                < 1e-4,
            "a fence 99% up buys NOTHING — the herd holds what a pastoral herd holds"
        );
        assert!(
            herd_density_gain(&closed.standing(), &closed, &fauna) > pastoral_density,
            "…and the last percent buys all of it"
        );

        // The escape RATE and the GRACE step with it, through the keeping rung.
        let grace_of = |herd: &Herd| {
            herd_keeping_rung(herd, &ladder)
                .map(|rung| rung.upkeep_grace_turns())
                .expect("a managed herd stands on a keeping rung")
        };
        assert_eq!(
            grace_of(&nearly),
            ladder.rung(RungKey::AnimalPastoral).upkeep_grace_turns(),
            "a herd part-way through a fence is forgiven on the PASTORAL grace"
        );
        assert_eq!(
            grace_of(&closed),
            ladder.rung(RungKey::AnimalPen).upkeep_grace_turns(),
            "…and on the pen's own the moment the fence closes"
        );
    }

    /// **⛔ THE §2.8 INVARIANT: THE COST AND THE BENEFIT MOVE TOGETHER.** At *any* position, the
    /// fraction of the way up the rung the bill has climbed is the fraction the payout has climbed.
    /// Swept rather than spot-checked, because the defect this arc fixed was visible only as a
    /// *shape*: the cost was a step at position `0+` and the benefit a step at the rung's top.
    #[test]
    fn the_cost_and_the_benefit_move_together() {
        let (wild, fauna, ladder) = herd_at(WILD);
        let (tame, _, _) = herd_at(WHOLLY_TAMED);

        let cost_span = (
            herd_upkeep_demand(&wild, &fauna, &ladder),
            herd_upkeep_demand(&tame, &fauna, &ladder),
        );
        let benefit_span = (
            herd_density_gain(&wild.standing(), &wild, &fauna),
            herd_density_gain(&tame.standing(), &tame, &fauna),
        );
        assert!(
            cost_span.1 > cost_span.0 && benefit_span.1 > benefit_span.0,
            "fixture: both must climb across the rung, or the equality is 0/0"
        );

        for step in 0..=10 {
            let fraction = step as f32 / 10.0;
            let (herd, _, _) = herd_at(fraction);
            let cost_share = (herd_upkeep_demand(&herd, &fauna, &ladder) - cost_span.0)
                / (cost_span.1 - cost_span.0);
            let benefit_share = (herd_density_gain(&herd.standing(), &herd, &fauna)
                - benefit_span.0)
                / (benefit_span.1 - benefit_span.0);
            assert!(
                (cost_share - benefit_share).abs() < 1e-4,
                "at {fraction} up the rung the herd has {cost_share} of the cost and \
                 {benefit_share} of the benefit — §2.8 says those are one number"
            );
        }
    }

    // ---------------------------------------------------------------------------------------------
    // THE FLOOR SQUEEZE (`docs/plan_standing_upkeep.md` §4.14) — the growth-share backstop
    // ---------------------------------------------------------------------------------------------

    /// **⛔ THE DEGENERACY THE BACKSTOP MUST NOT BREAK.** `floor = 1.0` means *"leave the whole herd
    /// standing"*, and it is the escapement room reading `0` **by construction** that makes it mean
    /// that. A flat growth share would have made it cull every turn — the take would have become
    /// `share × growth` on a floor that exists to take nothing.
    ///
    /// `growth × (1 − floor)` is what makes it survive **without a special case**: at `floor = 1.0`
    /// the factor is `× 0`, so the take is nothing and the build's gate refuses, continuously rather
    /// than by a branch.
    ///
    /// Run on a herd **below `K` and growing**, because that is the only fixture that can tell the
    /// two apart: at `K` the growth is zero and a broken backstop would look correct.
    #[test]
    fn a_full_floor_takes_nothing_and_refuses_the_build() {
        const LEAVE_IT_ALL_STANDING: f32 = 1.0;
        const WELL_BELOW_CAPACITY: f32 = 0.4;
        let (mut herd, fauna, _) = herd_at(WILD);
        herd.carrying_capacity = 1200.0;
        herd.biomass = herd.carrying_capacity * WELL_BELOW_CAPACITY;
        herd.biomass_before_regrowth = herd.biomass;
        // **KEPT, or it does not grow at all.** `regrow_biomass`'s `abandoned_pastoral` gate zeroes
        // the growth of an owned herd whose keeping went wholly unmet — and `herd_at` records an
        // owner. A fixture that skipped this would assert against a frozen herd and pass on a
        // backstop that shared nothing.
        herd.upkeep_supplied = ONE_KEEPER_LOAD;
        regrow_biomass(&mut herd, &fauna);

        // **THE PRECONDITION** — the herd really is growing, or `× 0` is indistinguishable from
        // `× anything` and this test passes on a broken backstop.
        assert!(
            herd.growth_this_turn() > 0.0,
            "fixture: the herd must be growing, or the whole point of the backstop is untested"
        );
        assert!(
            herd.biomass < herd.carrying_capacity,
            "fixture: …which means it must be below its own capacity"
        );

        assert_eq!(
            growth_share(LEAVE_IT_ALL_STANDING, herd.growth_this_turn()),
            0.0,
            "a floor that leaves everything standing shares none of the growth"
        );
        assert_eq!(
            herd_take_room(&herd, LEAVE_IT_ALL_STANDING, &fauna),
            0.0,
            "…so there is nothing to take — which is what `floor = 1.0` MEANS"
        );
        // And the build gate reads that same zero, so watching still builds nothing.
        assert_eq!(
            hunt_take_room(
                LEAVE_IT_ALL_STANDING,
                herd.biomass,
                herd_capacity(&herd, &fauna),
                herd.growth_this_turn(),
            ),
            0.0,
            "…and the build's gate refuses it, exactly as the escapement room alone used to"
        );
    }

    /// **⛔ THE TWO GATES ARE DIFFERENT QUESTIONS, AND EXACTLY ONE OF THEM WIDENED.**
    ///
    /// They were one bool, and that is what stalled a tame forever. Splitting them is the fix; this
    /// pins the split at the seam, because a test that drives the take room directly would pass with
    /// the two re-merged in either direction.
    ///
    /// On a herd **below its floor but growing** — the exact state a tame's own `K` creates — the two
    /// must disagree:
    /// - the **build**'s seam ([`herd_take_room`]) is positive: there is a herd here to gentle;
    /// - the **lesson**'s seam ([`hunt_escapement_ceiling`]) is zero: nothing stands above the floor,
    ///   so watching teaches nothing.
    ///
    /// Merging them **either way** breaks a real thing. Widening the lesson opens
    /// `intensification::learn_multiplier`'s self-limit — a near-`1.0` floor would farm knowledge at
    /// ×2 for free, which its own doc forbids clamping. Narrowing the build restores the stall.
    #[test]
    fn the_build_gate_widened_and_the_lessons_gate_did_not() {
        const HALF_THE_STOCK: f32 = 0.5;
        let (mut herd, fauna, _) = herd_at(WILD);
        herd.carrying_capacity = 1200.0;
        // Below its floor — where a tame's own density gain puts a herd that started on it.
        herd.biomass = herd.carrying_capacity * 0.4;
        herd.biomass_before_regrowth = herd.biomass;
        herd.upkeep_supplied = ONE_KEEPER_LOAD;
        regrow_biomass(&mut herd, &fauna);

        // **THE PRECONDITION** — the state really is the interesting one: below the floor, growing.
        assert!(
            herd.growth_this_turn() > 0.0,
            "fixture: the herd must be growing, or the two seams agree for a boring reason"
        );

        let lesson_seam =
            hunt_escapement_ceiling(HALF_THE_STOCK, herd.biomass, herd_capacity(&herd, &fauna));
        let build_seam = herd_take_room(&herd, HALF_THE_STOCK, &fauna);

        assert_eq!(
            lesson_seam, 0.0,
            "the LESSON's gate stays the pure escapement room, and it is empty here — watching \
             teaches nothing, which is what stops a near-1.0 floor farming knowledge for free"
        );
        assert!(
            build_seam > 0.0,
            "…while the BUILD's gate reads what the take will pay, and there is a herd here to \
             gentle ({build_seam})"
        );
    }

    /// **THE BACKSTOP PAYS WHERE THE ROOM CANNOT** — a herd pushed below its own floor by the `K` its
    /// taming raised still hands over the share of the turn's growth its floor left takeable.
    ///
    /// The pair is the test: **the escapement room really is zero** (or the `max` is selecting the
    /// room and the backstop is untested), and the take room is nevertheless positive.
    #[test]
    fn a_herd_below_its_climbing_floor_still_hands_over_the_growth_share() {
        const HALF_THE_STOCK: f32 = 0.5;
        let (mut herd, fauna, _) = herd_at(WILD);
        herd.carrying_capacity = 1200.0;
        // Below the floor: the taming raised `K` out from under it.
        herd.biomass = herd.carrying_capacity * 0.4;
        herd.biomass_before_regrowth = herd.biomass;
        // **KEPT, or it does not grow at all.** `regrow_biomass`'s `abandoned_pastoral` gate zeroes
        // the growth of an owned herd whose keeping went wholly unmet — and `herd_at` records an
        // owner. A fixture that skipped this would assert against a frozen herd and pass on a
        // backstop that shared nothing.
        herd.upkeep_supplied = ONE_KEEPER_LOAD;
        regrow_biomass(&mut herd, &fauna);

        let room =
            hunt_escapement_ceiling(HALF_THE_STOCK, herd.biomass, herd_capacity(&herd, &fauna));
        assert_eq!(
            room, 0.0,
            "fixture: the herd must be BELOW its floor, or the escapement room is what pays and \
             the backstop is untested ({room})"
        );
        let growth = herd.growth_this_turn();
        assert!(growth > 0.0, "fixture: and it must be growing");

        let take = herd_take_room(&herd, HALF_THE_STOCK, &fauna);
        assert!(
            (take - growth * HALF_THE_STOCK).abs() < 1e-3,
            "it hands over the share of the growth the floor left takeable: {take} against {}",
            growth * HALF_THE_STOCK
        );
        assert!(
            take > 0.0,
            "…so the build's gate no longer refuses a herd its own improvement pushed under"
        );
    }

    /// **⛔ AND THE PUBLISHED ROW PAYS IT TOO** — the animal twin of
    /// `forage::tests::a_patch_below_its_climbing_floor_publishes_the_take_it_will_hand_over`, and
    /// the same defect: [`SourceYieldForecast::ceiling_at`] read the bare escapement room while
    /// [`herd_take_room`] carried the backstop, so on a herd its own Tame pushed under its floor the
    /// sim handed the hunters `growth × (1 − floor)` while the row beside it published `actual 0`,
    /// `range 0` and no useful crew.
    ///
    /// **Asserted against the take itself**, run through the shipped `hunt_take` on the herd the row
    /// is about — one Logistics regrowth on ([`next_turns_quarry`]) — because a comparison against a
    /// re-derived formula passes with both sides wrong in the same way.
    ///
    /// **The fixture is a BOAR rather than the aurochs the rest of this section uses**, and that is
    /// load-bearing: the animal take lands in whole bodies, and an aurochs' growth share at this
    /// squeeze is a fraction of its 120-unit body, so both sides would agree on a quantised zero and
    /// the test would pass against the defect. The precondition below states it.
    #[test]
    fn a_herd_below_its_climbing_floor_publishes_the_take_it_will_hand_over() {
        /// The shipped species whose body is light enough that a squeezed herd's growth share is
        /// still several whole animals — see this test's doc.
        const BOAR: &str = "Wild Boar";
        const HALF_THE_STOCK: f32 = 0.5;
        /// Enough hunters that the *herd*, not the party's reach, bounds the take.
        const HUNTERS: u32 = 20;
        /// The wild ceiling the fixture starts from, and the stock it stands at — half of it, so the
        /// herd is exactly on its floor **before** the Tame raises `K` out from under it.
        const WILD_CEILING: f32 = 1_000.0;
        const ON_THE_WILD_FLOOR: f32 = WILD_CEILING * HALF_THE_STOCK;
        /// Horizons for the row's two projections; the assertion is on `actual`, so these only have
        /// to be live.
        const SHORT_HORIZON: u32 = 4;
        /// The reported band's width. It only moves `range`, which this test does not assert on —
        /// `actual` is the row's expectation whatever the band around it is.
        const RANGE_SIGMAS: f32 = 1.0;
        /// What one hunter hauls, in biomass — the reference rate the other forecast fixtures in
        /// this module pass.
        const PER_HUNTER_HAUL: f32 = 40.0;
        /// A band at neutral productivity — the row ships at this multiplier by contract.
        const NEUTRAL_OUTPUT: f32 = 1.0;

        let fauna = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        let def = fauna
            .species_by_display(BOAR)
            .expect("the fixture names a shipped species");
        let mut herd = Herd::new(
            "game_squeeze".to_string(),
            BOAR.to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            ON_THE_WILD_FLOOR,
            WILD_CEILING,
            def.fodder_per_biomass,
            def.regrowth_rate.expect("a tameable species breeds"),
            def.body_mass,
        );
        herd.taming_cost_multiplier = fauna.taming_cost_multiplier_for(BOAR);
        assert!(
            herd.tame_outright(FactionId(0), &ladder),
            "fixture: the species must be tameable, or `K` never climbs and there is no squeeze"
        );
        // **AND THE `K` THE TAME BOUGHT, THROUGH THE SHIPPED SEAM.** Domestication makes the land
        // hold more animals ([`herd_density_gain`]); in play the graze pass restamps
        // `carrying_capacity` with it each turn, and a synthetic herd has no pasture under it — so
        // the fixture applies the herd's own gain rather than naming a number. This is the squeeze:
        // `floor · K` climbs out from under a herd that was standing exactly on it.
        herd.carrying_capacity *= herd_density_gain(&herd.standing(), &herd, &fauna);

        // **THE PRECONDITIONS**, both on the herd the row is about — one Logistics regrowth on.
        let quarry = next_turns_quarry(&herd, &fauna);
        let capacity = herd_capacity(&quarry, &fauna);
        let room = hunt_escapement_ceiling(HALF_THE_STOCK, quarry.biomass, capacity);
        assert_eq!(
            room, 0.0,
            "fixture: the Tame must have raised `K` out from under the herd, or the escapement room \
             is what pays and the backstop is untested ({room})"
        );
        let backstop = herd_take_room(&quarry, HALF_THE_STOCK, &fauna);
        assert!(
            backstop > quarry.body_mass,
            "fixture: the growth share ({backstop}) must afford a whole body ({}), or both sides \
             agree on a quantised zero and the defect is invisible",
            quarry.body_mass
        );

        let party = HuntingParty::builtin_equipped();
        let published = hunt_source_yield_preview(
            &herd,
            &fauna,
            PER_HUNTER_HAUL,
            &party,
            NEUTRAL_OUTPUT,
            HUNTERS,
            HALF_THE_STOCK,
            SHORT_HORIZON,
            SHORT_HORIZON,
            RANGE_SIGMAS,
        );

        let handed_over = {
            let mut taking = quarry.clone();
            let outcome = crate::systems::hunt_take(
                &mut taking,
                HUNTERS,
                HALF_THE_STOCK,
                PER_HUNTER_HAUL,
                &party,
                &fauna,
                // A resident band banks the whole take — the Hunt labor arm's own carry room.
                f32::INFINITY,
                // **THE SAME READING OF THE STOCHASTIC STAGES THE ROW PUBLISHES.** A boar carries
                // `wariness 0.25`, so the retreat is a real distribution and a *seeded* take would
                // differ from the row's expectation by the draw rather than by the defect. That
                // spread is what `SourceYield::range` reports; `actual` is the expectation, and this
                // asserts the expectation.
                HuntDraw::EXPECTED,
            );
            herd_hunt_yield(&quarry, &fauna)
                .apply(outcome.take.carried, NEUTRAL_OUTPUT)
                .provisions
        };

        assert!(
            handed_over > 0.0,
            "fixture: the take must hand over the growth share, or there is no disagreement to catch"
        );
        assert!(
            (published.actual - handed_over).abs() < 1e-4,
            "the row publishes what the hunters are handed: {} against {handed_over}",
            published.actual
        );
    }

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
    /// tamed* — the pastoral analog of `managed_yield`/`corralYield`. It exists so the client can
    /// quote Tame's `→ +Y` rather than only the wild hunt beside it, which hides that taming
    /// out-yields wild hunting. (It was named for the retired investment *dip*, which is what the
    /// during-building row used to read; the dip is gone — a build has its own crew — and the payoff
    /// this field carries never depended on it.)
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
    fn the_tame_rung_advertises_its_payoff_above_wild_sustain() {
        let fauna = FaunaConfig::builtin();
        // A healthy Wild Boar herd at capacity — a pennable species (`husbandry_ceiling == pen`).
        let mut herd = herd_of_size(SizeClass::Big, 1000.0, 1000.0, 0.06);
        herd.species = "Wild Boar".to_string();
        herd.regrowth_rate = 0.10;
        herd.husbandry_ceiling = HusbandryCeiling::Pen;
        herd.body_mass = 50.0;
        let forecast = hunt_forecast(&herd, &fauna, 40.0, &HuntingParty::builtin_equipped(), 1.0);

        // **The take is the hunters' own, whatever is being built beside them**
        // (`docs/plan_standing_upkeep.md` §2.2). The during-building *dip* this test was built
        // around is gone: a Tame is staffed in its own right, so what a Tame costs the hunt is the
        // hands that are on the Tame instead. What survives is that a wild hunt pays a real number
        // for the payoffs below to be compared against.
        let wild_sustain =
            forecast_expected_take(&forecast, DIP_VISIBLE_CREW, PEAK_FLOOR).provisions;
        assert!(
            wild_sustain > 0.0,
            "liveness: the wild hunt must pay something for the rung payoffs to beat"
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
    /// itself a rate. See `the_tame_rung_advertises_its_payoff_above_wild_sustain`.
    #[test]
    fn the_forecast_ceilings_are_the_escapement_stock_and_stay_ordered() {
        let fauna = FaunaConfig::builtin();
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

        let forecast = hunt_forecast(&herd, &fauna, 40.0, &HuntingParty::builtin_equipped(), 1.0);

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
        let expected = |crew, floor| forecast_expected_take(&forecast, crew, floor).provisions;
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
        // **A verb in flight moves NOTHING about the take** (`docs/plan_standing_upkeep.md` §2.2)
        // — the ceiling never carried a dip, and now neither does the crew: a build is staffed in
        // its own right, so the hunters carry what hunters carry. Asked at a staffing the carry
        // binds, which is where a dip would have been visible if one were left.
        let take = expected(DIP_VISIBLE_CREW, PEAK_FLOOR);
        assert!(
            take > 0.0,
            "liveness: this staffing must actually take something"
        );
        // **A deeper floor still takes more now** — the pressure axis is the player's own and the
        // build does not touch it, which is the separation the three allocations complete.
        assert!(
            expected(DIP_VISIBLE_CREW, 0.15) >= take,
            "a deeper floor never takes less"
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
        let mut herd = herd_of_size(SizeClass::Big, 1000.0, 1000.0, 0.06);
        herd.species = "Wild Boar".to_string();
        herd.husbandry_ceiling = HusbandryCeiling::Pen;
        herd.corralled_at = Some(UVec2::new(1, 1));
        let forecast = hunt_forecast(&herd, &fauna, 40.0, &HuntingParty::builtin_equipped(), 1.0);
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
        wild.tame_outright(faction, &LadderConfig::builtin());
        assert_eq!(wild.ladder_position(), 0.0, "a wild herd never tames");
        assert_eq!(wild.owner, None, "and never picks up an owner");

        let mut pastoral = herd_of_size(SizeClass::Migratory, 4000.0, 9000.0, 0.05);
        pastoral.husbandry_ceiling = HusbandryCeiling::Pastoral;
        assert!(pastoral.can_domesticate() && !pastoral.can_pen());
        pastoral.tame_outright(faction, &LadderConfig::builtin());
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
        registry.herds[0].tame_outright(faction, &LadderConfig::builtin());
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
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
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
        let forecast_after =
            hunt_forecast(after, &fauna, 40.0, &HuntingParty::builtin_equipped(), 1.0);
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
}
