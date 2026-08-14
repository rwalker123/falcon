//! **The intensification ladder** — one grammar for both food webs
//! (`docs/plan_intensification_ladder.md`, authoritative spec: `core_sim/CLAUDE.md` → "The
//! Intensification Ladder").
//!
//! Plants and animals climb the *same* three-rung ladder — rung 1 you take what's there, rung 2 you
//! manage the wild source in place, rung 3 you control its reproduction — and every rung-transition
//! is the same **Cultivate-shaped verb**: pick it → the source pays a *reduced* yield while the crew
//! prepares rather than harvests → a **per-source build meter** climbs → it decays if you walk away →
//! at the job's declared cost the source steps up a rung.
//!
//! **AN IMPROVEMENT COSTS WORK, NOT TURNS** (`docs/plan_unit_costed_work.md`). A rung declares a fixed
//! [`RungBuild::work_cost`] in work units; a crew produces [`PER_WORKER_OUTPUT`] per worker per turn,
//! scaled by the floor it holds and the kit it brought; **turns are the output**. That is what lets a
//! rung up the ladder be a *bigger job* than the one below it, and what makes a build finish sooner as
//! the faction improves.
//!
//! **THE PLAYER SPLITS THE BAND, and the sim splits nothing** (`docs/plan_standing_upkeep.md` §2.2).
//! A source carries a **take** crew and a **build** crew, each a number the player typed, and the
//! keeping is a band-level pool beside them; all three draw on one finite band, so the competition
//! between them is visible in the allocation rather than derived from a fraction. That is what
//! **dissolved the investment dip** — `yield_fraction_while_building` said *"this crew is preparing
//! ground, not gathering"*, which is true of a **shared** crew and of nothing else.
//!
//! **The build's own output is net of nothing.** The rung's standing [`RungUpkeep`] is owed every
//! turn, while building and while holding alike — but it is owed by the band's **keeping pool**, for
//! every meter carrying work at any fullness (`docs/plan_standing_upkeep.md` §4.6a), so a build
//! crew's whole output is progress and the pace is `work_cost / crew`. What can still eat a build is
//! the **rot**: what the keeping failed to cover, bleeding off the very meter the builders are
//! raising ([`RungDef::meter_rot`]).
//!
//! This module is the **data + the seam**, not a second copy of the rules:
//! - [`LadderConfig`] (`data/intensification_ladder.json`) holds one [`RungDef`] record per rung —
//!   the links (verb, unlock/earns knowledge, previous rung, husbandry ceiling) and the build dials.
//!   Adding a rung that recombines existing primitives is a one-record edit.
//! - [`RungDef::build_accrual`] / [`RungDef::build_cost`] / [`RungDef::build_decay`] are **the**
//!   build seam, and [`RungDef::upkeep_demand`] / [`RungDef::upkeep_decay`] the **standing-cost**
//!   one beside it. Both tracks call them instead of reaching for their own bespoke
//!   accrue/cost/decay levers, so the plant and animal ladders can never drift apart numerically.
//!   The per-source *state* stays where it lives (`ForagePatch::cultivation_progress`,
//!   `Herd::domestication_progress`, `Herd::corral_progress`, each beside a stored companion **cost**)
//!   — the engine supplies the amounts, the source owns its meter and the side-effects of completing
//!   it (ownership, `corralled_at`, …).
//! - [`knows`] is the one knowledge gate. It retires the inlined
//!   `ledger.get_progress(faction, ID) >= threshold` checks that used to sit in the labor arms and
//!   the command handlers.
//!
//! **The config describes what the sim does TODAY**, deliberately — a later slice changes behaviour
//! by *editing the JSON*, which is the whole point of extracting it. Slice 3a proved that: giving
//! animal `pastoral` its `tame` verb + `herding` gate + build dials was (on the config side) a
//! one-record edit. The engine simply **does not drive** a rung with no verb — which is all the
//! `wild` rungs are now.
//!
//! **Behavior primitives are parsed and validated, but nothing reads them yet** ([`RungBehavior`]).
//! They are the bounded coded set §5 calls for: a future rung that recombines them is pure config; a
//! rung needing a *new* primitive codes that one primitive once, after which it too is config.

use std::{
    collections::{BTreeMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};
use crate::{
    components::Improvement,
    fauna::{FODDERING_DISCOVERY_ID, HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID},
    fauna_config::HusbandryCeiling,
    forage::{CULTIVATION_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID},
    labor_config::NO_FORAGE_CAPACITY,
    orders::FactionId,
    resources::DiscoveryProgressLedger,
    scalar::scalar_from_f32,
};

pub const BUILTIN_INTENSIFICATION_LADDER: &str = include_str!("data/intensification_ladder.json");

/// **WHAT ONE WORKER PRODUCES IN ONE TURN** — the unit every `work_cost` on the ladder is quoted in
/// (`docs/plan_unit_costed_work.md` §1.1): *one worker-turn at the food peak, with no gear*. So a
/// `work_cost` of `50` reads itself — "fifty worker-turns at the food peak" — and the config needs no
/// second dial to be interpreted.
///
/// **It is deliberately NOT a config lever.** A tunable worker output would give two authorities over
/// the same pacing, and the cost side is the one this arc exists to expose: a rung becomes a bigger
/// job by *declaring a bigger job*, never by making the crew worse at it.
pub const PER_WORKER_OUTPUT: f32 = 1.0;

/// **An untouched build meter** — zero work units banked. Named because the distinction *"has any
/// progress been banked here at all"* is a real question with real consequences (ownership is set on
/// the first accrual, and the plant web unwinds *newest rung first*, so
/// `forage::patch_unwinding_rung` asks it of both meters), so the threshold deserves one home rather
/// than a bare literal at each site that asks.
///
/// It is also the **unstamped** value of a meter's companion cost field: a source with cost
/// [`RUNG_UNSTARTED`] has never been worked at this rung, which is why every completion predicate
/// asks `cost > RUNG_UNSTARTED && progress >= cost` rather than `progress >= cost` alone (`0 >= 0`
/// would read every wild source as finished).
pub const RUNG_UNSTARTED: f32 = 0.0;

/// **The cost of a source a FIXTURE fabricated as already finished** — one worker-turn, the smallest
/// job that can be complete.
///
/// A test that wants a tamed herd or a tended patch runs the **real** accrual against this nominal
/// job (`Herd::tame_outright`, `ForagePatch::complete_cultivation`) rather than writing the meter, so
/// the husbandry ceiling and the owner-lock still hold — you cannot fabricate a domesticated `wild`
/// herd. The job's *size* cannot affect any predicate (they all read `progress >= cost`), and stating
/// it as one worker-turn keeps the fabricated state readable — *one hand, one turn* — instead of
/// pretending to a price the ladder never quoted.
pub const FABRICATED_BUILD_COST: f32 = PER_WORKER_OUTPUT;

/// **The grace of a rung there is nothing to neglect on** — a source standing on a rung with no
/// build meter (either `wild` rung). Zero rather than "infinite" because it is never *used* as a
/// grace: both webs consult [`RungDef::neglect_grace_turns`] only for a rung whose meter or flock is
/// actually at risk, and a value that silently forgave everything would be the more dangerous default
/// if that ever stopped being true.
pub const NO_NEGLECT_GRACE: u32 = 0;

// **RETIRED: `source_crew_needed(standing_crew, take_workers)`** — `max(standing, take)`, the one
// blended head count a source published.
//
// It existed because one crew did every job on a source, so *"how many people does this want"* had
// to reconcile a herding count, a hauling count and a build's staffing floor into a single number,
// and reporting only one half made the UI contradict itself (`workersNeeded: 1` beside
// `wastedYield: 0.80` — *drop workers* and *add workers*, on the same row).
//
// **The player states each activity's crew now** (`docs/plan_standing_upkeep.md` §2.2), so the
// question is asked per activity and each answer is in its own unit: hands to meet the upkeep
// (`ceil(upkeep_demand / PER_WORKER_OUTPUT)`), hands to haul the offer (`fauna::hunt_take_workers` /
// `workers_needed_for_take`). A `max` across units was always the compromise a single allocation
// forced.

/// **How many more turns of neglect this source can absorb before the penalty bites** —
/// `(grace_turns + 1) - neglect_turns`, floored at zero, and THE one place that arithmetic lives so
/// the two webs (and the wire) cannot disagree about what a grace means.
///
/// The `+ 1` is what makes the published number readable without any client-side subtraction: the
/// penalty applies while `neglect_turns > grace_turns`, so at `neglect_turns == grace_turns` the very
/// next un-worked turn bites and this reads **`1`** ("one turn left"), while **`0`** means the penalty
/// is biting *now*. A source being worked reads `grace_turns + 1` — *"walk away and you have this
/// long"*, which is a true and useful reading rather than a state that has to be special-cased.
pub fn neglect_grace_remaining(neglect_turns: u16, grace_turns: u32) -> u32 {
    (grace_turns + 1).saturating_sub(u32::from(neglect_turns))
}

/// **A source nobody has neglected** — the reset value of `ForagePatch::neglect_turns` /
/// `Herd::neglect_turns`, written every turn the source's upkeep requirement is met (a crew worked
/// the patch; the herd's keepers can hold its animals).
pub const NEGLECT_NONE: u16 = 0;

/// **HOW FAST A CREW WORKING A SOURCE AT `floor` LEARNS AND BUILDS** — `floor / MSY_BIOMASS_FRACTION`,
/// normalised so the **food peak is ×1.0** (`docs/plan_harvest_floor.md` §3).
///
/// One rate replaced three gates that were each wrong differently. Restraint is no longer a
/// *predicate* — "does this teach?" — but a *rate*: a crew that leaves more standing learns and builds
/// faster, in proportion. The normalisation is what makes that free of a balance reset: `0.5` is the
/// floor a fresh assignment gets and the one a player is most likely to hold, so today's 25-turn
/// Cultivate is still 25 turns there.
///
/// # THE SHAPE IS A SEAM, and that is why it is a function
///
/// Linear is what the plan specifies and the standing answer to its §10 Q1. The alternative — a
/// **knee**, so knowledge reads as a *commitment* rather than a dividend: little below the food peak,
/// steep above — is a change to **this function and nothing else**. Neither call site knows the shape.
///
/// # IT IS NOT A TIMESCALE — it scales ACCRUAL ONLY
///
/// [`RungDef::build_accrual`] and [`RungDef::build_decay`] deliberately share a `timescale` factor, so
/// the reflex here is to scale both. **Do not.** Decay happens on turns *nobody works the source* —
/// there is no assignment in that state, so there is no floor. Multiplying decay by
/// `learn_multiplier` would be scaling by a number that does not exist where the decay is applied,
/// and the caller would have to invent one (whose? the last crew's? the default?). The floor scales
/// what a working crew earns; neglect is not a rate the crew set.
///
/// # BOTH ENDS ARE NON-DEGENERATE, and the top end is deliberate
///
/// - `floor = 0` strips the source and returns `0`: **stripping teaches nothing.**
/// - `floor = 1.0` leaves the whole source standing, so nothing is above the floor and the caller's
///   `eligible` (`systems::labor::crew_is_working_the_source`) is false: **watching teaches
///   nothing.**
///
/// A floor just *under* `1.0` on a full source therefore learns at nearly **×2 while taking almost
/// nothing** — every calorie given up for maximum learning. That is the trade this dial exists to
/// offer, taken to its limit, and it **self-limits**: the source has to actually stand above the
/// floor for a take to exist at all, so the herd or patch must already be near capacity. It is not a
/// defect and must not be "fixed" into a clamp.
pub fn learn_multiplier(floor: f32) -> f32 {
    (floor / crate::fauna::MSY_BIOMASS_FRACTION).max(0.0)
}

/// **WHAT ONE WORKER BANKS ON A BUILD IN ONE TURN AT THE FOOD PEAK**, before the floor and the kit
/// scale it — the per-worker half of [`crew_work_output`], and **the sum of terms** the model is
/// written as (`docs/plan_unit_costed_work.md` §5).
///
/// **Exactly one term today**, and that shape is deliberate rather than premature: §5 rules that
/// knowledge does **not** feed throughput (it reaches it through the tools it unlocks, which §6
/// prices), so there is nothing to add here yet — but a future buff mechanic needs a place to land
/// that is not a re-inversion of the model.
///
/// **It is a function rather than [`PER_WORKER_OUTPUT`] read directly, because it is PUBLISHED**
/// (`ForagePatchState.buildWorkPerWorkerTurn` / `HerdTelemetryState.buildWorkPerWorkerTurn`). The
/// compose sheet evaluates the turn estimate against a crew the player is *proposing*, so it needs
/// this term rather than the sim's answer for the committed crew — and the day a second term lands
/// here, the client tracks it with no change of its own.
pub fn build_work_per_worker_turn() -> f32 {
    PER_WORKER_OUTPUT
}

/// **WHAT THE CREW'S TOOLS TAKE OFF THE JOB** — `Σ over the crew`, which
/// [`crate::equipment_config::KitCoverage::weighted_rate`] × head count already is
/// (`docs/plan_unit_costed_work.md` §6.2).
///
/// **The tool is WIELDED**: a worker holding one contributes its worth, a worker without one
/// contributes nothing, so this is a **sum and not an average** — an un-geared hand adds zero rather
/// than diluting, which is the pathology the retired `BuildRate` multiplier had when averaged
/// (*bring fewer keepers and the pen goes up faster*). `per_worker` is
/// [`crate::equipment_config::EquipmentConfig::build_work_per_worker`] resolved through the crew's
/// coverage, so a partly-equipped party gets exactly the share it carries.
///
/// **`workers` IS THE BUILD'S OWN CREW**, never the band's crew on the source
/// (`docs/plan_standing_upkeep.md` §2.2). `per_worker` is a **rate per worker**, so the count it
/// multiplies has to be the workers actually doing the job — and since
/// [`LadderConfig::effective_build_cost`] is deliberately unfloored, pairing a band-wide count with a
/// one-hand build would let a single builder beside a large gathering crew pay a whole job off
/// outright. That became reachable the moment the two crews split.
///
/// Floored at [`NO_BUILD_GEAR`] so a config that somehow named a negative contribution cannot *add*
/// work to a job — `EquipmentConfig::validate` rejects one, and this is the arithmetic's own guard.
pub fn build_work_from_gear(per_worker: f32, workers: u32) -> f32 {
    (workers as f32 * per_worker).max(NO_BUILD_GEAR)
}

/// **THE `0..1` FRACTION THE WIRE PUBLISHES** — `done / cost`, so the meter can store absolute work
/// units while `cultivationProgress` / `fieldProgress` / `corralProgress` / `domestication` keep the
/// type, meaning and range every shipped readout already renders. **The sim divides at capture**; the
/// client does no arithmetic (`docs/plan_unit_costed_work.md` §8).
///
/// **It divides by the SOURCE'S OWN stamped cost, not the ladder's live one**, which is what makes a
/// finished source read exactly `1.0` beside an `is_cultivated()` that is already `true` — a later
/// retune moves the *price* on the wire (`workCost`) without contradicting the rung the player has
/// already paid for. `RUNG_UNSTARTED` on a source nobody has started, where the ratio is `0/0`.
pub fn build_fraction(done: f32, cost: f32) -> f32 {
    if cost <= RUNG_UNSTARTED {
        return RUNG_UNSTARTED;
    }
    (done / cost).clamp(0.0, 1.0)
}

/// **HOW MANY TURNS A JOB NEEDS AT A STATED CREW, FLOOR AND KIT** — `ceil((cost − done) /
/// work_this_turn)`, and THE one place that arithmetic lives so the wire's `buildTurnsRemaining`
/// cannot drift from the meter it describes.
///
/// Two callers, one expression. The **live** one passes the crew that just worked the source and its
/// meter (the build in flight); the **projection** ([`LadderConfig::projected_build_turns`]) passes
/// the same crew against the rung it would climb next, which is what lets the compose sheet quote a
/// job before the player commits to it.
///
/// `None` = **no estimate**, and it means exactly ONE thing, which the wire renders as
/// [`NO_BUILD_TURNS_ESTIMATE`]: the crew produced nothing this turn, so the build is **stalled** — a
/// stall has no finite answer, and quoting a huge one would read as a promise. (The callers add the
/// other no-answer cases before they ever reach here: no crew on the source, the top of the ladder,
/// a gate that refuses.)
///
/// **A bar the meter is already at or past is [`BUILD_FINISHES_IN_ONE_TURN`], not "no answer"**, and
/// the two states that reach it are the same sentence: the work is already banked, or the crew's
/// **gear pays the job off outright** ([`LadderConfig::effective_build_cost`] is unfloored, so a
/// well-equipped crew drives the bar to or below zero). Both *"finish on the first worked turn"*
/// (`docs/plan_unit_costed_work.md` §6.2), and answering `-1` there broke the arc's own headline
/// claim at exactly the crew size that demonstrates it — the estimate fell 25 → 13 → 4 → 2 → *no
/// estimate* as hands were added.
///
/// **The sim answers it because the client cannot**: the client holds neither the crew's output, nor
/// the floor multiplier, nor the kit's build rate — the same division of labour as `penFeedUpkeep`
/// and the yield forecast.
pub fn build_turns_remaining(cost: f32, done: f32, work_this_turn: f32) -> Option<u32> {
    if work_this_turn <= 0.0 {
        return None;
    }
    let remaining = cost - done;
    if remaining <= 0.0 {
        return Some(BUILD_FINISHES_IN_ONE_TURN);
    }
    Some((remaining / work_this_turn).ceil() as u32)
}

/// **WHAT A SOURCE'S BUILD COUNTDOWN SAYS — FOUR ANSWERS, NOT TWO.**
///
/// [`build_turns_remaining`] is the arithmetic and answers `Option<u32>`; this is what the *source*
/// stores and the wire publishes, because *"there is no answer"*, *"the meter is standing still"*
/// and *"the meter is going backwards"* are different facts and were being collapsed into one
/// sentinel — first all three into `-1`, then the last two into `-2`.
///
/// | this enum | wire | what it means |
/// |---|---|---|
/// | `Some(Turns(n))` | `n` | a real finish date |
/// | `Some(Holding)` | [`sim_schema::BUILD_METER_HOLDS`] | **the meter holds exactly where it is** |
/// | `Some(Rotting)` | [`sim_schema::BUILD_METER_ROTS`] | **the meter is going backwards** |
/// | `None` | [`sim_schema::NO_BUILD_TURNS_ESTIMATE`] | there is genuinely no answer |
///
/// # WHY THE TWO NON-FINISHING STATES ARE NOT ONE
///
/// **THE ROT IS THE DENOMINATOR** (`docs/plan_standing_upkeep.md` §4.6a). A build crew supplies
/// nothing toward the maintenance rate — the keeping pool owes that for every meter carrying work,
/// at any fullness — so what eats a build is the **rot**: what the keeping failed to cover, bleeding
/// off the very meter the builders are raising ([`RungDef::meter_rot`]). Builders raising a meter
/// more slowly than it bleeds are losing work already bought.
///
/// That the two non-finishing states are **actionable and permanent** — standing facts about a
/// staffing the player has already committed — is what separates them from the no-answer state,
/// which is a transient absence of information; folded into one sentinel they rendered as *no line
/// at all* on the tile card and the herd drawer.
///
/// **And "never finishes" is itself two pieces of news**, costing the player differently: exactly at
/// the rot the meter **holds** and the turn is merely wasted; **below** it the decay pass takes back
/// more than the builders banked. *grows / holds / rots* is the vocabulary this enum reuses.
///
/// **Inside the grace the rot is zero**, so a build publishes a real count and then flips to losing
/// when the grace expires. That is what the grace means; it is not smoothed.
///
/// **The animal web cannot reach [`BuildTurns::Rotting`], and that is not an omission**: neither
/// animal rung declares a `meter_decay` (their penalty is the shed), so their rot is always
/// [`NO_UPKEEP_DECAY`] and an animal build with any crew on it publishes a real count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildTurns {
    /// `ceil((cost − done) / (build_work − rot))` at the crew that is on it.
    Turns(u32),
    /// **A build banking exactly what its meter bleeds** — the ground stands still.
    ///
    /// **It is not only a failure state.** With **no builders and the keeping met** the balance is
    /// exactly zero, and that is a player **parking a half-built improvement**: the keeping pool
    /// holds it indefinitely, at no risk, until they come back to it — which is the case
    /// `docs/plan_standing_upkeep.md` §2.4 exists to make possible. A reader that renders this as a
    /// warning is telling the player something is wrong with a decision they made deliberately;
    /// [`BuildTurns::Rotting`] beside it is the one that is unambiguously bad.
    Holding,
    /// **A build banking LESS than its meter bleeds** — the ground is going backwards under it, so
    /// work already bought is being lost. Unambiguously bad, and the remedy is the keeping.
    Rotting,
}

/// **THE COUNTDOWN A SOURCE PUBLISHES** — [`build_turns_remaining`]'s arithmetic, with the two states
/// it cannot see folded in: whether this build has **promised** anything, and which side of the
/// **rot** its balance falls on.
///
/// `balance` is the **signed** twin of the accrual ([`RungDef::build_balance`] = `build_work −
/// rot`), not the raw accrual a meter is handed: a meter may only ever be *added* to, and the rot is
/// the decay pass's to take off the same meter, so the accrual alone maps *holding* and *rotting*
/// onto the same reading.
///
/// # THE BOUNDARY IS WORK BANKED, NOT HANDS ON THE JOB
///
/// | state | answer |
/// |---|---|
/// | no build in flight, or the rung's own gate refuses it (`!gate_holds`) | `None` |
/// | a build in flight, meter at **zero**, no builders | `None` — nobody has promised anything yet |
/// | otherwise | the sign of `balance` |
///
/// **A meter carrying work has promised something — the player paid for it.** The rule used to be
/// *"an unstaffed source reads `None`, because nobody has promised anything"*, which was written when
/// unstaffed meant *nobody has declared anything*. Since §4.6a a half-built meter with nobody on it is
/// exactly *the meter holds* (the keeping covers it) or *the meter is losing ground* (it does not) —
/// which is what the two sentinels mean, so hiding them there hid the states from the case that
/// reaches them **most often on the shipped ladder**: both plant rot rates are below one worker-turn,
/// so a *staffed* plant build always out-runs its own rot.
///
/// `builders` is the crew on the verb, or the crew a projection is being quoted at.
pub fn build_turns_estimate(
    cost: f32,
    done: f32,
    balance: f32,
    gate_holds: bool,
    builders: u32,
) -> Option<BuildTurns> {
    // Work already banked promises as much as a crew does — see the table above.
    let promised = gate_holds && (builders > NO_CREW_ON_THIS_ACTIVITY || done > RUNG_UNSTARTED);
    match build_turns_remaining(cost, done, balance) {
        Some(turns) => Some(BuildTurns::Turns(turns)),
        None if !promised => None,
        None if balance < BUILD_BALANCE_HOLDS => Some(BuildTurns::Rotting),
        None => Some(BuildTurns::Holding),
    }
}

/// **THE BALANCE AT WHICH A METER NEITHER GROWS NOR ROTS** — a crew banking exactly what the meter
/// bleeds, so `build_work − rot` is exactly this.
///
/// It is the boundary [`BuildTurns::Holding`] and [`BuildTurns::Rotting`] are split on, and it is
/// named separately from [`NO_BUILD_PROGRESS`] — which is the same number — because the two are
/// different statements about it: that one is a **floor** on what a meter may add, this one is the
/// **cut point** between two published answers. A reader that borrowed the floor's name here would
/// be asserting the rot case away.
pub const BUILD_BALANCE_HOLDS: f32 = 0.0;

/// **THE ANSWER FOR A BAR A WORKING CREW IS ALREADY AT OR PAST** — one turn, because a build with no
/// work left to do completes on the first turn anybody works it.
///
/// Named rather than a bare `1` at the one site that returns it, so the *reason* travels with the
/// value: this is *"there is nothing left to wait for"*, which is a real answer, and deliberately
/// **not** [`NO_BUILD_TURNS_ESTIMATE`], which means *"there is no answer"*.
pub const BUILD_FINISHES_IN_ONE_TURN: u32 = 1;

/// **The cost multiplier of a source that costs exactly what its rung declares.** Passed by every
/// caller with no per-source multiplier to apply (the plant `tended` patch, the plant `field`, the
/// animal `pen` and its `ExtendPen` rings) — the rung's `work_cost` *is* the price there.
///
/// The multiplier exists because rung 2 of the animal ladder is **not** one-size-fits-all: a species
/// declares its own `taming_cost_multiplier` (`fauna_config`), and the honest statement is that the
/// animal is *more work*, not that the crew is worse at their job. See [`RungDef::build_cost`].
pub const RUNG_COST_UNSCALED: f32 = 1.0;

/// **The work a crew carrying no gear that helps takes off a job** — nothing. The neutral `0.0` a
/// kit resolves to when none of its live items declares
/// [`crate::equipment_config::EquipmentStat::BuildWork`].
///
/// **It is `0.0` and not `1.0`, because the gear is ADDITIVE off the job's cost now**
/// (`docs/plan_unit_costed_work.md` §6): it was the neutral of a *multiplier* on the crew's output,
/// under which a tool saved the same percentage of turns on a garden and on a farm alike — the
/// multiplier problem the arc exists to escape. A neutral read as the wrong one of these is a build
/// either free or unbuildable, which is why the two live on the stat
/// ([`crate::equipment_config::EquipmentStat::neutral`]) rather than at the call sites.
///
/// It is [`crate::equipment_config::EquipmentConfig::build_work_per_worker`]'s answer for every plant
/// build today (no plant item declares the stat yet — issue #539) and for every animal build whose
/// crew went out on a kit without handling gear. Named rather than a bare `0.0` at the call sites
/// that have no band to resolve a kit against — a forecast probe, a test fixture — so *"this crew
/// brought nothing"* reads as a stated fact rather than an unexplained literal.
pub const NO_BUILD_GEAR: f32 = 0.0;

/// **THE REFERENCE JOB every build readout is quoted against** — the `plant:tended` rung, the
/// smallest shipped improvement and the first one a player meets. A work unit means nothing to
/// someone holding a hoe, so `ItemDefinition::headline_wear`'s life gauge reads *"≈12 gardens'
/// worth"* rather than *"625 work units"*; the garden is this rung's own `work_cost`.
///
/// Named here, on the ladder, because it is a **ladder** fact: the readout must move with a retune of
/// the rung rather than carrying its own copy of the number.
pub const REFERENCE_BUILD_RUNG: RungKey = RungKey::PlantTended;

