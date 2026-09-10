//! **`sim_ai bench`** — the harness (`docs/plan_ai_driver.md` §8.1 → Bench, §8.3).
//!
//! For each seed: write a scratch config from the shipped one with `map_seed` and
//! `default_ai_faction_count` pinned and the port block moved to a free private base; start the
//! built `server` on it; build the world with `new_game` from an unseated connection; spawn one
//! `sim_ai` per requested seat with `--turns` and `--log-dir`; wait for them all; kill the server.
//! Then read every seat's two logs into the measures of §8.2 (`measures.rs`), write
//! `report.json`, and print a table (`ratchet.rs`).
//!
//! **No host is needed.** Once every occupied seat has submitted, the server resolves the turn
//! itself (`SeatTurnGate` → `TurnWait::Resolve`, `core_sim/src/seats.rs`), auto-submitting the
//! vacant human seat. A bench is exactly a server and N player processes, nothing in-process.
//!
//! **The human seat is held until every rival is seated.** The gate resolves the moment every
//! *occupied* seat has submitted, so a rival that claims and readies before its neighbour has
//! claimed would advance the world alone — and which tick each seat first sees would depend on
//! process scheduling. The bench therefore claims seat 0 itself before spawning the rivals, waits
//! for the server to log each rival's claim, and only then releases it. Nothing is sent on it.
//!
//! The subprocess shape is `core_sim/tests/query_seat_gate.rs` (`start_server`,
//! `write_test_config`, `await_ports_file`); the config rewrite restates `core_sim::apply_port_base`
//! because this crate cannot link the server.

pub mod measures;
pub mod ratchet;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::time::{Duration, Instant};

use clap::Parser;
use sim_runtime::CommandPayload;
use tracing::info;

use crate::link::{Endpoints, Link, UnseatedConnection};
use crate::BrainKind;
use measures::{measures_for_seat, Measures};
use ratchet::{Baselines, CheckOutcome, Report, RunMeasures, REPORT_FILE};

// =================================================================================================
// The world
// =================================================================================================

/// **The shipped simulation config, embedded at build time** — the same file the server embeds as
/// `BUILTIN_SIMULATION_CONFIG` (`core_sim/src/resources.rs`). A file include, not a crate link.
const SHIPPED_CONFIG: &str = include_str!("../../../core_sim/src/data/simulation_config.json");

/// The same small world the scenario tests drive (`core_sim/tests/ai_seat_scenario.rs`), so a
/// bench turn is a fraction of a second and thirty of them are seconds.
const MAP_WIDTH: u32 = 24;
const MAP_HEIGHT: u32 = 16;
const MAP_PRESET: &str = "earthlike";
const START_PROFILE: &str = "late_forager_tribe";
/// The shipped separation seats one faction on a map this small; shrunk so the rivals are seated.
const START_SEPARATION: u32 = 6;
/// The seat the bench holds while the rivals claim theirs (`HudConst.PLAYER_FACTION_ID`).
const HUMAN_SEAT: u32 = 0;

/// The config keys the bench rewrites.
const KEY_MAP_SEED: &str = "map_seed";
const KEY_AI_FACTION_COUNT: &str = "default_ai_faction_count";
const KEY_START_SEPARATION: &str = "faction_start_min_separation";
/// **The port block's four keys and their offsets.** Restated from `core_sim::apply_port_base`
/// (`core_sim/src/resources.rs`) and `port_alloc.rs`: slot 0 is the reserved base, then command,
/// snapshot_flat, log.
const KEY_PORT_BASE_BIND: &str = "port_base_bind";
const KEY_COMMAND_BIND: &str = "command_bind";
const KEY_SNAPSHOT_FLAT_BIND: &str = "snapshot_flat_bind";
const KEY_LOG_BIND: &str = "log_bind";
const COMMAND_PORT_OFFSET: u16 = 1;
const SNAPSHOT_FLAT_PORT_OFFSET: u16 = 2;
const LOG_PORT_OFFSET: u16 = 3;
/// Where the bench looks for a free block: well above the default 41000 block and the test
/// harnesses' 45xxx bases, stepping by the server's own stride (`port_alloc::PORT_BLOCK_STRIDE`).
const PORT_PROBE_START: u16 = 46000;
const PORT_BLOCK_STRIDE: u16 = 10;
const PORT_PROBE_ATTEMPTS: u16 = 200;
const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

