//! ⛔ **THE QUERY CHANNEL OBEYS THE SEAT GATE — over the real socket, on the real binary.**
//!
//! Three of the five questions carry a client-supplied `faction_id` and are answered out of that
//! faction's private state: a named band's live equipment wear, its idle workers, its take curve. A
//! connection that could ask them about *another* seat's faction would be told, which is the same
//! disclosure the per-seat frame closes — one channel over, and it would defeat the point of scoping
//! a frame at all.
//!
//! The other two (`ListSaves`, `FactionCapacity`) name no faction and are asked from
//! `LandingScreen.gd` **before a world, and therefore before any seat exists**. A gate that reached
//! them would make the load menu unopenable, which is why the case below asks them from a connection
//! holding no seat and insists on a real answer.
//!
//! **It drives the built `server` binary over its command socket**, for the same reason
//! `save_load_over_the_socket.rs` does: the gate lives in the main loop's dispatch, beside the
//! `world_active` gate and the seat registry, and an in-process assertion would be testing a copy of
//! the routing rather than the routing. `CARGO_BIN_EXE_server` is also what makes cargo build the
//! binary before this test runs, and it is only defined for the package owning the bin — which is
//! why this file lives in `core_sim/tests/`.
//!
//! **A refusal is asserted as a REPLY, never as silence.** Every read here has a timeout, so a
//! query the server dropped instead of refusing fails as a timeout with the server's own log
//! underneath it — which is the failure mode this gate must not introduce: a client holding a
//! forecast sheet open forever.
//!
//! See `.claude/rules/core_sim/factions.md` → "Seats: a connection claims the faction it drives".

use std::fs;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use core_sim::{apply_port_base, SimulationConfig};
use sim_runtime::commands::{
    query_error, DenialRaidForecastQuery, FactionCapacityQuery, HuntCrewTakeQuery,
    HuntTripForecastQuery, QueryPayload, SeatClaimReply,
};
use sim_runtime::{CommandEnvelope, CommandPayload, QueryReply, QueryReplyEnvelope};

// =================================================================================================
// The world this test drives
// =================================================================================================

/// A small map: the case is about who may ask a question, not about terrain, and every second of
/// worldgen here is a second on the suite.
const MAP_WIDTH: u32 = 24;
const MAP_HEIGHT: u32 = 16;
const MAP_PRESET: &str = "earthlike";
const START_PROFILE: &str = "late_forager_tribe";
/// Fixed and non-zero: `seed == 0` asks the server to randomise.
const MAP_SEED: u64 = 11;

/// **One rival, so there is a second seat to be refused from.** Without it every faction id but the
/// home seat's would be refused by the roster rather than by the gate, and the case would prove
/// nothing.
const ONE_RIVAL: u32 = 1;

/// **How far apart this test's world puts two starts.** The shipped separation seats exactly one
/// faction on a map this small (`max_faction_starts`), so a 24x16 world asked for a rival would be
/// clamped back to none — and the second seat under test would not exist. Shrinking the separation
/// buys the second seat without paying for a bigger map; nothing here reads the distance between
/// the two peoples.
const TEST_START_SEPARATION: u32 = 6;

/// The two seats. `FactionId(0)` is the human every world has; `FactionId(1)` is the rival.
const HOME_SEAT: u32 = 0;
const RIVAL_SEAT: u32 = 1;

/// **A herd id no world contains.** The own-seat arm has to prove the question reached the *sim*,
/// and the sim's first check on a hunt question is the herd — so a question that passes the gate is
/// answered `unknown_herd`, and one the gate refused is answered `not_your_seat`. Two different
/// tokens off one payload is what makes the arms distinguishable without building a real band, a
/// real herd and a real kit into the fixture.
const NO_SUCH_HERD: &str = "no_such_herd_query_seat_gate";
/// A real `equipment.json` hunting kit, so an own-seat question is refused for the herd rather than
/// for the kit.
const HUNT_KIT: &str = "big_game";
/// A band id, a party and a floor that are all *valid* — the point is to reach the herd check, and
/// an invalid floor or an empty party would be refused ahead of it.
const ANY_BAND: u64 = 1;
const ANY_PARTY: u32 = 3;
const ANY_FLOOR: f32 = 0.25;
const ANY_CREW_CAP: u32 = 4;

