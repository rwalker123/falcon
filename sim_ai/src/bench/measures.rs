//! **From the two measured logs to a number per layer** (`docs/plan_ai_driver.md` §8.2).
//!
//! A seat's measures are computed from its `scoreboard.jsonl` and `decisions.jsonl` alone. They
//! are a flat map of measure name → value, `None` where the record that would answer it is not
//! written yet — the orchestrator's rows need `PlanRecord`s and `AlarmRecord`s, which arrive with
//! the real brain — so a report is honest about what it could not read rather than silent.
//!
//! The names are dotted paths (`specialist.food.accepted`, `deaths.hunger`,
//! `link.turns_observed`) so a baseline file, a comparison and a table all key on one string.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use crate::instruments::decisions::{DecisionRecord, LinkEventKind, Outcome, DECISIONS_FILE};
use crate::instruments::scoreboard::{ScoreRow, DEATH_CAUSES, DEATH_CAUSE_HUNGER, SCOREBOARD_FILE};
use crate::specialists::food::{INTENT_SPLIT, INTENT_UPGRADE};
use crate::specialists::{intent_key, INTENT_SEPARATOR, SPECIALIST_FOOD};

/// Measure name → value. `None` is "the log cannot answer this yet".
pub type Measures = BTreeMap<String, Option<f64>>;

/// **The liveness window** (§8.2): a specialist is live when it has an accepted proposal in every
/// window of this many turns over the run. Also the churn window.
pub const LIVENESS_WINDOW_TURNS: u64 = 10;
/// Stance switches are quoted per this many turns.
const STANCE_SWITCH_PER_TURNS: f64 = 100.0;

// --- whole seat (the `ScoreRow` at the last tick) -----------------------------------------------
pub const M_POPULATION_CHILDREN: &str = "population_children";
pub const M_POPULATION_WORKING: &str = "population_working";
pub const M_POPULATION_ELDERS: &str = "population_elders";
pub const M_FOOD_STOCK: &str = "food_stock";
pub const M_FOOD_INCOME: &str = "food_income";
pub const M_FOOD_CONSUMPTION: &str = "food_consumption";
pub const M_SUSTAINABLE_YIELD: &str = "sustainable_yield";
pub const M_ACTUAL_YIELD: &str = "actual_yield";
pub const M_RUNWAY_TURNS: &str = "runway_turns";
pub const M_IDLE_WORKERS: &str = "idle_workers";
pub const M_PATCHES_OWNED: &str = "patches_owned";
pub const M_PATCHES_IMPROVED: &str = "patches_improved";
pub const M_HERD_BIOMASS_IN_VIEW: &str = "herd_biomass_in_view";
pub const M_HERDS_CORRALLED: &str = "herds_corralled";
/// Prefixes of the map-valued row fields, one measure per key.
pub const M_INTENSIFICATION_PREFIX: &str = "knowledge.intensification.";
pub const M_CRAFT_PREFIX: &str = "knowledge.craft.";
pub const M_DEATHS_PREFIX: &str = "deaths.";
pub const M_VICTORY_PREFIX: &str = "victory.";
/// The guard: hunger deaths summed over every row of the run.
pub const M_HUNGER_DEATHS_TOTAL: &str = "hunger_deaths_total";
/// Commands the sim refused, summed over the run — a specialist proposing what the server will
/// not take.
pub const M_COMMANDS_FAILED_TOTAL: &str = "commands_failed_total";
/// `intent.<specialist>:<kind>`: the share of accepted decisions under each intent class — the
/// histogram two profiles are told apart by (§8.2, profile divergence).
pub const M_INTENT_PREFIX: &str = "intent.";
/// **The two `Food` rule firings slice 6 is done-when'd on** (`plan_ai_driver.md` §11 row 6):
/// accepted `food:upgrade` intents (a `Cultivate`/`Sow` declared) and accepted `food:split`
/// intents, counted over the run. Absolute counts, not shares, so "never once" reads as 0.
pub const M_UPGRADES_DECLARED: &str = "food.upgrades_declared";
pub const M_SPLITS: &str = "food.splits";
// --- per specialist ------------------------------------------------------------------------------
pub const M_SPECIALIST_PREFIX: &str = "specialist.";
pub const M_ACCEPTED: &str = "accepted";
pub const M_REJECTED_PREFIX: &str = "rejected.";
pub const M_ACCEPTANCE_RATE: &str = "acceptance_rate";
pub const M_LIVENESS: &str = "liveness";
pub const M_INTENT_CHURN: &str = "intent_churn";
// --- orchestrator --------------------------------------------------------------------------------
pub const M_STANCE_SWITCHES: &str = "orchestrator.stance_switches_per_100_turns";
pub const M_ALARM_LATENCY: &str = "orchestrator.alarm_latency_turns";
// --- link ----------------------------------------------------------------------------------------
pub const M_TURNS_OBSERVED: &str = "link.turns_observed";
pub const M_TURNS_LOST: &str = "link.turns_lost_to_timeout";
pub const M_RECONNECTS: &str = "link.reconnects";

