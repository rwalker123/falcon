use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, select, Receiver, Sender, TrySendError};

use crate::orders::FactionId;
use crate::seats::SeatToken;
use crate::snapshot::FrameSink;

/// How long the accept thread waits before retrying after a genuine `accept()` error, so a
/// persistent error (an exhausted fd table, say) cannot hot-spin a core.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(200);

/// The bounds that keep one wedged client from taking the server with it.
///
/// These exist as a struct rather than as bare constants so the socket tests can shrink them
/// (`core_sim/tests/snapshot_socket.rs`): the behaviours worth pinning are all "what happens once
/// a limit is reached", and waiting out the shipped limits would make the suite minutes long.
pub struct SnapshotServerLimits {
    /// Per-client write timeout. A client that cannot absorb a frame within it is dropped.
    pub write_timeout: Duration,
    /// How many encoded frames may sit between the publisher and the socket before frames are
    /// dropped instead of queued.
    pub frame_queue_capacity: usize,
    /// How many accepted-but-not-yet-registered clients may be in flight from the accept thread
    /// to the broadcast thread.
    pub pending_client_capacity: usize,
    /// How long an accepted stream socket has to present its [`SEAT_TOKEN_BYTES`]-byte seat token
    /// before it is registered as **unseated**. See [`spawn_handshake`].
    pub handshake_timeout: Duration,
}

impl Default for SnapshotServerLimits {
    fn default() -> Self {
        Self {
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            frame_queue_capacity: DEFAULT_FRAME_QUEUE_CAPACITY,
            pending_client_capacity: DEFAULT_PENDING_CLIENT_CAPACITY,
            handshake_timeout: DEFAULT_HANDSHAKE_TIMEOUT,
        }
    }
}

/// Five seconds is ~two orders of magnitude beyond any legitimate pause in a client that polls the
/// socket every rendered frame; past it the process is wedged (a breakpoint, a `SIGSTOP`, a hung
/// render thread), not slow.
///
/// It is deliberately generous rather than snug because the cost of a false positive is total: the
/// Godot client does **not** reconnect a dropped snapshot stream (`SnapshotLoader.enable_stream` is
/// called once), so a client dropped here is a dead session until the player restarts.
const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Caps the queue's memory at 64 × the largest frame. Deltas are small and a full frame is rare, so
/// a healthy client never leaves more than one or two queued — this ceiling is only ever approached
/// while a client is wedged.
const DEFAULT_FRAME_QUEUE_CAPACITY: usize = 64;

/// A file-descriptor bound on a connection flood, not a tuning knob: 32 accepted sockets may await
/// registration before the accept thread starts closing new ones.
const DEFAULT_PENDING_CLIENT_CAPACITY: usize = 32;

/// **The seat token a stream connection presents**: the `u64` inside the [`SeatToken`] its command
/// connection was handed by the claim reply, little-endian.
///
/// A **fixed-width** greeting rather than a framed message because it is the whole protocol: the
/// socket is one-way from here on, and a length prefix would only describe a payload whose size is
/// a constant. [`SeatToken::NONE`] (`0`) is the explicit *"I hold no seat"* greeting, which
/// is how a tool skips the wait below instead of sitting out [`DEFAULT_HANDSHAKE_TIMEOUT`].
pub const SEAT_TOKEN_BYTES: usize = 8;

/// How long an accepted stream socket has to present its seat token.
///
/// Generous on purpose, and it costs a **seated** client nothing: the client already holds its token
/// when it opens this socket (the claim is answered on the command socket first), so it writes
/// immediately. The wait is only ever paid by a connection that presents nothing — an Inspector, a
/// `nc`, a client from before the token existed — and what it buys is that such a connection is
/// registered as *unseated* rather than dropped. Two seconds is far beyond a local write and short
/// enough that a watching tool is not left wondering.
const DEFAULT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct SnapshotServer {
    /// Frames ride as the `Arc` the encoder produced, **each labelled with the seat it was captured
    /// for**. They used to be copied into a fresh `Vec` per broadcast — a whole frame memcpy'd on
    /// the turn thread for nothing, since the sender already owned a shared, immutable buffer.
    sender: Sender<(FactionId, Arc<Vec<u8>>)>,
    connected: Arc<AtomicUsize>,
    seated: Arc<AtomicUsize>,
    dropped_frames: Arc<AtomicU64>,
    /// **Seat token → the seat it holds.** Written by the server's main loop as seats are claimed
    /// and released, read once per accepted stream socket by its handshake. A `Mutex` rather than a
    /// channel because both sides are human-paced: a claim is a player action and a lookup happens
    /// once per connection.
    seats: Arc<Mutex<HashMap<SeatToken, FactionId>>>,
}

