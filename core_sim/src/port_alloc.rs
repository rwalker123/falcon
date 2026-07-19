//! All-or-nothing allocation of the server's four-port block, plus the
//! ports handshake file the client uses to discover a bumped block.
//!
//! Historically each of the four listeners bound itself independently and
//! failed differently: the command listener panicked, snapshot/log streaming
//! merely warned and disabled themselves. A single busy port could therefore
//! leave a *running* server with no snapshot stream at all. This module binds
//! the whole block up front and hands the already-bound listeners to the
//! subsystems, so the server either owns all four sockets or refuses to start.

use std::env;
use std::fs;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::path::PathBuf;

use serde::Serialize;

use crate::resources::{
    COMMAND_PORT_OFFSET, LOG_PORT_OFFSET, SNAPSHOT_FLAT_PORT_OFFSET, SNAPSHOT_PORT_OFFSET,
};

/// Auto-derived bases are spaced this far apart so each concurrent server gets
/// its own contiguous block of four ports without overlapping its neighbours.
/// Mirrors `PORT_BLOCK_STRIDE` in `scripts/run_stack.sh`.
pub const PORT_BLOCK_STRIDE: u16 = 10;

/// Number of distinct auto-derived slots tried before giving up
/// (`base .. base + (PORT_SLOT_COUNT - 1) * PORT_BLOCK_STRIDE`).
/// Mirrors `PORT_SLOT_COUNT` in `scripts/run_stack.sh`.
pub const PORT_SLOT_COUNT: u16 = 100;

/// Directory name used under the per-user app-data root for the ports file.
const PORTS_FILE_DIR: &str = "ShadowScale";
/// File name of the ports handshake file.
const PORTS_FILE_NAME: &str = "ports.json";
/// Relative fallback path under `$HOME` on Linux when `XDG_STATE_HOME` is unset.
const LINUX_STATE_FALLBACK: &str = ".local/state";
/// Relative path under `$HOME` for the macOS per-user app-data root.
const MACOS_APP_SUPPORT: &str = "Library/Application Support";

/// The four listeners for one port block, already bound and owned.
#[derive(Debug)]
pub struct BoundPorts {
    /// Base port the block was bound at (may differ from the configured base
    /// if the block was auto-bumped).
    pub base: u16,
    /// Host all four listeners are bound to.
    pub host: IpAddr,
    pub snapshot: TcpListener,
    pub command: TcpListener,
    pub snapshot_flat: TcpListener,
    pub log: TcpListener,
}

impl BoundPorts {
    pub fn snapshot_port(&self) -> u16 {
        self.base + SNAPSHOT_PORT_OFFSET
    }

    pub fn command_port(&self) -> u16 {
        self.base + COMMAND_PORT_OFFSET
    }

    pub fn snapshot_flat_port(&self) -> u16 {
        self.base + SNAPSHOT_FLAT_PORT_OFFSET
    }

    pub fn log_port(&self) -> u16 {
        self.base + LOG_PORT_OFFSET
    }
}

/// Binds all four ports of the block at `base`, all-or-nothing.
///
/// On failure every listener bound so far is dropped before returning, so the
/// partial block is released and the next slot can be tried cleanly.
pub fn bind_block(host: IpAddr, base: u16) -> io::Result<BoundPorts> {
    if base.checked_add(LOG_PORT_OFFSET).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("port base {base} would overflow the 16-bit port range"),
        ));
    }
    let bind_one = |offset: u16| TcpListener::bind(SocketAddr::new(host, base + offset));
    Ok(BoundPorts {
        base,
        host,
        snapshot: bind_one(SNAPSHOT_PORT_OFFSET)?,
        command: bind_one(COMMAND_PORT_OFFSET)?,
        snapshot_flat: bind_one(SNAPSHOT_FLAT_PORT_OFFSET)?,
        log: bind_one(LOG_PORT_OFFSET)?,
    })
}

