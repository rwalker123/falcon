//! **The run record** — what the server writes so a played game can be viewed afterwards, seat by
//! seat, exactly as a `sim_ai` run is (`docs/plan_ai_driver.md` §8.4; the reader is `sim_ai
//! import-record`).
//!
//! Recording is on only while [`RECORD_DIR_ENV`] names a directory. Under it:
//!
//! ```text
//! <record>/run.json                          the world: preset, size, seed, profile, roster
//! <record>/commands.jsonl                    one line per command the timeline logged
//! <record>/seat_<f>/frames/<frame_seq>.bin   every frame published to seat f, as sent
//! ```
//!
//! A frame is the FlatBuffers envelope exactly as the stream socket writes it (without the socket's
//! `u32` length prefix), so `decode_frame_flatbuffer` reads it back; the file is named by the
//! header's `frame_seq`, which is the order the deltas chain in. A command line is the same text
//! the AI writes into its own decision log (`sim_runtime::render_command_line`), stamped with the
//! tick it was dispatched at and the seat the sending connection held — **never its token**.
//!
//! ⛔ **Nothing here may stall the turn or the publisher.** Every write happens on one dedicated
//! `run-recorder` thread behind an unbounded channel: the publisher thread and the command loop
//! hand it a job and return. A write that fails is a `warn!` and the job is dropped — a record with
//! a hole is still a record, and a disk problem is never a reason for the game to wait.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{unbounded, Sender};
use serde::{Deserialize, Serialize};
use sim_runtime::decode_frame_header;
use tracing::warn;

use crate::orders::FactionId;
use crate::snapshot::FrameSink;

/// The environment variable that switches recording on: the directory the record is written under.
pub const RECORD_DIR_ENV: &str = "SIM_RECORD_DIR";
/// The per-seat directory, `seat_<faction>` — the same prefix the bench and the viewer use for a
/// seat's log directory, so a run directory reads uniformly.
pub const SEAT_DIR_PREFIX: &str = "seat_";
/// Where a seat's frames go, under its seat directory.
pub const FRAMES_DIR: &str = "frames";
/// A frame file's extension; its stem is the frame's `frame_seq`.
pub const FRAME_FILE_EXTENSION: &str = "bin";
/// The command log, one [`CommandRecord`] per line.
pub const COMMANDS_FILE: &str = "commands.jsonl";
/// The world description, one [`RunInfo`], rewritten at every world build.
pub const RUN_FILE: &str = "run.json";
/// The writer thread's name, for a thread listing.
const WRITER_THREAD_NAME: &str = "run-recorder";

/// **The world a record was taken on**, written at the `seats.roster` moment of every build so the
/// record is self-describing: the New Game the player made, in the menu's own terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInfo {
    pub map_preset_id: String,
    pub width: u32,
    pub height: u32,
    /// The seed the world was actually built from (a requested `0` has been resolved by then).
    pub map_seed: u64,
    pub start_profile_id: String,
    /// The faction roster in order, as the `seats.roster` event announces it.
    pub roster: Vec<u32>,
    pub world_epoch: u32,
}

/// **One logged command**: the tick it was dispatched at, the seat the sending connection held
/// (`None` for an unseated connection — the operator channel, or the server's own voice), the
/// connection's opaque id, the line's verb, and the line itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRecord {
    pub tick: u64,
    pub faction: Option<u32>,
    pub connection: u64,
    pub verb: String,
    pub command: String,
}

/// What the writer thread is handed.
enum Job {
    Frame {
        seat: FactionId,
        frame: Arc<Vec<u8>>,
    },
    Command(CommandRecord),
    Run(RunInfo),
    /// A barrier: answered once every job queued ahead of it has been written.
    Sync(Sender<()>),
}

/// The handle the server holds: a channel to the writer thread.
pub struct RunRecorder {
    dir: PathBuf,
    sender: Sender<Job>,
}