impl SnapshotServer {
    /// **Queues a frame for the clients holding `seat`.** Never blocks, whatever the clients do.
    ///
    /// This runs on the publisher thread (`snapshot::publish`, via [`FrameSink`]), which must
    /// never wait on a socket — see `.claude/rules/core_sim/turn-profiling.md`. `try_send` is what
    /// guarantees that.
    ///
    /// **It is addressed, not broadcast.** A frame is one viewer's world since PR #648, so the
    /// clients that do not hold this seat — another seat's, or none — are not sent it. An unseated
    /// connection therefore receives nothing at all rather than somebody else's view; see
    /// `.claude/rules/core_sim/snapshot-socket.md`.
    pub fn deliver(&self, seat: FactionId, bytes: &Arc<Vec<u8>>) {
        match self.sender.try_send((seat, Arc::clone(bytes))) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                // Dropping a frame is recoverable, and that is the whole reason a bounded queue is
                // allowed to drop rather than block: the client notices the gap by itself — a delta
                // whose `baseFrameSeq` names a frame it never applied is discarded and raises
                // `resync_needed` (`clients/godot_thin_client/native/src/bridge/decoder.rs`) — and
                // sends `resync`, which the server answers with a fresh full frame. Do NOT "fix"
                // this into a blocking send: that would put the publisher thread back behind a
                // wedged client's TCP window, which is the defect this arm exists to prevent.
                let dropped = self.dropped_frames.fetch_add(1, Ordering::Relaxed) + 1;
                log::warn!(
                    "Snapshot frame queue full; dropped frame ({} dropped so far). A client is not \
                     reading; it will resync once it drains.",
                    dropped
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                log::error!("Failed to queue snapshot frame: broadcast thread is gone");
            }
        }
    }

    /// Clients currently registered with the broadcast thread. A client accepted but not yet
    /// handed over — one still in its seat handshake — is not counted yet.
    pub fn connected_clients(&self) -> usize {
        self.connected.load(Ordering::Relaxed)
    }

    /// Registered clients that have presented a seat token. The rest are **unseated** and receive
    /// nothing — see [`StreamClient`].
    pub fn seated_clients(&self) -> usize {
        self.seated.load(Ordering::Relaxed)
    }

    /// **Replace the token→seat table with the live claims.** Called by the server's main loop
    /// wherever `SeatRegistry` changes — a claim, a release, a world rebuild that strands one.
    ///
    /// Wholesale rather than one binding at a time, and **read per frame rather than resolved once
    /// per connection**, because both halves can move under a live stream: a connection that
    /// released its seat must stop receiving that seat's frames *even though its stream socket is
    /// still open*, or the next occupant's world would go to its predecessor.
    pub fn set_seats(&self, claims: &[(FactionId, SeatToken)]) {
        let mut seats = self.lock_seats();
        seats.clear();
        for (seat, token) in claims {
            seats.insert(*token, *seat);
        }
    }

    fn lock_seats(&self) -> std::sync::MutexGuard<'_, HashMap<SeatToken, FactionId>> {
        self.seats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Frames queued for the broadcast thread but not yet written.
    pub fn queued_frames(&self) -> usize {
        self.sender.len()
    }

    /// Frames discarded because the queue was full — see [`SnapshotServer::deliver`]. Monotonic;
    /// a rising number means some client is not draining its socket.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }
}

