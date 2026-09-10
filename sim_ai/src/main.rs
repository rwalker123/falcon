//! **`sim_ai` — a player process** (`docs/plan_ai_driver.md` §1, §7; `docs/plan_ai_opponents.md`
//! §1). It connects to the command and stream ports, claims a seat, receives that seat's frames
//! and sends commands. The simulation cannot tell it from the Godot client, and it links the wire
//! (`sim_runtime`) and never the world (`tests/crate_boundary.rs`).
//!
//! The turn loop: on every frame the view is updated; when the frame's tick is one the brain has
//! not acted on, `decide` runs, its commands go out on the seated link, and `Orders { Ready }`
//! follows — **always**, even when the brain returned nothing. A mid-turn recapture arrives with
//! the same tick and is never acted on twice.

mod brain;
mod link;
mod view;

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use rand::rngs::StdRng;
use rand::SeedableRng;
use sim_runtime::{CommandPayload, OrdersDirective};
use tracing::{error, info, warn};

use brain::{Brain, PassBrain, ScriptedBrain};
use link::{Endpoints, Link, LinkEvent};
use view::{FrameOutcome, Perception};

/// The launcher's contract: the handshake file it writes for every child. Contract twin of
/// `ENV_PORTS_FILE` in `launcher/src/main.rs`.
const ENV_PORTS_FILE: &str = "SIM_PORTS_FILE";
/// The handshake file's keys this process reads (`.claude/rules/core_sim/ports.md`).
const PORTS_KEY_HOST: &str = "host";
const PORTS_KEY_COMMAND: &str = "command";
const PORTS_KEY_STREAM: &str = "snapshot_flat";

/// How long one `decide` may take. Well inside the server's `seat_turn_timeout_seconds` (120 s):
/// a brain that blocks loses flavour, not the turn — the loop submits `ready` after it regardless
/// and says so.
const DECIDE_BUDGET: Duration = Duration::from_secs(30);
/// How long the loop waits for a socket event before checking its own bookkeeping.
const EVENT_POLL: Duration = Duration::from_millis(250);
/// The one seed value meaning "derive from the faction", so two rivals with no seed differ.
const DERIVE_SEED_FROM_FACTION: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BrainKind {
    Pass,
    Scripted,
}

