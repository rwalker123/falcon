//! **The arbiter** — proposals become the turn (`docs/plan_ai_driver.md` §5). Six steps, in
//! order, and every step is a reason a proposal can be rejected — which is what the decision log
//! records:
//!
//! 1. **Behaviour gate** — an intent class the profile's `behaviors` forbid → `behavior_gated`.
//! 2. **Priority** — `score *= plan.priorities[specialist]`.
//! 3. **Commitment** — `score *= 1 + profile.commitment` when the intent was chosen last turn.
//! 4. **Selection under difficulty** — sorted by final score; `selection_top_k = 1` is argmax,
//!    above it each pass picks uniformly among the top k remaining.
//! 5. **Feasibility** — walking that order: a repeated intent is `outscored`; a band already
//!    ordered this turn is `conflict`; workers past the specialist's share of the working-age
//!    pool is `over_budget`.
//! 6. **Emit** — the accepted proposals' commands, in order. `ready` is the loop's, and always
//!    follows.
//!
//! The pass-through arbiter (the scripted fixture's) skips every step: every proposal is accepted
//! in order, raw and final scores equal, so a script's records are exactly what slice 3 wrote.

use std::collections::{BTreeMap, BTreeSet};

use rand::rngs::StdRng;
use rand::Rng;
use sim_runtime::CommandPayload;

use crate::instruments::decisions::{
    command_text, Decision, DecisionRecord, DecisionSink, Outcome,
};
use crate::orchestrator::Plan;
use crate::profile::{AiProfile, Difficulty, ARGMAX_TOP_K};
use crate::specialists::{
    intent_class, Proposal, SpecialistId, INTENT_CLASS_RAID, INTENT_CLASS_TRADE,
};
use crate::view::SeatMemory;

/// The fixed rejection set (`plan_ai_driver.md` §5, §8.2).
pub const REJECTED_BEHAVIOR_GATED: &str = "behavior_gated";
pub const REJECTED_OVER_BUDGET: &str = "over_budget";
pub const REJECTED_CONFLICT: &str = "conflict";
pub const REJECTED_OUTSCORED: &str = "outscored";