/// The snapshot socket is where the publisher thread's frames go (`snapshot::publish`).
impl FrameSink for SnapshotServer {
    fn publish_frame(&self, seat: FactionId, frame: &Arc<Vec<u8>>) {
        self.deliver(seat, frame);
    }
}

/// Starts the snapshot broadcaster on an already-bound listener, with the shipped limits.
///
/// The listener is bound up front by `port_alloc::allocate`, so binding can no
/// longer fail here — a busy port is caught before the server starts rather
/// than silently disabling broadcasting on a running server.
///
/// A newly accepted client is deliberately sent **nothing** until the next
/// broadcast: any cached frame belongs to whatever world existed at accept time,
/// which is not necessarily the world the connecting client asked for, and the
/// client cannot tell the two apart — so the server must not offer the guess.
pub fn start_snapshot_server(listener: TcpListener) -> SnapshotServer {
    start_snapshot_server_with_limits(listener, SnapshotServerLimits::default())
}

/// [`start_snapshot_server`] with the limits injected — see [`SnapshotServerLimits`].
///
/// **Accepting and writing are separate threads on purpose.** They used to be one, whose iteration
/// was `accept() → register → write every queued frame to every client`; a client that stopped
/// reading filled its TCP window, the blocking `write_all` never returned, and the thread that
/// could no longer write was the same thread that could no longer accept. Splitting them means a
/// wedged client can cost at most itself: the accept thread never writes to a client, and the
/// broadcast thread owns the client list outright, so no lock joins the two either.
pub fn start_snapshot_server_with_limits(
    listener: TcpListener,
    limits: SnapshotServerLimits,
) -> SnapshotServer {
    let (sender, frames) = bounded::<(FactionId, Arc<Vec<u8>>)>(limits.frame_queue_capacity);
    let (client_sender, clients) = bounded::<StreamClient>(limits.pending_client_capacity);
    let (greeting_sender, greetings) = bounded::<Greeting>(limits.pending_client_capacity);
    let connected = Arc::new(AtomicUsize::new(0));
    let seated = Arc::new(AtomicUsize::new(0));
    let dropped_frames = Arc::new(AtomicU64::new(0));
    let seats: Arc<Mutex<HashMap<SeatToken, FactionId>>> = Arc::new(Mutex::new(HashMap::new()));

    // The listener blocks. The old loop polled it non-blocking every 50 ms so the same thread could
    // also drain the frame channel; with the threads split there is nothing else for this one to do
    // between connections.
    if let Err(err) = listener.set_nonblocking(false) {
        log::warn!("Failed to set blocking mode on the snapshot listener: {err}");
    }

    spawn_accept_thread(
        listener,
        client_sender,
        greeting_sender,
        limits.write_timeout,
        limits.handshake_timeout,
    );
    spawn_broadcast_thread(
        clients,
        greetings,
        frames,
        Arc::clone(&connected),
        Arc::clone(&seated),
        Arc::clone(&seats),
    );

    SnapshotServer {
        sender,
        connected,
        seated,
        dropped_frames,
        seats,
    }
}

/// Mints one id per accepted stream socket, so a greeting arriving later can name the connection it
/// belongs to. Local to this module and never on the wire — the client's identity on the wire is its
/// **seat token**, which is a different thing minted by a different socket.
static NEXT_STREAM_CLIENT: AtomicU64 = AtomicU64::new(0);

/// A registered stream connection: the socket, and **the seat token it presented**, if it has yet.
///
/// It holds the token rather than the seat the token resolved to, because a claim can end while the
/// stream socket stays open — so which seat a connection is entitled to is asked at delivery, off
/// the live table, and a released token resolves to nothing from that moment.
///
/// [`SeatToken::NONE`] is an *unseated* connection: it presented no token, presented the explicit
/// "no seat" one, or has not greeted yet — as is a **wrong or guessed** token, which resolves to no
/// seat in the table below. It stays registered (the socket is not churned)
/// and is delivered nothing, which is the only reading of "no seat" that cannot leak — the
/// alternative, falling back to some default faction, hands a tool the private world of whichever
/// faction that turned out to be.
struct StreamClient {
    id: u64,
    stream: TcpStream,
    token: SeatToken,
}

