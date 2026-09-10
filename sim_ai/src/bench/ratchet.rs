//! **The report, the comparison and the ratchet** (`docs/plan_ai_driver.md` §8.3).
//!
//! A run's measures go to `report.json`; `--compare` subtracts another run's; `--check` holds
//! them against `baselines.json` and fails on a drop beyond tolerance; `--write-baselines`
//! regenerates that file. The AI is deterministic given `(map_seed, faction, turn)`, so a
//! comparison is exact on a replay of the same configuration and a change in the numbers is a
//! change in the AI (or the sim), never noise.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::measures::{
    Measures, M_COMMANDS_FAILED_TOTAL, M_CRAFT_PREFIX, M_DEATHS_PREFIX, M_FOOD_STOCK,
    M_HUNGER_DEATHS_TOTAL, M_INTENSIFICATION_PREFIX, M_INTENT_PREFIX, M_POPULATION_CHILDREN,
    M_POPULATION_ELDERS, M_POPULATION_WORKING, M_RECONNECTS, M_STANCE_SWITCHES, M_TURNS_LOST,
    M_VICTORY_PREFIX,
};

/// The report file, under `--out`.
pub const REPORT_FILE: &str = "report.json";

/// **The ratcheted measures**, written to a baseline file with [`BASELINE_TOLERANCE`] each: the
/// primary (population) and the guard (hunger deaths) of §8.2, plus the larder. Every other
/// measure rides the report and the comparison, but only these fail a `--check`.
pub const RATCHETED_MEASURES: [&str; 7] = [
    M_POPULATION_CHILDREN,
    M_POPULATION_WORKING,
    M_POPULATION_ELDERS,
    M_FOOD_STOCK,
    M_HUNGER_DEATHS_TOTAL,
    M_COMMANDS_FAILED_TOTAL,
    M_STANCE_SWITCHES,
];
/// The synthetic measure `--compare` adds per seat: the L1 distance between the two runs'
/// intent histograms (Σ |Δ| over every `intent.*` key, an absent key reading 0) — the number
/// "two profiles that visibly differ" resolves to (§8.2, profile divergence). 0 is the same
/// behaviour; 2 is disjoint.
pub const M_INTENT_DISTANCE_L1: &str = "intent_distance_l1";
/// The seat specs of a baseline entry are joined by this into the entry's key.
const BASELINE_KEY_SEPARATOR: &str = " ";
/// The tolerance a regenerated baseline carries: none, because the replay is exact.
pub const BASELINE_TOLERANCE: f64 = 0.0;

/// **The shipped baselines** (`sim_ai/bench/baselines.json`): the all-Pass control and the
/// utility forager on these two seeds for this many turns — the runs every later brain is compared
/// to. Regenerated with `sim_ai bench --seeds 11,23 --turns 30 --seats <one of the sets below>
/// --write-baselines sim_ai/bench/baselines.json` (one run per set; the file merges), in the PR
/// that moves it, with the numbers in the PR body.
#[cfg(test)]
pub const BASELINE_SEEDS: [u64; 2] = [11, 23];
#[cfg(test)]
pub const BASELINE_TURNS: u64 = 30;
#[cfg(test)]
pub const BASELINE_SEAT_SETS: [[&str; 2]; 2] =
    [["1=pass", "2=pass"], ["1=utility:forager", "2=pass"]];
/// The shipped file, embedded so a test can hold it to the constants above without a path.
#[cfg(test)]
const SHIPPED_BASELINES: &str = include_str!("../../bench/baselines.json");

/// **The measures that are better lower**: a check fails when they rise past the tolerance. Every
/// other measure is better higher and fails when it drops.
const LOWER_IS_BETTER: [&str; 5] = [
    M_HUNGER_DEATHS_TOTAL,
    M_TURNS_LOST,
    M_RECONNECTS,
    M_COMMANDS_FAILED_TOTAL,
    M_STANCE_SWITCHES,
];
const LOWER_IS_BETTER_PREFIXES: [&str; 1] = [M_DEATHS_PREFIX];

