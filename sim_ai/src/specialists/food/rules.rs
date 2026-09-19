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
    survives, Book, Change, PatchBook, Projection, Reassignment, BEST_FLOOR,
};
use super::sources::{
    cluster_sites, crew_take, deal_free_hands, foreign_band_at, is_walkable,
    patch_per_worker_yield, surplus_hands, sustained_hands, workable_patch_at, DealtSite, Source,
    SourceKey,
};
use super::{
    Food, INTENT_ASSIGN, INTENT_DRAWDOWN, INTENT_FEED_MOVE, INTENT_HOLD, INTENT_HUNT,
    INTENT_SETTLE, INTENT_SPLIT, INTENT_UPGRADE, ROLE_AGRICULTURE, ROLE_BUILDERS, ROLE_FORAGE,
    ROLE_HUNT,
};
use crate::board::{Demand, Resource, BARE_KIT_ID};
use crate::geometry::Tile;
use crate::ground::{BandGround, Shape, Site};
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
/// **The least *hold the ground* adds to a pool that reads short**: one hand. The wire's
/// `upkeep_workers_needed` is `ceil(demand / PER_WORKER_OUTPUT)`, and a bare keeper delivers
/// under that output, so a pool at the summed need can still leave a patch short by a fraction
/// of a hand (49,5 on seed 23: `need 1, supplied 0.98, short 0.92`); the next hand closes it.
const HOLD_MIN_HANDS: u32 = 1;
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
/// **The hoe estimate comes after the kits that feed today**: the tended rung is a cadence away
/// (the lesson has to be practised first), so its bone, fibre and craft rank under both kits at
/// the board and above `Land`'s scout.
pub(super) const DEMAND_PRIORITY_HOES: f32 = 0.6;
/// The roster's gathering kit (`equipment.json` → `gathering`, jobs `forage`, baskets).
const GATHERING_KIT_ID: &str = "gathering";

// ---- The hoe estimate's terms, restated in one place: this crate cannot link the server's
// config, and the server's values are the authority.
/// The tillage kit's recipe id (`recipes.json` → `hoes`), the item a `craft` demand names.
const HOES_RECIPE_ID: &str = "hoes";
/// One hoe's bone (`recipes.json` → `hoes.inputs[material == "bone"].amount`, `1`).
const HOE_RECIPE_BONE: u32 = 1;
/// One hoe's fibre (`recipes.json` → `hoes.inputs[material == "fibre"].amount`, `2`).
const HOE_RECIPE_FIBRE: u32 = 2;
/// One hoe's bench work in worker-turns (`recipes.json` → `hoes.work`, `5`).
const HOE_RECIPE_WORK: f32 = 5.0;
/// What a worker-turn at the bench is worth (`recipes.json` → `crafting.progress_per_worker_turn`,
/// `1.0`).
const CRAFT_PROGRESS_PER_WORKER_TURN: f32 = 1.0;
/// The bare hand's craft speed on the hoes' bench material (`materials.json` → `bone`
/// `hand_working.rate`, `0.5` — the hoes read bone's `density`, so bone is the bench material
/// and its rate applies with no tool).
const BARE_HAND_CRAFT_RATE: f32 = 0.5;
/// The crafter crew the estimate is timed for: one hand at the bench.
const HOE_CRAFT_CREW: u32 = 1;
/// What the cultivation lesson costs in practice units (`intensification_ladder.json` →
/// `knowledge.lesson_costs.cultivation`, `20`).
const CULTIVATION_LESSON_COST: f32 = 20.0;
/// What one worked turn of one source is worth in practice units
/// (`intensification_ladder.json` → `knowledge.learn_rate`, `1.0`) — charged once per source
/// per turn, so the shape's patch sites each teach it.
const LADDER_LEARN_RATE: f32 = 1.0;
/// The two materials a hoe is made of, by `materials.json` id.
const MATERIAL_BONE: &str = "bone";
const MATERIAL_FIBRE: &str = "fibre";
/// A herd whose `body_mass` reads this has none on the wire — *"`0` if unknown"*
/// (`HerdTelemetryState::body_mass`) — and only a kit with no upper mass bound is trusted on it.
const BODY_MASS_UNKNOWN: f32 = 0.0;
/// A kit whose mass bound reads this has none — *"`0` on either end means unbounded"*
/// (`KitOptionState::attack_min_body_mass`).
const MASS_UNBOUNDED: f32 = 0.0;

/// [`Food::hoe_estimate`]'s answer: the hoes wanted and the turn a crafter should start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoeEstimate {
    hoes: u32,
    start_tick: u64,
}

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
    /// The band-wide pools released from ([`PoolRelease`]): the role, the hands taken off it and
    /// the hands left on it. Free hands like the idle ones — a pool with nothing to do earns
    /// nothing where it stands.
    pool_cuts: Vec<PoolCut>,
}

/// One pool a draw took hands off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PoolCut {
    role: &'static str,
    taken: u32,
    left: u32,
}

/// **A band-wide pool with hands to spare** ([`Food::pool_releases`]): `free` of the `held` hands
/// on `role` are free hands, offered to every rule that draws — the `builders` pool with nothing
/// to raise, the `agriculture` pool above the band's plant bill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PoolRelease {
    role: &'static str,
    held: u32,
    free: u32,
}

/// A reassignment *negative income* weighs.
struct Candidate {
    commands: Vec<CommandPayload>,
    hands: u32,
    change: Reassignment,
    subject: String,
}

/// A site *split to feed* could send a child to, and what that child would be.
struct SplitSite {
    tile: Tile,
    /// Beyond `split_search_tiles` — the far ring, tried only when the near ring has nothing.
    far: bool,
    distance: u32,
    /// The child's crew: the site's sustained hands, at least the founding floor, within what
    /// the parent may give up.
    crew: u32,
    /// The child's projected net income per turn once the site's series settles.
    net: f32,
    /// The site's settled income for the crew — what the reason quotes.
    income: f32,
}

/// The rung *upgrade the ground* would declare on a patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Climb {
    Tended,
    Field,
}

