//! ShadowScale desktop launcher.
//!
//! The game ships as two programs: a simulation SERVER that binds a block of
//! four local TCP ports, and PLAYER programs that connect to them — today one,
//! the Godot client the human at this keyboard sits in front of. A player
//! double-clicks one icon, so something has to start the server, wait for it to
//! be reachable, run one process per seat this machine hosts, and reap them all
//! afterwards. This binary is that supervisor; it replaces the per-platform
//! shell scripts
//! (`scripts/macos_dist/run.command`, `scripts/windows_dist/run.bat`) with one
//! implementation, and fixes the two bugs both of them shared:
//!
//! * they slept a fixed two seconds instead of waiting for an actual readiness
//!   signal, so a slow machine raced the client against an unbound server;
//! * they only cleaned the server up on the *clean* exit path, so a crashed or
//!   force-killed launcher orphaned a running server holding the ports.
//!
//! Readiness here is a fully written ports handshake file
//! (`core_sim::port_alloc`), at a path unique to this launcher process, and
//! shutdown is a `Drop` guard plus, on Windows, a kill-on-close Job Object that
//! the OS honours even if this process is terminated. Both cover **every**
//! child, not just the server: a seat's process is a child like any other, and
//! the reaping guarantee is the reason this binary exists.
//!
//! **Seats** (`.claude/rules/core_sim/launcher.md`). A world has N faction
//! seats and the sim knows only whether one is occupied; the person who started
//! the game is a remote player whose process happens to be local. So the human's
//! client is spawned *as the process filling a seat*, through the same path any
//! other locally-hosted seat would use — there is deliberately no in-process
//! fast path for a local player.
//!
//! **Rival seats are supervised, not listed.** The server decides the roster at
//! every world build — boot, `new_game` from the client's menu, a load — and
//! announces it as a `seats.roster` event on its log stream. A supervisor thread
//! reads that stream and, on every event, the session reconciles: one `sim_ai`
//! child per rival faction not yet running, and a child whose faction left the
//! roster is reaped. The human's client owns the session window: when it exits,
//! every AI child is reaped, then the server.

// No console window when the player double-clicks the packaged .exe. Errors are
// surfaced through `report_error` (a message box) rather than stdio.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::fs;
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Contract constants
// ---------------------------------------------------------------------------

/// Environment variable that both the server (`core_sim::port_alloc::
/// ports_file_path`) and the client (`ServerPortsFile.gd`) honour *verbatim*
/// when set. The launcher sets it for both children to a path unique to this
/// process (see [`ports_file_path`]), so this run's handshake is confused
/// neither with a file left behind by a crashed run nor with a *concurrently
/// running* second launcher.
const ENV_PORTS_FILE: &str = "SIM_PORTS_FILE";

/// Handshake file name, assembled as `{prefix}{pid}{extension}`.
///
/// Deliberately *not* `ports.json`: keeping the launcher's file distinct from
/// the server's default means a developer running the server by hand and a
/// player running the packaged game never collide.
///
/// The pid is what makes the name per-run rather than merely per-install, and
/// that matters because two launchers can legitimately run at once —
/// `core_sim::port_alloc::allocate` auto-bumps by `PORT_BLOCK_STRIDE` when no
/// explicit base is given, precisely so a second copy gets its own port block.
/// With one fixed name the second launcher's startup cleanup deleted the *first*
/// server's live handshake file; since the server writes that file exactly once
/// and never rewrites it, the first launcher then waited out the whole
/// [`READY_TIMEOUT`] and blamed a server that was perfectly healthy.
const PORTS_FILE_PREFIX: &str = "launcher-ports-";
const PORTS_FILE_EXTENSION: &str = ".json";

/// Directory name under the per-user app-data root. Contract twin of
/// `PORTS_FILE_DIR` in `core_sim/src/port_alloc.rs`.
const APP_DATA_DIR: &str = "ShadowScale";
/// Per-user app-data root on macOS, relative to `$HOME`. Mirrors
/// `MACOS_APP_SUPPORT` in `core_sim/src/port_alloc.rs`.
const MACOS_APP_SUPPORT: &str = "Library/Application Support";
/// Per-user state root on Linux, relative to `$HOME`, when `XDG_STATE_HOME` is
/// unset. Mirrors `LINUX_STATE_FALLBACK` in `core_sim/src/port_alloc.rs`.
const LINUX_STATE_FALLBACK: &str = ".local/state";

/// Environment variables consulted when deriving the per-user app-data root.
/// Same set, same precedence, as `core_sim::port_alloc::ports_file_path`.
const ENV_LOCALAPPDATA: &str = "LOCALAPPDATA";
const ENV_HOME: &str = "HOME";
const ENV_XDG_STATE_HOME: &str = "XDG_STATE_HOME";

/// How often the readiness loop checks for the handshake file. Short enough
/// that a fast local start feels instant, long enough not to spin the CPU.
const READY_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Upper bound on server startup. Worldgen on a cold, slow disk is the worst
/// case; past this the server is considered wedged rather than slow.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Exit code the server uses when it cannot bind its port block. Contract twin
/// of `PORT_ALLOC_EXIT_CODE` in `core_sim/src/bin/server.rs`; it is the one
/// failure with a player-actionable explanation, so it gets its own message.
const SERVER_PORT_ALLOC_EXIT_CODE: i32 = 2;

// ---------------------------------------------------------------------------
// Packaged file layout
// ---------------------------------------------------------------------------

/// Name of the server executable inside the package (all platforms; Windows
/// adds the `.exe` suffix below).
const SERVER_STEM: &str = "server";
/// Name of the client executable/bundle inside the package.
const CLIENT_STEM: &str = "ShadowScaleClient";
/// Name of the AI player executable inside the package, beside the server
/// (crate `sim_ai`; Windows adds the `.exe` suffix below).
const AI_STEM: &str = "sim_ai";

/// **The brain every rival seat plays on.** `sim_ai`'s own `--brain` default is
/// `pass`, a seat that assigns no labor and starves, because Pass is the
/// control the bench measures every other brain against — a default meant for
/// tests. The shipped game wants rivals that play, so the launcher names the
/// brain instead of inheriting that one. Contract twin of `BrainKind::as_str`
/// in `sim_ai/src/main.rs`.
const AI_BRAIN_DEFAULT: &str = "utility";
/// ⛔ **EVERY BRAIN `sim_ai` ACCEPTS, RESTATED — because a rejected one dies
/// invisibly.** `sim_ai`'s clap refuses an unknown `--brain`, but it does so
/// *after* the launcher has spawned it: `spawn()` succeeds, the child exits 2 in
/// milliseconds, the reconcile loop reaps the corpse, and a typo in
/// [`ENV_AI_BRAIN`] costs the run every rival with nothing said anywhere. So the
/// value is checked here, before anything starts. Contract twin of `BrainKind`
/// in `sim_ai/src/main.rs`: a variant added there is added here.
const AI_BRAIN_NAMES: [&str; 3] = ["pass", "scripted", "utility"];
/// Replaces [`AI_BRAIN_DEFAULT`] for one run — how a developer puts the rivals
/// back on `pass` (or on `scripted`) without a rebuild, the same way the server's
/// own levers are set (`core_sim/CLAUDE.md` → Environment Overrides). Empty or
/// unset is the default.
const ENV_AI_BRAIN: &str = "SIM_AI_BRAIN";

// ---------------------------------------------------------------------------
// The run directory: what a played game leaves behind for the viewer
// ---------------------------------------------------------------------------

