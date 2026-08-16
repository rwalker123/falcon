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
    components::{Improvement, SourceYield, Tile},
    fauna::{
        classify_ecology_phase, escapement_ceiling, forecast_source_yield,
        reseeding_logistic_regrowth, sustainable_yield, EcologyPhase, SourceYieldForecast,
        NO_PASTORAL_YIELD,
    },
    fauna_config::{EcologyConfig, YieldAccounts},
    flora_config::{FloraConfig, FloraShare},
    food::FoodModuleTag,
    intensification::{
        upkeep_shortfall, LadderConfig, LadderConfigHandle, RungDef, RungKey, SiteRefusal,
        FABRICATED_BUILD_COST, NEGLECT_NONE, NO_BUILD_GEAR, NO_CREW_ON_THIS_ACTIVITY,
        NO_UPKEEP_DECAY, NO_UPKEEP_DEMAND, RUNG_UNSTARTED, UNSCALED_UPKEEP,
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

/// **The season a MANAGED harvest is worked at** — full weight, always. A Field's crop is not a wild
/// stand whose bounty comes and goes with the year: it is standing where you planted it, and its
/// harvest is biomass-based and seasonless (`field_provisions`). So the crew's collection cap on it
/// reads the throughput at full season rather than the tile's `FoodModuleTag::seasonal_weight`.
///
/// **Load-bearing, not cosmetic:** `Sow` may place a Field on ground with **no food module at all**
/// (slice 5), whose gather season is [`NO_FORAGE_SEASON`] — zero. Capping a Field's collection by that
/// would let a crew carry home exactly nothing from the rung the whole arc climbs toward.
const MANAGED_HARVEST_SEASON: f32 = 1.0;

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
    /// Cultivation progress **in absolute work units**; the patch is cultivated once it reaches
    /// [`Self::cultivation_cost`]. Accrues **only** while a band works this patch with the
    /// [`crate::components::Improvement::Cultivate`] verb in flight (faction knows Cultivation);
    /// decays on a patch nobody is working (see `advance_cultivation`). The plant mirror of
    /// `Herd::corral_progress`.
    ///
    /// **It is no longer a `0..1` fraction** (`docs/plan_unit_costed_work.md`): a job has a size now,
    /// and a normalized meter cannot express one. The **wire** still publishes the fraction — the sim
    /// divides at capture, so every shipped readout is untouched.
    pub cultivation_progress: f32,
    /// **What this patch's Cultivate costs, in work units** — the companion of
    /// [`Self::cultivation_progress`], and what [`Self::is_cultivated`] compares it against.
    ///
    /// # Why the cost is STORED rather than looked up
    ///
    /// `is_cultivated()` has ~a hundred call sites, most of them nowhere near the ladder, and a
    /// predicate that took a config would spread that config through the whole plant web. So the
    /// accrual seam **stamps** the live resolved cost here while the meter is incomplete, and every
    /// reader asks the source.
    ///
    /// **It is never re-stamped once the rung is complete**, which is the point: a later config
    /// retune that raises the cost must not silently *un*-cultivate ground the player already paid
    /// for. And it resets to [`RUNG_UNSTARTED`] when the meter decays to zero — the patch is
    /// unstarted again.
    ///
    /// **It is the JOB's size, and it is no longer what [`Self::is_cultivated`] compares against** —
    /// that is [`Self::cultivation_retain_bar`]. The two used to be one number, which is why the
    /// first bleed of any size took the rung away.
    pub cultivation_cost: f32,
    /// **THE BAR THIS PATCH IS TENDED DOWN TO** — the rung's
    /// [`crate::intensification::RungDef::retention_bar`] at this patch's own stamped cost, written
    /// the turn the Cultivate completes and cleared the turn the meter falls below it.
    ///
    /// # A RUNG IS NOT LOST THE INSTANT ITS METER DIPS
    ///
    /// A completed meter sits **exactly at its cost**, so a predicate reading `progress >= cost` made
    /// the very first bleed revoke the rung: finish a Cultivate and the patch could be out of
    /// *tended* before its keepers were assigned, and no grace and no rate could fix it because the
    /// loss was a **threshold test** rather than anything continuous. The rung's **achieved** state
    /// and the meter's **fullness** are two facts now, and this is what separates them — a tended
    /// patch stays tended while its meter erodes, and is lost at a stated point well below.
    ///
    /// # WHY IT IS STAMPED RATHER THAN LOOKED UP
    ///
    /// [`Self::is_cultivated`]'s ~hundred call sites hold no ladder config, which is
    /// [`Self::cultivation_cost`]'s own reason for existing — so the bar is stamped beside the cost,
    /// by the same seam, and doubles as the *achieved* marker: [`RUNG_UNSTARTED`] means this rung was
    /// never earned (a wild patch, or one still being raised), so the predicate needs no second
    /// field and no `cost > 0` guard of its own.
    ///
    /// **The rung is still EARNED at `progress >= cost`** — the accrual completes exactly where it
    /// always did. Only losing it moves.
    pub cultivation_retain_bar: f32,
    /// **Field**-build progress in absolute work units; the patch is a sown Field (the plant ladder's
    /// **rung 3**) once it reaches [`Self::field_cost`]. Accrues only while a band works this patch
    /// with [`crate::components::Improvement::Sow`] in flight (faction knows **Seed Selection**);
    /// decays on a patch nobody is working (see `advance_cultivation`). The plant mirror of
    /// `Herd::corral_progress` — and, exactly like the herd's two meters, it is **its own** meter
    /// rather than a second reading of `cultivation_progress`: a branch with two investment rungs
    /// carries two meters, one per rung.
    ///
    /// **Independent of `cultivation_progress`, deliberately.** `Sow` needs no prior patch (§2 — seed
    /// travels), so a Field may stand on ground that was never tended, and a Field that lapses simply
    /// reveals whatever rung the tile still supports underneath (today: wild, since the same untended
    /// turn bleeds both meters).
    pub field_progress: f32,
    /// **What this patch's Sow costs, in work units** — the rung-3 twin of [`Self::cultivation_cost`],
    /// with the same stamping rule and the same reason for existing.
    pub field_cost: f32,
    /// **THE BAR THIS PATCH IS A FIELD DOWN TO** — the rung-3 twin of
    /// [`Self::cultivation_retain_bar`], with the same rule and the same reason for existing.
    pub field_retain_bar: f32,
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
    // **NEWEST METER FIRST, exactly as the unwind resolves** ([`patch_unwinding_rung`]): a Field with
    // any progress on it governs the tended ground beneath, so a `Cultivate` declared on a Field is
    // dead rather than stalled — the case that used to need the retired "nothing left to build" test.
    if patch.field_progress > RUNG_UNSTARTED {
        return (!patch.field_meter_full()).then_some(Improvement::Sow);
    }
    // **A declaration counts for a meter at zero**, which the field's is here — so a `Sow` on tended
    // ground (or on a bare gathering site) is the player starting the rung above.
    if declared == Some(Improvement::Sow) {
        return Some(Improvement::Sow);
    }
    if patch.cultivation_progress > RUNG_UNSTARTED {
        return (!patch.cultivation_meter_full()).then_some(Improvement::Cultivate);
    }
    // Both meters at zero: only the player can say which rung this ground climbs.
    (declared == Some(Improvement::Cultivate)).then_some(Improvement::Cultivate)
}