/// `true` as a measure — a specialist accepted in every window. **Reported, not a gate**: the
/// ratchet's precondition asks a specialist for one accepted decision over the whole run, not one
/// per window, because on a one-band seat a window lost to a higher-scoring sibling is ordinary
/// arbitration (`ratchet::Report::specialist_ignored_violations`). It is ratchetable through a
/// baseline `tolerance` like any other measure.
pub const LIVE: f64 = 1.0;
/// …and `false`: a window with nothing accepted in it.
pub const NOT_LIVE: f64 = 0.0;

#[derive(Debug, thiserror::Error)]
pub enum MeasureError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("{path} line {line} is not a record: {source}")]
    Parse {
        path: String,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
}

/// Read one seat's two measured logs from `log_dir` and compute its measures.
pub fn measures_for_seat(log_dir: &Path) -> Result<Measures, MeasureError> {
    let rows: Vec<ScoreRow> = read_jsonl(&log_dir.join(SCOREBOARD_FILE))?;
    let records: Vec<DecisionRecord> = read_jsonl(&log_dir.join(DECISIONS_FILE))?;
    Ok(compute(&rows, &records))
}

pub(crate) fn read_jsonl<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<Vec<T>, MeasureError> {
    let text = fs::read_to_string(path).map_err(|source| MeasureError::Read {
        path: path.display().to_string(),
        source,
    })?;
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line).map_err(|source| MeasureError::Parse {
                path: path.display().to_string(),
                line: index + 1,
                source,
            })
        })
        .collect()
}

/// The measures of §8.2 from the rows and records of one seat.
pub fn compute(rows: &[ScoreRow], records: &[DecisionRecord]) -> Measures {
    let mut measures = Measures::new();
    whole_seat(rows, &mut measures);
    specialists(rows, records, &mut measures);
    orchestrator(rows, records, &mut measures);
    link(rows, records, &mut measures);
    measures
}

fn put(measures: &mut Measures, name: impl Into<String>, value: f64) {
    measures.insert(name.into(), Some(value));
}

