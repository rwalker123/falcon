//! **Save and load driven over the real sockets, asserting on the bytes a client would receive.**
//!
//! Every other save/load test drives `apply_command` in-process and asserts on server-side state.
//! Two defects shipped underneath that suite and were found by a human playing the game:
//!
//! 1. the load ran a real turn, so the restored world aged;
//! 2. the load published no ring entry, so the client's world-handoff gate never saw a full frame
//!    and sat on the loading overlay forever — while a test asserting "a frame was published"
//!    passed, because both publication kinds set it.
//!
//! Both are properties of **what arrives on the snapshot socket**, so this test connects to it the
//! way `clients/godot_thin_client` does — a command socket for the orders, a snapshot socket for the
//! frames — and decodes the FlatBuffers envelopes off the wire.
//!
//! **It spawns the built `server` binary as a child process** rather than standing the socket layer
//! up in-process. The main loop in `src/bin/server.rs` is precisely where both defects lived: it
//! owns the `world_active` gate, the command dispatch, the load handler and the post-command
//! recapture. Re-creating any of that inside the test would test the copy. `CARGO_BIN_EXE_server` is
//! also what makes cargo *build* the binary before this test runs, and it is only defined for the
//! package that owns the bin — which is why this file lives in `core_sim/tests/` rather than in
//! `integration_tests/`, where the path would have to be guessed and `cargo test -p
//! integration_tests` would never build it.
//!
//! See `.claude/rules/core_sim/save-game.md` → "What a load owes beyond restoring the world".

use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use core_sim::{apply_port_base, SimulationConfig};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;
use sim_runtime::commands::SaveOpReply;
use sim_runtime::{CommandEnvelope, CommandPayload, QueryReply, QueryReplyEnvelope};

// =================================================================================================
// The world this test drives
// =================================================================================================

/// A small map: the properties under test are about publication, not about terrain, and every
/// second of worldgen here is a second on the suite. Same dimensions the in-process save tests use.
const MAP_WIDTH: u32 = 24;
const MAP_HEIGHT: u32 = 16;
const MAP_PRESET: &str = "earthlike";
const START_PROFILE: &str = "late_forager_tribe";
/// Fixed and non-zero: `seed == 0` asks the server to randomise, and a test that cannot say which
/// world it built cannot say which world came back.
const MAP_SEED: u64 = 7;

/// Turns resolved before the save, so the blob holds a world that has *run* — a load that aged by a
/// turn is indistinguishable from a correct one if the save is taken at the world's first tick.
const TURNS_BEFORE_SAVE: u32 = 3;

const SAVE_SLOT: &str = "socket_round_trip";

/// Correlation ids for the two save-channel round trips. Distinct so a reply that answered the
/// wrong request would be caught rather than accepted.
const SAVE_REQUEST_ID: u64 = 1;
const LOAD_REQUEST_ID: u64 = 2;

/// The epoch of the idle boot app, before any world exists (`server.rs` starts `world_epoch` at 0).
const IDLE_WORLD_EPOCH: u32 = 0;

/// **The `frameSeq` of a world's very first publication.** `SnapshotHistory::next_publication`
/// hands out `frame_seq + 1` from a counter that starts at 0, and every world is a brand-new `App`
/// with a brand-new history — so a first frame carrying anything else means this test *missed* the
/// real first frame, which is the one ambiguity that would let a wrong verdict through: "the load
/// published a delta" and "we joined after the load's baseline" look identical otherwise.
const FIRST_PUBLICATION_SEQ: u64 = 1;

// =================================================================================================
// Ports, and why nothing here is a fixed port
// =================================================================================================

/// Where this test's server *starts looking* for a free block — far above the 41000 development
/// block and the per-worktree blocks `scripts/run_stack.sh` hands out, so a suite run never squats
/// a port a human is using.
///
/// It is a starting point, not a binding: the base is set through the config file rather than
/// through `SIM_PORT_BASE` precisely so it stays **non-explicit** and `port_alloc::allocate`
/// auto-bumps by `PORT_BLOCK_STRIDE` when a block is busy. Two concurrent suite runs therefore
/// cannot collide, and the test never has to probe-then-bind (which would race). The ports it
/// actually talks to are read back from the handshake file the server publishes.
const TEST_PORT_BASE: u16 = 45000;

