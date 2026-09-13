//! **The projection ledger** — what-if arithmetic on one band's food book
//! (`docs/plan_ai_driver.md` §4 → "The projection ledger").
//!
//! Learning cultivation costs income now for income later, and a per-turn score cannot see that
//! trade. So `Food` projects a band's stock turn by turn under a proposed reassignment: what the
//! change takes off income until its payoff, what it adds from then on, and where the stock
//! bottoms out on the way. A change that takes income negative is acceptable **iff the projected
//! stock stays above zero** ([`survives`]), and the proposal's reason carries the turn the net
//! goes positive again. The same projection answers the goals the plan handed down: the gap
//! between projected runway / net income and the targets is the score ([`goal_progress`]).
//!
//! **The ledger models the patch, not only the band** (§4), once *draw down to survive* exists: a
//! flat income line cannot price a lowered floor, because taking below Best draws the standing
//! biomass down now and pays the lower regrowth after. [`PatchBook`] is one worked patch as the
//! frame shows it and [`floor_income`] walks it at a floor, turn by turn; [`Change::Series`] is
//! that walk as a change the projection takes beside the flat ones.
//!
//! A pure function of numbers — no view, no memory — so it is tested on its own.

use crate::instruments::scoreboard::NOT_FOOD_LIMITED_TURNS;
use crate::orchestrator::FoodGoals;

/// **The food peak — the Best floor.** `components::DEFAULT_ESCAPEMENT_FLOOR`
/// (`fauna::MSY_BIOMASS_FRACTION`, `core_sim`), the wire default every assignment gets when its
/// `floor` is `None`. Sustained take at floor `f` is `r · fK · (1 − f)`, maximal at `0.5`
/// (`docs/plan_harvest_floor.md` §2). Restated: this crate cannot link the server.
pub const BEST_FLOOR: f32 = 0.5;

/// One band's food book, read off the frame: `stores[FOOD_CARGO_KEY]` (fixed-point divided out),
/// `food_income`, `food_consumption` on `PopulationCohortState`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Book {
    pub stock: f32,
    pub income: f32,
    pub consumption: f32,
}

/// A change the ledger is asked about.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reassignment {
    /// Income lost from now on (workers leaving rows), per turn. ≥ 0.
    pub income_lost: f32,
    /// Income gained from `payoff_turn` on, per turn. ≥ 0.
    pub income_gained: f32,
    /// Turns from now until the gain starts (0 = immediately, e.g. moving hands between rows).
    pub payoff_turn: u32,
}

impl Reassignment {
    /// No change at all — the projection of the book as it stands.
    pub const NONE: Self = Self {
        income_lost: 0.0,
        income_gained: 0.0,
        payoff_turn: 0,
    };
}

/// **A change as the projection takes it**: a flat [`Reassignment`], or a per-turn income series
/// in place of the flat gain — what a floor change on a patch is ([`floor_income`]), since its
/// take is front-loaded and then settles on the floor's regrowth.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Flat(Reassignment),
    Series {
        /// Income lost from now on, per turn — the row's take today, which the series replaces.
        income_lost: f32,
        /// Income gained per turn, from turn 0; the last entry holds past the series' end.
        income_gained: Vec<f32>,
    },
}

impl Change {
    fn income_lost(&self) -> f32 {
        match self {
            Change::Flat(change) => change.income_lost,
            Change::Series { income_lost, .. } => *income_lost,
        }
    }

    /// What this change adds on `turn`.
    fn gained_at(&self, turn: u32) -> f32 {
        match self {
            Change::Flat(change) => {
                if turn >= change.payoff_turn {
                    change.income_gained
                } else {
                    0.0
                }
            }
            Change::Series { income_gained, .. } => income_gained
                .get(turn as usize)
                .or(income_gained.last())
                .copied()
                .unwrap_or(0.0),
        }
    }

    /// What this change adds once it has paid off — a series' last entry.
    fn gained_after(&self) -> f32 {
        match self {
            Change::Flat(change) => change.income_gained,
            Change::Series { income_gained, .. } => income_gained.last().copied().unwrap_or(0.0),
        }
    }
}

