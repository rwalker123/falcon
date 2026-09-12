//! **The seven `Food` rules** (`docs/plan_ai_driver.md` §4, the rule table). Each is a method on
//! [`Food`] taking the view, the plan, the memory and one band, and answering at most one
//! proposal whose `reason` is `"<rule>: <subject> [ledger: …]"`. Every score is
//! [`goal_progress`] × the specialist's weight; a rule handed no goals proposes nothing. Rules
//! 2–5 also answer the change they priced (`*_change`), so the last rule can project the plan in
//! force — the book plus everything proposed this turn.
//!
//! The rules share one piece of arithmetic: [`Food::draw`], the hands a change frees and what
//! they earn where they stand — idle first (they cost nothing), then the **surplus** on the rows
//! offered for it (hands a row's take never needed, [`surplus_hands`]; nothing lost either), then
//! the rows named, in the order named, until each is empty.

use sim_runtime::{
    CommandPayload, FloraShareInfo, ForagePatchState, HerdTelemetryState, KitOptionState,
    LaborAssignmentState, PopulationCohortState,
};

use super::ledger::{
    floor_income, goal_progress, ledger_note, project, project_all, project_changes, regrowth_at,
    survives, Change, PatchBook, Projection, Reassignment, BEST_FLOOR,
};
use super::sources::{
    cluster_sites, cluster_take_over, crew_take, patch_per_worker_yield, surplus_hands,
    sustained_hands, workable_patch_at, Source, SourceKey,
};
use super::{
    Food, INTENT_ASSIGN, INTENT_DRAWDOWN, INTENT_FEED_MOVE, INTENT_HOLD, INTENT_HUNT,
    INTENT_SETTLE, INTENT_SPLIT, INTENT_UPGRADE, ROLE_AGRICULTURE, ROLE_BUILDERS, ROLE_FORAGE,
    ROLE_HUNT,
};
use crate::board::{Demand, Resource, BARE_KIT_ID};
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
pub(super) const REASON_HOLD_GROUND: &str = "hold the ground";
pub(super) const REASON_DRAW_DOWN: &str = "draw down to survive";
/// *Draw down to survive*'s other half: the floor put back to Best once the projection clears.
pub(super) const REASON_FLOOR_RESTORE: &str = "floor back to best";
/// **The harvest-floor slider's coarse step**, chosen so the search is ten projections per row
/// from Best down to zero.
const FLOOR_STEP: f32 = 0.1;
/// How far apart two floors may read and still be the same floor — a tenth of a step, so a
/// floor the wire rounded is still the ladder's rung.
const FLOOR_TOLERANCE: f32 = FLOOR_STEP / 10.0;
/// **A rung is worth stepping only if it buys the band a turn**: a further floor must raise the
/// projected trough by at least this many turns of the band's consumption over the plan in force
/// as it stands. Without it the slide stepped a rung a turn for a hair of gap each (bench seed
/// 23, t34–t40: 0.4 → 0.3 → 0.2 → 0.1).
const RUNG_MIN_GAIN_TURNS: f32 = 1.0;
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

