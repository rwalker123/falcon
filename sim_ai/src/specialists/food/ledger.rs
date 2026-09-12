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
//! A pure function of numbers — no view, no memory — so it is tested on its own.

use crate::instruments::scoreboard::NOT_FOOD_LIMITED_TURNS;
use crate::orchestrator::FoodGoals;

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
/// `stock_t+1 = stock_t + income − Σ lost + Σ gained(t) − consumption`, where a change's gain
/// counts from its own `payoff_turn` on.
pub fn project_all(book: &Book, changes: &[Reassignment], horizon: u32) -> Projection {
    let lost: f32 = changes.iter().map(|change| change.income_lost).sum();
    let mut stock = Vec::with_capacity(horizon as usize);
    let mut level = book.stock;
    let mut trough = (book.stock, 0);
    let mut positive_again = None;
    for turn in 0..horizon {
        let gained: f32 = changes
            .iter()
            .filter(|change| turn >= change.payoff_turn)
            .map(|change| change.income_gained)
            .sum();
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
    let gained_all: f32 = changes.iter().map(|change| change.income_gained).sum();
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
