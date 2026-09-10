//! **THE SEATED COMMAND CONNECTION** — one long-lived socket that holds this client's seat.
//!
//! ## Why one connection, and why it has a read path
//!
//! The server takes the faction a command acts on from the **seat the sending CONNECTION claimed**,
//! not from the `faction_id` on the wire (`.claude/rules/core_sim/factions.md` → "Seats"). A seat is
//! released the instant its connection closes, so a client that opened a fresh `TcpStream` per
//! command — which is exactly what this module replaced — claimed a seat on a socket it then dropped
//! and had every later command refused with `command.rejected=not_this_connections_seat`. **The seat
//! is a property of the connection**, so the connection has to outlive the command.
//!
//! It also has to be **read**: a claim is answered with a `QueryReplyEnvelope` on the same socket
//! (`handle_proto_client`, which spawns a per-connection writer thread for exactly this), and a
//! client that cannot tell whether it holds the seat cannot know whether its orders will be obeyed.
//! The answer is handed to the drain the main thread already pumps once a frame
//! ([`query::deliver_reply`]) rather than to a second one.
//!
//! ## ⛔ The client NEVER restates its identity
//!
//! Nothing here sends a session id, a token, or a faction the server is asked to trust. The claim is
//! made **once per connection** and thereafter the server knows who is speaking because of *which
//! socket* the bytes arrived on. Any scheme where a later command re-asserts the seat re-opens the
//! forgery hole seats exist to close (`docs/plan_multiplayer_seats.md` §4.1).
//!
//! ## Which socket a command goes out on
//!
//! [`dispatch`] is the whole routing rule, and it has two arms:
//!
//! - **the host verbs** (`turn`, `rollback`) go out on a THROWAWAY connection, because the server
//!   refuses them from a *seated* one: they move the world for everyone, so "host" is defined as
//!   holding no seat (`SeatRegistry::may_issue_host_verb`). The Inspector's `+1`/`+10` buttons and
//!   autoplay are their only callers and they keep working exactly as they did.
//! - **everything else** goes out on the seated link. That deliberately includes the world verbs
//!   (`new_game`, `map_size`, `resync`, the save channel's siblings) even though they name no
//!   faction: a seated connection may send them, so there is nothing to gain from a second socket —
//!   and routing by "does this carry a faction" would mean restating the server's 40-arm
//!   `commanding_faction` match here, where it could silently drift. **A misrouted faction-bearing
//!   command is refused at runtime with no compile error**, so the classification is kept as small
//!   as it can be: two variants, mirroring `is_host_verb`.
//!
//! ## The faction-bearing QUESTIONS ride it too
//!
//! A forecast query names a `faction_id` and is answered with that faction's private state — its
//! bands' equipment wear, its idle workers, its take curve. That is the same disclosure the seat gate
//! closes on commands, so the three faction-bearing queries are written on **this** socket rather
//! than on a throwaway one of their own ([`send_query`], routed from `bridge/query.rs`). The two that
//! name no faction (`ListSaves`, `FactionCapacity`) stay on the per-round-trip connection, because
//! they are asked from the landing screen before any seat exists.
//!
//! A query written here is fire-and-forget on the way out and correlated by `request_id` on the way
//! back, exactly as a seat claim is. What the worker keeps is a **deadline per outstanding query**,
//! so the three ways an answer can fail to arrive — the socket would not open, the socket died, the
//! server never answered — all land on the drain as an error rather than as silence.
//!
//! ## Reconnect
//!
//! A dropped link is re-established and the seat **re-claimed** on the new connection, because the
//! alternative is the failure this module exists to fix: every later command silently refused. The
//! server frees the seat when the old socket closes (`Command::ReleaseSeat` from its own read loop),
//! and a reconnect can race that release — so a `seat_occupied` refusal is retried a bounded number
//! of times before it is reported as a real one.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use sim_runtime::commands::seat_error;
use sim_runtime::{
    CommandEncodeError, CommandEnvelope, CommandPayload, QueryReply, QueryReplyEnvelope,
    MAX_PROTO_FRAME,
};

use crate::bridge::query;