/// Under the app-data root: one directory per launcher session, so a game just played can be
/// opened in `sim_ai viewer` afterwards — the rivals' own logs and the server's record of every
/// seat, the human's included (`.claude/rules/core_sim/ai-driver.md` → the run viewer).
const RUNS_DIR: &str = "runs";
/// A run id's prefix; the rest is the launcher's start time in Unix seconds and its pid, so ids
/// sort by time and two launchers started in the same second do not share one.
const RUN_ID_PREFIX: &str = "run-";
/// How many run directories survive a launcher start, this session's included. Older ones are
/// removed, oldest first: a run holds every frame of every seat, and a machine that plays daily
/// would otherwise fill up with games nobody will look at again.
const KEPT_RUNS: usize = 5;
/// The server's record under a run directory. Contract twin of `RECORD_DIR` in
/// `sim_ai/src/viewer/mod.rs`, which is what finds it again.
const RECORD_DIR: &str = "record";
/// The environment variable the server records under when set. Contract twin of
/// `core_sim::record::RECORD_DIR_ENV`.
const ENV_RECORD_DIR: &str = "SIM_RECORD_DIR";
/// A rival's `--log-dir` under the run directory: `seat_<faction>`, the prefix the bench, the
/// record and the viewer all use for a seat.
const SEAT_LOG_DIR_PREFIX: &str = "seat_";
/// `sim_ai`'s viewer subcommand (`VIEWER_SUBCOMMAND` in `sim_ai/src/main.rs`) and the page each
/// printed line writes beside the seat's directory: `<run dir>/seat_<f>.html`.
const VIEWER_SUBCOMMAND: &str = "viewer";
const VIEWER_PAGE_EXTENSION: &str = ".html";
/// How the printed viewer lines are introduced, at start and at exit.
const VIEWER_LINES_LABEL_START: &str = "after quitting, open this run with:";
const VIEWER_LINES_LABEL_EXIT: &str = "open this run with:";

/// **The human's faction.** Contract twin of `PLAYER_FACTION_ID` in
/// `clients/godot_thin_client/src/scripts/ui/hud/hud_const.gd`: the seat the
/// Godot client claims, and therefore the one roster entry that never gets a
/// `sim_ai` child.
const HUMAN_FACTION_ID: u32 = 0;

/// The roster event's identity on the log stream. Contract twin of the
/// `seats.roster` event `retain_claimed_seats` emits in
/// `core_sim/src/bin/server.rs`, whose doc comment states the shape.
const ROSTER_EVENT_TARGET: &str = "shadow_scale::server";
const ROSTER_EVENT_MESSAGE: &str = "seats.roster";
/// Its two fields: the faction ids as a JSON array in a string, and the world
/// build they belong to.
const ROSTER_FIELD_FACTIONS: &str = "factions";
const ROSTER_FIELD_WORLD_EPOCH: &str = "world_epoch";
/// The handshake file's keys the supervisor reads to reach the log stream.
const PORTS_KEY_HOST: &str = "host";
const PORTS_KEY_LOG: &str = "log";
/// A log-stream frame is `[u32 LE length][JSON]` (`core_sim/src/log_stream.rs`).
const LOG_FRAME_PREFIX_BYTES: usize = std::mem::size_of::<u32>();
/// A log line longer than this is not a log line.
const MAX_LOG_FRAME: usize = 1024 * 1024;
/// How long the supervisor waits before redialling a dropped log stream.
const LOG_RECONNECT_BACKOFF: Duration = Duration::from_secs(1);
/// How often the session checks on the human's process between roster events.
const HUMAN_EXIT_POLL: Duration = Duration::from_millis(250);

/// How the launcher's messages name the simulation server when reporting a
/// failure that is about the *process* rather than the package layout.
const SERVER_LABEL: &str = "the server";
/// How they name the human's player program. One constant because the same
/// phrase has to read naturally in "Could not find …", "Could not start …" and
/// "Could not add … to the process group".
const HUMAN_PLAYER_LABEL: &str = "the game";
/// …and a rival's.
const AI_PLAYER_LABEL: &str = "a rival player";

/// On macOS both children live in `ShadowScale.app/Contents/Helpers/`, one level
/// up from the launcher's own `Contents/MacOS/`.
#[cfg(target_os = "macos")]
const MACOS_HELPERS_DIR: &str = "Helpers";
/// Path inside a macOS `.app` bundle holding its executable.
#[cfg(target_os = "macos")]
const MACOS_BUNDLE_EXEC_DIR: &str = "Contents/MacOS";

fn main() {
    if let Err(message) = run() {
        report_error(&message);
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let exe_dir = current_exe_dir()?;
    let layout = Layout::resolve(&exe_dir)?;

    // Everything the server writes relative to its CWD (notably `export_map`'s
    // `exports/map-tick<N>-seed<M>.json`) lands here. A process launched from
    // inside a .app bundle inherits `/` as its CWD, where those writes would
    // either fail or land somewhere the player can never find.
    let data_dir = app_data_dir()?;
    fs::create_dir_all(&data_dir)
        .map_err(|err| format!("Could not create {}: {err}", data_dir.display()))?;

    let ports_file = ports_file_path(&data_dir);
    // Still required even though the name carries our pid: PIDs are recycled, so
    // a crashed run can have left a stale file at exactly this path, and it would
    // satisfy the readiness wait instantly and hand the client dead ports.
    remove_ports_file(&ports_file);

    // Belt and braces on Windows: the guard below covers orderly exits, the job
    // object covers this process being killed outright. It is created before the
    // first spawn so that every child — server and seats alike — joins it the
    // instant it exists.
    // Before anything is spawned: a `SIM_AI_BRAIN` the AI would refuse is a run
    // with no rivals at all, so it stops here, with the offending value named.
    let rival_brain = rival_brain()?;

    let group = ProcessGroup::kill_on_close()?;

    // This session's run directory, pruned so only the last few games are kept. Its path is the
    // one thing a player needs to open the game they just played in the viewer, so it is printed
    // at start and again at exit.
    let run_dir = create_run_dir(&data_dir.join(RUNS_DIR), &mint_run_id())?;
    report_info(&format!("run directory: {}", run_dir.display()));
    // The human's line is known before anything runs; a rival's is printed the moment the
    // supervisor starts it, so a crash still leaves the whole recipe in the log.
    report_viewer_lines(
        VIEWER_LINES_LABEL_START,
        &viewer_lines(&layout.ai, &run_dir, &[HUMAN_FACTION_ID]),
    );

    let server = Command::new(&layout.server)
        .current_dir(&data_dir)
        .env(ENV_PORTS_FILE, &ports_file)
        .env(ENV_RECORD_DIR, run_dir.join(RECORD_DIR))
        .spawn()
        .map_err(|err| {
            format!(
                "Could not start the server:\n{}\n\n{err}",
                layout.server.display()
            )
        })?;

    // From here on every exit path must reap every child, so ownership moves
    // into a guard rather than being cleaned up at each `return`.
    let mut session = Session::new(server, ports_file.clone(), rival_brain, run_dir.clone());
    group.adopt(session.server(), SERVER_LABEL)?;

    wait_for_ready(&mut session, &ports_file)?;

    // The supervisor listens BEFORE the human's client starts: the boot world is
    // idle until that client asks for one, so every roster the server will ever
    // announce comes after this connection exists.
    let roster_events = spawn_roster_watcher(log_stream_addr(&ports_file)?);

    // The human's seat. The server never spawns players: in a real multiplayer
    // game the other seats are on other machines, so a server that spawned its
    // players would only work locally.
    session.fill_seat(&human_seat(&layout), &data_dir, &ports_file, &group)?;

    let outcome =
        session.wait_for_human(&roster_events, &layout.ai, &data_dir, &ports_file, &group);
    report_info(&format!("run directory: {}", run_dir.display()));
    report_viewer_lines(
        VIEWER_LINES_LABEL_EXIT,
        &viewer_lines(&layout.ai, &run_dir, &session.seats_of_run()),
    );
    outcome
}

/// **One paste-ready viewer command per seat**: `<sim_ai> viewer <run dir> --seat <f> --out
/// <run dir>/seat_<f>.html`, with the `sim_ai` the layout resolved for spawning rivals and the
/// run directory as created — both absolute — so a player copies a line and gets the page.
fn viewer_lines(sim_ai: &Path, run_dir: &Path, seats: &[u32]) -> Vec<String> {
    seats
        .iter()
        .map(|seat| {
            format!(
                "{} {VIEWER_SUBCOMMAND} {} --seat {seat} --out {}",
                sim_ai.display(),
                run_dir.display(),
                run_dir
                    .join(format!(
                        "{SEAT_LOG_DIR_PREFIX}{seat}{VIEWER_PAGE_EXTENSION}"
                    ))
                    .display()
            )
        })
        .collect()
}

/// Print the viewer lines under `label`, one per line, at the launcher's ordinary verbosity.
fn report_viewer_lines(label: &str, lines: &[String]) {
    report_info(label);
    for line in lines {
        eprintln!("  {line}");
    }
}

/// This launcher session's run id: [`RUN_ID_PREFIX`], the start time in Unix seconds, the pid.
fn mint_run_id() -> String {
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    format!("{RUN_ID_PREFIX}{started}-{}", std::process::id())
}

/// Create `<runs_dir>/<run_id>` and prune the runs directory to [`KEPT_RUNS`] entries, the new
/// one included. Pruning is best-effort: a run that cannot be removed is left, never reported.
fn create_run_dir(runs_dir: &Path, run_id: &str) -> Result<PathBuf, String> {
    let run_dir = runs_dir.join(run_id);
    fs::create_dir_all(&run_dir)
        .map_err(|err| format!("Could not create {}: {err}", run_dir.display()))?;
    prune_runs(runs_dir, KEPT_RUNS);
    Ok(run_dir)
}

/// Remove every run directory under `runs_dir` but the newest `keep`, by name — a run id sorts by
/// its start time. Only directories carrying [`RUN_ID_PREFIX`] are touched.
fn prune_runs(runs_dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(runs_dir) else {
        return;
    };
    let mut runs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(RUN_ID_PREFIX))
        })
        .collect();
    runs.sort();
    let stale = runs.len().saturating_sub(keep);
    for run in runs.into_iter().take(stale) {
        let _ = fs::remove_dir_all(run);
    }
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Absolute paths to the three child programs in the installed package.
struct Layout {
    server: PathBuf,
    client: PathBuf,
    /// The AI player program, staged beside the server.
    ai: PathBuf,
}