/// How long to wait for the spawned server to bind its block and publish the ports file. Generous:
/// it covers process start plus every config file the boot reads, on a loaded CI machine.
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(60);
/// Poll interval while waiting for that file. A deadline loop, not a sleep that stands in for
/// synchronisation — the loop also notices the child exiting.
const PORTS_FILE_POLL: Duration = Duration::from_millis(25);

/// How long any single frame or reply may take to arrive. Covers worldgen on a cold debug build.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// A ceiling on how many frames a "read until…" loop will consume before giving up. The longest
/// wait here spans a handful of turn publications and their recaptures, so this is orders above the
/// real count; its job is to turn a stream that never carries what we want into a *bounded* failure
/// with a message instead of a hang.
const MAX_FRAMES_AWAITED: usize = 256;

/// Sanity bound on a length prefix read off the snapshot socket. A 24x16 world's full frame is far
/// below it; anything above means the stream desynchronised, and failing on the length beats
/// allocating it.
const MAX_SNAPSHOT_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// `RUST_LOG` for the child. The server's own `save.*` / `new_game.*` lines are the first thing
/// anyone reads when this test fails, so they are captured rather than silenced.
const SERVER_LOG_FILTER: &str = "info";
/// How much of the server log a failure quotes. Enough for the boot line and the last few commands.
const LOG_TAIL_LINES: usize = 40;

// =================================================================================================
// Frames off the wire
// =================================================================================================

/// The four header fields this test judges a frame by, decoded from the published bytes.
#[derive(Debug, Clone, Copy)]
struct FrameInfo {
    world_epoch: u32,
    tick: u64,
    frame_seq: u64,
    /// `true` for a `snapshot` payload, `false` for a `delta`. **The distinction is the point**: a
    /// delta is not a baseline — a field that happens to equal its default compares unchanged and is
    /// never sent — so the client's reveal gate waits for a full frame and only a full frame.
    full: bool,
}

impl FrameInfo {
    fn decode(bytes: &[u8]) -> Self {
        let envelope = fb::root_as_envelope(bytes).expect("a published frame is a valid envelope");
        let (header, full) = match envelope.payload_type() {
            fb::SnapshotPayload::snapshot => (
                envelope
                    .payload_as_snapshot()
                    .expect("the envelope names a snapshot payload")
                    .header(),
                true,
            ),
            fb::SnapshotPayload::delta => (
                envelope
                    .payload_as_delta()
                    .expect("the envelope names a delta payload")
                    .header(),
                false,
            ),
            other => panic!("a published frame carried neither a snapshot nor a delta: {other:?}"),
        };
        let header = header.expect("every published frame carries a header");
        Self {
            world_epoch: header.worldEpoch(),
            tick: header.tick(),
            frame_seq: header.frameSeq(),
            full,
        }
    }

    fn kind(&self) -> &'static str {
        if self.full {
            "full"
        } else {
            "delta"
        }
    }
}

// =================================================================================================
// The harness
// =================================================================================================

/// A scratch directory that takes the save slots, the patched config and the server log with it.
struct Scratch {
    dir: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Best effort: a leftover temp directory is a nuisance, a failed test that panics inside
        // its own cleanup is worse.
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The spawned server. **Killed on drop, including on a panic**, so a failing assertion cannot
/// leave a server holding a port block.
struct ServerProcess {
    child: Child,
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The ports the server actually bound, read back from the handshake file — the client's own
/// discovery path (`.claude/rules/core_sim/ports.md`), and what makes the auto-bump above usable.
struct Ports {
    command: SocketAddr,
    snapshot_flat: SocketAddr,
}

struct Harness {
    /// Declared first so it is dropped first: the child dies before its scratch directory does.
    /// Both are drop guards and neither is read, hence the underscores.
    _process: ServerProcess,
    _scratch: Scratch,
    log_path: PathBuf,
    /// Where each command's own connection goes — see [`Harness::open_command_connection`].
    command_addr: SocketAddr,
    frames: BufReader<TcpStream>,
}

impl Harness {
    /// Boot a server on its own port block, with its own save directory, and subscribe to frames.
    ///
    /// The snapshot socket is a **standing** connection opened **before** the first `new_game`,
    /// exactly as the client does: the server sends a newly accepted client nothing until the next
    /// broadcast, so a client that connects afterwards is not entitled to the frame it asked for.
    /// The command socket, by contrast, gets a fresh connection per verb — see
    /// [`Harness::open_command_connection`].
    fn start(case: &str) -> Self {
        let scratch = Scratch {
            dir: std::env::temp_dir()
                .join(format!("shadow_scale_socket_{case}_{}", std::process::id())),
        };
        let _ = fs::remove_dir_all(&scratch.dir);
        fs::create_dir_all(&scratch.dir).expect("scratch directory");

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
            // An explicit base is honoured EXACTLY and a busy block is fatal; the config file's
            // base auto-bumps instead. Inheriting one from the developer's shell would put this
            // test on a block someone is using and make a collision a hard failure.
            .env_remove("SIM_PORT_BASE")
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .spawn()
            .expect("the built server binary starts");
        let mut process = ServerProcess { child };

        let ports = await_ports_file(&mut process, &ports_path, &log_path);

        let snapshots =
            TcpStream::connect(ports.snapshot_flat).expect("connect to the snapshot socket");
        snapshots
            .set_read_timeout(Some(RESPONSE_TIMEOUT))
            .expect("snapshot socket read timeout");

        Self {
            _process: process,
            _scratch: scratch,
            log_path,
            command_addr: ports.command,
            frames: BufReader::new(snapshots),
        }
    }

