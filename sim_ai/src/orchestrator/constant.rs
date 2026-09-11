//! **`ConstantStance`** — the v1 orchestrator (`docs/plan_ai_driver.md` §3).
//!
//! The stance is the archetype's. The budgets are the profile's weights normalised over the
//! enabled specialists; the priorities are the same weights un-normalised. An alarm is answered by
//! moving `tuning.alarm_budget_shift` of worker share to the alarming specialist, taken from the
//! others in proportion to what they hold, for **one cadence** — the next cadence plan reverts it.
//! That is enough to prove the plan channel and the alarm channel are both live.
//!
//! It re-plans when `tick − since_turn ≥ goal_cadence_turns`, or when an alarm arrived since the
//! last plan. The stance never changes, so its switch count is zero by construction — the bench
//! pins it there.

use std::collections::BTreeMap;

use super::{Alarm, Budget, Orchestrator, Plan, Stance};
use crate::profile::{AiProfile, WEIGHT_TO_SPECIALIST};
use crate::specialists::SpecialistId;
use crate::view::{SeatMemory, SeatView};

pub struct ConstantStance {
    /// The specialists the plan funds, in the order the weights name them.
    enabled: Vec<SpecialistId>,
    goal_cadence_turns: u64,
    alarm_budget_shift: f32,
    current: Option<Plan>,
}

impl ConstantStance {
    pub fn new(enabled: &[SpecialistId], goal_cadence_turns: u64, alarm_budget_shift: f32) -> Self {
        Self {
            enabled: enabled.to_vec(),
            goal_cadence_turns,
            alarm_budget_shift,
            current: None,
        }
    }

    /// The profile's weights, over the enabled specialists: normalised as budgets, raw as
    /// priorities. A profile whose enabled weights sum to zero funds everything equally.
    fn base_plan(&self, profile: &AiProfile, stance: Stance, tick: u64) -> Plan {
        let raw: Vec<(SpecialistId, f32)> = WEIGHT_TO_SPECIALIST
            .iter()
            .filter(|(_, specialist)| self.enabled.contains(specialist))
            .map(|(key, specialist)| (*specialist, profile.weight(key)))
            .collect();
        let total: f32 = raw.iter().map(|(_, weight)| weight).sum();
        let count = raw.len() as f32;
        let budgets = raw
            .iter()
            .map(|(specialist, weight)| {
                let share = if total > 0.0 {
                    weight / total
                } else {
                    1.0 / count
                };
                (
                    *specialist,
                    Budget {
                        worker_share: share,
                    },
                )
            })
            .collect();
        let priorities = raw.into_iter().collect::<BTreeMap<_, _>>();
        Plan {
            stance,
            budgets,
            priorities,
            since_turn: tick,
        }
    }

    /// Move `alarm_budget_shift` of share to `to`, from the others in proportion to their holding.
    fn shift_toward(plan: &mut Plan, to: SpecialistId, shift: f32) {
        let others: f32 = plan
            .budgets
            .iter()
            .filter(|(id, _)| **id != to)
            .map(|(_, budget)| budget.worker_share)
            .sum();
        if others <= 0.0 || !plan.budgets.contains_key(to) {
            return;
        }
        let moved = shift.min(others);
        for (id, budget) in plan.budgets.iter_mut() {
            if *id == to {
                budget.worker_share += moved;
            } else {
                budget.worker_share -= moved * (budget.worker_share / others);
            }
        }
    }
}

impl Orchestrator for ConstantStance {
    fn plan(
        &mut self,
        view: &SeatView,
        _memory: &SeatMemory,
        profile: &AiProfile,
        alarms: &[Alarm],
    ) -> Option<Plan> {
        let tick = view.snapshot.header.tick;
        let due = match &self.current {
            None => true,
            Some(plan) => tick.saturating_sub(plan.since_turn) >= self.goal_cadence_turns,
        };
        if !due && alarms.is_empty() {
            return None;
        }
        let mut plan = self.base_plan(profile, profile.archetype.stance(), tick);
        // The most recent alarm wins the shift; one cadence later the base plan is back.
        if let Some(alarm) = alarms.last() {
            Self::shift_toward(&mut plan, alarm.specialist, self.alarm_budget_shift);
        }
        self.current = Some(plan.clone());
        Some(plan)
    }