/// A connection's seat token, arriving after the connection itself. See [`spawn_handshake`].
struct Greeting {
    client: u64,
    token: SeatToken,
}

fn spawn_accept_thread(
    listener: TcpListener,
    client_sender: Sender<StreamClient>,
    greeting_sender: Sender<Greeting>,
    write_timeout: Duration,
    handshake_timeout: Duration,
) {
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                log::info!("Snapshot client connected: {}", addr);
                if let Err(err) = stream.set_nodelay(true) {
                    log::warn!("Failed to set TCP_NODELAY for snapshot client {addr}: {err}");
                }
                if let Err(err) = stream.set_write_timeout(Some(write_timeout)) {
                    log::warn!("Failed to set write timeout for snapshot client {addr}: {err}");
                }
                let id = NEXT_STREAM_CLIENT.fetch_add(1, Ordering::Relaxed);
                // ⛔ **THE SOCKET IS HANDED OVER IMMEDIATELY AND ITS TOKEN FOLLOWS SEPARATELY.**
                //
                // Reading the greeting here would let one client that connects and says nothing
                // hold up every other connection for `handshake_timeout` — the head-of-line stall
                // #406 split these threads to remove, in a new costume. Reading it on a per-socket
                // thread that *also* did the handoff would keep accept free but make the handoff
                // order the order tokens happen to arrive in, and the backlog's refusal arm
                // (`the_accept_thread_keeps_running_while_a_write_is_blocked`) is a statement about
                // **accept order**. So the accept thread still registers, in order, and the
                // greeting catches up.
                //
                // A client is therefore **unseated between registration and its greeting**, and
                // receives nothing in that window. That is the dropped-first-frame race
                // `world-handoff.md` already describes, healed the same way: the client asks.
                // Cloned before the handoff, and the handshake spawned only **after** it
                // succeeds: a refused connection has to be *closed*, and a reader thread still
                // holding a dup of that socket would hold it open until its own timeout — which is
                // exactly the EOF `the_accept_thread_keeps_running_while_a_write_is_blocked` waits
                // for.
                //
                // ⛔ **THAT ORDER — HAND THE CLIENT OVER, THEN SPAWN THE HANDSHAKE — IS ALSO WHAT
                // MAKES [`seat_greeting`] TOTAL.** It is what guarantees that a greeting's
                // `StreamClient` is already in the handoff channel by the time the greeting
                // arrives, so draining that channel always finds it. Reversing the two would let a
                // greeting name a client that is in neither the channel nor the list, and that
                // connection would stay unseated for the rest of its life.
                let reader = match stream.try_clone() {
                    Ok(reader) => Some(reader),
                    Err(err) => {
                        log::warn!(
                            "Failed to clone the socket for snapshot client {addr}: {err}; it \
                             cannot present a seat token and will receive no frames"
                        );
                        None
                    }
                };
                match client_sender.try_send(StreamClient {
                    id,
                    stream,
                    token: SeatToken::NONE,
                }) {
                    Ok(()) => {
                        if let Some(reader) = reader {
                            spawn_handshake(
                                reader,
                                id,
                                addr.to_string(),
                                greeting_sender.clone(),
                                handshake_timeout,
                            );
                        }
                    }
                    Err(TrySendError::Full(client)) => {
                        // Bounds file descriptors under a connection flood: the socket closes with
                        // the dropped stream rather than piling up awaiting registration.
                        log::warn!(
                            "Snapshot client backlog is full; refusing {addr} without registering it"
                        );
                        drop(client);
                    }
                    Err(TrySendError::Disconnected(client)) => {
                        // The broadcaster exits when `SnapshotServer` is dropped, i.e. at
                        // shutdown. There is nothing left to accept for, so stop rather than
                        // accept-and-close every future connection.
                        log::info!("Broadcast thread is gone; snapshot listener is shutting down");
                        drop(client);
                        break;
                    }
                }
            }
            Err(err) => {
                log::error!("Error accepting snapshot client: {}", err);
                thread::sleep(ACCEPT_ERROR_BACKOFF);
            }
        }
    });
}