// =================================================================================================
// Files, environment, timing
// =================================================================================================

/// The server's environment (`core_sim/CLAUDE.md` → Environment Overrides).
const ENV_SAVE_DIR: &str = "SIM_SAVE_DIR";
const ENV_CONFIG_PATH: &str = "SIM_CONFIG_PATH";
const ENV_PORTS_FILE: &str = "SIM_PORTS_FILE";
/// An explicit base is honoured exactly and a busy block is fatal; the config's base auto-bumps.
/// Inheriting one from the operator's shell would put the bench on a block someone is using.
const ENV_PORT_BASE: &str = "SIM_PORT_BASE";
const ENV_RUST_LOG: &str = "RUST_LOG";
/// `seat.claimed` is an INFO line; the bench reads it.
const SERVER_LOG_FILTER: &str = "info";
const SEAT_LOG_FILTER: &str = "info";

const CONFIG_FILE: &str = "simulation_config.json";
const PORTS_FILE: &str = "ports.json";
const SERVER_LOG_FILE: &str = "server.log";
const SAVES_DIR: &str = "saves";
const SEAT_DIR_PREFIX: &str = "seat_";
const SEAT_LOG_FILE: &str = "sim_ai.log";
/// The server binary beside this one, by default.
const SERVER_STEM: &str = "server";
/// The ports file's keys (`.claude/rules/core_sim/ports.md`).
const PORTS_KEY_HOST: &str = "host";
const PORTS_KEY_COMMAND: &str = "command";
const PORTS_KEY_STREAM: &str = "snapshot_flat";
const PORTS_KEY_PID: &str = "pid";
/// The server's claim line and its faction field (`core_sim/src/bin/server.rs`, `seat.claimed`).
const SEAT_CLAIMED_MARKER: &str = "seat.claimed";
const SEAT_CLAIMED_FACTION_FIELD: &str = "faction=";

const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(60);
const WORLD_BUILD_TIMEOUT: Duration = Duration::from_secs(60);
const SEAT_CLAIM_TIMEOUT: Duration = Duration::from_secs(60);
/// How long the seats get to finish: a fixed allowance for claiming and the first frame, plus
/// this much per turn — generous against a cold debug build, tight against a wedge.
const SEATS_STARTUP_ALLOWANCE: Duration = Duration::from_secs(60);
const PER_TURN_ALLOWANCE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(50);
const LOG_TAIL_LINES: usize = 30;

/// The introducer of an ANSI escape sequence and the byte that ends an SGR one — the server's
/// fmt layer colours its fields, so `faction=1` is not one token until they are stripped.
const ANSI_ESCAPE: char = '\x1b';
const ANSI_SGR_END: char = 'm';

/// The seat spec's separators: `<faction>=<brain>[:<script>]`.
const SEAT_SPEC_BRAIN_SEPARATOR: char = '=';
const SEAT_SPEC_SCRIPT_SEPARATOR: char = ':';

// =================================================================================================
// Arguments
// =================================================================================================

/// One seat to fill: `<faction>=<brain>[:<script>]`, e.g. `1=pass` or `2=scripted:orders.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatSpec {
    pub faction: u32,
    pub brain: BrainKind,
    pub script: Option<PathBuf>,
    /// The spec as given, the key the report and baselines carry.
    text: String,
}

impl FromStr for SeatSpec {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (faction, rest) = text
            .split_once(SEAT_SPEC_BRAIN_SEPARATOR)
            .ok_or_else(|| format!("`{text}`: expected <faction>=<brain>[:<script>]"))?;
        let faction: u32 = faction
            .trim()
            .parse()
            .map_err(|_| format!("`{faction}` is not a faction id"))?;
        let (brain, script) = match rest.split_once(SEAT_SPEC_SCRIPT_SEPARATOR) {
            Some((brain, script)) => (brain, Some(PathBuf::from(script))),
            None => (rest, None),
        };
        let brain = match brain.trim() {
            "pass" => BrainKind::Pass,
            "scripted" => BrainKind::Scripted,
            other => return Err(format!("`{other}` is not a brain (pass | scripted)")),
        };
        if brain == BrainKind::Scripted && script.is_none() {
            return Err(format!(
                "`{text}`: scripted needs a script (`scripted:<path>`)"
            ));
        }
        Ok(Self {
            faction,
            brain,
            script,
            text: text.to_owned(),
        })
    }
}

