//! **The seated-connection harness** shared by the tests that drive a built `server` and a built
//! `sim_ai` over the real sockets — `ai_seat_scenario.rs` and `ai_record_import.rs`.
//!
//! One small world (the same `query_seat_gate.rs` drives), a server started on a caller-chosen
//! port block with the shipped config's separation shrunk so a rival is seated, and a [`Link`]
//! that claims a seat, greets the stream with the token and reads a full frame — the shipped
//! client's own handshake, so the world is read through the seat exactly as a player reads it.

use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use core_sim::network::SEAT_TOKEN_BYTES;
use core_sim::{apply_port_base, SimulationConfig};
use sim_runtime::commands::{QueryPayload, SeatClaimReply};
use sim_runtime::{
    decode_frame_flatbuffer, CommandEnvelope, CommandPayload, FramePayload, QueryReply,
    QueryReplyEnvelope, WorldSnapshot,
};

use super::ai_process::{log_tail, Process, Scratch, LOG_TAIL_LINES};

// =================================================================================================
// The world
// =================================================================================================

/// The same small world `query_seat_gate.rs` drives, for the same reasons.
pub const MAP_WIDTH: u32 = 24;
pub const MAP_HEIGHT: u32 = 16;
pub const MAP_PRESET: &str = "earthlike";
pub const START_PROFILE: &str = "late_forager_tribe";
/// Fixed and non-zero: `seed == 0` asks the server to randomise.
pub const MAP_SEED: u64 = 11;
/// One rival, so there is a seat for the AI to fill.
pub const ONE_RIVAL: u32 = 1;
/// The shipped separation seats one faction on a map this small; shrunk so a rival is seated.
pub const TEST_START_SEPARATION: u32 = 6;
pub const RIVAL_SEAT: u32 = 1;

/// **The scripted order both tests fire: `split_band`, at the founding floor.** A fresh
/// `late_forager_tribe` band is 30 people, and `expedition_config.json`'s `settle` floors are
/// `min_founding_workers 4` / `parent_min_workers 6`, so asking for exactly the founding floor is
/// legal for a starting band and leaves the parent well above its own floor.
pub const SPLIT_WORKERS: u32 = 4;
/// The script: fire on the first tick the brain sees, whatever number it is.
pub const SCRIPT: &str = "# the scenario's one order\n+0: split_band {faction} {own_band:0} 4\n";

// =================================================================================================
// Ports, timeouts and logs
// =================================================================================================

const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(60);
const PORTS_FILE_POLL: Duration = Duration::from_millis(25);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the AI may take to play its turns and exit. Three turns on a small map are seconds;
/// this covers a cold debug build.
const AI_EXIT_TIMEOUT: Duration = Duration::from_secs(120);
const AI_EXIT_POLL: Duration = Duration::from_millis(50);
/// A released seat is freed when the server's read loop sees the socket close; a claim that races
/// it is refused `seat_occupied` and retried, the client's own rule.
const SEAT_CLAIM_ATTEMPTS: u32 = 8;
const SEAT_CLAIM_RETRY_BACKOFF: Duration = Duration::from_millis(250);
/// A full snapshot on a small map is well under this.
const MAX_SNAPSHOT_FRAME_BYTES: usize = 64 * 1024 * 1024;
/// Frames read past while waiting for a full one, before giving up.
const MAX_FRAMES_AWAITED: usize = 32;
const SERVER_LOG_FILTER: &str = "info";
const AI_LOG_FILTER: &str = "info";
const SYNC_QUERY_ID: u64 = 1;
/// The record directory's environment variable (`core_sim::record::RECORD_DIR_ENV`).
const ENV_RECORD_DIR: &str = "SIM_RECORD_DIR";

// =================================================================================================
// The server
// =================================================================================================

pub struct Ports {
    pub command: SocketAddr,
    pub stream: SocketAddr,
}

/// A built server on its own scratch directory. Fields drop in order: the process is killed
/// before the scratch directory it wrote into is removed.
pub struct Server {
    pub process: Process,
    pub scratch: Scratch,
    pub log_path: PathBuf,
    pub ports_path: PathBuf,
    pub ports: Ports,
}

