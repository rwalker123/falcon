//! ⛔ **A REAL `sim_ai` AGAINST A REAL SERVER, OVER THE REAL SOCKETS.**
//!
//! `sim_ai` is a player process (`docs/plan_ai_opponents.md` §1): it claims a seat, is sent that
//! seat's frames, and sends commands. Nothing in-process can prove that — the crate cannot link
//! `core_sim`, and the seat gate, the greeting and the turn scheduler all live in the server's main
//! loop. So this test drives the **built** `server` and the **built** `sim_ai`, exactly as the
//! launcher would, and observes the world afterwards through a connection of its own.
//!
//! **Why it lives in `core_sim/tests/`.** `CARGO_BIN_EXE_server` is only defined for the package
//! owning that bin, and the `sim_ai` binary is resolved as its **sibling** in the target directory
//! (`common::ai_process`, shared with `ai_bench.rs`, which also owns the fallback build).
//!
//! **What it asserts, and through whom.** Frames are viewer-scoped and fogged — a rival band the
//! human has never seen is in no frame of seat 0's — so the rival's world is read through seat 1
//! both times: *before*, on a connection released before the AI starts, and *after*, once the AI
//! has exited and released it. The rival's band count moved by the scripted `split_band`, and the
//! tick advanced by the turns the AI submitted `ready` for.

mod common;

use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use common::ai_process::{log_tail, sim_ai_binary, strip_ansi, Process, Scratch, LOG_TAIL_LINES};
use core_sim::network::SEAT_TOKEN_BYTES;
use core_sim::{apply_port_base, SimulationConfig};
use sim_runtime::commands::{QueryPayload, SeatClaimReply};
use sim_runtime::{
    decode_frame_flatbuffer, CommandEnvelope, CommandPayload, FramePayload, QueryReply,
    QueryReplyEnvelope, WorldSnapshot,
};

// =================================================================================================
// The world
// =================================================================================================

/// The same small world `query_seat_gate.rs` drives, for the same reasons.
const MAP_WIDTH: u32 = 24;
const MAP_HEIGHT: u32 = 16;
const MAP_PRESET: &str = "earthlike";
const START_PROFILE: &str = "late_forager_tribe";
/// Fixed and non-zero: `seed == 0` asks the server to randomise.
const MAP_SEED: u64 = 11;
/// One rival, so there is a seat for the AI to fill.
const ONE_RIVAL: u32 = 1;
/// The shipped separation seats one faction on a map this small; shrunk so a rival is seated.
const TEST_START_SEPARATION: u32 = 6;

const RIVAL_SEAT: u32 = 1;

/// **The scripted command: `split_band`, at the founding floor.** A fresh `late_forager_tribe`
/// band is 30 people, and `expedition_config.json`'s `settle` floors are `min_founding_workers 4`
/// / `parent_min_workers 6`, so asking for exactly the founding floor is legal for a starting band
/// and leaves the parent well above its own floor. The effect is a second resident band for the
/// rival — a count the frame states outright.
const SPLIT_WORKERS: u32 = 4;
/// How many resolved turns the AI plays before releasing its seat.
const AI_TURNS: u64 = 3;
/// The script: fire on the first tick the brain sees, whatever number it is.
const SCRIPT: &str = "# the scenario's one order\n+0: split_band {faction} {own_band:0} 4\n";

// =================================================================================================
// Ports, timeouts and logs
// =================================================================================================

const TEST_PORT_BASE: u16 = 45300;
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

const SYNC_QUERY_ID: u64 = 1;
/// Claim ids are spaced by the retry count, since a retried claim spends one id per attempt.
const BEFORE_CLAIM_ID: u64 = 100;
const AFTER_CLAIM_ID: u64 = 200;

// =================================================================================================
// The harness
// =================================================================================================

struct Ports {
    command: SocketAddr,
    stream: SocketAddr,
}