    /// Abort the test naming what never arrived, with the server's own log underneath it.
    fn fail(&self, what: &str) -> ! {
        panic!(
            "{what}\n--- server log (last {LOG_TAIL_LINES} lines) ---\n{}",
            log_tail(&self.log_path)
        );
    }

    /// **One connection per command, which is how the shipped client talks to this socket.**
    ///
    /// `transmit_proto_command` (the Godot client's native bridge) connects, writes one
    /// length-prefixed protobuf frame, and drops the socket; the save/query verbs
    /// (`bridge/query.rs`) do the same but hold the connection open to read the answer. Holding one
    /// persistent connection instead would exercise a path no client uses — the mistake this whole
    /// test exists to stop making — so the transport is the client's, verb for verb.
    ///
    /// Fire-and-forget means two commands in flight at once could reach the dispatch loop in either
    /// order, so **every command here is sent only after the previous one's effect has been observed
    /// on the snapshot socket or answered on the command socket**. That is the synchronisation; there
    /// are no sleeps.
    fn open_command_connection(&self, waiting_for: &str) -> TcpStream {
        match TcpStream::connect(self.command_addr) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(RESPONSE_TIMEOUT))
                    .expect("command socket read timeout");
                stream
            }
            Err(err) => self.fail(&format!(
                "the command socket refused a connection for {waiting_for}: {err}"
            )),
        }
    }

    fn write_command(&self, stream: &mut TcpStream, payload: CommandPayload, waiting_for: &str) {
        let envelope = CommandEnvelope {
            payload,
            correlation_id: None,
        };
        let bytes = envelope.encode_to_vec().expect("the command encodes");
        let mut framed = Vec::with_capacity(std::mem::size_of::<u32>() + bytes.len());
        framed.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        framed.extend_from_slice(&bytes);
        if let Err(err) = stream.write_all(&framed) {
            self.fail(&format!(
                "the command socket refused the frame for {waiting_for}: {err}"
            ));
        }
        if let Err(err) = stream.flush() {
            self.fail(&format!(
                "the command socket refused a flush for {waiting_for}: {err}"
            ));
        }
    }

    /// An order: written, then the connection is dropped. What happened is learned from the frames.
    fn send(&self, payload: CommandPayload, waiting_for: &str) {
        let mut stream = self.open_command_connection(waiting_for);
        self.write_command(&mut stream, payload, waiting_for);
    }

    /// The next frame broadcast on the snapshot socket, or a failure naming what we were waiting
    /// for.
    fn next_frame(&mut self, waiting_for: &str) -> FrameInfo {
        let mut len = [0u8; std::mem::size_of::<u32>()];
        if let Err(err) = self.frames.read_exact(&mut len) {
            self.fail(&format!(
                "no frame arrived on the snapshot socket while waiting for {waiting_for} \
                 (after {}s): {err}",
                RESPONSE_TIMEOUT.as_secs()
            ));
        }
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 || len > MAX_SNAPSHOT_FRAME_BYTES {
            self.fail(&format!(
                "the snapshot socket announced a {len}-byte frame while waiting for \
                 {waiting_for}; the stream is desynchronised"
            ));
        }
        let mut payload = vec![0u8; len];
        if let Err(err) = self.frames.read_exact(&mut payload) {
            self.fail(&format!(
                "a {len}-byte frame was announced but never finished arriving while waiting for \
                 {waiting_for}: {err}"
            ));
        }
        FrameInfo::decode(&payload)
    }

    /// Read frames until `accept` takes one, bounded by [`MAX_FRAMES_AWAITED`].
    fn frame_matching(
        &mut self,
        waiting_for: &str,
        accept: impl Fn(&FrameInfo) -> bool,
    ) -> FrameInfo {
        let mut seen = Vec::new();
        for _ in 0..MAX_FRAMES_AWAITED {
            let frame = self.next_frame(waiting_for);
            if accept(&frame) {
                return frame;
            }
            seen.push(format!(
                "{}(epoch={} tick={} seq={})",
                frame.kind(),
                frame.world_epoch,
                frame.tick,
                frame.frame_seq
            ));
        }
        self.fail(&format!(
            "{MAX_FRAMES_AWAITED} frames went by without {waiting_for}; they were: {}",
            seen.join(", ")
        ));
    }

    /// **A save-channel verb: write it, then read its answer on the connection that asked.**
    ///
    /// `save_game` / `load_game` are commands rather than questions, but they answer on the query
    /// envelope because that is the socket's one way back — so the connection has to stay open
    /// until the answer lands, exactly as `bridge/query.rs` keeps it open. It is also the
    /// synchronisation for everything after: the reply is proof the verb was dispatched.
    fn save_op(&self, request_id: u64, payload: CommandPayload, waiting_for: &str) -> SaveOpReply {
        let mut stream = self.open_command_connection(waiting_for);
        self.write_command(&mut stream, payload, waiting_for);
        let mut replies = BufReader::new(stream);

        let mut len = [0u8; std::mem::size_of::<u32>()];
        if let Err(err) = replies.read_exact(&mut len) {
            self.fail(&format!(
                "no reply arrived on the command socket while waiting for {waiting_for} \
                 (after {}s): {err}",
                RESPONSE_TIMEOUT.as_secs()
            ));
        }
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 || len > sim_runtime::MAX_PROTO_FRAME {
            self.fail(&format!(
                "the command socket announced a {len}-byte reply while waiting for {waiting_for}"
            ));
        }
        let mut payload = vec![0u8; len];
        if let Err(err) = replies.read_exact(&mut payload) {
            self.fail(&format!(
                "a {len}-byte reply was announced but never finished arriving while waiting for \
                 {waiting_for}: {err}"
            ));
        }
        let envelope = QueryReplyEnvelope::decode(&payload).expect("the reply decodes");
        assert_eq!(
            envelope.request_id, request_id,
            "the reply to {waiting_for} answered a different request"
        );
        match envelope.reply {
            QueryReply::SaveOp(answer) => answer,
            other => self.fail(&format!(
                "{waiting_for} was answered with {other:?} rather than a SaveOpReply"
            )),
        }
    }

    /// **Reveal a world, the way the client does** — send `new_game` and wait for the full frame
    /// that opens the new epoch.
    ///
    /// The one retry covers the accept race the world-handoff rule describes: a frame broadcast
    /// while our connection is still in the listen backlog reaches nobody, and no amount of waiting
    /// recovers it. It is detected rather than assumed — a first frame whose `frameSeq` is not the
    /// world's first means we joined late — and the retry is the client's own
    /// retry-until-answered. After it, our socket is provably registered: we received a frame on it.
    fn reveal_a_new_world(&mut self) -> FrameInfo {
        self.send(new_game(), "the first world");
        let first = self.next_frame("the first world's baseline frame");
        let baseline = if first.frame_seq == FIRST_PUBLICATION_SEQ {
            first
        } else {
            self.send(
                new_game(),
                "a second world after joining the first one late",
            );
            self.frame_matching("a world whose first publication we did not miss", |frame| {
                frame.frame_seq == FIRST_PUBLICATION_SEQ
            })
        };
        assert!(
            baseline.full,
            "a generated world's first publication must be a FULL frame; it was a {} \
             (epoch={} tick={})",
            baseline.kind(),
            baseline.world_epoch,
            baseline.tick
        );
        assert_ne!(
            baseline.world_epoch, IDLE_WORLD_EPOCH,
            "a generated world must carry an epoch past the idle boot app's"
        );
        baseline
    }

    /// Ask for a resync and return the full frame it answers with.
    ///
    /// Only a resync (or a rollback) publishes a full frame mid-world — every turn and every
    /// post-command recapture publishes a delta — so the next full frame on the stream is this
    /// resync's answer.
    fn resync_full_frame(&mut self, waiting_for: &str) -> FrameInfo {
        self.send(CommandPayload::Resync, waiting_for);
        self.frame_matching(waiting_for, |frame| frame.full)
    }
}