/// **How long the link waits before rebuilding a dropped connection.**
///
/// A lever, not a latency budget: the only thing waiting on it is the *seat*, and the reconnect is
/// what keeps the seat held while the player is idle. Short enough that a server restart is invisible
/// by the time the player clicks anything, long enough that a server which is down does not become a
/// connect() spin on the machine's loopback.
const RECONNECT_BACKOFF: Duration = Duration::from_millis(500);

/// **How long a claim may go unanswered before the player is told.**
///
/// The claim is answered between turns like any other reply on this socket, so a healthy answer lands
/// in well under a frame. This is the bound on a socket that will never answer — a server wedged
/// mid-turn, or one that accepted the frame and said nothing — and its expiry is reported rather than
/// swallowed, because "nothing I click does anything" is precisely the state an unanswered claim
/// leaves the client in.
const SEAT_CLAIM_REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// **How long to wait out the previous connection's seat release before re-claiming.**
///
/// On a reconnect the seat may still be registered to the socket that just died: the server frees it
/// when its own read loop sees the EOF, which is concurrent with this side's reconnect. That refusal
/// is transient and is the one refusal worth retrying.
const SEAT_CLAIM_RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// **How many transient `seat_occupied` refusals to ride out before believing one.**
///
/// Times [`SEAT_CLAIM_RETRY_BACKOFF`] this is the window in which "the old socket has not been
/// reaped yet" and "another player holds this seat" are indistinguishable. Past it the refusal is
/// reported, because a claim that retries forever is the silent failure with extra steps.
const SEAT_CLAIM_ATTEMPTS: u32 = 8;

/// **How long a caller waits for the link to say whether its command reached the socket.**
///
/// The link writes on the caller's behalf, so this is the bound on a wedged link worker rather than
/// on the network — `CommandBridge::send_line` applies its own, tighter bound to the whole hop and is
/// what a player actually waits on. It exists so that a worker which somehow stopped draining cannot
/// wedge the command bridge for the session.
const LINK_ACK_TIMEOUT: Duration = Duration::from_secs(2);

/// **What an unanswered question is reported as.** Free-text detail, not a token: the seam renders
/// one failure line whatever went wrong, and the tokens a caller may branch on are the server's
/// (`sim_runtime::query_error`) plus the transport one `bridge/query.rs` owns.
const QUERY_DETAIL_UNANSWERED: &str = "query went unanswered";
/// The link was pointed at a different server before this question could be answered.
const QUERY_DETAIL_ENDPOINT_CHANGED: &str = "query endpoint changed before the reply arrived";
/// The write half vanished between the connect and the write — reported rather than retried, for the
/// same reason a command is: a question written twice is a question the sheet did not ask twice.
const QUERY_DETAIL_NOT_CONNECTED: &str = "command link is not connected";

/// Where the link connects. Carried on every message because the endpoint is resolved by GDScript
/// (env var → ports file → default) and only the caller knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoint {
    host: String,
    port: u16,
}

impl Endpoint {
    fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// One thing the link worker has to act on. Both directions of the socket and both control paths
/// arrive as this, so the worker is a single-threaded state machine over one channel — nothing about
/// the seat is shared state a lock could be forgotten around.
enum LinkMessage {
    /// GDScript asked for a seat. Replaces any previous intent and is re-asserted on every later
    /// reconnect.
    Claim {
        endpoint: Endpoint,
        faction_id: u32,
        request_id: u64,
    },
    /// One command to write on the seated socket. `ack` carries back whether it reached the wire.
    Send {
        endpoint: Endpoint,
        envelope: Box<CommandEnvelope>,
        ack: Sender<Result<(), String>>,
    },
    /// One faction-bearing question to write on the seated socket. Unlike [`Self::Send`] it carries
    /// no ack channel: `send_query` is called from Godot's main thread and must not block on the
    /// worker, so a dispatch failure is delivered to the query drain under `request_id` like every
    /// other way this question can fail to be answered.
    Query {
        endpoint: Endpoint,
        envelope: Box<CommandEnvelope>,
        request_id: u64,
        timeout: Duration,
    },
    /// A framed reply read off the socket by the reader thread of `generation`.
    Inbound { generation: u64, frame: Vec<u8> },
    /// The socket of `generation` ended — EOF, a framing violation, or a read error.
    Dropped { generation: u64, detail: String },
}

/// What the link is trying to hold, and how far it has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeatState {
    /// No claim is on the wire for the current connection.
    Unclaimed,
    /// A claim has been written and is awaiting its reply.
    Pending {
        deadline: Instant,
        attempts_left: u32,
    },
    /// The server granted it. Nothing is re-sent while this holds — a second claim on a connection
    /// that already has a seat is refused (`already_seated`), by design.
    Granted,
    /// The server refused it for a reason retrying cannot change. Reported to the player; a later
    /// reconnect will try once more, since a *new* connection is a new claimant.
    Refused,
}

