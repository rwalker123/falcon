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
//!
//! **The v1 outfit resolution** ([`Orchestrator::outfit`]): the kit demands ranked by
//! `priority × the profile weight of the requester's domain` (`food` → `food_security`, `land` →
//! `land_claim`, ties by requester id), walked granting `min(asked, budget left)` — on a
//! splinter's take, capped by the parent's supply of **every item** the kit lists — two demands
//! for one kit coalesced into one line; materials the same way against `material_budget`, and on
//! a grant window **the campaign pre-fill fills whatever material budget the demands leave**
//! (`opening_loadout.material_defaults`, the sim's own suggestion, scaled to the points left in
//! its own proportions and floored — [`prefill_over`]). Never a `none` kit, a kit the roster does
//! not name, or a total above either budget: the sim refuses the whole order for any of them. **Every grant under the ask says why** ([`Grant`]): a demand
//! the budget cannot reach at all is declined, one it reaches partly is trimmed — and a craft
//! demand is declined `no crafter yet`, because nothing runs a bench for the seat.

use std::collections::BTreeMap;

use sim_runtime::{BandLoadoutWindowState, OpeningMaterialDefaultState, PopulationCohortState};

use super::{Alarm, Budget, FoodGoals, Goals, Orchestrator, Outfit, Plan, Stance};
use crate::board::{Entry, Grant, Resource, BARE_KIT_ID};
use crate::profile::{AiProfile, WEIGHT_TO_SPECIALIST};
use crate::specialists::{SpecialistId, SPECIALIST_FOOD};
use crate::view::{SeatMemory, SeatView};

/// A grant window: `parent_band_id` is `0` — *"`0` — a GRANT window. The picks mint gear, capped
/// by `kit_budget` … and `material_budget`"* (`BandLoadoutWindowState`); non-zero is a take on
/// that band, capped by the parent's supply instead.
const GRANT_WINDOW: u64 = 0;

/// **Why a grant fell short**, as the board records it.
pub const DECLINED_NO_CRAFTER: &str = "no crafter yet";
pub const DECLINED_BARE_KIT: &str = "the bare kit is never a line";
pub const DECLINED_UNKNOWN_KIT: &str = "not on the kit roster";
pub const DECLINED_KIT_BUDGET: &str = "kit budget spent";
pub const DECLINED_PARENT_SUPPLY: &str = "parent cannot supply";
pub const DECLINED_MATERIAL_BUDGET: &str = "material budget spent";
pub const DECLINED_NOT_PICKABLE: &str = "not on the pick list";

/// The grant for `asked` against `cap`, with the reason when it falls short.
fn grant_against(asked: u32, cap: u32, short_because: &str) -> Grant {
    let granted = asked.min(cap);
    if granted == 0 {
        Grant::declined(short_because)
    } else if granted < asked {
        Grant::trimmed(
            granted,
            format!("trimmed from {asked} to {granted}: {short_because}"),
        )
    } else {
        Grant::whole(granted)
    }
}

/// **The campaign pre-fill over the material points the demands left**: each default row scaled
/// by `min(1, budget_left / Σ defaults)` and floored — proportional, remainder unspent, the
/// `clamped_kit_defaults` rule on the material side — so a grant that spent part of its budget
/// on what a specialist asked for still carries the sim's suggestion for the rest, in the
/// suggestion's own mix. Clamping row by row instead spent the whole budget on the first rows
/// and none on the last; skipping the pre-fill when anything was asked left a third of the
/// budget on the table (the hoe estimate's `bone 6, fibre 12` against 30 points).
fn prefill_over(defaults: &[OpeningMaterialDefaultState], budget_left: u32) -> Vec<(String, u32)> {
    let total: u32 = defaults.iter().map(|row| row.units).sum();
    if total == 0 || budget_left == 0 {
        return Vec::new();
    }
    let scale = (budget_left as f32 / total as f32).min(1.0);
    defaults
        .iter()
        .map(|row| {
            (
                row.material_id.clone(),
                (row.units as f32 * scale).floor() as u32,
            )
        })
        .filter(|(_, units)| *units > 0)
        .collect()
}