/// Read one connection's seat token and forward it to the broadcaster.
///
/// **Two ways to stay unseated**: the peer sends nothing within `handshake_timeout`, or it sends
/// [`SeatToken::NONE`] — the explicit "I hold no seat", which is how a tool skips the wait. A
/// third, a token naming no live claim, is not decided here at all: the token is resolved to a seat
/// at **delivery**, so a stream that greets a moment before its claim registers is seated as soon as
/// the claim lands, and one whose claim ends stops being seated the moment it does.
///
/// Either way the connection stays registered rather than being dropped — a connected tool that
/// receives no frames is a defined state; a socket the server closes under it is not.
fn spawn_handshake(
    reader: TcpStream,
    client: u64,
    addr: String,
    greeting_sender: Sender<Greeting>,
    handshake_timeout: Duration,
) {
    thread::spawn(move || {
        let token = read_seat_token(&reader, &addr, handshake_timeout).unwrap_or(SeatToken::NONE);
        if token == SeatToken::NONE {
            log::info!("Snapshot client {addr} holds no seat; it will receive no frames");
            // Nothing to say: the broadcaster already holds this client as unseated.
            return;
        }
        // ⛔ **THE TOKEN IS A SECRET AND DOES NOT GO IN THE LOG** — see [`SeatToken`]. The line says
        // *that* a token arrived, on which socket; who holds the seat is already logged where the
        // claim was granted (`seat.claimed`, with the connection id and the faction), and this
        // thread could not name that connection anyway — a stream socket knows only the secret.
        log::info!(
            "Snapshot client {addr} presented a seat token; it will receive the frames of whatever \
             seat holds that token"
        );
        match greeting_sender.try_send(Greeting { client, token }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => log::warn!(
                "Snapshot greeting backlog is full; {addr} stays unseated and will receive no \
                 frames until it reconnects"
            ),
            Err(TrySendError::Disconnected(_)) => {
                log::info!("Broadcast thread is gone; dropping the greeting from {addr}")
            }
        }
    });
}

/// The greeting: exactly [`SEAT_TOKEN_BYTES`] little-endian bytes, or nothing.
fn read_seat_token(stream: &TcpStream, addr: &str, timeout: Duration) -> Option<SeatToken> {
    if let Err(err) = stream.set_read_timeout(Some(timeout)) {
        log::warn!("Failed to set the handshake read timeout for snapshot client {addr}: {err}");
        return None;
    }
    let mut token = [0u8; SEAT_TOKEN_BYTES];
    let mut reader = stream;
    match reader.read_exact(&mut token) {
        Ok(()) => Some(SeatToken::from_wire(u64::from_le_bytes(token))),
        Err(err) => {
            log::info!(
                "Snapshot client {addr} presented no seat token ({err}); it will receive no frames"
            );
            None
        }
    }
}

