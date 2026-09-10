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

pub mod constant;

use std::collections::BTreeMap;

use crate::profile::AiProfile;
use crate::specialists::SpecialistId;
use crate::view::{SeatMemory, SeatView};

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

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub stance: Stance,
    pub budgets: BTreeMap<SpecialistId, Budget>,
    pub priorities: BTreeMap<SpecialistId, f32>,
    /// The tick this plan was adopted — hysteresis is measurable.
    pub since_turn: u64,
}

impl Plan {
    /// The plan a brain with no orchestrator runs under: no stance to hold, no budget to
    /// charge — the pass-through arbiter reads none of it.
    pub fn pass_through(tick: u64) -> Self {
        Self {
            stance: Stance::Consolidate,
            budgets: BTreeMap::new(),
            priorities: BTreeMap::new(),
            since_turn: tick,
        }
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
}