// =================================================================================================
// Ports, timeouts and logs — the same harness shape `save_load_over_the_socket.rs` uses
// =================================================================================================

/// Where this test's server *starts looking* for a free block. A starting point, not a binding: the
/// base is set through the config file rather than through `SIM_PORT_BASE`, so `port_alloc` bumps by
/// a whole block when one is busy and two concurrent suite runs cannot collide.
const TEST_PORT_BASE: u16 = 45200;

/// How long to wait for the spawned server to bind its block and publish the ports file.
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(60);
/// Poll interval while waiting for that file — a deadline loop that also notices the child exiting.
const PORTS_FILE_POLL: Duration = Duration::from_millis(25);
/// How long any single reply may take to arrive. Covers worldgen on a cold debug build, and is what
/// turns a *dropped* query into a failure rather than a hang.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// `RUST_LOG` for the child: the server's own `query.refused=…` line is the first thing anyone reads
/// when this test fails.
const SERVER_LOG_FILTER: &str = "info";
/// How much of the server log a failure quotes.
const LOG_TAIL_LINES: usize = 40;

/// Correlation ids. Distinct per round trip, so a reply that answered a different question is caught
/// rather than accepted.
const HOME_CLAIM_ID: u64 = 1;
const RIVAL_CLAIM_ID: u64 = 2;
const HOME_ASKS_ITS_OWN_ID: u64 = 3;
const HOME_ASKS_THE_RIVALS_ID: u64 = 4;
const RIVAL_ASKS_ITS_OWN_ID: u64 = 5;
const RIVAL_ASKS_THE_HOMES_ID: u64 = 6;
const UNSEATED_ASKS_A_FACTION_ID: u64 = 7;
const LIST_SAVES_ID: u64 = 8;
const FACTION_CAPACITY_ID: u64 = 9;

// =================================================================================================
// The harness
// =================================================================================================

/// A scratch directory that takes the save slots, the patched config and the server log with it.
struct Scratch {
    dir: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Best effort: a leftover temp directory is a nuisance, a test that panics inside its own
        // cleanup is worse.
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The spawned server. **Killed on drop, including on a panic**, so a failing assertion cannot leave
/// a server holding a port block.
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
/// discovery path (`.claude/rules/core_sim/ports.md`).
struct Ports {
    command: SocketAddr,
}

/// **One command-socket connection, and the question-and-answer it can carry.**
///
/// A seat belongs to a *connection*, so every arm of this test needs its own socket held open: the
/// claim only holds while the socket does, and the whole point of the gate is that it reads the seat
/// of the connection the question arrived on.
struct Link {
    stream: TcpStream,
    replies: BufReader<TcpStream>,
    log_path: PathBuf,
}

impl Link {
    fn open(addr: SocketAddr, log_path: &Path) -> Self {
        let stream = TcpStream::connect(addr).expect("connect to the command socket");
        stream
            .set_read_timeout(Some(RESPONSE_TIMEOUT))
            .expect("command socket read timeout");
        let replies = BufReader::new(stream.try_clone().expect("the reply half of the socket"));
        Self {
            stream,
            replies,
            log_path: log_path.to_path_buf(),
        }
    }

    fn fail(&self, what: &str) -> ! {
        panic!(
            "{what}\n--- server log (last {LOG_TAIL_LINES} lines) ---\n{}",
            log_tail(&self.log_path)
        );
    }

    /// Write one length-prefixed protobuf frame, exactly as the client's emit path does.
    fn write(&mut self, payload: CommandPayload, waiting_for: &str) {
        let envelope = CommandEnvelope {
            payload,
            correlation_id: None,
        };
        let bytes = envelope.encode_to_vec().expect("the command encodes");
        let mut framed = Vec::with_capacity(std::mem::size_of::<u32>() + bytes.len());
        framed.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        framed.extend_from_slice(&bytes);
        if let Err(err) = self.stream.write_all(&framed) {
            self.fail(&format!(
                "the command socket refused the frame for {waiting_for}: {err}"
            ));
        }
        if let Err(err) = self.stream.flush() {
            self.fail(&format!(
                "the command socket refused a flush for {waiting_for}: {err}"
            ));
        }
    }