fn spawn_broadcast_thread(
    new_clients: Receiver<StreamClient>,
    greetings: Receiver<Greeting>,
    frames: Receiver<(FactionId, Arc<Vec<u8>>)>,
    connected: Arc<AtomicUsize>,
    seated: Arc<AtomicUsize>,
    seats: Arc<Mutex<HashMap<SeatToken, FactionId>>>,
) {
    thread::spawn(move || {
        // Owned outright by this thread — no `Arc<Mutex<…>>`, because the only other thread that
        // ever wanted the list (accept) now hands sockets over the channel instead.
        let mut clients: Vec<StreamClient> = Vec::new();
        loop {
            select! {
                recv(new_clients) -> client => match client {
                    Ok(client) => {
                        clients.push(client);
                        connected.store(clients.len(), Ordering::Relaxed);
                    }
                    Err(_) => {
                        // The accept thread outlives this one in every ordinary shutdown, so
                        // reaching here means it died by panic. Exiting is mandatory either way:
                        // a disconnected channel is permanently ready, so leaving the arm in
                        // `select!` would spin a core forever.
                        log::error!("Snapshot accept thread is gone; stopping the broadcaster");
                        break;
                    }
                },
                recv(greetings) -> greeting => match greeting {
                    Ok(greeting) => {
                        seat_greeting(&mut clients, &new_clients, &greeting);
                        // The drain inside `seat_greeting` can register clients, so **both** counts
                        // are restated here — the greeting arm is a second way into the client list.
                        connected.store(clients.len(), Ordering::Relaxed);
                        seated.store(seated_count(&clients), Ordering::Relaxed);
                    }
                    Err(_) => {
                        log::error!("Snapshot handshake channel is gone; stopping the broadcaster");
                        break;
                    }
                },
                recv(frames) -> frame => match frame {
                    Ok((seat, frame)) => {
                        // One lock per frame, not per client: the table is rewritten only when a
                        // seat is claimed or released, both human-paced.
                        let holders: Vec<SeatToken> = seats
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .iter()
                            .filter(|(_, held)| **held == seat)
                            .map(|(token, _)| *token)
                            .collect();
                        deliver_frame(&mut clients, &holders, &frame);
                        connected.store(clients.len(), Ordering::Relaxed);
                        seated.store(seated_count(&clients), Ordering::Relaxed);
                    }
                    Err(_) => {
                        // The frame sender dies with `SnapshotServer`, i.e. at shutdown.
                        log::info!("Snapshot frame channel closed; stopping the broadcaster");
                        break;
                    }
                },
            }
        }
    });
}

/// **Attach a greeting's seat token to the connection it names**, registering any client still in
/// the handoff channel first if that connection is not in the list yet.
///
/// ⛔ **A GREETING CAN OUTRUN ITS OWN CLIENT, AND THE DRAIN IS WHAT MAKES THE LOOKUP TOTAL.** The
/// accept thread sends the [`StreamClient`] and only *then* spawns the handshake, so a client whose
/// token is already sitting in the socket buffer can have its greeting queued while its
/// `StreamClient` is still in `new_clients` — and `select!` picks at random among ready operations,
/// so the broadcaster may take the greeting first. Without the drain that greeting names nobody, the
/// token is discarded, nothing re-sends it, and the connection stays registered-but-unseated **for
/// the rest of its life**, receiving no frames at all.
///
/// The drain closes it rather than narrowing it: the send on `new_clients` **happens before**
/// `spawn_handshake` is called, and a send that fails closes the connection without spawning a
/// handshake at all — so whenever a greeting exists, its client is already in that channel or in the
/// list. Biasing the `select!` toward `new_clients` would only shrink the window, and reading the
/// greeting on the accept thread would restore the head-of-line stall the thread split exists to
/// remove (`.claude/rules/core_sim/snapshot-socket.md`).
fn seat_greeting(
    clients: &mut Vec<StreamClient>,
    new_clients: &Receiver<StreamClient>,
    greeting: &Greeting,
) {
    if attach_token(clients, greeting) {
        return;
    }
    clients.extend(new_clients.try_iter());
    if !attach_token(clients, greeting) {
        // Now genuinely nobody: the connection was dropped (a wedged peer evicted while its
        // handshake was still in flight), so the token has no socket to belong to.
        log::info!(
            "A snapshot seat token arrived for a connection that is no longer registered; \
             discarding it"
        );
    }
}

/// Sets `greeting`'s token on the client it names, reporting whether that client was found.
fn attach_token(clients: &mut [StreamClient], greeting: &Greeting) -> bool {
    match clients
        .iter_mut()
        .find(|client| client.id == greeting.client)
    {
        Some(client) => {
            client.token = greeting.token;
            true
        }
        None => false,
    }
}

/// How many registered clients have presented a seat token — the count [`SnapshotServer::seated_clients`]
/// publishes.
fn seated_count(clients: &[StreamClient]) -> usize {
    clients
        .iter()
        .filter(|client| client.token != SeatToken::NONE)
        .count()
}

fn write_frame(stream: &mut TcpStream, frame: &[u8]) -> io::Result<()> {
    let len = frame.len() as u32;
    let mut buffer = Vec::with_capacity(4 + frame.len());
    buffer.extend_from_slice(&len.to_le_bytes());
    buffer.extend_from_slice(frame);
    stream.write_all(&buffer)
}