/// One seated connection and, once greeted, its stream.
struct Link {
    command: TcpStream,
    replies: BufReader<TcpStream>,
    frames: Option<BufReader<TcpStream>>,
    log_path: PathBuf,
}

impl Link {
    fn open(addr: SocketAddr, log_path: &Path) -> Self {
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

    fn fail(&self, what: &str) -> ! {
        panic!(
            "{what}\n--- server log (last {LOG_TAIL_LINES} lines) ---\n{}",
            log_tail(&self.log_path)
        );
    }

    fn write(&mut self, payload: CommandPayload, waiting_for: &str) {
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

    fn read_reply(&mut self, request_id: u64, waiting_for: &str) -> QueryReply {
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
    fn claim_seat(&mut self, request_id: u64, faction: u32) -> SeatClaimReply {
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
    fn full_frame_for(&mut self, stream_addr: SocketAddr, token: u64) -> WorldSnapshot {
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
}

fn start_server() -> (Process, Scratch, PathBuf, PathBuf, Ports) {
    let scratch = Scratch::new("ai_seat_scenario");
    let saves = scratch.dir.join("saves");
    fs::create_dir_all(&saves).expect("scratch save directory");
    let config_path = write_test_config(&scratch.dir);
    let ports_path = scratch.dir.join("ports.json");
    let log_path = scratch.dir.join("server.log");

    let log = fs::File::create(&log_path).expect("server log file");
    let log_err = log.try_clone().expect("server log file (stderr half)");
    let child = Command::new(env!("CARGO_BIN_EXE_server"))
        .current_dir(&scratch.dir)
        .env("SIM_SAVE_DIR", &saves)
        .env("SIM_CONFIG_PATH", &config_path)
        .env("SIM_PORTS_FILE", &ports_path)
        .env("RUST_LOG", SERVER_LOG_FILTER)
        .env_remove("SIM_PORT_BASE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .expect("the built server binary starts");
    let mut process = Process { child };
    let ports = await_ports_file(&mut process, &ports_path, &log_path);
    (process, scratch, log_path, ports_path, ports)
}

/// The shipped config, its port block moved, its separation shrunk, its seed and rival count
/// pinned.
fn write_test_config(dir: &Path) -> PathBuf {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/data/simulation_config.json");
    let raw = fs::read_to_string(&shipped).expect("the shipped simulation config reads");
    let mut config = SimulationConfig::from_file(&shipped).expect("the shipped config parses");
    assert!(apply_port_base(&mut config, TEST_PORT_BASE));
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

fn new_game() -> CommandPayload {
    CommandPayload::NewGame {
        preset_id: MAP_PRESET.to_string(),
        width: MAP_WIDTH,
        height: MAP_HEIGHT,
        seed: MAP_SEED,
        profile_id: START_PROFILE.to_string(),
        ai_faction_count: Some(ONE_RIVAL),
    }
}

/// The rival's RESIDENT bands: a split makes a band, and a detached party would flatter the count.
fn rival_band_count(snapshot: &WorldSnapshot) -> usize {
    snapshot
        .populations
        .iter()
        .filter(|cohort| cohort.faction == RIVAL_SEAT && !cohort.is_expedition)
        .count()
}

// =================================================================================================
// The test
// =================================================================================================

/// ⛔ **The AI claims its seat, plays its script, submits its turns, and leaves the world changed.**
#[test]
fn a_scripted_sim_ai_plays_a_seat_over_the_real_sockets() {
    let (_server, scratch, log_path, ports_path, ports) = start_server();
    let sim_ai = sim_ai_binary();

    // The world, asked for by an unseated connection (`new_game` names no faction and is not a
    // host verb), and synchronised by a question answered in order behind it.
    let mut builder = Link::open(ports.command, &log_path);
    builder.write(new_game(), "the world under test");
    match builder.ask_list_saves() {
        QueryReply::ListSaves(_) => {}
        other => builder.fail(&format!("the sync question was answered with {other:?}")),
    }

    // The world BEFORE, through the rival's own seat — then released, so the AI can claim it and
    // is the only occupant, and turns resolve on its `ready` alone.
    let (tick_before, bands_before) = {
        let mut before = Link::open(ports.command, &log_path);
        let claim = before.claim_seat(BEFORE_CLAIM_ID, RIVAL_SEAT);
        assert!(
            claim.ok,
            "seat {RIVAL_SEAT} could not be claimed: {} — this world must seat a rival",
            claim.error
        );
        let snapshot = before.full_frame_for(ports.stream, claim.seat_token);
        (snapshot.header.tick, rival_band_count(&snapshot))
    };
    assert!(
        bands_before > 0,
        "the world seats no rival band, so there is nothing for the AI to command"
    );

    let script_path = scratch.dir.join("scenario.script");
    fs::write(&script_path, SCRIPT).expect("the script writes");
    let ai_log_path = scratch.dir.join("sim_ai.log");
    let ai_log = fs::File::create(&ai_log_path).expect("sim_ai log file");
    let ai_log_err = ai_log.try_clone().expect("sim_ai log file (stderr half)");
    let mut ai = Process {
        child: Command::new(&sim_ai)
            .current_dir(&scratch.dir)
            .args(["--ports-file"])
            .arg(&ports_path)
            .args(["--faction", &RIVAL_SEAT.to_string()])
            .args(["--brain", "scripted", "--script"])
            .arg(&script_path)
            .args(["--turns", &AI_TURNS.to_string()])
            .env("RUST_LOG", "info")
            .stdin(Stdio::null())
            .stdout(Stdio::from(ai_log))
            .stderr(Stdio::from(ai_log_err))
            .spawn()
            .expect("the built sim_ai starts"),
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
            log_tail(&log_path)
        );
        std::thread::sleep(AI_EXIT_POLL);
    };
    assert!(
        status.success(),
        "sim_ai exited {status}\n--- sim_ai log ---\n{}\n--- server log ---\n{}",
        log_tail(&ai_log_path),
        log_tail(&log_path)
    );

    // The world AFTER, through the seat the AI just released.
    let mut rival = Link::open(ports.command, &log_path);
    let claim = rival.claim_seat(AFTER_CLAIM_ID, RIVAL_SEAT);
    assert!(
        claim.ok,
        "seat {RIVAL_SEAT} could not be re-claimed: {}",
        claim.error
    );
    let after = rival.full_frame_for(ports.stream, claim.seat_token);

    assert_eq!(
        rival_band_count(&after),
        bands_before + 1,
        "the scripted split_band ({SPLIT_WORKERS} workers off band 0) did not make a second rival \
         band\n--- sim_ai log ---\n{}\n--- server log ---\n{}",
        log_tail(&ai_log_path),
        log_tail(&log_path)
    );
    assert!(
        after.header.tick >= tick_before + AI_TURNS,
        "the tick moved {tick_before} → {} while the AI submitted ready for {AI_TURNS} turns",
        after.header.tick
    );

    // The server's fmt layer colours its fields, so `faction=1` is not one substring until the
    // escape sequences are stripped.
    let server_log = strip_ansi(&fs::read_to_string(&log_path).expect("the server log reads"));
    assert!(
        server_log
            .lines()
            .any(|line| line.contains("seat.claimed")
                && line.contains(&format!("faction={RIVAL_SEAT}"))),
        "the server never logged the AI's claim of seat {RIVAL_SEAT}\n{}",
        log_tail(&log_path)
    );
    assert!(
        !server_log.contains("command.rejected"),
        "the server refused a command during the run\n{}",
        log_tail(&log_path)
    );
}

impl Link {
    /// A question that names no faction, answered in order behind whatever was written before it.
    fn ask_list_saves(&mut self) -> QueryReply {
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
