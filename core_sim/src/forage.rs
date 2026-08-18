//! Depletable forage patches (Intensification §0-ii — "forage parity with hunting").
//!
//! Transposes the herd biomass / logistic-regrowth model (`fauna.rs`) onto worked forage tiles.
//! Every `FoodModuleTag` tile gains a live per-patch `{ biomass, carrying_capacity, ecology_phase }`
//! (`ForagePatch`) held in the authoritative `ForageRegistry` resource, keyed by tile coord.
//!
//! **This is the HUMAN food web** — seeds, nuts, tubers, fruit, shellfish — and its capacity is a
//! property of the **land**: `forage.capacity_by_biome`, a per-biome table over the 37 biomes
//! (`labor_config.json`). Its twin is the *animal* food web, `graze.capacity_by_biome`
//! (`fauna_config.json`, `graze.rs`), and **the two are meant to disagree**: a closed-canopy
//! woodland is rich in mast and poor in pasture, a prairie steppe is the reverse, a silt floodplain
//! is cropland rather than range. *Your best farm is not your best pasture*
//! (`docs/plan_grazing_foundation.md` §1). The `FoodModuleTag` still decides what **kind** of
//! gathering a tile offers (and its `seasonal_weight`); the table decides **how much** is there.
//! Foraging **draws the patch down** (`forage_take`), and `advance_forage_regrowth` regrows it each
//! turn toward `carrying_capacity`. The patch's state round-trips through rollback because the
//! checkpoint carries the whole `ForageRegistry` (`SimState::forage`) — the same way the
//! `HerdRegistry` persists.
//!
//! Unlike a wild herd, a patch uses **pure logistic regrowth** (no Allee / critical-depensation
//! crash) and **never despawns** — plants reseed, so a depleted (feral) patch always recovers. A
//! small **reseed floor** (`forage.reseed_floor_fraction × carrying_capacity`) lifts a fully-depleted
//! patch back to a seed stock before regrowth each turn, so even a patch driven to exactly `0`
//! (Eradicate / f32 underflow / a restored `biomass = 0`) recovers rather than sticking at `0`. The
//! Allee branch of `net_biomass_delta` (via `sustainable_yield`) still sizes the **Sustain** gather
//! ceiling (so a collapsed patch yields no sustainable surplus). Foraging honors the full policy axis
//! (Sustain/Surplus/Deplete/Eradicate — §0-iii, parity with hunting): the `LaborTarget::Forage`
//! policy flows through `advance_labor_allocation` into `forage_take`, and a Deplete gather sells its
//! take as trade goods.
//!
//! **Cultivation** (Phase 1a) is the plant mirror of `fauna.rs`'s corral — an *investment*, not a
//! by-product of gathering (authoritative spec: `core_sim/CLAUDE.md` → Cultivation):
//! - A **Sustain** forage on a **Thriving** patch earns the faction **Cultivation knowledge**
//!   (`CULTIVATION_DISCOVERY_ID`, in the `DiscoveryProgressLedger`) — the gate on the policy below.
//!   Sustain **never** accrues a patch's `cultivation_progress`.
//! - Taming a patch means **queueing the `Cultivate` and staffing the band's `builders` pool**: the
//!   whole pool banks work units toward the `plant:tended` rung's `work_cost` (read off the shared
//!   ladder, `crate::intensification`) while this patch is the **head** of that queue, and the
//!   gatherers beside them carry exactly what they always did — what a Cultivate costs is the people
//!   who are clearing instead (`docs/plan_standing_upkeep.md` §2.5). The `cultivate` command only
//!   **appends the entry** on bands already foraging the tile; it names no crew and claims nothing.
//! - A completed ("tended") patch pays only the band that **tends it** (a Forage assignment worked it
//!   this turn — place-local, in `advance_labor_allocation`) a higher-than-wild yield without drawing
//!   biomass down; `advance_cultivation` takes an **untended** patch **feral** (progress decays back
//!   below the cultivated threshold, reverting it to a wild gather patch).
//!
//! **The Field** (rung 3, slice 5) is the same patch one rung up: `Sow` fills `field_progress`, and a
//! completed Field pays its workers `biomass × field_provisions_per_biomass` — the tended patch's
//! shape at twice the rate. It needs no *patch* below it: sowing a qualifying tile that spawned none
//! *creates* one (`ForagePatch::sown`), at that tile's own biome capacity.
//!
//! **Where it may be sown is SCARCE, and that is the mechanic** — the tile must be a **gathering
//! site** (the curated `FoodSiteRegistry`) *and* **near fresh water**: rung 3 can move seed but not
//! water, and does not yet work ground its people do not already gather. The `plant:field` rung's
//! `site_requirement` states it; `rung_site_refusal` + `tile_is_fresh_watered` are the one seam the
//! command, the labor arm and the wire all judge through.
//!
//! **This reversed the earlier rule that Sow "needs no source below it — seed travels", under which
//! any sufficiently fertile, watered tile was a legal target (46 of 4160 on the standard map).** The
//! problem was reach: gathering is itself site-bound, so the only tiles a band works are gathering
//! sites, and ground that qualified on fertility alone could never be occupied to sow. "Seed travels"
//! is now rung 4 (Farm)'s identity — the first rung to drop the gathering-site term, with a fertility
//! floor back in its place. Design: `docs/plan_intensification_ladder.md` §2.

use std::{borrow::Cow, collections::HashMap};

use bevy::prelude::*;

use crate::{
    components::{take_overdraws, Improvement, SourceYield, TakeSelection, Tile},
    fauna::{
        classify_ecology_phase, escapement_ceiling, floor_reach_band, forecast_source_yield,
        peak_regrowth_between, reseeding_logistic_regrowth, sustainable_yield, EcologyPhase,
        SourceYieldForecast, NO_PASTORAL_YIELD,
    },
    fauna_config::{EcologyConfig, YieldAccounts},
    flora_config::{FloraConfig, FloraShare},
    food::FoodModuleTag,
    intensification::{
        interpolate, rung_span, upkeep_shortfall, BuildLeg, LadderConfig, LadderConfigHandle,
        RungBranch, RungDef, RungKey, RungStanding, SiteRefusal, LEG_ALREADY_PAID, NEGLECT_NONE,
        NO_BUILD_GEAR, NO_CREW_ON_THIS_ACTIVITY, NO_RUNG_CREDIT, NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND,
        RUNG_COST_UNSCALED, RUNG_UNSTARTED,
    },
    labor_config::{ForageLaborConfig, LaborConfigHandle, NO_FORAGE_CAPACITY},
    materials_config::MaterialPayoff,
    orders::FactionId,
    resources::{CommandEventEntry, CommandEventKind, CommandEventLog, SimulationTick},
    scalar::{scalar_from_f32, Scalar},
};

/// Discovery id for the faction-level **Cultivation** knowledge (Intensification Rung 1b — the
/// earned-knowledge gate on the plant path, `docs/plan_intensification.md` §4b). Knowledge is
/// **earned by doing**: a band Sustain-foraging a Thriving patch accrues this discovery in the
/// per-faction `DiscoveryProgressLedger` (`advance_labor_allocation`), and a patch cannot become a
/// tended crop until the faction knows Cultivation. Declared as a start-profile knowledge tag
/// (`cultivation` → this id in `data/start_profile_knowledge_tags.json`) purely so it is mappable;
/// it is deliberately **not** listed in any start profile's `starting_knowledge_tags`, so no faction
/// starts knowing it. Next free id after `nomadic_wayfinding` (2001) / `portable_forge` (2002).
pub const CULTIVATION_DISCOVERY_ID: u32 = 2003;

/// Discovery id for the faction-level **Seed Selection** knowledge — the plant ladder's **rung-3**
/// gate (`docs/plan_intensification_ladder.md` §2a/§4.3), and the twin of `fauna::PENNING_DISCOVERY_ID`.
///
/// **Earned by practising rung 2**: working a *tended* patch under a stewardship policy teaches it
/// (`RungDef::knowledge_earned`, driven by the `plant:tended` rung's `earns_knowledge`) — you learn
/// to select seed by *farming*, not by gathering wild stands. Like every other ladder knowledge it is
/// declared as a start-profile knowledge tag (`seed_selection` → this id in
/// `data/start_profile_knowledge_tags.json`) purely so it is mappable, and is deliberately **not**
/// listed in any start profile's `starting_knowledge_tags` — nothing on the ladder is start-granted.
///
/// **Its consumer landed in slice 5**: it gates the `Sow` verb (the `plant:field` rung's
/// `unlock_knowledge`), so a faction may only place a Field once it has learned to select seed by
/// farming. Earned in slice 4, spent here — a knowledge you accumulate before its verb exists is
/// exactly the "practice paces the ladder" model. Next free id after `herding` (2004).
pub const SEED_SELECTION_DISCOVERY_ID: u32 = 2005;

/// **The gather season of a tile with no `FoodModuleTag`** — i.e. no wild gather at all: the season
/// scales a forager's *throughput* (`forage_per_worker_biomass`), so a zero here means no worker can
/// gather anything there, which is exactly right for ground the wild put no food site on.
///
/// It became a reachable reading in slice 5: `Sow` places a Field on any ground the `plant:field`
/// rung's `site_requirement` accepts — module or not — so a patch may now stand on a tile with no
/// module. Such a patch offers nothing to
/// **gather** — the only thing to work there is the crop you sowed, whose managed harvest is
/// biomass-based and seasonless (`field_provisions`). Shared by the Forage labor arm, the assign-time
/// yield seed and the snapshot forecast, so all three read the same "no season" answer.
pub const NO_FORAGE_SEASON: f32 = 0.0;

// **RETIRED: `MANAGED_HARVEST_SEASON`** — full weight, always, because a Field's crop stood where
// you planted it and its harvest was seasonless. It is gathered like every other plant rung now, so
// it reads the tile's own `seasonal_weight` with everything else.

/// A live depletable forage patch on a `FoodModuleTag` tile. Mirrors the herd biomass model's
/// ecology subset, including cultivation (`cultivation_progress`/`owner`) — the plant analog of a
/// herd's domestication (Phase 1a).
#[derive(Debug, Clone)]
pub struct ForagePatch {
    /// Tile the patch sits on (its registry key).
    pub tile: UVec2,
    /// Live gatherable stock, drawn down by `forage_take`, regrown by `advance_forage_regrowth`.
    pub biomass: f32,
    /// Per-patch carrying cap that biomass regrows toward — **the tile's**, seeded from
    /// `forage.capacity_by_biome[terrain]` (the human food web's per-biome table), never a global
    /// constant. The exact counterpart of `GrazePatch::carrying_capacity`.
    pub carrying_capacity: f32,
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from biomass vs
    /// `carrying_capacity`. Lights the client over-forage readout the same way herds do.
    pub ecology_phase: EcologyPhase,
    /// **HOW FAR UP ITS BRANCH THIS PATCH HAS BEEN WORKED, in cumulative work units** — the plant
    /// web's **one** meter (`docs/plan_standing_upkeep.md` §2.8). `plant:tended` runs `0 → 50` and
    /// `plant:field` `50 → 125`, each rung's span being its own `build.work_cost`, so a position of
    /// `60` is a whole tended patch plus a Field one seventy-fifth of the way up.
    ///
    /// # IT REPLACED TWO METERS, AND THE STATE THAT ALLOWED IS WHY
    ///
    /// `cultivation_progress` + `field_progress` were **independent**, so *"Field at 1% while
    /// Cultivation reads 99%"* was representable and had to be policed by rules that missed it. Here
    /// it cannot be written down: a Field's range begins where the tended rung's ends. **Decay eats
    /// from the top for free** — a Field at 10% decaying is this number falling `57.5 → 50`, reaching
    /// the tended rung's range only once the Field is wholly gone — which is *"if Sow is above 0%,
    /// cultivation can never decrease"* as arithmetic rather than as an invariant somebody enforces.
    ///
    /// # ⛔ PRIVATE, AND THAT IS THE MECHANISM RATHER THAN A STYLE CHOICE
    ///
    /// [`Self::standing`] is derived from it and is what the ladder-free predicates and every rate
    /// seam read. A public field would let a caller move one without the other, and this arc has
    /// already shipped three defects of exactly that shape. **The only way to move it is
    /// [`Self::set_ladder_position`]**, which takes the ladder and writes both together, so drift is
    /// unrepresentable rather than merely discouraged.
    ladder_position: f32,
    /// **WHERE THIS PATCH STANDS — DERIVED, AND RE-STAMPED ON EVERY WRITE TO
    /// [`Self::ladder_position`]** ([`RungStanding`]).
    ///
    /// **It is stored rather than resolved on demand because the readers hold no config.**
    /// `is_cultivated()` has ~a hundred call sites all over the plant web, and the rate seams run on
    /// paths that carry no `LadderConfig` — which is exactly why the retired `standing_rung` was
    /// written ladder-free in the first place. Stamping is what lets the predicates keep their
    /// signatures while the *boundaries* they test come from live config.
    ///
    /// It is not a cache anyone may refresh: there is one writer, and it writes the pair.
    standing: RungStanding,
    /// **How many more turns a build on this patch needs, at the crew, floor and kit that worked it
    /// this turn** — stamped by the labor arm and published as
    /// `ForagePatchState.buildTurnsRemaining`.
    ///
    /// **It is a PROJECTION when nothing is being built**, and that is the field's point: with a verb
    /// in flight it is [`crate::intensification::build_turns_remaining`] on the running meter, and
    /// with none it is [`crate::intensification::LadderConfig::projected_build_turns`] on the rung
    /// this patch would climb **next**, so the compose sheet can quote the job before the player
    /// commits. Same rule as `HerdTelemetryState.penUpkeep`: always meaningful, never
    /// zero-because-not-started.
    ///
    /// `None` = **no estimate**, and it means there genuinely is no answer: the patch is a Field (the
    /// top of the plant ladder, nothing left to build), the next rung's own site/knowledge/species
    /// gates refuse it for this faction, or the crew produced nothing this turn and a running build is
    /// stalled. **The client cannot compute any of it** (it holds neither the crew's output, nor the
    /// floor multiplier, nor the kit), which is why the sim answers — the `penFeedUpkeep` discipline.
    ///
    /// Transient per-turn scratch on the same one-turn cycle as [`Self::tended_this_turn`]: written
    /// in Population, cleared by `advance_cultivation` in the *next* turn's Logistics, so the value
    /// the Snapshot stage captures is always the turn's own.
    /// `None` is the wire's [`sim_schema::NO_BUILD_TURNS_ESTIMATE`],
    /// [`crate::intensification::BuildTurns::Holding`] its [`sim_schema::BUILD_METER_HOLDS`] and
    /// [`crate::intensification::BuildTurns::Rotting`] its [`sim_schema::BUILD_METER_ROTS`] — three
    /// negatives, three facts, because *"there is no answer"*, *"the meter is standing still"* and
    /// *"the meter is going backwards"* are not the same thing to a player who has already
    /// committed a crew.
    pub build_turns_remaining: Option<crate::intensification::BuildTurns>,
    /// **What the crew's TOOLS ADD to this patch's running build, per turn**, in work units —
    /// [`crate::intensification::gear_work_supply`] over the pool, published as
    /// `ForagePatchState.buildWorkFromGear` so a readout can say *"your hoes: +9 work a turn"*
    /// against a price that does not move under it.
    ///
    /// **It is an ADDEND on the pool's output, never a deduction from the job**
    /// (`docs/plan_standing_upkeep.md` §4.8) — `work_cost` is the same pile with hoes and without.
    ///
    /// [`crate::intensification::NO_BUILD_GEAR`] when no build is in flight or the crew carries
    /// nothing that helps — a pool sent out bare, or one carrying the animal web's `hurdles`, whose
    /// `build_work` names the branch it serves. Transient per-turn scratch on
    /// [`Self::build_turns_remaining`]'s cycle, and for its reason: the kit is re-read every turn, so
    /// no state may record *"this build was geared"*.
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
    /// **WHY THE BAND'S BUILDERS ARE STUCK ON THIS SOURCE** — the conjunct of the rung's own gate
    /// that refused ([`crate::intensification::BuildGate`]), or [`crate::intensification::BuildGate::Open`]
    /// (wire key `""`) whenever this source is not a blocked build
    /// (`docs/plan_standing_upkeep.md` §4.6b).
    ///
    /// **It exists because [`crate::intensification::BuildTurns::Blocked`] states only THAT the
    /// queue is stuck.** The measured playtest sat on a blocked Tame for turns, fixing the one cause
    /// a surface happened to name while the real refusal — the herd below its escapement floor —
    /// went unmentioned by every surface in the game. The sim decides `eligible`, so the sim says
    /// why; a client re-deriving it would be a second producer of one verdict.
    ///
    /// **It rides the same winner as [`Self::build_turns_remaining`]** and is **carried down the
    /// queue** with the sentinel: everything behind a blocked head is stuck for the head's reason.
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

    /// **The named plant this patch is COMMITTED to** — a `flora_config.json` species key, or `None`
    /// for the **wild mixed basket** (`docs/plan_flora_roster.md` §4.2/§4.3). Stored as the config
    /// key rather than the display name because the key is what `FloraConfig::species` and
    /// `FloraShare::species` are keyed by; the animal side stores a display name on `Herd::species`
    /// only because *its* roster is looked up that way.
    ///
    /// **Set on the first turn a crew works this patch under `Cultivate` or `Sow`** (the assignment's
    /// selection, or the highest-share species in this tile's basket that the rung's
    /// `cultivation_ceiling` permits), and fixed from then on. **Cleared when both improvement meters
    /// lapse to zero** ([`ForagePatch::reconcile_owner`]) — a patch that has gone fully feral is a
    /// wild stand again, and a wild stand is the whole basket.
    ///
    /// What the commitment *does* is two things and only two ([`patch_composition`] /
    /// [`patch_provisions_per_biomass`]): it **reweights** the tile's basket toward this one plant
    /// (weeding at rung 2, planting at rung 3 — the tile's `K` never moves, because the land owns
    /// it), and it changes how well biomass **converts** in every account (the tended rung's
    /// `tended_conversion_gain`, on this species' term alone). Both take effect only once the
    /// improvement is *complete* — while a crew is still preparing, the stand is still the mixed
    /// basket it started as.
    pub species: Option<String>,
    /// Faction tending/owning this patch (`Some` iff either improvement meter is `> 0`).
    pub owner: Option<FactionId>,
    /// **How many consecutive turns this patch's standing upkeep has gone unmet** — the counter
    /// `advance_cultivation` gates the feral bleed on. Reset to [`NEGLECT_NONE`] on any turn the
    /// keepers met the demand ([`Self::upkeep_supplied`] covers the at-risk rung's
    /// [`RungDef::upkeep_demand`]); incremented on every turn they did not. The bleed applies only
    /// while this exceeds that rung's [`crate::intensification::RungUpkeep::grace_turns`], so a crew
    /// may be away for a few turns — re-tasked, raided, following a herd — without the patch starting
    /// to revert.
    ///
    /// **It counts SHORTFALL turns, not un-worked ones, and that is the behavioural headline of
    /// `docs/plan_standing_upkeep.md` §2.4.** The retired `tended_this_turn` flag was set by *any*
    /// crew on the tile, so a patch somebody was **gathering** was spared for free — holding an
    /// improvement cost nothing as long as you were taking from it. Holding is its own allocation
    /// now: a patch being gathered but not *maintained* has an unmet demand and does start to revert.
    ///
    /// **The requirement stays per-SOURCE, not per-band** (the keepers of every band on the tile sum
    /// into one [`Self::upkeep_supplied`]), but it is no longer **binary**: half the hands a patch
    /// needs is half a shortfall and bleeds at half rate, which is precisely what the flag could not
    /// express.
    ///
    /// Rides the checkpoint with the rest of the registry, so a rollback rewinds the grace along with
    /// the meter it protects — otherwise a restore could hand a patch a fresh grace it had already
    /// spent.
    pub neglect_turns: u16,
    /// **WHAT THE AT-RISK METER'S OWN CREW SUPPLIED THIS TURN**, in work units — the **keepers**
    /// once that rung is built and the **builders** while it is not
    /// ([`patch_upkeep_supply`], `docs/plan_standing_upkeep.md` §2.4), stamped by the labor arm that
    /// resolved it.
    ///
    /// **It is the ONE stored fact of the upkeep**, and the reason is that it is the only one a crew
    /// authors: the demand is the at-risk rung's ([`patch_unwinding_rung`]) and the shortfall is the
    /// difference, so both are derived wherever they are needed rather than stored twice and left to
    /// drift. It rides the same one-turn cycle as [`Self::build_turns_remaining`] — written in
    /// Population, read and cleared by `advance_cultivation` in the *next* turn's Logistics — which
    /// is exactly the carry-across-turns signal the retired `tended_this_turn` flag was, and it
    /// survives a rollback for the same reason (the checkpoint clones the whole `ForageRegistry`), so
    /// the first post-restore decay pass does not bleed a patch whose keepers are still on it.
    pub upkeep_supplied: f32,
    /// **THE BILL THIS PATCH'S KEEPERS WERE HANDED** — the demand [`patch_upkeep_demand`] answered
    /// when [`Self::upkeep_supplied`] was stamped, and `None` on a turn nobody answered for the
    /// source at all.
    ///
    /// # ⛔ IT EXISTS BECAUSE THE DEMAND MOVES NOW, AND A LAGGED SUPPLY AGAINST A MOVING BILL IS
    /// PERMANENTLY SHORT
    ///
    /// The supply is stamped in **Population** and judged by the **next** Logistics pass, and while
    /// the demand was the rung's flat rate that carry was exact. Interpolation makes it a moving
    /// target: a build banks work between the stamp and the judgement, so the demand the pass reads
    /// is always a little above the one the keepers were asked to cover. Measured on a three-entry
    /// build queue with the `agriculture` role fully staffed, that gap bled **~0.03 work a turn**
    /// off the very meter it was funding — a permanent shortfall on a correctly-played band, which
    /// also re-armed `neglect_turns` every turn and left the wire counting down a grace that could
    /// never reset.
    ///
    /// So the pass judges the **bill that was issued**, not the one that has since risen. `None`
    /// means no band answered for this source this turn — an abandoned patch — and the live demand
    /// is then the honest basis, because the whole of it went unmet.
    ///
    /// # ⛔ THE FIRST BAND TO REACH THE SOURCE WRITES IT, AND NOBODY OVERWRITES IT
    ///
    /// The demand is per-source, but that does **not** make it safe for every band to assign the
    /// same number: the position it interpolates on moves *between* band visits, because each
    /// band's build accrual runs inside its own arm. Every band's keeping share, meanwhile, was
    /// split from the pool **before** the loop, at the position as it stood then — so a later
    /// band's stamp is a bill nobody was handed, and two bands on one patch (one keeping it, one
    /// building it) judged a correctly-staffed keeping short in one of the two visit orders. The
    /// shares are struck at the pre-accrual position, so the bill is too.
    ///
    /// Transient per-turn scratch on [`Self::upkeep_supplied`]'s exact cycle, and for its reason.
    pub upkeep_demanded: Option<f32>,
    // **RETIRED before it shipped: `ForagePatch::repair_verb`** — a per-turn flag stamped on the edge a
    // completed meter fell below its cost, so the labor arm could re-stamp the rung's verb.
    //
    // **The verb is DERIVED from the meter now** ([`patch_build_verb`]), so there is no edge to catch and
    // nothing to re-stamp: a meter carrying progress *is* the declaration. An edge-triggered write would
    // have been a second authority beside the meter, and the two could disagree the moment anything else
    // touched a build.
}

// **RETIRED: `ForagePatch::maintain` / `Herd::maintain`** — a per-source boolean the player toggled
// to stop paying a standing upkeep.
//
// It answered *"is this being kept"* beside a crew that answered *"by how many hands"*, and the two
// could disagree: a source maintained by nobody and a source deliberately written off are the same
// state, said twice. **`LaborAssignment::maintain_workers == 0` is the whole of "stop maintaining
// this"** now (`docs/plan_standing_upkeep.md` §2.2), and it lives with the band that would have
// supplied the hands rather than on the ground they stand on.

/// **WHICH RUNG THIS PATCH IS BUILDING** — derived from its meters, with the player's declaration
/// answering only for a meter that is at zero.
///
/// # THE METER IS THE DECLARATION
///
/// | meter | state | who declares |
/// |---|---|---|
/// | **zero** | nothing in flight | **the player** — a wild patch could climb to tended *or* be sown, and the sim cannot guess |
/// | **between zero and its cost** | building that rung, **implied** | nobody — the progress banked on it *is* the answer |
/// | **at its cost** | maintaining | nobody |
///
/// **Per METER, not per source**: a completed tended patch the player wants to sow is still a
/// declaration, because its field meter is at zero.
///
/// # WHAT THIS FIXED
///
/// `RungDef::build_accrual` banks nothing unless the rung's verb is in flight, and completion cleared
/// the stored verb — so a completed rung that eroded back below its cost re-entered the *building*
/// state with nothing set, accrued nothing, and could not be repaired until the player re-issued
/// `cultivate`/`sow`. They never withdrew that intent. **A player who has paid for a rung and watched
/// it slip adds hands, not a command.**
///
/// # AND IT RETIRED `abandon_improvement`
///
/// That command existed to let a player walk away from a 25-turn commitment while the *verb* was the
/// commitment. A command that cleared a *derived* value would either do nothing or fight the
/// derivation, and there is no stored authority left for it to clear. **The undo is its own verb
/// now**: `unqueue <faction> <source…>` withdraws the declaration and leaves the row, the take crew,
/// the kit and the meter alone, and `abandon` puts the whole holding down
/// (`docs/plan_standing_upkeep.md` §2.5).
///
/// **The declaration is honoured only at a zero meter**, which is what makes a spent one inert: once
/// the first accrual lands, the meter answers and the stale declaration is filtered out here rather
/// than having to be cleaned up somewhere.
pub fn patch_build_verb(patch: &ForagePatch, declared: Option<Improvement>) -> Option<Improvement> {
    // **THE RUNG BEING RAISED IS THE FLOOR OF THE ANSWER.** `None` at the top of the branch: a Field
    // has nothing above it, so a `Cultivate` declared on one is dead rather than stalled — the case
    // that used to need the retired "nothing left to build" test.
    let raising = patch.standing().raising?;

    // **A LIVE DECLARATION NAMES THE JOB, and it may name a rung ABOVE the one in flight.** A `sow`
    // on untended ground is the player asking for a Field: the crew works toward `plant:field` and
    // the position climbs the tended rung's range on the way, so the Field's own work still cannot
    // exist without that ground under it (`docs/plan_standing_upkeep.md` §2.8, rule 1's *outcome*).
    //
    // **The kit is the SOW's, and that is the deliberate departure from rule 1's argument.** Rule 1
    // reasons that a Sow's work must not implicitly finish the clearing beneath, because the two jobs
    // want different tools. Deriving `Cultivate` there would honour the tool and **dead-end the rung**:
    // `Sow` is the create-from-nothing verb, its accrual deliberately carries no work predicate
    // (bare ground stands below every floor by construction), and the Cultivate arm's gate does carry
    // one — so a bare-ground Sow would bank nothing, for ever, with the queue entry live. The whole
    // capability rung 3 exists for would go. The kit question is left open rather than answered by
    // making the verb unusable.
    //
    // A declaration **below** the rung in flight is dead — a `Cultivate` on ground already tended —
    // which is the state the retired two-meter form could not tell apart from a stalled one.
    if let Some(target) = declared.map(RungKey::built_by) {
        if target.is_at_or_above(raising) {
            return target.builder_verb();
        }
    }

    // **WITH NO USABLE DECLARATION THE METER ANSWERS FOR ITSELF** — work banked on the rung in
    // flight *is* the statement that this rung is being raised, which is what lets a player who has
    // watched a rung slip add hands rather than a command. Nothing banked and nothing declared is a
    // live entry that has simply not started, and answers `None`.
    (patch.standing().credit > NO_RUNG_CREDIT)
        .then(|| raising.builder_verb())
        .flatten()
}

/// **IS THE RUNG THIS QUEUE ENTRY DECLARED ALREADY STANDING?** — the test that retires a **dead**
/// entry (`docs/plan_standing_upkeep.md` §2.5), and the plant twin of
/// `fauna::herd_rung_already_built`.
///
/// # IT IS THE STANDING, AND THERE IS NO LONGER A SECOND READING TO DISAGREE WITH
///
/// It used to ask the *meter's fullness* while the player-facing gate asked the *retention bar*, and
/// the gap between them was a rung the builders were legitimately repairing. The bar is deleted
/// (`docs/plan_standing_upkeep.md` §2.8), so *achieved* and *its meter is full* are one fact again:
/// `held` is at or above the declared rung.
///
/// # AND IT IS EMPHATICALLY NOT "THE DERIVED VERB IS `None`"
///
/// [`patch_build_verb`] also answers `None` for a source with nothing banked and nothing declared —
/// a live entry that has simply not started — so retiring on that would drop an entry the turn the
/// player made it.
pub fn patch_rung_already_built(patch: &ForagePatch, declared: Improvement) -> bool {
    let target = RungKey::built_by(declared);
    // A rung the animal web owns can never stand on ground; `is_at_or_above` answers `false` across
    // branches, so the cross-web case needs no arm of its own.
    patch.standing().held.is_at_or_above(target)
}

/// **WHERE A BUILD METER LANDS THIS TURN** — what was banked plus what the crew produced, capped at
/// the job's own cost so a pool that out-produces the remainder finishes at exactly `1.0` rather than
/// overshooting it (`build_fraction` clamps for display, but a stored meter above its cost would make
/// the `*WorkDone` / `*WorkCost` pair on the wire read as more than the whole job).
///
// **RETIRED: `banked_or_paid_off(banked, cost, effective_cost)`** — *"crossing the **effective** bar
// sets the meter to the RAW cost"*, the jump that reconciled a tooled bar with the untooled cost the
// four completion predicates compare against.
//
// **A JOB'S WORK REQUIREMENT NEVER CHANGES** (`docs/plan_standing_upkeep.md` §4.8): a kit raises what
// a builder delivers per turn, so there is one bar, it is the stamped cost, and there is nothing to
// reconcile. The jump was honest under the old model — those units really were worked, by the tool —
// but it existed only because the tool was allowed to pre-pay part of the pile.
pub(crate) fn banked_up_to_cost(banked: f32, cost: f32) -> f32 {
    banked.min(cost)
}