/// Write one seat's frame to the clients holding that seat, dropping any that cannot take it.
///
/// **A client holding another seat, or none, is skipped rather than written to** — that is the whole
/// of per-seat delivery, and the wedged-client contract above it is unchanged: a write that fails
/// still closes the connection, because a partially written frame desynchronises the stream.
fn deliver_frame(clients: &mut Vec<StreamClient>, holders: &[SeatToken], frame: &[u8]) {
    clients.retain_mut(|client| {
        if !holders.contains(&client.token) {
            return true;
        }
        match write_frame(&mut client.stream, frame) {
            Ok(_) => true,
            Err(err) => {
                // Dropping the client is **mandatory**, not a convenience. The wire format is a `u32`
                // length prefix followed by exactly that many payload bytes, and a `write_all` that
                // fails — a timeout above all — may already have put part of the frame on the wire.
                // The stream is desynchronised from that byte on: the client's next length read would
                // be payload bytes reinterpreted as a length. There is no resuming it, so the only
                // correct move is to close the connection — which is not free, since the Godot client
                // does not reconnect one (`.claude/rules/core_sim/snapshot-socket.md`), and is why the
                // write timeout is sized to fire only for a wedged peer.
                log::warn!("Dropping snapshot client: {}", err);
                false
            }
        }
    });
}

/// Guards for the ordering of the two handoffs the broadcaster selects over — see [`seat_greeting`].
///
/// These reach past `start_snapshot_server_with_limits` and drive `spawn_broadcast_thread` (and its
/// greeting helper) directly, because the property is about **which of two ready channels the
/// broadcaster takes first**, and nothing above this layer can choose that: the accept and handshake
/// threads decide when each message is queued, and `select!` decides at random which is served. A
/// socket-level test can only hope to hit the ordering, which is why the shipped suite passed on the
/// defect these pin.
#[cfg(test)]
mod greeting_order_tests {
    use super::*;
    use std::time::Instant;

    /// The token every client here presents. Any non-[`SeatToken::NONE`] value serves — what these
    /// pin is whether the token is *attached*, not which seat it resolves to.
    const TEST_TOKEN: SeatToken = SeatToken::from_wire(7);
    /// The id the accept thread would have minted for the connection under test.
    const TEST_CLIENT: u64 = 0;
    /// Capacity for the test channels. Nothing here queues more than a couple of messages; the
    /// figure only has to be more than that.
    const TEST_CHANNEL_CAPACITY: usize = 8;

    /// How many times [`a_greeting_taken_before_its_client_still_seats_that_client`] replays the
    /// ordering. One is enough on the current `crossbeam-channel`, whose `select!` shuffle is a
    /// thread-local seeded with a constant — so a fresh broadcast thread facing two ready channels
    /// resolves them the same way every run — but that is an implementation detail of the
    /// dependency, and repeating makes the guard survive its RNG changing.
    const RACE_REPLAYS: usize = 16;
    /// How long one replay waits for the client to be seated before calling it stranded. Generous
    /// because scheduling three threads on a loaded box is not the failure under test; a passing
    /// replay never comes close to it.
    const SEATING_DEADLINE: Duration = Duration::from_secs(5);
    /// Gap between polls while waiting out [`SEATING_DEADLINE`].
    const POLL_INTERVAL: Duration = Duration::from_millis(5);