    /// Drop a plan the new epoch is earlier than, so the very next `plan` is due at the new tick
    /// rather than waiting out a `since_turn` from a world that no longer exists.
    fn forget_after(&mut self, tick: u64) {
        if self
            .current
            .as_ref()
            .is_some_and(|plan| plan.since_turn > tick)
        {
            self.current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::AlarmKind;
    use crate::profile::{AiProfiles, DEFAULT_DIFFICULTY};
    use crate::specialists::{SPECIALIST_FOOD, SPECIALIST_LAND};
    use crate::view::SeatMemory;
    use sim_runtime::WorldSnapshot;

    const SHIFT: f32 = 0.25;

    fn view_at(tick: u64) -> SeatView {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.tick = tick;
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    fn forager() -> AiProfile {
        AiProfiles::builtin().profile("forager").unwrap().clone()
    }

    fn cadence() -> u64 {
        AiProfiles::builtin()
            .difficulty(DEFAULT_DIFFICULTY)
            .unwrap()
            .goal_cadence_turns
    }

    #[test]
    fn budgets_normalise_over_the_enabled_specialists_and_priorities_stay_raw() {
        let mut orchestrator =
            ConstantStance::new(&[SPECIALIST_FOOD, SPECIALIST_LAND], cadence(), SHIFT);
        let profile = forager();
        let memory = SeatMemory::default();
        let plan = orchestrator
            .plan(&view_at(1), &memory, &profile, &[])
            .expect("the first call plans");
        assert_eq!(plan.stance, Stance::Consolidate);
        let total: f32 = plan.budgets.values().map(|b| b.worker_share).sum();
        assert!((total - 1.0).abs() < 1e-6, "shares sum to one: {total}");
        assert!(plan.worker_share(SPECIALIST_FOOD) > plan.worker_share(SPECIALIST_LAND));
        assert_eq!(
            plan.priority(SPECIALIST_FOOD),
            profile.weight("food_security")
        );
        assert_eq!(plan.since_turn, 1);
        // Only Food enabled: it holds the whole budget.
        let mut solo = ConstantStance::new(&[SPECIALIST_FOOD], cadence(), SHIFT);
        let plan = solo.plan(&view_at(1), &memory, &profile, &[]).unwrap();
        assert_eq!(plan.worker_share(SPECIALIST_FOOD), 1.0);
        assert_eq!(plan.worker_share(SPECIALIST_LAND), 0.0);
    }

    #[test]
    fn it_replans_on_its_cadence_and_not_before() {
        let mut orchestrator =
            ConstantStance::new(&[SPECIALIST_FOOD, SPECIALIST_LAND], cadence(), SHIFT);
        let profile = forager();
        let memory = SeatMemory::default();
        assert!(orchestrator
            .plan(&view_at(1), &memory, &profile, &[])
            .is_some());
        assert!(orchestrator
            .plan(&view_at(1 + cadence() - 1), &memory, &profile, &[])
            .is_none());
        let renewed = orchestrator
            .plan(&view_at(1 + cadence()), &memory, &profile, &[])
            .expect("the cadence came round");
        assert_eq!(renewed.since_turn, 1 + cadence());
    }

    #[test]
    fn an_alarm_shifts_budget_to_the_alarming_specialist_and_the_next_cadence_reverts_it() {
        let mut orchestrator =
            ConstantStance::new(&[SPECIALIST_FOOD, SPECIALIST_LAND], cadence(), SHIFT);
        let profile = forager();
        let memory = SeatMemory::default();
        let base = orchestrator
            .plan(&view_at(1), &memory, &profile, &[])
            .unwrap();
        let alarm = Alarm {
            specialist: SPECIALIST_LAND,
            kind: AlarmKind::LandShort,
            since_tick: 2,
        };
        let shifted = orchestrator
            .plan(&view_at(2), &memory, &profile, &[alarm])
            .expect("an alarm re-plans early");
        assert!(
            (shifted.worker_share(SPECIALIST_LAND) - (base.worker_share(SPECIALIST_LAND) + SHIFT))
                .abs()
                < 1e-6
        );
        let total: f32 = shifted.budgets.values().map(|b| b.worker_share).sum();
        assert!(
            (total - 1.0).abs() < 1e-6,
            "the shift is a transfer, not a grant"
        );
        assert_eq!(shifted.stance, base.stance, "the stance never moves");
        let reverted = orchestrator
            .plan(&view_at(2 + cadence()), &memory, &profile, &[])
            .expect("the cadence came round");
        assert_eq!(reverted.budgets, base.budgets);
    }
}