/// **The crew the grace bound measures a build against** — one worker, which is the **longest**
/// that build can take and therefore the loosest the bound can be. A guard that catches a grace
/// swallowing its own build must err toward permitting, not toward rejecting a config nobody could
/// have known was wrong.
pub const SOLE_BUILDER: u32 = 1;

/// **A rung that declares no standing upkeep** — what its `upkeep_demand` answers, and what a crew
/// with no maintenance allocation supplies. On the shipped ladder that is the two **wild** rungs and
/// nothing else: land nobody improved costs nobody anything to hold. All four managed rungs declare
/// an upkeep.
pub const NO_UPKEEP_DEMAND: f32 = 0.0;

/// **A shortfall that costs the meter nothing** — what [`RungDef::upkeep_decay`] answers for a rung
/// with no upkeep, and for one still inside its grace. Named for the same reason
/// [`NO_NEGLECT_GRACE`] is: a bare `0.0` at a decay site reads as *"the meter is fine"* when the
/// question asked was *"how much did neglect cost it"*.
pub const NO_UPKEEP_DECAY: f32 = 0.0;

/// **A SOURCE THAT IS FULLY STAFFED** — the shortfall fraction of a demand that is entirely met, and
/// of a rung that demands nothing at all. Named because [`upkeep_shortfall_fraction`]'s `0` is a
/// *statement* ("these hands are all there") rather than the absence of an answer.
pub const FULLY_SUPPLIED: f32 = 0.0;

/// **A SOURCE NOBODY IS HOLDING** — the shortfall fraction of a demand nothing was supplied against,
/// and therefore the multiplier at which a rung bleeds its whole [`RungMeterDecay::per_turn`].
pub const WHOLLY_UNSUPPLIED: f32 = 1.0;

/// **NOBODY IS ON THIS ACTIVITY** — the crew of an assignment that is building nothing, or keeping
/// nothing. It is the shipped default of both `LaborAssignment::improvement_workers` and
/// `LaborAssignment::maintain_workers`, and it is how a player says *"stop maintaining this"*: there
/// is no toggle, there is a **number**, and zero is the whole of what "off" means.
pub const NO_CREW_ON_THIS_ACTIVITY: u32 = 0;

/// **THE WORK A CREW ON ONE ACTIVITY PRODUCES IN ONE TURN** — `workers × PER_WORKER_OUTPUT`, and the
/// arithmetic that replaced the retired one-pool work budget
/// (`docs/plan_standing_upkeep.md` §2.2).
///
/// # THE PLAYER STATES THE SPLIT — there is nothing to derive
///
/// A source carries a **take** crew and a **build** crew (`assign_labor` and the improvement verb),
/// and its keeping is a share of the band's own pool (§2.5). They draw on the same finite band, so
/// competing for hands is the opportunity cost, and it is **visible in the numbers the player typed**
/// rather than buried in a fraction or in a priority order they cannot see:
///
/// ```text
/// upkeep_supplied  = this source's share of the band's keeping POOL — at ANY meter fullness
/// upkeep_shortfall = max(0, upkeep_demand − upkeep_supplied)   // → the rot, past grace
/// build_work       = build_workers × PER_WORKER_OUTPUT         // − what the crew's gear takes off the job
/// net              = build_work − rot                          // what the COUNTDOWN reads
/// take             = min(take_workers × per_worker_capacity, source_offer)
/// ```
///
/// **There is no cap on any of them**, exactly as there is none on a build's crew: fifty hands may
/// keep a pen and fifty more may widen it, and the constraint is what those hands are not doing
/// elsewhere.
///
/// # What this retired, and why the retirement is a simplification
///
/// The predecessor was one pool per crew-turn with a fixed priority order (upkeep → build →
/// production) and a derived take. It bought the dip's retirement, which **stands** — but it also
/// had to answer *"what share of one crew's turn reaches the take"*, and that question has a
/// degenerate `0/0` answer at a floor of zero (`learn_multiplier(0.0)` is `0`), plus a hard-coded
/// ordering the player could neither see nor state. Three allocations answer it by not asking.
pub fn activity_work(workers: u32) -> f32 {
    workers as f32 * build_work_per_worker_turn()
}

// **RETIRED: `net_build_supply(supply, maintenance_rate)`** — `max(0, supply − rate)`, the
// maintenance rate netted off a build crew's output (`docs/plan_standing_upkeep.md` §4.6a).
//
// **A BUILD CREW SUPPLIES NOTHING TOWARD THE RATE; ITS WHOLE OUTPUT IS PROGRESS.** The rate is owed
// by the band's keeping pool for **every** meter carrying work, at any fullness (§2.4), so there is
// nothing left for a build's accrual to subtract. What eats a build now is the **rot** — what the
// keeping failed to cover, bleeding off the same meter the builders are raising — and that is a
// signed term on [`RungDef::build_balance`] rather than a floor on the accrual.

/// **A BUILD THAT BANKS NOTHING THIS TURN** — the answer for a rung this crew is not building at
/// all: it has nothing to build, its gates refuse, or the verb names another rung. Named because a
/// bare `0.0` at [`RungDef::build_accrual`]'s no-build arm reads as *"the crew produced nothing"*
/// when what it says is *"there is no build here"*.
pub const NO_BUILD_PROGRESS: f32 = 0.0;

/// **NOTHING IS IN PLAY ON THIS RUNG** — [`RungDef::build_balance`]'s answer where the crew is not
/// building this rung at all: it has nothing to build, its gates refuse, or the verb names another
/// rung. Neither growing nor rotting, because there is no build to describe.
///
/// Distinct from [`BUILD_BALANCE_HOLDS`], which is a real crew banking exactly what the meter
/// bleeds. Nothing acts on it — every caller reaching a non-positive balance is already gated by
/// `staffed` — and it is named so that absence cannot be misread as the holding case if one ever
/// does.
pub const NO_BUILD_BALANCE: f32 = 0.0;

/// **WHAT THE RUNG'S STANDING DEMAND LEAVES UNMET**, in work units — `max(0, demand − supplied)`,
/// and therefore exactly what [`RungDef::upkeep_decay`] bleeds off the meter past the grace
/// (`docs/plan_standing_upkeep.md` §2.4).
///
/// Stated here rather than at the two webs' call sites so *"shortfall is the decay"* is one
/// subtraction with one home — the same discipline [`RungDef::build_cost`] and
/// [`RungDef::build_decay`] already follow.
pub fn upkeep_shortfall(demand: f32, supplied: f32) -> f32 {
    (demand - supplied).max(NO_UPKEEP_DEMAND)
}

/// **HOW SHORT YOU ARE, AS A FRACTION OF WHAT WAS ASKED** — `shortfall / demand`, clamped to
/// `0..=1`. [`FULLY_SUPPLIED`] with every hand on it, [`WHOLLY_UNSUPPLIED`] with none.
///
/// # THE DECAY RIDES THIS, NOT THE SHORTFALL ITSELF
///
/// *Shortfall was the decay* welded two questions together — **how much work does holding this
/// want** and **how fast does it rot when you stop** — so raising a demand made the thing rot faster
/// in exact proportion and neither number could be retuned without moving the other. A rung states
/// them separately now: the demand is [`RungUpkeep::work_per_turn`], the rate is
/// [`RungMeterDecay::per_turn`], and *this* is what couples them —
///
/// ```text
/// decay_this_turn = shortfall_fraction × decay_per_turn      // past the grace
/// ```
///
/// **It is the shape the animal web already had.** A herd's shed is `shortfall_in_loads ×
/// animals_per_herder`, which is exactly this fraction times the head count, taken at the species'
/// own escape fraction — so the fraction is stated once here and each web supplies its own rate.
///
/// A demand of nothing is [`FULLY_SUPPLIED`]: a rung with no upkeep is not "wholly unheld", it is
/// unbilled.
pub fn upkeep_shortfall_fraction(demand: f32, supplied: f32) -> f32 {
    if demand <= NO_UPKEEP_DEMAND {
        return FULLY_SUPPLIED;
    }
    (upkeep_shortfall(demand, supplied) / demand).clamp(FULLY_SUPPLIED, WHOLLY_UNSUPPLIED)
}

/// **HOW A BAND SPLITS ITS MAINTENANCE POOL WHEN IT CANNOT COVER EVERYTHING** — a per-band player
/// option (`docs/plan_standing_upkeep.md` §2.5), because the two answers are both defensible and the
/// choice between them is a real one.
///
/// It rides [`crate::components::LaborAllocation`], so it is `SimState` and a checkpoint restores
/// the allocation it produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UpkeepFundMode {
    /// **Everything degrades a little** — each source is funded in proportion to its demand, so a
    /// band at 60% of its total holds every source at 60%. The default, because it is what an
    /// unstated policy means: nobody is singled out.
    #[default]
    Spread,
    /// **Fund sources completely until the pool runs out, most-invested first** — the biggest
    /// investments stay whole and the marginal ones rot. Ordering is the caller's
    /// ([`distribute_upkeep_pool`] funds in slice order) and is total and deterministic, so a
    /// checkpoint restores the same allocation.
    Priority,
}

impl UpkeepFundMode {
    /// Stable config/command/wire token — the [`RungBranch::as_str`] convention.
    pub fn as_str(self) -> &'static str {
        match self {
            UpkeepFundMode::Spread => "spread",
            UpkeepFundMode::Priority => "priority",
        }
    }

    /// Parse a command/wire token. `None` for anything else, which the caller reports rather than
    /// guessing at — the `maintain`-grammar discipline that an unknown token fails loudly.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "spread" => Some(UpkeepFundMode::Spread),
            "priority" => Some(UpkeepFundMode::Priority),
            _ => None,
        }
    }
}

/// **WHAT EACH SOURCE GETS OUT OF THE BAND'S POOL** — one share per demand, in the order given, and
/// **never more than the pool** (`docs/plan_standing_upkeep.md` §2.5).
///
/// # A POOL HAS NO LEFTOVER BY CONSTRUCTION
///
/// The predecessor was a per-source keeper crew, and an indivisible supplier meeting a per-source
/// demand **wastes whatever it does not spend**: a demand of `1.5` staffed by two hands throws half
/// a worker away, and the waste grows as gear makes a hand worth more. One pool against the summed
/// demand cannot waste anything — every unit either meets a demand or is still in the pool.
///
/// **The two modes are the player's** ([`UpkeepFundMode`]):
/// - [`UpkeepFundMode::Spread`] scales every demand by the same `pool / total` coverage.
/// - [`UpkeepFundMode::Priority`] walks the slice in order, paying each demand **in full** until the
///   pool runs out. The **caller owns the order** — most-invested first, tie-broken on a stable
///   per-web key — because "most invested" is a per-web reading (a patch's stamped meter cost, a
///   herd's) and the ladder has no business knowing either. What the ladder owns is that the order it
///   is handed is the order it funds.
pub fn distribute_upkeep_pool(pool: f32, demands: &[f32], mode: UpkeepFundMode) -> Vec<f32> {
    let pool = pool.max(NO_UPKEEP_DEMAND);
    match mode {
        UpkeepFundMode::Spread => {
            let total: f32 = demands
                .iter()
                .map(|demand| demand.max(NO_UPKEEP_DEMAND))
                .sum();
            if total <= NO_UPKEEP_DEMAND {
                return vec![NO_UPKEEP_DEMAND; demands.len()];
            }
            let coverage = (pool / total).min(WHOLLY_UNSUPPLIED);
            demands
                .iter()
                .map(|demand| demand.max(NO_UPKEEP_DEMAND) * coverage)
                .collect()
        }
        UpkeepFundMode::Priority => {
            let mut left = pool;
            demands
                .iter()
                .map(|demand| {
                    let share = demand.max(NO_UPKEEP_DEMAND).min(left);
                    left -= share;
                    share
                })
                .collect()
        }
    }
}

/// Which food web a rung belongs to. The two webs are separate ladders that never share a rung — a
/// master rancher isn't automatically a farmer (`plan_intensification_ladder.md` §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungBranch {
    /// The **human** food web: forage patches (`forage.rs`).
    Plant,
    /// The **animal** food web: herds (`fauna.rs`).
    Animal,
}

impl RungBranch {
    /// Stable key (the JSON `branch` value), used in validation messages.
    pub fn as_str(self) -> &'static str {
        match self {
            RungBranch::Plant => "plant",
            RungBranch::Animal => "animal",
        }
    }
}

/// **The rungs the engine currently knows how to drive.** The ladder is data, but the *code* that
/// reads a specific rung has to name it; this bounded set is what [`LadderConfig::validate`] insists
/// the config actually defines, so [`LadderConfig::rung`] is infallible and a broken override can
/// never silently no-op a shipped rung. Appending a *new* rung record is still free — this list only
/// pins the ones a system reaches for by name today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RungKey {
    /// A wild, depletable forage patch.
    PlantWild,
    /// A **tended** patch — the `Cultivate` investment (`ForagePatch::cultivation_progress`).
    PlantTended,
    /// A **Field** — the `Sow` investment (`ForagePatch::field_progress`), the plant branch's rung 3
    /// and the twin of [`RungKey::AnimalPen`].
    PlantField,
    /// A wild herd.
    AnimalWild,
    /// A **pastoral** (mobile domesticated) herd — the `Tame` investment
    /// (`Herd::domestication_progress`).
    AnimalPastoral,
    /// A **penned** herd — the `Corral` investment (`Herd::corral_progress`).
    AnimalPen,
}

impl RungKey {
    /// Every rung a system names today — what `validate` requires the config to define.
    pub const ALL: [RungKey; 6] = [
        RungKey::PlantWild,
        RungKey::PlantTended,
        RungKey::PlantField,
        RungKey::AnimalWild,
        RungKey::AnimalPastoral,
        RungKey::AnimalPen,
    ];

    pub fn branch(self) -> RungBranch {
        match self {
            RungKey::PlantWild | RungKey::PlantTended | RungKey::PlantField => RungBranch::Plant,
            RungKey::AnimalWild | RungKey::AnimalPastoral | RungKey::AnimalPen => {
                RungBranch::Animal
            }
        }
    }

    /// The record's `id` in `intensification_ladder.json`.
    pub fn id(self) -> &'static str {
        match self {
            RungKey::PlantWild | RungKey::AnimalWild => "wild",
            RungKey::PlantTended => "tended",
            RungKey::PlantField => "field",
            RungKey::AnimalPastoral => "pastoral",
            RungKey::AnimalPen => "pen",
        }
    }

    /// **THE RUNG A VERB BUILDS** — the key half of [`LadderConfig::rung_for`], so a caller that
    /// needs to *name* the rung an improvement climbs (rather than read its dials) does not pair a
    /// verb with a `RungKey` by hand. The completion hand-off asks it: *which rung did this crew just
    /// finish, and does it cost anything to hold?*
    ///
    /// Exhaustive, like [`Improvement::valid_for_forage`](crate::components::Improvement) — a new
    /// verb fails to compile until someone states which rung it builds.
    pub fn built_by(improvement: Improvement) -> Self {
        match improvement {
            Improvement::Cultivate => RungKey::PlantTended,
            Improvement::Sow => RungKey::PlantField,
            Improvement::Tame => RungKey::AnimalPastoral,
            Improvement::Corral => RungKey::AnimalPen,
        }
    }

    // **RETIRED: `upkeep_role()`** — which standing role keeps this rung. Its one caller was the
    // completion hand-off that moved a finished build's crew onto its web's keeping role, and that
    // hand-off is retired (`docs/plan_standing_upkeep.md` §2.3): the keeping bill starts at the
    // **first work banked**, not at completion, so the failure it guarded against — a brand-new
    // improvement decaying on turn one because nobody noticed it had begun costing something —
    // cannot happen. A completed build's crew frees to idle through `set_improvement(None)`.

    /// **THE RUNG DIRECTLY ABOVE THIS ONE** — what a source standing here would climb next. `None` at
    /// the top of a branch: there is nothing left to build, and that is the honest answer rather than
    /// a rung quoted out of the other web.
    ///
    /// It is the seam a **projection** resolves through. [`crate::forage::patch_rung`] /
    /// [`crate::fauna::herd_rung`] answer *where the source stands*; this answers *what it would
    /// climb*, so the wire's `buildTurnsRemaining` can quote an unstarted job without any call site
    /// re-deriving the ladder's order from `is_cultivated()`.
    ///
    /// **Exhaustive, like `Improvement::valid_for_forage`** — a new rung fails to compile until
    /// someone states its place in the climb, rather than defaulting to "nothing above it" and
    /// silently making the rung above unquotable. `ladder_order_matches_the_coded_climb` pins it
    /// against the shipped records' own `order`, so the coded ladder and the config's cannot drift.
    pub fn above(self) -> Option<RungKey> {
        match self {
            RungKey::PlantWild => Some(RungKey::PlantTended),
            RungKey::PlantTended => Some(RungKey::PlantField),
            RungKey::PlantField => None,
            RungKey::AnimalWild => Some(RungKey::AnimalPastoral),
            RungKey::AnimalPastoral => Some(RungKey::AnimalPen),
            RungKey::AnimalPen => None,
        }
    }
}

/// How a source at this rung moves — **the proximity spine, far → near → fixed**
/// (`docs/plan_intensification_ladder.md` §3, dial 4). A bounded coded primitive (§5), and the
/// **first one the engine actually applies**: `fauna::advance_herds` dispatches a herd's movement off
/// the rung it stands on, so a rung that recombines these is pure config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungMovement {
    /// Pinned to one place: a forage patch, a penned herd's fence.
    Fixed,
    /// Roams its own range — you go to it.
    Roam,
    /// **Drifts toward its owner's nearest band** and stays near it: less chasing, and it *reads* as
    /// domesticated. Composes with (never replaces) the roam's own barren-avoidance / graze
    /// preference — see `fauna::advance_herd_roam`. A source with no owner, or an owner with no
    /// bands, simply roams.
    DriftToOwner,
    /// **Steps toward the nearest prey it can eat** — the carnivore transpose of `drift_to_owner`,
    /// over the same shared attractor path (`fauna::relocate_toward_resource`), with clearable prey
    /// positions as the attractor tiles instead of owner camps. Resolved **diet-aware** in
    /// `fauna::advance_herds` (a wild carnivore selects it) rather than assigned from a rung record:
    /// the husbandry rungs are diet-orthogonal (`animal:wild` is one rung shared by a deer and a
    /// wolf), so a carnivore's food-seeking movement can't be a husbandry-rung record today.
    Pursue,
}

/// How a source at this rung feeds itself. A bounded coded primitive (§5) — **not read yet**
/// (`movement` is the only primitive the engine reads today).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungFeeding {
    /// Needs no feed at all — the plant web regrows from the land it stands on.
    Photosynthesis,
    /// Eats the **open** graze layer wherever it roams (`GrazePatch`, `fodder_per_biomass`).
    Forage,
    /// Feeds off its own **fenced footprint**'s graze, the keeper's larder covering the shortfall
    /// (the pen economy, `docs/plan_grazing_2d.md`).
    SelfGraze,
}

/// How the harvest comes off a source at this rung. A bounded coded primitive (§5) — **not read
/// yet**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungHarvest {
    /// Workers **draw the source down**: a wild gather / a wild hunt.
    WorkerTake,
    /// Workers take a **managed** harvest that never overdraws (a tended patch, a pen).
    WorkerTend,
    /// Pays its owner with **no workers at all**. **No shipped rung is `passive` any more** —
    /// `plan_intensification_ladder.md` §3 retired the passive-free pastoral rung in slice 3b (every
    /// rung is worker-driven; intensifying buys *yield per worker*, not zero workers). The variant
    /// survives as vocabulary for a future rung that genuinely pays for nothing.
    Passive,
}

/// The behavior primitives a rung recombines. Bounded enums over coded behavior, per §5 — a rung
/// that recombines existing primitives is pure config. Only **`movement`** is read today (slice 3b —
/// `fauna::advance_herds`); `feeding` / `harvest` are still parsed and validated only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RungBehavior {
    pub movement: RungMovement,
    pub feeding: RungFeeding,
    pub harvest: RungHarvest,
}

/// **What the LAND must be for a rung to be placed on it** — the plant branch's twin of
/// [`RungDef::ceiling_required`], keyed on the ground rather than on the species
/// (`docs/plan_intensification_ladder.md` §2).
///
/// **THE WHOLE PLANT BRANCH IS SITE-BOUND UNTIL RUNG 4.** Rungs 1–3 (gather, Cultivate, Sow) all
/// require a **gathering site** — the curated `FoodSiteRegistry` entry — and differ only in what they
/// add on top: Cultivate adds nothing (it improves the output of a site you already work), Sow adds
/// fresh water (you may move seed, but not water). **Rung 4 (Farm) is the first rung that drops the
/// site requirement**, which is precisely what it is *for*: planting one of the things that grows on
/// fertile ground you are not already gathering from. Rung 5 (Irrigation) then relaxes the water term
/// to "fresh water, or connected to it".
///
/// **Scarcity is the point, not a side effect**: few gathering sites ⇒ *which* tile matters ⇒ a band
/// may have to **move** to eat at all, and the early game's real decision is which site to sit on.
/// That friction is the design pillar the requirement exists to create, so every dial is a lever
/// rather than a constant — and rung 4's identity is a **config edit** to this record, which is the
/// arc's config-driven thesis paying out.
///
/// `None` on a rung that may be built anywhere its source already is — i.e. every animal rung, which
/// needs no such record because a herd carries its own site with it.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RungSiteRequirement {
    /// **The site rule**: the tile must be a curated **gathering site** (`FoodSiteRegistry`), the
    /// ground the player can actually work. `false` on rung 4 and above — that is the whole of what
    /// Farm unlocks.
    ///
    /// It is deliberately NOT "the tile has a food module": a `FoodModuleTag` sits on ~every land
    /// tile, so a module test admits nearly the whole map and would make this rule vacuous.
    pub requires_gathering_site: bool,
    /// **The fertility floor**: the tile's own human-food carrying capacity
    /// (`forage.capacity_by_biome`, via `forage::tile_forage_capacity` — the *same* number that sizes
    /// a wild patch, never a rung-specific table) must reach this for the rung to be placed there.
    /// `0` = no floor.
    ///
    /// **`0` on every rung today.** It carried rung 3's scarcity (a floor of 195, admitting only the
    /// river-deposit class) until the gathering-site rule above took that job: stacking both made Sow
    /// need a curated site that *also* landed on one of three biomes, which is scarcity twice over.
    /// It stays a live dial because rung 4 is where it earns its keep — Farm has no site rule, so
    /// fertility is the only thing standing between it and planting a glacier.
    pub min_forage_capacity: f32,
    /// **The water rule**: the tile must be on or beside **fresh** water — a river along one of its
    /// sides, fresh-water ground, or a lake/channel/marsh next door (`forage::tile_is_fresh_watered`).
    /// A salt coast is **not** water for this purpose; you do not plant a field in the sea spray.
    pub requires_fresh_water: bool,
}

/// **Why the land refuses a rung** — the shape of [`RungSiteRequirement::refusal`], so the *rung*
/// says what is wrong with the ground and the caller only phrases it. The fertility and water
/// failures are real and distinct (rich-but-dry upland vs. watered-but-thin scrub), and a tile can
/// fail both at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteRefusal {
    /// **Nobody gathers here** — the tile is not a curated gathering site, so no plant rung below 4
    /// can stand on it. It **supersedes** the two ground readings rather than joining them: whether
    /// such a tile is also thin or dry is moot while there is no way to work it at all, and a refusal
    /// naming three faults teaches the player two they cannot act on.
    NotGatheringSite,
    /// The ground is watered, but too thin to take a crop without fertilization.
    TooPoor,
    /// The ground is rich, but too dry to farm without irrigation.
    TooDry,
    /// Both.
    TooPoorAndTooDry,
}

impl SiteRefusal {
    /// Stable wire/config key (`ForagePatchState.sowSiteRefusal`) — the `EcologyPhase::as_str`
    /// convention: the live enum stays serde-free and crosses as a string, so a future refusal is a
    /// new key rather than a schema change. **"" is not a variant** — the empty string is the wire's
    /// "this ground takes seed", i.e. `Option::None`.
    pub fn as_str(self) -> &'static str {
        match self {
            SiteRefusal::NotGatheringSite => "not_gathering_site",
            SiteRefusal::TooPoor => "too_poor",
            SiteRefusal::TooDry => "too_dry",
            SiteRefusal::TooPoorAndTooDry => "too_poor_and_too_dry",
        }
    }
}

/// The wire key for **"the land takes seed"** — the empty `sowSiteRefusal`, the `Option::None` of
/// [`SiteRefusal::as_str`]. Named because both the capture and its tests state it, and "" is exactly
/// the kind of literal that means nothing at a call site.
pub const SITE_ACCEPTED: &str = "";

impl RungSiteRequirement {
    /// **THE site seam** — the twin of [`RungDef::build_accrual`]: the rung states the rule, the
    /// caller supplies the two readings it judges and phrases the answer. Pure (no ECS, no config
    /// lookup), so the `sow` command's rejection and the labor arm's placement gate resolve the *same*
    /// rule and can never drift into disagreeing about which ground is farmable.
    ///
    /// `None` = the land permits it.
    ///
    /// **The gathering-site test short-circuits** — see [`SiteRefusal::NotGatheringSite`].
    pub fn refusal(
        &self,
        gathering_site: bool,
        forage_capacity: f32,
        fresh_water: bool,
    ) -> Option<SiteRefusal> {
        if self.requires_gathering_site && !gathering_site {
            return Some(SiteRefusal::NotGatheringSite);
        }
        let too_poor = forage_capacity < self.min_forage_capacity;
        let too_dry = self.requires_fresh_water && !fresh_water;
        match (too_poor, too_dry) {
            (false, false) => None,
            (true, false) => Some(SiteRefusal::TooPoor),
            (false, true) => Some(SiteRefusal::TooDry),
            (true, true) => Some(SiteRefusal::TooPoorAndTooDry),
        }
    }
}

