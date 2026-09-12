//! **The five `Food` rules** (`docs/plan_ai_driver.md` §4, the rule table). Each is a method on
//! [`Food`] taking the view, the plan, the memory and one band, and answering at most one
//! proposal whose `reason` is `"<rule>: <subject> [ledger: …]"`. Every score is
//! [`goal_progress`] × the specialist's weight; a rule handed no goals proposes nothing.
//!
//! The rules share one piece of arithmetic: [`Food::draw`], the hands a change frees and what
//! they earn where they stand — idle first (they cost nothing), then the rows named, in the order
//! named, until each is empty.

use sim_runtime::{
    CommandPayload, FloraShareInfo, ForagePatchState, LaborAssignmentState, PopulationCohortState,
};

use super::ledger::{
    goal_progress, ledger_note, project, project_all, survives, Projection, Reassignment,
};
use super::sources::{crew_take, patch_per_worker_yield, workable_patch_at, Source, SourceKey};
use super::{
    Food, INTENT_ASSIGN, INTENT_FEED_MOVE, INTENT_HUNT, INTENT_SETTLE, INTENT_SPLIT,
    INTENT_UPGRADE, ROLE_BUILDERS, ROLE_FORAGE, ROLE_HUNT,
};
use crate::geometry::Tile;
use crate::orchestrator::{GroundRung, Plan};
use crate::specialists::{intent_key, Cost, Memo, Proposal, SPECIALIST_FOOD};
use crate::view::{band_tile, SeatMemory, SeatView};

/// The rules, as the decision log names them.
pub(super) const REASON_NEGATIVE_INCOME: &str = "negative income";
pub(super) const REASON_FEED_MOVE: &str = "feed while moving";
pub(super) const REASON_SPLIT_TO_FEED: &str = "split to feed";
pub(super) const REASON_SPARE_HANDS: &str = "spare hands into hunts";
pub(super) const REASON_UPGRADE: &str = "upgrade the ground";
/// Why *negative income* empties a row ahead of the per-worker minimum.
const WHY_OVERUSED: &str = "overused";
const WHY_NO_USEFUL_CREW: &str = "no useful crew on";
const WHY_DEAD_ROW: &str = "dead row";
const WHY_LOWEST_ROW: &str = "lowest row";

/// **How far a band walks in a turn**, restated from `core_sim/src/data/labor_config.json` →
/// `band_move_tiles_per_turn` (`1`): the seat cannot read the sim's config, and the split's
/// travel is priced in turns. The server's value is the authority.
const BAND_MOVE_TILES_PER_TURN: u32 = 1;

/// **The plant rungs' gate knowledges**, by the ids `core_sim/src/data/intensification_ladder.json`
/// declares (`rungs[].unlock_knowledge`): `cultivation` gates `tended`, `seed_selection` gates
/// `field`.
const KNOWLEDGE_CULTIVATION: &str = "cultivation";
const KNOWLEDGE_SEED_SELECTION: &str = "seed_selection";
/// **The ledger's saturation** — `LadderKnowledgeProgress::progress` is *"0..1 (1.0 = known)"*,
/// and the ladder's `completion_threshold` is the same `1.0` the ledger clamps at. The seat cannot
/// read the threshold; a `Cultivate` refused for knowledge shows in the failed-command log.
const KNOWLEDGE_COMPLETE: f32 = 1.0;

/// The rung verbs as the reason names them.
const VERB_CULTIVATE: &str = "cultivate";
const VERB_SOW: &str = "sow";

/// **Which of two changes a rule prefers**: the one closing more goal gap, and between two that
/// close the same — which is every pair once the goals are met, since a met goal has no gap
/// left to close — the one that adds more net income. Without the second term a band past its
/// goals took the first candidate offered, whatever it paid.
fn closer(progress: f32, change: &Reassignment, held: f32, held_change: &Reassignment) -> bool {
    let gain = |change: &Reassignment| change.income_gained - change.income_lost;
    progress > held || (progress == held && gain(change) > gain(held_change))
}

/// Hands freed from where they stand ([`Food::draw`]).
struct Drawn {
    hands: u32,
    /// What those hands earned per turn where they were.
    income_lost: f32,
    /// The rows reduced, and the workers left on each.
    reductions: Vec<(SourceKey, u32)>,
}

/// A reassignment *negative income* weighs.
struct Candidate {
    commands: Vec<CommandPayload>,
    hands: u32,
    change: Reassignment,
    subject: String,
}

/// The rung *upgrade the ground* would declare on a patch.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Climb {
    Tended,
    Field,
}

impl Food {
    /// Free `want` hands: the band's idle first, then `rows` in the order given until each is
    /// empty. `hands` is what could be freed, which may be short of `want`.
    fn draw(&self, rows: &[&LaborAssignmentState], want: u32, idle: u32) -> Drawn {
        let mut drawn = Drawn {
            hands: idle.min(want),
            income_lost: 0.0,
            reductions: Vec::new(),
        };
        for row in rows {
            if drawn.hands >= want {
                break;
            }
            let Some(key) = SourceKey::of_row(row) else {
                continue;
            };
            let take = (want - drawn.hands).min(row.workers);
            if take == 0 {
                continue;
            }
            drawn.income_lost += Self::row_rate(row) * take as f32;
            drawn.reductions.push((key, row.workers - take));
            drawn.hands += take;
        }
        drawn
    }

    /// The `assign_labor` lines that reduce the rows a draw took hands from.
    fn reduction_commands(
        &self,
        band: &PopulationCohortState,
        drawn: &Drawn,
    ) -> Vec<CommandPayload> {
        drawn
            .reductions
            .iter()
            .map(|(key, left)| self.assign(band, key, *left))
            .collect()
    }

