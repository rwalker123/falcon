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

use std::path::Path;

use rand::rngs::StdRng;
use sim_runtime::CommandPayload;
use tracing::{info, warn};

use sim_runtime::{render_command_line, StartingKitAllocation, StartingMaterialAllocation};

use crate::arbiter::{Arbiter, Offered};
use crate::board::{Board, Entry, Resource};
use crate::instruments::decisions::{
    AlarmRecord, Decision, DecisionRecord, DecisionSink, Outcome, PlanRecord,
};
use crate::instruments::scoreboard::{COMMAND_FAILED_LABEL_SUFFIX, EVENT_TICK_LAG};
use crate::orchestrator::constant::ConstantStance;
use crate::orchestrator::{Alarm, Orchestrator, Plan, INTENT_OUTFIT, ORCHESTRATOR_ID};
use crate::profile::{AiProfile, AiProfiles, Difficulty, ProfileError};
use crate::specialists::food::Food;
use crate::specialists::land::Land;
use crate::specialists::scripted::{ScriptError, Scripted};
use crate::specialists::{
    intent_key, Specialist, SpecialistId, DISABLEABLE_SPECIALISTS, SPECIALIST_FOOD, SPECIALIST_LAND,
};
use crate::view::{SeatMemory, SeatView};

/// **A loadout's score on the decision log**: it passes no arbiter step — it spends no worker
/// budget and no band order — so it is recorded accepted at a score that means "not weighed",
/// as the scripted fixture's `SCRIPT_SCORE` does.
pub const OUTFIT_SCORE: f32 = 1.0;

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

    /// What this brain holds that the frame does not — the plan and alarms in force and its
    /// memory — so the observation record (`instruments::observations`) can say what the brain
    /// was looking at. A brain with none of it (`PassBrain`) answers the empty lens.
    fn lens(&self) -> BrainLens<'_> {
        BrainLens::default()
    }
}

/// A read-only window into a brain's state at the moment before it decides
/// (`docs/plan_ai_driver.md` §8.4). Every field is optional because the pass and scripted brains
/// hold none of it.
#[derive(Default, Clone, Copy)]
pub struct BrainLens<'a> {
    /// The plan in force — adopted on an earlier tick; this tick's `decide` may replace it.
    pub plan: Option<&'a Plan>,
    /// The alarms raised since that plan, which the next plan will weigh.
    pub alarms: &'a [Alarm],
    pub memory: Option<&'a SeatMemory>,
    /// The profile's `land.horizon_tiles`: how far `Land` looks, and the observation's radius floor.
    pub horizon_tiles: u32,
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