impl Layout {
    fn resolve(exe_dir: &Path) -> Result<Self, String> {
        let layout = Self::platform_layout(exe_dir)?;
        require_file(&layout.server, "the simulation server")?;
        require_file(&layout.client, HUMAN_PLAYER_LABEL)?;
        require_file(&layout.ai, AI_PLAYER_LABEL)?;
        Ok(layout)
    }

    /// macOS: the launcher is `ShadowScale.app/Contents/MacOS/shadowscale_launcher`
    /// and the two children are staged as helpers in `Contents/Helpers/`, which
    /// keeps a single icon in Finder and a single code-signed bundle.
    #[cfg(target_os = "macos")]
    fn platform_layout(exe_dir: &Path) -> Result<Self, String> {
        let helpers = exe_dir
            .parent()
            .ok_or_else(|| unexpected_layout(exe_dir))?
            .join(MACOS_HELPERS_DIR);
        let bundle = helpers.join(format!("{CLIENT_STEM}.app"));
        Ok(Self {
            server: helpers.join(SERVER_STEM),
            client: macos_bundle_executable(&bundle)?,
            ai: helpers.join(AI_STEM),
        })
    }

    /// Windows: a flat package directory, the launcher sitting beside both
    /// children as `ShadowScale.exe`.
    #[cfg(windows)]
    fn platform_layout(exe_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            server: exe_dir.join(format!("{SERVER_STEM}.exe")),
            client: exe_dir.join(format!("{CLIENT_STEM}.exe")),
            ai: exe_dir.join(format!("{AI_STEM}.exe")),
        })
    }

    /// Linux and anything else: a flat package directory. Not shipped today,
    /// but the layout the packaging script would produce if it were.
    #[cfg(not(any(target_os = "macos", windows)))]
    fn platform_layout(exe_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            server: exe_dir.join(SERVER_STEM),
            client: exe_dir.join(CLIENT_STEM),
            ai: exe_dir.join(AI_STEM),
        })
    }
}

/// The executable inside a macOS `.app`, found by enumerating
/// `Contents/MacOS/`.
///
/// Godot names that binary after the *project*, not after the export filename,
/// so it cannot be hardcoded. The old `run.command` shelled out to PlistBuddy to
/// read `CFBundleExecutable`; reading the directory gets the same answer without
/// this crate needing a plist parser, because a Godot export puts exactly one
/// executable there.
#[cfg(target_os = "macos")]
fn macos_bundle_executable(bundle: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    /// Any execute bit (user/group/other) marks the bundle's binary.
    const EXEC_BITS: u32 = 0o111;

    let exec_dir = bundle.join(MACOS_BUNDLE_EXEC_DIR);
    let entries = fs::read_dir(&exec_dir).map_err(|err| {
        format!(
            "Could not look inside the game app:\n{}\n\n{err}\n\n{UNZIP_HINT}",
            exec_dir.display()
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_file() && meta.permissions().mode() & EXEC_BITS != 0 {
            return Ok(path);
        }
    }

    Err(format!(
        "The game app contains no runnable program:\n{}\n\n{UNZIP_HINT}",
        exec_dir.display()
    ))
}

/// Shared tail for "a file we expected in the package isn't there" errors. By
/// far the most common cause is a partially expanded ZIP.
const UNZIP_HINT: &str = "The package may not be fully unzipped — expand the ZIP again and \
                          launch from the expanded folder (not from inside the ZIP).";

fn require_file(path: &Path, what: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!(
            "Could not find {what}:\n{}\n\n{UNZIP_HINT}",
            path.display()
        ))
    }
}

#[cfg(target_os = "macos")]
fn unexpected_layout(exe_dir: &Path) -> String {
    format!(
        "ShadowScale is not installed as expected:\n{}\n\n{UNZIP_HINT}",
        exe_dir.display()
    )
}

fn current_exe_dir() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|err| format!("Could not locate the ShadowScale program: {err}"))?;
    // Canonicalize so a symlinked launcher still resolves the package layout
    // relative to the real file.
    let exe = exe.canonicalize().unwrap_or(exe);
    exe.parent().map(Path::to_path_buf).ok_or_else(|| {
        format!(
            "ShadowScale is not installed in a folder: {}",
            exe.display()
        )
    })
}

/// Per-user app-data root, used for both the handshake file and the children's
/// working directory. Mirrors the derivation in
/// `core_sim::port_alloc::ports_file_path` (never the temp dir — antivirus
/// heuristics there are aggressive).
fn app_data_dir() -> Result<PathBuf, String> {
    let root: PathBuf = if cfg!(windows) {
        env_path(ENV_LOCALAPPDATA)?
    } else if cfg!(target_os = "macos") {
        env_path(ENV_HOME)?.join(MACOS_APP_SUPPORT)
    } else if let Some(state) = std::env::var_os(ENV_XDG_STATE_HOME) {
        PathBuf::from(state)
    } else {
        env_path(ENV_HOME)?.join(LINUX_STATE_FALLBACK)
    };
    Ok(root.join(APP_DATA_DIR))
}

/// This run's handshake path: the shared app-data directory plus a file name
/// carrying our own process id, so concurrent launchers never touch each other's
/// file (neither the startup cleanup nor [`Session::drop`]).
///
/// Accepted tradeoff, decided rather than overlooked: a launcher killed with
/// SIGKILL / `taskkill /f` leaves its ~100-byte file behind, where the old fixed
/// name self-limited the litter to one. This crate deliberately does *not* sweep
/// orphaned `launcher-ports-*.json` files — a sweep would have to distinguish a
/// dead pid from a live one on two platforms to avoid deleting a healthy
/// concurrent run's handshake, which is far more machinery than the leak costs.
fn ports_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(format!(
        "{PORTS_FILE_PREFIX}{}{PORTS_FILE_EXTENSION}",
        std::process::id()
    ))
}

fn env_path(key: &str) -> Result<PathBuf, String> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("The {key} environment variable is not set, so ShadowScale cannot find a place to store its files."))
}

// ---------------------------------------------------------------------------
// Locally-hosted seats
// ---------------------------------------------------------------------------

/// A faction seat this machine fills, and the program that fills it.
///
/// A seat is the sim's entire model of "who is playing": occupied by whatever
/// connection claimed it, or vacant. Nothing downstream of the socket knows what
/// kind of program is on the other end, so the launcher's job is only to start
/// one process per seat *this machine* hosts and keep them alive together.
struct LocalSeat<'a> {
    /// Program run to fill the seat.
    program: &'a Path,
    /// How failures involving this process are phrased to the player.
    what: &'a str,
}