/// **One worked patch as the frame shows it**, for a floor projection ([`floor_income`]) — the
/// `ForagePatchState` terms the harvest-floor arc put on the wire so a client can evaluate the
/// take at any floor (*"`max(0, B − floor·K) × dip × rate` at **any** floor"*,
/// `ForagePatchState::provisions_per_biomass`), plus the crew on the row.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchBook {
    /// `ForagePatchState::biomass` — what stands on the patch now.
    pub biomass: f32,
    /// `ForagePatchState::carrying_capacity` — the patch's `K`, the rung already folded in.
    pub capacity: f32,
    /// `ForagePatchState::provisions_per_biomass` — what one unit of the standing crop is worth.
    pub provisions_per_biomass: f32,
    /// `ForagePatchState::regrowth_samples` — *"This patch's own per-turn regrowth, in biomass,
    /// sampled at evenly spaced fractions of `K` … Sample `i` of `n` is the delta at `B = i/(n−1)
    /// × K`; the x-axis is implicit and a client interpolates."* The wire's curve, not the
    /// logistic's shape: a tended patch's curve is the one its rung bought.
    pub regrowth_samples: Vec<f32>,
    /// The crew on the row.
    pub hands: u32,
    /// `ForagePatchState::per_worker_biomass` — what one gatherer moves per turn, in biomass;
    /// `0` in a dead season, when the crew carries nothing.
    pub per_worker_biomass: f32,
}

/// **What `patch` regrows in one turn standing at `fraction` of its capacity** — the wire's
/// samples interpolated at that fraction; `0` when the frame sent none (a curve that was not
/// sent is not a curve that is flat).
pub fn regrowth_at(samples: &[f32], fraction: f32) -> f32 {
    match samples {
        [] => 0.0,
        [only] => *only,
        _ => {
            let last = (samples.len() - 1) as f32;
            let x = (fraction.clamp(0.0, 1.0) * last).clamp(0.0, last);
            let below = x.floor() as usize;
            let above = (below + 1).min(samples.len() - 1);
            let weight = x - below as f32;
            samples[below] + (samples[above] - samples[below]) * weight
        }
    }
}

/// What [`floor_income`] answers: the income series, and the turn the patch is spent.
#[derive(Debug, Clone, PartialEq)]
pub struct FloorIncome {
    /// What the crew takes each turn, `horizon` entries, in provisions.
    pub per_turn: Vec<f32>,
    /// The first turn whose take leaves the patch standing at the floor — the standing biomass
    /// above `floor × K` is spent, and every turn after pays the floor's regrowth. `None` when
    /// the crew cannot draw it down within the horizon.
    pub spent_at: Option<u32>,
}

/// **What a crew takes from `patch` at `floor`, turn by turn for `horizon` turns**: the standing
/// biomass above `floor × capacity` first (capped by what the hands carry, `hands ×
/// per_worker_biomass` a turn), then the regrowth at that floor — the same order the sim resolves
/// a turn in, the take then the regrowth. Sustained, that is the harvest-floor arc's
/// `r · fK · (1 − f)` read off the wire's own curve.
pub fn floor_income(patch: &PatchBook, floor: f32, horizon: u32) -> FloorIncome {
    let carried = patch.hands as f32 * patch.per_worker_biomass;
    let held = floor * patch.capacity;
    let mut standing = patch.biomass;
    let mut per_turn = Vec::with_capacity(horizon as usize);
    let mut spent_at = None;
    for turn in 0..horizon {
        let room = (standing - held).max(0.0);
        let take = room.min(carried);
        if spent_at.is_none() && room > 0.0 && take >= room {
            spent_at = Some(turn);
        }
        standing -= take;
        let fraction = if patch.capacity > 0.0 {
            standing / patch.capacity
        } else {
            0.0
        };
        standing = (standing + regrowth_at(&patch.regrowth_samples, fraction)).min(patch.capacity);
        per_turn.push(take * patch.provisions_per_biomass);
    }
    FloorIncome { per_turn, spent_at }
}

/// What a projection says about a change.
#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    /// The stock at the end of each projected turn, `horizon` entries.
    pub stock: Vec<f32>,
    /// The lowest projected stock and the turn it occurs — the starting stock at turn 0 when the
    /// horizon is empty.
    pub trough: (f32, u32),
    /// The first turn at which the projected net income is ≥ 0, if within the horizon.
    pub positive_again: Option<u32>,
    /// Net income per turn once every change has paid off.
    pub net_after: f32,
    /// The stock at the end of the horizon over the band's consumption — the runway the goal is
    /// held against; [`NOT_FOOD_LIMITED_TURNS`] when the band eats nothing.
    pub runway_at_end: f32,
}