/// Allocates the port block for this process.
///
/// When `base_is_explicit` (the operator set `SIM_PORT_BASE`) the base is
/// honoured exactly and a conflict is fatal — `scripts/run_stack.sh` and the
/// per-worktree port assignment depend on an explicit base being deterministic.
/// Otherwise the block is bumped by [`PORT_BLOCK_STRIDE`] on `AddrInUse` for up
/// to [`PORT_SLOT_COUNT`] slots. Any other IO error surfaces immediately rather
/// than silently walking the whole range.
pub fn allocate(host: IpAddr, base: u16, base_is_explicit: bool) -> io::Result<BoundPorts> {
    if base_is_explicit {
        return bind_block(host, base).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!(
                    "SIM_PORT_BASE={base} was set explicitly but ports {base}-{last} could not be \
                     bound on {host}: {err}. Free those ports or choose another base.",
                    last = base + LOG_PORT_OFFSET
                ),
            )
        });
    }

    for slot in 0..PORT_SLOT_COUNT {
        let Some(candidate) = base.checked_add(slot * PORT_BLOCK_STRIDE) else {
            break;
        };
        match bind_block(host, candidate) {
            Ok(bound) => return Ok(bound),
            Err(err) if err.kind() == io::ErrorKind::AddrInUse => continue,
            Err(err) => return Err(err),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        format!(
            "no free port block found on {host}: tried {PORT_SLOT_COUNT} blocks of 4 ports from \
             {base} upwards in steps of {PORT_BLOCK_STRIDE}. Close other Shadow-Scale servers or \
             set SIM_PORT_BASE to a free base."
        ),
    ))
}

/// Contents of the ports handshake file. Key names are a contract with the
/// Godot client's discovery reader — do not rename them.
#[derive(Serialize)]
struct PortsFile {
    host: String,
    snapshot: u16,
    command: u16,
    snapshot_flat: u16,
    log: u16,
    pid: u32,
}

/// Resolves the ports handshake file path.
///
/// `SIM_PORTS_FILE` overrides it verbatim. Otherwise a per-user app-data
/// location is used (never the temp dir, where AV heuristics are aggressive).
pub fn ports_file_path() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("SIM_PORTS_FILE") {
        return Some(PathBuf::from(explicit));
    }
    let root = if cfg!(target_os = "windows") {
        PathBuf::from(env::var_os("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(env::var_os("HOME")?).join(MACOS_APP_SUPPORT)
    } else if let Some(state) = env::var_os("XDG_STATE_HOME") {
        PathBuf::from(state)
    } else {
        PathBuf::from(env::var_os("HOME")?).join(LINUX_STATE_FALLBACK)
    };
    Some(root.join(PORTS_FILE_DIR).join(PORTS_FILE_NAME))
}

/// Removes the ports handshake file when the server exits normally.
///
/// A stale file left behind by a crash or an uncaught signal is expected and
/// tolerated: the client validates the file and falls back to the default
/// block, so no liveness machinery beyond the recorded pid lives here.
pub struct PortsFileGuard {
    path: PathBuf,
}

impl PortsFileGuard {
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// JSON key holding the writing process's pid. Contract with the Godot client's
/// discovery reader and with [`PortsFile`] — do not rename.
const PORTS_FILE_PID_KEY: &str = "pid";

/// Whether the file at `path` still records `pid` as its owner.
///
/// Best-effort and deliberately biased toward `false`: a missing, unreadable or
/// malformed file is *not* ours to delete. Wrongly removing a live server's
/// discovery record breaks its clients, whereas leaving a stale file behind is
/// the already-accepted crash/SIGINT behaviour.
fn ports_file_is_owned_by(path: &std::path::Path, pid: u32) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    parsed
        .get(PORTS_FILE_PID_KEY)
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|recorded| recorded == u64::from(pid))
}