/// Measure prefixes the printed table leaves out (the report carries them): one row per
/// knowledge and per victory mode would swamp the seat's dozen scalars.
const TABLE_OMITTED_PREFIXES: [&str; 3] =
    [M_INTENSIFICATION_PREFIX, M_CRAFT_PREFIX, M_VICTORY_PREFIX];
/// Column widths of the printed table.
const TABLE_NAME_WIDTH: usize = 44;
const TABLE_VALUE_WIDTH: usize = 14;
/// Decimals printed per value.
const TABLE_DECIMALS: usize = 3;
/// What an unmeasured cell prints as.
const TABLE_NULL: &str = "-";

/// seed → seat → measures.
pub type RunMeasures = BTreeMap<String, BTreeMap<String, Measures>>;

/// `report.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub seeds: Vec<u64>,
    pub turns: u64,
    /// The seat specs as given (`<faction>=<brain>[:<script>]`).
    pub seats: Vec<String>,
    /// Wall-clock seconds per seed, for the record.
    pub wall_seconds: BTreeMap<String, f64>,
    pub measures: RunMeasures,
    /// `this − other` per seed, seat and measure, when `--compare` was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compare: Option<RunMeasures>,
    /// The outcome of `--check`, when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<CheckOutcome>,
}

/// One entry of `baselines.json`: a seat set's measures on pinned seeds and turns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Baselines {
    pub seeds: Vec<u64>,
    pub turns: u64,
    pub seats: Vec<String>,
    pub measures: RunMeasures,
    /// Measure name → absolute tolerance. Only measures named here are checked.
    pub tolerance: BTreeMap<String, f64>,
}

/// `baselines.json`: one entry per seat set, keyed by the specs joined with a space.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BaselinesFile {
    pub runs: BTreeMap<String, Baselines>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Violation {
    pub seed: String,
    pub seat: String,
    pub measure: String,
    pub baseline: f64,
    pub tolerance: f64,
    pub actual: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckOutcome {
    pub baselines: String,
    pub violations: Vec<Violation>,
}

#[derive(Debug, thiserror::Error)]
pub enum RatchetError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not a report: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    /// The other run or the baseline has different seeds, turns or seats: the numbers would answer
    /// different questions.
    #[error("{0}")]
    Mismatch(String),
}

