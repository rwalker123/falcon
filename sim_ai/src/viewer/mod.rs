//! **`sim_ai viewer`** — one seat's three logs, joined by tick into one self-contained page
//! (`docs/plan_ai_driver.md` §8.4: *look before tuning*).
//!
//! `sim_ai viewer <run-dir> [--seed <s>] [--seat <f>] --out <page.html>` reads
//! `<run-dir>/<seed>/seat_<f>/{scoreboard,decisions,observations}.jsonl` — the layout `bench`
//! writes — and joins them into one [`RunModel`]: a [`Turn`] per tick carrying the `ScoreRow`, the
//! `Observation`, the decisions, the plan adopted and the alarms raised on that tick. A turn with a
//! part missing carries `null` for it and never fails the page: a seat killed mid-turn, or a log
//! written by an older build, is still readable.
//!
//! **A launcher run directory is accepted too** (`<data_dir>/runs/<run_id>`, `launcher.md`): it
//! holds `seat_<f>/` log directories for the rivals the launcher spawned and a `record/` the server
//! wrote (`core_sim/src/record.rs`). A seat with a log directory is read from it; a seat the record
//! holds frames for but no log directory — the human's — is imported into `seat_<f>/` first
//! (`import_record`), on the fly, so both seats of a played game open through the one command.
//!
//! ⛔ **The page is one file with nothing outside it.** The model is inlined as JSON and every
//! byte of CSS and JS is in the template (`page.html`, `include_str!`): no `<script src>`, no
//! `<link href>`, no font, no image by URL. It is published where every external host is blocked
//! and opened from a file with no network, and [`assert_self_contained`] refuses to write a page
//! that would break either.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Serialize;
use tracing::info;

use crate::bench::measures::{read_jsonl, MeasureError};
use crate::import_record::{self, ImportError};
use crate::instruments::decisions::{
    AlarmRecord, Decision, DecisionRecord, LinkRecord, PlanRecord, DECISIONS_FILE,
};
use crate::instruments::observations::{
    AlarmInForce, Observation, ObservationRecord, PlanInForce, OBSERVATIONS_FILE,
};
use crate::instruments::scoreboard::{ScoreRow, SCOREBOARD_FILE};

/// The page, with the model marker in it.
const PAGE_TEMPLATE: &str = include_str!("page.html");
/// Where the template takes the model: replaced, once, by the JSON of a [`RunModel`].
const MODEL_MARKER: &str = "__RUN_MODEL_JSON__";
/// The seat directory prefix the bench writes (`bench::SEAT_DIR_PREFIX`, restated as the layout
/// this reader expects).
const SEAT_DIR_PREFIX: &str = "seat_";
/// The server's record under a launcher run directory (`RECORD_DIR` in `launcher/src/main.rs`).
const RECORD_DIR: &str = "record";
/// What a self-contained page must never contain: a fetch of anything by URL.
const EXTERNAL_RESOURCE_MARKERS: [&str; 3] = ["http://", "https://", "src="];