/// The seat this client wants, and the id its answer is correlated by. The request id is reused
/// across reconnects on purpose: the server only echoes it, and the GDScript seam matches on it.
struct SeatIntent {
    faction_id: u32,
    request_id: u64,
    state: SeatState,
    /// When a transient refusal should be retried.
    retry_at: Option<Instant>,
}

/// One question written on the seated socket and not yet answered.
///
/// The list exists for the failure paths only — a healthy answer is matched by `request_id` off the
/// reader thread and forwarded within a frame. What it buys is that *no* question can end in
/// silence: the deadline is the bound on a server that never answers, and a dropped socket fails
/// everything it was still holding.
struct PendingQuery {
    request_id: u64,
    deadline: Instant,
}

/// **Ask for the seat that drives `faction_id`.** Returns whether the ask reached the worker; the
/// ANSWER arrives on the query drain under `request_id` (`kind: "seat_claim"`), because a claim is a
/// command that is answered.
pub(crate) fn claim_seat(
    host: &str,
    port: u16,
    faction_id: u32,
    request_id: u64,
) -> Result<(), String> {
    link_sender()
        .send(LinkMessage::Claim {
            endpoint: Endpoint {
                host: host.to_string(),
                port,
            },
            faction_id,
            request_id,
        })
        .map_err(|err| format!("seat claim dispatch error: {err}"))
}

/// **Put one command on the socket it belongs on** — the whole routing rule, stated once. See the
/// module header for why the classification is two variants wide and not forty.
pub(crate) fn dispatch(host: &str, port: u16, envelope: &CommandEnvelope) -> Result<(), String> {
    if is_host_verb(&envelope.payload) {
        return transmit_one_shot(host, port, envelope);
    }
    let (ack_tx, ack_rx) = mpsc::channel();
    link_sender()
        .send(LinkMessage::Send {
            endpoint: Endpoint {
                host: host.to_string(),
                port,
            },
            envelope: Box::new(envelope.clone()),
            ack: ack_tx,
        })
        .map_err(|err| format!("command dispatch error: {err}"))?;
    match ack_rx.recv_timeout(LINK_ACK_TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err("command link did not answer".to_string()),
    }
}

/// **Put one faction-bearing question on the seated socket.**
///
/// Returns whether the ask reached the worker; every later outcome — a socket that would not open,
/// one that died holding the question, a server that never answered — arrives on the query drain
/// under `request_id`, in the same `error` field a server refusal would use. `timeout` is the
/// caller's patience for this question (`bridge/query.rs` chooses it per kind), not a constant here.
pub(crate) fn send_query(
    host: &str,
    port: u16,
    request_id: u64,
    envelope: CommandEnvelope,
    timeout: Duration,
) -> Result<(), String> {
    link_sender()
        .send(LinkMessage::Query {
            endpoint: Endpoint {
                host: host.to_string(),
                port,
            },
            envelope: Box::new(envelope),
            request_id,
            timeout,
        })
        .map_err(|err| format!("query dispatch error: {err}"))
}

/// **The verbs that move the world for EVERYONE and are therefore the host's, not a seat's.**
///
/// Mirrors `core_sim`'s `is_host_verb`: the server refuses `Turn` and `Rollback` from a connection
/// that holds a seat, so they must NOT ride the seated link. A throwaway connection holds no seat,
/// which makes it the operator channel these two belong on.
fn is_host_verb(payload: &CommandPayload) -> bool {
    matches!(
        payload,
        CommandPayload::Turn { .. } | CommandPayload::Rollback { .. }
    )
}