/// Start the built server for `case` on `port_base`, recording under `record_dir` when given.
pub fn start_server(case: &str, port_base: u16, record_dir: Option<&Path>) -> Server {
    let scratch = Scratch::new(case);
    let saves = scratch.dir.join("saves");
    fs::create_dir_all(&saves).expect("scratch save directory");
    let config_path = write_test_config(&scratch.dir, port_base);
    let ports_path = scratch.dir.join("ports.json");
    let log_path = scratch.dir.join("server.log");

    let log = fs::File::create(&log_path).expect("server log file");
    let log_err = log.try_clone().expect("server log file (stderr half)");
    let mut command = Command::new(env!("CARGO_BIN_EXE_server"));
    command
        .current_dir(&scratch.dir)
        .env("SIM_SAVE_DIR", &saves)
        .env("SIM_CONFIG_PATH", &config_path)
        .env("SIM_PORTS_FILE", &ports_path)
        .env("RUST_LOG", SERVER_LOG_FILTER)
        .env_remove("SIM_PORT_BASE")
        .env_remove(ENV_RECORD_DIR)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    if let Some(record_dir) = record_dir {
        command.env(ENV_RECORD_DIR, record_dir);
    }
    let child = command.spawn().expect("the built server binary starts");
    let mut process = Process { child };
    let ports = await_ports_file(&mut process, &ports_path, &log_path);
    Server {
        process,
        scratch,
        log_path,
        ports_path,
        ports,
    }
}

/// The shipped config, its port block moved, its separation shrunk, its seed and rival count
/// pinned.
fn write_test_config(dir: &Path, port_base: u16) -> PathBuf {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/data/simulation_config.json");
    let raw = fs::read_to_string(&shipped).expect("the shipped simulation config reads");
    let mut config = SimulationConfig::from_file(&shipped).expect("the shipped config parses");
    assert!(apply_port_base(&mut config, port_base));
    let mut json: serde_json::Value = serde_json::from_str(&raw).expect("the config is JSON");
    for (key, addr) in [
        ("port_base_bind", config.port_base_bind),
        ("command_bind", config.command_bind),
        ("snapshot_flat_bind", config.snapshot_flat_bind),
        ("log_bind", config.log_bind),
    ] {
        json[key] = serde_json::Value::String(addr.to_string());
    }
    json["faction_start_min_separation"] =
        serde_json::Value::from(u64::from(TEST_START_SEPARATION));
    json["map_seed"] = serde_json::Value::from(MAP_SEED);
    json["default_ai_faction_count"] = serde_json::Value::from(u64::from(ONE_RIVAL));
    let path = dir.join("simulation_config.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).expect("serialises"),
    )
    .expect("writes");
    path
}

fn await_ports_file(process: &mut Process, ports_path: &Path, log_path: &Path) -> Ports {
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    let pid = process.child.id();
    loop {
        if let Ok(Some(status)) = process.child.try_wait() {
            panic!(
                "the server exited ({status}) before publishing its ports file\n{}",
                log_tail(log_path)
            );
        }
        if let Some(ports) = read_ports_file(ports_path, pid) {
            return ports;
        }
        if Instant::now() >= deadline {
            panic!(
                "the server never published its ports file\n{}",
                log_tail(log_path)
            );
        }
        std::thread::sleep(PORTS_FILE_POLL);
    }
}

fn read_ports_file(path: &Path, pid: u32) -> Option<Ports> {
    let raw = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if json.get("pid")?.as_u64()? != u64::from(pid) {
        return None;
    }
    let host = json.get("host")?.as_str()?.parse().ok()?;
    let port = |key: &str| u16::try_from(json.get(key)?.as_u64()?).ok();
    Some(Ports {
        command: SocketAddr::new(host, port("command")?),
        stream: SocketAddr::new(host, port("snapshot_flat")?),
    })
}

/// The `new_game` both tests ask for.
pub fn new_game() -> CommandPayload {
    CommandPayload::NewGame {
        preset_id: MAP_PRESET.to_string(),
        width: MAP_WIDTH,
        height: MAP_HEIGHT,
        seed: MAP_SEED,
        profile_id: START_PROFILE.to_string(),
        ai_faction_count: Some(ONE_RIVAL),
    }
}

/// Ask for the world on an unseated connection (`new_game` names no faction and is not a host
/// verb) and synchronise on a question answered in order behind it.
pub fn build_world(server: &Server) {
    let mut builder = Link::open(server.ports.command, &server.log_path);
    builder.write(new_game(), "the world under test");
    match builder.ask_list_saves() {
        QueryReply::ListSaves(_) => {}
        other => builder.fail(&format!("the sync question was answered with {other:?}")),
    }
}

/// The rival's RESIDENT bands: a split makes a band, and a detached party would flatter the count.
pub fn rival_band_count(snapshot: &WorldSnapshot) -> usize {
    snapshot
        .populations
        .iter()
        .filter(|cohort| cohort.faction == RIVAL_SEAT && !cohort.is_expedition)
        .count()
}

// =================================================================================================
// The AI
// =================================================================================================

/// What a scripted `sim_ai` run left behind.
pub struct AiRun {
    pub status: ExitStatus,
    pub log_path: PathBuf,
}