fn new_game() -> CommandPayload {
    CommandPayload::NewGame {
        preset_id: MAP_PRESET.to_string(),
        width: MAP_WIDTH,
        height: MAP_HEIGHT,
        seed: MAP_SEED,
        profile_id: START_PROFILE.to_string(),
    }
}

/// The shipped simulation config with its port block moved to [`TEST_PORT_BASE`].
///
/// The four bind addresses are derived by [`apply_port_base`] rather than by arithmetic here, so
/// the block's layout has exactly one home (`resources.rs`) and this test cannot drift from it.
fn write_test_config(dir: &Path) -> PathBuf {
    let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/data/simulation_config.json");
    let raw = fs::read_to_string(&shipped).expect("the shipped simulation config reads");
    let mut config = SimulationConfig::from_file(&shipped).expect("the shipped config parses");
    assert!(
        apply_port_base(&mut config, TEST_PORT_BASE),
        "TEST_PORT_BASE must be a base a whole block fits above"
    );

    let mut json: serde_json::Value =
        serde_json::from_str(&raw).expect("the shipped config is JSON");
    for (key, addr) in [
        ("port_base_bind", config.port_base_bind),
        ("command_bind", config.command_bind),
        ("snapshot_flat_bind", config.snapshot_flat_bind),
        ("log_bind", config.log_bind),
    ] {
        json[key] = serde_json::Value::String(addr.to_string());
    }

    let path = dir.join("simulation_config.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).expect("the patched config serialises"),
    )
    .expect("the patched config writes");
    path
}