impl ForagePatch {
    /// A fresh patch at full biomass (= carrying capacity). Phase is `Thriving` until refreshed
    /// against the ecology config.
    pub fn new(tile: UVec2, carrying_capacity: f32) -> Self {
        Self {
            tile,
            biomass: carrying_capacity,
            carrying_capacity,
            ecology_phase: EcologyPhase::Thriving,
            ladder_position: RUNG_UNSTARTED,
            // The pair is written together here as everywhere else — `unstarted` is the walk's own
            // answer for a position of zero, stated ladder-free so worldgen needs no config.
            standing: RungStanding::unstarted(RungBranch::Plant),
            build_turns_remaining: None,
            build_work_from_gear: NO_BUILD_GEAR,
            build_queue_position: crate::intensification::NOT_IN_ANY_BUILD_QUEUE,
            build_blocked_reason: crate::intensification::BuildGate::Open,
            build_destination: None,
            build_legs: Vec::new(),
            species: None,
            owner: None,
            neglect_turns: NEGLECT_NONE,
            upkeep_supplied: NO_UPKEEP_DEMAND,
            upkeep_demanded: None,
        }
    }

    /// **A patch a crew has just put seed into** — the plant rung-3 verb's create-from-nothing case
    /// ([`crate::components::Improvement::Sow`] on hospitable ground that carried no forage site at all,
    /// `docs/plan_intensification_ladder.md` §2). It is an ordinary patch from this moment on: same
    /// biomass model, same **tile** capacity (`tile_forage_capacity` — the *same* source a wild patch
    /// is seeded from, never a Field-specific table), same logistic regrowth.
    ///
    /// It starts at the **reseed floor**'s standing crop, not at capacity: sown ground is seed, and
    /// the floor is already this module's word for "the smallest stand plants recover from". So a new
    /// Field is worth nothing on the turn it is placed and grows into its yield — which is also why
    /// the `Sow` accrual is *not* gated on the patch being Thriving (see `advance_labor_allocation`):
    /// a freshly sown tile is Collapsing by construction, and gating it would make sowing bare ground
    /// impossible.
    pub(crate) fn sown(tile: UVec2, carrying_capacity: f32, reseed_floor_fraction: f32) -> Self {
        Self {
            biomass: carrying_capacity * reseed_floor_fraction,
            ..Self::new(tile, carrying_capacity)
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the forage ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }

    /// **WHERE THIS PATCH STANDS ON THE PLANT LADDER** — the derived verdict, re-stamped on every
    /// write to the position. Every rate seam interpolates on it and every predicate reads it, so
    /// there is exactly one answer to *"where is this source"*.
    pub fn standing(&self) -> RungStanding {
        self.standing
    }

    /// **HOW FAR UP ITS BRANCH THIS PATCH HAS BEEN WORKED**, in cumulative work units. Read-only:
    /// [`Self::set_ladder_position`] is the only writer, and it writes [`Self::standing`] with it.
    pub fn ladder_position(&self) -> f32 {
        self.ladder_position
    }

    /// **⛔ THE ONE MUTATOR — it writes the position AND re-derives the standing, together.**
    ///
    /// Every accrual, every decay and every fixture completion funnels through here, floored at
    /// [`RUNG_UNSTARTED`] and capped at the top of the branch, because the pair must never be
    /// half-written: the standing is what a hundred ladder-free readers see, and a stale one would be
    /// a patch that pays a rung its position no longer reaches. Ownership lapses on the same call
    /// ([`Self::reconcile_owner`]) — a patch worked back to nothing is a wild stand again.
    pub fn set_ladder_position(&mut self, position: f32, ladder: &LadderConfig) {
        self.ladder_position = position.max(RUNG_UNSTARTED);
        self.standing = plant_standing(self.ladder_position, ladder);
        self.reconcile_owner();
    }

    /// A fully-cultivated ("tended crop") patch: pays the band that tends it a higher-than-wild yield
    /// each turn (place-local, in `advance_labor_allocation`) and is not gather-drawn. The plant
    /// mirror of `Herd::is_domesticated`.
    ///
    /// **It is the STANDING, and it is still ladder-free** — `standing.held` is the highest rung this
    /// patch's position has fully covered, so *"tended or better"* is one enum comparison and the ~100
    /// call sites that ask it are untouched.
    ///
    /// **A Field is tended too, and that is now true BY CONSTRUCTION** rather than by a rule: the
    /// Field's range begins where the tended rung's ends, so no position exists at which
    /// [`Self::is_field`] holds and this does not.
    pub fn is_cultivated(&self) -> bool {
        self.standing.held.is_at_or_above(RungKey::PlantTended)
    }

    /// A fully-sown **Field** (the plant ladder's rung 3): pays the band that works it a *higher*
    /// managed yield than a tended patch (`field_provisions`) and, like a tended patch, is not
    /// gather-drawn. The plant mirror of `Herd::is_corralled`.
    pub fn is_field(&self) -> bool {
        self.standing.held == RungKey::PlantField
    }

    // **RETIRED: `cultivation_meter_full` / `field_meter_full`.** They asked whether a rung's own
    // meter had reached its own cost, as distinct from whether the rung was *achieved* — two facts
    // that came apart only because a rung was held at a **retention bar** below its cost, so a patch
    // could be tended and still have work left on the tended meter.
    //
    // **The bar is deleted with the cliff it patched** (`docs/plan_standing_upkeep.md` §2.8): a rung
    // is achieved exactly when the position reaches its top, which is exactly when its meter is full.
    // The two questions have one answer again, and it is `is_cultivated()` / `is_field()`.

    /// Is this patch a **completed improvement** — a Field or a tended patch? The single predicate
    /// for "this source is worked, not gathered": its harvest is biomass-based and never overdraws
    /// (`sustainable == actual`, no ⚠) and one worker suffices
    /// ([`crate::fauna::TENDED_SOURCE_WORKERS_NEEDED`]). Both the payout path and the forecast branch
    /// on it, so the two cannot disagree about which patches are managed.
    pub fn is_managed(&self) -> bool {
        self.is_cultivated()
    }

    /// **BANK WORK ON THE RUNG THIS PATCH IS RAISING** — the one accrual, in work units, capped at
    /// the top of that rung so a crew cannot spill past it.
    ///
    /// **THE CAP IS WHAT KEEPS EVERY UNIT OF WORK PRICED AT THE KIT THAT DID IT**
    /// (`docs/plan_standing_upkeep.md` §2.8, rule 1). It bites in both directions: a Cultivate crew's
    /// surplus may not run on into the Field above it — that would be doing Sow's work with
    /// Cultivate's tool — and a Sow crew cannot implicitly finish the tended ground beneath, which is
    /// the case the rule is written about. The position stops at the boundary and the *next* turn's
    /// derived verb names the job the player must then be raising.
    ///
    /// Sets ownership on the first work banked; only the owner makes progress. Returns **every rung
    /// this call completed**, in climb order — `accrue_corral`'s convention widened to a climb, and
    /// load-bearing for the feed line, since the verb commands declare on *every* band working the
    /// patch and a post-hoc `is_cultivated()` would announce once per band.
    pub fn accrue_rung(
        &mut self,
        faction: FactionId,
        amount: f32,
        ladder: &LadderConfig,
    ) -> Vec<RungKey> {
        let Some(raising) = self.standing.raising else {
            // Top of the branch — there is nothing left to raise, so there is nothing to bank.
            return Vec::new();
        };
        self.accrue_toward(faction, amount, raising, ladder)
    }

    /// **BANK WORK TOWARD `target`, CAPPED AT ITS OWN TOP** — the verb-specific arm the two labor
    /// sites call.
    ///
    /// **`target` may sit ABOVE the rung currently in flight**, which is a `sow` on untended ground:
    /// the crew is working toward the Field and the position climbs the tended rung's range on the
    /// way, so a Field's work still cannot exist without that ground beneath it. What the cap
    /// forbids is the other direction — a `Cultivate` whose surplus ran on into the Field above it,
    /// or a verb whose rung the source has already passed banking anything at all.
    /// **BANK WORK TOWARD A DESTINATION, CROSSING EVERY RUNG BETWEEN HERE AND THERE** — the seam a
    /// queue entry accrues through, returning every rung this call completed.
    ///
    /// ⛔ **THE CAP IS THE DESTINATION, NEVER THE LEG.** Capping each leg at its own top would throw
    /// away whatever a turn's work overshot a boundary by — a hidden tax of up to one turn per leg,
    /// paid exactly when a climb crosses a rung — and it would make the published legs a lie: they
    /// sum to what the climb costs, so the climb must cost their sum.
    pub fn accrue_to(
        &mut self,
        faction: FactionId,
        amount: f32,
        destination: RungKey,
        ladder: &LadderConfig,
    ) -> Vec<RungKey> {
        self.accrue_toward(faction, amount, destination, ladder)
    }

    fn accrue_toward(
        &mut self,
        faction: FactionId,
        amount: f32,
        target: RungKey,
        ladder: &LadderConfig,
    ) -> Vec<RungKey> {
        let Some(raising) = self.standing.raising else {
            return Vec::new();
        };
        if !target.is_at_or_above(raising) {
            return Vec::new();
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return Vec::new();
        }
        let was = self.standing.held;
        let (base, width) = plant_rung_span(target, ladder);
        let top = base + width;
        self.set_ladder_position((self.ladder_position + amount).min(top), ladder);
        // **EVERY rung this call crossed, not just the destination.** A queue entry names a
        // destination and lays every leg to it, so a `sow` on untended ground raises the tended rung
        // on the way — and *that* completion is news a player who ordered "take it to Field" wants
        // to see. The queue still retires only at the destination; only the announcement is per rung.
        rungs_between(was, self.standing.held)
    }

    /// **The `Cultivate` arm's accrual** — banks toward `plant:tended` and no further, so a crew that
    /// has already finished the rung (or is standing on a Field) banks nothing. Returns *did this
    /// call finish it*, the feed line's trigger.
    pub(crate) fn accrue_cultivation(
        &mut self,
        faction: FactionId,
        amount: f32,
        ladder: &LadderConfig,
    ) -> bool {
        self.accrue_toward(faction, amount, RungKey::PlantTended, ladder)
            .contains(&RungKey::PlantTended)
    }

    // **RETIRED: `accrue_field`** — the `Sow` arm's own bool wrapper, *"did this call finish the
    // Field"*. A queue entry names a **destination** now (`docs/plan_standing_upkeep.md` §2.8), so
    // the arm banks through [`Self::accrue_to`] and reads the rungs it crossed: on untended ground a
    // `sow` lays the tended rung on the way and that completion is its own feed line, which a bool
    // could not carry. [`Self::accrue_cultivation`] survives because the Cultivate arm's destination
    // *is* one rung, so a bool is still the whole answer there.

    /// **A fixture's already-tended patch** — the honest replacement for writing a meter to its cost.
    /// It moves the position to the top of the tended rung through the one mutator, so ownership, the
    /// standing and every predicate behave exactly as they do in play.
    ///
    /// **It takes the ladder** because the rung boundaries are live config now: there is no
    /// fabricated cost a patch could carry that would put it on a real rung.
    pub fn complete_cultivation(&mut self, faction: FactionId, ladder: &LadderConfig) -> bool {
        self.complete_rung(faction, RungKey::PlantTended, ladder)
    }

    /// **A fixture's already-sown Field** — the rung-3 twin of [`Self::complete_cultivation`].
    pub fn complete_field(&mut self, faction: FactionId, ladder: &LadderConfig) -> bool {
        self.complete_rung(faction, RungKey::PlantField, ladder)
    }

    /// Move the position to the top of `rung`, taking ownership as the real accrual would. Returns
    /// whether the patch now holds that rung.
    fn complete_rung(&mut self, faction: FactionId, rung: RungKey, ladder: &LadderConfig) -> bool {
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        let (base, width) = plant_rung_span(rung, ladder);
        self.set_ladder_position(base + width, ladder);
        self.standing.held.is_at_or_above(rung)
    }

    /// **DECAY EATS FROM THE TOP, FOR FREE** — the position simply falls by `amount`, floored at
    /// [`RUNG_UNSTARTED`]. A Field at 10% decaying is `57.5 → 50`; it reaches the tended rung's range
    /// only once the Field is wholly gone, so *"a lower rung is never below full while a higher one
    /// has progress"* is arithmetic rather than a rule the pass has to honour.
    ///
    /// **Returns EVERY rung this call took the patch out of, newest first** — empty if it still holds
    /// what it held. The mirror of [`Self::accrue_rung`]'s "did this call finish it", and the caller
    /// announces on those edges and nowhere else: a 25-turn investment's payoff has just been
    /// destroyed, and the feed says so once per rung rather than every turn of the long bleed.
    ///
    /// # ⛔ EVERY rung, because ONE BLEED CAN CROSS TWO BOUNDARIES
    ///
    /// It reported only the **top** rung lost, which is right for every bleed the shipped ladder can
    /// produce (a rot of `0.75` against spans of 50 and 75) and silently wrong the moment one is not:
    /// a decay large enough to carry the position from inside the Field's range down past the tended
    /// rung's floor would take **two** rungs and announce **one**, and the loss that went unannounced
    /// is the cheaper-to-notice half. A rung the player paid 25 turns for must never be lost in
    /// silence, so the seam reports the set rather than the maximum.
    ///
    /// Newest first, which is the order the unwind itself runs in: the Field goes before the ground
    /// beneath it, so the feed reads the way the loss happened.
    ///
    /// **There is no retention bar left to cross.** A rung ends where its span ends, which is safe
    /// only because the payout interpolates: at 49.99 of the tended rung's 50 the patch pays 99.98%
    /// of a tended patch, so the predicate flipping there is a rounding rather than a cliff.
    pub fn decay_ladder(&mut self, amount: f32, ladder: &LadderConfig) -> Vec<RungKey> {
        let was = self.standing.held;
        self.set_ladder_position(self.ladder_position - amount, ladder);
        // Newest first, which is the order the unwind runs in: the Field goes before the ground
        // beneath it, so the feed reads the way the loss happened.
        let mut lost = rungs_between(self.standing.held, was);
        lost.reverse();
        lost
    }

    /// **Commit this patch to one named plant** — the first turn a crew works it under
    /// `Cultivate`/`Sow` (`docs/plan_flora_roster.md` §4.3). Idempotent and one-way: a patch already
    /// committed keeps its plant, because *"which crop is this ground"* is exactly the decision the
    /// rung exists to make and re-deciding it for free every turn would erase it. The commitment is
    /// released only by going fully feral ([`Self::reconcile_owner`]).
    pub(crate) fn commit_species(&mut self, species: &str) {
        if self.species.is_none() {
            self.species = Some(species.to_string());
        }
    }

    /// Hold the `owner is Some ⟺ some improvement remains` invariant: ownership lapses only once
    /// **both** meters are spent, so a decaying Field doesn't strand a stale owner (which would block
    /// another faction from ever working the tile) and doesn't drop its owner while its cultivation —
    /// or its own remaining progress — is still standing.
    ///
    /// **The species commitment lapses on exactly the same edge**, and for the same reason: once
    /// nothing is left of either improvement the ground is a wild stand again, and a wild stand is
    /// the tile's whole mixed basket rather than one plant somebody once chose. Re-committing then
    /// costs the full build again, at whatever the tile now favours.
    fn reconcile_owner(&mut self) {
        if self.ladder_position <= RUNG_UNSTARTED {
            self.owner = None;
            self.species = None;
        }
    }
}

/// **THE RUNGS STRICTLY ABOVE `floor`, UP TO AND INCLUDING `ceiling`** — in climb order, and empty
/// when the two are the same.
///
/// The one walk both meter edges read: an accrual crossed *these* rungs upward, a decay crossed
/// *these* rungs downward. **A single call can cross more than one** — a crew that out-produces the
/// remainder of a leg, a bleed larger than a rung's span — and a seam reporting only the outermost
/// would announce one of them and lose the rest in silence, which on a 25-turn investment is the
/// half a player most needs told.
fn rungs_between(floor: RungKey, ceiling: RungKey) -> Vec<RungKey> {
    if floor == ceiling {
        // The overwhelmingly common case, and it allocates nothing: `Vec::new` does not.
        return Vec::new();
    }
    let mut crossed = Vec::new();
    let mut cursor = floor.above();
    while let Some(rung) = cursor {
        crossed.push(rung);
        if rung == ceiling {
            break;
        }
        cursor = rung.above();
    }
    crossed
}

/// **THIS PATCH'S OWN COST FOR A RUNG** — the resolver [`RungStanding::at`] and [`rung_span`] take.
/// Always [`RUNG_COST_UNSCALED`]: the ladder's one per-source cost multiplier is a species'
/// `taming_cost_multiplier`, and a plant has no species. It is a named function rather than a closure
/// at four call sites so *"the plant web is unscaled"* is stated once.
fn plant_rung_cost(rung: RungKey, ladder: &LadderConfig) -> Option<f32> {
    ladder.rung(rung).build_cost(RUNG_COST_UNSCALED)
}

/// **WHERE A PLANT RUNG STARTS AND HOW WIDE IT IS**, in cumulative work units —
/// [`crate::intensification::rung_span`] at this web's own costs. `plant:tended` is `(0, 50)` and
/// `plant:field` `(50, 75)` on the shipped ladder.
pub fn plant_rung_span(rung: RungKey, ladder: &LadderConfig) -> (f32, f32) {
    rung_span(rung, &|key| plant_rung_cost(key, ladder))
}

/// **RESOLVE A PLANT POSITION INTO A STANDING** — the one call
/// [`ForagePatch::set_ladder_position`] makes, so no other seam constructs a plant standing.
fn plant_standing(position: f32, ladder: &LadderConfig) -> RungStanding {
    RungStanding::at(ladder, RungBranch::Plant, position, |key| {
        plant_rung_cost(key, ladder)
    })
}

/// **THE LEGS A QUEUE ENTRY STILL HAS TO LAY ON THIS PATCH** — every rung from where the position
/// stands up to and including `destination`, each carrying what it owes **from here**
/// (`docs/plan_standing_upkeep.md` §2.8).
///
/// A `sow` on untended ground is two legs (`plant:tended` 0→50, then `plant:field` 50→125) and costs
/// the whole branch; the same order on a patch that already holds the tended rung is **one** leg
/// owing 75; and on a patch thirty units into its Cultivate it is two legs owing **20** and 75. That
/// last case is the one the shape exists for: a previous improvement is a **receipt, not a
/// discount**, so the player is never asked to buy work already paid for and never handed work they
/// have not.
///
/// **Empty when the destination is already held** — there is nothing left to climb, which is exactly
/// when the entry retires.
///
/// **A rung the player's order does not reach is not a leg**: `cultivate` on wild ground names
/// `plant:tended` and stops there, so the Field is absent rather than present at zero.
pub fn patch_build_legs(
    patch: &ForagePatch,
    destination: RungKey,
    ladder: &LadderConfig,
) -> Vec<BuildLeg> {
    if destination.branch() != RungBranch::Plant {
        // A rung the animal web owns can never stand on ground.
        return Vec::new();
    }
    let position = patch.ladder_position();
    let mut legs = Vec::new();
    let mut cursor = RungBranch::Plant.root_rung();
    while let Some(rung) = cursor.above() {
        if !destination.is_at_or_above(rung) {
            break;
        }
        let (base, width) = plant_rung_span(rung, ladder);
        // What is left of THIS rung from where the source stands: the whole of it when the position
        // has not reached its base, part of it mid-rung, none of it once the position is past its top.
        let owed = (base + width - position.max(base)).clamp(LEG_ALREADY_PAID, width);
        if owed > LEG_ALREADY_PAID {
            legs.push(BuildLeg {
                rung,
                work_remaining: owed,
            });
        }
        cursor = rung;
    }
    legs
}

/// **THE WORK BANKED ON ONE RUNG**, in work units — this patch's position clamped into that rung's
/// own span. It is what lets the **wire** keep publishing two per-rung meters off one position:
/// `cultivationWorkDone` is this at `plant:tended`, `fieldWorkDone` at `plant:field`.
///
/// **A readout, not a second authority.** Nothing in the sim branches on it; the position and its
/// standing decide everything, and this only restates them in the shape the client already reads.
pub fn patch_rung_work_done(patch: &ForagePatch, rung: RungKey, ladder: &LadderConfig) -> f32 {
    let (base, width) = plant_rung_span(rung, ladder);
    (patch.ladder_position() - base).clamp(RUNG_UNSTARTED, width)
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ForageRegistry {
    /// Live patches keyed by tile coord. Iteration order is non-deterministic; the snapshot capture
    /// sorts by coord for a stable rollback record.
    pub patches: HashMap<UVec2, ForagePatch>,
}

impl ForageRegistry {
    pub fn patch(&self, tile: UVec2) -> Option<&ForagePatch> {
        self.patches.get(&tile)
    }

    pub fn patch_mut(&mut self, tile: UVec2) -> Option<&mut ForagePatch> {
        self.patches.get_mut(&tile)
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    pub fn len(&self) -> usize {
        self.patches.len()
    }

    /// Number of **completed plant improvements** owned by `faction` — tended patches *and* sown
    /// Fields (`ForagePatch::is_managed`). Folded (with domesticated herds) into the sedentarization
    /// "domestication" signal — plant + animal domestication share one driver. The plant mirror of
    /// `HerdRegistry::domesticated_count`.
    ///
    /// It counts Fields deliberately: a Field is rung **3**, so reading it as *less* domesticated
    /// than the rung-2 patch below it would invert the signal (and a bare-ground Field carries no
    /// cultivation meter at all — see `ForagePatch::field_progress`).
    pub fn cultivated_count(&self, faction: FactionId) -> usize {
        self.patches
            .values()
            .filter(|patch| patch.is_managed() && patch.owner == Some(faction))
            .count()
    }
}

/// **Is this tile on or beside FRESH water?** — the water half of a rung's
/// [`RungSiteRequirement`], and the reason rung 3 lands in river valleys.
///
/// Three ways to be watered, all read off **existing** hydrology seams (`hydrology.rs` — this
/// invents no adjacency concept of its own):
/// 1. **The tile is fresh-water ground** (`TerrainTags::FRESHWATER`) — a floodplain, a river delta,
///    an oasis basin, a marsh, a lake, a navigable channel.
/// 2. **A river runs along one of its six sides** (`Tile::has_any_river_edge`) — the riverbank. This
///    is *the* edge-river primitive, and `generate_hydrology` sets it on **both** hexes flanking every
///    traced edge, so "I am on the river" needs no neighbour lookup at all.
/// 3. **A fresh-water hex is next door** — the lake shore, the bank of a navigable trunk. Odd-r hex
///    adjacency (`hex_neighbors_wrapped`, wrap-aware), the same adjacency gameplay and the client use.
///
/// **A salt coast is NOT water for this purpose.** `ContinentalShelf`, `TidalFlat`, `MangroveSwamp`
/// and `CoralShelf` are `COASTAL` without `FRESHWATER`; you cannot farm on sea spray, and admitting
/// them would hand every shoreline the rung-3 gate the rule exists to withhold.
///
/// `neighbor_tags` resolves a coord to that tile's tags (`None` = off-map / no tile). A closure rather
/// than a `&TileRegistry` + query pair because the two callers reach tiles differently — the `sow`
/// command through `&App`, the labor arm through its `Query` — and the *rule* must live in one place
/// even though the lookup cannot.
pub fn tile_is_fresh_watered(
    tile: &Tile,
    grid_width: u32,
    grid_height: u32,
    wrap_horizontal: bool,
    neighbor_tags: impl Fn(UVec2) -> Option<sim_runtime::TerrainTags>,
) -> bool {
    if tile
        .terrain_tags
        .contains(sim_runtime::TerrainTags::FRESHWATER)
        || tile.has_any_river_edge()
    {
        return true;
    }
    crate::grid_utils::hex_neighbors_wrapped(
        tile.position.x,
        tile.position.y,
        grid_width,
        grid_height,
        wrap_horizontal,
    )
    .any(|(x, y)| {
        neighbor_tags(UVec2::new(x, y))
            .is_some_and(|tags| tags.contains(sim_runtime::TerrainTags::FRESHWATER))
    })
}

/// **Does `rung`'s site requirement admit this tile?** — the one place the three readings a
/// [`RungSiteRequirement`] judges (whether the tile is a gathering site, its own forage capacity, and
/// whether it is fresh-watered) are gathered, so every gate on the plant branch — the `assign_labor`
/// Forage arm, `cultivate`, `sow`, and the wire's own refusal — resolves the *same* rule and they
/// cannot drift into disagreeing about which ground may be worked.
///
/// `gathering_site` is the caller's `FoodSiteRegistry::is_site` reading; it is passed IN rather than
/// looked up here so this stays a pure function of the rung and the ground, like the other two.
///
/// `None` = the rung asks nothing of the site, or the land permits it. `Some(refusal)` says **which**
/// way the ground fell short, so the caller can phrase each distinctly — they are different problems
/// with different answers (work a site instead, move, or wait for a rung that relaxes the dial).
pub fn rung_site_refusal(
    rung: &RungDef,
    tile: &Tile,
    forage: &ForageLaborConfig,
    gathering_site: bool,
    fresh_water: bool,
) -> Option<SiteRefusal> {
    rung.site_requirement.as_ref()?.refusal(
        gathering_site,
        tile_forage_capacity(forage, tile),
        fresh_water,
    )
}

/// THE forage-capacity of a tile — the single source the seeding path and the wire path both read,
/// so a navigable hex's seeded patch and its exported `forage_capacity` can never drift.
///
/// A `NavigableRiver` hex reads its **underlying** biome (`resource_terrain()`) plus the river
/// fishing bonus (`navigable_forage_capacity`, always `> 0` — a navigable river is always a fishery,
/// so it always seeds a patch even over a barren biome). Every other tile reads its own biome
/// (`resource_terrain()` == `terrain` there).
pub fn tile_forage_capacity(forage: &ForageLaborConfig, tile: &Tile) -> f32 {
    if tile.terrain == sim_runtime::TerrainType::NavigableRiver {
        forage.navigable_forage_capacity(tile.resource_terrain())
    } else {
        forage.capacity_for(tile.resource_terrain())
    }
}

/// THE named plants a tile's forage capacity is made of — the **flora twin of
/// [`tile_forage_capacity`]**, branching on exactly the same condition so the composition and the
/// capacity it decomposes can never disagree about a tile's shape.
///
/// A `NavigableRiver` hex has a **two-term** capacity (the valley it cut **plus** the fishery the
/// channel is), so it gets the blended basket ([`FloraConfig::realized_navigable_composition`]); every
/// other tile reads its own biome's basket. **The result is `Cow::Owned` on both arms** — since the
/// §10 realization addition each tile's basket is a freshly-built subset, so neither arm borrows.
///
/// Every caller (today: the snapshot capture) must go through this, never
/// [`FloraConfig::composition`] on a raw terrain: reading the underlying biome alone on a navigable
/// hex leaves that hex's fishery bonus **unnamed**, which breaks the decomposition ruling on a whole
/// class of tiles and is invisible to `validate_against_forage`.
/// **Now realizes per tile** (`docs/plan_flora_roster.md` §10): the affinity roster answers *what CAN
/// grow here*, and this seam answers *what IS growing here* — a seeded, deterministic subset keyed on
/// `(map_seed, tile)`, so two tiles of one biome carry different baskets. Every non-Sow-from-nothing
/// caller (display, wild gather, Cultivate, Sow-upgrade, and the wire `ForagePatchState.composition`)
/// reads the realized basket through this one function. Owned on both arms now, because realization
/// always produces a fresh subset.
pub fn tile_flora_composition<'a>(
    flora: &'a FloraConfig,
    forage: &ForageLaborConfig,
    tile: &Tile,
    map_seed: u64,
) -> Cow<'a, [FloraShare]> {
    if tile.terrain == sim_runtime::TerrainType::NavigableRiver {
        Cow::Owned(flora.realized_navigable_composition(
            tile.resource_terrain(),
            forage,
            tile.position,
            map_seed,
        ))
    } else {
        Cow::Owned(flora.realized_composition(tile.resource_terrain(), tile.position, map_seed))
    }
}

/// **The whole of a tile's basket** — `1.0`. The ceiling weeding may push a favored crop to, and the
/// share a planted Field's single crop holds. Named rather than a bare `1.0` because at both sites it
/// states *which* whole the number is one of: **the land owns `K`**, so a rung may only change what
/// the tile's constant production is *made of*, never how much of it there is
/// (`docs/plan_flora_roster.md` §4.3).
pub const WHOLE_BASKET: f32 = 1.0;

/// **The conversion gain a species the patch is NOT committed to converts at** — the identity. The
/// volunteers still standing in a tended field are still wild, so only the favored term is multiplied
/// (see [`basket_rate`]).
const NO_CONVERSION_GAIN: f32 = 1.0;

/// **A remaining share this small is subtraction residue, not a plant.** `weeded` takes the whole of
/// an entry whenever this much or less would be left of it, so a favored crop weeded all the way to
/// the [`WHOLE_BASKET`] leaves a basket of exactly one species rather than one species plus ~1e-8 of
/// a ghost. Orders of magnitude below any realized share (the smallest in the shipped roster is a few
/// percent) and below the wire's own zero-share filter.
const VANISHED_SHARE: f32 = 1e-6;

/// **How far out of balance a weeded basket may land before it is a bug** — pure f32 slack. `weeded`
/// moves `delta` out of the other species and into the favored one, so both the "the others could
/// cover it" balance and the "still sums to 1" invariant are exact in real arithmetic and only ever
/// off by accumulated rounding here.
const WEEDING_BALANCE_EPSILON: f32 = 1e-3;

/// **THE effective-basket seam** — the plants a patch's biomass is ACTUALLY made of right now
/// (`docs/plan_flora_roster.md` §4.3). Every yield rate on the plant web is the share-weighted
/// average of this, at *every* rung including wild, which is what makes "a tile's production is
/// constant across rungs 1–3; a rung changes only which plants it is made of" a property of the code
/// rather than a claim about it.
///
/// - **wild** (uncommitted, or an improvement still building) — the tile's realized basket verbatim.
///   A patch still being cleared reads the tile basket for the same reason it always did: the crew
///   has not displaced anything yet, and both halves of a commitment switch on together at completion.
/// - **tended** (rung 2) — [`weeded`]: the favored crop's share rises to `min(1, share × gain)`, taken
///   from the least abundant remaining species first. That *is* weeding.
/// - **field** (rung 3) — [`planted`]: one entry, the crop, at [`WHOLE_BASKET`]. You sowed it.
///
/// Borrowed on the wild arm (`Cow`), because that arm is >99% of patches and this is resolved inside
/// the forward-projection loops — deep-copying a `String` per named plant per simulated turn is the
/// cost the memo in `snapshot/flora_quotes.rs` exists to avoid paying elsewhere.
pub fn patch_composition<'a>(
    patch: &ForagePatch,
    tile_composition: &'a [FloraShare],
    forage: &ForageLaborConfig,
) -> Cow<'a, [FloraShare]> {
    // **⛔ THE BASKET IS RESOLVED AT `held`, AND IT IS THE ONE THING THAT CANNOT BE INTERPOLATED.**
    // Every *rate* on this patch now lerps across the rung it is raising (see [`patch_interpolate`]),
    // but this returns a **species basket**, not a number, and a half-weeded basket is not a blend of
    // two baskets — mixing them would invent shares of plants that are not growing there, which is
    // the same objection that keeps a material's characteristic vector out of `basket_rate`.
    //
    // So the composition steps at the rung actually **achieved** and the *rates* carry the smoothing:
    // a Field at 40% is priced on a tended patch's weeded basket at a rate 40% of the way to the
    // Field's. This is deliberate, not an oversight.
    composition_for_rung(patch, tile_composition, forage, patch.standing().held)
}