/// Run the built `sim_ai` as a scripted seat on `faction` for `turns`, with `--log-dir` when
/// given, and wait for it to exit (or fail with both logs).
pub fn run_scripted_sim_ai(
    sim_ai: &Path,
    server: &Server,
    faction: u32,
    script: &str,
    turns: u64,
    log_dir: Option<&Path>,
) -> AiRun {
    let script_path = server.scratch.dir.join(format!("seat_{faction}.script"));
    fs::write(&script_path, script).expect("the script writes");
    let script_arg = script_path.to_string_lossy().into_owned();
    run_sim_ai(
        sim_ai,
        server,
        faction,
        &["--brain", "scripted", "--script", &script_arg],
        turns,
        log_dir,
    )
}

/// A built `sim_ai` playing seat `faction` with the utility brain at `profile` / `difficulty`
/// for `turns` turns, its instruments under `log_dir`.
pub fn run_utility_sim_ai(
    sim_ai: &Path,
    server: &Server,
    faction: u32,
    profile: &str,
    difficulty: &str,
    turns: u64,
    log_dir: Option<&Path>,
) -> AiRun {
    run_sim_ai(
        sim_ai,
        server,
        faction,
        &[
            "--brain",
            "utility",
            "--profile",
            profile,
            "--difficulty",
            difficulty,
        ],
        turns,
        log_dir,
    )
}

/// A built `sim_ai` on seat `faction` with `brain_args` for `turns` turns, waited for and asserted
/// to exit cleanly.
fn run_sim_ai(
    sim_ai: &Path,
    server: &Server,
    faction: u32,
    brain_args: &[&str],
    turns: u64,
    log_dir: Option<&Path>,
) -> AiRun {
    let ai_log_path = server.scratch.dir.join(format!("sim_ai_{faction}.log"));
    let ai_log = fs::File::create(&ai_log_path).expect("sim_ai log file");
    let ai_log_err = ai_log.try_clone().expect("sim_ai log file (stderr half)");
    let mut command = Command::new(sim_ai);
    command
        .current_dir(&server.scratch.dir)
        .args(["--ports-file"])
        .arg(&server.ports_path)
        .args(["--faction", &faction.to_string()])
        .args(brain_args)
        .args(["--turns", &turns.to_string()])
        .env("RUST_LOG", AI_LOG_FILTER)
        .stdin(Stdio::null())
        .stdout(Stdio::from(ai_log))
        .stderr(Stdio::from(ai_log_err));
    if let Some(log_dir) = log_dir {
        command.arg("--log-dir").arg(log_dir);
    }
    let mut ai = Process {
        child: command.spawn().expect("the built sim_ai starts"),
    };
    let deadline = Instant::now() + AI_EXIT_TIMEOUT;
    let status = loop {
        if let Ok(Some(status)) = ai.child.try_wait() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "sim_ai did not exit within {}s\n--- sim_ai log ---\n{}\n--- server log ---\n{}",
            AI_EXIT_TIMEOUT.as_secs(),
            log_tail(&ai_log_path),
            log_tail(&server.log_path)
        );
        std::thread::sleep(AI_EXIT_POLL);
    };
    assert!(
        status.success(),
        "sim_ai exited {status}\n--- sim_ai log ---\n{}\n--- server log ---\n{}",
        log_tail(&ai_log_path),
        log_tail(&server.log_path)
    );
    AiRun {
        status,
        log_path: ai_log_path,
    }
}

// =================================================================================================
// A seated connection
// =================================================================================================

/// One seated connection and, once greeted, its stream.
pub struct Link {
    command: TcpStream,
    replies: BufReader<TcpStream>,
    frames: Option<BufReader<TcpStream>>,
    log_path: PathBuf,
}

impl Link {
    pub fn open(addr: SocketAddr, log_path: &Path) -> Self {
        let command = TcpStream::connect(addr).expect("connect to the command socket");
        command
            .set_read_timeout(Some(RESPONSE_TIMEOUT))
            .expect("command socket read timeout");
        let replies = BufReader::new(command.try_clone().expect("the reply half"));
        Self {
            command,
            replies,
            frames: None,
            log_path: log_path.to_path_buf(),
        }
    }

    pub fn fail(&self, what: &str) -> ! {
        panic!(
            "{what}\n--- server log (last {LOG_TAIL_LINES} lines) ---\n{}",
            log_tail(&self.log_path)
        );
    }