/// **Connect, write one frame, drop the socket** — the fire-and-forget path, now reserved for the
/// host verbs. It is what the whole client used to do, and the seat gate is why it no longer can:
/// this connection is unseated by construction, which is exactly what a host verb requires and
/// exactly what a faction-bearing one must not have.
fn transmit_one_shot(host: &str, port: u16, envelope: &CommandEnvelope) -> Result<(), String> {
    let bytes = encode(envelope)?;
    let addr = format!("{host}:{port}");
    let mut stream = TcpStream::connect(&addr).map_err(|err| format!("connect error: {err}"))?;
    let _ = stream.set_nodelay(true);
    write_frame(&mut stream, &bytes)
}

fn encode(envelope: &CommandEnvelope) -> Result<Vec<u8>, String> {
    let bytes = envelope
        .encode_to_vec()
        .map_err(|CommandEncodeError::Encode(err)| format!("encode error: {err}"))?;
    // Refused before it is written, by the same bound the server refuses a read at: putting a frame
    // on the wire that the other end will drop the connection over is worse than not sending it.
    if bytes.len() > MAX_PROTO_FRAME {
        return Err(format!(
            "command frame {} exceeds the {MAX_PROTO_FRAME}-byte bound",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// The framing both directions of this socket use: a 4-byte little-endian length, then the payload.
fn write_frame(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), String> {
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|err| format!("length write error: {err}"))?;
    stream
        .write_all(bytes)
        .map_err(|err| format!("payload write error: {err}"))?;
    stream.flush().map_err(|err| format!("flush error: {err}"))
}

static LINK_SENDER: OnceLock<Mutex<Sender<LinkMessage>>> = OnceLock::new();

/// The worker's inbox, stood up on first use. `Mutex` for the same reason [`query`]'s answer sender
/// needs one: an `mpsc` sender is `Send` but not `Sync`.
fn link_sender() -> Sender<LinkMessage> {
    let sender = LINK_SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<LinkMessage>();
        let worker_tx = tx.clone();
        thread::Builder::new()
            .name("command-link-worker".into())
            .spawn(move || LinkWorker::new(rx, worker_tx).run())
            .expect("failed to spawn command link worker thread");
        Mutex::new(tx)
    });
    // A poisoned lock would mean a panic while cloning a sender, which cannot happen; treating it as
    // recoverable keeps a poisoned mutex from taking the command path down with it.
    match sender.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// **The one owner of the socket, the seat, and the reconnect clock.**
///
/// Everything reaches it as a [`LinkMessage`], so there is exactly one thread that may decide the
/// seat's state and exactly one that writes to the socket.
struct LinkWorker {
    inbox: Receiver<LinkMessage>,
    /// Handed to each reader thread so it can post what it reads back to this loop.
    postbox: Sender<LinkMessage>,
    endpoint: Option<Endpoint>,
    /// The write half of the live connection, or `None` while disconnected.
    write: Option<TcpStream>,
    /// Rises on every connect, so a frame or a drop notice from a socket already replaced is
    /// recognised as stale and ignored.
    generation: u64,
    seat: Option<SeatIntent>,
    reconnect_at: Option<Instant>,
    /// Questions written on the live socket and still owed an answer. Short by construction — a
    /// sheet asks one question per interaction — so a linear scan is the right shape.
    pending: Vec<PendingQuery>,
}

impl LinkWorker {
    fn new(inbox: Receiver<LinkMessage>, postbox: Sender<LinkMessage>) -> Self {
        Self {
            inbox,
            postbox,
            endpoint: None,
            write: None,
            generation: 0,
            seat: None,
            reconnect_at: None,
            pending: Vec::new(),
        }
    }

    fn run(mut self) {
        loop {
            let received = match self.next_wake() {
                Some(at) => {
                    let now = Instant::now();
                    let wait = at.saturating_duration_since(now);
                    self.inbox.recv_timeout(wait)
                }
                // Nothing is scheduled: block, so an idle client costs nothing.
                None => self
                    .inbox
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected),
            };
            match received {
                Ok(message) => self.handle(message),
                Err(RecvTimeoutError::Timeout) => self.on_deadline(),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    /// The earliest scheduled wake — a pending reconnect, a claim retry, or a claim that has gone
    /// unanswered. `None` means "block until something arrives".
    fn next_wake(&self) -> Option<Instant> {
        let mut earliest = self.reconnect_at;
        if let Some(seat) = &self.seat {
            if let Some(retry_at) = seat.retry_at {
                earliest = Some(min_instant(earliest, retry_at));
            }
            if let SeatState::Pending { deadline, .. } = seat.state {
                earliest = Some(min_instant(earliest, deadline));
            }
        }
        for query in &self.pending {
            earliest = Some(min_instant(earliest, query.deadline));
        }
        earliest
    }

    fn handle(&mut self, message: LinkMessage) {
        match message {
            LinkMessage::Claim {
                endpoint,
                faction_id,
                request_id,
            } => self.on_claim(endpoint, faction_id, request_id),
            LinkMessage::Send {
                endpoint,
                envelope,
                ack,
            } => {
                let result = self.on_send(endpoint, &envelope);
                let _ = ack.send(result);
            }
            LinkMessage::Query {
                endpoint,
                envelope,
                request_id,
                timeout,
            } => self.on_query(endpoint, &envelope, request_id, timeout),
            LinkMessage::Inbound { generation, frame } => self.on_inbound(generation, &frame),
            LinkMessage::Dropped { generation, detail } => self.on_dropped(generation, &detail),
        }
    }

    fn on_claim(&mut self, endpoint: Endpoint, faction_id: u32, request_id: u64) {
        self.retarget(&endpoint);
        self.seat = Some(SeatIntent {
            faction_id,
            request_id,
            state: SeatState::Unclaimed,
            retry_at: None,
        });
        if self.write.is_none() {
            // Connecting IS claiming — the claim is the first frame on a fresh socket.
            if self.connect().is_err() {
                // **A server that is not listening YET is not a refusal.** The client is routinely
                // started beside the server (`run_stack.sh`) and can reach here before the listener
                // is up, so the claim waits for the reconnect rather than telling the player their
                // seat was denied — a report that would be wrong seconds later. The deadline is armed
                // from the ASK, so a server that never comes up is still reported, once.
                self.arm_claim_deadline();
                self.reconnect_at = Some(Instant::now() + RECONNECT_BACKOFF);
            }
            return;
        }
        self.write_claim();
    }

    fn on_send(&mut self, endpoint: Endpoint, envelope: &CommandEnvelope) -> Result<(), String> {
        self.retarget(&endpoint);
        let bytes = encode(envelope)?;
        if self.write.is_none() {
            // The seat is re-claimed inside `connect`, ahead of this command on the same socket, so
            // the server has seated the connection before it gates what follows.
            if let Err(err) = self.connect() {
                if self.seat.is_some() {
                    self.reconnect_at = Some(Instant::now() + RECONNECT_BACKOFF);
                }
                return Err(err);
            }
        }
        let Some(stream) = self.write.as_mut() else {
            return Err("command link is not connected".to_string());
        };
        let generation = self.generation;
        match write_frame(stream, &bytes) {
            Ok(()) => Ok(()),
            Err(err) => {
                // **Written at most once.** A retry on a fresh socket could double-apply a command
                // the server already had, so the caller is told instead.
                self.on_dropped(generation, &err);
                Err(err)
            }
        }
    }

    /// **Write one question, and remember that it is owed an answer.**
    ///
    /// ⛔ **No queue, and none is needed.** A question asked while the link is down connects here,
    /// and `connect` writes the seat claim as the first frame on the new socket — the server reads
    /// one connection's frames in order into one channel (`handle_proto_client`), so the claim is
    /// registered before the question is evaluated. Wire order IS the queue, which is why a question
    /// asked mid-reconnect needs no holding pen and a question asked before the claim was granted is
    /// still asked from a seated connection.
    ///
    /// A socket that will not open fails the question **now** rather than parking it: the sheet
    /// renders one failure line and reopening it asks again, whereas a parked question would come
    /// back at an arbitrary later moment against a world that had moved on.
    fn on_query(
        &mut self,
        endpoint: Endpoint,
        envelope: &CommandEnvelope,
        request_id: u64,
        timeout: Duration,
    ) {
        self.retarget(&endpoint);
        let bytes = match encode(envelope) {
            Ok(bytes) => bytes,
            Err(err) => {
                query::deliver_reply(request_id, Err(err));
                return;
            }
        };
        if self.write.is_none() {
            if let Err(err) = self.connect() {
                if self.seat.is_some() {
                    self.reconnect_at = Some(Instant::now() + RECONNECT_BACKOFF);
                }
                query::deliver_reply(request_id, Err(err));
                return;
            }
        }
        // Recorded as owed BEFORE the write, so a write that fails is reported by the one path that
        // reports every other way this socket loses an answer, rather than by a second one here.
        self.pending.push(PendingQuery {
            request_id,
            deadline: Instant::now() + timeout,
        });
        let generation = self.generation;
        let written = match self.write.as_mut() {
            Some(stream) => write_frame(stream, &bytes),
            None => Err(QUERY_DETAIL_NOT_CONNECTED.to_string()),
        };
        if let Err(err) = written {
            self.on_dropped(generation, &err);
        }
    }

    /// **Fail every outstanding question with one detail.** Called where the answer can no longer
    /// arrive — the socket that owed it is gone, or its deadline passed. Nothing is re-asked: a
    /// re-sent question would be one the sheet did not ask, answered against a later world.
    fn fail_pending(&mut self, detail: &str, expired_only: bool) {
        let now = Instant::now();
        let mut failed = Vec::new();
        self.pending.retain(|query| {
            if expired_only && now < query.deadline {
                return true;
            }
            failed.push(query.request_id);
            false
        });
        for request_id in failed {
            query::deliver_reply(request_id, Err(detail.to_string()));
        }
    }

    /// A different endpoint is a different server: drop what we hold rather than writing this
    /// client's seat traffic to whatever answers on the old address.
    fn retarget(&mut self, endpoint: &Endpoint) {
        if self.endpoint.as_ref() == Some(endpoint) {
            return;
        }
        self.endpoint = Some(endpoint.clone());
        self.write = None;
        self.generation += 1;
        self.reconnect_at = None;
        // Whatever was outstanding was asked of the OLD server and will never be answered here.
        self.fail_pending(QUERY_DETAIL_ENDPOINT_CHANGED, false);
        if let Some(seat) = self.seat.as_mut() {
            seat.state = SeatState::Unclaimed;
            seat.retry_at = None;
        }
    }

    fn connect(&mut self) -> Result<(), String> {
        let Some(endpoint) = self.endpoint.clone() else {
            return Err("command link has no endpoint".to_string());
        };
        let stream =
            TcpStream::connect(endpoint.addr()).map_err(|err| format!("connect error: {err}"))?;
        let _ = stream.set_nodelay(true);
        let read_half = stream
            .try_clone()
            .map_err(|err| format!("read half unavailable: {err}"))?;
        self.generation += 1;
        self.reconnect_at = None;
        self.write = Some(stream);
        let generation = self.generation;
        let postbox = self.postbox.clone();
        thread::Builder::new()
            .name("command-link-reader".into())
            .spawn(move || read_replies(read_half, generation, postbox))
            .map_err(|err| format!("reader thread unavailable: {err}"))?;
        if self.seat.is_some() {
            self.write_claim();
        }
        Ok(())
    }

    /// Put the claim on the wire and arm its answer deadline. The one place a `ClaimSeat` is written,
    /// so a claim can never be sent without something watching for its reply.
    fn write_claim(&mut self) {
        let Some((request_id, faction_id, state)) = self
            .seat
            .as_ref()
            .map(|seat| (seat.request_id, seat.faction_id, seat.state))
        else {
            return;
        };
        // **Never claim twice on one connection.** The server refuses that (`already_seated`) on
        // purpose, so a granted seat is left alone until the connection it was granted on goes away.
        if state == SeatState::Granted {
            return;
        }
        let attempts_left = match state {
            SeatState::Pending { attempts_left, .. } => attempts_left,
            _ => SEAT_CLAIM_ATTEMPTS,
        };
        let envelope = CommandEnvelope {
            payload: CommandPayload::ClaimSeat {
                request_id,
                faction_id,
            },
            correlation_id: None,
        };
        let bytes = match encode(&envelope) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.report_claim_failure(&err);
                return;
            }
        };
        let generation = self.generation;
        let Some(stream) = self.write.as_mut() else {
            self.report_claim_failure("command link is not connected");
            return;
        };
        if let Err(err) = write_frame(stream, &bytes) {
            self.on_dropped(generation, &err);
            return;
        }
        if let Some(seat) = self.seat.as_mut() {
            seat.retry_at = None;
            seat.state = SeatState::Pending {
                deadline: Instant::now() + SEAT_CLAIM_REPLY_TIMEOUT,
                attempts_left,
            };
        }
    }

    /// Start the clock on a claim that has been ASKED FOR but not yet written, so a server that
    /// never accepts a connection is reported exactly once instead of waiting forever in silence.
    fn arm_claim_deadline(&mut self) {
        if let Some(seat) = self.seat.as_mut() {
            if matches!(seat.state, SeatState::Unclaimed) {
                seat.state = SeatState::Pending {
                    deadline: Instant::now() + SEAT_CLAIM_REPLY_TIMEOUT,
                    attempts_left: SEAT_CLAIM_ATTEMPTS,
                };
            }
        }
    }

    fn on_inbound(&mut self, generation: u64, frame: &[u8]) {
        if generation != self.generation {
            return;
        }
        let reply = match QueryReplyEnvelope::decode(frame) {
            Ok(reply) => reply,
            Err(err) => {
                // A frame this end cannot parse means the stream is no longer understood; dropping
                // it is the same conclusion the server's own framing loop reaches.
                self.on_dropped(generation, &format!("reply decode error: {err}"));
                return;
            }
        };
        let claim_id = self
            .seat
            .as_ref()
            .filter(|seat| matches!(seat.state, SeatState::Pending { .. }))
            .map(|seat| seat.request_id);
        if claim_id == Some(reply.request_id) {
            self.on_claim_reply(reply.reply);
            return;
        }
        // Anything else answered on this socket belongs to whichever seam spent that id — this
        // worker's own outstanding questions among them, so the one that just landed stops being
        // owed an answer before it is forwarded.
        self.pending
            .retain(|query| query.request_id != reply.request_id);
        query::deliver_reply(reply.request_id, Ok(reply.reply));
    }

    fn on_claim_reply(&mut self, reply: QueryReply) {
        let QueryReply::SeatClaim(answer) = reply else {
            // An answer of another kind under the claim's id is a desynchronised stream, not a seat
            // decision; forward it so the id is not left dangling and stop waiting on it.
            if let Some(seat) = self.seat.as_mut() {
                seat.state = SeatState::Unclaimed;
            }
            return;
        };
        let Some(seat) = self.seat.as_mut() else {
            return;
        };
        if answer.ok {
            seat.state = SeatState::Granted;
            seat.retry_at = None;
            let request_id = seat.request_id;
            query::deliver_reply(request_id, Ok(QueryReply::SeatClaim(answer)));
            return;
        }
        let attempts_left = match seat.state {
            SeatState::Pending { attempts_left, .. } => attempts_left,
            _ => 0,
        };
        // **The one retryable refusal.** On a reconnect the seat can still be registered to the
        // socket that just died, because the server frees it when its own read loop notices the EOF.
        if answer.error == seat_error::SEAT_OCCUPIED && attempts_left > 0 {
            seat.state = SeatState::Pending {
                deadline: Instant::now() + SEAT_CLAIM_REPLY_TIMEOUT,
                attempts_left: attempts_left - 1,
            };
            seat.retry_at = Some(Instant::now() + SEAT_CLAIM_RETRY_BACKOFF);
            return;
        }
        seat.state = SeatState::Refused;
        seat.retry_at = None;
        let request_id = seat.request_id;
        query::deliver_reply(request_id, Ok(QueryReply::SeatClaim(answer)));
    }

    fn on_dropped(&mut self, generation: u64, detail: &str) {
        if generation != self.generation {
            return;
        }
        self.write = None;
        self.generation += 1;
        // The answers were owed on the socket that just died; the reply direction died with it.
        self.fail_pending(detail, false);
        if let Some(seat) = self.seat.as_mut() {
            // The seat went with the socket. It is re-claimed on the next connection, which is what
            // keeps a mid-session server restart from silently unseating the player.
            seat.state = SeatState::Unclaimed;
            seat.retry_at = None;
            self.reconnect_at = Some(Instant::now() + RECONNECT_BACKOFF);
            // …and the clock starts again from HERE, so a reconnect that never lands is reported
            // rather than leaving the player holding no seat and hearing nothing about it. A
            // reconnect inside the window re-grants the seat and the seam says so.
            self.arm_claim_deadline();
        }
    }

    fn on_deadline(&mut self) {
        let now = Instant::now();
        self.fail_pending(QUERY_DETAIL_UNANSWERED, true);
        if let Some(at) = self.reconnect_at {
            if now >= at && self.write.is_none() {
                if let Err(err) = self.connect() {
                    let _ = err;
                    // Still down. Keep trying: the seat is only held while a connection is.
                    self.reconnect_at = Some(now + RECONNECT_BACKOFF);
                }
            }
        }
        let retry_due = self
            .seat
            .as_ref()
            .and_then(|seat| seat.retry_at)
            .is_some_and(|at| now >= at);
        if retry_due && self.write.is_some() {
            self.write_claim();
            return;
        }
        let expired = match self.seat.as_ref().map(|seat| seat.state) {
            Some(SeatState::Pending { deadline, .. }) => now >= deadline,
            _ => false,
        };
        if expired {
            if let Some(seat) = self.seat.as_mut() {
                seat.state = SeatState::Unclaimed;
                seat.retry_at = None;
            }
            self.report_claim_failure("seat claim went unanswered");
        }
    }

    /// **A claim that never got an answer is still an answer to the player.** It lands in the same
    /// field a refusal does, carrying the transport token, so the seam renders one honest line
    /// whether the server said no or said nothing.
    fn report_claim_failure(&self, detail: &str) {
        let Some(seat) = self.seat.as_ref() else {
            return;
        };
        query::deliver_reply(seat.request_id, Err(detail.to_string()));
    }
}

fn min_instant(current: Option<Instant>, candidate: Instant) -> Instant {
    match current {
        Some(existing) if existing <= candidate => existing,
        _ => candidate,
    }
}

/// One connection's reader: frame, post, repeat — the mirror of the server's own framing loop.
///
/// It posts back to the worker rather than to the drain directly, because a claim's answer is the
/// worker's business (it decides whether to retry) and only the worker knows which generation is
/// live.
fn read_replies(stream: TcpStream, generation: u64, postbox: Sender<LinkMessage>) {
    let mut reader = std::io::BufReader::new(stream);
    loop {
        let mut len_buf = [0u8; 4];
        if let Err(err) = reader.read_exact(&mut len_buf) {
            let _ = postbox.send(LinkMessage::Dropped {
                generation,
                detail: format!("reply length read error: {err}"),
            });
            return;
        }
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        // Refused rather than read: the server bounds its own writes by the same number, so a frame
        // past it is a desynchronised stream and reading it would consume the next one.
        if frame_len == 0 || frame_len > MAX_PROTO_FRAME {
            let _ = postbox.send(LinkMessage::Dropped {
                generation,
                detail: format!("reply frame {frame_len} is out of bounds"),
            });
            return;
        }
        let mut frame = vec![0u8; frame_len];
        if let Err(err) = reader.read_exact(&mut frame) {
            let _ = postbox.send(LinkMessage::Dropped {
                generation,
                detail: format!("reply read error: {err}"),
            });
            return;
        }
        if postbox
            .send(LinkMessage::Inbound { generation, frame })
            .is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_verbs_do_not_ride_the_seated_link() {
        // `Turn` and `Rollback` are refused from a seated connection, so they must be the two the
        // router keeps on a throwaway socket — and nothing else may join them by accident.
        assert!(is_host_verb(&CommandPayload::Turn { steps: 1 }));
        assert!(is_host_verb(&CommandPayload::Rollback { tick: 3 }));
        assert!(!is_host_verb(&CommandPayload::Orders {
            faction_id: 0,
            directive: sim_runtime::OrdersDirective::Ready,
        }));
        assert!(!is_host_verb(&CommandPayload::ClaimSeat {
            request_id: 1,
            faction_id: 0,
        }));
        assert!(!is_host_verb(&CommandPayload::Resync));
    }

    #[test]
    fn an_oversized_frame_is_refused_before_the_wire() {
        // The server drops a connection over a frame past the bound, so a client that wrote one
        // would take its own seat down with it.
        let payload = CommandPayload::ExportMap {
            path: Some("x".repeat(MAX_PROTO_FRAME + 1)),
        };
        let envelope = CommandEnvelope {
            payload,
            correlation_id: None,
        };
        assert!(encode(&envelope).is_err());
    }
}