/// **IS THE RUNG THIS QUEUE ENTRY DECLARED ALREADY STANDING?** — the test that retires a **dead**
/// entry (`docs/plan_standing_upkeep.md` §2.5), and the plant twin of
/// `fauna::herd_rung_already_built`.
///
/// # IT IS THE METER'S OWN FULLNESS, NOT THE RETAIN BAR
///
/// It asks exactly what [`patch_build_verb`] asks — [`ForagePatch::cultivation_meter_full`] /
/// [`ForagePatch::field_meter_full`] — so the two can never disagree about whether there is work
/// left. `is_cultivated()` (what `validate_cultivate` asks the **player**) compares against the
/// *retain bar*, which sits below the cost: a meter that has eroded between the two is a rung the
/// builders are legitimately repairing, and retiring its entry would cancel that repair.
///
/// # AND IT IS EMPHATICALLY NOT "THE DERIVED VERB IS `None`"
///
/// [`patch_build_verb`] also answers `None` for a source with nothing banked and nothing declared —
/// a live entry that has simply not started — so retiring on that would drop an entry the turn the
/// player made it.
pub fn patch_rung_already_built(patch: &ForagePatch, declared: Improvement) -> bool {
    match declared {
        Improvement::Cultivate => patch.cultivation_meter_full(),
        Improvement::Sow => patch.field_meter_full(),
        // A rung the animal web owns can never stand on ground.
        Improvement::Tame | Improvement::Corral => false,
    }
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
            cultivation_progress: RUNG_UNSTARTED,
            cultivation_cost: RUNG_UNSTARTED,
            cultivation_retain_bar: RUNG_UNSTARTED,
            field_progress: RUNG_UNSTARTED,
            field_cost: RUNG_UNSTARTED,
            field_retain_bar: RUNG_UNSTARTED,
            build_turns_remaining: None,
            build_work_from_gear: NO_BUILD_GEAR,
            build_queue_position: crate::intensification::NOT_IN_ANY_BUILD_QUEUE,
            build_blocked_reason: crate::intensification::BuildGate::Open,
            species: None,
            owner: None,
            neglect_turns: NEGLECT_NONE,
            upkeep_supplied: NO_UPKEEP_DEMAND,
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

    /// A fully-cultivated ("tended crop") patch: pays the band that tends it a higher-than-wild yield
    /// each turn (place-local, in `advance_labor_allocation`) and is not gather-drawn. Reverts to a
    /// wild gather patch the moment `cultivation_progress` decays below `1.0` (feral — see
    /// `advance_cultivation`). The plant mirror of `Herd::is_domesticated`.
    ///
    /// **`bar > RUNG_UNSTARTED` is load-bearing, not defensive**: a wild patch — and one still being
    /// prepared — carries `0` there, and `0 >= 0` would read every untouched stand on the map as a
    /// tended crop. It is the *achieved* half of the predicate: an unearned rung has no bar.
    ///
    /// **IT READS THE RETENTION BAR, NOT THE COST** ([`Self::cultivation_retain_bar`]). The rung is
    /// earned at `progress >= cost` and held until `progress` falls below a stated fraction of that
    /// cost — so the meter may erode for many turns under a patch that is still, correctly, tended.
    pub fn is_cultivated(&self) -> bool {
        self.cultivation_retain_bar > RUNG_UNSTARTED
            && self.cultivation_progress >= self.cultivation_retain_bar
    }

    /// A fully-sown **Field** (the plant ladder's rung 3): pays the band that works it a *higher*
    /// managed yield than a tended patch (`field_provisions`) and, like a tended patch, is not
    /// gather-drawn. Reverts once `field_progress` erodes below [`Self::field_retain_bar`] (see
    /// `advance_cultivation`). The plant mirror of `Herd::is_corralled`.
    pub fn is_field(&self) -> bool {
        self.field_retain_bar > RUNG_UNSTARTED && self.field_progress >= self.field_retain_bar
    }

    /// **IS THE CULTIVATION METER FULL?** — `progress >= cost`, the *building vs maintaining* state
    /// test (`docs/plan_standing_upkeep.md` §2.4) and **not** the same question as
    /// [`Self::is_cultivated`].
    ///
    /// The two came apart when a rung stopped being lost on its first bleed. A patch eroded to 99% of
    /// its cost is **still tended** (it is above its retention bar) and is **building** (its meter is
    /// not full), so a `Cultivate` crew may repair it and pays the maintenance rate while doing so.
    /// Fullness is what the build accrual, the "nothing left to build" test and the supplier test all
    /// ask; the retention bar is what the ~hundred *is this ground tended* call sites ask.
    ///
    /// `cost > RUNG_UNSTARTED` is load-bearing for the reason it always was: a wild patch carries `0`
    /// in both fields and `0 >= 0` would read every untouched stand as finished.
    pub fn cultivation_meter_full(&self) -> bool {
        self.cultivation_cost > RUNG_UNSTARTED && self.cultivation_progress >= self.cultivation_cost
    }

    /// **IS THE FIELD METER FULL?** — the rung-3 twin of [`Self::cultivation_meter_full`].
    pub fn field_meter_full(&self) -> bool {
        self.field_cost > RUNG_UNSTARTED && self.field_progress >= self.field_cost
    }

    /// Is this patch a **completed improvement** — a Field or a tended patch? The single predicate
    /// for "this source is worked, not gathered": its harvest is biomass-based and never overdraws
    /// (`sustainable == actual`, no ⚠) and one worker suffices
    /// ([`crate::fauna::TENDED_SOURCE_WORKERS_NEEDED`]). Both the payout path and the forecast branch
    /// on it, so the two cannot disagree about which patches are managed.
    pub fn is_managed(&self) -> bool {
        self.is_field() || self.is_cultivated()
    }

    /// Accrue cultivation progress for `faction` (the preparing band, working the patch with
    /// [`crate::components::Improvement::Cultivate`] in flight). Sets ownership on the first accrual;
    /// only the owner makes progress.
    /// Clamped to 1.0 — reaching it makes the patch a tended crop from the *next* turn's payout on
    /// (the accrual runs after this turn's take, so the pre-commit forecast can't lie). No-op once the
    /// patch is cultivated. Mirrors `Herd::accrue_corral`.
    ///
    /// **Returns `true` only when THIS call finished the rung** — `accrue_corral`'s convention, and
    /// load-bearing for the feed line: `handle_cultivate` sets the verb on *every* band working the
    /// patch, so a post-hoc `is_cultivated()` test would announce "Cultivated patch at (x, y)" once
    /// per band. Whether a band's *improvement* should be cleared is a different question (it should,
    /// whoever finished it) and is answered separately by the caller.
    ///
    /// `cost` is the job's size in work units ([`RungDef::build_cost`]), **stamped onto the patch**
    /// while the meter is incomplete so that [`Self::is_cultivated`] needs no config — see
    /// [`Self::cultivation_cost`].
    ///
    /// `retain_bar` is the rung's [`RungDef::retention_bar`] at that cost, and is stamped **only on
    /// the turn the rung completes** ([`Self::cultivation_retain_bar`]) — a build in flight has
    /// earned nothing to hold, and stamping it early would read as tended ground the moment the meter
    /// passed three-quarters of the job.
    pub(crate) fn accrue_cultivation(
        &mut self,
        faction: FactionId,
        amount: f32,
        cost: f32,
        retain_bar: f32,
    ) -> bool {
        // **The guard is the METER, not the rung** (`Self::cultivation_meter_full`). A tended patch
        // eroded below its cost is *building* — that shortfall is a repair, and refusing it here
        // would make erosion a one-way ratchet with no way back up short of losing the rung.
        if self.cultivation_meter_full() {
            return false;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        self.cultivation_cost = cost;
        self.cultivation_progress = banked_up_to_cost(self.cultivation_progress + amount, cost);
        // **The rung is EARNED at the full job** — the meter reaching its own cost, exactly where it
        // always completed. Only *losing* it moved down to the bar this stamps.
        if cost > RUNG_UNSTARTED && self.cultivation_progress >= cost {
            self.cultivation_retain_bar = retain_bar;
        }
        self.is_cultivated()
    }

    /// **A fixture's already-tended patch** — the honest replacement for writing
    /// `cultivation_progress = 1.0`, which no longer means anything now that a job has a size. It
    /// runs the real accrual against [`FABRICATED_BUILD_COST`], so ownership and the owner-lock
    /// behave exactly as they do in play.
    ///
    /// **It is held at its WHOLE cost** — the strictest retention bar there is, and the pre-arc
    /// reading — because a fixture holds no ladder to ask for a fraction. A test about the *erosion*
    /// of a completed rung must therefore build the patch through the real accrual with the rung's
    /// own [`RungDef::retention_bar`]; this helper is for tests that merely need tended ground.
    pub fn complete_cultivation(&mut self, faction: FactionId) -> bool {
        self.accrue_cultivation(
            faction,
            FABRICATED_BUILD_COST,
            FABRICATED_BUILD_COST,
            FABRICATED_BUILD_COST,
        )
    }

    /// Accrue **Field**-build progress for `faction` (the sowing band, working the patch with
    /// [`crate::components::Improvement::Sow`] in flight) — the exact twin of `accrue_cultivation` one
    /// rung up, with the same
    /// owner-locking, the same clamp, the same "no-op once complete", and the same
    /// this-call-finished-it return.
    pub(crate) fn accrue_field(
        &mut self,
        faction: FactionId,
        amount: f32,
        cost: f32,
        retain_bar: f32,
    ) -> bool {
        if self.field_meter_full() {
            return false;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        self.field_cost = cost;
        self.field_progress = banked_up_to_cost(self.field_progress + amount, cost);
        if cost > RUNG_UNSTARTED && self.field_progress >= cost {
            self.field_retain_bar = retain_bar;
        }
        self.is_field()
    }

    /// **A fixture's already-sown Field** — the rung-3 twin of [`Self::complete_cultivation`], held
    /// at its whole cost for that helper's reason.
    pub fn complete_field(&mut self, faction: FactionId) -> bool {
        self.accrue_field(
            faction,
            FABRICATED_BUILD_COST,
            FABRICATED_BUILD_COST,
            FABRICATED_BUILD_COST,
        )
    }

    /// Decay cultivation progress toward zero by `amount`. Applies to **any** patch — a completed
    /// (`is_cultivated`) patch decays too (going feral once it drops below `1.0`, reverting to a wild
    /// gather patch); the *caller* (`advance_cultivation`) decides when to spare a worked patch.
    /// Mirrors `Herd::decay_domestication` (minus the domesticated short-circuit — a tended patch left
    /// untended is meant to go feral).
    ///
    /// **Returns `true` only when THIS call took the rung back below its RETENTION BAR** — the feral
    /// *edge*, the exact mirror of [`Self::accrue_cultivation`]'s "did this call finish it". The
    /// caller announces on that edge and nowhere else: a 25-turn investment's payoff has just been
    /// destroyed, and the feed says so once rather than every turn of the long bleed that follows.
    ///
    /// **The bar is where the rung is LOST, and crossing it clears the bar**
    /// ([`Self::cultivation_retain_bar`] back to [`RUNG_UNSTARTED`]) — which is what makes the loss
    /// stick: the patch has to be re-earned at the full cost, from wherever its meter landed, and the
    /// bar cannot be re-crossed on the way back up without completing the rung again.
    ///
    /// **A meter that reaches zero forgets its cost too** ([`Self::cultivation_cost`] back to
    /// [`RUNG_UNSTARTED`]): the ground is unstarted again, and a stranded cost would leave a wild
    /// patch quoting a price nobody is paying.
    pub(crate) fn decay_cultivation(&mut self, amount: f32) -> bool {
        let was_cultivated = self.is_cultivated();
        self.cultivation_progress = (self.cultivation_progress - amount).max(RUNG_UNSTARTED);
        if self.cultivation_progress < self.cultivation_retain_bar {
            self.cultivation_retain_bar = RUNG_UNSTARTED;
        }
        if self.cultivation_progress <= RUNG_UNSTARTED {
            self.cultivation_cost = RUNG_UNSTARTED;
        }
        self.reconcile_owner();
        was_cultivated && !self.is_cultivated()
    }

    /// Decay **Field**-build progress toward zero by `amount` — the rung-3 twin of
    /// `decay_cultivation`, and (unlike the pen, which is lost outright when its herd bolts) a
    /// *gradual* bleed for the same reason cultivation bleeds gradually: **a patch is a place and a
    /// herd is not**, so leftover progress still refers to the same ground.
    ///
    /// **Returns `true` only when THIS call took the rung back below its retention bar** — see
    /// [`Self::decay_cultivation`] for why the announcement rides the edge and why crossing the bar
    /// clears it.
    pub(crate) fn decay_field(&mut self, amount: f32) -> bool {
        let was_field = self.is_field();
        self.field_progress = (self.field_progress - amount).max(RUNG_UNSTARTED);
        if self.field_progress < self.field_retain_bar {
            self.field_retain_bar = RUNG_UNSTARTED;
        }
        if self.field_progress <= RUNG_UNSTARTED {
            self.field_cost = RUNG_UNSTARTED;
        }
        self.reconcile_owner();
        was_field && !self.is_field()
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
        if self.cultivation_progress <= RUNG_UNSTARTED && self.field_progress <= RUNG_UNSTARTED {
            self.owner = None;
            self.species = None;
        }
    }
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
    composition_for_rung(patch, tile_composition, forage, standing_rung(patch))
}

/// **The rung a patch actually STANDS on**, as a [`RungKey`] — sown → `plant:field`, cultivated →
/// `plant:tended`, else `plant:wild`. The ladder-free twin of [`patch_rung`] (which resolves a whole
/// `RungDef` and therefore needs the ladder config); this one exists because the rate seams need the
/// *key* and nothing else, on paths that carry no ladder.
fn standing_rung(patch: &ForagePatch) -> RungKey {
    if patch.is_field() {
        RungKey::PlantField
    } else if patch.is_cultivated() {
        RungKey::PlantTended
    } else {
        RungKey::PlantWild
    }
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
fn rung_rate(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
    rate_of: impl Fn(&crate::flora_config::FloraDef) -> f32,
    fallback: f32,
) -> f32 {
    basket_rate(
        &composition_for_rung(patch, tile_composition, forage, rung),
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
    let rung = standing_rung(patch);
    let composition = composition_for_rung(patch, tile_composition, forage, rung);
    let favored_gain = favored_conversion_gain(rung, forage);
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
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        standing_rung(patch),
        |def| def.yield_.provisions_per_biomass,
        forage.provisions_per_biomass,
    )
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
    match rung {
        RungKey::PlantField => {
            field_provisions(patch, tile_composition, forage, flora, output_multiplier)
        }
        RungKey::PlantTended => {
            tended_provisions(patch, tile_composition, forage, flora, output_multiplier)
        }
        _ => forage_provisions(
            sustainable_yield(patch.biomass, patch.carrying_capacity, &forage.ecology)
                .clamp(0.0, patch.biomass),
            patch_provisions_per_biomass(patch, tile_composition, flora, forage),
            output_multiplier,
        ),
    }
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
/// an owner (the accrual sets one on first progress), and the quote is a pure function of ground and
/// config, so the faction it names cannot matter; it is stated rather than spelled `FactionId(0)` at
/// the two call sites so the *irrelevance* travels with the value.
const HYPOTHETICAL_OWNER: FactionId = FactionId(0);

fn hypothetical_patch(
    tile: UVec2,
    tile_capacity: f32,
    species: Option<&str>,
    rung: RungKey,
) -> ForagePatch {
    let mut patch = ForagePatch::new(tile, tile_capacity);
    patch.biomass = tile_capacity * settled_biomass_fraction(rung);
    if let Some(key) = species {
        patch.species = Some(key.to_string());
        // The hypothetical is a *quote*, not a source anyone paid for, so it fabricates the
        // completed meter rather than pricing one — the rate seams only ask `is_field()` /
        // `is_cultivated()`.
        match rung {
            RungKey::PlantField => patch.complete_field(HYPOTHETICAL_OWNER),
            _ => patch.complete_cultivation(HYPOTHETICAL_OWNER),
        };
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

/// **The fraction of its own capacity a patch on `rung` settles at.** A drawn-down rung is gathered
/// to its MSY operating point (`MSY_BIOMASS_FRACTION` — Sustain's escapement, the point a harvested
/// stand *lives* at); a Field is never drawn down, so it stands at its capacity.
fn settled_biomass_fraction(rung: RungKey) -> f32 {
    match rung {
        RungKey::PlantField => FULL_STANDING_CROP,
        _ => crate::fauna::MSY_BIOMASS_FRACTION,
    }
}

/// **A Field's standing crop is its whole capacity** — it is never drawn down, so it regrows to `K`
/// and stays there. Named rather than a bare `1.0` because it states *which* stock the number is a
/// fraction of.
const FULL_STANDING_CROP: f32 = 1.0;

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
    let patch = hypothetical_patch(tile, tile_capacity, Some(species), rung);
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
    let patch = hypothetical_patch(tile, tile_capacity, Some(species), rung);
    rung_fodder_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **What a patch pays in FODDER, standing on `rung`** — the fodder arm of [`rung_payoff`], dispatching
/// to the *same* helpers the sim pays each rung with: [`field_fodder`] at rung 3 (a managed rate on the
/// standing crop) and [`tended_fodder`] at rung 2 (the MSY skim, because rung 2 is drawn down). Rung 1
/// pays no *committed* fodder quote — a wild gather's hay is not a commitment's payoff — so it is `0`,
/// the same "cannot climb this rung" sentinel the ratios use.
fn rung_fodder_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    match rung {
        RungKey::PlantField => {
            field_fodder(patch, tile_composition, forage, flora, output_multiplier)
        }
        RungKey::PlantTended => {
            tended_fodder(patch, tile_composition, forage, flora, output_multiplier)
        }
        _ => 0.0,
    }
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
/// expressions the sim pays with — [`field_harvest_production`] at rung 3, [`tended_msy_take`] at
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
    let patch = hypothetical_patch(tile, tile_capacity, Some(species), rung);
    rung_material_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **What a patch pays in MATERIALS, standing on `rung`** — the material arm of [`rung_payoff`],
/// dispatching to the *same* harvest each rung is paid on: [`field_harvest_production`] at rung 3 (a
/// managed rate on the standing crop) and [`tended_msy_take`] at rung 2 (the MSY skim, because rung
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
    let harvest_biomass = match rung {
        RungKey::PlantField => field_harvest_production(patch, forage),
        RungKey::PlantTended => tended_msy_take(patch, forage),
        _ => return Vec::new(),
    };
    // **The same rows `credit_material_yield` is handed, through the same expression** — the quote is
    // the payout's own arithmetic rather than a re-derivation of it.
    crate::materials_config::material_yield_totals(
        &patch_material_yields(patch, tile_composition, flora, forage),
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
    let patch = hypothetical_patch(tile, tile_capacity, None, RungKey::PlantWild);
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
        // `fauna::ecological_carrying_capacity`'s one write, and since #433 the tile's capacity
        // *verbatim* at every rung: no rung below 4 raises `K` and **none lowers it**, so a
        // commitment changes only what the patch's biomass is made of ([`patch_composition`]).
        // Idempotent (the tile's capacity is never read back into itself), so a retuned
        // `capacity_by_biome` reaches patches already on the map without a second write path. A
        // patch whose tile is absent from the map keeps whatever capacity it was seeded with —
        // which is what lets test harnesses build synthetic patches on tiles that do not exist.
        if let Some(tile) = tile_registry
            .index(patch.tile.x, patch.tile.y)
            .and_then(|entity| tiles.get(entity).ok())
        {
            patch.carrying_capacity = tile_forage_capacity(forage, tile);
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
pub fn patch_unwinding_rung<'a>(
    patch: &ForagePatch,
    ladder: &'a LadderConfig,
) -> Option<&'a RungDef> {
    if patch.field_progress > RUNG_UNSTARTED {
        Some(ladder.rung(RungKey::PlantField))
    } else if patch.cultivation_progress > RUNG_UNSTARTED {
        Some(ladder.rung(RungKey::PlantTended))
    } else {
        None
    }
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

/// **WHAT IT COSTS TO HOLD THIS PATCH THIS TURN**, in work units — the at-risk rung's
/// [`RungDef::upkeep_demand`], or [`NO_UPKEEP_DEMAND`] for a wild patch, which has nothing built on it
/// to hold. [`UNSCALED_UPKEEP`] because a patch is **one tile**: the plant web has no head count for a
/// rate to ride, which is the whole reason [`crate::intensification::UpkeepScale::SourceLoad`] exists
/// for the pen instead.
///
/// **THE one definition**, reached by the decay pass, the labor arm's stamp and the snapshot alike —
/// so the demand the sim bleeds against, the demand the player is billed for and the demand the wire
/// shows can never be three different rungs' answers.
pub fn patch_upkeep_demand(patch: &ForagePatch, ladder: &LadderConfig) -> f32 {
    patch_unwinding_rung(patch, ladder)
        .map_or(NO_UPKEEP_DEMAND, |rung| rung.upkeep_demand(UNSCALED_UPKEEP))
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

/// **WHAT HAS BEEN SUNK INTO THE METER AT RISK** — that meter's own stamped cost, in work units, and
/// [`RUNG_UNSTARTED`] for a wild patch.
///
/// It is the ordering key of [`crate::intensification::UpkeepFundMode::Priority`]: *most-invested
/// first*, so a band that cannot cover its whole plant web funds the Field before the garden and
/// lets the marginal ground rot. **The stored cost rather than the live progress**, because the
/// question is *what did this cost me*, and a meter eroding under a shortfall would otherwise slide
/// down the priority order exactly as it started to need the hands.
pub fn patch_at_risk_cost(patch: &ForagePatch) -> f32 {
    if patch.field_progress > RUNG_UNSTARTED {
        patch.field_cost
    } else if patch.cultivation_progress > RUNG_UNSTARTED {
        patch.cultivation_cost
    } else {
        RUNG_UNSTARTED
    }
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
/// [`NO_UPKEEP_DEMAND`] where there is no meter to hold and none being started: nothing is owed, so
/// nothing can be short.
pub fn patch_upkeep_supply(
    patch: &ForagePatch,
    improvement: Option<Improvement>,
    keeping_share: f32,
) -> f32 {
    match patch_meter_answering_for(patch, improvement) {
        Some(_) => keeping_share,
        None => NO_UPKEEP_DEMAND,
    }
}

/// **WHICH METER THIS TURN'S KEEPING ANSWERS FOR** — the **newest** of two readings: the meter with
/// progress on it, and the meter this crew's verb is filling.
///
/// **The verb half is what survives the one-turn carry.** The supply is stamped in Population and
/// read by the *next* Logistics pass, so it has to describe the meter that pass will judge — not the
/// one that was at risk when it was written. A crew starting a `Sow` on a finished tended patch is
/// answering for the **Field** from its very first turn, even though the Field has no progress on it
/// until that turn's accrual lands. Reading progress alone put their work against the tended rung,
/// and the next pass — seeing a Field that now *does* have progress — judged that Field against a
/// supply nobody had credited to it, so a Sow bled `0.75` off its own meter on turn two.
///
/// Since §4.6a the **identity** of the meter no longer changes what is supplied — the keeping share
/// is the answer for either — but the resolution stays, because *which* meter this describes is what
/// the carry is about, and because `None` (nothing built, nothing being built) is a real third
/// answer that must not be paid.
fn patch_meter_answering_for(
    patch: &ForagePatch,
    improvement: Option<Improvement>,
) -> Option<RungKey> {
    let by_progress = if patch.field_progress > RUNG_UNSTARTED {
        Some(RungKey::PlantField)
    } else if patch.cultivation_progress > RUNG_UNSTARTED {
        Some(RungKey::PlantTended)
    } else {
        None
    };
    // **Exhaustive on purpose — a catch-all here fails SILENTLY.** A new plant verb (rung 4's Farm)
    // falling through to `None` would leave its meter answered for by progress alone, which is
    // precisely the reading that bled a Sow off its own meter on turn two. `Improvement::valid_for_*`
    // is exhaustive for the same reason: the retired `!matches!` complements it replaced defaulted a
    // new verb to legal on both webs. Name every variant so the compiler asks the question.
    let by_verb = improvement.and_then(|verb| match verb {
        Improvement::Sow => Some(RungKey::PlantField),
        Improvement::Cultivate => Some(RungKey::PlantTended),
        Improvement::Tame | Improvement::Corral => None,
    });
    match (by_progress, by_verb) {
        (Some(RungKey::PlantField), _) | (_, Some(RungKey::PlantField)) => {
            Some(RungKey::PlantField)
        }
        (Some(key), _) | (None, Some(key)) => Some(key),
        // Nothing built here and nothing being built, so nothing is owed and nothing can be short.
        (None, None) => None,
    }
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
pub fn patch_meter_rot(patch: &ForagePatch, ladder: &LadderConfig) -> f32 {
    patch_unwinding_rung(patch, ladder).map_or(NO_UPKEEP_DECAY, |rung| {
        rung.meter_rot(UNSCALED_UPKEEP, patch.upkeep_supplied, patch.neglect_turns)
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
pub fn patch_upkeep_shortfall(patch: &ForagePatch, ladder: &LadderConfig) -> f32 {
    upkeep_shortfall(patch_upkeep_demand(patch, ladder), patch.upkeep_supplied)
}

/// **The MAINTAIN activity's own `workers_needed`** — whole keepers to meet
/// [`patch_upkeep_demand`], `0` on a wild patch. The plant twin of the herd row's, and the readout
/// that makes the standing cost legible: *"this wants 1, you have 0"*.
///
/// **IT IS PUBLISHED WHILE THE METER IS STILL BEING BUILT TOO**, and it means exactly the same thing
/// there: the keeping pool owes the rate from the first work banked, so these are the hands that
/// hold a half-built meter as much as a finished one. It is **not** a minimum viable build crew —
/// a build crew supplies nothing toward the rate (`docs/plan_standing_upkeep.md` §4.6a), so a lone
/// builder against a demand of `2.0` still banks its whole turn's work. It used to read `0`
/// mid-build, on the since-retired premise that an unfinished meter owed no keeping.
pub fn patch_upkeep_workers_needed(patch: &ForagePatch, ladder: &LadderConfig) -> u32 {
    patch_unwinding_rung(patch, ladder).map_or(NO_CREW_ON_THIS_ACTIVITY, |rung| {
        rung.upkeep_crew_needed(UNSCALED_UPKEEP)
    })
}

pub fn advance_cultivation(
    mut registry: ResMut<ForageRegistry>,
    ladder_config: Res<LadderConfigHandle>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
) {
    let ladder = ladder_config.get();
    for patch in registry.patches.values_mut() {
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
        let demand = patch_upkeep_demand(patch, &ladder);
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
                    let verb = rung.verb_improvement();
                    let lost = if verb == Some(Improvement::Sow) {
                        patch.decay_field(decay)
                    } else {
                        patch.decay_cultivation(decay)
                    };
                    if lost {
                        announce_rung_lost(&mut event_log, tick.0, patch.owner, verb, patch.tile);
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
        // **And this turn's supply**, on the same cycle and for the same reason: it describes the
        // keepers that held the patch, so a patch whose keepers have gone must stop reporting what
        // they paid. Clearing it is also what re-arms this pass — next turn's shortfall is the whole
        // demand again unless somebody restates it.
        patch.upkeep_supplied = NO_UPKEEP_DEMAND;
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
    if patch.is_field() {
        RungKey::PlantField
    } else if patch.is_cultivated() {
        RungKey::PlantTended
    } else {
        RungKey::PlantWild
    }
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn forage_take(
    patch: &mut ForagePatch,
    tile_composition: &[FloraShare],
    workers: u32,
    floor: f32,
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
    let take_ceiling = forage_escapement_ceiling(floor, patch.biomass, patch.carrying_capacity);
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
        .clamp(0.0, patch.biomass);
    // The **conversion** half of the commit trade: every patch turns its biomass into food at its own
    // effective basket's share-weighted average, with the tended rung's gain on the favored crop.
    // Resolved before the take is applied so it reads the same patch state the ceiling did.
    let rate = patch_provisions_per_biomass(patch, tile_composition, flora, forage);
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
        tended_msy_take(patch, forage),
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
fn tended_msy_take(patch: &ForagePatch, forage: &ForageLaborConfig) -> f32 {
    sustainable_yield(
        patch.biomass,
        patch.carrying_capacity,
        &tended_ecology(forage),
    )
    .clamp(0.0, patch.biomass)
}

/// **What a patch would pay in FODDER as a TENDED patch** — the rung-2 quote twin of
/// [`tended_provisions`], routing the yield vector's fodder component instead of its provisions one.
/// The hay counterpart of [`field_fodder`] one rung down, and the number the crop picker's Cultivate
/// rung needs: before this, the picker had only `sowFodderPayoff` and therefore quoted a *sown Field's*
/// hay on the Cultivate row.
///
/// **Priced on [`tended_msy_take`], the same take the food quote uses**, and converted through the
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
        tended_msy_take(patch, forage),
        rung_fodder_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantTended),
        output_multiplier,
    )
}

/// **THE ecology a patch actually lives under** — the plant twin of `fauna::herd_ecology`, and the one
/// place the plant ladder's rung → growth-rate mapping lives. Tending buys a *growth rate*, and
/// nothing else:
///
/// - **wild** (`forage.ecology`, `r` = 0.25) — an untended stand;
/// - **managed** (a tended patch or a Field) — [`tended_ecology`]: `r × cultivation.tended_regrowth_gain`.
///
/// Every consumer of a patch's ecology — regrowth, the MSY/policy ceilings, the phase classification,
/// the forecast — resolves it *here*. **No call site may re-derive it**: a second copy of this mapping
/// is exactly how a forecast starts promising a number the take won't pay (the lesson `herd_ecology`
/// already paid for).
///
/// **Both managed rungs share one curve, deliberately.** A Field is never drawn down (its harvest is a
/// managed rate on the standing crop), so its `r` moves nothing but how fast it recovers from a
/// collapse — inventing a `field_regrowth_gain` nobody's yield reads would be a lever that lies about
/// having an effect. Rung 3's payoff is `field_provisions`, not a curve.
pub fn patch_ecology(patch: &ForagePatch, forage: &ForageLaborConfig) -> EcologyConfig {
    if patch.is_managed() {
        tended_ecology(forage)
    } else {
        forage.ecology
    }
}

/// The **tended** curve: the wild forage ecology with only its `regrowth_rate` scaled by the rung's
/// `cultivation.tended_regrowth_gain`, leaving the shared phase bands
/// (`collapse_fraction`/`stressed_fraction`/`extinction_floor`) intact — the exact shape
/// `fauna::pastoral_ecology_for` gives a tamed herd. Split out from [`patch_ecology`] because the
/// forecast must also answer it for a patch that is **not tended yet** ("what will this pay once
/// cultivated?").
fn tended_ecology(forage: &ForageLaborConfig) -> EcologyConfig {
    EcologyConfig {
        regrowth_rate: forage.ecology.regrowth_rate * forage.cultivation.tended_regrowth_gain,
        ..forage.ecology
    }
}

/// The place-local managed harvest a sown **Field** (rung 3) pays the band working it each turn:
/// `biomass × cultivation.field_provisions_per_biomass`, no biomass drawn down — the *same shape* as
/// [`tended_provisions`] one rung down, at a higher rate. That shape is the point: rung 3 must
/// out-yield rung 2 on the same tile at the same biomass, or the rung is pointless, and holding the
/// shape fixed makes the comparison a single lever rather than a re-derivation.
///
/// Shared by the Forage arm of `advance_labor_allocation` (the payout) and `forage_forecast`, so
/// forecast == actual.
pub(crate) fn field_provisions(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    // **Scaled by the projected basket's relative quality** (Flora Roster S1) —
    // `field_provisions_per_biomass` stays the rung's one dial and `species_quality` is *derived*
    // from the conversion rate, never a second per-species field that could drift from it. A sown
    // Field's basket is 100% its crop, so this is exactly the crop's rate over the wild baseline.
    patch.biomass
        * forage.cultivation.field_provisions_per_biomass
        * patch_species_quality(patch, tile_composition, flora, forage)
        * output_multiplier
}

/// **What a sown Field OFFERS each turn, stated in BIOMASS, before any crew is counted** — the
/// production half of [`field_harvest_biomass`], and the basis every rung-3 *quote* is priced on.
///
/// Split out for the reason [`tended_msy_take`] is: the picker's material quote
/// ([`commit_material_payoff`]) and the payout must describe the same harvest, and a second copy of
/// `biomass × field_provisions_per_biomass` is exactly how they would start to disagree. It is the
/// production term of the same `min(production, collection)` the payout runs, which is why a Field
/// staffed past its collection cap quotes and pays the identical number.
///
/// A Field is never drawn down, so this is a *rate on the standing crop* and `patch.biomass` is
/// unchanged by it.
pub(crate) fn field_harvest_production(patch: &ForagePatch, forage: &ForageLaborConfig) -> f32 {
    patch.biomass * forage.cultivation.field_provisions_per_biomass
}

/// **What a sown Field hands over each turn, stated in BIOMASS** — the managed harvest before it is
/// routed into any one currency, capped by what the crew can carry.
///
/// The scalar accounts each convert this through their own rate, so neither of them ever needs the
/// biomass itself. The **material** account does: a material's `per_biomass` is a rate on the crop
/// rather than on a currency it would otherwise have been sold as, and a cash Field's provisions are
/// `0`, so there is no currency to scale off. Same `min(production, collection)` shape the others
/// run — an understaffed Field brings home less of everything, in step.
pub(crate) fn field_harvest_biomass(
    patch: &ForagePatch,
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    workers: u32,
) -> f32 {
    let collection =
        workers as f32 * forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON);
    field_harvest_production(patch, forage).min(collection)
}

/// The **projected** fodder conversion rate — the projected basket's `yield.fodder_per_biomass`
/// average once the improvement completes (the fodder twin of `projected_provisions_per_biomass`,
/// `docs/plan_flora_roster.md` §5). A sown Field's basket is 100% its crop, so this reads `0.0` for a
/// grain Field and the hay rate for a hay Field, with **no `role` branch** — the vector does the
/// routing. Used by the managed-fodder payout and forecast so a hay Field being sown quotes the hay
/// it *will* pay.
fn field_fodder_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        RungKey::PlantField,
        |def| def.yield_.fodder_per_biomass,
        NO_UNCOMMITTED_YIELD_RATE,
    )
}

/// The place-local managed **fodder** harvest a sown hay **Field** (rung 3) pays into the band's
/// `FODDER` store each turn — the exact fodder twin of [`field_provisions`], routed by the yield
/// vector's fodder component instead of its provisions component. Same shape
/// (`biomass × field_provisions_per_biomass × fodder_quality`, no biomass drawn down), so a hay Field
/// and a grain Field of the same standing crop harvest the same *fraction* of their biomass — they
/// differ only in which account it lands in. `0` for any patch not committed to a fodder crop, so a
/// grain Field credits no fodder, with no role branch.
///
/// `fodder_quality` = the committed crop's `fodder_per_biomass` relative to the **wild provisions
/// baseline** — the same normalization [`patch_species_quality`] uses for the food account, so the
/// field rung's one rate dial (`field_provisions_per_biomass`) prices both accounts consistently.
pub(crate) fn field_fodder(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    if forage.provisions_per_biomass <= 0.0 {
        return 0.0;
    }
    let fodder_quality = field_fodder_per_biomass(patch, tile_composition, flora, forage)
        / forage.provisions_per_biomass;
    patch.biomass
        * forage.cultivation.field_provisions_per_biomass
        * fodder_quality
        * output_multiplier
}

/// **What one worker can carry home from a hay Field**, in fodder/turn — the fodder twin of
/// [`managed_per_worker_yield`]. The crew carries hay exactly as it carries grain, at the same
/// per-worker throughput, so the collection cap on a hay Field is this, in fodder units. `0` for a
/// non-fodder crop (a grain Field's fodder collection is moot).
pub(crate) fn managed_per_worker_fodder(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON),
        field_fodder_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    )
}

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
    rung_fodder_per_biomass(patch, tile_composition, flora, forage, standing_rung(patch))
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
pub fn tended_take_fodder(
    take_biomass: f32,
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        take_biomass,
        patch_fodder_per_biomass(patch, tile_composition, flora, forage),
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
    equipped_gather_rate: f32,
    // **The crew's per-gatherer throughput for THIS turn, season already folded in**
    // (`forage_per_worker_biomass(resolved basket tier, seasonal)`). Taken pre-folded rather than as
    // the tier + the season, so this signature stays inside clippy's argument budget and there is
    // exactly one place the two multiply.
    per_worker_gather_biomass: f32,
    output_multiplier: f32,
) -> SourceYieldForecast {
    // A Field's harvest is biomass-based and **seasonless** — the crop is standing in the field you
    // built it to stand in — so its collection cap is too, and it must not read the gather season
    // (which is `NO_FORAGE_SEASON` on module-less ground a crew sowed: a Field there would forecast,
    // and be paid, exactly nothing).
    if patch.is_field() {
        return SourceYieldForecast::managed(
            plant_food_only(field_provisions(
                patch,
                tile_composition,
                forage,
                flora,
                output_multiplier,
            )),
            plant_food_only(managed_per_worker_yield(
                patch,
                tile_composition,
                forage,
                equipped_gather_rate,
                flora,
                output_multiplier,
            )),
            // Plants never quantise — you harvest grain by the handful (slice 8; see
            // `SourceYieldForecast::body_mass_yield`). The whole-animal rule is animal-only because
            // *the products differ*, not by omission.
            PLANTS_DO_NOT_QUANTISE,
        );
    }
    // The patch's IN-EFFECT conversion rate — the same one `forage_take` pays with, so every ceiling
    // the forecast composes is the number the sim will hand over.
    let rate = patch_provisions_per_biomass(patch, tile_composition, flora, forage);
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
        // `forage_take` computes, at any floor the player's dial can name.
        biomass: patch.biomass,
        carrying_capacity: patch.carrying_capacity,
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
        // `tended_msy_take` and the crop picker prices its material quote on that directly
        // (`commit_material_payoff`), so nothing reads these here — and a patch offers no `Tame`
        // rung at all. Stated as the "no such rung" zero rather than a measurement.
        managed_yield_biomass: crate::fauna::NO_INVESTMENT_RUNG_BIOMASS,
        pastoral_yield_biomass: crate::fauna::NO_INVESTMENT_RUNG_BIOMASS,
    }
}

/// **What one worker can carry home from a MANAGED plant source** (a Field), in provisions/turn — the
/// gather throughput `forage_per_worker_biomass` gives, at the seasonless weight, through the gather
/// conversion.
///
/// This is the **collection** half of production-vs-collection (slice 7): rung 3 collapses the *policy*
/// axis (the crop is yours; there is no wild stock to over-skim) but **not** the worker cap — you
/// still have to carry the harvest home, so a Field's actual take is
/// `min(field_provisions, workers × this)` and the surplus it offered beyond that is wasted. Deliberately
/// **not** a new lever: it is the same `per_worker_biomass_capacity` a wild gather is capped by, which
/// is what keeps "a worker can carry X" one number for the whole plant web.
pub(crate) fn managed_per_worker_yield(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON),
        rung_provisions_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantField),
        output_multiplier,
    )
}

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
    equipped_gather_rate: f32,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
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
        // Population: a Field is a managed harvest (no drawdown, policy axis collapsed, worker-capped);
        // every other patch is the drawn-down policy gather through the shared `forage_take` path.
        let take = if sim.is_field() {
            let production =
                field_provisions(&sim, tile_composition, forage, flora, output_multiplier);
            let collection = workers as f32
                * managed_per_worker_yield(
                    &sim,
                    tile_composition,
                    forage,
                    equipped_gather_rate,
                    flora,
                    output_multiplier,
                );
            production.min(collection)
        } else {
            forage_take(
                &mut sim,
                tile_composition,
                workers,
                floor,
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
    equipped_gather_rate: f32,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
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
        // Population: the same branch `project_realized_forage` and the real Forage arm both take — a
        // Field is a managed harvest (no drawdown, policy axis collapsed, worker-capped); every other
        // patch is the drawn-down policy gather through the shared `forage_take` path.
        *slot = if sim.is_field() {
            let production =
                field_provisions(&sim, tile_composition, forage, flora, output_multiplier);
            let collection = workers as f32
                * managed_per_worker_yield(
                    &sim,
                    tile_composition,
                    forage,
                    equipped_gather_rate,
                    flora,
                    output_multiplier,
                );
            production.min(collection)
        } else {
            forage_take(
                &mut sim,
                tile_composition,
                workers,
                floor,
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
    equipped_gather_rate: f32,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
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
        equipped_gather_rate,
        forage_per_worker_biomass(per_worker_biomass_capacity, seasonal),
        output_multiplier,
    );
    // The patch's OWN MSY (`patch_ecology`) — a tended patch's sustainable line sits on its boosted
    // curve, so a Sustain gather of it reads no ⚠ while a Surplus gather of it does. Reading
    // `forage.ecology` here would flag every tended Sustain as an overdraw.
    let sustainable = forage_provisions(
        sustainable_yield(
            patch.biomass,
            patch.carrying_capacity,
            &patch_ecology(patch, forage),
        ),
        patch_provisions_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    );
    // The steady headline is the forward projection from THIS patch state — the same computation the
    // resolved Forage arm runs, so seed == first resolved value exactly.
    let realized = project_realized_forage(
        patch,
        tile_composition,
        forage,
        flora,
        equipped_gather_rate,
        per_worker_biomass_capacity,
        seasonal,
        output_multiplier,
        workers,
        floor,
        realized_horizon,
    );
    // The discrete twin, from the same patch state: what lands on each of the next
    // `arrivals_horizon` turns. A gather is continuous, so this is normally positive throughout.
    let arrivals = project_arrivals_forage(
        patch,
        tile_composition,
        forage,
        flora,
        equipped_gather_rate,
        per_worker_biomass_capacity,
        seasonal,
        output_multiplier,
        workers,
        floor,
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
    /// cost, so the arithmetic reads in the units the config states.
    const CULTIVATE_COST: f32 = 50.0;

    #[test]
    fn cultivation_accrual_is_owner_locked_and_clamped() {
        let mut patch = ForagePatch::new(UVec2::new(1, 1), 120.0);
        // First accrual claims ownership for the acting faction, and stamps the job's cost.
        patch.accrue_cultivation(FactionId(0), 15.0, CULTIVATE_COST, CULTIVATE_COST);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.cultivation_progress - 15.0).abs() < 1e-6);
        assert_eq!(patch.cultivation_cost, CULTIVATE_COST);
        // A different faction cannot accrue on an already-owned patch.
        patch.accrue_cultivation(FactionId(1), 25.0, CULTIVATE_COST, CULTIVATE_COST);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.cultivation_progress - 15.0).abs() < 1e-6);
        // Owner accrues; progress clamps at the job's cost and latches cultivated.
        patch.accrue_cultivation(FactionId(0), 45.0, CULTIVATE_COST, CULTIVATE_COST);
        assert!(patch.is_cultivated());
        assert_eq!(patch.cultivation_progress, CULTIVATE_COST);
        // A cultivated patch is a no-op for further accrual — including for its stamped cost, so a
        // later retune cannot un-cultivate ground the player has already paid for.
        patch.accrue_cultivation(
            FactionId(0),
            25.0,
            CULTIVATE_COST * 2.0,
            CULTIVATE_COST * 2.0,
        );
        assert_eq!(patch.cultivation_progress, CULTIVATE_COST);
        assert_eq!(patch.cultivation_cost, CULTIVATE_COST);
        assert!(patch.is_cultivated());
    }

    /// **An unstarted patch is not a finished one**, and `progress >= cost` alone would say it was:
    /// a wild stand carries `0` in both fields. The cost being positive is the predicate's other half.
    #[test]
    fn a_wild_patch_is_not_cultivated_even_though_both_meters_read_zero() {
        let patch = ForagePatch::new(UVec2::new(7, 7), 120.0);
        assert_eq!(patch.cultivation_progress, RUNG_UNSTARTED);
        assert_eq!(patch.cultivation_cost, RUNG_UNSTARTED);
        assert!(!patch.is_cultivated());
        assert!(!patch.is_field());
        assert!(!patch.is_managed());
    }

    #[test]
    fn cultivation_decay_clears_owner_at_zero_and_takes_cultivated_feral() {
        let mut patch = ForagePatch::new(UVec2::new(2, 2), 120.0);
        patch.accrue_cultivation(FactionId(0), 2.5, CULTIVATE_COST, CULTIVATE_COST);
        patch.decay_cultivation(1.0);
        assert!((patch.cultivation_progress - 1.5).abs() < 1e-6);
        assert_eq!(patch.owner, Some(FactionId(0)), "owner held above zero");
        // Decaying to zero clears ownership so another faction can later tend it — and the stamped
        // cost with it, since the ground is unstarted again.
        patch.decay_cultivation(CULTIVATE_COST);
        assert_eq!(patch.cultivation_progress, RUNG_UNSTARTED);
        assert_eq!(patch.cultivation_cost, RUNG_UNSTARTED);
        assert_eq!(patch.owner, None);
        // Rung 1a: a cultivated patch now DOES decay when decayed (an untended tended patch goes
        // feral) — it reverts to wild the moment progress drops below its own cost.
        patch.accrue_cultivation(FactionId(1), CULTIVATE_COST, CULTIVATE_COST, CULTIVATE_COST);
        assert!(patch.is_cultivated());
        patch.decay_cultivation(CULTIVATE_COST * 0.5);
        assert!(
            !patch.is_cultivated(),
            "an untended tended patch reverts to wild"
        );
        assert!((patch.cultivation_progress - CULTIVATE_COST * 0.5).abs() < 1e-4);
    }

    /// **The commitment is recorded once and released only by going fully feral** (Flora Roster S1).
    /// Re-deciding which crop a patch is every turn would erase the decision the rung exists to
    /// make; keeping it after both meters lapse would leave a wild stand wearing one plant's name.
    #[test]
    fn a_species_commitment_is_one_way_and_lapses_only_when_the_patch_goes_fully_feral() {
        let mut patch = ForagePatch::new(UVec2::new(3, 4), 120.0);
        patch.commit_species("wild_emmer");
        assert_eq!(patch.species.as_deref(), Some("wild_emmer"));
        // One-way while the ground is committed: a later assignment cannot re-crop it for free.
        patch.commit_species("wild_tubers");
        assert_eq!(patch.species.as_deref(), Some("wild_emmer"));

        // A patch with *either* meter still standing keeps its crop...
        patch.complete_cultivation(FactionId(0));
        patch.complete_field(FactionId(0));
        patch.decay_field(FABRICATED_BUILD_COST);
        assert_eq!(
            patch.species.as_deref(),
            Some("wild_emmer"),
            "a lapsed Field over a standing tended patch is still that crop"
        );
        // ...and lapses only when nothing is left of either.
        patch.decay_cultivation(FABRICATED_BUILD_COST);
        assert_eq!(patch.cultivation_progress, RUNG_UNSTARTED);
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
            let demand = rung.upkeep_demand(UNSCALED_UPKEEP);
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
            .upkeep_demand(UNSCALED_UPKEEP);

        let mut patch = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));
        patch.cultivation_cost = cost;

        // --- Half-built: billed to the pool, and asking for the same keepers. -------------------
        patch.cultivation_progress = cost / 2.0;
        assert_eq!(
            patch_upkeep_workers_needed(&patch, &ladder),
            demand.ceil() as u32,
            "hands to meet the rate, the same count on both sides of completion"
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
            "the verb names the meter, so a Sow's first turn is credited to the Field it starts"
        );

        // --- Finished: the same supplier and the same count. ------------------------------------
        // **The rung is EARNED at the full cost and HELD down to its retention bar**, so a fixture
        // that fills the meter by hand has to stamp the bar the completion would have
        // (`RungDef::retention_bar`) — the achieved state and the meter's fullness are two facts.
        patch.cultivation_progress = cost;
        patch.cultivation_retain_bar = ladder.rung(RungKey::PlantTended).retention_bar(cost);
        assert_eq!(
            patch_upkeep_workers_needed(&patch, &ladder),
            ladder
                .rung(RungKey::PlantTended)
                .upkeep_crew_needed(UNSCALED_UPKEEP),
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
        patch.cultivation_progress = cost * 0.99;
        assert_eq!(
            patch_upkeep_supply(&patch, None, A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "a dipped rung does not stop being the keeping pool's when it starts needing it"
        );
        assert!(
            patch.is_cultivated(),
            "fixture: …and it is still tended — the retention bar is the separate axis"
        );

        // --- A crew doing something else no longer withholds the keeping. -----------------------
        // The meter answered for is the newest (the half-sown Field); the pool holds it whether or
        // not the hands on the row are the ones filling it.
        let mut half_sown = ForagePatch::new(UVec2::new(3, 3), forage.capacity_for(TEST_BIOME));
        let field_cost = ladder
            .rung(RungKey::PlantField)
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the field rung builds");
        half_sown.cultivation_cost = cost;
        half_sown.cultivation_progress = cost;
        half_sown.field_cost = field_cost;
        half_sown.field_progress = field_cost / 2.0;
        assert_eq!(
            patch_upkeep_supply(&half_sown, Some(Improvement::Cultivate), A_KEEPERS_TURN),
            A_KEEPERS_TURN,
            "the pool holds the Field, whatever the crew standing on the tile is doing"
        );

        // --- A wild patch: nothing built, nothing owed, nobody wanted. -------------------------
        let wild = ForagePatch::new(UVec2::new(2, 2), forage.capacity_for(TEST_BIOME));
        assert_eq!(patch_upkeep_demand(&wild, &ladder), NO_UPKEEP_DEMAND);
        assert_eq!(
            patch_upkeep_supply(&wild, None, A_KEEPERS_TURN),
            NO_UPKEEP_DEMAND,
            "ground with nothing on it cannot be billed, so nothing can be short"
        );
        assert_eq!(
            patch_upkeep_workers_needed(&wild, &ladder),
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
        // **THE SAME RATE A HELD RUNG OWES** — the demand does not move with the meter's fullness,
        // and neither does the pool that pays it (`docs/plan_standing_upkeep.md` §4.6a). There is no
        // second demand, which is why the retired `meter_raising_demand` had to go.
        let demand = rung.upkeep_demand(UNSCALED_UPKEEP);

        let mut patch = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));
        patch.cultivation_cost = cost;
        patch.cultivation_progress = cost / 2.0;

        let short_at = |keeping_share: f32| -> f32 {
            let supplied = patch_upkeep_supply(&patch, Some(Improvement::Cultivate), keeping_share);
            crate::intensification::upkeep_shortfall(patch_upkeep_demand(&patch, &ladder), supplied)
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
        let demand = tended_rung.upkeep_demand(UNSCALED_UPKEEP);
        let cost = tended_rung
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        assert!(demand > 0.0, "the tended rung costs something to hold");

        // Kept every turn → the shortfall is zero, so nothing bleeds however long you wait.
        let mut kept = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));
        kept.accrue_cultivation(FactionId(0), cost, cost, cost);
        for _ in 0..200 {
            kept.decay_cultivation(crate::intensification::upkeep_shortfall(demand, demand));
        }
        assert!(kept.is_cultivated(), "a kept patch never decays");
        assert_eq!(kept.owner, Some(FactionId(0)));

        // Nobody keeping it → the whole demand is unmet, and that is the bleed.
        let unkept_bleed = crate::intensification::upkeep_shortfall(demand, NO_UPKEEP_DEMAND);
        let mut feral = ForagePatch::new(UVec2::new(2, 2), forage.capacity_for(TEST_BIOME));
        feral.accrue_cultivation(FactionId(0), cost, cost, cost);
        feral.decay_cultivation(unkept_bleed);
        assert!(
            !feral.is_cultivated(),
            "one bleeding turn reverts a farm to wild"
        );
        // Over ~cost/demand further turns it fully decays and clears ownership.
        let turns_to_zero = (cost / unkept_bleed).ceil() as usize + 2;
        for _ in 0..turns_to_zero {
            feral.decay_cultivation(unkept_bleed);
        }
        assert_eq!(
            feral.cultivation_progress, RUNG_UNSTARTED,
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
        let demand = rung.upkeep_demand(UNSCALED_UPKEEP);
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
        let mut registry = ForageRegistry::default();
        let mut a = ForagePatch::new(UVec2::new(0, 0), 120.0);
        a.complete_cultivation(FactionId(0));
        let mut b = ForagePatch::new(UVec2::new(1, 0), 120.0);
        b.complete_cultivation(FactionId(1));
        let uncultivated = ForagePatch::new(UVec2::new(2, 0), 120.0);
        registry.patches.insert(a.tile, a);
        registry.patches.insert(b.tile, b);
        registry.patches.insert(uncultivated.tile, uncultivated);
        assert_eq!(registry.cultivated_count(FactionId(0)), 1);
        assert_eq!(registry.cultivated_count(FactionId(1)), 1);
        assert_eq!(registry.cultivated_count(FactionId(2)), 0);
    }
}
