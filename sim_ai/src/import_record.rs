//! **`sim_ai import-record`** — a played game's record, turned into the seat log directory the
//! viewer reads, for any seat the server recorded: a rival's, or the human's.
//!
//! `sim_ai import-record <record-dir> --seat <f> --out <log-dir>` reads what the server wrote under
//! `SIM_RECORD_DIR` (`core_sim/src/record.rs`; the layout is restated in the constants below):
//! `seat_<f>/frames/<world_epoch>/<frame_seq>.bin`, every frame published to that seat exactly as
//! sent, and
//! `commands.jsonl`, one line per command the timeline logged. It replays the frames through the
//! same `decode_frame_flatbuffer` + `apply_delta` chain a live seat uses, and for each tick writes
//! the `ScoreRow` and `Observation` a `sim_ai` process would have written off that view — with no
//! brain behind it: `plan` and `alarms` null, the radius `OBSERVATION_RADIUS_FLOOR`. Every command
//! the seat sent that tick becomes one accepted [`Decision`] under the [`HUMAN_SPECIALIST`], and an
//! `order … ready` becomes the tick's [`ReadyRecord`], exactly as the turn loop writes its own.
//!
//! **A tick's state is its LAST frame.** A mid-tick recapture — the frame the server publishes after
//! a world-mutating command — carries the same tick as the turn frame before it, so a tick can
//! have several frames; the view written for it is the one after the last of them, i.e. the
//! world with that tick's commands applied. (A live `sim_ai` observes the *first* frame of a
//! tick, before its own commands: the two pages differ by exactly the seat's own orders.)
//!
//! ⛔ **One record directory holds several worlds, and an import reads exactly one of them.** A
//! launcher session builds a new world on every New Game and every Load, and a seat's `frame_seq`
//! restarts with it — so the record files each world's frames under its own `world_epoch`
//! (`core_sim/src/record.rs`). The importer takes the **latest** epoch present, which is the world
//! `run.json` describes, and says on stderr when it passed over earlier ones rather than replaying
//! two worlds' chains spliced into one run.
//!
//! The output is a seat log directory the existing `viewer` consumes unchanged.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::{Deserialize, Serialize};
use sim_runtime::{decode_frame_flatbuffer, FramePayload, ORDERS_VERB};
use tracing::{info, warn};

use crate::bench::measures::{read_jsonl, MeasureError};
use crate::brain::BrainLens;
use crate::instruments::decisions::{Decision, DecisionRecord, DecisionSink, Outcome, ReadyRecord};
use crate::instruments::observations::{Observation, ObservationRecord};
use crate::instruments::scoreboard::ScoreRow;
use crate::instruments::Instruments;
use crate::view::SeatView;

/// **The record layout**, restated from `core_sim::record` (the writer is the authority; this crate
/// cannot link it): the per-seat directory prefix, the frames directory under it (whose
/// subdirectories are `world_epoch`s), a frame file's extension (its stem is the `frame_seq`), the
/// command log and the world description.
pub const SEAT_DIR_PREFIX: &str = "seat_";
pub const FRAMES_DIR: &str = "frames";
pub const FRAME_FILE_EXTENSION: &str = "bin";
pub const COMMANDS_FILE: &str = "commands.jsonl";
pub const RUN_FILE: &str = "run.json";

/// The specialist an imported seat's commands are filed under, and the score they carry — a
/// human's order is accepted by definition, so both scores are the scripted brain's `SCRIPT_SCORE`.
pub const HUMAN_SPECIALIST: &str = "human";
pub const HUMAN_SCORE: f32 = 1.0;
/// The intent's separator, the same one every specialist's key uses (`specialists::INTENT_SEPARATOR`).
const INTENT_SEPARATOR: char = ':';

/// One line of the record's `commands.jsonl` — contract twin of `core_sim::record::CommandRecord`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRecord {
    pub tick: u64,
    pub faction: Option<u32>,
    pub connection: u64,
    pub verb: String,
    pub command: String,
}

/// The record's `run.json` — contract twin of `core_sim::record::RunInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInfo {
    pub map_preset_id: String,
    pub width: u32,
    pub height: u32,
    pub map_seed: u64,
    pub start_profile_id: String,
    pub roster: Vec<u32>,
    pub world_epoch: u32,
}

