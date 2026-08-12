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
//! This module is the **data + the seam**, not a second copy of the rules:
//! - [`LadderConfig`] (`data/intensification_ladder.json`) holds one [`RungDef`] record per rung —
//!   the links (verb, unlock/earns knowledge, previous rung, husbandry ceiling) and the build dials.
//!   Adding a rung that recombines existing primitives is a one-record edit.
//! - [`RungDef::build_accrual`] / [`RungDef::build_cost`] / [`RungDef::build_decay`] /
//!   [`RungDef::yield_fraction_while_building`]
//!   are **the** build seam. Both tracks call them instead of reaching for their own bespoke
//!   accrue/cost/decay/dip levers, so the plant and animal ladders can never drift apart numerically.
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

/// **A crew of nobody** — the rejected value of [`RungBuild::crew_needed`]. A staffing *floor* of
/// nobody is nonsense; a rung with no crew model of its own says `null`.
pub const NO_CREW: u32 = 0;

/// **No improvement is being built on this source**, so its build contributes no crew demand and
/// [`source_crew_needed`] collapses to the take-side count — the pre-crew-axis behaviour, verbatim.
/// Also what a rung whose web sizes its crew off the *source* rather than the rung reports (both
/// animal rungs — see [`RungDef::build_crew_needed`]).
pub const NO_BUILD_CREW: u32 = 0;

/// **THE crew a worked source demands: `max(standing crew, take crew)`** — one crew that can cover
/// its busiest job, and the single definition **both** the resolved turn (`advance_labor_allocation`)
/// and the assign-time seed (`forage::forage_source_yield_preview` /
/// `fauna::hunt_source_yield_preview`, through `fauna::forecast_source_yield`) report through. It
/// lives here, on the rung engine, because the *standing* half is a rung's own
/// [`RungDef::build_crew_needed`] on the plant web and a herd's `herders_needed` on the animal one —
/// neither module can own a rule the other must obey.
///
/// **The two halves are different units and neither dominates**, which is why this is a `max` and not
/// a sum or a pick:
///
/// ```text
/// herding = per HEAD     — one herder minds 12 aurochs   (animals_per_herder)
/// hauling = per BIOMASS  — one hauler carries 40 biomass  (per_worker_biomass_capacity)
/// building = per RUNG    — a Cultivate wants 2 pairs of hands whatever the patch pays
/// ```
///
/// A shepherd minds ~300 sheep and could not carry three of them; an aurochs herder minds 12 head
/// (960 biomass) but hauls 40. `+` would be two separate teams; `max` is one crew sized by whichever
/// job binds.
///
/// **Reporting only one half made the UI contradict itself.** On the animal web, the herder count
/// alone read `workersNeeded: 1` beside `wastedYield: 0.80` — *drop workers* and *add workers*, at
/// the same time, on the same row. On the plant web, a crew *preparing* a patch is paid the
/// investment dip, so inverting that (dipped) take gave `workers_needed = 1` where the same patch's
/// wild Sustain gather wants 2: the panel asked for **fewer** people to do **more** work, and flagged
/// the second worker as overstaffing.
///
/// **Wild hunting is untouched by construction**: a wild herd isn't yours to maintain, so
/// `fauna::herd_herders_needed` is [`NO_CREW`] and this collapses to the take-side count — the
/// shipped behaviour, verbatim. (`hunt = reach + carry`; `harvest = maintain + take`.)
pub fn source_crew_needed(standing_crew: u32, take_workers: u32) -> u32 {
    standing_crew.max(take_workers)
}

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

/// **WHAT A CREW OF `workers` PRODUCES IN ONE TURN**, before the floor and the kit scale it — the
/// first term of [`RungDef::build_accrual`].
///
/// **Written as a per-worker SUM OF TERMS with exactly one term today**, and that shape is
/// deliberate rather than premature: `docs/plan_unit_costed_work.md` §5 rules that knowledge does
/// **not** feed throughput (it reaches it through the tools it unlocks, which §6 prices), so there is
/// nothing to add here yet — but a future buff mechanic needs a place to land that is not a
/// re-inversion of the model.
fn crew_work_output(workers: u32) -> f32 {
    let per_worker = PER_WORKER_OUTPUT;
    workers as f32 * per_worker
}

/// **HOW MANY MORE TURNS THIS BUILD NEEDS at the crew, floor and kit that just worked it** —
/// `ceil((cost − done) / work_this_turn)`, and THE one place that arithmetic lives so the wire's
/// `buildTurnsRemaining` cannot drift from the meter it describes.
///
/// `None` = **no estimate**, and it means exactly two things, both of which the wire renders as
/// [`NO_BUILD_TURNS_ESTIMATE`]: the job is already paid for (nothing left to wait for), or the crew
/// produced nothing this turn and the build is **stalled** — a stall has no finite answer, and
/// quoting a huge one would read as a promise.
///
/// **The sim answers it because the client cannot**: the client holds neither the crew's output, nor
/// the floor multiplier, nor the kit's build rate — the same division of labour as `penFeedUpkeep`
/// and the yield forecast.
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

pub fn build_turns_remaining(cost: f32, done: f32, work_this_turn: f32) -> Option<u32> {
    if work_this_turn <= 0.0 {
        return None;
    }
    let remaining = cost - done;
    if remaining <= 0.0 {
        return None;
    }
    Some((remaining / work_this_turn).ceil() as u32)
}

