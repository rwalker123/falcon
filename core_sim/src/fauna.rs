use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::f32::consts::TAU;

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, Rng, SeedableRng};
use sim_runtime::TerrainTags;
use sim_schema::HerdState;
use tracing::info;

use std::hash::{Hash, Hasher};

use crate::{
    components::{FollowPolicy, PopulationCohort, ResidentBand, SourceYield, Tile},
    fauna_config::{
        default_loiter_radius, EcologyConfig, FaunaConfig, FaunaConfigHandle, GrazeConfig,
        HusbandryCeiling, SizeClass, SpeciesDef, NO_GRAZE_CAPACITY,
    },
    food::{classify_food_module, FoodModule},
    graze::GrazeRegistry,
    grid_utils::{hex_distance_wrapped, hex_neighbor, hex_range_tiles, HEX_DIRECTION_COUNT},
    hashing::FnvHasher,
    intensification::{LadderConfig, LadderConfigHandle, RungDef, RungKey, RungMovement},
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

/// Id prefix marking a short-range wild-game group (migratory herds use `herd_`). The
/// `abundance.max_total_game` cap applies to these groups only — both at initial spawn
/// (`placed.len()`) and per-turn immigration.
const GAME_ID_PREFIX: &str = "game_";

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

/// Stable string keys for `RoamState`, shared by the snapshot capture (`HerdRoamState.mode`) and
/// the rollback restore (`RoamState::from_mode`) so the mapping lives in one place.
const ROAM_MODE_GRAZE_WANDER: &str = "graze_wander";
const ROAM_MODE_LOITER: &str = "loiter";
const ROAM_MODE_MIGRATE: &str = "migrate";

impl RoamState {
    /// Stable string key for the movement mode (snapshot `HerdRoamState.mode`).
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
    /// (pastoral) or penned herd ignores it and keeps its rung's own faster `r`. Round-tripped through
    /// the rollback snapshot (`HerdState.regrowth_rate`, sim-side only — not on the client wire).
    pub regrowth_rate: f32,
    /// **How far up the husbandry ladder this herd's species can climb** (Grazing 2d-δ), cached from
    /// the `SpeciesDef` at spawn (mirroring `regrowth_rate` / `fodder_per_biomass`). Gates the three
    /// husbandry seams without re-resolving config: taming accrual + the `tame` command (a `Wild` herd
    /// never tames), and the `corral` / `extend_pen` paths (only a `Pen` herd pens).
    /// Round-tripped through `HerdState.husbandry_ceiling` and exported as `husbandryCeiling`.
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
    /// snapshot-persisted. The animal mirror of a cultivated patch being a fixed tended patch;
    /// contrast the deliberate asymmetry — an *un*corralled domesticated herd stays mobile
    /// (pastoralism travels with the band).
    pub corralled_at: Option<UVec2>,
    /// Pen-construction progress in `[0.0, 1.0]`; `1.0` = the pen is built (and `corralled_at` is set
    /// that same turn). Accrues **only** while a band works this herd under the explicit
    /// `FollowPolicy::Corral` policy (faction knows **Penning** + owns the *domesticated* herd), at
    /// `husbandry.corral_build_progress_per_turn`. The animal mirror of
    /// `ForagePatch::cultivation_progress`, and the investment the `corralling_yield_fraction` dip
    /// buys. Authoritative sim state — snapshot-persisted (`HerdState.corral_progress`), so a rollback
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
    /// count) so β only has to grow it. Authoritative sim state — snapshot-persisted.
    pub pen_radius: u32,
    /// Pen-**extension** build progress `[0.0, 1.0]` for the in-flight ring (the `ExtendPen` labor
    /// ladder, 2d-β), accrued each turn the keeper tends an *extending* pen at
    /// `husbandry.corral_build_progress_per_turn`; at `1.0` the ring completes (`pen_radius += 1`, this
    /// resets to `0.0`, `pen_extending` clears). Exported as `penExtendProgress` for a "Fencing N%"
    /// badge. Snapshot-persisted alongside `pen_radius`.
    pub pen_extend_progress: f32,
    /// **The `ExtendPen` "extending" state** (2d-β): `true` while a keeper is fencing the next ring
    /// (`pen_extend_progress` accruing, the harvest dipped to `corralling_yield_fraction`), the animal
    /// mirror of a herd's under-construction `corral_progress`. Set by the `ExtendPen` command, cleared
    /// when the ring completes. Snapshot-persisted so a rollback rewinds an in-flight extension rather
    /// than stranding a half-progress meter that never completes.
    pub pen_extending: bool,
    /// Transient per-turn scratch: the graze biomass this herd actually drew from its footprint this
    /// turn (`advance_herd_grazing`, Logistics), read the same turn by the pen larder-offset in
    /// `advance_labor_allocation` (Population). For a penned herd it is what the fenced footprint fed
    /// the pen; the larder pays only the remainder. **Not** snapshot-persisted (recomputed each turn).
    pub footprint_intake: f32,
    /// Transient per-turn scratch: the share of a penned herd's feed its footprint covered last FEED
    /// (`footprint_intake / (fodder_per_biomass × biomass)`, clamped `[0, 1]`; Grazing 2d §2.3). `1.0`
    /// = the pasture feeds the pen for free; `0.0` = a barren footprint pays the full larder bill.
    /// Exported as `penPastureFraction`. `0.0` for an unpenned herd. **Not** snapshot-persisted.
    pub pen_pasture_fraction: f32,
    /// Transient per-turn flag: a Hunt assignment tended this corralled herd this turn (set in
    /// `advance_labor_allocation`, Population). `advance_husbandry` (Logistics, the *next* turn —
    /// Logistics runs before Population) reads it: a corralled herd tended this turn is spared, an
    /// untended one **escapes** (reverts to mobile). Mirrors `ForagePatch::tended_this_turn`. **Not**
    /// snapshot-persisted (derived) — a rehydrated corralled herd reads `false` until tended again,
    /// so a rollback can only *delay* an escape by one turn, never resurrect a broken-out herd.
    pub corralled_tended_this_turn: bool,
    /// Transient per-turn flag: the fraction of the pen's **feed** demand its keeper actually paid last
    /// turn (`paid / demand ∈ [0, 1]`; `1.0` = fully fed, and the value when nothing was demanded).
    /// Written by the corral-tend branch of `advance_labor_allocation` (Population) and read one turn
    /// later by `advance_husbandry` (Logistics), which **starves** an underfed pen — the same
    /// deliberate one-turn lag as `corralled_tended_this_turn`, and reset to `1.0` after reading.
    /// **Not** snapshot-persisted (derived): a rehydrated herd reads `1.0` (fed), so a rollback can
    /// only *delay* a starvation turn, never invent one.
    pub pen_fed_fraction: f32,
    /// Transient edge-gate for the starving-pen feed line: `true` while the herd is *already known* to
    /// be starving, so `advance_husbandry` announces the famine **once** on the turn it starts rather
    /// than every turn it continues. Cleared when the pen is fed again (so a *second* famine is
    /// announced afresh). Not snapshot-persisted — a rollback can at worst re-announce.
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
    /// **Not** snapshot-persisted (derived) — a rehydrated herd reads `false` ("untended"), so a
    /// rollback can only *delay* a decay turn by one, never invent progress.
    pub tamed_this_turn: bool,
}

impl Herd {
    // A constructor that mirrors the herd's identity + spawn-state fields (id/species/size/route/
    // biomass/K/fodder/regrowth) — bundling them into a struct would just move the noise.
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
            corralled_tended_this_turn: false,
            pen_fed_fraction: PEN_FULLY_FED,
            pen_starving: false,
            tamed_this_turn: false,
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

    /// Accrue taming progress for `faction` (the band working this herd under `FollowPolicy::Tame`).
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
    pub fn accrue_domestication(&mut self, faction: FactionId, amount: f32) {
        if self.is_domesticated() || !self.can_domesticate() {
            return;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner == Some(faction) {
            self.domestication_progress = (self.domestication_progress + amount).min(1.0);
        }
    }

    /// Decay husbandry progress toward zero when the group isn't being actively tended;
    /// ownership lapses once progress reaches zero. A domesticated group is left alone.
    pub(crate) fn decay_domestication(&mut self, amount: f32) {
        if self.is_domesticated() {
            return;
        }
        self.domestication_progress = (self.domestication_progress - amount).max(0.0);
        // Reconcile the `owner is Some ⟺ progress > 0` invariant unconditionally, so a
        // group that reaches (or somehow sits at) zero progress never keeps a stale owner
        // — which would otherwise block another faction from ever tending it.
        if self.domestication_progress <= 0.0 {
            self.owner = None;
        }
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

    /// Accrue pen-construction progress for `faction` (the keeper band, working the herd under
    /// `FollowPolicy::Corral`); at `1.0` the pen is finished and the herd is penned at `tile`. Only
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
/// `Herd::pen_fed_fraction`, so an un-penned / freshly-rehydrated herd never starves.
pub(crate) const PEN_FULLY_FED: f32 = 1.0;

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

/// **The feed a pen demands — or WOULD demand once built** — at the herd's current biomass:
/// `upkeep_per_biomass × biomass`, drawn from the keeper band's larder. A penned herd cannot graze;
/// this is the physical price of the thing that makes a pen a pen, and the tether that gives "the pen
/// pins the band" its teeth.
///
/// **Answered for EVERY herd, penned or not** — a *projection* for an unpenned one, the *live* demand
/// for a penned one — on the **same biomass basis** [`corral_provisions`] (`hunt_forecast`'s
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
/// standing above the MSY point (`K/2`), never more than one turn's peak regrowth
/// ([`peak_regrowth`] = `r·K/4` — the same shared curve, no second formula).
///
/// ```text
/// take = min(peak_regrowth(K), max(0, B − K/2))
/// ```
///
/// **Why escapement, and not the constant-catch `sustainable_yield` a wild `Sustain` hunt takes.**
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
/// Takes the raw `(biomass, capacity, ecology)` rather than a `&Herd` because the forecast must also
/// answer it for a herd that is **not penned yet** ("what will this pay once the pen is built?").
pub(crate) fn managed_yield_biomass(biomass: f32, capacity: f32, ecology: &EcologyConfig) -> f32 {
    let escapement = capacity * MSY_BIOMASS_FRACTION;
    (biomass - escapement)
        .max(0.0)
        .min(peak_regrowth(capacity, ecology))
}

/// The **gross managed harvest a PEN yields**, in biomass: [`managed_yield_biomass`] against the herd's
/// per-species pen ecology ([`pen_ecology_for`]) and the pen's capacity (the herd's
/// `carrying_capacity`, which for a penned herd is its fenced footprint's `K` — Grazing 2d). Takes the
/// `&Herd` (not raw scalars) because the per-species pen `r` needs the herd's own wild rate; the
/// forecast still calls it for a herd that is **not penned yet** to project "what would this pay once
/// penned?".
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
    /// building N%" meter while a keeper works the herd under the Corral policy.
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
    /// `SedentarizationScore` reads for its "domestication progress" input (`TASKS.md`).
    pub fn domesticated_count(&self, faction: FactionId) -> usize {
        self.herds
            .iter()
            .filter(|herd| herd.is_domesticated() && herd.owner == Some(faction))
            .count()
    }

    /// Rebuild the authoritative herd list from a rollback snapshot's `HerdState`s (clear + rebuild,
    /// mirroring `GenerationRegistry::update_from_states`). Restores biomass / position / movement /
    /// ecology so a rollback rewinds herd sim state, not just display telemetry.
    pub fn update_from_states(&mut self, states: &[HerdState]) {
        self.herds = states.iter().map(herd_from_state).collect();
    }

    /// Construct a registry directly from snapshot `HerdState`s (mirrors
    /// `GenerationRegistry::from_states`).
    pub fn from_states(states: &[HerdState]) -> Self {
        let mut registry = Self::default();
        registry.update_from_states(states);
        registry
    }
}

/// Reconstruct a live `Herd` from its snapshot mirror (the rollback restore side of `herd_state`
/// in `snapshot.rs`). Parses the `ecology_phase` / `size_class` / `roam` string keys back to their
/// live enums.
fn herd_from_state(state: &HerdState) -> Herd {
    Herd {
        id: state.id.clone(),
        label: state.label.clone(),
        species: state.species.clone(),
        size_class: SizeClass::from_key(&state.size_class),
        route: state.route.iter().map(|&(x, y)| UVec2::new(x, y)).collect(),
        step_index: state.step_index as usize,
        current_pos: UVec2::new(state.current_pos.0, state.current_pos.1),
        dwell_remaining: state.dwell_remaining,
        roam: RoamState::from_mode(&state.roam.mode, state.roam.loiter_turns_left),
        next_pos: state.next_pos.map(|(x, y)| UVec2::new(x, y)),
        biomass: state.ecology.biomass,
        carrying_capacity: state.ecology.carrying_capacity,
        fodder_per_biomass: state.fodder_per_biomass,
        regrowth_rate: state.regrowth_rate,
        husbandry_ceiling: HusbandryCeiling::from_key(&state.husbandry_ceiling),
        ecology_phase: EcologyPhase::from_key(&state.ecology.ecology_phase),
        domestication_progress: state.ecology.progress,
        owner: state.ecology.owner.map(FactionId),
        corralled_at: state.corralled_at.map(|(x, y)| UVec2::new(x, y)),
        corral_progress: state.corral_progress,
        pen_radius: state.pen_radius,
        pen_extend_progress: state.pen_extend_progress,
        pen_extending: state.pen_extending,
        // Transient (not persisted) — recomputed each turn (footprint/pasture) or reset to the neutral
        // value: a rehydrated corralled herd is "untended" until worked again, and "fed" (so a rollback
        // can delay a starvation turn but never invent one).
        footprint_intake: 0.0,
        pen_pasture_fraction: 0.0,
        corralled_tended_this_turn: false,
        pen_fed_fraction: PEN_FULLY_FED,
        pen_starving: false,
        tamed_this_turn: false,
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
    let base = start_location
        .position()
        .unwrap_or(UVec2::new(width / 2, height / 2));

    let mut herds = Vec::new();
    // 1. Long-range migratory herds — start-anchored, species/biomass from config.
    spawn_migratory_herds(
        &fauna,
        base,
        width,
        height,
        &tile_registry,
        &tiles,
        &mut rng,
        &mut herds,
    );
    // 2. Short-range wild game — biome-density placement across the whole map.
    spawn_short_range_game(
        &fauna,
        width,
        height,
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

/// Long-range migratory herds: a handful of cross-region walkers anchored on the
/// start area, one per `determine_herd_count`, species drawn from the config's
/// migratory rows.
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
) {
    let migratory = fauna.migratory_species();
    if migratory.is_empty() {
        return;
    }
    let herd_target = determine_herd_count(width, height);
    for idx in 0..herd_target {
        let (key, def) = migratory[rng.gen_range(0..migratory.len())];
        let steps = def.sample_route_len(rng);
        let Some(route) = build_route(
            base,
            width,
            height,
            tile_registry,
            tiles,
            &fauna.graze,
            rng,
            steps,
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
    }
}

/// Short-range wild game (big + small): iterate land tiles, roll the per-biome
/// abundance, then greedily place bounded, spaced-out groups from a shuffled pool
/// so placement is spread across the map rather than clustered by scan order.
fn spawn_short_range_game(
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
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
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    rng: &mut SmallRng,
) -> Option<Herd> {
    let candidates = fauna.game_species_for_biome(module_key);
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
    );
    // Cache the species' husbandry ceiling (Grazing 2d-δ) so the gates read a herd field.
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
    let width = config.grid_size.x.max(1);
    let height = config.grid_size.y.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    let base_seed = world_seed.map(|s| s.0).unwrap_or(config.map_seed) ^ tick.0;
    // A `None`/empty graze layer → plain land movement (pre-2b-i); a seeded one → graze-aware roam.
    let empty_graze = GrazeRegistry::default();
    let graze = graze.as_deref().unwrap_or(&empty_graze);
    let owner_camps = owner_camp_tiles(&bands, &tiles);
    for herd in registry.herds.iter_mut() {
        // Deterministic per-herd, per-turn RNG (rollback-stable): map_seed ^ tick ^ salt ^ id-hash.
        let mut hasher = FnvHasher::new();
        herd.id.hash(&mut hasher);
        let mut rng =
            SmallRng::seed_from_u64(base_seed ^ HERD_MOVEMENT_SEED_SALT ^ hasher.finish());
        // Movement cadence levers for this species (fall back to a slow game default if unresolved).
        let def = fauna.species_by_display(&herd.species);
        // **The movement primitive comes from the herd's RUNG, not from `is_domesticated()`** — the
        // ladder's `behavior.movement` is config (§5), and this is the first place the engine reads
        // it. §3's proximity spine falls out of the shipped records: wild `roam` → pastoral
        // `drift_to_owner` → pen `fixed`.
        match herd_rung(herd, &ladder).behavior.movement {
            // A `fixed` source does not roam — today's penned herd, pinned at `corralled_at` (Rung
            // 1c). It still grazes/regrows (ecology is independent of movement); only its wander is
            // skipped.
            RungMovement::Fixed => herd.next_pos = None,
            RungMovement::Roam => advance_herd_roam(
                herd,
                def,
                None, // no drift: a wild herd roams its own full range
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
        if let Some(k) = ecological_carrying_capacity(herd, def, graze, &fauna, width, height, wrap)
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
    registry.herds.retain(|herd| {
        herd.is_corralled()
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
fn ecological_carrying_capacity(
    herd: &Herd,
    def: Option<&SpeciesDef>,
    graze: &GrazeRegistry,
    fauna: &FaunaConfig,
    width: u32,
    height: u32,
    wrap: bool,
) -> Option<f32> {
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
    Some(flow / herd.fodder_per_biomass)
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
/// Vec is itself rollback-persisted in a fixed order (captured coord-stable and rebuilt by
/// `update_from_states`), and `advance_herds`' `retain` / immigration's `push` both preserve it — so
/// two herds sharing a tile always draw in the same order, and the eaten state is reproducible.
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

/// One turn of graze-wander / loiter-migrate movement (`docs/plan_wildlife_hunting_overlay.md`
/// "Herd Movement"). Deterministic under the per-turn seeded `rng`. Mutates the herd's
/// `current_pos` / `dwell_remaining` / `roam` / `step_index` / `next_pos`. `def` supplies the
/// species' cadence levers (`None` → a slow game default). Movement is ≤ 1 hex/turn and land-clamped;
/// it never touches `biomass` (ecology stays independent — a loitering herd still grazes/regrows).
///
/// **`drift`** carries the `drift_to_owner` primitive (§3): `Some(camps)` = this herd's rung says it
/// drifts, and these are its owner's band tiles. It **composes with, and does not replace, the wild
/// roam**: the drift only pre-empts the roam's own target on a turn it can genuinely get *closer* to a
/// camp. Once the herd is as near as it can get — at the camp, or hemmed in — the turn falls through
/// to the normal state machine, so a tamed herd grazes around its people instead of freezing on their
/// tile. `None` (a wild herd, an unowned one, or an owner with no bands) is exactly today's roam.
// Args are the herd + its cadence levers + the grid/tile context needed to land-clamp a hex step;
// bundling them adds noise without clarity (matches the other fauna spawn/movement helpers).
#[allow(clippy::too_many_arguments)]
fn advance_herd_roam(
    herd: &mut Herd,
    def: Option<&SpeciesDef>,
    drift: Option<&[UVec2]>,
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

    // **`drift_to_owner`** — the tamed rung's attractor is its owner's camp, not its wild route, so it
    // is resolved *before* the roam state machine and takes the turn when it can close the distance.
    // The species' own `dwell_turns` cadence still applies: taming an animal does not make it faster,
    // it makes it *near*.
    if let Some(camps) = drift.filter(|camps| !camps.is_empty()) {
        if herd.dwell_remaining > 0 {
            herd.dwell_remaining -= 1;
            return;
        }
        if drift_step_toward_owner(herd, camps, registry, tiles, graze, width, height, wrap) {
            herd.dwell_remaining = dwell_turns;
            return;
        }
        // Already at the camp (or no acceptable step gets nearer) → fall through to the normal roam.
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

/// **The `drift_to_owner` primitive** (`docs/plan_intensification_ladder.md` §3, dial 4): one step
/// toward the owner's **nearest** camp. Returns whether the herd moved — `false` = it is already as
/// near as it can get (standing in the camp, or no acceptable step closes the distance), and the
/// caller falls through to the herd's normal roam, so a tamed herd grazes *around* its people rather
/// than freezing on their tile.
///
/// **It composes with the roam, it does not replace it.** The candidates are exactly
/// [`acceptable_steps`] — the roam's own land + barren-avoidance filter — and the roam's fertility
/// preference survives as the second sort key. What the primitive changes is only the herd's
/// *attractor*: its owner's camp instead of its wild route anchor.
///
/// **The order is TOTAL and hasher-independent**: `(camp distance ASC, graze capacity DESC, y ASC,
/// x ASC)`. The last two keys exist because the first two can genuinely tie (two neighbours the same
/// distance out on the same biome) and a tie broken by anything incidental is a flake waiting to
/// happen — the lesson of `GrazeRegistry::richest_patch`'s `HashMap`-order tie. `camp_distance` mins
/// over the camps, so the camp list's order cannot leak in either.
///
/// **There is no drift-strength lever, deliberately** — this is a preference *ordering*, not a
/// weight: one step per turn toward the people, and nothing to tune.
#[allow(clippy::too_many_arguments)]
fn drift_step_toward_owner(
    herd: &mut Herd,
    camps: &[UVec2],
    registry: &TileRegistry,
    tiles: &Query<&Tile>,
    graze: &GrazeRegistry,
    width: u32,
    height: u32,
    wrap: bool,
) -> bool {
    let current = camp_distance(herd.current_pos, camps, width, wrap);
    if current == 0 {
        // Standing on the camp tile: nothing to close, so the normal roam takes the turn.
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
        let d = camp_distance(np, camps, width, wrap);
        // Only a step that genuinely closes the distance is a drift; anything else is roaming, and
        // roaming is the fall-through's job.
        if d >= current {
            continue;
        }
        let candidate = (d, cap, np);
        let better = best.is_none_or(|best| drift_order(candidate, best) == Ordering::Less);
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

/// The drift's **total** candidate order: nearest the camp, then the richer pasture, then a
/// deterministic `(y, x)` tie-break. See [`drift_step_toward_owner`] on why the last key is not
/// optional.
fn drift_order(a: (u32, f32, UVec2), b: (u32, f32, UVec2)) -> Ordering {
    a.0.cmp(&b.0)
        .then_with(|| b.1.total_cmp(&a.1))
        .then_with(|| a.2.y.cmp(&b.2.y))
        .then_with(|| a.2.x.cmp(&b.2.x))
}

/// Hex distance from `from` to the **nearest** of `camps` (wrap-aware). A `min` over the list, so the
/// list's order cannot change the answer — the drift's determinism rests on this. An empty list never
/// reaches here (the caller filters it into a plain roam).
fn camp_distance(from: UVec2, camps: &[UVec2], width: u32, wrap: bool) -> u32 {
    camps
        .iter()
        .map(|&camp| hex_distance_wrapped(from, camp, width, wrap))
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

/// Per-turn husbandry (`TurnStage::Logistics`, after `advance_herds`): run the **penned** groups'
/// escape/starvation checks and decay husbandry progress on any not-yet-tamed group. Runs before the
/// same turn's accrual in `advance_labor_allocation` (`Population`), so a `Tame`-worked group nets
/// `progress_per_turn - decay_per_turn` while an abandoned one only decays.
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
/// (`advance_labor_allocation`); this pass runs its two neglect checks and nothing else. Logistics
/// runs before Population, so both flags were written **last** turn (the
/// deliberate one-turn lag, mirroring `ForagePatch::tended_this_turn`):
/// - **No keeper → escape.** An untended pen clears `corralled_at`, **zeroes `corral_progress`, and
///   resets the whole fenced-footprint state** (`pen_radius` / `pen_extend_progress` / `pen_extending`,
///   Grazing 2d — the pen is lost with the herd that roamed off it, so re-penning rebuilds every
///   ExtendPen ring from scratch and no stale radius teleports onto the next tile), and pushes a
///   `CommandEventKind::Corral` feed line, so a destroyed investment is never silent. Nobody was
///   minding the gate.
/// - **A keeper who cannot pay the feed → starvation.** An underfed pen (`pen_fed_fraction < 1`)
///   **shrinks** by `pen.starve_shrink_rate × (1 − fed) × biomass`, floored at
///   `pen.ecology.extinction_floor × K_pen`. It does **not** despawn and does **not** lose the pen: the
///   herd withers to a remnant and **recovers when fed again** — a recoverable famine the player can
///   see and fix (edge-gated feed line on the first starving turn), not a silent void of the
///   investment. Starving your animals to feed your people becomes a *decision*.
///
/// The animal mirror of `forage::advance_cultivation`'s feral pass.
pub fn advance_husbandry(
    mut registry: ResMut<HerdRegistry>,
    fauna_config: Res<FaunaConfigHandle>,
    ladder_config: Res<LadderConfigHandle>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
) {
    let fauna = fauna_config.get();
    let ladder = ladder_config.get();
    // The go-feral rate is the `animal:pastoral` rung's own build decay — the shared ladder seam
    // (`crate::intensification`), exactly as `advance_cultivation` reads the `plant:tended` rung's.
    // It is read **per herd** below, at that species' `taming_rate` (slice 3c): the multiplier scales
    // the rung's whole build timescale, decay included, so a species that is slow to tame is equally
    // slow to forget (a Steppe Runner bleeds its partial taming 5× slower than a rabbit).
    let pastoral_rung = ladder.rung(RungKey::AnimalPastoral);
    for herd in registry.herds.iter_mut() {
        // One-turn-lag treatment for the *taming* flag (written last turn by the `Tame` arm),
        // cleared for every herd so it can never go stale.
        let tamed_last_turn = herd.tamed_this_turn;
        herd.tamed_this_turn = false;
        if herd.is_domesticated() {
            // **A tamed herd is paid NOTHING here** — passive-free pastoral is retired (§3): it
            // yields only through a worker's `Hunt` assignment, at the pastoral rung's better `r`.
            //
            // Corral (Rung 1c): a penned herd's keeper harvests the **pen's** MSY place-local
            // (`advance_labor_allocation`), and the pen **escapes** if left untended, or **starves**
            // if its keeper cannot pay the feed. Logistics runs before Population, so both flags read
            // here were written last turn (a one-turn lag, mirroring `ForagePatch::tended_this_turn`).
            if herd.is_corralled() {
                if herd.corralled_tended_this_turn {
                    herd.corralled_tended_this_turn = false;
                    starve_underfed_pen(herd, &fauna, &mut event_log, tick.0);
                } else {
                    let pen = herd.corralled_at;
                    herd.corralled_at = None;
                    // The pen is LOST, not merely opened: zero the build progress **and the whole
                    // fenced-footprint state** (Grazing 2d — `pen_radius`, `pen_extend_progress`,
                    // `pen_extending`) so re-penning pays the full `corral_build_progress_per_turn`
                    // investment again AND rebuilds every ExtendPen ring from scratch (~25 turns/ring).
                    // **A patch is a place and a herd is not** — `cultivation_progress` may decay
                    // gradually because the improvement sits on a tile that cannot move, so partial
                    // progress still refers to the same patch; `corral_progress` and the fence live on
                    // the *herd*, which roams, so any retained state would re-materialize the pen (and
                    // its grown radius) at whatever tile the animal has since wandered to (a teleporting
                    // corral inheriting its old fence for free) and make abandoning a pen cost one turn
                    // instead of the rebuild. Resetting here — the single place a completed pen's
                    // `corralled_at` is cleared — also clears the stale `penRadius`/`penExtendProgress`
                    // the wire would otherwise export on the now-mobile herd (a phantom "Fencing N%"
                    // badge). Contrast the **mid-build** gate lapse, which is NOT this branch (it only
                    // fires on a completed pen): a half-built pen keeps its progress — materials on the
                    // ground at a tile the herd is still at.
                    herd.corral_progress = 0.0;
                    herd.pen_radius = 0;
                    herd.pen_extend_progress = 0.0;
                    herd.pen_extending = false;
                    // Also clear the last penned pasture share: it is written only in the corral-tend
                    // branch (never for a mobile herd), so without this the escaped herd would keep its
                    // last value (~1.0 on lush pasture) and the wire would re-export it, violating the
                    // "0.0 for an unpenned herd" contract on `penPastureFraction`. (`footprint_intake`
                    // needs no reset — it is recomputed for every herd each turn in `advance_herd_grazing`
                    // and is not on the wire.)
                    herd.pen_pasture_fraction = 0.0;
                    info!(
                        target: "shadow_scale::analytics",
                        event = "corral_escape",
                        herd = %herd.id,
                        faction = herd.owner.map(|f| f.0).unwrap_or_default(),
                    );
                    // Tell the player. The escape now DESTROYS a 25-turn investment (the reset
                    // above), so it must never be silent: the corral meter would otherwise snap
                    // 1.0 → 0.0 with no explanation. Same `CommandEventKind::Corral` the pen's
                    // *completion* pushes from `advance_labor_allocation` — one feed line for the
                    // pen's whole life. Human text names the **species** (`herd.species`), not the
                    // internal id, and says what happened AND why; the detail carries the
                    // machine-readable `status=… reason=… herd=…` fields.
                    if let Some(owner) = herd.owner {
                        let (pen_x, pen_y) = pen.map(|t| (t.x, t.y)).unwrap_or_default();
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Corral,
                            owner,
                            format!(
                                "The {} herd broke out — untended, the pen is lost",
                                herd.species
                            ),
                            Some(format!(
                                "status=escaped reason=untended action=corral herd={} x={} y={}",
                                herd.id, pen_x, pen_y
                            )),
                        ));
                    }
                }
            }
        } else if !tamed_last_turn {
            // **Feral if untamed** — the animal mirror of `advance_cultivation`. A herd a crew
            // worked under `Tame` last turn is **spared**; an abandoned part-tamed herd bleeds the
            // `animal:pastoral` rung's own `decay_per_turn` back toward wild (its owner lapsing at
            // zero), **at its species' own taming timescale**. Reading the rate off the ladder is what
            // keeps the two food webs' abandon-decay tuned together — it is the shared build seam's,
            // not a fauna-only lever.
            herd.decay_domestication(
                pastoral_rung.build_decay(fauna.taming_rate_for(&herd.species)),
            );
        }
    }
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
/// Each `ceiling_*` is the policy ceiling **already clamped to the source's remaining biomass**, so
/// that `min` IS the take the sim pays. **Forecast == actual is an invariant**: the forecast and
/// the take path (`hunt_take` / `forage::forage_take`) share the same ceiling + conversion helpers
/// (`hunt_policy_ceiling`/`hunt_provisions`, `forage_policy_ceiling`/`forage_provisions`) — never
/// duplicate the formulas, or the UI will lie.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceYieldForecast {
    /// Food/turn one worker contributes at this source (throughput → provisions), before the policy
    /// ceiling binds. `0.0` means no worker can extract anything this turn (e.g. a zero seasonal
    /// weight) — consumers must not divide by it.
    pub per_worker_yield: f32,
    /// Food/turn cap under **Sustain** (the MSY skim).
    pub ceiling_sustain: f32,
    /// Food/turn cap under **Surplus**.
    pub ceiling_surplus: f32,
    /// Food/turn cap under **Market**.
    pub ceiling_market: f32,
    /// Food/turn cap under **Eradicate**.
    pub ceiling_eradicate: f32,
    /// Food/turn cap under the source's **top investment** policy — `Cultivate` for a forage patch,
    /// `Corral` for a herd. This is the **preparing** yield: `yield_fraction_while_building × the
    /// Sustain (MSY) ceiling`, the up-front cost of the improvement. Crosses the wire as
    /// `ForagePatchState.ceilingCultivate` / `HerdTelemetryState.ceilingCorral`.
    pub ceiling_prepare: f32,
    /// Food/turn cap under **`Tame`** — the animal rung-2 dip (the `animal:pastoral` rung's
    /// `yield_fraction_while_building × MSY`). `0` on a forage patch, where `Tame` is not a legal
    /// policy.
    ///
    /// **Its own field, deliberately.** The animal branch now has *two* investment rungs, so
    /// [`SourceYieldForecast::ceiling_prepare`] can no longer serve "the investment policy" — it is
    /// Corral's. The two dips are *coincidentally* equal on today's shipped levers (both 0.50), and
    /// folding Tame onto Corral's field would pass every forecast==actual test **by that
    /// coincidence** while silently lying the moment either rung's dial is retuned — exactly the
    /// "two copies agreeing with each other while both disagree with the take" failure this
    /// codebase has already paid for once. Reaches the client through the free-form
    /// `huntPolicyCeilings` list (no schema change), not a scalar field.
    pub ceiling_tame: f32,
    /// Food/turn cap under **`Sow`** — the plant rung-3 dip (the `plant:field` rung's
    /// `yield_fraction_while_building × MSY`). `0` on a herd, where `Sow` is not a legal policy.
    ///
    /// **Its own field, for [`SourceYieldForecast::ceiling_tame`]'s reason**, now on the plant side:
    /// the plant branch has *two* investment rungs, so `ceiling_prepare` can no longer serve "the
    /// investment policy" — it is Cultivate's. The two dips are *coincidentally* equal on today's
    /// shipped levers (both 0.25); folding Sow onto Cultivate's field would pass every
    /// forecast==actual test by that coincidence and lie the moment either rung is retuned.
    ///
    /// **Not on the wire yet** — the client's patch card is slice 6, which needs a `ceilingSow` (and
    /// a `fieldYield`) beside today's `ceilingCultivate`/`tendedYield`.
    pub ceiling_sow: f32,
    /// Food/turn the source pays **once the improvement completes** — the tended-patch harvest
    /// (`tended_provisions`) / the corral harvest (`corral_provisions`) at its current biomass. Lets
    /// the client show the payoff ("preparing X → then Y") *before* the player commits to the dip.
    /// Crosses the wire as `ForagePatchState.tendedYield` / `HerdTelemetryState.corralYield`.
    pub managed_yield: f32,
}

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
    pub(crate) fn managed(production: f32, per_worker_yield: f32) -> Self {
        Self {
            per_worker_yield,
            ceiling_sustain: production,
            ceiling_surplus: production,
            ceiling_market: production,
            ceiling_eradicate: production,
            // The improvement is already built — "preparing" and "once complete" are both just the
            // managed yield it pays now. (A source at this rung is past taming too.) Honest *here*,
            // unlike on a rung-2 source, because there is genuinely nothing left to build: the plant
            // web's rung-2 patch takes the policy-live path instead (`forage::forage_forecast`).
            ceiling_prepare: production,
            ceiling_tame: production,
            ceiling_sow: production,
            managed_yield: production,
        }
    }

    /// The food/turn cap this source pays under `policy` — the `ceiling[policy]` lookup over the
    /// exposed ceilings (wire: `ceilingSustain`/…). While an improvement is being prepared the source
    /// pays that rung's reduced `yield_fraction_while_building` bite, and **every investment rung has
    /// its own field**: `ceiling_prepare` is the one each branch already puts on the wire
    /// (`Cultivate` → `ceilingCultivate` on a patch, `Corral` → `ceilingCorral` on a herd — the two
    /// are kind-exclusive, so one field serves both), while each branch's *other* investment rung
    /// carries its own (`ceiling_tame`, `ceiling_sow`). Two rungs never share a field just because
    /// today's levers agree — that coincidence is how a preview starts lying under a retune. Once an
    /// improvement *completes*
    /// the source is `tended()`, whose every ceiling already **is** `managed_yield` — so this one
    /// lookup covers both sides of every investment without a second formula.
    pub fn ceiling_for(&self, policy: FollowPolicy) -> f32 {
        match policy {
            FollowPolicy::Sustain => self.ceiling_sustain,
            FollowPolicy::Surplus => self.ceiling_surplus,
            FollowPolicy::Market => self.ceiling_market,
            FollowPolicy::Eradicate => self.ceiling_eradicate,
            FollowPolicy::Tame => self.ceiling_tame,
            FollowPolicy::Sow => self.ceiling_sow,
            FollowPolicy::Cultivate | FollowPolicy::Corral => self.ceiling_prepare,
        }
    }
}

/// **The expected take**: the food/turn `workers` will produce at this source under `policy` —
/// `min(workers × per_worker_yield, ceiling(policy))`, the exact composition the take path
/// (`forage_take` / `hunt_take`, both `min(worker_cap, policy_ceiling)` clamped to biomass — the
/// clamp is already folded into every `ceiling_*`) pays and the client's "Expected yield" row
/// promises. The one place that formula lives: the client's compose-time preview, the assign-time
/// telemetry seed (`SourceYield` — so a brand-new assignment displays its yield before the turn
/// resolves), and the forecast==actual tests all call it.
pub fn forecast_expected_take(
    forecast: &SourceYieldForecast,
    workers: u32,
    policy: FollowPolicy,
) -> f32 {
    (workers as f32 * forecast.per_worker_yield).min(forecast.ceiling_for(policy))
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
/// - `wasted` = the understaffing signal: the production the crew could not collect,
/// - `workers_needed` = the overstaffing signal, inverted from the expected take by the per-worker
///   throughput (a ratio, so provisions-space matches the resolution path's biomass-space result).
///
/// **`managed` is rung 3 only** (slice 7): it marks "this source's harvest cannot overdraw", and only
/// the rungs you own qualify. A *tended* patch is still a wild stand on a better curve — it draws
/// down and can be over-farmed — so it takes the ordinary branch, ⚠ and all.
///
/// Both signals are computed for **every** rung, from the one expected take: the two rung-kinds differ
/// in what their ceilings mean, never in whether the crew has to carry the food home.
pub(crate) fn forecast_source_yield(
    forecast: &SourceYieldForecast,
    sustainable: f32,
    managed: bool,
    workers: u32,
    policy: FollowPolicy,
) -> SourceYield {
    let production = forecast.ceiling_for(policy);
    let actual = forecast_expected_take(forecast, workers, policy);
    SourceYield {
        actual,
        sustainable: if managed { actual } else { sustainable },
        wasted: (production - actual).max(0.0),
        workers_needed: workers_needed_for_take(actual, forecast.per_worker_yield, workers),
    }
}

/// The assign-time yield telemetry seed for a **Hunt** source: what staffing `herd` with `workers`
/// hunters under `policy` will pay next turn, in the same shape the Hunt arm of
/// `advance_labor_allocation` records after the take. Reuses `hunt_forecast` (hence `hunt_take`'s own
/// ceiling/conversion helpers) and the shared MSY `sustainable_yield`, so the seed is exactly the
/// number the turn then produces — no jump. The plant mirror is `forage::forage_source_yield_preview`.
pub fn hunt_source_yield_preview(
    herd: &Herd,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    output_multiplier: f32,
    workers: u32,
    policy: FollowPolicy,
) -> SourceYield {
    let forecast = hunt_forecast(
        herd,
        fauna,
        ladder,
        per_worker_biomass_capacity,
        output_multiplier,
    );
    let sustainable = hunt_provisions(
        sustainable_yield(
            herd.biomass,
            herd_capacity(herd, fauna),
            &herd_ecology(herd, fauna),
        ),
        fauna,
        output_multiplier,
    );
    forecast_source_yield(&forecast, sustainable, herd.is_corralled(), workers, policy)
}

/// **THE single source of the per-policy hunt take ceiling** (in *biomass*) at a herd's current
/// stock, shared by every hunter of a herd: the band's Hunt labor arm and the scout's opportunistic
/// replenish (via `systems::hunt_take`), the hunting expedition (via
/// `systems::hunt_expedition_ceiling` / `hunt_trip_forecast`), and the pre-commit forecast
/// (`hunt_forecast`). One word, one meaning:
/// - **Sustain** — the **Maximum Sustainable Yield** flow ([`sustainable_yield`]): regrowth at the
///   most-productive biomass (K/2), so a herd at capacity still yields a positive skim and a
///   collapsing (sub-Allee) herd yields nothing. This is a *flow* ceiling, not a stock target.
/// - **Surplus** — that × `follow.surplus_multiplier` (overdraw → slow decline).
/// - **Market** — a commercial share `market.take_fraction × biomass` (fast decline).
/// - **Eradicate** — the one-shot max take `hunt.take_from(biomass)` (drives extinction).
/// - **Tame** / **Corral** — the *investment dips* while the herd is gentled / the pen is built: the
///   `animal:pastoral` / `animal:pen` rung's `yield_fraction_while_building ×` the MSY ceiling
///   (reusing the same [`sustainable_yield`] helper — never a second ecology), so the preparing take
///   is sustainable and the herd stays healthy while the crew works.
/// - **Cultivate** — Forage-only; a *hunt* ceiling for it is meaningless. Yields `0.0`, the symmetric
///   defensive case to `forage::forage_policy_ceiling`'s `Corral` arm (both are rejected at
///   `assign_labor` by [`FollowPolicy::valid_for_hunt`] / `valid_for_forage`).
///
/// `ecology` + `carrying_capacity` are **the herd's own** — resolved by [`herd_ecology`] /
/// [`herd_capacity`], never by the caller reaching for `fauna.ecology` or `herd.carrying_capacity`
/// directly. The husbandry ladder is expressed *entirely* by handing this function a different
/// ecology (wild `r` = 0.05 / pastoral 0.15 / pen 0.60), so a call site that re-derives one silently
/// hunts a tame herd on the wild curve.
///
/// Not clamped to biomass — the caller does that alongside its own throughput / carry-room cap.
pub fn hunt_policy_ceiling(
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
) -> f32 {
    match policy {
        FollowPolicy::Sustain => sustainable_yield(biomass, carrying_capacity, ecology),
        FollowPolicy::Surplus => {
            sustainable_yield(biomass, carrying_capacity, ecology) * fauna.follow.surplus_multiplier
        }
        FollowPolicy::Market => fauna.market.take_fraction * biomass,
        FollowPolicy::Eradicate => fauna.hunt.take_from(biomass),
        // The two investment dips, read off the ladder's own rungs — the same seam the plant side's
        // Cultivate dip reads, so every rung's investment cost is tuned in one file.
        FollowPolicy::Tame => {
            sustainable_yield(biomass, carrying_capacity, ecology)
                * ladder
                    .rung(RungKey::AnimalPastoral)
                    .yield_fraction_while_building()
                    .expect("the pastoral rung is an investment — it has a build meter")
        }
        FollowPolicy::Corral => {
            sustainable_yield(biomass, carrying_capacity, ecology)
                * ladder
                    .rung(RungKey::AnimalPen)
                    .yield_fraction_while_building()
                    .expect("the pen rung is an investment — it has a build meter")
        }
        // The plant-only investment rungs — rejected on a Hunt assignment at `assign_labor`
        // (`FollowPolicy::valid_for_hunt`). Unreachable in practice; defensively yield nothing rather
        // than silently hunting under a nonsense policy. The symmetric twin of
        // `forage_policy_ceiling`'s `Tame | Corral` arm.
        FollowPolicy::Cultivate | FollowPolicy::Sow => 0.0,
    }
}

/// The single biomass→provisions conversion for a hunt take: `take × hunt.provisions_per_biomass ×
/// output_multiplier` (the caller's productivity). Shared by `systems::hunt_take` (which quantizes the
/// result onto the larder's `Scalar` grid) and the pre-commit forecast, so the two can't drift. FOOD
/// income is fully fractional — a few hunters may yield < 1 provision per turn.
pub fn hunt_provisions(biomass_take: f32, fauna: &FaunaConfig, output_multiplier: f32) -> f32 {
    biomass_take * fauna.hunt.provisions_per_biomass * output_multiplier
}

/// The **gross** managed yield a **penned** herd hands its keeper each turn, in provisions: the pen's
/// MSY ([`pen_yield_biomass`]) through the shared biomass→provisions conversion. Gross, deliberately —
/// the pen's feed ([`pen_upkeep`]) is a *separate* debit on the keeper's larder, so the player can see
/// both halves of the trade instead of one netted number.
///
/// Shared by the corral-tend branch of `advance_labor_allocation` (the payout) and [`hunt_forecast`]
/// (the forecast + the "what will this herd pay once penned?" projection), so forecast == actual.
pub(crate) fn corral_provisions(herd: &Herd, fauna: &FaunaConfig, output_multiplier: f32) -> f32 {
    hunt_provisions(pen_yield_biomass(herd, fauna), fauna, output_multiplier)
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
    output_multiplier: f32,
) -> SourceYieldForecast {
    // The pen's yield is **gross** — its feed is debited separately (wire: `penUpkeep`).
    //
    // A pen collapses the *policy* axis (the herd is yours) but **not** the worker cap: the keeper
    // still has to carry the meat home, so `managed` gets the same real per-hunter throughput a wild
    // hunt is capped by, and the pen's take is `min(pen MSY, hunters × throughput)` (slice 7 — the
    // Field's twin, and the same fix: the old `::tended` claimed one keeper collected the whole pen).
    if herd.is_corralled() {
        return SourceYieldForecast::managed(
            corral_provisions(herd, fauna, output_multiplier),
            hunt_provisions(
                per_worker_biomass_capacity.max(0.0),
                fauna,
                output_multiplier,
            ),
        );
    }
    let ecology = herd_ecology(herd, fauna);
    let capacity = herd_capacity(herd, fauna);
    let ceiling = |policy| {
        hunt_provisions(
            hunt_policy_ceiling(policy, herd.biomass, capacity, &ecology, fauna, ladder)
                .clamp(0.0, herd.biomass),
            fauna,
            output_multiplier,
        )
    };
    SourceYieldForecast {
        per_worker_yield: hunt_provisions(
            per_worker_biomass_capacity.max(0.0),
            fauna,
            output_multiplier,
        ),
        ceiling_sustain: ceiling(FollowPolicy::Sustain),
        ceiling_surplus: ceiling(FollowPolicy::Surplus),
        ceiling_market: ceiling(FollowPolicy::Market),
        ceiling_eradicate: ceiling(FollowPolicy::Eradicate),
        // The investment rung: what the herd pays *while the pen is built* (Corral — the dip, on the
        // herd's CURRENT ecology), and what it will pay *once penned* (the pen's MSY, which is why
        // `corral_provisions` takes the raw capacity rather than a penned herd) — so the client can
        // show "preparing X → then Y" before the player commits to the 25-turn cost.
        ceiling_prepare: ceiling(FollowPolicy::Corral),
        // The rung below: what the herd pays *while it is being tamed* (the `animal:pastoral` dip).
        ceiling_tame: ceiling(FollowPolicy::Tame),
        // `Sow` is plant-only — a herd has no field rung, and `hunt_policy_ceiling` yields `0` for
        // it. Resolved through the same `ceiling` closure rather than a literal, so the "not a hunt
        // policy" rule stays stated in exactly one place (the mirror of `forage_forecast`'s
        // `ceiling_tame`).
        ceiling_sow: ceiling(FollowPolicy::Sow),
        managed_yield: corral_provisions(herd, fauna, output_multiplier),
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
const MSY_BIOMASS_FRACTION: f32 = 0.5;

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

fn determine_herd_count(width: u32, height: u32) -> u32 {
    let area = width.saturating_mul(height).max(1);
    let baseline = area / 3000;
    baseline.clamp(2, 6)
}

/// Radius (hexes) of the neighbourhood `build_route` searches to pull a migratory anchor onto the
/// most fertile nearby ground (Grazing 2b-i §4.1). Small — a local nudge that shifts the anchor onto
/// grass without redrawing the spiral's shape.
const ANCHOR_FERTILITY_SCAN_RADIUS: u32 = 1;

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
    use crate::intensification::RUNG_COMPLETE;
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};

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

    // ---- Grazing Phase 2b-i ----------------------------------------------------------------

    use crate::graze::{GrazePatch, GrazeRegistry};

    /// Wild per-species regrowth rate for the 2b-i grazing harnesses (inert on `K`, so any live rate
    /// works); the global wild default the retired single ecology used.
    const WILD_TEST_REGROWTH_RATE: f32 = 0.05;

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
        )
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
        let forecast_before = hunt_forecast(&before, &fauna, &LadderConfig::builtin(), 40.0, 1.0);

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
        let forecast_after = hunt_forecast(after, &fauna, &LadderConfig::builtin(), 40.0, 1.0);
        assert_eq!(
            forecast_before.ceiling_sustain, forecast_after.ceiling_sustain,
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
}
