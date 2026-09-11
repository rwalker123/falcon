//! **Brains** — the plug the turn loop drives (`docs/plan_ai_opponents.md` §2;
//! `docs/plan_ai_driver.md` §1).
//!
//! `decide` is handed the seat's view and hands back the commands to send this turn; the loop
//! sends them and then submits `ready` whatever came back. Three brains ship, and two of them are
//! configurations of one [`Composite`] — the layered shape with parts removed, not separate code:
//!
//! | Brain | Orchestrator | Specialists | Arbiter |
//! |---|---|---|---|
//! | [`PassBrain`] | none | none | emits only `ready` — the **control** |
//! | [`ScriptedBrain`] | none | `Scripted` | pass-through — the **fixture** |
//! | [`UtilityBrain`] | `ConstantStance` | `Food`, `Land` | the six steps — the opponent |

use std::collections::BTreeSet;
use std::path::Path;

use rand::rngs::StdRng;
use sim_runtime::CommandPayload;
use tracing::{info, warn};

use crate::arbiter::{Arbiter, Offered};
use crate::instruments::decisions::{AlarmRecord, DecisionRecord, DecisionSink, PlanRecord};
use crate::instruments::scoreboard::{COMMAND_FAILED_LABEL_SUFFIX, EVENT_TICK_LAG};
use crate::orchestrator::constant::ConstantStance;
use crate::orchestrator::{Orchestrator, Plan};
use crate::profile::{AiProfile, AiProfiles, Difficulty, ProfileError};
use crate::specialists::food::Food;
use crate::specialists::land::Land;
use crate::specialists::scripted::{ScriptError, Scripted};
use crate::specialists::{
    Specialist, SpecialistId, DISABLEABLE_SPECIALISTS, SPECIALIST_FOOD, SPECIALIST_LAND,
};
use crate::view::{SeatMemory, SeatView};

/// The plug. An external program in another language implements the same contract over the
/// socket; inside this crate it is this trait.
///
/// `sink` is where the brain's decision records go (`docs/plan_ai_driver.md` §8.1): a brain that
/// weighs proposals writes one `Decision` per proposal, accepted or not, so every command it
/// returns has a row behind it (§10). A brain with nothing to say leaves the sink untouched — the
/// `ready` row is the loop's, not the brain's.
pub trait Brain {
    fn decide(
        &mut self,
        view: &SeatView,
        rng: &mut StdRng,
        sink: &mut dyn DecisionSink,
    ) -> Vec<CommandPayload>;

    /// A full frame at `tick` replaced the view (a resync, a rollback): memory stamped later than
    /// it is for a world that no longer exists.
    fn on_full_frame(&mut self, _tick: u64) {}
}

/// Submits end-turn and nothing else.
#[derive(Debug, Default)]
pub struct PassBrain;

impl Brain for PassBrain {
    fn decide(
        &mut self,
        _view: &SeatView,
        _rng: &mut StdRng,
        _sink: &mut dyn DecisionSink,
    ) -> Vec<CommandPayload> {
        Vec::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BrainError {
    #[error(transparent)]
    Profile(#[from] ProfileError),
    #[error(
        "`{0}` is not a specialist that can be disabled; the roster is {DISABLEABLE_SPECIALISTS:?}"
    )]
    NoSuchSpecialist(String),
}

/// The layered shape: orchestrator → specialists → arbiter, over one memory.
pub struct Composite {
    faction: u32,
    profile: AiProfile,
    difficulty: Difficulty,
    orchestrator: Option<Box<dyn Orchestrator>>,
    specialists: Vec<Box<dyn Specialist>>,
    arbiter: Arbiter,
    memory: SeatMemory,
    plan: Option<Plan>,
}

impl Composite {
    fn new(
        faction: u32,
        profile: AiProfile,
        difficulty: Difficulty,
        orchestrator: Option<Box<dyn Orchestrator>>,
        specialists: Vec<Box<dyn Specialist>>,
        arbiter: Arbiter,
    ) -> Self {
        Self {
            faction,
            profile,
            difficulty,
            orchestrator,
            specialists,
            arbiter,
            memory: SeatMemory::new(difficulty.memory_horizon_turns),
            plan: None,
        }
    }

    #[cfg(test)]
    pub fn specialist_ids(&self) -> Vec<SpecialistId> {
        self.specialists.iter().map(|s| s.id()).collect()
    }