impl Drop for PortsFileGuard {
    /// Removes the handshake file only while this process still owns it.
    ///
    /// A later server that found the block busy and bumped will have
    /// *overwritten* the file with its own ports and pid; deleting it then would
    /// strand that live server's clients on the hardcoded default block. Never
    /// panics and never fails the process.
    fn drop(&mut self) {
        if ports_file_is_owned_by(&self.path, std::process::id()) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Writes the ports handshake file, overwriting unconditionally.
///
/// Returns `None` if the path can't be resolved or the write fails; that is
/// never fatal — the server still runs, only client auto-discovery is lost.
pub fn write_ports_file(bound: &BoundPorts) -> Option<PortsFileGuard> {
    write_ports_file_at(ports_file_path()?, bound)
}

/// [`write_ports_file`] with an explicit destination, for tests and for the
/// `SIM_PORTS_FILE` override path.
pub fn write_ports_file_at(path: PathBuf, bound: &BoundPorts) -> Option<PortsFileGuard> {
    let payload = PortsFile {
        host: bound.host.to_string(),
        snapshot: bound.snapshot_port(),
        command: bound.command_port(),
        snapshot_flat: bound.snapshot_flat_port(),
        log: bound.log_port(),
        pid: std::process::id(),
    };
    let json = serde_json::to_string(&payload).ok()?;
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            tracing::warn!(
                target: "shadow_scale::server",
                path = %parent.display(),
                error = %err,
                "ports_file.create_dir_failed"
            );
            return None;
        }
    }
    if let Err(err) = fs::write(&path, json) {
        tracing::warn!(
            target: "shadow_scale::server",
            path = %path.display(),
            error = %err,
            "ports_file.write_failed"
        );
        return None;
    }
    Some(PortsFileGuard { path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::{Mutex, MutexGuard};

    const LOCALHOST: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    /// These tests bind real sockets, so they must not run concurrently with
    /// each other: an ephemeral probe released by one test can be handed to
    /// another while the first is still using the block.
    static PORT_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Start of the private range these tests allocate from. Well above the
    /// server's 41000 block and below the ephemeral range, so a test block can
    /// never collide with a running server or an OS-assigned port.
    const TEST_BASE_START: u16 = 45000;
    /// Spacing between successive test blocks. Wider than
    /// `PORT_BLOCK_STRIDE × 2` so the bump test's hops stay inside its own slot.
    const TEST_BASE_STRIDE: u16 = 100;
    /// How far the scan walks before giving up on finding a free test block.
    const TEST_BASE_MAX_ATTEMPTS: u16 = 50;

    static NEXT_TEST_BASE: AtomicU16 = AtomicU16::new(TEST_BASE_START);

    fn serialized() -> MutexGuard<'static, ()> {
        PORT_TEST_LOCK.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// Binds `port` and keeps it bound for as long as the returned listener lives.
    fn occupy(port: u16) -> TcpListener {
        TcpListener::bind(SocketAddr::new(LOCALHOST, port)).expect("occupy test port")
    }

    /// A base whose whole block (and the next stride's, for the bump test) is
    /// free. Each call advances a shared cursor, so no two tests share a block.
    fn free_base() -> u16 {
        for _ in 0..TEST_BASE_MAX_ATTEMPTS {
            let base = NEXT_TEST_BASE.fetch_add(TEST_BASE_STRIDE, Ordering::SeqCst);
            let probe = bind_block(LOCALHOST, base)
                .and_then(|_| bind_block(LOCALHOST, base + PORT_BLOCK_STRIDE));
            if probe.is_ok() {
                return base;
            }
        }
        panic!("no free test port block found from {TEST_BASE_START} upwards");
    }

    #[test]
    fn bind_block_claims_all_four_ports() {
        let _serial = serialized();
        let base = free_base();
        let bound = bind_block(LOCALHOST, base).expect("block should bind");
        assert_eq!(bound.snapshot_port(), base);
        assert_eq!(bound.command_port(), base + COMMAND_PORT_OFFSET);
        assert_eq!(bound.snapshot_flat_port(), base + SNAPSHOT_FLAT_PORT_OFFSET);
        assert_eq!(bound.log_port(), base + LOG_PORT_OFFSET);
    }

    #[test]
    fn bind_block_is_all_or_nothing() {
        let _serial = serialized();
        let base = free_base();
        let blocker = occupy(base + COMMAND_PORT_OFFSET);
        let err = bind_block(LOCALHOST, base).expect_err("one busy port fails the block");
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
        drop(blocker);
        // The partial block was released, so a retry now succeeds.
        bind_block(LOCALHOST, base).expect("block should bind after blocker released");
    }

    #[test]
    fn allocate_bumps_by_stride_when_base_is_busy() {
        let _serial = serialized();
        let base = free_base();
        let _blocker = occupy(base + COMMAND_PORT_OFFSET);
        let bound = allocate(LOCALHOST, base, false).expect("should bump to the next slot");
        assert_eq!(bound.base, base + PORT_BLOCK_STRIDE);
    }

    #[test]
    fn allocate_does_not_bump_an_explicit_base() {
        let _serial = serialized();
        let base = free_base();
        let _blocker = occupy(base + COMMAND_PORT_OFFSET);
        let err = allocate(LOCALHOST, base, true).expect_err("explicit base must be fatal");
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn ports_file_round_trips_the_contract_keys() {
        let _serial = serialized();
        let dir = std::env::temp_dir().join(format!("shadow_scale_ports_{}", std::process::id()));
        let path = dir.join(PORTS_FILE_NAME);
        let base = free_base();
        let bound = bind_block(LOCALHOST, base).expect("block should bind");
        let guard =
            write_ports_file_at(path.clone(), &bound).expect("ports file should be written");
        let raw = fs::read_to_string(guard.path()).expect("read ports file");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("valid json");
        assert_eq!(parsed["host"], "127.0.0.1");
        assert_eq!(parsed["snapshot"], base);
        assert_eq!(parsed["command"], base + COMMAND_PORT_OFFSET);
        assert_eq!(parsed["snapshot_flat"], base + SNAPSHOT_FLAT_PORT_OFFSET);
        assert_eq!(parsed["log"], base + LOG_PORT_OFFSET);
        assert_eq!(parsed["pid"], std::process::id());
        drop(guard);
        assert!(!path.exists(), "guard removes the file on drop");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Offset applied to this process's pid to fabricate a pid that is not ours,
    /// standing in for a second server that bumped its block and took ownership
    /// of the handshake file.
    const FOREIGN_PID_OFFSET: u32 = 1;

    /// A unique scratch dir per ownership test, so the filesystem-only tests
    /// never collide with each other or with the socket-binding tests.
    fn scratch_path(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "shadow_scale_ports_owner_{}_{tag}",
                std::process::id()
            ))
            .join(PORTS_FILE_NAME)
    }

    /// Writes a handshake file recording `pid`, creating parent dirs as needed.
    fn write_ports_file_with_pid(path: &PathBuf, pid: u32) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create scratch dir");
        }
        fs::write(path, format!("{{\"{PORTS_FILE_PID_KEY}\":{pid}}}")).expect("write ports file");
    }

