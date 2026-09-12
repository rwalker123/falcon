//! **Specialists** — one domain each, proposing and never sending (`docs/plan_ai_driver.md` §4).
//!
//! A specialist is a pure function of the view, its plan slice and its memory: hand it a frame and
//! assert on the proposals, with no server, no socket and no other specialist in the room. It never
//! sends — the arbiter does — so it cannot overspend, cannot move a band another specialist is
//! moving, and cannot bypass a behaviour gate. Each owns one scoreboard metric and one alarm.
//!
//! **The intent key** is `"<specialist>:<kind>:<subject>"` (`food:assign:17`, `land:move:17`):
//! stable across turns, so the arbiter's commitment bonus can recognise "the same thing as last
//! turn", and its middle token is the *class* the behaviour gate reads (`raid`, `trade`). The
//! scripted fixture's intent is the bare `script`, exactly as slice 3 wrote it.

pub mod food;
pub mod land;
pub mod scripted;

use std::fmt::Display;

use sim_runtime::CommandPayload;

use crate::board::Demand;
use crate::geometry::Tile;
use crate::orchestrator::{Alarm, Plan};
use crate::view::{SeatMemory, SeatView};

/// A specialist's name, as the plan's budgets and the decision log key it.
pub type SpecialistId = &'static str;
pub const SPECIALIST_FOOD: SpecialistId = "food";
pub const SPECIALIST_LAND: SpecialistId = "land";
pub const SPECIALIST_SCRIPTED: SpecialistId = "scripted";
/// Every specialist a `--disable` may name.
pub const DISABLEABLE_SPECIALISTS: [SpecialistId; 2] = [SPECIALIST_FOOD, SPECIALIST_LAND];

/// The intent key's separator.
pub const INTENT_SEPARATOR: char = ':';
/// The intent classes the behaviour gate knows (`plan_ai_driver.md` §5 step 1).
pub const INTENT_CLASS_RAID: &str = "raid";
pub const INTENT_CLASS_TRADE: &str = "trade";

/// `"<specialist>:<kind>:<subject>"`.
pub fn intent_key(specialist: SpecialistId, kind: &str, subject: impl Display) -> String {
    format!("{specialist}{INTENT_SEPARATOR}{kind}{INTENT_SEPARATOR}{subject}")
}

/// The `<kind>` of an intent key — its class — or the whole key when it has no separators.
pub fn intent_class(intent: &str) -> &str {
    let mut parts = intent.split(INTENT_SEPARATOR);
    let first = parts.next().unwrap_or(intent);
    parts.next().unwrap_or(first)
}

/// The scarce units a proposal spends (`plan_ai_driver.md` §4 → Budgets and costs).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cost {
    /// Workers this proposal draws from the seat's working-age pool.
    pub workers: u32,
    /// Bands this proposal gives an order to — one order per band per turn.
    pub bands: Vec<u64>,
}

/// **What the memory should remember if this proposal is accepted** — stated by the specialist,
/// not parsed back out of its commands, so `SeatMemory::record_choices` never has to know a
/// verb's shape. A `Move` is the target a band is walking to (any specialist's `move_band`); a
/// `Split` is the site the child band the sim will spawn next turn is meant for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Memo {
    /// `from` is where the band stands as the move is accepted — the tile it departs, which
    /// *better ground* will not walk it back onto while the memory holds it.
    Move { band: u64, target: Tile, from: Tile },
    Split {
        band: u64,
        target: Tile,
        workers: u32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// The wire actions, already faction-tagged.
    pub commands: Vec<CommandPayload>,
    /// What this is for, stable across turns — the commitment key.
    pub intent: String,
    /// This specialist's utility, before priority and commitment.
    pub score: f32,
    pub cost: Cost,
    /// The consideration that produced it — the decision log's why.
    pub reason: String,
    /// What to remember on acceptance, if anything.
    pub memo: Option<Memo>,
}

#[derive(Debug, Default)]
pub struct Proposals {
    pub proposals: Vec<Proposal>,
    pub alarm: Option<Alarm>,
    /// What this specialist asks the board for this turn (`board.rs`) — a kit or a material for
    /// a band whose outfitting window is open. Empty for a specialist with nothing to ask.
    pub demands: Vec<Demand>,
}

pub trait Specialist {
    fn id(&self) -> SpecialistId;
    fn propose(&mut self, view: &SeatView, plan: &Plan, memory: &SeatMemory) -> Proposals;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_intent_key_is_three_tokens_and_its_class_is_the_middle_one() {
        let key = intent_key(SPECIALIST_LAND, "move", 17);
        assert_eq!(key, "land:move:17");
        assert_eq!(intent_class(&key), "move");
        assert_eq!(intent_class("script"), "script");
    }
}