    /// The plan in force at `tick`: the orchestrator's when it re-plans, else the standing one,
    /// else — with no orchestrator — the unbounded pass-through plan.
    fn plan_for(&mut self, view: &SeatView, sink: &mut dyn DecisionSink) -> Plan {
        let tick = view.tick();
        let alarms = self.memory.take_alarms();
        if let Some(orchestrator) = self.orchestrator.as_mut() {
            if let Some(plan) = orchestrator.plan(view, &self.memory, &self.profile, &alarms) {
                sink.record(DecisionRecord::Plan(PlanRecord {
                    tick,
                    stance: plan.stance.as_str().to_owned(),
                    since_tick: plan.since_turn,
                    budgets: plan.budgets_record(),
                    priorities: plan.priorities_record(),
                }));
                self.plan = Some(plan);
            }
        }
        self.plan
            .clone()
            .unwrap_or_else(|| Plan::pass_through(tick))
    }
}

impl Brain for Composite {
    fn decide(
        &mut self,
        view: &SeatView,
        rng: &mut StdRng,
        sink: &mut dyn DecisionSink,
    ) -> Vec<CommandPayload> {
        let tick = view.tick();
        self.memory.observe(view, self.faction);
        // A command the sim refused last turn is a specialist's bug; say which, with the sim's reason.
        for refused in view
            .snapshot
            .command_events
            .iter()
            .filter(|event| event.tick + EVENT_TICK_LAG == tick && event.faction == self.faction)
            .filter(|event| event.label.ends_with(COMMAND_FAILED_LABEL_SUFFIX))
        {
            warn!(
                tick,
                label = %refused.label,
                detail = refused.detail.as_deref().unwrap_or_default(),
                "the server refused a command"
            );
        }
        let plan = self.plan_for(view, sink);

        let mut offered = Vec::new();
        for specialist in &mut self.specialists {
            let id = specialist.id();
            let proposals = specialist.propose(view, &plan, &self.memory);
            if let Some(alarm) = proposals.alarm {
                sink.record(DecisionRecord::Alarm(AlarmRecord {
                    tick,
                    specialist: alarm.specialist.to_owned(),
                    alarm: alarm.kind.as_str().to_owned(),
                }));
                self.memory.push_alarm(alarm);
            }
            offered.extend(proposals.proposals.into_iter().map(|proposal| Offered {
                specialist: id,
                proposal,
            }));
        }

        let working_age_total: u32 = view
            .own_bands(self.faction)
            .map(|band| band.working_age)
            .sum();
        let accepted = self.arbiter.arbitrate(
            tick,
            offered,
            &plan,
            &self.profile,
            &self.difficulty,
            &self.memory,
            working_age_total,
            rng,
            sink,
        );
        let intents: BTreeSet<String> = accepted.iter().map(|a| a.intent.clone()).collect();
        let commands: Vec<CommandPayload> = accepted
            .into_iter()
            .flat_map(|accepted| accepted.commands)
            .collect();
        self.memory.record_choices(tick, intents, commands.iter());
        self.memory.remember_runways(view, self.faction);
        commands
    }

    fn on_full_frame(&mut self, tick: u64) {
        self.memory.forget_after(tick);
        // The orchestrator forgets with the plan: a stale `since_turn` is what would leave the seat
        // on `Plan::pass_through` — no budget, no priority, no orders — for a whole cadence after a
        // rebuild (`Orchestrator::forget_after`).
        if let Some(orchestrator) = self.orchestrator.as_mut() {
            orchestrator.forget_after(tick);
        }
        if self
            .plan
            .as_ref()
            .is_some_and(|plan| plan.since_turn > tick)
        {
            self.plan = None;
        }
    }
}

/// The fixture: one `Scripted` specialist through a pass-through arbiter.
pub struct ScriptedBrain;

impl ScriptedBrain {
    pub fn load(path: &Path, faction: u32) -> Result<Composite, ScriptError> {
        Ok(Self::composite(Scripted::load(path, faction)?, faction))
    }

    #[cfg(test)]
    pub fn from_script(text: &str, faction: u32) -> Result<Composite, ScriptError> {
        Ok(Self::composite(
            Scripted::from_script(text, faction)?,
            faction,
        ))
    }

