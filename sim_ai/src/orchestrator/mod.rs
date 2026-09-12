//! **The orchestrator** — personality becomes a plan (`docs/plan_ai_driver.md` §3).
//!
//! It reads the view, the profile, the alarms raised since it last ran and its own memory, and
//! produces a [`Plan`]: a stance, a budget per specialist in the units specialists spend, and a
//! priority per specialist. It never emits a command and never sees a proposal. Personality enters
//! the AI here and only here — a specialist reads *its* budget and priority, never the profile's
//! weights.
//!
//! v1 is [`constant::ConstantStance`]; a `UtilityOrchestrator` or an LLM one produces the same
//! `Plan`, so nothing below the plan changes when the orchestrator does.
//!
//! **It also resolves the demand board** (`plan_ai_driver.md` §4, *"The board's first customer
//! is outfitting"*): for every own band whose outfitting window is open, [`Orchestrator::outfit`]
//! turns the specialists' posted demands into one [`Outfit`] against the window's two budgets —
//! by its goals and personality — and the composite emits the `set_starting_loadout`. It still
//! never emits a command: it resolves, the composite sends.

pub mod constant;

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sim_runtime::{BandLoadoutWindowState, PopulationCohortState};

use crate::board::{Entry, Resource};
use crate::instruments::decisions::GoalsRecord;
use crate::profile::{AiProfile, FoodGoalLevers};
use crate::specialists::{SpecialistId, SPECIALIST_FOOD};
use crate::view::{SeatMemory, SeatView};

/// The name the decision log files a loadout under — not a specialist, and never on the roster.
pub const ORCHESTRATOR_ID: &str = "orchestrator";
/// The loadout's intent kind: `orchestrator:outfit:<band>`.
pub const INTENT_OUTFIT: &str = "outfit";

/// **One window's loadout, resolved**: the kit and material lines to send (coalesced, never a
/// `none` kit, never above either budget) and, per demand **in the order given**, what each
/// was granted — what the board is told.
#[derive(Debug, Clone, PartialEq)]
pub struct Outfit {
    pub band: u64,
    pub kits: Vec<(String, u32)>,
    pub materials: Vec<(String, u32)>,
    pub grants: Vec<(Resource, u32)>,
}

/// The v1 stance set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    Expand,
    Consolidate,
    Seek,
}

impl Stance {
    /// The token the plan record carries.
    pub fn as_str(self) -> &'static str {
        match self {
            Stance::Expand => "expand",
            Stance::Consolidate => "consolidate",
            Stance::Seek => "seek",
        }
    }
}

/// What a specialist may spend this turn. **v1 unit: a share of the seat's working-age
/// population** — bands are arbitrated by conflict (one order per band), not budgeted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub worker_share: f32,
}

/// **The plant rungs a seat can be sent toward** (`core_sim/src/data/intensification_ladder.json`
/// → `rungs[branch == "plant"]`: `wild` → `tended` → `field`). Ordered: `Field > Tended > Wild`,
/// so "the goal is above wild" is a comparison.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GroundRung {
    Wild,
    Tended,
    Field,
}

impl GroundRung {
    /// The token the plan record carries — the ladder's own rung id.
    pub fn as_str(self) -> &'static str {
        match self {
            GroundRung::Wild => "wild",
            GroundRung::Tended => "tended",
            GroundRung::Field => "field",
        }
    }
}

/// What `Food` is for this cadence, in the units the frame reports (`plan_ai_driver.md` §3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoodGoals {
    /// Target food income minus consumption, per turn, for the seat's bands together. ≥ 0.
    pub net_income_per_turn: f32,
    /// Target minimum own-band runway, in turns.
    pub runway_turns: f32,
    /// The rung the seat should be climbing toward: `Tended` (cultivate) or `Field` (sow).
    pub ground_rung: GroundRung,
}

impl FoodGoals {
    /// The profile's levers, as the goals the plan carries.
    pub fn from_levers(levers: &FoodGoalLevers) -> Self {
        Self {
            net_income_per_turn: levers.net_income_per_turn,
            runway_turns: levers.runway_turns,
            ground_rung: levers.ground_rung,
        }
    }

    /// The goals as the plan record carries them.
    pub fn record(&self) -> GoalsRecord {
        GoalsRecord {
            net_income_per_turn: self.net_income_per_turn,
            runway_turns: self.runway_turns,
            ground_rung: self.ground_rung.as_str().to_owned(),
        }
    }
}