/// Add `units` of `id` to a coalesced line list.
fn coalesce(lines: &mut Vec<(String, u32)>, id: &str, units: u32) {
    match lines.iter_mut().find(|(held, _)| held == id) {
        Some((_, held)) => *held += units,
        None => lines.push((id.to_owned(), units)),
    }
}

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
    /// priorities. A profile whose enabled weights sum to zero funds everything equally. `Food`,
    /// when enabled, is handed the profile's goals; they never move with an alarm in v1 — the
    /// budget shift is the alarm's answer.
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
        let goals = self
            .enabled
            .contains(&SPECIALIST_FOOD)
            .then(|| {
                (
                    SPECIALIST_FOOD,
                    Goals::Food(FoodGoals::from_levers(&profile.goals)),
                )
            })
            .into_iter()
            .collect();
        Plan {
            stance,
            budgets,
            priorities,
            goals,
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

    fn outfit(
        &mut self,
        view: &SeatView,
        profile: &AiProfile,
        band: &PopulationCohortState,
        window: &BandLoadoutWindowState,
        demands: &[&Entry],
    ) -> Outfit {
        // The weight of a requester's domain: the inverse of `WEIGHT_TO_SPECIALIST`.
        let weight_of = |requester: SpecialistId| {
            WEIGHT_TO_SPECIALIST
                .iter()
                .find(|(_, specialist)| *specialist == requester)
                .map_or(0.0, |(key, _)| profile.weight(key))
        };
        let rank = |entry: &Entry| entry.demand.priority * weight_of(entry.demand.requester);
        let mut order: Vec<usize> = (0..demands.len()).collect();
        order.sort_by(|&a, &b| {
            rank(demands[b])
                .total_cmp(&rank(demands[a]))
                .then_with(|| demands[a].demand.requester.cmp(demands[b].demand.requester))
        });
        let is_take = window.parent_band_id != GRANT_WINDOW;
        let mut kit_budget = window.kit_budget;
        let mut material_budget = window.material_budget;
        // A take's caps: `BandLoadoutSupplyRowState` is *"One cap row … how many units of `id`
        // this take may claim"*, keyed per **item** — *"A kit row cannot be capped on its own …
        // what the sim validates is the expanded item list, whole"* — so a kit's cap is the
        // minimum over its items, and every kit granted draws its items down.
        let mut item_supply: BTreeMap<&str, u32> = window
            .parent_item_supply
            .iter()
            .map(|row| (row.id.as_str(), row.units))
            .collect();
        let mut material_supply: BTreeMap<&str, u32> = window
            .parent_material_supply
            .iter()
            .map(|row| (row.id.as_str(), row.units))
            .collect();
        let mut grants: Vec<(Resource, Grant)> = demands
            .iter()
            .map(|entry| {
                (
                    entry.demand.resource.clone(),
                    Grant::declined(DECLINED_KIT_BUDGET),
                )
            })
            .collect();
        let mut kits = Vec::new();
        let mut materials = Vec::new();
        for index in order {
            let demand = &demands[index].demand;
            match &demand.resource {
                Resource::Kit(id) => {
                    if id == BARE_KIT_ID {
                        grants[index].1 = Grant::declined(DECLINED_BARE_KIT);
                        continue;
                    }
                    let Some(kit) = view.snapshot.kits.iter().find(|kit| &kit.id == id) else {
                        grants[index].1 = Grant::declined(DECLINED_UNKNOWN_KIT);
                        continue;
                    };
                    let (cap, short_because) = if is_take {
                        (
                            kit.item_ids
                                .iter()
                                .map(|item| item_supply.get(item.as_str()).copied().unwrap_or(0))
                                .min()
                                .unwrap_or(0),
                            DECLINED_PARENT_SUPPLY,
                        )
                    } else {
                        (kit_budget, DECLINED_KIT_BUDGET)
                    };
                    let grant = grant_against(demand.amount, cap, short_because);
                    let granted = grant.granted;
                    grants[index].1 = grant;
                    if granted == 0 {
                        continue;
                    }
                    if is_take {
                        for item in &kit.item_ids {
                            if let Some(units) = item_supply.get_mut(item.as_str()) {
                                *units -= granted;
                            }
                        }
                    } else {
                        kit_budget -= granted;
                    }
                    coalesce(&mut kits, id, granted);
                }
                Resource::Material(id) => {
                    let (cap, short_because) = if is_take {
                        (
                            material_supply.get(id.as_str()).copied().unwrap_or(0),
                            DECLINED_PARENT_SUPPLY,
                        )
                    } else if view
                        .snapshot
                        .opening_loadout
                        .pickable_materials
                        .contains(id)
                    {
                        (material_budget, DECLINED_MATERIAL_BUDGET)
                    } else {
                        (0, DECLINED_NOT_PICKABLE)
                    };
                    let grant = grant_against(demand.amount, cap, short_because);
                    let granted = grant.granted;
                    grants[index].1 = grant;
                    if granted == 0 {
                        continue;
                    }
                    if is_take {
                        if let Some(units) = material_supply.get_mut(id.as_str()) {
                            *units -= granted;
                        }
                    } else {
                        material_budget -= granted;
                    }
                    coalesce(&mut materials, id, granted);
                }
                // Nothing runs a bench for the seat: the ask and its timing are recorded, and
                // that is all this slice does with it.
                Resource::Craft { .. } => {
                    grants[index].1 = Grant::declined(DECLINED_NO_CRAFTER);
                }
            }
        }
        // A grant window: the campaign's own pre-fill over whatever the demands left, in its own
        // proportions. A splinter's take carries no pre-fill.
        if !is_take {
            for (id, units) in prefill_over(
                &view.snapshot.opening_loadout.material_defaults,
                material_budget,
            ) {
                coalesce(&mut materials, &id, units);
            }
        }
        Outfit {
            band: band.band_id,
            kits,
            materials,
            grants,
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

    use crate::board::{Demand, DemandState};
    use crate::specialists::food::tests::{a_view, BAND, FORAGE_KIT, HUNT_KIT, HUNT_KIT_ITEM};
    use sim_runtime::{BandLoadoutSupplyRowState, KitOptionState, OpeningMaterialDefaultState};

    fn entry(requester: SpecialistId, resource: Resource, amount: u32, priority: f32) -> Entry {
        Entry {
            demand: Demand {
                requester,
                band: BAND,
                resource,
                amount,
                by_tick: 1,
                priority,
            },
            posted_tick: 1,
            state: DemandState::Posted,
        }
    }

    fn grant_window(kit_budget: u32, material_budget: u32) -> BandLoadoutWindowState {
        BandLoadoutWindowState {
            open: true,
            kit_budget,
            material_budget,
            ..Default::default()
        }
    }

    /// Two kit demands over the budget: the higher-ranked (`priority × domain weight`) is granted
    /// whole and the other what is left; a `none` kit is never a line; a grant window nobody
    /// asked material for takes the campaign pre-fill, scaled to the budget in its own
    /// proportions (17 : 8 over 20 points is 13 : 6, floored).
    #[test]
    fn outfit_grants_in_priority_order_and_pre_fills_material_on_a_grant_window() {
        let mut orchestrator =
            ConstantStance::new(&[SPECIALIST_FOOD, SPECIALIST_LAND], cadence(), SHIFT);
        let mut view = a_view();
        view.snapshot.opening_loadout.material_defaults = vec![
            OpeningMaterialDefaultState {
                material_id: "fibre".to_owned(),
                units: 17,
            },
            OpeningMaterialDefaultState {
                material_id: "hide".to_owned(),
                units: 8,
            },
        ];
        let band = &view.snapshot.populations[0];
        // Land's scout kit at 0.5 × 0.3 ranks under Food's baskets at 1.0 × 0.9 and its spears at
        // 0.8 × 0.9; the budget of 10 covers 8 baskets and 2 of the 3 spears, and no scout kit
        // (the fixture roster does not carry one either).
        let entries = [
            entry(
                SPECIALIST_LAND,
                Resource::Kit("wayfinding".to_owned()),
                1,
                0.5,
            ),
            entry(
                SPECIALIST_FOOD,
                Resource::Kit(FORAGE_KIT.to_owned()),
                8,
                1.0,
            ),
            entry(SPECIALIST_FOOD, Resource::Kit(HUNT_KIT.to_owned()), 3, 0.8),
            entry(SPECIALIST_FOOD, Resource::Kit("none".to_owned()), 1, 1.0),
        ];
        let demands: Vec<&Entry> = entries.iter().collect();
        let outfit = orchestrator.outfit(&view, &forager(), band, &grant_window(10, 20), &demands);
        assert_eq!(outfit.band, BAND);
        assert_eq!(
            outfit.kits,
            vec![(FORAGE_KIT.to_owned(), 8), (HUNT_KIT.to_owned(), 2)]
        );
        assert_eq!(
            outfit.grants,
            vec![
                (
                    Resource::Kit("wayfinding".to_owned()),
                    Grant::declined(DECLINED_UNKNOWN_KIT)
                ),
                (Resource::Kit(FORAGE_KIT.to_owned()), Grant::whole(8)),
                (
                    Resource::Kit(HUNT_KIT.to_owned()),
                    Grant::trimmed(2, format!("trimmed from 3 to 2: {DECLINED_KIT_BUDGET}"))
                ),
                (
                    Resource::Kit("none".to_owned()),
                    Grant::declined(DECLINED_BARE_KIT)
                ),
            ]
        );
        assert_eq!(
            outfit.materials,
            vec![("fibre".to_owned(), 13), ("hide".to_owned(), 6)],
            "the pre-fill, scaled to the 20 points in its own mix"
        );
        // A material demand is capped by the budget and the pick list; a budget it spends whole
        // leaves no pre-fill.
        view.snapshot.opening_loadout.pickable_materials = vec!["bone".to_owned()];
        let entries = [
            entry(
                SPECIALIST_FOOD,
                Resource::Material("bone".to_owned()),
                25,
                1.0,
            ),
            entry(
                SPECIALIST_FOOD,
                Resource::Material("gold".to_owned()),
                1,
                1.0,
            ),
            entry(
                SPECIALIST_FOOD,
                Resource::Craft {
                    item: "hoes".to_owned(),
                    start_tick: 9,
                },
                4,
                1.0,
            ),
        ];
        let demands: Vec<&Entry> = entries.iter().collect();
        let band = &view.snapshot.populations[0];
        let outfit = orchestrator.outfit(&view, &forager(), band, &grant_window(10, 20), &demands);
        assert_eq!(outfit.materials, vec![("bone".to_owned(), 20)]);
        assert_eq!(
            outfit.grants[0].1,
            Grant::trimmed(
                20,
                format!("trimmed from 25 to 20: {DECLINED_MATERIAL_BUDGET}")
            )
        );
        assert_eq!(
            outfit.grants[1].1,
            Grant::declined(DECLINED_NOT_PICKABLE),
            "not on the pick list"
        );
        assert_eq!(
            outfit.grants[2].1,
            Grant::declined(DECLINED_NO_CRAFTER),
            "a craft is recorded and declined; nothing crafts"
        );
    }

    /// **Demands first, the pre-fill over what is left.** Six bone asked of thirty points with a
    /// pre-fill of bone 3 / fibre 17 / hide 8 (28): the six are granted, the 24 points left take
    /// the pre-fill at 24/28 — bone 2, fibre 14, hide 6, floored — so the line reads bone 8,
    /// fibre 14, hide 6; nothing is displaced and nothing is spent past the budget. A pre-fill
    /// smaller than what is left goes whole.
    #[test]
    fn outfit_fills_the_material_budget_the_demands_leave_with_the_pre_fill() {
        let mut orchestrator = ConstantStance::new(&[SPECIALIST_FOOD], cadence(), SHIFT);
        let mut view = a_view();
        let default = |id: &str, units: u32| OpeningMaterialDefaultState {
            material_id: id.to_owned(),
            units,
        };
        view.snapshot.opening_loadout.material_defaults =
            vec![default("bone", 3), default("fibre", 17), default("hide", 8)];
        view.snapshot.opening_loadout.pickable_materials =
            vec!["bone".to_owned(), "fibre".to_owned(), "hide".to_owned()];
        let entries = [entry(
            SPECIALIST_FOOD,
            Resource::Material("bone".to_owned()),
            6,
            1.0,
        )];
        let demands: Vec<&Entry> = entries.iter().collect();
        let band = &view.snapshot.populations[0];
        let outfit = orchestrator.outfit(&view, &forager(), band, &grant_window(10, 30), &demands);
        assert_eq!(outfit.grants[0].1, Grant::whole(6));
        assert_eq!(
            outfit.materials,
            vec![
                ("bone".to_owned(), 8),
                ("fibre".to_owned(), 14),
                ("hide".to_owned(), 6)
            ]
        );
        assert!(
            outfit.materials.iter().map(|(_, units)| units).sum::<u32>() <= 30,
            "never past the budget"
        );
        // Sixty points: the pre-fill goes whole on top of the demand.
        let outfit = orchestrator.outfit(&view, &forager(), band, &grant_window(10, 60), &demands);
        assert_eq!(
            outfit.materials,
            vec![
                ("bone".to_owned(), 9),
                ("fibre".to_owned(), 17),
                ("hide".to_owned(), 8)
            ]
        );
        assert_eq!(prefill_over(&[], 30), Vec::<(String, u32)>::new());
        assert_eq!(prefill_over(&[default("hide", 8)], 0), Vec::new());
    }

    /// A splinter's take is capped by the parent's supply of every item the kit lists — the
    /// sled both hunting kits carry is drawn down by the first grant — and takes no material
    /// pre-fill.
    #[test]
    fn outfit_caps_a_splinters_take_at_the_parents_supply_of_every_item() {
        let mut orchestrator = ConstantStance::new(&[SPECIALIST_FOOD], cadence(), SHIFT);
        let mut view = a_view();
        for kit in &mut view.snapshot.kits {
            if kit.id == HUNT_KIT {
                kit.item_ids.push("sled".to_owned());
            }
        }
        view.snapshot.kits.push(KitOptionState {
            id: "trapping".to_owned(),
            jobs: vec!["hunt".to_owned()],
            item_ids: vec!["traps".to_owned(), "sled".to_owned()],
            ..Default::default()
        });
        view.snapshot.opening_loadout.material_defaults = vec![OpeningMaterialDefaultState {
            material_id: "fibre".to_owned(),
            units: 17,
        }];
        let supply = |id: &str, units: u32| BandLoadoutSupplyRowState {
            id: id.to_owned(),
            units,
        };
        let window = BandLoadoutWindowState {
            open: true,
            parent_band_id: BAND,
            parent_item_supply: vec![
                supply(HUNT_KIT_ITEM, 6),
                supply("traps", 4),
                supply("sled", 5),
            ],
            ..Default::default()
        };
        let entries = [
            entry(SPECIALIST_FOOD, Resource::Kit(HUNT_KIT.to_owned()), 9, 0.9),
            entry(
                SPECIALIST_FOOD,
                Resource::Kit("trapping".to_owned()),
                9,
                0.8,
            ),
        ];
        let demands: Vec<&Entry> = entries.iter().collect();
        let band = &view.snapshot.populations[0];
        let outfit = orchestrator.outfit(&view, &forager(), band, &window, &demands);
        // Spears cap the stalking kit at 5 (6 spears, 5 sleds); the trapping kit gets the 0
        // sleds left — nothing.
        assert_eq!(outfit.kits, vec![(HUNT_KIT.to_owned(), 5)]);
        assert_eq!(outfit.grants[1].1, Grant::declined(DECLINED_PARENT_SUPPLY));
        assert!(outfit.materials.is_empty(), "a splinter takes no pre-fill");
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
    fn food_is_handed_the_profiles_goals_and_a_plan_without_food_carries_none() {
        let profile = forager();
        let memory = SeatMemory::default();
        let mut orchestrator =
            ConstantStance::new(&[SPECIALIST_FOOD, SPECIALIST_LAND], cadence(), SHIFT);
        let plan = orchestrator
            .plan(&view_at(1), &memory, &profile, &[])
            .unwrap();
        let goals = plan.food_goals().expect("Food is funded, so it has goals");
        assert_eq!(goals.net_income_per_turn, profile.goals.net_income_per_turn);
        assert_eq!(goals.runway_turns, profile.goals.runway_turns);
        assert_eq!(goals.ground_rung, profile.goals.ground_rung);
        let record = plan.goals_record();
        assert_eq!(record.len(), 1, "Land has no goals in v1: {record:?}");
        assert_eq!(record[SPECIALIST_FOOD].ground_rung, "field");
        // An alarm shifts budget, never the goals.
        let alarm = Alarm {
            specialist: SPECIALIST_LAND,
            kind: AlarmKind::LandShort,
            since_tick: 2,
        };
        let shifted = orchestrator
            .plan(&view_at(2), &memory, &profile, &[alarm])
            .unwrap();
        assert_eq!(shifted.food_goals(), Some(goals));
        // Food off the roster: no goals for it.
        let mut land_only = ConstantStance::new(&[SPECIALIST_LAND], cadence(), SHIFT);
        let plan = land_only.plan(&view_at(1), &memory, &profile, &[]).unwrap();
        assert_eq!(plan.food_goals(), None);
        assert_eq!(Plan::pass_through(1).food_goals(), None);
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