/// **A PER-RUNG QUANTITY AT THIS PATCH'S STANDING** — [`interpolate`] bound to the patch, so no rate
/// seam repeats the delta form or reaches for the standing itself.
///
/// `value_at` states the **absolute** each rung pays; a patch part-way up a rung gets everything
/// below it in full plus its fraction of the step it is on.
fn patch_interpolate(patch: &ForagePatch, value_at: impl Fn(RungKey) -> f32) -> f32 {
    interpolate(&patch.standing(), value_at)
}

/// **The basket this patch's crop would make of the tile STANDING ON `rung`** — the seam every
/// *quote* reads, and the reason a rung's payoff can never be assembled out of another rung's
/// composition.
///
/// - `PlantField` → [`planted`]: one entry, the crop, holding the whole basket.
/// - `PlantTended` → [`weeded`]: the favored share rises to `min(1, share × tended_weeding_gain)`.
/// - anything below → the tile's basket verbatim; there is nothing a rung-1 stand reweights.
///
/// **It answers the rung it is ASKED about, never the rung the patch happens to stand on**, and that
/// is load-bearing. `fieldYield` is published for *every* patch — including a tended one — so a Field
/// quote that read the asking patch's own rung would hand the rung-3 number rung 2's weeded basket
/// *and* its conversion gain, overstating it by roughly `tended_conversion_gain`: a published quote
/// disagreeing with what the sim would pay, which is exactly the class of bug
/// [`commit_yield_ratio`]'s history records. It is the same rule `hypothetical_patch`'s per-rung
/// standing crop and the forecast's separate `ceiling_cultivate`/`ceiling_sow` already encode — **two
/// investment rungs on one branch never share a number.**
///
/// [`patch_composition`] is this seam at the patch's own [`standing_rung`], which is the *live*
/// reading the take path and the wire's published basket want.
pub fn composition_for_rung<'a>(
    patch: &ForagePatch,
    tile_composition: &'a [FloraShare],
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> Cow<'a, [FloraShare]> {
    let Some(favored) = patch.species.as_deref() else {
        return Cow::Borrowed(tile_composition);
    };
    match rung {
        RungKey::PlantField => Cow::Owned(planted(favored)),
        RungKey::PlantTended => Cow::Owned(weeded(
            tile_composition,
            favored,
            forage.cultivation.tended_weeding_gain,
        )),
        _ => Cow::Borrowed(tile_composition),
    }
}

/// **WEEDING, stated once** — the rung-2 reweight: the favored crop's share rises to
/// `min(WHOLE_BASKET, share × gain)` and the increase is taken from the **least abundant remaining
/// species first** (share ASC, ties by species key ASC), each giving up `min(its share, what is left
/// to take)`. Entries emptied to `0` drop out; the result comes back in the wire's total order
/// (share DESC, then species key ASC) and still sums to [`WHOLE_BASKET`].
///
/// **Least abundant first is deliberate, and it is NOT "lowest-yielding".** Ranking by yield would
/// mean comparing a food rate against a trade rate — an exchange rate this codebase does not have and
/// should not invent. Abundance is currency-free, deterministic from the composition alone, and
/// independent of which crop was favored. Do not "improve" this to a yield ranking.
///
/// A `favored` the tile does not actually grow returns the basket verbatim: there is nothing to weed
/// toward.
fn weeded(composition: &[FloraShare], favored: &str, gain: f32) -> Vec<FloraShare> {
    let Some(share) = composition
        .iter()
        .find(|entry| entry.species == favored)
        .map(|entry| entry.share)
        .filter(|share| *share > NO_SHARE)
    else {
        return composition.to_vec();
    };
    let target = (share * gain).min(WHOLE_BASKET);
    let mut owed = target - share;
    // The others, LEAST ABUNDANT FIRST. Sorted before anything is summed — this output goes on the
    // wire, so a differently-ordered f32 addition is a snapshot-hash flake (`flora.md`).
    let mut others: Vec<FloraShare> = composition
        .iter()
        .filter(|entry| entry.species != favored)
        .cloned()
        .collect();
    others.sort_by(|a, b| {
        a.share
            .total_cmp(&b.share)
            .then_with(|| a.species.cmp(&b.species))
    });
    for entry in others.iter_mut() {
        let wanted = owed.max(0.0);
        // Take the whole entry whenever what would be left of it is f32 residue rather than a
        // plant — otherwise a saturating weed leaves ~1e-8 of a species standing, and a basket
        // that is supposed to be one crop publishes two.
        let taken = if entry.share - wanted <= VANISHED_SHARE {
            entry.share
        } else {
            wanted
        };
        entry.share -= taken;
        owed -= taken;
    }
    // `owed <= WHOLE_BASKET - share = Σ others`, so the others can always cover it.
    debug_assert!(
        owed <= WEEDING_BALANCE_EPSILON,
        "weeding {favored} to {target} left {owed} unpaid — the basket did not sum to 1"
    );
    let mut weeded: Vec<FloraShare> = others
        .into_iter()
        .filter(|entry| entry.share > NO_SHARE)
        .collect();
    weeded.push(FloraShare {
        species: favored.to_string(),
        share: target,
    });
    weeded.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then_with(|| a.species.cmp(&b.species))
    });
    debug_assert!(
        (weeded.iter().map(|entry| entry.share).sum::<f32>() - WHOLE_BASKET).abs()
            <= WEEDING_BALANCE_EPSILON,
        "a weeded basket must still be a whole basket"
    );
    weeded
}

/// **PLANTING, stated once** — the rung-3 reweight: one entry, the sown crop, holding the
/// [`WHOLE_BASKET`]. A Field has no volunteers.
fn planted(favored: &str) -> Vec<FloraShare> {
    vec![FloraShare {
        species: favored.to_string(),
        share: WHOLE_BASKET,
    }]
}

/// **The multiplier the FAVORED species' yield vector carries on `rung`** —
/// `cultivation.tended_conversion_gain` at rung 2, the identity at every other rung (a Field converts
/// at its own dial, `field_provisions_per_biomass`; a wild stand at nobody's).
///
/// **It applies to the favored term ONLY, and that is the whole point.** Tending is knowing *your*
/// crop; a blanket multiplier on the entire basket would make every commitment pay ~`gain` whatever
/// you favored, which erases the crop choice. On the favored term it *compounds* with weeding, so
/// favoring a dominant plant pays and favoring a marginal one barely moves. It multiplies the whole
/// vector — food, fodder and trade alike — so this stays commodity-generic with no `role` branch.
///
/// **THE SELECTIVE-GATHER SEAM — the basket a crew that named some plants actually works.**
///
/// Filters a resolved basket down to the crew's [`TakeSelection`] and **renormalizes the survivors**,
/// so a selection's members carry their shares *within the selection*. That is the whole of the
/// narrowing rule: how much is standing is the selected species' summed share of the tile
/// ([`selected_biomass_share`]); what a unit of that take converts to is this basket's own average.
///
/// **Applied AFTER the rung's own reweight, never before it.** Weeding and planting
/// ([`composition_for_rung`]) are properties of the ground and are computed on the whole basket that
/// is standing there; a crew choosing what to carry home cannot change what grew. Narrowing first
/// would hand [`weeded`] a basket that does not sum to the [`WHOLE_BASKET`].
///
/// Borrowed on the whole-basket arm, which is every assignment that names nothing — the default, and
/// the reason the neutrality bar costs nothing to hold.
///
/// An **empty** result is reachable and honest: a crew still asking for cotton on ground that has
/// since been sown to emmer finds none of it standing, takes nothing, and is told so by a `0` take.
fn narrowed<'a>(composition: &'a [FloraShare], take: &TakeSelection) -> Cow<'a, [FloraShare]> {
    if take.is_everything() {
        return Cow::Borrowed(composition);
    }
    let total = selected_biomass_share(composition, take);
    if total <= NO_SHARE {
        return Cow::Owned(Vec::new());
    }
    Cow::Owned(
        composition
            .iter()
            .filter(|entry| take.takes(&entry.species))
            .map(|entry| FloraShare {
                species: entry.species.clone(),
                share: entry.share / total,
            })
            .collect(),
    )
}

/// **HOW MUCH OF THIS STAND THE CREW IS HERE FOR** — the selected species' summed share of the
/// basket, and therefore the fraction of the escapement ceiling and of the standing crop they may
/// take (`max(0, B − floor·K) × this`).
///
/// [`WHOLE_BASKET`] for a crew that named nothing, which is what makes naming nothing byte-identical
/// to the take before selective gathering existed. Clamped to the whole basket because a share table
/// sums to `1` by construction and f32 addition is not exact.
pub fn selected_biomass_share(composition: &[FloraShare], take: &TakeSelection) -> f32 {
    if take.is_everything() {
        return WHOLE_BASKET;
    }
    composition
        .iter()
        .filter(|entry| take.takes(&entry.species))
        .map(|entry| entry.share)
        .sum::<f32>()
        .clamp(NO_SHARE, WHOLE_BASKET)
}

/// Keyed on the **rung being asked about**, exactly as [`composition_for_rung`] is, so the gain and
/// the basket it multiplies can never come from two different rungs.
fn favored_conversion_gain(rung: RungKey, forage: &ForageLaborConfig) -> f32 {
    match rung {
        RungKey::PlantTended => forage.cultivation.tended_conversion_gain,
        _ => NO_CONVERSION_GAIN,
    }
}

/// **What one unit of this patch's biomass would convert at STANDING ON `rung`** — the basket that
/// rung would make of the tile, priced through that rung's own conversion gain. The single seam every
/// per-rung rate below is one line of, so no consumer can pair one rung's basket with another's gain.
///
/// **`take` narrows the basket to what the crew carries home** ([`narrowed`]) — the whole basket for
/// every quote and for every crew that named nothing, which is why the selective gather changes no
/// number it was not asked to.
#[allow(clippy::too_many_arguments)] // the patch, the basket, both configs, the rung, the selection and the accessor
fn rung_rate(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
    take: &TakeSelection,
    rate_of: impl Fn(&crate::flora_config::FloraDef) -> f32,
    fallback: f32,
) -> f32 {
    let standing = composition_for_rung(patch, tile_composition, forage, rung);
    basket_rate(
        &narrowed(&standing, take),
        patch.species.as_deref(),
        favored_conversion_gain(rung, forage),
        flora,
        rate_of,
        fallback,
    )
}

/// **THE basket arithmetic**, stated once: `Σ shareᵢ × rate(speciesᵢ)`, with the **favored** species'
/// term multiplied by `favored_gain`. `fallback` when the basket is empty or names nothing the roster
/// knows — the only two ways a basket cannot be decomposed at all.
///
/// Commodity-generic by construction: `rate_of` picks the component (provisions / fodder / trade) off
/// the species' one yield vector, so all three accounts are priced through the same average and a
/// fourth costs a closure, not a branch.
fn basket_rate(
    composition: &[FloraShare],
    favored: Option<&str>,
    favored_gain: f32,
    flora: &FloraConfig,
    rate_of: impl Fn(&crate::flora_config::FloraDef) -> f32,
    fallback: f32,
) -> f32 {
    let mut named = NO_SHARE;
    let mut rate = 0.0_f32;
    // The composition is already in a total order (share DESC, species key ASC) wherever it is built,
    // so this sum is in a fixed order and nothing here reaches `HashMap` iteration order.
    for entry in composition {
        let Some(def) = flora.species.get(&entry.species) else {
            continue;
        };
        named += entry.share;
        let gain = if favored == Some(entry.species.as_str()) {
            favored_gain
        } else {
            NO_CONVERSION_GAIN
        };
        rate += entry.share * rate_of(def) * gain;
    }
    if named <= NO_SHARE {
        fallback
    } else {
        rate
    }
}

/// **The MATERIAL account of the same basket** — what a harvest of this patch is *made of*, per unit
/// of biomass (`docs/plan_crafting_and_materials.md` §2).
///
/// It cannot ride [`basket_rate`]'s closure the way the other three accounts do, and the reason is
/// the model rather than the plumbing: food, fodder and trade are interchangeable **scalars**, so a
/// basket averages them into one number, while a material carries a **characteristic vector** and
/// averaging two species' would invent a plant that is not growing there. So the basket is
/// *decomposed* instead of summed: one row per species per material, each keeping that species' own
/// exact reading and carrying its share in the rate. Rows that land in the same band merge in the
/// store, which is where merging belongs.
///
/// Reads the patch's **standing** rung and applies the same favored-crop conversion gain the other
/// three accounts get — tending is knowing your crop, whichever account it pays into.
pub fn patch_material_yields(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> Vec<crate::materials_config::MaterialYieldDef> {
    patch_material_yields_taking(
        patch,
        tile_composition,
        flora,
        forage,
        &TakeSelection::EVERYTHING,
    )
}

/// [`patch_material_yields`] for a crew that named **which plants it carries home** — the material
/// account of the selective gather, decomposed over the selected subset exactly as the whole basket
/// is over all of it. Commodity-generic: the same [`narrowed`] basket routes food, fodder and every
/// material row, with no `role` branch anywhere.
pub fn patch_material_yields_taking(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    take: &TakeSelection,
) -> Vec<crate::materials_config::MaterialYieldDef> {
    // **THE BASKET STEPS AT `held`; THE GAIN INTERPOLATES.** A row's `per_biomass` is a rate and
    // lerps like every other rate on this patch, but the *rows themselves* come from a basket, and a
    // half-weeded basket is not a blend of two baskets — see [`patch_composition`].
    rung_material_yields_at(
        patch,
        tile_composition,
        flora,
        forage,
        patch.standing().held,
        patch_interpolate(patch, |rung| favored_conversion_gain(rung, forage)),
        take,
    )
}

/// **WHAT ONE UNIT OF EACH NAMED PLANT'S BIOMASS CONVERTS AT, ON THIS PATCH RIGHT NOW** — the
/// per-species scalar twin of [`patch_material_yields`], and the seam the selective gather's
/// pre-commit sheet is composed from.
///
/// One row per entry of [`patch_composition`], **in that basket's own order**, so a caller can pair
/// them by index with the shares it is already holding. Each row is the species' own
/// `provisions_per_biomass` / `fodder_per_biomass` with the favored crop's conversion gain applied
/// to *its* term only — the identical treatment [`basket_rate`] gives it, at the identical
/// interpolated gain, which is what makes the identity below exact rather than approximate:
///
/// ```text
/// Σ shareᵢ × provisionsᵢ  ==  patch_provisions_per_biomass(patch, …)
/// ```
///
/// **It is NOT scaled by share, and that is what makes it composable.** A sheet narrowing to a
/// subset `S` needs `Σ_S share × rate ÷ Σ_S share` — the rate *within* the selection — so a
/// share-scaled row would have to be un-scaled before it could be used and would silently be wrong
/// wherever it wasn't. The share is already on the wire beside it.
///
/// **A plant the roster no longer knows reads `0` in both accounts**, exactly as [`basket_rate`]
/// skips it: an unnamed plant contributes nothing to either side of the identity. (The one case the
/// identity does not cover is a basket in which *nothing* is named, where the whole-basket seam falls
/// back to `forage.provisions_per_biomass` — there are no rows to compose there either.)
pub fn patch_species_rates(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> Vec<SpeciesRate> {
    // **THE BASKET STEPS AT `held`; THE GAIN INTERPOLATES** — `patch_material_yields`' rule, and for
    // its reason: a rate lerps across the rung being raised, a *basket* cannot be blended.
    let composition = patch_composition(patch, tile_composition, forage);
    let favored_gain = patch_interpolate(patch, |rung| favored_conversion_gain(rung, forage));
    composition
        .iter()
        .map(|entry| {
            let gain = if patch.species.as_deref() == Some(entry.species.as_str()) {
                favored_gain
            } else {
                NO_CONVERSION_GAIN
            };
            let def = flora.species.get(&entry.species);
            SpeciesRate {
                species: entry.species.clone(),
                provisions_per_biomass: def.map_or(NO_UNCOMMITTED_YIELD_RATE, |def| {
                    def.yield_.provisions_per_biomass
                }) * gain,
                fodder_per_biomass: def.map_or(NO_UNCOMMITTED_YIELD_RATE, |def| {
                    def.yield_.fodder_per_biomass
                }) * gain,
                // **The MATERIAL account of the same plant** — its own rows at its own gain, merged
                // by material id **within this species** (a `BTreeMap`, so the order is the id's own
                // and stable). Merging stops at the species boundary on purpose: two plants' rows
                // are never added together here, because a characteristic reading belongs to the
                // batch a take creates and averaging two species' would invent a plant that is not
                // growing there. Empty for a plant that pays no material — *no row*, never a zero.
                materials: def.map(|def| material_rates_of(&def.yield_.materials, gain)),
            }
        })
        .collect()
}

/// One plant's own conversion rates on a patch — see [`patch_species_rates`]. A named record rather
/// than a pair of parallel `Vec`s at the seam, because *which* account a bare `f32` is cannot be read
/// off a call site; the **wire** splits them into index-aligned vectors, which is a different
/// trade (see `ForagePatchState`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpeciesRate {
    /// The `flora_config.json` key — carried so a caller can assert its pairing rather than trust it.
    pub species: String,
    /// Food per unit of this plant's biomass, favored-crop gain included.
    pub provisions_per_biomass: f32,
    /// The fodder twin, `0` for a plant whose vector pays no hay — commodity-generic, no `role`
    /// branch, exactly as the basket-averaged seams are.
    pub fodder_per_biomass: f32,
    /// **What one unit of this plant's biomass is MADE OF** — its own material rows, one per material
    /// id, at the same gain the two scalars above carry and (like them) **not scaled by share**.
    ///
    /// **`None` is not the same as an empty list.** `None` means the roster does not name this plant
    /// at all — it contributes to no account and there is nothing to say; an empty `Some` is a plant
    /// that genuinely pays no material, which is *"no row"* and is how a grain says so. A `0`-valued
    /// row would read as a crop that pays badly.
    pub materials: Option<Vec<MaterialPayoff>>,
}

/// One plant's material rows, merged by id and ordered by it — [`material_yield_totals`]'s merge at
/// the **per-biomass** basis rather than over a take, so the result is a *rate* a sheet can compose
/// against any biomass it likes. Split out so the seam above states one thing per line.
fn material_rates_of(
    rows: &[crate::materials_config::MaterialYieldDef],
    gain: f32,
) -> Vec<MaterialPayoff> {
    crate::materials_config::material_yield_totals(rows, crate::fauna::ONE_UNIT_OF_BIOMASS, gain)
}

/// **THE MATERIAL ROWS A PATCH WOULD PAY STANDING ON `rung`** — [`patch_material_yields`] asked
/// about a rung the patch may not stand on, at that rung's own basket and its own conversion gain.
///
/// It exists for [`composition_for_rung`]'s reason, stated for the fourth account:
/// `commit_material_payoff` quotes a *commitment at a named rung* against a hypothetical patch, and
/// reading the asking patch's own standing there would hand a rung-3 quote rung 2's weeded basket
/// and its gain.
pub fn rung_material_yields(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> Vec<crate::materials_config::MaterialYieldDef> {
    rung_material_yields_at(
        patch,
        tile_composition,
        flora,
        forage,
        rung,
        favored_conversion_gain(rung, forage),
        &TakeSelection::EVERYTHING,
    )
}

/// The decomposition itself: one row per species per material, at `basket_rung`'s shares and the
/// stated `favored_gain`. Split out so the live reading (an interpolated gain) and a per-rung quote
/// (that rung's own gain) cannot come to be two decompositions.
#[allow(clippy::too_many_arguments)] // the patch, the basket, both configs, the rung, the gain and the selection
fn rung_material_yields_at(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    basket_rung: RungKey,
    favored_gain: f32,
    take: &TakeSelection,
) -> Vec<crate::materials_config::MaterialYieldDef> {
    let standing = composition_for_rung(patch, tile_composition, forage, basket_rung);
    let composition = narrowed(&standing, take);
    let mut rows = Vec::new();
    for entry in composition.iter() {
        let Some(def) = flora.species.get(&entry.species) else {
            continue;
        };
        let gain = if patch.species.as_deref() == Some(entry.species.as_str()) {
            favored_gain
        } else {
            NO_CONVERSION_GAIN
        };
        for row in &def.yield_.materials {
            rows.push(crate::materials_config::MaterialYieldDef {
                material: row.material.clone(),
                per_biomass: entry.share * row.per_biomass * gain,
                characteristics: row.characteristics.clone(),
            });
        }
    }
    rows
}

/// **THE conversion seam** — how well one unit of this patch's biomass turns into food
/// (`docs/plan_flora_roster.md` §4.3): the share-weighted average of the patch's **effective** basket
/// ([`patch_composition`]), with the tended rung's conversion gain on the favored crop's term.
///
/// A **wild** patch therefore pays *its own tile's* basket rather than a map-wide constant — two tiles
/// of one biome with different realized baskets pay different rates, which is what makes the §10
/// realization visible in the economy. `forage.provisions_per_biomass` survives only as the
/// **empty-basket fallback** (and as the rung-3 quality normalization baseline).
///
/// Every biomass→provisions conversion on the plant web resolves the rate here; no call site may
/// reach for `forage.provisions_per_biomass` on a patch directly, for the reason `patch_ecology`
/// exists.
pub fn patch_provisions_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    patch_provisions_per_biomass_taking(
        patch,
        tile_composition,
        flora,
        forage,
        &TakeSelection::EVERYTHING,
    )
}

/// [`patch_provisions_per_biomass`] for a crew that named **which plants it carries home** — the
/// share-weighted average over the selected subset alone, each member weighted *within* the
/// selection. Narrowing to the food species of a mixed stand therefore converts at that plant's own
/// rate rather than at the basket's, which is the whole point of choosing.
pub fn patch_provisions_per_biomass_taking(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    take: &TakeSelection,
) -> f32 {
    // **THE RATE INTERPOLATES ON THE PATCH'S POSITION** (`docs/plan_standing_upkeep.md` §2.8): a
    // Field 40% raised converts at a whole tended patch's rate plus 40% of the Field's extra, so the
    // payoff starts on turn one of a build instead of arriving all at once at the end. `rung_rate`
    // still answers each rung's own **absolute**; the delta is `interpolate`'s and nowhere else's.
    patch_interpolate(patch, |rung| {
        rung_rate(
            patch,
            tile_composition,
            flora,
            forage,
            rung,
            take,
            |def| def.yield_.provisions_per_biomass,
            forage.provisions_per_biomass,
        )
    })
}

/// The conversion rate this patch's crop would reach **on `rung`** — [`patch_provisions_per_biomass`]
/// asked about a rung the patch may not stand on yet (or may already have passed). Used by the two
/// managed-rung payoff quotes, each naming *its own* rung: [`tended_provisions`] asks
/// `PlantTended`, [`field_provisions`] (through [`patch_species_quality`]) asks `PlantField`.
fn rung_provisions_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        rung,
        &TakeSelection::EVERYTHING,
        |def| def.yield_.provisions_per_biomass,
        forage.provisions_per_biomass,
    )
}

/// **The FIELD basket's conversion rate RELATIVE to the wild baseline** — dimensionless, `1.0` =
/// exactly baseline. Rung 3's managed rate is a *rate on the standing crop*, so it scales by this
/// rather than by the absolute rate: `field_payoff = biomass × field_provisions_per_biomass ×
/// species_quality`.
///
/// **It reads `PlantField` whatever rung the patch stands on**, which is the whole point: a Field's
/// basket is 100% its crop and takes no rung-2 conversion gain, so this is exactly `crop rate ÷ wild
/// rate` — the number a Field would really pay — even when the patch it is asked about is currently
/// tended. `fieldYield` is published for every patch, so anything else is a quote that disagrees with
/// the payout (see [`composition_for_rung`]).
///
/// **Derived, never a second config field.** A `field_provisions_multiplier` per species would be a
/// redundant lever that could drift from the conversion rate it is supposed to express.
pub fn patch_species_quality(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    if forage.provisions_per_biomass <= 0.0 {
        return WILD_SPECIES_QUALITY; // `validate()` pins the wild rate positive; never divide by 0.
    }
    rung_provisions_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantField)
        / forage.provisions_per_biomass
}

/// **The species-quality of a basket that converts exactly at the wild baseline** — the dimensionless
/// `1.0` [`patch_species_quality`] falls back to. Named because "1.0" at a call site says nothing
/// about which baseline it is one *of*.
const WILD_SPECIES_QUALITY: f32 = 1.0;

/// **The wire quote `0` uses for "this plant cannot climb this rung".** Distinct from a real ratio of
/// `0`, which cannot occur: a species that appears in a tile's basket has `share > 0`, and
/// `FloraConfig::validate` pins every yield vector positive.
pub const CANNOT_CLIMB_RATIO: f32 = 0.0;

/// **Which plant a `Cultivate`/`Sow` on this tile may commit to** — the legality rule, stated once
/// (`docs/plan_flora_roster.md` §4.3) and read by both the `assign_labor` rejection and the labor
/// arm's commit.
///
/// A selection is legal iff the roster knows it, the rung's `cultivation_ceiling` permits it
/// (`allows_cultivate` for the tended rung, `allows_sow` for the Field), **and it is in this tile's
/// basket** — resolved through [`tile_flora_composition`], never `FloraConfig::composition` on a raw
/// terrain, so a navigable hex's two-term basket is judged the way it is actually made.
pub fn species_is_legal_here(
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    rung: RungKey,
) -> bool {
    composition
        .iter()
        .any(|entry| entry.species == species && species_climbs(species, entry.share, flora, rung))
}

/// **The share a species must exceed to count as present in a tile's basket.** A zero-share entry is
/// a plant that is named on the tile and takes none of it — nothing to commit to.
const NO_SHARE: f32 = 0.0;

/// **The plant a commitment falls to when the player named none** — the highest-share species in this
/// tile's basket that the rung permits. The composition is already sorted share-DESC then key-ASC (a
/// *total* order), so this is deterministic without a second sort.
///
/// `None` = **this ground grows nothing that can climb this rung** — an open-water fishery, an alpine
/// peak, a MixedWoodland asked to be sown. That is the `cultivation_ceiling` ruling working ("not
/// every plant climbs"), not a gap, and the caller turns it into a refusal.
pub fn default_species_for_rung(
    composition: &[FloraShare],
    flora: &FloraConfig,
    rung: RungKey,
) -> Option<String> {
    composition
        .iter()
        .find(|entry| species_is_legal_here(&entry.species, composition, flora, rung))
        .map(|entry| entry.species.clone())
}

/// **Why a Cultivate/Sow may not commit this patch to this plant** — the species-side twin of
/// [`SiteRefusal`], in the same style (a small enum with a stable string key, the live value staying
/// serde-free). They are deliberately *separate* enums because they judge different things:
/// `SiteRefusal` judges **the land** (and is therefore a property of the tile the wire can publish
/// per-tile), while this judges **a selection against a rung** and only exists in the context of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesRefusal {
    /// The named key is not in `flora_config.json` at all.
    Unknown,
    /// The species exists but its `cultivation_ceiling` stops below this rung — an oak's mast is a
    /// wild harvest forever.
    CeilingTooLow,
    /// The species exists and climbs, but it does not grow on this tile.
    NotHere,
    /// **Nothing** in this tile's basket can climb this rung, so there is nothing to commit to.
    NothingClimbsHere,
}

impl SpeciesRefusal {
    /// Stable string key — the [`SiteRefusal::as_str`] convention.
    pub fn as_str(self) -> &'static str {
        match self {
            SpeciesRefusal::Unknown => "unknown_species",
            SpeciesRefusal::CeilingTooLow => "species_ceiling_too_low",
            SpeciesRefusal::NotHere => "species_not_here",
            SpeciesRefusal::NothingClimbsHere => "nothing_climbs_here",
        }
    }
}

/// **Resolve what a Cultivate/Sow on this tile would commit to** — the one seam the `assign_labor`
/// rejection and the labor arm's commit both read, so a selection the command accepted can never be
/// one the turn then refuses.
///
/// `selection` is the assignment's species choice (`None` = "pick for me"). `Ok` carries the species
/// key that will be committed; `Err` names why nothing can be.
pub fn resolve_committed_species(
    selection: Option<&str>,
    composition: &[FloraShare],
    flora: &FloraConfig,
    rung: RungKey,
) -> Result<String, SpeciesRefusal> {
    match selection {
        Some(species) => {
            if species_is_legal_here(species, composition, flora, rung) {
                return Ok(species.to_string());
            }
            let Some(def) = flora.species.get(species) else {
                return Err(SpeciesRefusal::Unknown);
            };
            let climbs = match rung {
                RungKey::PlantField => def.cultivation_ceiling.allows_sow(),
                _ => def.cultivation_ceiling.allows_cultivate(),
            };
            Err(if climbs {
                SpeciesRefusal::NotHere
            } else {
                SpeciesRefusal::CeilingTooLow
            })
        }
        None => default_species_for_rung(composition, flora, rung)
            .ok_or(SpeciesRefusal::NothingClimbsHere),
    }
}

/// **May this crew ask for these plants HERE?** — the take selection's legality, the seam the
/// `assign_labor` rejection reads (the selective gather's twin of [`resolve_committed_species`]).
///
/// A named species is legal iff the roster knows it **and it is in this tile's basket** — resolved
/// through [`tile_flora_composition`], never `FloraConfig::composition` on a raw terrain, so a
/// navigable hex is judged on the two-term basket it actually has. **There is no rung gate**: this
/// says what a crew carries home from the stand that is standing, not what the ground may be
/// committed to, so a `wild`-ceiling species (an oak's mast, a fishery) is a perfectly legal choice.
///
/// **It fails closed at the command**, like the floor does. A silently-dropped selection is
/// indistinguishable from *"take everything"* on every readout the player has, so it would be
/// undiagnosable; the refusal names the first offending key in the selection's own order.
///
/// The whole basket ([`TakeSelection::is_everything`]) is always legal — it names no plant to be
/// wrong about.
pub fn resolve_take_selection<'a>(
    take: &'a TakeSelection,
    composition: &[FloraShare],
    flora: &FloraConfig,
) -> Result<(), (&'a str, SpeciesRefusal)> {
    for species in take.keys() {
        if composition
            .iter()
            .any(|entry| entry.species == species && entry.share > NO_SHARE)
        {
            continue;
        }
        return Err((
            species,
            if flora.species.contains_key(species) {
                SpeciesRefusal::NotHere
            } else {
                SpeciesRefusal::Unknown
            },
        ));
    }
    Ok(())
}