    pub fn write(&mut self, payload: CommandPayload, waiting_for: &str) {
        let envelope = CommandEnvelope {
            payload,
            correlation_id: None,
        };
        let bytes = envelope.encode_to_vec().expect("the command encodes");
        let mut framed = Vec::with_capacity(std::mem::size_of::<u32>() + bytes.len());
        framed.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        framed.extend_from_slice(&bytes);
        if let Err(err) = self.command.write_all(&framed) {
            self.fail(&format!("the command socket refused {waiting_for}: {err}"));
        }
        if let Err(err) = self.command.flush() {
            self.fail(&format!(
                "the command socket refused a flush for {waiting_for}: {err}"
            ));
        }
    }

    pub fn read_reply(&mut self, request_id: u64, waiting_for: &str) -> QueryReply {
        let mut len = [0u8; std::mem::size_of::<u32>()];
        if let Err(err) = self.replies.read_exact(&mut len) {
            self.fail(&format!("no reply arrived for {waiting_for}: {err}"));
        }
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 || len > sim_runtime::MAX_PROTO_FRAME {
            self.fail(&format!(
                "a {len}-byte reply was announced for {waiting_for}"
            ));
        }
        let mut payload = vec![0u8; len];
        if let Err(err) = self.replies.read_exact(&mut payload) {
            self.fail(&format!(
                "the reply for {waiting_for} never finished arriving: {err}"
            ));
        }
        let envelope = QueryReplyEnvelope::decode(&payload).expect("the reply decodes");
        assert_eq!(
            envelope.request_id, request_id,
            "a reply answered another request"
        );
        envelope.reply
    }

    /// Claim `faction`, retrying `seat_occupied` the way the shipped client does.
    pub fn claim_seat(&mut self, request_id: u64, faction: u32) -> SeatClaimReply {
        let waiting_for = format!("the claim of seat {faction}");
        for attempt in 0..SEAT_CLAIM_ATTEMPTS {
            self.write(
                CommandPayload::ClaimSeat {
                    request_id: request_id + u64::from(attempt),
                    faction_id: faction,
                },
                &waiting_for,
            );
            let answer = match self.read_reply(request_id + u64::from(attempt), &waiting_for) {
                QueryReply::SeatClaim(answer) => answer,
                other => self.fail(&format!("{waiting_for} was answered with {other:?}")),
            };
            if answer.ok || answer.error != sim_runtime::commands::seat_error::SEAT_OCCUPIED {
                return answer;
            }
            std::thread::sleep(SEAT_CLAIM_RETRY_BACKOFF);
        }
        self.fail(&format!(
            "{waiting_for} stayed occupied for {SEAT_CLAIM_ATTEMPTS} attempts"
        ))
    }

    /// Greet the stream with `token`, ask for a full frame, and decode it.
    pub fn full_frame_for(&mut self, stream_addr: SocketAddr, token: u64) -> WorldSnapshot {
        let mut stream = TcpStream::connect(stream_addr).expect("connect to the stream socket");
        stream
            .set_read_timeout(Some(RESPONSE_TIMEOUT))
            .expect("stream socket read timeout");
        assert_eq!(SEAT_TOKEN_BYTES, std::mem::size_of::<u64>());
        stream
            .write_all(&token.to_le_bytes())
            .expect("present the seat token");
        self.frames = Some(BufReader::new(stream));
        self.write(CommandPayload::Resync, "the resync");
        for _ in 0..MAX_FRAMES_AWAITED {
            let bytes = self.next_frame_bytes();
            if let FramePayload::Snapshot(snapshot) =
                decode_frame_flatbuffer(&bytes).expect("a published frame decodes")
            {
                return snapshot;
            }
        }
        self.fail(&format!(
            "no full frame arrived within {MAX_FRAMES_AWAITED} frames of the resync"
        ))
    }

    fn next_frame_bytes(&mut self) -> Vec<u8> {
        let mut len = [0u8; std::mem::size_of::<u32>()];
        let frames = self.frames.as_mut().expect("a greeted stream");
        if let Err(err) = frames.read_exact(&mut len) {
            self.fail(&format!("no frame arrived on the stream: {err}"));
        }
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 || len > MAX_SNAPSHOT_FRAME_BYTES {
            self.fail(&format!("the stream announced a {len}-byte frame"));
        }
        let mut payload = vec![0u8; len];
        if let Err(err) = self
            .frames
            .as_mut()
            .expect("a greeted stream")
            .read_exact(&mut payload)
        {
            self.fail(&format!(
                "a {len}-byte frame never finished arriving: {err}"
            ));
        }
        payload
    }

    /// A question that names no faction, answered in order behind whatever was written before it.
    pub fn ask_list_saves(&mut self) -> QueryReply {
        self.write(
            CommandPayload::Query {
                request_id: SYNC_QUERY_ID,
                query: QueryPayload::ListSaves,
            },
            "the sync question",
        );
        self.read_reply(SYNC_QUERY_ID, "the sync question")
    }
}