/// Wait for the server to publish its handshake file, and read the block it actually bound.
///
/// Polls rather than sleeps a fixed amount, and gives up on a deadline; it also watches for the
/// child exiting, so a server that refuses to start fails here with its own log rather than at a
/// connect timeout further on.
fn await_ports_file(process: &mut ServerProcess, ports_path: &Path, log_path: &Path) -> Ports {
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    let pid = process.child.id();
    loop {
        if let Ok(Some(status)) = process.child.try_wait() {
            panic!(
                "the server exited ({status}) before publishing its ports file\n\
                 --- server log (last {LOG_TAIL_LINES} lines) ---\n{}",
                log_tail(log_path)
            );
        }
        if let Some(ports) = read_ports_file(ports_path, pid) {
            return ports;
        }
        if Instant::now() >= deadline {
            panic!(
                "the server never published {} within {}s\n\
                 --- server log (last {LOG_TAIL_LINES} lines) ---\n{}",
                ports_path.display(),
                SERVER_READY_TIMEOUT.as_secs(),
                log_tail(log_path)
            );
        }
        std::thread::sleep(PORTS_FILE_POLL);
    }
}

/// The handshake file's contract is its key names (`ports.md`). A file that is half-written, or one
/// left behind by some other process, reads as "not ready yet" rather than as an error.
fn read_ports_file(path: &Path, pid: u32) -> Option<Ports> {
    let raw = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if json.get("pid")?.as_u64()? != u64::from(pid) {
        return None;
    }
    let host = json.get("host")?.as_str()?.parse().ok()?;
    let port = |key: &str| -> Option<u16> { u16::try_from(json.get(key)?.as_u64()?).ok() };
    Some(Ports {
        command: SocketAddr::new(host, port("command")?),
        snapshot_flat: SocketAddr::new(host, port("snapshot_flat")?),
    })
}

fn log_tail(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(LOG_TAIL_LINES)..].join("\n")
        }
        Err(err) => format!(
            "(the server log at {} could not be read: {err})",
            path.display()
        ),
    }
}