/// The human at this keyboard, in the Godot client. The client claims its own
/// seat over the command socket at handshake (`SeatClaim.gd`), so the launcher
/// passes no seat identity. Rival seats are filled by [`Session::reconcile`] as
/// the server announces them.
fn human_seat(layout: &Layout) -> LocalSeat<'_> {
    LocalSeat {
        program: &layout.client,
        what: HUMAN_PLAYER_LABEL,
    }
}

// ---------------------------------------------------------------------------
// The seat supervisor
// ---------------------------------------------------------------------------

/// One `seats.roster` event: which factions the world now seats.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RosterEvent {
    factions: Vec<u32>,
    world_epoch: u64,
}

/// A line that **is** the roster event and whose fields the launcher could not
/// read: the contract broke, and that is a different thing from a line about
/// something else.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RosterEventRejected {
    /// Which clause of the contract failed, in the launcher's words.
    reason: &'static str,
    /// The frame verbatim, so the mismatch is legible rather than described.
    payload: String,
}

impl RosterEventRejected {
    fn message(&self) -> String {
        format!(
            "the server's {ROSTER_EVENT_MESSAGE} event {}, so no rival players could be started \
             for this world: {}",
            self.reason, self.payload
        )
    }
}

/// The `seats.roster` event, if this log line is one. The contract twin of
/// `core_sim::log_stream::emit_seats_roster`.
///
/// ⛔ **THREE ANSWERS, NOT TWO.** `None` is *some other line* — the log stream
/// carries every event the server emits, and skipping them is the normal case.
/// `Some(Err(_))` is this line **being** the roster event and failing to parse,
/// which is the launcher's whole contract broken and must never be swallowed as
/// though it were one more line about something else: changing the event's
/// `factions` field from `%` to `?` is enough to produce it, and the packaged
/// game then ships with zero rivals and zero diagnostics.
fn parse_roster_event(line: &[u8]) -> Option<Result<RosterEvent, RosterEventRejected>> {
    let envelope: serde_json::Value = serde_json::from_slice(line).ok()?;
    if envelope.get("target")?.as_str()? != ROSTER_EVENT_TARGET
        || envelope.get("message")?.as_str()? != ROSTER_EVENT_MESSAGE
    {
        return None;
    }
    Some(
        roster_fields(&envelope).map_err(|reason| RosterEventRejected {
            reason,
            payload: String::from_utf8_lossy(line).into_owned(),
        }),
    )
}

/// The roster event's two fields, or the clause of the contract that failed.
fn roster_fields(envelope: &serde_json::Value) -> Result<RosterEvent, &'static str> {
    let fields = envelope.get("fields").ok_or("carries no `fields` object")?;
    let factions = fields
        .get(ROSTER_FIELD_FACTIONS)
        .and_then(serde_json::Value::as_str)
        .ok_or("names no `factions` string (a tracing field recorded with `?` quotes itself)")?;
    let factions: Vec<u32> = serde_json::from_str(factions)
        .map_err(|_| "carries a `factions` string that is not a JSON array of faction ids")?;
    let world_epoch = fields
        .get(ROSTER_FIELD_WORLD_EPOCH)
        .and_then(serde_json::Value::as_u64)
        .ok_or("names no numeric `world_epoch`")?;
    Ok(RosterEvent {
        factions,
        world_epoch,
    })
}

/// Where the server's log stream is, off the handshake file the readiness wait
/// already confirmed complete.
fn log_stream_addr(ports_file: &Path) -> Result<SocketAddr, String> {
    let text = fs::read_to_string(ports_file)
        .map_err(|err| format!("Could not read the server's ports file: {err}"))?;
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("The server's ports file is not JSON: {err}"))?;
    let host = json
        .get(PORTS_KEY_HOST)
        .and_then(serde_json::Value::as_str)
        .and_then(|host| host.parse().ok())
        .ok_or_else(|| "The server's ports file names no host.".to_string())?;
    let port = json
        .get(PORTS_KEY_LOG)
        .and_then(serde_json::Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .ok_or_else(|| "The server's ports file names no log port.".to_string())?;
    Ok(SocketAddr::new(host, port))
}

/// Read the server's log stream on its own thread, forwarding every roster
/// event. A dropped stream is redialled; the thread ends when the session stops
/// listening (the receiver is dropped).
fn spawn_roster_watcher(addr: SocketAddr) -> Receiver<RosterEvent> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("seat-supervisor".into())
        .spawn(move || watch_roster(addr, sender))
        .expect("spawn the seat supervisor thread");
    receiver
}

/// A dropped stream is only ever redialled — never reported as an empty roster,
/// which would reap every rival over a hiccup on the log socket. The thread ends
/// with the process, or when the session stops listening.
fn watch_roster(addr: SocketAddr, sender: Sender<RosterEvent>) {
    loop {
        if let Ok(mut stream) = TcpStream::connect(addr) {
            while let Some(line) = read_log_frame(&mut stream) {
                match parse_roster_event(&line) {
                    Some(Ok(event)) => {
                        if sender.send(event).is_err() {
                            return;
                        }
                    }
                    Some(Err(rejected)) => report_warning(&rejected.message()),
                    None => {}
                }
            }
        }
        thread::sleep(LOG_RECONNECT_BACKOFF);
    }
}

/// One `[u32 LE length][JSON]` frame off the log stream, or `None` on EOF or a
/// frame that cannot be a log line.
///
/// Generic over the reader only so the contract test can feed it bytes the
/// server's own encoder produced, rather than a socket.
fn read_log_frame<R: Read>(stream: &mut R) -> Option<Vec<u8>> {
    let mut prefix = [0u8; LOG_FRAME_PREFIX_BYTES];
    stream.read_exact(&mut prefix).ok()?;
    let len = u32::from_le_bytes(prefix) as usize;
    if len == 0 || len > MAX_LOG_FRAME {
        return None;
    }
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).ok()?;
    Some(payload)
}

// ---------------------------------------------------------------------------
// Readiness
// ---------------------------------------------------------------------------

/// Keys the client reads out of the handshake file, and therefore the definition
/// of a *complete* one. Contract twin of `KEY_HOST` / `KEY_COMMAND` / `KEY_LOG` /
/// `KEY_SNAPSHOT_FLAT` in
/// `clients/godot_thin_client/src/scripts/ServerPortsFile.gd`.
///
/// Note the stream port is `snapshot_flat`, and it is now the **only** snapshot
/// port: the legacy bincode socket that published a `snapshot` key was retired
/// in #388 and its slot (`base+0`) reserved, so that key no longer exists and
/// requiring it here would hang the launcher forever. Any change here must move
/// in lockstep with that script.
const REQUIRED_PORTS_KEYS: [&str; 4] = ["host", "command", "log", "snapshot_flat"];

/// Blocks until the server publishes a *complete* handshake file, it exits, or
/// [`READY_TIMEOUT`] elapses.
///
/// That file is the only signal that the server has actually bound its ports (it
/// is written immediately after an all-or-nothing bind), which is why this
/// replaces the fixed `sleep 2` both launch scripts used.
///
/// Existence alone is not enough to hand off on, though. `core_sim::port_alloc::
/// write_ports_file_at` uses `fs::write`, i.e. create + truncate + write, so an
/// empty or half-written file is briefly observable. The window is tiny, but
/// losing that race fails *silently*: `ServerPortsFile.gd` degrades a failed
/// parse to an empty dict and falls back to the hardcoded 41000 block, dialling a
/// dead port with no error anywhere. So the gate is "parses as an object carrying
/// every key the client reads" instead.
fn wait_for_ready(session: &mut Session, ports_file: &Path) -> Result<(), String> {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if ports_file_complete(ports_file) {
            return Ok(());
        }
        // A server that already died will never write the file; report why now
        // instead of making the player wait out the whole timeout.
        match session.server().try_wait() {
            Ok(Some(status)) => return Err(server_exit_message(status.code())),
            Ok(None) => {}
            Err(err) => return Err(format!("Lost track of the server process: {err}")),
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "The simulation server did not finish starting within {} seconds, so ShadowScale \
                 could not launch.",
                READY_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(READY_POLL_INTERVAL);
    }
}