#[derive(Parser, Debug)]
#[command(
    name = "sim_ai import-record",
    about = "Turn a server run record into the seat log directory the viewer reads."
)]
pub struct ImportArgs {
    /// The server's record directory (`SIM_RECORD_DIR`).
    pub record_dir: PathBuf,
    /// The seat to import: `seat_<f>/frames` must exist under the record.
    #[arg(long)]
    pub seat: u32,
    /// The seat log directory to write (`scoreboard.jsonl`, `decisions.jsonl`, `observations.jsonl`).
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("no frames for seat {seat} under {0}: {SEAT_DIR_PREFIX}{seat}/{FRAMES_DIR}/<world_epoch> is missing or empty", seat = .1)]
    NoFrames(PathBuf, u32),
    #[error("io at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Log(#[from] MeasureError),
}

/// What an import produced, for the log line and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    /// Ticks with a score row and an observation.
    pub ticks: usize,
    /// Accepted `human` decisions written.
    pub decisions: usize,
    /// `ready` records written.
    pub ready: usize,
    /// Frames that could not be applied (undecodable, or a delta off a broken chain).
    pub frames_dropped: usize,
}

pub fn run(args: ImportArgs) -> Result<(), ImportError> {
    let summary = import(&args.record_dir, args.seat, &args.out)?;
    info!(
        record = %args.record_dir.display(),
        seat = args.seat,
        out = %args.out.display(),
        ticks = summary.ticks,
        decisions = summary.decisions,
        ready = summary.ready,
        frames_dropped = summary.frames_dropped,
        "record imported"
    );
    Ok(())
}

fn io_at(path: &Path, source: io::Error) -> ImportError {
    ImportError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// `<record>/seat_<f>/frames`.
pub fn seat_frames_dir(record_dir: &Path, seat: u32) -> PathBuf {
    record_dir
        .join(format!("{SEAT_DIR_PREFIX}{seat}"))
        .join(FRAMES_DIR)
}

/// `<record>/seat_<f>/frames/<world_epoch>` — one world's chain for that seat.
pub fn seat_epoch_frames_dir(record_dir: &Path, seat: u32, world_epoch: u32) -> PathBuf {
    seat_frames_dir(record_dir, seat).join(world_epoch.to_string())
}

/// Every world the record holds frames of for this seat, ascending. A directory whose name is not
/// a number is not one of ours and is ignored.
pub fn recorded_epochs(record_dir: &Path, seat: u32) -> Vec<u32> {
    let Ok(entries) = fs::read_dir(seat_frames_dir(record_dir, seat)) else {
        return Vec::new();
    };
    let mut epochs: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .path()
                .is_dir()
                .then(|| entry.file_name().to_str()?.parse().ok())
                .flatten()
        })
        .collect();
    epochs.sort_unstable();
    epochs
}

/// Every seat the record holds frames for, ascending.
pub fn recorded_seats(record_dir: &Path) -> Vec<u32> {
    let Ok(entries) = fs::read_dir(record_dir) else {
        return Vec::new();
    };
    let mut seats: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let seat: u32 = name.to_str()?.strip_prefix(SEAT_DIR_PREFIX)?.parse().ok()?;
            entry.path().join(FRAMES_DIR).is_dir().then_some(seat)
        })
        .collect();
    seats.sort_unstable();
    seats
}