/// The **per-source build meter** dials of one rung: what it costs to climb it, in labor and in
/// forgone yield.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RungBuild {
    /// **WHAT THIS RUNG COSTS, IN WORK UNITS** — the fixed size of the job
    /// (`docs/plan_unit_costed_work.md` §1). One unit is one worker-turn at the food peak with no
    /// gear ([`PER_WORKER_OUTPUT`]), so `50` reads as *"fifty worker-turns"* and the number needs no
    /// second dial to interpret. **Turns are the OUTPUT**: `work_cost / (workers × output)` is how
    /// long it takes, and that falls as the faction puts more hands, a shallower floor or better
    /// tools on the same fixed job.
    ///
    /// It replaced a `progress_per_turn` rate against a normalized `1.0` meter, under which every
    /// improvement on both webs was literally the same 25-turn job and a rung could only become
    /// *bigger* by declaring the crew *worse*. Validated finite and `> 0` — a zero would silently
    /// make the rung free.
    pub work_cost: f32,
    /// **The neglect GRACE — the UN-WORKED-BUILD trigger, which is now the ANIMAL branch's alone.**
    /// How many consecutive turns a source may go un-worked before this rung's neglect penalty
    /// starts. The penalty applies only while the source's `neglect_turns` counter is **strictly
    /// greater** than this, so `0` restores the old no-grace behaviour and `n` forgives exactly `n`
    /// unworked turns. An animal rung sheds animals over its labor capacity
    /// (`fauna::advance_husbandry`); the rung the source stands on owns the number, which is the
    /// point of it being per-rung — *a fence stands for years*, so `animal:pastoral` and `animal:pen`
    /// want different answers.
    ///
    /// **`None` = this rung's neglect is not counted in un-worked turns**, which is the whole plant
    /// branch: a plant rung's penalty is its **upkeep shortfall**
    /// (`docs/plan_standing_upkeep.md` §2.4), so its grace is [`RungUpkeep::grace_turns`] and a
    /// second number here would be a dial nothing reads. Absent states that, exactly as the retired
    /// `decay_fraction_per_turn` did — a parked value reads like a live dial.
    ///
    /// Validated when present: `< work_cost / PER_WORKER_OUTPUT` — the grace may not outlast the
    /// build itself. A longer grace makes neglect free over the whole span it took to raise the rung,
    /// which is how a penalty evaporates silently (the `site_requirement`-that-requires-nothing
    /// failure, in the time axis).
    #[serde(default)]
    pub grace_turns: Option<u32>,
}

// **RETIRED: `RungBuild::decay_fraction_per_turn`** — the fraction of a rung's own cost bled per turn
// nobody worked the source (`0.01` on both plant rungs, `null` on both animal ones).
//
// **SHORTFALL IS THE DECAY** (`docs/plan_standing_upkeep.md` §2.4): what an improvement loses is
// exactly the work nobody supplied toward its [`RungUpkeep`], so a second dial saying *how fast it
// forgets* is a second answer to a question the upkeep already answers. Two numbers described one
// mechanic, and they could disagree — a rung could bleed faster than its own upkeep cost to hold,
// which is a source you are better off abandoning than keeping. [`RungDef::build_decay`] went with
// it; the plant branch's two call sites now read [`RungDef::upkeep_decay`].

// **RETIRED: `RungBuild::crew_needed`** — a per-rung staffing *floor* on the source's published
// `workers_needed`.
//
// It existed because a source's worker cap was inverted out of its **take**, and a crew building
// paid a dipped take, so committing to a 25-turn Cultivate made the panel ask for *one* forager
// where the same wild patch asks for two. **The player states each activity's crew now**
// (`docs/plan_standing_upkeep.md` §2.2): there is no blended head count left for a floor to raise,
// and `workers_needed` is answered per activity from the work model — hands to meet the upkeep,
// hands to haul the offer. A rung-level constant would be a number nothing reads.

/// **What SCALES a rung's standing upkeep** — the bounded coded set
/// `docs/plan_standing_upkeep.md` §2.6 calls for, and the same "config over coded primitives" idiom
/// [`RungBehavior`] already uses for `movement` / `feeding` / `harvest`.
///
/// An upkeep cannot be a flat per-rung number, because what makes a thing expensive to *hold* differs
/// by what it is: a pen scales with the herd it holds, a farm with the area it works, a route with
/// its length and the terrain it crosses. So a rung declares a **rate** plus **what scales it**,
/// chosen from here — adding a primitive is coding one thing once, after which using it is a config
/// edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpkeepScale {
    /// **The rate as declared** — the demand does not vary with the source. What a rung whose cost is
    /// the *thing existing* rather than the thing's size says.
    Flat,
    /// **× the source's own LOAD reading, in whatever unit the rung quotes its rate in.** The
    /// animal rungs quote **per keeper-load** — `head count / animals_per_herder` — which is what
    /// lets one rate say *a shepherd minds 300 sheep and a cowherd 80 cattle*: the species owns the
    /// ratio, the rung owns the rate. Linear in the source either way, which is the pen's shape:
    /// twice the animals, twice the keeping.
    ///
    /// **It is deliberately NOT "head count".** A per-*head* rate would say "one keeper per 100 fowl
    /// but one per 2 boar" and invent a 45-herder steppe megaherd that is a pure artifact of the
    /// unit — the measurement error `animals_per_herder` exists to prevent, restated one level up.
    /// The caller supplies the reading (`fauna::herd_keeper_loads`), so the species' own ratio is
    /// folded in before the ladder ever sees it and there is still exactly one definition of it.
    SourceLoad,
}

impl UpkeepScale {
    /// Stable config key, used in validation messages — the [`RungBranch::as_str`] convention.
    pub fn as_str(self) -> &'static str {
        match self {
            UpkeepScale::Flat => "flat",
            UpkeepScale::SourceLoad => "source_load",
        }
    }

    /// **The scale term this primitive reads**, given the source's own measure. `source_measure` is
    /// the load reading for [`Self::SourceLoad`] and is ignored by [`Self::Flat`], which is why the
    /// caller may pass anything — including [`UNSCALED_UPKEEP`] — for a rung it has not resolved.
    pub fn factor(self, source_measure: f32) -> f32 {
        match self {
            UpkeepScale::Flat => UNSCALED_UPKEEP,
            UpkeepScale::SourceLoad => source_measure.max(0.0),
        }
    }
}

/// **The scale term of a source that costs exactly what its rung declares** — the identity, passed by
/// every caller with no per-source measure to apply (and read by [`UpkeepScale::Flat`] whatever it is
/// handed). The upkeep twin of [`RUNG_COST_UNSCALED`].
pub const UNSCALED_UPKEEP: f32 = 1.0;

/// **WHAT IT COSTS TO HOLD THIS RUNG** — the *rate* half of the ladder, beside `build`'s *pile*
/// (`docs/plan_standing_upkeep.md` §2.1). Both are in work units; the build is a fixed job you finish
/// once, this is work you must supply **every turn** or the improvement slides back down.
///
/// **`upkeep: null` means this rung has no standing cost** — the two **wild** rungs on the shipped
/// ladder, where there is nothing built to hold. The whole block is optional for the same reason
/// [`RungBuild::decay_fraction_per_turn`] is: a parked `0` says "no upkeep" while reading like a live
/// dial, so the config states the absence by being absent.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RungUpkeep {
    /// **THE STANDING DEMAND, IN WORK UNITS PER TURN**, before [`Self::scaled_by`] scales it. One
    /// unit is one worker-turn at the food peak with no gear ([`PER_WORKER_OUTPUT`]) — the same
    /// currency [`RungBuild::work_cost`] is quoted in, deliberately, so *"what does it cost to hold
    /// this"* has one answer in one unit whichever rung is asked.
    ///
    /// Validated finite and `> 0` when the block is present. A parked `0` is rejected for exactly the
    /// reason a parked `decay_fraction_per_turn: 0` is: it means *"no upkeep"* while reading like a
    /// live dial, and the config already has a way to say that — `upkeep: null`.
    pub work_per_turn: f32,
    /// **What multiplies the rate** ([`UpkeepScale`]) — the generic piece, so a rung that recombines
    /// existing primitives is pure config.
    pub scaled_by: UpkeepScale,
    /// **WHAT AN UNMET DEMAND DOES TO THIS RUNG'S METER** ([`RungMeterDecay`]) — the rate it rots at
    /// and the point below which the rung is revoked.
    ///
    /// **`null` means this rung's penalty is not a meter bleed**, which is both animal rungs: an
    /// under-kept flock **sheds animals** at the husbandry config's own
    /// `pen_escape_fraction` / `pastoral_escape_fraction`, and a second rate here would be a
    /// duplicate of those — two numbers for one mechanic, free to disagree. The shortfall *fraction*
    /// is shared ([`upkeep_shortfall_fraction`]); only the rate each web applies it at differs.
    #[serde(default)]
    pub meter_decay: Option<RungMeterDecay>,
    /// **The upkeep GRACE** — consecutive turns of shortfall forgiven before decay begins. Exactly
    /// [`RungBuild::grace_turns`]'s meaning, on the upkeep's own trigger: the penalty applies while
    /// the source's shortfall counter is **strictly greater** than this.
    ///
    /// It is the rung's own number rather than a reading of the build's because the two answer
    /// different questions — *how long may a build sit un-worked* and *how long may a standing cost go
    /// unpaid* — and a rung is free to be forgiving about one and strict about the other.
    pub grace_turns: u32,
}

/// **WHAT AN UNMET DEMAND COSTS A RUNG WHOSE PENALTY IS A METER BLEED** — the third and fourth
/// dials of the standing upkeep, beside the demand and the grace
/// (`docs/plan_standing_upkeep.md` §2.4).
///
/// # THE RATE IS NOT THE DEMAND
///
/// *Shortfall was the decay*: a patch short by `0.75` work lost `0.75` off its meter. That welded
/// **how much work holding this wants** to **how fast it rots when you stop**, so the demand could
/// never be retuned — raising it made the improvement rot faster in exact proportion, and lowering
/// it made neglect cheaper. Splitting them is what lets `plant:field` ask for four hands a turn and
/// still rot at the three-quarters of a work unit it always rotted at.
///
/// # AND THE RUNG IS NOT LOST THE INSTANT ITS METER DIPS
///
/// A completed meter sits **exactly at its own cost**, so under a `progress >= cost` predicate the
/// first bleed of any size revoked the rung: finish a Cultivate and the patch could be out of
/// *tended* before its keepers were assigned, which no grace and no rate could fix because the loss
/// was a **threshold test**. [`Self::retain_fraction`] is that threshold, moved down to a stated
/// point and made scale-free.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RungMeterDecay {
    /// **WHAT A WHOLLY UNMAINTAINED RUNG LOSES EVERY TURN**, in work units — the rate the shortfall
    /// fraction scales. A rung staffed at half its demand loses half this.
    ///
    /// Validated finite and `> 0`: a rate of zero says *"this rung never rots"*, which the config
    /// already says by declaring no `meter_decay` at all.
    pub per_turn: f32,
    /// **HOW FAR THE METER MAY ERODE BEFORE THE RUNG IS REVOKED**, as a fraction of that rung's own
    /// stamped cost. `0.75` = *"a tended patch stays tended while its meter is at least
    /// three-quarters full"*.
    ///
    /// **A fraction rather than an amount, so it survives a cost retune** — the bar is stamped onto
    /// the source when the rung completes ([`RungDef::retention_bar`]), exactly as the job's cost is,
    /// so a later retune moves the price without revoking a rung the player has already paid for.
    ///
    /// **The rung is still EARNED at `progress >= cost`.** Only losing it moves. Validated finite and
    /// in `0.0..=1.0`: above `1.0` would revoke a rung the turn it completed, and `1.0` itself is the
    /// pre-arc behaviour, stated rather than stumbled into.
    pub retain_fraction: f32,
}

/// **One rung of one ladder** — the record §5 promises: the links and the dials, no logic.
#[derive(Debug, Clone, Deserialize)]
pub struct RungDef {
    /// Rung name, unique within its branch (`wild`, `tended`, `pastoral`, `pen`, …).
    pub id: String,
    /// Which food web this rung belongs to.
    pub branch: RungBranch,
    /// Position on its branch's ladder, `1` = the wild source. Unique within the branch; the ladder
    /// is strictly sequential (see `requires_rung`).
    pub order: u32,
    /// The [`Improvement`] whose verb fills this rung's build meter. `None` = **this rung is not
    /// driven by a verb today** — the engine skips it (both wild rungs, which are nothing to build).
    pub verb: Option<String>,
    /// The knowledge a faction must hold before it may select `verb`. `None` = ungated today.
    pub unlock_knowledge: Option<String>,
    /// The knowledge **practising this rung** teaches the faction (§4 — "practice rung N unlocks rung
    /// N+1"). `None` = this rung teaches nothing today.
    pub earns_knowledge: Option<String>,
    /// **The rung directly below this one on the ladder** — a statement about the ladder's *shape*,
    /// asserted by [`LadderConfig::validate_sequence`] (same branch, `order - 1`; `None` iff
    /// `order == 1`). It is what makes the ladder a sequence rather than a bag of rungs.
    ///
    /// **It is deliberately NOT a per-source precondition, and nothing reads it as one.** Whether
    /// *this* source may start *this* rung is a coded gate the rung's own verb owns (`corral` refuses
    /// a herd that isn't pastoral; `cultivate` refuses a patch that is already tended), because the
    /// rule genuinely differs per branch: `Corral` needs a herd you already tamed, while **`Sow`
    /// needs no prior *patch*** — it places a source on a tile that grew none, so it starts from
    /// ground rather than from a source. Both facts are true at once: `plant:field` still sits
    /// directly above `plant:tended` on the ladder, and sowing an unpatched tile skips no *step of
    /// the ladder*.
    ///
    /// **§2's "seed travels, so rung 3 can create a source where none existed" was reversed by the
    /// gathering-site arc** and now belongs to rung 4 (Farm). Rung 3 may still sow a tile carrying no
    /// patch, but only a tile its people already **gather** — see
    /// [`RungSiteRequirement::requires_gathering_site`]. That is a `site_requirement` fact, not a
    /// `requires_rung` one, which is exactly why the two fields stay separate.
    pub requires_rung: Option<String>,
    /// The per-species `husbandry_ceiling` a herd needs to reach this rung (Grazing 2d-δ). Animal
    /// branch only — a plant has no species ceiling.
    pub ceiling_required: Option<HusbandryCeiling>,
    /// **What the LAND must be** for this rung to be placed on a tile ([`RungSiteRequirement`]) — the
    /// plant twin of `ceiling_required`, keyed on the ground instead of the species. `None` = the rung
    /// asks nothing of the site.
    ///
    /// **Today that is the animal rungs only: all three plant rungs state one**, each requiring a
    /// gathering site (`every_plant_rung_is_bound_to_a_gathering_site` asserts it). It used to read
    /// "every rung but `plant:field`" — the gathering-site arc pushed the requirement down the whole
    /// plant branch, so a new plant rung without a `site_requirement` is now the anomaly rather than
    /// the norm. **Rung 4 (Farm) differs from rungs 1–3, not from rung 3 alone**: it is the first to
    /// set `requires_gathering_site: false`, and puts a fertility floor back in its place.
    pub site_requirement: Option<RungSiteRequirement>,
    /// The build meter's dials, or `None` for a rung with nothing to build.
    pub build: Option<RungBuild>,
    /// **What it costs to HOLD this rung, per turn** ([`RungUpkeep`]). `None` = this rung has no
    /// standing cost — the two **wild** rungs, and only those: all four managed rungs declare one.
    #[serde(default)]
    pub upkeep: Option<RungUpkeep>,
    /// The coded primitives this rung recombines. **Not read yet.**
    pub behavior: RungBehavior,
}

impl RungDef {
    /// The improvement that drives this rung's build meter, already parsed. `None` for a rung no verb
    /// drives today. (Validated at load, so the parse cannot fail here.)
    pub fn verb_improvement(&self) -> Option<Improvement> {
        self.verb
            .as_deref()
            .map(|verb| Improvement::from_str(verb).expect("validated at load"))
    }

    /// The discovery gating this rung's verb. `None` for an ungated rung. (Validated at load.)
    pub fn unlock_discovery_id(&self) -> Option<u32> {
        self.unlock_knowledge
            .as_deref()
            .map(|name| discovery_id_for(name).expect("validated at load"))
    }

    /// The discovery practising this rung teaches. `None` for a rung that teaches nothing.
    /// (Validated at load.)
    pub fn earns_discovery_id(&self) -> Option<u32> {
        self.earns_knowledge
            .as_deref()
            .map(|name| discovery_id_for(name).expect("validated at load"))
    }

    /// **THE earn seam — "practice rung N unlocks rung N+1"** (`docs/plan_intensification_ladder.md`
    /// §4), the exact twin of [`RungDef::build_accrual`]: the rung says *what* is learned and *how
    /// much*, the caller applies it to the ledger it owns.
    ///
    /// `self` is **the rung the source currently stands on** — *not* the rung whose verb is being
    /// used. That distinction is the whole model: you learn **herding** by managing **wild** herds and
    /// **penning** by managing **tamed** ones, so a hunt teaches Herding on a wild herd and
    /// **Penning** on a pastoral one — same crew, different rung, different lesson. Callers resolve
    /// the rung with [`crate::fauna::herd_rung`] / [`crate::forage::patch_rung`].
    ///
    /// # It answers "how much of what", not "is this earned"
    ///
    /// It was `knowledge_earned(floor, eligible) -> Option<u32>` while restraint was a **predicate**
    /// (`floor_teaches` — teach at or above the food peak, nothing below). The harvest floor made
    /// restraint a **rate**, so the amount is now part of the answer and the rename says so.
    ///
    /// # The amount is PRACTICE over the lesson's own COST
    ///
    /// ```text
    /// practice = learn_rate × learn_multiplier(floor)     // per SOURCE per turn, NOT per worker
    /// amount   = practice / lesson_cost(this rung's knowledge)
    /// ```
    ///
    /// **`workers` is deliberately not a parameter here, and that asymmetry with
    /// [`RungDef::build_accrual`] is the model** — see [`LadderKnowledge`]: work is earned by a
    /// worker-turn and scales with hands, practice is earned by a *turn the source is worked* and
    /// must not, or a faction would learn ten times faster by piling hands onto one patch.
    ///
    /// Returns `(discovery, amount)`, or `None` when:
    /// - `eligible` is false — the caller's composed gate. It carries the rung's own terms **and the
    ///   work predicate** (`systems::labor::crew_is_working_the_source`: is anything standing above
    ///   this assignment's floor?). That replaced the `EcologyPhase::Thriving` term both earn sites
    ///   used to carry, which was a gate where the model now wants a rate. It is deliberately the
    ///   escapement **room** and not the take — see the predicate for why `killed == 0` is a
    ///   quantisation fact rather than a fact about work.
    /// - the rung simply teaches nothing (`earns_knowledge: null` — today the `plant:field` rung, whose
    ///   `irrigation`/`rotation` is a future rung's business; the `animal:pen` rung teaches Foddering).
    ///
    /// A zero amount is returned as `None` rather than as `Some(id, 0.0)`: at `floor = 0` there is no
    /// lesson, and a caller crediting `0` to a ledger is a write that says nothing.
    ///
    /// **The two webs cannot cross-teach** (§4.2) and it costs no code to guarantee: the lesson is
    /// read off the *source's own rung*, and a rung belongs to exactly one [`RungBranch`], so a hunt
    /// can only ever reach an `animal` rung's `earns_knowledge` and a forage a `plant` one's. A master
    /// rancher isn't automatically a farmer.
    ///
    /// The rung names the lesson; the ladder prices it ([`LadderKnowledge::lesson_costs`]) and the
    /// floor scales the practice that pays for it.
    pub fn knowledge_accrual(
        &self,
        floor: f32,
        eligible: bool,
        knowledge: &LadderKnowledge,
    ) -> Option<(u32, f32)> {
        if !eligible {
            return None;
        }
        let practice = knowledge.learn_rate * learn_multiplier(floor);
        if practice <= 0.0 {
            return None;
        }
        let name = self.earns_knowledge.as_deref()?;
        let amount = knowledge
            .ledger_credit(name, practice)
            .expect("validate requires a lesson cost for every knowledge a rung teaches");
        self.earns_discovery_id().map(|lesson| (lesson, amount))
    }

    /// **The build seam — the accrual side. THE WORK UNITS THIS BUILD CREW PRODUCES THIS TURN**, not
    /// a fraction of anything and net of nothing: [`activity_work`]`(workers)` when `improvement`
    /// **is** the rung's verb *and* the caller's rung-specific gates hold (`eligible` — knows the
    /// unlock knowledge, the species' ceiling allows it, the faction owns it), otherwise
    /// [`NO_BUILD_PROGRESS`].
    ///
    /// # THE MAINTENANCE RATE IS NOT A TAX ON BUILDING — `work_cost / crew` IS the pace
    ///
    /// **A build crew supplies nothing toward the rate; its whole output is progress**
    /// (`docs/plan_standing_upkeep.md` §4.6a). The band's keeping pool owes the rate for every meter
    /// carrying work, at **any** fullness, so there is nothing here to subtract:
    ///
    /// ```text
    /// build_work = builders × PER_WORKER_OUTPUT
    /// turns      = work_cost / build_work            // …less whatever the meter is bleeding
    /// ```
    ///
    /// **A build can still fail to finish, but the term that eats it is the ROT** — what the keeping
    /// failed to cover, bleeding off the very meter the builders are raising. That is a *signed*
    /// reading and belongs to the countdown, not to a meter that may only be added to, so it lives on
    /// [`build_balance`](Self::build_balance) and this seam stays the crew's own output.
    ///
    /// The caller adds it to the source's own meter and completes the rung when the meter reaches
    /// [`RungDef::build_cost`]. **Turns are the output, not an input** — the same fixed job finishes
    /// sooner as the faction puts more hands or better tools on it, which is the progression
    /// statement the normalized meter could not make.
    ///
    /// # `workers` is the BUILD's OWN CREW, and there is no cap
    ///
    /// `LaborAssignment::improvement_workers`, named by the improvement verb — **not** the take crew
    /// this build rides beside (`docs/plan_standing_upkeep.md` §2.2). A worker is worth
    /// [`PER_WORKER_OUTPUT`], so `n` workers produce `n` units a turn and a Cultivate staffed at
    /// fifty finishes in a turn. That is allowed: the constraint is opportunity cost across systems,
    /// not a rule forbidding a play style (`docs/plan_unit_costed_work.md` §1.2).
    ///
    /// A rung with **no verb** (`verb: null`) or **no build** is never driven: it returns `0` — which
    /// is what keeps the `wild` rungs (nothing to build) out of the engine.
    ///
    /// # THE FLOOR IS NOT HERE ANY MORE, and its removal is the point of the separate crew
    ///
    /// [`learn_multiplier`] used to scale this, on the rule *"a crew pulling hard on the source it is
    /// improving builds slowly"*. **That rule was written when ONE crew did both jobs.** With the
    /// build staffed in its own right the build crew is not pulling anything — and a build crew on a
    /// source nobody is harvesting has **no floor to read at all**, so the term would have to be
    /// invented from a default nobody chose. The floor stays where it still means something:
    /// [`RungDef::knowledge_accrual`], where *how much you leave standing shapes what you learn*.
    ///
    /// **The shipped pace is unchanged**, because `learn_multiplier` is exactly `×1.0` at the food
    /// peak, which is the floor a fresh assignment gets. Only sub-peak floors build faster now
    /// (`the_food_peak_preserves_every_rungs_stated_build_length` is the proof).
    ///
    /// # THE GEAR IS NOT HERE EITHER, and that is the shape of §6
    ///
    /// A tool's help lands on the **job**, not on the crew's output: it takes work units off the cost
    /// ([`LadderConfig::effective_build_cost`]), because a multiplier on the crew cancels the cost
    /// (`turns_geared / turns_bare = w / (w + h)` for any job) and therefore saves the same
    /// *percentage* of turns on a garden and on a farm alike. The retired `build_rate` parameter was
    /// exactly that multiplier.
    pub fn build_accrual(
        &self,
        improvement: Option<Improvement>,
        eligible: bool,
        workers: u32,
    ) -> f32 {
        self.build_supply(improvement, eligible, workers)
            .unwrap_or(NO_BUILD_PROGRESS)
    }

    /// **THE SIGNED TWIN THE COUNTDOWN READS** — `build_work − rot_this_turn`, the same crew output
    /// [`build_accrual`](Self::build_accrual) hands a meter, less what the Logistics decay pass will
    /// bleed off that meter (`docs/plan_standing_upkeep.md` §4.6a).
    ///
    /// # THE ROT IS THE DENOMINATOR, AND IT DOES NOT VARY WITH THE BUILD CREW
    ///
    /// A meter must never be handed a negative amount to add, and the bleed is the decay pass's to
    /// apply — so the accrual is the crew's output and this is where the two accounts meet. A crew
    /// banking exactly what the meter bleeds is [`BuildTurns::Holding`]; one banking less is
    /// [`BuildTurns::Rotting`], losing work already bought.
    ///
    /// `rot_this_turn` comes from [`Self::meter_rot`] — the one seam, so the published countdown
    /// cannot drift from the bleed it describes, nor from the `meterRotPerTurn` the wire quotes
    /// beside it. It is a **constant with respect to the crew**, which is what lets a compose sheet
    /// re-evaluate the estimate for a crew the player is merely proposing.
    ///
    /// [`NO_BUILD_BALANCE`] for a rung this crew is not building at all — nothing is in play, and
    /// every caller that could act on the value is already gated by `staffed`.
    pub fn build_balance(
        &self,
        improvement: Option<Improvement>,
        eligible: bool,
        workers: u32,
        rot_this_turn: f32,
    ) -> f32 {
        self.build_supply(improvement, eligible, workers)
            .map_or(NO_BUILD_BALANCE, |supply| supply - rot_this_turn)
    }

    /// **WHAT THIS BUILD CREW SUPPLIES THIS TURN** — or `None` when this rung is not the one this
    /// crew is building: it has nothing to build, the caller's gates refuse it, or the assignment's
    /// verb names a different rung.
    ///
    /// One gate for the two readings above, so [`build_accrual`](Self::build_accrual) and
    /// [`build_balance`](Self::build_balance) cannot come to disagree about *whether* a build is
    /// running while agreeing about its arithmetic.
    fn build_supply(
        &self,
        improvement: Option<Improvement>,
        eligible: bool,
        workers: u32,
    ) -> Option<f32> {
        self.build.as_ref()?;
        if !eligible || improvement.is_none() || self.verb_improvement() != improvement {
            return None;
        }
        Some(activity_work(workers))
    }