/// **The floor a build on a RUNG-3 MANAGED source passes** — the food peak, so [`learn_multiplier`]
/// is exactly `×1.0`.
///
/// A Field and a penned herd are *yours*: their take is `managed_production` at **every** floor
/// (`SourceYieldForecast::managed`), so the assignment's floor is inert there and there is no
/// pressure the crew actually chose. Reading the dial anyway would scale a keeper's learning by a
/// number that changed nothing about what they took. Two builds are in that state — `ExtendPen`
/// (which rides a corralled herd's tend branch) and a Field's own harvest — and both climb at the
/// rung's stated pace instead.
///
/// Named rather than spelled `MSY_BIOMASS_FRACTION` at the call sites so the *reason* travels with
/// the value: this is "the floor axis has collapsed here", not "the food peak happens to be right".
pub const MANAGED_SOURCE_FLOOR: f32 = crate::fauna::MSY_BIOMASS_FRACTION;

/// **The cost multiplier of a source that costs exactly what its rung declares.** Passed by every
/// caller with no per-source multiplier to apply (the plant `tended` patch, the plant `field`, the
/// animal `pen` and its `ExtendPen` rings) — the rung's `work_cost` *is* the price there.
///
/// The multiplier exists because rung 2 of the animal ladder is **not** one-size-fits-all: a species
/// declares its own `taming_cost_multiplier` (`fauna_config`), and the honest statement is that the
/// animal is *more work*, not that the crew is worse at their job. See [`RungDef::build_cost`].
pub const RUNG_COST_UNSCALED: f32 = 1.0;

/// **The build multiplier of a crew carrying no gear that helps** — the neutral `1.0` a kit resolves
/// to when none of its live items declares [`crate::equipment_config::EquipmentStat::BuildRate`].
///
/// It is [`crate::equipment_config::EquipmentConfig::build_rate`]'s answer for every plant build
/// today (no plant item declares the stat yet — issue #539) and for every animal build whose crew
/// went out on a kit without handling gear. Named rather than a bare `1.0` at the call sites that
/// have no band to resolve a kit against — a forecast probe, a test fixture — so *"this crew brought
/// nothing"* reads as a stated fact rather than an unexplained literal.
pub const NO_BUILD_GEAR: f32 = 1.0;

/// **The yield dip of an assignment that is building nothing** — the identity, so a pure harvest pays
/// its stance's whole ceiling. Named rather than a bare `1.0` at [`LadderConfig::build_dip`]'s `None`
/// arm, where the number says nothing about which multiplier is being declined.
pub const NO_BUILD_UNDERWAY_DIP: f32 = 1.0;

/// **The dip a source with NOTHING LEFT TO BUILD publishes on the wire** — a Field, a penned herd.
/// `snapshot.fbs` documents a build fraction as `0 < f < 1`, so this sits deliberately *outside*
/// that range and means *"this rung is not on offer here"*; the client's compose sheet already
/// declines to quote a deal on a non-positive fraction
/// (`SourceForecast.gd::improvement_forecast`), so the sentinel needs no client change to be read
/// correctly. Publishing the identity `1.0` instead said two false things at once: that a finished
/// source's build costs nothing, and that it is still available.
///
/// **It is a WIRE value, never a multiplier.** [`BuildDips::of`] still answers
/// [`NO_BUILD_UNDERWAY_DIP`] for a rung there is nothing to build, because a ceiling scaled by `0`
/// would pay a managed source's crew nothing.
pub const NO_BUILD_REMAINING_FRACTION: f32 = 0.0;

/// **One food web's two build dips**, carried on a `fauna::SourceYieldForecast` so a forecast can
/// price a build without holding the ladder — the pre-commit twin of [`LadderConfig::build_dip`].
///
/// Two slots, not four, because the improvements are **kind-exclusive**: a plant source is only ever
/// asked about `Cultivate`/`Sow` and an animal one about `Tame`/`Corral`, so "rung 2" and "rung 3"
/// name the pair unambiguously for whichever branch the source belongs to.
///
/// **A slot is `None` when the source has nothing left to build there** — the rung-3 managed shape
/// ([`crate::fauna::SourceYieldForecast::managed`]). That state has to be *representable*: it is not
/// the same fact as "the dip happens to be 1.0", and the wire distinguishes them
/// ([`NO_BUILD_REMAINING_FRACTION`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildDips {
    /// The rung-2 verb's `yield_fraction_while_building` (`Cultivate` on plants, `Tame` on animals),
    /// or `None` when this source has nothing left to build at that rung.
    pub rung_two: Option<f32>,
    /// The rung-3 verb's (`Sow` on plants, `Corral` on animals), same convention.
    pub rung_three: Option<f32>,
}

impl Default for BuildDips {
    /// A default-constructed forecast offers no build — matching [`BuildDips::NOTHING_LEFT_TO_BUILD`].
    /// (`SourceYieldForecast` derives `Default` for its test fixtures.)
    fn default() -> Self {
        Self::NOTHING_LEFT_TO_BUILD
    }
}

impl BuildDips {
    /// **Nothing left to build** — the value a rung-3 managed source carries (a Field, a penned
    /// herd): its policy axis has collapsed onto one managed number, and quoting a dip on it would
    /// price a build that cannot be started.
    pub const NOTHING_LEFT_TO_BUILD: Self = Self {
        rung_two: None,
        rung_three: None,
    };

    /// Read one branch's pair off the ladder.
    pub fn for_branch(ladder: &LadderConfig, branch: RungBranch) -> Self {
        let (rung_two, rung_three) = match branch {
            RungBranch::Plant => (Improvement::Cultivate, Improvement::Sow),
            RungBranch::Animal => (Improvement::Tame, Improvement::Corral),
        };
        Self {
            rung_two: Some(ladder.build_dip(Some(rung_two))),
            rung_three: Some(ladder.build_dip(Some(rung_three))),
        }
    }