impl RunRecorder {
    /// Create `dir` and start the writer thread. The directory must be creatable — a record that
    /// cannot be opened at all is refused here, once, rather than warned about on every frame.
    pub fn open(dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let (sender, receiver) = unbounded::<Job>();
        let mut writer = Writer {
            dir: dir.to_path_buf(),
            commands: None,
        };
        thread::Builder::new()
            .name(WRITER_THREAD_NAME.to_string())
            .spawn(move || {
                for job in receiver {
                    writer.write(job);
                }
            })?;
        Ok(Self {
            dir: dir.to_path_buf(),
            sender,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Queue one published frame for `seat`. Returns at once.
    pub fn record_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>) {
        let _ = self.sender.send(Job::Frame {
            seat,
            frame: Arc::clone(frame),
        });
    }

    /// Queue one command line. Returns at once.
    pub fn record_command(&self, record: CommandRecord) {
        let _ = self.sender.send(Job::Command(record));
    }

    /// Queue the world description. Returns at once.
    pub fn record_run(&self, info: RunInfo) {
        let _ = self.sender.send(Job::Run(info));
    }

    /// Wait until everything queued so far is on disk. For tests and an orderly exit; nothing on
    /// the turn path calls it.
    pub fn flush(&self) {
        let (reply, done) = crossbeam_channel::bounded(1);
        if self.sender.send(Job::Sync(reply)).is_ok() {
            let _ = done.recv();
        }
    }

    /// The path a frame is filed under.
    pub fn frame_path(dir: &Path, seat: FactionId, frame_seq: u64) -> PathBuf {
        seat_frames_dir(dir, seat).join(format!("{frame_seq}.{FRAME_FILE_EXTENSION}"))
    }
}

/// `<record>/seat_<f>/frames`.
pub fn seat_frames_dir(dir: &Path, seat: FactionId) -> PathBuf {
    dir.join(format!("{SEAT_DIR_PREFIX}{}", seat.0))
        .join(FRAMES_DIR)
}

/// The writer thread's state: the record directory and the command log, opened on first use.
struct Writer {
    dir: PathBuf,
    commands: Option<BufWriter<File>>,
}

impl Writer {
    fn write(&mut self, job: Job) {
        match job {
            Job::Frame { seat, frame } => {
                if let Err(err) = self.write_frame(seat, &frame) {
                    warn!(
                        target: "shadow_scale::record",
                        faction = %seat,
                        bytes = frame.len(),
                        %err,
                        "record.frame.failed"
                    );
                }
            }
            Job::Command(record) => {
                if let Err(err) = self.write_command(&record) {
                    warn!(
                        target: "shadow_scale::record",
                        verb = %record.verb,
                        %err,
                        "record.command.failed"
                    );
                }
            }
            Job::Run(info) => {
                if let Err(err) = self.write_run(&info) {
                    warn!(target: "shadow_scale::record", %err, "record.run.failed");
                }
            }
            Job::Sync(reply) => {
                if let Some(commands) = self.commands.as_mut() {
                    let _ = commands.flush();
                }
                let _ = reply.send(());
            }
        }
    }

    fn write_frame(&self, seat: FactionId, frame: &[u8]) -> io::Result<()> {
        let header = decode_frame_header(frame).map_err(io::Error::other)?;
        let dir = seat_frames_dir(&self.dir, seat);
        fs::create_dir_all(&dir)?;
        fs::write(
            RunRecorder::frame_path(&self.dir, seat, header.frame_seq),
            frame,
        )
    }

    fn write_command(&mut self, record: &CommandRecord) -> io::Result<()> {
        if self.commands.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join(COMMANDS_FILE))?;
            self.commands = Some(BufWriter::new(file));
        }
        let writer = self.commands.as_mut().expect("opened above");
        serde_json::to_writer(&mut *writer, record)?;
        writer.write_all(b"\n")?;
        writer.flush()
    }

    fn write_run(&self, info: &RunInfo) -> io::Result<()> {
        let json = serde_json::to_string_pretty(info)?;
        fs::write(self.dir.join(RUN_FILE), json)
    }
}

/// **A sink that records what it delivers.** Wraps the socket sink the publisher normally holds:
/// every frame goes to the socket first, then to the recorder's queue. The seat is the one the
/// frame was captured for, so the record is per seat exactly as delivery is.
pub struct RecordingSink {
    socket: Arc<dyn FrameSink>,
    recorder: Arc<RunRecorder>,
}

impl RecordingSink {
    pub fn new(socket: Arc<dyn FrameSink>, recorder: Arc<RunRecorder>) -> Self {
        Self { socket, recorder }
    }
}