/// The layered shape: orchestrator → specialists → arbiter, over one memory and one board.
pub struct Composite {
    faction: u32,
    profile: AiProfile,
    difficulty: Difficulty,
    orchestrator: Option<Box<dyn Orchestrator>>,
    specialists: Vec<Box<dyn Specialist>>,
    arbiter: Arbiter,
    memory: SeatMemory,
    plan: Option<Plan>,
    /// The demand board (`board.rs`): nothing but this composite and the orchestrator touch it,
    /// and a specialist never sees it.
    board: Board,
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
        let memory = SeatMemory::new(
            difficulty.memory_horizon_turns,
            profile.food.split_settle_turns,
        );
        Self {
            faction,
            profile,
            difficulty,
            orchestrator,
            specialists,
            arbiter,
            memory,
            plan: None,
            board: Board::default(),
        }
    }

    /// **The loadouts for every own band whose window is open**, one `set_starting_loadout`
    /// each, resolved by the orchestrator from the board's open demands and recorded as an
    /// accepted decision under `orchestrator:outfit:<band>`. A window with nothing to send gets
    /// no order: an empty order on a splinter's take *"would hand the whole dowry back"*
    /// (`BandLoadoutWindowState::kits`).
    fn outfit_windows(
        &mut self,
        view: &SeatView,
        tick: u64,
        sink: &mut dyn DecisionSink,
    ) -> Vec<CommandPayload> {
        let mut commands = Vec::new();
        let Some(orchestrator) = self.orchestrator.as_mut() else {
            return commands;
        };
        let roster: Vec<SpecialistId> = self.specialists.iter().map(|s| s.id()).collect();
        for band in view.own_bands(self.faction) {
            let Some(window) = band.loadout_window.as_ref().filter(|window| window.open) else {
                continue;
            };
            let entries: Vec<Entry> = self.board.open_for(band.band_id, tick).cloned().collect();
            let borrowed: Vec<&Entry> = entries.iter().collect();
            let outfit = orchestrator.outfit(view, &self.profile, band, window, &borrowed);
            self.board.plan(tick, band.band_id, &outfit.grants, sink);
            if outfit.kits.is_empty() && outfit.materials.is_empty() {
                continue;
            }
            let command = CommandPayload::SetStartingLoadout {
                faction_id: self.faction,
                band_id: band.band_id,
                kits: outfit
                    .kits
                    .iter()
                    .map(|(kit_id, count)| StartingKitAllocation {
                        kit_id: kit_id.clone(),
                        count: *count,
                    })
                    .collect(),
                materials: outfit
                    .materials
                    .iter()
                    .map(|(material_id, units)| StartingMaterialAllocation {
                        material_id: material_id.clone(),
                        units: *units,
                    })
                    .collect(),
            };
            let sent: Vec<String> = outfit
                .kits
                .iter()
                .map(|(kit, count)| format!("{kit} {count}"))
                .chain(
                    outfit
                        .materials
                        .iter()
                        .map(|(material, units)| format!("{material} {units}")),
                )
                .collect();
            let asked: Vec<String> = roster
                .iter()
                .map(|specialist| {
                    let kits: u32 = entries
                        .iter()
                        .filter(|entry| entry.demand.requester == *specialist)
                        .filter(|entry| matches!(entry.demand.resource, Resource::Kit(_)))
                        .map(|entry| entry.demand.amount)
                        .sum();
                    format!("{specialist} {kits} asked")
                })
                .collect();
            sink.record(DecisionRecord::Decision(Decision {
                tick,
                specialist: ORCHESTRATOR_ID.to_owned(),
                intent: intent_key(ORCHESTRATOR_ID, INTENT_OUTFIT, band.band_id),
                score_raw: OUTFIT_SCORE,
                score_final: OUTFIT_SCORE,
                outcome: Outcome::Accepted,
                reason: format!("outfit: {} [{}]", sent.join(", "), asked.join(", ")),
                commands: 1,
                commands_text: vec![render_command_line(&command)],
            }));
            commands.push(command);
        }
        commands
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
                    goals: plan.goals_record(),
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
        // The board settles on the frame before anything is asked of it: a grant the last frame
        // carried is fulfilled, one it did not is expired.
        self.board.settle(tick, view, sink);
        let plan = self.plan_for(view, sink);

        let mut offered = Vec::new();
        let mut demands = Vec::new();
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
            demands.extend(proposals.demands);
            offered.extend(proposals.proposals.into_iter().map(|proposal| Offered {
                specialist: id,
                proposal,
            }));
        }
        self.board.post(tick, demands, sink);
        // The loadouts go first, and they do not pass the arbiter: they spend no worker budget
        // and no band order (`arbiter.rs`).
        let loadouts = self.outfit_windows(view, tick, sink);

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
        self.memory.record_choices(
            tick,
            accepted
                .iter()
                .map(|accepted| (accepted.intent.clone(), accepted.memo)),
        );
        let commands: Vec<CommandPayload> = loadouts
            .into_iter()
            .chain(accepted.into_iter().flat_map(|accepted| accepted.commands))
            .collect();
        self.memory.remember_runways(view, self.faction);
        commands
    }

    fn lens(&self) -> BrainLens<'_> {
        BrainLens {
            plan: self.plan.as_ref(),
            alarms: self.memory.pending_alarms(),
            memory: Some(&self.memory),
            horizon_tiles: self.profile.land.horizon_tiles,
        }
    }

    fn on_full_frame(&mut self, tick: u64) {
        self.memory.forget_after(tick);
        self.board.forget_after(tick);
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

    /// A band whose outfitting window is open: the specialists post, the orchestrator resolves,
    /// the loadout is the first command and is recorded under `orchestrator:outfit:<band>`, and
    /// the board records the round trip — posted and planned on the tick, fulfilled on the next
    /// frame that carries the kit.
    #[test]
    fn an_open_window_is_outfitted_first_from_the_specialists_demands_and_the_board_records_it() {
        use crate::instruments::decisions::DemandRecord;
        use sim_runtime::BandLoadoutWindowState;
        let profiles = AiProfiles::builtin();
        let mut brain = UtilityBrain::build(FACTION, &profiles, "forager", "hard", &[]).unwrap();
        let mut view = a_view();
        let working_age = view.snapshot.populations[0].working_age;
        view.snapshot.populations[0].loadout_window = Some(BandLoadoutWindowState {
            open: true,
            kit_budget: working_age,
            material_budget: 0,
            ..Default::default()
        });
        let mut sink = VecSink::default();
        let commands = brain.decide(&view, &mut rng(), &mut sink);
        let CommandPayload::SetStartingLoadout { band_id, kits, .. } = &commands[0] else {
            panic!("the loadout goes first: {:?}", commands[0]);
        };
        assert_eq!(*band_id, 7001);
        assert_eq!(
            kits.iter().map(|kit| kit.count).sum::<u32>(),
            working_age,
            "one kit per hand: {kits:?}"
        );
        let outfit = decisions(VecSink(sink.0.clone()))
            .into_iter()
            .find(|d| d.intent == "orchestrator:outfit:7001")
            .expect("the loadout is recorded");
        assert_eq!(outfit.specialist, "orchestrator");
        assert_eq!(outfit.outcome, Outcome::Accepted);
        assert!(
            outfit.commands_text[0].starts_with("set_starting_loadout 3 7001 kit "),
            "{}",
            outfit.commands_text[0]
        );
        assert!(outfit.reason.starts_with("outfit: "), "{}", outfit.reason);
        let states = |sink: &VecSink| -> Vec<(String, String)> {
            sink.0
                .iter()
                .filter_map(|record| match record {
                    DecisionRecord::Demand(DemandRecord {
                        resource, state, ..
                    }) => Some((resource.clone(), state.clone())),
                    _ => None,
                })
                .collect()
        };
        let recorded = states(&sink);
        assert!(
            recorded.contains(&("kit:gathering".to_owned(), "posted".to_owned()))
                && recorded.contains(&("kit:gathering".to_owned(), "planned".to_owned())),
            "{recorded:?}"
        );
        // The next frame carries the baskets: the demand is fulfilled.
        let mut next = a_view();
        next.snapshot.header.tick += 1;
        for tier in &mut next.snapshot.populations[0].kit_tiers {
            if tier.kit_id == "gathering" {
                tier.forage_carry_per_worker_biomass = 8.0;
            }
        }
        let mut sink = VecSink::default();
        brain.decide(&next, &mut rng(), &mut sink);
        assert!(
            states(&sink).contains(&("kit:gathering".to_owned(), "fulfilled".to_owned())),
            "{:?}",
            states(&sink)
        );
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