impl fmt::Display for SeatSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "sim_ai bench",
    about = "Start a server per seed, seat one sim_ai per --seats, and measure them from their logs."
)]
pub struct BenchArgs {
    /// The server binary (default: `server` beside this executable).
    #[arg(long)]
    pub server: Option<PathBuf>,
    /// A simulation config to start from (default: the shipped one, embedded at build time).
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// The map seeds, comma-separated.
    #[arg(long, value_delimiter = ',', required = true)]
    pub seeds: Vec<u64>,
    /// Turns each seat plays.
    #[arg(long)]
    pub turns: u64,
    /// A seat to fill, `<faction>=<brain>[:<script>]`; repeatable.
    #[arg(long = "seats", required = true)]
    pub seats: Vec<SeatSpec>,
    /// Where the run's logs and `report.json` go.
    #[arg(long)]
    pub out: PathBuf,
    /// Another run's `--out` directory: report `this − other` per seat and measure.
    #[arg(long)]
    pub compare: Option<PathBuf>,
    /// A baselines file: fail (exit 1) on any drop beyond tolerance.
    #[arg(long)]
    pub check: Option<PathBuf>,
    /// Regenerate a baselines file from this run.
    #[arg(long)]
    pub write_baselines: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("no server binary at {0}; pass --server")]
    NoServer(PathBuf),
    #[error("the seat specs name faction {0} twice")]
    DuplicateSeat(u32),
    #[error("the seat specs name the human seat {HUMAN_SEAT}; the bench holds it")]
    HumanSeat,
    #[error("no free port block found from {PORT_PROBE_START} upwards")]
    NoFreePorts,
    #[error("could not read the config {path}: {source}")]
    Config {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("the config is not JSON: {0}")]
    ConfigJson(#[from] serde_json::Error),
    #[error("io at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("seed {seed}: the server exited ({status}) before publishing its ports\n{log}")]
    ServerExited {
        seed: u64,
        status: String,
        log: String,
    },
    #[error("seed {seed}: the server never published its ports file\n{log}")]
    ServerNotReady { seed: u64, log: String },
    #[error("seed {seed}: the world could not be built: {detail}\n{log}")]
    WorldBuild {
        seed: u64,
        detail: String,
        log: String,
    },
    #[error(
        "seed {seed}: seat {faction} was not claimed within {:?}\n{log}",
        SEAT_CLAIM_TIMEOUT
    )]
    SeatNotClaimed {
        seed: u64,
        faction: u32,
        log: String,
    },
    #[error("seed {seed}: seat {faction} exited {status}\n--- sim_ai ---\n{seat_log}\n--- server ---\n{server_log}")]
    SeatFailed {
        seed: u64,
        faction: u32,
        status: String,
        seat_log: String,
        server_log: String,
    },
    #[error("seed {seed}: seat {faction} did not finish {turns} turns in time\n--- sim_ai ---\n{seat_log}\n--- server ---\n{server_log}")]
    SeatTimedOut {
        seed: u64,
        faction: u32,
        turns: u64,
        seat_log: String,
        server_log: String,
    },
    #[error(transparent)]
    Link(#[from] crate::link::LinkError),
    #[error(transparent)]
    Measure(#[from] measures::MeasureError),
    #[error(transparent)]
    Ratchet(#[from] ratchet::RatchetError),
    #[error("{0} baseline violation(s); see the table above and {REPORT_FILE}")]
    CheckFailed(usize),
}

// =================================================================================================
// The harness
// =================================================================================================

/// A child killed on drop, including on a panic or an early return.
struct Process {
    child: Child,
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Run the bench: every seed, then the report. `Ok` only when `--check` (if given) passes.
pub fn run(args: BenchArgs) -> Result<(), BenchError> {
    let server = match &args.server {
        Some(server) => server.clone(),
        None => default_server_path(),
    };
    if !server.is_file() {
        return Err(BenchError::NoServer(server));
    }
    validate_seats(&args.seats)?;
    let base_config: serde_json::Value = match &args.config {
        Some(path) => serde_json::from_str(&fs::read_to_string(path).map_err(|source| {
            BenchError::Config {
                path: path.display().to_string(),
                source,
            }
        })?)?,
        None => serde_json::from_str(SHIPPED_CONFIG)?,
    };
    fs::create_dir_all(&args.out).map_err(|source| io_at(&args.out, source))?;

    let mut measures: RunMeasures = BTreeMap::new();
    let mut wall_seconds = BTreeMap::new();
    for &seed in &args.seeds {
        let started = Instant::now();
        let seed_dir = args.out.join(seed.to_string());
        run_seed(&server, &base_config, seed, &args, &seed_dir)?;
        let mut seats: BTreeMap<String, Measures> = BTreeMap::new();
        for seat in &args.seats {
            seats.insert(
                seat.faction.to_string(),
                measures_for_seat(&seat_dir(&seed_dir, seat.faction))?,
            );
        }
        measures.insert(seed.to_string(), seats);
        wall_seconds.insert(seed.to_string(), started.elapsed().as_secs_f64());
        info!(seed, seconds = started.elapsed().as_secs_f64(), "seed done");
    }

    let mut report = Report {
        seeds: args.seeds.clone(),
        turns: args.turns,
        seats: args.seats.iter().map(ToString::to_string).collect(),
        wall_seconds,
        measures,
        compare: None,
        check: None,
    };
    if let Some(other_dir) = &args.compare {
        let other = Report::read(&other_dir.join(REPORT_FILE))?;
        report.compare = Some(report.compare(&other)?);
    }
    let mut violations = 0;
    if let Some(baselines_path) = &args.check {
        let baselines = Baselines::read(baselines_path)?;
        let found = report.check(&baselines)?;
        violations = found.len();
        report.check = Some(CheckOutcome {
            baselines: baselines_path.display().to_string(),
            violations: found,
        });
    }
    if let Some(path) = &args.write_baselines {
        report.as_baselines().write(path)?;
        info!(path = %path.display(), "baselines written");
    }
    report.write(&args.out.join(REPORT_FILE))?;
    print!("{}", report.table());
    if violations > 0 {
        return Err(BenchError::CheckFailed(violations));
    }
    Ok(())
}

fn default_server_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    exe.with_file_name(format!("{SERVER_STEM}{}", std::env::consts::EXE_SUFFIX))
}

fn validate_seats(seats: &[SeatSpec]) -> Result<(), BenchError> {
    for (index, seat) in seats.iter().enumerate() {
        if seat.faction == HUMAN_SEAT {
            return Err(BenchError::HumanSeat);
        }
        if seats[..index]
            .iter()
            .any(|other| other.faction == seat.faction)
        {
            return Err(BenchError::DuplicateSeat(seat.faction));
        }
    }
    Ok(())
}

fn io_at(path: &Path, source: io::Error) -> BenchError {
    BenchError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn seat_dir(seed_dir: &Path, faction: u32) -> PathBuf {
    seed_dir.join(format!("{SEAT_DIR_PREFIX}{faction}"))
}

/// One seed: server up, world built, seats played, server down. The logs are left in `seed_dir`.
fn run_seed(
    server: &Path,
    base_config: &serde_json::Value,
    seed: u64,
    args: &BenchArgs,
    seed_dir: &Path,
) -> Result<(), BenchError> {
    let _ = fs::remove_dir_all(seed_dir);
    fs::create_dir_all(seed_dir).map_err(|source| io_at(seed_dir, source))?;
    let saves = seed_dir.join(SAVES_DIR);
    fs::create_dir_all(&saves).map_err(|source| io_at(&saves, source))?;
    let rivals = u32::try_from(args.seats.len()).expect("a handful of seats");
    let config_path = write_config(base_config, seed, rivals, seed_dir)?;
    let ports_path = seed_dir.join(PORTS_FILE);
    let server_log = seed_dir.join(SERVER_LOG_FILE);

    let log = fs::File::create(&server_log).map_err(|source| io_at(&server_log, source))?;
    let log_err = log
        .try_clone()
        .map_err(|source| io_at(&server_log, source))?;
    let mut server = Process {
        child: Command::new(server)
            .current_dir(seed_dir)
            .env(ENV_SAVE_DIR, &saves)
            .env(ENV_CONFIG_PATH, &config_path)
            .env(ENV_PORTS_FILE, &ports_path)
            .env(ENV_RUST_LOG, SERVER_LOG_FILTER)
            .env_remove(ENV_PORT_BASE)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .spawn()
            .map_err(|source| io_at(server, source))?,
    };
    let endpoints = await_ports_file(&mut server, &ports_path, &server_log, seed)?;
    info!(seed, command = %endpoints.command, "server up");

    // The world, from an unseated connection, then a question answered in order behind it.
    {
        let mut builder = UnseatedConnection::connect(endpoints.command).map_err(|err| {
            BenchError::WorldBuild {
                seed,
                detail: err.to_string(),
                log: log_tail(&server_log),
            }
        })?;
        builder
            .send(CommandPayload::NewGame {
                preset_id: MAP_PRESET.to_owned(),
                width: MAP_WIDTH,
                height: MAP_HEIGHT,
                seed,
                profile_id: START_PROFILE.to_owned(),
                ai_faction_count: Some(rivals),
            })
            .and_then(|()| builder.sync(WORLD_BUILD_TIMEOUT))
            .map_err(|err| BenchError::WorldBuild {
                seed,
                detail: err.to_string(),
                log: log_tail(&server_log),
            })?;
    }

    // Hold the human seat until every rival has claimed (module docs).
    let human_seat = Link::connect(endpoints, HUMAN_SEAT)?;
    let mut seats = Vec::with_capacity(args.seats.len());
    for seat in &args.seats {
        seats.push((
            seat.faction,
            spawn_seat(seat, &ports_path, seed_dir, args.turns)?,
        ));
    }
    await_claims(&args.seats, &server_log, seed)?;
    drop(human_seat);
    info!(seed, "every rival seated; the human seat is released");

    let deadline = Instant::now()
        + SEATS_STARTUP_ALLOWANCE
        + PER_TURN_ALLOWANCE * u32::try_from(args.turns).unwrap_or(u32::MAX);
    for (faction, mut process) in seats {
        let seat_log = seat_dir(seed_dir, faction).join(SEAT_LOG_FILE);
        loop {
            if let Ok(Some(status)) = process.child.try_wait() {
                if !status.success() {
                    return Err(BenchError::SeatFailed {
                        seed,
                        faction,
                        status: status.to_string(),
                        seat_log: log_tail(&seat_log),
                        server_log: log_tail(&server_log),
                    });
                }
                break;
            }
            if Instant::now() >= deadline {
                return Err(BenchError::SeatTimedOut {
                    seed,
                    faction,
                    turns: args.turns,
                    seat_log: log_tail(&seat_log),
                    server_log: log_tail(&server_log),
                });
            }
            std::thread::sleep(POLL);
        }
    }
    drop(server);
    Ok(())
}

/// The shipped config with the seed, the rival count and the separation pinned and the port block
/// moved to a free base.
fn write_config(
    base: &serde_json::Value,
    seed: u64,
    rivals: u32,
    seed_dir: &Path,
) -> Result<PathBuf, BenchError> {
    let mut json = base.clone();
    json[KEY_MAP_SEED] = serde_json::Value::from(seed);
    json[KEY_AI_FACTION_COUNT] = serde_json::Value::from(u64::from(rivals));
    json[KEY_START_SEPARATION] = serde_json::Value::from(u64::from(START_SEPARATION));
    let port_base = free_port_base()?;
    for (key, offset) in [
        (KEY_PORT_BASE_BIND, 0),
        (KEY_COMMAND_BIND, COMMAND_PORT_OFFSET),
        (KEY_SNAPSHOT_FLAT_BIND, SNAPSHOT_FLAT_PORT_OFFSET),
        (KEY_LOG_BIND, LOG_PORT_OFFSET),
    ] {
        json[key] =
            serde_json::Value::String(SocketAddr::new(LOCALHOST, port_base + offset).to_string());
    }
    let path = seed_dir.join(CONFIG_FILE);
    fs::write(&path, serde_json::to_string_pretty(&json)?)
        .map_err(|source| io_at(&path, source))?;
    Ok(path)
}

/// A base whose three used slots bind right now. The server auto-bumps a busy block anyway (its
/// config base is not an explicit `SIM_PORT_BASE`) and the bench reads the block it actually
/// bound from the ports file; the probe just keeps the first try honest.
fn free_port_base() -> Result<u16, BenchError> {
    for attempt in 0..PORT_PROBE_ATTEMPTS {
        let Some(base) = PORT_PROBE_START.checked_add(attempt * PORT_BLOCK_STRIDE) else {
            break;
        };
        let bound: Result<Vec<TcpListener>, _> = [
            COMMAND_PORT_OFFSET,
            SNAPSHOT_FLAT_PORT_OFFSET,
            LOG_PORT_OFFSET,
        ]
        .iter()
        .map(|offset| TcpListener::bind(SocketAddr::new(LOCALHOST, base + offset)))
        .collect();
        if bound.is_ok() {
            return Ok(base);
        }
    }
    Err(BenchError::NoFreePorts)
}

fn await_ports_file(
    server: &mut Process,
    ports_path: &Path,
    server_log: &Path,
    seed: u64,
) -> Result<Endpoints, BenchError> {
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    let pid = server.child.id();
    loop {
        if let Ok(Some(status)) = server.child.try_wait() {
            return Err(BenchError::ServerExited {
                seed,
                status: status.to_string(),
                log: log_tail(server_log),
            });
        }
        if let Some(endpoints) = read_ports_file(ports_path, pid) {
            return Ok(endpoints);
        }
        if Instant::now() >= deadline {
            return Err(BenchError::ServerNotReady {
                seed,
                log: log_tail(server_log),
            });
        }
        std::thread::sleep(POLL);
    }
}

/// The handshake file, once it records `pid` as its writer. A half-written file, or one left by
/// another process, reads as "not ready yet".
fn read_ports_file(path: &Path, pid: u32) -> Option<Endpoints> {
    let raw = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if json.get(PORTS_KEY_PID)?.as_u64()? != u64::from(pid) {
        return None;
    }
    let host: IpAddr = json.get(PORTS_KEY_HOST)?.as_str()?.parse().ok()?;
    let port = |key: &str| u16::try_from(json.get(key)?.as_u64()?).ok();
    Some(Endpoints {
        command: SocketAddr::new(host, port(PORTS_KEY_COMMAND)?),
        stream: SocketAddr::new(host, port(PORTS_KEY_STREAM)?),
    })
}

/// Start one `sim_ai` — this executable — on `seat`, logging under `seed_dir/seat_<f>/`.
fn spawn_seat(
    seat: &SeatSpec,
    ports_path: &Path,
    seed_dir: &Path,
    turns: u64,
) -> Result<Process, BenchError> {
    let log_dir = seat_dir(seed_dir, seat.faction);
    fs::create_dir_all(&log_dir).map_err(|source| io_at(&log_dir, source))?;
    let log_path = log_dir.join(SEAT_LOG_FILE);
    let log = fs::File::create(&log_path).map_err(|source| io_at(&log_path, source))?;
    let log_err = log.try_clone().map_err(|source| io_at(&log_path, source))?;
    let this = std::env::current_exe().map_err(|source| io_at(Path::new("sim_ai"), source))?;
    let mut command = Command::new(this);
    command
        .current_dir(seed_dir)
        .arg("--ports-file")
        .arg(ports_path)
        .args(["--faction", &seat.faction.to_string()])
        .args(["--turns", &turns.to_string()])
        .arg("--log-dir")
        .arg(&log_dir)
        .env(ENV_RUST_LOG, SEAT_LOG_FILTER)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    match seat.brain {
        BrainKind::Pass => {
            command.args(["--brain", "pass"]);
        }
        BrainKind::Scripted => {
            let script = seat
                .script
                .as_ref()
                .expect("a scripted spec carries a script");
            // The child runs in the seed directory; the script was named from the operator's.
            let script = std::path::absolute(script).map_err(|source| io_at(script, source))?;
            command
                .args(["--brain", "scripted"])
                .arg("--script")
                .arg(script);
        }
    }
    Ok(Process {
        child: command.spawn().map_err(|source| io_at(&log_path, source))?,
    })
}

/// Wait until the server has logged `seat.claimed … faction=<f>` for every seat.
fn await_claims(seats: &[SeatSpec], server_log: &Path, seed: u64) -> Result<(), BenchError> {
    let deadline = Instant::now() + SEAT_CLAIM_TIMEOUT;
    loop {
        let text = strip_ansi(&fs::read_to_string(server_log).unwrap_or_default());
        let unclaimed = seats.iter().find(|seat| !claim_logged(&text, seat.faction));
        let Some(seat) = unclaimed else {
            return Ok(());
        };
        if Instant::now() >= deadline {
            return Err(BenchError::SeatNotClaimed {
                seed,
                faction: seat.faction,
                log: log_tail(server_log),
            });
        }
        std::thread::sleep(POLL);
    }
}

fn claim_logged(server_log: &str, faction: u32) -> bool {
    let field = format!("{SEAT_CLAIMED_FACTION_FIELD}{faction}");
    server_log
        .lines()
        .filter(|line| line.contains(SEAT_CLAIMED_MARKER))
        .any(|line| line.split_whitespace().any(|token| token == field))
}

/// `text` without its ANSI colour sequences.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_escape = false;
    for ch in text.chars() {
        if in_escape {
            if ch == ANSI_SGR_END {
                in_escape = false;
            }
        } else if ch == ANSI_ESCAPE {
            in_escape = true;
        } else {
            out.push(ch);
        }
    }
    out
}

fn log_tail(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
        }
        Err(err) => format!("(the log at {} could not be read: {err})", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seat_spec_parses_its_three_forms_and_refuses_the_rest() {
        let pass: SeatSpec = "1=pass".parse().expect("pass");
        assert_eq!(pass.faction, 1);
        assert_eq!(pass.brain, BrainKind::Pass);
        assert_eq!(pass.script, None);
        assert_eq!(pass.to_string(), "1=pass");
        let scripted: SeatSpec = "2=scripted:orders.txt".parse().expect("scripted");
        assert_eq!(scripted.brain, BrainKind::Scripted);
        assert_eq!(scripted.script, Some(PathBuf::from("orders.txt")));
        assert!("2=scripted".parse::<SeatSpec>().is_err(), "no script");
        assert!("x=pass".parse::<SeatSpec>().is_err(), "no faction");
        assert!(
            "1=utility".parse::<SeatSpec>().is_err(),
            "no such brain yet"
        );
        assert!("1".parse::<SeatSpec>().is_err(), "no brain");
    }

    #[test]
    fn the_human_seat_and_a_duplicate_are_refused() {
        let human: SeatSpec = "0=pass".parse().unwrap();
        assert!(matches!(
            validate_seats(&[human]),
            Err(BenchError::HumanSeat)
        ));
        let one: SeatSpec = "1=pass".parse().unwrap();
        assert!(matches!(
            validate_seats(&[one.clone(), one]),
            Err(BenchError::DuplicateSeat(1))
        ));
    }

    #[test]
    fn the_shipped_config_embeds_and_rewrites_to_a_pinned_world() {
        let base: serde_json::Value = serde_json::from_str(SHIPPED_CONFIG).expect("JSON");
        let dir = std::env::temp_dir().join(format!("sim_ai_bench_config_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = write_config(&base, 11, 2, &dir).expect("writes");
        let json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(json[KEY_MAP_SEED], 11);
        assert_eq!(json[KEY_AI_FACTION_COUNT], 2);
        assert_eq!(json[KEY_START_SEPARATION], START_SEPARATION);
        let base_port: SocketAddr = json[KEY_PORT_BASE_BIND].as_str().unwrap().parse().unwrap();
        let command: SocketAddr = json[KEY_COMMAND_BIND].as_str().unwrap().parse().unwrap();
        assert!(
            base_port.port() >= PORT_PROBE_START,
            "never the default block"
        );
        assert_eq!(command.port(), base_port.port() + COMMAND_PORT_OFFSET);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_claim_is_found_by_its_faction_token_after_the_colours_are_stripped() {
        let log = "\x1b[2m2026\x1b[0m INFO shadow_scale::server: seat.claimed \x1b[3mconnection\x1b[0m=2 \x1b[3mfaction\x1b[0m=1\n";
        let text = strip_ansi(log);
        assert!(claim_logged(&text, 1));
        assert!(!claim_logged(&text, 10), "faction=1 is not faction=10");
        assert!(!claim_logged("seat.released faction=1", 1));
    }
}