/// The record's `run.json`, when it is there and parses.
pub fn read_run_info(record_dir: &Path) -> Option<RunInfo> {
    let text = fs::read_to_string(record_dir.join(RUN_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The seat's frame files in `frame_seq` order, from **one** world: the latest epoch the record
/// holds for it (the module doc). Earlier worlds are named on stderr and left alone — their
/// `frame_seq`s count from the same origin, so replaying them together would splice two worlds
/// into one run.
fn frame_files(record_dir: &Path, seat: u32) -> Result<Vec<(u64, PathBuf)>, ImportError> {
    let epochs = recorded_epochs(record_dir, seat);
    let Some((&world_epoch, earlier)) = epochs.split_last() else {
        return Err(ImportError::NoFrames(record_dir.to_path_buf(), seat));
    };
    if !earlier.is_empty() {
        warn!(
            seat,
            world_epoch,
            skipped = ?earlier,
            "the record holds more than one world for this seat; importing the latest only"
        );
    }
    let dir = seat_epoch_frames_dir(record_dir, seat, world_epoch);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Err(ImportError::NoFrames(record_dir.to_path_buf(), seat)),
    };
    let mut frames: Vec<(u64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()?.to_str()? != FRAME_FILE_EXTENSION {
                return None;
            }
            let seq: u64 = path.file_stem()?.to_str()?.parse().ok()?;
            Some((seq, path))
        })
        .collect();
    if frames.is_empty() {
        return Err(ImportError::NoFrames(record_dir.to_path_buf(), seat));
    }
    frames.sort_unstable_by_key(|(seq, _)| *seq);
    Ok(frames)
}

/// Import `seat` from `record_dir` into the seat log directory `out`.
pub fn import(record_dir: &Path, seat: u32, out: &Path) -> Result<ImportSummary, ImportError> {
    let frames = frame_files(record_dir, seat)?;
    let commands_path = record_dir.join(COMMANDS_FILE);
    let commands: Vec<CommandRecord> = if commands_path.is_file() {
        read_jsonl(&commands_path)?
    } else {
        Vec::new()
    };

    // Replay the chain. A tick's row is captured off the view the moment a frame of another tick
    // arrives (the last frame of the tick has been applied by then), and once more at the end.
    let mut view: Option<SeatView> = None;
    let mut per_tick: BTreeMap<u64, (ScoreRow, Observation)> = BTreeMap::new();
    let mut frames_dropped = 0;
    let lens = BrainLens::default();
    for (seq, path) in frames {
        let bytes = fs::read(&path).map_err(|source| io_at(&path, source))?;
        let payload = match decode_frame_flatbuffer(&bytes) {
            Ok(payload) => payload,
            Err(err) => {
                warn!(frame_seq = seq, %err, "frame dropped: undecodable");
                frames_dropped += 1;
                continue;
            }
        };
        let incoming_tick = match &payload {
            FramePayload::Snapshot(snapshot) => snapshot.header.tick,
            FramePayload::Delta(delta) => delta.header.tick,
        };
        if let Some(held) = view.as_ref() {
            if held.tick() != incoming_tick {
                capture_tick(&mut per_tick, held, seat, &lens);
            }
        }
        match payload {
            FramePayload::Snapshot(snapshot) => {
                view = Some(SeatView {
                    snapshot,
                    last_acted_tick: None,
                });
            }
            FramePayload::Delta(delta) => {
                let Some(held) = view.as_mut() else {
                    warn!(
                        frame_seq = seq,
                        "frame dropped: a delta before any full frame"
                    );
                    frames_dropped += 1;
                    continue;
                };
                if let Err(err) = held.snapshot.apply_delta(&delta) {
                    warn!(frame_seq = seq, %err, "frame dropped: the chain is broken until the next full frame");
                    frames_dropped += 1;
                    view = None;
                    continue;
                }
            }
        }
    }
    if let Some(held) = view.as_ref() {
        capture_tick(&mut per_tick, held, seat, &lens);
    }

    let mut instruments = Instruments::open(out).map_err(|source| io_at(out, source))?;
    for (row, observation) in per_tick.values() {
        instruments
            .record_score(row)
            .map_err(|source| io_at(out, source))?;
        instruments
            .record_observation(&ObservationRecord::Observation(observation.clone()))
            .map_err(|source| io_at(out, source))?;
    }
    let mut decisions = 0;
    let mut ready = 0;
    for command in commands.iter().filter(|c| c.faction == Some(seat)) {
        if command.verb == ORDERS_VERB {
            instruments.record(DecisionRecord::Ready(ReadyRecord { tick: command.tick }));
            ready += 1;
        } else {
            instruments.record(DecisionRecord::Decision(human_decision(command)));
            decisions += 1;
        }
    }
    instruments.flush().map_err(|source| io_at(out, source))?;
    Ok(ImportSummary {
        ticks: per_tick.len(),
        decisions,
        ready,
        frames_dropped,
    })
}

/// The tick's row and observation off `view`, replacing an earlier capture of the same tick.
fn capture_tick(
    per_tick: &mut BTreeMap<u64, (ScoreRow, Observation)>,
    view: &SeatView,
    seat: u32,
    lens: &BrainLens<'_>,
) {
    let row = ScoreRow::from_snapshot(&view.snapshot, seat);
    let observation = Observation::capture(view, &row, lens);
    per_tick.insert(view.tick(), (row, observation));
}

/// One command the seat sent, as the accepted decision the viewer lists it under.
pub fn human_decision(command: &CommandRecord) -> Decision {
    Decision {
        tick: command.tick,
        specialist: HUMAN_SPECIALIST.to_owned(),
        intent: format!("{HUMAN_SPECIALIST}{INTENT_SEPARATOR}{}", command.verb),
        score_raw: HUMAN_SCORE,
        score_final: HUMAN_SCORE,
        outcome: Outcome::Accepted,
        reason: format!("sent on connection {}", command.connection),
        commands: 1,
        commands_text: vec![command.command.clone()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruments::decisions::DECISIONS_FILE;
    use crate::instruments::observations::{OBSERVATIONS_FILE, OBSERVATION_RADIUS_FLOOR};
    use crate::instruments::scoreboard::SCOREBOARD_FILE;
    use sim_runtime::{
        encode_delta_flatbuffer, encode_snapshot_flatbuffer, WorldDelta, WorldSnapshot,
    };

    const SEAT: u32 = 1;
    const OTHER_SEAT: u32 = 2;
    const FULL_SEQ: u64 = 10;
    const FIRST_TICK: u64 = 4;
    const EPOCH: u32 = 1;
    const NEXT_EPOCH: u32 = EPOCH + 1;

    fn scratch(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sim_ai_import_{case}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_frame(record: &Path, seat: u32, seq: u64, bytes: &[u8]) {
        write_frame_of(record, seat, EPOCH, seq, bytes);
    }

    fn write_frame_of(record: &Path, seat: u32, world_epoch: u32, seq: u64, bytes: &[u8]) {
        let dir = seat_epoch_frames_dir(record, seat, world_epoch);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{seq}.{FRAME_FILE_EXTENSION}")), bytes).unwrap();
    }

    fn full_frame(tick: u64) -> Vec<u8> {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.frame_seq = FULL_SEQ;
        snapshot.header.tick = tick;
        encode_snapshot_flatbuffer(&snapshot)
    }

    fn delta_frame(base: u64, tick: u64) -> Vec<u8> {
        let mut delta = WorldDelta::default();
        delta.header.base_frame_seq = base;
        delta.header.frame_seq = base + 1;
        delta.header.tick = tick;
        encode_delta_flatbuffer(&delta)
    }

    fn a_command(tick: u64, faction: Option<u32>, verb: &str, line: &str) -> CommandRecord {
        CommandRecord {
            tick,
            faction,
            connection: 3,
            verb: verb.into(),
            command: line.into(),
        }
    }

    /// A hand-built record: a full frame at tick 4, a delta to tick 5, and three commands — a
    /// split and a ready from the seat at tick 4, and a rival's order at tick 5 that is not ours.
    fn a_record(dir: &Path) -> PathBuf {
        let record = dir.join("record");
        write_frame(&record, SEAT, FULL_SEQ, &full_frame(FIRST_TICK));
        write_frame(
            &record,
            SEAT,
            FULL_SEQ + 1,
            &delta_frame(FULL_SEQ, FIRST_TICK + 1),
        );
        let commands = [
            a_command(FIRST_TICK, Some(SEAT), "split_band", "split_band 1 7001 4"),
            a_command(FIRST_TICK, Some(SEAT), ORDERS_VERB, "order 1 ready"),
            a_command(
                FIRST_TICK + 1,
                Some(OTHER_SEAT),
                ORDERS_VERB,
                "order 2 ready",
            ),
        ];
        let lines: Vec<String> = commands
            .iter()
            .map(|c| serde_json::to_string(c).unwrap())
            .collect();
        fs::write(record.join(COMMANDS_FILE), lines.join("\n") + "\n").unwrap();
        record
    }

    #[test]
    fn a_hand_built_record_yields_one_row_per_tick_and_the_seats_commands() {
        let dir = scratch("import");
        let record = a_record(&dir);
        let out = dir.join("seat_1");
        let summary = import(&record, SEAT, &out).expect("imports");
        assert_eq!(
            summary,
            ImportSummary {
                ticks: 2,
                decisions: 1,
                ready: 1,
                frames_dropped: 0
            }
        );
        let rows: Vec<ScoreRow> = read_jsonl(&out.join(SCOREBOARD_FILE)).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.tick).collect::<Vec<_>>(),
            vec![FIRST_TICK, FIRST_TICK + 1]
        );
        assert!(rows.iter().all(|r| r.faction == SEAT));
        let observations: Vec<ObservationRecord> =
            read_jsonl(&out.join(OBSERVATIONS_FILE)).unwrap();
        assert_eq!(observations.len(), 2);
        let ObservationRecord::Observation(first) = &observations[0];
        assert_eq!(first.tick, FIRST_TICK);
        assert!(
            first.plan.is_none() && first.alarms.is_empty(),
            "no brain lens"
        );
        assert_eq!(first.radius, OBSERVATION_RADIUS_FLOOR);
        let records: Vec<DecisionRecord> = read_jsonl(&out.join(DECISIONS_FILE)).unwrap();
        assert_eq!(records.len(), 2, "the rival's order is not this seat's");
        match &records[0] {
            DecisionRecord::Decision(d) => {
                assert_eq!(d.tick, FIRST_TICK);
                assert_eq!(d.specialist, HUMAN_SPECIALIST);
                assert_eq!(d.intent, "human:split_band");
                assert_eq!(d.outcome, Outcome::Accepted);
                assert_eq!(d.score_raw, HUMAN_SCORE);
                assert_eq!(d.commands_text, vec!["split_band 1 7001 4"]);
            }
            other => panic!("expected the split decision, got {other:?}"),
        }
        assert_eq!(
            records[1],
            DecisionRecord::Ready(ReadyRecord { tick: FIRST_TICK })
        );
        assert_eq!(recorded_seats(&record), vec![SEAT]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_seat_with_no_frames_is_refused_by_name() {
        let dir = scratch("noframes");
        let record = a_record(&dir);
        let err = import(&record, OTHER_SEAT, &dir.join("seat_2")).unwrap_err();
        assert!(matches!(err, ImportError::NoFrames(_, OTHER_SEAT)), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A delta off a broken chain is dropped and the replay resumes at the next full frame; the
    /// tick with several frames keeps its last.
    #[test]
    fn a_broken_chain_drops_the_delta_and_the_last_frame_of_a_tick_wins() {
        let dir = scratch("chain");
        let record = dir.join("record");
        write_frame(&record, SEAT, FULL_SEQ, &full_frame(FIRST_TICK));
        // A delta naming a base that was never applied.
        write_frame(
            &record,
            SEAT,
            FULL_SEQ + 1,
            &delta_frame(FULL_SEQ + 5, FIRST_TICK + 1),
        );
        // A fresh full frame at the same tick as the first, then a recapture-shaped delta on it.
        let mut later = WorldSnapshot::default();
        later.header.frame_seq = FULL_SEQ + 2;
        later.header.tick = FIRST_TICK;
        write_frame(
            &record,
            SEAT,
            FULL_SEQ + 2,
            &encode_snapshot_flatbuffer(&later),
        );
        write_frame(
            &record,
            SEAT,
            FULL_SEQ + 3,
            &delta_frame(FULL_SEQ + 2, FIRST_TICK),
        );
        let summary = import(&record, SEAT, &dir.join("seat_1")).expect("imports");
        assert_eq!(summary.frames_dropped, 1);
        assert_eq!(summary.ticks, 1, "three frames of one tick are one row");
        let _ = fs::remove_dir_all(&dir);
    }

    /// ⛔ **Two worlds in one record are not one run.** Both chains start at the same `frame_seq`,
    /// so the importer must read one epoch — the latest, which is the world `run.json` describes —
    /// rather than every `.bin` under the seat.
    #[test]
    fn only_the_latest_worlds_frames_are_replayed() {
        let dir = scratch("epochs");
        let record = dir.join("record");
        write_frame_of(&record, SEAT, EPOCH, FULL_SEQ, &full_frame(FIRST_TICK));
        write_frame_of(
            &record,
            SEAT,
            EPOCH,
            FULL_SEQ + 1,
            &delta_frame(FULL_SEQ, FIRST_TICK + 1),
        );
        let mut rebuilt = WorldSnapshot::default();
        rebuilt.header.frame_seq = FULL_SEQ;
        rebuilt.header.tick = FIRST_TICK;
        rebuilt.header.world_epoch = NEXT_EPOCH;
        write_frame_of(
            &record,
            SEAT,
            NEXT_EPOCH,
            FULL_SEQ,
            &encode_snapshot_flatbuffer(&rebuilt),
        );
        assert_eq!(recorded_epochs(&record, SEAT), vec![EPOCH, NEXT_EPOCH]);
        let summary = import(&record, SEAT, &dir.join("seat_1")).expect("imports");
        assert_eq!(
            summary.ticks, 1,
            "the second world's one frame is the whole run"
        );
        assert_eq!(summary.frames_dropped, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_json_round_trips_through_the_twin() {
        let dir = scratch("runjson");
        let info = RunInfo {
            map_preset_id: "earthlike".into(),
            width: 56,
            height: 36,
            map_seed: 11,
            start_profile_id: "late_forager_tribe".into(),
            roster: vec![0, 1, 2],
            world_epoch: 1,
        };
        fs::write(dir.join(RUN_FILE), serde_json::to_string(&info).unwrap()).unwrap();
        assert_eq!(read_run_info(&dir), Some(info));
        assert_eq!(read_run_info(&dir.join("nowhere")), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