/// Project `book` under every change in `changes` together, `horizon` turns ahead:
/// `stock_t+1 = stock_t + income − Σ lost + Σ gained(t) − consumption`, where a flat change's gain
/// counts from its own `payoff_turn` on and a series' is its entry for the turn.
pub fn project_changes(book: &Book, changes: &[Change], horizon: u32) -> Projection {
    let lost: f32 = changes.iter().map(Change::income_lost).sum();
    let mut stock = Vec::with_capacity(horizon as usize);
    let mut level = book.stock;
    let mut trough = (book.stock, 0);
    let mut positive_again = None;
    for turn in 0..horizon {
        let gained: f32 = changes.iter().map(|change| change.gained_at(turn)).sum();
        let net = book.income - lost + gained - book.consumption;
        if net >= 0.0 && positive_again.is_none() {
            positive_again = Some(turn);
        }
        level += net;
        if turn == 0 || level < trough.0 {
            trough = (level, turn);
        }
        stock.push(level);
    }
    let gained_all: f32 = changes.iter().map(Change::gained_after).sum();
    let end = stock.last().copied().unwrap_or(book.stock);
    Projection {
        stock,
        trough,
        positive_again,
        net_after: book.income - lost + gained_all - book.consumption,
        runway_at_end: if book.consumption > 0.0 {
            end / book.consumption
        } else {
            NOT_FOOD_LIMITED_TURNS
        },
    }
}

/// [`project_changes`] over flat changes only.
pub fn project_all(book: &Book, changes: &[Reassignment], horizon: u32) -> Projection {
    let changes: Vec<Change> = changes.iter().copied().map(Change::Flat).collect();
    project_changes(book, &changes, horizon)
}

/// Project `book` under one `change`, `horizon` turns ahead.
pub fn project(book: &Book, change: &Reassignment, horizon: u32) -> Projection {
    project_all(book, std::slice::from_ref(change), horizon)
}

/// §4: "acceptable iff the projected stock stays above zero until the projected payoff".
pub fn survives(projection: &Projection) -> bool {
    projection.trough.0 > 0.0
}

/// **The two goal terms** [`goal_progress`] weighs, equally: the runway gap and the net-income
/// gap. `1.0` = a change that closes both whole gaps.
pub const GOAL_TERMS: f32 = 2.0;

/// The share of `goal` still to reach from `value`, `0..1` — nothing once the goal is met.
fn gap(goal: f32, value: f32) -> f32 {
    (goal - value).max(0.0) / goal
}

/// How much closer `after` brings the band to `goals` than `before` does: the runway gap and the
/// net-income gap closed, each normalised to the goal, averaged over [`GOAL_TERMS`]. `0` for no
/// change, positive toward the goals, negative away from them.
pub fn goal_progress(goals: &FoodGoals, before: &Projection, after: &Projection) -> f32 {
    let runway = gap(goals.runway_turns, before.runway_at_end)
        - gap(goals.runway_turns, after.runway_at_end);
    let income = gap(goals.net_income_per_turn, before.net_after)
        - gap(goals.net_income_per_turn, after.net_after);
    (runway + income) / GOAL_TERMS
}

