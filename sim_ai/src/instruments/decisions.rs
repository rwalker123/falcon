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
use sim_runtime::{render_command_line, CommandPayload};

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
    ///
    /// `default` (the empty list) so a log written before this field existed still deserializes:
    /// `bench::measures::read_jsonl` collects into a `Result`, so one unreadable line
    /// would fail the whole run rather than that record.
    #[serde(default)]
    pub commands_text: Vec<String>,
}

/// **A command as a line of the text grammar** — `sim_runtime::render_command_line`, the one
/// renderer both sides of the wire share (the server's run record writes the same lines), so a
/// `commands_text` line reads exactly as an operator would type it and parses back through
/// `parse_command_line`.
pub fn command_text(payload: &CommandPayload) -> String {
    render_command_line(payload)
}

/// A specialist's goals as the plan record carries them (§3) — flat, one shape for every
/// specialist, so the JSON is a row and not a tagged union. `Food` is the one specialist with
/// goals today; a specialist without any has no entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalsRecord {
    pub net_income_per_turn: f32,
    pub runway_turns: f32,
    /// The plant rung the seat is climbing toward (`wild` / `tended` / `field`).
    pub ground_rung: String,
}

/// A plan adopted by the orchestrator (§3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRecord {
    pub tick: u64,
    pub stance: String,
    pub since_tick: u64,
    pub budgets: BTreeMap<String, f32>,
    pub priorities: BTreeMap<String, f32>,
    /// `default` (no goals) for the same reason `Decision::commands_text` carries it: a log
    /// written before goals existed still reads, or `read_jsonl` fails the whole run.
    #[serde(default)]
    pub goals: BTreeMap<String, GoalsRecord>,
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
                goals: BTreeMap::from([(
                    "food".to_owned(),
                    GoalsRecord {
                        net_income_per_turn: 1.0,
                        runway_turns: 12.0,
                        ground_rung: "field".to_owned(),
                    },
                )]),
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

    /// ⛔ **A log written before `commands_text` existed still reads.** `read_jsonl` collects into
    /// a `Result`, so one line that will not deserialize fails the whole viewer page rather than
    /// that record.
    #[test]
    fn a_decision_line_without_commands_text_parses_with_none() {
        let line = r#"{"kind":"decision","tick":7,"specialist":"food","intent":"food:assign:2",
            "score_raw":0.5,"score_final":0.75,"outcome":"accepted","reason":"idle hands",
            "commands":1}"#;
        let record: DecisionRecord = serde_json::from_str(line).expect("parses without the field");
        match record {
            DecisionRecord::Decision(decision) => {
                assert!(decision.commands_text.is_empty());
                assert_eq!(decision.commands, 1);
            }
            other => panic!("expected a decision, got {other:?}"),
        }
    }

    /// The line is the shared renderer's (`sim_runtime::render_command_line`, tested there): it
    /// parses back through the grammar.
    #[test]
    fn a_rendered_command_parses_back_to_itself() {
        use sim_runtime::parse_command_line;
        let payload = CommandPayload::SplitBand {
            faction_id: 1,
            band_id: Some(7001),
            workers: 4,
        };
        let line = command_text(&payload);
        assert_eq!(line, "split_band 1 7001 4");
        assert_eq!(parse_command_line(&line).expect("parses"), payload);
    }

    /// A plan line written before goals existed reads as a plan with none, for the reason
    /// `commands_text` defaults: one unreadable line fails the whole viewer page.
    #[test]
    fn a_plan_line_without_goals_parses_with_none_and_goals_are_flat() {
        let line = r#"{"kind":"plan","tick":7,"stance":"consolidate","since_tick":7,
            "budgets":{"food":0.75},"priorities":{"food":0.9}}"#;
        let record: DecisionRecord = serde_json::from_str(line).expect("parses without goals");
        match record {
            DecisionRecord::Plan(plan) => assert!(plan.goals.is_empty()),
            other => panic!("expected a plan, got {other:?}"),
        }
        let plan: serde_json::Value = serde_json::to_value(&every_record()[2]).unwrap();
        assert_eq!(plan["goals"]["food"]["ground_rung"], "field");
        assert_eq!(plan["goals"]["food"]["runway_turns"], 12.0);
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