    fn composite(scripted: Scripted, faction: u32) -> Composite {
        let profiles = AiProfiles::builtin();
        let profile = profiles
            .profile(profiles.default_profile_id())
            .expect("the default profile exists")
            .clone();
        let difficulty = profiles
            .difficulty(crate::profile::DEFAULT_DIFFICULTY)
            .expect("the default difficulty exists");
        Composite::new(
            faction,
            profile,
            difficulty,
            None,
            vec![Box::new(scripted)],
            Arbiter::PassThrough,
        )
    }
}

/// The opponent: `ConstantStance`, `Food`, `Land`, the six-step arbiter.
pub struct UtilityBrain;

impl UtilityBrain {
    /// The composite for `profile_id` at `difficulty_id`, minus the `disabled` specialists.
    pub fn build(
        faction: u32,
        profiles: &AiProfiles,
        profile_id: &str,
        difficulty_id: &str,
        disabled: &[String],
    ) -> Result<Composite, BrainError> {
        for name in disabled {
            if !DISABLEABLE_SPECIALISTS.contains(&name.as_str()) {
                return Err(BrainError::NoSuchSpecialist(name.clone()));
            }
        }
        let profile = profiles.profile(profile_id)?.clone();
        let difficulty = profiles.difficulty(difficulty_id)?;
        let enabled = |id: SpecialistId| !disabled.iter().any(|name| name == id);
        let mut specialists: Vec<Box<dyn Specialist>> = Vec::new();
        if enabled(SPECIALIST_FOOD) {
            specialists.push(Box::new(Food::new(
                faction,
                profile.food,
                profile.weight(crate::profile::WEIGHT_FOOD_SECURITY),
            )));
        }
        if enabled(SPECIALIST_LAND) {
            specialists.push(Box::new(Land::new(
                faction,
                profile.land,
                profile.weight(crate::profile::WEIGHT_LAND_CLAIM),
            )));
        }
        let roster: Vec<SpecialistId> = specialists.iter().map(|s| s.id()).collect();
        let orchestrator = ConstantStance::new(
            &roster,
            difficulty.goal_cadence_turns,
            profiles.tuning.alarm_budget_shift,
        );
        info!(
            faction,
            profile = profile_id,
            difficulty = difficulty_id,
            ?roster,
            "utility brain"
        );
        Ok(Composite::new(
            faction,
            profile,
            difficulty,
            Some(Box::new(orchestrator)),
            specialists,
            Arbiter::Weighing,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::decisions::{Decision, Outcome, VecSink};
    use crate::profile::DEFAULT_DIFFICULTY;
    use crate::specialists::food::tests::{a_view, FACTION};
    use crate::specialists::scripted::{SCRIPT_INTENT, SCRIPT_SCORE};
    use crate::specialists::SPECIALIST_SCRIPTED;
    use rand::SeedableRng;
    use sim_runtime::{OrdersDirective, PopulationCohortState, WorldSnapshot};

    const A_TICK: u64 = 12;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(0)
    }

    fn a_view_at(tick: u64) -> SeatView {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.tick = tick;
        snapshot.populations.push(PopulationCohortState {
            faction: 1,
            band_id: 7001,
            ..Default::default()
        });
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    fn decisions(sink: VecSink) -> Vec<Decision> {
        sink.0
            .into_iter()
            .filter_map(|record| match record {
                DecisionRecord::Decision(decision) => Some(decision),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_pass_brain_submits_nothing_and_records_nothing() {
        let mut sink = VecSink::default();
        assert!(PassBrain
            .decide(&a_view_at(A_TICK), &mut rng(), &mut sink)
            .is_empty());
        assert!(sink.0.is_empty());
    }

    #[test]
    fn the_scripted_brain_records_slice_threes_decision_and_never_a_plan() {
        let mut brain = ScriptedBrain::from_script(
            &format!("{A_TICK}: split_band {{faction}} {{own_band:0}} 4"),
            1,
        )
        .expect("parses");
        let mut sink = VecSink::default();
        assert!(brain
            .decide(&a_view_at(A_TICK - 1), &mut rng(), &mut sink)
            .is_empty());
        let commands = brain.decide(&a_view_at(A_TICK), &mut rng(), &mut sink);
        assert_eq!(commands.len(), 1);
        assert!(!sink.0.iter().any(|r| matches!(r, DecisionRecord::Plan(_))));
        let decisions = decisions(sink);
        assert_eq!(decisions.len(), 1);
        let decision = &decisions[0];
        assert_eq!(decision.specialist, SPECIALIST_SCRIPTED);
        assert_eq!(decision.intent, SCRIPT_INTENT);
        assert_eq!(
            (decision.score_raw, decision.score_final),
            (SCRIPT_SCORE, SCRIPT_SCORE)
        );
        assert_eq!(decision.outcome, Outcome::Accepted);
        assert_eq!(decision.reason, "split_band 1 7001 4");
        assert_eq!(decision.commands, 1);
    }

    #[test]
    fn the_utility_brain_plans_on_its_first_turn_assigns_idle_hands_and_never_sends_ready() {
        let profiles = AiProfiles::builtin();
        let mut brain =
            UtilityBrain::build(FACTION, &profiles, "forager", DEFAULT_DIFFICULTY, &[]).unwrap();
        assert_eq!(
            brain.specialist_ids(),
            vec![SPECIALIST_FOOD, SPECIALIST_LAND]
        );
        let mut sink = VecSink::default();
        let commands = brain.decide(&a_view(), &mut rng(), &mut sink);
        assert!(
            !commands.iter().any(|c| matches!(
                c,
                CommandPayload::Orders {
                    directive: OrdersDirective::Ready,
                    ..
                }
            )),
            "ready is the loop's, and always last"
        );
        assert!(
            sink.0.iter().any(|r| matches!(r, DecisionRecord::Plan(_))),
            "a plan on the first turn"
        );
        let decisions = decisions(sink);
        let assigned = decisions
            .iter()
            .find(|d| d.intent == "food:assign:7001")
            .expect("idle hands proposed");
        assert_eq!(assigned.outcome, Outcome::Accepted);
        assert!(commands
            .iter()
            .any(|c| matches!(c, CommandPayload::AssignLabor { .. })));
    }

    #[test]
    fn a_disabled_specialist_is_off_the_roster_and_an_unknown_one_is_refused() {
        let profiles = AiProfiles::builtin();
        let brain = UtilityBrain::build(
            FACTION,
            &profiles,
            "rover",
            "hard",
            &[SPECIALIST_LAND.to_owned()],
        )
        .unwrap();
        assert_eq!(brain.specialist_ids(), vec![SPECIALIST_FOOD]);
        assert!(matches!(
            UtilityBrain::build(FACTION, &profiles, "rover", "hard", &["herd".to_owned()]),
            Err(BrainError::NoSuchSpecialist(_))
        ));
        assert!(matches!(
            UtilityBrain::build(FACTION, &profiles, "warlord", "hard", &[]),
            Err(BrainError::Profile(_))
        ));
    }

    /// ⛔ **A REBUILT WORLD PLAYS ON ITS FIRST TICK.** Dropping the plan is half the job: the
    /// orchestrator still held the old world's `since_turn`, so at the new epoch's tick 0 no
    /// re-plan was due, `plan_for` fell back to `Plan::pass_through` — no budget, no priority —
    /// and the seat proposed nothing at all until the old cadence would have come round.
    #[test]
    fn a_full_frame_earlier_than_the_plan_drops_it_and_the_band_acts_at_the_new_epoch() {
        let profiles = AiProfiles::builtin();
        let mut brain =
            UtilityBrain::build(FACTION, &profiles, "forager", DEFAULT_DIFFICULTY, &[]).unwrap();
        let mut sink = VecSink::default();
        brain.decide(&a_view(), &mut rng(), &mut sink);
        assert!(brain.plan.is_some());

        // The rebuild: a full frame at tick 0 of a world the standing plan is later than.
        let rebuilt_tick = 0;
        brain.on_full_frame(rebuilt_tick);
        assert!(
            brain.plan.is_none(),
            "a rollback before the plan forgets it"
        );

        let mut rebuilt = a_view();
        rebuilt.snapshot.header.tick = rebuilt_tick;
        let mut sink = VecSink::default();
        let commands = brain.decide(&rebuilt, &mut rng(), &mut sink);
        let plan = sink
            .0
            .iter()
            .find_map(|record| match record {
                DecisionRecord::Plan(plan) => Some(plan),
                _ => None,
            })
            .expect("the first tick of the new epoch re-plans");
        assert_eq!(plan.since_tick, rebuilt_tick);
        assert!(
            plan.budgets.values().any(|share| *share > 0.0),
            "a pass-through plan funds nobody: {:?}",
            plan.budgets
        );
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, CommandPayload::AssignLabor { .. })),
            "the band puts its idle hands to work on the new world's first tick"
        );
    }
}