#[derive(Parser, Debug)]
#[command(
    name = "sim_ai viewer",
    about = "Join one seat's bench logs by tick into a self-contained HTML page."
)]
pub struct ViewerArgs {
    /// A bench `--out` directory (`<run-dir>/<seed>/seat_<f>/` holds the logs), or a launcher
    /// run directory (`<run-dir>/seat_<f>/` and `<run-dir>/record/`).
    pub run_dir: PathBuf,
    /// The seed to read (default: the lowest seed directory found). Ignored for a launcher run.
    #[arg(long)]
    pub seed: Option<String>,
    /// The seat's faction to read (default: the lowest `seat_<f>` found — logged or recorded).
    #[arg(long)]
    pub seat: Option<u32>,
    /// Where the page is written.
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ViewerError {
    #[error("no seed directory under {0}")]
    NoSeeds(PathBuf),
    #[error("no {SEAT_DIR_PREFIX}<faction> directory under {0}")]
    NoSeats(PathBuf),
    #[error(transparent)]
    Log(#[from] MeasureError),
    #[error("io at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("the model could not be serialised: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the page would not be self-contained: it contains `{0}`")]
    ExternalResource(String),
    #[error(transparent)]
    Import(#[from] ImportError),
}

/// One tick of one seat, every instrument's word on it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Turn {
    pub tick: u64,
    pub score: Option<ScoreRow>,
    pub observation: Option<Observation>,
    /// Every proposal weighed this tick, in the order the arbiter recorded them.
    pub decisions: Vec<Decision>,
    /// The plan adopted **on** this tick, if the orchestrator re-planned.
    pub plan: Option<PlanRecord>,
    pub alarms: Vec<AlarmRecord>,
    pub ready: bool,
    pub link_events: Vec<LinkRecord>,
    /// **The plan in force on this tick**, resolved by [`resolve_in_force`]: this tick's `plan`
    /// record, else the observation's plan, else the plan in force on the previous turn. `None`
    /// only while the run has shown no plan at all.
    pub plan_in_force: Option<PlanInForce>,
    /// The alarms in force on this tick, by the same rule: raised this tick, else the
    /// observation's, else the previous turn's.
    pub alarms_in_force: Vec<AlarmInForce>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunModel {
    pub seed: String,
    pub faction: u32,
    pub seat_dir: String,
    pub turns: Vec<Turn>,
    /// Every specialist the decision log names, sorted — the page's tab set, so a new specialist
    /// appears with no template change.
    pub specialists: Vec<String>,
}

/// Run the viewer: locate the seat, join its logs, write the page.
pub fn run(args: ViewerArgs) -> Result<(), ViewerError> {
    let (label, seat_dir, faction) = locate_seat(&args)?;
    let model = read_model(&seat_dir, label, faction)?;
    let page = render_page(&model)?;
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent).map_err(|source| io_at(parent, source))?;
    }
    fs::write(&args.out, page).map_err(|source| io_at(&args.out, source))?;
    info!(
        page = %args.out.display(),
        turns = model.turns.len(),
        faction,
        "viewer page written"
    );
    Ok(())
}

/// **Where the seat's logs are, and what to call the run.** A launcher run directory (one holding
/// `record/`, or `seat_<f>/` directly) is the seed directory itself, labelled by its name and the
/// recorded world's seed; a seat under it with no logs but with recorded frames is imported first.
/// Anything else is a bench `--out`, `<run-dir>/<seed>/seat_<f>/`.
fn locate_seat(args: &ViewerArgs) -> Result<(String, PathBuf, u32), ViewerError> {
    let record_dir = args.run_dir.join(RECORD_DIR);
    let is_run_dir = record_dir.is_dir() || first_seat(&args.run_dir).is_ok();
    if !is_run_dir {
        let (seed, seed_dir) = match &args.seed {
            Some(seed) => (seed.clone(), args.run_dir.join(seed)),
            None => first_seed_dir(&args.run_dir)?,
        };
        let faction = match args.seat {
            Some(faction) => faction,
            None => first_seat(&seed_dir)?,
        };
        let seat_dir = seed_dir.join(format!("{SEAT_DIR_PREFIX}{faction}"));
        return Ok((seed, seat_dir, faction));
    }
    let recorded = import_record::recorded_seats(&record_dir);
    let faction = match args.seat {
        Some(faction) => faction,
        None => {
            let logged = first_seat(&args.run_dir).ok();
            match (logged, recorded.first().copied()) {
                (Some(a), Some(b)) => a.min(b),
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => return Err(ViewerError::NoSeats(args.run_dir.clone())),
            }
        }
    };
    let seat_dir = args.run_dir.join(format!("{SEAT_DIR_PREFIX}{faction}"));
    if !seat_dir.join(SCOREBOARD_FILE).is_file() && recorded.contains(&faction) {
        let summary = import_record::import(&record_dir, faction, &seat_dir)?;
        info!(
            faction,
            ticks = summary.ticks,
            decisions = summary.decisions,
            "seat imported from the record"
        );
    }
    let run_name = args
        .run_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let label = match import_record::read_run_info(&record_dir) {
        Some(info) => format!("{run_name} (seed {})", info.map_seed),
        None => run_name,
    };
    Ok((label, seat_dir, faction))
}

fn io_at(path: &Path, source: io::Error) -> ViewerError {
    ViewerError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// The lowest-numbered seed directory under `run_dir`.
fn first_seed_dir(run_dir: &Path) -> Result<(String, PathBuf), ViewerError> {
    let mut seeds: Vec<u64> = fs::read_dir(run_dir)
        .map_err(|source| io_at(run_dir, source))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect();
    seeds.sort_unstable();
    let seed = seeds
        .first()
        .ok_or_else(|| ViewerError::NoSeeds(run_dir.to_path_buf()))?
        .to_string();
    let dir = run_dir.join(&seed);
    Ok((seed, dir))
}

/// The lowest faction with a `seat_<f>` directory under `seed_dir`.
fn first_seat(seed_dir: &Path) -> Result<u32, ViewerError> {
    let mut seats: Vec<u32> = fs::read_dir(seed_dir)
        .map_err(|source| io_at(seed_dir, source))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_prefix(SEAT_DIR_PREFIX)?
                .parse()
                .ok()
        })
        .collect();
    seats.sort_unstable();
    seats
        .first()
        .copied()
        .ok_or_else(|| ViewerError::NoSeats(seed_dir.to_path_buf()))
}