    #[test]
    fn guard_removes_the_file_it_still_owns() {
        let path = scratch_path("owned");
        write_ports_file_with_pid(&path, std::process::id());
        drop(PortsFileGuard { path: path.clone() });
        assert!(!path.exists(), "our own file should be removed on drop");
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn guard_leaves_a_file_another_process_took_ownership_of() {
        // Regression: an older server exiting must not delete the handshake file
        // a newer, still-running server overwrote with its own ports and pid.
        let path = scratch_path("foreign");
        let foreign_pid = std::process::id() + FOREIGN_PID_OFFSET;
        write_ports_file_with_pid(&path, foreign_pid);
        drop(PortsFileGuard { path: path.clone() });
        assert!(path.exists(), "another process's file must be left alone");
        let raw = fs::read_to_string(&path).expect("read ports file");
        assert!(
            raw.contains(&foreign_pid.to_string()),
            "the file's contents must be untouched"
        );
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn guard_tolerates_a_missing_or_malformed_file() {
        let missing = scratch_path("missing");
        // No file written at all: dropping must not panic.
        drop(PortsFileGuard {
            path: missing.clone(),
        });
        assert!(!missing.exists());

        let malformed = scratch_path("malformed");
        if let Some(parent) = malformed.parent() {
            fs::create_dir_all(parent).expect("create scratch dir");
        }
        fs::write(&malformed, "{not json").expect("write malformed ports file");
        drop(PortsFileGuard {
            path: malformed.clone(),
        });
        assert!(
            malformed.exists(),
            "an unparseable file is not provably ours, so it stays"
        );
        for path in [&missing, &malformed] {
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }
}