fn whole_seat(rows: &[ScoreRow], measures: &mut Measures) {
    let hunger_total: u32 = rows
        .iter()
        .filter_map(|row| row.deaths_by_cause.get(DEATH_CAUSE_HUNGER))
        .sum();
    put(measures, M_HUNGER_DEATHS_TOTAL, f64::from(hunger_total));
    let failed_total: u32 = rows.iter().map(|row| row.commands_failed).sum();
    put(measures, M_COMMANDS_FAILED_TOTAL, f64::from(failed_total));
    let Some(last) = rows.last() else {
        return;
    };
    put(
        measures,
        M_POPULATION_CHILDREN,
        f64::from(last.population_children),
    );
    put(
        measures,
        M_POPULATION_WORKING,
        f64::from(last.population_working),
    );
    put(
        measures,
        M_POPULATION_ELDERS,
        f64::from(last.population_elders),
    );
    put(measures, M_FOOD_STOCK, f64::from(last.food_stock));
    put(measures, M_FOOD_INCOME, f64::from(last.food_income));
    put(
        measures,
        M_FOOD_CONSUMPTION,
        f64::from(last.food_consumption),
    );
    put(
        measures,
        M_SUSTAINABLE_YIELD,
        f64::from(last.sustainable_yield),
    );
    put(measures, M_ACTUAL_YIELD, f64::from(last.actual_yield));
    put(measures, M_RUNWAY_TURNS, f64::from(last.runway_turns));
    put(measures, M_IDLE_WORKERS, f64::from(last.idle_workers));
    put(measures, M_PATCHES_OWNED, last.patches_owned as f64);
    put(measures, M_PATCHES_IMPROVED, last.patches_improved as f64);
    put(
        measures,
        M_HERD_BIOMASS_IN_VIEW,
        f64::from(last.herd_biomass_in_view),
    );
    put(measures, M_HERDS_CORRALLED, last.herds_corralled as f64);
    for (id, value) in &last.intensification_knowledge {
        put(
            measures,
            format!("{M_INTENSIFICATION_PREFIX}{id}"),
            f64::from(*value),
        );
    }
    for (id, value) in &last.craft_knowledge {
        put(measures, format!("{M_CRAFT_PREFIX}{id}"), f64::from(*value));
    }
    // Every cause in the vocabulary, zero when nobody died of it, so two runs always carry the
    // same death measures and a comparison never reads "absent" for "none".
    for cause in DEATH_CAUSES {
        let count = last.deaths_by_cause.get(cause).copied().unwrap_or(0);
        put(
            measures,
            format!("{M_DEATHS_PREFIX}{cause}"),
            f64::from(count),
        );
    }
    for (mode, progress) in &last.victory_progress {
        put(
            measures,
            format!("{M_VICTORY_PREFIX}{mode}"),
            f64::from(*progress),
        );
    }
}

/// The run's tick span, from the scoreboard: the windows are cut over it.
fn tick_span(rows: &[ScoreRow]) -> Option<(u64, u64)> {
    let first = rows.iter().map(|row| row.tick).min()?;
    let last = rows.iter().map(|row| row.tick).max()?;
    Some((first, last))
}

/// The windows of [`LIVENESS_WINDOW_TURNS`] over `[first, last]`, as `[start, end)` pairs.
fn windows(first: u64, last: u64) -> Vec<(u64, u64)> {
    (first..=last)
        .step_by(LIVENESS_WINDOW_TURNS as usize)
        .map(|start| (start, start.saturating_add(LIVENESS_WINDOW_TURNS)))
        .collect()
}

