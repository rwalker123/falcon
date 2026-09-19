//! **The crew-take oracle** — the seam through which a herd is forecast by the sim's own answer
//! rather than by the wire's per-hunter rate.
//!
//! The herd row's `per_worker_yield` is a **carry** (the kit's sled, `40 biomass × 0.02`), not a
//! kill rate: every herd in view reads `0.8` a hunter, and five flint spears against a boar bring
//! home one animal every few turns. The sim already answers the real question —
//! `QueryPayload::HuntCrewTake` returns, per crew size, the low / likely / high animals a
//! *resident* band would bring down this turn at the base tuning, with the band's live kit wear
//! and coverage priced in (`core_sim/src/forecast_query.rs` → `answer_hunt_crew_take`). This
//! module is the brain's way of asking it: a [`CrewTakeOracle`] answers a [`CrewTakeAsk`] with a
//! [`CrewTakeCurve`] in **food per turn**, the link-backed [`LinkOracle`] asks the seated link,
//! and [`Unasked`] answers nothing (a brain folding a frame in with no link to hand).
//!
//! `SeatMemory` owns the cache and the ask budget (`view.rs` → `refresh_crew_takes`); `Food`
//! reads the cached curve as a herd's `Source`; the land reading reads it as the herd site's food.

use std::time::Duration;

use sim_runtime::commands::{HuntCrewTakeQuery, HuntCrewTakeReply, QueryPayload};
use sim_runtime::QueryReply;
use tracing::warn;

use crate::link::{Link, QUERY_REPLY_TIMEOUT};

/// One question: what a crew of each size would bring home off `herd_id` for `band_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct CrewTakeAsk {
    pub faction: u32,
    /// The asking band's durable id — its live equipment wear and coverage price the answer.
    pub band_id: u64,
    pub herd_id: String,
    /// The kit the herd is hunted under (`Food::herd_kit_id`), required by the query.
    pub kit_id: String,
    /// The floor the curve is answered at, as a fraction of the herd's `K` — the wire default an
    /// assignment gets with `floor: None`.
    pub floor: f32,
    /// The largest crew asked about; the reply is one row per crew `1..=this`.
    pub max_workers: u32,
    /// One whole animal's food (`HerdTelemetryState::food_per_animal`): the reply counts animals,
    /// the brain counts provisions.
    pub food_per_animal: f32,
}

/// One row of the curve: what a crew of `crew` brings home per turn, in food.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrewTakeRow {
    pub crew: u32,
    /// The pessimistic bound.
    pub low: f32,
    /// The expectation — the number a herd is ranked on.
    pub likely: f32,
    /// The optimistic bound.
    pub high: f32,
}

/// **The sim's crew-take curve, in food per turn**: one row per crew size ascending from `1`,
/// the crew the sim reads as armed, and the food one animal is worth (the kill quantum the dead-row
/// window is measured in).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CrewTakeCurve {
    pub rows: Vec<CrewTakeRow>,
    /// `HuntCrewTakeReply::armed_crew` — how many of the crew carry something that can hurt this
    /// quarry; `0` when nothing the band holds can.
    pub armed_crew: u32,
    /// The food one animal of the herd is worth — `body_mass × provisions_per_biomass`, the row's
    /// own `food_per_animal`.
    pub body_food: f32,
}

impl CrewTakeCurve {
    /// The reply converted to food: every animal count times `food_per_animal`.
    pub fn from_reply(reply: &HuntCrewTakeReply, food_per_animal: f32) -> Self {
        Self {
            rows: reply
                .per_crew
                .iter()
                .map(|row| CrewTakeRow {
                    crew: row.workers,
                    low: row.animals_low * food_per_animal,
                    likely: row.animals_likely * food_per_animal,
                    high: row.animals_high * food_per_animal,
                })
                .collect(),
            armed_crew: reply.armed_crew,
            body_food: food_per_animal,
        }
    }

    /// The row for a crew of `crew`: none for a crew of nobody, the last row for a crew past the
    /// curve's end (the curve was asked for the whole working-age pool, so nothing larger can be
    /// sent).
    pub fn row(&self, crew: u32) -> Option<&CrewTakeRow> {
        if crew == 0 {
            return None;
        }
        self.rows
            .iter()
            .find(|row| row.crew == crew)
            .or_else(|| self.rows.last())
    }