/// The projection as a proposal's reason states it: `ledger: trough 4.2 at t9, positive again
/// t17` — or `never positive` when the horizon ends first.
pub fn ledger_note(projection: &Projection) -> String {
    let (trough, at) = projection.trough;
    let positive = match projection.positive_again {
        Some(turn) => format!("positive again t{turn}"),
        None => "never positive".to_owned(),
    };
    format!("ledger: trough {trough:.1} at t{at}, {positive}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::GroundRung;

    const HORIZON: u32 = 20;

    fn goals() -> FoodGoals {
        FoodGoals {
            net_income_per_turn: 1.0,
            runway_turns: 12.0,
            ground_rung: GroundRung::Field,
        }
    }

    /// Eight in the larder, eating 2 a turn and earning 2: level until something changes.
    fn a_book() -> Book {
        Book {
            stock: 8.0,
            income: 2.0,
            consumption: 2.0,
        }
    }

    #[test]
    fn a_change_whose_trough_stays_positive_survives_and_reports_when_it_pays_off() {
        // Two hands leave the rows (−1.0 a turn) for a build that pays 3.0 a turn from turn 5.
        let build = Reassignment {
            income_lost: 1.0,
            income_gained: 3.0,
            payoff_turn: 5,
        };
        let projection = project(&a_book(), &build, HORIZON);
        assert_eq!(projection.stock.len(), HORIZON as usize);
        // Five turns at −1.0: 7, 6, 5, 4, 3 — the trough is 3 at t4; from t5 on +2.0 a turn.
        assert_eq!(projection.trough, (3.0, 4));
        assert_eq!(projection.positive_again, Some(5));
        assert_eq!(projection.net_after, 2.0);
        assert!(survives(&projection));
        assert_eq!(projection.stock[5], 5.0);
        // The same change on a thinner larder does not.
        let thin = Book {
            stock: 4.0,
            ..a_book()
        };
        let projection = project(&thin, &build, HORIZON);
        assert_eq!(projection.trough, (-1.0, 4));
        assert!(!survives(&projection));
        assert_eq!(
            ledger_note(&projection),
            "ledger: trough -1.0 at t4, positive again t5"
        );
    }

    #[test]
    fn an_immediate_gain_reads_positive_at_turn_zero_and_no_change_is_the_book_as_it_stands() {
        let hands = Reassignment {
            income_lost: 0.0,
            income_gained: 1.5,
            payoff_turn: 0,
        };
        let projection = project(&a_book(), &hands, HORIZON);
        assert_eq!(projection.positive_again, Some(0));
        assert_eq!(projection.trough, (9.5, 0));
        let none = project(&a_book(), &Reassignment::NONE, HORIZON);
        assert_eq!(none.net_after, 0.0);
        assert_eq!(none.positive_again, Some(0), "level is not negative");
        assert_eq!(
            none.runway_at_end, 4.0,
            "8 in the larder at the end, eating 2"
        );
        // A band eating nothing has no runway to speak of.
        let fed = Book {
            consumption: 0.0,
            ..a_book()
        };
        assert_eq!(
            project(&fed, &Reassignment::NONE, HORIZON).runway_at_end,
            NOT_FOOD_LIMITED_TURNS
        );
        // A never-positive change says so.
        let drain = Reassignment {
            income_lost: 1.0,
            income_gained: 0.0,
            payoff_turn: 0,
        };
        let projection = project(&a_book(), &drain, HORIZON);
        assert_eq!(projection.positive_again, None);
        assert!(ledger_note(&projection).ends_with("never positive"));
        // An empty horizon is the book as it stands.
        let flat = project(&a_book(), &Reassignment::NONE, 0);
        assert_eq!(flat.trough, (8.0, 0));
        assert_eq!(flat.runway_at_end, 4.0);
    }

    /// A patch at its capacity of 40, regrowing a flat 6 a turn wherever it stands (nothing at
    /// zero), seventeen hands carrying one each.
    fn a_patch() -> PatchBook {
        PatchBook {
            biomass: 40.0,
            capacity: 40.0,
            provisions_per_biomass: 1.0,
            regrowth_samples: vec![0.0, 6.0, 6.0, 6.0, 6.0, 6.0],
            hands: 17,
            per_worker_biomass: 1.0,
        }
    }

    /// At Best the crew clears the 20 above the floor in two turns and then takes the floor's
    /// regrowth, turn after turn — the regrowth line; at zero it front-loads the whole stand and
    /// then reads the zero regrowth of a stripped patch.
    #[test]
    fn floor_income_is_the_regrowth_line_at_best_and_front_loads_the_stand_at_zero() {
        let best = floor_income(&a_patch(), BEST_FLOOR, 6);
        // t0: room 20, take 17 → 23, regrows to 29; t1: room 9, take 9 → 20, regrows to 26; then
        // room 6 each turn: the regrowth at the floor.
        assert_eq!(best.per_turn, vec![17.0, 9.0, 6.0, 6.0, 6.0, 6.0]);
        assert_eq!(best.spent_at, Some(1));
        let stripped = floor_income(&a_patch(), 0.0, 6);
        // t0: 17 → 23, regrows to 29; t1: 17 → 12, regrows to 18; t2: 17 → 1, regrows to 7 (the
        // curve interpolates 0 → 6 over the first fifth); t3: 7 → 0, and nothing regrows at zero.
        assert_eq!(stripped.per_turn[..2], [17.0, 17.0]);
        assert_eq!(stripped.spent_at, Some(3));
        assert_eq!(stripped.per_turn[4..], [0.0, 0.0]);
        // A patch already standing at the floor pays the regrowth from the first turn.
        let settled = PatchBook {
            biomass: 20.0,
            ..a_patch()
        };
        assert_eq!(
            floor_income(&settled, BEST_FLOOR, 3).per_turn,
            vec![0.0, 6.0, 6.0]
        );
        // The crew, not the room, may be the cap: two hands take 2 a turn, and never spend it.
        let few = PatchBook {
            hands: 2,
            ..a_patch()
        };
        let income = floor_income(&few, BEST_FLOOR, 3);
        assert_eq!(income.per_turn, vec![2.0, 2.0, 2.0]);
        assert_eq!(income.spent_at, None);
        // No curve sent: nothing regrows.
        assert_eq!(regrowth_at(&[], 0.5), 0.0);
        assert_eq!(regrowth_at(&[0.0, 10.0, 0.0], 0.25), 5.0);
    }

    /// A series is taken beside the flat changes, entry by entry, and holds its last entry past
    /// its end.
    #[test]
    fn a_series_change_is_projected_turn_by_turn() {
        let series = Change::Series {
            income_lost: 2.0,
            income_gained: vec![10.0, 4.0],
        };
        let projection = project_changes(&a_book(), &[series], 4);
        // 8 + (2 − 2 + 10 − 2) = 16; then +2 a turn on the held last entry of 4.
        assert_eq!(projection.stock, vec![16.0, 18.0, 20.0, 22.0]);
        assert_eq!(projection.net_after, 2.0);
        assert_eq!(projection.positive_again, Some(0));
    }

    #[test]
    fn two_changes_pay_off_on_their_own_turns() {
        let now = Reassignment {
            income_lost: 0.0,
            income_gained: 1.0,
            payoff_turn: 0,
        };
        let later = Reassignment {
            income_lost: 1.0,
            income_gained: 4.0,
            payoff_turn: 2,
        };
        let projection = project_all(&a_book(), &[now, later], HORIZON);
        // t0, t1: +1 −1 = 0 a turn; from t2: +4 a turn.
        assert_eq!(projection.stock[1], 8.0);
        assert_eq!(projection.stock[2], 12.0);
        assert_eq!(projection.net_after, 4.0);
    }

    #[test]
    fn goal_progress_is_zero_for_no_change_positive_toward_the_goals_and_negative_away() {
        let before = project(&a_book(), &Reassignment::NONE, HORIZON);
        assert_eq!(goal_progress(&goals(), &before, &before), 0.0);
        let toward = project(
            &a_book(),
            &Reassignment {
                income_lost: 0.0,
                income_gained: 1.0,
                payoff_turn: 0,
            },
            HORIZON,
        );
        let progress = goal_progress(&goals(), &before, &toward);
        assert!(progress > 0.0, "{progress}");
        // Net income goes from 0 to the goal of 1.0: that whole gap (1.0) closes. The runway goes
        // from 4 turns to (8 + 20) / 2 = 14, past the goal of 12: its gap of 8/12 closes too.
        let expected = (8.0 / 12.0 + 1.0) / GOAL_TERMS;
        assert!(
            (progress - expected).abs() < 1e-6,
            "{progress} vs {expected}"
        );
        let away = project(
            &a_book(),
            &Reassignment {
                income_lost: 1.0,
                income_gained: 0.0,
                payoff_turn: 0,
            },
            HORIZON,
        );
        assert!(goal_progress(&goals(), &before, &away) < 0.0);
        // Half the income gap closed and the runway gap untouched reads as a quarter.
        let half = project(
            &a_book(),
            &Reassignment {
                income_lost: 0.0,
                income_gained: 0.5,
                payoff_turn: 0,
            },
            HORIZON,
        );
        let progress = goal_progress(&goals(), &before, &half);
        // Runway: 4 → (8 + 10) / 2 = 9 turns, gap 8/12 → 3/12; income: gap 1 → 0.5.
        let expected = ((8.0 - 3.0) / 12.0 + 0.5) / GOAL_TERMS;
        assert!(
            (progress - expected).abs() < 1e-6,
            "{progress} vs {expected}"
        );
    }
}