/// One accepted proposal: what to send and what to remember.
#[derive(Debug)]
pub struct Accepted {
    pub intent: String,
    pub commands: Vec<CommandPayload>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arbiter {
    /// The six steps.
    Weighing,
    /// Everything accepted in order — the scripted fixture's.
    PassThrough,
}

/// A proposal with its owner, as the arbiter receives it.
#[derive(Debug)]
pub struct Offered {
    pub specialist: SpecialistId,
    pub proposal: Proposal,
}

struct Scored {
    offered: Offered,
    score_final: f32,
}

impl Arbiter {
    /// Arbitrate one turn. `working_age_total` is the pool the budgets are shares of.
    #[allow(clippy::too_many_arguments)]
    pub fn arbitrate(
        self,
        tick: u64,
        offered: Vec<Offered>,
        plan: &Plan,
        profile: &AiProfile,
        difficulty: &Difficulty,
        memory: &SeatMemory,
        working_age_total: u32,
        rng: &mut StdRng,
        sink: &mut dyn DecisionSink,
    ) -> Vec<Accepted> {
        match self {
            Arbiter::PassThrough => offered
                .into_iter()
                .map(|offered| {
                    let score = offered.proposal.score;
                    record(sink, tick, &offered, score, Outcome::Accepted);
                    accept(offered)
                })
                .collect(),
            Arbiter::Weighing => weigh(
                tick,
                offered,
                plan,
                profile,
                difficulty,
                memory,
                working_age_total,
                rng,
                sink,
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn weigh(
    tick: u64,
    offered: Vec<Offered>,
    plan: &Plan,
    profile: &AiProfile,
    difficulty: &Difficulty,
    memory: &SeatMemory,
    working_age_total: u32,
    rng: &mut StdRng,
    sink: &mut dyn DecisionSink,
) -> Vec<Accepted> {
    // 1–3: gate, priority, commitment.
    let mut scored: Vec<Scored> = Vec::with_capacity(offered.len());
    for offered in offered {
        if let Some(gate) = gated_by(profile, &offered.proposal.intent) {
            record(sink, tick, &offered, offered.proposal.score, rejected(gate));
            continue;
        }
        let mut score_final = offered.proposal.score * plan.priority(offered.specialist);
        if memory.chosen_last_turn(&offered.proposal.intent) {
            score_final *= 1.0 + profile.commitment;
        }
        scored.push(Scored {
            offered,
            score_final,
        });
    }

    // 4: selection under difficulty.
    let ordered = select(scored, difficulty.selection_top_k, rng);

    // 5: feasibility, in that order.
    let mut budget: BTreeMap<SpecialistId, u32> = plan
        .budgets
        .keys()
        .map(|specialist| {
            let share = plan.worker_share(specialist);
            (
                *specialist,
                (share * working_age_total as f32).floor() as u32,
            )
        })
        .collect();
    let mut ordered_bands: BTreeSet<u64> = BTreeSet::new();
    let mut intents: BTreeSet<String> = BTreeSet::new();
    let mut accepted = Vec::new();
    for Scored {
        offered,
        score_final,
    } in ordered
    {
        let proposal = &offered.proposal;
        let remaining = budget.entry(offered.specialist).or_insert(0);
        let verdict = if intents.contains(&proposal.intent) {
            Err(REJECTED_OUTSCORED)
        } else if proposal
            .cost
            .bands
            .iter()
            .any(|band| ordered_bands.contains(band))
        {
            Err(REJECTED_CONFLICT)
        } else if proposal.cost.workers > *remaining {
            Err(REJECTED_OVER_BUDGET)
        } else {
            Ok(())
        };
        match verdict {
            Ok(()) => {
                *remaining -= proposal.cost.workers;
                ordered_bands.extend(proposal.cost.bands.iter().copied());
                intents.insert(proposal.intent.clone());
                record(sink, tick, &offered, score_final, Outcome::Accepted);
                accepted.push(accept(offered));
            }
            Err(reason) => record(sink, tick, &offered, score_final, rejected(reason)),
        }
    }
    accepted
}

/// Step 1: the class of intent the profile forbids, if this is one.
fn gated_by(profile: &AiProfile, intent: &str) -> Option<&'static str> {
    let class = intent_class(intent);
    let forbidden = (class == INTENT_CLASS_RAID && !profile.behaviors.will_raid)
        || (class == INTENT_CLASS_TRADE && !profile.behaviors.will_trade);
    forbidden.then_some(REJECTED_BEHAVIOR_GATED)
}

/// Step 4: the order proposals are walked in. Sorted by final score; at `top_k > 1` each pass
/// draws uniformly among the top k still unpicked, so a lower-scored proposal can go first —
/// the AI evaluates correctly at every difficulty and only its follow-through varies.
fn select(mut scored: Vec<Scored>, top_k: u32, rng: &mut StdRng) -> Vec<Scored> {
    scored.sort_by(|a, b| b.score_final.total_cmp(&a.score_final));
    if top_k <= ARGMAX_TOP_K {
        return scored;
    }
    let mut ordered = Vec::with_capacity(scored.len());
    while !scored.is_empty() {
        let window = (top_k as usize).min(scored.len());
        let pick = rng.gen_range(0..window);
        ordered.push(scored.remove(pick));
    }
    ordered
}

fn rejected(reason: &str) -> Outcome {
    Outcome::Rejected {
        rejected_by: reason.to_owned(),
    }
}

fn accept(offered: Offered) -> Accepted {
    Accepted {
        intent: offered.proposal.intent,
        commands: offered.proposal.commands,
    }
}

fn record(
    sink: &mut dyn DecisionSink,
    tick: u64,
    offered: &Offered,
    score_final: f32,
    outcome: Outcome,
) {
    sink.record(DecisionRecord::Decision(Decision {
        tick,
        specialist: offered.specialist.to_owned(),
        intent: offered.proposal.intent.clone(),
        score_raw: offered.proposal.score,
        score_final,
        outcome,
        reason: offered.proposal.reason.clone(),
        commands: offered.proposal.commands.len(),
        commands_text: offered.proposal.commands.iter().map(command_text).collect(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::decisions::VecSink;
    use crate::orchestrator::{Budget, Stance};
    use crate::profile::{AiProfiles, DEFAULT_DIFFICULTY};
    use crate::specialists::{Cost, SPECIALIST_FOOD, SPECIALIST_LAND};
    use rand::SeedableRng;

    const TICK: u64 = 5;
    const WORKING_AGE: u32 = 20;
    const BAND_A: u64 = 1;
    const BAND_B: u64 = 2;

    fn profile() -> AiProfile {
        AiProfiles::builtin().profile("forager").unwrap().clone()
    }

    fn difficulty(top_k: u32) -> Difficulty {
        Difficulty {
            selection_top_k: top_k,
            ..AiProfiles::builtin()
                .difficulty(DEFAULT_DIFFICULTY)
                .unwrap()
        }
    }

    /// Food holds half the pool (10 workers), Land the other half; both at priority 1.
    fn plan() -> Plan {
        Plan {
            stance: Stance::Consolidate,
            budgets: BTreeMap::from([
                (SPECIALIST_FOOD, Budget { worker_share: 0.5 }),
                (SPECIALIST_LAND, Budget { worker_share: 0.5 }),
            ]),
            priorities: BTreeMap::from([(SPECIALIST_FOOD, 1.0), (SPECIALIST_LAND, 1.0)]),
            since_turn: TICK,
        }
    }

    fn offer(
        specialist: SpecialistId,
        intent: &str,
        score: f32,
        workers: u32,
        band: u64,
    ) -> Offered {
        Offered {
            specialist,
            proposal: Proposal {
                commands: vec![CommandPayload::Resync],
                intent: intent.to_owned(),
                score,
                cost: Cost {
                    workers,
                    bands: vec![band],
                },
                reason: "test".to_owned(),
            },
        }
    }

    fn run(
        offered: Vec<Offered>,
        top_k: u32,
        memory: &SeatMemory,
    ) -> (Vec<Accepted>, Vec<Decision>) {
        let mut sink = VecSink::default();
        let accepted = Arbiter::Weighing.arbitrate(
            TICK,
            offered,
            &plan(),
            &profile(),
            &difficulty(top_k),
            memory,
            WORKING_AGE,
            &mut StdRng::seed_from_u64(0),
            &mut sink,
        );
        let decisions = sink
            .0
            .into_iter()
            .filter_map(|record| match record {
                DecisionRecord::Decision(decision) => Some(decision),
                _ => None,
            })
            .collect();
        (accepted, decisions)
    }

    fn rejection<'a>(decisions: &'a [Decision], intent: &str) -> Option<&'a str> {
        decisions
            .iter()
            .find(|decision| decision.intent == intent)
            .and_then(|decision| match &decision.outcome {
                Outcome::Rejected { rejected_by } => Some(rejected_by.as_str()),
                Outcome::Accepted => None,
            })
    }

    #[test]
    fn every_rejection_reason_is_produced_and_every_proposal_is_recorded() {
        let offered = vec![
            offer(SPECIALIST_FOOD, "food:assign:1", 0.9, 4, BAND_A),
            offer(SPECIALIST_FOOD, "food:assign:1", 0.5, 4, BAND_B), // same intent: outscored
            offer(SPECIALIST_LAND, "land:move:1", 0.8, 0, BAND_A),   // band A taken: conflict
            offer(SPECIALIST_LAND, "land:split:2", 0.7, 11, BAND_B), // 11 > Land's 10: over budget
            offer(SPECIALIST_LAND, "contact:raid:9", 1.0, 0, 9),     // will_raid false: gated
        ];
        let (accepted, decisions) = run(offered, ARGMAX_TOP_K, &SeatMemory::default());
        assert_eq!(decisions.len(), 5, "one record per proposal");
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].intent, "food:assign:1");
        assert_eq!(
            rejection(&decisions, "contact:raid:9"),
            Some(REJECTED_BEHAVIOR_GATED)
        );
        assert_eq!(
            rejection(&decisions, "land:move:1"),
            Some(REJECTED_CONFLICT)
        );
        assert_eq!(
            rejection(&decisions, "land:split:2"),
            Some(REJECTED_OVER_BUDGET)
        );
        let outscored = decisions
            .iter()
            .filter(|d| d.intent == "food:assign:1")
            .filter_map(|d| match &d.outcome {
                Outcome::Rejected { rejected_by } => Some(rejected_by.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(outscored, vec![REJECTED_OUTSCORED]);
    }

    #[test]
    fn the_commitment_bonus_applies_only_to_a_remembered_intent() {
        let mut memory = SeatMemory::default();
        memory.record_choices(
            TICK - 1,
            BTreeSet::from(["land:move:1".to_owned()]),
            [].iter(),
        );
        let offered = vec![
            offer(SPECIALIST_FOOD, "food:assign:1", 0.5, 1, BAND_A),
            offer(SPECIALIST_LAND, "land:move:1", 0.4, 0, BAND_B),
        ];
        let (_, decisions) = run(offered, ARGMAX_TOP_K, &memory);
        let by_intent = |intent: &str| decisions.iter().find(|d| d.intent == intent).unwrap();
        assert_eq!(by_intent("food:assign:1").score_final, 0.5);
        let bonus = 1.0 + profile().commitment;
        assert!((by_intent("land:move:1").score_final - 0.4 * bonus).abs() < 1e-6);
    }

    #[test]
    fn top_k_one_is_argmax_and_feasibility_never_orders_a_band_twice() {
        let offered = vec![
            offer(SPECIALIST_FOOD, "food:assign:1", 0.2, 1, BAND_A),
            offer(SPECIALIST_LAND, "land:move:1", 0.9, 0, BAND_A),
            offer(SPECIALIST_FOOD, "food:assign:2", 0.5, 1, BAND_B),
        ];
        let (accepted, _) = run(offered, ARGMAX_TOP_K, &SeatMemory::default());
        let intents: Vec<&str> = accepted.iter().map(|a| a.intent.as_str()).collect();
        assert_eq!(
            intents,
            vec!["land:move:1", "food:assign:2"],
            "highest first, band A once"
        );
    }

    #[test]
    fn top_k_above_one_samples_among_the_top_k_deterministically_from_the_seed() {
        let offered = || {
            vec![
                offer(SPECIALIST_FOOD, "food:assign:1", 0.9, 1, BAND_A),
                offer(SPECIALIST_FOOD, "food:assign:2", 0.8, 1, BAND_B),
                offer(SPECIALIST_FOOD, "food:assign:3", 0.1, 1, 3),
            ]
        };
        let (first, _) = run(offered(), 2, &SeatMemory::default());
        let (again, _) = run(offered(), 2, &SeatMemory::default());
        let order = |accepted: &[Accepted]| {
            accepted
                .iter()
                .map(|a| a.intent.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            order(&first),
            order(&again),
            "the same seed replays the same order"
        );
        assert_eq!(first.len(), 3, "sampling changes the order, not the set");
        assert_ne!(
            order(&first)[2],
            "food:assign:1",
            "the top proposal is never last with k = 2"
        );
    }

    #[test]
    fn pass_through_accepts_everything_in_order_with_raw_equal_to_final() {
        let mut sink = VecSink::default();
        let offered = vec![
            offer(SPECIALIST_FOOD, "script", 1.0, 0, BAND_A),
            offer(SPECIALIST_FOOD, "script", 1.0, 0, BAND_A),
        ];
        let accepted = Arbiter::PassThrough.arbitrate(
            TICK,
            offered,
            &Plan::pass_through(TICK),
            &profile(),
            &difficulty(ARGMAX_TOP_K),
            &SeatMemory::default(),
            0,
            &mut StdRng::seed_from_u64(0),
            &mut sink,
        );
        assert_eq!(
            accepted.len(),
            2,
            "no conflict, no budget, no duplicate-intent rule"
        );
        assert!(sink.0.iter().all(|record| matches!(
            record,
            DecisionRecord::Decision(Decision { score_raw, score_final, outcome: Outcome::Accepted, .. }) if score_raw == score_final
        )));
    }
}
