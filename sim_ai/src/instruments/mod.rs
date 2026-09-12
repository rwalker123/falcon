//! **The instruments** — the three logs a player process writes so that a layer can be measured on
//! its own (`docs/plan_ai_driver.md` §8.1, §8.4). All are JSON lines under `--log-dir`; without
//! one, nothing here is opened and the process plays unmeasured.
//!
//! - [`scoreboard::ScoreRow`] → `scoreboard.jsonl`: one row per acted tick, read from the
//!   `SeatView` the process already holds. The *whole seat* instrument.
//! - [`decisions::DecisionRecord`] → `decisions.jsonl`: one line per proposal, plan, alarm, ready
//!   submission and link event. The instrument that measures a *part*.
//! - [`observations::Observation`] → `observations.jsonl`: one record per acted tick, what the
//!   brain was looking at when it decided. The instrument the run viewer reads.
//!
//! The bench (`crate::bench`) reads the first two back and computes the measures of §8.2 from
//! them alone — the server never learns the AI exists. The viewer (`crate::viewer`) joins all three.

pub mod decisions;
pub mod observations;
pub mod scoreboard;

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use decisions::{DecisionRecord, DecisionSink};
use observations::ObservationRecord;
use scoreboard::ScoreRow;

/// The three writers, opened together under one directory.
pub struct Instruments {
    scoreboard: BufWriter<File>,
    decisions: BufWriter<File>,
    observations: BufWriter<File>,
}

impl Instruments {
    /// Create `dir` and open (truncating) every log inside it.
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            scoreboard: BufWriter::new(File::create(dir.join(scoreboard::SCOREBOARD_FILE))?),
            decisions: BufWriter::new(File::create(dir.join(decisions::DECISIONS_FILE))?),
            observations: BufWriter::new(File::create(dir.join(observations::OBSERVATIONS_FILE))?),
        })
    }

    /// Append one scoreboard row and flush it, so a killed process leaves every acted tick behind.
    pub fn record_score(&mut self, row: &ScoreRow) -> io::Result<()> {
        write_line(&mut self.scoreboard, row)
    }

    /// Append one observation record and flush it.
    pub fn record_observation(&mut self, record: &ObservationRecord) -> io::Result<()> {
        write_line(&mut self.observations, record)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.scoreboard.flush()?;
        self.decisions.flush()?;
        self.observations.flush()
    }
}

impl DecisionSink for Instruments {
    fn record(&mut self, record: DecisionRecord) {
        if let Err(err) = write_line(&mut self.decisions, &record) {
            tracing::error!(%err, "the decision log could not be written");
        }
    }
}

fn write_line<T: serde::Serialize>(writer: &mut BufWriter<File>, value: &T) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()
}
