//! **The decision log** — `decisions.jsonl`, one JSON line per record, tagged by `kind`
//! (`docs/plan_ai_driver.md` §5, §8.1).
//!
//! Every proposal the arbiter weighs becomes a [`Decision`], accepted or not; every plan the
//! orchestrator adopts a [`PlanRecord`]; every alarm a specialist raises an [`AlarmRecord`]; every
//! `ready` the loop submits a [`ReadyRecord`], so a lost turn is countable; and every reconnect a
//! [`LinkRecord`]. The records are written through a [`DecisionSink`], which is what a brain is
//! handed — a brain never sees a file, and a brain that records nothing ignores the sink.
//!
//! The log carries the faction id and never the seat token (§10).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sim_runtime::CommandPayload;

/// The file the records go to, under `--log-dir`.
pub const DECISIONS_FILE: &str = "decisions.jsonl";

/// What became of a proposal (§5). `rejected_by` is the arbiter's step token —
/// `behavior_gated` / `over_budget` / `conflict` / `outscored` — and is the key the bench's
/// rejection mix groups on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Rejected { rejected_by: String },
}

/// One proposal, weighed. `reason` is the specialist's own account of why it proposed this — the
/// consideration that fired — distinct from the arbiter's `rejected_by`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub tick: u64,
    pub specialist: String,
    pub intent: String,
    pub score_raw: f32,
    pub score_final: f32,
    #[serde(flatten)]
    pub outcome: Outcome,
    pub reason: String,
    /// How many commands the proposal carried (0 for a rejected one that emitted nothing).
    pub commands: usize,
    /// The same commands, one readable line each ([`command_text`]) — what the viewer shows.
    pub commands_text: Vec<String>,
}

/// **A command as a line of the text grammar** (`sim_runtime::command_text`), for the verbs the
/// specialists emit — `assign_labor` (forage / hunt / the pool roles), `move_band`, `split_band` —
/// so a line reads exactly as an operator would type it and parses back through
/// `parse_command_line`. `sim_runtime` has a parser and no printer, and `CommandPayload` is not
/// serialisable, so any other verb falls back to its `Debug` form.
pub fn command_text(payload: &CommandPayload) -> String {
    match payload {
        CommandPayload::AssignLabor {
            faction_id,
            band_id,
            role,
            workers,
            target_x,
            target_y,
            fauna_id,
            floor,
            species,
            kit_id,
            take_species,
            ..
        } => {
            let band = band_id.map_or_else(|| NO_BAND.to_owned(), |band| band.to_string());
            let mut line = format!("assign_labor {faction_id} {band} {role}");
            if let (Some(x), Some(y)) = (target_x, target_y) {
                line.push_str(&format!(" {x} {y}"));
            }
            if let Some(herd) = fauna_id {
                line.push_str(&format!(" {herd}"));
            }
            if let Some(floor) = floor {
                line.push_str(&format!(" {floor}"));
            }
            if let Some(species) = species {
                line.push_str(&format!(" {species}"));
            }
            if !take_species.is_empty() {
                line.push_str(&format!(
                    " {TAKE_SELECTION_PREFIX}{}",
                    take_species.join(TAKE_SELECTION_SEPARATOR)
                ));
            }
            line.push_str(&format!(" {workers}"));
            if let Some(kit) = kit_id {
                line.push_str(&format!(" kit {kit}"));
            }
            line
        }
        CommandPayload::MoveBand {
            faction_id,
            band_id,
            target_x,
            target_y,
        } => {
            let band = band_id.map_or_else(|| NO_BAND.to_owned(), |band| band.to_string());
            format!("move_band {faction_id} {band} {target_x} {target_y}")
        }
        CommandPayload::SplitBand {
            faction_id,
            band_id,
            workers,
        } => {
            let band = band_id.map_or_else(|| NO_BAND.to_owned(), |band| band.to_string());
            format!("split_band {faction_id} {band} {workers}")
        }
        other => format!("{other:?}"),
    }
}

/// What a command line shows where the payload named no band — the grammar has no token for it,
/// so the line is readable and deliberately does not parse.
const NO_BAND: &str = "<no band>";
/// The take-selection token's prefix and separator, restated from `sim_runtime::command_text`
/// (`TAKE_SELECTION_PREFIX`, private there): `take:<a>,<b>`.
const TAKE_SELECTION_PREFIX: &str = "take:";
const TAKE_SELECTION_SEPARATOR: &str = ",";

/// A plan adopted by the orchestrator (§3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRecord {
    pub tick: u64,
    pub stance: String,
    pub since_tick: u64,
    pub budgets: BTreeMap<String, f32>,
    pub priorities: BTreeMap<String, f32>,
}

/// An alarm a specialist raised (§4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlarmRecord {
    pub tick: u64,
    pub specialist: String,
    pub alarm: String,
}

/// One `ready` submission. Written by the turn loop, not a brain, because the loop is what
/// submits — a brain that overran its budget still gets one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadyRecord {
    pub tick: u64,
}

/// A link event worth counting (§8.2, the Link row). `tick` is the last tick the view held, or
/// `None` before the first frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkRecord {
    pub tick: Option<u64>,
    pub event: LinkEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkEventKind {
    /// The command socket dropped and the seat was re-claimed with a fresh token.
    CommandReconnect,
    /// The stream socket dropped alone and was reopened with the token the seat still held.
    StreamReopen,
}

/// The one line type: every record, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionRecord {
    Decision(Decision),
    Plan(PlanRecord),
    Alarm(AlarmRecord),
    Ready(ReadyRecord),
    Link(LinkRecord),
}