    /// Read the next reply off this connection, or fail naming what never arrived.
    fn read_reply(&mut self, request_id: u64, waiting_for: &str) -> QueryReply {
        let mut len = [0u8; std::mem::size_of::<u32>()];
        if let Err(err) = self.replies.read_exact(&mut len) {
            self.fail(&format!(
                "no reply arrived for {waiting_for} (after {}s): {err} — a question the server \
                 DROPPED instead of refusing looks exactly like this, and on a client it is a \
                 sheet that waits forever",
                RESPONSE_TIMEOUT.as_secs()
            ));
        }
        let len = u32::from_le_bytes(len) as usize;
        if len == 0 || len > sim_runtime::MAX_PROTO_FRAME {
            self.fail(&format!(
                "the command socket announced a {len}-byte reply for {waiting_for}"
            ));
        }
        let mut payload = vec![0u8; len];
        if let Err(err) = self.replies.read_exact(&mut payload) {
            self.fail(&format!(
                "a {len}-byte reply for {waiting_for} never finished arriving: {err}"
            ));
        }
        let envelope = QueryReplyEnvelope::decode(&payload).expect("the reply decodes");
        assert_eq!(
            envelope.request_id, request_id,
            "the reply to {waiting_for} answered a different request"
        );
        envelope.reply
    }

    /// Ask one question and read its answer back on this connection.
    fn ask(&mut self, request_id: u64, query: QueryPayload, waiting_for: &str) -> QueryReply {
        self.write(CommandPayload::Query { request_id, query }, waiting_for);
        self.read_reply(request_id, waiting_for)
    }