/// **What a patch pays, standing on `rung`** — in provisions/turn, through the *same* helpers the sim
/// itself quotes and pays each rung with, never a re-derivation of their arithmetic:
///
/// - **wild / anything below rung 2** — its long-run sustainable yield (MSY) on the patch's own wild
///   ecology, converted at the patch's **basket** rate. **A rung PAYOFF, not a take ceiling**: since
///   the harvest floor the take is constant escapement (`forage_escapement_ceiling`), which is
///   `r`-independent and so cannot compare two rungs at all — `r` is exactly what a rung buys;
/// - **tended** — [`tended_provisions`], the rung-2 payoff quote (the wire's `tendedYield`), which
///   rides `tended_ecology` and therefore **carries `cultivation.tended_regrowth_gain`**;
/// - **field** — [`field_provisions`], the rung-3 managed rate the labor arm actually pays.
///
/// That third bullet is the whole reason this exists. The two drawn-down rungs are compared as MSY
/// (`r · K / 4`), where `r` **does not cancel** between wild and tended — tending changes `r`, that is
/// its payoff — so any comparison built on capacity alone silently drops the regrowth gain and
/// understates rung 2 by exactly it. Rung 3 is not an MSY at all but a flat rate on the standing
/// crop, so it is not even the same *shape* of number. One function, three arms, so a quote can never
/// be assembled out of the wrong shape again.
///
/// `tile_composition` is the **tile's** realized basket — the rung derives the patch's effective one
/// from it ([`patch_composition`]), so a quote and the payout it quotes read the same reweight.
pub fn rung_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    // **ONE SHAPE AT EVERY RUNG** — the MSY skim on that rung's own curve, priced through that
    // rung's own basket and conversion. **No rung changes the DRAW**: rung 3 used to branch here into
    // a flat managed rate on a crop that was never drawn down, which is the model this arc retired
    // (see the gravestone on the managed-harvest family). What a rung buys is the *curve and the
    // capacity* the skim is taken from.
    forage_provisions(
        rung_msy_take(patch, forage, rung),
        rung_provisions_per_biomass(patch, tile_composition, flora, forage, rung),
        output_multiplier,
    )
}

/// **The patch a tile WOULD carry, had this crop been committed and the rung finished** — the
/// hypothetical every per-species quote is taken against: the rung's meter complete, **the tile's own
/// `K`**, and the standing crop that rung settles at.
///
/// **Its capacity is the tile's at every rung** (#433): no rung below 4 raises `K` and none lowers
/// it, so the hypothetical differs from the wild counterfactual only in what the patch's biomass is
/// *made of* — which the rate seams derive from the commitment through [`patch_composition`]. The
/// retired concentration term used to shrink this capacity to `share × gain`, which is exactly the
/// bug that made a commitment cost production.
///
/// `species = None` builds the **wild** counterfactual, which is the denominator of
/// [`commit_yield_ratio`] and the reason both sides of that ratio come out of one construction rather
/// than two.
///
/// **The standing crop is per-rung, and that is load-bearing.** Each rung is quoted where a *running*
/// patch on it actually stands: the drawn-down rungs at their MSY operating point (Sustain settles a
/// patch at `K/2`), and a Field at its capacity, because a Field is never drawn down and regrows to
/// it. For a rung already built, that is the number the shipped `tendedYield`/`fieldYield` read too.
/// **Whose ground the per-species quote imagines** — nobody's in particular. A committed meter needs
/// # ⛔ IT CARRIES NO METER AT ALL, AND THAT IS THE ONE-POSITION LADDER PAYING OUT
///
/// It used to fabricate the completed rung (`complete_field` / `complete_cultivation`) so that the
/// rate seams' `is_field()` / `is_cultivated()` reads would land on the right one. **Every seam this
/// construction reaches is rung-**parameterised** now** — `rung_payoff` dispatches on the `rung`
/// argument, `tended_provisions` / `field_provisions` name their own rung, and
/// `rung_material_payoff` asks `rung_material_yields` — so the hypothetical's own standing is read
/// by nothing, and stamping a position on a quote nobody paid for would be inventing one.
///
/// **This is also what keeps the ladder out of the quote path.** The position→standing pair may only
/// be written together with a `LadderConfig` in hand, and these quotes are memoized per
/// `(terrain, resource_terrain)` in `snapshot/flora_quotes.rs` with no config in scope. A fabricated
/// standing beside a zero position would be exactly the drift the pair exists to forbid.
fn hypothetical_patch(tile: UVec2, tile_capacity: f32, species: Option<&str>) -> ForagePatch {
    // **The TILE's own capacity, and the operating point every rung now settles at.** A Field used to
    // be quoted at its FULL standing crop because it was never drawn down; it is drawn down like
    // everything else now, so it settles where a Sustain skim leaves it.
    //
    // **The rung's capacity gain is NOT applied here** — [`rung_msy_take`] re-bases onto the
    // asked-about rung, and it must be the only place that does, or a quote taken through this
    // construction would carry the gain twice.
    let mut patch = ForagePatch::new(tile, tile_capacity);
    patch.biomass = tile_capacity * crate::fauna::MSY_BIOMASS_FRACTION;
    if let Some(key) = species {
        patch.species = Some(key.to_string());
    }
    patch
}

/// **This species' share of a tile's basket** — [`NO_SHARE`] when the tile does not grow it. The one
/// lookup the per-species quotes take against the composition they are handed, so a quote's legality
/// check and its payoff read the same number.
fn share_of(composition: &[FloraShare], species: &str) -> f32 {
    composition
        .iter()
        .find(|entry| entry.species == species)
        .map_or(NO_SHARE, |entry| entry.share)
}

// **RETIRED: `settled_biomass_fraction` / `FULL_STANDING_CROP`** — *"a Field is quoted at its whole
// capacity because it is never drawn down"*. Every plant rung is drawn down now, so every rung
// settles at the same operating point a Sustain skim leaves it at (`fauna::MSY_BIOMASS_FRACTION`),
// and what rung 3 buys is the capacity that point is a fraction *of*.

/// **What this tile would pay per turn once committed to THIS plant and worked up to `rung`** —
/// provisions/turn, in the same units and at the same `output_multiplier` convention as the shipped
/// per-patch forecast quotes (`tendedYield`/`fieldYield`), so the client can substitute one for the
/// other with no arithmetic of its own.
///
/// The point of it being *per species* is that the shipped quotes are species-**blind**: they read
/// whatever the patch is already committed to (usually nothing), so a player choosing between crops
/// in the compose sheet is shown one number for every option. [`CANNOT_CLIMB_RATIO`] when the plant
/// cannot climb `rung` here.
// A per-species quote needs the whole tile context (where, how much land, how much standing crop,
// which plant, how much of the basket it is) plus both config tables and the rung — the same shape
// `forage_source_yield_preview` already carries, and none of it is derivable from the rest.
#[allow(clippy::too_many_arguments)]
pub fn commit_payoff(
    tile: UVec2,
    tile_capacity: f32,
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    if !species_climbs(species, share_of(composition, species), flora, rung) {
        return CANNOT_CLIMB_RATIO;
    }
    let patch = hypothetical_patch(tile, tile_capacity, Some(species));
    rung_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **The FODDER (hay) committing this tile to THIS plant would pay per turn, on `rung`** (Flora Roster
/// F3, §5, Part D) — the fodder twin of [`commit_payoff`], so the crop picker can show a hay crop's
/// real value instead of the bare `0×` its provisions ratio reads. Built through the *same*
/// `hypothetical_patch` construction and the *same* payoff functions the sim pays with (the §4.3
/// "assert the quote against the payoff function" rule), so the published number and the payout cannot
/// drift. `0.0` for a plant that pays no fodder or cannot climb `rung` here.
///
/// **It takes a `rung` for the reason [`commit_payoff`] does**, and it did not always: F3 quoted the
/// Field arm alone, so the Cultivate row of the picker had nothing to state but a *sown Field's* hay.
/// The two rungs pay different amounts off different baskets, so one number cannot answer both.
#[allow(clippy::too_many_arguments)]
pub fn commit_fodder_payoff(
    tile: UVec2,
    tile_capacity: f32,
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    if !species_climbs(species, share_of(composition, species), flora, rung) {
        return 0.0;
    }
    let patch = hypothetical_patch(tile, tile_capacity, Some(species));
    rung_fodder_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **What a patch pays in FODDER, standing on `rung`** — the fodder arm of [`rung_payoff`], dispatching
/// to the *same* helpers the sim pays each rung with: [`field_fodder`] at rung 3 (a managed rate on the
/// standing crop) and [`tended_fodder`] at rung 2 (the MSY skim, because rung 2 is drawn down). Rung 1
/// pays no *committed* fodder quote — a wild gather's hay is not a commitment's payoff — so it is `0`,
/// the same "cannot climb this rung" sentinel the ratios use.
pub fn rung_fodder_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    // The fodder twin of [`rung_payoff`], on the identical shape — one harvest, split by account.
    forage_provisions(
        rung_msy_take(patch, forage, rung),
        rung_fodder_per_biomass(patch, tile_composition, flora, forage, rung),
        output_multiplier,
    )
}

// **RETIRED: `commit_trade_payoff` / `rung_trade_payoff`** (arc #527), with the trade-goods axis
// they quoted. What a cash crop actually pays is **materials** — see `commit_material_payoff`
// below, which is the replacement rather than a restoration: it answers per material instead of
// flattening every one of them into a single number, which is the whole reason the scalar went.

/// **The MATERIALS committing this tile to THIS plant would pay per turn, on `rung`** — the
/// replacement for the retired `commit_trade_payoff` (arc #527), and the number the crop picker's
/// cash-crop row states.
///
/// **A VECTOR, not a scalar, and that is the whole difference.** The retired quote answered "how
/// much trade", which a market could total but a player could not act on; this answers "0.29 fibre"
/// or "0.21 tobacco", which is what a cash crop *is*. Totalling it back into one number would be the
/// retired axis under a new name.
///
/// Built through the *same* `hypothetical_patch` construction and the *same* per-rung harvest
/// expressions the sim pays with — [`field_harvest_production`] at rung 3, [`rung_msy_take`] at
/// rung 2 — so the published number and the payout cannot drift (the §4.3 "assert the quote against
/// the payoff function" rule). **Empty** for a plant that pays no material or cannot climb `rung`
/// here, which a client must render as *no row*, never as a zero.
///
/// **Rows are merged per material id, in id order.** A mixed rung-2 basket can name one material
/// twice (cotton fibre beside hay straw), and those land in *different* batches in the store because
/// their readings differ — but `LocalStore::material_total` sums exactly this way, which is what
/// makes the quote checkable against what the band ends up holding.
#[allow(clippy::too_many_arguments)]
pub fn commit_material_payoff(
    tile: UVec2,
    tile_capacity: f32,
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> Vec<MaterialPayoff> {
    if !species_climbs(species, share_of(composition, species), flora, rung) {
        return Vec::new();
    }
    let patch = hypothetical_patch(tile, tile_capacity, Some(species));
    rung_material_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **What a patch pays in MATERIALS, standing on `rung`** — the material arm of [`rung_payoff`],
/// dispatching to the *same* harvest each rung is paid on: [`field_harvest_production`] at rung 3 (a
/// managed rate on the standing crop) and [`rung_msy_take`] at rung 2 (the MSY skim, because rung
/// 2 is drawn down). Rung 1 pays no *committed* quote — a wild gather's fibre is not a commitment's
/// payoff — so it is empty, the same "cannot climb this rung" sentinel the ratios use.
fn rung_material_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> Vec<MaterialPayoff> {
    // Rung 1 pays no *committed* quote — a wild gather's fibre is not a commitment's payoff.
    if !rung.is_at_or_above(RungKey::PlantTended) {
        return Vec::new();
    }
    // **The same take the other three accounts are priced on**, at this rung's own curve.
    let harvest_biomass = rung_msy_take(patch, forage, rung);
    // **The same rows `credit_material_yield` is handed, through the same expression** — the quote is
    // the payout's own arithmetic rather than a re-derivation of it.
    //
    // **ASKED AT `rung`, NEVER AT THE PATCH'S OWN STANDING**, which is `composition_for_rung`'s rule
    // stated for the material account: this function already knows which rung it is quoting, and
    // reading the hypothetical's standing here would hand a rung-3 quote rung 2's weeded basket and
    // its conversion gain. It is also what lets `hypothetical_patch` stop fabricating a meter.
    crate::materials_config::material_yield_totals(
        &rung_material_yields(patch, tile_composition, flora, forage, rung),
        harvest_biomass,
        output_multiplier,
    )
}

/// **What this tile pays per turn left WILD** — the denominator of [`commit_yield_ratio`], and the
/// same Sustain skim `rung_payoff` gives any uncommitted patch.
///
/// It takes the **composition** rather than nothing because a wild gather is no longer priced at a
/// map-wide constant (#433): you gather the whole basket, so you get *this tile's* basket average.
pub fn wild_payoff(
    tile: UVec2,
    tile_capacity: f32,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    let patch = hypothetical_patch(tile, tile_capacity, None);
    rung_payoff(
        &patch,
        composition,
        forage,
        flora,
        output_multiplier,
        RungKey::PlantWild,
    )
}

/// Can this plant climb `rung` on a tile where it holds `share` of the basket? The ceiling half of
/// [`species_is_legal_here`], split out for the quote path, which already has the share in hand.
fn species_climbs(species: &str, share: f32, flora: &FloraConfig, rung: RungKey) -> bool {
    let Some(def) = flora.species.get(species) else {
        return false;
    };
    let climbs = match rung {
        RungKey::PlantField => def.cultivation_ceiling.allows_sow(),
        _ => def.cultivation_ceiling.allows_cultivate(),
    };
    climbs && share > NO_SHARE
}

/// **What committing THIS tile to THIS plant is worth, against just gathering it wild** — the single
/// number the crop-picker decision turns on (`docs/plan_flora_roster.md` §4.3).
///
/// **It is the two published payoffs, divided — not a formula that reproduces them.** Both arguments
/// come from [`commit_payoff`] / [`wild_payoff`], i.e. from the functions the sim itself quotes and
/// pays each rung with, so the ratio and the payoffs it relates are one computation and cannot
/// disagree. Taking a ratio of *arithmetic* instead of a ratio of *payoffs* is exactly the bug this
/// signature exists to make unrepresentable: the previous version divided
/// `concentration × rate ÷ base_rate`, a **capacity**-based basis in which the ecology's `r` cancels
/// — but rungs 1–2 pay **MSY** (`r · K / 4`) and tending's payoff *is* that it scales `r` by
/// `cultivation.tended_regrowth_gain`, so every Cultivate ratio shipped at exactly half its true
/// value and told the player that tending a good delta crop *lost*.
///
/// `> 1.0` committing beats gathering the whole basket; `< 1.0` it is a loss — **a legal one the
/// player stays free to choose**, which is the whole decision, so this is never clamped and a sub-1
/// crop is never refused. [`CANNOT_CLIMB_RATIO`] when the plant cannot climb the rung (`payoff` is
/// then the same sentinel) or when the tile pays nothing wild.
pub fn commit_yield_ratio(payoff: f32, wild: f32) -> f32 {
    if wild <= 0.0 {
        return CANNOT_CLIMB_RATIO;
    }
    payoff / wild
}

/// Seed a full patch on every `FoodModuleTag` tile at Startup (idempotent — a world that already
/// carries patches, e.g. after a rollback restore, is skipped). Runs in the Startup chain after
/// `spawn_initial_world` has stamped the food-module tags. Mirrors `spawn_initial_herds`.
///
/// **The patch's cap is the TILE's, not a constant** — `forage.capacity_by_biome[tile.terrain]`, the
/// human food web's per-biome table (the mirror of `graze.capacity_by_biome`). A food-module tile
/// whose biome carries nothing human-edible (`NO_FORAGE_CAPACITY` — a glacier, a salt pan, a
/// deep-sea vent field: the module classifier tags these off their *tags*, not off anything growing
/// there) is seeded **no patch at all**, exactly as a zero-graze tile holds no `GrazePatch`: "no food
/// here" is an *absent* reading, never a zero one, and a zero-cap patch would be a permanently
/// Collapsing source with a zero reseed floor.
pub fn spawn_initial_forage(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
    tiles: Query<(&Tile, &FoodModuleTag)>,
) {
    if !registry.patches.is_empty() {
        return;
    }
    let labor = labor_config.get();
    let forage = &labor.forage;
    for (tile, _module) in tiles.iter() {
        let capacity = tile_forage_capacity(forage, tile);
        if capacity <= NO_FORAGE_CAPACITY {
            continue;
        }
        let mut patch = ForagePatch::new(tile.position, capacity);
        patch.refresh_ecology_phase(&forage.ecology);
        registry.patches.insert(tile.position, patch);
    }
}

/// Per-turn forage regrowth (`TurnStage::Logistics`, alongside `advance_herds`): regrow every patch
/// toward its carrying capacity and refresh its ecology phase. Patches never despawn.
pub fn advance_forage_regrowth(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
    tile_registry: Res<crate::resources::TileRegistry>,
    tiles: Query<&Tile>,
) {
    let labor = labor_config.get();
    let forage = &labor.forage;
    for patch in registry.patches.values_mut() {
        // **THE LAND OWNS `K`, recomputed fresh from the tile every turn** — the plant twin of
        // `fauna::ecological_carrying_capacity`'s one write, and the **only** place a patch's
        // capacity is set.
        //
        // **A RUNG MAY RAISE IT AND MAY NEVER LOWER IT** (#433). Rung 3 raises it: a sown field is
        // planted densely with the competitors pulled out, so it holds more standing crop than the
        // same ground wild ([`patch_carrying_capacity`], interpolating on the position like every
        // other rung quantity). The retired concentration term *shrank* capacity to `share × gain`
        // and threw the remainder away, which made a commitment cost production — that direction is
        // now a config rejection, not a convention.
        //
        // **Idempotent**, because the gain multiplies the TILE's capacity rather than the patch's
        // own: nothing is read back into itself, a retuned `capacity_by_biome` still reaches patches
        // already on the map, and a lapsed Field hands the capacity straight back on the next pass. A
        // patch whose tile is absent from the map keeps whatever capacity it was seeded with — which
        // is what lets test harnesses build synthetic patches on tiles that do not exist.
        if let Some(tile) = tile_registry
            .index(patch.tile.x, patch.tile.y)
            .and_then(|entity| tiles.get(entity).ok())
        {
            patch.carrying_capacity =
                patch_carrying_capacity(tile_forage_capacity(forage, tile), patch, forage);
        }
        regrow_patch(patch, forage);
    }
}

/// Per-turn cultivation feral/decay pass (`TurnStage::Logistics`, alongside `advance_forage_regrowth`).
///
/// **SHORTFALL IS THE DECAY** (`docs/plan_standing_upkeep.md` §2.4). The tended-crop *food* is not
/// paid here (the old even-split across all the owner's bands is retired): it is paid **place-local**
/// in the labor arm (`advance_labor_allocation`, Population) to the band whose Forage assignment
/// actually gathers the patch. This pass handles **decay/feral**, and it asks exactly one question —
/// *did anybody hold this rung this turn?*:
/// - The at-risk rung's [`RungDef::upkeep_demand`] is what holding it costs, per turn, forever. The
///   keepers on the `maintain` allocation supply [`ForagePatch::upkeep_supplied`] against it, and
///   whatever is left over is the shortfall — which **is** the amount the meter loses
///   ([`RungDef::upkeep_decay`]).
/// - A **fully unmaintained** cultivated patch bleeds the rung's whole demand: it drops below its own
///   cost so the patch reverts to a wild depletable gather patch, and keeps decaying toward 0 (owner
///   clears at 0 — the investment is fully lost, and re-preparing must re-accrue from wherever
///   progress landed). At the shipped rates that is `0.5`/turn on `plant:tended` and `0.75` on
///   `plant:field`, which is **exactly** what the retired `decay_fraction_per_turn` bled.
/// - A **partly** maintained one bleeds the difference, and that is the whole reason the term is a
///   rate rather than a flag: half the hands means it slides at half rate, where the retired
///   `tended_this_turn` boolean made a crew of one and a crew of ten equally sufficient.
/// - An **abandoned** part-prepared patch's partial accrual decays the same way (walk away
///   mid-investment and the cleared ground grows back over) — a meter with progress on it is a meter
///   at risk, whether or not it ever completed.
///
/// **WORKING A PATCH NO LONGER SPARES IT, and that is the behavioural headline of this pass.** The
/// retired flag was set by *any* crew on the tile, so a patch somebody was **gathering** never
/// decayed and holding an improvement was free as long as you were taking from it. Holding and taking
/// are separate allocations now (§2.2), so a gathered-but-unmaintained patch does revert — at the
/// shipped demands, one keeper covers either plant rung.
///
/// **A GRACE first.** Nothing decays on the first turn of shortfall. Each patch carries a
/// [`ForagePatch::neglect_turns`] counter — reset whenever the demand was met, incremented whenever
/// it was not — and the bleed applies only while it *exceeds* the at-risk rung's own
/// [`RungDef::upkeep_grace_turns`]. A crew re-tasked for a couple of turns, a band that walked to
/// answer a raid: none of those cost the investment. The animal twin is the same counter gating the
/// shed in [`crate::fauna::advance_husbandry`] — one trigger, two penalties.
///
/// **The unwind is NEWEST-FIRST: one meter at a time, the highest rung with progress on it.** The
/// least-established improvement is the most fragile, so a Field bleeds to nothing *before* the tended
/// ground beneath it loses anything, and `cultivation_progress` **cannot move while `field_progress >
/// RUNG_UNSTARTED`**. Bleeding both at once produced an unrecoverable state: a gap in a `Sow` knocked
/// cultivation to `0.99`, and once the crew came back the running `Sow` marked the patch worked every
/// turn, so the tended rung could neither decay further nor re-accrue (only `Cultivate` accrues it, and
/// at most one improvement is ever in flight). The patch was stranded one hundredth below a rung it had
/// already paid for, permanently. Ordering the unwind makes that state unreachable by construction.
///
/// It still does *not* step a lapsing Field down to a tended patch: rung 3 unwinding to zero reveals
/// whatever rung 2 the ground already had — which may be nothing — and never pays the deserter a rung
/// they did not build.
///
/// **A lost rung is ANNOUNCED.** Crossing back below its own cost destroys a 25-turn investment's
/// payoff, so each decay call reports that edge and this pass pushes the rung's own feed line
/// (`CommandEventKind::Cultivate` / `Sow`) — once, on the transition, the way the animal web has always
/// announced a lost pen (`fauna::announce_pen_lost`). The long bleed to zero that follows says nothing
/// further: the loss already happened.
///
/// **Stage ordering.** Logistics runs *before* Population, so the [`ForagePatch::upkeep_supplied`]
/// this pass reads was written by the labor arm **last** turn (a one-turn lag) — a deliberate
/// carry-across-turns signal, exactly as the flag it replaced was. Each patch's supply is cleared here
/// after it is read, so the labor arm re-stamps it next Population stage. Net effect: a patch whose
/// keepers meet the demand every turn never decays; a patch whose keepers leave starts counting toward
/// its rung's grace one turn later. The plant counterpart of `fauna::advance_husbandry`'s decay side.
/// **Which rung's meter is unwinding on this patch right now** — the *newest* improvement that still
/// has progress banked on it, because the plant web unwinds newest-first (see
/// [`advance_cultivation`]). `None` for a wild patch: nothing has been built here, so there is nothing
/// to lose and no grace to spend.
///
/// **It is also the rung whose UPKEEP is due**, and that is one fact rather than two: what a patch
/// costs to hold is what it costs to hold *the thing it would otherwise lose*
/// (`docs/plan_standing_upkeep.md` §2.4). So a patch standing on `plant:tended` with a half-built Sow
/// on it owes the **field** rung's demand and bleeds the **field** meter — the same rung on both
/// sides, which is what makes the shortfall the sim bleeds and the shortfall the wire shows the same
/// number.
///
/// **One seam, four readers**, and that is the point: `advance_cultivation` bleeds the rung this
/// returns, the labor arm stamps its shortfall, and `snapshot_forage_patches` publishes *that* rung's
/// demand and remaining grace. Deriving the at-risk rung twice is how the wire comes to count down a
/// grace on a rung the sim is not touching.
///
/// **It is [`patch_keeping_meter`] asked with no verb in flight** ([`NOTHING_IN_FLIGHT`]), rather
/// than a second copy of the same two comparisons. Every caller here is outside the labor arm and
/// genuinely cannot see the band's queue, so the progress-only reading is the honest one — and
/// stating it as *"the keeping meter, absent a verb"* is what stops the two spellings drifting the
/// way they did while the eligibility gate carried one of them by hand.
pub fn patch_unwinding_rung<'a>(
    patch: &ForagePatch,
    ladder: &'a LadderConfig,
) -> Option<&'a RungDef> {
    patch_unwinding_key(patch).map(|key| ladder.rung(key))
}

/// **THE RUNG THIS PATCH'S POSITION IS SITTING IN** — the topmost rung carrying any work, which is
/// the rung a decay eats and the rung whose grace and rot rate govern. `None` on a wild patch, which
/// has nothing at risk.
///
/// It is **one read of the standing**, and the newest-first unwind falls out of it rather than being
/// a rule: the position eats from the top, so the rung being *raised* is at risk while it has any
/// credit, and the rung *held* is at risk once the one above it is empty.
pub fn patch_unwinding_key(patch: &ForagePatch) -> Option<RungKey> {
    let standing = patch.standing();
    if standing.credit > NO_RUNG_CREDIT {
        return standing.raising;
    }
    (standing.held != RungKey::PlantWild).then_some(standing.held)
}

/// **Turns of SHORTFALL this patch can still absorb before its feral bleed starts** — the wire's
/// countdown, resolved through [`patch_unwinding_rung`] so it always describes the rung
/// [`advance_cultivation`] would actually bleed. `None` = a wild patch, with nothing at risk.
///
/// It reads the **upkeep's** grace, not the build's: on the plant branch the neglect trigger is an
/// unmet standing demand rather than an un-worked build, so both plant rungs declare
/// `build.grace_turns: null` and the live number lives in their `upkeep` block.
pub fn patch_neglect_grace_remaining(patch: &ForagePatch, ladder: &LadderConfig) -> Option<u32> {
    patch_unwinding_rung(patch, ladder).map(|rung| {
        crate::intensification::neglect_grace_remaining(
            patch.neglect_turns,
            rung.upkeep_grace_turns(),
        )
    })
}

/// **WHAT IT COSTS TO HOLD THIS PATCH THIS TURN**, in work units — **interpolated on the patch's own
/// standing** (`docs/plan_standing_upkeep.md` §2.8): a whole tended patch plus this Field's share of
/// the extra a Field demands. [`NO_UPKEEP_DEMAND`] on wild ground, which has nothing built on it to
/// hold, and on a patch one work unit into a Cultivate it is one fiftieth of the tended rung's rate.
///
/// # THE COST MOVES WITH THE BENEFIT, OR NOT AT ALL
///
/// A patch 1% into a Cultivate used to owe the **whole** rung's rate to hold a hundredth of a thing,
/// which was reported from play three times in one session. The payout interpolates now, so the
/// demand does too — an interim with one scaled and the other flat is a worse asymmetry than the
/// flat rate was. A queued upgrade therefore raises the keeping bill before it pays anything back,
/// and goes on raising it as the meter climbs: that is what makes an upgrade a decision rather than
/// a free ratchet.
///
/// **THE one definition**, reached by the decay pass, the labor arm's stamp and the snapshot alike —
/// so the demand the sim bleeds against, the demand the player is billed for and the demand the wire
/// shows can never be three different answers.
///
/// # IT SCALES WITH THE SIZE OF THE LAND, and the measure is [`patch_tender_loads`]
///
/// Both plant rungs declare `scaled_by: source_load` and quote their rate **per tender-load**, so a
/// rich alluvial patch costs more to hold than a thin steppe one. One tile is exactly what the load
/// measures — the tile's own `K` over `forage.cultivation.capacity_per_tender` — which is why the
/// tile's capacity is a **parameter here** rather than something the caller pre-scales: no caller can
/// pass a bare `1.0` by habit and quietly re-flatten the bill.
///
/// **The measure applies ONCE, across the interpolation**, because it rides inside each endpoint's
/// own `upkeep_demand` and both endpoints carry the same load. A Field half-raised on a tile of
/// `loads` therefore owes `loads × (tended + 0.5 × (field − tended))`, never the factor twice.
///
/// **It takes no verb any more.** The verb term existed because the demand *stepped* when a `Sow`
/// started on finished tended ground — the claim side and the payment side had to agree which meter
/// they meant across the Population→Logistics carry. There is no step left to carry: at the boundary
/// the interpolated demand is exactly the tended rung's, and it rises continuously from there. What
/// the verb still decides — *does this source claim a share at all before it has banked anything* —
/// is [`patch_claims_keeping`].
pub fn patch_upkeep_demand(
    patch: &ForagePatch,
    ladder: &LadderConfig,
    tile_capacity: f32,
    forage: &ForageLaborConfig,
) -> f32 {
    let loads = patch_tender_loads(tile_capacity, forage);
    interpolate(&patch.standing(), |rung| {
        ladder.rung(rung).upkeep_demand(loads)
    })
}

/// **THE PLANT WEB'S SCALE MEASURE** — how many *tender-loads* a tile presents, `tile_capacity /
/// forage.cultivation.capacity_per_tender`. The exact twin of [`crate::fauna::herd_keeper_loads`],
/// which divides a herd's head count by the species' `animals_per_herder`: the web owns the ratio,
/// the rung owns the rate.
///
/// # ⛔ IT TAKES THE **TILE'S** `K`, NEVER [`ForagePatch::carrying_capacity`]
///
/// [`patch_carrying_capacity`] has already multiplied the tile's `K` by
/// `cultivation.field_capacity_gain`, **interpolated on the very ladder position the upkeep demand
/// interpolates on**. Reading the patch would therefore bill the gain and the rate's own climb
/// together, landing a Field near ten times a tended patch — a cost nobody chose. **The tile's `K` is
/// the size of the place; the gain is the rung's payout.** Callers resolve it through
/// [`tile_forage_capacity`], the single source of truth for the land's `K`.
///
/// # It is a MEASURE, not a demand — the ladder turns it into one
///
/// Both plant rungs declare `work_per_turn × scaled_by: source_load`, so `upkeep_demand = rate ×
/// loads`. [`NO_TENDER_LOAD`] where either term is not positive — barren ground, or a patch whose
/// tile is off the map — which is the same *"there is nothing here to keep"* the animal web's
/// `NO_KEEPER_LOAD` states.
pub fn patch_tender_loads(tile_capacity: f32, forage: &ForageLaborConfig) -> f32 {
    // NaN-safe by construction: every guard is a positive test, so a NaN input falls through to `0`
    // (no load) rather than sneaking past a negated comparison.
    let sane = tile_capacity > 0.0 && forage.cultivation.capacity_per_tender > 0.0;
    if !sane {
        return NO_TENDER_LOAD;
    }
    tile_capacity / forage.cultivation.capacity_per_tender
}

/// **GROUND THAT PRESENTS NOTHING TO TEND** — a tile with no forage capacity at all, or one that is
/// not on the map. Named because a bare `0.0` in a load position reads as a missing value rather than
/// the deliberate *"there is nothing here to keep"* it is. The plant twin of
/// [`crate::fauna::NO_KEEPER_LOAD`].
pub const NO_TENDER_LOAD: f32 = 0.0;

/// **ONE tender-load** — the measure at which both plant rungs' `upkeep.work_per_turn` is quoted, so
/// `rung.upkeep_demand(ONE_TENDER_LOAD)` reads back *"the work one tender-load costs"*. The plant twin
/// of [`crate::fauna::ONE_KEEPER_LOAD`], and the reading the **reference tile** presents by
/// construction: `cultivation.capacity_per_tender` ships at that tile's own `K`.
pub const ONE_TENDER_LOAD: f32 = 1.0;

/// **DOES THIS SOURCE DRAW ON THE BAND'S KEEPING POOL AT ALL?** — the boolean the three keeping seams
/// share (`docs/plan_standing_upkeep.md` §2.5/§4.6a): there is work on the ladder to hold, **or** a
/// verb in flight that is about to bank some.
///
/// # ⛔ THE VERB TERM IS THE ONE-TURN CARRY, AND DROPPING IT REOPENS A SHIPPED BUG
///
/// `maintenance_shares` runs **before** the turn's build accrual and the capture reads the patch
/// **after** it. On the turn a build banks its first work, a claim resolved on the position alone
/// reads zero, the share comes back zero, and the capture then publishes `supplied 0` against a live
/// demand on a **staffed** `agriculture` role. That is the defect `patch_keeping_meter`'s verb term
/// was added for, and it survives here in the only form the interpolated demand still needs.
///
/// **Exhaustive on the verb, on purpose** — a new plant verb falling through to `false` would leave
/// its first turn unclaimed, which is precisely that bug.
pub fn patch_claims_keeping(patch: &ForagePatch, improvement: Option<Improvement>) -> bool {
    let by_position = patch.ladder_position() > RUNG_UNSTARTED;
    let by_verb = improvement.is_some_and(|verb| match verb {
        Improvement::Cultivate | Improvement::Sow => true,
        Improvement::Tame | Improvement::Corral => false,
    });
    by_position || by_verb
}

// **RETIRED: `patch_is_maintaining`** — *"is this patch building or maintaining"*, the meter's own
// **fullness**, which used to decide who supplies the maintenance rate: the build crew below the
// meter's cost, the band's keeping pool at it (`docs/plan_standing_upkeep.md` §4.6a).
//
// **NOTHING ABOUT HOW FULL A METER IS DECIDES WHO PAYS.** The keeping pool owes the rate for every
// meter carrying work, from the first work banked until the last, and a build crew supplies nothing
// toward it. §2.4's autopsy names the two states the fullness test made unreachable, both reported
// from ordinary play: a **half-built** meter whose builders left could not be held at all — it was
// billed to a crew that was not there and bled its full rate with keepers idle in the role and no
// command that could aim them at it — and a **held** rung eroding to 99% flipped into *building*,
// where the next slice's queue would have had it displace the build the player actually ordered,
// then dip again the moment it was topped up.
//
// **RETIRED WITH IT: `patch_keeping_meter(patch, improvement)`.** It answered *which* of the two
// meters this turn's keeping spoke for, and there is only one meter now. Its two remaining jobs
// split cleanly: *how much* is [`patch_upkeep_demand`], which interpolates and needs no verb, and
// *does this claim at all* is [`patch_claims_keeping`], which keeps the verb term for the one-turn
// carry.

/// **WHAT HAS BEEN SUNK INTO THIS PATCH** — its [`ForagePatch::ladder_position`], in work units, and
/// [`RUNG_UNSTARTED`] for a wild patch.
///
/// It is the ordering key of [`crate::intensification::UpkeepFundMode::Priority`]: *most-invested
/// first*, so a band that cannot cover its whole plant web funds the Field before the garden and
/// lets the marginal ground rot.
///
/// **The position IS "what did this cost me"**, which is what the retired stored-cost reading was
/// approximating. It used to read the at-risk meter's *stamped cost* rather than its progress,
/// deliberately, so a meter eroding under a shortfall would not slide down the priority order
/// exactly as it started to need the hands — that hazard is real and is accepted here: a source
/// really is worth less once it has rotted, and there is no stamped cost left to prefer.
pub fn patch_at_risk_cost(patch: &ForagePatch) -> f32 {
    patch.ladder_position()
}

/// **THE WORK THE AT-RISK METER WAS OWED THIS TURN, AND THE KEEPING POOL OWES ALL OF IT**
/// (`docs/plan_standing_upkeep.md` §2.4/§4.6a).
///
/// **A meter carrying work is billed to the band's `agriculture` pool at any fullness** — from the
/// first work banked until the last — and a build crew supplies nothing toward it. What a crew
/// mid-`Cultivate` owes is what a finished tended patch owes, and it is owed to the same hands. The
/// retired fullness test is what made a half-built meter unholdable and a dipped rung the builders'
/// business again; `patch_is_maintaining`'s gravestone above carries both autopsies.
///
/// `keeping_share` is this source's slice of that pool ([`crate::systems::maintenance_shares`]) — a
/// work amount, not a crew, because a pool does not divide into whole people.
///
/// [`NO_UPKEEP_DEMAND`] where there is no work on the ladder and none being started: nothing is
/// owed, so nothing can be short.
pub fn patch_upkeep_supply(
    patch: &ForagePatch,
    improvement: Option<Improvement>,
    keeping_share: f32,
) -> f32 {
    if patch_claims_keeping(patch, improvement) {
        keeping_share
    } else {
        NO_UPKEEP_DEMAND
    }
}

/// **THE BILL THE KEEPING IS JUDGED AGAINST** — [`ForagePatch::upkeep_demanded`] where a band
/// answered for this source, and the live [`patch_upkeep_demand`] where none did.
///
/// **One function, three readers** — the decay pass, the published shortfall and the published rot —
/// so what the sim bleeds, what the wire bills and what the wire forecasts cannot be three different
/// demands. The `None` arm is what keeps an **abandoned** patch honest: nobody was handed a bill, so
/// the whole of the live one went unmet and the ground reverts.
pub fn patch_keeping_basis(
    patch: &ForagePatch,
    ladder: &LadderConfig,
    tile_capacity: f32,
    forage: &ForageLaborConfig,
) -> f32 {
    patch
        .upkeep_demanded
        .unwrap_or_else(|| patch_upkeep_demand(patch, ladder, tile_capacity, forage))
}

/// **WHAT THIS PATCH'S AT-RISK METER WILL LOSE ON THE NEXT DECAY PASS**, in work units — what
/// the **next** [`advance_cultivation`] will take off it, quoted through the rung's own
/// [`RungDef::meter_rot`] seam so the published number and the pass that applies it cannot use two
/// different rates.
///
/// **It is a forecast the sim can make exactly, not an estimate**: that pass judges the supply this
/// turn has just stamped, so the bleed is already determined — `RungDef::meter_rot` states the
/// ordering. It is `0` for as long as the grace forgives the shortfall.
///
/// **The build countdown's denominator and the wire's `meterRotPerTurn` are this one number**: what
/// eats a build is not the maintenance rate (the keeping pool owes that whatever the builders do) but
/// the ground going backwards under them.
///
/// [`NO_UPKEEP_DECAY`] on a wild patch, on one whose keeping covers its demand, and on one still
/// inside its rung's grace.
pub fn patch_meter_rot(
    patch: &ForagePatch,
    ladder: &LadderConfig,
    tile_capacity: f32,
    forage: &ForageLaborConfig,
) -> f32 {
    patch_unwinding_rung(patch, ladder).map_or(NO_UPKEEP_DECAY, |rung| {
        // **Against the bill that was issued**, not the rung's own declared rate — the demand this
        // web owes is interpolated *and* scaled by the tile's tender-loads, so the rung's number is
        // not what these keepers were asked for.
        rung.meter_rot_against(
            patch_keeping_basis(patch, ladder, tile_capacity, forage),
            patch.upkeep_supplied,
            patch.neglect_turns,
        )
    })
}

/// **WHAT WENT UNMET THIS TURN**, in work units — [`patch_upkeep_demand`] less what the meter's own
/// crew supplied, floored at zero. Exactly what [`RungDef::upkeep_decay`] bleeds off the at-risk meter once
/// the shortfall has outlasted that rung's grace.
///
/// **It is DERIVED, never stored, and that is what keeps an unworked patch honest.** The labor arm
/// only visits sources some band is assigned to, so a stored shortfall would read a tidy `0` on
/// exactly the patches that are bleeding — a wire row saying *"demand 0.75, supplied 0, shortfall 0"*
/// while the sim reverts the ground underneath it.
pub fn patch_upkeep_shortfall(
    patch: &ForagePatch,
    ladder: &LadderConfig,
    tile_capacity: f32,
    forage: &ForageLaborConfig,
) -> f32 {
    upkeep_shortfall(
        patch_keeping_basis(patch, ladder, tile_capacity, forage),
        patch.upkeep_supplied,
    )
}

/// **The MAINTAIN activity's own `workers_needed`** — whole keepers to meet
/// [`patch_upkeep_demand`], `0` on a wild patch. The plant twin of the herd row's, and the readout
/// that makes the standing cost legible: *"this wants 1, you have 0"*.
///
/// **IT IS PUBLISHED WHILE THE METER IS STILL BEING BUILT TOO**, and it means exactly the same thing
/// there: the keeping pool owes the rate from the first work banked, so these are the hands that
/// hold a half-built meter as much as a finished one. It is **not** a minimum viable build crew —
/// a build crew supplies nothing toward the rate (`docs/plan_standing_upkeep.md` §4.6a), so a lone
/// builder against a demand of `2.0` still banks its whole turn's work.
///
/// **IT IS `ceil` OF THE INTERPOLATED DEMAND, not of the at-risk rung's own rate.** Reading the
/// rung would say *"two hands"* about a patch one work unit into a Cultivate that is billed a
/// fiftieth of a hand — the readout and the bill describing different sources, which is exactly the
/// drift the demand seam exists to prevent. It rounds **up**, so any live demand at all asks for at
/// least one keeper: you cannot send a fiftieth of a person.
///
/// **AND IT IS `ceil` OF THE BILL, THE SAME [`patch_keeping_basis`] THE ROW'S OTHER TWO TERMS READ.**
/// The wire states the identity `upkeepWorkersNeeded == ceil(upkeepDemand / PER_WORKER_OUTPUT)` and
/// tells the client to do no arithmetic of its own, so a second reading here is a row that
/// contradicts itself: the bill is stamped *before* the turn's build accrual, so the live demand at
/// capture is already the higher number, and a patch mid-`Sow` published *"wants 3, you have 2"*
/// beside a shortfall of zero.
pub fn patch_upkeep_workers_needed(
    patch: &ForagePatch,
    ladder: &LadderConfig,
    tile_capacity: f32,
    forage: &ForageLaborConfig,
) -> u32 {
    let demand = patch_keeping_basis(patch, ladder, tile_capacity, forage);
    if demand <= NO_UPKEEP_DEMAND {
        return NO_CREW_ON_THIS_ACTIVITY;
    }
    (demand / crate::intensification::PER_WORKER_OUTPUT).ceil() as u32
}

pub fn advance_cultivation(
    mut registry: ResMut<ForageRegistry>,
    ladder_config: Res<LadderConfigHandle>,
    labor_config: Res<LaborConfigHandle>,
    tile_registry: Res<crate::resources::TileRegistry>,
    tiles: Query<&Tile>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
) {
    let ladder = ladder_config.get();
    let labor = labor_config.get();
    let forage = &labor.forage;
    for patch in registry.patches.values_mut() {
        // **THE SIZE OF THE LAND UNDER THIS PATCH** — the tile's own `K`, looked up exactly as
        // `advance_forage_regrowth` looks it up, because the plant upkeep is quoted **per
        // tender-load** (`patch_tender_loads`) and one tile is what a load measures. A patch whose
        // tile is absent from the map reads [`NO_FORAGE_CAPACITY`] and therefore no load at all —
        // the same *"ground nobody can see offers nothing"* reading the regrowth pass, the labor
        // arm's composition and the capture already take, rather than a substituted capacity that
        // would bill land that is not there.
        let tile_capacity = tile_registry
            .index(patch.tile.x, patch.tile.y)
            .and_then(|entity| tiles.get(entity).ok())
            .map_or(NO_FORAGE_CAPACITY, |tile| {
                tile_forage_capacity(forage, tile)
            });
        // **Newest first, through the one seam the wire reads too.** Exactly one meter is ever at
        // risk — the Field while it has anything left, then the tended ground under it — and that
        // rung owns *both* halves of the question, because the grace sits inside the same `upkeep`
        // block as the demand it forgives. Every number here is the ladder's
        // (`crate::intensification`), so a rung can be retuned without this system knowing what it
        // says.
        let at_risk = patch_unwinding_rung(patch, &ladder);
        // **The shortfall is DERIVED here rather than read off the patch**, and that is what makes
        // an abandoned patch decay at all: the labor arm stamps a shortfall only on patches some
        // band is working, so a patch nobody works would otherwise report a tidy `0` unmet — the
        // exact state that must bleed. What the crew supplied is the one thing stored, because it is
        // the one thing a crew authors.
        //
        // **This pass cannot see a band's build queue and does not need to** ([`NOTHING_IN_FLIGHT`]):
        // it runs a whole stage after the accrual that banked the turn's work, so any meter a verb
        // is filling already carries progress and answers on the progress term alone.
        // **THE BILL THE KEEPERS WERE HANDED, not the one that has since risen** — see
        // `ForagePatch::upkeep_demanded`. Judging the lagged supply against a demand that moved
        // under it makes a fully-staffed source permanently short the moment its meter climbs.
        let demand = patch_keeping_basis(patch, &ladder, tile_capacity, forage);
        let shortfall = upkeep_shortfall(demand, patch.upkeep_supplied);
        // **HOW SHORT, as a fraction of what was asked** — what the decay actually rides
        // (`crate::intensification::upkeep_shortfall_fraction`). The absolute shortfall still gates
        // the counter, because *any* unmet work is an unmet turn.
        //
        // **This covers a BUILD in flight as well as a held rung**, and it is the same subtraction
        // for both: the maintenance rate is owed either way, and only the supplier moved. A build
        // crew below the rate leaves a shortfall and the meter goes backwards; one exactly at it
        // holds; one above it banks the surplus (`RungDef::build_accrual`).
        let shortfall_fraction =
            crate::intensification::upkeep_shortfall_fraction(demand, patch.upkeep_supplied);
        if shortfall <= NO_UPKEEP_DEMAND {
            // The demand was met (or there was none to meet). Forgive whatever neglect had
            // accumulated, so the grace is about *consecutive* shortfall rather than a lifetime
            // budget.
            patch.neglect_turns = NEGLECT_NONE;
        } else {
            patch.neglect_turns = patch.neglect_turns.saturating_add(1);
            if let Some(rung) = at_risk {
                // **The decay is PROPORTIONAL to the shortfall, at the rung's own rate, past the
                // grace** — three dials answering three questions (how much work holding this wants,
                // how fast it rots when you stop, how long before it starts) where the shortfall
                // used to *be* the decay and the first two were welded together. `upkeep_decay` owns
                // both the rate and the grace comparison, so this system never restates the `>` that
                // decides whether the penalty is biting.
                let decay = rung.upkeep_decay(shortfall_fraction, patch.neglect_turns);
                if decay > NO_UPKEEP_DECAY {
                    // **ONE CALL, AND THE UNWIND IS ARITHMETIC.** The position falls; because a
                    // Field's range sits above the tended rung's, the Field is consumed first and
                    // the ground beneath it is untouched until the Field is wholly gone. The
                    // newest-first rule this pass used to spell out is now a property of the number
                    // — including the ORDER these announce in, which is the order they were lost.
                    for lost in patch.decay_ladder(decay, &ladder) {
                        announce_rung_lost(
                            &mut event_log,
                            tick.0,
                            patch.owner,
                            lost.builder_verb(),
                            patch.tile,
                        );
                    }
                }
            }
        }
        // **Losing ground needs no flag**: a meter below its cost derives its own rung's verb
        // ([`patch_build_verb`]), so the labor arm sees a build in flight next Population stage
        // without anything being stamped here.
        //
        // **The turns estimate**, on the one-turn cycle: a build the player abandoned must stop
        // publishing a finish date, and the labor arm re-stamps it this turn if a crew is still on it
        // (Logistics runs before Population).
        patch.build_turns_remaining = None;
        patch.build_work_from_gear = NO_BUILD_GEAR;
        patch.build_queue_position = crate::intensification::NOT_IN_ANY_BUILD_QUEUE;
        patch.build_blocked_reason = crate::intensification::BuildGate::Open;
        patch.build_destination = None;
        patch.build_legs = Vec::new();
        // **And this turn's supply**, on the same cycle and for the same reason: it describes the
        // keepers that held the patch, so a patch whose keepers have gone must stop reporting what
        // they paid. Clearing it is also what re-arms this pass — next turn's shortfall is the whole
        // demand again unless somebody restates it.
        patch.upkeep_supplied = NO_UPKEEP_DEMAND;
        patch.upkeep_demanded = None;
    }
}

/// **Announce a lost plant rung** — the plant twin of `fauna::announce_pen_lost`, and pushed on the
/// same edge: the turn a *completed* improvement crosses back below its own cost. A completed rung
/// is 25 turns of forgone harvest, so losing it is never silent; the partial bleed that follows is not
/// announced, because the thing that mattered has already happened.
///
/// Rides the verb's **own** feed kind (`cultivate` / `sow`), so a rung's whole life — the command, the
/// completion, the loss — reads on one channel, exactly as the pen's does.
fn announce_rung_lost(
    event_log: &mut CommandEventLog,
    tick: u64,
    owner: Option<FactionId>,
    verb: Option<Improvement>,
    tile: UVec2,
) {
    let (Some(owner), Some(verb)) = (owner, verb) else {
        return;
    };
    let (kind, what) = match verb {
        Improvement::Sow => (CommandEventKind::Sow, "field"),
        _ => (CommandEventKind::Cultivate, "tended patch"),
    };
    let (x, y) = (tile.x, tile.y);
    event_log.push(CommandEventEntry::new(
        tick,
        kind,
        owner,
        format!("The {what} at ({x}, {y}) has gone feral — untended, the ground is reverting"),
        Some(format!(
            "status=feral reason=untended action={} x={x} y={y}",
            verb.as_str()
        )),
    ));
}

/// Apply one turn of **pure logistic** regrowth toward the patch's carrying capacity and refresh its
/// ecology phase. Unlike a wild herd (`fauna::regrow_biomass`, which crashes below the Allee
/// threshold and despawns), a patch has no critical-depensation crash — a depleted (feral) patch
/// always recovers, and patches never despawn.
///
/// **Reseed floor.** `logistic_regrowth` returns `0` at `biomass == 0`, so a patch driven to exactly
/// `0` (repeated Eradicate + f32 underflow, `take_fraction = 1.0`, or a restored snapshot carrying
/// `biomass = 0`) would otherwise be stuck at `0` forever — contradicting the "always recovers"
/// invariant. To model plants reseeding from surrounding vegetation, a depleted patch is first lifted
/// to a small standing crop (`reseed_floor_fraction × carrying_capacity`) before regrowth, so it
/// recovers from that floor via the normal logistic curve. The lift only touches patches below the
/// floor — a healthy patch is untouched — and the floor is small (below `collapse_fraction`), so
/// Eradicate still crashes a patch hard into the Collapsing band; it just can't hold it at `0`.
///
/// **The patch's OWN ecology** ([`patch_ecology`]), never `forage.ecology` reached for directly: a
/// tended patch regrows on the boosted `r` its rung bought, which is what makes its faster MSY a
/// harvest the land can actually sustain rather than a promise the stock cannot keep. The animal
/// mirror is `fauna::regrow_biomass`, which resolves `herd_ecology` for exactly this reason.
fn regrow_patch(patch: &mut ForagePatch, forage: &ForageLaborConfig) {
    let ecology = patch_ecology(patch, forage);
    // The reseed lift + logistic step is the shared plant curve (`fauna::reseeding_logistic_regrowth`),
    // so the human-edible forage stock and the animal-edible graze stock can never drift apart.
    patch.biomass = reseeding_logistic_regrowth(
        patch.biomass,
        patch.carrying_capacity,
        ecology.regrowth_rate,
        forage.reseed_floor_fraction,
    );
    patch.refresh_ecology_phase(&ecology);
}

/// **The rung a patch stands on** — the plant ladder resolved for one patch, top-down: sown →
/// `plant:field`, cultivated → `plant:tended`, else `plant:wild`. The exact twin of
/// `fauna::herd_rung`, and the same seam: a system asks the patch for its rung and reads what that
/// rung declares, rather than re-deriving the ladder from `is_cultivated()` at the call site.
///
/// Its one reader today is the Forage arm of `advance_labor_allocation` — **which knowledge this
/// patch's rung teaches** (`RungDef::knowledge_earned`, slice 4). The plant web has no movement
/// primitive to dispatch (a patch is a place), so unlike the animal side there is no second caller.
pub(crate) fn patch_rung<'a>(patch: &ForagePatch, ladder: &'a LadderConfig) -> &'a RungDef {
    ladder.rung(patch_rung_key(patch))
}