    /// What a crew of `crew` likely brings home per turn; `0` for a crew of nobody.
    pub fn likely(&self, crew: u32) -> f32 {
        self.row(crew).map_or(0.0, |row| row.likely)
    }

    /// **The plateau**: the smallest crew whose likely take is the curve's greatest — the crew
    /// past which another hand buys nothing. `0` on an empty curve; `1` on a curve that never
    /// rises.
    pub fn plateau(&self) -> u32 {
        let most = self.rows.iter().map(|row| row.likely).fold(0.0, f32::max);
        self.rows
            .iter()
            .find(|row| row.likely >= most)
            .map_or(0, |row| row.crew)
    }

    /// **The best crew**: the size that takes the most per hunter (`likely / crew`) among those
    /// that take anything; the smaller on a tie; `None` when no crew takes anything.
    pub fn best_crew(&self) -> Option<u32> {
        self.rows
            .iter()
            .filter(|row| row.likely > 0.0 && row.crew > 0)
            .max_by(|a, b| {
                (a.likely / a.crew as f32)
                    .total_cmp(&(b.likely / b.crew as f32))
                    .then_with(|| b.crew.cmp(&a.crew))
            })
            .map(|row| row.crew)
    }
}

/// What answers a [`CrewTakeAsk`].
pub trait CrewTakeOracle {
    /// The curve for the ask, or `None` when it cannot be had this tick (no link, a refused or
    /// unanswered question) — a herd with no curve is not a source.
    fn crew_take(&mut self, ask: &CrewTakeAsk) -> Option<CrewTakeCurve>;
}

/// An oracle with nothing to ask: every question is unanswered. What a brain folds a frame in
/// with when it is not handed a link (`Composite::decide`'s own fold, the tests' default).
pub struct Unasked;

impl CrewTakeOracle for Unasked {
    fn crew_take(&mut self, _ask: &CrewTakeAsk) -> Option<CrewTakeCurve> {
        None
    }
}

/// The seated link as an oracle: one `HuntCrewTake` query per ask, answered on the command
/// socket within [`QUERY_REPLY_TIMEOUT`]. A refusal or a dead link is a warning and `None`.
pub struct LinkOracle<'a> {
    link: &'a mut Link,
    timeout: Duration,
}

impl<'a> LinkOracle<'a> {
    pub fn new(link: &'a mut Link) -> Self {
        Self {
            link,
            timeout: QUERY_REPLY_TIMEOUT,
        }
    }
}

impl CrewTakeOracle for LinkOracle<'_> {
    fn crew_take(&mut self, ask: &CrewTakeAsk) -> Option<CrewTakeCurve> {
        let query = QueryPayload::HuntCrewTake(HuntCrewTakeQuery {
            faction_id: ask.faction,
            band_id: ask.band_id,
            herd_id: ask.herd_id.clone(),
            kit_id: ask.kit_id.clone(),
            floor: ask.floor,
            max_workers: ask.max_workers,
        });
        match self.link.ask(query, self.timeout) {
            Ok(QueryReply::HuntCrewTake(reply)) => {
                Some(CrewTakeCurve::from_reply(&reply, ask.food_per_animal))
            }
            Ok(QueryReply::Error(token)) => {
                warn!(
                    band = ask.band_id,
                    herd = %ask.herd_id,
                    kit = %ask.kit_id,
                    token,
                    "the crew-take question was refused"
                );
                None
            }
            Ok(other) => {
                warn!(
                    band = ask.band_id,
                    herd = %ask.herd_id,
                    reply = ?other,
                    "the crew-take question was answered with something else"
                );
                None
            }
            Err(err) => {
                warn!(
                    band = ask.band_id,
                    herd = %ask.herd_id,
                    %err,
                    "the crew-take question went unanswered"
                );
                None
            }
        }
    }
}

