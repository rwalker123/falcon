//! ShadowScale desktop launcher.
//!
//! The game ships as two programs: a simulation SERVER that binds a block of
//! four local TCP ports, and a Godot CLIENT that connects to them. A player
//! double-clicks one icon, so something has to start the server, wait for it to
//! be reachable, run the client, and reap the server afterwards. This binary is
//! that supervisor; it replaces the per-platform shell scripts
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
//! the OS honours even if this process is terminated.

// No console window when the player double-clicks the packaged .exe. Errors are
// surfaced through `report_error` (a message box) rather than stdio.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

    let server = Command::new(&layout.server)
        .current_dir(&data_dir)
        .env(ENV_PORTS_FILE, &ports_file)
        .spawn()
        .map_err(|err| {
            format!(
                "Could not start the server:\n{}\n\n{err}",
                layout.server.display()
            )
        })?;

    // From here on every exit path must reap the server, so ownership moves
    // into a guard rather than being cleaned up at each `return`.
    let mut session = Session::new(server, ports_file.clone());

    // Belt and braces on Windows: the guard covers orderly exits, the job
    // object covers this process being killed outright.
    let _job = ProcessGroup::kill_on_close(session.server())?;

    wait_for_ready(&mut session, &ports_file)?;

    let mut client = Command::new(&layout.client)
        .current_dir(&data_dir)
        .env(ENV_PORTS_FILE, &ports_file)
        // The client is a GUI app; inheriting stdio is harmless but noisy when
        // the launcher is run from a terminal, and meaningless otherwise.
        .stdin(Stdio::null())
        .spawn()
        .map_err(|err| {
            format!(
                "Could not start the game:\n{}\n\n{err}",
                layout.client.display()
            )
        })?;

    client
        .wait()
        .map_err(|err| format!("Lost track of the game process: {err}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Absolute paths to the two child programs in the installed package.
struct Layout {
    server: PathBuf,
    client: PathBuf,
}

impl Layout {
    fn resolve(exe_dir: &Path) -> Result<Self, String> {
        let layout = Self::platform_layout(exe_dir)?;
        require_file(&layout.server, "the simulation server")?;
        require_file(&layout.client, "the game")?;
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
        })
    }

    /// Windows: a flat package directory, the launcher sitting beside both
    /// children as `ShadowScale.exe`.
    #[cfg(windows)]
    fn platform_layout(exe_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            server: exe_dir.join(format!("{SERVER_STEM}.exe")),
            client: exe_dir.join(format!("{CLIENT_STEM}.exe")),
        })
    }

    /// Linux and anything else: a flat package directory. Not shipped today,
    /// but the layout the packaging script would produce if it were.
    #[cfg(not(any(target_os = "macos", windows)))]
    fn platform_layout(exe_dir: &Path) -> Result<Self, String> {
        Ok(Self {
            server: exe_dir.join(SERVER_STEM),
            client: exe_dir.join(CLIENT_STEM),
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
// Readiness
// ---------------------------------------------------------------------------

/// Keys the client reads out of the handshake file, and therefore the definition
/// of a *complete* one. Contract twin of `KEY_HOST` / `KEY_COMMAND` / `KEY_LOG` /
/// `KEY_SNAPSHOT_FLAT` in
/// `clients/godot_thin_client/src/scripts/ServerPortsFile.gd`.
///
/// Note the stream port is `snapshot_flat`, **not** `snapshot`: `snapshot` is the
/// legacy JSON socket, and a client pointed at it connects to a live socket and
/// then silently never renders. Any change here must move in lockstep with that
/// script.
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

/// Owns the running server for the rest of the launcher's life.
///
/// Cleanup lives in `Drop` rather than at each `return` so that every failure
/// path after the spawn — readiness timeout, client spawn failure, or an
/// ordinary quit — reaps the server and clears the handshake file exactly once.
struct Session {
    server: Child,
    ports_file: PathBuf,
}

impl Session {
    fn new(server: Child, ports_file: PathBuf) -> Self {
        Self { server, ports_file }
    }

    fn server(&mut self) -> &mut Child {
        &mut self.server
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Both calls are best-effort: an already-exited server makes `kill`
        // fail, which is exactly the state we want anyway.
        let _ = self.server.kill();
        let _ = self.server.wait();
        remove_ports_file(&self.ports_file);
    }
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
    /// kill, and the OS closes every handle of a dying process, the server is
    /// reaped even if the launcher is force-killed or crashes. The handle must
    /// therefore stay alive for the launcher's entire run — dropping this value
    /// early kills the server.
    pub struct ProcessGroup {
        handle: HANDLE,
    }

    impl ProcessGroup {
        pub fn kill_on_close(child: &mut Child) -> Result<Self, String> {
            // SAFETY: all three calls take either null (default security
            // attributes / unnamed job) or pointers to locals that outlive the
            // call, and every result is checked before use.
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

                if AssignProcessToJobObject(group.handle, child.as_raw_handle() as HANDLE) == 0 {
                    return Err(last_error("add the server to the process group"));
                }
                Ok(group)
            }
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
        pub fn kill_on_close(_child: &mut Child) -> Result<Self, String> {
            Ok(Self)
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