/// **Forage pays first**: a hand without a basket gathers nothing better than bare hands do, so
/// the baskets for the sites in reach are the first thing `Food` asks the board for.
pub(super) const DEMAND_PRIORITY_GATHERING: f32 = 1.0;
/// **Hunting is what opens penning**, second to the sites: a spear for every hand the sites in
/// reach cannot use, when a herd in reach can be brought down with it.
pub(super) const DEMAND_PRIORITY_HUNTING: f32 = 0.8;
/// The roster's gathering kit (`equipment.json` → `gathering`, jobs `forage`, baskets).
const GATHERING_KIT_ID: &str = "gathering";
/// A herd whose `body_mass` reads this has none on the wire — *"`0` if unknown"*
/// (`HerdTelemetryState::body_mass`) — and only a kit with no upper mass bound is trusted on it.
const BODY_MASS_UNKNOWN: f32 = 0.0;
/// A kit whose mass bound reads this has none — *"`0` on either end means unbounded"*
/// (`KitOptionState::attack_min_body_mass`).
const MASS_UNBOUNDED: f32 = 0.0;

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
    /// How many of `hands` were the band's idle ones — what the draw wanted, capped by the idle
    /// on offer. The rest came off rows. A candidate names its free hands from this, never from
    /// the idle count it was handed: a draw wanting fewer than the idle takes fewer.
    idle: u32,
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
    /// Free `want` hands: the band's idle first, then the surplus on `surplus_from` — each row
    /// down to its `workers_needed`, at no cost ([`surplus_hands`]) — then `rows` in the order
    /// given until each is empty. `hands` is what could be freed, which may be short of `want`.
    fn draw(
        &self,
        surplus_from: &[&LaborAssignmentState],
        rows: &[&LaborAssignmentState],
        want: u32,
        idle: u32,
    ) -> Drawn {
        let mut drawn = Drawn {
            hands: idle.min(want),
            idle: idle.min(want),
            income_lost: 0.0,
            reductions: Vec::new(),
        };
        for row in surplus_from {
            if drawn.hands >= want {
                break;
            }
            let Some(key) = SourceKey::of_row(row) else {
                continue;
            };
            let take = (want - drawn.hands).min(surplus_hands(row));
            if take == 0 {
                continue;
            }
            drawn.reductions.push((key, row.workers - take));
            drawn.hands += take;
        }
        for row in rows {
            if drawn.hands >= want {
                break;
            }
            let Some(key) = SourceKey::of_row(row) else {
                continue;
            };
            // What the surplus tier left on this row, if it drew from it.
            let left = drawn
                .reductions
                .iter()
                .find(|(reduced, _)| *reduced == key)
                .map_or(row.workers, |(_, left)| *left);
            let take = (want - drawn.hands).min(left);
            if take == 0 {
                continue;
            }
            drawn.income_lost += Self::row_rate(row) * take as f32;
            match drawn
                .reductions
                .iter_mut()
                .find(|(reduced, _)| *reduced == key)
            {
                Some((_, held)) => *held = left - take,
                None => drawn.reductions.push((key, left - take)),
            }
            drawn.hands += take;
        }
        drawn
    }

    /// The band's rows carrying surplus hands ([`surplus_hands`]), lowest-paying first.
    fn surplus_rows(band: &PopulationCohortState) -> Vec<&LaborAssignmentState> {
        let mut rows: Vec<&LaborAssignmentState> = band
            .labor_assignments
            .iter()
            .filter(|row| surplus_hands(row) > 0)
            .collect();
        rows.sort_by(|a, b| Self::row_rate(a).total_cmp(&Self::row_rate(b)));
        rows
    }

    /// How a candidate names its free hands: *"9 idle hands"*, *"9 surplus hands"*, or both.
    fn free_hands_phrase(idle: u32, surplus: u32) -> String {
        match (idle, surplus) {
            (_, 0) => format!("{idle} idle hands"),
            (0, _) => format!("{surplus} surplus hands"),
            _ => format!("{idle} idle and {surplus} surplus hands"),
        }
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
            let why = if row.actual_yield > row.sustainable_yield && Self::at_its_floor(view, row) {
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

    /// **Distinctness is not improvement, on the free-hand path too.** Free hands move onto a
    /// site only when the site's take actually rises by them: the marginal take of `moved` hands
    /// there must be at least `food.runway_gain_fraction × moved × rate` (the idiom the row-empty
    /// guard uses), **and** the band's row on that site, if it has one, must not already read at
    /// or past the crew the frame says the take needs (`workers ≥ workers_needed`) — a hand that
    /// would read surplus where it lands stays where it is. The second half is the frame's own
    /// word: the ceiling the model deals by said 47,5 and 49,5 each had room for one more hand
    /// while the frame read that hand as surplus wherever it stood, and rule 1 sent it back and
    /// forth every turn of seed 23's t45–t52.
    fn improves(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        tile: Tile,
        moved: u32,
        take: f32,
    ) -> bool {
        if moved == 0 {
            return false;
        }
        let Some(patch) = view.patch_at(tile) else {
            return false;
        };
        let rate = patch_per_worker_yield(memory, band, patch);
        let row_full = band
            .labor_assignments
            .iter()
            .filter(|row| row.workers > 0 && row.workers_needed > 0)
            .any(|row| {
                SourceKey::of_row(row) == Some(SourceKey::Patch(tile))
                    && row.workers >= row.workers_needed
            });
        !row_full && take >= self.floors.runway_gain_fraction * moved as f32 * rate
    }

    /// **Whether a row's source is at or below its floor** — where `actual_yield >
    /// sustainable_yield` is overuse. A patch whose `biomass > floor × carrying_capacity` is not
    /// overused by a take above its regrowth: that is the room above the floor being taken by
    /// design, and the floor protects the stand. A hunt row keeps the trigger as it is (the
    /// herd's floor is its escapement, not priced here). Before this, a fresh patch read
    /// "overused" every other turn and rule 1 shuffled band 2's hands between 47,5 and 49,5 for
    /// the whole of seed 23's t45–t50.
    fn at_its_floor(view: &SeatView, row: &LaborAssignmentState) -> bool {
        if row.kind != ROLE_FORAGE {
            return true;
        }
        view.patch_at(Tile::new(row.target_x, row.target_y))
            .is_none_or(|patch| patch.biomass <= row.floor * patch.carrying_capacity)
    }

    /// The reassignments *negative income* chooses among, all within `budget`: (a) the free hands
    /// — the idle ones plus every row's surplus, each donor row reduced to its `workers_needed` —
    /// **dealt across the sites in reach the way [`cluster_take_over`] deals them** (a band in a
    /// cluster spreads over it instead of piling onto one site), and, weighed beside it, the same
    /// hands onto the single best source none of them leave (which may be a herd); (b) the row to
    /// empty first onto the best other source, when that buys something; (c) both onto the best
    /// source for the whole crew.
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
        let surplus_rows = Self::surplus_rows(band);
        let free = self.draw(&surplus_rows, &[], budget, idle);
        let donors: Vec<SourceKey> = free.reductions.iter().map(|(key, _)| key.clone()).collect();
        if free.hands > 0 {
            let is_dead =
                |key: &SourceKey, forecast: f32| self.is_dead(memory, band, key, forecast);
            let cluster = cluster_take_over(
                view,
                memory,
                band,
                band_tile(band),
                free.hands,
                &is_dead,
                &|tile| {
                    (!donors.contains(&SourceKey::Patch(tile)))
                        .then(|| Self::workers_on(band, &SourceKey::Patch(tile)))
                },
            );
            // Only the sites the hands improve take any; the hands a site could not use stay
            // where they are, so the donors are drawn down only by what is placed.
            let sites: Vec<&(Tile, u32, f32)> = cluster
                .sites
                .iter()
                .filter(|(tile, hands, take)| {
                    self.improves(view, memory, band, *tile, *hands, *take)
                })
                .collect();
            let dealt: u32 = sites.iter().map(|(_, hands, _)| hands).sum();
            if dealt > 0 {
                let placed_hands = self.draw(&surplus_rows, &[], dealt, idle);
                let mut commands = self.reduction_commands(band, &placed_hands);
                let mut placed = Vec::new();
                for (tile, hands, _) in &sites {
                    let key = SourceKey::Patch(*tile);
                    commands.push(self.assign(band, &key, Self::workers_on(band, &key) + hands));
                    placed.push(format!("{} ×{hands}", key.describe()));
                }
                out.push(Candidate {
                    commands,
                    hands: dealt,
                    change: Reassignment {
                        income_lost: 0.0,
                        income_gained: sites.iter().map(|(_, _, take)| take).sum(),
                        payoff_turn: 0,
                    },
                    subject: format!(
                        "{} -> {}",
                        Self::free_hands_phrase(
                            placed_hands.idle,
                            placed_hands.hands - placed_hands.idle
                        ),
                        placed.join(", ")
                    ),
                });
            }
        }
        let free_onto = (free.hands > 0)
            .then(|| Self::best_source(sources, free.hands, &donors))
            .flatten()
            .filter(|best| {
                let existing = Self::workers_on(band, &best.key);
                let take = best.marginal(existing, free.hands);
                match &best.key {
                    SourceKey::Patch(tile) => {
                        self.improves(view, memory, band, *tile, free.hands, take)
                    }
                    SourceKey::Herd(_) => {
                        take >= self.floors.runway_gain_fraction
                            * free.hands as f32
                            * best.per_worker_yield
                    }
                }
            });
        if let Some(best) = free_onto {
            let existing = Self::workers_on(band, &best.key);
            let mut commands = self.reduction_commands(band, &free);
            commands.push(self.assign(band, &best.key, existing + free.hands));
            out.push(Candidate {
                commands,
                hands: free.hands,
                change: Reassignment {
                    income_lost: 0.0,
                    income_gained: best.marginal(existing, free.hands),
                    payoff_turn: 0,
                },
                subject: format!(
                    "{} -> {}",
                    Self::free_hands_phrase(free.idle, free.hands - free.idle),
                    best.key.describe()
                ),
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
        if let Some(next) = Self::best_source(sources, moved, std::slice::from_ref(&low_key))
            .filter(|next| clears(next))
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
        // The free hands again, less the emptied row's own surplus (its whole crew moves).
        let surplus_elsewhere: Vec<&LaborAssignmentState> = surplus_rows
            .iter()
            .copied()
            .filter(|other| SourceKey::of_row(other).as_ref() != Some(&low_key))
            .collect();
        let free = self.draw(&surplus_elsewhere, &[], budget.saturating_sub(moved), idle);
        let both = free.hands + moved;
        if free.hands > 0 && both <= budget {
            let mut except: Vec<SourceKey> =
                free.reductions.iter().map(|(key, _)| key.clone()).collect();
            except.push(low_key.clone());
            if let Some(best) =
                Self::best_source(sources, both, &except).filter(|next| clears(next))
            {
                let existing = Self::workers_on(band, &best.key);
                let change = Reassignment {
                    income_lost: low * moved as f32,
                    income_gained: best.marginal(existing, both),
                    payoff_turn: 0,
                };
                if change.income_gained > change.income_lost {
                    let mut commands = self.reduction_commands(band, &free);
                    commands.push(self.assign(band, &low_key, row.workers - moved));
                    commands.push(self.assign(band, &best.key, existing + both));
                    out.push(Candidate {
                        commands,
                        hands: both,
                        change,
                        subject: format!(
                            "{} and {why} {} -> {}",
                            Self::free_hands_phrase(free.idle, free.hands - free.idle),
                            low_key.describe(),
                            best.key.describe()
                        ),
                    });
                }
            }
        }
        out
    }

    /// **Rule 1 — negative income.** Income below consumption, or idle hands, or surplus hands on
    /// a row ([`surplus_hands`] — both are negative income against what they could earn),
    /// reassigns to the source the crew takes the most from; the change that closes the most goal
    /// gap wins. Answers the proposal and the change it carries,
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
        let fires = band.food_income < band.food_consumption
            || band.idle_workers > 0
            || !Self::surplus_rows(band).is_empty();
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

    /// The rule as a proposal alone — what the tests drive; `propose` goes through
    /// [`Food::feed_while_moving_change`] to carry the change forward.
    #[cfg(test)]
    pub fn feed_while_moving(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<Proposal> {
        self.feed_while_moving_change(view, plan, memory, band)
            .map(|(proposal, _)| proposal)
    }

    /// **Rule 2 — feed while moving.** A band with a move in force and not yet under way works
    /// what will fall *outside* its range from the target before it leaves: the idle hands, and
    /// the crews of rows that will still be in range after the move, onto the best source that
    /// will not. Never a freshly split band within `split_settle_turns` of its birth — it must not
    /// strip the parent's ground on its way out.
    ///
    /// Answers the change it priced beside the proposal, so the last rule can project the
    /// plan in force.
    pub(super) fn feed_while_moving_change(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Option<(Proposal, Reassignment)> {
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
            self.draw(&[], &[], idle, idle),
            self.draw(&[], &staying, budget, idle),
        ] {
            if drawn.hands == 0 {
                continue;
            }
            let Some(source) = Self::best_source(&falling, drawn.hands, &[]) else {
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
        let (drawn, source, after, progress, change) = best?;
        let existing = Self::workers_on(band, &source.key);
        let mut commands = self.reduction_commands(band, &drawn);
        commands.push(self.assign(band, &source.key, existing + drawn.hands));
        let proposal = Proposal {
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
        };
        Some((proposal, change))
    }

    /// The rule as a proposal alone — what the tests drive; `propose` goes through
    /// [`Food::split_to_feed_change`] to carry the change forward.
    #[cfg(test)]
    pub fn split_to_feed(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        self.split_to_feed_change(view, plan, memory, band, carried)
            .map(|(proposal, _)| proposal)
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
    ///
    /// Answers the change it priced beside the proposal, so the last rule can project the
    /// plan in force.
    pub(super) fn split_to_feed_change(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<(Proposal, Reassignment)> {
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
        let drawn = self.draw(&Self::surplus_rows(band), &rows, crew, band.idle_workers);
        let change = Reassignment {
            income_lost: drawn.income_lost,
            income_gained: take,
            payoff_turn: travel,
        };
        let after_split = project_all(&book, &[*carried, change], horizon);
        let target = Tile::new(patch.x, patch.y);
        let proposal = Proposal {
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
        };
        Some((proposal, change))
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

    /// The rule as a proposal alone — what the tests drive; `propose` goes through
    /// [`Food::spare_hands_into_hunts_change`] to carry the change forward.
    #[cfg(test)]
    pub fn spare_hands_into_hunts(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        self.spare_hands_into_hunts_change(view, plan, memory, band, carried)
            .map(|(proposal, _)| proposal)
    }

    /// **Rule 4 — spare hands into hunts.** Projected net income (after rule 1's change) at the
    /// goal or within `near_positive_fraction` of it, and a live huntable herd in reach: the
    /// surplus — the most hands whose leaving the lowest-paying forage rows keeps the projected net
    /// at or above the goal, the herd's take counted — goes onto that herd, which is what opens
    /// penning. The projection must survive. Idle hands are not surplus; they are rule 1's.
    ///
    /// Answers the change it priced beside the proposal, so the last rule can project the
    /// plan in force.
    pub(super) fn spare_hands_into_hunts_change(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<(Proposal, Reassignment)> {
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
        let forage_surplus: Vec<&LaborAssignmentState> = Self::surplus_rows(band)
            .into_iter()
            .filter(|row| row.kind == ROLE_FORAGE)
            .collect();
        let mut best: Option<(Drawn, &Source, Projection, Reassignment)> = None;
        for hands in 1..=budget {
            // Off the rows only — their surplus first, then the lowest-paying: the idle hands are
            // *negative income*'s to place, and a hunt drawn from them would compete with that
            // assignment for the band's one order.
            let drawn = self.draw(&forage_surplus, &forage, hands, 0);
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
            best = Some((drawn, herd, projection, change));
        }
        let (drawn, herd, projection, change) = best?;
        let existing = Self::workers_on(band, &herd.key);
        let mut commands = self.reduction_commands(band, &drawn);
        commands.push(self.assign(band, &herd.key, existing + drawn.hands));
        let proposal = Proposal {
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
        };
        Some((proposal, change))
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

    /// The rule as a proposal alone — what the tests drive; `propose` goes through
    /// [`Food::upgrade_the_ground_change`] to carry the change forward.
    #[cfg(test)]
    pub fn upgrade_the_ground(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        self.upgrade_the_ground_change(view, plan, memory, band, carried)
            .map(|(proposal, _)| proposal)
    }

    /// **Rule 5 — upgrade the ground.** The goal rung above wild, its gate knowledge known, and
    /// a worked forage patch below it with nothing queued: declare the climb and staff the
    /// smallest `builders` pool whose projection survives and pays off inside the horizon. The
    /// builders are drawn from the idle hands, then every row's surplus — **the patch's own row
    /// included**, down to its `workers_needed`, which is what keeps the declaration attached —
    /// then the hunt rows, then the lowest forage rows. The free hands (idle and surplus) all go,
    /// since they cost nothing where they stand; the crew grows past them only as far as the
    /// projection survives.
    ///
    /// Answers the change it priced beside the proposal, so the last rule can project the
    /// plan in force.
    pub(super) fn upgrade_the_ground_change(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<(Proposal, Reassignment)> {
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
        let surplus_rows = Self::surplus_rows(band);
        let free = self.draw(&surplus_rows, &[], budget, idle);
        let mut best: Option<(Proposal, f32, Reassignment)> = None;
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
            for hands in free.hands.max(1)..=budget {
                let drawn = self.draw(&surplus_rows, &pool, hands, idle);
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
                if best.as_ref().is_none_or(|(_, held, _)| progress > *held) {
                    best = Some((proposal, progress, change));
                }
                break;
            }
        }
        best.map(|(proposal, _, change)| (proposal, change))
    }

    /// **What `Food` asks the board for when `band`'s outfitting window is open** (`board.rs`;
    /// `docs/plan_ai_driver.md` §4, *"the board's first customer is outfitting"*): `gathering`
    /// kits for the hands **worth more on a basket than on a spear** ([`Food::outfit_split`],
    /// the walk by value), at [`DEMAND_PRIORITY_GATHERING`]; a hunting kit for every other
    /// hand when a huntable herd
    /// within `hunt_reach` can be brought down with it ([`Food::hunting_kit_for`]), at
    /// [`DEMAND_PRIORITY_HUNTING`]; and when no herd in reach clears any kit, those hands ask for
    /// baskets too — a spare basket is not forfeited budget, an unspent slot is. A window that
    /// is not open asks for nothing.
    pub fn outfit_demands(
        &self,
        view: &SeatView,
        _plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
    ) -> Vec<Demand> {
        if !band
            .loadout_window
            .as_ref()
            .is_some_and(|window| window.open)
        {
            return Vec::new();
        }
        let tick = view.tick();
        let is_dead = |key: &SourceKey, forecast: f32| self.is_dead(memory, band, key, forecast);
        let horizon = self.floors.projection_horizon_turns;
        let (gathering, spare) = self.outfit_split(view, memory, band, &is_dead, horizon);
        let demand = |resource: Resource, amount: u32, priority: f32| Demand {
            requester: SPECIALIST_FOOD,
            band: band.band_id,
            resource,
            amount,
            by_tick: tick,
            priority,
        };
        let hunting = (spare > 0)
            .then(|| Self::hunting_kit_for(view, band))
            .flatten();
        let gathering = match hunting {
            Some(_) => gathering,
            None => gathering + spare,
        };
        let mut demands = Vec::new();
        if gathering > 0 {
            demands.push(demand(
                Resource::Kit(GATHERING_KIT_ID.to_owned()),
                gathering,
                DEMAND_PRIORITY_GATHERING,
            ));
        }
        if let Some(kit) = hunting {
            demands.push(demand(Resource::Kit(kit), spare, DEMAND_PRIORITY_HUNTING));
        }
        demands
    }

    /// **The basket / spear split, by value** (the outfitting reading that shipped): the band's
    /// hands are walked one at a time, each going to a basket while the cluster's best marginal
    /// take over `food.projection_horizon_turns` is at least the best herd's marginal take at the
    /// Best floor, and to the hunting kit otherwise. A site's `k`-th hand earns its share of the
    /// site's sustained regrowth at Best (`regrowth_at(BEST_FLOOR) × provisions_per_biomass ×
    /// horizon / sustained_hands`) while `k ≤ sustained_hands`, plus what it carries of the room
    /// above the floor that the hands before it cannot (`min(per_worker_biomass × horizon, room
    /// left) × provisions_per_biomass`) — the fresh plateau first, the sustained after. A herd's
    /// `j`-th hunter earns its share of the herd's sustained take at Best while `j ≤ ` the crew
    /// that carries it (`ceil(sustained biomass / per_worker_biomass)`, at least one), and
    /// nothing past it: at the Best floor the take is the regrowth whatever the crew.
    ///
    /// **The herd side is priced off the wire, for an unworked herd**:
    /// `HerdTelemetryState::regrowth_samples` — *"This herd's own per-turn regrowth, in biomass,
    /// sampled at evenly spaced fractions of `K` … Sample `i` of `n` is the delta at `B = i/(n−1)
    /// × K`"* — and `per_worker_biomass` — *"What ONE hunter moves this turn, in BIOMASS … It is
    /// what turns a ceiling into a crew count"*; the row-level `sustainable_yield` a worked hunt
    /// row shows is *"the herd's net regrowth"*, the same quantity. Ties go to the basket: on the
    /// bench's ground a deer herd's regrowth is one hunter's work, so one spear opens the hunting
    /// web and the rest of the band gathers — `gathering 17` fed seed 23 to t60 where sizing the
    /// baskets at the sustained plateau alone (`gathering 2, big_game 15`) starved it.
    fn outfit_split(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        is_dead: &super::sources::IsDead<'_>,
        horizon: u32,
    ) -> (u32, u32) {
        let horizon = horizon as f32;
        // Per site: the sustained take a hand earns, the hands that share it, the room above the
        // floor in biomass, what one hand carries over the horizon, and provisions per biomass.
        struct Site {
            sustained_per_hand: f32,
            sustained_hands: u32,
            room: f32,
            carry: f32,
            provisions_per_biomass: f32,
            hands: u32,
        }
        let mut sites: Vec<Site> = cluster_sites(view, memory, band, band_tile(band), is_dead)
            .into_iter()
            .map(|(patch, rate)| {
                let hands = sustained_hands(patch, rate);
                let sustained = regrowth_at(&patch.regrowth_samples, BEST_FLOOR)
                    * patch.provisions_per_biomass
                    * horizon;
                Site {
                    sustained_per_hand: if hands > 0 {
                        sustained / hands as f32
                    } else {
                        0.0
                    },
                    sustained_hands: hands,
                    room: (patch.biomass - BEST_FLOOR * patch.carrying_capacity).max(0.0),
                    carry: patch.per_worker_biomass * horizon,
                    provisions_per_biomass: patch.provisions_per_biomass,
                    hands: 0,
                }
            })
            .collect();
        let site_marginal = |site: &Site| {
            let k = site.hands;
            let sustained = if k < site.sustained_hands {
                site.sustained_per_hand
            } else {
                0.0
            };
            let room_left = (site.room - k as f32 * site.carry).max(0.0);
            sustained + site.carry.min(room_left) * site.provisions_per_biomass
        };
        // The best herd a kit clears: its sustained take at Best, and the crew that carries it.
        let herd = Self::best_herd_for(view, band).map(|herd| {
            let sustained_biomass = regrowth_at(&herd.regrowth_samples, BEST_FLOOR).max(0.0);
            let crew = if herd.per_worker_biomass > 0.0 {
                ((sustained_biomass / herd.per_worker_biomass).ceil() as u32).max(1)
            } else {
                1
            };
            (
                sustained_biomass * herd.provisions_per_biomass * horizon / crew as f32,
                crew,
            )
        });
        let mut gathering = 0;
        let mut hunters = 0;
        for _ in 0..band.working_age {
            let best_site = sites
                .iter()
                .enumerate()
                .map(|(index, site)| (index, site_marginal(site)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let basket = best_site.map_or(0.0, |(_, value)| value);
            let spear = herd
                .filter(|(_, crew)| hunters < *crew)
                .map_or(0.0, |(value, _)| value);
            if basket >= spear {
                gathering += 1;
                if let Some((index, _)) = best_site {
                    sites[index].hands += 1;
                }
            } else {
                hunters += 1;
            }
        }
        (gathering, hunters)
    }

    /// The huntable herd within `hunt_reach` a roster kit clears, with the greatest sustained
    /// take at Best — what the outfit walk prices a spear against.
    fn best_herd_for<'v>(
        view: &'v SeatView,
        band: &PopulationCohortState,
    ) -> Option<&'v HerdTelemetryState> {
        let grid = view.grid();
        let here = band_tile(band);
        view.snapshot
            .herds
            .iter()
            .filter(|herd| herd.huntable)
            .filter(|herd| grid.distance(here, Tile::new(herd.x, herd.y)) <= band.hunt_reach)
            .filter(|herd| Self::kit_clearing(view, herd).is_some())
            .max_by(|a, b| {
                let take = |herd: &HerdTelemetryState| {
                    regrowth_at(&herd.regrowth_samples, BEST_FLOOR) * herd.provisions_per_biomass
                };
                take(a).total_cmp(&take(b)).then_with(|| b.id.cmp(&a.id))
            })
    }

    /// The roster's hunt-job kit (never `none`) with the greatest fresh `attack` whose mass
    /// window admits `herd` and clears its `defense`.
    fn kit_clearing<'v>(
        view: &'v SeatView,
        herd: &HerdTelemetryState,
    ) -> Option<&'v KitOptionState> {
        view.snapshot
            .kits
            .iter()
            .filter(|kit| kit.id != BARE_KIT_ID && kit.jobs.iter().any(|job| job == ROLE_HUNT))
            .filter(|kit| Self::kit_admits(kit, herd.body_mass) && kit.attack > herd.defense)
            .max_by(|a, b| a.attack.total_cmp(&b.attack).then_with(|| b.id.cmp(&a.id)))
    }

    /// Whether `kit`'s attack applies to an animal of `body_mass`: within its mass window, `0`
    /// on either end being unbounded; a herd whose mass is not on the wire is trusted only to a
    /// kit with no upper bound.
    fn kit_admits(kit: &KitOptionState, body_mass: f32) -> bool {
        if body_mass == BODY_MASS_UNKNOWN {
            return kit.attack_max_body_mass == MASS_UNBOUNDED;
        }
        (kit.attack_max_body_mass == MASS_UNBOUNDED || body_mass <= kit.attack_max_body_mass)
            && (kit.attack_min_body_mass == MASS_UNBOUNDED || body_mass >= kit.attack_min_body_mass)
    }

    /// **The hunting kit to ask for**: among the roster's hunt-job kits (never the item-less
    /// `none`), the one with the greatest fresh `attack` whose mass window admits the biggest
    /// huntable herd within `hunt_reach` and whose attack clears that herd's `defense` — the
    /// gate is `max(0, attack − defense)`, and *"below a species' `defense` that species cannot
    /// be hunted at all"*. Herds are tried biggest first, so a kit that cannot take the mammoth
    /// may still be asked for the deer. `None` when no herd in reach clears any kit.
    fn hunting_kit_for(view: &SeatView, band: &PopulationCohortState) -> Option<String> {
        let grid = view.grid();
        let here = band_tile(band);
        let mut herds: Vec<&HerdTelemetryState> = view
            .snapshot
            .herds
            .iter()
            .filter(|herd| herd.huntable)
            .filter(|herd| grid.distance(here, Tile::new(herd.x, herd.y)) <= band.hunt_reach)
            .collect();
        herds.sort_by(|a, b| b.body_mass.total_cmp(&a.body_mass));
        herds
            .into_iter()
            .find_map(|herd| Self::kit_clearing(view, herd).map(|kit| kit.id.clone()))
    }

    #[cfg(test)]
    pub fn hold_the_ground(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<Proposal> {
        self.hold_the_ground_change(view, plan, memory, band, carried)
            .map(|(proposal, _)| proposal)
    }

    /// **Rule 5a — hold the ground.** A patch the seat owns whose `upkeep` row reads
    /// `upkeep_shortfall > 0` — the standing-upkeep bill for holding its rung, unpaid — gets
    /// `assign_labor … agriculture <upkeep_workers_needed>` on the band that works it (the kit
    /// left `None`, so the wire derives `tillage`; the hoes are the board's business). The hands
    /// come from the surplus first, then the lowest rows, and the change is priced like any
    /// reassignment: what the hands earned where they stood against **the rung lost** — an
    /// unpaid bill costs the whole improvement. What holding keeps, as a series over the
    /// horizon: the rung's premium per turn (the tile's `tended_yield`, `field_yield` on a field,
    /// less the wild take the same hands make on that patch) **once the rung is complete**
    /// (`is_cultivated` / `is_field` — the bill runs during the build too, but a patch mid-build
    /// earns no premium yet), and the rebuild the seat would otherwise declare again — the work
    /// already done (`cultivation_work_done`, `field_work_done`; the full cost once complete) in
    /// builder-turns (`/ build_work_per_worker_turn`) at the row's own rate — **on the horizon's
    /// last turn**, where an avoided cost belongs: it raises the runway the goal is held against
    /// and never the trough. Put at the first turn instead it read as food in hand, the
    /// survival check lied, and band 4 on seed 23 gave its last forage hand to a hold at t25 and
    /// starved. Fires before *upgrade the ground*: holding what the band has beats declaring the
    /// next rung. The fact that forced the rule: seed 23's cultivate on 49,5 completed at t44 and
    /// read `cultivated: false, 0.99` at t45, decaying a hundredth a turn to 0.84 at t60, with
    /// the tile's `upkeep` row at `demand 1.92, supplied 0.0, shortfall 1.92, workers_needed 2,
    /// kit_id tillage` the whole way and two builders still on the `builders` role — the role the
    /// upkeep wants is `agriculture`, and it pays a patch's bill whether or not the band still
    /// works that patch (band 4 supplied 51,9 with its forage row empty).
    pub(super) fn hold_the_ground_change(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<(Proposal, Reassignment)> {
        let goals = plan.food_goals()?;
        if band.is_traveling {
            return None;
        }
        let budget = self.budget_workers(view, plan);
        if budget == 0 {
            return None;
        }
        let held = Self::workers_in_pool(band, ROLE_AGRICULTURE);
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let before = project(&book, carried, horizon);
        let idle = band.idle_workers.min(budget);
        let surplus_rows = Self::surplus_rows(band);
        let mut best: Option<(Proposal, Reassignment, f32)> = None;
        for row in band
            .labor_assignments
            .iter()
            .filter(|row| row.kind == ROLE_FORAGE && row.workers > 0)
        {
            let tile = Tile::new(row.target_x, row.target_y);
            let Some(patch) = view
                .patch_at(tile)
                .filter(|patch| patch.owner == Some(self.faction))
                .filter(|patch| patch.upkeep_shortfall > 0.0 && patch.upkeep_workers_needed > 0)
            else {
                continue;
            };
            let want = patch.upkeep_workers_needed.saturating_sub(held).min(budget);
            if want == 0 {
                continue;
            }
            let mut pool = Self::rows_ascending(band, ROLE_HUNT);
            pool.extend(Self::rows_ascending(band, ROLE_FORAGE));
            let drawn = self.draw(&surplus_rows, &pool, want, idle);
            if drawn.hands < want {
                continue;
            }
            let (rung_yield, work_done) = if patch.is_field {
                (patch.field_yield, patch.field_work_done)
            } else {
                (patch.tended_yield, patch.cultivation_work_done)
            };
            // The rung's premium over the wild take these hands would make on the patch — earned
            // only once the rung is complete.
            let complete = patch.is_cultivated || patch.is_field;
            let wild = crew_take(
                row.workers,
                patch_per_worker_yield(memory, band, patch),
                patch.biomass * patch.provisions_per_biomass,
            );
            let premium = if complete {
                (rung_yield - wild).max(0.0)
            } else {
                0.0
            };
            // The rebuild an unwound rung costs, once, at the horizon's end: the work already
            // done in builder-turns at the row's rate.
            let rebuild = if patch.build_work_per_worker_turn > 0.0 {
                work_done / patch.build_work_per_worker_turn * Self::row_rate(row)
            } else {
                0.0
            };
            let mut series = vec![premium; horizon as usize];
            if let Some(last) = series.last_mut() {
                *last += rebuild;
            }
            let change = Change::Series {
                income_lost: drawn.income_lost,
                income_gained: series,
            };
            let after = project_changes(&book, &[Change::Flat(*carried), change], horizon);
            let progress = goal_progress(&goals, &before, &after);
            if progress <= 0.0 || best.as_ref().is_some_and(|(_, _, held)| progress <= *held) {
                continue;
            }
            let mut commands = self.reduction_commands(band, &drawn);
            commands.push(self.assign_pool(band, ROLE_AGRICULTURE, held + want));
            let flat = Reassignment {
                income_lost: drawn.income_lost,
                income_gained: premium,
                payoff_turn: 0,
            };
            best = Some((
                Proposal {
                    commands,
                    intent: intent_key(SPECIALIST_FOOD, INTENT_HOLD, format!("{},{}", tile.x, tile.y)),
                    score: progress * self.weight,
                    cost: Cost {
                        workers: want,
                        bands: vec![band.band_id],
                    },
                    reason: format!(
                        "{REASON_HOLD_GROUND}: {want} hands on agriculture for {},{} short {:.2} [{}]",
                        tile.x,
                        tile.y,
                        patch.upkeep_shortfall,
                        ledger_note(&after)
                    ),
                    memo: None,
                },
                flat,
                progress,
            ));
        }
        best.map(|(proposal, change, _)| (proposal, change))
    }

    /// The patch under a forage `row` as the ledger models it, with the row's crew.
    fn patch_book(patch: &ForagePatchState, hands: u32) -> PatchBook {
        PatchBook {
            biomass: patch.biomass,
            capacity: patch.carrying_capacity,
            provisions_per_biomass: patch.provisions_per_biomass,
            regrowth_samples: patch.regrowth_samples.clone(),
            hands,
            per_worker_biomass: patch.per_worker_biomass,
        }
    }

    /// The change of setting `row`'s floor to `floor`: its take today drops out and the
    /// floor's series comes in ([`floor_income`]).
    fn floor_change(
        row: &LaborAssignmentState,
        patch: &ForagePatchState,
        floor: f32,
        horizon: u32,
    ) -> (Change, Option<u32>) {
        let income = floor_income(&Self::patch_book(patch, row.workers), floor, horizon);
        (
            Change::Series {
                income_lost: row.actual_yield,
                income_gained: income.per_turn,
            },
            income.spent_at,
        )
    }

    /// The floors *draw down to survive* may set, from a step under Best down to the profile's
    /// `survival_floor`, each rounded to the ladder's rung.
    fn floor_ladder(&self) -> Vec<f32> {
        let rungs = ((BEST_FLOOR - self.floors.survival_floor) / FLOOR_STEP).round() as u32;
        (1..=rungs)
            .map(|rung| ((BEST_FLOOR - rung as f32 * FLOOR_STEP) / FLOOR_STEP).round() * FLOOR_STEP)
            .collect()
    }

    /// **Rule 6 — draw down to survive.** The projection of the plan in force — the band's
    /// book plus the changes rules 1–5 proposed this turn (`in_force`) — troughs at or below
    /// zero: lower the harvest floor on a worked forage patch, to the **highest** floor from a
    /// step under Best down to `food.survival_floor` whose projection survives; none surviving,
    /// the floor whose projection has the **highest trough** — the latest, shallowest failure.
    /// *Survival outranks the peak* means "die last", never "strip the stand". Either way **a
    /// rung must buy a turn** ([`RUNG_MIN_GAIN_TURNS`]): a floor whose trough is not at least one
    /// turn of consumption above the plan in force's is not stepped to. And only when the
    /// ledger prices it as closing goal gap (`goal_progress > 0`, the guard every rule has): a
    /// crew already carrying less than the room above Best takes the same at any floor, so its
    /// series is the book it already has; stripping such a patch moves nothing but the learning
    /// rate (`plan_harvest_floor.md` §3: *"stripping teaches nothing"*), and on the first bench
    /// run it was proposed at −0.7 and accepted for want of a rival. The command is the row's
    /// own `assign_labor` with the floor stated and the same workers; the reason names the floor
    /// and the turn the patch is spent. Never below `survival_floor`; never on a hunt row (a
    /// hunt's floor is the herd's escapement, which this rule does not price).
    ///
    /// **Restore, with hysteresis.** A row below Best is put back to `floor: None` — under the
    /// same intent, reason `"floor back to best"` — only when the plan in force clears at its
    /// floor, still clears with the floor back at Best, **and** the band's `turns_of_food` is at
    /// or above `goals.runway_turns`, the runway the plan already asks `Food` for. Until then the
    /// drawn-down floor holds: without the third condition the seat alternated a drawdown and a
    /// restore on the same row every other turn (bench seed 23, t31–t40), because a Best-floor
    /// projection front-loads the room above Best just as the drawdown did.
    pub fn draw_down_to_survive(
        &self,
        view: &SeatView,
        plan: &Plan,
        _memory: &SeatMemory,
        band: &PopulationCohortState,
        in_force: &[Reassignment],
    ) -> Option<Proposal> {
        let goals = plan.food_goals()?;
        if band.is_traveling {
            return None;
        }
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let in_force: Vec<Change> = in_force.iter().copied().map(Change::Flat).collect();
        let before = project_changes(&book, &in_force, horizon);
        let rows: Vec<(&LaborAssignmentState, Tile, &ForagePatchState)> = band
            .labor_assignments
            .iter()
            .filter(|row| row.kind == ROLE_FORAGE && row.workers > 0)
            .filter_map(|row| {
                let tile = Tile::new(row.target_x, row.target_y);
                workable_patch_at(view, tile).map(|patch| (row, tile, patch))
            })
            .collect();
        let with = |change: Change| {
            let mut changes = in_force.clone();
            changes.push(change);
            project_changes(&book, &changes, horizon)
        };
        if survives(&before) {
            // Restore: a drawn-down row that would clear at Best goes back to Best — once the
            // band's runway has reached the goal.
            if band.turns_of_food < goals.runway_turns {
                return None;
            }
            return rows
                .iter()
                .filter(|(row, _, _)| row.floor < BEST_FLOOR - FLOOR_TOLERANCE)
                .find_map(|(row, tile, patch)| {
                    let (change, _) = Self::floor_change(row, patch, BEST_FLOOR, horizon);
                    let after = with(change);
                    survives(&after).then(|| Proposal {
                        commands: vec![self.assign_at_floor(band, *tile, row.workers, None)],
                        intent: intent_key(SPECIALIST_FOOD, INTENT_DRAWDOWN, band.band_id),
                        score: goal_progress(&goals, &before, &after) * self.weight,
                        cost: Cost {
                            workers: 0,
                            bands: vec![band.band_id],
                        },
                        reason: format!(
                            "{REASON_FLOOR_RESTORE}: {} [{}]",
                            SourceKey::Patch(*tile).describe(),
                            ledger_note(&after)
                        ),
                        memo: None,
                    })
                });
        }
        let ladder = self.floor_ladder();
        // A rung buys a turn or it is not stepped to.
        let worth_a_turn = |after: &Projection| {
            after.trough.0 - before.trough.0 >= RUNG_MIN_GAIN_TURNS * book.consumption
        };
        let mut best: Option<(Proposal, f32)> = None;
        for (row, tile, patch) in rows {
            // The highest floor under the row's own that survives; none surviving, the one whose
            // projection troughs highest — each only if it buys a turn.
            let mut chosen = None;
            let mut shallowest: Option<(f32, Option<u32>, Projection)> = None;
            for floor in ladder
                .iter()
                .copied()
                .filter(|floor| *floor < row.floor - FLOOR_TOLERANCE)
            {
                let (change, spent) = Self::floor_change(row, patch, floor, horizon);
                let after = with(change);
                if !worth_a_turn(&after) {
                    continue;
                }
                if survives(&after) {
                    chosen = Some((floor, spent, after));
                    break;
                }
                if shallowest
                    .as_ref()
                    .is_none_or(|(_, _, held)| after.trough.0 > held.trough.0)
                {
                    shallowest = Some((floor, spent, after));
                }
            }
            let Some((floor, spent, after)) = chosen.or(shallowest) else {
                continue;
            };
            let progress = goal_progress(&goals, &before, &after);
            if progress <= 0.0 || best.as_ref().is_some_and(|(_, held)| progress <= *held) {
                continue;
            }
            let spent = match spent {
                Some(turn) => format!("patch spent by t{turn}"),
                None => "patch never spent".to_owned(),
            };
            best = Some((
                Proposal {
                    commands: vec![self.assign_at_floor(band, tile, row.workers, Some(floor))],
                    intent: intent_key(SPECIALIST_FOOD, INTENT_DRAWDOWN, band.band_id),
                    score: progress * self.weight,
                    cost: Cost {
                        workers: 0,
                        bands: vec![band.band_id],
                    },
                    reason: format!(
                        "{REASON_DRAW_DOWN}: {} to floor {floor:.1}, {spent} [{}]",
                        SourceKey::Patch(tile).describe(),
                        ledger_note(&after)
                    ),
                    memo: None,
                },
                progress,
            ));
        }
        best.map(|(proposal, _)| proposal)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{
        a_view, food, goals, memory, own_band, plan_toward, plan_with_food_share, ARMED_ATTACK,
        BAND, BARE_ATTACK, FACTION, FORAGE_KIT, HERD_AT, HERD_ID, HERE, HUNT_KIT, NEAR_PATCH,
        RICH_PATCH, STOCK, TICK,
    };
    use super::*;
    use crate::orchestrator::Plan;
    use crate::specialists::Specialist;
    use sim_runtime::{
        CohortStoreState, HerdTelemetryState, IntensificationKnowledgeState,
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
            // Assigned with `floor: None`, the row reads the wire default: Best.
            floor: BEST_FLOOR,
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
    /// a band whose hunt-kit tier resolves to the bare hand is never sent to a herd, however rich;
    /// a tier above it and the herd is a source again. The reading is the band's `kit_tiers`, not
    /// its batches: a sled without a spear resolves to the bare hand and counts for nothing.
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
        let tier_of = |view: &mut SeatView, kit: &str, attack: f32| {
            for tier in &mut view.snapshot.populations[0].kit_tiers {
                if tier.kit_id == kit {
                    tier.attack = attack;
                }
            }
        };
        tier_of(&mut view, HUNT_KIT, BARE_ATTACK);
        assert_eq!(
            assigned_role(&view),
            ROLE_FORAGE,
            "the spear kit resolves to the bare hand: the herd is not"
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
        // A forage kit above the bare hand is not a hunting kit; no tiers published reads bare.
        tier_of(&mut view, FORAGE_KIT, ARMED_ATTACK);
        assert_eq!(
            assigned_role(&view),
            ROLE_FORAGE,
            "baskets are not a weapon"
        );
        tier_of(&mut view, HUNT_KIT, ARMED_ATTACK);
        assert_eq!(assigned_role(&view), ROLE_HUNT, "armed again");
        view.snapshot.populations[0].kit_tiers.clear();
        assert_eq!(
            assigned_role(&view),
            ROLE_FORAGE,
            "no tiers on the wire: nothing to hunt with"
        );
    }

    // ---- outfitting demands ---------------------------------------------------------------------

    /// The fixture with its window open, two sites in reach that at the Best floor's sustained
    /// regrowth pay 8 a turn over four hands and 6 over six (nothing to carry off the room: the
    /// fixture's patches move no biomass per hand), and the herd at `defense` regrowing 1.0 a
    /// turn at Best that one hunter carries: walked by value over the forager's 40-turn horizon
    /// the rich hands earn 80 each, the near hands 40, one spear 40 — ties to the basket — and
    /// every hand past them nothing; so 16 baskets and one stalking kit, else 17 baskets.
    fn a_band_at_its_window(defense: f32) -> SeatView {
        a_view_with(|view| {
            view.snapshot.populations[0].loadout_window =
                Some(sim_runtime::BandLoadoutWindowState {
                    open: true,
                    kit_budget: 17,
                    material_budget: 30,
                    ..Default::default()
                });
            for patch in &mut view.snapshot.forage_patches {
                let tile = Tile::new(patch.x, patch.y);
                if tile == RICH_PATCH {
                    patch.regrowth_samples = vec![0.0, 8.0, 8.0, 8.0, 8.0, 8.0];
                } else if tile == NEAR_PATCH {
                    patch.regrowth_samples = vec![0.0, 6.0, 6.0, 6.0, 6.0, 6.0];
                }
            }
            view.snapshot.herds[0].defense = defense;
            view.snapshot.herds[0].regrowth_samples = vec![-1.0, 0.5, 1.0, 1.0, 0.5, 0.0];
            view.snapshot.herds[0].per_worker_biomass = 40.0;
        })
    }

    #[test]
    fn an_open_window_asks_for_baskets_for_the_cluster_and_spears_for_the_rest() {
        let view = a_band_at_its_window(5.0);
        let demands = food().outfit_demands(
            &view,
            &plan_with_food_share(1.0),
            &memory(),
            own_band(&view),
        );
        let asked: Vec<(String, u32, f32)> = demands
            .iter()
            .map(|d| (d.resource.to_string(), d.amount, d.priority))
            .collect();
        assert_eq!(
            asked,
            vec![
                ("kit:gathering".to_owned(), 16, DEMAND_PRIORITY_GATHERING),
                (format!("kit:{HUNT_KIT}"), 1, DEMAND_PRIORITY_HUNTING),
            ]
        );
        assert!(demands.iter().all(|d| d.band == BAND && d.by_tick == TICK));
        // The herd too tough for a fresh spear: every hand asks for a basket.
        let tough = a_band_at_its_window(40.0);
        let demands = food().outfit_demands(
            &tough,
            &plan_with_food_share(1.0),
            &memory(),
            own_band(&tough),
        );
        assert_eq!(
            demands
                .iter()
                .map(|d| (d.resource.to_string(), d.amount))
                .collect::<Vec<_>>(),
            vec![("kit:gathering".to_owned(), 17)]
        );
        // A trapping kit bounded to small game is not asked for a heavy herd; the unbounded
        // stalking kit is, and a herd of unknown mass trusts only the unbounded one.
        let mut heavy = a_band_at_its_window(5.0);
        heavy.snapshot.kits.push(sim_runtime::KitOptionState {
            id: "trapping".to_owned(),
            jobs: vec![ROLE_HUNT.to_owned()],
            attack: ARMED_ATTACK * 2.0,
            attack_max_body_mass: 1.0,
            item_ids: vec!["traps".to_owned()],
            ..Default::default()
        });
        heavy.snapshot.herds[0].body_mass = 300.0;
        let demands = food().outfit_demands(
            &heavy,
            &plan_with_food_share(1.0),
            &memory(),
            own_band(&heavy),
        );
        assert_eq!(demands[1].resource.to_string(), format!("kit:{HUNT_KIT}"));
        // No window: nothing asked.
        let mut closed = a_band_at_its_window(5.0);
        closed.snapshot.populations[0].loadout_window = None;
        assert!(food()
            .outfit_demands(
                &closed,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&closed)
            )
            .is_empty());
    }

    // ---- rule 5a: hold the ground ------------------------------------------------------------

    /// The parked band's rich patch is the seat's own, its upkeep row unpaid (`shortfall 1.92,
    /// workers_needed 2`): two of the nine surplus hands go to `agriculture`, the row cut to
    /// fifteen; with nothing short, nothing is proposed.
    #[test]
    fn an_owned_patch_short_of_upkeep_gets_its_agriculture_hands() {
        let held = |shortfall: f32| {
            a_view_with(|view| {
                let band = &mut view.snapshot.populations[0];
                band.idle_workers = 0;
                band.food_income = band.food_consumption;
                band.labor_assignments = vec![LaborAssignmentState {
                    workers_needed: 8,
                    ..forage_row(RICH_PATCH, 17, 26.0)
                }];
                for patch in &mut view.snapshot.forage_patches {
                    if Tile::new(patch.x, patch.y) == RICH_PATCH {
                        patch.owner = Some(FACTION);
                        patch.is_cultivated = true;
                        patch.tended_yield = 40.0;
                        patch.cultivation_work_cost = 50.0;
                        patch.cultivation_work_done = 50.0;
                        patch.build_work_per_worker_turn = 1.0;
                        patch.upkeep_demand = 1.92;
                        patch.upkeep_shortfall = shortfall;
                        patch.upkeep_workers_needed = 2;
                    }
                }
            })
        };
        let view = held(1.92);
        let proposal = food()
            .hold_the_ground(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("the upkeep is paid");
        assert_eq!(proposal.intent, "food:hold:2,3");
        assert!(
            proposal
                .reason
                .starts_with("hold the ground: 2 hands on agriculture for 2,3 short 1.92 ["),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_FORAGE.to_owned(), 15, Some(RICH_PATCH), None),
            "two surplus hands leave the row"
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_AGRICULTURE.to_owned(), 2, None, None)
        );
        assert_eq!(proposal.cost.workers, 2);
        assert!(proposal.score > 0.0);
        // Priced as the rung lost — 6 a turn of premium over the horizon plus the 50-work rebuild
        // at the row's rate on the last turn — the hold outscores the one-hand assignment the
        // same band could make instead (a hand onto the near patch at 0.5, half the net-income
        // goal; once a change meets both goals every change reads the same, so the assignment
        // must fall short of the goal for the two to be told apart).
        let mut one_surplus = held(1.92);
        one_surplus.snapshot.populations[0].labor_assignments[0].workers_needed = 16;
        one_surplus.snapshot.herds.clear();
        for patch in &mut one_surplus.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == NEAR_PATCH {
                patch.per_worker_yield = 0.5;
            }
        }
        let hold = food()
            .hold_the_ground(
                &one_surplus,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&one_surplus),
                &Reassignment::NONE,
            )
            .expect("the hold");
        let assign = food()
            .negative_income(
                &one_surplus,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&one_surplus),
            )
            .expect("one surplus hand moves");
        assert_eq!(assign.cost.workers, 1);
        assert!(
            hold.score > assign.score,
            "hold {} vs assign {}",
            hold.score,
            assign.score
        );
        let paid = held(0.0);
        assert!(food()
            .hold_the_ground(
                &paid,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&paid),
                &Reassignment::NONE,
            )
            .is_none());
        // A rung still being built earns no premium and has little work to lose: the bill runs,
        // but with no surplus to spare, two hands off a row paying 1.5 each for a 4-work rebuild
        // at the horizon's end is a change the ledger prices as a loss, and nothing is proposed.
        let mut building = held(0.08);
        building.snapshot.populations[0].labor_assignments[0].workers_needed = 17;
        for patch in &mut building.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.is_cultivated = false;
                patch.cultivation_work_done = 4.0;
            }
        }
        assert!(food()
            .hold_the_ground(
                &building,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&building),
                &Reassignment::NONE,
            )
            .is_none());
    }

    /// A take above a fresh patch's regrowth is the room above the floor being taken, not
    /// overuse: the row is not the one to empty first. At the floor it is.
    #[test]
    fn a_patch_above_its_floor_is_never_overused() {
        let mut view = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.labor_assignments = vec![LaborAssignmentState {
                sustainable_yield: 6.0,
                ..forage_row(RICH_PATCH, 17, 26.0)
            }];
        });
        // The fixture's rich patch stands at 1.5 × K, well above the Best floor.
        let (_, _, troubled, why) = food()
            .row_to_empty(&view, &memory(), own_band(&view))
            .expect("one worked row");
        assert!(!troubled, "{why}");
        assert_eq!(why, WHY_LOWEST_ROW);
        for patch in &mut view.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.biomass = BEST_FLOOR * patch.carrying_capacity;
            }
        }
        let (_, _, troubled, why) = food()
            .row_to_empty(&view, &memory(), own_band(&view))
            .expect("one worked row");
        assert!(troubled);
        assert_eq!(why, WHY_OVERUSED);
    }

    // ---- rule 6: draw down to survive ---------------------------------------------------------

    /// A band of seventeen on the rich patch, breaking even at Best on a fresh stand of 40 that
    /// regrows a flat 6 a turn, two in the larder. `stock` and the row's floor are the test's.
    fn a_band_on_a_fresh_stand(stock: f32, floor: f32) -> SeatView {
        a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = 6.0;
            band.food_consumption = 6.0;
            band.stores = vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (stock * FIXED_POINT_SCALE as f32) as i64,
            }];
            band.labor_assignments = vec![LaborAssignmentState {
                floor,
                ..forage_row(RICH_PATCH, 17, 6.0)
            }];
            for patch in &mut view.snapshot.forage_patches {
                if Tile::new(patch.x, patch.y) == RICH_PATCH {
                    patch.biomass = 40.0;
                    patch.carrying_capacity = 40.0;
                    patch.provisions_per_biomass = 1.0;
                    patch.per_worker_biomass = 1.0;
                    patch.regrowth_samples = vec![0.0, 6.0, 6.0, 6.0, 6.0, 6.0];
                }
            }
        })
    }

    /// An upgrade in force takes 4 a turn off the book for five turns: with two in the larder the
    /// plan troughs at −18. At 0.4 the crew front-loads 17 then 13 and the stock touches zero on
    /// t4; at 0.3 it front-loads 17 and 17 and the trough is 4 — the highest floor that survives.
    /// A drain nothing bridges, and a crew the floor cannot move, propose nothing.
    #[test]
    fn a_starving_band_draws_its_patch_down_to_the_highest_floor_that_survives() {
        let view = a_band_on_a_fresh_stand(2.0, BEST_FLOOR);
        let build = Reassignment {
            income_lost: 4.0,
            income_gained: 6.0,
            payoff_turn: 5,
        };
        let proposal = food()
            .draw_down_to_survive(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &[build],
            )
            .expect("the plan in force troughs below zero");
        assert_eq!(proposal.intent, "food:drawdown:7001");
        assert_eq!(
            proposal.reason,
            "draw down to survive: forage 2,3 to floor 0.3, patch spent by t1 [ledger: trough 4.0 at t4, positive again t0]"
        );
        match &proposal.commands[0] {
            CommandPayload::AssignLabor {
                role,
                workers,
                target_x,
                target_y,
                floor,
                ..
            } => {
                assert_eq!(
                    (role.as_str(), *workers, *target_x, *target_y),
                    (ROLE_FORAGE, 17, Some(2), Some(3))
                );
                assert!(
                    floor.is_some_and(|floor| (floor - 0.3).abs() < 1e-6),
                    "{floor:?}"
                );
            }
            other => panic!("not an assignment: {other:?}"),
        }
        assert_eq!(proposal.cost.workers, 0);
        assert!(proposal.score > 0.0);
        // A build whose dip no floor bridges: the floor whose projection troughs highest — 0.2,
        // front-loading 17, 17 and 10 to trough at −4 on t7 — is the latest, shallowest failure,
        // and it still closes gap (the stand's regrowth is what it was, plus the front-load).
        let long_build = Reassignment {
            income_lost: 4.0,
            income_gained: 6.0,
            payoff_turn: 8,
        };
        let proposal = food()
            .draw_down_to_survive(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &[long_build],
            )
            .expect("die last");
        assert_eq!(
            proposal.reason,
            "draw down to survive: forage 2,3 to floor 0.2, patch spent by t2 [ledger: trough -4.0 at t7, positive again t0]"
        );
        // A rung must buy a turn. A row already at 0.4 on a stand 5 above its floor: the next
        // rung, 0.3, frees 9 once — 3 more than the row takes today, against the 6 a turn the
        // band eats — a hair, and it is not stepped to; 0.2 frees 13, buys a turn, and is.
        let mut hair = a_band_on_a_fresh_stand(2.0, 0.4);
        for patch in &mut hair.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.biomass = 0.4 * 40.0 + 5.0;
            }
        }
        let proposal = food()
            .draw_down_to_survive(
                &hair,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&hair),
                &[build],
            )
            .expect("the rung that buys a turn");
        assert!(
            proposal.reason.contains("to floor 0.2,"),
            "the hair at 0.3 is skipped: {}",
            proposal.reason
        );
        // A drain nothing bridges: the band dies whatever the floor, and latest at 0.2 — the
        // trough is −323 at the horizon's end against −358 untouched, a hair of goal gap closed —
        // so 0.2 is proposed, never the stripped stand (−545, the worst of them).
        let drain = Reassignment {
            income_lost: 9.0,
            income_gained: 0.0,
            payoff_turn: 0,
        };
        let proposal = food()
            .draw_down_to_survive(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &[drain],
            )
            .expect("die last");
        assert!(
            proposal.reason.contains("to floor 0.2,")
                && proposal.reason.contains("trough -332.0 at t39"),
            "{}",
            proposal.reason
        );
        // A crew carrying less than the room above Best takes the same at any floor: the series
        // is the book it already has, the ledger prices no gain, and nothing is proposed — a
        // stripped patch would only stop the band learning.
        let mut crew_bound = a_band_on_a_fresh_stand(2.0, BEST_FLOOR);
        for patch in &mut crew_bound.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                // Seventeen hands carry 6 a turn — exactly the row's take today.
                patch.per_worker_biomass = 6.0 / 17.0;
            }
        }
        assert!(food()
            .draw_down_to_survive(
                &crew_bound,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&crew_bound),
                &[build],
            )
            .is_none());
    }

    /// The plan in force survives at Best: nothing to draw down, and nothing to restore.
    #[test]
    fn a_band_whose_plan_survives_at_best_proposes_no_drawdown() {
        let view = a_band_on_a_fresh_stand(STOCK, BEST_FLOOR);
        assert!(food()
            .draw_down_to_survive(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &[Reassignment::NONE],
            )
            .is_none());
    }

    /// A rule re-issuing a worked row carries the row's floor: the free hands leaving a row
    /// drawn down to 0.2 leave it at 0.2, not at the wire default that would undo the drawdown;
    /// a row the band does not work yet takes the default.
    #[test]
    fn a_reassignment_of_a_worked_row_keeps_its_floor() {
        let view = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = band.food_consumption;
            band.labor_assignments = vec![LaborAssignmentState {
                workers_needed: 8,
                floor: 0.2,
                ..forage_row(RICH_PATCH, 17, 26.0)
            }];
        });
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("nine surplus hands move");
        let floors: Vec<(Option<Tile>, Option<f32>)> = proposal
            .commands
            .iter()
            .map(|command| match command {
                CommandPayload::AssignLabor {
                    target_x,
                    target_y,
                    floor,
                    ..
                } => (
                    target_x.zip(*target_y).map(|(x, y)| Tile::new(x, y)),
                    *floor,
                ),
                other => panic!("not an assignment: {other:?}"),
            })
            .collect();
        assert_eq!(
            floors,
            vec![(Some(RICH_PATCH), Some(0.2)), (Some(NEAR_PATCH), None)]
        );
    }

    /// A row drawn down to 0.2 on a stand that has recovered to its capacity: the projection
    /// clears at its floor and still clears with the floor back at Best, and the band's runway
    /// (14 turns) is past the forager's goal of 12, so the row is put back. At a runway of 3 the
    /// same band holds its floor.
    #[test]
    fn a_drawn_down_row_that_now_survives_at_best_is_restored() {
        let mut short = a_band_on_a_fresh_stand(STOCK, 0.2);
        short.snapshot.populations[0].turns_of_food = 3.0;
        assert!(
            food()
                .draw_down_to_survive(
                    &short,
                    &plan_with_food_share(1.0),
                    &memory(),
                    own_band(&short),
                    &[Reassignment::NONE],
                )
                .is_none(),
            "the runway is under the goal: the floor holds"
        );
        let view = a_band_on_a_fresh_stand(STOCK, 0.2);
        assert!(own_band(&view).turns_of_food >= goals().runway_turns);
        let proposal = food()
            .draw_down_to_survive(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &[Reassignment::NONE],
            )
            .expect("the restore");
        assert_eq!(proposal.intent, "food:drawdown:7001");
        assert!(
            proposal
                .reason
                .starts_with("floor back to best: forage 2,3 [ledger:"),
            "{}",
            proposal.reason
        );
        assert!(matches!(
            &proposal.commands[0],
            CommandPayload::AssignLabor {
                workers: 17,
                floor: None,
                ..
            }
        ));
        // Still starving at Best (nothing in the larder): the row stays drawn down.
        let starving = a_band_on_a_fresh_stand(0.0, 0.2);
        assert!(food()
            .draw_down_to_survive(
                &starving,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&starving),
                &[Reassignment {
                    income_lost: 9.0,
                    income_gained: 0.0,
                    payoff_turn: 0
                }],
            )
            .is_none_or(|proposal| !proposal.reason.starts_with(REASON_FLOOR_RESTORE)));
    }

    // ---- surplus hands --------------------------------------------------------------------------

    /// `workers − workers_needed`; `0` on a row whose `workers_needed` is `0`, fresh or barren.
    #[test]
    fn surplus_is_the_hands_the_take_did_not_need_and_a_fresh_row_has_none() {
        let mut row = forage_row(NEAR_PATCH, 17, 8.0);
        row.workers_needed = 8;
        assert_eq!(surplus_hands(&row), 9);
        row.workers_needed = 17;
        assert_eq!(surplus_hands(&row), 0);
        // Fresh: nothing resolved yet.
        assert_eq!(surplus_hands(&forage_row(NEAR_PATCH, 5, 0.0)), 0);
        // Produced nothing: the sim's `0`, and not a surplus either.
        let mut barren = hunt_row(12, 0.0, 0);
        barren.workers_needed = 0;
        assert_eq!(surplus_hands(&barren), 0);
    }

    /// Seventeen hands parked on the rich patch, which needed eight of them: nine are surplus.
    /// Breaking even, the larder full, cultivation known and the patch priced for it.
    fn a_parked_band() -> SeatView {
        a_view_with(|view| {
            knowledge(view, 1.0, 0.0);
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = band.food_consumption;
            band.labor_assignments = vec![LaborAssignmentState {
                workers_needed: 8,
                ..forage_row(RICH_PATCH, 17, 26.0)
            }];
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

    /// The nine surplus hands are the builders — off the patch's own row, which keeps its eight
    /// and so its declaration — and all nine go, since they cost nothing where they stand.
    #[test]
    fn the_surplus_on_the_patchs_own_row_builds_its_upgrade() {
        let view = a_parked_band();
        let proposal = food()
            .upgrade_the_ground(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("nine free builders");
        assert!(
            proposal
                .reason
                .contains("cultivate 2,3 (hazel) with 9 builders, payoff turn 6"),
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
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 8, Some(RICH_PATCH), None),
            "the row keeps what its take needs"
        );
        assert_eq!(
            assigned_to(&proposal.commands[2]),
            (ROLE_BUILDERS.to_owned(), 9, None, None)
        );
        assert_eq!(proposal.cost.workers, 9);
    }

    /// §4: *"a band standing in a cluster spreads over it"*. Three sites in reach with plateaus
    /// of 8, 6 and 3 hands: seventeen idle hands are dealt 8/6/3, the best rate first — not 17
    /// onto the richest, which would leave nine of them past its plateau.
    #[test]
    fn free_hands_are_dealt_across_the_cluster_up_to_each_sites_plateau() {
        let third = Tile::new(2, 1);
        let view = a_view_with(|view| {
            for patch in &mut view.snapshot.forage_patches {
                let tile = Tile::new(patch.x, patch.y);
                // The ceiling is `biomass × provisions_per_biomass`; the plateau `ceil(ceiling / rate)`.
                if tile == RICH_PATCH {
                    patch.biomass = 16.0;
                } else if tile == NEAR_PATCH {
                    patch.biomass = 6.0;
                }
            }
            view.snapshot.forage_patches.push(ForagePatchState {
                x: third.x,
                y: third.y,
                owner: None,
                per_worker_yield: 0.5,
                carrying_capacity: 1.0,
                biomass: 1.5,
                provisions_per_biomass: 1.0,
                ..Default::default()
            });
            view.snapshot
                .food_modules
                .push(sim_runtime::FoodModuleState {
                    x: third.x,
                    y: third.y,
                    ..view.snapshot.food_modules[0].clone()
                });
            assert_eq!(view.grid().distance(HERE, third), 1);
        });
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("a proposal");
        assert!(
            proposal
                .reason
                .starts_with(&format!(
                    "{REASON_NEGATIVE_INCOME}: 17 idle hands -> forage 2,3 ×8, forage 4,2 ×6, forage 2,1 ×3 ["
                )),
            "{}",
            proposal.reason
        );
        let dealt: Vec<(u32, Option<Tile>)> = proposal
            .commands
            .iter()
            .map(|command| {
                let (_, workers, tile, _) = assigned_to(command);
                (workers, tile)
            })
            .collect();
        assert_eq!(
            dealt,
            vec![
                (8, Some(RICH_PATCH)),
                (6, Some(NEAR_PATCH)),
                (3, Some(third))
            ]
        );
        assert_eq!(proposal.cost.workers, 17);
        // 8 × 2.0 + 6 × 1.0 + 3 × 0.5 = 23.5 a turn, against 16 for all seventeen on the rich patch.
        assert!(
            proposal.reason.contains("positive again t0"),
            "{}",
            proposal.reason
        );
    }

    /// A hand that would read surplus where it lands stays where it is: seventeen on a site
    /// reading `workers_needed 8`, the second site already at the crew the frame says it needs —
    /// nothing moves, and the same with the second site at its ceiling. (The near row pays its
    /// hands more than the rich one does, so rule 1's row-to-empty path has nowhere better to
    /// send the rich row either, and the free-hand path is what is pinned.)
    #[test]
    fn a_free_hand_does_not_move_onto_a_site_that_cannot_use_it() {
        let parked = |near: LaborAssignmentState| {
            a_view_with(|view| {
                view.snapshot.herds.clear();
                let band = &mut view.snapshot.populations[0];
                band.idle_workers = 0;
                band.food_income = band.food_consumption;
                band.labor_assignments = vec![
                    LaborAssignmentState {
                        workers_needed: 8,
                        ..forage_row(RICH_PATCH, 17, 26.0)
                    },
                    near,
                ];
            })
        };
        // The near row reads full: six hands, six needed.
        let full = parked(LaborAssignmentState {
            workers_needed: 6,
            ..forage_row(NEAR_PATCH, 6, 12.0)
        });
        assert!(food()
            .negative_income(
                &full,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&full)
            )
            .is_none());
        // The near row at its ceiling — the model's own plateau — with no frame reading yet.
        let mut at_ceiling = parked(forage_row(NEAR_PATCH, 6, 12.0));
        for patch in &mut at_ceiling.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == NEAR_PATCH {
                patch.biomass = 6.0;
            }
        }
        assert!(food()
            .negative_income(
                &at_ceiling,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&at_ceiling)
            )
            .is_none());
    }

    /// Surplus hands fire *negative income* on a band that is otherwise breaking even, and go to
    /// the best source that is not the row they leave — the near patch, the rich row cut to eight.
    #[test]
    fn surplus_hands_are_moved_off_their_row_onto_a_second_site() {
        let view = a_parked_band();
        let proposal = food()
            .negative_income(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
            )
            .expect("nine surplus hands with a second site in reach");
        assert!(
            proposal.reason.contains("9 surplus hands -> forage 4,2"),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_FORAGE.to_owned(), 8, Some(RICH_PATCH), None)
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 9, Some(NEAR_PATCH), None)
        );
        assert_eq!(proposal.cost.workers, 9);
        // With the near patch gone there is nowhere to put them, and the rule is silent.
        let mut alone = a_parked_band();
        alone
            .snapshot
            .food_modules
            .retain(|site| site.x != NEAR_PATCH.x || site.y != NEAR_PATCH.y);
        alone.snapshot.herds.clear();
        assert!(food()
            .negative_income(
                &alone,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&alone)
            )
            .is_none());
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

    /// ⛔ **A DRAW NAMES THE IDLE HANDS IT TOOK, NOT THE IDLE ON OFFER.** Twelve hunters on a
    /// dead row move whole; with a budget of fourteen the free hands drawn alongside them are the
    /// two the budget leaves, though five stand idle. Named as "five idle" the surplus count went
    /// below zero and the seat panicked on the integration world's first frame (a start band with
    /// its idle hands and one row to leave, under a food share short of the whole pool).
    #[test]
    fn free_hands_drawn_beside_a_moving_row_are_named_by_what_the_budget_left() {
        /// A budget of fourteen of the fixture's seventeen working-age hands (`floor(0.85 × 17)`).
        const SHARE_OF_FOURTEEN: f32 = 0.85;
        let mut view = a_view();
        view.snapshot.herds[0].biomass = 100.0;
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
            .negative_income(
                &view,
                &plan_with_food_share(SHARE_OF_FOURTEEN),
                &memory,
                own_band(&view),
            )
            .expect("the dead row and the two free hands the budget leaves");
        assert!(
            proposal.reason.contains("2 idle hands and dead row"),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_FORAGE.to_owned(), 14, Some(RICH_PATCH), None)
        );
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