fn specialists(rows: &[ScoreRow], records: &[DecisionRecord], measures: &mut Measures) {
    let decisions: Vec<_> = records
        .iter()
        .filter_map(|record| match record {
            DecisionRecord::Decision(decision) => Some(decision),
            _ => None,
        })
        .collect();
    let names: BTreeSet<&str> = decisions
        .iter()
        .map(|decision| decision.specialist.as_str())
        .collect();
    let span = tick_span(rows);
    // The intent histogram: accepted decisions by `<specialist>:<kind>`, as shares.
    let accepted_all: Vec<_> = decisions
        .iter()
        .filter(|decision| decision.outcome == Outcome::Accepted)
        .collect();
    let mut histogram: BTreeMap<String, u32> = BTreeMap::new();
    for decision in &accepted_all {
        *histogram
            .entry(intent_class_key(&decision.intent))
            .or_insert(0) += 1;
    }
    for (class, count) in &histogram {
        put(
            measures,
            format!("{M_INTENT_PREFIX}{class}"),
            f64::from(*count) / accepted_all.len() as f64,
        );
    }
    let firings = |kind: &str| {
        f64::from(
            histogram
                .get(&intent_class_key(&intent_key(SPECIALIST_FOOD, kind, "")))
                .copied()
                .unwrap_or(0),
        )
    };
    put(measures, M_UPGRADES_DECLARED, firings(INTENT_UPGRADE));
    put(measures, M_SPLITS, firings(INTENT_SPLIT));
    for name in names {
        let own: Vec<_> = decisions
            .iter()
            .filter(|decision| decision.specialist == name)
            .collect();
        let accepted: Vec<_> = own
            .iter()
            .filter(|decision| decision.outcome == Outcome::Accepted)
            .collect();
        let mut rejected: BTreeMap<&str, u32> = BTreeMap::new();
        for decision in &own {
            if let Outcome::Rejected { rejected_by } = &decision.outcome {
                *rejected.entry(rejected_by.as_str()).or_insert(0) += 1;
            }
        }
        let prefix = format!("{M_SPECIALIST_PREFIX}{name}.");
        put(
            measures,
            format!("{prefix}{M_ACCEPTED}"),
            accepted.len() as f64,
        );
        for (reason, count) in &rejected {
            put(
                measures,
                format!("{prefix}{M_REJECTED_PREFIX}{reason}"),
                f64::from(*count),
            );
        }
        put(
            measures,
            format!("{prefix}{M_ACCEPTANCE_RATE}"),
            accepted.len() as f64 / own.len() as f64,
        );
        let (liveness, churn) = match span {
            Some((first, last)) => {
                let windows = windows(first, last);
                let live = windows.iter().all(|(start, end)| {
                    accepted
                        .iter()
                        .any(|decision| decision.tick >= *start && decision.tick < *end)
                });
                let distinct_per_window: f64 = windows
                    .iter()
                    .map(|(start, end)| {
                        accepted
                            .iter()
                            .filter(|decision| decision.tick >= *start && decision.tick < *end)
                            .map(|decision| decision.intent.as_str())
                            .collect::<BTreeSet<_>>()
                            .len() as f64
                    })
                    .sum::<f64>()
                    / windows.len() as f64;
                (if live { LIVE } else { NOT_LIVE }, distinct_per_window)
            }
            // No scoreboard rows: no turn was acted on, so nothing was live in any window.
            None => (NOT_LIVE, 0.0),
        };
        put(measures, format!("{prefix}{M_LIVENESS}"), liveness);
        put(measures, format!("{prefix}{M_INTENT_CHURN}"), churn);
    }
}

/// `<specialist>:<kind>` of an intent key — the first two tokens; a bare key is its own class.
fn intent_class_key(intent: &str) -> String {
    let mut tokens = intent.split(INTENT_SEPARATOR);
    match (tokens.next(), tokens.next()) {
        (Some(specialist), Some(kind)) => format!("{specialist}{INTENT_SEPARATOR}{kind}"),
        _ => intent.to_owned(),
    }
}

fn orchestrator(rows: &[ScoreRow], records: &[DecisionRecord], measures: &mut Measures) {
    let plans: Vec<_> = records
        .iter()
        .filter_map(|record| match record {
            DecisionRecord::Plan(plan) => Some(plan),
            _ => None,
        })
        .collect();
    let alarms: Vec<_> = records
        .iter()
        .filter_map(|record| match record {
            DecisionRecord::Alarm(alarm) => Some(alarm),
            _ => None,
        })
        .collect();

    let switches = match (plans.is_empty(), tick_span(rows)) {
        (false, Some((first, last))) => {
            let turns = (last - first + 1) as f64;
            let switches = plans
                .windows(2)
                .filter(|pair| pair[0].stance != pair[1].stance)
                .count() as f64;
            Some(switches * STANCE_SWITCH_PER_TURNS / turns)
        }
        _ => None,
    };
    measures.insert(M_STANCE_SWITCHES.to_owned(), switches);

    // From each alarm to the next plan whose budgets differ from the plan in force at the alarm.
    let latencies: Vec<f64> = alarms
        .iter()
        .filter_map(|alarm| {
            let in_force = plans
                .iter()
                .rev()
                .find(|plan| plan.tick <= alarm.tick)
                .map(|plan| &plan.budgets);
            plans
                .iter()
                .filter(|plan| plan.tick >= alarm.tick)
                .find(|plan| in_force != Some(&plan.budgets))
                .map(|plan| (plan.tick - alarm.tick) as f64)
        })
        .collect();
    // No alarm, no plan, or no plan that answered: nothing to average.
    let latency = if latencies.is_empty() {
        None
    } else {
        Some(latencies.iter().sum::<f64>() / latencies.len() as f64)
    };
    measures.insert(M_ALARM_LATENCY.to_owned(), latency);
}