/// Read the three logs under `seat_dir` and join them. A missing observations log is an empty
/// one — every turn's `observation` is then `null`; the two measured logs are required.
pub fn read_model(seat_dir: &Path, seed: String, faction: u32) -> Result<RunModel, ViewerError> {
    let rows: Vec<ScoreRow> = read_jsonl(&seat_dir.join(SCOREBOARD_FILE))?;
    let records: Vec<DecisionRecord> = read_jsonl(&seat_dir.join(DECISIONS_FILE))?;
    let observations_path = seat_dir.join(OBSERVATIONS_FILE);
    let observations: Vec<ObservationRecord> = if observations_path.is_file() {
        read_jsonl(&observations_path)?
    } else {
        Vec::new()
    };
    let turns = join(rows, records, observations);
    let specialists = specialists_of(&turns);
    Ok(RunModel {
        seed,
        faction,
        seat_dir: seat_dir.display().to_string(),
        turns,
        specialists,
    })
}

/// Every specialist named by a decision anywhere in the run, sorted and deduplicated.
pub fn specialists_of(turns: &[Turn]) -> Vec<String> {
    let mut specialists: Vec<String> = turns
        .iter()
        .flat_map(|turn| turn.decisions.iter().map(|d| d.specialist.clone()))
        .collect();
    specialists.sort();
    specialists.dedup();
    specialists
}

/// Join the three logs by tick, ascending. A tick any log names gets a turn; a part no log wrote
/// is `None`. A link event with no tick (before the first frame) lands on the first turn.
pub fn join(
    rows: Vec<ScoreRow>,
    records: Vec<DecisionRecord>,
    observations: Vec<ObservationRecord>,
) -> Vec<Turn> {
    let mut turns: BTreeMap<u64, Turn> = BTreeMap::new();
    for row in rows {
        let tick = row.tick;
        turn_at(&mut turns, tick).score = Some(row);
    }
    for ObservationRecord::Observation(observation) in observations {
        let tick = observation.tick;
        turn_at(&mut turns, tick).observation = Some(observation);
    }
    let mut untimed_links = Vec::new();
    for record in records {
        match record {
            DecisionRecord::Decision(decision) => {
                turn_at(&mut turns, decision.tick).decisions.push(decision)
            }
            DecisionRecord::Plan(plan) => {
                let tick = plan.tick;
                turn_at(&mut turns, tick).plan = Some(plan);
            }
            DecisionRecord::Alarm(alarm) => turn_at(&mut turns, alarm.tick).alarms.push(alarm),
            DecisionRecord::Ready(ready) => turn_at(&mut turns, ready.tick).ready = true,
            DecisionRecord::Link(link) => match link.tick {
                Some(tick) => turn_at(&mut turns, tick).link_events.push(link),
                None => untimed_links.push(link),
            },
        }
    }
    if let Some(first) = turns.values_mut().next() {
        first.link_events.splice(0..0, untimed_links);
    }
    let mut turns: Vec<Turn> = turns.into_values().collect();
    resolve_in_force(&mut turns);
    turns
}