/// A specialist's goals. `Land` gets none in v1; the enum is the extension point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goals {
    Food(FoodGoals),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub stance: Stance,
    pub budgets: BTreeMap<SpecialistId, Budget>,
    pub priorities: BTreeMap<SpecialistId, f32>,
    /// The targets each specialist scores progress toward.
    pub goals: BTreeMap<SpecialistId, Goals>,
    /// The tick this plan was adopted — hysteresis is measurable.
    pub since_turn: u64,
}

impl Plan {
    /// The plan a brain with no orchestrator runs under: no stance to hold, no budget to
    /// charge, no goals — the pass-through arbiter reads none of it, and the scripted brain it
    /// serves has no `Food` specialist.
    pub fn pass_through(tick: u64) -> Self {
        Self {
            stance: Stance::Consolidate,
            budgets: BTreeMap::new(),
            priorities: BTreeMap::new(),
            goals: BTreeMap::new(),
            since_turn: tick,
        }
    }

    /// `Food`'s goals, or `None` when the plan does not fund it — a rule that needs a goal and
    /// gets none proposes nothing.
    pub fn food_goals(&self) -> Option<FoodGoals> {
        self.goals
            .get(SPECIALIST_FOOD)
            .map(|Goals::Food(goals)| *goals)
    }

    /// The goals as the plan record carries them.
    pub fn goals_record(&self) -> BTreeMap<String, GoalsRecord> {
        self.goals
            .iter()
            .map(|(id, goals)| {
                let record = match goals {
                    Goals::Food(food) => food.record(),
                };
                ((*id).to_owned(), record)
            })
            .collect()
    }

    /// A specialist's share, or nothing: a specialist the plan does not fund spends nothing.
    pub fn worker_share(&self, specialist: SpecialistId) -> f32 {
        self.budgets
            .get(specialist)
            .map_or(0.0, |budget| budget.worker_share)
    }

    pub fn priority(&self, specialist: SpecialistId) -> f32 {
        self.priorities.get(specialist).copied().unwrap_or_default()
    }

    /// The budgets as the plan record carries them.
    pub fn budgets_record(&self) -> BTreeMap<String, f32> {
        self.budgets
            .iter()
            .map(|(id, budget)| ((*id).to_owned(), budget.worker_share))
            .collect()
    }

    pub fn priorities_record(&self) -> BTreeMap<String, f32> {
        self.priorities
            .iter()
            .map(|(id, priority)| ((*id).to_owned(), *priority))
            .collect()
    }
}

/// What a specialist alarms about. One kind per specialist — the metric it owns, failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmKind {
    /// `Food`: the minimum own-band runway is below the profile's floor.
    FoodShort,
    /// `Land`: a band's ground cannot feed it and nothing better is in view.
    LandShort,
}

impl AlarmKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AlarmKind::FoodShort => "food_short",
            AlarmKind::LandShort => "land_short",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alarm {
    pub specialist: SpecialistId,
    pub kind: AlarmKind,
    pub since_tick: u64,
}

/// The plug. `plan` is asked every turn and answers `Some` only when it re-plans — on its cadence
/// or on an alarm — so the caller records exactly the plans adopted.
pub trait Orchestrator {
    fn plan(
        &mut self,
        view: &SeatView,
        memory: &SeatMemory,
        profile: &AiProfile,
        alarms: &[Alarm],
    ) -> Option<Plan>;

    /// ⛔ **A DROPPED PLAN MUST DROP THE CADENCE THAT MADE IT.** A full frame at `tick` replaced the
    /// view with a world the standing plan is *later* than (a `new_game`, a load, a rollback), so
    /// the brain forgets it — and unless the orchestrator forgets its own bookkeeping with it, the
    /// next `plan` is still measured against the old world's `since_turn`.
    ///
    /// That is not a cosmetic mismatch. The new epoch starts at tick 0, `tick − since_turn`
    /// underflows to nothing, no re-plan is due, and `Composite::plan_for` falls back to
    /// [`Plan::pass_through`] — empty budgets and zero priorities, so `Food::budget_workers` is 0,
    /// every consideration returns `None` and the seat plays nothing at all until the old world's
    /// cadence would have come round.
    fn forget_after(&mut self, _tick: u64) {}

    /// Resolve `band`'s open outfitting `window` against the `demands` posted for it (module
    /// docs): what to send, and what each demand was granted.
    fn outfit(
        &mut self,
        view: &SeatView,
        profile: &AiProfile,
        band: &PopulationCohortState,
        window: &BandLoadoutWindowState,
        demands: &[&Entry],
    ) -> Outfit;
}