#[derive(Parser, Debug)]
#[command(
    name = "sim_ai",
    about = "A Shadow-Scale player process: claims a seat and plays it."
)]
struct Args {
    /// The server's handshake file (default: `$SIM_PORTS_FILE`, as the launcher sets it).
    #[arg(long, env = ENV_PORTS_FILE, conflicts_with_all = ["host", "command_port", "stream_port"])]
    ports_file: Option<PathBuf>,
    /// The server host, when the ports are given explicitly.
    #[arg(long, requires_all = ["command_port", "stream_port"])]
    host: Option<IpAddr>,
    #[arg(long)]
    command_port: Option<u16>,
    #[arg(long)]
    stream_port: Option<u16>,
    /// The faction seat this process fills.
    #[arg(long)]
    faction: u32,
    #[arg(long, value_enum, default_value_t = BrainKind::Pass)]
    brain: BrainKind,
    /// The command script (required for `--brain scripted`).
    #[arg(long, required_if_eq("brain", "scripted"))]
    script: Option<PathBuf>,
    /// The brain's rng seed; `0` derives it from the faction.
    #[arg(long, default_value_t = DERIVE_SEED_FROM_FACTION)]
    seed: u64,
    /// Exit 0 after this many resolved turns (advances of the frame's tick).
    #[arg(long)]
    turns: Option<u64>,
    /// Where the instruments will write. Accepted now, used by a later slice.
    #[arg(long)]
    log_dir: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
enum RunError {
    #[error("neither --ports-file (or {ENV_PORTS_FILE}) nor --host/--command-port/--stream-port was given")]
    NoEndpoints,
    #[error("could not read the ports file {path}: {detail}")]
    PortsFile { path: String, detail: String },
    #[error(transparent)]
    Link(#[from] link::LinkError),
    #[error(transparent)]
    Script(#[from] brain::ScriptError),
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    if let Err(err) = run(args) {
        error!(%err, "sim_ai exiting");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), RunError> {
    let endpoints = resolve_endpoints(&args)?;
    let faction = args.faction;
    let seed = if args.seed == DERIVE_SEED_FROM_FACTION {
        u64::from(faction)
    } else {
        args.seed
    };
    let mut brain: Box<dyn Brain> = match args.brain {
        BrainKind::Pass => Box::new(PassBrain),
        BrainKind::Scripted => {
            let script = args
                .script
                .as_deref()
                .expect("clap requires --script for scripted");
            Box::new(ScriptedBrain::load(script, faction)?)
        }
    };
    if let Some(log_dir) = &args.log_dir {
        info!(log_dir = %log_dir.display(), "log dir accepted; the instruments arrive in a later slice");
    }

    let mut link = Link::connect(endpoints, faction)?;
    let mut perception = Perception::default();
    // A stream connection is sent nothing it did not ask for: ask.
    perception.expect_full_frame();
    link.resync()?;

    let mut turns_observed: u64 = 0;
    let mut last_tick_seen: Option<u64> = None;
    loop {
        let Some(event) = link.next_event(EVENT_POLL) else {
            continue;
        };
        match event {
            LinkEvent::Frame(bytes) => match perception.ingest(&bytes) {
                FrameOutcome::Replaced | FrameOutcome::Applied => {}
                FrameOutcome::ChainBroken(err) => {
                    warn!(%err, "asking for a full frame");
                    link.resync()?;
                    continue;
                }
                FrameOutcome::AwaitingFullFrame => continue,
                FrameOutcome::Undecodable(err) => {
                    error!(%err, "frame dropped");
                    continue;
                }
            },
            LinkEvent::CommandDropped(detail) => {
                warn!(%detail, "command link dropped");
                link.reconnect()?;
                perception.expect_full_frame();
                link.resync()?;
                continue;
            }
            LinkEvent::StreamDropped(detail) => {
                warn!(%detail, "stream dropped");
                link.reopen_stream()?;
                perception.expect_full_frame();
                link.resync()?;
                continue;
            }
            LinkEvent::Reply(reply) => {
                info!(request_id = reply.request_id, "unsolicited reply ignored");
                continue;
            }
        }

        let Some(view) = perception.view_mut() else {
            continue;
        };
        let tick = view.snapshot.header.tick;
        if let Some(previous) = last_tick_seen {
            if tick > previous {
                turns_observed += 1;
            }
        }
        last_tick_seen = Some(tick);
        if let Some(limit) = args.turns {
            if turns_observed >= limit {
                info!(turns_observed, "turn budget reached; releasing the seat");
                return Ok(());
            }
        }
        if view.last_acted_tick.is_some_and(|acted| tick <= acted) {
            continue;
        }

        let started = Instant::now();
        let mut rng = decision_rng(seed, faction, tick);
        let commands = brain.decide(view, &mut rng);
        let elapsed = started.elapsed();
        if elapsed > DECIDE_BUDGET {
            warn!(
                tick,
                elapsed_ms = elapsed.as_millis(),
                budget_ms = DECIDE_BUDGET.as_millis(),
                "decide overran its budget; submitting ready regardless"
            );
        }
        for command in commands {
            link.send(command)?;
        }
        link.send(CommandPayload::Orders {
            faction_id: faction,
            directive: OrdersDirective::Ready,
        })?;
        view.last_acted_tick = Some(tick);
    }
}

/// One rng per decision, seeded from `(seed, faction, tick)` so a run replays exactly.
fn decision_rng(seed: u64, faction: u32, tick: u64) -> StdRng {
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..16].copy_from_slice(&u64::from(faction).to_le_bytes());
    bytes[16..24].copy_from_slice(&tick.to_le_bytes());
    StdRng::from_seed(bytes)
}

fn resolve_endpoints(args: &Args) -> Result<Endpoints, RunError> {
    if let (Some(host), Some(command), Some(stream)) =
        (args.host, args.command_port, args.stream_port)
    {
        return Ok(Endpoints {
            command: SocketAddr::new(host, command),
            stream: SocketAddr::new(host, stream),
        });
    }
    let Some(path) = args.ports_file.as_deref() else {
        return Err(RunError::NoEndpoints);
    };
    read_ports_file(path)
}

/// The handshake file, read the way `ServerPortsFile.gd` and the launcher read it: `host` plus the
/// `command` and `snapshot_flat` ports.
fn read_ports_file(path: &Path) -> Result<Endpoints, RunError> {
    let fail = |detail: String| RunError::PortsFile {
        path: path.display().to_string(),
        detail,
    };
    let raw = std::fs::read_to_string(path).map_err(|err| fail(err.to_string()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| fail(err.to_string()))?;
    let host: IpAddr = json
        .get(PORTS_KEY_HOST)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| fail(format!("no `{PORTS_KEY_HOST}` key")))?
        .parse()
        .map_err(|err| fail(format!("`{PORTS_KEY_HOST}` is not an address: {err}")))?;
    let port = |key: &str| -> Result<u16, RunError> {
        json.get(key)
            .and_then(serde_json::Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
            .ok_or_else(|| fail(format!("no `{key}` port")))
    };
    Ok(Endpoints {
        command: SocketAddr::new(host, port(PORTS_KEY_COMMAND)?),
        stream: SocketAddr::new(host, port(PORTS_KEY_STREAM)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn the_decision_rng_is_a_function_of_seed_faction_and_tick() {
        let a: u64 = decision_rng(1, 2, 3).gen();
        assert_eq!(a, decision_rng(1, 2, 3).gen::<u64>());
        assert_ne!(a, decision_rng(1, 2, 4).gen::<u64>());
        assert_ne!(a, decision_rng(1, 3, 3).gen::<u64>());
        assert_ne!(a, decision_rng(2, 2, 3).gen::<u64>());
    }

    #[test]
    fn the_ports_file_yields_the_command_and_stream_endpoints() {
        let dir = std::env::temp_dir().join(format!("sim_ai_ports_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("ports.json");
        std::fs::write(
            &path,
            r#"{"host":"127.0.0.1","command":41001,"snapshot_flat":41002,"log":41003,"pid":1}"#,
        )
        .expect("write");
        let endpoints = read_ports_file(&path).expect("reads");
        assert_eq!(endpoints.command, "127.0.0.1:41001".parse().unwrap());
        assert_eq!(endpoints.stream, "127.0.0.1:41002".parse().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
