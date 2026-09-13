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
use crate::view::{row_key, SeatMemory, SeatView};

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

/// The scarce units a proposal spends (`plan_ai_driver.md` §4 → Budgets and costs): the
/// workers it draws, and **what it claims** — the resources two proposals cannot both set in one
/// turn. The sim takes several labor orders for one band in a turn, so a band is not a claim; a
/// *move* of it is, and so is each labor *row* it sets.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cost {
    /// Workers this proposal draws from the seat's working-age pool.
    pub workers: u32,
    /// The bands this proposal moves or splits (`move_band`, `split_band`) — a band walks one
    /// way a turn.
    pub moves: Vec<u64>,
    /// The labor rows this proposal sets, keyed as [`row_key`] does (band, kind, target) —
    /// every `assign_labor` it emits, donors and targets alike, and the patch's forage row for a
    /// `cultivate` / `sow`; a `builders` or `agriculture` pool is a row too. Two proposals
    /// setting one row would each overwrite the other's count.
    pub rows: Vec<String>,
}

impl Cost {
    /// **What `commands` claim**, read off the commands themselves so a proposal can never claim
    /// less than it sends: `workers` as given, a move per `move_band` / `split_band`, a row per
    /// `assign_labor` (its own band, else `band_id`) and the forage row of a `cultivate` / `sow`
    /// on `band_id` — the band the proposal works the patch with, which those two verbs do not
    /// carry. Duplicates are folded, so one proposal setting a row twice claims it once.
    pub fn claimed(workers: u32, band_id: u64, commands: &[CommandPayload]) -> Self {
        let mut moves = Vec::new();
        let mut rows = Vec::new();
        for command in commands {
            match command {
                CommandPayload::MoveBand { band_id: moved, .. }
                | CommandPayload::SplitBand { band_id: moved, .. } => {
                    moves.push(moved.unwrap_or(band_id));
                }
                CommandPayload::AssignLabor {
                    band_id: on,
                    role,
                    target_x,
                    target_y,
                    fauna_id,
                    ..
                } => rows.push(row_key(
                    on.unwrap_or(band_id),
                    role,
                    target_x.unwrap_or_default(),
                    target_y.unwrap_or_default(),
                    fauna_id.as_deref().unwrap_or_default(),
                )),
                CommandPayload::Cultivate {
                    target_x, target_y, ..
                }
                | CommandPayload::Sow {
                    target_x, target_y, ..
                } => rows.push(row_key(
                    band_id,
                    food::ROLE_FORAGE,
                    *target_x,
                    *target_y,
                    "",
                )),
                _ => {}
            }
        }
        moves.sort_unstable();
        moves.dedup();
        rows.sort_unstable();
        rows.dedup();
        Self {
            workers,
            moves,
            rows,
        }
    }
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
    /// **A standing bill the seat owes** — the arbiter pays it before weighing anything else
    /// (`arbiter.rs`): accepted first, in score order, still under the budget and the claims,
    /// and only then is the rest selected with those claims taken. Only *hold the ground* on a
    /// completed rung says so; every bid says `false`.
    pub standing: bool,
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

    /// The claims are the commands': a donor row and a target row both, a pool row, the forage
    /// row of a cultivate on the proposing band, a move per move or split — folded, sorted.
    #[test]
    fn a_cost_claims_every_row_and_move_its_commands_touch() {
        const BAND: u64 = 7;
        let labor = |role: &str, x: Option<u32>, y: Option<u32>, fauna: Option<&str>| {
            CommandPayload::AssignLabor {
                faction_id: 1,
                band_id: Some(BAND),
                role: role.to_owned(),
                workers: 3,
                target_x: x,
                target_y: y,
                fauna_id: fauna.map(str::to_owned),
                policy: None,
                species: None,
                floor: None,
                kit_id: None,
                take_species: Vec::new(),
            }
        };
        let commands = vec![
            labor("forage", Some(4), Some(2), None),
            labor("forage", Some(2), Some(3), None),
            labor("hunt", None, None, Some("herd_1")),
            labor("builders", None, None, None),
            labor("forage", Some(4), Some(2), None),
            CommandPayload::Cultivate {
                faction_id: 1,
                target_x: 6,
                target_y: 1,
            },
            CommandPayload::MoveBand {
                faction_id: 1,
                band_id: Some(BAND),
                target_x: 1,
                target_y: 1,
            },
            CommandPayload::SplitBand {
                faction_id: 1,
                band_id: Some(BAND + 1),
                workers: 4,
            },
            CommandPayload::Resync,
        ];
        let cost = Cost::claimed(5, BAND, &commands);
        assert_eq!(cost.workers, 5);
        assert_eq!(cost.moves, vec![BAND, BAND + 1]);
        assert_eq!(
            cost.rows,
            vec![
                "7:builders:0,0",
                "7:forage:2,3",
                "7:forage:4,2",
                "7:forage:6,1",
                "7:hunt:herd_1"
            ]
        );
        assert_eq!(Cost::claimed(0, BAND, &[]), Cost::default());
    }
}