impl Food {
    /// Free `want` hands: the band's idle first, then the pools `released` ([`PoolRelease`] —
    /// hands a pool has nothing for, at no cost), then the surplus on `surplus_from` — each row
    /// down to its `workers_needed`, at no cost ([`surplus_hands`]) — then `rows` in the order
    /// given until each is empty. `hands` is what could be freed, which may be short of `want`.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        released: &[PoolRelease],
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
            pool_cuts: Vec::new(),
        };
        for pool in released {
            if drawn.hands >= want {
                break;
            }
            let take = (want - drawn.hands).min(pool.free);
            if take == 0 {
                continue;
            }
            drawn.pool_cuts.push(PoolCut {
                role: pool.role,
                taken: take,
                left: pool.held - take,
            });
            drawn.hands += take;
        }
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
            drawn.income_lost += Self::row_rate(memory, band, row) * take as f32;
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
    fn surplus_rows<'b>(
        memory: &SeatMemory,
        band: &'b PopulationCohortState,
    ) -> Vec<&'b LaborAssignmentState> {
        let mut rows: Vec<&LaborAssignmentState> = band
            .labor_assignments
            .iter()
            .filter(|row| surplus_hands(row) > 0)
            .collect();
        rows.sort_by(|a, b| {
            Self::row_rate(memory, band, a).total_cmp(&Self::row_rate(memory, band, b))
        });
        rows
    }

    /// How a candidate names its free hands: *"9 idle hands"*, *"9 surplus hands"*, *"3 hands
    /// off builders"*, or any of them joined — *"2 idle, 3 surplus and 3 off builders hands"*.
    fn free_hands_phrase(drawn: &Drawn) -> String {
        let surplus =
            drawn.hands - drawn.idle - drawn.pool_cuts.iter().map(|cut| cut.taken).sum::<u32>();
        let mut parts = Vec::new();
        if drawn.idle > 0 {
            parts.push(format!("{} idle", drawn.idle));
        }
        if surplus > 0 {
            parts.push(format!("{surplus} surplus"));
        }
        for cut in &drawn.pool_cuts {
            parts.push(format!("{} off {}", cut.taken, cut.role));
        }
        let named = match parts.len() {
            0 => format!("{} idle", drawn.idle),
            1 => parts.remove(0),
            n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
        };
        format!("{named} hands")
    }

    /// The `assign_labor` lines that reduce the rows and the pools a draw took hands from.
    fn reduction_commands(
        &self,
        band: &PopulationCohortState,
        drawn: &Drawn,
    ) -> Vec<CommandPayload> {
        self.reduction_commands_keeping(band, drawn, None)
    }

    /// [`Food::reduction_commands`] without the cut of the pool `keep` — for a rule that restaffs
    /// that pool itself and would otherwise send two lines on one row.
    fn reduction_commands_keeping(
        &self,
        band: &PopulationCohortState,
        drawn: &Drawn,
        keep: Option<&str>,
    ) -> Vec<CommandPayload> {
        drawn
            .reductions
            .iter()
            .map(|(key, left)| self.assign(band, key, *left))
            .chain(
                drawn
                    .pool_cuts
                    .iter()
                    .filter(|cut| keep != Some(cut.role))
                    .map(|cut| self.assign_pool(band, cut.role, cut.left)),
            )
            .collect()
    }

    /// **The pools with hands to spare** — free hands, offered to every rule that draws, since a
    /// pool with nothing to do earns nothing where it stands and no rule could reach into one
    /// (bench seed 27: the parent ended with every hand on `builders` and `agriculture`, income
    /// 0.00 for twenty turns, and *negative income* was silent for want of a row to draw from).
    ///
    /// **Builders release.** The whole `builders` pool when the band's `build_queue` is empty —
    /// nothing to raise — or when the head of it is blocked: the head's source row publishes a
    /// non-empty `build_blocked_reason` (*"WHY THE BAND'S BUILDERS ARE STUCK ON THIS SOURCE"*,
    /// `ForagePatchState` / `HerdTelemetryState`; the whole pool goes on the head, so a blocked
    /// head idles all of it). A head the frame does not carry is not read as blocked.
    ///
    /// **Keeper trim.** The `agriculture` pool is one pool against the band's summed plant bill
    /// (*hold the ground*): Σ `upkeep_workers_needed` over the patches the band holds a row on
    /// ([`Food::kept_patches`]). Above that sum the excess is free — **except a single hand**,
    /// [`HOLD_MIN_HANDS`], which is the slack the hold itself adds when the pool at the sum still
    /// leaves a patch short; trimmed, the hold would add it back the next turn, every other turn.
    /// Nothing is trimmed while a held patch reads short: the pool is the hold's then.
    /// `husbandry` is not trimmed: no rule of this specialist staffs it, so it never holds a hand
    /// to spare.
    pub(super) fn pool_releases(
        &self,
        view: &SeatView,
        band: &PopulationCohortState,
    ) -> Vec<PoolRelease> {
        let mut out = Vec::new();
        let builders = Self::workers_in_pool(band, ROLE_BUILDERS);
        if builders > 0 && Self::build_head_idle(view, band) {
            out.push(PoolRelease {
                role: ROLE_BUILDERS,
                held: builders,
                free: builders,
            });
        }
        let keepers = Self::workers_in_pool(band, ROLE_AGRICULTURE);
        if keepers > 0 {
            let kept = self.kept_patches(view, band);
            let short = kept.iter().any(|(_, patch)| patch.upkeep_shortfall > 0.0);
            let need: u32 = kept
                .iter()
                .map(|(_, patch)| patch.upkeep_workers_needed)
                .sum();
            let excess = keepers.saturating_sub(need);
            if !short && excess > HOLD_MIN_HANDS {
                out.push(PoolRelease {
                    role: ROLE_AGRICULTURE,
                    held: keepers,
                    free: excess,
                });
            }
        }
        out
    }

    /// Whether the band's `builders` pool has nothing to raise: the build queue is empty, or its
    /// head's source publishes a blocked reason ([`Food::pool_releases`]).
    fn build_head_idle(view: &SeatView, band: &PopulationCohortState) -> bool {
        let Some(head) = band.build_queue.first() else {
            return true;
        };
        if head.kind == ROLE_HUNT || !head.fauna_id.is_empty() {
            view.snapshot
                .herds
                .iter()
                .find(|herd| herd.id == head.fauna_id)
                .is_some_and(|herd| !herd.build_blocked_reason.is_empty())
        } else {
            view.patch_at(Tile::new(head.target_x, head.target_y))
                .is_some_and(|patch| !patch.build_blocked_reason.is_empty())
        }
    }

    /// **The patches the band's `agriculture` pool answers for**: every forage row it holds,
    /// hands or none, whose patch the seat owns and whose ladder has work to hold
    /// (`upkeep_workers_needed > 0`).
    fn kept_patches<'v>(
        &self,
        view: &'v SeatView,
        band: &'v PopulationCohortState,
    ) -> Vec<(&'v LaborAssignmentState, &'v ForagePatchState)> {
        band.labor_assignments
            .iter()
            .filter(|row| row.kind == ROLE_FORAGE)
            .filter_map(|row| {
                let patch = view
                    .patch_at(Tile::new(row.target_x, row.target_y))
                    .filter(|patch| patch.owner == Some(self.faction))
                    .filter(|patch| patch.upkeep_workers_needed > 0)?;
                Some((row, patch))
            })
            .collect()
    }

    /// The band's worked rows under `role`, lowest-paying first.
    fn rows_ascending<'b>(
        memory: &SeatMemory,
        band: &'b PopulationCohortState,
        role: &str,
    ) -> Vec<&'b LaborAssignmentState> {
        let mut rows: Vec<&LaborAssignmentState> = band
            .labor_assignments
            .iter()
            .filter(|row| row.kind == role && row.workers > 0)
            .collect();
        rows.sort_by(|a, b| {
            Self::row_rate(memory, band, a).total_cmp(&Self::row_rate(memory, band, b))
        });
        rows
    }

    /// The row *negative income* empties first: one that is overused ([`Food::overused`] — a hunt
    /// row the sim says `overdraws`, a forage row whose patch stands at its floor), a hunt row the
    /// sim says no crew is useful on, or a dead row — and failing those, the row paying the least
    /// per worker. `true` when the row is one of the troubled kinds, which need no gain guard to
    /// be worth leaving.
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
            let why = if Self::overused(view, row) {
                WHY_OVERUSED
            } else if row.kind == ROLE_HUNT && row.hunt_useful_workers == 0 {
                WHY_NO_USEFUL_CREW
            } else if self.is_dead(view, memory, band, key) {
                WHY_DEAD_ROW
            } else {
                continue;
            };
            return Some((row, key.clone(), true, why));
        }
        worked
            .into_iter()
            .min_by(|(a, _), (b, _)| {
                Self::row_rate(memory, band, a).total_cmp(&Self::row_rate(memory, band, b))
            })
            .map(|(row, key)| (row, key, false, WHY_LOWEST_ROW))
    }

    /// **Distinctness is not improvement, on the free-hand path too.** Free hands move onto a
    /// site only when the site's take actually rises by them: the marginal take of `moved` hands
    /// there — `take`, read against the patch's **honest ceiling** (`honest_ceiling`: the room
    /// above the Best floor plus the floor's regrowth, what the ground can give this turn) — must
    /// be at least `food.runway_gain_fraction × moved × rate`, the idiom the row-empty guard
    /// uses. A patch at its floor with its regrowth already taken has a marginal of nothing for
    /// the next hand, so the hand stays: seed 23's t45–t52 shuffle (47,5 and 49,5 each read as
    /// having room for one more hand by the standing stock while the frame read that hand as
    /// surplus wherever it stood) is held by this alone. There is no second half: reading the
    /// band's row there as full when `workers ≥ workers_needed` misread a one-hand row, since
    /// `workers_needed` is only the crew that produced *this turn's* take — a row at `w1 n1` on
    /// a patch with room read "full", and seed 50's parent sat with six spare hands beside 60,12
    /// and starved (11 working / 11 hunger deaths at t60).
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
        take >= self.floors.runway_gain_fraction * moved as f32 * rate
    }

    /// **Whether a worked row is overused**, read by job.
    ///
    /// A **hunt** row reads the sim's `overdraws`: a kill turn cashes a whole banked animal, so a
    /// hunt's `actual_yield` spikes above its `sustainable_yield` under any floor, and the field
    /// *"replaces the client-derived `actual_yield > sustainable_yield` test, which mis-fires on a
    /// hunt's lumpy per-turn take"*.
    ///
    /// A **forage** row reads a take above its regrowth (`actual_yield > sustainable_yield`) on a
    /// patch at its floor ([`Food::at_its_floor`]): `overdraws` needs `floor <
    /// MSY_BIOMASS_FRACTION` (the sim's `floor_overdraws`), so it is never true for a row at the
    /// default floor. Read through `overdraws` alone (80b6c1e8) no forage row was ever the row to
    /// empty first, rule 1 fell through to the lowest-paying row, moved hands onto stripped
    /// ground, and the forager seat starved on both bench seeds (seed 19: 1 working, 25 hunger
    /// deaths; seed 40: 0 working, 26) against a baseline of 16 and 20 working with none.
    fn overused(view: &SeatView, row: &LaborAssignmentState) -> bool {
        if row.kind == ROLE_FORAGE {
            row.actual_yield > row.sustainable_yield && Self::at_its_floor(view, row)
        } else {
            row.overdraws
        }
    }

    /// **Whether a forage row's patch stands at or below the row's floor**: `biomass ≤ floor ×
    /// carrying_capacity`, at the floor the row reads on the wire (Best, 0.5, for an assignment
    /// sent with `floor: None`); a patch the frame does not carry reads at its floor. A take above
    /// the regrowth on a patch above its floor is the room above the floor being taken, not
    /// overuse, so a row *draw down to survive* set below Best is not emptied while it strips that
    /// room. Before the floor half, a fresh patch read "overused" every other turn and rule 1
    /// shuffled band 2's hands between 47,5 and 49,5 for the whole of seed 23's t45–t50.
    fn at_its_floor(view: &SeatView, row: &LaborAssignmentState) -> bool {
        view.patch_at(Tile::new(row.target_x, row.target_y))
            .is_none_or(|patch| patch.biomass <= row.floor * patch.carrying_capacity)
    }

    /// The reassignments *negative income* chooses among, all within `budget`: (a) the free hands
    /// — the idle ones plus every row's surplus, each donor row reduced to its `workers_needed` —
    /// **dealt across the sites in reach in two passes** ([`deal_free_hands`]: each site's
    /// sustained crew first, then the room above the floor; a band in a cluster spreads over it
    /// instead of piling onto one site, and a patch at its floor takes no more), and, weighed
    /// beside it, the same hands onto the single best source none of them leave (which may be a
    /// herd, up to the kit units held); (b) the row to empty first onto the best other source,
    /// when that buys something; (c) both onto the best source for the whole crew.
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
        let released = self.pool_releases(view, band);
        let surplus_rows = Self::surplus_rows(memory, band);
        let free = self.draw(memory, band, &released, &surplus_rows, &[], budget, idle);
        let donors: Vec<SourceKey> = free.reductions.iter().map(|(key, _)| key.clone()).collect();
        if free.hands > 0 {
            let is_dead = |key: &SourceKey| self.is_dead(view, memory, band, key);
            // Only the sites the hands improve take any; the hands a site could not use stay
            // where they are, so the donors are drawn down only by what is placed.
            let improves = |tile: Tile, hands: u32, take: f32| {
                self.improves(view, memory, band, tile, hands, take)
            };
            let sites = deal_free_hands(
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
                &improves,
            );
            let dealt: u32 = sites.iter().map(DealtSite::hands).sum();
            if dealt > 0 {
                let placed_hands =
                    self.draw(memory, band, &released, &surplus_rows, &[], dealt, idle);
                let mut commands = self.reduction_commands(band, &placed_hands);
                let mut placed = Vec::new();
                for site in &sites {
                    let key = SourceKey::Patch(site.tile);
                    commands.push(self.assign(
                        band,
                        &key,
                        Self::workers_on(band, &key) + site.hands(),
                    ));
                    placed.push(format!(
                        "{} ×{} (sustained {}, surplus {})",
                        key.describe(),
                        site.hands(),
                        site.sustained,
                        site.surplus
                    ));
                }
                out.push(Candidate {
                    commands,
                    hands: dealt,
                    change: Reassignment {
                        income_lost: 0.0,
                        income_gained: sites.iter().map(|site| site.take).sum(),
                        payoff_turn: 0,
                    },
                    subject: format!(
                        "{} -> {}",
                        Self::free_hands_phrase(&placed_hands),
                        placed.join(", ")
                    ),
                });
            }
        }
        // The single best source, sent only the hands it can use (a herd: the kit units held).
        let free_onto = (free.hands > 0)
            .then(|| Self::best_source(sources, free.hands, &donors))
            .flatten()
            .map(|best| {
                (
                    best,
                    best.usable(Self::workers_on(band, &best.key), free.hands),
                )
            })
            .filter(|(best, hands)| {
                let existing = Self::workers_on(band, &best.key);
                let take = best.marginal(existing, *hands);
                match &best.key {
                    SourceKey::Patch(tile) => {
                        self.improves(view, memory, band, *tile, *hands, take)
                    }
                    SourceKey::Herd(_) => {
                        *hands > 0
                            && take
                                >= self.floors.runway_gain_fraction
                                    * *hands as f32
                                    * best.per_worker_yield
                    }
                }
            });
        if let Some((best, hands)) = free_onto {
            let existing = Self::workers_on(band, &best.key);
            let sent = self.draw(memory, band, &released, &surplus_rows, &[], hands, idle);
            let mut commands = self.reduction_commands(band, &sent);
            commands.push(self.assign(band, &best.key, existing + sent.hands));
            out.push(Candidate {
                commands,
                hands: sent.hands,
                change: Reassignment {
                    income_lost: 0.0,
                    income_gained: best.marginal(existing, sent.hands),
                    payoff_turn: 0,
                },
                subject: format!(
                    "{} -> {}",
                    Self::free_hands_phrase(&sent),
                    best.describe_for(existing + sent.hands)
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
        // The row as the reason names it; a dead row carries its accounting — `dead row hunt
        // herd_9: took 0.24 of 1.20 expected over 4 turns`.
        let why = if why == WHY_DEAD_ROW {
            format!(
                "{why} {}: {}",
                low_key.describe(),
                Self::accounting(memory, band, &low_key)
            )
        } else {
            format!("{why} {}", low_key.describe())
        };
        let low = Self::row_rate(memory, band, row);
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
        // ⛔ **A merely lowest row lands only where its hands improve the take** — the same
        // guard the free-hand path has (`improves`): a patch whose row already reads `workers ≥
        // workers_needed` takes no more. A troubled row leaves whatever it lands on. Without
        // this a hunter on a zero turn was the lowest row, went back onto the full patch it had
        // been surplus on, was surplus there again, and went back to the herd — every other turn
        // of seed 54's t26–t45, each bounce the band's one order. (Narrowed to rows already
        // carrying surplus, `workers > workers_needed`, the eight seeds read 147 working and 26
        // hunger deaths against this guard's 164 and 20 — the seat shuffled whole rows onto
        // exactly-staffed patches every turn again.)
        let lands_well = |next: &Source, hands: u32, take: f32| {
            troubled
                || match next.key {
                    SourceKey::Patch(tile) => self.improves(view, memory, band, tile, hands, take),
                    SourceKey::Herd(_) => true,
                }
        };
        if let Some(next) = Self::best_source(sources, moved, std::slice::from_ref(&low_key))
            .filter(|next| clears(next))
        {
            let existing = Self::workers_on(band, &next.key);
            // A herd takes only the hands its kit units arm. A troubled row is emptied whatever
            // the next source can use (its crew earns nothing where it stands); a merely lowest
            // row gives up only the hands that land somewhere.
            let placed = next.usable(existing, moved);
            let leaving = if troubled { moved } else { placed };
            let change = Reassignment {
                income_lost: low * leaving as f32,
                income_gained: next.marginal(existing, placed),
                payoff_turn: 0,
            };
            if placed > 0
                && change.income_gained > change.income_lost
                && lands_well(next, placed, change.income_gained)
            {
                out.push(Candidate {
                    commands: vec![
                        self.assign(band, &low_key, row.workers - leaving),
                        self.assign(band, &next.key, existing + placed),
                    ],
                    hands: leaving,
                    change,
                    subject: format!("{why} -> {}", next.describe_for(existing + placed)),
                });
            }
        }
        // The free hands again, less the emptied row's own surplus (its whole crew moves).
        let surplus_elsewhere: Vec<&LaborAssignmentState> = surplus_rows
            .iter()
            .copied()
            .filter(|other| SourceKey::of_row(other).as_ref() != Some(&low_key))
            .collect();
        let free = self.draw(
            memory,
            band,
            &released,
            &surplus_elsewhere,
            &[],
            budget.saturating_sub(moved),
            idle,
        );
        let both = free.hands + moved;
        if free.hands > 0 && both <= budget {
            let mut except: Vec<SourceKey> =
                free.reductions.iter().map(|(key, _)| key.clone()).collect();
            except.push(low_key.clone());
            // Both onto one source: only where that source can use the whole crew.
            if let Some(best) = Self::best_source(sources, both, &except)
                .filter(|next| clears(next))
                .filter(|next| next.usable(Self::workers_on(band, &next.key), both) == both)
            {
                let existing = Self::workers_on(band, &best.key);
                let change = Reassignment {
                    income_lost: low * moved as f32,
                    income_gained: best.marginal(existing, both),
                    payoff_turn: 0,
                };
                if change.income_gained > change.income_lost
                    && lands_well(best, both, change.income_gained)
                {
                    let mut commands = self.reduction_commands(band, &free);
                    commands.push(self.assign(band, &low_key, row.workers - moved));
                    commands.push(self.assign(band, &best.key, existing + both));
                    out.push(Candidate {
                        commands,
                        hands: both,
                        change,
                        subject: format!(
                            "{} and {why} -> {}",
                            Self::free_hands_phrase(&free),
                            best.describe_for(existing + both)
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
            || !Self::surplus_rows(memory, band).is_empty()
            || !self.pool_releases(view, band).is_empty();
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
            cost: Cost::claimed(candidate.hands, band.band_id, &candidate.commands),
            commands: candidate.commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_ASSIGN, band.band_id),
            score: progress * self.weight,
            reason: format!(
                "{REASON_NEGATIVE_INCOME}: {} [{}]",
                candidate.subject,
                ledger_note(&after)
            ),
            memo: None,
            standing: false,
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
        staying.sort_by(|a, b| {
            Self::row_rate(memory, band, a).total_cmp(&Self::row_rate(memory, band, b))
        });
        let idle = band.idle_workers.min(budget);
        let released = self.pool_releases(view, band);
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let before = project(&book, &Reassignment::NONE, horizon);
        let mut best: Option<(Drawn, &Source, Projection, f32, Reassignment)> = None;
        let none: [&LaborAssignmentState; 0] = [];
        for (rows, want) in [(&none[..], idle), (&staying[..], budget)] {
            let drawn = self.draw(memory, band, &released, &[], rows, want, idle);
            if drawn.hands == 0 {
                continue;
            }
            let Some(source) = Self::best_source(&falling, drawn.hands, &[]) else {
                continue;
            };
            let existing = Self::workers_on(band, &source.key);
            // A herd takes only the hands its kit units arm: draw again for what it can use.
            let usable = source.usable(existing, drawn.hands);
            if usable == 0 {
                continue;
            }
            let drawn = if usable < drawn.hands {
                self.draw(memory, band, &released, &[], rows, usable, idle)
            } else {
                drawn
            };
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
            cost: Cost::claimed(drawn.hands, band.band_id, &commands),
            commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_FEED_MOVE, band.band_id),
            score: progress * self.weight,
            reason: format!(
                "{REASON_FEED_MOVE}: {} hands onto {} before it falls out of range from {},{} [{}]",
                drawn.hands,
                source.describe_for(existing + drawn.hands),
                target.x,
                target.y,
                ledger_note(&after)
            ),
            memo: None,
            standing: false,
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

    /// **Rule 3 — split to feed.** After rule 1's change the band's projected net income is
    /// still negative, or its projected runway still under the goal, and a discovered, workable,
    /// walkable site outside its reach would feed a child crew on its own: split toward it. The
    /// child appears on the parent's tile next turn and [`Food::settle`] walks it there.
    ///
    /// **The child is sized to the site**: its crew is the site's [`sustained_hands`] at the
    /// band's rate, at least `founding_min_workers`, capped at `working_age −
    /// founding_parent_min_workers`; the rule is silent when that cap is under the founding
    /// floor. The sim's two split floors cross the wire on every cohort
    /// (`PopulationCohortState::founding_min_workers` / `founding_parent_min_workers` — *"The
    /// two floors cross the wire; the verdict does not."*), so a crew the sim would refuse as too
    /// small is not asked for. The refusal memory ([`SeatMemory::split_refused_at`]) stays as a
    /// belt: a split can still be refused for reasons the floors do not state.
    ///
    /// **Feasible means the child survives on its own** ([`Food::child_projection`]): its share
    /// of the larder and of the band's consumption, and the site's Best-floor income series for
    /// its crew, projected over the horizon, must `survives`. Sites in the near ring
    /// (`split_search_tiles`, the supply-pooling reach) beat sites in the far ring
    /// (`split_reach_tiles`); within a ring the child's projected net income ranks, nearer first
    /// on a tie.
    ///
    /// **Budget-free.** A split moves people out of the band; it is not labor churn against the
    /// specialist's share, so its cost claims zero workers — the band-move claim still collides
    /// with any other move of the band. Charged to the budget it was `over_budget` every turn
    /// rule 1's shuffle had claimed the hands first (bench seed 19: proposed at t6, rejected,
    /// silent until t33).
    ///
    /// Answers the change it priced beside the proposal — the rows the crew leaves, against the
    /// mouths that leave with it; the child's take is the child's — so the last rule can project
    /// the plan in force.
    pub(super) fn split_to_feed_change(
        &self,
        view: &SeatView,
        plan: &Plan,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        carried: &Reassignment,
    ) -> Option<(Proposal, Reassignment)> {
        let goals = plan.food_goals()?;
        // The most the parent may give up.
        let cap = band
            .working_age
            .saturating_sub(band.founding_parent_min_workers);
        // A split the sim refused at this size is not asked for again until the band has grown.
        let refused_at_this_size = memory
            .split_refused_at(band.band_id)
            .is_some_and(|refused_at| refused_at >= band.working_age);
        // Not a child still walking to the site it was split toward: its book on the road reads
        // no income at all, which is the walk, not ground that cannot feed it — the same
        // reading that keeps *negative income* off it.
        if band.is_traveling
            || memory.pending_split(band.band_id).is_some()
            || memory.born_by_split(band.band_id).is_some()
            || refused_at_this_size
            || cap < band.founding_min_workers
        {
            return None;
        }
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let after = project(&book, carried, horizon);
        if after.net_after >= 0.0 && after.runway_at_end >= goals.runway_turns {
            return None;
        }
        let grid = view.grid();
        let here = band_tile(band);
        // **A site is one child's.** The crew is sized to the site's sustained hands, so a site
        // another own band already works from (within its `work_range`), or that a split is
        // pending toward or a child is still walking to, has no room for a second child. The
        // pending entry clears the turn the child appears, so without this the parent split
        // toward the same site again the very next turn (bench seed 19: t3, t4 and t16, three
        // children of four onto a site sustaining four; seed 40: t5 and t8).
        let claimed = |tile: Tile| {
            view.own_bands(self.faction)
                .filter(|other| other.band_id != band.band_id)
                .any(|other| {
                    grid.distance(band_tile(other), tile) <= other.work_range
                        || memory
                            .pending_split(other.band_id)
                            .is_some_and(|pending| pending.target == tile)
                        || memory
                            .born_by_split(other.band_id)
                            .is_some_and(|birth| birth.target == tile)
                })
        };
        let mut sites: Vec<SplitSite> = view
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
                    && distance <= self.floors.split_reach_tiles
                    && is_walkable(view, tile)
                    && !foreign_band_at(view, self.faction, tile)
                    && !claimed(tile))
                .then(|| workable_patch_at(view, tile))
                .flatten()
                .map(|patch| (patch, distance))
            })
            .filter(|(patch, _)| {
                let key = SourceKey::Patch(Tile::new(patch.x, patch.y));
                !self.is_dead(view, memory, band, &key)
            })
            .filter_map(|(patch, distance)| {
                let rate = patch_per_worker_yield(memory, band, patch);
                let crew = sustained_hands(patch, rate)
                    .max(band.founding_min_workers)
                    .min(cap);
                let (projection, income) = self.child_projection(band, patch, crew, horizon);
                survives(&projection).then_some(SplitSite {
                    tile: Tile::new(patch.x, patch.y),
                    far: distance > self.floors.split_search_tiles,
                    distance,
                    crew,
                    net: projection.net_after,
                    income,
                })
            })
            .collect();
        sites.sort_by(|a, b| {
            a.far
                .cmp(&b.far)
                .then_with(|| b.net.total_cmp(&a.net))
                .then_with(|| a.distance.cmp(&b.distance))
        });
        let site = sites.into_iter().next()?;
        let crew = site.crew;
        let travel = site.distance.div_ceil(BAND_MOVE_TILES_PER_TURN);
        // The hands leave the parent's rows, lowest-paying first (its idle ones cost nothing).
        let mut rows = Self::rows_ascending(memory, band, ROLE_HUNT);
        rows.extend(Self::rows_ascending(memory, band, ROLE_FORAGE));
        rows.sort_by(|a, b| {
            Self::row_rate(memory, band, a).total_cmp(&Self::row_rate(memory, band, b))
        });
        let drawn = self.draw(
            memory,
            band,
            &self.pool_releases(view, band),
            &Self::surplus_rows(memory, band),
            &rows,
            crew,
            band.idle_workers,
        );
        // What the parent keeps: the rows drawn are lost, the mouths that leave are gained.
        let change = Reassignment {
            income_lost: drawn.income_lost,
            income_gained: Self::crew_share(band, crew) * band.food_consumption,
            payoff_turn: 0,
        };
        let after_split = project_all(&book, &[*carried, change], horizon);
        let target = site.tile;
        let commands = vec![CommandPayload::SplitBand {
            faction_id: self.faction,
            band_id: Some(band.band_id),
            workers: crew,
        }];
        let proposal = Proposal {
            cost: Cost::claimed(0, band.band_id, &commands),
            commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_SPLIT, band.band_id),
            score: goal_progress(&goals, &after, &after_split) * self.weight,
            reason: format!(
                "{REASON_SPLIT_TO_FEED}: {crew} toward {},{} taking {:.1}/turn from t{travel} [{}]",
                target.x,
                target.y,
                site.income,
                ledger_note(&after_split)
            ),
            memo: Some(Memo::Split {
                band: band.band_id,
                target,
                workers: crew,
            }),
            standing: false,
        };
        Some((proposal, change))
    }

    /// The share of `band` a child of `crew` working-age hands is: what it takes of the larder
    /// and of the mouths.
    fn crew_share(band: &PopulationCohortState, crew: u32) -> f32 {
        crew as f32 / band.working_age.max(1) as f32
    }

    /// **The child's book, projected on its own**: its share of the parent's larder and
    /// consumption ([`Food::crew_share`]), no income but the site — the Best-floor income series
    /// of `patch` worked by `crew` ([`floor_income`]) — over `horizon`. Answers the projection
    /// and the series' settled income, the sustained take once the room above the floor is
    /// spent.
    fn child_projection(
        &self,
        band: &PopulationCohortState,
        patch: &ForagePatchState,
        crew: u32,
        horizon: u32,
    ) -> (Projection, f32) {
        let share = Self::crew_share(band, crew);
        let parent = Self::book(band);
        let child = Book {
            stock: parent.stock * share,
            income: 0.0,
            consumption: parent.consumption * share,
        };
        let income = floor_income(&Self::patch_book(patch, crew), BEST_FLOOR, horizon);
        let settled = income.per_turn.last().copied().unwrap_or(0.0);
        let projection = project_changes(
            &child,
            &[Change::Series {
                income_lost: 0.0,
                income_gained: income.per_turn,
            }],
            horizon,
        );
        (projection, settled)
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
        let commands = vec![CommandPayload::MoveBand {
            faction_id: self.faction,
            band_id: Some(band.band_id),
            target_x: birth.target.x,
            target_y: birth.target.y,
        }];
        Some(Proposal {
            cost: Cost::claimed(0, band.band_id, &commands),
            commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_SETTLE, band.band_id),
            score: goal_progress(&goals, &before, &after) * self.weight,
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
            standing: false,
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
        let forage = Self::rows_ascending(memory, band, ROLE_FORAGE);
        let forage_surplus: Vec<&LaborAssignmentState> = Self::surplus_rows(memory, band)
            .into_iter()
            .filter(|row| row.kind == ROLE_FORAGE)
            .collect();
        let mut best: Option<(Drawn, &Source, Projection, Reassignment)> = None;
        for hands in 1..=budget {
            // Off the rows only — their surplus first, then the lowest-paying: the idle hands
            // and the pools' spare hands are *negative income*'s to place, and a hunt drawn from
            // them would compete with that assignment for the same rows.
            let drawn = self.draw(memory, band, &[], &forage_surplus, &forage, hands, 0);
            if drawn.hands < hands {
                break;
            }
            let Some(herd) = herds.iter().copied().max_by(|a, b| {
                a.marginal(Self::workers_on(band, &a.key), hands)
                    .total_cmp(&b.marginal(Self::workers_on(band, &b.key), hands))
            }) else {
                break;
            };
            // Never more hands than the herd's kit units arm.
            if herd.usable(Self::workers_on(band, &herd.key), hands) < hands {
                continue;
            }
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
            cost: Cost::claimed(drawn.hands, band.band_id, &commands),
            commands,
            intent: intent_key(SPECIALIST_FOOD, INTENT_HUNT, band.band_id),
            score: goal_progress(&goals, &after, &projection) * self.weight,
            reason: format!(
                "{REASON_SPARE_HANDS}: {} hands onto {} [{}]",
                drawn.hands,
                herd.describe_for(existing + drawn.hands),
                ledger_note(&projection)
            ),
            memo: None,
            standing: false,
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
    /// (`FloraShareInfo::cultivate_payoff` / `sow_payoff`). The land reading (`ground.rs`)
    /// quotes a patch's farmed rungs through this same selection.
    pub(crate) fn climb_payoff(
        patch: &ForagePatchState,
        climb: Climb,
    ) -> Option<(&FloraShareInfo, f32)> {
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
        let released = self.pool_releases(view, band);
        let surplus_rows = Self::surplus_rows(memory, band);
        let free = self.draw(memory, band, &released, &surplus_rows, &[], budget, idle);
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
            let mut pool = Self::rows_ascending(memory, band, ROLE_HUNT);
            pool.extend(
                Self::rows_ascending(memory, band, ROLE_FORAGE)
                    .into_iter()
                    .filter(|other| Tile::new(other.target_x, other.target_y) != tile),
            );
            for hands in free.hands.max(1)..=budget {
                let drawn = self.draw(memory, band, &released, &surplus_rows, &pool, hands, idle);
                if drawn.hands < hands {
                    break;
                }
                // The builders already standing, less any the draw released (a pool with nothing
                // to raise is free hands, and this rule restaffs it whole below).
                let builders_kept = drawn
                    .pool_cuts
                    .iter()
                    .find(|cut| cut.role == ROLE_BUILDERS)
                    .map_or_else(
                        || Self::workers_in_pool(band, ROLE_BUILDERS),
                        |cut| cut.left,
                    );
                let builders = builders_kept + hands;
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
                commands.extend(self.reduction_commands_keeping(band, &drawn, Some(ROLE_BUILDERS)));
                commands.push(self.assign_pool(band, ROLE_BUILDERS, builders));
                let progress = goal_progress(&goals, &after, &projection);
                let proposal = Proposal {
                    cost: Cost::claimed(hands, band.band_id, &commands),
                    commands,
                    intent: intent_key(SPECIALIST_FOOD, INTENT_UPGRADE, band.band_id),
                    score: progress * self.weight,
                    reason: format!(
                        "{REASON_UPGRADE}: {verb} {},{} ({}) with {builders} builders, payoff turn {payoff_turn} [{}]",
                        tile.x,
                        tile.y,
                        plant.species,
                        ledger_note(&projection)
                    ),
                    memo: None,
                    standing: false,
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
    /// baskets too — a spare basket is not forfeited budget, an unspent slot is. Beside the kits,
    /// **the hoe estimate, posted not crafted** ([`Food::hoe_estimate`], off the land reading's
    /// shape when `ground` carries one): the bone and fibre for the hoes the first cultivate would
    /// want, and a `craft` demand for them with the turn a crafter should start, at
    /// [`DEMAND_PRIORITY_HOES`] — the orchestrator declines the craft (`no crafter yet`) and the
    /// log carries the ask. A window that is not open asks for nothing.
    pub fn outfit_demands(
        &self,
        view: &SeatView,
        memory: &SeatMemory,
        band: &PopulationCohortState,
        ground: Option<&BandGround>,
    ) -> Vec<Demand> {
        if !band
            .loadout_window
            .as_ref()
            .is_some_and(|window| window.open)
        {
            return Vec::new();
        }
        let tick = view.tick();
        let is_dead = |key: &SourceKey| self.is_dead(view, memory, band, key);
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
        if let Some(estimate) = ground.and_then(|ground| {
            Self::hoe_estimate(ground.shape(), &ground.reading.sites, band, tick)
        }) {
            demands.push(demand(
                Resource::Material(MATERIAL_BONE.to_owned()),
                estimate.hoes * HOE_RECIPE_BONE,
                DEMAND_PRIORITY_HOES,
            ));
            demands.push(demand(
                Resource::Material(MATERIAL_FIBRE.to_owned()),
                estimate.hoes * HOE_RECIPE_FIBRE,
                DEMAND_PRIORITY_HOES,
            ));
            demands.push(demand(
                Resource::Craft {
                    item: HOES_RECIPE_ID.to_owned(),
                    start_tick: estimate.start_tick,
                },
                estimate.hoes,
                DEMAND_PRIORITY_HOES,
            ));
        }
        demands
    }

    /// **The hoes the first cultivate would want, and when to start making them.** From the
    /// shape's first planned band that holds a patch with `tended_food > 0`, its richest such
    /// patch: `tended_keepers_hoed` keepers plus `founding_min_workers` builders (the wire's
    /// founding floor stands in for the crew the first cultivate would run with — a crew the sim
    /// would let stand on its own), one hoe each. The turn cultivation is expected known is
    /// `CULTIVATION_LESSON_COST / (LADDER_LEARN_RATE × the patch sites the shape works)` turns
    /// from now — the ladder charges one lesson per worked source per turn — and a crafter should
    /// start `hoes × HOE_RECIPE_WORK / (HOE_CRAFT_CREW × CRAFT_PROGRESS_PER_WORKER_TURN ×
    /// BARE_HAND_CRAFT_RATE)` turns before that, never before now. `None` when no planned band
    /// holds a climbable patch or the shape works no patch.
    fn hoe_estimate(
        shape: &Shape,
        sites: &[Site],
        band: &PopulationCohortState,
        tick: u64,
    ) -> Option<HoeEstimate> {
        let patch_sites_worked = shape
            .bands
            .iter()
            .flat_map(|planned| planned.sites.iter())
            .filter(|&&index| sites[index].herd_id.is_none())
            .count() as u32;
        if patch_sites_worked == 0 {
            return None;
        }
        let patch = shape.bands.iter().find_map(|planned| {
            planned
                .sites
                .iter()
                .map(|&index| &sites[index])
                .filter(|site| site.herd_id.is_none() && site.tended_food > 0.0)
                .max_by(|a, b| a.tended_food.total_cmp(&b.tended_food))
        })?;
        let hoes = patch.tended_keepers_hoed + band.founding_min_workers;
        if hoes == 0 {
            return None;
        }
        let turns_to_known = (CULTIVATION_LESSON_COST
            / (LADDER_LEARN_RATE * patch_sites_worked as f32))
            .ceil() as u64;
        let craft_turns = (hoes as f32 * HOE_RECIPE_WORK
            / (HOE_CRAFT_CREW as f32 * CRAFT_PROGRESS_PER_WORKER_TURN * BARE_HAND_CRAFT_RATE))
            .ceil() as u64;
        let expected_known = tick + turns_to_known;
        Some(HoeEstimate {
            hoes,
            start_tick: expected_known.saturating_sub(craft_turns).max(tick),
        })
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

    /// **Rule 5a — hold the ground.** The band's `agriculture` pool is **one pool against its
    /// summed plant bill** (`LaborTarget::Agriculture`, `systems::labor::maintenance_shares`):
    /// the sim divides the pool's head count across every forage row the band holds — **with or
    /// without hands on it**, `keeping_claims` walks the band's assignments whatever their
    /// `workers` — whose patch has work on the ladder. So the bill is the band's, not a patch's:
    /// when any owned patch it holds a row on reads `upkeep_shortfall > 0`, the pool is set to
    /// Σ `upkeep_workers_needed` over every such patch, less what it holds, and one more hand
    /// ([`HOLD_MIN_HANDS`]) when the pool already stands at the sum and a patch still reads
    /// short — a bare keeper delivers under the `PER_WORKER_OUTPUT` the wire's `workers_needed`
    /// is `ceil`ed by (49,5 on seed 23 read `need 1, supplied 0.98, short 0.92`). The kit is
    /// left `None`, so the wire derives `tillage`; the hoes are the board's business. Sized per
    /// patch less the whole pool it read `want 0` for 49,5 while the pool's two hands kept 53,8,
    /// and skipping a row the band had emptied it never proposed for 49,5 again; the patch
    /// unwound at t48 with two holds accepted twenty turns earlier.
    ///
    /// The hands come from the surplus first, then the lowest rows, and the change is priced
    /// like any reassignment: what they earned where they stood against **the rungs lost** — an
    /// unpaid bill costs the whole improvement. What holding keeps, as a series over the horizon,
    /// summed over the short patches: the rung's premium per turn (the tile's `tended_yield`,
    /// `field_yield` on a field, less the wild take the row's hands make on that patch) **once
    /// the rung is complete** (`is_cultivated` / `is_field` — the bill runs during the build too,
    /// but a patch mid-build earns no premium yet), and the rebuild the seat would otherwise
    /// declare again — the work already done (`cultivation_work_done`, `field_work_done`; the
    /// full cost once complete) in builder-turns (`/ build_work_per_worker_turn`) at the row's
    /// own rate (the patch's forecast rate on an emptied row) — **on the horizon's last turn**,
    /// where an avoided cost belongs: it raises the runway the goal is held against and never
    /// the trough. Put at the first turn instead it read as food in hand, the survival check
    /// lied, and band 4 on seed 23 gave its last forage hand to a hold at t25 and starved.
    /// **On a completed rung the proposal is `standing`** — a bill the arbiter pays before any
    /// bid is weighed (`arbiter.rs`); mid-build it is a bid like any other. **A bill the band
    /// cannot pay without starving is defaulted on** — the projection with the bill paid must
    /// `survives` (trough above zero), as every other rule's must: paid unconditionally, band 2
    /// on seed 24 held 7,18 three times at a projected trough of -22, -40 and 2 with seven
    /// hands, and by t32 kept three, built with four and fed nobody. Over sixty seeds the
    /// unconditional bill ended with 286 working, 1103 hunger deaths and 29 improved patches;
    /// gated, 399, 951 and 15 — survival is the purpose, so the gate stands. Fires before
    /// *upgrade the ground*: holding what the band has beats declaring the next rung. The fact
    /// that forced the rule: seed 23's cultivate on 49,5 completed at t44 and read
    /// `cultivated: false, 0.99` at t45, decaying a hundredth a turn to 0.84 at t60, with the
    /// tile's `upkeep` row at `demand 1.92, supplied 0.0, shortfall 1.92, workers_needed 2,
    /// kit_id tillage` the whole way and two builders still on the `builders` role — the role
    /// the upkeep wants is `agriculture`, and it pays a patch's bill whether or not the band
    /// still works that patch (band 4 supplied 51,9 with its forage row empty).
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
        // The patches the band's pool answers for ([`Food::kept_patches`]).
        let kept = self.kept_patches(view, band);
        let short: Vec<&(&LaborAssignmentState, &ForagePatchState)> = kept
            .iter()
            .filter(|(_, patch)| patch.upkeep_shortfall > 0.0)
            .collect();
        if short.is_empty() {
            return None;
        }
        let need: u32 = kept
            .iter()
            .map(|(_, patch)| patch.upkeep_workers_needed)
            .sum();
        let held = Self::workers_in_pool(band, ROLE_AGRICULTURE);
        let want = need.saturating_sub(held).max(HOLD_MIN_HANDS).min(budget);
        let idle = band.idle_workers.min(budget);
        // A patch reads short, so the keeper trim is silent: only builders can be released here.
        let released = self.pool_releases(view, band);
        let surplus_rows = Self::surplus_rows(memory, band);
        let mut pool = Self::rows_ascending(memory, band, ROLE_HUNT);
        pool.extend(Self::rows_ascending(memory, band, ROLE_FORAGE));
        let drawn = self.draw(memory, band, &released, &surplus_rows, &pool, want, idle);
        if drawn.hands < want {
            return None;
        }
        let book = Self::book(band);
        let horizon = self.floors.projection_horizon_turns;
        let before = project(&book, carried, horizon);
        let mut premium_total = 0.0;
        let mut rebuild_total = 0.0;
        let mut any_complete = false;
        let mut subjects = Vec::new();
        for (row, patch) in &short {
            let (rung_yield, work_done) = if patch.is_field {
                (patch.field_yield, patch.field_work_done)
            } else {
                (patch.tended_yield, patch.cultivation_work_done)
            };
            // The rung's premium over the wild take the row's hands make on the patch — earned
            // only once the rung is complete.
            let complete = patch.is_cultivated || patch.is_field;
            any_complete |= complete;
            let rate = patch_per_worker_yield(memory, band, patch);
            let wild = crew_take(
                row.workers,
                rate,
                patch.biomass * patch.provisions_per_biomass,
            );
            if complete {
                premium_total += (rung_yield - wild).max(0.0);
            }
            // The rebuild an unwound rung costs, once, at the horizon's end: the work already
            // done in builder-turns at the row's rate — the patch's forecast on an emptied row.
            let row_rate = if row.workers > 0 {
                Self::row_rate(memory, band, row)
            } else {
                rate
            };
            if patch.build_work_per_worker_turn > 0.0 {
                rebuild_total += work_done / patch.build_work_per_worker_turn * row_rate;
            }
            subjects.push(format!(
                "{},{} short {:.2}",
                patch.x, patch.y, patch.upkeep_shortfall
            ));
        }
        let mut series = vec![premium_total; horizon as usize];
        if let Some(last) = series.last_mut() {
            *last += rebuild_total;
        }
        let change = Change::Series {
            income_lost: drawn.income_lost,
            income_gained: series,
        };
        let after = project_changes(&book, &[Change::Flat(*carried), change], horizon);
        let progress = goal_progress(&goals, &before, &after);
        // A bill the band cannot pay without starving is defaulted on: the rung unwinds and the
        // people live (`survives`, as every other rule reads it).
        if progress <= 0.0 || !survives(&after) {
            return None;
        }
        let mut commands = self.reduction_commands(band, &drawn);
        commands.push(self.assign_pool(band, ROLE_AGRICULTURE, held + want));
        let flat = Reassignment {
            income_lost: drawn.income_lost,
            income_gained: premium_total,
            payoff_turn: 0,
        };
        Some((
            Proposal {
                cost: Cost::claimed(want, band.band_id, &commands),
                commands,
                intent: intent_key(SPECIALIST_FOOD, INTENT_HOLD, band.band_id),
                score: progress * self.weight,
                reason: format!(
                    "{REASON_HOLD_GROUND}: {want} hands on agriculture for {} [{}]",
                    subjects.join(", "),
                    ledger_note(&after)
                ),
                memo: None,
                // A bill on a completed rung, not a bid: paid before anything is weighed.
                standing: any_complete,
            },
            flat,
        ))
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
                    survives(&after).then(|| {
                        let commands = vec![self.assign_at_floor(band, *tile, row.workers, None)];
                        Proposal {
                            cost: Cost::claimed(0, band.band_id, &commands),
                            commands,
                            intent: intent_key(SPECIALIST_FOOD, INTENT_DRAWDOWN, band.band_id),
                            score: goal_progress(&goals, &before, &after) * self.weight,
                            reason: format!(
                                "{REASON_FLOOR_RESTORE}: {} [{}]",
                                SourceKey::Patch(*tile).describe(),
                                ledger_note(&after)
                            ),
                            memo: None,
                            standing: false,
                        }
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
            let commands = vec![self.assign_at_floor(band, tile, row.workers, Some(floor))];
            best = Some((
                Proposal {
                    cost: Cost::claimed(0, band.band_id, &commands),
                    commands,
                    intent: intent_key(SPECIALIST_FOOD, INTENT_DRAWDOWN, band.band_id),
                    score: progress * self.weight,
                    reason: format!(
                        "{REASON_DRAW_DOWN}: {} to floor {floor:.1}, {spent} [{}]",
                        SourceKey::Patch(tile).describe(),
                        ledger_note(&after)
                    ),
                    memo: None,
                    standing: false,
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
        a_view, food, goals, memory, memory_forecasting, own_band, plan_toward,
        plan_with_food_share, ARMED_ATTACK, BAND, BARE_KIT, FACTION, FAR_PATCH, FIXTURE_BODY_FOOD,
        FORAGE_KIT, HERD_AT, HERD_ID, HERE, HUNT_KIT, HUNT_KIT_UNITS, NEAR_PATCH, PATCH_WINDOW,
        RICH_PATCH, STOCK, TICK, WORK_RANGE,
    };
    use super::*;
    use crate::ground::{Classified, GroundReadings, PlannedBand, Reading, StartKind};
    use crate::oracle::curve_of;
    use crate::orchestrator::Plan;
    use crate::profile::AiProfiles;
    use crate::specialists::food::honest_ceiling;
    use crate::specialists::Specialist;
    use sim_runtime::{
        BuildQueueEntryState, CohortStoreState, HerdTelemetryState, IntensificationKnowledgeState,
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
        assert!(proposal.cost.moves.is_empty());
        assert_eq!(
            proposal.cost.rows,
            vec![format!("{BAND}:forage:{},{}", RICH_PATCH.x, RICH_PATCH.y)]
        );
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
    /// a herd is a source only for a band holding a unit of the kit the herd's row names, and
    /// for exactly as many hands as it holds units. (Rewritten from the `kit_tiers` reading: the
    /// gate is now the per-herd kit's units in `equipment_batches`, which also bounds the crew.)
    #[test]
    fn a_herd_is_a_source_for_exactly_the_kit_units_held() {
        let spears = |count: u32| {
            a_view_with(|view| {
                // The rich patch out of sight, the herd big enough to out-earn the near patch.
                view.snapshot.visibility_raster.samples
                    [(RICH_PATCH.y * 8 + RICH_PATCH.x) as usize] = 0;
                view.snapshot.herds[0].biomass = 100.0;
                view.snapshot.populations[0].equipment_batches[0].count = count;
            })
        };
        let plan = plan_with_food_share(1.0);
        let specialist = food();
        let herd_source = |view: &SeatView| {
            specialist
                .reachable_sources(view, &memory_forecasting(view), own_band(view))
                .into_iter()
                .find(|source| matches!(source.key, SourceKey::Herd(_)))
        };
        // Spears for all: the herd credits every hand, and takes them all.
        let armed = spears(17);
        let source = herd_source(&armed).expect("a source");
        assert_eq!(source.crew_cap, Some(17));
        let proposal = specialist
            .negative_income(&armed, &plan, &memory_forecasting(&armed), own_band(&armed))
            .expect("a proposal");
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_HUNT.to_owned(), 17, None, Some(HERD_ID.to_owned()))
        );
        // One spear: a source for one hand — the second adds nothing, and is not sent.
        let one = spears(1);
        let source = herd_source(&one).expect("a source");
        assert_eq!(source.crew_cap, Some(1));
        assert_eq!(source.expected(1), 1.5);
        assert_eq!(source.expected(12), 1.5, "hands past the unit add nothing");
        assert_eq!(source.marginal(1, 11), 0.0);
        assert_eq!(source.usable(0, 12), 1);
        assert_eq!(source.usable(1, 12), 0);
        let mut alone = SeatView {
            snapshot: one.snapshot.clone(),
            last_acted_tick: None,
        };
        alone.snapshot.forage_patches.clear();
        let proposal = specialist
            .negative_income(&alone, &plan, &memory_forecasting(&alone), own_band(&alone))
            .expect("the one hand");
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_HUNT.to_owned(), 1, None, Some(HERD_ID.to_owned()))
        );
        assert_eq!(proposal.cost.workers, 1);
        // The one spear already on this herd: a sibling herd under the same kit is not a source
        // — the units are the band's, not the herd's.
        let mut committed = SeatView {
            snapshot: one.snapshot.clone(),
            last_acted_tick: None,
        };
        let sibling = "herd_10";
        committed.snapshot.herds.push(HerdTelemetryState {
            id: sibling.to_owned(),
            ..committed.snapshot.herds[0].clone()
        });
        committed.snapshot.populations[0].labor_assignments = vec![hunt_row(1, 1.5, 1)];
        committed.snapshot.populations[0].idle_workers = 16;
        let sources = specialist.reachable_sources(
            &committed,
            &memory_forecasting(&committed),
            own_band(&committed),
        );
        let herd_caps: Vec<(String, Option<u32>)> = sources
            .iter()
            .filter_map(|source| match &source.key {
                SourceKey::Herd(id) => Some((id.clone(), source.crew_cap)),
                SourceKey::Patch(_) => None,
            })
            .collect();
        assert_eq!(herd_caps, vec![(HERD_ID.to_owned(), Some(1))]);
        // No spear: not a source, for rule 1 or rule 4.
        let bare = spears(0);
        assert!(herd_source(&bare).is_none());
        let proposal = specialist
            .negative_income(&bare, &plan, &memory_forecasting(&bare), own_band(&bare))
            .expect("the near patch");
        assert_eq!(assigned_to(&proposal.commands[0]).0, ROLE_FORAGE);
        assert!(
            specialist
                .spare_hands_into_hunts(
                    &bare,
                    &plan,
                    &memory_forecasting(&bare),
                    own_band(&bare),
                    &Reassignment::NONE
                )
                .is_none(),
            "nor does rule 4 see it"
        );
        // The kit is the herd's own row, else the hunt job's default; a kit that carries nothing
        // bounds nothing; a kit the roster does not list arms nobody.
        let mut fallback = spears(2);
        fallback.snapshot.herds[0].default_kit_id.clear();
        fallback.snapshot.default_hunt_kit_id = HUNT_KIT.to_owned();
        assert_eq!(herd_source(&fallback).map(|s| s.crew_cap), Some(Some(2)));
        fallback.snapshot.herds[0].default_kit_id = BARE_KIT.to_owned();
        // A kit that carries nothing bounds nothing; the curve's plateau still does.
        assert_eq!(
            herd_source(&fallback).map(|s| s.crew_cap),
            Some(Some(HUNT_KIT_UNITS))
        );
        fallback.snapshot.herds[0].default_kit_id = "no_such_kit".to_owned();
        assert!(herd_source(&fallback).is_none());
        // The outfit demand is the roster's business, not the batches': bare, the window still
        // asks for the hunting kit.
        let mut window = spears(0);
        window.snapshot.populations[0].loadout_window = Some(sim_runtime::BandLoadoutWindowState {
            open: true,
            kit_budget: 17,
            material_budget: 30,
            ..Default::default()
        });
        window.snapshot.herds[0].regrowth_samples = vec![-1.0, 0.5, 1.0, 1.0, 0.5, 0.0];
        window.snapshot.herds[0].per_worker_biomass = 40.0;
        // A defense the roster's spear clears (the saturated fixture's herd is armoured).
        window.snapshot.herds[0].defense = 5.0;
        let demands = specialist.outfit_demands(
            &window,
            &memory_forecasting(&window),
            own_band(&window),
            None,
        );
        assert!(
            demands
                .iter()
                .any(|demand| demand.resource == Resource::Kit(HUNT_KIT.to_owned())),
            "{demands:?}"
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
        let demands = food().outfit_demands(&view, &memory(), own_band(&view), None);
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
        let demands = food().outfit_demands(&tough, &memory(), own_band(&tough), None);
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
        let demands = food().outfit_demands(&heavy, &memory(), own_band(&heavy), None);
        assert_eq!(demands[1].resource.to_string(), format!("kit:{HUNT_KIT}"));
        // No window: nothing asked.
        let mut closed = a_band_at_its_window(5.0);
        closed.snapshot.populations[0].loadout_window = None;
        assert!(food()
            .outfit_demands(&closed, &memory(), own_band(&closed), None)
            .is_empty());
    }

    // ---- the hoe estimate ----------------------------------------------------------------------

    /// A patch site at `tile` sustaining `hands` at `food` a turn; `tended_food` and its hoed
    /// keepers say whether it can climb.
    fn patch_site(tile: Tile, food: f32, hands: u32, tended_food: f32, keepers: u32) -> Site {
        Site {
            x: tile.x,
            y: tile.y,
            herd_id: None,
            reach: WORK_RANGE,
            sustained_food: food,
            sustained_hands: hands,
            tended_food,
            tended_hands: hands + keepers,
            tended_hands_hoed: hands + keepers,
            tended_keepers_hoed: keepers,
            field_food: 0.0,
            field_hands: 0,
            field_hands_hoed: 0,
            kit_needed: None,
            kit_units_held: None,
            unforecast: false,
        }
    }

    /// A planned band standing on `tile` claiming `claimed` of `sites`, with `hands`.
    fn planned(tile: Tile, sites: &[Site], claimed: Vec<usize>, hands: u32) -> PlannedBand {
        let baskets = claimed
            .iter()
            .map(|&index| sites[index].sustained_hands)
            .sum();
        let food = claimed
            .iter()
            .map(|&index| sites[index].sustained_food)
            .sum();
        PlannedBand {
            x: tile.x,
            y: tile.y,
            sites: claimed,
            hands,
            people: hands * 30 / 17,
            food,
            tended_food: food,
            field_food: food,
            nearest_planned_distance: None,
            baskets,
            hunt_kits: std::collections::BTreeMap::new(),
        }
    }

    /// The reading for `band` over `sites` with `bands` planned, classified `Stay` with that
    /// shape as every covering — what a fixture hands the outfit where the composite hands the
    /// turn's reading.
    fn shaped(view: &SeatView, sites: Vec<Site>, bands: Vec<PlannedBand>) -> BandGround {
        let band = own_band(view);
        let candidates: Vec<Tile> = bands.iter().map(PlannedBand::tile).collect();
        let reading = Reading::from_sites(
            view.grid(),
            band_tile(band),
            band.size,
            band.working_age,
            (band.founding_min_workers, band.founding_parent_min_workers),
            band.food_consumption / band.size.max(1) as f32,
            sites,
            candidates,
        );
        let people: u32 = bands.iter().map(|planned| planned.people).sum();
        let food: f32 = bands.iter().map(|planned| planned.food).sum();
        let shape = Shape {
            people_fed: reading.people_fed(food),
            people_fed_tended: reading.people_fed(food),
            people_fed_field: reading.people_fed(food),
            people_uncovered: band.size.saturating_sub(people),
            move_target: None,
            bands,
        };
        let classified = Classified {
            kind: StartKind::Stay,
            stay: shape.clone(),
            local: shape.clone(),
            far: shape.clone(),
            visible: shape,
            move_target: None,
            around_target: None,
        };
        BandGround {
            reading,
            classified,
        }
    }

    /// Beside the value walk's kits, the hoe estimate: the shape's climbable patch's 2 hoed
    /// keepers plus the 4-hand founding crew, one hoe each — 6 bone, 12 fibre, and a craft of 6
    /// timed from the lesson (20 practice over 2 worked patches = 10 turns) less the bench time
    /// (6 × 5 work at 0.5 a turn = 60 turns, so now) — which the orchestrator declines: nothing
    /// crafts. A shape with no climbable patch, or no reading at all, posts the kits alone.
    #[test]
    fn an_open_window_posts_the_hoe_estimate_beside_the_kits_and_the_craft_is_declined() {
        use crate::board::{DemandState, Entry, Grant};
        use crate::orchestrator::constant::{ConstantStance, DECLINED_NO_CRAFTER};
        use crate::orchestrator::Orchestrator;
        let mut view = a_band_at_its_window(5.0);
        view.snapshot.opening_loadout.pickable_materials =
            vec!["bone".to_owned(), "fibre".to_owned()];
        // No pre-fill on this fixture: the line is the estimate's alone.
        view.snapshot.opening_loadout.material_defaults.clear();
        let sites = vec![
            patch_site(NEAR_PATCH, 3.0, 3, 8.0, 2),
            patch_site(RICH_PATCH, 2.0, 2, 0.0, 0),
        ];
        let bands = vec![planned(HERE, &sites, vec![0, 1], 6)];
        let ground = shaped(&view, sites, bands);
        let demands = food().outfit_demands(&view, &memory(), own_band(&view), Some(&ground));
        let asked: Vec<(String, u32, f32)> = demands
            .iter()
            .map(|d| (d.resource.to_string(), d.amount, d.priority))
            .collect();
        assert_eq!(
            asked,
            vec![
                ("kit:gathering".to_owned(), 16, DEMAND_PRIORITY_GATHERING),
                (format!("kit:{HUNT_KIT}"), 1, DEMAND_PRIORITY_HUNTING),
                ("material:bone".to_owned(), 6, DEMAND_PRIORITY_HOES),
                ("material:fibre".to_owned(), 12, DEMAND_PRIORITY_HOES),
                (format!("craft:hoes@t{TICK}"), 6, DEMAND_PRIORITY_HOES),
            ]
        );
        assert!(demands.iter().all(|d| d.band == BAND && d.by_tick == TICK));
        // The orchestrator: the kits and the materials are granted, the craft is declined.
        let entries: Vec<Entry> = demands
            .iter()
            .cloned()
            .map(|demand| Entry {
                demand,
                posted_tick: TICK,
                state: DemandState::Posted,
            })
            .collect();
        let borrowed: Vec<&Entry> = entries.iter().collect();
        let profile = AiProfiles::builtin().profile("forager").unwrap().clone();
        let mut orchestrator = ConstantStance::new(&[SPECIALIST_FOOD], 1, 0.0);
        let window = view.snapshot.populations[0].loadout_window.clone().unwrap();
        let outfit = orchestrator.outfit(&view, &profile, own_band(&view), &window, &borrowed);
        assert_eq!(
            outfit.kits,
            vec![(FORAGE_KIT.to_owned(), 16), (HUNT_KIT.to_owned(), 1)]
        );
        assert_eq!(
            outfit.materials,
            vec![("bone".to_owned(), 6), ("fibre".to_owned(), 12)]
        );
        assert_eq!(outfit.grants[4].1, Grant::declined(DECLINED_NO_CRAFTER));
        // No climbable patch: the kits alone.
        let sites = vec![patch_site(NEAR_PATCH, 3.0, 3, 0.0, 0)];
        let bands = vec![planned(HERE, &sites, vec![0], 6)];
        let no_climb = shaped(&view, sites, bands);
        let demands = food().outfit_demands(&view, &memory(), own_band(&view), Some(&no_climb));
        assert_eq!(demands.len(), 2, "{demands:?}");
        // The timing: with one worked patch the lesson takes 20 turns, and one hoe (no keepers,
        // a founding crew of 1) is 10 bench turns — the crafter starts at t+10.
        let mut one = a_band_at_its_window(5.0);
        one.snapshot.populations[0].founding_min_workers = 1;
        let sites = vec![patch_site(NEAR_PATCH, 3.0, 3, 8.0, 0)];
        let bands = vec![planned(HERE, &sites, vec![0], 6)];
        let ground = shaped(&one, sites, bands);
        let demands = food().outfit_demands(&one, &memory(), own_band(&one), Some(&ground));
        let craft = demands
            .iter()
            .find(|d| matches!(d.resource, Resource::Craft { .. }))
            .expect("a craft demand");
        assert_eq!(craft.amount, 1);
        assert_eq!(
            craft.resource,
            Resource::Craft {
                item: "hoes".to_owned(),
                start_tick: TICK + 20 - 10,
            }
        );
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
        assert_eq!(proposal.intent, format!("food:hold:{BAND}"));
        assert!(proposal.standing, "a completed rung's hold is a bill");
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
        // A band whose projection starves with the bill paid defaults on it: no hold.
        let mut starving = held(1.92);
        starving.snapshot.populations[0].food_income = 0.0;
        starving.snapshot.populations[0].labor_assignments[0].workers_needed = 17;
        assert!(food()
            .hold_the_ground(
                &starving,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&starving),
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

    /// ⛔ **THE BILL IS THE BAND'S, NOT A PATCH'S.** The pool keeps every patch the band holds a
    /// row on — an emptied row included — and is sized against their summed need: with two hands
    /// already keeping the rich patch (need 2, paid) and the near patch (need 1) short on a row
    /// with nobody on it, the hold adds the one hand the sum is short of, names only the short
    /// patch, and stands. Sized per patch less the whole pool it read `want 0` and never fired.
    #[test]
    fn a_band_keeps_every_patch_it_holds_a_row_on_from_one_pool() {
        const KEPT_ALREADY: u32 = 2;
        let mut view = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = band.food_consumption;
            band.labor_assignments = vec![
                LaborAssignmentState {
                    workers_needed: 8,
                    ..forage_row(RICH_PATCH, 15, 23.0)
                },
                forage_row(NEAR_PATCH, 0, 0.0),
                LaborAssignmentState {
                    kind: ROLE_AGRICULTURE.into(),
                    workers: KEPT_ALREADY,
                    ..Default::default()
                },
            ];
            for patch in &mut view.snapshot.forage_patches {
                let tile = Tile::new(patch.x, patch.y);
                if tile == RICH_PATCH || tile == NEAR_PATCH {
                    patch.owner = Some(FACTION);
                    patch.is_cultivated = true;
                    patch.cultivation_work_cost = 50.0;
                    patch.cultivation_work_done = 50.0;
                    patch.build_work_per_worker_turn = 1.0;
                }
                if tile == RICH_PATCH {
                    patch.tended_yield = 40.0;
                    patch.upkeep_demand = 1.92;
                    patch.upkeep_shortfall = 0.0;
                    patch.upkeep_workers_needed = 2;
                }
                if tile == NEAR_PATCH {
                    patch.tended_yield = 20.0;
                    patch.upkeep_demand = 0.96;
                    patch.upkeep_shortfall = 0.96;
                    patch.upkeep_workers_needed = 1;
                }
            }
        });
        let proposal = food()
            .hold_the_ground(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("the near patch's bill");
        assert_eq!(proposal.cost.workers, 1, "{}", proposal.reason);
        assert!(
            proposal
                .reason
                .starts_with("hold the ground: 1 hands on agriculture for 4,2 short 0.96 ["),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(proposal.commands.last().unwrap()),
            (ROLE_AGRICULTURE.to_owned(), KEPT_ALREADY + 1, None, None)
        );
        assert!(proposal.standing);
        // The pool at the sum and a patch still short: one more hand, not none.
        view.snapshot.populations[0].labor_assignments[2].workers = KEPT_ALREADY + 1;
        let one_more = food()
            .hold_the_ground(
                &view,
                &plan_with_food_share(1.0),
                &memory(),
                own_band(&view),
                &Reassignment::NONE,
            )
            .expect("one more hand");
        assert_eq!(one_more.cost.workers, HOLD_MIN_HANDS);
        assert_eq!(
            assigned_to(one_more.commands.last().unwrap()),
            (ROLE_AGRICULTURE.to_owned(), KEPT_ALREADY + 2, None, None)
        );
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
                overdraws: true,
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

    /// **A forage row at the default floor is overused on a take above its regrowth.** The sim's
    /// `overdraws` is never true there (`floor_overdraws` needs a floor below the food peak), so
    /// the row reads `actual_yield > sustainable_yield` on a patch at its floor: a take equal to
    /// the regrowth is not overuse, and neither is any take on a patch above its floor. Read
    /// through `overdraws` alone this row was never the one to empty first, and the forager seat
    /// starved on both bench seeds.
    #[test]
    fn a_forage_row_at_its_default_floor_is_overused_only_on_a_take_above_its_regrowth() {
        const REGROWTH: f32 = 2.0;
        const ABOVE_REGROWTH: f32 = 2.5;
        let reading = |actual_yield: f32, biomass_over_capacity: f32| {
            let view = a_view_with(|view| {
                let band = &mut view.snapshot.populations[0];
                band.idle_workers = 0;
                band.labor_assignments = vec![LaborAssignmentState {
                    sustainable_yield: REGROWTH,
                    overdraws: false,
                    ..forage_row(RICH_PATCH, 17, actual_yield)
                }];
                for patch in &mut view.snapshot.forage_patches {
                    if Tile::new(patch.x, patch.y) == RICH_PATCH {
                        patch.biomass = biomass_over_capacity * patch.carrying_capacity;
                    }
                }
            });
            let (row, _, troubled, why) = food()
                .row_to_empty(&view, &memory(), own_band(&view))
                .expect("one worked row");
            assert!(!row.overdraws);
            assert_eq!(row.floor, BEST_FLOOR, "the wire default");
            (troubled, why)
        };
        assert_eq!(
            reading(ABOVE_REGROWTH, BEST_FLOOR),
            (true, WHY_OVERUSED),
            "above its regrowth, at its floor"
        );
        assert_ne!(
            reading(REGROWTH, BEST_FLOOR).1,
            WHY_OVERUSED,
            "the regrowth taken exactly"
        );
        assert_ne!(
            reading(ABOVE_REGROWTH, BEST_FLOOR + FLOOR_STEP).1,
            WHY_OVERUSED,
            "above its floor"
        );
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
    /// Free hands are dealt across the cluster **by the room above each site's floor** when the
    /// sites send no regrowth curve (a sustained crew of nought): the rich patch's 12 of room at
    /// 2.0 takes six hands, the near patch's 4 at 1.0 four, the third's 1 at 0.5 two — twelve of
    /// the seventeen, and the five no site has room for stay idle. (Rewritten from the
    /// standing-stock plateau: the capacities are cut so the rich patch's standing 16 for all
    /// seventeen loses to the cluster's 17.)
    #[test]
    fn free_hands_are_dealt_across_the_cluster_up_to_each_sites_surplus_room() {
        let third = Tile::new(2, 1);
        let view = a_view_with(|view| {
            for patch in &mut view.snapshot.forage_patches {
                let tile = Tile::new(patch.x, patch.y);
                // The room is `biomass − 0.5 × K`; the surplus plateau `ceil(room / rate)`.
                if tile == RICH_PATCH {
                    patch.carrying_capacity = 8.0;
                    patch.biomass = 16.0;
                } else if tile == NEAR_PATCH {
                    patch.carrying_capacity = 4.0;
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
                    "{REASON_NEGATIVE_INCOME}: 12 idle hands -> forage 2,3 ×6 (sustained 0, surplus 6), forage 4,2 ×4 (sustained 0, surplus 4), forage 2,1 ×2 (sustained 0, surplus 2) ["
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
                (6, Some(RICH_PATCH)),
                (4, Some(NEAR_PATCH)),
                (2, Some(third))
            ]
        );
        assert_eq!(proposal.cost.workers, 12);
        // 12 + 4 + 1 = 17 a turn, against 16 for all seventeen on the rich patch's standing stock.
        assert!(
            proposal.reason.contains("positive again t0"),
            "{}",
            proposal.reason
        );
    }

    /// **Pass one before pass two.** The rich patch sustains three hands (regrowth 6 at 2.0) and
    /// the near patch two (regrowth 2 at 1.0); the rich patch stands 8 above its floor (room for
    /// four more at 2.0) and the near patch stands *at* its floor. The deal places 3 and 2 first,
    /// then four surplus hands onto the rich patch and none onto the near one; the other eight
    /// stay idle. Lifted above its floor by 6, the near patch takes six surplus hands too.
    #[test]
    fn free_hands_fill_each_sites_sustained_crew_before_any_surplus_and_a_patch_at_its_floor_takes_none(
    ) {
        let curves = |near_biomass: f32| {
            a_view_with(|view| {
                view.snapshot.herds.clear();
                for patch in &mut view.snapshot.forage_patches {
                    let tile = Tile::new(patch.x, patch.y);
                    if tile == RICH_PATCH {
                        patch.carrying_capacity = 12.0;
                        patch.biomass = 14.0;
                        patch.regrowth_samples = vec![6.0, 6.0, 6.0, 6.0, 6.0, 6.0];
                    } else if tile == NEAR_PATCH {
                        patch.carrying_capacity = 20.0;
                        patch.biomass = near_biomass;
                        patch.regrowth_samples = vec![2.0, 2.0, 2.0, 2.0, 2.0, 2.0];
                    }
                }
            })
        };
        let dealt_by = |view: &SeatView| {
            let proposal = food()
                .negative_income(view, &plan_with_food_share(1.0), &memory(), own_band(view))
                .expect("a proposal");
            let dealt: Vec<(u32, Option<Tile>)> = proposal
                .commands
                .iter()
                .map(|command| {
                    let (_, workers, tile, _) = assigned_to(command);
                    (workers, tile)
                })
                .collect();
            (proposal, dealt)
        };
        let at_floor = curves(10.0);
        let (proposal, dealt) = dealt_by(&at_floor);
        assert!(
            proposal.reason.starts_with(&format!(
                "{REASON_NEGATIVE_INCOME}: 9 idle hands -> forage 2,3 ×7 (sustained 3, surplus 4), forage 4,2 ×2 (sustained 2, surplus 0) ["
            )),
            "{}",
            proposal.reason
        );
        assert_eq!(dealt, vec![(7, Some(RICH_PATCH)), (2, Some(NEAR_PATCH))]);
        assert_eq!(proposal.cost.workers, 9);
        let above_floor = curves(16.0);
        let (proposal, dealt) = dealt_by(&above_floor);
        assert!(
            proposal.reason.starts_with(&format!(
                "{REASON_NEGATIVE_INCOME}: 15 idle hands -> forage 2,3 ×7 (sustained 3, surplus 4), forage 4,2 ×8 (sustained 2, surplus 6) ["
            )),
            "{}",
            proposal.reason
        );
        assert_eq!(dealt, vec![(7, Some(RICH_PATCH)), (8, Some(NEAR_PATCH))]);
    }

    /// A hand that would read surplus where it lands stays where it is: seventeen on a site
    /// reading `workers_needed 8`, the second site standing at its floor with its six hands
    /// already taking the floor's regrowth — its honest ceiling — so another hand's marginal
    /// there is nothing; nothing moves, and the same with the second site at its ceiling. (The
    /// near row pays its hands more than the rich one does, so rule 1's row-to-empty path has
    /// nowhere better to send the rich row either, and the free-hand path is what is pinned.)
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
        // The near patch at its floor, regrowing six a turn: its six hands take exactly that.
        let mut full = parked(LaborAssignmentState {
            workers_needed: 6,
            ..forage_row(NEAR_PATCH, 6, 6.0)
        });
        for patch in &mut full.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == NEAR_PATCH {
                patch.biomass = BEST_FLOOR * patch.carrying_capacity;
                patch.regrowth_samples = vec![6.0, 6.0];
            }
        }
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
                &memory_forecasting(&view),
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
                &memory_forecasting(&view),
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
                &memory_forecasting(&view),
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
        // The curve forecasts 18 a turn for twelve; the row's window at that rate is one turn
        // (one animal is one food), so one turn of 0.12 is the verdict.
        let mut memory = memory_forecasting(&view);
        memory.observe(&view, FACTION);
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
        // One turn: the row's window at the curve's 18 a turn (see the test above).
        let mut memory = memory_forecasting(&view);
        memory.observe(&view, FACTION);
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
    /// `reachable_sources`, on a row the specialist had staffed that way itself. (Now the
    /// sim's curve is the forecast, and it plateaus at two: twelve are forecast what two take.)
    #[test]
    fn an_over_staffed_hunt_row_is_measured_on_its_plateau_and_is_not_dead() {
        const ASSIGNED: u32 = 12;
        const PLATEAU: u32 = 2;
        let mut view = a_view();
        view.snapshot.herds[0].biomass = 100.0;
        // The curve: 1.5 for one, 3.0 from two on; the plateau of two takes 3.0, which is exactly it.
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 10.0,
            ..hunt_row(ASSIGNED, PLATEAU as f32 * 1.5, PLATEAU)
        }];
        view.snapshot.populations[0].idle_workers = 5;
        let specialist = food();
        let mut memory = memory();
        let points: Vec<(u32, f32)> = (1..=HUNT_KIT_UNITS)
            .map(|n| (n, (n.min(PLATEAU)) as f32 * 1.5))
            .collect();
        memory.remember_crew_take(BAND, HERD_ID, curve_of(&points, FIXTURE_BODY_FOOD));
        for _ in 0..4 {
            memory.observe(&view, FACTION);
        }
        // It is still a source, and not dead: twelve are forecast what two take, and took it.
        // (It credits no hand past its plateau, so the idle hands have nowhere to go on it.)
        assert!(!specialist.is_dead(
            &view,
            &memory,
            own_band(&view),
            &SourceKey::Herd(HERD_ID.into())
        ));
        let sources = specialist.reachable_sources(&view, &memory, own_band(&view));
        let herd = sources
            .iter()
            .find(|source| source.key == SourceKey::Herd(HERD_ID.into()))
            .expect("the herd is still a source");
        assert_eq!(herd.crew_cap, Some(PLATEAU));
        assert_eq!(herd.expected(ASSIGNED), 3.0);
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
        let mut memory = memory_forecasting(&view);
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

    /// **A kill turn is not overuse.** A hunt's take is lumpy — a kill cashes a whole banked
    /// animal, so `actual_yield` spikes above the steady `sustainable_yield` under any floor. The
    /// sim's `overdraws` is the verdict: the spike alone flags nothing, and `overdraws` still does.
    #[test]
    fn a_hunt_row_on_a_kill_turn_is_overused_only_when_the_sim_says_it_overdraws() {
        let mut view = a_view();
        view.snapshot.populations[0].idle_workers = 0;
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 2.0,
            overdraws: false,
            ..hunt_row(6, 8.0, 6)
        }];
        let (row, _, troubled, why) = food()
            .row_to_empty(&view, &memory(), own_band(&view))
            .expect("one worked row");
        assert!(row.actual_yield > row.sustainable_yield, "a kill turn");
        assert!(!troubled, "{why}");
        assert_ne!(why, WHY_OVERUSED);
        view.snapshot.populations[0].labor_assignments[0].overdraws = true;
        let (_, _, troubled, why) = food()
            .row_to_empty(&view, &memory(), own_band(&view))
            .expect("one worked row");
        assert!(troubled);
        assert_eq!(why, WHY_OVERUSED);
    }

    #[test]
    fn an_overused_row_is_emptied_first_onto_the_next_best_source() {
        let mut view = a_view();
        view.snapshot.populations[0].idle_workers = 0;
        view.snapshot.populations[0].labor_assignments = vec![LaborAssignmentState {
            sustainable_yield: 2.0,
            overdraws: true,
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
        // A west stand of K 40 at 3.0 a hand: its room above the floor (45) out-pays the rich
        // patch's (40) for seventeen hands, under the honest ceiling.
        let view = a_view_with(|view| add_patch(view, WEST_PATCH, 3.0, 40.0));
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

    /// [`add_patch`] with what a child's projection reads: a flat regrowth curve of `regrowth`
    /// a turn (so the site sustains `ceil(regrowth / rate)` hands) and each hand carrying its
    /// rate in biomass.
    fn add_site(view: &mut SeatView, tile: Tile, rate: f32, capacity: f32, regrowth: f32) {
        add_patch(view, tile, rate, capacity);
        let patch = view.snapshot.forage_patches.last_mut().unwrap();
        patch.regrowth_samples = vec![regrowth; 6];
        patch.per_worker_biomass = rate;
    }

    /// A band short of food with nothing better in reach: twelve hands on the near patch earning
    /// 12 against 16 eaten, the rich patch no site, and a site three tiles east sustaining six
    /// hands at 1.5 (regrowth 9).
    fn a_short_band() -> SeatView {
        a_view_with(|view| {
            view.snapshot
                .food_modules
                .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
            add_site(view, SPLIT_SITE, 1.5, 20.0, 9.0);
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

    /// (The crew reads 6, the site's sustained hands, where it read the profile's 5; the cost
    /// claims no workers.)
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
            CommandPayload::SplitBand { workers: 6, .. }
        ));
        assert_eq!(
            proposal.memo,
            Some(Memo::Split {
                band: BAND,
                target: SPLIT_SITE,
                workers: 6
            })
        );
        assert_eq!(proposal.cost.workers, 0, "a split is not labor churn");
        assert_eq!(proposal.cost.splits, vec![BAND]);
        assert!(proposal.cost.moves.is_empty(), "a split is not a walk");
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
        // Grown to seventeen: the refusal was of a band of twelve. (This memory has measured the
        // near row at 1.0 a hand, so the web's prior rates the site at that rather than its 1.5
        // forecast, and the site's nine of regrowth now sustains nine hands.)
        later.snapshot.populations[0].working_age = 17;
        let grown = specialist
            .split_to_feed(&later, &plan, &refused, own_band(&later), &carried)
            .expect("asked again, larger");
        assert!(matches!(
            grown.commands[0],
            CommandPayload::SplitBand { workers: 9, .. }
        ));
        // A site the split crew could not feed itself on is not a fix: half a food a hand
        // against the crew's share of what the band eats.
        let mut poor = SeatView {
            snapshot: view.snapshot.clone(),
            last_acted_tick: None,
        };
        for patch in &mut poor.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == SPLIT_SITE {
                patch.per_worker_yield = 0.5;
                patch.per_worker_biomass = 0.5;
            }
        }
        assert!(specialist
            .split_to_feed(&poor, &plan, &memory(), own_band(&poor), &carried)
            .is_none());
    }

    /// The child crew is the site's sustained hands, at least the founding floor, within what
    /// the parent may give up — and the rule is silent under the founding floor: with parent
    /// floor 6, founding floor 4 and a site sustaining six, ten hands split 4, nine split
    /// nothing, seventeen split the site's 6. (Was "seventeen split the profile's 5".)
    #[test]
    fn the_split_crew_is_sized_by_the_site_within_the_wires_floors() {
        let specialist = food();
        let plan = plan_with_food_share(1.0);
        let at = |working_age: u32| {
            a_view_with(|view| {
                view.snapshot
                    .food_modules
                    .retain(|site| site.x != RICH_PATCH.x || site.y != RICH_PATCH.y);
                // A site rich enough that any crew the floors allow feeds itself: six hands at
                // 2.0 sustained.
                add_site(view, SPLIT_SITE, 2.0, 40.0, 12.0);
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
                        assert_eq!(proposal.cost.workers, 0);
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
        assert_eq!(crew_at(17), Some(6), "the site's sustained crew");
        // A site sustaining fewer than the founding floor (three, at regrowth 6) still gets the
        // floor — a crew of four the site's six a turn feeds.
        let mut thin = at(17);
        for patch in &mut thin.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == SPLIT_SITE {
                patch.regrowth_samples = vec![6.0; 6];
            }
        }
        let (_, carried) = specialist.assess_income(&thin, &plan, &memory(), own_band(&thin));
        let proposal = specialist
            .split_to_feed(&thin, &plan, &memory(), own_band(&thin), &carried)
            .expect("the founding floor");
        assert!(matches!(
            proposal.commands[0],
            CommandPayload::SplitBand { workers: 4, .. }
        ));
    }

    /// The split fires on a negative projected net income even with the runway above the goal
    /// — a larder does not make a band that eats more than it earns fed — and not when the net
    /// is non-negative and the runway at the goal. It is budget-free: a plan funding `Food` a
    /// tenth of the pool (one hand) still gets the split, which claims no workers.
    #[test]
    fn the_split_fires_on_negative_net_income_whatever_the_runway_and_needs_no_budget() {
        let specialist = food();
        let stocked = |stock: f32, income: f32| {
            let mut view = a_short_band();
            let band = &mut view.snapshot.populations[0];
            band.food_income = income;
            band.stores = vec![CohortStoreState {
                item: FOOD_CARGO_KEY.to_owned(),
                quantity: (stock * FIXED_POINT_SCALE as f32) as i64,
            }];
            view
        };
        let plan = plan_with_food_share(1.0);
        // Two thousand in the larder: the runway at the horizon is far past the goal; net −4.
        let rich_larder = stocked(2000.0, 12.0);
        let (_, carried) =
            specialist.assess_income(&rich_larder, &plan, &memory(), own_band(&rich_larder));
        let after = project(
            &Food::book(own_band(&rich_larder)),
            &carried,
            specialist.floors.projection_horizon_turns,
        );
        assert!(after.runway_at_end > goals().runway_turns);
        assert!(after.net_after < 0.0);
        assert!(specialist
            .split_to_feed(
                &rich_larder,
                &plan,
                &memory(),
                own_band(&rich_larder),
                &carried
            )
            .is_some());
        // Breaking even on the same larder: nothing to fix.
        let fed = stocked(2000.0, 16.0);
        let (_, carried) = specialist.assess_income(&fed, &plan, &memory(), own_band(&fed));
        assert!(specialist
            .split_to_feed(&fed, &plan, &memory(), own_band(&fed), &carried)
            .is_none());
        // A tenth of the pool funds one hand; the split needs none.
        let thin_plan = plan_with_food_share(0.1);
        let short = a_short_band();
        assert_eq!(specialist.budget_workers(&short, &thin_plan), 1);
        let (_, carried) =
            specialist.assess_income(&short, &thin_plan, &memory(), own_band(&short));
        let proposal = specialist
            .split_to_feed(&short, &thin_plan, &memory(), own_band(&short), &carried)
            .expect("budget-free");
        assert_eq!(proposal.cost.workers, 0);
    }

    /// A feasible site in the near ring beats a richer one in the far ring; the far ring is
    /// used when the near ring has nothing feasible; a site the child could not survive on is
    /// never chosen, whichever ring it is in.
    #[test]
    fn the_split_prefers_the_near_ring_and_falls_back_to_the_far_ring() {
        let specialist = food();
        let plan = plan_with_food_share(1.0);
        let view = a_short_band();
        let grid = view.grid();
        let (near, far) = (
            specialist.floors.split_search_tiles,
            specialist.floors.split_reach_tiles,
        );
        assert!(grid.distance(HERE, SPLIT_SITE) <= near);
        let far_distance = grid.distance(HERE, FAR_PATCH);
        assert!(far_distance > near && far_distance <= far, "{far_distance}");
        // The fixture's far patch, given a curve and a carry: nine a hand, sustaining three.
        let with_far_site = |view: &SeatView| {
            let mut view = SeatView {
                snapshot: view.snapshot.clone(),
                last_acted_tick: None,
            };
            for patch in &mut view.snapshot.forage_patches {
                if Tile::new(patch.x, patch.y) == FAR_PATCH {
                    patch.regrowth_samples = vec![27.0; 6];
                    patch.per_worker_biomass = 9.0;
                }
            }
            view
        };
        let target_of = |view: &SeatView| {
            let (_, carried) = specialist.assess_income(view, &plan, &memory(), own_band(view));
            specialist
                .split_to_feed(view, &plan, &memory(), own_band(view), &carried)
                .map(|proposal| match proposal.memo {
                    Some(Memo::Split { target, .. }) => target,
                    other => panic!("not a split memo: {other:?}"),
                })
        };
        // Both feasible: the near site, though the far one pays six times as much.
        let both = with_far_site(&view);
        assert_eq!(target_of(&both), Some(SPLIT_SITE));
        // The near site no longer a gathering site: the far ring.
        let mut far_only = with_far_site(&view);
        far_only
            .snapshot
            .food_modules
            .retain(|site| site.x != SPLIT_SITE.x || site.y != SPLIT_SITE.y);
        assert_eq!(target_of(&far_only), Some(FAR_PATCH));
        // The far site at a carry the child starves on: nothing, though it is in reach.
        let mut starving = far_only;
        for patch in &mut starving.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == FAR_PATCH {
                patch.per_worker_biomass = 0.1;
            }
        }
        assert_eq!(target_of(&starving), None);
        // Without the far site's curve the fixture's far patch feeds nobody, and the near site
        // out of reach of the near ring is the far ring's — nothing without a feasible site.
        assert_eq!(target_of(&view), Some(SPLIT_SITE));
        // Water is not settled on: the near site on a water tile is skipped for the far one.
        let mut flooded = with_far_site(&view);
        for tile in &mut flooded.snapshot.tiles {
            if Tile::new(tile.x, tile.y) == SPLIT_SITE {
                tile.terrain_tags = sim_runtime::TerrainTags::WATER;
            }
        }
        assert_eq!(target_of(&flooded), Some(FAR_PATCH));
        // A site is one child's: with a child of this seat still walking to the near site, the
        // parent's next split looks past it to the far ring — and the same with another own
        // band standing on it.
        let mut walking = with_far_site(&view);
        walking.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: CHILD,
            current_x: HERE.x,
            current_y: HERE.y,
            working_age: 4,
            work_range: 2,
            ..Default::default()
        });
        let memory = memory_with_a_birth(&walking, BAND, CHILD, SPLIT_SITE);
        let (_, carried) = specialist.assess_income(&walking, &plan, &memory, own_band(&walking));
        let proposal = specialist
            .split_to_feed(&walking, &plan, &memory, own_band(&walking), &carried)
            .expect("the far ring");
        assert_eq!(
            proposal.memo.map(|memo| match memo {
                Memo::Split { target, .. } => target,
                other => panic!("{other:?}"),
            }),
            Some(FAR_PATCH)
        );
        let mut settled = with_far_site(&view);
        settled.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: CHILD,
            current_x: SPLIT_SITE.x,
            current_y: SPLIT_SITE.y,
            working_age: 4,
            work_range: 2,
            ..Default::default()
        });
        assert_eq!(target_of(&settled), Some(FAR_PATCH));
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
        assert_eq!(proposal.cost.moves, vec![CHILD]);
        // Accepted: the memory holds the move under the settle intent, and it is re-proposed.
        memory.record_choices(TICK, [(proposal.intent.clone(), proposal.memo)].into_iter());
        assert_eq!(memory.move_intent(CHILD), Some("food:settle:7002"));
        let again = specialist
            .settle(&view, &plan, &memory, child)
            .expect("persists");
        assert_eq!(again.intent, proposal.intent);
        // The whole roster: the child's one proposal is the settle, not an assignment.
        let mut specialist = food();
        let proposals = specialist.propose(&view, &plan, &memory, &GroundReadings::default());
        let for_child: Vec<&str> = proposals
            .proposals
            .iter()
            .filter(|p| p.intent.ends_with(&format!(":{CHILD}")))
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

    // ---- the crew-take curve ---------------------------------------------------------------------

    /// **A herd is forecast by the sim's crew take, through the oracle.** With a curve cached
    /// for it, the herd's `expected(n)` is the curve's likely at `n`, `marginal` the difference,
    /// `per_worker_yield` the smallest crew's likely, and its crew is capped at the plateau; with
    /// no curve it is not a source at all, whatever the row's `per_worker_yield` says.
    #[test]
    fn a_herd_is_forecast_by_the_curve_and_is_no_source_without_one() {
        let view = a_view_with(|view| view.snapshot.herds[0].biomass = 100.0);
        let specialist = food();
        let herd_of = |memory: &SeatMemory| {
            specialist
                .reachable_sources(&view, memory, own_band(&view))
                .into_iter()
                .find(|source| matches!(source.key, SourceKey::Herd(_)))
        };
        assert!(
            herd_of(&memory()).is_none(),
            "no curve, no source — the row's 1.5 a hand is the kit's carry"
        );
        let mut memory = memory();
        memory.remember_crew_take(
            BAND,
            HERD_ID,
            curve_of(
                &[(1, 0.1), (2, 0.25), (3, 0.5), (4, 0.7), (5, 0.8), (6, 0.8)],
                FIXTURE_BODY_FOOD,
            ),
        );
        let herd = herd_of(&memory).expect("a source once the sim has answered");
        assert_eq!(herd.per_worker_yield, 0.1);
        assert_eq!(herd.expected(5), 0.8);
        assert_eq!(herd.expected(9), 0.8, "no hand past the plateau counts");
        assert_eq!(herd.marginal(2, 3), 0.8 - 0.25);
        assert_eq!(
            herd.crew_cap,
            Some(5),
            "the plateau, under seventeen spears"
        );
        assert_eq!(
            herd.describe_for(5),
            "hunt herd_9: 5 hunters, likely 0.80/turn (sim crew take, low 0.80 high 0.80)"
        );
    }

    /// **Rule 1 reads the curve, not the rate.** A forage row at 0.44 a hand is the band's
    /// lowest row; the herd in reach is forecast 0.3 for five hunters by the sim. Before the
    /// curve the herd read 1.5 a hand off the wire and the row was emptied onto it (seed 54:
    /// five hunters read 4.0 a turn, the boar paid 0.24). Now the row stays; and a herd the sim
    /// forecasts at five a turn for five draws it, quoting the curve.
    #[test]
    fn a_forage_row_is_not_emptied_onto_a_herd_the_sim_forecasts_less_for() {
        const HANDS: u32 = 5;
        let view = a_view_with(|view| {
            view.snapshot
                .forage_patches
                .retain(|patch| Tile::new(patch.x, patch.y) == NEAR_PATCH);
            view.snapshot.herds[0].biomass = 100.0;
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.working_age = HANDS;
            band.labor_assignments = vec![forage_row(NEAR_PATCH, HANDS, 0.44 * HANDS as f32)];
        });
        let specialist = food();
        let plan = plan_with_food_share(1.0);
        let mut poor = memory();
        poor.remember_crew_take(
            BAND,
            HERD_ID,
            curve_of(&[(1, 0.06), (5, 0.3)], FIXTURE_BODY_FOOD),
        );
        assert!(
            specialist
                .negative_income(&view, &plan, &poor, own_band(&view))
                .is_none(),
            "0.3 for five does not beat 2.2 for five"
        );
        let mut rich = memory();
        rich.remember_crew_take(
            BAND,
            HERD_ID,
            curve_of(&[(1, 1.0), (5, 5.0)], FIXTURE_BODY_FOOD),
        );
        let proposal = specialist
            .negative_income(&view, &plan, &rich, own_band(&view))
            .expect("five a turn for five does");
        assert_eq!(
            assigned_to(proposal.commands.last().unwrap()),
            (ROLE_HUNT.to_owned(), HANDS, None, Some(HERD_ID.to_owned()))
        );
        assert!(
            proposal.reason.contains(
                "hunt herd_9: 5 hunters, likely 5.00/turn (sim crew take, low 5.00 high 5.00)"
            ),
            "{}",
            proposal.reason
        );
    }

    /// **A zero turn does not re-rate a herd.** A hunt pays in whole animals; a turn of nothing
    /// before the kill is due is not a rate. The herd's rank stays the curve's, and the row is
    /// not dead while its window runs. (A patch is still ranked on what it realized.)
    #[test]
    fn a_herds_rank_survives_a_zero_turn() {
        const CREW: u32 = 3;
        let view = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.labor_assignments = vec![hunt_row(CREW, 0.0, CREW)];
        });
        let specialist = food();
        let mut hunted = memory();
        // Two food a turn for three, off animals of four food each: a kill every second turn.
        hunted.remember_crew_take(BAND, HERD_ID, curve_of(&[(1, 1.5), (3, 2.0)], 4.0));
        hunted.observe(&view, FACTION);
        let herd_key = SourceKey::Herd(HERD_ID.into());
        assert!(!specialist.is_dead(&view, &hunted, own_band(&view), &herd_key));
        let herd = specialist
            .reachable_sources(&view, &hunted, own_band(&view))
            .into_iter()
            .find(|source| source.key == herd_key)
            .expect("still a source");
        assert_eq!(herd.per_worker_yield, 1.5);
        assert_eq!(herd.expected(CREW), 2.0);
        assert_eq!(
            Food::rate(&hunted, own_band(&view), &herd_key, 0.8),
            0.8,
            "a herd's rate is whatever it is handed, never the realized zero"
        );
        // A patch, for contrast, is re-rated by what it realized.
        let patch = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.labor_assignments = vec![forage_row(NEAR_PATCH, 4, 0.4)];
        });
        let mut gathered = memory();
        gathered.observe(&patch, FACTION);
        assert_eq!(
            Food::rate(
                &gathered,
                own_band(&patch),
                &SourceKey::Patch(NEAR_PATCH),
                1.0
            ),
            0.1
        );
    }

    /// **A hunt row in its window is priced at its forecast, not its zero turn.** One hunter
    /// forecast 0.5 a turn off animals of two food each has a window of four turns: on a zero
    /// turn the row still pays 0.5 a hand (`Food::row_rate`), so the forage row at 0.4 is the
    /// band's lowest, not the hunt; once the window has run with nothing taken, the row pays
    /// what it realized — nothing — and is the lowest.
    #[test]
    fn a_hunt_row_in_its_window_is_priced_at_its_forecast() {
        const LIKELY: f32 = 0.5;
        const BODY_FOOD: f32 = 2.0;
        let view = a_view_with(|view| {
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.labor_assignments = vec![forage_row(NEAR_PATCH, 4, 1.6), hunt_row(1, 0.0, 1)];
        });
        let band = own_band(&view);
        let hunt = &band.labor_assignments[1];
        let mut hunted = memory();
        hunted.remember_crew_take(BAND, HERD_ID, curve_of(&[(1, LIKELY)], BODY_FOOD));
        hunted.observe(&view, FACTION);
        assert_eq!(Food::row_rate(&hunted, band, hunt), LIKELY);
        let (row, _, _, why) = food()
            .row_to_empty(&view, &hunted, band)
            .expect("two worked rows");
        assert_eq!((row.kind.as_str(), why), (ROLE_FORAGE, WHY_LOWEST_ROW));
        // Four turns of nothing: the window has run, and the row pays what it took.
        for _ in 0..3 {
            hunted.observe(&view, FACTION);
        }
        assert_eq!(Food::row_rate(&hunted, band, hunt), 0.0);
        let (row, _, troubled, _) = food()
            .row_to_empty(&view, &hunted, band)
            .expect("two worked rows");
        assert_eq!((row.kind.as_str(), troubled), (ROLE_HUNT, true), "dead now");
        // With no curve the row is priced like a patch, at its take.
        assert_eq!(Food::row_rate(&memory(), band, hunt), 0.0);
    }

    /// **A merely lowest row does not land on a patch that has no room for it.** The one
    /// hunter's row is the lowest (no curve: it pays its zero); the only patch in reach stands
    /// at its floor with its eight hands taking the floor's regrowth — its honest ceiling — so
    /// the hunter's marginal there is nothing and it stays; with room above the floor the hunter
    /// moves. The frame's `workers_needed` reads `8` either way: the row-full reading is not
    /// what holds the hunter. Before the guard the hunter bounced onto the full patch and back
    /// every other turn of seed 54's t26–t45.
    #[test]
    fn a_lowest_row_does_not_land_where_its_hands_would_be_surplus() {
        let a_band_with_patch_at = |biomass_share: f32| {
            a_view_with(|view| {
                view.snapshot
                    .forage_patches
                    .retain(|patch| Tile::new(patch.x, patch.y) == RICH_PATCH);
                for patch in &mut view.snapshot.forage_patches {
                    patch.biomass = biomass_share * patch.carrying_capacity;
                    // Eight hands' worth of regrowth at the rich patch's 2.0 a hand.
                    patch.regrowth_samples = vec![16.0, 16.0];
                }
                let band = &mut view.snapshot.populations[0];
                band.idle_workers = 0;
                band.labor_assignments = vec![
                    LaborAssignmentState {
                        workers_needed: 8,
                        ..forage_row(RICH_PATCH, 8, 16.0)
                    },
                    hunt_row(1, 0.0, 1),
                ];
            })
        };
        let plan = plan_with_food_share(1.0);
        let full = a_band_with_patch_at(BEST_FLOOR);
        assert!(
            food()
                .negative_income(&full, &plan, &memory(), own_band(&full))
                .is_none(),
            "the patch gives its regrowth and no more: the hunter stays"
        );
        let room = a_band_with_patch_at(1.0);
        let proposal = food()
            .negative_income(&room, &plan, &memory(), own_band(&room))
            .expect("the patch has room above its floor");
        assert!(
            proposal.reason.contains(WHY_LOWEST_ROW),
            "{}",
            proposal.reason
        );
        assert_eq!(
            assigned_to(proposal.commands.last().unwrap()),
            (ROLE_FORAGE.to_owned(), 9, Some(RICH_PATCH), None)
        );
    }

    /// **Seed 23's t45–t52 shuffle is held by the marginal test alone.** Two patches at their
    /// Best floor, each with a crew of two taking exactly its regrowth, and one hand over on one
    /// of them: by the standing stock each patch has room for one more hand (the fixture pins
    /// that reading), and the frame reads the spare hand as surplus wherever it stands — the
    /// shape rule 1 sent back and forth every turn. Against the honest ceiling the other patch's
    /// marginal for that hand is nothing, so it stays, whichever patch it stands on, with no
    /// row-full reading in the way.
    #[test]
    fn the_seed_23_shuffle_is_held_by_the_marginal_test_alone() {
        const CREW: u32 = 2;
        let a_band_with_the_spare_hand_on = |spare_on: Tile| {
            a_view_with(|view| {
                view.snapshot.herds.clear();
                view.snapshot.forage_patches.retain(|patch| {
                    [NEAR_PATCH, RICH_PATCH].contains(&Tile::new(patch.x, patch.y))
                });
                for patch in &mut view.snapshot.forage_patches {
                    patch.biomass = BEST_FLOOR * patch.carrying_capacity;
                    // The crew's worth of regrowth at each patch's own rate (1.0 near, 2.0 rich).
                    patch.regrowth_samples = vec![CREW as f32 * patch.per_worker_yield; 2];
                    // The standing stock says a third hand would take more here; the honest
                    // ceiling says the regrowth is spoken for.
                    let standing = patch.biomass * patch.provisions_per_biomass;
                    assert!(
                        crew_take(CREW + 1, patch.per_worker_yield, standing)
                            > crew_take(CREW, patch.per_worker_yield, standing)
                    );
                    assert_eq!(
                        crew_take(CREW + 1, patch.per_worker_yield, honest_ceiling(patch)),
                        crew_take(CREW, patch.per_worker_yield, honest_ceiling(patch))
                    );
                }
                let band = &mut view.snapshot.populations[0];
                band.idle_workers = 0;
                band.food_income = band.food_consumption;
                band.labor_assignments = [NEAR_PATCH, RICH_PATCH]
                    .into_iter()
                    .map(|tile| {
                        let rate = if tile == NEAR_PATCH { 1.0 } else { 2.0 };
                        let spare = u32::from(tile == spare_on);
                        LaborAssignmentState {
                            workers_needed: CREW,
                            ..forage_row(tile, CREW + spare, CREW as f32 * rate)
                        }
                    })
                    .collect();
            })
        };
        let plan = plan_with_food_share(1.0);
        for spare_on in [NEAR_PATCH, RICH_PATCH] {
            let view = a_band_with_the_spare_hand_on(spare_on);
            let proposal = food().negative_income(&view, &plan, &memory(), own_band(&view));
            assert!(
                proposal.is_none(),
                "the spare hand on {},{} has nowhere better: {:?}",
                spare_on.x,
                spare_on.y,
                proposal.map(|proposal| proposal.reason)
            );
        }
    }

    // ---- the pools release their spare hands ---------------------------------------------------

    /// A band with hands in a pool and nothing else spare: eight on the rich patch, its take
    /// needing all eight, the rest on `role`, breaking even.
    fn a_band_pooled(role: &str, pooled: u32) -> SeatView {
        a_view_with(|view| {
            view.snapshot.herds.clear();
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.food_income = band.food_consumption;
            band.labor_assignments = vec![
                LaborAssignmentState {
                    workers_needed: 8,
                    ..forage_row(RICH_PATCH, 8, 16.0)
                },
                LaborAssignmentState {
                    kind: role.into(),
                    workers: pooled,
                    ..Default::default()
                },
            ];
        })
    }

    /// The role and count of every pool line among `commands`.
    fn pool_lines(commands: &[CommandPayload]) -> Vec<(String, u32)> {
        commands
            .iter()
            .map(assigned_to)
            .filter(|(role, _, tile, herd)| {
                tile.is_none() && herd.is_none() && role != ROLE_FORAGE && role != ROLE_HUNT
            })
            .map(|(role, workers, _, _)| (role, workers))
            .collect()
    }

    /// **Builders with nothing to raise are free hands.** Three on `builders` with an empty
    /// build queue: *negative income* fires on them alone, places all three on the ground in
    /// reach, and sends `builders 0` with the deal.
    #[test]
    fn builders_with_an_empty_queue_are_dealt_onto_the_ground_and_the_pool_is_emptied() {
        let view = a_band_pooled(ROLE_BUILDERS, 3);
        let band = own_band(&view);
        assert_eq!(
            food().pool_releases(&view, band),
            vec![PoolRelease {
                role: ROLE_BUILDERS,
                held: 3,
                free: 3
            }]
        );
        let proposal = food()
            .negative_income(&view, &plan_with_food_share(1.0), &memory(), band)
            .expect("three builders with nothing to build");
        assert_eq!(proposal.cost.workers, 3, "{}", proposal.reason);
        assert!(
            proposal
                .reason
                .starts_with("negative income: 3 off builders hands -> "),
            "{}",
            proposal.reason
        );
        assert_eq!(
            pool_lines(&proposal.commands),
            vec![(ROLE_BUILDERS.to_owned(), 0)]
        );
        let placed: u32 = proposal
            .commands
            .iter()
            .map(assigned_to)
            .filter(|(role, _, _, _)| role == ROLE_FORAGE)
            .map(|(_, workers, tile, _)| {
                workers - Food::workers_on(band, &SourceKey::Patch(tile.unwrap()))
            })
            .sum();
        assert_eq!(placed, 3, "every released hand lands: {}", proposal.reason);
    }

    /// **A blocked head idles the whole pool**: the queue's head names the rich patch and the
    /// frame says its build is blocked, so the three builders are free hands — the same deal as
    /// the empty queue.
    #[test]
    fn builders_behind_a_blocked_head_are_released_the_same_way() {
        let mut view = a_band_pooled(ROLE_BUILDERS, 3);
        view.snapshot.populations[0].build_queue = vec![BuildQueueEntryState {
            kind: ROLE_FORAGE.into(),
            target_x: RICH_PATCH.x,
            target_y: RICH_PATCH.y,
            ..Default::default()
        }];
        for patch in &mut view.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == RICH_PATCH {
                patch.build_blocked_reason = "knowledge".into();
            }
        }
        let band = own_band(&view);
        assert!(Food::build_head_idle(&view, band));
        let proposal = food()
            .negative_income(&view, &plan_with_food_share(1.0), &memory(), band)
            .expect("three builders stuck behind a blocked head");
        assert_eq!(proposal.cost.workers, 3, "{}", proposal.reason);
        assert_eq!(
            pool_lines(&proposal.commands),
            vec![(ROLE_BUILDERS.to_owned(), 0)]
        );
    }

    /// **A live head keeps its builders.** The same queue with the build unblocked: the pool is
    /// not free, nothing else is spare, and *negative income* is silent.
    #[test]
    fn builders_on_a_live_unblocked_head_stay() {
        let mut view = a_band_pooled(ROLE_BUILDERS, 3);
        view.snapshot.populations[0].build_queue = vec![BuildQueueEntryState {
            kind: ROLE_FORAGE.into(),
            target_x: RICH_PATCH.x,
            target_y: RICH_PATCH.y,
            ..Default::default()
        }];
        let band = own_band(&view);
        assert!(!Food::build_head_idle(&view, band));
        assert!(food().pool_releases(&view, band).is_empty());
        assert!(food()
            .negative_income(&view, &plan_with_food_share(1.0), &memory(), band)
            .is_none());
    }

    /// **Keepers above the bill are free hands, one hand of slack excepted.** Four on
    /// `agriculture` against a bill of two (the rich patch, owned and tended, `workers_needed
    /// 2`, paid): two are free and the deal sends `agriculture 2`. Three against the same bill
    /// stand — the one hand over is the slack *hold the ground* adds when the pool at the sum
    /// still leaves a patch short, and trimming it would have the hold add it back next turn.
    /// Four with the patch reading short stand too: the pool is the hold's then.
    #[test]
    fn keepers_above_the_plant_bill_are_freed_less_the_holds_slack() {
        let kept = |pooled: u32, shortfall: f32| {
            let mut view = a_band_pooled(ROLE_AGRICULTURE, pooled);
            for patch in &mut view.snapshot.forage_patches {
                if Tile::new(patch.x, patch.y) == RICH_PATCH {
                    patch.owner = Some(FACTION);
                    patch.is_cultivated = true;
                    patch.upkeep_demand = 1.92;
                    patch.upkeep_shortfall = shortfall;
                    patch.upkeep_workers_needed = 2;
                }
            }
            view
        };
        let view = kept(4, 0.0);
        let band = own_band(&view);
        assert_eq!(
            food().pool_releases(&view, band),
            vec![PoolRelease {
                role: ROLE_AGRICULTURE,
                held: 4,
                free: 2
            }]
        );
        let proposal = food()
            .negative_income(&view, &plan_with_food_share(1.0), &memory(), band)
            .expect("two keepers over the bill");
        assert_eq!(proposal.cost.workers, 2, "{}", proposal.reason);
        assert!(
            proposal
                .reason
                .starts_with("negative income: 2 off agriculture hands -> "),
            "{}",
            proposal.reason
        );
        assert_eq!(
            pool_lines(&proposal.commands),
            vec![(ROLE_AGRICULTURE.to_owned(), 2)]
        );
        let slack = kept(2 + HOLD_MIN_HANDS, 0.0);
        assert!(food().pool_releases(&slack, own_band(&slack)).is_empty());
        let short = kept(4, 0.5);
        assert!(food().pool_releases(&short, own_band(&short)).is_empty());
    }

    /// **A dead patch is dead while it stands at its floor.** Four hands on a patch stripped to
    /// the Best floor took nothing for the forager's window, against a regrowth of one a turn:
    /// dead, and not a source. The same record with the stand regrown above the floor no longer
    /// condemns it — the frame says there is food again.
    #[test]
    fn a_dead_patch_row_lapses_once_the_stand_regrows_above_its_floor() {
        let at_floor = a_view_with(|view| {
            for patch in &mut view.snapshot.forage_patches {
                if Tile::new(patch.x, patch.y) == NEAR_PATCH {
                    patch.biomass = BEST_FLOOR * patch.carrying_capacity;
                    patch.regrowth_samples = vec![1.0, 1.0];
                }
            }
            let band = &mut view.snapshot.populations[0];
            band.idle_workers = 0;
            band.labor_assignments = vec![LaborAssignmentState {
                workers_needed: 0,
                ..forage_row(NEAR_PATCH, 4, 0.0)
            }];
        });
        let specialist = food();
        let key = SourceKey::Patch(NEAR_PATCH);
        let mut memory = memory();
        for turn in 1..=PATCH_WINDOW {
            memory.observe(&at_floor, FACTION);
            assert_eq!(
                specialist.is_dead(&at_floor, &memory, own_band(&at_floor), &key),
                turn == PATCH_WINDOW,
                "turn {turn}"
            );
        }
        assert!(!specialist
            .reachable_sources(&at_floor, &memory, own_band(&at_floor))
            .iter()
            .any(|source| source.key == key));
        let mut regrown = at_floor;
        for patch in &mut regrown.snapshot.forage_patches {
            if Tile::new(patch.x, patch.y) == NEAR_PATCH {
                patch.biomass = patch.carrying_capacity;
            }
        }
        assert!(!specialist.is_dead(&regrown, &memory, own_band(&regrown), &key));
        assert!(specialist
            .reachable_sources(&regrown, &memory, own_band(&regrown))
            .iter()
            .any(|source| source.key == key));
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
                &memory_forecasting(&view),
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
        // Net 3.0 against a goal of 1.0: one hand off the near patch (−1.0) onto a herd that
        // hands back 0.5 leaves 2.5. The curve plateaus at one hunter (half an animal is all
        // there is), so a second is never sent — before the curve, two went for the same 0.5.
        assert_eq!(proposal.cost.workers, 1);
        assert_eq!(
            assigned_to(&proposal.commands[0]),
            (ROLE_FORAGE.to_owned(), 3, Some(NEAR_PATCH), None)
        );
        assert_eq!(
            assigned_to(&proposal.commands[1]),
            (ROLE_HUNT.to_owned(), 1, None, Some(HERD_ID.to_owned()))
        );
        assert!(
            proposal
                .reason
                .contains("1 hunters, likely 0.50/turn (sim crew take, low 0.50 high 0.50)"),
            "{}",
            proposal.reason
        );
        // Exactly at the goal: any hand leaving drops below it, so none does.
        let at_goal = a_fed_band(goals().net_income_per_turn);
        assert!(food()
            .spare_hands_into_hunts(
                &at_goal,
                &plan,
                &memory_forecasting(&at_goal),
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
                &memory_forecasting(&short),
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
        let proposals = specialist.propose(&view, &plan, &memory(), &GroundReadings::default());
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