    /// Claim a seat on this connection and read the answer.
    fn claim_seat(&mut self, request_id: u64, faction_id: u32) -> SeatClaimReply {
        let waiting_for = format!("the claim of seat {faction_id}");
        self.write(
            CommandPayload::ClaimSeat {
                request_id,
                faction_id,
            },
            &waiting_for,
        );
        match self.read_reply(request_id, &waiting_for) {
            QueryReply::SeatClaim(answer) => answer,
            other => self.fail(&format!(
                "{waiting_for} was answered with {other:?} rather than a SeatClaimReply"
            )),
        }
    }
}

/// The refusal token on a `QueryReply::Error`, or a failure naming what came back instead.
fn error_token(reply: &QueryReply, what: &str) -> String {
    match reply {
        QueryReply::Error(token) => token.clone(),
        other => panic!("{what} was answered with {other:?} rather than a refusal"),
    }
}

/// A hunt-trip question about `faction_id`. Valid in every respect except the herd, so the sim's own
/// answer to it is [`query_error::UNKNOWN_HERD`] — see [`NO_SUCH_HERD`].
fn hunt_trip_about(faction_id: u32) -> QueryPayload {
    QueryPayload::HuntTripForecast(HuntTripForecastQuery {
        faction_id,
        band_id: ANY_BAND,
        herd_id: NO_SUCH_HERD.to_string(),
        kit_id: HUNT_KIT.to_string(),
        party_workers: ANY_PARTY,
        floor: ANY_FLOOR,
        preset_floors: Vec::new(),
        max_party_workers: 0,
    })
}

fn denial_raid_about(faction_id: u32) -> QueryPayload {
    QueryPayload::DenialRaidForecast(DenialRaidForecastQuery {
        faction_id,
        band_id: ANY_BAND,
        herd_id: NO_SUCH_HERD.to_string(),
        kit_id: HUNT_KIT.to_string(),
        party_workers: ANY_PARTY,
        max_party_workers: 0,
    })
}

fn crew_take_about(faction_id: u32) -> QueryPayload {
    QueryPayload::HuntCrewTake(HuntCrewTakeQuery {
        faction_id,
        band_id: ANY_BAND,
        herd_id: NO_SUCH_HERD.to_string(),
        kit_id: HUNT_KIT.to_string(),
        floor: ANY_FLOOR,
        max_workers: ANY_CREW_CAP,
    })
}

/// Boot a server on its own port block with its own save directory, and hand back where to reach it.
fn start_server(case: &str) -> (ServerProcess, Scratch, PathBuf, Ports) {
    let scratch = Scratch {
        dir: std::env::temp_dir().join(format!(
            "shadow_scale_query_gate_{case}_{}",
            std::process::id()
        )),
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
        // An explicit base is honoured EXACTLY and a busy block is fatal; the config file's base
        // auto-bumps instead. Inheriting one from the developer's shell would put this test on a
        // block someone is using.
        .env_remove("SIM_PORT_BASE")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .expect("the built server binary starts");
    let mut process = ServerProcess { child };
    let ports = await_ports_file(&mut process, &ports_path, &log_path);
    (process, scratch, log_path, ports)
}

/// The shipped simulation config with its port block moved to [`TEST_PORT_BASE`] and its start
/// separation shrunk to [`TEST_START_SEPARATION`].
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
    json["faction_start_min_separation"] =
        serde_json::Value::from(u64::from(TEST_START_SEPARATION));

    let path = dir.join("simulation_config.json");
    fs::write(
        &path,
        serde_json::to_string_pretty(&json).expect("the patched config serialises"),
    )
    .expect("the patched config writes");
    path
}

/// Wait for the server to publish its handshake file, and read the block it actually bound.
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

/// The handshake file's contract is its key names (`ports.md`). A half-written file, or one left by
/// another process, reads as "not ready yet" rather than as an error.
fn read_ports_file(path: &Path, pid: u32) -> Option<Ports> {
    let raw = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    if json.get("pid")?.as_u64()? != u64::from(pid) {
        return None;
    }
    let host = json.get("host")?.as_str()?.parse().ok()?;
    let port = u16::try_from(json.get("command")?.as_u64()?).ok()?;
    Some(Ports {
        command: SocketAddr::new(host, port),
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

// =================================================================================================
// The test
// =================================================================================================

/// ⛔ **A QUESTION NAMING ANOTHER SEAT'S FACTION IS REFUSED; ONE NAMING YOUR OWN IS ANSWERED AS
/// BEFORE; AND THE TWO THAT NAME NO FACTION ANSWER AN UNSEATED CONNECTION.**
///
/// All three in one server, because the arms only mean anything against each other: a gate that
/// refuses everything satisfies the first, a gate that refuses nothing satisfies the second, and
/// either can still break the load menu, which is the third.
///
/// The mismatch arm and the own-seat arm send **the same payload** but for the `faction_id`, so the
/// two tokens that come back (`not_your_seat` vs the sim's own `unknown_herd`) differ for exactly
/// one reason.
#[test]
fn a_question_about_another_seats_faction_is_refused_over_the_socket() {
    let (_process, _scratch, log_path, ports) = start_server("gate");

    // The home player sits down and builds the world. `new_game` names no faction and is not a host
    // verb, so a seated connection may send it — the same path the shipped client takes.
    let mut home = Link::open(ports.command, &log_path);
    let claim = home.claim_seat(HOME_CLAIM_ID, HOME_SEAT);
    assert!(
        claim.ok,
        "the home seat could not be claimed: {}",
        claim.error
    );
    home.write(new_game(), "the world under test");

    // **This round trip is also the synchronisation.** Replies on one connection are answered in
    // order, so an answer to a question written after `new_game` proves the loop is past it — and
    // the token proves the question reached the *sim* rather than being refused by the gate.
    let own = home.ask(
        HOME_ASKS_ITS_OWN_ID,
        hunt_trip_about(HOME_SEAT),
        "the home seat's question about its own faction",
    );
    assert_eq!(
        error_token(&own, "the home seat's own question"),
        query_error::UNKNOWN_HERD,
        "a seat's question about its own faction must be answered exactly as it was before the \
         gate existed — on the merits of the question, by the sim"
    );

    for (request_id, query, label) in [
        (
            HOME_ASKS_THE_RIVALS_ID,
            hunt_trip_about(RIVAL_SEAT),
            "hunt_trip_forecast",
        ),
        (
            HOME_ASKS_THE_RIVALS_ID + REQUEST_ID_STRIDE,
            denial_raid_about(RIVAL_SEAT),
            "denial_raid_forecast",
        ),
        (
            HOME_ASKS_THE_RIVALS_ID + 2 * REQUEST_ID_STRIDE,
            crew_take_about(RIVAL_SEAT),
            "hunt_crew_take",
        ),
    ] {
        let refused = home.ask(
            request_id,
            query,
            &format!("the home seat's {label} about the RIVAL's faction"),
        );
        assert_eq!(
            error_token(&refused, label),
            query_error::NOT_YOUR_SEAT,
            "{label} let the home seat read faction {RIVAL_SEAT}'s private state"
        );
    }

    // **The gate is about the connection, not about which faction is 'the player's'.** The rival's
    // client is refused the home seat's question and answered its own, which is the same pair of
    // answers the other way round.
    let mut rival = Link::open(ports.command, &log_path);
    let claim = rival.claim_seat(RIVAL_CLAIM_ID, RIVAL_SEAT);
    assert!(
        claim.ok,
        "the rival seat could not be claimed: {} — this world must seat two peoples, or the case \
         above is about a roster refusal rather than about the gate",
        claim.error
    );
    let refused = rival.ask(
        RIVAL_ASKS_THE_HOMES_ID,
        hunt_trip_about(HOME_SEAT),
        "the rival seat's question about the HOME faction",
    );
    assert_eq!(
        error_token(&refused, "the rival seat's question about the home faction"),
        query_error::NOT_YOUR_SEAT
    );
    let own = rival.ask(
        RIVAL_ASKS_ITS_OWN_ID,
        hunt_trip_about(RIVAL_SEAT),
        "the rival seat's question about its own faction",
    );
    assert_eq!(
        error_token(&own, "the rival seat's own question"),
        query_error::UNKNOWN_HERD,
        "the rival's client must be answered about its own faction on the merits"
    );

    // ⛔ **The load menu's guard.** `LandingScreen` asks these two before `Main` exists — no world,
    // no seat — and they are answered ahead of the `world_active` gate. A gate that reached them
    // would leave the load menu unopenable, and no other test in the suite would notice.
    let mut unseated = Link::open(ports.command, &log_path);
    match unseated.ask(LIST_SAVES_ID, QueryPayload::ListSaves, "the save list") {
        QueryReply::ListSaves(_) => {}
        other => panic!(
            "an unseated connection must still be told what is on disk; it got {other:?} — the \
             load menu opens from exactly this question"
        ),
    }
    match unseated.ask(
        FACTION_CAPACITY_ID,
        QueryPayload::FactionCapacity(FactionCapacityQuery {
            width: MAP_WIDTH,
            height: MAP_HEIGHT,
        }),
        "the rival ceiling for a grid",
    ) {
        QueryReply::FactionCapacity(_) => {}
        other => panic!(
            "an unseated connection must still be told what a grid seats; it got {other:?} — the \
             New Game screen draws its rival control from exactly this question"
        ),
    }

    // And the same connection, asking a question that DOES name a faction, is refused: holding no
    // seat is holding nobody's private state.
    let refused = unseated.ask(
        UNSEATED_ASKS_A_FACTION_ID,
        hunt_trip_about(HOME_SEAT),
        "an unseated connection's faction-bearing question",
    );
    assert_eq!(
        error_token(
            &refused,
            "an unseated connection's faction-bearing question"
        ),
        query_error::NOT_YOUR_SEAT
    );
}

/// Spacing between the correlation ids of the three refusals asked in one loop, so each round trip
/// still carries an id of its own.
const REQUEST_ID_STRIDE: u64 = 100;