/// **[`patch_rung`] without the ladder** — the same top-down reading, answered as the key rather than
/// the record, for the callers that want to walk the ladder from it ([`RungKey::above`], and the
/// `RungKey`-taking seams like [`resolve_committed_species`]) instead of reading a record's dials.
///
/// It exists so the "sown → field, cultivated → tended, else wild" test has exactly **one** home: a
/// projection that re-derived it from `is_cultivated()` at its own call site is the second copy this
/// seam was created to prevent.
pub(crate) fn patch_rung_key(patch: &ForagePatch) -> RungKey {
    patch.standing().held
}

/// The forage counterpart of `fauna::hunt_take`: resolve the **escapement ceiling**, cap it by the
/// gathering crew's throughput (`workers × per_worker_biomass_capacity × seasonal`), clamp to the
/// patch's remaining biomass, and convert the take to provisions (× the caller's productivity
/// `output_multiplier`). Returns the provisions gathered.
///
/// **The two webs' take paths are the same expression** (`docs/plan_harvest_floor.md` §1):
/// `min(crew throughput, max(0, B − floor·K))`. The **floor** is a fraction of `K` the assignment
/// carries (`0.5` holds the patch on its most productive biomass, `0` strips it).
///
/// **`workers` is the TAKE crew and there is no build term in the expression at all**
/// (`docs/plan_standing_upkeep.md` §2.2). A build on this patch is its own allocation with its own
/// hands, so what the gatherers carry does not depend on what the builders beside them are doing.
///
/// The take resolves the patch's **conversion rate** off its own basket as well as its ecology, so
/// it carries the tile's composition and the flora table alongside the forage config — one extra
/// reference each, not one extra model.
///
/// **WHAT THE CREW IS HERE FOR is `take_species`** (the selective gather). Empty — the default —
/// takes the whole basket and is byte-identical to the take before the selection existed. Naming
/// plants scales the escapement ceiling by their summed share, converts at *their* basket average
/// ([`narrowed`]), and draws down **only what was taken**: gathering the wheat does not trample the
/// cotton, so the biomass hit is never scaled back up to the whole stand.
#[allow(clippy::too_many_arguments)]
pub(crate) fn forage_take(
    patch: &mut ForagePatch,
    tile_composition: &[FloraShare],
    workers: u32,
    floor: f32,
    // **Which plants this crew carries home** — empty is the whole basket.
    take_species: &TakeSelection,
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    // **This crew's resolved BASKET tier**, in biomass/worker before the season — see
    // `forage_per_worker_biomass`.
    per_worker_biomass_capacity: f32,
    seasonal: f32,
) -> Scalar {
    // The stance's escapement ceiling + the gather throughput, both from the shared helpers the
    // pre-commit forecast (`forage_forecast`) reads — the take and the forecast can never disagree.
    // The ceiling is `r`-independent, so unlike the retired MSY skim it does **not** vary with the
    // patch's rung: what a tended patch buys is a faster refill, which shows up next turn as more
    // stock standing above the floor. One call still serves rungs 1 and 2 alike.
    // **THE SELECTION SCALES THE OFFER, and nothing else about it.** Only the named plants' share of
    // the stand is standing there to be carried home, so the ceiling and the standing-crop clamp
    // below are both taken on that share. `WHOLE_BASKET` when the crew named nothing.
    let selected = selected_biomass_share(
        &patch_composition(patch, tile_composition, forage),
        take_species,
    );
    let take_ceiling =
        forage_escapement_ceiling(floor, patch.biomass, patch.carrying_capacity) * selected;
    // **`workers` IS THE TAKE CREW, and it is the only crew in this expression**
    // (`docs/plan_standing_upkeep.md` §2.2). A build and a keeping on this same patch are their own
    // allocations with their own hands; nothing they do scales what these gatherers carry. The
    // retired `yield_fraction_while_building` multiplied this term to say *"the crew is clearing,
    // not gathering"* — which is a statement the player now makes by putting the hands where they
    // want them, rather than one the sim derives from a fraction.
    let worker_cap =
        workers as f32 * forage_per_worker_biomass(per_worker_biomass_capacity, seasonal);
    let take = worker_cap
        .min(take_ceiling)
        .max(0.0)
        // The selected species' own standing crop — belt-and-braces beside the ceiling, which is
        // already `≤ B × selected` for any floor `≥ 0`, and the honest bound either way: a crew
        // cannot carry home more wheat than there is wheat.
        .clamp(0.0, patch.biomass * selected);
    // The **conversion** half of the commit trade: every patch turns its biomass into food at its own
    // effective basket's share-weighted average, with the tended rung's gain on the favored crop.
    // Resolved before the take is applied so it reads the same patch state the ceiling did — and over
    // the **selected** subset, so what one unit of the take converts at is what the crew chose.
    let rate =
        patch_provisions_per_biomass_taking(patch, tile_composition, flora, forage, take_species);
    patch.biomass -= take;
    // FOOD income is fully fractional (a few foragers may gather < 1 provision/turn).
    scalar_from_f32(forage_provisions(take, rate, output_multiplier))
}

/// The **biomass standing above the assignment's floor** at the patch's current stock — the single
/// source of the gather ceiling, shared by `forage_take` (the take path) and `forage_forecast` (the
/// pre-commit forecast), and the exact plant-web twin of `fauna::hunt_escapement_ceiling`:
///
/// ```text
/// max(0, B − floor·K)
/// ```
///
/// **Constant escapement replaced the four per-stance RATES** (`docs/plan_harvest_floor.md` §1): the
/// MSY skim, its `surplus_multiplier`, and the two fraction-of-stock draws (`market.take_fraction`,
/// `eradicate.take_fraction`) are all one expression parameterised by a floor, which the assignment
/// now carries directly ([`crate::components::LaborTarget::Forage`]). Not yet clamped to biomass —
/// callers do that alongside their own throughput cap, and it is belt-and-braces there
/// (`B − floor·K ≤ B` for any floor `≥ 0`).
///
/// **No `ecology`, no `ForageLaborConfig`, and that removal is the point.** An escapement ceiling is
/// `r`-INDEPENDENT, so a take path that cannot reach the growth curve cannot accidentally start
/// depending on it again. The rung-2 payoff is unchanged in substance and clearer in mechanism: a
/// tended patch regrows faster, so *next* turn it has more stock standing above the floor.
///
/// **NO BUILD TERM REACHES THIS CEILING, and none reaches the crew term beside it either**
/// (`docs/plan_standing_upkeep.md` §2.2). A rung's `yield_fraction_while_building` used to scale this
/// ceiling and then, briefly, the crew; it is retired on both webs. The build has its own crew now,
/// so *"these hands are clearing, not gathering"* is a fact about where the player put them rather
/// than a fraction the sim multiplies — and the price is the same statement at every staffing, where
/// the dip's depended on whether the patch's standing stock happened to be binding.
pub(crate) fn forage_escapement_ceiling(floor: f32, biomass: f32, carrying_capacity: f32) -> f32 {
    escapement_ceiling(floor, biomass, carrying_capacity)
}

/// **Can a crew of `workers` gatherers draw THIS patch to `floor`, and is that floor below the food
/// peak?** — the plant web's producer of [`SourceYield::overdraws`], and the only thing the Forage
/// arms (resolved and seeded) publish that flag through. The animal twin is
/// [`crate::fauna::hunt_take_overdraws`]; the predicate they share is
/// [`crate::components::take_overdraws`].
///
/// **The stock terms are the SELECTED share's**, exactly as the row's `sustainable` is: a crew
/// carrying one species out of a mixed stand is drawing down *that* stand, and the logistic curve is
/// homogeneous in `(B, K)`, so scaling both is the same patch seen at the size the crew is working.
/// `crew_biomass_per_turn` is **not** scaled — a gatherer carries what a gatherer carries, and the
/// take is `min(crew, ceiling × share)`.
///
/// **There is no engagement bound on this web** — nothing is stalked and nothing breaks off — so the
/// crew's throughput is simply what it can carry, which is where the animal twin's `min` comes from.
pub fn forage_take_overdraws(
    patch: &ForagePatch,
    forage: &ForageLaborConfig,
    biomass: f32,
    carrying_capacity: f32,
    crew_biomass_per_turn: f32,
    floor: f32,
) -> bool {
    let ecology = patch_ecology(patch, forage);
    let (low, high) = floor_reach_band(floor, biomass, carrying_capacity);
    take_overdraws(
        floor,
        crew_biomass_per_turn,
        peak_regrowth_between(carrying_capacity, low, high, |stock| {
            reseeding_logistic_regrowth(
                stock,
                carrying_capacity,
                ecology.regrowth_rate,
                forage.reseed_floor_fraction,
            ) - stock
        }),
    )
}

/// Biomass one forager can gather this turn (`per_worker_biomass_capacity × seasonal_weight`) — the
/// per-worker throughput `forage_take`'s worker cap multiplies by the head-count, shared with the
/// forecast. Hunting has no seasonal factor, so it has no counterpart helper.
///
/// **`per_worker_biomass_capacity` is a RESOLVED tier, not a config read** (`plan_hunt_through_combat`
/// §4.8): a band with baskets gathers at `labor_config.json`'s `forage.per_worker_biomass_capacity`
/// and a bare-handed one at `equipment.json`'s `basket_kit.unequipped_per_worker_biomass_capacity`,
/// resolved once per band per turn through
/// [`crate::equipment_config::EquipmentConfig::forage_per_worker_biomass_capacity`]. Sites with no
/// band to resolve against (a tile's telemetry, a Field's managed collection cap) pass the shipped
/// *equipped reference* rate, exactly as `HerdTelemetryState::per_worker_biomass` does on the animal
/// web.
pub fn forage_per_worker_biomass(per_worker_biomass_capacity: f32, seasonal: f32) -> f32 {
    per_worker_biomass_capacity * seasonal.max(0.0)
}