    /// **The build seam — the cost side. WHAT THIS JOB COSTS ON THIS SOURCE**, in work units:
    /// `work_cost × cost_multiplier`. `None` for a rung with nothing to build.
    ///
    /// `cost_multiplier` is the source's own nature — today only a species'
    /// `taming_cost_multiplier` on `animal:pastoral` (a Steppe Runner is five times the work of a
    /// rabbit); every other caller passes [`RUNG_COST_UNSCALED`]. The honest statement is that *the
    /// animal costs more work*, where the retired `taming_rate` said *your people are worse at this*.
    pub fn build_cost(&self, cost_multiplier: f32) -> Option<f32> {
        self.build
            .as_ref()
            .map(|build| build.work_cost * cost_multiplier)
    }

    /// **The build seam — the neglect grace.** How many consecutive un-worked turns this rung
    /// forgives before its neglect penalty starts biting ([`RungBuild::grace_turns`]).
    /// [`NO_NEGLECT_GRACE`] for a rung with no build, and for one that counts no un-worked turns at
    /// all — the whole plant branch, whose penalty is an upkeep shortfall and whose grace is
    /// therefore [`Self::upkeep_grace_turns`].
    ///
    /// Unlike the accrual this takes **no `timescale`**: the grace is a count of *turns a crew was
    /// absent*, not an amount of progress, and a species that is slow to tame is not thereby slower
    /// to notice its keepers have gone.
    pub fn neglect_grace_turns(&self) -> u32 {
        self.build
            .as_ref()
            .and_then(|build| build.grace_turns)
            .unwrap_or(NO_NEGLECT_GRACE)
    }

    /// **The upkeep seam — the crew that MEETS the demand**, in whole workers:
    /// `ceil(upkeep_demand / PER_WORKER_OUTPUT)`. This is the `workers_needed` the maintain activity
    /// publishes (`docs/plan_standing_upkeep.md` §2.2) — *"hands to keep this standing"*, in its own
    /// unit, beside the take's *"hands to haul the offer"*.
    ///
    /// `0` for a rung that declares no upkeep — the two **wild** rungs: nobody is needed to hold
    /// something that costs nothing to hold.
    pub fn upkeep_crew_needed(&self, source_measure: f32) -> u32 {
        let demand = self.upkeep_demand(source_measure);
        if demand <= NO_UPKEEP_DEMAND {
            return NO_CREW_ON_THIS_ACTIVITY;
        }
        (demand / PER_WORKER_OUTPUT).ceil() as u32
    }

    // **RETIRED: `yield_fraction_while_building()`** — the investment dip, `0.50` on all four rungs
    // that declared a build. It said *"this crew is preparing ground, not gathering"*, which is true
    // of a **shared** crew and of nothing else; the player names the build's own crew now
    // (`docs/plan_standing_upkeep.md` §2.2), so what a build costs is the people who are clearing
    // instead and the gatherers beside them are untouched. Four magic numbers retired with it, along
    // with a term nobody ever chose — the plant web sat at `0.25` for years purely because that was
    // the pre-move constant's value.

    /// **The upkeep seam — WHAT THIS RUNG DEMANDS EVERY TURN**, in work units:
    /// `work_per_turn × scaled_by(source_measure)`. [`NO_UPKEEP_DEMAND`] for a rung that declares
    /// none — the two **wild** rungs, where a crew's whole output reaches its take because there is
    /// nothing standing on the ground to hold. On the four managed rungs the term is live, and it is
    /// charged **while building as well as while holding** — but always to the band's **keeping
    /// pool** (§4.6a). The builders pay none of it, so a lone builder banks a whole worker-turn on
    /// the dearest rung on the ladder.
    ///
    /// `source_measure` is the source's own scale reading — a herd's head count for
    /// [`UpkeepScale::SourceLoad`], ignored by [`UpkeepScale::Flat`], so a caller with nothing to
    /// measure passes [`UNSCALED_UPKEEP`]. It is the exact twin of [`Self::build_cost`]'s
    /// `cost_multiplier`: the rung owns the mechanic, the source is priced.
    pub fn upkeep_demand(&self, source_measure: f32) -> f32 {
        self.upkeep.as_ref().map_or(NO_UPKEEP_DEMAND, |upkeep| {
            upkeep.work_per_turn * upkeep.scaled_by.factor(source_measure)
        })
    }

    /// **Does this rung cost anything to HOLD?** — the question the build's completion hand-off asks
    /// (`docs/plan_standing_upkeep.md` §2.2): a finished rung that declares an upkeep keeps the crew
    /// that raised it, and one that declares none frees them.
    ///
    /// **`true` on all four managed rungs** — `plant:tended`, `plant:field`, `animal:pastoral` and
    /// `animal:pen` each declare one, so every completed build hands its crew onto the keeping rather
    /// than freeing it. Only the two `wild` rungs answer `false`, and nothing is ever *built* on
    /// those, so no completion reaches this asking about one.
    pub fn declares_upkeep(&self) -> bool {
        self.upkeep.is_some()
    }

    /// **The upkeep seam — the grace.** Consecutive turns of shortfall this rung forgives before its
    /// decay starts ([`RungUpkeep::grace_turns`]). [`NO_NEGLECT_GRACE`] for a rung with no upkeep —
    /// there is no unmet demand to forgive.
    pub fn upkeep_grace_turns(&self) -> u32 {
        self.upkeep
            .as_ref()
            .map_or(NO_NEGLECT_GRACE, |upkeep| upkeep.grace_turns)
    }

    /// **THE UPKEEP SEAM — THE DECAY IS PROPORTIONAL TO HOW SHORT YOU ARE, AT THIS RUNG'S OWN
    /// RATE** (`docs/plan_standing_upkeep.md` §2.4):
    ///
    /// ```text
    /// decay_this_turn = shortfall_fraction × meter_decay.per_turn
    /// ```
    ///
    /// once the shortfall has outlasted this rung's own [`RungUpkeep::grace_turns`].
    ///
    /// **It is continuous, and that is the whole point.** Half the hands a source needs means it
    /// slides at half rate — not at the full neglect rate and not at nothing. The binary *"is this
    /// source worked"* flag could not express that, which is why a crew half the size a source needed
    /// used to count as fully worked.
    ///
    /// **The RATE is the rung's, not the demand's.** *Shortfall was the decay* until this arc, which
    /// meant the demand and the rot rate were one number wearing two hats: retuning the demand
    /// silently retuned the rot. They are separate dials now, and the plant demands moved from
    /// `0.5`/`0.75` to `2`/`4` with the rot rate held exactly where it was
    /// ([`upkeep_shortfall_fraction`]).
    ///
    /// `shortfall_turns` is the source's **consecutive** unmet-demand counter, read on the same
    /// convention as [`neglect_grace_remaining`]: the decay applies while it is **strictly greater**
    /// than the grace, so `grace_turns: 0` bleeds on the first unmet turn.
    ///
    /// [`NO_UPKEEP_DECAY`] for a rung with no upkeep, for one whose penalty is not a meter bleed
    /// (both animal rungs — their flock sheds instead), and for one still inside its grace.
    pub fn upkeep_decay(&self, shortfall_fraction: f32, shortfall_turns: u16) -> f32 {
        let Some(upkeep) = self.upkeep.as_ref() else {
            return NO_UPKEEP_DECAY;
        };
        let Some(decay) = upkeep.meter_decay.as_ref() else {
            return NO_UPKEEP_DECAY;
        };
        if u32::from(shortfall_turns) <= upkeep.grace_turns {
            return NO_UPKEEP_DECAY;
        }
        (shortfall_fraction.clamp(FULLY_SUPPLIED, WHOLLY_UNSUPPLIED) * decay.per_turn)
            .max(NO_UPKEEP_DECAY)
    }

    /// **WHAT THIS METER IS LOSING PER TURN RIGHT NOW**, in work units —
    /// [`Self::upkeep_decay`] evaluated at the live shortfall, and **the one seam** three readers
    /// share: the build countdown's denominator ([`Self::build_balance`]), the wire's
    /// `meterRotPerTurn`, and the compose sheet that re-derives the countdown from it.
    ///
    /// ```text
    /// rot = upkeep_decay(upkeep_shortfall_fraction(upkeep_demand(measure), supplied), shortfall_turns)
    /// ```
    ///
    /// It is [`NO_UPKEEP_DECAY`] when the keeping covers the demand, when the source is still inside
    /// its grace, and when the rung declares no `meter_decay` at all — **which is both animal rungs**,
    /// whose penalty is the shed rather than a meter bleed. An animal source therefore always reads
    /// `0` here, and that is the model rather than a gap: nothing eats an animal build.
    ///
    /// `shortfall_turns` is the source's own consecutive-shortfall counter (`neglect_turns`), read on
    /// [`Self::upkeep_decay`]'s convention — so inside the grace this is `0` and a build publishes a
    /// real count, then flips to losing the turn the grace expires.
    pub fn meter_rot(&self, source_measure: f32, supplied: f32, shortfall_turns: u16) -> f32 {
        self.upkeep_decay(
            upkeep_shortfall_fraction(self.upkeep_demand(source_measure), supplied),
            shortfall_turns,
        )
    }

    // **RETIRED: `meter_raising_demand`** — "what an unfinished meter is owed", as distinct from what
    // holding the finished rung costs.
    //
    // **There was never a second demand.** The maintenance rate is owed *always*, while building and
    // while held alike (`docs/plan_standing_upkeep.md` §2.4); what the meter decides is only **who
    // supplies it** — the build crew below its cost, the band's keeping pool at it. A second concept
    // for the same rate could only ever drift from the first, and the per-web split it carried (a
    // plant meter owed its rot rate, an animal one owed its whole keeping) was an exception with no
    // fact under it: *you cannot be billed to hold something you have not finished building* is
    // answered by who pays, not by discounting the bill.

    /// **THE BAR THIS RUNG IS HELD AT ONCE IT IS EARNED** — `retain_fraction × cost`, the point the
    /// meter may erode to before the rung is revoked (`docs/plan_standing_upkeep.md` §2.4).
    ///
    /// **The caller stamps it onto the source at completion**, beside the job's own cost, so the
    /// predicates that ask *"is this patch tended"* need no config in scope — the reason
    /// `is_cultivated()` can stay a method on the patch across its hundred call sites. It also makes
    /// the bar survive a retune of the rung's price, exactly as the stamped cost does.
    ///
    /// A rung with no `meter_decay` (or no upkeep at all) is held at its **whole** cost, which is the
    /// pre-arc predicate stated rather than special-cased: nothing bleeds it, so the bar is never
    /// tested.
    pub fn retention_bar(&self, cost: f32) -> f32 {
        self.upkeep
            .as_ref()
            .and_then(|upkeep| upkeep.meter_decay.as_ref())
            .map_or(cost, |decay| cost * decay.retain_fraction)
    }
}

/// **How fast the ladder is learned** — what a turn of practice is worth, what each lesson costs, and
/// the bar at which a faction may act on one.
///
/// These dials used to be **duplicated, at identical values, in both webs** (`labor_config`'s
/// `forage.cultivation` and `fauna_config`'s `husbandry`) because each web had its own hard-coded earn
/// site. Slice 4 made the earn path **one rung-driven seam**
/// ([`RungDef::knowledge_accrual`]), so two per-web copies became a pure DRY hazard — nothing but
/// discipline kept "20 turns to learn Herding" and "20 turns to learn Cultivation" the same
/// statement. They live here for the same reason the build dials do: the two ladders must climb on
/// the same numbers, and the ladder is where a number that describes *both* webs belongs.
///
/// # A LESSON COSTS PRACTICE — and practice is NOT work
///
/// The build half prices a job in **work units** and a crew's hands are its throughput
/// ([`RungBuild::work_cost`]). The lesson half is the same inversion in a **deliberately separate
/// currency** (`docs/plan_unit_costed_work.md` §2), and **naming them apart is what stops anyone
/// adding them**:
///
/// | | **work units** | **practice units** |
/// |---|---|---|
/// | earned by | a **worker-turn** on the source | a **turn** the source is worked |
/// | scales with hands? | **yes** — that is what the build arc is for | **no** |
/// | scaled by the floor? | yes ([`learn_multiplier`]) | yes ([`learn_multiplier`]) |
/// | tools contribute? | yes | no |
/// | spent on | a per-source build meter | the faction knowledge ledger |
///
/// **LEARNING MUST NOT SCALE WITH HANDS.** Knowledge is faction-level and credited **once per source
/// per turn** (`systems::labor::credit_rung_lesson`), so a per-worker rate would let a faction learn
/// ten times faster by piling hands onto one patch — the build arc's no-cap decision without the
/// opportunity-cost brake that justifies it, since a second lesson costs nothing extra. *You learn by
/// watching the practice, not by counting the hands doing it.*
#[derive(Debug, Clone, Deserialize)]
pub struct LadderKnowledge {
    /// **What ONE TURN of practice at the food peak is worth, in practice units** — charged once per
    /// source per turn a crew works a rung that teaches, scaled by the assignment's floor
    /// ([`learn_multiplier`], food peak = ×1.0). It is `1.0`, so a [`Self::lesson_costs`] entry reads
    /// itself: a cost of `20` means *twenty worked turns at the food peak*, and a crew leaving more
    /// standing learns it faster in proportion.
    ///
    /// It replaced a `progress_per_turn` of `0.05` — a rate straight into a normalized ledger, under
    /// which **every lesson on both webs took the same ~20 turns** and a knowledge could only be
    /// dearer by making the crew worse at learning it. Validated finite and `> 0` — a zero would make
    /// every knowledge unlearnable, silently freezing the ladder at rung 1.
    pub learn_rate: f32,
    /// Ledger progress at which a faction **knows** a discovery and may select the verb it gates
    /// ([`knows`]). Validated `0 < t <= 1`: at `0` every knowledge would be known before it was
    /// learned (every gate open from turn 1); above `1` no gate could **ever** open, since
    /// `DiscoveryProgressLedger` clamps accrual to `1.0`.
    pub completion_threshold: f32,
    /// **WHAT EACH LESSON COSTS, IN PRACTICE UNITS**, keyed by the knowledge's name — the same names
    /// a rung's `earns_knowledge` spells and [`discovery_id_for`] resolves. `20` is twenty worked
    /// turns at the food peak.
    ///
    /// **Keyed by the KNOWLEDGE, not by the rung that teaches it**, because that is whose property
    /// the cost is: a knowledge can in principle be taught by more than one rung (and a craft is
    /// taught by no rung at all), so hanging the number off a rung record would make the same lesson
    /// cost two different things depending on where it was practised. It lives in this file for the
    /// reason [`Self::craft_lesson_per_item`] does — **every knowledge pace in the game is tuned in
    /// one place**.
    ///
    /// **Every name the ladder can teach must have an entry**, and `validate` insists on it for the
    /// rungs' `earns_knowledge` *and* for every craft: a missing entry would make the pace whatever
    /// a fallback happened to be, which is the parked-`0` failure in a new costume.
    ///
    /// All eight are **20** today, which is this slice's own pacing proof (`1.0 / 20` reproduces the
    /// retired `progress_per_turn` of `0.05` exactly). The spread — rung-3's lessons dearer, and
    /// `foddering` dearer again — is a later config-only slice.
    pub lesson_costs: BTreeMap<String, f32>,
    /// **What one item finished at a bench is worth, in PRACTICE UNITS** — the craft twin of
    /// [`Self::learn_rate`] (`docs/plan_crafting_and_materials.md` §5). At `4.0` against a craft's
    /// cost of `20`, a craft is learned in **5 items**.
    ///
    /// **It is a sibling of [`Self::learn_rate`] rather than a reading of it, because the quantum
    /// differs.** A ladder lesson is charged per *turn worked* and scaled by the crew's floor; a
    /// craft lesson is charged **per item completed**, on the same quantum as the bench tool's wear,
    /// so the thing that consumes the tool and the thing that teaches the craft cannot drift. There
    /// is no floor to scale it by and no turn to charge it on.
    ///
    /// **It moved with the currency rather than being left alone**: it was `lesson_per_crafted_item`
    /// `0.2`, a fraction of a normalized threshold, and leaving it that way while its sibling became
    /// a cost is precisely the drift the slice-4 consolidation existed to prevent. `4.0 / 20` is the
    /// same 5 items.
    ///
    /// **The crafts pace themselves off the land, which is the point.** Weaving is learned quickly
    /// by a gathering band (fibre is everywhere) and Bone-working slowly by anyone (bone is the
    /// scarcest yield on the roster), with no per-craft dial saying so.
    pub craft_lesson_per_item: f32,
}

impl LadderKnowledge {
    /// **THE one place practice units become LEDGER progress** — `practice / lesson_cost` — so the
    /// two teachers (a rung's [`RungDef::knowledge_accrual`] and a bench's craft lesson) cannot
    /// divide by different things.
    ///
    /// **The ledger stays normalized and this is the divisor at the seam.**
    /// `DiscoveryProgressLedger::add_progress` clamps to `1.0` and is shared with great discoveries,
    /// espionage and the start profiles, so widening its unit would be a large blast radius for no
    /// gain; [`Self::completion_threshold`] stays the ledger bar, the wire's
    /// `IntensificationKnowledgeState` fields stay `0..1`, and the per-knowledge cost lives here
    /// instead.
    ///
    /// `None` for a knowledge the ladder does not price — unreachable for anything the sim teaches,
    /// since `validate` requires an entry for every rung lesson and every craft.
    pub fn ledger_credit(&self, knowledge: &str, practice: f32) -> Option<f32> {
        self.lesson_cost(knowledge).map(|cost| practice / cost)
    }

    /// What `knowledge` costs in practice units ([`Self::lesson_costs`]).
    pub fn lesson_cost(&self, knowledge: &str) -> Option<f32> {
        self.lesson_costs.get(knowledge).copied()
    }
}

/// The whole ladder: every rung of both branches, plus the pace they are learned at
/// (`data/intensification_ladder.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct LadderConfig {
    /// The knowledge dials shared by **both** webs — see [`LadderKnowledge`].
    pub knowledge: LadderKnowledge,
    pub rungs: Vec<RungDef>,
}

impl LadderConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            LadderConfig::from_json_str(BUILTIN_INTENSIFICATION_LADDER)
                .expect("builtin intensification ladder should parse and validate"),
        )
    }

    /// Parse **and validate** (the `fauna_config.rs` / `labor_config.rs` convention, so *every* load
    /// path — builtin, default file, `INTENSIFICATION_LADDER_PATH` override — is covered and a
    /// broken ladder can never be silently accepted).
    pub fn from_json_str(json: &str) -> Result<Self, LadderConfigError> {
        let config: LadderConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, LadderConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| LadderConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        LadderConfig::from_json_str(&contents)
    }

    /// A rung the engine names ([`RungKey`]). Infallible: `validate` requires every `RungKey` to be
    /// defined, and `validate` runs on every load path.
    pub fn rung(&self, key: RungKey) -> &RungDef {
        self.find(key.branch(), key.id())
            .expect("validate requires every coded rung to be defined")
    }

    /// **The rung an [`Improvement`] builds** — the one verb→rung map, so no call site pairs a verb
    /// with a `RungKey` by hand. The inverse of [`RungDef::verb_improvement`], and total by
    /// construction: every verb names exactly one rung of exactly one branch.
    pub fn rung_for(&self, improvement: Improvement) -> &RungDef {
        self.rung(RungKey::built_by(improvement))
    }

    /// **THE BAR A GEARED CREW'S METER MUST REACH** — `cost − t`, where `t` is
    /// [`build_work_from_gear`].
    ///
    /// **The stored companion cost on the source stays the RAW job** (`work_cost ×
    /// cost_multiplier`), un-tooled and stable, because that is what `is_cultivated()` and its three
    /// siblings compare against and a meter whose bar moved with the crew's kit would un-complete a
    /// rung the moment a tool wore out. The offset therefore applies at the **completion comparison
    /// only**, and the accrue helpers set the meter to the raw cost when it is crossed — the jump is
    /// the units the tool pre-paid, and it is honest: those units were worked, they were simply
    /// worked by the tool.
    ///
    /// **Nothing floors it.** How far a kit may shrink a job is decided by the job's `work_cost` and
    /// the tool's `EquipmentStat::BuildWork` — both dials — and later work is *meant* to be
    /// impractical bare-handed, which means the right tool must be able to reduce a job to a small
    /// fraction of itself. A bar at or below zero simply means the build completes on the first turn
    /// it is worked, which is the same no-cap outcome as putting fifty hands on it.
    pub fn effective_build_cost(&self, cost: f32, gear_work: f32) -> f32 {
        cost - gear_work
    }

    /// **HOW MANY TURNS THIS CREW WOULD NEED TO CLIMB `rung`** — the *projection* half of the wire's
    /// `buildTurnsRemaining`, assembled from exactly the four calls the in-flight stamp makes
    /// ([`RungDef::build_cost`], [`build_work_from_gear`], [`Self::effective_build_cost`],
    /// [`RungDef::build_accrual`], then [`build_turns_remaining`]) so a quote for a job nobody has
    /// started cannot be arithmetic the running build would disagree with.
    ///
    /// **It exists because "nothing is being built" is the state the compose sheet is looking at**
    /// (`docs/plan_unit_costed_work.md` §11). `workCost` answers *what does this job cost*; this
    /// answers *what would my people take to finish it*, which is the half that makes the arc's
    /// thesis legible — put more hands on it and watch the number fall. The client cannot derive it:
    /// it holds neither the crew's output, nor the floor multiplier, nor the kit's contribution. Same
    /// division of labour, and the same "always meaningful, never zero-because-not-started" rule, as
    /// `HerdTelemetryState.penUpkeep`.
    ///
    /// `banked` is the work already on **that rung's** meter — [`RUNG_UNSTARTED`] for the ordinary
    /// pre-commit case, and the real figure for a build the player walked away from, so the quote
    /// agrees with the `workDone` / `workCost` pair published beside it.
    ///
    /// `eligible` is the caller's composed gate, exactly as [`RungDef::build_accrual`] takes it: the
    /// web supplies the rung's own site / ceiling / knowledge / ownership terms. **A projection must
    /// never quote a rung the gates would refuse**, so a caller that cannot answer one of them passes
    /// `false` and the wire says "no estimate" instead of naming a job the player cannot take.
    ///
    /// `None` — no estimate — for a rung with nothing to build, a gate that refuses, or a crew that
    /// produces nothing. A meter already past the bar — including a bar the crew's **gear** pays off
    /// outright — is [`BUILD_FINISHES_IN_ONE_TURN`], not `None`: it is an answer.
    // The rung, its per-source price, its meter, and the two things a build crew brings (hands, kit)
    // are all genuinely inputs — the same list `build_accrual` + `build_cost` +
    // `effective_build_cost` take between them. The **floor** left this list with the floor's term in
    // `build_accrual`: a build crew is not pulling on the source, so there is nothing for it to read.
    #[allow(clippy::too_many_arguments)]
    pub fn projected_build_turns(
        &self,
        rung: &RungDef,
        cost_multiplier: f32,
        banked: f32,
        workers: u32,
        gear_per_worker: f32,
        eligible: bool,
        rot_this_turn: f32,
    ) -> Option<BuildTurns> {
        let cost = rung.build_cost(cost_multiplier)?;
        let bar = self.effective_build_cost(cost, build_work_from_gear(gear_per_worker, workers));
        // **Quoted NET OF THE ROT, exactly as the live stamp is** — never net of the maintenance
        // rate, which the keeping pool owes whatever this crew does. `rot_this_turn` is the
        // **source's** live bleed ([`RungDef::meter_rot`] on the meter at risk), so a quote and the
        // card beside it describe one number. On ground nobody has started there is nothing banked
        // and therefore nothing to rot, so the answer is `work_cost / crew`.
        let balance = rung.build_balance(rung.verb_improvement(), eligible, workers, rot_this_turn);
        // **A quoted crew that cannot out-raise the rot never gets there** — the same standing fact
        // a running build states, so a projection says so rather than withholding the line. And a
        // quote for a rung with **work already banked on it** answers even at a crew of zero: the
        // player paid for that work, so *"it holds"* / *"it is losing ground"* is the news.
        build_turns_estimate(bar, banked, balance, eligible, workers)
    }

    /// **The reference job every build-quantum readout is quoted against**, in work units — the
    /// [`REFERENCE_BUILD_RUNG`]'s own `work_cost`. See that constant for why the number is the
    /// ladder's rather than the readout's.
    pub fn reference_build_cost(&self) -> f32 {
        self.rung(REFERENCE_BUILD_RUNG)
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the reference rung is an investment — it has a build meter")
    }

    /// A rung by branch + id, if it exists.
    pub fn find(&self, branch: RungBranch, id: &str) -> Option<&RungDef> {
        self.rungs
            .iter()
            .find(|rung| rung.branch == branch && rung.id == id)
    }

    /// Invariants a ladder must satisfy to be drivable. A ladder that breaks one doesn't crash — it
    /// quietly stops a rung being reachable, or reads a rung's dial off the wrong record, which is
    /// exactly the failure mode config validation exists to catch.
    pub fn validate(&self) -> Result<(), LadderConfigError> {
        validate_knowledge(&self.knowledge)?;

        let mut seen_ids: HashSet<(RungBranch, &str)> = HashSet::new();
        let mut seen_orders: HashSet<(RungBranch, u32)> = HashSet::new();

        for rung in &self.rungs {
            let where_ = format!("rungs[{}:{}]", rung.branch.as_str(), rung.id);

            if !seen_ids.insert((rung.branch, rung.id.as_str())) {
                return Err(LadderConfigError::Invalid {
                    field: where_,
                    constraint: "name each rung of a branch exactly once — a duplicate id makes \
                                 every by-name lookup ambiguous"
                        .to_string(),
                    value: format!("id '{}' appears twice", rung.id),
                });
            }
            if !seen_orders.insert((rung.branch, rung.order)) {
                return Err(LadderConfigError::Invalid {
                    field: where_,
                    constraint: "give each rung of a branch its own order — two rungs on one step \
                                 have no defined sequence"
                        .to_string(),
                    value: format!("order {} appears twice", rung.order),
                });
            }
            self.validate_sequence(rung, &where_)?;
            validate_links(rung, &where_)?;
            validate_build(rung, &where_)?;
            validate_upkeep(rung, &where_)?;
            validate_site_requirement(rung, &where_)?;
        }

        for branch in [RungBranch::Plant, RungBranch::Animal] {
            let roots = self
                .rungs
                .iter()
                .filter(|rung| rung.branch == branch && rung.order == FIRST_RUNG_ORDER)
                .count();
            if roots != 1 {
                return Err(LadderConfigError::Invalid {
                    field: format!("rungs[{}]", branch.as_str()),
                    constraint: format!(
                        "give the branch exactly one order-{FIRST_RUNG_ORDER} rung — the wild \
                         source every ladder starts from"
                    ),
                    value: format!("{roots} rungs at order {FIRST_RUNG_ORDER}"),
                });
            }
        }

        // **Every lesson the sim can teach must be PRICED.** Run after the rung loop, so the names
        // are already known to resolve to real discoveries.
        self.validate_lesson_cost_coverage()?;

        // Every rung a system reaches for by name must exist, so `rung()` is infallible and an
        // override can't silently delete a shipped rung out from under the engine.
        for key in RungKey::ALL {
            if self.find(key.branch(), key.id()).is_none() {
                return Err(LadderConfigError::Invalid {
                    field: format!("rungs[{}:{}]", key.branch().as_str(), key.id()),
                    constraint: "define every rung the simulation drives by name (see RungKey)"
                        .to_string(),
                    value: "missing".to_string(),
                });
            }
        }
        Ok(())
    }

    /// **Every knowledge the sim teaches has a price** — each rung's `earns_knowledge` and each of
    /// the crafts ([`crate::crafting::CRAFTS_WITH_A_DISCOVERY`], the coded set a bench can teach).
    ///
    /// **A missing entry is a load failure, not a silent default.** A defaulted lesson would be paced
    /// by whatever the fallback happened to be — a number nobody chose, on a knowledge nobody could
    /// find the dial for — which is the parked-`0` failure mode in a new costume. `knowledge_accrual`
    /// and `credit_craft_lesson` therefore both read the map as total.
    fn validate_lesson_cost_coverage(&self) -> Result<(), LadderConfigError> {
        let taught = self
            .rungs
            .iter()
            .filter_map(|rung| rung.earns_knowledge.as_deref());
        for name in taught.chain(crate::crafting::CRAFTS_WITH_A_DISCOVERY) {
            if self.knowledge.lesson_cost(name).is_none() {
                return Err(LadderConfigError::Invalid {
                    field: format!("knowledge.lesson_costs[{name}]"),
                    constraint: "price every knowledge the sim can teach — a rung's \
                                 `earns_knowledge` and every craft. A missing cost would pace the \
                                 lesson off a default nobody chose"
                        .to_string(),
                    value: "missing".to_string(),
                });
            }
        }
        Ok(())
    }

    /// The ladder is strictly sequential: rung 1 requires nothing, and every rung above it names the
    /// rung directly below it (same branch, `order - 1`). A ladder with a hole in it — or with a rung
    /// pointing at something two steps down — is not a ladder (`plan_intensification_ladder.md` §4).
    /// See [`RungDef::requires_rung`] for why this is a claim about the *ladder's shape* and not a
    /// per-source precondition (which is each verb's own coded gate — and which `Sow` deliberately
    /// does not have).
    fn validate_sequence(&self, rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
        match (rung.order, rung.requires_rung.as_deref()) {
            (FIRST_RUNG_ORDER, None) => Ok(()),
            (FIRST_RUNG_ORDER, Some(requires)) => Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: format!(
                    "leave `requires_rung` null on the order-{FIRST_RUNG_ORDER} rung — the wild \
                     source sits on nothing"
                ),
                value: format!("requires_rung = '{requires}'"),
            }),
            (_, None) => Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "name the rung below it in `requires_rung` — the ladder is sequential"
                    .to_string(),
                value: format!("order {} with requires_rung = null", rung.order),
            }),
            (order, Some(requires)) => {
                let below = self.find(rung.branch, requires);
                match below {
                    Some(below) if below.order == order - 1 => Ok(()),
                    Some(below) => Err(LadderConfigError::Invalid {
                        field: where_.to_string(),
                        constraint: "require the rung directly below it (order - 1) — the ladder \
                                     has no skipped steps"
                            .to_string(),
                        value: format!(
                            "order {order} requires '{requires}' at order {}",
                            below.order
                        ),
                    }),
                    None => Err(LadderConfigError::Invalid {
                        field: where_.to_string(),
                        constraint: "require a rung that exists on the same branch".to_string(),
                        value: format!("requires_rung = '{requires}'"),
                    }),
                }
            }
        }
    }
}