impl Report {
    pub fn write(&self, path: &Path) -> Result<(), RatchetError> {
        let text = serde_json::to_string_pretty(self).expect("a report serialises");
        fs::write(path, text).map_err(|source| RatchetError::Write {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn read(path: &Path) -> Result<Self, RatchetError> {
        let text = fs::read_to_string(path).map_err(|source| RatchetError::Read {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| RatchetError::Parse {
            path: path.display().to_string(),
            source,
        })
    }

    /// `this − other`, per seed, seat and measure both runs measured, plus
    /// [`M_INTENT_DISTANCE_L1`] per seat. The other run must have the same seeds, turns and seat
    /// **factions** — the brains may differ, which is what a comparison is for.
    pub fn compare(&self, other: &Report) -> Result<RunMeasures, RatchetError> {
        if self.seeds != other.seeds
            || self.turns != other.turns
            || seat_factions(&self.seats) != seat_factions(&other.seats)
        {
            return Err(RatchetError::Mismatch(format!(
                "--compare: this run has seeds {:?} / turns {} / seats {:?}, the other has {:?} / {} / {:?}",
                self.seeds, self.turns, self.seats, other.seeds, other.turns, other.seats
            )));
        }
        let mut deltas = subtract(&self.measures, &other.measures);
        for (seed, seats) in &mut deltas {
            for (seat, measures) in seats {
                let ours = self.measures.get(seed).and_then(|s| s.get(seat));
                let theirs = other.measures.get(seed).and_then(|s| s.get(seat));
                if let (Some(ours), Some(theirs)) = (ours, theirs) {
                    measures.insert(
                        M_INTENT_DISTANCE_L1.to_owned(),
                        Some(intent_distance_l1(ours, theirs)),
                    );
                }
            }
        }
        Ok(deltas)
    }

    /// Hold this run against the file's entry for its seat set: every violation, or none.
    pub fn check(&self, file: &BaselinesFile) -> Result<Vec<Violation>, RatchetError> {
        let baselines = file.find(&self.seeds, self.turns, &self.seats)?;
        let mut violations = Vec::new();
        for (seed, seats) in &baselines.measures {
            for (seat, measures) in seats {
                for (measure, tolerance) in &baselines.tolerance {
                    let Some(Some(baseline)) = measures.get(measure) else {
                        continue;
                    };
                    let Some(Some(actual)) = self
                        .measures
                        .get(seed)
                        .and_then(|seats| seats.get(seat))
                        .and_then(|measures| measures.get(measure))
                    else {
                        continue;
                    };
                    let violated = if lower_is_better(measure) {
                        *actual > baseline + tolerance
                    } else {
                        *actual < baseline - tolerance
                    };
                    if violated {
                        violations.push(Violation {
                            seed: seed.clone(),
                            seat: seat.clone(),
                            measure: measure.clone(),
                            baseline: *baseline,
                            tolerance: *tolerance,
                            actual: *actual,
                        });
                    }
                }
            }
        }
        Ok(violations)
    }

    /// The baseline file this run would be: the ratcheted measures at [`BASELINE_TOLERANCE`].
    pub fn as_baselines(&self) -> Baselines {
        Baselines {
            seeds: self.seeds.clone(),
            turns: self.turns,
            seats: self.seats.clone(),
            measures: self.measures.clone(),
            tolerance: RATCHETED_MEASURES
                .iter()
                .map(|measure| ((*measure).to_owned(), BASELINE_TOLERANCE))
                .collect(),
        }
    }

    /// The text table: one block per seed, a row per measure, a column per seat — and, with a
    /// comparison, a delta column beside each.
    pub fn table(&self) -> String {
        let mut out = String::new();
        for (seed, seats) in &self.measures {
            let _ = writeln!(out, "seed {seed}  ({} turns)", self.turns);
            let _ = write!(out, "{:<TABLE_NAME_WIDTH$}", "measure");
            for seat in seats.keys() {
                let _ = write!(out, "{:>TABLE_VALUE_WIDTH$}", format!("seat {seat}"));
                if self.compare.is_some() {
                    let _ = write!(out, "{:>TABLE_VALUE_WIDTH$}", "delta");
                }
            }
            let _ = writeln!(out);
            let names: std::collections::BTreeSet<&String> = seats
                .values()
                .flat_map(|measures| measures.keys())
                .filter(|name| {
                    !TABLE_OMITTED_PREFIXES
                        .iter()
                        .any(|prefix| name.starts_with(prefix))
                })
                .collect();
            for name in names {
                let _ = write!(out, "{name:<TABLE_NAME_WIDTH$}");
                for (seat, measures) in seats {
                    let _ = write!(
                        out,
                        "{:>TABLE_VALUE_WIDTH$}",
                        cell(measures.get(name).copied().flatten())
                    );
                    if let Some(compare) = &self.compare {
                        let delta = compare
                            .get(seed)
                            .and_then(|seats| seats.get(seat))
                            .and_then(|measures| measures.get(name))
                            .copied()
                            .flatten();
                        let _ = write!(out, "{:>TABLE_VALUE_WIDTH$}", cell(delta));
                    }
                }
                let _ = writeln!(out);
            }
            let _ = writeln!(
                out,
                "({} rows omitted from the table; see {REPORT_FILE})",
                TABLE_OMITTED_PREFIXES.join(" ")
            );
        }
        if let Some(check) = &self.check {
            if check.violations.is_empty() {
                let _ = writeln!(out, "check against {}: no violations", check.baselines);
            } else {
                let _ = writeln!(
                    out,
                    "check against {}: {} violation(s)",
                    check.baselines,
                    check.violations.len()
                );
                for violation in &check.violations {
                    let _ = writeln!(
                        out,
                        "  seed {} seat {} {}: {} vs baseline {} ± {}",
                        violation.seed,
                        violation.seat,
                        violation.measure,
                        violation.actual,
                        violation.baseline,
                        violation.tolerance
                    );
                }
            }
        }
        out
    }
}

impl Baselines {
    /// The entry's key in the file.
    pub fn key(&self) -> String {
        self.seats.join(BASELINE_KEY_SEPARATOR)
    }
}

impl BaselinesFile {
    pub fn read(path: &Path) -> Result<Self, RatchetError> {
        let text = fs::read_to_string(path).map_err(|source| RatchetError::Read {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| RatchetError::Parse {
            path: path.display().to_string(),
            source,
        })
    }

    /// The file at `path`, or an empty one when there is none yet — a first `--write-baselines`.
    pub fn read_or_empty(path: &Path) -> Result<Self, RatchetError> {
        if path.is_file() {
            Self::read(path)
        } else {
            Ok(Self::default())
        }
    }

    /// Add or replace the entry for `baselines`' seat set.
    pub fn upsert(&mut self, baselines: Baselines) {
        self.runs.insert(baselines.key(), baselines);
    }

    /// The entry for exactly this run shape, or a mismatch naming what the file holds.
    pub fn find(
        &self,
        seeds: &[u64],
        turns: u64,
        seats: &[String],
    ) -> Result<&Baselines, RatchetError> {
        self.runs
            .values()
            .find(|entry| entry.seeds == seeds && entry.turns == turns && entry.seats == seats)
            .ok_or_else(|| {
                RatchetError::Mismatch(format!(
                    "--check: no baseline for seeds {seeds:?} / turns {turns} / seats {seats:?}; the file holds {:?}",
                    self.runs
                        .values()
                        .map(|entry| format!("{:?} / {} / {}", entry.seeds, entry.turns, entry.key()))
                        .collect::<Vec<_>>()
                ))
            })
    }

    pub fn write(&self, path: &Path) -> Result<(), RatchetError> {
        let text = serde_json::to_string_pretty(self).expect("baselines serialise");
        fs::write(path, text).map_err(|source| RatchetError::Write {
            path: path.display().to_string(),
            source,
        })
    }
}

/// The faction of each seat spec — the part before `=`.
fn seat_factions(seats: &[String]) -> Vec<&str> {
    seats
        .iter()
        .map(|seat| {
            seat.split_once('=')
                .map_or(seat.as_str(), |(faction, _)| faction)
        })
        .collect()
}

/// Σ |a − b| over the union of the two seats' `intent.*` keys.
fn intent_distance_l1(ours: &Measures, theirs: &Measures) -> f64 {
    let keys: std::collections::BTreeSet<&String> = ours
        .keys()
        .chain(theirs.keys())
        .filter(|key| key.starts_with(M_INTENT_PREFIX))
        .collect();
    keys.into_iter()
        .map(|key| {
            let share = |measures: &Measures| measures.get(key).copied().flatten().unwrap_or(0.0);
            (share(ours) - share(theirs)).abs()
        })
        .sum()
}

fn lower_is_better(measure: &str) -> bool {
    LOWER_IS_BETTER.contains(&measure)
        || LOWER_IS_BETTER_PREFIXES
            .iter()
            .any(|prefix| measure.starts_with(prefix))
}

fn subtract(ours: &RunMeasures, theirs: &RunMeasures) -> RunMeasures {
    ours.iter()
        .map(|(seed, seats)| {
            let seats = seats
                .iter()
                .map(|(seat, measures)| {
                    let measures = measures
                        .iter()
                        .map(|(name, value)| {
                            let other = theirs
                                .get(seed)
                                .and_then(|seats| seats.get(seat))
                                .and_then(|measures| measures.get(name))
                                .copied()
                                .flatten();
                            let delta = match (value, other) {
                                (Some(ours), Some(theirs)) => Some(ours - theirs),
                                _ => None,
                            };
                            (name.clone(), delta)
                        })
                        .collect();
                    (seat.clone(), measures)
                })
                .collect();
            (seed.clone(), seats)
        })
        .collect()
}

fn cell(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.TABLE_DECIMALS$}"),
        None => TABLE_NULL.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: &str = "7";
    const SEAT: &str = "1";

    fn report_with(measures: &[(&str, Option<f64>)]) -> Report {
        let measures: Measures = measures
            .iter()
            .map(|(name, value)| ((*name).to_owned(), *value))
            .collect();
        Report {
            seeds: vec![7],
            turns: 6,
            seats: vec!["1=pass".to_owned()],
            wall_seconds: BTreeMap::new(),
            measures: BTreeMap::from([(
                SEED.to_owned(),
                BTreeMap::from([(SEAT.to_owned(), measures)]),
            )]),
            compare: None,
            check: None,
        }
    }

    fn baselines_with(measures: &[(&str, Option<f64>)], tolerance: f64) -> BaselinesFile {
        let mut baselines = report_with(measures).as_baselines();
        for value in baselines.tolerance.values_mut() {
            *value = tolerance;
        }
        let mut file = BaselinesFile::default();
        file.upsert(baselines);
        file
    }

    fn entry(file: &mut BaselinesFile) -> &mut Baselines {
        file.runs.values_mut().next().unwrap()
    }

    #[test]
    fn a_drop_past_tolerance_fails_and_a_rise_passes_where_higher_is_better() {
        let baselines = baselines_with(&[(M_POPULATION_WORKING, Some(20.0))], 1.0);
        let within = report_with(&[(M_POPULATION_WORKING, Some(19.0))]);
        assert!(within.check(&baselines).unwrap().is_empty());
        let risen = report_with(&[(M_POPULATION_WORKING, Some(30.0))]);
        assert!(risen.check(&baselines).unwrap().is_empty());
        let dropped = report_with(&[(M_POPULATION_WORKING, Some(18.9))]);
        let violations = dropped.check(&baselines).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].measure, M_POPULATION_WORKING);
        assert_eq!(violations[0].actual, 18.9);
    }

    #[test]
    fn a_rise_past_tolerance_fails_and_a_drop_passes_where_lower_is_better() {
        let baselines = baselines_with(&[(M_HUNGER_DEATHS_TOTAL, Some(2.0))], 1.0);
        let within = report_with(&[(M_HUNGER_DEATHS_TOTAL, Some(3.0))]);
        assert!(within.check(&baselines).unwrap().is_empty());
        let fewer = report_with(&[(M_HUNGER_DEATHS_TOTAL, Some(0.0))]);
        assert!(fewer.check(&baselines).unwrap().is_empty());
        let more = report_with(&[(M_HUNGER_DEATHS_TOTAL, Some(3.5))]);
        assert_eq!(more.check(&baselines).unwrap().len(), 1);
    }

    #[test]
    fn a_measure_without_a_tolerance_entry_or_a_value_is_not_checked() {
        let mut baselines = baselines_with(&[(M_FOOD_STOCK, Some(10.0))], 0.0);
        entry(&mut baselines).tolerance.remove(M_FOOD_STOCK);
        let dropped = report_with(&[(M_FOOD_STOCK, Some(0.0))]);
        assert!(dropped.check(&baselines).unwrap().is_empty());
        let unmeasured = baselines_with(&[(M_FOOD_STOCK, None)], 0.0);
        assert!(dropped.check(&unmeasured).unwrap().is_empty());
    }

    #[test]
    fn a_different_shape_is_refused_by_check_and_compare() {
        let report = report_with(&[(M_FOOD_STOCK, Some(1.0))]);
        let mut other = report.clone();
        other.turns += 1;
        assert!(matches!(
            report.compare(&other),
            Err(RatchetError::Mismatch(_))
        ));
        let mut baselines = report.as_baselines();
        baselines.seeds.push(8);
        let mut file = BaselinesFile::default();
        file.upsert(baselines);
        assert!(matches!(
            report.check(&file),
            Err(RatchetError::Mismatch(_))
        ));
        // A comparison across brains on the same factions is allowed; across factions is not.
        let mut utility = report.clone();
        utility.seats = vec!["1=utility:rover".to_owned()];
        assert!(report.compare(&utility).is_ok());
        let mut other_faction = report.clone();
        other_faction.seats = vec!["2=pass".to_owned()];
        assert!(matches!(
            report.compare(&other_faction),
            Err(RatchetError::Mismatch(_))
        ));
    }

    #[test]
    fn compare_subtracts_where_both_measured_and_nulls_elsewhere() {
        let ours = report_with(&[(M_FOOD_STOCK, Some(5.0)), (M_POPULATION_WORKING, None)]);
        let theirs = report_with(&[(M_FOOD_STOCK, Some(3.0)), (M_POPULATION_WORKING, Some(1.0))]);
        let delta = ours.compare(&theirs).unwrap();
        let seat = &delta[SEED][SEAT];
        assert_eq!(seat[M_FOOD_STOCK], Some(2.0));
        assert_eq!(seat[M_POPULATION_WORKING], None);
        assert_eq!(
            seat[M_INTENT_DISTANCE_L1],
            Some(0.0),
            "no intents on either side"
        );
    }

    #[test]
    fn the_intent_distance_is_the_l1_gap_between_two_histograms() {
        let food = format!("{M_INTENT_PREFIX}food:assign");
        let land = format!("{M_INTENT_PREFIX}land:move");
        let ours = report_with(&[(&food, Some(0.75)), (&land, Some(0.25))]);
        let theirs = report_with(&[(&food, Some(1.0))]);
        let delta = ours.compare(&theirs).unwrap();
        assert!((delta[SEED][SEAT][M_INTENT_DISTANCE_L1].unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_shipped_baselines_hold_the_control_and_the_forager_on_the_pinned_seeds() {
        let file: BaselinesFile =
            serde_json::from_str(SHIPPED_BASELINES).expect("the shipped baselines parse");
        assert_eq!(file.runs.len(), BASELINE_SEAT_SETS.len());
        for seats in BASELINE_SEAT_SETS {
            let seats: Vec<String> = seats.iter().map(|s| (*s).to_owned()).collect();
            let baselines = file
                .find(&BASELINE_SEEDS, BASELINE_TURNS, &seats)
                .expect("an entry per shipped seat set");
            for measure in RATCHETED_MEASURES {
                assert!(
                    baselines.tolerance.contains_key(measure),
                    "{measure} is ratcheted but carries no tolerance"
                );
            }
            for seed in BASELINE_SEEDS {
                let by_seat = &baselines.measures[&seed.to_string()];
                for seat in &seats {
                    let (faction, brain) = seat.split_once('=').unwrap();
                    let measures = &by_seat[faction];
                    for measure in RATCHETED_MEASURES {
                        // The orchestrator row is null on a seat with no orchestrator.
                        let pass_seat = brain == "pass" && measure == M_STANCE_SWITCHES;
                        assert!(
                            pass_seat || matches!(measures.get(measure), Some(Some(_))),
                            "seed {seed} seat {faction}: {measure} is unmeasured in the shipped file"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn baselines_round_trip_and_carry_the_ratchet_set() {
        let report = report_with(&[(M_FOOD_STOCK, Some(1.0))]);
        let baselines = report.as_baselines();
        assert_eq!(baselines.tolerance.len(), RATCHETED_MEASURES.len());
        let text = serde_json::to_string(&baselines).unwrap();
        let back: Baselines = serde_json::from_str(&text).unwrap();
        assert_eq!(back, baselines);
    }

    #[test]
    fn the_table_prints_a_value_per_seat_and_a_dash_for_null() {
        let mut report = report_with(&[(M_FOOD_STOCK, Some(1.5)), (M_POPULATION_WORKING, None)]);
        let table = report.table();
        assert!(table.contains("seat 1"));
        assert!(table.contains("1.500"));
        assert!(table.contains(TABLE_NULL));
        report.compare = Some(report.compare(&report.clone()).unwrap());
        assert!(report.table().contains("delta"));
    }
}