fn link(rows: &[ScoreRow], records: &[DecisionRecord], measures: &mut Measures) {
    let observed: BTreeSet<u64> = rows.iter().map(|row| row.tick).collect();
    let ready: BTreeSet<u64> = records
        .iter()
        .filter_map(|record| match record {
            DecisionRecord::Ready(ready) => Some(ready.tick),
            _ => None,
        })
        .collect();
    let lost = observed.difference(&ready).count();
    let reconnects = records
        .iter()
        .filter(|record| {
            matches!(
                record,
                DecisionRecord::Link(link) if link.event == LinkEventKind::CommandReconnect
            )
        })
        .count();
    put(measures, M_TURNS_OBSERVED, observed.len() as f64);
    put(measures, M_TURNS_LOST, lost as f64);
    put(measures, M_RECONNECTS, reconnects as f64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::decisions::{
        AlarmRecord, Decision, LinkRecord, PlanRecord, ReadyRecord,
    };

    const FIRST_TICK: u64 = 5;
    const FOOD: &str = "food";
    const LAND: &str = "land";

    fn row(tick: u64, hunger: u32) -> ScoreRow {
        let mut deaths = BTreeMap::new();
        if hunger > 0 {
            deaths.insert(DEATH_CAUSE_HUNGER.to_owned(), hunger);
        }
        ScoreRow {
            tick,
            faction: 1,
            population_children: 3,
            population_working: 10 + tick as u32,
            population_elders: 2,
            food_stock: 40.0,
            food_income: 5.0,
            food_consumption: 4.0,
            sustainable_yield: 6.0,
            actual_yield: 5.0,
            runway_turns: 8.0,
            idle_workers: 1,
            patches_owned: 2,
            patches_improved: 1,
            herd_biomass_in_view: 100.0,
            herds_corralled: 0,
            intensification_knowledge: BTreeMap::from([("corral".to_owned(), 0.25)]),
            craft_knowledge: BTreeMap::new(),
            deaths_by_cause: deaths,
            victory_progress: BTreeMap::from([("survive".to_owned(), 0.1)]),
            commands_failed: u32::from(tick == FIRST_TICK + 2),
        }
    }

    fn decision(tick: u64, specialist: &str, intent: &str, outcome: Outcome) -> DecisionRecord {
        DecisionRecord::Decision(Decision {
            tick,
            specialist: specialist.to_owned(),
            intent: intent.to_owned(),
            score_raw: 1.0,
            score_final: 1.0,
            outcome,
            reason: String::new(),
            commands: 1,
            commands_text: Vec::new(),
        })
    }

    fn rejected(reason: &str) -> Outcome {
        Outcome::Rejected {
            rejected_by: reason.to_owned(),
        }
    }

    /// Two windows' worth of turns; `food` accepted in both, `land` only in the first.
    fn a_run() -> (Vec<ScoreRow>, Vec<DecisionRecord>) {
        let last = FIRST_TICK + LIVENESS_WINDOW_TURNS + 2;
        let rows: Vec<ScoreRow> = (FIRST_TICK..=last)
            .map(|tick| row(tick, u32::from(tick == FIRST_TICK + 1) * 2))
            .collect();
        let mut records = vec![
            decision(FIRST_TICK, FOOD, "forage", Outcome::Accepted),
            decision(FIRST_TICK, FOOD, "hunt", rejected("over_budget")),
            decision(FIRST_TICK + 1, FOOD, "hunt", Outcome::Accepted),
            decision(FIRST_TICK + 1, LAND, "cultivate", Outcome::Accepted),
            decision(FIRST_TICK + 2, LAND, "cultivate", rejected("conflict")),
            decision(FIRST_TICK + 2, LAND, "claim", rejected("conflict")),
            decision(last, FOOD, "forage", Outcome::Accepted),
        ];
        // Every tick but the last was submitted.
        records.extend((FIRST_TICK..last).map(|tick| DecisionRecord::Ready(ReadyRecord { tick })));
        records.push(DecisionRecord::Link(LinkRecord {
            tick: Some(FIRST_TICK + 3),
            event: LinkEventKind::CommandReconnect,
        }));
        records.push(DecisionRecord::Link(LinkRecord {
            tick: Some(FIRST_TICK + 4),
            event: LinkEventKind::StreamReopen,
        }));
        (rows, records)
    }

    fn value(measures: &Measures, name: &str) -> f64 {
        measures
            .get(name)
            .copied()
            .flatten()
            .unwrap_or_else(|| panic!("{name} is measured"))
    }

    #[test]
    fn the_whole_seat_reads_the_last_row_and_sums_hunger_over_the_run() {
        let (rows, records) = a_run();
        let measures = compute(&rows, &records);
        let last = rows.last().unwrap();
        assert_eq!(
            value(&measures, M_POPULATION_WORKING),
            f64::from(last.population_working)
        );
        assert_eq!(value(&measures, M_HUNGER_DEATHS_TOTAL), 2.0);
        assert_eq!(value(&measures, M_COMMANDS_FAILED_TOTAL), 1.0);
        assert_eq!(
            value(&measures, &format!("{M_INTENSIFICATION_PREFIX}corral")),
            0.25
        );
        assert_eq!(
            value(&measures, &format!("{M_VICTORY_PREFIX}survive")),
            f64::from(0.1f32),
            "an f32 row value is widened, not re-rounded"
        );
        assert_eq!(
            value(&measures, &format!("{M_DEATHS_PREFIX}{DEATH_CAUSE_HUNGER}")),
            0.0,
            "no death on the last tick reads as zero, not as absent"
        );
    }

    #[test]
    fn a_specialist_is_measured_on_acceptance_liveness_and_churn() {
        let (rows, records) = a_run();
        let measures = compute(&rows, &records);
        let food = |name: &str| value(&measures, &format!("{M_SPECIALIST_PREFIX}{FOOD}.{name}"));
        let land = |name: &str| value(&measures, &format!("{M_SPECIALIST_PREFIX}{LAND}.{name}"));
        assert_eq!(food(M_ACCEPTED), 3.0);
        assert_eq!(food(&format!("{M_REJECTED_PREFIX}over_budget")), 1.0);
        assert_eq!(food(M_ACCEPTANCE_RATE), 0.75);
        assert_eq!(food(M_LIVENESS), LIVE, "accepted in both windows");
        // First window: forage + hunt = 2 intents; second: forage = 1. Mean 1.5.
        assert_eq!(food(M_INTENT_CHURN), 1.5);
        assert_eq!(land(M_ACCEPTED), 1.0);
        assert_eq!(land(&format!("{M_REJECTED_PREFIX}conflict")), 2.0);
        assert_eq!(land(M_LIVENESS), NOT_LIVE, "the second window is empty");
        assert_eq!(land(M_INTENT_CHURN), 0.5);
        // Neither Food rule the slice counts fired: the bare fixture intents are not `food:*`.
        assert_eq!(value(&measures, M_UPGRADES_DECLARED), 0.0);
        assert_eq!(value(&measures, M_SPLITS), 0.0);
        let (rows, mut records) = a_run();
        records.push(decision(
            FIRST_TICK + 1,
            FOOD,
            "food:upgrade:7001",
            Outcome::Accepted,
        ));
        records.push(decision(
            FIRST_TICK + 2,
            FOOD,
            "food:upgrade:7001",
            rejected("conflict"),
        ));
        records.push(decision(
            FIRST_TICK + 2,
            FOOD,
            "food:split:7001",
            Outcome::Accepted,
        ));
        let with_firings = compute(&rows, &records);
        assert_eq!(
            value(&with_firings, M_UPGRADES_DECLARED),
            1.0,
            "accepted firings only"
        );
        assert_eq!(value(&with_firings, M_SPLITS), 1.0);
        // Four accepted with bare intents: forage, hunt, cultivate, forage.
        assert_eq!(value(&measures, &format!("{M_INTENT_PREFIX}forage")), 0.5);
        assert_eq!(value(&measures, &format!("{M_INTENT_PREFIX}hunt")), 0.25);
        assert_eq!(
            value(&measures, &format!("{M_INTENT_PREFIX}cultivate")),
            0.25
        );
    }

    #[test]
    fn the_link_counts_observed_lost_and_command_reconnects() {
        let (rows, records) = a_run();
        let measures = compute(&rows, &records);
        assert_eq!(value(&measures, M_TURNS_OBSERVED), rows.len() as f64);
        assert_eq!(
            value(&measures, M_TURNS_LOST),
            1.0,
            "the last tick had no ready"
        );
        assert_eq!(
            value(&measures, M_RECONNECTS),
            1.0,
            "a stream reopen is not a reconnect"
        );
    }

    #[test]
    fn the_orchestrator_rows_are_null_without_plan_records() {
        let (rows, records) = a_run();
        let measures = compute(&rows, &records);
        assert_eq!(measures.get(M_STANCE_SWITCHES), Some(&None));
        assert_eq!(measures.get(M_ALARM_LATENCY), Some(&None));
    }

    #[test]
    fn the_orchestrator_rows_read_switches_and_alarm_latency_from_plans() {
        let (rows, mut records) = a_run();
        let plan = |tick: u64, stance: &str, food_budget: f32| {
            DecisionRecord::Plan(PlanRecord {
                tick,
                stance: stance.to_owned(),
                since_tick: tick,
                budgets: BTreeMap::from([(FOOD.to_owned(), food_budget)]),
                priorities: BTreeMap::new(),
                goals: BTreeMap::new(),
            })
        };
        records.push(plan(FIRST_TICK, "settle", 1.0));
        records.push(DecisionRecord::Alarm(AlarmRecord {
            tick: FIRST_TICK + 2,
            specialist: FOOD.to_owned(),
            alarm: "hunger".to_owned(),
        }));
        records.push(plan(FIRST_TICK + 3, "settle", 1.0)); // same budgets: not a response
        records.push(plan(FIRST_TICK + 5, "roam", 2.0)); // a switch, and the response
        let measures = compute(&rows, &records);
        let turns = rows.len() as f64;
        assert_eq!(
            value(&measures, M_STANCE_SWITCHES),
            STANCE_SWITCH_PER_TURNS / turns
        );
        assert_eq!(value(&measures, M_ALARM_LATENCY), 3.0);
    }

    #[test]
    fn an_empty_scoreboard_measures_only_the_totals_and_the_link() {
        let measures = compute(&[], &[]);
        assert_eq!(value(&measures, M_HUNGER_DEATHS_TOTAL), 0.0);
        assert_eq!(value(&measures, M_TURNS_OBSERVED), 0.0);
        assert!(!measures.contains_key(M_POPULATION_WORKING));
    }
}