/// Where a brain's records go. The loop hands one to `decide`; a brain that has nothing to record
/// never calls it, and a brain from another crate needs nothing from `instruments` to be one.
pub trait DecisionSink {
    fn record(&mut self, record: DecisionRecord);
}

/// The sink of a process with no `--log-dir`: drops everything.
#[derive(Debug, Default)]
pub struct NullSink;

impl DecisionSink for NullSink {
    fn record(&mut self, _record: DecisionRecord) {}
}

/// A sink that keeps what it is given, so a test can read a brain's records back.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct VecSink(pub Vec<DecisionRecord>);

#[cfg(test)]
impl DecisionSink for VecSink {
    fn record(&mut self, record: DecisionRecord) {
        self.0.push(record);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_TICK: u64 = 7;

    fn every_record() -> Vec<DecisionRecord> {
        vec![
            DecisionRecord::Decision(Decision {
                tick: A_TICK,
                specialist: "food".into(),
                intent: "forage".into(),
                score_raw: 0.5,
                score_final: 0.75,
                outcome: Outcome::Accepted,
                reason: "runway short".into(),
                commands: 1,
                commands_text: vec!["assign_labor 1 7 forage 3 4 5".into()],
            }),
            DecisionRecord::Decision(Decision {
                tick: A_TICK,
                specialist: "land".into(),
                intent: "cultivate".into(),
                score_raw: 0.4,
                score_final: 0.1,
                outcome: Outcome::Rejected {
                    rejected_by: "over_budget".into(),
                },
                reason: "patch free".into(),
                commands: 0,
                commands_text: Vec::new(),
            }),
            DecisionRecord::Plan(PlanRecord {
                tick: A_TICK,
                stance: "settle".into(),
                since_tick: A_TICK - 1,
                budgets: BTreeMap::from([("food".to_owned(), 3.0)]),
                priorities: BTreeMap::from([("food".to_owned(), 1.0)]),
            }),
            DecisionRecord::Alarm(AlarmRecord {
                tick: A_TICK,
                specialist: "food".into(),
                alarm: "hunger".into(),
            }),
            DecisionRecord::Ready(ReadyRecord { tick: A_TICK }),
            DecisionRecord::Link(LinkRecord {
                tick: Some(A_TICK),
                event: LinkEventKind::CommandReconnect,
            }),
        ]
    }

    #[test]
    fn every_record_round_trips_through_one_json_line() {
        for record in every_record() {
            let line = serde_json::to_string(&record).expect("serialises");
            assert!(!line.contains('\n'), "one line: {line}");
            let back: DecisionRecord = serde_json::from_str(&line).expect("parses");
            assert_eq!(back, record);
        }
    }

    /// The rendered line is the grammar's: it parses back to the payload it was rendered from.
    #[test]
    fn a_rendered_command_parses_back_to_itself() {
        use sim_runtime::parse_command_line;
        let assign_with =
            |role: &str,
             x: Option<u32>,
             fauna: Option<&str>,
             floor: Option<f32>,
             kit: Option<&str>| CommandPayload::AssignLabor {
                faction_id: 1,
                band_id: Some(7001),
                role: role.to_owned(),
                workers: 5,
                target_x: x,
                target_y: x.map(|x| x + 1),
                fauna_id: fauna.map(str::to_owned),
                policy: None,
                species: None,
                floor,
                kit_id: kit.map(str::to_owned),
                take_species: Vec::new(),
            };
        let assign = |role: &str, x: Option<u32>, fauna: Option<&str>| {
            assign_with(role, x, fauna, None, None)
        };
        for payload in [
            assign("forage", Some(3), None),
            assign("hunt", None, Some("game_boar_05")),
            assign("scout", None, None),
            CommandPayload::MoveBand {
                faction_id: 1,
                band_id: Some(7001),
                target_x: 4,
                target_y: 9,
            },
            CommandPayload::SplitBand {
                faction_id: 1,
                band_id: Some(7001),
                workers: 4,
            },
        ] {
            let line = command_text(&payload);
            let back = parse_command_line(&line).unwrap_or_else(|err| panic!("`{line}`: {err}"));
            assert_eq!(back, payload, "`{line}`");
        }
        assert_eq!(
            command_text(&assign("forage", Some(3), None)),
            "assign_labor 1 7001 forage 3 4 5"
        );
        let with_kit = assign_with(
            "hunt",
            None,
            Some("game_boar_05"),
            Some(0.5),
            Some("stone_knife"),
        );
        let line = command_text(&with_kit);
        assert_eq!(
            line,
            "assign_labor 1 7001 hunt game_boar_05 0.5 5 kit stone_knife"
        );
        assert_eq!(parse_command_line(&line).expect("parses"), with_kit);
        assert!(
            command_text(&CommandPayload::Resync).contains("Resync"),
            "an unrendered verb falls back to its debug form"
        );
    }

    #[test]
    fn the_kind_tag_and_the_outcome_tag_are_flat_keys() {
        let records = every_record();
        let accepted: serde_json::Value = serde_json::to_value(&records[0]).unwrap();
        assert_eq!(accepted["kind"], "decision");
        assert_eq!(accepted["outcome"], "accepted");
        assert!(accepted.get("rejected_by").is_none());
        let rejected: serde_json::Value = serde_json::to_value(&records[1]).unwrap();
        assert_eq!(rejected["outcome"], "rejected");
        assert_eq!(rejected["rejected_by"], "over_budget");
        let ready: serde_json::Value = serde_json::to_value(&records[4]).unwrap();
        assert_eq!(ready["kind"], "ready");
        assert_eq!(ready["tick"], A_TICK);
    }
}