/// Whether the handshake file is readable, parseable, and carries every key in
/// [`REQUIRED_PORTS_KEYS`].
///
/// Every failure mode here — missing, unreadable, truncated mid-write, missing a
/// key — means the same thing to the caller: *not ready yet*. None is fatal on
/// its own; only [`READY_TIMEOUT`] elapsing is an error, so this returns a plain
/// bool rather than a `Result` the poll loop would only discard.
fn ports_file_complete(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(serde_json::Value::Object(entries)) = serde_json::from_str::<serde_json::Value>(&text)
    else {
        return false;
    };
    REQUIRED_PORTS_KEYS
        .iter()
        .all(|key| entries.contains_key(*key))
}

fn server_exit_message(code: Option<i32>) -> String {
    match code {
        Some(SERVER_PORT_ALLOC_EXIT_CODE) => "The simulation server could not bind its local \
             ports; another copy of ShadowScale may already be running. Close it and try again."
            .to_string(),
        Some(code) => format!("The simulation server stopped unexpectedly (exit code {code})."),
        None => "The simulation server stopped unexpectedly.".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Shutdown
// ---------------------------------------------------------------------------

/// Owns every child the launcher started — the server and one process per
/// locally-hosted seat — for the rest of the launcher's life.
///
/// Cleanup lives in `Drop` rather than at each `return` so that every failure
/// path after the first spawn — readiness timeout, a seat's program failing to
/// start, or an ordinary quit — reaps **all** of them and clears the handshake
/// file exactly once. A child that escaped this guard would outlive the launcher
/// holding the ports, which is the orphaned-server bug this binary exists to
/// prevent; so a player process is only ever spawned through [`fill_seat`],
/// which moves it in here in the same expression that starts it.
///
/// [`fill_seat`]: Session::fill_seat
struct Session {
    server: Child,
    /// One per filled seat, in start order — the human's client.
    players: Vec<Child>,
    /// One `sim_ai` per rival faction the server's roster names, keyed by faction.
    rivals: Vec<(u32, Child)>,
    ports_file: PathBuf,
    /// The `--brain` every rival is spawned on, resolved and **validated** before
    /// this session existed ([`rival_brain`]).
    rival_brain: String,
    /// This session's run directory: every rival's `--log-dir` is `seat_<faction>` under it.
    run_dir: PathBuf,
    /// Every rival faction the supervisor ever started this session, in first-start order —
    /// the seats whose pages the exit lines name, whether or not the child is still running.
    started_rivals: Vec<u32>,
}

impl Session {
    fn new(server: Child, ports_file: PathBuf, rival_brain: String, run_dir: PathBuf) -> Self {
        Self {
            server,
            players: Vec::new(),
            rivals: Vec::new(),
            ports_file,
            rival_brain,
            run_dir,
            started_rivals: Vec::new(),
        }
    }

    /// The seats this run can be viewed for: the human's, then every rival ever started.
    fn seats_of_run(&self) -> Vec<u32> {
        let mut seats = vec![HUMAN_FACTION_ID];
        seats.extend(self.started_rivals.iter().copied());
        seats
    }

    /// Where a rival's instruments go: `<run_dir>/seat_<faction>`.
    fn rival_log_dir(&self, faction: u32) -> PathBuf {
        self.run_dir.join(format!("{SEAT_LOG_DIR_PREFIX}{faction}"))
    }

    fn server(&mut self) -> &mut Child {
        &mut self.server
    }

    /// Starts `seat`'s program, takes ownership of it, and puts it in the
    /// process group.
    ///
    /// Ownership transfers before the group call so that a job-object failure
    /// still leaves the process reaped by [`Session::drop`].
    fn fill_seat(
        &mut self,
        seat: &LocalSeat<'_>,
        data_dir: &Path,
        ports_file: &Path,
        group: &ProcessGroup,
    ) -> Result<(), String> {
        let player = Command::new(seat.program)
            .current_dir(data_dir)
            .env(ENV_PORTS_FILE, ports_file)
            // A player program is a GUI app or a headless bot; inheriting stdio
            // is harmless but noisy when the launcher is run from a terminal,
            // and meaningless otherwise.
            .stdin(Stdio::null())
            .spawn()
            .map_err(|err| {
                format!(
                    "Could not start {}:\n{}\n\n{err}",
                    seat.what,
                    seat.program.display()
                )
            })?;
        group.adopt(self.adopt_player(player), seat.what)
    }

    /// Takes ownership of an already-started player process, and hands back the
    /// borrow the caller needs to finish setting it up.
    fn adopt_player(&mut self, player: Child) -> &mut Child {
        self.players.push(player);
        self.players
            .last_mut()
            .expect("the player just pushed is the last one")
    }

    /// **The human's client owns the session window.** Blocks until it exits,
    /// reconciling the rival seats against every roster event that arrives in
    /// the meantime; the launcher then returns, and `Drop` reaps every AI child
    /// and the server behind it. An AI child exiting on its own does not end
    /// the run — its seat goes vacant, and the next roster event may respawn it.
    fn wait_for_human(
        &mut self,
        roster_events: &Receiver<RosterEvent>,
        ai_program: &Path,
        data_dir: &Path,
        ports_file: &Path,
        group: &ProcessGroup,
    ) -> Result<(), String> {
        loop {
            match self.human().try_wait() {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {}
                Err(err) => return Err(format!("Lost track of the game process: {err}")),
            }
            match roster_events.recv_timeout(HUMAN_EXIT_POLL) {
                Ok(event) => {
                    self.reconcile(&event.factions, ai_program, data_dir, ports_file, group)?;
                }
                Err(RecvTimeoutError::Timeout) => {}
                // The supervisor thread is gone; the human's exit is still the
                // only thing that ends the run.
                Err(RecvTimeoutError::Disconnected) => thread::sleep(HUMAN_EXIT_POLL),
            }
        }
    }

    /// The human's client: the first seat filled.
    fn human(&mut self) -> &mut Child {
        self.players
            .first_mut()
            .expect("the human's seat is filled before the session waits on it")
    }

    /// Bring the rival children into line with `roster`: spawn one per rival
    /// faction with no running child, reap every child whose faction is not
    /// named. A child that exited on its own is dropped here and respawned by
    /// this event if its faction is still in the roster — one respawn per
    /// roster event, never a tight loop.
    fn reconcile(
        &mut self,
        roster: &[u32],
        ai_program: &Path,
        data_dir: &Path,
        ports_file: &Path,
        group: &ProcessGroup,
    ) -> Result<(), String> {
        let mut kept = Vec::with_capacity(self.rivals.len());
        for (faction, mut child) in self.rivals.drain(..) {
            // ⛔ **A RIVAL THAT DIED SAYS SO.** Reaping in silence covered every
            // way this seat can fail at startup — a `--brain` clap refuses, a seat
            // claim the server will not grant, a panic — and roster events fire
            // only on a world build, so within one world the seat then stays empty
            // for the rest of the session with nothing anywhere to read.
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        report_warning(&format!(
                            "{AI_PLAYER_LABEL} on seat {faction} exited ({status}); that seat is \
                             unplayed until the next world build."
                        ));
                    }
                    continue;
                }
                Ok(None) => {}
                Err(err) => {
                    report_warning(&format!(
                        "Lost track of {AI_PLAYER_LABEL} on seat {faction}: {err}"
                    ));
                    continue;
                }
            }
            if roster.contains(&faction) {
                kept.push((faction, child));
            } else {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        self.rivals = kept;
        for &faction in roster {
            if faction == HUMAN_FACTION_ID || self.rivals.iter().any(|(held, _)| *held == faction) {
                continue;
            }
            self.spawn_rival(faction, ai_program, data_dir, ports_file, group)?;
        }
        Ok(())
    }

    /// Start one `sim_ai` for `faction`, take ownership of it, and put it in the
    /// process group — the same ordering [`Session::fill_seat`] follows.
    fn spawn_rival(
        &mut self,
        faction: u32,
        ai_program: &Path,
        data_dir: &Path,
        ports_file: &Path,
        group: &ProcessGroup,
    ) -> Result<(), String> {
        let child = Command::new(ai_program)
            .arg("--ports-file")
            .arg(ports_file)
            .arg("--faction")
            .arg(faction.to_string())
            .arg("--brain")
            .arg(&self.rival_brain)
            .arg("--log-dir")
            .arg(self.rival_log_dir(faction))
            .current_dir(data_dir)
            .env(ENV_PORTS_FILE, ports_file)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|err| {
                format!(
                    "Could not start {AI_PLAYER_LABEL}:\n{}\n\n{err}",
                    ai_program.display()
                )
            })?;
        self.rivals.push((faction, child));
        if !self.started_rivals.contains(&faction) {
            self.started_rivals.push(faction);
            report_viewer_lines(
                VIEWER_LINES_LABEL_START,
                &viewer_lines(ai_program, &self.run_dir, &[faction]),
            );
        }
        let (_, child) = self
            .rivals
            .last_mut()
            .expect("the rival just pushed is the last one");
        group.adopt(child, AI_PLAYER_LABEL)
    }

    /// The factions with a running rival child, in spawn order.
    #[cfg(test)]
    fn rival_factions(&self) -> Vec<u32> {
        self.rivals.iter().map(|(faction, _)| *faction).collect()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Players before the server: a player outliving the sim it was talking
        // to would spend its last moments reporting a dropped connection, which
        // is a confusing thing to show on the way out of a clean quit.
        //
        // Every call is best-effort — an already-exited child makes `kill` fail,
        // which is exactly the state we want anyway — and none may short-circuit
        // the rest, because a stop halfway down this list is an orphan.
        for (_, rival) in &mut self.rivals {
            let _ = rival.kill();
            let _ = rival.wait();
        }
        for player in &mut self.players {
            let _ = player.kill();
            let _ = player.wait();
        }
        let _ = self.server.kill();
        let _ = self.server.wait();
        remove_ports_file(&self.ports_file);
    }
}