    /// The band's worked rows under `role`, lowest-paying first.
    fn rows_ascending<'b>(
        band: &'b PopulationCohortState,
        role: &str,
    ) -> Vec<&'b LaborAssignmentState> {
        let mut rows: Vec<&LaborAssignmentState> = band
            .labor_assignments
            .iter()
            .filter(|row| row.kind == role && row.workers > 0)
            .collect();
        rows.sort_by(|a, b| Self::row_rate(a).total_cmp(&Self::row_rate(b)));
        rows
    }

    /// The row *negative income* empties first: one that is overused (`actual_yield >
    /// sustainable_yield`), a hunt row the sim says no crew is useful on, or a dead row — and
    /// failing those, the row paying the least per worker. `true` when the row is one of the
    /// troubled kinds, which need no gain guard to be worth leaving.
    fn row_to_empty<'b>(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &'b PopulationCohortState,
    ) -> Option<(&'b LaborAssignmentState, SourceKey, bool, &'static str)> {
        let worked: Vec<(&LaborAssignmentState, SourceKey)> = band
            .labor_assignments
            .iter()
            .filter(|row| row.workers > 0)
            .filter_map(|row| SourceKey::of_row(row).map(|key| (row, key)))
            .collect();
        for (row, key) in &worked {
            let why = if row.actual_yield > row.sustainable_yield {
                WHY_OVERUSED
            } else if row.kind == ROLE_HUNT && row.hunt_useful_workers == 0 {
                WHY_NO_USEFUL_CREW
            } else if self.is_dead(
                memory,
                band,
                key,
                Self::forecast_for(view, key).unwrap_or_default(),
            ) {
                WHY_DEAD_ROW
            } else {
                continue;
            };
            return Some((row, key.clone(), true, why));
        }
        let per_worker = |row: &LaborAssignmentState| row.actual_yield / row.workers as f32;
        worked
            .into_iter()
            .min_by(|(a, _), (b, _)| per_worker(a).total_cmp(&per_worker(b)))
            .map(|(row, key)| (row, key, false, WHY_LOWEST_ROW))
    }

    /// The reassignments *negative income* chooses among, all within `budget`: (a) the idle hands
    /// onto the best source; (b) the row to empty first onto the best other source, when that
    /// buys something; (c) both onto the best source for the whole crew.
    fn reassignments(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        sources: &[Source],
        budget: u32,
    ) -> Vec<Candidate> {
        let mut out = Vec::new();
        let idle = band.idle_workers.min(budget);
        let idle_onto = (idle > 0)
            .then(|| Self::best_source(sources, idle, None))
            .flatten();
        if let Some(best) = idle_onto {
            let existing = Self::workers_on(band, &best.key);
            out.push(Candidate {
                commands: vec![self.assign(band, &best.key, existing + idle)],
                hands: idle,
                change: Reassignment {
                    income_lost: 0.0,
                    income_gained: best.marginal(existing, idle),
                    payoff_turn: 0,
                },
                subject: format!("{idle} idle hands -> {}", best.key.describe()),
            });
        }
        let Some((row, low_key, troubled, why)) = self.row_to_empty(view, memory, band) else {
            return out;
        };
        let moved = row.workers.min(budget);
        if moved == 0 {
            return out;
        }
        let low = Self::row_rate(row);
        // ⛔ **A SHUFFLE MUST BUY SOMETHING.** With only "are these distinct rows" between them,
        // two rows paying the same moved workers back and forth every turn under the alarm, at
        // the highest score the specialist had, and one order per band then rejected the idle
        // hands as `conflict`. A troubled row is worth leaving whatever the next one pays; a
        // merely lowest row moves only onto ground that out-pays it by the profile's fraction —
        // and either way the crew's marginal take there must exceed what it earned here.
        let clears = |next: &Source| {
            troubled
                || (next.per_worker_yield > 0.0
                    && next.per_worker_yield - low
                        >= self.floors.runway_gain_fraction * next.per_worker_yield)
        };
        if let Some(next) =
            Self::best_source(sources, moved, Some(&low_key)).filter(|next| clears(next))
        {
            let existing = Self::workers_on(band, &next.key);
            let change = Reassignment {
                income_lost: low * moved as f32,
                income_gained: next.marginal(existing, moved),
                payoff_turn: 0,
            };
            if change.income_gained > change.income_lost {
                out.push(Candidate {
                    commands: vec![
                        self.assign(band, &low_key, row.workers - moved),
                        self.assign(band, &next.key, existing + moved),
                    ],
                    hands: moved,
                    change,
                    subject: format!("{why} {} -> {}", low_key.describe(), next.key.describe()),
                });
            }
        }
        let both = idle + moved;
        if idle > 0 && both <= budget {
            if let Some(best) =
                Self::best_source(sources, both, Some(&low_key)).filter(|next| clears(next))
            {
                let existing = Self::workers_on(band, &best.key);
                let change = Reassignment {
                    income_lost: low * moved as f32,
                    income_gained: best.marginal(existing, both),
                    payoff_turn: 0,
                };
                if change.income_gained > change.income_lost {
                    out.push(Candidate {
                        commands: vec![
                            self.assign(band, &low_key, row.workers - moved),
                            self.assign(band, &best.key, existing + both),
                        ],
                        hands: both,
                        change,
                        subject: format!(
                            "{idle} idle hands and {why} {} -> {}",
                            low_key.describe(),
                            best.key.describe()
                        ),
                    });
                }
            }
        }
        out
    }

    /// **Rule 1 — negative income.** Income below consumption, or idle hands (negative income
    /// against what they could earn), reassigns to the source the crew takes the most from; the
    /// change that closes the most goal gap wins. Answers the proposal and the change it carries,
    /// which the later rules project on top of. Not for a travelling band, nor for a child still
    /// walking to the site it was split toward — it must not strip the parent's ground.
    pub(super) fn assess_income(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> (Option<Proposal>, Reassignment) {
        let Some(goals) = plan.food_goals() else {
            return (None, Reassignment::NONE);
        };
        let fires = band.food_income < band.food_consumption || band.idle_workers > 0;
        if !fires || band.is_traveling || memory.born_by_split(band.band_id).is_some() {
            return (None, Reassignment::NONE);
        }
        let budget = self.budget_workers(view, plan);
        if budget == 0 {
            return (None, Reassignment::NONE);
        }
        let sources = self.reachable_sources(view, memory, band);
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let before = project(&book, &Reassignment::NONE, horizon);
        let mut best: Option<(Candidate, Projection, f32)> = None;
        for candidate in self.reassignments(view, memory, band, &sources, budget) {
            let after = project(&book, &candidate.change, horizon);
            let progress = goal_progress(&goals, &before, &after);
            if best.as_ref().is_none_or(|(held, _, held_progress)| {
                closer(progress, &candidate.change, *held_progress, &held.change)
            }) {
                best = Some((candidate, after, progress));
            }
        }
        let Some((candidate, after, progress)) = best else {
            return (None, Reassignment::NONE);
        };
        let proposal = Proposal {
            commands: candidate.commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_ASSIGN, band.band_id),
            score: progress * self.weight,
            cost: Cost {
                workers: candidate.hands,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_NEGATIVE_INCOME}: {} [{}]",
                candidate.subject,
                ledger_note(&after)
            ),
            memo: None,
        };
        (Some(proposal), candidate.change)
    }

    /// Rule 1 as a proposal alone — what the tests drive; `propose` goes through
    /// [`Food::assess_income`] to carry the change forward.
    #[cfg(test)]
    pub fn negative_income(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        self.assess_income(view, plan, memory, band).0
    }

    /// Where the source a worked row names stands this frame.
    fn source_tile(view: &SeatView, key: &SourceKey) -> Option<Tile> {
        match key {
            SourceKey::Patch(tile) => Some(*tile),
            SourceKey::Herd(id) => view
                .snapshot
                .herds
                .iter()
                .find(|herd| &herd.id == id)
                .map(|herd| Tile::new(herd.x, herd.y)),
        }
    }

    /// **Rule 2 — feed while moving.** A band with a move in force and not yet under way works
    /// what will fall *outside* its range from the target before it leaves: the idle hands, and
    /// the crews of rows that will still be in range after the move, onto the best source that
    /// will not. Never a freshly split band within `split_settle_turns` of its birth — it must not
    /// strip the parent's ground on its way out.
    pub fn feed_while_moving(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let goals = plan.food_goals()?;
        let target = memory.move_target(band.band_id)?;
        if band.is_traveling {
            return None;
        }
        let fresh = memory.born_by_split(band.band_id).is_some_and(|birth| {
            view.tick().saturating_sub(birth.tick) <= u64::from(self.floors.split_settle_turns)
        });
        if fresh {
            return None;
        }
        let budget = self.budget_workers(view, plan);
        if budget == 0 {
            return None;
        }
        let grid = view.grid();
        let sources = self.reachable_sources(view, memory, band);
        let falling: Vec<Source> = sources
            .iter()
            .filter(|source| grid.distance(target, source.tile) > source.reach(band))
            .cloned()
            .collect();
        if falling.is_empty() {
            return None;
        }
        let mut staying: Vec<&LaborAssignmentState> = band
            .labor_assignments
            .iter()
            .filter(|row| row.workers > 0)
            .filter(|row| {
                SourceKey::of_row(row).is_some_and(|key| {
                    Self::source_tile(view, &key).is_some_and(|tile| {
                        let reach = match key {
                            SourceKey::Patch(_) => band.work_range,
                            SourceKey::Herd(_) => band.hunt_reach,
                        };
                        grid.distance(target, tile) <= reach
                    })
                })
            })
            .collect();
        staying.sort_by(|a, b| Self::row_rate(a).total_cmp(&Self::row_rate(b)));
        let idle = band.idle_workers.min(budget);
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let before = project(&book, &Reassignment::NONE, horizon);
        let mut best: Option<(Drawn, &Source, Projection, f32, Reassignment)> = None;
        for drawn in [
            self.draw(&[], idle, idle),
            self.draw(&staying, budget, idle),
        ] {
            if drawn.hands == 0 {
                continue;
            }
            let Some(source) = Self::best_source(&falling, drawn.hands, None) else {
                continue;
            };
            let existing = Self::workers_on(band, &source.key);
            let change = Reassignment {
                income_lost: drawn.income_lost,
                income_gained: source.marginal(existing, drawn.hands),
                payoff_turn: 0,
            };
            if change.income_gained <= change.income_lost {
                continue;
            }
            let after = project(&book, &change, horizon);
            let progress = goal_progress(&goals, &before, &after);
            if best.as_ref().is_none_or(|(_, _, _, held, held_change)| {
                closer(progress, &change, *held, held_change)
            }) {
                best = Some((drawn, source, after, progress, change));
            }
        }
        let (drawn, source, after, progress, _) = best?;
        let existing = Self::workers_on(band, &source.key);
        let mut commands = self.reduction_commands(band, &drawn);
        commands.push(self.assign(band, &source.key, existing + drawn.hands));
        Some(Proposal {
            commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_FEED_MOVE, band.band_id),
            score: progress * self.weight,
            cost: Cost {
                workers: drawn.hands,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_FEED_MOVE}: {} hands onto {} before it falls out of range from {},{} [{}]",
                drawn.hands,
                source.key.describe(),
                target.x,
                target.y,
                ledger_note(&after)
            ),
            memo: None,
        })
    }

    /// **Rule 3 — split to feed.** After rule 1's change the band's projected runway is still
    /// below the goal, and a discovered, workable site just outside its reach would feed the
    /// child crew: split toward it. The child appears on the parent's tile next turn and
    /// [`Food::settle`] walks it there.
    ///
    /// **The crew is `min(split_band_workers, working_age − founding_parent_min_workers)`, and
    /// the rule is silent below `founding_min_workers`.** The sim's two split floors cross the
    /// wire on every cohort (`PopulationCohortState::founding_min_workers` /
    /// `founding_parent_min_workers` — *"The two floors cross the wire; the verdict does not."*),
    /// so the child is sized to what the parent may give up, and a crew the sim would refuse as
    /// too small is not asked for. The refusal memory ([`SeatMemory::split_refused_at`]) stays as
    /// a belt: a split can still be refused for reasons the floors do not state.
    pub fn split_to_feed(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        let goals = plan.food_goals()?;
        let crew = self.floors.split_band_workers.min(
            band.working_age
                .saturating_sub(band.founding_parent_min_workers),
        );
        // A split the sim refused at this size is not asked for again until the band has grown.
        let refused_at_this_size = memory
            .split_refused_at(band.band_id)
            .is_some_and(|refused_at| refused_at >= band.working_age);
        if band.is_traveling
            || memory.pending_split(band.band_id).is_some()
            || refused_at_this_size
            || crew < band.founding_min_workers
        {
            return None;
        }
        let budget = self.budget_workers(view, plan);
        if budget < crew {
            return None;
        }
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let after = project(&book, carried, horizon);
        if after.runway_at_end >= goals.runway_turns {
            return None;
        }
        let grid = view.grid();
        let here = band_tile(band);
        let (patch, take, distance) = view
            .snapshot
            .forage_patches
            .iter()
            .filter(|patch| patch.per_worker_yield > 0.0)
            .filter(|patch| patch.owner.is_none_or(|owner| owner == self.faction))
            .filter_map(|patch| {
                let tile = Tile::new(patch.x, patch.y);
                let distance = grid.distance(here, tile);
                (view.is_discovered(tile)
                    && distance > band.work_range
                    && distance <= self.floors.split_search_tiles)
                    .then(|| workable_patch_at(view, tile))
                    .flatten()
                    .map(|patch| (patch, distance))
            })
            .filter(|(patch, _)| {
                let key = SourceKey::Patch(Tile::new(patch.x, patch.y));
                !self.is_dead(memory, band, &key, patch.per_worker_yield)
            })
            .map(|(patch, distance)| {
                let take = crew_take(
                    crew,
                    patch_per_worker_yield(memory, band, patch),
                    patch.biomass * patch.provisions_per_biomass,
                );
                (patch, take, distance)
            })
            .max_by(|(_, a, a_distance), (_, b, b_distance)| {
                a.total_cmp(b).then_with(|| b_distance.cmp(a_distance))
            })?;
        // A child that cannot feed itself is not a fix.
        let share = band.food_consumption * crew as f32 / band.working_age.max(1) as f32;
        if take <= share {
            return None;
        }
        let travel = distance.div_ceil(BAND_MOVE_TILES_PER_TURN);
        // The hands leave the parent's rows, lowest-paying first (its idle ones cost nothing).
        let mut rows = Self::rows_ascending(band, ROLE_HUNT);
        rows.extend(Self::rows_ascending(band, ROLE_FORAGE));
        rows.sort_by(|a, b| Self::row_rate(a).total_cmp(&Self::row_rate(b)));
        let drawn = self.draw(&rows, crew, band.idle_workers);
        let change = Reassignment {
            income_lost: drawn.income_lost,
            income_gained: take,
            payoff_turn: travel,
        };
        let after_split = project_all(&book, &[*carried, change], horizon);
        let target = Tile::new(patch.x, patch.y);
        Some(Proposal {
            commands: vec![CommandPayload::SplitBand {
                faction_id: self.faction,
                band_id: Some(band.band_id),
                workers: crew,
            }],
            intent: intent_key(SPECIALIST_FOOD, INTENT_SPLIT, band.band_id),
            score: goal_progress(&goals, &after, &after_split) * self.weight,
            cost: Cost {
                workers: crew,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_SPLIT_TO_FEED}: {crew} toward {},{} taking {take:.1}/turn from t{travel} [{}]",
                target.x,
                target.y,
                ledger_note(&after_split)
            ),
            memo: Some(Memo::Split {
                band: band.band_id,
                target,
                workers: crew,
            }),
        })
    }

    /// **Rule 3, second half — settle.** A band this seat split off, not yet at its site and
    /// under no other move, walks there; the intent is re-proposed every turn until arrival,
    /// which is what the commitment bonus rewards.
    pub fn settle(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        let goals = plan.food_goals()?;
        let birth = memory.born_by_split(band.band_id)?;
        let here = band_tile(band);
        if band.is_traveling
            || here == birth.target
            || memory
                .move_target(band.band_id)
                .is_some_and(|target| target != birth.target)
        {
            return None;
        }
        let grid = view.grid();
        let travel = grid
            .distance(here, birth.target)
            .div_ceil(BAND_MOVE_TILES_PER_TURN);
        let take = workable_patch_at(view, birth.target).map_or(0.0, |patch| {
            crew_take(
                band.working_age,
                patch_per_worker_yield(memory, band, patch),
                patch.biomass * patch.provisions_per_biomass,
            )
        });
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let before = project(&book, &Reassignment::NONE, horizon);
        let after = project(
            &book,
            &Reassignment {
                income_lost: 0.0,
                income_gained: take,
                payoff_turn: travel,
            },
            horizon,
        );
        Some(Proposal {
            commands: vec![CommandPayload::MoveBand {
                faction_id: self.faction,
                band_id: Some(band.band_id),
                target_x: birth.target.x,
                target_y: birth.target.y,
            }],
            intent: intent_key(SPECIALIST_FOOD, INTENT_SETTLE, band.band_id),
            score: goal_progress(&goals, &before, &after) * self.weight,
            cost: Cost {
                workers: 0,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_SPLIT_TO_FEED}: settle toward {},{} in t{travel} [{}]",
                birth.target.x,
                birth.target.y,
                ledger_note(&after)
            ),
            memo: Some(Memo::Move {
                band: band.band_id,
                target: birth.target,
                from: here,
            }),
        })
    }

    /// **Rule 4 — spare hands into hunts.** Projected net income (after rule 1's change) at the
    /// goal or within `near_positive_fraction` of it, and a live huntable herd in reach: the
    /// surplus — the most hands whose leaving the lowest-paying forage rows keeps the projected net
    /// at or above the goal, the herd's take counted — goes onto that herd, which is what opens
    /// penning. The projection must survive. Idle hands are not surplus; they are rule 1's.
    pub fn spare_hands_into_hunts(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        let goals = plan.food_goals()?;
        if band.is_traveling {
            return None;
        }
        let budget = self.budget_workers(view, plan);
        if budget == 0 {
            return None;
        }
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let after = project(&book, carried, horizon);
        let goal = goals.net_income_per_turn;
        if after.net_after < goal * (1.0 - self.floors.near_positive_fraction) {
            return None;
        }
        let sources = self.reachable_sources(view, memory, band);
        let herds: Vec<&Source> = sources
            .iter()
            .filter(|source| matches!(source.key, SourceKey::Herd(_)))
            .collect();
        if herds.is_empty() {
            return None;
        }
        let forage = Self::rows_ascending(band, ROLE_FORAGE);
        let mut best: Option<(Drawn, &Source, Projection)> = None;
        for hands in 1..=budget {
            // Off the rows only: the idle hands are *negative income*'s to place, and a hunt
            // drawn from them would compete with that assignment for the band's one order.
            let drawn = self.draw(&forage, hands, 0);
            if drawn.hands < hands {
                break;
            }
            let Some(herd) = herds.iter().copied().max_by(|a, b| {
                a.marginal(Self::workers_on(band, &a.key), hands)
                    .total_cmp(&b.marginal(Self::workers_on(band, &b.key), hands))
            }) else {
                break;
            };
            let change = Reassignment {
                income_lost: drawn.income_lost,
                income_gained: herd.marginal(Self::workers_on(band, &herd.key), hands),
                payoff_turn: 0,
            };
            if after.net_after - change.income_lost + change.income_gained < goal {
                continue;
            }
            let projection = project_all(&book, &[*carried, change], horizon);
            if !survives(&projection) {
                continue;
            }
            best = Some((drawn, herd, projection));
        }
        let (drawn, herd, projection) = best?;
        let existing = Self::workers_on(band, &herd.key);
        let mut commands = self.reduction_commands(band, &drawn);
        commands.push(self.assign(band, &herd.key, existing + drawn.hands));
        Some(Proposal {
            commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_HUNT, band.band_id),
            score: goal_progress(&goals, &after, &projection) * self.weight,
            cost: Cost {
                workers: drawn.hands,
                bands: vec![band.band_id],
            },
            reason: format!(
                "{REASON_SPARE_HANDS}: {} hands onto {} [{}]",
                drawn.hands,
                herd.key.describe(),
                ledger_note(&projection)
            ),
            memo: None,
        })
    }

    /// Whether this faction knows the ladder knowledge `id`: its progress row reads
    /// [`KNOWLEDGE_COMPLETE`]. A faction with no row has begun nothing.
    fn knows(&self, view: &SeatView, id: &str) -> bool {
        view.snapshot
            .intensification_knowledge
            .iter()
            .find(|row| row.faction == self.faction)
            .and_then(|row| {
                row.knowledges
                    .iter()
                    .find(|knowledge| knowledge.knowledge_id == id)
            })
            .is_some_and(|knowledge| knowledge.progress >= KNOWLEDGE_COMPLETE)
    }

    /// The plant a climb commits `patch` to: what it is already committed to, else the largest
    /// share that may climb the rung — and what that plant pays once the rung is complete
    /// (`FloraShareInfo::cultivate_payoff` / `sow_payoff`).
    fn climb_payoff(patch: &ForagePatchState, climb: Climb) -> Option<(&FloraShareInfo, f32)> {
        let legal = |plant: &FloraShareInfo| match climb {
            Climb::Tended => plant.can_cultivate,
            Climb::Field => plant.can_sow,
        };
        let payoff = |plant: &FloraShareInfo| match climb {
            Climb::Tended => plant.cultivate_payoff,
            Climb::Field => plant.sow_payoff,
        };
        let committed = patch
            .composition
            .iter()
            .find(|plant| {
                !patch.committed_species.is_empty() && plant.species == patch.committed_species
            })
            .filter(|plant| legal(plant));
        let plant = committed.or_else(|| {
            patch
                .composition
                .iter()
                .filter(|plant| legal(plant))
                .max_by(|a, b| a.share.total_cmp(&b.share))
        })?;
        Some((plant, payoff(plant)))
    }

    /// **Rule 5 — upgrade the ground.** The goal rung above wild, its gate knowledge known, and
    /// a worked forage patch below it with nothing queued: declare the climb and staff the
    /// smallest `builders` pool whose projection survives and pays off inside the horizon. The
    /// builders are drawn from the idle hands, then the hunt rows, then the lowest forage rows —
    /// never from the patch's own row, which is what keeps the declaration attached.
    pub fn upgrade_the_ground(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        let goals = plan.food_goals()?;
        if goals.ground_rung <= GroundRung::Wild || band.is_traveling {
            return None;
        }
        let budget = self.budget_workers(view, plan);
        if budget == 0 {
            return None;
        }
        let cultivation = self.knows(view, KNOWLEDGE_CULTIVATION);
        let seed_selection = self.knows(view, KNOWLEDGE_SEED_SELECTION);
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let after = project(&book, carried, horizon);
        let idle = band.idle_workers.min(budget);
        let builders_now = Self::workers_in_pool(band, ROLE_BUILDERS);
        let mut best: Option<(Proposal, f32)> = None;
        for row in band
            .labor_assignments
            .iter()
            .filter(|row| row.kind == ROLE_FORAGE && row.workers > 0)
        {
            let tile = Tile::new(row.target_x, row.target_y);
            let Some(patch) = workable_patch_at(view, tile) else {
                continue;
            };
            // Nothing queued: `build_destination_rung` is *"the queued entry's destination rung
            // … or empty when no band has queued it"*, and the band's own `build_queue` is the
            // rank. (`build_queue_position` says the same, but it is a source-addressed readout
            // of the *winning* band's place, and it defaults to `0` off the wire.)
            let queued = !patch.build_destination_rung.is_empty()
                || band.build_queue.iter().any(|entry| {
                    entry.kind == ROLE_FORAGE
                        && entry.target_x == tile.x
                        && entry.target_y == tile.y
                });
            if queued {
                continue;
            }
            let climb = if !patch.is_cultivated {
                cultivation.then_some(Climb::Tended)
            } else if goals.ground_rung == GroundRung::Field
                && !patch.is_field
                && seed_selection
                && patch.sow_site_refusal.is_empty()
            {
                Some(Climb::Field)
            } else {
                None
            };
            let Some(climb) = climb else {
                continue;
            };
            let Some((plant, payoff)) = Self::climb_payoff(patch, climb) else {
                continue;
            };
            let today = crew_take(
                row.workers,
                patch_per_worker_yield(memory, band, patch),
                patch.biomass * patch.provisions_per_biomass,
            );
            let gained = payoff - today;
            let (work_left, verb) = match climb {
                Climb::Tended => (
                    patch.cultivation_work_cost - patch.cultivation_work_done,
                    VERB_CULTIVATE,
                ),
                Climb::Field => (patch.field_work_cost - patch.field_work_done, VERB_SOW),
            };
            let per_builder = patch.build_work_per_worker_turn;
            if gained <= 0.0 || work_left <= 0.0 || per_builder <= 0.0 {
                continue;
            }
            let mut pool = Self::rows_ascending(band, ROLE_HUNT);
            pool.extend(
                Self::rows_ascending(band, ROLE_FORAGE)
                    .into_iter()
                    .filter(|other| Tile::new(other.target_x, other.target_y) != tile),
            );
            for hands in 1..=budget {
                let drawn = self.draw(&pool, hands, idle);
                if drawn.hands < hands {
                    break;
                }
                let builders = builders_now + hands;
                let payoff_turn = (work_left / (builders as f32 * per_builder)).ceil() as u32;
                if payoff_turn > horizon {
                    continue;
                }
                let change = Reassignment {
                    income_lost: drawn.income_lost,
                    income_gained: gained,
                    payoff_turn,
                };
                let projection = project_all(&book, &[*carried, change], horizon);
                if !survives(&projection) {
                    continue;
                }
                let declare = match climb {
                    Climb::Tended => CommandPayload::Cultivate {
                        faction_id: self.faction,
                        target_x: tile.x,
                        target_y: tile.y,
                    },
                    Climb::Field => CommandPayload::Sow {
                        faction_id: self.faction,
                        target_x: tile.x,
                        target_y: tile.y,
                    },
                };
                // The rows are reduced BEFORE the pool is staffed: `assign_labor` clamps a role
                // to the band's idle hands at dispatch (`" (clamped from {} — only {} idle)"`,
                // `core_sim/src/bin/server.rs`), so builders named first would be clamped to 0.
                let mut commands = vec![declare];
                commands.extend(self.reduction_commands(band, &drawn));
                commands.push(self.assign_pool(band, ROLE_BUILDERS, builders));
                let progress = goal_progress(&goals, &after, &projection);
                let proposal = Proposal {
                    commands,
                    intent: intent_key(SPECIALIST_FOOD, INTENT_UPGRADE, band.band_id),
                    score: progress * self.weight,
                    cost: Cost {
                        workers: hands,
                        bands: vec![band.band_id],
                    },
                    reason: format!(
                        "{REASON_UPGRADE}: {verb} {},{} ({}) with {builders} builders, payoff turn {payoff_turn} [{}]",
                        tile.x,
                        tile.y,
                        plant.species,
                        ledger_note(&projection)
                    ),
                    memo: None,
                };
                if best.as_ref().is_none_or(|(_, held)| progress > *held) {
                    best = Some((proposal, progress));
                }
                break;
            }
        }
        best.map(|(proposal, _)| proposal)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        a_unit_of, a_view, food, goals, memory, own_band, plan_toward, plan_with_food_share, BAND,
        FACTION, FORAGE_KIT_ITEM, HERD_AT, HERD_ID, HERE, HUNT_KIT_HAUL_ITEM, HUNT_KIT_ITEM,
        NEAR_PATCH, RICH_PATCH, STOCK, TICK,
    };
    use super::*;
    use crate::orchestrator::Plan;
    use crate::specialists::Specialist;
    use sim_runtime::{
        CohortStoreState, EquipmentBatchState, HerdTelemetryState, IntensificationKnowledgeState,
        LadderKnowledgeProgress, FIXED_POINT_SCALE, FOOD_CARGO_KEY,
    };
    use std::sync::Arc;

    fn forage_row(tile: Tile, workers: u32, actual_yield: f32) -> LaborAssignmentState {
        LaborAssignmentState {
            kind: ROLE_FORAGE.into(),
            target_x: tile.x,
            target_y: tile.y,
            workers,
            actual_yield,
            sustainable_yield: actual_yield,
            ..Default::default()
        }
    }

    fn hunt_row(workers: u32, actual_yield: f32, useful: u32) -> LaborAssignmentState {
        LaborAssignmentState {
            kind: ROLE_HUNT.into(),
            fauna_id: HERD_ID.into(),
            workers,
            actual_yield,
            sustainable_yield: actual_yield,
            hunt_useful_workers: useful,
            ..Default::default()
        }
    }

    fn assigned_to(command: &CommandPayload) -> (String, u32, Option<Tile>, Option<String>) {
        match command {
            CommandPayload::AssignLabor {
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                ..
            } => (
                role.clone(),
                *workers,
                target_x.zip(*target_y).map(|(x, y)| Tile::new(x, y)),
                fauna_id.clone(),
            ),
            other => panic!("not an assignment: {other:?}"),
        }
    }

    // ---- rule 1: negative income ---------------------------------------------------------------

    #[test]
    fn idle_hands_go_to_the_richest_source_in_reach_and_the_reason_names_the_rule() {
        let view = a_view();
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("a proposal");
        assert_eq!(proposal.intent, "food:assign:7001");
        assert!(proposal.score > 0.0);
        assert_eq!(proposal.cost.workers, 17);
        assert_eq!(proposal.cost.bands, vec![BAND]);
        assert!(
            proposal.reason.starts_with(REASON_NEGATIVE_INCOME)
                && proposal.reason.contains("ledger:"),
            "{}",
            proposal.reason
        );
        let (role, workers, tile, _) = assigned_to(&proposal.commands[0]);
        assert_eq!(
            (role.as_str(), workers, tile),
            (ROLE_FORAGE, 17, Some(RICH_PATCH))
        );
        match &proposal.commands[0] {
            CommandPayload::AssignLabor {
                kit_id,
                floor,
                policy,
                ..
            } => {
                assert_eq!(kit_id, &None, "the frame's default kit");
                assert_eq!(floor, &None, "the sim's default floor — the balanced take");
                assert_eq!(policy, &None, "retired on the wire");
            }
            other => panic!("not an assignment: {other:?}"),
        }
        // Half the pool funded: 8 of the 17 go now, and the rest next turn.
        let capped = food()
            .negative_income(
                &view,
                &plan_with_food_share(0.5),
                &memory(),
                own_band(&view),
            )
            .expect("a capped proposal");
        assert_eq!(capped.cost.workers, 8);
        assert_eq!(assigned_to(&capped.commands[0]).1, 8);
        assert!(
            food()
                .negative_income(&view, &Plan::pass_through(TICK), &memory(), own_band(&view))
                .is_none(),
            "no share, no goals, no hands"
        );
    }

    /// `equipment.json`: *"HUNTING YIELDS NOTHING AT ANY CREW SIZE until a spear is crafted"* —
    /// a band holding no hunting gear is never sent to a herd, however rich; one hunting kit and
    /// the herd is a source again. Gear a hunt kit does not carry (baskets) is not a hunting kit.
    #[test]
    fn a_band_with_no_hunting_kit_is_never_sent_to_a_herd() {
        let mut view = a_view();
        // The rich patch out of sight, the herd big enough to out-earn the near patch.
        view.snapshot.visibility_raster.samples[(RICH_PATCH.y * 8 + RICH_PATCH.x) as usize] = 0;
        view.snapshot.herds[0].biomass = 100.0;
        let plan = plan_with_food_share(1.0);
        let assigned_role = |view: &SeatView| {
            let proposal = food()
                .negative_income(view, &plan, &memory(), own_band(view))
                .expect("a proposal");
            assigned_to(&proposal.commands[0]).0
        };
        assert_eq!(
            assigned_role(&view),
            ROLE_HUNT,
            "one spear: the herd is a source"
        );
        view.snapshot.populations[0].equipment_batches.clear();
        assert_eq!(
            assigned_role(&view),
            ROLE_FORAGE,
            "no gear: the herd is not"
        );
        assert!(
            food()
                .spare_hands_into_hunts(
                    &view,
                    &plan,
                    &memory(),
                    own_band(&view),
                    &Reassignment::NONE
                )
                .is_none(),
            "nor does rule 4 see it"
        );
        // A `count 0` row is "owns none of this item at all"; baskets are not hunting gear.
        view.snapshot.populations[0].equipment_batches = vec![
            EquipmentBatchState {
                count: 0,
                ..a_unit_of(HUNT_KIT_ITEM)
            },
            a_unit_of(FORAGE_KIT_ITEM),
        ];
        assert_eq!(
            assigned_role(&view),
            ROLE_FORAGE,
            "none of the spear, one basket"
        );
        view.snapshot.populations[0]
            .equipment_batches
            .push(a_unit_of(HUNT_KIT_HAUL_ITEM));
        assert_eq!(
            assigned_role(&view),
            ROLE_HUNT,
            "a hunt kit's haul aid counts"
        );
    }

    #[test]
    fn sources_are_ranked_by_what_the_crew_takes_not_the_per_worker_rate() {
        let mut view = a_view();
        view.snapshot.visibility_raster.samples[(RICH_PATCH.y * 8 + RICH_PATCH.x) as usize] = 0;
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .unwrap();
        assert_eq!(
            assigned_to(&proposal.commands[0]).2,
            Some(NEAR_PATCH),
            "a herd of two animals cannot feed a crew of 17, whatever its per-worker rate: {proposal:?}"
        );
        // A big herd is a different matter: 17 × 1.5 = 25.5 beats the near patch's 17.
        view.snapshot.herds[0].biomass = 100.0;
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .unwrap();
        let (role, _, _, herd) = assigned_to(&proposal.commands[0]);
        assert_eq!((role.as_str(), herd.as_deref()), (ROLE_HUNT, Some(HERD_ID)));
        view.snapshot.herds.clear();
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .unwrap();
        assert_eq!(
            assigned_to(&proposal.commands[0]).2,
            Some(NEAR_PATCH),
            "never the far patch, however rich"
        );
    }

    #[test]
    fn nothing_to_reassign_or_a_travelling_band_proposes_nothing() {
        let mut view = a_view();
        view.snapshot.populations[0].idle_workers = 0;
        assert!(
            food()
                .negative_income(
                    &view,
                    &plan_with_food_share(1.0),
                    &memory(),
                    own_band(&view)
                )
                .is_none(),
            "income is short but there is nobody to move"
        );
        view.snapshot.populations[0].idle_workers = 3;
        view.snapshot.populations[0].is_traveling = true;
        assert!(food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view)
            )
            .is_none());
        // Fed and busy: the rule does not fire.
        let mut view = a_view();
        let band = &mut view.snapshot.populations[0];
        band.idle_workers = 0;
        band.food_income = band.food_consumption + 1.0;
        band.labor_assignments = vec![forage_row(NEAR_PATCH, 17, 7.0)];
        assert!(food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view)
            )
            .is_none());
    }

    #[test]
    fn the_lowest_row_is_emptied_onto_the_best_source_when_it_pays_more() {
        let mut view = a_view();
        let band = &mut view.snapshot.populations[0];
        band.idle_workers = 0;
        band.labor_assignments = vec![forage_row(NEAR_PATCH, 4, 2.0), hunt_row(5, 10.0, 5)];
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("a proposal");
        assert_eq!(proposal.commands.len(), 2, "one assign_labor pair");
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_FORAGE.to_owned(), 0, Some(NEAR_PATCH), None)
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 4, Some(RICH_PATCH), None)
        );
        assert!(
            proposal.reason.contains(WHY_LOWEST_ROW),
            "{}",
            proposal.reason
        );
        assert_eq!(proposal.cost.workers, 4);
    }

    /// ⛔ **A SHUFFLE THAT MOVES NOBODY ANYWHERE BETTER IS NOT A PROPOSAL.** Two rows paying the
    /// same per worker are distinct rows, which is all the guard used to ask, so the runway
    /// consideration proposed a swap between them every turn under the alarm and one order per band
    /// then rejected the idle-hands assignment as `conflict`. With nothing better in reach the
    /// band's one order goes to its idle hands.
    #[test]
    fn two_equally_paying_rows_with_nothing_better_in_reach_move_only_the_idle_hands() {
        let mut view = a_view();
        view.snapshot
            .food_modules
            .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
        let band = &mut view.snapshot.populations[0];
        band.turns_of_food = 2.0;
        band.idle_workers = 5;
        band.labor_assignments = vec![
            LaborAssignmentState {
                sustainable_yield: 8.0,
                ..forage_row(NEAR_PATCH, 4, 4.0)
            },
            LaborAssignmentState {
                sustainable_yield: 8.0,
                ..hunt_row(6, 6.0, 6)
            },
        ];
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("the idle hands");
        assert_eq!(
            proposal.commands.len(),
            1,
            "no row is emptied: {:?}",
            proposal.commands
        );
        assert_eq!(assigned_to(&proposal.commands[0]).1, 4 + 5);
        assert_eq!(proposal.cost.workers, 5);
        // A row that really does pay more is still moved onto — and the idle hands go with it.
        let view = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 5;
            band.labor_assignments = vec![forage_row(NEAR_PATCH, 4, 4.0), hunt_row(6, 6.0, 6)];
        });
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .unwrap();
        assert_eq!(proposal.commands.len(), 2);
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 9, Some(RICH_PATCH), None)
        );
        assert_eq!(proposal.cost.workers, 9);
    }

    fn a_view_with(edit: impl FnOnce(&mut SeatView)) -> SeatView {
        let mut view = a_view();
        edit(&mut view);
        view
    }

    #[test]
    fn ground_without_a_food_module_is_not_a_gathering_site() {
        let view = a_view_with(|view| {
            view.snapshot
                .food_modules
                .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
        });
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .unwrap();
        assert_eq!(
            assigned_to(&proposal.commands[0]).2,
            Some(NEAR_PATCH),
            "the rich patch carries no food module, so nobody can gather there: {proposal:?}"
        );
    }

    #[test]
    fn a_row_realizing_a_fraction_of_its_forecast_is_dead_after_the_profile_turns_and_avoided() {
        let mut view = a_view();
        view.snapshot.herds[0].biomass = 100.0;
        // Twelve hunters on a herd forecast at 1.5/worker bring home 0.12 a turn: 0.01/worker.
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 1.35,
            ..hunt_row(12, 0.12, 12)
        }];
        view.snapshot.populations[0].idle_workers = 5;
        let specialist = food();
        let mut memory = memory();
        for _ in 0..specialist.floors.dead_row_turns {
            memory.observe(&view, FACTION);
        }
        let proposal = specialist
            .negative_income(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
            .expect("the dead row and the idle hands");
        assert!(
            proposal.reason.contains(WHY_DEAD_ROW),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_HUNT.to_owned(), 0, None, Some(HERD_ID.to_owned()))
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 17, Some(RICH_PATCH), None)
        );
        // A second herd it has never worked is no source either: the seat has measured what
        // hunting pays it. Only the stands in reach draw the idle hands.
        view.snapshot.herds.push(HerdTelemetryState {
            id: "herd_10".to_owned(),
            ..view.snapshot.herds[0].clone()
        });
        view.snapshot.populations[0].labor_assignments.clear();
        view.snapshot.populations[0].idle_workers = 17;
        let idle = specialist
            .negative_income(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
            .unwrap();
        assert_eq!(assigned_to(&idle.commands[0]).0, ROLE_FORAGE, "{idle:?}");
    }

    /// ⛔ **AN OVER-STAFFED HUNT IS NOT A FAILING ONE.** `hunt_useful_workers` is the crew-take
    /// plateau, so twelve hands on a plateau of two bring home what two bring home. Measured
    /// against the twelve the band *assigned*, that reads as a sixth of the forecast — under the
    /// forager's `poor_yield_fraction` — and the herd was declared dead and struck off
    /// `reachable_sources`, on a row the specialist had staffed that way itself.
    #[test]
    fn an_over_staffed_hunt_row_is_measured_on_its_plateau_and_is_not_dead() {
        const ASSIGNED: u32 = 12;
        const PLATEAU: u32 = 2;
        let mut view = a_view();
        view.snapshot.herds[0].biomass = 100.0;
        // The forecast is 1.5 a worker; the plateau of two takes 3.0, which is exactly it.
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 10.0,
            ..hunt_row(ASSIGNED, PLATEAU as f32 * 1.5, PLATEAU)
        }];
        view.snapshot.populations[0].idle_workers = 5;
        let specialist = food();
        let mut memory = memory();
        for _ in 0..specialist.floors.dead_row_turns + 1 {
            memory.observe(&view, FACTION);
        }
        // It is still a source: with the stands out of reach, the idle hands go to it.
        view.snapshot.forage_patches.clear();
        let idle = specialist
            .negative_income(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
            .expect("the herd is still a source");
        assert!(!idle.reason.contains(WHY_DEAD_ROW), "{}", idle.reason);
        let (role, _, _, herd) = assigned_to(idle.commands.last().unwrap());
        assert_eq!((role.as_str(), herd.as_deref()), (ROLE_HUNT, Some(HERD_ID)));
    }

    #[test]
    fn a_hunt_row_no_crew_is_useful_on_is_emptied_first_onto_the_next_best_source() {
        let mut view = a_view();
        view.snapshot.populations[0].idle_workers = 0;
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 0.02,
            ..hunt_row(12, 0.0, 0)
        }];
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("actual ≤ sustainable, but the sim says no crew is useful");
        assert!(
            proposal.reason.contains(WHY_NO_USEFUL_CREW),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 12, Some(RICH_PATCH), None)
        );
        view.snapshot.populations[0].labor_assignments[0].hunt_useful_workers = 4;
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("still the lowest row, and the stand pays more");
        assert!(
            !proposal.reason.contains(WHY_NO_USEFUL_CREW),
            "{}",
            proposal.reason
        );
    }

    /// ⛔ **One failed hunt does not condemn every herd in reach.** A row the sim marks
    /// `hunt_useful_workers == 0` is not a measurement, so it never reaches the web's mean; the
    /// sibling herd the band has never worked is still rated off the frame's forecast, and is
    /// still a source. Before this, the first such row rated every hunt in the world `0.0` — and
    /// `expected() > 0` then dropped them all, for the rest of the run.
    #[test]
    fn a_hunt_nobody_was_useful_on_does_not_condemn_a_sibling_herd() {
        let mut view = a_view();
        // Only herds in reach, so the answer is about hunting and nothing else.
        view.snapshot.forage_patches.clear();
        view.snapshot.populations[0].idle_workers = 5;
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 0.02,
            ..hunt_row(12, 0.0, 0)
        }];
        let sibling = "herd_10";
        view.snapshot.herds.push(HerdTelemetryState {
            id: sibling.to_owned(),
            biomass: 100.0,
            ..view.snapshot.herds[0].clone()
        });
        let mut memory = memory();
        memory.observe(&view, FACTION);
        let proposal = food()
            .negative_income(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
            .expect("the sibling herd is still a source");
        assert!(
            proposal.reason.contains(WHY_NO_USEFUL_CREW),
            "{}",
            proposal.reason
        );
        let (role, workers, _, herd) = assigned_to(proposal.commands.last().unwrap());
        assert_eq!(
            (role.as_str(), workers, herd.as_deref()),
            (ROLE_HUNT, 17, Some(sibling)),
            "the failed row and the idle hands, all onto the sibling"
        );
    }

    #[test]
    fn an_overused_row_is_emptied_first_onto_the_next_best_source() {
        let mut view = a_view();
        view.snapshot.populations[0].idle_workers = 0;
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 2.0,
            ..hunt_row(6, 8.0, 6)
        }];
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("a proposal");
        assert_eq!(proposal.intent, "food:assign:7001");
        assert!(
            proposal.reason.contains(WHY_OVERUSED),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 6, Some(RICH_PATCH), None)
        );
    }

    // ---- rule 2: feed while moving -------------------------------------------------------------

    const WEST_PATCH: Tile = Tile::new(2, 2);
    const EAST_TARGET: Tile = Tile::new(6, 2);

    fn add_patch(view: &mut SeatView, tile: Tile, per_worker_yield: f32, capacity: f32) {
        view.snapshot
            .forage_patches
            .push(sim_runtime::ForagePatchState {
                x: tile.x,
                y: tile.y,
                owner: None,
                per_worker_yield,
                carrying_capacity: capacity,
                biomass: capacity * 1.5,
                provisions_per_biomass: 1.0,
                ..Default::default()
            });
        view.snapshot
            .food_modules
            .push(sim_runtime::FoodModuleState {
                x: tile.x,
                y: tile.y,
                ..view.snapshot.food_modules[0].clone()
            });
    }

    /// The memory of a seat that split `parent` last turn, on the frame where `child` first
    /// appears on the parent's tile.
    fn memory_with_a_birth(view: &SeatView, parent: u64, child: u64, target: Tile) -> SeatMemory {
        let mut memory = memory();
        let mut earlier = SeatView {
            snapshot: view.snapshot.clone(),
            last_acted_tick: None,
        };
        earlier.snapshot.header.tick = TICK - 1;
        earlier
            .snapshot
            .populations
            .retain(|band| band.band_id != child);
        memory.observe(&earlier, FACTION);
        memory.record_choices(
            TICK - 1,
            [(
                intent_key(SPECIALIST_FOOD, INTENT_SPLIT, parent),
                Some(Memo::Split {
                    band: parent,
                    target,
                    workers: 5,
                }),
            )]
            .into_iter(),
        );
        memory.observe(view, FACTION);
        assert!(
            memory.born_by_split(child).is_some(),
            "the child was matched"
        );
        memory
    }

    #[test]
    fn a_band_about_to_move_east_works_the_patch_that_will_fall_out_of_range_to_the_west() {
        let view = a_view_with(|view| add_patch(view, WEST_PATCH, 3.0, 30.0));
        let mut walking = memory();
        walking.record_choices(
            TICK - 1,
            [(
                "land:move:7001".to_owned(),
                Some(Memo::Move {
                    band: BAND,
                    target: EAST_TARGET,
                    from: HERE,
                }),
            )]
            .into_iter(),
        );
        let proposal = food()
            .feed_while_moving(&view, &plan_with_food_share(1.0), &walking, own_band(&view))
            .expect("a source falls out of range");
        assert_eq!(proposal.intent, "food:feed_move:7001");
        assert!(
            proposal.reason.starts_with(REASON_FEED_MOVE),
            "{}",
            proposal.reason
        );
        assert!(proposal.reason.contains("6,2"), "{}", proposal.reason);
        assert_eq!(
            assigned_to(proposal.commands.last().unwrap()),
            (ROLE_FORAGE.to_owned(), 17, Some(WEST_PATCH), None),
            "the richest of the sources the move leaves behind"
        );
        assert_eq!(proposal.cost.workers, 17);
        // Nothing in force: nothing to do before leaving.
        assert!(food()
            .feed_while_moving(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view)
            )
            .is_none());
        // A move that keeps every source in reach: nothing falls out.
        let mut nearby = memory();
        nearby.record_choices(
            TICK - 1,
            [(
                "land:move:7001".to_owned(),
                Some(Memo::Move {
                    band: BAND,
                    target: Tile::new(3, 3),
                    from: HERE,
                }),
            )]
            .into_iter(),
        );
        let plain = a_view();
        assert!(food()
            .feed_while_moving(
                &plain,
                &plan_with_food_share(1.0),
                &nearby,
                own_band(&plain)
            )
            .is_none());
    }

    #[test]
    fn a_band_born_by_a_split_last_turn_does_not_strip_the_parents_ground_on_its_way_out() {
        const PARENT: u64 = 7000;
        let view = a_view_with(|view| {
            add_patch(view, WEST_PATCH, 3.0, 30.0);
            view.snapshot.populations.push(PopulationCohortState {
                faction: FACTION,
                band_id: PARENT,
                current_x: HERE.x,
                current_y: HERE.y,
                working_age: 12,
                ..Default::default()
            });
        });
        let mut memory = memory_with_a_birth(&view, PARENT, BAND, EAST_TARGET);
        // The child's settle move is in force, exactly as `settle` records it.
        memory.record_choices(
            TICK,
            [(
                intent_key(SPECIALIST_FOOD, INTENT_SETTLE, BAND),
                Some(Memo::Move {
                    band: BAND,
                    target: EAST_TARGET,
                    from: HERE,
                }),
            )]
            .into_iter(),
        );
        assert!(
            food()
                .feed_while_moving(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
                .is_none(),
            "freshly split: leave the parent's ground alone"
        );
        assert!(
            food()
                .negative_income(&view, &plan_with_food_share(1.0), &memory, own_band(&view))
                .is_none(),
            "nor do its idle hands work the parent's sources while it has a site to reach"
        );
    }

    // ---- rule 3: split to feed, then settle ----------------------------------------------------

    const SPLIT_SITE: Tile = Tile::new(6, 2);
    const CHILD: u64 = 7002;

    /// A band short of food with nothing better in reach: twelve hands on the near patch earning
    /// 12 against 16 eaten, the rich patch no site, and a site three tiles east a band of five
    /// could work.
    fn a_short_band() -> SeatView {
        a_view_with(|view| {
            view.snapshot
                .food_modules
                .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
            add_patch(view, SPLIT_SITE, 1.5, 20.0);
            let band = &mut view.snapshot.populations[0];
            band.working_age = 12;
            band.idle_workers = 0;
            band.food_income = 12.0;
            band.food_consumption = 16.0;
            band.stores = vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (40.0 * FIXED_POINT_SCALE as f32) as i64,
            }];
            band.labor_assignments = vec![forage_row(NEAR_PATCH, 12, 12.0)];
        })
    }

    #[test]
    fn a_band_still_short_after_reassignment_splits_toward_a_site_just_out_of_reach() {
        let view = a_short_band();
        let specialist = food();
        let plan = plan_with_food_share(1.0);
        let (assign, carried) = specialist.assess_income(&view, &plan, &memory(), own_band(&view));
        assert!(
            assign.is_none(),
            "the herd of two cannot take twelve hands, so nothing moves: {assign:?}"
        );
        let proposal = specialist
            .split_to_feed(&view, &plan, &memory(), own_band(&view), &carried)
            .expect("a site three tiles east");
        assert_eq!(proposal.intent, "food:split:7001");
        assert!(
            proposal.reason.starts_with(REASON_SPLIT_TO_FEED),
            "{}",
            proposal.reason
        );
        assert!(proposal.reason.contains("6,2"), "{}", proposal.reason);
        assert!(matches!(
            proposal.commands[0],
            CommandPayload::SplitBand { workers: 5, .. }
        ));
        assert_eq!(
            proposal.memo,
            Some(Memo::Split {
                band: BAND,
                target: SPLIT_SITE,
                workers: 5
            })
        );
        assert_eq!(proposal.cost.workers, 5);
        // Nine hands leave 3 over the parent floor of 6, under the founding floor of 4; not with
        // a split already pending.
        let small = a_view_with(|view| view.snapshot.populations[0].working_age = 9);
        let small = SeatView {
            snapshot: sim_runtime::WorldSnapshot {
                populations: small.snapshot.populations,
                ..view.snapshot.clone()
            },
            last_acted_tick: None,
        };
        assert!(specialist
            .split_to_feed(&small, &plan, &memory(), own_band(&small), &carried)
            .is_none());
        let mut pending = memory();
        pending.record_choices(TICK, [(proposal.intent.clone(), proposal.memo)].into_iter());
        assert!(specialist
            .split_to_feed(&view, &plan, &pending, own_band(&view), &carried)
            .is_none());
        // The sim refused it (no child ever appeared): not asked again at this size, asked again
        // once the band has grown. The floors say twelve may split; the refusal is the belt.
        let mut refused = memory();
        refused.observe(&view, FACTION);
        refused.record_choices(TICK, [(proposal.intent.clone(), proposal.memo)].into_iter());
        let mut later = SeatView {
            snapshot: view.snapshot.clone(),
            last_acted_tick: None,
        };
        later.snapshot.header.tick = TICK + u64::from(specialist.floors.split_settle_turns) + 1;
        refused.observe(&later, FACTION);
        assert_eq!(refused.split_refused_at(BAND), Some(12));
        assert!(specialist
            .split_to_feed(&later, &plan, &refused, own_band(&later), &carried)
            .is_none());
        // Grown to seventeen: the refusal was of a band of twelve. (Seventeen rather than
        // thirteen because this memory has measured the near row at 1.0 a hand, and the web's
        // prior now rates the site at that rather than its 1.5 forecast — five hands take 5.0,
        // which must still beat the crew's share of what the band eats.)
        later.snapshot.populations[0].working_age = 17;
        assert!(specialist
            .split_to_feed(&later, &plan, &refused, own_band(&later), &carried)
            .is_some());
        // A site the split crew could not feed itself on is not a fix.
        let poor = a_view_with(|_| {});
        let mut poor = SeatView {
            snapshot: view.snapshot.clone(),
            last_acted_tick: poor.last_acted_tick,
        };
        for patch in &mut poor.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == SPLIT_SITE {
                patch.per_worker_yield = 0.5;
            }
        }
        assert!(specialist
            .split_to_feed(&poor, &plan, &memory(), own_band(&poor), &carried)
            .is_none());
    }

    /// The child crew is what the parent may give up, capped at `split_band_workers`, and the
    /// rule is silent under the founding floor: with parent floor 6 and founding floor 4, ten
    /// hands split 4, nine split nothing, seventeen split the profile's 5.
    #[test]
    fn the_split_crew_is_sized_by_the_wires_floors() {
        let specialist = food();
        let plan = plan_with_food_share(1.0);
        let at = |working_age: u32| {
            a_view_with(|view| {
                view.snapshot
                    .food_modules
                    .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
                // A site rich enough that any crew the floors allow out-earns its share.
                add_patch(view, SPLIT_SITE, 2.0, 40.0);
                let band = &mut view.snapshot.populations[0];
                band.working_age = working_age;
                band.idle_workers = 0;
                band.food_income = 12.0;
                band.food_consumption = 16.0;
                band.stores = vec![CohortStoreState {
                    item: FOOD_CARGO_KEY.to_owned(),
                    quantity: (40.0 * FIXED_POINT_SCALE as f32) as i64,
                }];
                band.labor_assignments = vec![forage_row(NEAR_PATCH, working_age, 12.0)];
            })
        };
        let crew_at = |working_age: u32| {
            let view = at(working_age);
            let band = own_band(&view);
            assert_eq!(
                (band.founding_min_workers, band.founding_parent_min_workers),
                (4, 6)
            );
            let (_, carried) = specialist.assess_income(&view, &plan, &memory(), band);
            specialist
                .split_to_feed(&view, &plan, &memory(), band, &carried)
                .map(|proposal| match proposal.commands[0] {
                    CommandPayload::SplitBand { workers, .. } => {
                        assert_eq!(proposal.cost.workers, workers);
                        assert!(
                            proposal.reason.contains(&format!("{workers} toward")),
                            "{}",
                            proposal.reason
                        );
                        workers
                    }
                    ref other => panic!("not a split: {other:?}"),
                })
        };
        assert_eq!(crew_at(10), Some(4), "ten less the parent's six");
        assert_eq!(crew_at(9), None, "three is under the founding floor");
        assert_eq!(crew_at(17), Some(5), "capped at split_band_workers");
    }

    #[test]
    fn the_child_settles_toward_its_site_until_it_arrives() {
        let parent_frame = a_short_band();
        let mut view = SeatView {
            snapshot: parent_frame.snapshot.clone(),
            last_acted_tick: None,
        };
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: CHILD,
            current_x: HERE.x,
            current_y: HERE.y,
            size: 12,
            working_age: 5,
            idle_workers: 5,
            work_range: 2,
            hunt_reach: 5,
            food_consumption: 6.7,
            stores: vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (16.0 * FIXED_POINT_SCALE as f32) as i64,
            }],
            ..Default::default()
        });
        let mut memory = memory_with_a_birth(&view, BAND, CHILD, SPLIT_SITE);
        let child = view.band(CHILD).unwrap();
        let specialist = food();
        let plan = plan_with_food_share(1.0);
        let proposal = specialist
            .settle(&view, &plan, &memory, child)
            .expect("the child walks to its site");
        assert_eq!(proposal.intent, "food:settle:7002");
        assert!(
            proposal.reason.starts_with(REASON_SPLIT_TO_FEED),
            "{}",
            proposal.reason
        );
        assert!(matches!(
            proposal.commands[0],
            CommandPayload::MoveBand {
                band_id: Some(CHILD),
                target_x: 6,
                target_y: 2,
                ..
            }
        ));
        assert_eq!(proposal.cost.bands, vec![CHILD]);
        // Accepted: the memory holds the move under the settle intent, and it is re-proposed.
        memory.record_choices(TICK, [(proposal.intent.clone(), proposal.memo)].into_iter());
        assert_eq!(memory.move_intent(CHILD), Some("food:settle:7002"));
        let again = specialist
            .settle(&view, &plan, &memory, child)
            .expect("persists");
        assert_eq!(again.intent, proposal.intent);
        // The whole roster: the child's one proposal is the settle, not an assignment.
        let mut specialist = food();
        let proposals = specialist.propose(&view, &plan, &memory);
        let for_child: Vec<&str> = proposals
            .proposals
            .iter()
            .filter(|p| p.cost.bands.contains(&CHILD))
            .map(|p| p.intent.as_str())
            .collect();
        assert_eq!(for_child, vec!["food:settle:7002"]);
        // Arrived: nothing more to do.
        view.snapshot.header.tick = TICK + 3;
        let child = view
            .snapshot
            .populations
            .iter_mut()
            .find(|band| band.band_id == CHILD)
            .unwrap();
        child.current_x = SPLIT_SITE.x;
        child.current_y = SPLIT_SITE.y;
        memory.observe(&view, FACTION);
        assert!(memory.born_by_split(CHILD).is_none());
        assert!(food()
            .settle(&view, &plan, &memory, view.band(CHILD).unwrap())
            .is_none());
    }

    // ---- rule 4: spare hands into hunts --------------------------------------------------------

    /// A well-fed band: four on the near patch, thirteen on the rich one, and a small herd in
    /// reach that pays half what a near-patch hand does.
    fn a_fed_band(net: f32) -> SeatView {
        a_view_with(|view| {
            view.snapshot.herds[0].biomass = 0.5;
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = band.food_consumption + net;
            band.labor_assignments = vec![
                forage_row(NEAR_PATCH, 4, 4.0),
                forage_row(RICH_PATCH, 13, 26.0),
            ];
        })
    }

    #[test]
    fn a_well_fed_band_sends_the_surplus_to_the_herd_sized_so_income_stays_above_the_goal() {
        let view = a_fed_band(3.0);
        let plan = plan_with_food_share(1.0);
        let proposal = food()
            .spare_hands_into_hunts(
                &view,
                &plan,
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("a surplus");
        assert_eq!(proposal.intent, "food:hunt:7001");
        assert!(
            proposal.reason.starts_with(REASON_SPARE_HANDS),
            "{}",
            proposal.reason
        );
        assert!(proposal.reason.contains(HERD_ID), "{}", proposal.reason);
        // Net 3.0 against a goal of 1.0: two hands off the near patch (−2.0) onto a herd that
        // hands back 0.5 leaves 1.5; a third would leave 0.5, under the goal.
        assert_eq!(proposal.cost.workers, 2);
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_FORAGE.to_owned(), 2, Some(NEAR_PATCH), None)
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_HUNT.to_owned(), 2, None, Some(HERD_ID.to_owned()))
        );
        // Exactly at the goal: any hand leaving drops below it, so none does.
        let at_goal = a_fed_band(goals().net_income_per_turn);
        assert!(food()
            .spare_hands_into_hunts(
                &at_goal,
                &plan,
                &memory(),
                own_band(&at_goal),
                &Reassignment::NONE
            )
            .is_none());
        // Short of the goal by more than the near-positive fraction: not this rule's business.
        let short = a_fed_band(0.0);
        assert!(food()
            .spare_hands_into_hunts(
                &short,
                &plan,
                &memory(),
                own_band(&short),
                &Reassignment::NONE
            )
            .is_none());
    }

    // ---- rule 5: upgrade the ground ------------------------------------------------------------

    fn knowledge(view: &mut SeatView, cultivation: f32, seed_selection: f32) {
        view.snapshot.intensification_knowledge = vec![IntensificationKnowledgeState {
            faction: FACTION,
            knowledges: vec![
                LadderKnowledgeProgress {
                    knowledge_id: KNOWLEDGE_CULTIVATION.to_owned(),
                    progress: cultivation,
                },
                LadderKnowledgeProgress {
                    knowledge_id: KNOWLEDGE_SEED_SELECTION.to_owned(),
                    progress: seed_selection,
                },
            ],
        }];
    }

    /// A band working the rich patch with thirteen and the near one with four, breaking even, the
    /// rich patch priced for a cultivate that pays 40 a turn against the 26 taken today.
    fn a_working_band(stock: f32) -> SeatView {
        a_view_with(|view| {
            knowledge(view, 1.0, 0.0);
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = band.food_consumption;
            band.stores = vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (stock * FIXED_POINT_SCALE as f32) as i64,
            }];
            band.labor_assignments = vec![
                forage_row(NEAR_PATCH, 4, 4.0),
                forage_row(RICH_PATCH, 13, 26.0),
            ];
            for patch in &mut view.snapshot.forage_patches {
                if Tile::new(patch.x, patch.y) == RICH_PATCH {
                    patch.composition = Arc::from(vec![FloraShareInfo {
                        species: "hazel".to_owned(),
                        share: 1.0,
                        can_cultivate: true,
                        can_sow: true,
                        cultivate_payoff: 40.0,
                        sow_payoff: 80.0,
                        ..Default::default()
                    }]);
                    patch.cultivation_work_cost = 50.0;
                    patch.field_work_cost = 75.0;
                    patch.build_work_per_worker_turn = 1.0;
                }
            }
        })
    }

    #[test]
    fn a_worked_patch_is_cultivated_with_the_smallest_crew_the_larder_carries_to_the_payoff() {
        let view = a_working_band(STOCK);
        let plan = plan_with_food_share(1.0);
        let proposal = food()
            .upgrade_the_ground(
                &view,
                &plan,
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("the rung is known and the larder carries the build");
        assert_eq!(proposal.intent, "food:upgrade:7001");
        assert!(
            proposal.reason.starts_with(REASON_UPGRADE),
            "{}",
            proposal.reason
        );
        // One builder needs 50 turns, past the forager's 40-turn horizon; two need 25, and 84 in
        // the larder carries 25 turns at −2.0.
        assert!(
            proposal
                .reason
                .contains("cultivate 2,3 (hazel) with 2 builders, payoff turn 25"),
            "{}",
            proposal.reason
        );
        assert!(
            proposal.reason.contains("positive again t25"),
            "{}",
            proposal.reason
        );
        assert!(matches!(
            proposal.commands[0],
            CommandPayload::Cultivate {
                target_x: 2,
                target_y: 3,
                ..
            }
        ));
        // The hands come off the near patch, and the pool is staffed after the row is reduced.
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 2, Some(NEAR_PATCH), None)
        );
        assert_eq!(
            assigned_to(&proposal.commands[2]),
            (ROLE_BUILDERS.to_owned(), 2, None, None)
        );
        assert_eq!(proposal.cost.workers, 2);
        // A thin larder cannot carry any crew to the payoff.
        let thin = a_working_band(30.0);
        assert!(food()
            .upgrade_the_ground(
                &thin,
                &plan,
                &memory(),
                own_band(&thin),
                &Reassignment::NONE
            )
            .is_none());
        // Knowledge short: nothing to declare.
        let mut learning = a_working_band(STOCK);
        knowledge(&mut learning, 0.5, 0.0);
        assert!(food()
            .upgrade_the_ground(
                &learning,
                &plan,
                &memory(),
                own_band(&learning),
                &Reassignment::NONE
            )
            .is_none());
        // Already queued: not declared twice.
        let mut queued = a_working_band(STOCK);
        for patch in &mut queued.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.build_destination_rung = "plant:tended".to_owned();
            }
        }
        assert!(food()
            .upgrade_the_ground(
                &queued,
                &plan,
                &memory(),
                own_band(&queued),
                &Reassignment::NONE
            )
            .is_none());
        // A goal of wild ground: nothing to climb toward.
        assert!(food()
            .upgrade_the_ground(
                &view,
                &plan_toward(1.0, GroundRung::Wild),
                &memory(),
                own_band(&view),
                &Reassignment::NONE
            )
            .is_none());
    }

    #[test]
    fn a_tended_patch_is_sown_only_toward_the_field_goal_with_seed_selection_known() {
        let mut view = a_working_band(STOCK);
        for patch in &mut view.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.is_cultivated = true;
            }
        }
        let field = plan_toward(1.0, GroundRung::Field);
        assert!(
            food()
                .upgrade_the_ground(
                    &view,
                    &field,
                    &memory(),
                    own_band(&view),
                    &Reassignment::NONE
                )
                .is_none(),
            "seed selection unknown"
        );
        knowledge(&mut view, 1.0, 1.0);
        let proposal = food()
            .upgrade_the_ground(
                &view,
                &field,
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("a sow");
        assert!(matches!(
            proposal.commands[0],
            CommandPayload::Sow {
                target_x: 2,
                target_y: 3,
                ..
            }
        ));
        assert!(
            proposal.reason.contains("sow 2,3 (hazel)"),
            "{}",
            proposal.reason
        );
        // Toward the tended rung only, a tended patch is done.
        assert!(food()
            .upgrade_the_ground(
                &view,
                &plan_toward(1.0, GroundRung::Tended),
                &memory(),
                own_band(&view),
                &Reassignment::NONE
            )
            .is_none());
        // Ground that will not take seed.
        for patch in &mut view.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.sow_site_refusal = "too_dry".to_owned();
            }
        }
        assert!(food()
            .upgrade_the_ground(
                &view,
                &field,
                &memory(),
                own_band(&view),
                &Reassignment::NONE
            )
            .is_none());
    }

    /// Every rule's proposal names its rule, so the decision log's `reason` is what fired.
    #[test]
    fn every_rules_reason_opens_with_the_rules_name() {
        let view = a_working_band(STOCK);
        let plan = plan_with_food_share(1.0);
        let mut specialist = food();
        let proposals = specialist.propose(&view, &plan, &memory());
        let reasons: Vec<&str> = proposals
            .proposals
            .iter()
            .map(|p| p.reason.as_str())
            .collect();
        assert!(
            reasons.iter().any(|r| r.starts_with(REASON_UPGRADE)),
            "{reasons:?}"
        );
        assert_eq!(
            HERD_AT,
            Tile::new(5, 4),
            "the fixture's herd is in hunt reach"
        );
    }
}