    /// A real, connected socket to hang a [`StreamClient`] on. The peer end is returned so it stays
    /// open for the life of the test — a closed peer would let the broadcaster drop the client for
    /// an unrelated reason.
    fn connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback listener");
        let addr = listener.local_addr().expect("read the listener address");
        let peer = TcpStream::connect(addr).expect("connect to the loopback listener");
        let (stream, _) = listener.accept().expect("accept the loopback connection");
        (peer, stream)
    }

    fn pending_client(stream: TcpStream) -> StreamClient {
        StreamClient {
            id: TEST_CLIENT,
            stream,
            token: SeatToken::NONE,
        }
    }

    /// **The ordering, with no threads in it at all**: the client is in the handoff channel — where
    /// the accept thread put it before spawning the handshake — but has not been registered yet, and
    /// its greeting is served first.
    #[test]
    fn a_greeting_names_a_client_still_in_the_handoff_channel() {
        let (_peer, stream) = connected_pair();
        let (client_sender, new_clients) = bounded::<StreamClient>(TEST_CHANNEL_CAPACITY);
        client_sender
            .send(pending_client(stream))
            .expect("hand the client over");

        let mut clients: Vec<StreamClient> = Vec::new();
        seat_greeting(
            &mut clients,
            &new_clients,
            &Greeting {
                client: TEST_CLIENT,
                token: TEST_TOKEN,
            },
        );

        assert_eq!(
            clients.len(),
            1,
            "the drained client must be registered, not merely inspected"
        );
        assert_eq!(clients[0].token, TEST_TOKEN, "its token must be attached");
        assert_eq!(seated_count(&clients), 1);
    }

    /// A greeting for a connection in neither the list nor the channel is still discarded — the
    /// drain must not turn "already dropped" into a hang or a phantom client.
    #[test]
    fn a_greeting_for_a_dropped_client_is_discarded() {
        let (client_sender, new_clients) = bounded::<StreamClient>(TEST_CHANNEL_CAPACITY);
        let mut clients: Vec<StreamClient> = Vec::new();
        seat_greeting(
            &mut clients,
            &new_clients,
            &Greeting {
                client: TEST_CLIENT,
                token: TEST_TOKEN,
            },
        );
        assert!(clients.is_empty());
        assert_eq!(seated_count(&clients), 0);
        drop(client_sender);
    }

    /// The same ordering through the **real broadcast thread**: both handoffs are queued before it
    /// starts, in the order the accept thread produces them, and the client must end up seated
    /// whichever arm `select!` serves first.
    #[test]
    fn a_greeting_taken_before_its_client_still_seats_that_client() {
        for replay in 0..RACE_REPLAYS {
            let (client_sender, new_clients) = bounded::<StreamClient>(TEST_CHANNEL_CAPACITY);
            let (greeting_sender, greetings) = bounded::<Greeting>(TEST_CHANNEL_CAPACITY);
            let (frame_sender, frames) =
                bounded::<(FactionId, Arc<Vec<u8>>)>(TEST_CHANNEL_CAPACITY);
            let connected = Arc::new(AtomicUsize::new(0));
            let seated = Arc::new(AtomicUsize::new(0));
            let seats = Arc::new(Mutex::new(HashMap::new()));
            let (_peer, stream) = connected_pair();

            // The accept thread's order: the client is handed over first, and only then does the
            // handshake it spawns produce the greeting. Both are queued before the broadcaster runs,
            // which is the state a loaded box reaches whenever the broadcaster is busy across the
            // window between the two sends.
            client_sender
                .send(pending_client(stream))
                .expect("hand the client over");
            greeting_sender
                .send(Greeting {
                    client: TEST_CLIENT,
                    token: TEST_TOKEN,
                })
                .expect("present the seat token");

            spawn_broadcast_thread(
                new_clients,
                greetings,
                frames,
                Arc::clone(&connected),
                Arc::clone(&seated),
                seats,
            );

            let deadline = Instant::now() + SEATING_DEADLINE;
            while seated.load(Ordering::Relaxed) == 0 && Instant::now() < deadline {
                thread::sleep(POLL_INTERVAL);
            }
            assert_eq!(
                seated.load(Ordering::Relaxed),
                1,
                "replay {replay}: the client greeted correctly and was never seated, so it would \
                 receive no frame for the life of its connection"
            );
            assert_eq!(
                connected.load(Ordering::Relaxed),
                1,
                "replay {replay}: a client registered by the greeting arm must be counted there too"
            );

            // Dropping the frame sender is what stops the broadcast thread, exactly as dropping
            // `SnapshotServer` does.
            drop(frame_sender);
            drop(client_sender);
            drop(greeting_sender);
        }
    }
}