/// Biomass → provisions for a gather take (× the caller's productivity multiplier) — the one
/// conversion `forage_take` pays, shared with the forecast. The plant mirror of
/// the animal web's `HuntYield::apply` (which retired the global `fauna::hunt_provisions`).
pub fn forage_provisions(
    biomass_take: f32,
    provisions_per_biomass: f32,
    output_multiplier: f32,
) -> f32 {
    biomass_take * provisions_per_biomass * output_multiplier
}

/// **What a patch would pay its gatherers as a TENDED patch**, in provisions — its Sustain (MSY)
/// ceiling on the *tended* curve ([`tended_ecology`]), clamped to the standing crop.
///
/// This is the plant ladder's **rung-2 payoff quote**, and slice 7 retargeted what it means. It used
/// to be `biomass × tended_provisions_per_biomass` — a *managed rate*, paid whatever the policy, never
/// drawing the patch down. But rung 2 is **still a wild stand**: what tending buys is a faster curve,
/// so the honest quote is "the best sustainable skim this patch will offer once tended", which is
/// exactly the number the tended patch's own `ceiling_sustain` then reads. Its consumer is the
/// forecast's `managed_yield` — the "then Y" of Cultivate's *"preparing X → then Y"* pair — and the
/// wire's `ForagePatchState.tendedYield`.
///
/// The rung-3 twin, [`field_provisions`], **stays** a managed rate: a Field is yours.
pub(crate) fn tended_provisions(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        rung_msy_take(patch, forage, RungKey::PlantTended),
        // The rate this patch converts at **on the TENDED rung** — asked about rung 2 by name, so a
        // patch mid-Cultivate quotes the weeded basket it is planting rather than the one it is
        // still in, and a patch that has already climbed to a Field still answers what rung 2 pays
        // here rather than borrowing rung 3's.
        rung_provisions_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantTended),
        output_multiplier,
    )
}

/// **THE take a rung-2 quote is priced on** — the Sustain (MSY) skim on the *tended* curve, clamped to
/// the standing crop. Stated once because rung 2 pays **three** accounts off one take
/// ([`tended_provisions`], [`tended_fodder`], [`tended_trade_goods`]), and a second copy of this
/// expression is exactly how the food quote and the trade quote would start describing different
/// harvests — the `patch_ecology` lesson, applied to the take instead of the curve.
///
/// It is the **quote's** take, not a policy's: worker-unconstrained and policy-blind, the same
/// convention `tendedYield` has always been published under. What the sim actually credits rides
/// `forage_take`'s policy ceiling and worker cap ([`tended_take_fodder`] /
/// [`tended_take_trade_goods`]), and under `Sustain` the two coincide.
fn rung_msy_take(patch: &ForagePatch, forage: &ForageLaborConfig, rung: RungKey) -> f32 {
    // **THE GROUND AS `rung` WOULD HAVE IT** — the patch's own capacity re-based onto the asked-about
    // rung's, and its standing crop with it. `fieldYield` is published for **every** patch including
    // a tended one, so a rung-3 quote taken on the patch's own `K` would quote rung 3's curve over
    // rung 2's land and read as though the Field bought only its regrowth gain.
    //
    // **It is a RATIO, so a patch already standing on `rung` re-bases by exactly `1.0`** — its stored
    // `K` already carries that rung's gain — which is what keeps the live reading and the quote one
    // number. `composition_for_rung`'s rule, applied to the land instead of the basket.
    let rebase = rung_capacity_gain(rung, forage)
        / patch_interpolate(patch, |held| rung_capacity_gain(held, forage));
    let capacity = patch.carrying_capacity * rebase;
    let biomass = patch.biomass * rebase;
    sustainable_yield(biomass, capacity, &rung_ecology(rung, forage)).clamp(0.0, biomass)
}

/// **What a patch would pay in FODDER as a TENDED patch** — the rung-2 quote twin of
/// [`tended_provisions`], routing the yield vector's fodder component instead of its provisions one.
/// The hay counterpart of [`field_fodder`] one rung down, and the number the crop picker's Cultivate
/// rung needs: before this, the picker had only `sowFodderPayoff` and therefore quoted a *sown Field's*
/// hay on the Cultivate row.
///
/// **Priced on [`rung_msy_take`], the same take the food quote uses**, and converted through the
/// same [`rung_rate`] seam at `PlantTended` — so the three accounts of one rung-2 harvest are one
/// harvest, split three ways, and cannot disagree about its size. `0` for a crop whose vector pays no
/// fodder, with no `role` branch.
pub(crate) fn tended_fodder(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        rung_msy_take(patch, forage, RungKey::PlantTended),
        rung_fodder_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantTended),
        output_multiplier,
    )
}

/// **WHAT A RUNG BUYS THE LAND — production, never draw.**
///
/// # ⛔ PRODUCTION AND DRAW ARE SEPARATE CONCERNS
///
/// A rung may change how much the tile **grows**. **No rung changes how it is drawn.** Every plant
/// rung is foraged through the same `forage_take` path — escapement floor live, worker-capped, drawn
/// down — so a Field **can be over-farmed** and the overdraw ⚠ fires on it, exactly as it does on a
/// tended patch and a wild stand.
///
/// Rung 3 used to break that: a flat managed rate on a crop that was never drawn down. It made the
/// harvest floor — the one pressure lever the player holds — do **nothing** at rung 3, and it is why
/// the rung's payout could not interpolate, being a different *kind* of harvest from the rung below.
///
/// **This is the animal web's shape**, which is the argument for it: a herd already gets a regrowth
/// multiplier and a density multiplier on the land's capacity at pastoral and again at pen.
///
/// The two gains **multiply** through `r × K / 4`; see
/// [`crate::labor_config::CultivationConfig::field_regrowth_gain`].
fn rung_regrowth_gain(rung: RungKey, forage: &ForageLaborConfig) -> f32 {
    match rung {
        RungKey::PlantTended => forage.cultivation.tended_regrowth_gain,
        RungKey::PlantField => forage.cultivation.field_regrowth_gain,
        _ => NO_GROWTH_GAIN,
    }
}

/// **THE CAPACITY GAIN A RUNG BUYS** — see [`rung_regrowth_gain`] for the model. Only rung 3 raises
/// `K` today: a tended patch is better-kept ground, not denser planting.
///
/// **A rung may RAISE `K` and may never LOWER it** (#433). A retired concentration term shrank
/// capacity to `share × gain` and threw the remainder away, which made a commitment cost production;
/// the config bound is `>= 1.0` so that cannot come back by a retune.
fn rung_capacity_gain(rung: RungKey, forage: &ForageLaborConfig) -> f32 {
    match rung {
        RungKey::PlantField => forage.cultivation.field_capacity_gain,
        _ => NO_GROWTH_GAIN,
    }
}

/// **A RUNG THAT CHANGES NOTHING ABOUT HOW THE LAND GROWS** — the identity, and the neutral of both
/// gains above. Named rather than a bare `1.0` so *"this rung buys no growth"* reads as a statement.
const NO_GROWTH_GAIN: f32 = 1.0;

/// **THE CURVE A PATCH GROWS ON** — the wild ecology with its `regrowth_rate` scaled by the growth
/// gain the patch's **position** has bought, leaving the shared phase bands
/// (`collapse_fraction`/`stressed_fraction`/`extinction_floor`) intact. The exact shape
/// `fauna::pastoral_ecology_for` gives a tamed herd.
///
/// **It INTERPOLATES on the ladder position** like every other rung quantity
/// (`docs/plan_standing_upkeep.md` §2.8), so a Sow half-raised grows at half the step between the
/// tended curve and the Field's — the payoff of a build starts on turn one.
///
/// Every consumer of a patch's ecology — regrowth, the MSY/policy ceilings, the phase classification,
/// the forecast — resolves it *here*. **No call site may re-derive it**: a second copy of this
/// mapping is exactly how a forecast starts promising a number the take won't pay (the lesson
/// `herd_ecology` already paid for).
pub fn patch_ecology(patch: &ForagePatch, forage: &ForageLaborConfig) -> EcologyConfig {
    scaled_ecology(
        forage,
        patch_interpolate(patch, |rung| rung_regrowth_gain(rung, forage)),
    )
}

/// [`patch_ecology`] asked about a rung the patch may not stand on — the forecast's *"what will this
/// pay once cultivated?"*, and the per-rung quotes'.
fn rung_ecology(rung: RungKey, forage: &ForageLaborConfig) -> EcologyConfig {
    scaled_ecology(forage, rung_regrowth_gain(rung, forage))
}

/// The wild curve with only its `regrowth_rate` scaled — stated once so the live reading and the
/// per-rung quote cannot become two curves.
fn scaled_ecology(forage: &ForageLaborConfig, gain: f32) -> EcologyConfig {
    EcologyConfig {
        regrowth_rate: forage.ecology.regrowth_rate * gain,
        ..forage.ecology
    }
}

/// **WHAT THIS PATCH'S GROUND HOLDS** — the tile's own capacity times the gain its position has
/// bought, interpolating like the curve above.
///
/// **The LAND still owns `K`**: this is a multiplier on `tile_forage_capacity`, applied at the one
/// write in [`advance_forage_regrowth`], which recomputes from the tile every turn. So it is
/// idempotent, a retuned `capacity_by_biome` still reaches patches already on the map, and a lapsed
/// Field hands the capacity straight back.
pub fn patch_carrying_capacity(
    tile_capacity: f32,
    patch: &ForagePatch,
    forage: &ForageLaborConfig,
) -> f32 {
    tile_capacity * patch_interpolate(patch, |rung| rung_capacity_gain(rung, forage))
}

// **RETIRED: `rung_carrying_capacity`** — the tile's capacity times a named rung's gain, for the
// per-rung quotes. [`rung_msy_take`] re-bases onto the asked-about rung itself, and it must be the
// only place that does or a quote taken through both would carry the gain twice.

// **RETIRED: the whole rung-3 MANAGED HARVEST** — `field_provisions`, `field_fodder`,
// `field_harvest_production`, `field_harvest_biomass`, `field_fodder_per_biomass`,
// `patch_species_quality`, `managed_per_worker_yield`, `managed_per_worker_fodder`, and the
// `cultivation.field_provisions_per_biomass` dial they all read.
//
// **A FIELD CHANGED HOW YOU HARVEST, WHEN ITS JOB IS TO CHANGE HOW MUCH THE TILE GROWS.** It paid a
// flat `biomass × rate` on a standing crop that was never drawn down — no escapement floor, no
// worker cap on the *production* term, no overdraw. Three things followed, and all three were wrong:
//
// 1. **The harvest floor did nothing at rung 3.** The one pressure lever the player holds was inert
//    on the rung the whole ladder climbs toward.
// 2. **The payout could not interpolate.** A managed rate and an MSY draw-down are different *kinds*
//    of harvest, not two values of one rate, so the tended↔Field boundary stayed a cliff while every
//    other rung quantity had become continuous (`docs/plan_standing_upkeep.md` §2.8).
// 3. **A Field could not be over-farmed**, which made rung 3 a strictly-better thing rather than a
//    commitment with a failure mode.
//
// **Production and draw are separate concerns. A rung may change production; no rung changes the
// draw.** A Field is now foraged through the ordinary `forage_take` path exactly as a tended patch
// and a wild stand are — floor-live, worker-capped, drawn down, `sustainable != actual` reachable and
// the ⚠ firing on it — and what rung 3 buys is [`rung_capacity_gain`] and [`rung_regrowth_gain`].
// **The animal web has had that shape all along**; plants were the odd web out.
//
// `patch_species_quality` went with them because it existed only to normalize a *rate dial* against
// the wild baseline; with no rate dial there is nothing to normalize, and the crop's own conversion
// already rides `rung_rate` in every account.

// **RETIRED: `field_trade_per_biomass` / `field_trade_goods` / `managed_per_worker_trade`**
// (arc #527). A sown cash Field's product is its **materials** — cotton fibre, tobacco leaf — banked
// as batches by `credit_material_yield` off the same `field_harvest_biomass` these three converted,
// and it always was. The trade scalar beside them was the flattened duplicate.

/// **The rate a basket the roster cannot decompose pays in the non-food accounts** — nothing. It
/// is the [`basket_rate`] fallback for fodder, where the food account falls back to
/// `forage.provisions_per_biomass` instead: a stand nobody can name pays *some* food (it is food, that
/// is why the tile has a capacity at all) but no hay. Named rather than a bare `0.0`
/// because at these call sites the zero is a *statement about an undecomposable basket*, not an absent
/// value.
const NO_UNCOMMITTED_YIELD_RATE: f32 = 0.0;

/// **THE fodder conversion seam** — the fodder twin of [`patch_provisions_per_biomass`]: how well one
/// unit of *this* patch's biomass turns into hay, as the share-weighted average of its **effective**
/// basket. A wild tile that realizes `hay_grass` therefore pays hay on any harvest — the §3 spine is
/// unconditional — and a basket with no fodder crop in it pays [`NO_UNCOMMITTED_YIELD_RATE`], so a
/// tended grain patch credits no hay with no `role` branch.
///
/// **The wild credit's KNOWLEDGE gate is not here.** Whether a band may bank hay it did not commit to
/// is a question about the *faction* (Foddering), and it lives at the credit site in `systems/labor.rs`
/// so this seam stays free of knowledge lookups and commodity-generic.
pub(crate) fn patch_fodder_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    patch_fodder_per_biomass_taking(
        patch,
        tile_composition,
        flora,
        forage,
        &TakeSelection::EVERYTHING,
    )
}

/// [`patch_fodder_per_biomass`] for a crew that named **which plants it carries home** — the fodder
/// twin of [`patch_provisions_per_biomass_taking`], and the second of the three accounts the one
/// [`narrowed`] basket routes.
pub(crate) fn patch_fodder_per_biomass_taking(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    take: &TakeSelection,
) -> f32 {
    // The fodder twin of `patch_provisions_per_biomass`, interpolating on the same standing.
    patch_interpolate(patch, |rung| {
        rung_rate(
            patch,
            tile_composition,
            flora,
            forage,
            rung,
            take,
            |def| def.yield_.fodder_per_biomass,
            NO_UNCOMMITTED_YIELD_RATE,
        )
    })
}

/// The fodder rate this patch would convert at **standing on `rung`** — [`patch_fodder_per_biomass`]
/// asked about a rung by name, the fodder twin of [`rung_provisions_per_biomass`]. A *quote* must ask
/// by name: a patch mid-Cultivate has to be told what the rung it is building pays, not what the wild
/// stand it still is does.
fn rung_fodder_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        rung,
        &TakeSelection::EVERYTHING,
        |def| def.yield_.fodder_per_biomass,
        NO_UNCOMMITTED_YIELD_RATE,
    )
}

// **RETIRED: `patch_trade_per_biomass` / `rung_trade_per_biomass`** (arc #527) — the conversion
// seam for an account that no longer exists. A drawn-down patch's non-food product is the material
// rows its basket names, credited per species by `patch_material_yields` so a mixed tile's readings
// are never averaged into a plant that is not growing there.

/// **The FODDER a completed Tended Patch (rung 2) harvest pays** into the working band's `FODDER`
/// store — `take × the committed crop's fodder_per_biomass`, the fodder twin of the provisions
/// conversion [`forage_take`] itself performs, through the same [`forage_provisions`] arithmetic.
/// `0` for an uncommitted patch or a crop whose vector pays no fodder, so this is commodity-generic
/// with **no `role` branch** — a harvest of `B` biomass pays `B × yield.*` into all three accounts
/// (`docs/plan_flora_roster.md` §3), at every rung, not only at rung 3.
///
/// **Driven by the TAKE, not by a managed rate — the deliberate difference from [`field_fodder`].**
/// A Field is never drawn down, so its harvest collapses the policy axis and is quoted as a rate on
/// the standing crop. A tended patch *is* drawn down by the ordinary gather, so its non-food accounts
/// must ride the same take the food account does: `Deplete` on a tended hay patch earns more fodder
/// than `Sustain` because it takes more, and over-farming it shows up in the ⚠ exactly as it does for
/// food. **The take is already worker-capped** by `forage_take`'s `workers × per_worker_biomass`
/// term, so there is deliberately no second collection cap here — the crop the crew carries home is
/// the take it made.
///
/// **`take` narrows it with the food account** — the crop the crew carries home decides both, so a
/// crew that named the grain alone banks no hay off a mixed stand.
pub fn tended_take_fodder(
    take_biomass: f32,
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    take: &TakeSelection,
) -> f32 {
    forage_provisions(
        take_biomass,
        patch_fodder_per_biomass_taking(patch, tile_composition, flora, forage, take),
        output_multiplier,
    )
}

// **RETIRED: `tended_take_trade_goods`** (arc #527). #433 added it because a tended cash crop
// (`provisions_per_biomass: 0`) was being drawn down at full MSY and producing nothing in any
// currency — a real bug, fixed with the currency that existed at the time. The five cash crops are
// paid in **materials** now, credited off the same `forage_take` biomass by `credit_material_yield`
// at rungs 1, 2 and 3 alike, so the hole this closed stays closed without the scalar.

// **RETIRED: `field_yield_fraction_while_building`** — the `plant:field` rung's dip, looked up here
// because two plant sites needed it and only one of them went through the shared ceiling helper. The
// dip itself is gone from both webs (`docs/plan_standing_upkeep.md` §2.2): a build is staffed in its
// own right, so there is no fraction left for any seam to hand out.

/// `SourceYieldForecast::body_mass_yield` for a plant source (slice 8) — `0` = *do not quantise*.
///
/// **A deliberate asymmetry with the animal web, and a principled one — do not "fix" it.** A hunt take
/// is rounded down to whole animals because you cannot half-kill a deer; a gather is not, because you
/// harvest grain by the handful. The two food webs quantise differently because *their products
/// differ* — the same reason seed travels and a herd doesn't (`docs/plan_intensification_ladder.md`).
const PLANTS_DO_NOT_QUANTISE: YieldAccounts = YieldAccounts::ZERO;

/// **The plant web's forecast FODDER component — a KNOWN GAP, not a claim that plants grow no hay**
/// (`docs/plan_hunt_yield_model.md` §8, issue #426).
///
/// A hay Field really does credit its band's `FODDER` store every turn, so a patch's honest fodder
/// forecast is **not** zero — the sim simply has not projected it yet. The client renders a fodder
/// line only when the component is `> 0`, so a patch shows *no* fodder line rather than a false
/// "0 fodder/turn". Do not let a reader treat this as "plants pay no fodder".
pub(crate) const PLANT_FODDER_FORECAST_NOT_YET_PROJECTED: f32 = 0.0;

/// A plant source's provisions-only forecast component: the food number the plant web computes, with
/// its fodder component the [`PLANT_FODDER_FORECAST_NOT_YET_PROJECTED`] gap.
///
/// **This helper is the remaining half of #426 and is meant to disappear.** Projecting the fodder
/// account needs it built from the rung's *biomass* ceiling times that rung's own rate
/// (`rung_provisions_per_biomass` / `rung_fodder_per_biomass`), which is a restructure of
/// [`forage_forecast`] rather than a wider return type here: this signature takes an
/// already-converted food number and so has nothing left to convert the other account *from*.
fn plant_food_only(provisions: f32) -> YieldAccounts {
    YieldAccounts {
        provisions,
        fodder: PLANT_FODDER_FORECAST_NOT_YET_PROJECTED,
    }
}

/// Pre-commit yield forecast for foraging `patch` at this tile's `seasonal` weight (its
/// `FoodModuleTag::seasonal_weight`). Mirrors `forage_take` exactly: same resolved ecology
/// ([`patch_ecology`]), same per-policy ceilings, same seasonal-folded per-worker throughput, same
/// biomass clamp, same biomass→provisions conversion — so the client's
/// `min(workers × per_worker_yield, ceiling[policy])` IS the take the sim pays. The plant mirror of
/// `fauna::hunt_forecast`.
///
/// **Two shapes, one per rung-kind** (slice 7 — this is where the plant ladder stopped collapsing a
/// rung early):
/// - A **Field** (rung 3) is *yours*: it pays a managed rate whatever the policy, so it forecasts
///   through [`SourceYieldForecast::managed`] — every ceiling is that rate, and `per_worker_yield` is
///   the crew's real throughput, so `max_useful_workers` falls out as the honest
///   `ceil(production / per_worker)` rather than a hardcoded 1.
/// - A **wild or tended** patch (rungs 1–2) is a wild stand either way, so it takes the full
///   policy-live path below — the *same* code, differing only in the ecology `patch_ecology` hands
///   it. That is the whole rung-2 fix: a tended patch's Sustain/Surplus/Deplete/Eradicate are four
///   different numbers again, and it can be over-farmed.
#[allow(clippy::too_many_arguments)] // the patch, both configs, the ladder and two rates are inputs
pub(crate) fn forage_forecast(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    // **The crew's per-gatherer throughput for THIS turn, season already folded in**
    // (`forage_per_worker_biomass(resolved basket tier, seasonal)`). Taken pre-folded rather than as
    // the tier + the season, so this signature stays inside clippy's argument budget and there is
    // exactly one place the two multiply.
    per_worker_gather_biomass: f32,
    output_multiplier: f32,
    // **Which plants the crew this forecast is for carries home** — empty is the whole basket, and
    // every band-agnostic caller (a patch row on the wire) passes exactly that. A crew that named
    // some must be forecast against what it will actually take, or `forecast == actual` fails on
    // the very readout the selection exists to move.
    take_species: &TakeSelection,
) -> SourceYieldForecast {
    // **A Field takes the ORDINARY path.** It used to short-circuit into a managed, seasonless,
    // never-drawn-down harvest — the model this arc retired, because a rung may change production and
    // no rung changes the draw. So the forecast is one shape at every plant rung, which is also what
    // makes it interpolate.
    // The patch's IN-EFFECT conversion rate — the same one `forage_take` pays with, so every ceiling
    // the forecast composes is the number the sim will hand over.
    let rate =
        patch_provisions_per_biomass_taking(patch, tile_composition, flora, forage, take_species);
    // **THE SELECTION RIDES THE TWO STOCK TERMS, which is what keeps the ceiling one expression.**
    // `ceiling_at` is `max(0, B − floor·K) × rate`, and scaling both `B` and `K` by the selected
    // share scales that room by exactly the share — the same number `forage_take` multiplies its
    // ceiling by. `WHOLE_BASKET` leaves both terms untouched.
    let selected = selected_biomass_share(
        &patch_composition(patch, tile_composition, forage),
        take_species,
    );
    SourceYieldForecast {
        // A plant is not stalked — the engagement stage is an animal-web concept, and so is the fight
        // it feeds. Nothing on the plant web is brought down.
        engage_rate: f32::INFINITY,
        fight: None,
        per_worker_yield: plant_food_only(forage_provisions(
            per_worker_gather_biomass,
            rate,
            output_multiplier,
        )),
        body_mass_yield: PLANTS_DO_NOT_QUANTISE,
        // **The TERMS of the take** — `ceiling_at(floor, improvement)` composes exactly what
        // `forage_take` computes, at any floor the player's dial can name, on the share of the stand
        // the crew is here for.
        biomass: patch.biomass * selected,
        carrying_capacity: patch.carrying_capacity * selected,
        // What one unit of this patch's standing crop is worth, at its own basket rate. Food-only:
        // the plant web's trade/fodder PROJECTION is a known gap (`plant_food_only`), while the
        // trade a gather actually earns is reported on the resolved row.
        per_biomass_yield: plant_food_only(forage_provisions(
            crate::fauna::ONE_UNIT_OF_BIOMASS,
            rate,
            output_multiplier,
        )),
        // A wild or tended patch IS drawn down — it is a wild stand either way, which is what makes
        // rungs 1 and 2 floor-live and rung 3 (a Field) not.
        managed_production: None,
        // **Cultivate's "then Y"** — what this patch will pay once tended, on the tended curve. On a
        // patch that is *already* tended this is simply its own `ceiling_sustain`, which is the truth:
        // the rung is built, and the number is what it pays. (Sow's "then Y" is `field_provisions`,
        // exported beside this one as the wire's `fieldYield` — two rungs, two payoff quotes, never
        // one field doing both jobs.)
        managed_yield: plant_food_only(tended_provisions(
            patch,
            tile_composition,
            forage,
            flora,
            output_multiplier,
        )),
        // `Tame` is hunt-only — a patch has no pastoral rung — so it advertises no Tame payoff (the
        // plant twin of `ceiling_tame: 0`).
        pastoral_yield: NO_PASTORAL_YIELD,
        // **The plant web quotes no investment payoff in BIOMASS.** Rung 2's own harvest is
        // `rung_msy_take` and the crop picker prices its material quote on that directly
        // (`commit_material_payoff`), so nothing reads these here — and a patch offers no `Tame`
        // rung at all. Stated as the "no such rung" zero rather than a measurement.
        managed_yield_biomass: crate::fauna::NO_INVESTMENT_RUNG_BIOMASS,
        pastoral_yield_biomass: crate::fauna::NO_INVESTMENT_RUNG_BIOMASS,
    }
}

// **RETIRED: `managed_per_worker_yield`** — what one worker could carry home from a Field, the
// collection cap on its managed harvest. A Field is worker-capped through the ordinary
// `forage_take` path now, at the ordinary per-worker throughput, so there is nothing separate to cap.

/// **The negligible-take floor (in PROVISIONS) that ends a `realized` forward projection.** Below
/// this a patch is treated as *spent* — stripped to nothing — so the loop stops and the average
/// divides only by the turns that actually delivered.
///
/// **Provisions-space, which is why it is not [`crate::fauna::REALIZED_PROJECTION_TAKE_EPSILON`]**:
/// the animal twin breaks on a *biomass* take, while both branches here are already converted
/// (`field_provisions`, `forage_take`), so the two thresholds justify their magnitudes on different
/// scales and each gets its own constant rather than sharing one whose doc only covers biomass.
///
/// The magnitude is deliberately far below any live patch's one-turn gather: the smallest is a wild
/// Sustain skim, `r·K/4 × provisions_per_biomass` — ~0.61 provisions on the measured K=195
/// AlluvialPlain stand (see `labor_config.json` → `cultivation`), and a Field pays several times
/// that. Four orders of magnitude of headroom, so a healthy patch never trips it and a dead one
/// always does.
const REALIZED_PROJECTION_PROVISIONS_EPSILON: f32 = 1e-4;

/// **The steady `realized` yield for a forage source — a FORWARD PROJECTION** (the plant twin of
/// `fauna::project_realized_hunt`). The average food/turn the patch delivers over the next `horizon`
/// turns, simulated forward from its CURRENT state under `policy` + `workers`, mirroring the real turn
/// order (Logistics regrow → Population take). A **pure function of the passed patch state**, so the
/// assign-time seed and the resolved row compute the identical number (exact forecast == actual).
///
/// Foraging was never lumpy — `forage_take` is already rate-based (no kill-credit bank) — so the
/// projection just reuses the *same* take path the real turn runs each simulated turn: a **Field**
/// (rung 3) pays its managed `field_provisions` capped by the crew's throughput and never draws down;
/// every other patch pays `forage_take`'s drawn-down policy gather. So the projection is exactly the
/// forward average of what the source really pays, computed through one shared take path.
// The projection needs the full take context (source, config, ladder, season, multiplier, crew,
// policy, horizon) — the same shape `forage_source_yield_preview` already carries.
#[allow(clippy::too_many_arguments)]
pub fn project_realized_forage(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    // **What this crew carries home** — threaded so a projection runs the same take the turn will.
    take_species: &TakeSelection,
    horizon: u32,
) -> f32 {
    if horizon == 0 {
        return 0.0; // `LaborConfig::validate` pins `horizon > 0`; belt-and-braces against /0.
    }
    let mut sim = patch.clone();
    let mut total = 0.0_f32;
    // Turns actually simulated — the average divides by this, not the full `horizon`, so a
    // self-terminating gather (an Eradicate strip) reads the rate it delivers while the stand lasts
    // rather than being diluted by empty turns (the animal twin's rule). A patch reseeds, so in
    // practice it rarely trips the break — but the rule is uniform with `project_realized_hunt`.
    let mut turns = 0u32;
    for _ in 0..horizon {
        // Logistics: the patch regrows first, exactly as `advance_forage_regrowth` runs before the
        // Population stage's gather.
        regrow_patch(&mut sim, forage);
        // Population: **every** plant rung is the drawn-down policy gather through the shared
        // `forage_take` path — a Field included, since this arc retired its managed branch.
        let take = {
            forage_take(
                &mut sim,
                tile_composition,
                workers,
                floor,
                take_species,
                forage,
                flora,
                output_multiplier,
                per_worker_biomass_capacity,
                seasonal,
            )
            .to_f32()
        };
        if take <= REALIZED_PROJECTION_PROVISIONS_EPSILON {
            break; // the stand is spent — stop before diluting the average with empty turns.
        }
        total += take;
        turns += 1;
    }
    if turns > 0 {
        total / turns as f32
    } else {
        0.0
    }
}

/// **WHEN the food lands for a forage source** (the plant twin of `fauna::project_arrivals_hunt`) —
/// the discrete sibling of [`project_realized_forage`], run over the same forward simulation and
/// recording what is delivered on each projected turn. Returns exactly `horizon` entries: **index `i`
/// is the food delivered `i + 1` turns from now**.
///
/// **A gather is continuous, so a healthy patch is positive in EVERY slot** — and that is the correct
/// reading, not a degenerate one: `forage_take` has no kill-credit bank to quantise it, so the plant
/// web's schedule is a solid run where the animal web's is a pulse. The pair still exists for the
/// plant side because the *client* composes one larder projection out of every source's schedule, and
/// a continuous source has to contribute its own turns rather than be special-cased there.
///
/// Simulated on a private clone through the same take path the real turn runs, so the schedule is
/// what the sim will really pay. Unlike its animal twin there is no early completion test: a stripped
/// stand reseeds and regrows, so its remaining slots are genuinely small-but-positive rather than
/// "gone", and a truly dead source simply fills the schedule with zeros.
// Same shape as its `realized` sibling — the projection needs the full take context.
#[allow(clippy::too_many_arguments)]
pub fn project_arrivals_forage(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    // **What this crew carries home** — see [`project_realized_forage`].
    take_species: &TakeSelection,
    horizon: u32,
) -> Vec<f32> {
    // `LaborConfig::validate` pins `horizon > 0`; a zero horizon yields an empty schedule, which the
    // client reads as "no data" exactly like an unprojected row.
    let mut schedule = vec![0.0_f32; horizon as usize];
    let mut sim = patch.clone();
    for slot in schedule.iter_mut() {
        // Logistics: the patch regrows first, exactly as `advance_forage_regrowth` runs before the
        // Population stage's gather.
        regrow_patch(&mut sim, forage);
        // Population: the same one path `project_realized_forage` and the real Forage arm both take —
        // the drawn-down policy gather through the shared `forage_take`, at every plant rung.
        *slot = {
            forage_take(
                &mut sim,
                tile_composition,
                workers,
                floor,
                take_species,
                forage,
                flora,
                output_multiplier,
                per_worker_biomass_capacity,
                seasonal,
            )
            .to_f32()
        };
    }
    schedule
}