/// **What plan and alarms each turn was played under.** The observation is written *before*
/// `decide`, so on the first acted tick `observation.plan` is `null` while the `plan` record for
/// that same tick exists — the orchestrator adopted it during that tick's `decide`. So the plan in
/// force is: this tick's plan record, else the observation's plan, else what was in force on the
/// previous turn (a tick with neither, or with no observation at all, inherits). The alarms
/// follow the same rule. A run that never shows a plan resolves every turn to `None` — the
/// page's "no orchestrator".
pub fn resolve_in_force(turns: &mut [Turn]) {
    let mut plan: Option<PlanInForce> = None;
    let mut alarms: Vec<AlarmInForce> = Vec::new();
    for turn in turns.iter_mut() {
        if let Some(record) = &turn.plan {
            plan = Some(PlanInForce {
                stance: record.stance.clone(),
                since_tick: record.since_tick,
                budgets: record.budgets.clone(),
                priorities: record.priorities.clone(),
            });
        } else if let Some(observed) = turn.observation.as_ref().and_then(|o| o.plan.as_ref()) {
            plan = Some(observed.clone());
        }
        if !turn.alarms.is_empty() {
            alarms = turn
                .alarms
                .iter()
                .map(|raised| AlarmInForce {
                    specialist: raised.specialist.clone(),
                    alarm: raised.alarm.clone(),
                    since_tick: raised.tick,
                })
                .collect();
        } else if let Some(observation) = &turn.observation {
            alarms = observation.alarms.clone();
        }
        turn.plan_in_force = plan.clone();
        turn.alarms_in_force = alarms.clone();
    }
}

/// The turn at `tick`, made empty if no log has named it yet.
fn turn_at(turns: &mut BTreeMap<u64, Turn>, tick: u64) -> &mut Turn {
    turns.entry(tick).or_insert_with(|| Turn {
        tick,
        score: None,
        observation: None,
        decisions: Vec::new(),
        plan: None,
        alarms: Vec::new(),
        ready: false,
        link_events: Vec::new(),
        plan_in_force: None,
        alarms_in_force: Vec::new(),
    })
}

/// The template with the model inlined, checked to be self-contained.
pub fn render_page(model: &RunModel) -> Result<String, ViewerError> {
    let json = script_safe_json(&serde_json::to_string(model)?);
    let page = PAGE_TEMPLATE.replacen(MODEL_MARKER, &json, 1);
    assert_self_contained(&page)?;
    Ok(page)
}