/// **Canned curves, by herd id** — the tests' oracle. A herd it has no curve for is unanswered.
#[cfg(test)]
pub struct Canned {
    pub curves: std::collections::BTreeMap<String, CrewTakeCurve>,
    /// Every ask made, in order — so a test can say how many questions a tick asked.
    pub asked: Vec<CrewTakeAsk>,
}

#[cfg(test)]
impl Canned {
    pub fn new() -> Self {
        Self {
            curves: Default::default(),
            asked: Vec::new(),
        }
    }

    /// Serve `curve` for `herd_id`.
    pub fn with(mut self, herd_id: &str, curve: CrewTakeCurve) -> Self {
        self.curves.insert(herd_id.to_owned(), curve);
        self
    }
}

#[cfg(test)]
impl CrewTakeOracle for Canned {
    fn crew_take(&mut self, ask: &CrewTakeAsk) -> Option<CrewTakeCurve> {
        self.asked.push(ask.clone());
        self.curves.get(&ask.herd_id).cloned()
    }
}

/// A curve from `(crew, likely)` pairs with `low == high == likely` — the shape a test states.
#[cfg(test)]
pub fn curve_of(points: &[(u32, f32)], body_food: f32) -> CrewTakeCurve {
    CrewTakeCurve {
        rows: points
            .iter()
            .map(|(crew, likely)| CrewTakeRow {
                crew: *crew,
                low: *likely,
                likely: *likely,
                high: *likely,
            })
            .collect(),
        armed_crew: points.last().map_or(0, |(crew, _)| *crew),
        body_food,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_runtime::commands::HuntCrewTakeRow;

    #[test]
    fn the_reply_is_converted_to_food_and_read_by_crew() {
        let reply = HuntCrewTakeReply {
            per_crew: (1..=4)
                .map(|workers| HuntCrewTakeRow {
                    workers,
                    animals_low: 0.0,
                    animals_likely: (workers.min(3)) as f32 * 0.5,
                    animals_high: workers as f32,
                })
                .collect(),
            armed_crew: 3,
            weapon_item_id: "spears".into(),
        };
        let curve = CrewTakeCurve::from_reply(&reply, 0.25);
        assert_eq!(curve.body_food, 0.25);
        assert_eq!(curve.armed_crew, 3);
        assert_eq!(curve.likely(0), 0.0);
        assert_eq!(curve.likely(1), 0.125);
        assert_eq!(curve.likely(3), 0.375);
        assert_eq!(curve.likely(4), 0.375);
        assert_eq!(curve.likely(40), 0.375, "past the end, the last row");
        assert_eq!(curve.row(2).unwrap().high, 0.5);
        assert_eq!(curve.plateau(), 3);
    }

    #[test]
    fn the_best_crew_takes_the_most_per_hunter_and_a_flat_curve_has_none() {
        // A staircase: one hunter takes 0.3, two take 0.3, three take 1.2 (0.4 a hunter).
        let curve = curve_of(&[(1, 0.3), (2, 0.3), (3, 1.2), (4, 1.2)], 0.8);
        assert_eq!(curve.best_crew(), Some(3));
        assert_eq!(curve.plateau(), 3);
        // Ties go to the smaller crew.
        let even = curve_of(&[(1, 0.5), (2, 1.0)], 0.8);
        assert_eq!(even.best_crew(), Some(1));
        let nothing = curve_of(&[(1, 0.0), (2, 0.0)], 0.8);
        assert_eq!(nothing.best_crew(), None);
        assert_eq!(nothing.plateau(), 1);
        assert_eq!(CrewTakeCurve::default().plateau(), 0);
    }

    #[test]
    fn the_unasked_oracle_answers_nothing() {
        let ask = CrewTakeAsk {
            faction: 1,
            band_id: 2,
            herd_id: "herd_9".into(),
            kit_id: "big_game".into(),
            floor: 0.5,
            max_workers: 17,
            food_per_animal: 0.24,
        };
        assert_eq!(Unasked.crew_take(&ask), None);
        let mut canned = Canned::new().with("herd_9", curve_of(&[(1, 0.3)], 0.8));
        assert_eq!(canned.crew_take(&ask).map(|c| c.likely(1)), Some(0.3));
        assert_eq!(canned.asked.len(), 1);
    }
}