/// The brain every rival is spawned on: `$SIM_AI_BRAIN` when it is set to
/// something, else [`AI_BRAIN_DEFAULT`]. Resolved once, before the server
/// starts, so a rejected value stops the launch loudly instead of costing every
/// rival silently.
fn rival_brain() -> Result<String, String> {
    brain_from_override(std::env::var(ENV_AI_BRAIN).ok())
}

/// [`rival_brain`]'s rule, apart from the environment so it can be tested
/// without one: an override is honoured only when it carries a value, and only
/// when that value names a brain [`AI_BRAIN_NAMES`] holds.
fn brain_from_override(override_value: Option<String>) -> Result<String, String> {
    let Some(named) = override_value.filter(|brain| !brain.trim().is_empty()) else {
        return Ok(AI_BRAIN_DEFAULT.to_owned());
    };
    let named = named.trim().to_owned();
    if AI_BRAIN_NAMES.contains(&named.as_str()) {
        return Ok(named);
    }
    Err(format!(
        "{ENV_AI_BRAIN} is set to `{named}`, which is not a brain {AI_STEM} plays. The choices \
         are {}.",
        AI_BRAIN_NAMES.join(", ")
    ))
}

/// Deletes the handshake file if present, ignoring failure — a stale file is an
/// annoyance, a launcher that refuses to start or stop over one is a bug.
fn remove_ports_file(path: &Path) {
    let _ = fs::remove_file(path);
}

// ---------------------------------------------------------------------------
// Process group (Windows job object)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod process_group {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A Windows Job Object configured to kill its members when the last handle
    /// to it closes.
    ///
    /// This is the correctness win over `run.bat`, whose `taskkill` only ran on
    /// the clean exit path: because closing the handle is what triggers the
    /// kill, and the OS closes every handle of a dying process, the members are
    /// reaped even if the launcher is force-killed or crashes. The handle must
    /// therefore stay alive for the launcher's entire run — dropping this value
    /// early kills them.
    ///
    /// **Membership is per child and explicit.** The group is created empty and
    /// each child joins it through [`ProcessGroup::adopt`] right after it is
    /// spawned, because job membership is not inherited from the launcher: a
    /// seat's process left out of the group would survive a force-kill of the
    /// launcher exactly the way the orphaned server used to.
    pub struct ProcessGroup {
        handle: HANDLE,
    }

    impl ProcessGroup {
        /// Creates the (empty) kill-on-close group.
        pub fn kill_on_close() -> Result<Self, String> {
            // SAFETY: both calls take either null (default security attributes /
            // unnamed job) or pointers to locals that outlive the call, and
            // every result is checked before use.
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(last_error("create a process group"));
                }
                let group = Self { handle };

                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    group.handle,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(limits).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    return Err(last_error("configure the process group"));
                }
                Ok(group)
            }
        }

        /// Adds one running child to the group. `what` names it the way the
        /// player would ("the server", "the game").
        pub fn adopt(&self, child: &mut Child, what: &str) -> Result<(), String> {
            // SAFETY: `handle` is a live job object created above, and the raw
            // handle belongs to a `Child` the caller still owns, so it outlives
            // the call.
            unsafe {
                if AssignProcessToJobObject(self.handle, child.as_raw_handle() as HANDLE) == 0 {
                    return Err(last_error(&format!("add {what} to the process group")));
                }
            }
            Ok(())
        }
    }

    impl Drop for ProcessGroup {
        fn drop(&mut self) {
            // SAFETY: `handle` was created by `CreateJobObjectW` and is closed
            // exactly once, here.
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }

    fn last_error(action: &str) -> String {
        let code = std::io::Error::last_os_error();
        format!("ShadowScale could not {action}: {code}")
    }
}

/// Non-Windows platforms have no job objects; the `Drop`-based [`Session`]
/// cleanup is the whole story there. A no-op stand-in keeps the call site in
/// `run` free of `cfg` branches.
#[cfg(not(windows))]
mod process_group {
    use std::process::Child;

    pub struct ProcessGroup;

    impl ProcessGroup {
        pub fn kill_on_close() -> Result<Self, String> {
            Ok(Self)
        }

        pub fn adopt(&self, _child: &mut Child, _what: &str) -> Result<(), String> {
            Ok(())
        }
    }
}

use process_group::ProcessGroup;

// ---------------------------------------------------------------------------
// Error reporting
// ---------------------------------------------------------------------------

/// Title of the error dialog on every platform.
const ERROR_DIALOG_TITLE: &str = "ShadowScale";

/// Shows `message` where the player will actually see it.
///
/// The packaged builds are launched from Finder / Explorer with no console
/// attached (see the `windows_subsystem` attribute above), so stderr alone would
/// silently swallow every startup failure. The `eprintln!` is still emitted so a
/// terminal run — how a developer reproduces the failure — shows the same text.
fn report_error(message: &str) {
    eprintln!("{ERROR_DIALOG_TITLE}: {message}");
    show_error_dialog(message);
}

/// A problem the run **survives**, said out loud rather than swallowed.
///
/// No dialog: these arrive from the supervisor thread while the player is in the
/// game, and a modal box per malformed log line would be worse than the fault it
/// reports. `stderr` is the launcher's one diagnostic channel — it reaches a
/// developer running the package from a terminal, and the packaged Windows build
/// has no console at all, which is exactly why the swallowing was invisible.
fn report_warning(message: &str) {
    eprintln!("{ERROR_DIALOG_TITLE}: warning: {message}");
}

/// A line for the launcher's own log — where the run directory is, at start and at exit. Stderr,
/// like the warnings: there is no console on the packaged Windows build, and a dialog for a path
/// would be worse than none.
fn report_info(message: &str) {
    eprintln!("{ERROR_DIALOG_TITLE}: {message}");
}