/// The assign-time yield telemetry seed for a **Forage** source: what staffing `patch` with `workers`
/// gatherers under `policy` will pay next turn, in the same shape the Forage arm of
/// `advance_labor_allocation` records after the take. Reuses `forage_forecast` (hence `forage_take`'s
/// own ceiling/conversion helpers) and the shared MSY `sustainable_yield`, so the seed is exactly the
/// number the turn then produces — no jump. The animal mirror is `fauna::hunt_source_yield_preview`.
// The seed composes the whole telemetry row, so it carries the full take context (see the sibling
// `project_realized_forage`).
#[allow(clippy::too_many_arguments)]
pub fn forage_source_yield_preview(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    // **What this crew carries home** — empty is the whole basket. The seed must price the selection
    // the command just stored, or the row a player reads before committing is not the row the turn
    // pays (the `forecast == actual` rule, one axis over from the floor).
    take_species: &TakeSelection,
    realized_horizon: u32,
    arrivals_horizon: u32,
    // `combat_config.forecast_range_sigmas`. **The plant web has no stochastic stage** — no
    // engagement, no retreat, no fight — so its band is always a point whatever this says; it is
    // threaded so both webs seed their row through the one `fauna::forecast_source_yield`.
    range_sigmas: f32,
) -> SourceYield {
    let forecast = forage_forecast(
        patch,
        tile_composition,
        forage,
        flora,
        forage_per_worker_biomass(per_worker_biomass_capacity, seasonal),
        output_multiplier,
        take_species,
    );
    // The patch's OWN MSY (`patch_ecology`) — a tended patch's sustainable line sits on its boosted
    // curve, so a Sustain gather of it reads no ⚠ while a Surplus gather of it does. Reading
    // `forage.ecology` here would flag every tended Sustain as an overdraw.
    //
    // **The reference line narrows with the take it sits beside.** A crew gathering one species of a
    // mixed stand is comparing what it took against what *that* stand sustains, so both the stock
    // terms and the conversion read the selected subset — the same scaling the ceiling gets.
    let selected = selected_biomass_share(
        &patch_composition(patch, tile_composition, forage),
        take_species,
    );
    let sustainable = forage_provisions(
        sustainable_yield(
            patch.biomass * selected,
            patch.carrying_capacity * selected,
            &patch_ecology(patch, forage),
        ),
        patch_provisions_per_biomass_taking(patch, tile_composition, flora, forage, take_species),
        output_multiplier,
    );
    // The steady headline is the forward projection from THIS patch state — the same computation the
    // resolved Forage arm runs, so seed == first resolved value exactly.
    let realized = project_realized_forage(
        patch,
        tile_composition,
        forage,
        flora,
        per_worker_biomass_capacity,
        seasonal,
        output_multiplier,
        workers,
        floor,
        take_species,
        realized_horizon,
    );
    // The discrete twin, from the same patch state: what lands on each of the next
    // `arrivals_horizon` turns. A gather is continuous, so this is normally positive throughout.
    let arrivals = project_arrivals_forage(
        patch,
        tile_composition,
        forage,
        flora,
        per_worker_biomass_capacity,
        seasonal,
        output_multiplier,
        workers,
        floor,
        take_species,
        arrivals_horizon,
    );
    // **`managed` is rung 3 ONLY** (slice 7). It marks the sources whose harvest cannot overdraw —
    // and since rung 2 went back to being a drawn-down wild stand, a *tended* patch can be over-farmed
    // like any other, so it must keep its real sustainable line and its real ⚠.
    forecast_source_yield(
        &forecast,
        sustainable,
        patch.is_field(),
        workers,
        floor,
        realized,
        arrivals,
        // The ⚠, off this patch's own curve and this crew's carry — see [`forage_take_overdraws`].
        forage_take_overdraws(
            patch,
            forage,
            patch.biomass * selected,
            patch.carrying_capacity * selected,
            workers as f32 * forage_per_worker_biomass(per_worker_biomass_capacity, seasonal),
            floor,
        ),
        range_sigmas,
    )
}

/// TEMPORARY measurement harness (`#[ignore]`d) for the harvest-stance design review — it drives the
/// shipped take/regrowth/build functions of **both** webs forward and prints the tables. Lives here
/// because the plant half needs this module's private `regrow_patch`. Delete with the review.
#[cfg(test)]
mod stance_probe;