/// JSON that can sit inside a `<script>` element whatever the strings hold: `<`, `>` and `&`
/// become their `\u` escapes, so `</script>` inside a reason string cannot end the element early.
fn script_safe_json(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

/// Refuse a page that fetches anything by URL ([`EXTERNAL_RESOURCE_MARKERS`]).
pub fn assert_self_contained(page: &str) -> Result<(), ViewerError> {
    for marker in EXTERNAL_RESOURCE_MARKERS {
        if page.contains(marker) {
            return Err(ViewerError::ExternalResource(marker.to_owned()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::decisions::{LinkEventKind, Outcome, ReadyRecord};

    const FACTION: u32 = 1;
    const TICKS: [u64; 3] = [4, 5, 6];

    fn a_row(tick: u64) -> ScoreRow {
        let mut snapshot = sim_runtime::WorldSnapshot::default();
        snapshot.header.tick = tick;
        ScoreRow::from_snapshot(&snapshot, FACTION)
    }

    fn an_observation(tick: u64) -> ObservationRecord {
        let view = crate::view::SeatView {
            snapshot: {
                let mut snapshot = sim_runtime::WorldSnapshot::default();
                snapshot.header.tick = tick;
                snapshot
            },
            last_acted_tick: None,
        };
        ObservationRecord::Observation(Observation::capture(
            &view,
            &a_row(tick),
            &crate::brain::BrainLens::default(),
        ))
    }

    fn a_decision(tick: u64, outcome: Outcome) -> DecisionRecord {
        DecisionRecord::Decision(Decision {
            tick,
            specialist: "food".into(),
            intent: "food:assign:1".into(),
            score_raw: 0.5,
            score_final: 0.4,
            outcome,
            reason: "idle hands: forage 3,4 </script>".into(),
            commands: 1,
            commands_text: vec!["assign_labor 1 7001 forage 3 4 5".into()],
        })
    }

    /// Three hand-written sets: the scoreboard on every tick, decisions on the first two (one
    /// accepted, one rejected), a plan on the first, an untimed link event, and observations on
    /// the last two only.
    fn three_sets() -> (Vec<ScoreRow>, Vec<DecisionRecord>, Vec<ObservationRecord>) {
        let rows = TICKS.iter().map(|tick| a_row(*tick)).collect();
        let records = vec![
            DecisionRecord::Link(LinkRecord {
                tick: None,
                event: LinkEventKind::StreamReopen,
            }),
            DecisionRecord::Plan(PlanRecord {
                tick: TICKS[0],
                stance: "consolidate".into(),
                since_tick: TICKS[0],
                budgets: BTreeMap::from([("food".to_owned(), 1.0)]),
                priorities: BTreeMap::new(),
            }),
            a_decision(TICKS[0], Outcome::Accepted),
            DecisionRecord::Ready(ReadyRecord { tick: TICKS[0] }),
            a_decision(
                TICKS[1],
                Outcome::Rejected {
                    rejected_by: "over_budget".into(),
                },
            ),
            DecisionRecord::Alarm(AlarmRecord {
                tick: TICKS[1],
                specialist: "food".into(),
                alarm: "food_short".into(),
            }),
            DecisionRecord::Ready(ReadyRecord { tick: TICKS[1] }),
        ];
        let observations = vec![an_observation(TICKS[1]), an_observation(TICKS[2])];
        (rows, records, observations)
    }

    #[test]
    fn the_join_has_one_turn_per_tick_with_null_where_a_log_said_nothing() {
        let (rows, records, observations) = three_sets();
        let turns = join(rows, records, observations);
        assert_eq!(
            turns.iter().map(|turn| turn.tick).collect::<Vec<_>>(),
            TICKS.to_vec()
        );
        let first = &turns[0];
        assert!(first.score.is_some());
        assert!(first.observation.is_none(), "no observation on the first");
        assert!(first.plan.is_some());
        assert_eq!(first.decisions.len(), 1);
        assert!(first.ready);
        assert_eq!(
            first.link_events.len(),
            1,
            "the untimed link event lands on the first turn"
        );
        let second = &turns[1];
        assert!(second.observation.is_some());
        assert!(second.plan.is_none());
        assert_eq!(second.alarms.len(), 1);
        assert!(matches!(
            second.decisions[0].outcome,
            Outcome::Rejected { .. }
        ));
        let third = &turns[2];
        assert!(third.decisions.is_empty());
        assert!(!third.ready, "no ready record on the last tick");
        assert!(third.observation.is_some());
    }

    /// The observation is written before `decide`, so on the first acted tick its plan is null
    /// while that tick's plan record exists: the record wins. A later tick with neither inherits
    /// the last plan resolved. Alarms follow the same rule.
    #[test]
    fn the_plan_in_force_is_the_ticks_record_else_the_observation_else_the_last_one() {
        let (rows, records, observations) = three_sets();
        let turns = join(rows, records, observations);
        let first = &turns[0];
        assert!(
            first.observation.is_none() && first.plan.is_some(),
            "the fixture's first tick has a plan record and no observation"
        );
        let in_force = first
            .plan_in_force
            .as_ref()
            .expect("resolved to the record");
        assert_eq!(in_force.stance, "consolidate");
        assert_eq!(in_force.since_tick, TICKS[0]);
        assert_eq!(in_force.budgets.get("food"), Some(&1.0));
        let second = &turns[1];
        assert!(
            second.plan.is_none()
                && second
                    .observation
                    .as_ref()
                    .is_some_and(|o| o.plan.is_none()),
            "the second tick's observation carries no plan (a Pass lens) and no record"
        );
        assert_eq!(
            second.plan_in_force, first.plan_in_force,
            "a tick with neither inherits the last record"
        );
        assert_eq!(
            second.alarms_in_force.len(),
            1,
            "the alarm raised this tick is the one in force"
        );
        assert_eq!(second.alarms_in_force[0].since_tick, TICKS[1]);
        let third = &turns[2];
        assert_eq!(third.plan_in_force, first.plan_in_force);
        assert!(
            third.alarms_in_force.is_empty(),
            "the third tick's observation says no alarm is pending"
        );

        let none = join(vec![a_row(TICKS[0])], Vec::new(), Vec::new());
        assert!(
            none[0].plan_in_force.is_none(),
            "a run with no plan anywhere has no orchestrator"
        );
    }

    /// The tab set is the log's, sorted and deduplicated — a new specialist appears with no
    /// template change.
    #[test]
    fn the_specialist_tab_set_is_derived_from_the_decision_log() {
        let mut land = a_decision(TICKS[1], Outcome::Accepted);
        if let DecisionRecord::Decision(decision) = &mut land {
            decision.specialist = "land".into();
            decision.intent = "land:move:1".into();
        }
        let records = vec![
            a_decision(TICKS[0], Outcome::Accepted),
            land,
            a_decision(TICKS[2], Outcome::Accepted),
        ];
        let turns = join(Vec::new(), records, Vec::new());
        assert_eq!(specialists_of(&turns), vec!["food", "land"]);
        assert!(specialists_of(&[]).is_empty());
    }

    #[test]
    fn a_tick_only_the_decision_log_names_still_gets_a_turn() {
        let turns = join(
            vec![a_row(TICKS[0])],
            vec![DecisionRecord::Ready(ReadyRecord { tick: TICKS[2] })],
            Vec::new(),
        );
        assert_eq!(
            turns.iter().map(|turn| turn.tick).collect::<Vec<_>>(),
            vec![TICKS[0], TICKS[2]]
        );
        assert!(turns[1].score.is_none());
        assert!(turns[1].ready);
    }

    #[test]
    fn the_page_inlines_the_model_once_and_fetches_nothing() {
        let (rows, records, observations) = three_sets();
        let turns = join(rows, records, observations);
        let model = RunModel {
            seed: "11".into(),
            faction: FACTION,
            seat_dir: "seat_1".into(),
            specialists: specialists_of(&turns),
            turns,
        };
        let page = render_page(&model).expect("renders");
        assert!(!page.contains(MODEL_MARKER), "the marker was replaced");
        assert!(
            !page.contains("</script>\"") && page.contains("\\u003c/script\\u003e"),
            "a reason holding `</script>` cannot end the model element early"
        );
        assert_self_contained(&page).expect("no external resource");
        let start = page
            .find("<script id=\"run-model\"")
            .expect("the model element");
        let json_start = page[start..].find('>').expect("the tag closes") + start + 1;
        let json_end = page[json_start..]
            .find("</script>")
            .expect("the element closes")
            + json_start;
        let back: serde_json::Value =
            serde_json::from_str(&page[json_start..json_end]).expect("the inlined model is JSON");
        assert_eq!(back["turns"].as_array().map(Vec::len), Some(TICKS.len()));
        assert_eq!(back["faction"], FACTION);
        assert_eq!(
            back["turns"][0]["decisions"][0]["reason"],
            "idle hands: forage 3,4 </script>"
        );
    }

    #[test]
    fn a_page_with_an_external_resource_is_refused() {
        assert!(matches!(
            assert_self_contained("<script src=\"x.js\"></script>"),
            Err(ViewerError::ExternalResource(_))
        ));
        assert!(matches!(
            assert_self_contained("<a href=\"https://example.invalid\">"),
            Err(ViewerError::ExternalResource(_))
        ));
        assert!(assert_self_contained("<p>fine</p>").is_ok());
    }

    #[test]
    fn a_run_dir_without_observations_still_writes_a_page() {
        let dir = std::env::temp_dir().join(format!("sim_ai_viewer_{}", std::process::id()));
        let seat_dir = dir.join("11").join(format!("{SEAT_DIR_PREFIX}{FACTION}"));
        fs::create_dir_all(&seat_dir).unwrap();
        let (rows, records, _) = three_sets();
        let lines = |values: Vec<String>| values.join("\n") + "\n";
        fs::write(
            seat_dir.join(SCOREBOARD_FILE),
            lines(
                rows.iter()
                    .map(|row| serde_json::to_string(row).unwrap())
                    .collect(),
            ),
        )
        .unwrap();
        fs::write(
            seat_dir.join(DECISIONS_FILE),
            lines(
                records
                    .iter()
                    .map(|record| serde_json::to_string(record).unwrap())
                    .collect(),
            ),
        )
        .unwrap();
        let out = dir.join("page.html");
        run(ViewerArgs {
            run_dir: dir.clone(),
            seed: None,
            seat: None,
            out: out.clone(),
        })
        .expect("the page is written");
        let page = fs::read_to_string(&out).unwrap();
        assert!(page.contains("\"observation\":null"));
        assert!(page.contains("\"seed\":\"11\""));
        assert_self_contained(&page).unwrap();
        // A directory holding `seat_<f>` directly is a launcher run directory, labelled by name.
        run(ViewerArgs {
            run_dir: dir.join("11"),
            seed: None,
            seat: None,
            out: out.clone(),
        })
        .expect("a seat directory's parent is a run directory");
        let page = fs::read_to_string(&out).unwrap();
        assert!(page.contains("\"seed\":\"11\""));
        // A directory with neither seeds nor seats is nothing the viewer knows.
        let empty = dir.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(matches!(
            run(ViewerArgs {
                run_dir: empty,
                seed: None,
                seat: None,
                out: out.clone(),
            }),
            Err(ViewerError::NoSeeds(_))
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    /// A launcher run directory: a rival's seat has logs, the human's has only recorded frames, and
    /// the viewer imports the latter on the fly and labels the page with the recorded seed.
    #[test]
    fn a_launcher_run_dir_imports_a_recorded_seat_that_has_no_logs() {
        use crate::import_record::{
            seat_epoch_frames_dir, CommandRecord, RunInfo, COMMANDS_FILE, FRAME_FILE_EXTENSION,
            RUN_FILE,
        };
        use sim_runtime::encode_snapshot_flatbuffer;
        const HUMAN: u32 = 0;
        const TICK: u64 = 2;
        const EPOCH: u32 = 1;
        let dir = std::env::temp_dir().join(format!("sim_ai_viewer_run_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let record = dir.join(RECORD_DIR);
        let frames = seat_epoch_frames_dir(&record, HUMAN, EPOCH);
        fs::create_dir_all(&frames).unwrap();
        let mut snapshot = sim_runtime::WorldSnapshot::default();
        snapshot.header.tick = TICK;
        snapshot.header.frame_seq = 1;
        snapshot.header.world_epoch = EPOCH;
        fs::write(
            frames.join(format!("1.{FRAME_FILE_EXTENSION}")),
            encode_snapshot_flatbuffer(&snapshot),
        )
        .unwrap();
        let command = CommandRecord {
            tick: TICK,
            faction: Some(HUMAN),
            connection: 1,
            verb: "move_band".into(),
            command: "move_band 0 7001 4 9".into(),
        };
        fs::write(
            record.join(COMMANDS_FILE),
            serde_json::to_string(&command).unwrap() + "\n",
        )
        .unwrap();
        let info = RunInfo {
            map_preset_id: "earthlike".into(),
            width: 56,
            height: 36,
            map_seed: 23,
            start_profile_id: "late_forager_tribe".into(),
            roster: vec![0, 1],
            world_epoch: 1,
        };
        fs::write(record.join(RUN_FILE), serde_json::to_string(&info).unwrap()).unwrap();
        let out = dir.join("human.html");
        run(ViewerArgs {
            run_dir: dir.clone(),
            seed: None,
            seat: Some(HUMAN),
            out: out.clone(),
        })
        .expect("the human's seat is imported and viewed");
        assert!(
            dir.join(format!("{SEAT_DIR_PREFIX}{HUMAN}"))
                .join(SCOREBOARD_FILE)
                .is_file(),
            "the import left a seat log directory behind"
        );
        let page = fs::read_to_string(&out).unwrap();
        assert!(page.contains("\"specialists\":[\"human\"]"));
        assert!(page.contains("(seed 23)"));
        assert!(page.contains("human:move_band"));
        // With no seat named, the lowest seat — logged or recorded — is the default.
        run(ViewerArgs {
            run_dir: dir.clone(),
            seed: None,
            seat: None,
            out: out.clone(),
        })
        .expect("defaults to seat 0");
        assert!(fs::read_to_string(&out).unwrap().contains("\"faction\":0"));
        let _ = fs::remove_dir_all(&dir);
    }
}