#[cfg(windows)]
fn show_error_dialog(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let text = wide(message);
    let title = wide(ERROR_DIALOG_TITLE);
    // SAFETY: both buffers are NUL-terminated and outlive the call; a null
    // owner window makes the box application-modal, which is what we want.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// UTF-16, NUL-terminated, as the `W` Win32 entry points require.
#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// macOS has no console when launched from Finder either, so the message goes
/// through AppleScript's stock alert. Shelling out to `osascript` avoids linking
/// any UI framework into what is otherwise a tiny supervisor.
#[cfg(target_os = "macos")]
fn show_error_dialog(message: &str) {
    /// AppleScript expression showing a stop-icon alert with a single button.
    /// `{message}` is substituted with the escaped text.
    const DIALOG_SCRIPT: &str = r#"display dialog "{message}" with title "{title}" with icon stop buttons {"OK"} default button "OK""#;

    let script = DIALOG_SCRIPT
        .replace("{message}", &applescript_escape(message))
        .replace("{title}", ERROR_DIALOG_TITLE);
    let mut command = Command::new("osascript");
    command.arg("-e").arg(script);
    // Best-effort: if osascript is missing or the user dismisses it oddly, the
    // `eprintln!` in `report_error` is still the fallback.
    let _ = command.status();
}

/// Escapes the two characters that would otherwise break out of an AppleScript
/// string literal.
#[cfg(target_os = "macos")]
fn applescript_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// No standard dialog tool to rely on elsewhere; `report_error`'s `eprintln!`
/// carries the message.
#[cfg(not(any(windows, target_os = "macos")))]
fn show_error_dialog(_message: &str) {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A package layout pointing at names that need not exist: nothing here
    /// spawns `Layout`'s programs.
    fn fake_layout() -> Layout {
        Layout {
            server: PathBuf::from("server"),
            client: PathBuf::from("ShadowScaleClient"),
            ai: PathBuf::from("sim_ai"),
        }
    }

    /// The human is a *seat*, not a special case: it is filled through the same
    /// path any locally-hosted occupant would be.
    #[test]
    fn the_humans_seat_is_filled_with_the_client() {
        let layout = fake_layout();
        assert_eq!(human_seat(&layout).program, layout.client.as_path());
    }

    /// ⛔ **A RIVAL PLAYS.** `sim_ai`'s `--brain` default is `pass`, which assigns
    /// nobody and starves; the shipped game must spawn its rivals on the utility
    /// brain, and the environment lever is what puts them back on `pass` for a
    /// comparison.
    #[test]
    fn a_rival_spawns_on_the_utility_brain_unless_the_environment_says_otherwise() {
        let default = Ok(AI_BRAIN_DEFAULT.to_owned());
        assert_eq!(brain_from_override(None), default);
        assert_eq!(brain_from_override(Some(String::new())), default);
        assert_eq!(brain_from_override(Some("   ".to_owned())), default);
        assert_eq!(
            brain_from_override(Some("pass".to_owned())),
            Ok("pass".to_owned())
        );
    }

    /// ⛔ **A BRAIN `sim_ai` WOULD REFUSE NEVER REACHES A `spawn()`.** `spawn`
    /// succeeds for any name, clap then rejects it, and the child is gone in
    /// milliseconds — reaped without a word, leaving a game with no rivals and
    /// no diagnostics. The check is here, and the message names the value.
    #[test]
    fn a_brain_the_ai_does_not_play_is_refused_with_the_value_named() {
        let refused = brain_from_override(Some("utilty".to_owned()))
            .expect_err("a misspelt brain is not a brain");
        assert!(refused.contains("utilty"), "{refused}");
        assert!(refused.contains(ENV_AI_BRAIN), "{refused}");
        for brain in AI_BRAIN_NAMES {
            assert!(refused.contains(brain), "{refused}");
            assert_eq!(
                brain_from_override(Some(brain.to_owned())),
                Ok(brain.to_owned()),
                "every brain the AI plays is accepted"
            );
        }
    }

    /// The contract twin of the server's `seats.roster` event: the shape its
    /// doc comment states parses, and nothing else does.
    #[test]
    fn a_roster_event_parses_and_other_lines_do_not() {
        let line = br#"{"timestamp_ms":1,"level":"INFO","target":"shadow_scale::server","message":"seats.roster","fields":{"factions":"[0,1,2]","world_epoch":3}}"#;
        assert_eq!(
            parse_roster_event(line),
            Some(Ok(RosterEvent {
                factions: vec![0, 1, 2],
                world_epoch: 3,
            }))
        );
        let other = br#"{"timestamp_ms":1,"level":"INFO","target":"shadow_scale::server","message":"seat.claimed","fields":{"faction":"1"}}"#;
        assert_eq!(parse_roster_event(other), None);
        assert_eq!(parse_roster_event(b"not json"), None);
    }

    /// ⛔ **A ROSTER LINE THE PARSER REFUSES IS NOT A LINE TO SKIP.** It is the
    /// supervisor contract broken, and the launcher has to say so — the exact
    /// shape `?factions` instead of `%factions` produces, which is one character
    /// in the server and costs the packaged game every rival.
    #[test]
    fn a_roster_line_the_parser_refuses_is_rejected_and_not_silently_skipped() {
        // `?factions` — the tracing Debug sigil — quotes the string it records.
        let debug_sigil = br#"{"level":"INFO","target":"shadow_scale::server","message":"seats.roster","fields":{"factions":"\"[0,1,2]\"","world_epoch":3}}"#;
        let rejected = match parse_roster_event(debug_sigil) {
            Some(Err(rejected)) => rejected,
            other => panic!("a roster-shaped line must not read as another line: {other:?}"),
        };
        let message = rejected.message();
        assert!(message.contains(ROSTER_EVENT_MESSAGE), "{message}");
        assert!(
            message.contains(ROSTER_FIELD_FACTIONS) && message.contains("[0,1,2]"),
            "the offending frame is quoted back verbatim: {message}"
        );
        // The other two clauses answer the same way.
        assert!(
            matches!(
                parse_roster_event(
                    br#"{"target":"shadow_scale::server","message":"seats.roster","fields":{"factions":"[0]"}}"#
                ),
                Some(Err(_))
            ),
            "a roster event with no world_epoch"
        );
        assert!(
            matches!(
                parse_roster_event(
                    br#"{"target":"shadow_scale::server","message":"seats.roster"}"#
                ),
                Some(Err(_))
            ),
            "a roster event with no fields"
        );
    }

    /// ⛔ **PINNED TO THE SERVER'S OWN EMIT, NOT TO A HAND-WRITTEN STRING.** The
    /// only guard this contract had asserted against a JSON literal, so the
    /// launcher and the server could drift apart without a single test moving:
    /// changing `%factions` to `?factions` in the server left every launcher test
    /// green and shipped a game with zero rivals.
    ///
    /// So this drives the **real** chain — `core_sim::log_stream::emit_seats_roster`
    /// through the **real** `LogForwardLayer`, serialized the way the log-stream
    /// server serializes it, framed the way it frames it — into `read_log_frame`
    /// and `parse_roster_event`.
    #[test]
    fn the_servers_own_roster_emit_parses() {
        use tracing_subscriber::prelude::*;

        const FACTIONS: [u32; 3] = [0, 1, 2];
        const WORLD_EPOCH: u32 = 3;

        let (sender, receiver) = crossbeam_channel::unbounded();
        let subscriber =
            tracing_subscriber::registry().with(core_sim::log_stream::LogForwardLayer::new(sender));
        tracing::subscriber::with_default(subscriber, || {
            core_sim::log_stream::emit_seats_roster(&FACTIONS, WORLD_EPOCH);
        });
        let envelope = receiver.try_recv().expect("the server emitted the event");

        // `run_log_stream`'s own encoding: the envelope as JSON behind a u32 LE length.
        let payload = serde_json::to_vec(&envelope).expect("the envelope serialises");
        let mut framed = (payload.len() as u32).to_le_bytes().to_vec();
        framed.extend_from_slice(&payload);
        let mut stream = std::io::Cursor::new(framed);
        let line = read_log_frame(&mut stream).expect("one whole frame");

        assert_eq!(
            parse_roster_event(&line),
            Some(Ok(RosterEvent {
                factions: FACTIONS.to_vec(),
                world_epoch: WORLD_EPOCH as u64,
            })),
            "the launcher reads what the server writes: {}",
            String::from_utf8_lossy(&line)
        );
        assert_eq!(envelope.target, ROSTER_EVENT_TARGET);
        assert_eq!(envelope.message, ROSTER_EVENT_MESSAGE);
    }

    /// Reconciliation, on stand-in children: a roster spawns one rival per
    /// non-human faction, a shrunken roster reaps the departed and keeps the
    /// rest, and a grown one respawns. Unix-only for its `sleep` stand-in.
    #[cfg(unix)]
    #[test]
    fn reconcile_spawns_a_rival_per_roster_faction_and_reaps_the_departed() {
        const HUMAN_AND_TWO_RIVALS: [u32; 3] = [HUMAN_FACTION_ID, 1, 2];
        const HUMAN_AND_ONE_RIVAL: [u32; 2] = [HUMAN_FACTION_ID, 1];

        let ports_file = std::env::temp_dir().join(format!(
            "{PORTS_FILE_PREFIX}reconcile-{}{PORTS_FILE_EXTENSION}",
            std::process::id()
        ));
        fs::write(&ports_file, "{}").expect("write the stand-in handshake file");
        let data_dir = std::env::temp_dir();
        let group = ProcessGroup::kill_on_close().expect("a process group");
        let stand_in = Path::new("sleep");
        let mut session = Session::new(
            spawn_sleeper(),
            ports_file.clone(),
            AI_BRAIN_DEFAULT.to_owned(),
            data_dir.join(format!("{RUN_ID_PREFIX}reconcile-{}", std::process::id())),
        );

        // `sleep` needs a duration; the stand-in program gets the faction as its
        // argument, which is a legal (short) duration — long enough for the
        // assertions, short enough not to linger if a reap were skipped.
        session
            .reconcile(
                &HUMAN_AND_TWO_RIVALS,
                stand_in,
                &data_dir,
                &ports_file,
                &group,
            )
            .expect("reconcile spawns");
        assert_eq!(
            session.rival_factions(),
            vec![1, 2],
            "one rival per non-human faction, and none for the human"
        );
        let departed = session.rivals[1].1.id();
        assert!(process_is_alive(departed));

        session
            .reconcile(
                &HUMAN_AND_ONE_RIVAL,
                stand_in,
                &data_dir,
                &ports_file,
                &group,
            )
            .expect("reconcile reaps");
        assert_eq!(session.rival_factions(), vec![1]);
        assert_eq!(
            session.seats_of_run(),
            vec![HUMAN_FACTION_ID, 1, 2],
            "a reaped rival still has a page to open"
        );
        assert!(
            !process_is_alive(departed),
            "a rival whose faction left the roster must be reaped"
        );

        session
            .reconcile(
                &HUMAN_AND_TWO_RIVALS,
                stand_in,
                &data_dir,
                &ports_file,
                &group,
            )
            .expect("reconcile respawns");
        assert_eq!(session.rival_factions(), vec![1, 2]);
        assert_eq!(
            session.seats_of_run(),
            vec![HUMAN_FACTION_ID, 1, 2],
            "a respawned rival is not listed twice"
        );

        drop(session);
        let _ = fs::remove_file(&ports_file);
    }

    /// The reaping guarantee, exercised on N children rather than argued about:
    /// a session holding a server and several players kills and waits for every
    /// one of them, and clears the handshake file.
    ///
    /// Unix-only for its stand-in child (`sleep`); the Windows side of the
    /// guarantee is the job object, which needs a real Windows host to mean
    /// anything and is not simulated here.
    #[cfg(unix)]
    #[test]
    fn every_child_is_reaped_when_the_session_drops() {
        /// More than one, because a reap that stops at the first child is
        /// exactly the regression this guards.
        const PLAYER_COUNT: usize = 2;

        let ports_file = std::env::temp_dir().join(format!(
            "{PORTS_FILE_PREFIX}test-{}{PORTS_FILE_EXTENSION}",
            std::process::id()
        ));
        fs::write(&ports_file, "{}").expect("write the stand-in handshake file");

        /// A rival's stand-in, so the reap covers the supervised children too.
        const A_RIVAL_FACTION: u32 = 1;

        let mut session = Session::new(
            spawn_sleeper(),
            ports_file.clone(),
            AI_BRAIN_DEFAULT.to_owned(),
            std::env::temp_dir().join(format!("{RUN_ID_PREFIX}drop-{}", std::process::id())),
        );
        let mut pids = vec![session.server().id()];
        for _ in 0..PLAYER_COUNT {
            pids.push(session.adopt_player(spawn_sleeper()).id());
        }
        session.rivals.push((A_RIVAL_FACTION, spawn_sleeper()));
        pids.push(session.rivals[0].1.id());
        assert!(pids.iter().all(|pid| process_is_alive(*pid)));

        drop(session);

        for pid in pids {
            assert!(!process_is_alive(pid), "child {pid} outlived the session");
        }
        assert!(
            !ports_file.exists(),
            "the handshake file outlived the session"
        );
    }

    /// One line per seat, each a complete command a player can paste: the resolved `sim_ai`,
    /// the run directory, the seat, and the page beside that seat's directory.
    #[test]
    fn a_viewer_line_per_seat_names_the_program_the_run_and_the_page() {
        let sim_ai = Path::new("/pkg/Contents/Helpers/sim_ai");
        let run_dir = Path::new("/data/ShadowScale/runs/run-1757600000-42");
        let lines = viewer_lines(sim_ai, run_dir, &[HUMAN_FACTION_ID, 1, 2]);
        assert_eq!(
            lines,
            vec![
                "/pkg/Contents/Helpers/sim_ai viewer /data/ShadowScale/runs/run-1757600000-42 \
                 --seat 0 --out /data/ShadowScale/runs/run-1757600000-42/seat_0.html",
                "/pkg/Contents/Helpers/sim_ai viewer /data/ShadowScale/runs/run-1757600000-42 \
                 --seat 1 --out /data/ShadowScale/runs/run-1757600000-42/seat_1.html",
                "/pkg/Contents/Helpers/sim_ai viewer /data/ShadowScale/runs/run-1757600000-42 \
                 --seat 2 --out /data/ShadowScale/runs/run-1757600000-42/seat_2.html",
            ]
        );
        assert!(viewer_lines(sim_ai, run_dir, &[]).is_empty());
    }

    /// A run id sorts by its start time, which is what pruning by name relies on.
    #[test]
    fn a_run_id_carries_the_prefix_and_the_start_time() {
        let id = mint_run_id();
        let rest = id.strip_prefix(RUN_ID_PREFIX).expect("the prefix");
        let (secs, pid) = rest.split_once('-').expect("time and pid");
        assert!(secs.parse::<u64>().expect("seconds") > 0);
        assert_eq!(pid.parse::<u32>().expect("pid"), std::process::id());
    }

    /// Creating a run keeps the newest [`KEPT_RUNS`] directories, the new one included, and
    /// leaves anything that is not a run alone.
    #[test]
    fn creating_a_run_prunes_the_oldest_beyond_the_kept_count() {
        let runs = std::env::temp_dir().join(format!("shadowscale_runs_{}", std::process::id()));
        let _ = fs::remove_dir_all(&runs);
        for n in 0..KEPT_RUNS + 2 {
            fs::create_dir_all(runs.join(format!("{RUN_ID_PREFIX}{:010}-1", n)))
                .expect("an old run");
        }
        fs::create_dir_all(runs.join("not-a-run")).expect("a bystander");
        let newest = format!("{RUN_ID_PREFIX}{:010}-1", KEPT_RUNS + 10);
        let created = create_run_dir(&runs, &newest).expect("the run directory");
        assert!(created.is_dir());
        let mut kept: Vec<String> = fs::read_dir(&runs)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(RUN_ID_PREFIX))
            .collect();
        kept.sort();
        assert_eq!(
            kept.len(),
            KEPT_RUNS,
            "the newest {KEPT_RUNS} survive: {kept:?}"
        );
        assert_eq!(kept.last(), Some(&newest));
        assert!(
            !kept.contains(&format!("{RUN_ID_PREFIX}{:010}-1", 0)),
            "the oldest was pruned"
        );
        assert!(runs.join("not-a-run").is_dir(), "a bystander is untouched");
        let _ = fs::remove_dir_all(&runs);
    }

    /// How long a stand-in child would live if nothing killed it. Long enough
    /// that one observed alive after the drop could only have survived the
    /// reap, and long enough that a *missed* kill hangs the test on `wait`
    /// rather than passing by luck.
    #[cfg(unix)]
    const CHILD_LIFETIME_SECS: &str = "600";

    /// A child that stays alive until something kills it.
    ///
    /// Every stream is detached: a child that survived a broken reap would
    /// otherwise hold the test harness's captured output pipe open for its whole
    /// lifetime, turning a clean assertion failure into a ten-minute hang.
    #[cfg(unix)]
    fn spawn_sleeper() -> Child {
        Command::new("sleep")
            .arg(CHILD_LIFETIME_SECS)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a stand-in child process")
    }

    #[cfg(unix)]
    fn process_is_alive(pid: u32) -> bool {
        Command::new("ps")
            .arg("-p")
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}