/// The order of the wild rung every branch starts from.
const FIRST_RUNG_ORDER: u32 = 1;

/// **The knowledge ids the ladder may name** — the bounded coded set, mirroring the behavior
/// primitives: the ladder links to knowledge by *name*, and a name the sim has no discovery for is a
/// typo that would silently ungate a rung. A new rung's new knowledge codes its id once here (and in
/// `data/start_profile_knowledge_tags.json`), after which it is config.
fn discovery_id_for(name: &str) -> Option<u32> {
    match name {
        "cultivation" => Some(CULTIVATION_DISCOVERY_ID),
        "herding" => Some(HERDING_DISCOVERY_ID),
        // Slice 4's two new rung-3 gates. `seed_selection`'s *consumer* (the `Field`/`Sow` rung) is
        // slice 5 — it is earned now and spent later, which is the pacing model working as intended,
        // not a dangling name.
        "seed_selection" => Some(SEED_SELECTION_DISCOVERY_ID),
        "penning" => Some(PENNING_DISCOVERY_ID),
        // Flora Roster F3 — the `animal:pen` rung's `earns_knowledge`. Running a pen teaches
        // Foddering, which unlocks the fodder-draw (feed + `K_pen` term); it gates no rung of its own.
        "foddering" => Some(FODDERING_DISCOVERY_ID),
        // **The three CRAFTS** (`crafting.rs`). They are not ladder rungs and nothing here earns
        // them — a bench does, per item completed. They are named in this lookup for the same
        // reason the ladder's five are: it is the sim's one bounded set of knowledge names, and a
        // knowledge reachable by no name is a knowledge no start profile could ever grant and no
        // config could ever reference.
        name => crate::crafting::craft_discovery_id(name),
    }
}

/// Bound the ladder's knowledge dials. Both failure modes are silent rather than loud — the ladder
/// parses, and then either every gate is open from turn 1 or none can ever open — which is exactly
/// what config validation is for (the `FaunaConfig::validate` discipline these bounds were moved from,
/// where each web asserted its own copy of them).
fn validate_knowledge(knowledge: &LadderKnowledge) -> Result<(), LadderConfigError> {
    if !knowledge.learn_rate.is_finite() || knowledge.learn_rate <= 0.0 {
        return Err(LadderConfigError::Invalid {
            field: "knowledge".to_string(),
            constraint: "pay a positive amount of practice per worked turn — at `learn_rate <= 0` \
                         no knowledge is ever learned and the whole ladder silently freezes at \
                         rung 1"
                .to_string(),
            value: format!("learn_rate = {}", knowledge.learn_rate),
        });
    }
    for (name, cost) in &knowledge.lesson_costs {
        if !cost.is_finite() || *cost <= 0.0 {
            return Err(LadderConfigError::Invalid {
                field: format!("knowledge.lesson_costs[{name}]"),
                constraint:
                    "cost a positive amount of practice — a free lesson is known before it \
                             is learned, so every gate it holds is open on turn 1"
                        .to_string(),
                value: format!("{cost}"),
            });
        }
    }
    if !knowledge.completion_threshold.is_finite()
        || knowledge.completion_threshold <= 0.0
        || knowledge.completion_threshold > 1.0
    {
        return Err(LadderConfigError::Invalid {
            field: "knowledge".to_string(),
            constraint: "complete at a threshold in (0, 1] — at 0 every knowledge is known before \
                         it is learned, and above 1 no gate can ever open (the ledger clamps \
                         accrual to 1.0)"
                .to_string(),
            value: format!("completion_threshold = {}", knowledge.completion_threshold),
        });
    }
    if !knowledge.craft_lesson_per_item.is_finite() || knowledge.craft_lesson_per_item <= 0.0 {
        return Err(LadderConfigError::Invalid {
            field: "knowledge".to_string(),
            constraint: "pay a positive amount of practice per item crafted — at \
                         `craft_lesson_per_item <= 0` no craft is ever learned and every tool stays \
                         permanently unreachable"
                .to_string(),
            value: format!(
                "craft_lesson_per_item = {}",
                knowledge.craft_lesson_per_item
            ),
        });
    }
    Ok(())
}

/// The verb (when named) has to be a real improvement, and the knowledge links (when named) real
/// discoveries — otherwise the rung is unreachable in a way nothing on the map would explain.
fn validate_links(rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
    if let Some(verb) = rung.verb.as_deref() {
        if Improvement::from_str(verb).is_err() {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "name a real Improvement in `verb` (or null for a rung no verb drives)"
                    .to_string(),
                value: format!("verb = '{verb}'"),
            });
        }
    }
    for (field, knowledge) in [
        ("unlock_knowledge", rung.unlock_knowledge.as_deref()),
        ("earns_knowledge", rung.earns_knowledge.as_deref()),
    ] {
        let Some(knowledge) = knowledge else { continue };
        if discovery_id_for(knowledge).is_none() {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: format!(
                    "name a knowledge the sim has a discovery for in `{field}` (see \
                     `discovery_id_for`)"
                ),
                value: format!("{field} = '{knowledge}'"),
            });
        }
    }
    Ok(())
}

/// Bound the site requirement, in the same spirit as the build dials: its failure modes are **silent**
/// — the rung parses, and then either it can be placed on ground it was meant to refuse, or on none at
/// all. Since scarcity is the whole point of the rule, a requirement that quietly stops constraining is
/// exactly the bug this catches.
fn validate_site_requirement(rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
    let Some(site) = rung.site_requirement.as_ref() else {
        return Ok(());
    };
    if !site.min_forage_capacity.is_finite() || site.min_forage_capacity < NO_FORAGE_CAPACITY {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: format!(
                "set a finite fertility floor of at least {NO_FORAGE_CAPACITY} — the floor is                  compared against a tile's `forage.capacity_by_biome`, which is never negative"
            ),
            value: format!("min_forage_capacity = {}", site.min_forage_capacity),
        });
    }
    if site.min_forage_capacity <= NO_FORAGE_CAPACITY
        && !site.requires_fresh_water
        && !site.requires_gathering_site
    {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "require SOMETHING of the site, or state `site_requirement: null` — a                          requirement that admits every tile reads as a placement rule while being                          none, which is how a rung's scarcity silently evaporates"
                .to_string(),
            value: "min_forage_capacity = 0 with requires_fresh_water = false and \
                    requires_gathering_site = false"
                .to_string(),
        });
    }
    Ok(())
}

/// Bound the build dials whose `0`/inverted value would silently disable the rung rather than fail
/// loudly (the `FaunaConfig::validate` discipline).
fn validate_build(rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
    let Some(build) = rung.build.as_ref() else {
        return Ok(());
    };
    if !build.work_cost.is_finite() || build.work_cost <= 0.0 {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "cost a positive amount of work — a zero/negative `work_cost` makes the \
                         rung free the turn any crew touches it, silently"
                .to_string(),
            value: format!("work_cost = {}", build.work_cost),
        });
    }
    // The grace may not outlast the build itself: forgive neglect for longer than it took to raise
    // the rung and the penalty never fires within the span anyone would notice — the mechanic
    // evaporates without a word, which is the failure every bound in this function guards against.
    //
    // **The reference build is ONE WORKER.** Turns are an output, so "how long does this rung take"
    // has no single answer — it depends on the hands put on it — and a bound needs one anyway. A
    // single worker is the **most forgiving** reading (the longest possible build, so the loosest
    // possible bound), which is the safe direction for a guard whose job is to catch a grace that
    // swallows the whole build. It read `crew_needed.unwrap_or(1)` until the rung stopped declaring a
    // crew at all (`docs/plan_standing_upkeep.md` §2.2); the bound loosened and every shipped rung
    // still clears it by an order of magnitude (`the_shipped_graces_clear_the_loosened_bound`).
    let reference_output = SOLE_BUILDER as f32 * PER_WORKER_OUTPUT;
    let build_turns = build.work_cost / reference_output;
    if let Some(grace) = build.grace_turns {
        if (grace as f32) >= build_turns {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "forgive fewer turns of neglect than the rung takes to build at its \
                             own crew — a grace that outlasts its own build makes walking away \
                             free, silently"
                    .to_string(),
                value: format!(
                    "grace_turns = {grace} against a {build_turns}-turn build (work_cost = {} at a \
                     reference output of {reference_output}/turn)",
                    build.work_cost
                ),
            });
        }
    }
    Ok(())
}

/// **The upkeep block's bounds** — stated here, beside the build's, because
/// [`LadderConfig::validate`] already owns every ladder bound and a rate that describes both webs
/// belongs to the ladder.
fn validate_upkeep(rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
    let Some(upkeep) = rung.upkeep.as_ref() else {
        return Ok(());
    };
    if !upkeep.work_per_turn.is_finite() || upkeep.work_per_turn <= 0.0 {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "demand a positive, finite amount of work per turn — a rung that costs \
                         nothing to hold says so with `upkeep: null` rather than a `0` that reads \
                         like a live dial, exactly as `decay_fraction_per_turn` does"
                .to_string(),
            value: format!("upkeep.work_per_turn = {}", upkeep.work_per_turn),
        });
    }
    if let Some(decay) = upkeep.meter_decay.as_ref() {
        if !decay.per_turn.is_finite() || decay.per_turn <= 0.0 {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "rot at a positive, finite rate — a rung that never rots says so by \
                             declaring no `meter_decay` at all, rather than with a `0` that reads \
                             like a live dial"
                    .to_string(),
                value: format!("upkeep.meter_decay.per_turn = {}", decay.per_turn),
            });
        }
        if !decay.retain_fraction.is_finite()
            || !(FULLY_SUPPLIED..=WHOLLY_UNSUPPLIED).contains(&decay.retain_fraction)
        {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "hold the rung at a fraction of its own cost within 0..=1 — above 1 \
                             revokes the rung on the turn it completes, which is the very defect \
                             the retention bar exists to fix"
                    .to_string(),
                value: format!(
                    "upkeep.meter_decay.retain_fraction = {}",
                    decay.retain_fraction
                ),
            });
        }
    }
    // **The upkeep's grace may not outlast the rung's own build either**, and for the identical
    // reason the build's bound exists: forgive shortfall for longer than it took to raise the rung
    // and holding it is free over the whole span anyone would notice. The reference build is again
    // [`SOLE_BUILDER`], the most forgiving reading. A rung with an upkeep and no build (a route, in a
    // later slice) has no span to compare against, so it is unbounded here.
    if let Some(build) = rung.build.as_ref() {
        let build_turns = build.work_cost / (SOLE_BUILDER as f32 * PER_WORKER_OUTPUT);
        if (upkeep.grace_turns as f32) >= build_turns {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "forgive fewer turns of shortfall than the rung takes to build — an \
                             upkeep grace that outlasts its own build makes holding the rung free, \
                             silently"
                    .to_string(),
                value: format!(
                    "upkeep.grace_turns = {} against a {build_turns}-turn build",
                    upkeep.grace_turns
                ),
            });
        }
    }
    Ok(())
}

/// **THE knowledge gate.** Does `faction` know `discovery` well enough to act on it — i.e. has its
/// ledger progress reached the completion `threshold`? The single source of the check that used to
/// sit inlined at five call sites (both labor arms, the `cultivate`/`corral` assignment validators,
/// and `extend_pen`), each spelling `get_progress(..) >= threshold` for itself.
///
/// `threshold` stays a **parameter** (rather than being read off the ladder in here) to keep the
/// helper a pure comparison with no config lookup of its own — but there is now exactly **one** value
/// any caller passes: [`LadderKnowledge::completion_threshold`]. The per-web copies it used to
/// reconcile (`labor_config`'s `forage.cultivation`, `fauna_config`'s `husbandry`) are **gone** —
/// slice 4 moved them onto the ladder when the earn path became one rung-driven seam.
pub fn knows(
    ledger: &DiscoveryProgressLedger,
    faction: FactionId,
    discovery: u32,
    threshold: f32,
) -> bool {
    ledger.get_progress(faction, discovery) >= scalar_from_f32(threshold)
}

#[derive(Debug, Error)]
pub enum LadderConfigError {
    #[error("failed to read intensification ladder from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse intensification ladder: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid intensification ladder: {field} must {constraint} (was {value})")]
    Invalid {
        field: String,
        constraint: String,
        value: String,
    },
}

impl ConfigLoadError for LadderConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the intensification ladder.
#[derive(Resource, Debug, Clone)]
pub struct LadderConfigHandle(pub Arc<LadderConfig>);

impl LadderConfigHandle {
    pub fn new(config: Arc<LadderConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<LadderConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<LadderConfig>) {
        self.0 = config;
    }
}

impl Default for LadderConfigHandle {
    fn default() -> Self {
        Self(LadderConfig::builtin())
    }
}

/// Metadata about the intensification ladder source.
#[derive(Resource, Debug, Clone, Default)]
pub struct LadderConfigMetadata {
    path: Option<PathBuf>,
}