#[cfg(test)]
mod tests {
    /// **The shipped EQUIPPED gather rate** — what a kitted crew carries, off the baskets' own
    /// tier. `labor_config`'s `forage.per_worker_biomass_capacity` is the *bare-handed* baseline
    /// since quality tiers landed, so a fixture that wants "an ordinary band" asks the item table.
    fn equipped_gather_rate() -> f32 {
        crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
            crate::equipment_config::EquipmentStat::ForageCarry,
            crate::labor_config::LaborConfig::builtin()
                .forage
                .per_worker_biomass_capacity,
        )
    }

    use super::*;
    use crate::intensification::RUNG_COST_UNSCALED;
    use crate::labor_config::LaborConfig;
    use sim_runtime::TerrainType;

    /// The **shipped** forage config (the per-biome capacity table lives only in the JSON — the
    /// struct default is deliberately empty, so `ForageLaborConfig::default()` would read every
    /// biome as barren). Mirrors `graze::tests::test_graze_config`.
    fn test_forage_config() -> ForageLaborConfig {
        LaborConfig::builtin().forage.clone()
    }

    /// The biome the patch-mechanics tests stand their patch on. Any positive-capacity biome works
    /// (the mechanics are cap-relative); `AlluvialPlain` is the richest common human ground and the
    /// one a `RiverineDelta` food module actually sits on.
    const TEST_BIOME: TerrainType = TerrainType::AlluvialPlain;

    /// **[`TEST_BIOME`] IS THE REFERENCE TILE**, and every upkeep assertion below leans on it: its
    /// own `K` (195) is exactly `forage.cultivation.capacity_per_tender`, so a patch standing on it
    /// presents [`ONE_TENDER_LOAD`] and owes precisely the rate its rung declares. A test that wants
    /// the *scaling* to show has to stand somewhere else — see
    /// [`a_rich_patch_costs_more_to_hold_than_a_thin_one`].
    fn test_tile_capacity() -> f32 {
        test_forage_config().capacity_for(TEST_BIOME)
    }

    /// **The basket of a patch standing on no tile at all** — empty, which is exactly what these
    /// mechanics tests want: they exercise biomass/regrowth/policy, not composition, and an empty
    /// basket makes every rate fall back to `forage.provisions_per_biomass`, the number the
    /// pre-basket assertions were written against. The basket's *own* behaviour is pinned in
    /// `tests/forage_basket_reweight.rs`.
    const NO_BASKET: &[FloraShare] = &[];

    /// A navigable river keeps the valley it cut: it stays mechanically `NavigableRiver`, but its
    /// RESOURCE reads route through the preserved underlying biome (`resource_terrain`), and it is
    /// always a fishery (forage gets the river bonus on top of the underlying; graze gets the plain
    /// underlying value — you don't pasture on the channel).
    #[test]
    fn navigable_hex_reads_underlying_biome_plus_river_forage_bonus() {
        use crate::fauna_config::FaunaConfig;
        use sim_runtime::{TerrainTags, TerrainType};

        let forage = test_forage_config();
        let graze = FaunaConfig::builtin().graze.clone();

        // A navigable hex cut through fertile grassland: mechanically water, underlying preserved.
        let underlying = TerrainType::PrairieSteppe;
        let navigable = Tile {
            terrain: TerrainType::NavigableRiver,
            terrain_tags: TerrainTags::WATER | TerrainTags::FRESHWATER,
            underlying_terrain: Some(underlying),
            ..Default::default()
        };

        // Terrain stays NavigableRiver (movement/naval unchanged); resources read the valley.
        assert_eq!(navigable.terrain, TerrainType::NavigableRiver);
        assert_eq!(navigable.resource_terrain(), underlying);

        // Forage = underlying + river fishing bonus (the seeded patch cap, via the SHARED helper).
        let expected_forage = forage.capacity_for(underlying) + forage.navigable_river_forage_bonus;
        assert_eq!(tile_forage_capacity(&forage, &navigable), expected_forage);
        assert!(expected_forage > forage.capacity_for(underlying)); // strictly richer than dry land

        // Graze = the underlying biome's pasture, no bonus.
        assert_eq!(
            graze.capacity_for(navigable.resource_terrain()),
            graze.capacity_for(underlying)
        );
        assert!(graze.capacity_for(navigable.resource_terrain()) > 0.0); // grassland grazes

        // Even over an otherwise-barren biome (no human food), a navigable hex STILL seeds a patch —
        // a navigable river is always a fishery — at just the bonus.
        let barren = TerrainType::Glacier;
        assert_eq!(forage.capacity_for(barren), NO_FORAGE_CAPACITY);
        let navigable_over_barren = Tile {
            terrain: TerrainType::NavigableRiver,
            underlying_terrain: Some(barren),
            ..Default::default()
        };
        assert_eq!(
            tile_forage_capacity(&forage, &navigable_over_barren),
            forage.navigable_river_forage_bonus
        );
        assert!(tile_forage_capacity(&forage, &navigable_over_barren) > NO_FORAGE_CAPACITY);
    }

    #[test]
    fn sustain_on_full_patch_yields_msy_and_draws_to_half_cap() {
        // Regression (Phase 0 bug): a patch AT carrying capacity used to yield 0 under Sustain
        // (logistic regrowth is 0 at K), so a full patch stayed stuck at 0 forever. The MSY-based
        // `sustainable_yield` ceiling skims regrowth at the most-productive biomass (K/2), so a
        // full patch yields a positive harvest and Sustain draws it DOWN toward K/2 and holds.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let half_cap = cap * 0.5;
        let msy = sustainable_yield(cap, cap, &forage.ecology);
        assert!(
            msy > 0.0,
            "a full patch must be sustainably harvestable: {msy}"
        );

        // Seed FULL, exactly as real forage patches spawn.
        let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
        patch.biomass = cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // First Sustain gather off the full patch: **the standing surplus above `K/2`**, capped by
        // what the crew can carry. Under constant escapement the first harvest of an untouched patch
        // is the accumulated stock, not a rate — the crew empties the store the patch built up before
        // anyone worked it, and lands it exactly on its most productive biomass.
        let biomass_before = patch.biomass;
        let crew_cap = 20.0 * forage_per_worker_biomass(equipped_gather_rate(), 1.0);
        let expected_first = crew_cap.min(biomass_before - half_cap);
        let provisions = forage_take(
            &mut patch,
            NO_BASKET,
            20,
            0.5,
            &TakeSelection::EVERYTHING,
            &forage,
            &FloraConfig::builtin(),
            1.0,
            equipped_gather_rate(),
            1.0,
        );
        let take = biomass_before - patch.biomass;
        assert!(
            take > 0.0,
            "a full patch under Sustain must yield > 0: {take}"
        );
        assert!(
            (take - expected_first).abs() < 1e-3,
            "the first gather is the escapement surplus (crew permitting): {take} vs {expected_first}"
        );
        let actual = provisions.to_f32();
        assert!(
            (actual - take * forage.provisions_per_biomass).abs() < 1e-3,
            "the provisions paid are the take through the patch's own rate: {actual}"
        );
        assert!(
            patch.biomass >= half_cap - 1e-3,
            "Sustain never draws a patch below its escapement floor: {} vs {half_cap}",
            patch.biomass
        );

        // Over many take+regrowth turns Sustain draws the patch DOWN from full and then HOLDS: the
        // post-take biomass settles at the MSY point (K/2), so the stored biomass stabilizes just
        // above K/2 and the per-turn yield stays ≈ MSY (never falling back to 0).
        let mut prev = patch.biomass;
        let mut last_take = take;
        for turn in 0..200 {
            let before = patch.biomass;
            let _ = forage_take(
                &mut patch,
                NO_BASKET,
                20,
                0.5,
                &TakeSelection::EVERYTHING,
                &forage,
                &FloraConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            last_take = before - patch.biomass;
            regrow_patch(&mut patch, &forage);
            if turn >= 190 {
                assert!(
                    (patch.biomass - prev).abs() < 1.0,
                    "late turns: biomass has stabilized: {} vs {}",
                    patch.biomass,
                    prev
                );
            }
            prev = patch.biomass;
        }
        assert!(
            patch.biomass < cap,
            "Sustain drew the full patch down: {}",
            patch.biomass
        );
        assert!(
            patch.biomass > half_cap,
            "Sustain holds at/above the MSY point K/2: {} vs {}",
            patch.biomass,
            half_cap
        );
        assert!(
            (last_take - msy).abs() < 1e-3 && last_take > 0.0,
            "steady-state yield stays ≈ MSY: {last_take} vs {msy}"
        );
    }

    #[test]
    fn heavy_take_depletes_patch_and_drops_phase() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(2, 3), cap);
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // A heavier-than-sustainable draw (non-Sustain ceiling = throughput only) with enough
        // workers to out-pace regrowth drives biomass DOWN turn over turn and drops the phase.
        let mut last = patch.biomass;
        let mut saw_stressed = false;
        for _ in 0..40 {
            let _ = forage_take(
                &mut patch,
                NO_BASKET,
                3,
                0.0,
                &TakeSelection::EVERYTHING,
                &forage,
                &FloraConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            regrow_patch(&mut patch, &forage);
            assert!(patch.biomass < last + 1e-3, "biomass must trend downward");
            last = patch.biomass;
            if patch.ecology_phase == EcologyPhase::Stressed {
                saw_stressed = true;
            }
        }
        assert!(
            saw_stressed,
            "phase should pass through Stressed while depleting"
        );
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);
        assert!(patch.biomass < forage.ecology.collapse_fraction * cap);
    }

    /// The forage policy axis (parity with hunting): on an identical Thriving patch with ample
    /// workers (so the take is ceiling-bound, not throughput-bound), a **deeper floor** takes more —
    /// `Sustain ≤ Surplus < Deplete < Eradicate` — and the deeper floors deplete the patch faster
    /// (biomass drops more in a single turn).
    #[test]
    fn policy_ceilings_order_take_and_depletion() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let start = 0.8 * cap; // Thriving, clear positive net regrowth.
        let workers = 20; // worker_cap (20 × per_worker) far exceeds every policy ceiling.

        // One-turn take under each policy from the same starting biomass.
        let take_under = |policy: f32| -> (f32, f32) {
            let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
            patch.biomass = start;
            let provisions = forage_take(
                &mut patch,
                NO_BASKET,
                workers,
                policy,
                &TakeSelection::EVERYTHING,
                &forage,
                &FloraConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            let take = start - patch.biomass;
            (take, provisions.to_f32())
        };

        let (sustain_take, _) = take_under(0.5);
        let (surplus_take, _) = take_under(0.3);
        let (deplete_take, _) = take_under(0.15);
        let (eradicate_take, _) = take_under(0.0);

        // Sustain is the regrowth skim; Surplus overdraws it; Deplete/Eradicate strip a share.
        assert!(sustain_take <= surplus_take + 1e-4, "Sustain ≤ Surplus");
        assert!(surplus_take < deplete_take, "Surplus < Deplete");
        assert!(deplete_take < eradicate_take, "Deplete < Eradicate");
        // Heavier policies deplete the patch faster (more biomass removed this turn).
        assert!(
            deplete_take > sustain_take,
            "Deplete depletes faster than Sustain"
        );
        assert!(
            eradicate_take > sustain_take,
            "Eradicate depletes faster than Sustain"
        );
        // Sustain takes exactly the stock standing above its escapement floor — `B − K/2` at
        // `B = 0.8·K` — and so leaves the patch **on** its most productive biomass, never below it.
        let expected_sustain = start - cap * crate::fauna::MSY_BIOMASS_FRACTION;
        assert!(
            (sustain_take - expected_sustain).abs() < 1e-3,
            "Sustain takes `B - K/2`: {sustain_take} vs {expected_sustain}"
        );
    }

    #[test]
    fn below_cap_patch_regrows_toward_cap() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(0, 0), cap);
        patch.biomass = 0.25 * cap;
        patch.refresh_ecology_phase(&forage.ecology);

        let mut prev = patch.biomass;
        for _ in 0..30 {
            regrow_patch(&mut patch, &forage);
            assert!(patch.biomass >= prev, "regrowth must be monotonic upward");
            prev = patch.biomass;
        }
        // Converges toward the cap.
        assert!(patch.biomass > 0.9 * cap);
        assert!(patch.biomass <= cap);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }

    #[test]
    fn crashed_patch_recovers_no_extinction() {
        // Pure-logistic regrowth: a patch driven far below the Allee threshold still recovers
        // (plants have no critical-depensation crash / extinction floor).
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(4, 4), cap);
        patch.biomass = 0.02 * cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        for _ in 0..80 {
            regrow_patch(&mut patch, &forage);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn zero_biomass_patch_reseeds_and_recovers() {
        // Regression: a patch driven to *exactly* 0 (repeated Eradicate + f32 underflow,
        // `take_fraction = 1.0`, or a snapshot restore carrying biomass = 0) used to be stuck at 0
        // forever, because `logistic_regrowth(0, ..) == 0`. The reseed floor lifts a depleted patch
        // to a small standing crop each turn, so it recovers via normal regrowth — the "a feral
        // patch always recovers" invariant is now backed by code, not just the docstring.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let floor = forage.reseed_floor_fraction * cap;
        assert!(floor > 0.0, "reseed floor must be a positive standing crop");

        let mut patch = ForagePatch::new(UVec2::new(5, 5), cap);
        patch.biomass = 0.0;
        patch.refresh_ecology_phase(&forage.ecology);

        // One turn off dead-zero: reseeded to the floor and already regrowing above it (> 0).
        regrow_patch(&mut patch, &forage);
        assert!(
            patch.biomass > 0.0,
            "a 0-biomass patch must escape 0 via the reseed floor: {}",
            patch.biomass
        );
        assert!(patch.biomass >= floor);

        // Over subsequent turns it recovers toward a healthy level (Thriving), just like a patch
        // seeded a hair above 0 — no permanent stall at 0.
        for _ in 0..80 {
            regrow_patch(&mut patch, &forage);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn continuous_eradicate_bottoms_at_floor_then_recovers() {
        // The floor is small enough that Eradicate still crashes the patch hard (into Collapsing),
        // but it can't drive it *permanently* to 0: the patch bottoms out at ~the reseed floor and
        // recovers once Eradicate stops.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let floor = forage.reseed_floor_fraction * cap;
        let mut patch = ForagePatch::new(UVec2::new(6, 6), cap);
        patch.refresh_ecology_phase(&forage.ecology);

        // Hammer with Eradicate + regrowth: biomass crashes but never sits at 0 — it floats at/above
        // the reseed floor while still reading Collapsing (a hard crash, not extinction).
        for _ in 0..60 {
            let _ = forage_take(
                &mut patch,
                NO_BASKET,
                50,
                0.0,
                &TakeSelection::EVERYTHING,
                &forage,
                &FloraConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            regrow_patch(&mut patch, &forage);
            assert!(
                patch.biomass > 0.0,
                "Eradicate must not permanently zero a patch"
            );
        }
        assert!(
            patch.biomass < cap * forage.ecology.collapse_fraction,
            "Eradicate still crashes the patch hard: {} vs {}",
            patch.biomass,
            cap * forage.ecology.collapse_fraction
        );
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        // Stop hunting: from the crashed floor the patch recovers all the way back to Thriving.
        for _ in 0..120 {
            regrow_patch(&mut patch, &forage);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass >= floor);
    }

    #[test]
    fn reseed_floor_leaves_healthy_patch_regrowth_unchanged() {
        // A patch above the floor must regrow identically with or without the reseed lift (the floor
        // only reseeds depleted patches — a healthy patch is untouched).
        let forage = test_forage_config();
        // The "no reseed" baseline — the shipped config with only the lift switched off.
        let no_floor_forage = ForageLaborConfig {
            reseed_floor_fraction: 0.0,
            ..forage.clone()
        };
        let cap = forage.capacity_for(TEST_BIOME);
        let start = 0.5 * cap; // comfortably above reseed_floor_fraction × cap.

        let mut with_floor = ForagePatch::new(UVec2::new(7, 7), cap);
        with_floor.biomass = start;
        let mut without_floor = ForagePatch::new(UVec2::new(8, 8), cap);
        without_floor.biomass = start;

        for _ in 0..30 {
            regrow_patch(&mut with_floor, &forage);
            // A zero floor is the "no reseed" baseline.
            regrow_patch(&mut without_floor, &no_floor_forage);
        }
        assert!(
            (with_floor.biomass - without_floor.biomass).abs() < 1e-6,
            "reseed floor must not perturb a healthy patch's regrowth: {} vs {}",
            with_floor.biomass,
            without_floor.biomass
        );
    }

    #[test]
    fn sustainable_yield_is_zero_below_allee() {
        // A collapsing (sub-Allee) patch is not sustainably harvestable.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let below_allee = forage.ecology.collapse_fraction * cap * 0.5;
        assert_eq!(
            sustainable_yield(below_allee, cap, &forage.ecology),
            0.0,
            "a collapsing patch has no sustainable yield"
        );
    }

    #[test]
    fn sustainable_yield_plateaus_at_msy_above_half_cap() {
        // For any healthy biomass (>= K/2) the MSY ceiling is flat at the K/2 peak.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let msy = sustainable_yield(cap * 0.5, cap, &forage.ecology);
        assert!(msy > 0.0);
        for frac in [0.5_f32, 0.6, 0.75, 0.9, 1.0] {
            assert!(
                (sustainable_yield(cap * frac, cap, &forage.ecology) - msy).abs() < 1e-6,
                "flat MSY plateau at biomass = {frac}·K"
            );
        }
    }

    /// The Cultivate job every patch-level test below is priced against — the shipped `plant:tended`
    /// cost, so the arithmetic reads in the units the config states. Read off the ladder rather than
    /// transcribed: a retune must move these fixtures with it, not silently invalidate them.
    fn cultivate_cost(ladder: &LadderConfig) -> f32 {
        plant_rung_span(RungKey::PlantTended, ladder).1
    }

    /// The Sow job's own span, one rung up.
    fn field_cost(ladder: &LadderConfig) -> f32 {
        plant_rung_span(RungKey::PlantField, ladder).1
    }

    #[test]
    fn cultivation_accrual_is_owner_locked_and_clamped() {
        let ladder = LadderConfig::builtin();
        let cost = cultivate_cost(&ladder);
        let mut patch = ForagePatch::new(UVec2::new(1, 1), 120.0);
        // First accrual claims ownership for the acting faction.
        patch.accrue_cultivation(FactionId(0), 15.0, &ladder);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.ladder_position() - 15.0).abs() < 1e-6);
        // A different faction cannot accrue on an already-owned patch.
        patch.accrue_cultivation(FactionId(1), 25.0, &ladder);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.ladder_position() - 15.0).abs() < 1e-6);
        // Owner accrues; the position clamps at the RUNG'S OWN TOP and latches cultivated.
        patch.accrue_cultivation(FactionId(0), 45.0, &ladder);
        assert!(patch.is_cultivated());
        assert_eq!(patch.ladder_position(), cost);
        // **THE CAP IS THE RUNG, NOT THE PATCH.** A crew that out-produces what is left of the
        // tended rung banks nothing past it — a Cultivate's work may never implicitly finish the
        // Field above it, or the player would be doing Sow's work with Cultivate's tool
        // (`docs/plan_standing_upkeep.md` §2.8, rule 1).
        patch.accrue_cultivation(FactionId(0), cost, &ladder);
        assert_eq!(patch.ladder_position(), cost);
        assert!(patch.is_cultivated());
        assert!(!patch.is_field());
    }

    /// **An unstarted patch is not a finished one.** A wild stand sits at position zero, holding its
    /// branch's wild rung and raising the tended one at no credit.
    #[test]
    fn a_wild_patch_is_not_cultivated_even_though_its_position_reads_zero() {
        let patch = ForagePatch::new(UVec2::new(7, 7), 120.0);
        assert_eq!(patch.ladder_position(), RUNG_UNSTARTED);
        assert_eq!(patch.standing().held, RungKey::PlantWild);
        assert_eq!(patch.standing().raising, Some(RungKey::PlantTended));
        assert!(!patch.is_cultivated());
        assert!(!patch.is_field());
        assert!(!patch.is_managed());
    }

    /// **A FIELD AT 40% PAYS A WHOLE TENDED PATCH PLUS 40% OF THE FIELD'S EXTRA**
    /// (`docs/plan_standing_upkeep.md` §2.8) — the delta form, asserted against the **payout seam
    /// itself** at three standings rather than against a re-derivation of its arithmetic.
    ///
    /// That rule is `flora.md`'s own (*"assert a published quote against the payoff functions, never
    /// against a re-derivation of their arithmetic"*), and it is why the two endpoints are read back
    /// out of `patch_provisions_per_biomass` at completed rungs instead of being composed from
    /// `tended_conversion_gain` and `field_provisions_per_biomass` by hand: a retune moves all three
    /// readings together and the test goes on describing the model.
    #[test]
    fn a_field_part_way_up_pays_the_tended_patch_in_full_plus_its_share_of_the_step() {
        /// How far up the Field's own span the patch under test stands.
        const PART_WAY_UP: f32 = 0.4;
        /// The tile's crop — a `field`-ceiling staple, so both rungs are legal on it.
        const CROP: &str = "wild_emmer";

        let forage = test_forage_config();
        let flora = FloraConfig::builtin();
        let ladder = LadderConfig::builtin();
        let cap = forage.capacity_for(TEST_BIOME);
        let composition = flora.composition(TEST_BIOME).to_vec();

        let rate_of = |patch: &ForagePatch| -> f32 {
            patch_provisions_per_biomass(patch, &composition, &flora, &forage)
        };
        let committed = |position: f32| -> ForagePatch {
            let mut patch = ForagePatch::new(UVec2::new(0, 0), cap);
            patch.species = Some(CROP.to_string());
            patch.set_ladder_position(position, &ladder);
            patch
        };

        let (tended_base, tended_width) = plant_rung_span(RungKey::PlantTended, &ladder);
        let (field_base, field_width) = plant_rung_span(RungKey::PlantField, &ladder);
        let tended = rate_of(&committed(tended_base + tended_width));
        let field = rate_of(&committed(field_base + field_width));
        assert!(
            field > tended,
            "fixture: the ladder must climb, or there is no step to take a share of"
        );

        let part_way = rate_of(&committed(field_base + field_width * PART_WAY_UP));
        let expected = tended + PART_WAY_UP * (field - tended);
        assert!(
            (part_way - expected).abs() < 1e-4,
            "a Field {PART_WAY_UP} raised pays {expected}, not {part_way}"
        );
        // **The pair that makes it a slope rather than a step**: strictly between, at both ends.
        assert!(
            part_way > tended && part_way < field,
            "…and it is strictly between the two rungs: {tended} < {part_way} < {field}"
        );
    }

    /// **ONE WORK UNIT BANKED ALREADY PAYS MORE THAN WILD** — the discontinuity §4.10 exists to
    /// remove, stated at the smallest step there is: a build used to pay *nothing* for its whole
    /// span and then everything at once on the turn it completed.
    ///
    /// **The PAIR is what carries this, and so is the precondition.** *"More than wild"* alone
    /// passes for a rate that stepped straight to the tended value on the first work unit, and both
    /// halves pass vacuously if the two reference rates happen to coincide — so the fixture asserts
    /// that they differ before comparing anything against them.
    #[test]
    fn a_single_work_unit_pays_above_wild_and_below_tended() {
        /// One worker-turn on a fifty-unit rung — the smallest step the model has.
        const ONE_WORK_UNIT: f32 = 1.0;
        const CROP: &str = "wild_emmer";

        let forage = test_forage_config();
        let flora = FloraConfig::builtin();
        let ladder = LadderConfig::builtin();
        let cap = forage.capacity_for(TEST_BIOME);
        let composition = flora.composition(TEST_BIOME).to_vec();

        let rate_of = |patch: &ForagePatch| -> f32 {
            patch_provisions_per_biomass(patch, &composition, &flora, &forage)
        };
        let committed = |position: f32| -> ForagePatch {
            let mut patch = ForagePatch::new(UVec2::new(2, 2), cap);
            patch.species = Some(CROP.to_string());
            patch.set_ladder_position(position, &ladder);
            patch
        };

        // **The wild reference is an UNCOMMITTED patch** — a wild stand is the whole mixed basket,
        // and committing is what the rung buys.
        let wild = rate_of(&ForagePatch::new(UVec2::new(2, 2), cap));
        let (_, tended_width) = plant_rung_span(RungKey::PlantTended, &ladder);
        let tended = rate_of(&committed(tended_width));
        assert!(
            tended > wild,
            "PRECONDITION: the two reference rates must differ, or both halves below pass by \
             collapsing onto one number ({wild} against {tended})"
        );

        let barely = rate_of(&committed(ONE_WORK_UNIT));
        assert!(
            barely > wild,
            "one work unit already pays above wild — the payoff starts on turn one ({barely} \
             against {wild})"
        );
        assert!(
            barely < tended,
            "…and well below tended, or the rate STEPPED instead of sliding ({barely} against \
             {tended})"
        );
    }

    /// **THE COST MOVES WITH THE BENEFIT** (`docs/plan_standing_upkeep.md` §2.8) — the upkeep demand
    /// interpolates on exactly the standing the payout does, so a Field half-raised owes a whole
    /// tended patch plus half of what a Field adds.
    ///
    /// Per tender-load that is `2.0 + 0.5 × (4.0 − 2.0) = 3.0`, and the **delta** form is what is
    /// asserted: a flat fraction of the Field's own rate would answer `2.0`, which is the reading
    /// §2.8's callout exists to rule out.
    ///
    /// # AND THE SCALE MEASURE APPLIES **ONCE**, ACROSS THE INTERPOLATION
    ///
    /// It is deliberately run on a **scaled** tile rather than the reference one, so `loads ≠ 1`
    /// and every wrong place to apply the factor gives a different number: once per endpoint would
    /// square it, once on the delta alone would leave the tended term unscaled. The measure rides
    /// inside each endpoint's own `upkeep_demand` and both endpoints carry the same load, so the
    /// interpolation is linear in it and the answer is `loads × (tended + f × (field − tended))`.
    #[test]
    fn a_half_raised_field_owes_the_tended_rate_in_full_plus_half_the_step() {
        const HALF_WAY_UP: f32 = 0.5;
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let (field_base, field_width) = plant_rung_span(RungKey::PlantField, &ladder);
        let tended = ladder
            .rung(RungKey::PlantTended)
            .upkeep_demand(ONE_TENDER_LOAD);
        let field = ladder
            .rung(RungKey::PlantField)
            .upkeep_demand(ONE_TENDER_LOAD);
        assert!(
            field > tended,
            "fixture: the demands must climb, or the delta is zero and the form is untestable"
        );

        // A tile deliberately NOT the reference one, so the measure is visible in the answer.
        let tile_capacity = forage.capacity_for(SCALED_TILE_BIOME);
        let loads = patch_tender_loads(tile_capacity, &forage);
        assert!(
            (loads - ONE_TENDER_LOAD).abs() > 1e-3,
            "fixture: the tile must NOT read one load, or applying the factor twice would be \
             invisible ({loads})"
        );

        let mut patch = ForagePatch::new(UVec2::new(4, 4), tile_capacity);
        patch.set_ladder_position(field_base + field_width * HALF_WAY_UP, &ladder);
        let owed = patch_upkeep_demand(&patch, &ladder, tile_capacity, &forage);
        let expected = loads * (tended + HALF_WAY_UP * (field - tended));
        assert!(
            (owed - expected).abs() < 1e-4,
            "a half-raised Field on {loads} tender-loads owes {expected}, not {owed}"
        );
        assert!(
            (owed - loads * loads * (tended + HALF_WAY_UP * (field - tended))).abs() > 1e-3,
            "…and the measure is applied ONCE, not once per endpoint — {owed} against the squared \
             reading {}",
            loads * loads * (tended + HALF_WAY_UP * (field - tended))
        );
        assert!(
            owed > loads * field * HALF_WAY_UP,
            "…and NOT a flat fraction of the Field's own rate, which is the reading §2.8 rules out \
             ({owed} against {})",
            loads * field * HALF_WAY_UP
        );
    }

    /// **A tile that is NOT the reference tile** — `RiverDelta`'s `K` of 210 against the reference
    /// 195, so a patch on it reads `210 / 195` tender-loads and any figure quoted per load shows the
    /// scaling instead of hiding behind a factor of one.
    const SCALED_TILE_BIOME: TerrainType = TerrainType::RiverDelta;

    /// **A THIN TILE** — `PrairieSteppe`'s `K` of 70, well under the reference 195, so a patch on it
    /// owes materially LESS than the rung's declared rate. The other end of the same claim.
    const THIN_TILE_BIOME: TerrainType = TerrainType::PrairieSteppe;

    /// **THE BILL IS THE SIZE OF THE LAND** — two patches at the *same* ladder position on tiles of
    /// different `K` owe in exactly the ratio of those `K`s, because both plant rungs quote their rate
    /// per tender-load (`patch_tender_loads`). This is the whole of what `scaled_by: source_load`
    /// buys the plant web: a rich alluvial patch costs more to hold than a thin steppe one.
    #[test]
    fn a_rich_patch_costs_more_to_hold_than_a_thin_one() {
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let (base, width) = plant_rung_span(RungKey::PlantTended, &ladder);
        let held = base + width;

        let rich_capacity = forage.capacity_for(SCALED_TILE_BIOME);
        let thin_capacity = forage.capacity_for(THIN_TILE_BIOME);
        let mut rich = ForagePatch::new(UVec2::new(1, 1), rich_capacity);
        let mut thin = ForagePatch::new(UVec2::new(2, 2), thin_capacity);
        rich.set_ladder_position(held, &ladder);
        thin.set_ladder_position(held, &ladder);

        // **THE PRECONDITION: the two really are at the same position.** Without it the pair can pass
        // by both sides collapsing — two zeroes are in every ratio there is.
        assert_eq!(
            rich.ladder_position(),
            thin.ladder_position(),
            "PRECONDITION: the two patches must stand on the SAME rung, or this compares positions \
             and not ground"
        );
        assert!(
            rich_capacity > thin_capacity && thin_capacity > NO_FORAGE_CAPACITY,
            "PRECONDITION: the two tiles must differ in `K`, or the ratio below is 1 whatever the \
             mechanism ({rich_capacity} against {thin_capacity})"
        );

        let rich_bill = patch_upkeep_demand(&rich, &ladder, rich_capacity, &forage);
        let thin_bill = patch_upkeep_demand(&thin, &ladder, thin_capacity, &forage);
        assert!(
            thin_bill > NO_UPKEEP_DEMAND,
            "PRECONDITION: the thin patch must owe something, or the ratio is a division by zero"
        );
        assert!(
            (rich_bill / thin_bill - rich_capacity / thin_capacity).abs() < 1e-4,
            "the demand ratio IS the capacity ratio: {rich_bill}/{thin_bill} against \
             {rich_capacity}/{thin_capacity}"
        );
    }

    /// **⛔ THE TRAP: CLIMBING TO A FIELD MUST NOT COMPOUND THE CAPACITY GAIN.**
    ///
    /// [`patch_carrying_capacity`] multiplies the tile's `K` by `field_capacity_gain`, interpolated
    /// on the very ladder position the upkeep demand interpolates on. If the measure read the
    /// **patch's** capacity rather than the **tile's**, a finished Field would be billed
    /// `4.0 × gain × tile_K / capacity_per_tender` — the 2.53 gain stacked on top of the rate's own
    /// `2.0 → 4.0`, landing near ten times a tended patch's bill, a cost nobody chose.
    ///
    /// The tile's `K` is the size of the place; the gain is the rung's **payout**.
    #[test]
    fn climbing_to_field_does_not_compound_the_capacity_gain() {
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let (field_base, field_width) = plant_rung_span(RungKey::PlantField, &ladder);

        // Deliberately NOT the reference tile, so `tile_K / capacity_per_tender ≠ 1` and the two
        // readings below are different numbers.
        let tile_capacity = forage.capacity_for(SCALED_TILE_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(5, 5), tile_capacity);
        patch.set_ladder_position(field_base + field_width, &ladder);
        // The one write that sets a patch's capacity (`advance_forage_regrowth`), replayed here.
        patch.carrying_capacity = patch_carrying_capacity(tile_capacity, &patch, &forage);

        // **THE PRECONDITION: the gain really did apply.** Without it this test passes on any turn
        // the Field's capacity boost silently stops working, which is the failure it exists to catch.
        assert!(
            (patch.carrying_capacity - tile_capacity * forage.cultivation.field_capacity_gain)
                .abs()
                < 1e-3,
            "PRECONDITION: a standing Field holds `field_capacity_gain ×` the tile's own K — \
             {} against {tile_capacity} × {}",
            patch.carrying_capacity,
            forage.cultivation.field_capacity_gain
        );
        assert!(
            forage.cultivation.field_capacity_gain > NO_GROWTH_GAIN,
            "PRECONDITION: the gain must be a real multiplier, or there is nothing to compound"
        );

        let field_rate = ladder
            .rung(RungKey::PlantField)
            .upkeep_demand(ONE_TENDER_LOAD);
        let expected = field_rate * tile_capacity / forage.cultivation.capacity_per_tender;
        let owed = patch_upkeep_demand(&patch, &ladder, tile_capacity, &forage);
        assert!(
            (owed - expected).abs() < 1e-4,
            "a Field owes the rung's rate per tender-load of the TILE's K: {owed} against {expected}"
        );
        assert!(
            (owed - expected * forage.cultivation.field_capacity_gain).abs() > 1e-3,
            "…and emphatically NOT that times the capacity gain, which is the reading that lands a \
             Field near ten times a tended patch: {owed} against {}",
            expected * forage.cultivation.field_capacity_gain
        );
    }

    /// **THE PACING PIN: the reference tile owes exactly what it owed before the measure existed.**
    /// `cultivation.capacity_per_tender` ships at `AlluvialPlain`'s own `K`, so a patch there presents
    /// exactly [`ONE_TENDER_LOAD`] and the conversion from a flat rate to a per-load one is provably
    /// neutral: `2.0` tended and `4.0` on a Field, the rates the ladder already declared.
    #[test]
    fn the_reference_tile_owes_exactly_what_it_owed() {
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let reference = forage.capacity_for(TEST_BIOME);
        assert!(
            (patch_tender_loads(reference, &forage) - ONE_TENDER_LOAD).abs() < 1e-6,
            "PRECONDITION: `capacity_per_tender` ({}) is the reference tile's own K ({reference})",
            forage.cultivation.capacity_per_tender
        );

        for (key, expected) in [
            (RungKey::PlantTended, TENDED_RATE_BEFORE_THE_MEASURE),
            (RungKey::PlantField, FIELD_RATE_BEFORE_THE_MEASURE),
        ] {
            let (base, width) = plant_rung_span(key, &ladder);
            let mut patch = ForagePatch::new(UVec2::new(0, 0), reference);
            patch.set_ladder_position(base + width, &ladder);
            let owed = patch_upkeep_demand(&patch, &ladder, reference, &forage);
            assert!(
                (owed - expected).abs() < 1e-5,
                "{key:?} on the reference tile owes {expected} — the number it owed while the rate \
                 was flat — got {owed}"
            );
        }
    }

    /// **The two plant rungs' shipped rates, restated at the point the conversion promised to hold
    /// them.** Named rather than read back off the ladder deliberately: reading the ladder would make
    /// [`the_reference_tile_owes_exactly_what_it_owed`] a tautology, and what it pins is that these
    /// two *numbers* survived the move from a flat rate to a per-tender-load one.
    const TENDED_RATE_BEFORE_THE_MEASURE: f32 = 2.0;
    const FIELD_RATE_BEFORE_THE_MEASURE: f32 = 4.0;

    /// **⛔ THE RUNG-ORDERING BUG IS UNREPRESENTABLE.** Ray built a Field on a tended patch and got
    /// *Field above 0% while Cultivation read 99%*; with two independent meters that state could be
    /// written down and had to be policed. Here the Field's range **begins** where the tended rung's
    /// ends, so there is no position at which [`ForagePatch::is_field`] holds and
    /// [`ForagePatch::is_cultivated`] does not.
    ///
    /// Swept across the whole branch rather than spot-checked, and past its top, because the bug was
    /// a *boundary* fault and a two-point test is exactly what missed it.
    #[test]
    fn no_position_makes_a_field_that_is_not_also_cultivated() {
        let ladder = LadderConfig::builtin();
        let top = cultivate_cost(&ladder) + field_cost(&ladder);
        const SWEEP_STEPS: u32 = 500;
        const PAST_THE_TOP: f32 = 1.25;
        let mut saw_field = false;
        let mut saw_bare = false;
        for step in 0..=SWEEP_STEPS {
            let position = top * PAST_THE_TOP * (step as f32 / SWEEP_STEPS as f32);
            let mut patch = ForagePatch::new(UVec2::new(0, 0), 120.0);
            patch.set_ladder_position(position, &ladder);
            assert!(
                !patch.is_field() || patch.is_cultivated(),
                "a Field at position {position} read as not cultivated"
            );
            saw_field |= patch.is_field();
            saw_bare |= !patch.is_cultivated();
        }
        // The liveness half: a sweep that never reached a Field, or never left the wild rung, would
        // satisfy the implication vacuously.
        assert!(saw_field, "the sweep never reached a Field");
        assert!(saw_bare, "the sweep never saw untended ground");
    }

    /// **DECAY EATS FROM THE TOP, and the tended rung underneath does not move until the Field is
    /// gone** (`docs/plan_standing_upkeep.md` §2.8, rule 3). *"If Sow is above 0%, cultivation can
    /// never decrease"* is arithmetic here rather than an invariant somebody enforces.
    #[test]
    fn decay_from_mid_field_never_touches_the_tended_rung_until_the_field_is_spent() {
        let ladder = LadderConfig::builtin();
        let tended = cultivate_cost(&ladder);
        let field = field_cost(&ladder);
        const HALF_A_FIELD: f32 = 0.5;
        const BLEED_STEPS: u32 = 40;
        let mut patch = ForagePatch::new(UVec2::new(5, 5), 120.0);
        patch.set_ladder_position(tended + field * HALF_A_FIELD, &ladder);
        assert!(patch.is_cultivated() && !patch.is_field());

        let bleed = (field * HALF_A_FIELD) / BLEED_STEPS as f32;
        for step in 1..=BLEED_STEPS {
            patch.decay_ladder(bleed, &ladder);
            assert!(
                crate::forage::patch_rung_work_done(&patch, RungKey::PlantTended, &ladder)
                    >= tended
                    || patch.ladder_position() < tended,
                "the tended meter moved at step {step} while Field progress remained"
            );
            if patch.ladder_position() > tended {
                assert!(
                    patch.is_cultivated(),
                    "the tended rung was lost at step {step} with the Field still standing"
                );
            }
        }
        // Only once the Field is spent does the ground below it start to go.
        assert!((patch.ladder_position() - tended).abs() < 1e-3);
        assert!(patch.is_cultivated());
        patch.decay_ladder(bleed, &ladder);
        assert!(!patch.is_cultivated(), "and then the tended rung does go");
    }

    #[test]
    fn cultivation_decay_clears_owner_at_zero_and_takes_cultivated_feral() {
        let ladder = LadderConfig::builtin();
        let cost = cultivate_cost(&ladder);
        let mut patch = ForagePatch::new(UVec2::new(2, 2), 120.0);
        patch.accrue_cultivation(FactionId(0), 2.5, &ladder);
        patch.decay_ladder(1.0, &ladder);
        assert!((patch.ladder_position() - 1.5).abs() < 1e-6);
        assert_eq!(patch.owner, Some(FactionId(0)), "owner held above zero");
        // Decaying to zero clears ownership so another faction can later tend it.
        patch.decay_ladder(cost, &ladder);
        assert_eq!(patch.ladder_position(), RUNG_UNSTARTED);
        assert_eq!(patch.owner, None);
        // A completed rung is lost the moment the position drops below its top — there is no
        // retention bar left, which is safe because the payout fades continuously rather than
        // cliffing (see `RungMeterDecay`'s gravestone).
        patch.accrue_cultivation(FactionId(1), cost, &ladder);
        assert!(patch.is_cultivated());
        patch.decay_ladder(cost * 0.5, &ladder);
        assert!(
            !patch.is_cultivated(),
            "an untended tended patch reverts to wild"
        );
        assert!((patch.ladder_position() - cost * 0.5).abs() < 1e-4);
    }

    /// **The commitment is recorded once and released only by going fully feral** (Flora Roster S1).
    /// Re-deciding which crop a patch is every turn would erase the decision the rung exists to
    /// make; keeping it after the position lapses would leave a wild stand wearing one plant's name.
    #[test]
    fn a_species_commitment_is_one_way_and_lapses_only_when_the_patch_goes_fully_feral() {
        let ladder = LadderConfig::builtin();
        let mut patch = ForagePatch::new(UVec2::new(3, 4), 120.0);
        patch.commit_species("wild_emmer");
        assert_eq!(patch.species.as_deref(), Some("wild_emmer"));
        // One-way while the ground is committed: a later assignment cannot re-crop it for free.
        patch.commit_species("wild_tubers");
        assert_eq!(patch.species.as_deref(), Some("wild_emmer"));

        // A patch with anything left on the ladder keeps its crop...
        patch.complete_field(FactionId(0), &ladder);
        patch.decay_ladder(field_cost(&ladder), &ladder);
        assert_eq!(
            patch.species.as_deref(),
            Some("wild_emmer"),
            "a lapsed Field over a standing tended patch is still that crop"
        );
        // ...and lapses only when the position reaches zero.
        patch.decay_ladder(cultivate_cost(&ladder), &ladder);
        assert_eq!(patch.ladder_position(), RUNG_UNSTARTED);
        assert_eq!(
            patch.species, None,
            "a fully feral patch is the wild basket again"
        );
        assert_eq!(patch.owner, None);
    }

    /// **THE PLANT ROT RATES ARE THE PACING-NEUTRAL INVERSION OF THE RETIRED
    /// `decay_fraction_per_turn`, ASSERTED AS ARITHMETIC** (`docs/plan_standing_upkeep.md` §2.4).
    ///
    /// Both plant rungs used to bleed `0.01 × their own work_cost` on every turn nobody worked the
    /// patch, and for one slice that product was also their **demand**, because shortfall *was* the
    /// decay. The two questions have separate dials now — `work_per_turn` is what holding costs,
    /// `meter_decay.per_turn` is how fast it rots — and this pins the half that must not have moved:
    /// a **wholly unmaintained** improvement still loses precisely what it always lost, so the
    /// demands could be retuned to whole numbers with the decay axis provably untouched.
    ///
    /// Stated against the retired fraction rather than against the two literals, so a future retune
    /// of either `work_cost` reads as a deliberate change of the bleed rather than as a silent one.
    ///
    /// The **spread** between the two rungs is deliberately *not* asserted: 0.75 > 0.5 is an artefact
    /// of inverting `0.01` against a bigger job, not a claim that a Field rots faster.
    #[test]
    fn the_plant_rot_rates_are_exactly_what_the_retired_decay_fraction_bled() {
        /// The fraction of its own `work_cost` each plant rung bled per un-worked turn, before
        /// shortfall became the decay. Restated here because the dial it named is gone: this test is
        /// the only remaining record of the number the rot rates were derived from.
        const RETIRED_DECAY_FRACTION_PER_TURN: f32 = 0.01;
        /// The turn a wholly unmaintained rung is on once every grace on the ladder is spent —
        /// larger than any shipped `grace_turns`, so the decay is certainly biting.
        const WELL_PAST_ANY_GRACE: u16 = 32;
        let ladder = LadderConfig::builtin();
        for key in [RungKey::PlantTended, RungKey::PlantField] {
            let rung = ladder.rung(key);
            let cost = rung
                .build_cost(RUNG_COST_UNSCALED)
                .expect("both plant rungs build");
            // **The whole demand is what a patch with no keepers goes short by**, which is the step
            // that turns a staffing into a rot rate.
            let demand = rung.upkeep_demand(ONE_TENDER_LOAD);
            let fraction = crate::intensification::upkeep_shortfall_fraction(
                demand,
                crate::intensification::NO_UPKEEP_DEMAND,
            );
            assert_eq!(
                fraction,
                crate::intensification::WHOLLY_UNSUPPLIED,
                "{key:?}: nobody keeping it means the whole demand is unmet"
            );
            let bled = rung.upkeep_decay(fraction, WELL_PAST_ANY_GRACE);
            assert!(
                (bled - cost * RETIRED_DECAY_FRACTION_PER_TURN).abs() < 1e-6,
                "{key:?}: a wholly unmaintained improvement must bleed what it always bled — \
                 {bled} against {RETIRED_DECAY_FRACTION_PER_TURN} × cost {cost}"
            );
            // **And it is PROPORTIONAL**: half the demand met is half the rot, which is what makes
            // the demand retunable without touching the rot at all.
            let half_bled = rung.upkeep_decay(
                crate::intensification::upkeep_shortfall_fraction(demand, demand / 2.0),
                WELL_PAST_ANY_GRACE,
            );
            assert!(
                (half_bled - bled / 2.0).abs() < 1e-6,
                "{key:?}: half the hands means half the rot — {half_bled} against {bled}"
            );
        }
    }

    /// **THE KEEPING POOL HOLDS EVERY METER CARRYING WORK, AT ANY FULLNESS**
    /// (`docs/plan_standing_upkeep.md` §4.6a). One rung, one demand, **one** supplier — the
    /// fullness test that used to move a meter between two of them is deleted.
    ///
    /// The half-built rows are what it exists for: a meter mid-`Cultivate` was billed to a build
    /// crew, so taking the builders off it left keepers standing idle in the role with no command
    /// that could aim them at the ground they were staffed to hold.
    #[test]
    fn the_keeping_pool_holds_every_meter_at_any_fullness() {
        const A_KEEPERS_TURN: f32 = 1.0;
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let cost = ladder
            .rung(RungKey::PlantTended)
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        let demand = ladder
            .rung(RungKey::PlantTended)
            .upkeep_demand(ONE_TENDER_LOAD);

        const HALF_WAY_UP: f32 = 0.5;
        let mut patch = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));

        // --- Half-built: billed to the pool, at the INTERPOLATED rate. -------------------------
        patch.set_ladder_position(cost * HALF_WAY_UP, &ladder);
        assert_eq!(
            patch_upkeep_workers_needed(&patch, &ladder, test_tile_capacity(), &forage),
            (demand * HALF_WAY_UP).ceil() as u32,
            "hands to meet the rate the position actually owes"
        );
        assert_eq!(
            patch_upkeep_supply(&patch, Some(Improvement::Cultivate), A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "a meter being raised is held by the pool like any other"
        );
        assert_eq!(
            patch_upkeep_supply(&patch, None, A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "AND A HALF-BUILT METER NOBODY IS BUILDING CAN BE HELD — the defect the fullness test \
             made unreachable: the builders leave, the keepers stay, the ground holds"
        );
        // **A crew starting a Sow answers for the FIELD from its first turn**, even before that
        // meter has any progress on it — the verb says which meter their hands are on, and reading
        // only the progress would credit the keeping to the rung underneath and then let the next
        // turn's pass bleed the Field they had just started.
        assert_eq!(
            patch_upkeep_supply(&patch, Some(Improvement::Sow), A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "a verb in flight claims, so a Sow's first turn is not billed against a share of zero"
        );

        // --- Finished: the same supplier, and now the rung's whole rate. ------------------------
        // There is no retention bar to stamp any more (`docs/plan_standing_upkeep.md` §2.8) — a rung
        // is achieved exactly at the top of its span, which is where the position now sits.
        patch.set_ladder_position(cost, &ladder);
        assert_eq!(
            patch_upkeep_workers_needed(&patch, &ladder, test_tile_capacity(), &forage),
            ladder
                .rung(RungKey::PlantTended)
                .upkeep_crew_needed(ONE_TENDER_LOAD),
            "a held rung asks for the hands that hold it"
        );
        assert_eq!(
            patch_upkeep_supply(&patch, Some(Improvement::Cultivate), A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "a finished rung is held by the pool, whatever verb is still hanging off the assignment"
        );

        // --- A rung ERODED BELOW ITS COST is still the pool's, and that is the other autopsy. ---
        // It used to flip back into *building*, so a one-percent repair became the build crew's
        // business at the very moment the keeping needed to cover it.
        const ALL_BUT_FINISHED: f32 = 0.99;
        patch.set_ladder_position(cost * ALL_BUT_FINISHED, &ladder);
        assert_eq!(
            patch_upkeep_supply(&patch, None, A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "a dipped rung does not stop being the keeping pool's when it starts needing it"
        );
        assert!(
            !patch.is_cultivated(),
            "fixture: it is no longer TENDED — the bar is gone, so the rung ends where its span \
             does. What makes that safe is the payout, which is still 99% of a tended patch's"
        );
        assert!(
            (patch_upkeep_demand(&patch, &ladder, test_tile_capacity(), &forage)
                - demand * ALL_BUT_FINISHED)
                .abs()
                < 1e-3,
            "and it owes 99% of the rate, which is the other half of the same statement"
        );

        // --- A crew doing something else no longer withholds the keeping. -----------------------
        // The meter answered for is the newest (the half-sown Field); the pool holds it whether or
        // not the hands on the row are the ones filling it.
        let mut half_sown = ForagePatch::new(UVec2::new(3, 3), forage.capacity_for(TEST_BIOME));
        let field_cost = ladder
            .rung(RungKey::PlantField)
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the field rung builds");
        half_sown.set_ladder_position(cost + field_cost * HALF_WAY_UP, &ladder);
        assert_eq!(
            patch_upkeep_supply(&half_sown, Some(Improvement::Cultivate), A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "the pool holds the Field, whatever the crew standing on the tile is doing"
        );

        // --- A wild patch: nothing built, nothing owed, nobody wanted. -------------------------
        let wild = ForagePatch::new(UVec2::new(2, 2), forage.capacity_for(TEST_BIOME));
        assert_eq!(
            patch_upkeep_demand(&wild, &ladder, test_tile_capacity(), &forage),
            NO_UPKEEP_DEMAND
        );
        assert_eq!(
            patch_upkeep_supply(&wild, None, A_KEEPERS_TURN),
            NO_UPKEEP_DEMAND,
            "ground with nothing on it cannot be billed, so nothing can be short"
        );
        assert_eq!(
            patch_upkeep_workers_needed(&wild, &ladder, test_tile_capacity(), &forage),
            NO_CREW_ON_THIS_ACTIVITY
        );
        assert!(
            demand > NO_UPKEEP_DEMAND,
            "fixture: the rung does cost something to hold"
        );
    }

    /// **PARTIAL SUPPLY IS CONTINUOUS ON A METER BEING RAISED TOO** — half the pool's share on an
    /// unfinished meter is half a shortfall, exactly as it is on a finished one. Deleting the
    /// fullness test moved *who* answers for a meter; it did not make the arm a step function.
    ///
    /// Asserted at the seam rather than through the system because a build's *accrual* scales with
    /// the band's **builders pool**, so a system-level comparison would confound the two.
    #[test]
    fn a_half_staffed_keeping_is_half_short_on_the_meter_it_is_raising() {
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let rung = ladder.rung(RungKey::PlantTended);
        let cost = rung
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        // **THE DEMAND IS THE INTERPOLATED ONE NOW** — a meter half-way up the tended rung owes half
        // the tended rung's rate (`docs/plan_standing_upkeep.md` §2.8), where it used to owe the
        // whole of it. The pool that pays it is unchanged (§4.6a); what moved is the amount, and it
        // moved to match the payout, which interpolates on the same standing.
        const HALF_WAY_UP: f32 = 0.5;
        let mut patch = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));
        patch.set_ladder_position(cost * HALF_WAY_UP, &ladder);
        let demand = patch_upkeep_demand(&patch, &ladder, test_tile_capacity(), &forage);
        assert!(
            (demand - rung.upkeep_demand(ONE_TENDER_LOAD) * HALF_WAY_UP).abs() < 1e-4,
            "a half-raised tended rung owes half the tended rung's rate"
        );

        let short_at = |keeping_share: f32| -> f32 {
            let supplied = patch_upkeep_supply(&patch, Some(Improvement::Cultivate), keeping_share);
            crate::intensification::upkeep_shortfall(
                patch_upkeep_demand(&patch, &ladder, test_tile_capacity(), &forage),
                supplied,
            )
        };

        assert_eq!(
            short_at(NO_UPKEEP_DEMAND),
            demand,
            "no keepers, whole demand short"
        );
        assert!(
            (short_at(demand / 2.0) - demand / 2.0).abs() < 1e-6,
            "half the pool's share, half short"
        );
        assert_eq!(
            short_at(demand),
            NO_UPKEEP_DEMAND,
            "the whole share, nothing short"
        );
    }

    /// Rung 1a feral mechanic at the patch level: a patch whose keepers meet the demand never
    /// decays; one with nobody on it goes feral — reverting to wild on the first bleeding turn and
    /// fully lapsing to `0` (owner cleared) over ~`cost/demand` turns. Replicates the system's
    /// `decay(upkeep_shortfall(demand, supplied))`, which is the whole of the new pass.
    #[test]
    fn a_kept_patch_is_spared_and_an_unkept_one_goes_feral() {
        let forage = test_forage_config();
        let ladder = LadderConfig::builtin();
        let tended_rung = ladder.rung(RungKey::PlantTended);
        let demand = tended_rung.upkeep_demand(ONE_TENDER_LOAD);
        let cost = tended_rung
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        assert!(demand > 0.0, "the tended rung costs something to hold");

        // Kept every turn → the shortfall is zero, so nothing bleeds however long you wait.
        let mut kept = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));
        kept.accrue_cultivation(FactionId(0), cost, &ladder);
        for _ in 0..200 {
            kept.decay_ladder(
                crate::intensification::upkeep_shortfall(demand, demand),
                &ladder,
            );
        }
        assert!(kept.is_cultivated(), "a kept patch never decays");
        assert_eq!(kept.owner, Some(FactionId(0)));

        // Nobody keeping it → the whole demand is unmet, and that is the bleed.
        let unkept_bleed = crate::intensification::upkeep_shortfall(demand, NO_UPKEEP_DEMAND);
        let mut feral = ForagePatch::new(UVec2::new(2, 2), forage.capacity_for(TEST_BIOME));
        feral.accrue_cultivation(FactionId(0), cost, &ladder);
        feral.decay_ladder(unkept_bleed, &ladder);
        assert!(
            !feral.is_cultivated(),
            "one bleeding turn reverts a farm to wild"
        );
        // Over ~cost/demand further turns it fully decays and clears ownership.
        let turns_to_zero = (cost / unkept_bleed).ceil() as usize + 2;
        for _ in 0..turns_to_zero {
            feral.decay_ladder(unkept_bleed, &ladder);
        }
        assert_eq!(
            feral.ladder_position(),
            RUNG_UNSTARTED,
            "feral patch fully reverts"
        );
        assert_eq!(feral.owner, None, "ownership lapses once fully feral");
    }

    /// **HALF THE HANDS IS HALF THE BLEED** — the property the retired binary flag could not express
    /// at all, and the reason the upkeep is a *rate* (`docs/plan_standing_upkeep.md` §2.4). A crew of
    /// one on a rung wanting two used to count as fully worked, so under-crewing cost exactly nothing
    /// until it reached zero.
    #[test]
    fn a_half_staffed_keeping_bleeds_at_half_rate() {
        let ladder = LadderConfig::builtin();
        let rung = ladder.rung(RungKey::PlantTended);
        let demand = rung.upkeep_demand(ONE_TENDER_LOAD);
        let half = demand / 2.0;
        assert!(
            (crate::intensification::upkeep_shortfall(demand, half) - half).abs() < 1e-6,
            "half supplied is half short"
        );
        assert!(
            crate::intensification::upkeep_shortfall(demand, half)
                < crate::intensification::upkeep_shortfall(demand, NO_UPKEEP_DEMAND),
            "and strictly less than nobody at all — under-crewing has to cost something"
        );
        // Over-supplying is not a credit: the meter is not repaired by extra keepers.
        assert_eq!(
            crate::intensification::upkeep_shortfall(demand, demand * 2.0),
            NO_UPKEEP_DEMAND,
            "twice the hands is still zero short, never a negative bleed"
        );
    }

    #[test]
    fn cultivated_count_filters_by_owner() {
        let ladder = LadderConfig::builtin();
        let mut registry = ForageRegistry::default();
        let mut a = ForagePatch::new(UVec2::new(0, 0), 120.0);
        a.complete_cultivation(FactionId(0), &ladder);
        let mut b = ForagePatch::new(UVec2::new(1, 0), 120.0);
        b.complete_cultivation(FactionId(1), &ladder);
        let uncultivated = ForagePatch::new(UVec2::new(2, 0), 120.0);
        registry.patches.insert(a.tile, a);
        registry.patches.insert(b.tile, b);
        registry.patches.insert(uncultivated.tile, uncultivated);
        assert_eq!(registry.cultivated_count(FactionId(0)), 1);
        assert_eq!(registry.cultivated_count(FactionId(1)), 1);
        assert_eq!(registry.cultivated_count(FactionId(2)), 0);
    }
}