impl FrameSink for RecordingSink {
    fn publish_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>) {
        self.socket.publish_frame(seat, frame);
        self.recorder.record_frame(seat, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_runtime::{
        encode_delta_flatbuffer, encode_snapshot_flatbuffer, WorldDelta, WorldSnapshot,
    };
    use std::sync::Mutex;

    const SEAT: FactionId = FactionId(1);
    const FULL_SEQ: u64 = 3;
    const A_TICK: u64 = 4;

    fn scratch(case: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("shadow_scale_record_{case}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn a_full_frame() -> Arc<Vec<u8>> {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.frame_seq = FULL_SEQ;
        snapshot.header.tick = A_TICK;
        Arc::new(encode_snapshot_flatbuffer(&snapshot))
    }

    fn a_delta_frame() -> Arc<Vec<u8>> {
        let mut delta = WorldDelta::default();
        delta.header.base_frame_seq = FULL_SEQ;
        delta.header.frame_seq = FULL_SEQ + 1;
        delta.header.tick = A_TICK + 1;
        Arc::new(encode_delta_flatbuffer(&delta))
    }

    /// A sink double that counts what reached "the socket".
    struct Counting(Mutex<Vec<(FactionId, usize)>>);

    impl FrameSink for Counting {
        fn publish_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>) {
            self.0.lock().unwrap().push((seat, frame.len()));
        }
    }

    #[test]
    fn frames_are_filed_by_seat_and_frame_seq_exactly_as_sent() {
        let dir = scratch("frames");
        let recorder = RunRecorder::open(&dir).expect("opens");
        let full = a_full_frame();
        let delta = a_delta_frame();
        recorder.record_frame(SEAT, &full);
        recorder.record_frame(SEAT, &delta);
        recorder.flush();
        let on_disk =
            fs::read(RunRecorder::frame_path(&dir, SEAT, FULL_SEQ)).expect("the full frame");
        assert_eq!(on_disk, *full, "the bytes are the envelope, untouched");
        let on_disk =
            fs::read(RunRecorder::frame_path(&dir, SEAT, FULL_SEQ + 1)).expect("the delta");
        assert_eq!(on_disk, *delta);
        assert!(!RunRecorder::frame_path(&dir, FactionId(2), FULL_SEQ).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_recording_sink_delivers_to_the_socket_and_then_records() {
        let dir = scratch("sink");
        let recorder = Arc::new(RunRecorder::open(&dir).expect("opens"));
        let socket = Arc::new(Counting(Mutex::new(Vec::new())));
        let sink = RecordingSink::new(
            Arc::clone(&socket) as Arc<dyn FrameSink>,
            Arc::clone(&recorder),
        );
        let full = a_full_frame();
        sink.publish_frame(SEAT, &full);
        recorder.flush();
        assert_eq!(*socket.0.lock().unwrap(), vec![(SEAT, full.len())]);
        assert!(RunRecorder::frame_path(&dir, SEAT, FULL_SEQ).is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn commands_append_as_json_lines_and_the_run_file_round_trips() {
        let dir = scratch("commands");
        let recorder = RunRecorder::open(&dir).expect("opens");
        let first = CommandRecord {
            tick: A_TICK,
            faction: Some(SEAT.0),
            connection: 7,
            verb: "split_band".into(),
            command: "split_band 1 7001 4".into(),
        };
        let second = CommandRecord {
            tick: A_TICK,
            faction: None,
            connection: 0,
            verb: "Resync".into(),
            command: "Resync".into(),
        };
        recorder.record_command(first.clone());
        recorder.record_command(second.clone());
        let info = RunInfo {
            map_preset_id: "earthlike".into(),
            width: 56,
            height: 36,
            map_seed: 11,
            start_profile_id: "late_forager_tribe".into(),
            roster: vec![0, 1, 2],
            world_epoch: 1,
        };
        recorder.record_run(info.clone());
        recorder.flush();
        let lines: Vec<CommandRecord> = fs::read_to_string(dir.join(COMMANDS_FILE))
            .expect("the command log")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a record per line"))
            .collect();
        assert_eq!(lines, vec![first, second]);
        let back: RunInfo =
            serde_json::from_str(&fs::read_to_string(dir.join(RUN_FILE)).expect("run.json"))
                .expect("parses");
        assert_eq!(back, info);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A frame that is not a frame is dropped with a warning, and the recorder goes on.
    #[test]
    fn a_frame_that_cannot_be_decoded_is_dropped_not_fatal() {
        let dir = scratch("undecodable");
        let recorder = RunRecorder::open(&dir).expect("opens");
        recorder.record_frame(SEAT, &Arc::new(vec![1, 2, 3]));
        recorder.record_frame(SEAT, &a_full_frame());
        recorder.flush();
        assert!(RunRecorder::frame_path(&dir, SEAT, FULL_SEQ).is_file());
        let _ = fs::remove_dir_all(&dir);
    }
}