    /// The dip an assignment carrying `improvement` multiplies its stance's ceiling by —
    /// [`NO_BUILD_UNDERWAY_DIP`] when it is building nothing, and equally when the rung it names has
    /// nothing left to build (a crew standing on a finished source is harvesting, not preparing).
    pub fn of(self, improvement: Option<Improvement>) -> f32 {
        match improvement {
            None => NO_BUILD_UNDERWAY_DIP,
            Some(Improvement::Cultivate | Improvement::Tame) => {
                self.rung_two.unwrap_or(NO_BUILD_UNDERWAY_DIP)
            }
            Some(Improvement::Sow | Improvement::Corral) => {
                self.rung_three.unwrap_or(NO_BUILD_UNDERWAY_DIP)
            }
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
    /// **The fraction of this rung's own cost bled per turn nobody works the source** — "walk away
    /// and the cleared ground grows back over". `0.01` is ~100 turns to fully lapse *whatever the job
    /// costs*, which is why it is a fraction rather than an absolute: the build:decay ratio stays a
    /// per-rung ratio when the cost spread lands (§3.2). [`RungDef::build_decay`] turns it into
    /// absolute work units. Validated `0 < decay_fraction_per_turn < 1`: a rung that bled its whole
    /// cost in a turn could never complete.
    ///
    /// **`None` = this rung's meter does not bleed at all**, and that is the animal branch's whole
    /// story: `domestication_progress` is monotone-up (neglect sheds *animals*, never tameness —
    /// `docs/plan_fauna_neglect_escape.md` §2.1) and a pen is lost outright with the herd that bled
    /// out, so neither animal rung has a decaying meter. Both used to carry a number here
    /// (`pastoral` `0.01`, `pen` `0.0`) that **nothing read** — [`RungDef::build_decay`]'s only
    /// production call sites are `forage::advance_cultivation`'s two plant rungs — so the record
    /// documented a mechanic that does not exist. Absent states the truth and cannot be mistaken for
    /// a live dial.
    ///
    /// **That bound holds per-source too, and it is checked exactly once here.** A per-source cost
    /// multiplier ([`RungDef::build_cost`]) scales the cost, and [`RungDef::build_decay`] reads the
    /// decay off that same scaled cost — so the ratio is invariant for free and no per-species
    /// restatement of this bound is needed, only that the multiplier itself is positive and finite
    /// (which the roster that owns it, `FaunaConfig::validate`, enforces).
    #[serde(default)]
    pub decay_fraction_per_turn: Option<f32>,
    /// **The neglect GRACE** — how many consecutive turns a source may go un-worked before this
    /// rung's neglect penalty starts. The penalty applies only while the source's `neglect_turns`
    /// counter is **strictly greater** than this, so `0` restores the old no-grace behaviour and `n`
    /// forgives exactly `n` unworked turns.
    ///
    /// **One lever, both webs, per rung** — because the two webs' penalties differ in kind but not in
    /// trigger: a plant rung bleeds its build meter (`forage::advance_cultivation`), an animal rung
    /// sheds animals over its labor capacity (`fauna::advance_husbandry`). The rung the source stands
    /// on owns the number, which is the point of it being per-rung: *a weeded patch reverts in a
    /// season, a fence stands for years*, so `plant:tended` and `animal:pen` want different answers
    /// and the two webs are not even monotone in the same direction.
    ///
    /// Validated `< work_cost / reference_output` — the grace may not outlast the build itself, where
    /// the reference build is the one the rung's own [`Self::crew_needed`] (or one worker, where it
    /// declares none) runs at the food peak. A longer grace makes neglect free over the whole span it
    /// took to raise the rung, which is how a penalty evaporates silently (the
    /// `site_requirement`-that-requires-nothing failure, in the time axis).
    pub grace_turns: u32,
    /// **THE STAFFING FLOOR ON THE SOURCE'S WORKER CAP** — a build cannot make a source want *fewer*
    /// people than gathering it does. Without that floor the cap came only from the harvest
    /// (`ceil(ceiling / per_worker)`), and the ceiling during a build is the **dip** — so committing
    /// to a 25-turn improvement made the compose sheet ask for *one* forager where the same wild
    /// patch under Sustain asks for two. The animal web has always had this floor
    /// (`fauna::herd_herders_needed`); this is the plant twin of it. Read through
    /// [`source_crew_needed`], so it can only ever *raise* a count.
    ///
    /// **It has ONE job now, not two.** It used to scale the accrual as well
    /// (`min(workers / crew_needed, 1)`), which capped a build at the rung's stated rate and made
    /// over-crewing worthless. Under the work model a crew's output *is* its head count
    /// ([`RungDef::build_accrual`]) and there is no cap: fifty workers finish a Cultivate in a turn,
    /// and the constraint is opportunity cost across systems rather than a rule forbidding a play
    /// style (`docs/plan_unit_costed_work.md` §1.2).
    ///
    /// **`None` = this rung's build has no crew model of its own**, which is where both animal rungs
    /// stand: a herd's crew is derived from its *size* (`herders_needed(biomass, body_mass,
    /// animals_per_herder)`), not declared by the rung, so a rung-level constant there would be a
    /// number nothing reads. Validated `!= Some(0)` — a staffing floor of nobody is nonsense.
    #[serde(default)]
    pub crew_needed: Option<u32>,
    /// **The investment cost.** While the meter is filling, the source's take ceiling is only this
    /// fraction of the **selected stance's** ceiling — the crew is preparing, not harvesting. It rode
    /// the Sustain (MSY) ceiling until issue #442, when the build verb stopped *being* the stance; a
    /// fraction of a sustainable draw is still sustainable, so a Sustain builder keeps its source
    /// healthy, and a Deplete builder's dip rides a draw-down and undermines its own meter (§2.2).
    /// Validated `0 < f < 1`: at `0` the rung would starve its builders, at `>= 1` it would cost
    /// nothing and the whole "investment with a time horizon" decision would evaporate.
    pub yield_fraction_while_building: f32,
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

    /// **The build seam — the accrual side. THE WORK UNITS THIS CREW PRODUCES THIS TURN**, not a
    /// fraction of anything: `crew output × learn_multiplier(floor) × build_rate` when `improvement`
    /// **is** the rung's verb *and* the caller's rung-specific gates hold (`eligible` — knows the
    /// unlock knowledge, the crew is actually working the source, the species' ceiling allows it, the
    /// faction owns it), otherwise `0`.
    ///
    /// The caller adds it to the source's own meter and completes the rung when the meter reaches
    /// [`RungDef::build_cost`]. **Turns are the output, not an input** — the same fixed job finishes
    /// sooner as the faction puts more hands, a shallower floor or better tools on it, which is the
    /// progression statement the normalized meter could not make.
    ///
    /// # `workers` — the crew IS the throughput, and there is no cap
    ///
    /// A worker is worth [`PER_WORKER_OUTPUT`], so `n` workers produce `n` units a turn and a Cultivate
    /// staffed at fifty finishes in a turn. That is allowed: the constraint is opportunity cost across
    /// systems, not a rule forbidding a play style (`docs/plan_unit_costed_work.md` §1.2). The retired
    /// `crew_scale` (`min(workers / crew_needed, 1)`) is what capped it;
    /// [`RungBuild::crew_needed`] survives as the **staffing floor** alone.
    ///
    /// A rung with **no verb** (`verb: null`) or **no build** is never driven: it returns `0` — which
    /// is what keeps the `wild` rungs (nothing to build) out of the engine.
    ///
    /// # `floor` — building rides the same rate learning does
    ///
    /// [`learn_multiplier`] scales this by `floor / MSY_BIOMASS_FRACTION`, so a crew that leaves more
    /// standing builds faster and one stripping the ground barely builds at all. **That is what
    /// replaced the `Thriving` gate** on both webs: a build no longer *stops* when the source gets
    /// thin, it *slows* in proportion to how hard the crew is pulling — a rate where there was a
    /// cliff, with no lapse state to hold progress across.
    ///
    /// It is applied here and **not** to [`RungDef::build_decay`] — see [`learn_multiplier`].
    ///
    /// # `build_rate` — what the crew brought to the work
    ///
    /// The crew's kit ([`crate::equipment_config::EquipmentConfig::build_rate`]), neutral at
    /// [`NO_BUILD_GEAR`] for a crew carrying nothing that helps — which is every plant build and
    /// every animal one whose crew left the handling gear at home. Hurdles, halters and a butchering
    /// stone are animal-handling tools, and `Tame` and `Corral` are exactly the turns a band spends
    /// handling animals (issue #515).
    ///
    /// **It multiplies the crew's output and NOT [`RungDef::build_decay`]**, for the reason `floor`
    /// gets the same treatment: decay happens on turns nobody works the source, so there is no crew,
    /// no kit and no gear to read. Better tools make a build arrive sooner; they do not make an
    /// abandoned one forget more slowly. It does **not** touch
    /// [`RungDef::yield_fraction_while_building`] either — a faster build already pays the dip for
    /// fewer turns.
    pub fn build_accrual(
        &self,
        improvement: Option<Improvement>,
        eligible: bool,
        floor: f32,
        workers: u32,
        build_rate: f32,
    ) -> f32 {
        if self.build.is_none() {
            return 0.0;
        }
        if !eligible || improvement.is_none() || self.verb_improvement() != improvement {
            return 0.0;
        }
        crew_work_output(workers) * learn_multiplier(floor) * build_rate
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

    /// **The build seam — the decay side. ABSOLUTE WORK UNITS bled on a turn nobody works the
    /// source**: `decay_fraction_per_turn × work_cost × cost_multiplier`. `0` for a rung whose meter
    /// does not bleed (`decay_fraction_per_turn: null` — the whole animal branch) or that has nothing
    /// to build.
    ///
    /// **The per-source cost multiplier reaches the decay as well as the cost, and that is
    /// load-bearing** — it is what [`RUNG_COST_UNSCALED`]'s predecessor achieved by dilating a
    /// timescale. Because both sides read the *same* scaled cost, a rung's build:decay ratio is
    /// invariant under any per-source multiplier **for free**: a beast that takes a lifetime to gentle
    /// does not go feral in a season, and no per-species restatement of
    /// [`RungBuild::decay_fraction_per_turn`]'s bound is needed. Moot on the animal branch today
    /// (both animal rungs declare `null`), but it is the rule that keeps a future decaying rung
    /// correct.
    ///
    /// **It takes no `floor`, and that asymmetry with the accrual is deliberate.** Decay is what
    /// happens on a turn nobody works the source: there is no assignment, so there is no floor to
    /// read. See [`learn_multiplier`].
    pub fn build_decay(&self, cost_multiplier: f32) -> f32 {
        self.build.as_ref().map_or(0.0, |build| {
            build
                .decay_fraction_per_turn
                .map_or(0.0, |fraction| fraction * build.work_cost * cost_multiplier)
        })
    }

    /// **The build seam — the neglect grace.** How many consecutive un-worked turns this rung
    /// forgives before its neglect penalty starts biting ([`RungBuild::grace_turns`]).
    /// [`NO_NEGLECT_GRACE`] for a rung with no build — a source with nothing built on it has nothing
    /// to be forgiven for, and the callers only reach this for a rung whose meter (or flock) is
    /// actually at risk.
    ///
    /// Unlike the accrual/decay pair this takes **no `timescale`**: the grace is a count of *turns a
    /// crew was absent*, not an amount of progress, and a species that is slow to tame is not thereby
    /// slower to notice its keepers have gone.
    pub fn neglect_grace_turns(&self) -> u32 {
        self.build
            .as_ref()
            .map_or(NO_NEGLECT_GRACE, |build| build.grace_turns)
    }

    /// **The build seam — the crew demand.** The crew this rung's build wants
    /// ([`RungBuild::crew_needed`]), or `None` for a rung with no build, or one whose web derives its
    /// crew from the source rather than the rung (both animal rungs — see the field's doc).
    ///
    /// Read by the forecast as a **floor** on the source's worker cap, so a build never asks for
    /// fewer hands than the harvest it replaced.
    pub fn build_crew_needed(&self) -> Option<u32> {
        self.build.as_ref().and_then(|build| build.crew_needed)
    }

    /// **The build seam — the investment dip.** The fraction of the source's **selected stance's**
    /// ceiling it pays while this rung is being built. `None` for a rung with no build — a caller
    /// with no dip to apply must not silently substitute one.
    ///
    /// It rode the **Sustain** ceiling until issue #442, because a build verb *was* the policy and a
    /// builder could therefore not be in any other stance. With the axes split the same fraction
    /// multiplies whichever stance the player holds — the identical formula with the constant
    /// removed — which is what makes a Deplete-while-building self-punishing without a gate
    /// (`docs/plan_investment_rung_toggle.md` §2.2).
    pub fn yield_fraction_while_building(&self) -> Option<f32> {
        self.build
            .as_ref()
            .map(|build| build.yield_fraction_while_building)
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
        self.rung(match improvement {
            Improvement::Cultivate => RungKey::PlantTended,
            Improvement::Sow => RungKey::PlantField,
            Improvement::Tame => RungKey::AnimalPastoral,
            Improvement::Corral => RungKey::AnimalPen,
        })
    }

    /// **THE dip multiplier an assignment's ceiling carries** (`docs/plan_investment_rung_toggle.md`
    /// §2.2): the building rung's `yield_fraction_while_building`, or the identity
    /// [`NO_BUILD_UNDERWAY_DIP`] when the crew is only harvesting.
    ///
    /// One lookup for both webs, so the plant and animal take paths cannot apply the dip differently
    /// — and so the *stance* it multiplies is entirely the caller's business, which is what generalised
    /// the dip off its hardcoded Sustain base.
    pub fn build_dip(&self, improvement: Option<Improvement>) -> f32 {
        improvement.map_or(NO_BUILD_UNDERWAY_DIP, |improvement| {
            self.rung_for(improvement)
                .yield_fraction_while_building()
                .expect("a rung a verb builds is an investment — it has a build meter")
        })
    }

    /// **THE standing crew an assignment's build demands** — the building rung's
    /// [`RungDef::build_crew_needed`], or [`NO_BUILD_CREW`] when the crew is only harvesting, or when
    /// the rung declares no crew (both animal rungs, whose web sizes a crew off the *herd*).
    ///
    /// One lookup for both webs and for both halves of the yield row — the resolved turn and the
    /// assign-time seed — so a freshly-composed assignment cannot report a different crew from the
    /// turn that resolves it. It is the exact shape of [`Self::build_dip`], and for the same reason:
    /// the number belongs to the rung the verb builds, so no call site should pair a verb with a
    /// `RungKey` by hand. Read as the **standing** half of [`source_crew_needed`], so it can only
    /// ever *raise* a source's `workers_needed`.
    pub fn build_crew(&self, improvement: Option<Improvement>) -> u32 {
        improvement
            .and_then(|improvement| self.rung_for(improvement).build_crew_needed())
            .unwrap_or(NO_BUILD_CREW)
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
    if let Some(fraction) = build.decay_fraction_per_turn {
        if !fraction.is_finite() || fraction <= 0.0 || fraction >= 1.0 {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint:
                    "bleed a strict fraction of its own cost per turn (0 < f < 1) — a rung \
                             that bleeds its whole cost in a turn can never survive one turn of \
                             neglect, and a rung that does not bleed at all says so with \
                             `decay_fraction_per_turn: null` rather than a `0` that reads like a \
                             live dial"
                        .to_string(),
                value: format!("decay_fraction_per_turn = {fraction}"),
            });
        }
    }
    // The grace may not outlast the build itself: forgive neglect for longer than it took to raise
    // the rung and the penalty never fires within the span anyone would notice — the mechanic
    // evaporates without a word, which is the failure every bound in this function guards against.
    //
    // **The reference build is the rung's own crew at the food peak.** Turns are an output now, so
    // "how long does this rung take" has no single answer — it depends on the hands put on it. The
    // rung's declared `crew_needed` is the staffing this job was priced against; a rung that declares
    // none (both animal rungs, whose crew comes from the herd) is measured against one worker, which
    // is the most forgiving reading and therefore the safe one for a bound.
    let reference_output = build.crew_needed.unwrap_or(1) as f32 * PER_WORKER_OUTPUT;
    let build_turns = build.work_cost / reference_output;
    if (build.grace_turns as f32) >= build_turns {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "forgive fewer turns of neglect than the rung takes to build at its own \
                         crew — a grace that outlasts its own build makes walking away free, \
                         silently"
                .to_string(),
            value: format!(
                "grace_turns = {} against a {build_turns}-turn build (work_cost = {} at a \
                 reference output of {reference_output}/turn)",
                build.grace_turns, build.work_cost
            ),
        });
    }
    if build.crew_needed == Some(NO_CREW) {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "ask for at least one worker, or say `crew_needed: null` — `crew_needed` \
                         is the staffing FLOOR on the source's worker cap, and a floor of nobody is \
                         nonsense; a `0` that means \"no crew model\" is indistinguishable from one \
                         that means \"nobody need turn up\""
                .to_string(),
            value: "crew_needed = 0".to_string(),
        });
    }
    if !build.yield_fraction_while_building.is_finite()
        || build.yield_fraction_while_building <= 0.0
        || build.yield_fraction_while_building >= 1.0
    {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint:
                "dip the yield while building to a strict fraction of MSY (0 < f < 1) — at \
                         0 the crew starves, at 1 intensifying costs nothing"
                    .to_string(),
            value: format!(
                "yield_fraction_while_building = {}",
                build.yield_fraction_while_building
            ),
        });
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

    /// **The reference crew of a rung that declares none** — one worker, i.e. the crew the shipped
    /// animal rungs' costs were quoted against in the config comment. Named so that no bare `1`
    /// implies the test picked a staffing level for a reason of its own.
    const UNSCALED_CREW: u32 = 1;

    /// **The floor at which [`learn_multiplier`] is exactly ×1.0** — the food peak. Every accrual
    /// assertion that is *not about the floor* passes it, so the call reads the crew's own output
    /// rather than a floor's fraction of it. That is the normalisation's whole point: the 25-turn
    /// Cultivate is still 25 turns here.
    const FOOD_PEAK_FLOOR: f32 = crate::fauna::MSY_BIOMASS_FRACTION;

    /// Every accrual assertion below staffs the build to the rung's **reference** crew, so a
    /// build-length assertion reads the number the config comment quotes.
    fn reference_crew(rung: &RungDef) -> u32 {
        rung.build_crew_needed().unwrap_or(UNSCALED_CREW)
    }

    /// **What one turn of the reference crew at the food peak with no gear produces** — the accrual
    /// every build-length assertion divides the rung's cost by.
    fn reference_accrual(rung: &RungDef) -> f32 {
        rung.build_accrual(
            rung.verb_improvement(),
            true,
            FOOD_PEAK_FLOOR,
            reference_crew(rung),
            NO_BUILD_GEAR,
        )
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
        // Taming now costs yield, like every other rung — the `domesticate` early-claim that let a
        // player skip this investment is gone.
        assert!(
            pastoral
                .yield_fraction_while_building()
                .is_some_and(|dip| dip > 0.0 && dip < 1.0),
            "the pastoral rung is an investment — it must dip the take while building"
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

        // The crew IS the throughput now: `crew` workers at the food peak with no gear produce
        // `crew × PER_WORKER_OUTPUT` work units.
        let crew = build.crew_needed.expect("the tended rung declares a crew");
        assert_eq!(
            tended.build_accrual(
                Some(Improvement::Cultivate),
                true,
                FOOD_PEAK_FLOOR,
                crew,
                NO_BUILD_GEAR,
            ),
            crew as f32 * PER_WORKER_OUTPUT
        );
        // Wrong verb → nothing, even though the crew is working the patch.
        assert_eq!(
            tended.build_accrual(
                Some(Improvement::Sow),
                true,
                FOOD_PEAK_FLOOR,
                crew,
                NO_BUILD_GEAR,
            ),
            0.0
        );
        // No improvement at all → nothing. A crew that is only harvesting builds nothing, whatever
        // its stance.
        assert_eq!(
            tended.build_accrual(None, true, FOOD_PEAK_FLOOR, crew, NO_BUILD_GEAR),
            0.0
        );
        // Right verb, gate lapsed → nothing accrues (progress is neither lost nor advanced).
        assert_eq!(
            tended.build_accrual(
                Some(Improvement::Cultivate),
                false,
                FOOD_PEAK_FLOOR,
                crew,
                NO_BUILD_GEAR,
            ),
            0.0
        );
        // The cost is the job, and the decay is a fraction OF that job — both in absolute units.
        assert_eq!(tended.build_cost(RUNG_COST_UNSCALED), Some(build.work_cost));
        assert_eq!(
            tended.build_decay(RUNG_COST_UNSCALED),
            build
                .decay_fraction_per_turn
                .expect("the tended rung bleeds")
                * build.work_cost
        );
        assert_eq!(
            tended.yield_fraction_while_building(),
            Some(build.yield_fraction_while_building)
        );
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
            let work = |workers| {
                rung.build_accrual(
                    rung.verb_improvement(),
                    true,
                    FOOD_PEAK_FLOOR,
                    workers,
                    NO_BUILD_GEAR,
                )
            };
            assert_eq!(work(0), 0.0, "{key:?}: nobody working, nothing built");
            assert_eq!(
                work(1),
                PER_WORKER_OUTPUT,
                "{key:?}: one worker is worth one worker-turn"
            );
            assert_eq!(
                work(20),
                20.0 * PER_WORKER_OUTPUT,
                "{key:?}: twenty hands produce twenty units — there is no crew cap"
            );
            // Turns are the OUTPUT: the same fixed cost, twice the crew, half the turns.
            let cost = rung
                .build_cost(RUNG_COST_UNSCALED)
                .expect("the rung builds");
            assert_eq!(
                build_turns_remaining(cost, RUNG_UNSTARTED, work(2)),
                build_turns_remaining(cost, RUNG_UNSTARTED, work(1)).map(|turns| turns.div_ceil(2)),
                "{key:?}: doubling the crew halves the turns"
            );
        }
    }

    /// **The turns estimate is `ceil(remaining / this turn's work)`, and a STALL has no estimate.**
    /// `None` is the wire's [`NO_BUILD_TURNS_ESTIMATE`]: a build nobody is advancing cannot be quoted
    /// a finish date, and a huge number would read as a promise.
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
            None,
            "a paid-for job has nothing left to wait for"
        );
        assert_eq!(
            build_turns_remaining(COST, 10.0, 0.0),
            None,
            "a stalled build has no finite estimate"
        );
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
                    wild.build_accrual(
                        improvement,
                        true,
                        FOOD_PEAK_FLOOR,
                        reference_crew(wild),
                        NO_BUILD_GEAR,
                    ),
                    0.0
                );
            }
            assert_eq!(wild.build_cost(RUNG_COST_UNSCALED), None);
            assert_eq!(wild.build_decay(RUNG_COST_UNSCALED), 0.0);
            assert_eq!(wild.yield_fraction_while_building(), None);
        }
    }

    /// **A rung may not bleed its whole cost in a turn** — the shape the old "decay must be slower
    /// than the build" bound takes once decay is a *fraction of the job* rather than a rate against
    /// another rate. At `1.0` a rung would lose everything the first unworked turn, which is a rung
    /// nobody could hold rather than one nobody could build.
    #[test]
    fn rejects_a_decay_that_bleeds_the_whole_job_in_a_turn() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["decay_fraction_per_turn"] = (1.0).into();
        });
        assert_rejects(err, "plant:tended");
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
            pastoral.build_accrual(
                Some(Improvement::Tame),
                true,
                FOOD_PEAK_FLOOR,
                reference_crew(pastoral),
                NO_BUILD_GEAR,
            ),
            reference_crew(pastoral) as f32 * PER_WORKER_OUTPUT
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
                pastoral.build_accrual(
                    improvement,
                    true,
                    FOOD_PEAK_FLOOR,
                    reference_crew(pastoral),
                    NO_BUILD_GEAR,
                ),
                0.0,
                "{improvement:?} must not tame a herd — only Tame does"
            );
        }
        // Right verb, gate lapsed → nothing accrues (progress is neither lost nor advanced).
        assert_eq!(
            pastoral.build_accrual(
                Some(Improvement::Tame),
                false,
                FOOD_PEAK_FLOOR,
                reference_crew(pastoral),
                NO_BUILD_GEAR,
            ),
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
        let tended = ladder.rung(RungKey::PlantTended).neglect_grace_turns();
        let field = ladder.rung(RungKey::PlantField).neglect_grace_turns();
        let pastoral = ladder.rung(RungKey::AnimalPastoral).neglect_grace_turns();
        let pen = ladder.rung(RungKey::AnimalPen).neglect_grace_turns();

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
        // A rung with nothing built on it has nothing to forgive.
        for key in [RungKey::PlantWild, RungKey::AnimalWild] {
            assert_eq!(ladder.rung(key).neglect_grace_turns(), NO_NEGLECT_GRACE);
        }
    }

    /// **The animal branch's `decay_per_turn` is GONE, not zeroed** — it was a validated dial that
    /// nothing read (`build_decay`'s only production call sites are the two plant rungs), documenting
    /// a tameness-bleed the neglect-escape arc deleted. `null` states that; a `0` would read like a
    /// live dial that happened to be parked.
    #[test]
    fn only_the_plant_rungs_declare_a_build_decay() {
        let ladder = LadderConfig::builtin();
        for key in [RungKey::PlantTended, RungKey::PlantField] {
            assert!(
                ladder.rung(key).build_decay(RUNG_COST_UNSCALED) > 0.0,
                "an abandoned plant improvement bleeds"
            );
        }
        for key in [RungKey::AnimalPastoral, RungKey::AnimalPen] {
            let rung = ladder.rung(key);
            assert_eq!(
                rung.build
                    .as_ref()
                    .and_then(|build| build.decay_fraction_per_turn),
                None,
                "the animal branch's meters do not bleed — say so, do not park a zero"
            );
            assert_eq!(rung.build_decay(RUNG_COST_UNSCALED), 0.0);
        }
    }

    /// **Only the plant rungs declare a build crew**, because the animal web sizes a crew off the
    /// *herd* (`fauna::herders_needed`) rather than off the rung — and a `Cultivate` must want at
    /// least as many hands as it takes to gather the same ground, which is the defect the dial fixes.
    #[test]
    fn the_plant_rungs_declare_a_build_crew_and_the_animal_rungs_do_not() {
        let ladder = LadderConfig::builtin();
        for key in [RungKey::PlantTended, RungKey::PlantField] {
            assert!(
                ladder.rung(key).build_crew_needed().is_some_and(|c| c > 1),
                "a plant build wants a real crew, not one pair of hands"
            );
        }
        for key in [RungKey::AnimalPastoral, RungKey::AnimalPen] {
            assert_eq!(ladder.rung(key).build_crew_needed(), None);
        }
    }

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
            // The reference build is `50 / 2` = 25 turns.
            json["rungs"][idx]["build"]["grace_turns"] = (25).into();
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

    /// A rung whose build wants nobody states a staffing floor of nobody, which is nonsense. A rung
    /// with no crew model says `null`, which is a different statement.
    #[test]
    fn rejects_a_build_crew_of_nobody() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "field");
            json["rungs"][idx]["build"]["crew_needed"] = (0).into();
        });
        assert_rejects(err, "plant:field");
    }

    /// `decay_fraction_per_turn: 0` and `: null` would behave identically, so only one of them is
    /// allowed to mean it — a parked zero reads like a live dial.
    #[test]
    fn rejects_a_zero_build_decay_in_favour_of_null() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["decay_fraction_per_turn"] = (0.0).into();
        });
        assert_rejects(err, "plant:tended");
    }

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

    #[test]
    fn rejects_negative_decay() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["decay_fraction_per_turn"] = (-0.01).into();
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_a_free_investment() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["build"]["yield_fraction_while_building"] = (1.0).into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_starving_investment() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["build"]["yield_fraction_while_building"] = (0.0).into();
        });
        assert_rejects(err, "animal:pen");
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
        let build = |workers| {
            tended.build_accrual(
                tended.verb_improvement(),
                true,
                FOOD_PEAK_FLOOR,
                workers,
                NO_BUILD_GEAR,
            )
        };
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
        // **The animal rungs' reference crews are a CHOICE, not a derivation**, and this is where it
        // is written down beside the costs it produced: both declare `crew_needed: null` (a herd's
        // crew comes from its size), so there was no staffing to multiply today's 25 turns by.
        // `pastoral` takes 2 — it makes the claim rung 2 makes on plants, "you manage the wild
        // source in place" — and `pen` takes 3, matching the plant rung that also *places* a source.
        // See `intensification_ladder.json`'s `_comment_costs`.
        const PASTORAL_REFERENCE_CREW: u32 = 2;
        const PEN_REFERENCE_CREW: u32 = 3;
        for (key, crew) in [
            (
                RungKey::PlantTended,
                reference_crew(ladder.rung(RungKey::PlantTended)),
            ),
            (
                RungKey::PlantField,
                reference_crew(ladder.rung(RungKey::PlantField)),
            ),
            (RungKey::AnimalPastoral, PASTORAL_REFERENCE_CREW),
            (RungKey::AnimalPen, PEN_REFERENCE_CREW),
        ] {
            let rung = ladder.rung(key);
            let accrual = rung.build_accrual(
                rung.verb_improvement(),
                true,
                FOOD_PEAK_FLOOR,
                crew,
                NO_BUILD_GEAR,
            );
            assert_eq!(
                accrual,
                crew as f32 * PER_WORKER_OUTPUT,
                "{key:?}: a crew at the food peak with no gear is worth exactly its head count — \
                 the normalisation's whole point"
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

    /// **A build rides the same rate the lesson does** — increasing in the floor, with a liveness
    /// bound beside the ordering, and degenerate at both ends. The build twin of
    /// `knowledge_accrual_is_the_rungs_lesson_paced_by_the_floor`, and what replaced the
    /// `EcologyPhase::Thriving` gate on both webs (`docs/plan_harvest_floor.md` §3.2): a crew pulling
    /// hard on the source it is improving builds *slowly*, not *not at all*.
    #[test]
    fn a_deeper_floor_builds_slower_and_stripping_builds_nothing() {
        let ladder = LadderConfig::builtin();
        for key in [
            RungKey::PlantTended,
            RungKey::PlantField,
            RungKey::AnimalPastoral,
            RungKey::AnimalPen,
        ] {
            let rung = ladder.rung(key);
            let accrual = |floor| {
                rung.build_accrual(
                    rung.verb_improvement(),
                    true,
                    floor,
                    reference_crew(rung),
                    NO_BUILD_GEAR,
                )
            };
            // Liveness first: an ordering sweep alone would pass on an accrual that returned zero
            // everywhere, which is exactly what a broken gate looks like.
            assert!(
                accrual(FOOD_PEAK_FLOOR) > 0.0,
                "{key:?} must actually build at the food peak"
            );
            let mut previous = 0.0_f32;
            for floor in [0.1_f32, 0.25, FOOD_PEAK_FLOOR, 0.75, 1.0] {
                let built = accrual(floor);
                assert!(
                    built > previous,
                    "{key:?}: floor {floor} must build faster than the floor below it ({built} vs \
                     {previous})"
                );
                previous = built;
            }
            assert_eq!(
                accrual(STRIP_IT_BARE),
                0.0,
                "{key:?}: a crew stripping the source it is improving builds nothing"
            );
            // The **other** degenerate end is the caller's, not this seam's: at `floor = 1.0` the
            // rate is its highest but nothing stands above the floor, so the caller's `eligible` is
            // false and nothing is built. Asserted here as the seam's half of it — `eligible = false`
            // is always zero, whatever the floor.
            assert_eq!(
                rung.build_accrual(
                    rung.verb_improvement(),
                    false,
                    1.0,
                    reference_crew(rung),
                    NO_BUILD_GEAR,
                ),
                0.0,
                "{key:?}: watching a source builds nothing, however restrained the watching"
            );
        }
    }

    /// **`learn_multiplier` scales the ACCRUAL and not the decay** — the asymmetry with the cost
    /// multiplier, which reaches both. Decay happens on turns nobody works the source, so there is no
    /// assignment and no floor to read; scaling it would multiply by a number that does not exist in
    /// that state.
    #[test]
    fn the_floor_scales_the_build_but_never_the_decay() {
        let ladder = LadderConfig::builtin();
        let tended = ladder.rung(RungKey::PlantTended);
        let build = tended.build.as_ref().expect("the tended rung builds");
        let stated_bleed = build
            .decay_fraction_per_turn
            .expect("the tended rung bleeds")
            * build.work_cost;
        assert_eq!(
            tended.build_decay(RUNG_COST_UNSCALED),
            stated_bleed,
            "the decay is a fraction of the rung's own cost — no floor is folded into it"
        );
        // **The cost multiplier reaches BOTH sides**, which is what keeps a rung's build:decay ratio
        // invariant per source for free (`slow to tame, slow to forget`).
        const TWICE_THE_WORK: f32 = 2.0;
        assert_eq!(
            tended.build_decay(TWICE_THE_WORK),
            stated_bleed * TWICE_THE_WORK,
            "a source that is twice the job also bleeds twice as slowly, in proportion"
        );
        assert_eq!(
            tended.build_cost(TWICE_THE_WORK),
            Some(build.work_cost * TWICE_THE_WORK)
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
}
