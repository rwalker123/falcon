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
use crate::instruments::decisions::{
    AlarmRecord, Decision, DecisionRecord, LinkRecord, PlanRecord, DECISIONS_FILE,
};
use crate::instruments::observations::{Observation, ObservationRecord, OBSERVATIONS_FILE};
use crate::instruments::scoreboard::{ScoreRow, SCOREBOARD_FILE};

/// The page, with the model marker in it.
const PAGE_TEMPLATE: &str = include_str!("page.html");
/// Where the template takes the model: replaced, once, by the JSON of a [`RunModel`].
const MODEL_MARKER: &str = "__RUN_MODEL_JSON__";
/// The seat directory prefix the bench writes (`bench::SEAT_DIR_PREFIX`, restated as the layout
/// this reader expects).
const SEAT_DIR_PREFIX: &str = "seat_";
/// What a self-contained page must never contain: a fetch of anything by URL.
const EXTERNAL_RESOURCE_MARKERS: [&str; 3] = ["http://", "https://", "src="];

#[derive(Parser, Debug)]
#[command(
    name = "sim_ai viewer",
    about = "Join one seat's bench logs by tick into a self-contained HTML page."
)]
pub struct ViewerArgs {
    /// A bench `--out` directory: `<run-dir>/<seed>/seat_<f>/` holds the logs.
    pub run_dir: PathBuf,
    /// The seed to read (default: the lowest seed directory found).
    #[arg(long)]
    pub seed: Option<String>,
    /// The seat's faction to read (default: the lowest `seat_<f>` found under the seed).
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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunModel {
    pub seed: String,
    pub faction: u32,
    pub seat_dir: String,
    pub turns: Vec<Turn>,
}

/// Run the viewer: locate the seat, join its logs, write the page.
pub fn run(args: ViewerArgs) -> Result<(), ViewerError> {
    let (seed, seed_dir) = match &args.seed {
        Some(seed) => (seed.clone(), args.run_dir.join(seed)),
        None => first_seed_dir(&args.run_dir)?,
    };
    let faction = match args.seat {
        Some(faction) => faction,
        None => first_seat(&seed_dir)?,
    };
    let seat_dir = seed_dir.join(format!("{SEAT_DIR_PREFIX}{faction}"));
    let model = read_model(&seat_dir, seed, faction)?;
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
    Ok(RunModel {
        seed,
        faction,
        seat_dir: seat_dir.display().to_string(),
        turns: join(rows, records, observations),
    })
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
    turns.into_values().collect()
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
        let model = RunModel {
            seed: "11".into(),
            faction: FACTION,
            seat_dir: "seat_1".into(),
            turns: join(rows, records, observations),
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
        assert!(matches!(
            run(ViewerArgs {
                run_dir: dir.join("11"),
                seed: None,
                seat: None,
                out: out.clone(),
            }),
            Err(ViewerError::NoSeeds(_))
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