impl LadderConfigMetadata {
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

/// Load the ladder from environment (`INTENSIFICATION_LADDER_PATH`) or the default data path. The
/// ladder is **validated** on load, and a broken invariant is as fatal as a parse error.
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `INTENSIFICATION_LADDER_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_intensification_ladder_from_env() -> (Arc<LadderConfig>, LadderConfigMetadata) {
    let (config, source) = load_config_from_env(
        "INTENSIFICATION_LADDER_PATH",
        "intensification_ladder",
        "src/data/intensification_ladder.json",
        LadderConfig::builtin,
        LadderConfig::from_file,
    );
    (config, LadderConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// **The floor at which [`learn_multiplier`] is exactly ×1.0** — the food peak. Every accrual
    /// assertion that is *not about the floor* passes it, so the call reads the crew's own output
    /// rather than a floor's fraction of it. That is the normalisation's whole point: the 25-turn
    /// Cultivate is still 25 turns here.
    const FOOD_PEAK_FLOOR: f32 = crate::fauna::MSY_BIOMASS_FRACTION;

    /// **Two hands** — the shipped `plant:tended` crew, and the crew every budget assertion that is
    /// *not about the head count* passes, so a share reads as a share rather than as a rounding of
    /// one worker.
    const A_CREW_OF_TWO: u32 = 2;

    /// **THE STAFFING EVERY ACCRUAL ASSERTION BELOW USES** — the rung's keeper count plus a hand,
    /// held at that number only so the readings recorded against it stay comparable.
    ///
    /// **The rate is no longer a tax on building** (`docs/plan_standing_upkeep.md` §4.6a): a build
    /// crew supplies nothing toward it, so a lone worker banks its whole [`PER_WORKER_OUTPUT`] on
    /// every managed rung and this crew banks exactly `workers × PER_WORKER_OUTPUT`.
    fn reference_crew(rung: &RungDef) -> u32 {
        rung.upkeep_crew_needed(UNSCALED_UPKEEP)
            .saturating_add(SOLE_BUILDER)
    }

    /// **WHAT A CREW OF `workers` ACTUALLY BANKS ON THIS RUNG** — its whole output, which is what
    /// `build_accrual` answers now that the rate is nobody's tax but the keeping pool's
    /// (`docs/plan_standing_upkeep.md` §4.6a). Stated once here so every assertion below reads the
    /// model rather than restating the arithmetic.
    fn expected_net(_rung: &RungDef, workers: u32) -> f32 {
        activity_work(workers)
    }

    /// **What one turn of the reference crew at the food peak with no gear produces** — the accrual
    /// every build-length assertion divides the rung's cost by.
    fn reference_accrual(rung: &RungDef) -> f32 {
        rung.build_accrual(rung.verb_improvement(), true, reference_crew(rung))
    }

    /// Mutate the builtin ladder JSON and expect `validate` (inside `from_json_str`) to reject it —
    /// the `FaunaConfig::validate` rejection-test convention, one case per bound.
    fn reject(mutate: impl FnOnce(&mut Value)) -> LadderConfigError {
        let mut json: Value =
            serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER).expect("builtin parses as json");
        mutate(&mut json);
        LadderConfig::from_json_str(&json.to_string())
            .expect_err("mutated ladder should be rejected")
    }

    fn assert_rejects(err: LadderConfigError, expect_field: &str) {
        match err {
            LadderConfigError::Invalid { field, .. } => assert!(
                field.contains(expect_field),
                "expected a rejection naming '{expect_field}', got '{field}'"
            ),
            other => panic!("expected an Invalid rejection, got {other:?}"),
        }
    }

    /// Index of a rung record in the builtin's `rungs` array.
    fn rung_index(json: &Value, branch: &str, id: &str) -> usize {
        json["rungs"]
            .as_array()
            .expect("rungs is an array")
            .iter()
            .position(|rung| rung["branch"] == branch && rung["id"] == id)
            .unwrap_or_else(|| panic!("builtin defines the {branch}:{id} rung"))
    }

    #[test]
    fn builtin_ladder_parses_and_validates() {
        let ladder = LadderConfig::builtin();
        // Both branches, all five coded rungs.
        for key in RungKey::ALL {
            let rung = ladder.rung(key);
            assert_eq!(rung.branch, key.branch());
            assert_eq!(rung.id, key.id());
        }
    }

    /// The ladder must describe **what the sim does today**, not the target model — later slices
    /// change behaviour by editing it. Pin the current truth so a drifting edit is caught here.
    #[test]
    fn builtin_ladder_describes_todays_rungs() {
        let ladder = LadderConfig::builtin();

        // Rung 1 already teaches (§0: practice-earns-knowledge is shipped on both tracks) but is
        // driven by no verb — you don't *build* a wild source.
        let plant_wild = ladder.rung(RungKey::PlantWild);
        assert_eq!(plant_wild.verb_improvement(), None);
        assert_eq!(
            plant_wild.earns_discovery_id(),
            Some(CULTIVATION_DISCOVERY_ID)
        );
        let animal_wild = ladder.rung(RungKey::AnimalWild);
        assert_eq!(animal_wild.verb_improvement(), None);
        assert_eq!(animal_wild.earns_discovery_id(), Some(HERDING_DISCOVERY_ID));

        // Plant rung 2 — the shipped Cultivate investment, gated on Cultivation, and (slice 4)
        // **teaching Seed Selection**: practise the tended patch, learn to select seed.
        let tended = ladder.rung(RungKey::PlantTended);
        assert_eq!(tended.verb_improvement(), Some(Improvement::Cultivate));
        assert_eq!(tended.unlock_discovery_id(), Some(CULTIVATION_DISCOVERY_ID));
        assert_eq!(
            tended.earns_discovery_id(),
            Some(SEED_SELECTION_DISCOVERY_ID)
        );

        // Animal rung 2 — the `Tame` investment: an explicit, Herding-gated, *paid* verb. This is
        // the conflation fix (§4.1): the rung is driven by its own verb, not by a Sustain harvest.
        // Slice 4: practising it **teaches Penning**, the gate on rung 3.
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        assert_eq!(pastoral.verb_improvement(), Some(Improvement::Tame));
        assert_eq!(pastoral.unlock_discovery_id(), Some(HERDING_DISCOVERY_ID));
        assert_eq!(pastoral.earns_discovery_id(), Some(PENNING_DISCOVERY_ID));
        assert_eq!(pastoral.ceiling_required, Some(HusbandryCeiling::Pastoral));
        // Taming still costs hands, like every other rung — the `domesticate` early-claim that let
        // a player skip this investment is gone. It costs them **outright** now rather than through a
        // declared dip: the `tame` verb takes a crew, and those hands are not hunting.
        assert!(
            pastoral.build.is_some(),
            "the pastoral rung is an investment — it has a build meter to staff"
        );
        assert_eq!(
            pastoral.build_accrual(Some(Improvement::Tame), true, A_CREW_OF_TWO,),
            expected_net(pastoral, A_CREW_OF_TWO),
            "…and its crew's output goes into that meter, less the maintenance rate it is also \
             paying — the rate is owed while building too"
        );

        // Animal rung 3 — the shipped Corral investment, fenced only by a `pen`-ceiling species and,
        // since the §4.3 reshuffle (slice 4), gated on **Penning** rather than Herding: one
        // knowledge per transition. Since Flora Roster F3 it TEACHES **Foddering** (2007) — running a
        // pen is how you learn to hay one (`selective_breeding`, rung 4's lesson, stays parked).
        let pen = ladder.rung(RungKey::AnimalPen);
        assert_eq!(pen.verb_improvement(), Some(Improvement::Corral));
        assert_eq!(pen.unlock_discovery_id(), Some(PENNING_DISCOVERY_ID));
        assert_eq!(pen.earns_discovery_id(), Some(FODDERING_DISCOVERY_ID));
        assert_eq!(pen.ceiling_required, Some(HusbandryCeiling::Pen));

        // **The §4.3 invariant, stated as one assertion: every transition has its OWN knowledge.**
        // Herding gating both `tame` and `corral` was the pre-slice-4 defect; a future rung that
        // re-used a gate would be the same defect returning, and this catches it.
        let unlocks: Vec<u32> = ladder
            .rungs
            .iter()
            .filter_map(|rung| rung.unlock_discovery_id())
            .collect();
        let mut unique = unlocks.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unlocks.len(),
            unique.len(),
            "one knowledge per transition — no two rungs may share an unlock gate"
        );
    }

    /// The engine drives a rung only through its own verb, and only when the caller's gates hold.
    #[test]
    fn build_accrual_is_driven_by_the_rungs_own_verb() {
        let ladder = LadderConfig::builtin();
        let tended = ladder.rung(RungKey::PlantTended);
        let build = tended.build.as_ref().expect("tended rung builds");

        // The crew IS the throughput, with nothing netted off it: the maintenance rate is the
        // keeping pool's whatever the builders do (`docs/plan_standing_upkeep.md` §4.6a).
        let crew = A_CREW_OF_TWO;
        assert_eq!(
            tended.build_accrual(Some(Improvement::Cultivate), true, crew),
            expected_net(tended, crew)
        );
        // Wrong verb → nothing, even though the crew is working the patch.
        assert_eq!(
            tended.build_accrual(Some(Improvement::Sow), true, crew),
            0.0
        );
        // No improvement at all → nothing. A crew that is only harvesting builds nothing, whatever
        // its stance.
        assert_eq!(tended.build_accrual(None, true, crew), 0.0);
        // Right verb, gate lapsed → nothing accrues (progress is neither lost nor advanced).
        assert_eq!(
            tended.build_accrual(Some(Improvement::Cultivate), false, crew),
            0.0
        );
        // The cost is the job, in absolute units.
        assert_eq!(tended.build_cost(RUNG_COST_UNSCALED), Some(build.work_cost));
        // **The rung declares a standing upkeep**, and it is what a fully unmaintained patch bleeds:
        // shortfall IS the decay (`docs/plan_standing_upkeep.md` §2.4), so there is no second dial.
        assert!(tended.upkeep_demand(UNSCALED_UPKEEP) > NO_UPKEEP_DEMAND);
        // **Its neglect is counted in SHORTFALL turns, not un-worked ones**, so the build's own
        // grace is absent rather than parked at a value nothing reads.
        assert_eq!(build.grace_turns, None);
        assert!(tended.upkeep_grace_turns() > NO_NEGLECT_GRACE);
    }

    /// **A bigger crew finishes the same job sooner, with no cap** — the arc's whole claim at the
    /// seam (`docs/plan_unit_costed_work.md` §1.2). The retired `crew_scale` capped the accrual at
    /// the rung's stated rate, so over-crewing bought nothing; it now buys turns, on **every** rung
    /// including the two animal ones that were crew-*blind*.
    #[test]
    fn a_bigger_crew_finishes_the_same_job_sooner_on_every_rung() {
        let ladder = LadderConfig::builtin();
        for key in [
            RungKey::PlantTended,
            RungKey::PlantField,
            RungKey::AnimalPastoral,
            RungKey::AnimalPen,
        ] {
            let rung = ladder.rung(key);
            let work = |workers| rung.build_accrual(rung.verb_improvement(), true, workers);
            assert_eq!(work(0), 0.0, "{key:?}: nobody working, nothing built");
            // **THERE IS NO MINIMUM VIABLE CREW ANY MORE** (`docs/plan_standing_upkeep.md` §4.6a).
            // The maintenance rate used to be netted off here, so a crew at or below it banked
            // nothing; the keeping pool owes that rate whatever the builders do, so **one hand banks
            // one worker-turn on every rung**, however dear the rung is to hold.
            assert_eq!(
                work(SOLE_BUILDER),
                PER_WORKER_OUTPUT,
                "{key:?}: a lone builder is worth one worker-turn — the rate is not its bill"
            );
            assert_eq!(
                work(20),
                expected_net(rung, 20),
                "{key:?}: twenty hands produce twenty units — there is no crew cap"
            );
            // Turns are the OUTPUT: the same fixed cost, twice the crew, half the turns.
            let cost = rung
                .build_cost(RUNG_COST_UNSCALED)
                .expect("the rung builds");
            assert_eq!(
                build_turns_remaining(cost, RUNG_UNSTARTED, work(A_CREW_OF_TWO)),
                build_turns_remaining(cost, RUNG_UNSTARTED, work(SOLE_BUILDER))
                    .map(|turns| turns.div_ceil(2)),
                "{key:?}: doubling the crew halves the turns — `work_cost / crew` is the pace again"
            );
            // **Only an EMPTY crew has no estimate**, never a small one: what a build can fail to
            // out-run is the meter's rot, and that is the countdown's business
            // ([`RungDef::build_balance`]), not the accrual's.
            assert_eq!(
                build_turns_remaining(cost, RUNG_UNSTARTED, work(0)),
                None,
                "{key:?}: nobody on the job has no finish date"
            );
        }
    }

    /// **A TOOL'S SAVING IS A SHARE OF THE JOB, so it fades as the job grows** — the arithmetic §6.1
    /// turns on, and the reason the contribution is subtracted from the cost rather than multiplied
    /// into the crew.
    ///
    /// A **multiplier** would save the same *percentage* of turns whatever the job costs
    /// (`turns_geared / turns_bare = w / (w + h)`, the cost cancels). Subtracted, the same 17 units
    /// are a third of a 50-unit garden and a seventeenth of a 300-unit farm — **and the tool never
    /// mentions either improvement by name**. The fade is therefore *config's* job (a Farm is born
    /// large), not a property of the rule.
    #[test]
    fn a_tools_saving_shrinks_as_the_job_grows() {
        let ladder = LadderConfig::builtin();
        /// A fully-geared reference keeper crew: two hands, `husbandry_gear`'s 8.5 each.
        const GEAR_WORK: f32 = 17.0;
        /// The shipped `plant:tended` cost, and a stand-in for the ~300-unit Farm rung 4 is born at.
        const GARDEN: f32 = 50.0;
        const FARM: f32 = 300.0;

        let saved = |cost: f32| (cost - ladder.effective_build_cost(cost, GEAR_WORK)) / cost;
        assert!(
            (saved(GARDEN) - GEAR_WORK / GARDEN).abs() < 1e-6,
            "the tool takes a THIRD off a garden: {}",
            saved(GARDEN)
        );
        assert!(
            saved(FARM) * 3.0 < saved(GARDEN),
            "and is noise against a farm — under a third of the share, not the same share: {} vs {}",
            saved(FARM),
            saved(GARDEN)
        );
    }

    /// **THE GEAR IS A SUM OVER THE CREW, NEVER AN AVERAGE** (§6.2) — the partly-equipped-party rule
    /// the carries already run on, and the property that lets the cost-side stat be
    /// coverage-weighted where the retired multiplier could not be.
    #[test]
    fn the_gear_contribution_sums_over_the_crew_and_floors_at_neutral() {
        const PER_WORKER: f32 = 8.5;
        assert_eq!(build_work_from_gear(PER_WORKER, 0), NO_BUILD_GEAR);
        assert_eq!(build_work_from_gear(PER_WORKER, 2), 2.0 * PER_WORKER);
        assert_eq!(build_work_from_gear(PER_WORKER, 20), 20.0 * PER_WORKER);
        assert_eq!(
            build_work_from_gear(NO_BUILD_GEAR, 20),
            NO_BUILD_GEAR,
            "a crew carrying nothing that helps takes nothing off, however many of them there are"
        );
        // A negative contribution cannot *add* work — `EquipmentConfig::validate` rejects one, and
        // this is the arithmetic's own guard.
        assert_eq!(build_work_from_gear(-5.0, 2), NO_BUILD_GEAR);
    }

    /// **A FULLY-GEARED ANIMAL BUILD IS UNMOVED, and that is this slice's pacing proof** — the
    /// calibration §6.2 sets `husbandry_gear`'s 8.5 by: the retired `build_rate` ×1.5 on a 50-unit
    /// job at the reference keeper crew of 2 saved 8.33 of 25 turns, i.e. it was worth ≈17 units of
    /// the job, which is 8.5 **per worker**.
    #[test]
    fn a_fully_geared_reference_crew_tames_in_the_turns_the_retired_multiplier_gave() {
        let ladder = LadderConfig::builtin();
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        /// The reference keeper crew the animal costs were priced against — see
        /// `the_food_peak_preserves_every_rungs_stated_build_length`.
        const KEEPERS: u32 = 2;
        /// What `husbandry_gear`'s flint tier declares, per equipped worker.
        const PER_WORKER: f32 = 8.5;
        /// What the retired ×1.5 multiplier bought at this crew: `ceil(50 / (2 × 1.5))`.
        const GEARED_TURNS: u32 = 17;

        let cost = pastoral
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the pastoral rung builds");
        // **The gear is resolved at the reference KEEPERS, and so is the crew.** The calibration is
        // about what `husbandry_gear` is worth *in units of the job* (17); the crew's own supply is
        // the 2 worker-turns it was priced against, with nothing netted off it (§4.6a).
        let accrual = pastoral.build_accrual(pastoral.verb_improvement(), true, KEEPERS);
        let bar = ladder.effective_build_cost(cost, build_work_from_gear(PER_WORKER, KEEPERS));
        assert_eq!(
            build_turns_remaining(bar, RUNG_UNSTARTED, accrual),
            Some(GEARED_TURNS),
            "a fully-geared reference keeper crew must finish where the multiplier left it"
        );
        // And a HALF-geared crew is honestly slower, which it was not before: the multiplier was
        // resolved uncovered, so one set of hurdles among the crew bought the whole ×1.5.
        let half = ladder.effective_build_cost(cost, build_work_from_gear(PER_WORKER, 1));
        let half_turns =
            build_turns_remaining(half, RUNG_UNSTARTED, accrual).expect("a staffed build finishes");
        assert!(
            half_turns > GEARED_TURNS,
            "half the crew equipped must take longer than all of it: {half_turns} vs \
             {GEARED_TURNS}"
        );
    }

    /// **The turns estimate is `ceil(remaining / this turn's work)`, and a STALL has no estimate.**
    /// `None` is the wire's [`NO_BUILD_TURNS_ESTIMATE`], and a stall is the only thing that earns it:
    /// a build nobody is advancing cannot be quoted a finish date, and a huge number would read as a
    /// promise.
    ///
    /// **A bar already at or below the meter is `1`, NOT `None`** — the work is banked, so the job
    /// finishes the first turn anybody works it. That is the case a well-geared crew reaches through
    /// an unfloored [`LadderConfig::effective_build_cost`], and conflating it with *"no answer"* is
    /// what made the estimate vanish as hands were added.
    #[test]
    fn the_turns_estimate_rounds_up_and_declines_to_quote_a_stall() {
        const COST: f32 = 50.0;
        assert_eq!(build_turns_remaining(COST, RUNG_UNSTARTED, 2.0), Some(25));
        assert_eq!(build_turns_remaining(COST, 40.0, 3.0), Some(4), "rounds up");
        assert_eq!(
            build_turns_remaining(COST, 49.9, 2.0),
            Some(1),
            "the last sliver is still a turn"
        );
        assert_eq!(
            build_turns_remaining(COST, COST, 2.0),
            Some(BUILD_FINISHES_IN_ONE_TURN),
            "a job whose work is already banked finishes on the next worked turn"
        );
        assert_eq!(
            build_turns_remaining(-1.0, RUNG_UNSTARTED, 2.0),
            Some(BUILD_FINISHES_IN_ONE_TURN),
            "and so does one the crew's gear paid off outright — the bar is below zero, which is an \
             ANSWER"
        );
        assert_eq!(
            build_turns_remaining(COST, 10.0, 0.0),
            None,
            "a stalled build has no finite estimate"
        );
    }

    /// **AN OVER-GEARED CREW IS QUOTED ONE TURN, NOT "NO ESTIMATE"** — the projection half, on the
    /// shipped roster and at a crew a real band can staff, because that is where the defect was
    /// visible: the compose sheet is by definition looking at a rung nobody has started.
    ///
    /// Six keepers each holding handling gear take `6 × 8.5 = 51` work units off a 50-unit `Tame`,
    /// so [`LadderConfig::effective_build_cost`] — which is deliberately unfloored — hands
    /// [`build_turns_remaining`] a bar below zero. The quote must fall to `1` and stop there rather
    /// than disappearing at exactly the crew size that demonstrates *add hands and watch it drop*.
    #[test]
    fn a_crew_whose_gear_pays_the_job_off_is_quoted_one_turn_not_no_estimate() {
        const PER_WORKER: f32 = 8.5;
        /// The `taming_cost_multiplier` of a species that costs exactly the rung's own price — the
        /// regime the shipped roster's rabbit, fowl, crag goat, wild sheep and snow hare are all in.
        const UNSCALED_SPECIES: f32 = RUNG_COST_UNSCALED;
        let ladder = LadderConfig::builtin();
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        let cost = pastoral
            .build_cost(UNSCALED_SPECIES)
            .expect("the pastoral rung builds");

        let quote = |keepers: u32| {
            ladder.projected_build_turns(
                pastoral,
                UNSCALED_SPECIES,
                RUNG_UNSTARTED,
                keepers,
                PER_WORKER,
                true,
                UNSCALED_UPKEEP,
            )
        };

        // The fixture is only meaningful if the gear genuinely over-pays the job.
        let over_geared = 6;
        assert!(
            build_work_from_gear(PER_WORKER, over_geared) >= cost,
            "fixture: {over_geared} keepers' gear must cover the whole {cost}-unit job"
        );
        assert_eq!(
            quote(over_geared),
            Some(BuildTurns::Turns(BUILD_FINISHES_IN_ONE_TURN)),
            "gear that pays the job off outright finishes it on the first worked turn"
        );
        // **And a crew EXACTLY at the maintenance rate is quoted HOLDING, not silence** — a
        // projection states the same standing fact a running build does. The *rotting* half is
        // unreachable on this rung with whole hands (its demand is `1.0`, so the only staffing
        // under the rate is no staffing at all, which is `None` by design); the plant web's
        // demand of `2.0` is where that arm is pinned — `build_turns_on_the_wire.rs`.
        assert_eq!(
            quote(pastoral.upkeep_crew_needed(UNSCALED_UPKEEP)),
            Some(BuildTurns::Holding),
            "a quoted crew that exactly pays the rate holds the meter — and says so"
        );

        // And the quote is monotone into that floor rather than falling off it: each added hand
        // shortens the job until it cannot be shortened further. **Measured from the first crew that
        // clears the maintenance rate** — below it there is no quote at all, by design.
        let threshold = pastoral.upkeep_crew_needed(UNSCALED_UPKEEP);
        let first = threshold + 1;
        let finite = |keepers: u32| -> u32 {
            match quote(keepers) {
                Some(BuildTurns::Turns(turns)) => turns,
                other => panic!("a crew of {keepers} above the rate must quote a count: {other:?}"),
            }
        };
        let mut previous = finite(first);
        for keepers in (first + 1)..=over_geared {
            let turns = finite(keepers);
            assert!(
                turns <= previous,
                "adding a hand must never lengthen the quote: {turns} at {keepers} vs {previous}"
            );
            previous = turns;
        }
    }

    /// A rung with **no verb** is never driven — the `wild` rungs, which are nothing to *build*:
    /// you take what is there. (Retargeted from `pastoral`, which used to be the verbless example
    /// because taming accrued implicitly off a Sustain hunt; it now has the `tame` verb, so `wild`
    /// is what is left to make this point with.)
    #[test]
    fn a_verbless_rung_is_never_driven() {
        let ladder = LadderConfig::builtin();
        for key in [RungKey::AnimalWild, RungKey::PlantWild] {
            let wild = ladder.rung(key);
            for improvement in [
                None,
                Some(Improvement::Cultivate),
                Some(Improvement::Sow),
                Some(Improvement::Tame),
                Some(Improvement::Corral),
            ] {
                assert_eq!(
                    wild.build_accrual(improvement, true, reference_crew(wild)),
                    0.0
                );
            }
            assert_eq!(wild.build_cost(RUNG_COST_UNSCALED), None);
            assert_eq!(wild.upkeep_demand(UNSCALED_UPKEEP), NO_UPKEEP_DEMAND);
        }
    }

    /// **Sustain is not a taming verb** — the §4.1 de-conflation, asserted at the engine seam: the
    /// `pastoral` rung's meter advances under `Tame` and under nothing else. The sim-level twin of
    /// this (a Sustain hunt leaves `domestication_progress` at zero) lives in the labor tests.
    #[test]
    fn the_pastoral_rung_is_driven_by_tame_and_only_by_tame() {
        let ladder = LadderConfig::builtin();
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        let build = pastoral.build.as_ref().expect("the pastoral rung builds");

        assert_eq!(
            pastoral.build_accrual(Some(Improvement::Tame), true, reference_crew(pastoral),),
            expected_net(pastoral, reference_crew(pastoral))
        );
        assert_eq!(
            pastoral.build_cost(RUNG_COST_UNSCALED),
            Some(build.work_cost)
        );
        for improvement in [
            None,
            Some(Improvement::Cultivate),
            Some(Improvement::Sow),
            Some(Improvement::Corral),
        ] {
            assert_eq!(
                pastoral.build_accrual(improvement, true, reference_crew(pastoral),),
                0.0,
                "{improvement:?} must not tame a herd — only Tame does"
            );
        }
        // Right verb, gate lapsed → nothing accrues (progress is neither lost nor advanced).
        assert_eq!(
            pastoral.build_accrual(Some(Improvement::Tame), false, reference_crew(pastoral),),
            0.0
        );
    }

    /// **Every rung that can be neglected declares a grace, and the two webs are not monotone in the
    /// same direction** — which is the whole reason the dial is per-rung rather than global. On
    /// plants the *newest* rung is the most fragile (a standing crop wants hands every turn; the
    /// cleared ground under it keeps its clearing longer); on animals the *highest* is the most
    /// forgiving (the fence does the holding). Asserted as the two orderings, not as four literals,
    /// so a retune moves the numbers without moving the claim.
    #[test]
    fn the_neglect_grace_is_per_rung_and_the_two_webs_disagree_about_its_direction() {
        let ladder = LadderConfig::builtin();
        // **Every branch is asked on the one trigger there is** — consecutive turns of unmet
        // upkeep (`docs/plan_standing_upkeep.md` §2.4). The penalties still differ in kind (a plant
        // meter bleeds, an animal flock sheds); the grace that gates them does not.
        let tended = ladder.rung(RungKey::PlantTended).upkeep_grace_turns();
        let field = ladder.rung(RungKey::PlantField).upkeep_grace_turns();
        let pastoral = ladder.rung(RungKey::AnimalPastoral).upkeep_grace_turns();
        let pen = ladder.rung(RungKey::AnimalPen).upkeep_grace_turns();

        assert!(
            field < tended,
            "a standing crop is more fragile than the cleared ground under it: {field} vs {tended}"
        );
        assert!(
            pastoral < pen,
            "the fence buys TURNS as well as a slower rate: {pastoral} vs {pen}"
        );
        for grace in [tended, field, pastoral, pen] {
            assert!(
                grace > NO_NEGLECT_GRACE,
                "every buildable rung forgives something"
            );
        }
        // A rung with nothing built on it has nothing to forgive, on either trigger.
        for key in [RungKey::PlantWild, RungKey::AnimalWild] {
            assert_eq!(ladder.rung(key).neglect_grace_turns(), NO_NEGLECT_GRACE);
            assert_eq!(ladder.rung(key).upkeep_grace_turns(), NO_NEGLECT_GRACE);
        }
        // **No shipped rung counts un-worked BUILD turns any more** — the trigger the plant branch
        // retired in slice 3 and the animal branch in slice 4.
        for key in RungKey::ALL {
            assert_eq!(
                ladder.rung(key).neglect_grace_turns(),
                NO_NEGLECT_GRACE,
                "{}:{} still declares a build grace",
                key.branch().as_str(),
                key.id()
            );
        }
    }

    /// **EVERY BUILT RUNG COSTS WORK TO HOLD, and every one counts SHORTFALL turns**
    /// (`docs/plan_standing_upkeep.md` §2.4). The two halves are one statement: a rung with a
    /// standing cost is one whose neglect is measured as unmet demand, so `build.grace_turns` — the
    /// *un-worked-build* trigger — is `null` on all four and the live grace is the upkeep's.
    ///
    /// The two webs pay the penalty in different currencies (a plant meter bleeds, an animal flock
    /// sheds) and quote the rate in different units (`flat` per patch, `source_load` per keeper-load),
    /// which is exactly what the scale term exists to express — but *what* is owed, and *when* it
    /// starts costing, is now one mechanism for both.
    ///
    /// Its predecessor asserted a split about the retired `decay_fraction_per_turn`, which the upkeep
    /// replaced outright: two dials described one mechanic and could disagree, giving a rung that
    /// bled faster than it cost to hold.
    #[test]
    fn every_built_rung_declares_a_standing_upkeep_and_none_counts_unworked_turns() {
        let ladder = LadderConfig::builtin();
        for key in [
            RungKey::PlantTended,
            RungKey::PlantField,
            RungKey::AnimalPastoral,
            RungKey::AnimalPen,
        ] {
            let rung = ladder.rung(key);
            assert!(
                rung.declares_upkeep()
                    && rung.upkeep_demand(A_TWENTY_HEAD_FLOCK) > NO_UPKEEP_DEMAND,
                "{}:{} costs work to hold",
                key.branch().as_str(),
                key.id()
            );
            assert!(
                rung.upkeep_grace_turns() > NO_NEGLECT_GRACE,
                "{}:{} forgives some shortfall before the penalty bites",
                key.branch().as_str(),
                key.id()
            );
            assert_eq!(
                rung.build.as_ref().and_then(|build| build.grace_turns),
                None,
                "{}:{} counts shortfall turns, so the build's own grace would be a second number \
                 nothing reads",
                key.branch().as_str(),
                key.id()
            );
        }
        // **The scale term is where the two webs differ**, and that is the whole of the difference:
        // a patch is one tile (`flat`), a herd is as many keeper-loads as it has animals.
        assert_eq!(
            ladder
                .rung(RungKey::PlantTended)
                .upkeep
                .as_ref()
                .map(|upkeep| upkeep.scaled_by),
            Some(UpkeepScale::Flat)
        );
        assert_eq!(
            ladder
                .rung(RungKey::AnimalPen)
                .upkeep
                .as_ref()
                .map(|upkeep| upkeep.scaled_by),
            Some(UpkeepScale::SourceLoad)
        );
    }

    // **RETIRED: `the_plant_rungs_declare_a_build_crew_and_the_animal_rungs_do_not`** — the two
    // plant rungs declared a `crew_needed` and the two animal ones did not, because a herd's crew
    // came from its size. **No rung declares one now** (`docs/plan_standing_upkeep.md` §2.2): the
    // player states a build's staffing, so there is no rung-level number left to be asymmetric
    // about.

    /// **The published countdown reads without any subtraction**: `0` means the penalty is biting
    /// *now*, `N > 0` means it starts in `N` more un-worked turns, and a source nobody has neglected
    /// reads the full grace plus one ("walk away and you have this long").
    #[test]
    fn the_neglect_countdown_is_zero_exactly_when_the_penalty_bites() {
        const GRACE: u32 = 2;
        assert_eq!(neglect_grace_remaining(0, GRACE), GRACE + 1);
        assert_eq!(neglect_grace_remaining(1, GRACE), GRACE);
        // The last forgiven turn: one turn left, and nothing has been lost yet.
        assert_eq!(neglect_grace_remaining(GRACE as u16, GRACE), 1);
        // The first turn the penalty applies (`neglect > grace`) — and every turn after it.
        assert_eq!(neglect_grace_remaining(GRACE as u16 + 1, GRACE), 0);
        assert_eq!(neglect_grace_remaining(u16::MAX, GRACE), 0);
    }

    /// A grace that outlasts its own build makes walking away free for longer than it took to build
    /// — the penalty evaporating silently, which is the failure every bound in `validate_build`
    /// guards against.
    #[test]
    fn rejects_a_grace_that_outlasts_its_own_build() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            // The reference build is one builder against a 50-unit job — 50 turns — since the rung
            // stopped declaring a crew to measure it at (`docs/plan_standing_upkeep.md` §2.2). The
            // bound loosened; a grace of 50 still swallows the whole build.
            json["rungs"][idx]["build"]["grace_turns"] = (50).into();
        });
        assert_rejects(err, "plant:tended");
    }

    /// **The shipped rungs clear the grace bound**, and that is worth pinning positively: the bound
    /// moved from `1 / progress_per_turn` to `work_cost / reference_output`, so retuning a cost
    /// silently changes what a grace is allowed to be.
    #[test]
    fn every_shipped_grace_is_shorter_than_its_own_reference_build() {
        let ladder = LadderConfig::builtin();
        for key in [
            RungKey::PlantTended,
            RungKey::PlantField,
            RungKey::AnimalPastoral,
            RungKey::AnimalPen,
        ] {
            let rung = ladder.rung(key);
            let turns = build_turns_remaining(
                rung.build_cost(RUNG_COST_UNSCALED)
                    .expect("the rung builds"),
                RUNG_UNSTARTED,
                reference_accrual(rung),
            )
            .expect("a staffed build finishes");
            assert!(
                rung.neglect_grace_turns() < turns,
                "{key:?}: grace {} must be shorter than its {turns}-turn reference build",
                rung.neglect_grace_turns()
            );
        }
    }

    // **RETIRED: `rejects_a_build_crew_of_nobody`** — the bound on `crew_needed`, which retired with
    // the field (`docs/plan_standing_upkeep.md` §2.2): the player states a build's staffing, so there
    // is no rung-level floor left to be zero.

    // **RETIRED with the dial they bounded: `rejects_a_decay_that_bleeds_the_whole_job_in_a_turn`,
    // `rejects_a_zero_build_decay_in_favour_of_null` and `rejects_negative_decay`** — all three
    // guarded `RungBuild::decay_fraction_per_turn`, which the upkeep replaced outright
    // (`docs/plan_standing_upkeep.md` §2.4). What an improvement loses is now what its keepers did
    // not supply, so the guards that matter are the upkeep's own: a positive finite rate
    // (`rejects_an_upkeep_of_nothing`) and a grace that cannot outlast the build
    // (`rejects_an_upkeep_grace_that_outlasts_its_own_build`).

    #[test]
    fn rejects_a_duplicate_rung_id() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["id"] = "wild".into();
        });
        assert_rejects(err, "plant:wild");
    }

    #[test]
    fn rejects_a_duplicate_rung_order() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["order"] = (2).into();
        });
        assert_rejects(err, "animal:pen");
    }

    /// Every branch needs its wild source. (A branch that merely *renumbers* its rungs is caught
    /// earlier and more precisely by the sequential check; this guard is what's left — a branch with
    /// no ladder at all, which would otherwise read as "this food web cannot be intensified" with
    /// nothing on the map to explain it.)
    #[test]
    fn rejects_a_branch_without_exactly_one_first_rung() {
        let err = reject(|json| {
            let rungs = json["rungs"].as_array_mut().expect("array");
            rungs.retain(|rung| rung["branch"] != "plant");
        });
        assert_rejects(err, "rungs[plant]");
    }

    #[test]
    fn rejects_a_first_rung_that_requires_something() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "wild");
            json["rungs"][idx]["requires_rung"] = "pastoral".into();
        });
        assert_rejects(err, "animal:wild");
    }

    #[test]
    fn rejects_a_rung_that_requires_nothing_below_it() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["requires_rung"] = Value::Null;
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_a_rung_that_skips_a_step() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["requires_rung"] = "wild".into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_rung_requiring_a_rung_that_does_not_exist() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["requires_rung"] = "paddock".into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_verb_that_is_not_an_improvement() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["verb"] = "plough".into();
        });
        assert_rejects(err, "plant:tended");
    }

    // Both knowledge-link rejection tests used to reach for `penning` / `seed_selection` as their
    // *unknown* name. Slice 4 coded both, so they need a name that is still genuinely unmapped —
    // `selective_breeding` (rung 4, §6: named in the design, deliberately not built).
    #[test]
    fn rejects_an_unknown_unlock_knowledge() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["unlock_knowledge"] = "selective_breeding".into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_an_unknown_earns_knowledge() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "wild");
            json["rungs"][idx]["earns_knowledge"] = "irrigation".into();
        });
        assert_rejects(err, "plant:wild");
    }

    /// A rung that costs nothing is free the turn any crew touches it — the silent-disable failure
    /// mode, one axis over from a rate of zero.
    #[test]
    fn rejects_a_free_rung() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["work_cost"] = (0.0).into();
        });
        assert_rejects(err, "plant:tended");
    }

    // **RETIRED: `rejects_a_free_investment` / `rejects_a_starving_investment`** — the two bounds on
    // `yield_fraction_while_building` (`0 < f < 1`). The dial is gone: an investment's cost is the
    // crew's whole turn now, which is neither free nor tunable, so there is no fraction left to bound.
    // What replaced them is the upkeep block's own bound below, guarding the same failure mode (a
    // number that reads like a live dial while meaning "no cost at all").

    /// **A parked `0` upkeep is rejected in favour of `null`** — the exact rule
    /// `decay_fraction_per_turn` follows, and for the exact reason: `work_per_turn: 0` means *"this
    /// rung costs nothing to hold"* while **reading** like a dial someone chose, which is how
    /// `animal:pastoral`'s dead `0.01` decay survived for slices documenting a mechanic the sim did
    /// not have.
    #[test]
    fn rejects_a_parked_zero_upkeep_in_favour_of_null() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["upkeep"] = serde_json::json!({
                "work_per_turn": 0.0,
                "scaled_by": "flat",
                "grace_turns": 0,
            });
        });
        assert_rejects(err, "animal:pen");
    }

    /// **A negative upkeep would PAY the crew for holding the thing.** Same bound, other side.
    #[test]
    fn rejects_a_negative_upkeep() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["upkeep"] = serde_json::json!({
                "work_per_turn": -1.0,
                "scaled_by": "source_load",
                "grace_turns": 2,
            });
        });
        assert_rejects(err, "animal:pen");
    }

    /// **`scaled_by` must name a coded primitive** — the `behavior` idiom, where an unknown token is
    /// a parse failure rather than a silent default. A scale nobody coded would otherwise resolve to
    /// whatever `serde` fell back to, which is a demand nobody chose.
    #[test]
    fn rejects_an_unknown_upkeep_scale() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["upkeep"] = serde_json::json!({
                "work_per_turn": 1.0,
                "scaled_by": "route_length",
                "grace_turns": 0,
            });
        });
        assert!(
            matches!(err, LadderConfigError::Parse(_)),
            "an unknown scale primitive must fail the PARSE, not resolve to a default: {err:?}"
        );
    }

    /// **A RUNG WITH NOTHING BUILT ON IT COSTS NOTHING TO HOLD**, on both webs — the honest zero
    /// rather than a sentinel. The two plant rungs are the only shipped records that declare an
    /// upkeep (`only_the_plant_rungs_declare_a_standing_upkeep` pins that split); what this asserts
    /// is the floor of the model, which no later slice may move: you cannot be billed for a wild
    /// stand or a wild herd.
    #[test]
    fn a_wild_rung_costs_nothing_to_hold() {
        let ladder = LadderConfig::builtin();
        for key in [RungKey::PlantWild, RungKey::AnimalWild] {
            let rung = ladder.rung(key);
            assert!(
                rung.upkeep.is_none(),
                "{}:{} declares an upkeep — there is nothing built there to hold",
                key.branch().as_str(),
                key.id()
            );
            assert_eq!(
                rung.upkeep_demand(UNSCALED_UPKEEP),
                NO_UPKEEP_DEMAND,
                "…so its demand is the honest zero"
            );
            assert_eq!(
                rung.upkeep_crew_needed(UNSCALED_UPKEEP),
                NO_CREW_ON_THIS_ACTIVITY,
                "…and nobody is needed to keep it"
            );
        }
    }

    /// **AN UPKEEP GRACE MAY NOT OUTLAST THE RUNG'S OWN BUILD**, the twin of the build grace's bound
    /// and for the identical reason: forgive shortfall for longer than it took to raise the rung and
    /// holding it is free over the whole span anyone would notice. The reference is one builder, the
    /// most forgiving reading.
    #[test]
    fn rejects_an_upkeep_grace_that_outlasts_its_own_build() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            // The tended rung is 50 work units, i.e. 50 turns at one builder.
            json["rungs"][idx]["upkeep"]["grace_turns"] = (50).into();
        });
        assert_rejects(err, "plant:tended");
    }

    // ---- The three allocations, and the work each produces -----------------------------------

    /// **A herd big enough to want real keeping**, in head — the scale term
    /// [`UpkeepScale::SourceLoad`] reads. Chosen well above one so a scaled demand cannot be mistaken
    /// for a flat one.
    const A_TWENTY_HEAD_FLOCK: f32 = 20.0;

    /// **A standing demand of one worker-turn**, so a crew of two covers it with a hand to spare and
    /// a crew of one covers it exactly — the two readings a shortfall test needs either side of.
    const A_DEMAND_OF_ONE_WORKER_TURN: f32 = 1.0;

    /// **THE PLAYER STATES THE SPLIT** (`docs/plan_standing_upkeep.md` §2.2). Each activity's crew
    /// produces its own work directly — no pool, no priority order, no derived share — so the three
    /// numbers are independent and each is exactly `workers × PER_WORKER_OUTPUT`.
    #[test]
    fn each_activitys_crew_produces_its_own_work() {
        assert_eq!(
            activity_work(A_CREW_OF_TWO),
            A_CREW_OF_TWO as f32 * PER_WORKER_OUTPUT
        );
        assert_eq!(
            activity_work(NO_CREW_ON_THIS_ACTIVITY),
            NO_UPKEEP_DEMAND,
            "nobody on an activity produces nothing for it — which is the whole of 'off'"
        );
        // **It does not read the floor**, and that is the change: a build crew is not pulling on the
        // source, and a source nobody is harvesting has no floor for them to read.
        let ladder = LadderConfig::builtin();
        let rung = ladder.rung(RungKey::PlantTended);
        // Above the rung's maintenance rate, or the assertion would be that zero equals zero.
        let a_crew_above_the_rate = rung
            .upkeep_crew_needed(UNSCALED_UPKEEP)
            .saturating_add(A_CREW_OF_TWO);
        #[allow(non_snake_case)]
        let A_CREW_ABOVE_THE_RATE = a_crew_above_the_rate;
        for floor in [STRIP_IT_BARE_FLOOR, 0.15, FOOD_PEAK_FLOOR, 0.8, 1.0] {
            assert_eq!(
                rung.build_accrual(Some(Improvement::Cultivate), true, A_CREW_ABOVE_THE_RATE,),
                expected_net(rung, A_CREW_ABOVE_THE_RATE),
                "the build banks the same net whatever floor the gatherers hold ({floor})"
            );
        }
    }

    /// **THE FLOOR CAME OFF THE BUILD RATE, AND THE SHIPPED PACE IS UNCHANGED.** `learn_multiplier`
    /// is exactly `×1.0` at the food peak — the floor a fresh assignment gets — so every rung's
    /// stated build length is what it was; only sub-peak floors build faster now.
    ///
    /// **This is the slice's own proof**, and it is asserted against the retired arithmetic rather
    /// than against a remembered number: `crew × PER_WORKER_OUTPUT × learn_multiplier(peak)` is what
    /// the accrual used to be, and it must still equal what it is.
    #[test]
    fn taking_the_floor_off_the_build_rate_is_pacing_neutral_at_the_food_peak() {
        assert_eq!(
            learn_multiplier(FOOD_PEAK_FLOOR),
            1.0,
            "the premise: the food peak is the multiplier's identity"
        );
        let ladder = LadderConfig::builtin();
        for key in RungKey::ALL {
            let rung = ladder.rung(key);
            let Some(verb) = rung.verb_improvement() else {
                continue;
            };
            for crew in [SOLE_BUILDER, A_CREW_OF_TWO, 10] {
                let now = rung.build_accrual(Some(verb), true, crew);
                // The retired arithmetic, verbatim — and nothing is netted off it, because the
                // maintenance rate never was a term in a build's accrual once §4.6a deleted the
                // fullness test that made the builders supply it. What is compared is the floor's
                // term alone, which is what this proof is about.
                let before = crew as f32 * PER_WORKER_OUTPUT * learn_multiplier(FOOD_PEAK_FLOOR);
                assert_eq!(
                    now,
                    before,
                    "{}:{} at {crew} builders must bank what it banked at the food peak",
                    key.branch().as_str(),
                    key.id()
                );
            }
        }
    }

    /// **A crew that leaves nothing standing** — `learn_multiplier(0.0)` is `0`. It used to zero the
    /// build; it is now a knowledge term only, so a stripping *gatherer* beside a builder no longer
    /// stalls the build.
    const STRIP_IT_BARE_FLOOR: f32 = 0.0;

    /// **THE UPKEEP IS COVERED BY ITS OWN CREW, AND WHAT IS LEFT IS THE SHORTFALL** (§2.4). No
    /// ordering, no pool: `supplied = maintain_workers × PER_WORKER_OUTPUT`, and the rest goes unmet.
    #[test]
    fn the_maintain_crew_covers_the_demand_and_the_rest_is_shortfall() {
        // Two keepers against a one-worker-turn demand: covered, with a hand to spare.
        let supplied = activity_work(A_CREW_OF_TWO);
        assert_eq!(
            upkeep_shortfall(A_DEMAND_OF_ONE_WORKER_TURN, supplied),
            NO_UPKEEP_DEMAND,
            "a demand more than met leaves NOTHING short — never a negative, which would read as \
             the source paying its keepers"
        );
        // Nobody on the keeping: the whole demand goes unmet, which is the whole of "stop
        // maintaining this".
        assert_eq!(
            upkeep_shortfall(
                A_DEMAND_OF_ONE_WORKER_TURN,
                activity_work(NO_CREW_ON_THIS_ACTIVITY)
            ),
            A_DEMAND_OF_ONE_WORKER_TURN,
            "assigning zero keepers is how a player says 'let it go' — the demand still stands"
        );
        // Half-staffed: it slides at half rate, which is the continuity the retired binary
        // `tended_this_turn` flag could not express.
        const A_DEMAND_OF_TWO_WORKER_TURNS: f32 = 2.0;
        assert_eq!(
            upkeep_shortfall(A_DEMAND_OF_TWO_WORKER_TURNS, activity_work(SOLE_BUILDER)),
            A_DEMAND_OF_ONE_WORKER_TURN
        );
    }

    /// **THE THREE ALLOCATIONS ARE INDEPENDENT.** Staffing a build takes nothing from the take, and
    /// staffing the keeping takes nothing from either — the competition is for *hands*, which the
    /// band's own pool expresses, not for a share of one crew's turn.
    #[test]
    fn the_three_allocations_do_not_scale_one_another() {
        let ladder = LadderConfig::builtin();
        let rung = ladder.rung(RungKey::PlantTended);
        const A_BIG_BUILD_CREW: u32 = 50;
        assert_eq!(
            rung.build_accrual(
                Some(Improvement::Cultivate),
                true,
                A_BIG_BUILD_CREW,
            ),
            expected_net(rung, A_BIG_BUILD_CREW),
            "there is no cap on a build's crew: fifty hands finish a Cultivate in a turn, less the \
             rate they are also paying"
        );
        // And the upkeep's crew is priced by the same unit, so a reader can add the two and get the
        // band's real commitment to this source.
        assert_eq!(
            activity_work(A_BIG_BUILD_CREW) / A_BIG_BUILD_CREW as f32,
            PER_WORKER_OUTPUT,
            "one worker is worth one work unit on every activity — one currency, three claims"
        );
    }

    /// **HANDS TO MEET THE DEMAND** — the maintain activity's own `workers_needed`, and `0` for a
    /// rung that costs nothing to hold.
    ///
    /// **THE SHIPPED PLANT DEMANDS ARE WHOLE NUMBERS** (`2` and `4`), which is what the retune bought:
    /// a player can staff a tended patch *exactly*, where the retired sub-worker demands (`0.5` /
    /// `0.75`) rounded up to one hand and threw the rest of that hand away — the waste the band-level
    /// pool now has no way to create.
    #[test]
    fn the_upkeep_crew_needed_is_the_demand_in_whole_workers() {
        let ladder = LadderConfig::builtin();
        for key in RungKey::ALL {
            let rung = ladder.rung(key);
            let demand = rung.upkeep_demand(UNSCALED_UPKEEP);
            let expected = if rung.declares_upkeep() {
                demand.ceil() as u32
            } else {
                NO_CREW_ON_THIS_ACTIVITY
            };
            assert_eq!(
                rung.upkeep_crew_needed(UNSCALED_UPKEEP),
                expected,
                "{}:{} — nobody is needed to hold something that costs nothing to hold, and a rung \
                 that does asks for its demand in whole hands",
                key.branch().as_str(),
                key.id()
            );
        }
        // A fractional demand still wants a whole worker — you cannot send half a keeper.
        const HALF_A_WORKER_TURN: f32 = 0.5;
        let rung = rung_with_upkeep(HALF_A_WORKER_TURN, UpkeepScale::Flat, NO_NEGLECT_GRACE);
        assert_eq!(rung.upkeep_crew_needed(UNSCALED_UPKEEP), 1);
        let per_head = rung_with_upkeep(
            HALF_A_WORKER_TURN,
            UpkeepScale::SourceLoad,
            NO_NEGLECT_GRACE,
        );
        assert_eq!(per_head.upkeep_crew_needed(A_TWENTY_HEAD_FLOCK), 10);
    }

    /// **THE SHIPPED GRACES CLEAR THE LOOSENED BOUND.** `grace_turns < work_cost / reference_output`
    /// lost its `crew_needed` divisor when the rung stopped declaring a crew, so the reference is one
    /// builder and the bound got *looser*. Confirmed rather than assumed, and by an order of
    /// magnitude on every rung.
    #[test]
    fn the_shipped_graces_clear_the_loosened_bound() {
        let ladder = LadderConfig::builtin();
        for key in RungKey::ALL {
            let rung = ladder.rung(key);
            let Some(build) = rung.build.as_ref() else {
                continue;
            };
            let turns_at_one_builder = build.work_cost / (SOLE_BUILDER as f32 * PER_WORKER_OUTPUT);
            // **Whichever trigger the rung counts on** — the build's un-worked turns (the animal
            // branch) or the upkeep's shortfall turns (the plant branch) — the bound is the same and
            // both are checked, so neither branch can grow a grace that outlasts its own build.
            for grace in [
                build.grace_turns,
                rung.upkeep.as_ref().map(|u| u.grace_turns),
            ]
            .into_iter()
            .flatten()
            {
                assert!(
                    (grace as f32) < turns_at_one_builder,
                    "{}:{} forgives {grace} turns against a {turns_at_one_builder}-turn \
                     one-builder job",
                    key.branch().as_str(),
                    key.id(),
                );
            }
        }
    }

    /// **THE DECAY IS THE SHORTFALL FRACTION AT THE RUNG'S OWN RATE, past the upkeep's own grace**
    /// (§2.4) — three dials answering three questions, where *shortfall was the decay* welded the
    /// demand to the rot rate and made either one unretunable.
    #[test]
    fn the_upkeep_decay_is_the_shortfall_fraction_at_the_rungs_own_rate() {
        const GRACE: u32 = 2;
        /// A rot rate deliberately unequal to the demand beside it — the whole point of the split is
        /// that the two are different numbers, and a fixture that made them equal would pass against
        /// the retired *shortfall is the decay* arithmetic too.
        const ROT_PER_TURN: f32 = 0.25;
        let rung = rung_with_meter_decay(
            A_DEMAND_OF_ONE_WORKER_TURN,
            UpkeepScale::Flat,
            GRACE,
            ROT_PER_TURN,
            A_HALF_FULL_RETENTION_BAR,
        );
        for turns in 0..=GRACE as u16 {
            assert_eq!(
                rung.upkeep_decay(WHOLLY_UNSUPPLIED, turns),
                NO_UPKEEP_DECAY,
                "a grace of {GRACE} forgives turn {turns}"
            );
        }
        assert_eq!(
            rung.upkeep_decay(WHOLLY_UNSUPPLIED, GRACE as u16 + 1),
            ROT_PER_TURN,
            "…and the turn after it, a wholly unmaintained rung loses its own stated rate — never \
             the demand it went short by"
        );
        // **Proportional, which is the other half of the split**: half the hands, half the rot.
        assert_eq!(
            rung.upkeep_decay(WHOLLY_UNSUPPLIED / 2.0, GRACE as u16 + 1),
            ROT_PER_TURN / 2.0,
            "half short is half the rot"
        );
        assert_eq!(
            rung.upkeep_decay(FULLY_SUPPLIED, u16::MAX),
            NO_UPKEEP_DECAY,
            "and a fully staffed rung loses nothing however long it has been staffed"
        );
        // A rung with no upkeep has nothing to bleed, whatever it is handed.
        assert_eq!(
            LadderConfig::builtin()
                .rung(RungKey::PlantWild)
                .upkeep_decay(WHOLLY_UNSUPPLIED, u16::MAX),
            NO_UPKEEP_DECAY
        );
        // **And neither does a rung whose penalty is not a meter bleed** — both animal rungs, whose
        // flock sheds at the husbandry config's own escape fractions instead. A second rate here
        // would be two numbers for one mechanic.
        for key in [RungKey::AnimalPastoral, RungKey::AnimalPen] {
            assert_eq!(
                LadderConfig::builtin()
                    .rung(key)
                    .upkeep_decay(WHOLLY_UNSUPPLIED, u16::MAX),
                NO_UPKEEP_DECAY,
                "{}:{} sheds animals rather than bleeding a meter",
                key.branch().as_str(),
                key.id()
            );
        }
    }

    /// **THE RETENTION BAR IS A FRACTION OF THE RUNG'S OWN COST, and a rung that never rots is held
    /// at the whole of it** — the pre-arc predicate, stated rather than special-cased.
    #[test]
    fn the_retention_bar_is_the_rungs_own_fraction_of_the_job() {
        const A_JOB: f32 = 40.0;
        let rots = rung_with_meter_decay(
            A_DEMAND_OF_ONE_WORKER_TURN,
            UpkeepScale::Flat,
            NO_NEGLECT_GRACE,
            A_DEMAND_OF_ONE_WORKER_TURN,
            A_HALF_FULL_RETENTION_BAR,
        );
        assert_eq!(rots.retention_bar(A_JOB), A_JOB * A_HALF_FULL_RETENTION_BAR);
        assert_eq!(
            rung_with_upkeep(A_DEMAND_OF_ONE_WORKER_TURN, UpkeepScale::Flat, 0)
                .retention_bar(A_JOB),
            A_JOB,
            "a rung whose meter never bleeds is held at its whole cost — nothing tests the bar"
        );
        // And so is a rung with no upkeep at all.
        assert_eq!(
            LadderConfig::builtin()
                .rung(RungKey::PlantWild)
                .retention_bar(A_JOB),
            A_JOB
        );
    }

    /// **A POOL HAS NO LEFTOVER, UNDER EITHER MODE** (§2.5) — and that is the whole reason
    /// maintenance left the tile. An indivisible per-source supplier wastes whatever it does not
    /// spend, once per source, and the waste grows as gear makes a hand worth more.
    #[test]
    fn a_short_pool_is_spent_whole_under_both_modes() {
        /// Three demands that do not divide evenly into the pool, so a mode that rounded anywhere
        /// would leave a remainder this test can see.
        const DEMANDS: [f32; 3] = [2.0, 4.0, 1.0];
        /// Well under the `7.0` those demands total.
        const SHORT_POOL: f32 = 3.0;

        for mode in [UpkeepFundMode::Spread, UpkeepFundMode::Priority] {
            let shares = distribute_upkeep_pool(SHORT_POOL, &DEMANDS, mode);
            let spent: f32 = shares.iter().sum();
            assert!(
                (spent - SHORT_POOL).abs() < 1e-6,
                "{mode:?}: the whole pool is spent — {shares:?} sums to {spent}"
            );
            for (share, demand) in shares.iter().zip(DEMANDS) {
                assert!(
                    *share <= demand + 1e-6,
                    "{mode:?}: no source is funded past what it asked for — {share} against {demand}"
                );
            }
        }
    }

    /// **THE TWO MODES ANSWER DIFFERENTLY, and each answers what it is named for** (§2.5).
    #[test]
    fn spread_degrades_everything_and_priority_funds_in_order() {
        const DEMANDS: [f32; 3] = [2.0, 4.0, 1.0];
        /// Exactly half of the `7.0` total, so `spread`'s coverage is a clean one-half.
        const HALF_THE_TOTAL: f32 = 3.5;

        let spread = distribute_upkeep_pool(HALF_THE_TOTAL, &DEMANDS, UpkeepFundMode::Spread);
        for (share, demand) in spread.iter().zip(DEMANDS) {
            assert!(
                (share - demand / 2.0).abs() < 1e-6,
                "spread holds every source at the same coverage — {share} against {demand}"
            );
        }

        // **Priority funds in SLICE ORDER**, which is the caller's *most-invested first* ordering:
        // the ladder owns the arithmetic and the web owns the ranking, because "most invested" is a
        // per-web reading of a stamped meter cost.
        let priority = distribute_upkeep_pool(HALF_THE_TOTAL, &DEMANDS, UpkeepFundMode::Priority);
        assert_eq!(priority[0], DEMANDS[0], "the first claim is met in full");
        assert!(
            (priority[1] - (HALF_THE_TOTAL - DEMANDS[0])).abs() < 1e-6,
            "the second takes what is left — {}",
            priority[1]
        );
        assert_eq!(
            priority[2], 0.0,
            "and the marginal one rots, which is what the mode is for"
        );
    }

    /// **A pool that covers the total funds everything, and an EMPTY pool funds nothing** — the two
    /// ends, so neither mode can be right only in the middle.
    #[test]
    fn a_sufficient_pool_funds_every_source_and_an_empty_one_funds_none() {
        const DEMANDS: [f32; 3] = [2.0, 4.0, 1.0];
        const TOTAL: f32 = 7.0;

        for mode in [UpkeepFundMode::Spread, UpkeepFundMode::Priority] {
            let full = distribute_upkeep_pool(TOTAL, &DEMANDS, mode);
            for (share, demand) in full.iter().zip(DEMANDS) {
                assert!(
                    (share - demand).abs() < 1e-6,
                    "{mode:?}: a pool that covers the sum meets every demand"
                );
            }
            // And a pool bigger than the sum still funds no more than the sum — the leftover stays
            // in the pool rather than being poured into a source that did not ask for it.
            let ample = distribute_upkeep_pool(TOTAL * 2.0, &DEMANDS, mode);
            assert!(ample.iter().sum::<f32>() <= TOTAL + 1e-6);

            let empty = distribute_upkeep_pool(NO_UPKEEP_DEMAND, &DEMANDS, mode);
            assert!(
                empty.iter().all(|share| *share == NO_UPKEEP_DEMAND),
                "{mode:?}: nobody on the role means nobody is held"
            );
        }
    }

    /// **The mode round-trips its command/wire token, and an unknown one is refused rather than
    /// guessed at** — silently reading a typo as the default would leave a player believing they had
    /// protected their Field.
    #[test]
    fn the_fund_mode_round_trips_its_token_and_refuses_an_unknown_one() {
        for mode in [UpkeepFundMode::Spread, UpkeepFundMode::Priority] {
            assert_eq!(UpkeepFundMode::from_token(mode.as_str()), Some(mode));
        }
        assert_eq!(UpkeepFundMode::from_token("sideways"), None);
        assert_eq!(
            UpkeepFundMode::default(),
            UpkeepFundMode::Spread,
            "an unstated policy singles nobody out"
        );
    }

    /// **The scale term is the generic piece** (§2.6) — `flat` states the rate, `source_load`
    /// multiplies it by the source's own size.
    #[test]
    fn the_upkeep_scale_reads_the_sources_own_measure() {
        let flat = rung_with_upkeep(A_DEMAND_OF_ONE_WORKER_TURN, UpkeepScale::Flat, 0);
        assert_eq!(
            flat.upkeep_demand(A_TWENTY_HEAD_FLOCK),
            A_DEMAND_OF_ONE_WORKER_TURN,
            "a flat rate ignores whatever measure it is handed"
        );
        let per_head = rung_with_upkeep(A_DEMAND_OF_ONE_WORKER_TURN, UpkeepScale::SourceLoad, 0);
        assert_eq!(
            per_head.upkeep_demand(A_TWENTY_HEAD_FLOCK),
            A_DEMAND_OF_ONE_WORKER_TURN * A_TWENTY_HEAD_FLOCK,
            "twice the animals, twice the keeping"
        );
    }

    /// The shipped `animal:pen` record with an `upkeep` block bolted on — the fixture the upkeep
    /// seam's tests judge, so they exercise a **parsed** record rather than a hand-built struct that
    /// could drift from the config's own shape.
    fn rung_with_upkeep(work_per_turn: f32, scaled_by: UpkeepScale, grace_turns: u32) -> RungDef {
        rung_with_optional_meter_decay(work_per_turn, scaled_by, grace_turns, None)
    }

    /// **Half a meter's worth** — the retention bar these fixtures hold a rung at, chosen well clear
    /// of both `0` and `1` so a bar read as either endpoint fails rather than coincides.
    const A_HALF_FULL_RETENTION_BAR: f32 = 0.5;

    /// The same fixture with a `meter_decay` block — a rung whose penalty *is* a meter bleed, which
    /// no animal rung is and which is therefore the shape the plant web ships.
    fn rung_with_meter_decay(
        work_per_turn: f32,
        scaled_by: UpkeepScale,
        grace_turns: u32,
        per_turn: f32,
        retain_fraction: f32,
    ) -> RungDef {
        rung_with_optional_meter_decay(
            work_per_turn,
            scaled_by,
            grace_turns,
            Some(serde_json::json!({
                "per_turn": per_turn,
                "retain_fraction": retain_fraction,
            })),
        )
    }

    fn rung_with_optional_meter_decay(
        work_per_turn: f32,
        scaled_by: UpkeepScale,
        grace_turns: u32,
        meter_decay: Option<serde_json::Value>,
    ) -> RungDef {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER).expect("the builtin parses");
        let idx = rung_index(&json, "animal", "pen");
        json["rungs"][idx]["upkeep"] = serde_json::json!({
            "work_per_turn": work_per_turn,
            "scaled_by": scaled_by.as_str(),
            "grace_turns": grace_turns,
            "meter_decay": meter_decay,
        });
        let ladder = LadderConfig::from_json_str(&json.to_string()).expect("the fixture is valid");
        ladder.rung(RungKey::AnimalPen).clone()
    }

    // --- The ladder's knowledge dials. These bounds (and their rejection tests) **moved here from
    // `fauna_config::validate`** in slice 4, along with the dials themselves: both webs kept an
    // identical copy back when each had its own hard-coded earn site, and the ladder now states each
    // bound **once, for both webs**.

    #[test]
    fn rejects_a_ladder_nobody_could_ever_learn() {
        let err = reject(|json| json["knowledge"]["learn_rate"] = (0.0).into());
        assert_rejects(err, "knowledge");
    }

    /// **A free lesson is known before it is learned**, so every gate it holds opens on turn 1 — the
    /// silent-disable failure the cost side inherits from the rate it replaced.
    #[test]
    fn rejects_a_free_lesson() {
        for bad in [0.0, -1.0] {
            let err = reject(|json| json["knowledge"]["lesson_costs"]["herding"] = (bad).into());
            assert_rejects(err, "knowledge.lesson_costs[herding]");
        }
    }

    /// **Every lesson the sim can teach must be PRICED — a rung's and a bench's alike.** A missing
    /// entry would pace the lesson off a default nobody chose, which is the parked-`0` failure in a
    /// new costume, so it is a load failure instead.
    #[test]
    fn rejects_a_ladder_that_leaves_a_lesson_unpriced() {
        // A rung's own lesson…
        let err = reject(|json| {
            json["knowledge"]["lesson_costs"]
                .as_object_mut()
                .expect("lesson_costs is a map")
                .remove("penning");
        });
        assert_rejects(err, "knowledge.lesson_costs[penning]");
        // …and a craft, which no rung teaches at all.
        let err = reject(|json| {
            json["knowledge"]["lesson_costs"]
                .as_object_mut()
                .expect("lesson_costs is a map")
                .remove("weaving");
        });
        assert_rejects(err, "knowledge.lesson_costs[weaving]");
    }

    #[test]
    fn rejects_a_knowledge_gate_that_is_open_or_shut_from_the_start() {
        // `0` → every knowledge is "known" before it is learned: every gate open on turn 1.
        let err = reject(|json| json["knowledge"]["completion_threshold"] = (0.0).into());
        assert_rejects(err, "knowledge");
        // `> 1` → unreachable, since the ledger clamps accrual to 1.0: no gate could EVER open.
        let err = reject(|json| json["knowledge"]["completion_threshold"] = (1.5).into());
        assert_rejects(err, "knowledge");
    }

    /// The shipped pace: ~20 turns of stewardship per lesson, **and every priced lesson is on it**.
    /// A cost keyed by knowledge could spread them apart silently, so the sweep is over the whole map
    /// rather than over one exemplar.
    #[test]
    fn the_builtin_ladder_is_learned_at_one_shared_pace() {
        /// The turns of practice a shipped lesson takes at the food peak — this slice's pacing
        /// proof, since `learn_rate / lesson_cost` reproduces the retired `progress_per_turn` of
        /// `0.05` exactly.
        const SHIPPED_LESSON_TURNS: f32 = 20.0;
        let ladder = LadderConfig::builtin();
        let knowledge = &ladder.knowledge;
        assert!(knowledge.learn_rate > 0.0);
        assert!(knowledge.completion_threshold > 0.0 && knowledge.completion_threshold <= 1.0);
        assert!(
            !knowledge.lesson_costs.is_empty(),
            "the ladder prices lessons"
        );
        for (name, cost) in &knowledge.lesson_costs {
            let turns = knowledge.completion_threshold / (knowledge.learn_rate / cost);
            assert!(
                (turns - SHIPPED_LESSON_TURNS).abs() < 1e-3,
                "{name} should take ~{SHIPPED_LESSON_TURNS} turns of practice, got {turns}"
            );
        }
    }

    /// **A craft is 5 items, and it reads off the SAME cost a turn-worked lesson does** — the two are
    /// one currency charged on two quanta (`docs/plan_unit_costed_work.md` §4), which is what stops
    /// the bench and the ladder drifting apart.
    #[test]
    fn a_craft_is_learned_in_five_items_at_the_shared_lesson_cost() {
        /// The items a shipped craft takes — unchanged by the currency move (`0.2` of a normalized
        /// threshold and `4.0 / 20` are the same five).
        const SHIPPED_CRAFT_ITEMS: f32 = 5.0;
        let ladder = LadderConfig::builtin();
        let knowledge = &ladder.knowledge;
        for craft in crate::crafting::CRAFTS_WITH_A_DISCOVERY {
            let per_item = knowledge
                .ledger_credit(craft, knowledge.craft_lesson_per_item)
                .expect("validate prices every craft");
            let items = knowledge.completion_threshold / per_item;
            assert!(
                (items - SHIPPED_CRAFT_ITEMS).abs() < 1e-3,
                "{craft} should be learned in ~{SHIPPED_CRAFT_ITEMS} items, got {items}"
            );
        }
    }

    /// **PRACTICE DOES NOT SCALE WITH HANDS, and work does** — the two-currency rule at the seam
    /// (`docs/plan_unit_costed_work.md` §2). A lesson is credited once per source per turn, so a
    /// per-worker rate would let a faction learn ten times faster by piling hands onto one patch.
    ///
    /// Asserted as the **asymmetry**, because either half alone reads like an oversight: the build
    /// seam takes `workers` and moves with it, the lesson seam does not take them at all.
    #[test]
    fn a_lesson_is_paid_per_worked_turn_while_a_build_is_paid_per_worker() {
        let ladder = LadderConfig::builtin();
        let tended = ladder.rung(RungKey::PlantTended);

        let lesson = |_workers: u32| {
            tended
                .knowledge_accrual(FOOD_PEAK_FLOOR, true, &ladder.knowledge)
                .map(|(_, amount)| amount)
        };
        assert_eq!(
            lesson(1),
            lesson(50),
            "fifty hands on one patch learn exactly what one hand does — the seam has no worker \
             term to give them"
        );
        let build = |workers| tended.build_accrual(tended.verb_improvement(), true, workers);
        assert!(
            build(50) > build(1),
            "…while the same fifty hands build fifty times as fast: {} vs {}",
            build(50),
            build(1)
        );
    }

    /// **The earn seam** (§4), asserted at the rung: the lesson is the rung's `earns_knowledge` and
    /// the **amount** is the floor's, normalised so the food peak is ×1.0
    /// (`docs/plan_harvest_floor.md` §3). The sim-level twin (which rung a real hunt/forage resolves
    /// to) lives in the labor tests.
    ///
    /// It replaced `knowledge_earned_is_the_rungs_lesson_gated_on_stewardship_and_health`, whose
    /// subject — a **step** at the food peak, teach above / nothing below — is gone: restraint is a
    /// rate now, so the question "does this floor teach" has no answer and the assertions are about
    /// *how much* instead.
    #[test]
    fn knowledge_accrual_is_the_rungs_lesson_paced_by_the_floor() {
        let ladder = LadderConfig::builtin();
        let base = ladder
            .knowledge
            .ledger_credit("herding", ladder.knowledge.learn_rate)
            .expect("the ladder prices herding");
        let wild = ladder.rung(RungKey::AnimalWild);

        // The normalisation, which is the whole reason the multiplier is divided by the peak: a crew
        // holding the default floor learns at exactly the ladder's stated pace.
        assert_eq!(
            wild.knowledge_accrual(FOOD_PEAK_FLOOR, true, &ladder.knowledge),
            Some((HERDING_DISCOVERY_ID, base)),
            "the food peak is ×1.0 — today's ~20-turn lesson is still ~20 turns there"
        );

        // Strictly increasing in the floor, with a liveness bound beside it: an ordering assertion
        // alone would pass on an accrual that returned the same number everywhere.
        let mut previous = 0.0_f32;
        for floor in [0.1_f32, 0.25, FOOD_PEAK_FLOOR, 0.75, 1.0] {
            let (lesson, amount) = wild
                .knowledge_accrual(floor, true, &ladder.knowledge)
                .expect("a positive floor on a teaching rung earns its lesson");
            assert_eq!(lesson, HERDING_DISCOVERY_ID);
            assert!(
                amount > previous,
                "floor {floor} must learn faster than the one below it ({amount} vs {previous})"
            );
            previous = amount;
        }
        assert!(
            previous > base,
            "and a floor above the peak out-learns it: {previous} vs {base}"
        );

        // **Both degenerate ends.** Stripping teaches nothing because the rate is zero; watching
        // teaches nothing because the caller's `eligible` asks whether anything stands above the
        // floor, and at `1.0` nothing does.
        assert_eq!(
            wild.knowledge_accrual(STRIP_IT_BARE, true, &ladder.knowledge),
            None,
            "stripping the source teaches nothing"
        );
        assert_eq!(
            wild.knowledge_accrual(1.0, false, &ladder.knowledge),
            None,
            "watching teaches nothing — `eligible` is the caller's work predicate"
        );

        // Rung 2 teaches the rung-3 gate — the arc's whole claim, at the seam.
        assert_eq!(
            ladder
                .rung(RungKey::AnimalPastoral)
                .knowledge_accrual(FOOD_PEAK_FLOOR, true, &ladder.knowledge)
                .map(|(lesson, _)| lesson),
            Some(PENNING_DISCOVERY_ID)
        );
        assert_eq!(
            ladder
                .rung(RungKey::PlantTended)
                .knowledge_accrual(FOOD_PEAK_FLOOR, true, &ladder.knowledge)
                .map(|(lesson, _)| lesson),
            Some(SEED_SELECTION_DISCOVERY_ID)
        );
        // The `animal:pen` rung teaches **Foddering** (Flora Roster F3) — running a pen is how you
        // learn to hay one.
        assert_eq!(
            ladder
                .rung(RungKey::AnimalPen)
                .knowledge_accrual(FOOD_PEAK_FLOOR, true, &ladder.knowledge)
                .map(|(lesson, _)| lesson),
            Some(FODDERING_DISCOVERY_ID)
        );
        // A rung that teaches nothing yields nothing even when everything else holds (`plant:field`'s
        // `irrigation`/`rotation` is a parked rung-4 lesson).
        assert_eq!(
            ladder.rung(RungKey::PlantField).knowledge_accrual(
                FOOD_PEAK_FLOOR,
                true,
                &ladder.knowledge
            ),
            None
        );
    }

    /// *"Take everything"* — the floor-`0` end of the dial, named because `0.0` as a bare argument
    /// reads as an absent value rather than as the deliberate instruction it is.
    const STRIP_IT_BARE: f32 = 0.0;

    /// **THE NORMALISATION, PINNED BY NAME: a Cultivate at the food peak still takes 25 turns.**
    ///
    /// This is why [`learn_multiplier`] divides by [`crate::fauna::MSY_BIOMASS_FRACTION`] rather than
    /// being a bare `floor`. The floor a fresh assignment carries
    /// ([`crate::components::DEFAULT_ESCAPEMENT_FLOOR`]) is the peak, so the ladder's shipped build
    /// lengths are the ones a player who touches nothing gets — the whole arc costs no rebalance at
    /// the default. Asserted for **every** rung with a build, so a new one cannot opt out.
    ///
    /// **It is also this slice's own pacing proof.** The costs were chosen so that a rung's reference
    /// crew at the food peak takes exactly the turns it took before improvements were priced in work
    /// (`docs/plan_unit_costed_work.md` §3): 25 turns on every shipped rung.
    #[test]
    fn the_food_peak_preserves_every_rungs_stated_build_length() {
        let ladder = LadderConfig::builtin();
        /// The turns every shipped rung's reference crew takes at the food peak — the pacing this
        /// slice is neutral against.
        const REFERENCE_BUILD_TURNS: u32 = 25;
        // **EVERY reference crew is a FIXTURE now, not a config reading.** No rung declares a crew
        // — the player states a build's staffing (`docs/plan_standing_upkeep.md` §2.2) — so this
        // test names the staffings the shipped `work_cost`s were priced against. `pastoral` takes 2
        // (the claim rung 2 makes on plants: "you manage the wild source in place") and `pen` 3,
        // matching the plant rung that also *places* a source. See `intensification_ladder.json`'s
        // `_comment_costs`.
        const PASTORAL_REFERENCE_CREW: u32 = 2;
        const PEN_REFERENCE_CREW: u32 = 3;
        /// The plant rungs' reference crews, the two that used to be the `crew_needed` the ladder
        /// declared: `tended` was priced against the two hands its patch's wild Sustain gather
        /// wants, `field` against three.
        const TENDED_REFERENCE_CREW: u32 = 2;
        const FIELD_REFERENCE_CREW: u32 = 3;
        for (key, crew) in [
            (RungKey::PlantTended, TENDED_REFERENCE_CREW),
            (RungKey::PlantField, FIELD_REFERENCE_CREW),
            (RungKey::AnimalPastoral, PASTORAL_REFERENCE_CREW),
            (RungKey::AnimalPen, PEN_REFERENCE_CREW),
        ] {
            let rung = ladder.rung(key);
            // **THE REFERENCE CREW IS THE REFERENCE CREW AGAIN** (`docs/plan_standing_upkeep.md`
            // §4.6a): the maintenance rate is not a tax on building, so the staffing that banks the
            // reference `crew` worker-turns of progress is exactly `crew` hands. It briefly had to be
            // `crew` **plus** the hands the rate took, which is the reading this slice deleted.
            let accrual = rung.build_accrual(rung.verb_improvement(), true, crew);
            assert_eq!(
                accrual,
                crew as f32 * PER_WORKER_OUTPUT,
                "{key:?}: a crew at the food peak with no gear banks exactly its head count — the \
                 normalisation's whole point"
            );
            assert_eq!(
                build_turns_remaining(
                    rung.build_cost(RUNG_COST_UNSCALED)
                        .expect("the rung builds"),
                    RUNG_UNSTARTED,
                    accrual,
                ),
                Some(REFERENCE_BUILD_TURNS),
                "{key:?}: the food peak preserves the shipped {REFERENCE_BUILD_TURNS}-turn build \
                 at its reference crew of {crew}"
            );
        }
    }

    /// **A BUILD DOES NOT READ THE FLOOR AT ALL** — the same crew banks the same work whatever the
    /// gatherers beside them are doing (`docs/plan_standing_upkeep.md` §2.2).
    ///
    /// It replaced `a_deeper_floor_builds_slower_and_stripping_builds_nothing`, which pinned
    /// `learn_multiplier` on the accrual — the rate that replaced the `EcologyPhase::Thriving` gate
    /// (`docs/plan_harvest_floor.md` §3.2) on the rule *"a crew pulling hard on the source it is
    /// improving builds slowly"*. **That rule was written when one crew did both jobs.** With the
    /// build staffed in its own right the builders are not pulling anything, and a build crew on a
    /// source nobody is harvesting has no floor to read. The rate survives on
    /// [`RungDef::knowledge_accrual`], where restraint still shapes what is learned.
    #[test]
    fn a_build_banks_the_same_work_at_every_floor() {
        let ladder = LadderConfig::builtin();
        for key in [
            RungKey::PlantTended,
            RungKey::PlantField,
            RungKey::AnimalPastoral,
            RungKey::AnimalPen,
        ] {
            let rung = ladder.rung(key);
            let crew = reference_crew(rung);
            let banked = rung.build_accrual(rung.verb_improvement(), true, crew);
            // Liveness first: an invariance sweep alone would pass on an accrual that returned zero
            // everywhere, which is exactly what a broken gate looks like.
            assert!(banked > 0.0, "{key:?} must actually build");
            assert_eq!(
                banked,
                expected_net(rung, crew),
                "{key:?}: a build crew banks its head count LESS the maintenance rate it is also \
                 paying"
            );
            // **The floor cannot reach it: `build_accrual` does not take one.** What a *gatherer*
            // beside them holds is asserted end-to-end by
            // `a_build_in_flight_leaves_the_take_row_alone`; here the seam's own signature is the
            // guarantee, and the pacing it preserves is
            // `taking_the_floor_off_the_build_rate_is_pacing_neutral_at_the_food_peak`.
            //
            // The two ends the retired rate was degenerate at — `1.0` (watching) and `0`
            // (stripping) — are therefore no longer build states at all. `STRIP_IT_BARE` survives
            // as the knowledge path's degenerate end.
            let _ = STRIP_IT_BARE;
            // The **other** degenerate end is the caller's, not this seam's: at `floor = 1.0` the
            // rate is its highest but nothing stands above the floor, so the caller's `eligible` is
            // false and nothing is built. Asserted here as the seam's half of it — `eligible = false`
            // is always zero, whatever the floor.
            assert_eq!(
                rung.build_accrual(rung.verb_improvement(), false, reference_crew(rung),),
                0.0,
                "{key:?}: watching a source builds nothing, however restrained the watching"
            );
        }
    }

    /// **THE DECAY DOES NOT READ THE SOURCE'S COST MULTIPLIER, AND SINCE §2.4 IT CANNOT.** A bleed
    /// used to be a fraction of the rung's own `work_cost`, so it rode `cost_multiplier` and a rung's
    /// build:decay ratio stayed invariant per source for free. Shortfall is the decay now: what an
    /// improvement loses is what its **keepers** did not supply, which is a fact about the crew and
    /// the rung, not about how big the job was. A Steppe Runner is five times the work to tame and
    /// forgets at whatever rate its own upkeep names — the two are simply different questions.
    #[test]
    fn the_cost_multiplier_prices_the_job_and_never_the_upkeep() {
        let ladder = LadderConfig::builtin();
        let tended = ladder.rung(RungKey::PlantTended);
        let build = tended.build.as_ref().expect("the tended rung builds");
        const TWICE_THE_WORK: f32 = 2.0;
        assert_eq!(
            tended.build_cost(TWICE_THE_WORK),
            Some(build.work_cost * TWICE_THE_WORK),
            "the multiplier is the source's own nature, and it prices the JOB"
        );
        // The upkeep seam takes a `source_measure`, not a cost multiplier, and `flat` ignores it —
        // so nothing about the size of the build reaches what it costs to hold.
        assert_eq!(
            tended.upkeep_demand(UNSCALED_UPKEEP),
            tended.upkeep_demand(TWICE_THE_WORK),
            "a `flat` upkeep is the cost of the thing existing, whatever the job cost to raise"
        );
    }

    /// **[`learn_multiplier`] is the one shape, and it is a seam.** Pinned at the three points that
    /// carry the model — both ends and the normalising middle — rather than at a table of literals,
    /// so swapping the linear curve for a knee changes this test's *values* and nothing else's.
    #[test]
    fn the_learn_multiplier_is_normalised_on_the_food_peak() {
        assert_eq!(learn_multiplier(FOOD_PEAK_FLOOR), 1.0);
        assert_eq!(learn_multiplier(STRIP_IT_BARE), 0.0);
        assert!(
            (learn_multiplier(1.0) - 2.0).abs() < f32::EPSILON,
            "leaving the whole source standing learns at ×2 — the trade the dial exists to offer, \
             at its limit, and it self-limits because nothing above the floor means no take"
        );
        // A negative floor cannot reach here (`components::floor_is_valid` fails closed at the
        // command boundary), but the multiplier must not hand back a *negative rate* if one ever did.
        assert_eq!(learn_multiplier(-1.0), 0.0);
    }

    // --- The plant branch's SITE requirement (the twin of `ceiling_required`, keyed on the land):
    // the scarcity that makes *which ground you can reach* the early game's real decision.

    /// The shipped rule, pinned: **every plant rung is site-bound and no animal rung is** (a herd
    /// carries its own site with it), and rung 3 adds fresh water on top of what rungs 1–2 ask.
    #[test]
    fn every_plant_rung_is_bound_to_a_gathering_site() {
        let ladder = LadderConfig::builtin();
        for key in [
            RungKey::PlantWild,
            RungKey::PlantTended,
            RungKey::PlantField,
        ] {
            let site = ladder
                .rung(key)
                .site_requirement
                .unwrap_or_else(|| panic!("{key:?} must state its site rule"));
            assert!(
                site.requires_gathering_site,
                "{key:?} must stand on ground the people already work — that is the plant \
                 branch's scarcity, and rung 4 is the first rung to drop it"
            );
        }
        assert!(
            ladder
                .rung(RungKey::PlantField)
                .site_requirement
                .expect("rung 3 has a site requirement")
                .requires_fresh_water,
            "rung 3 can carry seed but not water — the field must be near fresh water"
        );
        for key in [
            RungKey::AnimalWild,
            RungKey::AnimalPastoral,
            RungKey::AnimalPen,
        ] {
            assert!(
                ladder.rung(key).site_requirement.is_none(),
                "{key:?} must ask nothing of the site — a herd carries its own"
            );
        }
    }

    /// **The site seam**, asserted at the rung: the rung judges the three readings and names *which*
    /// way the ground fell short, so the caller only phrases it. They are different problems with
    /// different answers, and the gathering-site fault **supersedes** rather than joining the others
    /// — teaching a player two faults they cannot act on is worse than teaching them the one they can.
    #[test]
    fn the_site_requirement_names_which_way_the_land_refuses() {
        let site = LadderConfig::builtin()
            .rung(RungKey::PlantField)
            .site_requirement
            .expect("rung 3 has a site requirement");

        assert_eq!(
            site.refusal(true, 0.0, true),
            None,
            "a watered gathering site takes seed"
        );
        assert_eq!(
            site.refusal(true, 0.0, false),
            Some(SiteRefusal::TooDry),
            "a dry gathering site is refused on water alone"
        );
        assert_eq!(
            site.refusal(false, 0.0, true),
            Some(SiteRefusal::NotGatheringSite),
            "ground nobody gathers is refused before anything else is asked of it"
        );
        assert_eq!(
            site.refusal(false, 0.0, false),
            Some(SiteRefusal::NotGatheringSite),
            "the gathering-site fault SUPERSEDES the others — a tile that is also dry must not be \
             told two things it cannot act on"
        );

        // The fertility floor still works where a rung sets one — it is rung 4's dial, parked at 0
        // on every shipped rung.
        let farm = RungSiteRequirement {
            requires_gathering_site: false,
            min_forage_capacity: 40.0,
            requires_fresh_water: true,
        };
        assert_eq!(farm.refusal(false, 39.0, true), Some(SiteRefusal::TooPoor));
        assert_eq!(
            farm.refusal(false, 39.0, false),
            Some(SiteRefusal::TooPoorAndTooDry)
        );
    }

    /// **Rung 4 (Farm) is THIS RECORD with the site rule dropped and the fertility floor put back** —
    /// the arc's config-driven thesis, and the reason every dial is a lever rather than a constant.
    /// Nothing but this record has to change to add it.
    #[test]
    fn a_looser_site_requirement_is_a_pure_config_edit() {
        let farm = RungSiteRequirement {
            requires_gathering_site: false,
            min_forage_capacity: 40.0,
            requires_fresh_water: true,
        };
        // Fertile, watered ground that simply is not a gathering site: refused at rung 3, farmable
        // at rung 4. That difference IS what Farm unlocks.
        assert_eq!(
            LadderConfig::builtin()
                .rung(RungKey::PlantField)
                .site_requirement
                .expect("rung 3 has a site requirement")
                .refusal(false, 195.0, true),
            Some(SiteRefusal::NotGatheringSite)
        );
        assert_eq!(farm.refusal(false, 195.0, true), None);
    }

    #[test]
    fn rejects_a_negative_fertility_floor() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "field");
            json["rungs"][idx]["site_requirement"]["min_forage_capacity"] = (-1.0).into();
        });
        assert_rejects(err, "plant:field");
    }

    /// A `site_requirement` that admits every tile is a placement rule that places no rule — say
    /// `null` instead. This is the bound that stops the rung's scarcity evaporating silently.
    #[test]
    fn rejects_a_site_requirement_that_requires_nothing() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "field");
            json["rungs"][idx]["site_requirement"]["requires_gathering_site"] = false.into();
            json["rungs"][idx]["site_requirement"]["min_forage_capacity"] = (0.0).into();
            json["rungs"][idx]["site_requirement"]["requires_fresh_water"] = false.into();
        });
        assert_rejects(err, "plant:field");
    }

    #[test]
    fn rejects_a_ladder_missing_a_rung_the_engine_drives() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"].as_array_mut().expect("array").remove(idx);
        });
        assert_rejects(err, "animal:pen");
    }

    /// **[`RungKey::above`] and the shipped records' own `order` are ONE ladder** — the coded climb a
    /// projection walks must be the climb the config declares, or `buildTurnsRemaining` would quote a
    /// rung the config puts somewhere else entirely.
    ///
    /// Asserted both ways round: every key with a rung above it names the record at `order + 1` on its
    /// own branch, and every key answering `None` really is the top of its branch. The second half is
    /// the liveness clause — a `above` that answered `None` for everything would pass the first alone,
    /// and would silently turn every projection into "no estimate".
    #[test]
    fn the_coded_climb_matches_the_shipped_ladders_own_order() {
        let ladder = LadderConfig::builtin();
        for key in RungKey::ALL {
            let here = ladder.rung(key);
            let taller = ladder
                .rungs
                .iter()
                .any(|rung| rung.branch == here.branch && rung.order > here.order);
            match key.above() {
                Some(next) => {
                    let next = ladder.rung(next);
                    assert_eq!(
                        next.branch,
                        here.branch,
                        "{} climbs its own branch",
                        key.id()
                    );
                    assert_eq!(
                        next.order,
                        here.order + 1,
                        "{}'s next rung is the record directly above it",
                        key.id()
                    );
                }
                None => assert!(
                    !taller,
                    "{} answers 'nothing above' while the ladder declares a taller rung on its \
                     branch",
                    key.id()
                ),
            }
        }
    }
}