// =================================================================================================
// The test
// =================================================================================================

/// **A load hands the client a FULL frame of the world that was saved, on the socket.**
///
/// Four assertions, each one a defect that shipped or the ambiguity that hid it:
///
/// - the first frame carrying the loaded world's epoch is a **full snapshot**, not a delta — the
///   world-handoff reveal gate waits for exactly that, and the delta that was published instead is
///   why a client sat on the loading overlay forever;
/// - it carries `frameSeq == 1`, which is what makes the assertion above *unambiguous*: it proves
///   the frame examined is the loaded world's first publication rather than one we joined late for;
/// - its tick is the **saved** tick — the load did not resolve a turn, which is the first defect;
/// - a `Resync` afterwards publishes a full frame rather than answering `resync.no_world`.
#[test]
fn a_load_over_the_socket_publishes_the_saved_world_as_a_full_frame() {
    let mut server = Harness::start("save_load");

    let revealed = server.reveal_a_new_world();

    // **The turns, synchronised on the wire.** Waiting for the tick the turns must land on is both
    // the synchronisation for the save that follows and the assertion that they ran at all.
    let saved_tick = revealed.tick + u64::from(TURNS_BEFORE_SAVE);
    server.send(
        CommandPayload::Turn {
            steps: TURNS_BEFORE_SAVE,
        },
        "the turns before the save",
    );
    let before_save = server.frame_matching(
        &format!("a frame at tick {saved_tick}, where the turns before the save must land"),
        |frame| frame.tick == saved_tick,
    );
    assert_eq!(
        before_save.world_epoch, revealed.world_epoch,
        "the turns ran in the world we revealed"
    );

    let saved = server.save_op(
        SAVE_REQUEST_ID,
        CommandPayload::SaveGame {
            request_id: SAVE_REQUEST_ID,
            slot: SAVE_SLOT.to_string(),
        },
        "the save",
    );
    assert!(saved.ok, "the save was refused with `{}`", saved.error);

    let loaded = server.save_op(
        LOAD_REQUEST_ID,
        CommandPayload::LoadGame {
            request_id: LOAD_REQUEST_ID,
            slot: SAVE_SLOT.to_string(),
        },
        "the load",
    );
    assert!(loaded.ok, "the load was refused with `{}`", loaded.error);
    assert!(
        loaded.config_drift.is_empty(),
        "the save and the load ran under one config, so nothing should have drifted: {:?}",
        loaded.config_drift
    );

    // **Everything the client's world handoff depends on, read off the snapshot socket.**
    let baseline = server.frame_matching("a frame from the loaded world", |frame| {
        frame.world_epoch != revealed.world_epoch
    });
    assert!(
        baseline.world_epoch > revealed.world_epoch,
        "a load must bump the world epoch past the world it replaced ({} -> {})",
        revealed.world_epoch,
        baseline.world_epoch
    );
    assert_eq!(
        baseline.frame_seq, FIRST_PUBLICATION_SEQ,
        "the frame examined must be the loaded world's FIRST publication (seq \
         {FIRST_PUBLICATION_SEQ}); at seq {} this test joined late and could not tell a delta \
         baseline from a missed one",
        baseline.frame_seq
    );
    assert!(
        baseline.full,
        "a loaded world's first frame must be a FULL snapshot — it is what the client's reveal \
         gate waits for, and a delta is not equivalent (a field equal to its default compares \
         unchanged and is never sent). It was a {}",
        baseline.kind()
    );
    assert_eq!(
        baseline.tick, saved_tick,
        "the loaded world must arrive at the tick it was saved at; a load that resolves a turn \
         ages the population and eats the food, and restoring the tick number would only hide it"
    );

    // **And a resync off the loaded world answers.** `resync.no_world` was the live symptom: the
    // client re-sent `load_game` forever because its recovery path was answered with nothing.
    let resynced = server.resync_full_frame("a resync of the loaded world");
    assert_eq!(
        resynced.world_epoch, baseline.world_epoch,
        "the resynced frame belongs to the loaded world"
    );
    assert_eq!(
        resynced.tick, saved_tick,
        "and it is still the saved world, not one that ran a turn to answer"
    );
    assert!(
        resynced.frame_seq > baseline.frame_seq,
        "a resync claims a live sequence number; a stale one reopens the gap it was asked to close"
    );
}
