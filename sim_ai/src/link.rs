//! **The Link** — claim · greet · hold · reconnect · resync (`docs/plan_ai_driver.md` §7).
//!
//! The Rust shape of the client's seated command connection
//! (`clients/godot_thin_client/native/src/bridge/command_link.rs`), with no Godot in it. The brain
//! never sees a socket: it is handed a [`SeatView`](crate::view::SeatView) and hands back
//! `CommandPayload`s, and everything about *how* those reach the server lives here.
//!
//! ## The wire sequence
//!
//! 1. Connect to the **command** port and write `ClaimSeat { request_id, faction_id }` as the first
//!    frame. Replies come back on the same socket as `QueryReplyEnvelope` frames, read on a thread
//!    and routed to the pending claim by `request_id`.
//! 2. A grant carries the seat **token**. Only then connect to the **stream** port and write the
//!    token as its first [`SEAT_TOKEN_BYTES`] bytes, little-endian, unframed — after which the
//!    socket is one-way and carries `[u32 LE length][FlatBuffers envelope]` frames forever.
//! 3. Ask for the world (`Resync`): a stream connection is sent nothing it did not ask for
//!    (`world-handoff.md`), so the first full frame is requested, not awaited.
//!
//! ⛔ **One command connection for the life of the process.** The seat belongs to the connection
//! (`factions.md` → Seats): a socket-per-command claims a seat and drops it with the socket, and
//! every later command is refused. Every command goes on the one link, and a dropped link is
//! rebuilt, re-claimed (a fresh token) and the stream re-greeted with the new token — a stale token
//! is a stream that is silently sent nothing.
//!
//! **Host verbs are never sent** (`Turn`, `Rollback`, `SetFogEnabled`): the first two are refused
//! from a seated connection, the third discloses every seat's world. [`Link::send`] debug-asserts
//! on all three, so a brain that tries one fails a test rather than a game.
//!
//! **The token is a secret.** [`SeatToken`]'s `Debug` is redacted and it has no `Display`,
//! mirroring `core_sim::SeatToken`, so it cannot reach a log line by accident.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use sim_runtime::commands::{seat_error, QueryPayload, SeatClaimReply};
use sim_runtime::{
    CommandEnvelope, CommandPayload, QueryReply, QueryReplyEnvelope, MAX_PROTO_FRAME,
};
use tracing::{info, warn};

// -------------------------------------------------------------------------------------------------
// Restated wire constants
// -------------------------------------------------------------------------------------------------

/// **The greeting's width.** Restated from `core_sim/src/network.rs` (`SEAT_TOKEN_BYTES`) by the
/// same rule `SnapshotStream.gd` restates it: this crate must not link `core_sim`, so the two
/// values are duplicated on purpose and the server's is the authority.
pub const SEAT_TOKEN_BYTES: usize = 8;

/// The framing prefix on both sockets: a `u32`, little-endian, giving the payload length.
const LENGTH_PREFIX_BYTES: usize = std::mem::size_of::<u32>();

/// The retry constants of the client's seated link (`command_link.rs`), restated so both occupants
/// race a freed seat the same way.
///
/// A reconnect can arrive before the server's read loop has noticed the old socket's EOF and freed
/// the seat, so `seat_occupied` is retried this many times, this far apart, before it is a refusal.
pub const SEAT_CLAIM_ATTEMPTS: u32 = 8;
pub const SEAT_CLAIM_RETRY_BACKOFF: Duration = Duration::from_millis(250);
/// How long a claim may go unanswered before the link is treated as dead.
pub const SEAT_CLAIM_REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// How long to wait before rebuilding a dropped command link.
pub const RECONNECT_BACKOFF: Duration = Duration::from_millis(500);
/// **`unknown_seat` is retried forever, slowly.** The seat this process was told to fill may not
/// exist *yet* — the launcher spawns it off a roster event and a world rebuild can re-seat the
/// faction later — so an unknown seat is a wait, not a refusal.
pub const UNKNOWN_SEAT_RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// How many bytes a stream frame may announce. A FlatBuffers full snapshot of a large map is a few
/// megabytes; anything past this is a desynchronised stream, not a frame.
const MAX_STREAM_FRAME: usize = 256 * 1024 * 1024;

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// The secret a granted claim minted. Redacted `Debug`, no `Display`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SeatToken(u64);

impl SeatToken {
    /// The greeting bytes: the token, little-endian.
    fn greeting(self) -> [u8; SEAT_TOKEN_BYTES] {
        self.0.to_le_bytes()
    }
}

impl std::fmt::Debug for SeatToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeatToken(<redacted>)")
    }
}

/// Where the two sockets are.
#[derive(Debug, Clone, Copy)]
pub struct Endpoints {
    pub command: SocketAddr,
    pub stream: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("could not connect to the command socket at {addr}: {source}")]
    Connect {
        addr: SocketAddr,
        #[source]
        source: io::Error,
    },
    #[error("the command socket dropped: {0}")]
    CommandDropped(String),
    #[error("the claim of seat {faction} was refused: {token}")]
    ClaimRefused { faction: u32, token: String },
    #[error(
        "the claim of seat {faction} went unanswered for {:?}",
        SEAT_CLAIM_REPLY_TIMEOUT
    )]
    ClaimUnanswered { faction: u32 },
    #[error("could not connect to the stream socket at {addr}: {source}")]
    StreamConnect {
        addr: SocketAddr,
        #[source]
        source: io::Error,
    },
    #[error("a command could not be encoded: {0}")]
    Encode(String),
}

/// What the link's reader threads hand the main loop.
#[derive(Debug)]
pub enum LinkEvent {
    /// One stream frame: a FlatBuffers envelope, length prefix stripped.
    Frame(Vec<u8>),
    /// A reply on the command socket that is not the pending claim's.
    Reply(QueryReplyEnvelope),
    /// The command socket died; the seat went with it. [`Link::reconnect`] is the answer.
    CommandDropped(String),
    /// The stream socket died; the seat is intact. [`Link::reopen_stream`] is the answer.
    StreamDropped(String),
}

/// A reader thread's message, stamped with the connection generation that spawned the thread so a
/// stale thread's last words are ignored after a reconnect.
enum Inbound {
    Reply {
        generation: u64,
        envelope: QueryReplyEnvelope,
    },
    CommandDropped {
        generation: u64,
        detail: String,
    },
    Frame {
        generation: u64,
        bytes: Vec<u8>,
    },
    StreamDropped {
        generation: u64,
        detail: String,
    },
}

/// The seated link: one command connection, one stream connection, one token.
pub struct Link {
    endpoints: Endpoints,
    faction: u32,
    command: TcpStream,
    stream: TcpStream,
    token: SeatToken,
    inbound: Receiver<Inbound>,
    sender: Sender<Inbound>,
    /// Bumped on every **command** reconnect; the reply reader carries the value it was spawned
    /// with, so a thread from before a reconnect is ignored.
    generation: u64,
    /// Bumped on every **stream** reopen — and on a reconnect, which opens a new stream too.
    ///
    /// ⛔ **The two generations are separate because there is exactly one reply reader per command
    /// socket, for its lifetime.** Reopening the stream leaves the command socket untouched, so it
    /// must not respawn that reader: two `read_exact` calls on one socket split a reply frame
    /// between them — one takes the 4-byte length prefix, the other the payload — and each reopen
    /// would leak another blocked thread and fd clone.
    stream_generation: u64,
    next_request_id: u64,
}

impl Link {
    /// Claim `faction`'s seat and greet the stream — the whole sequence in the module docs.
    pub fn connect(endpoints: Endpoints, faction: u32) -> Result<Self, LinkError> {
        let (sender, inbound) = mpsc::channel();
        let generation = 1;
        let mut next_request_id = 1;
        let (command, token) = claim_seat(
            endpoints,
            faction,
            generation,
            &sender,
            &inbound,
            &mut next_request_id,
        )?;
        let stream_generation = generation;
        let stream = open_stream(endpoints, token, stream_generation, &sender)?;
        info!(faction, "seat claimed and stream greeted");
        Ok(Self {
            endpoints,
            faction,
            command,
            stream,
            token,
            inbound,
            sender,
            generation,
            stream_generation,
            next_request_id,
        })
    }

    /// The next event from either socket, or `None` when `timeout` elapses with nothing to say.
    pub fn next_event(&mut self, timeout: Duration) -> Option<LinkEvent> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let message = match self.inbound.recv_timeout(remaining) {
                Ok(message) => message,
                Err(RecvTimeoutError::Timeout) => return None,
                Err(RecvTimeoutError::Disconnected) => {
                    unreachable!("the link holds its own sender, so the channel cannot close")
                }
            };
            let event = match message {
                Inbound::Reply {
                    generation,
                    envelope,
                } if generation == self.generation => LinkEvent::Reply(envelope),
                Inbound::CommandDropped { generation, detail } if generation == self.generation => {
                    LinkEvent::CommandDropped(detail)
                }
                Inbound::Frame { generation, bytes } if generation == self.stream_generation => {
                    LinkEvent::Frame(bytes)
                }
                Inbound::StreamDropped { generation, detail }
                    if generation == self.stream_generation =>
                {
                    LinkEvent::StreamDropped(detail)
                }
                // A thread from before a reconnect or a stream reopen: whatever it read belongs to
                // a socket that no longer holds the seat.
                _ => continue,
            };
            return Some(event);
        }
    }

    /// Write one command on the seated connection.
    pub fn send(&mut self, payload: CommandPayload) -> Result<(), LinkError> {
        debug_assert!(
            !is_host_verb(&payload),
            "a seated process never sends a host verb: {payload:?}"
        );
        let envelope = CommandEnvelope {
            payload,
            correlation_id: None,
        };
        let bytes = envelope
            .encode_to_vec()
            .map_err(|err| LinkError::Encode(err.to_string()))?;
        write_frame(&mut self.command, &bytes)
            .map_err(|err| LinkError::CommandDropped(err.to_string()))
    }

    /// Ask the server for a full frame of this seat's world.
    pub fn resync(&mut self) -> Result<(), LinkError> {
        self.send(CommandPayload::Resync)
    }

    /// The command socket died: rebuild it, re-claim (a fresh token), and re-greet the stream with
    /// the new token. Blocks through [`RECONNECT_BACKOFF`] and the claim retries.
    pub fn reconnect(&mut self) -> Result<(), LinkError> {
        warn!(faction = self.faction, "command link dropped; reconnecting");
        thread::sleep(RECONNECT_BACKOFF);
        self.generation += 1;
        let (command, token) = claim_seat(
            self.endpoints,
            self.faction,
            self.generation,
            &self.sender,
            &self.inbound,
            &mut self.next_request_id,
        )?;
        self.command = command;
        self.token = token;
        // The old stream's token names nothing now; close it so its reader sees EOF, and open a
        // fresh one greeting with the token this claim minted.
        self.stream_generation += 1;
        let _ = self.stream.shutdown(Shutdown::Both);
        self.stream = open_stream(
            self.endpoints,
            self.token,
            self.stream_generation,
            &self.sender,
        )?;
        info!(
            faction = self.faction,
            "seat re-claimed and stream re-greeted"
        );
        Ok(())
    }

    /// The stream socket died on its own: reopen it with the token the seat still holds.
    ///
    /// Only the **stream** generation moves. The command socket is untouched and its one reply
    /// reader keeps reading it under the generation it was spawned with — see the
    /// `stream_generation` field.
    pub fn reopen_stream(&mut self) -> Result<(), LinkError> {
        warn!(faction = self.faction, "stream dropped; reopening");
        thread::sleep(RECONNECT_BACKOFF);
        self.stream_generation += 1;
        let _ = self.stream.shutdown(Shutdown::Both);
        self.stream = open_stream(
            self.endpoints,
            self.token,
            self.stream_generation,
            &self.sender,
        )?;
        Ok(())
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        // Closing the command socket is what releases the seat (`Command::ReleaseSeat` from the
        // server's read loop); the stream goes with it.
        let _ = self.command.shutdown(Shutdown::Both);
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

// -------------------------------------------------------------------------------------------------
// The unseated connection
// -------------------------------------------------------------------------------------------------

/// **A command connection that claims no seat** — the bench's world builder (`crate::bench`).
///
/// It sends the verbs that name no faction (`new_game`) and holds no seat, exactly as the scenario
/// tests' builder does: `factions.md` → Seats says an unseated connection is right for world verbs
/// and nothing else. It is not a host — it never sends `Turn` — and it is dropped once the world
/// exists.
pub struct UnseatedConnection {
    socket: TcpStream,
    next_request_id: u64,
}

impl UnseatedConnection {
    pub fn connect(addr: SocketAddr) -> io::Result<Self> {
        let socket = TcpStream::connect(addr)?;
        let _ = socket.set_nodelay(true);
        Ok(Self {
            socket,
            next_request_id: 1,
        })
    }

    /// Write one world verb.
    pub fn send(&mut self, payload: CommandPayload) -> io::Result<()> {
        debug_assert!(
            !is_host_verb(&payload),
            "the bench's builder is not a host: {payload:?}"
        );
        let envelope = CommandEnvelope {
            payload,
            correlation_id: None,
        };
        let bytes = envelope
            .encode_to_vec()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        write_frame(&mut self.socket, &bytes)
    }

    /// Block until everything sent before this has been applied: a `ListSaves` question names
    /// no faction, so an unseated connection is answered, and it is answered **in order** behind
    /// whatever was written before it.
    pub fn sync(&mut self, timeout: Duration) -> io::Result<()> {
        self.ask(QueryPayload::ListSaves, timeout).map(|_| ())
    }

    /// Ask one question that names no faction (`ListSaves`, `FactionCapacity`) and block for its
    /// answer. A faction-bearing question is refused from an unseated connection, and is never
    /// this connection's to ask.
    pub fn ask(&mut self, query: QueryPayload, timeout: Duration) -> io::Result<QueryReply> {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        self.send(CommandPayload::Query { request_id, query })?;
        self.socket.set_read_timeout(Some(timeout))?;
        loop {
            let bytes = read_frame(&mut self.socket, MAX_PROTO_FRAME)?;
            let envelope = QueryReplyEnvelope::decode(&bytes)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
            if envelope.request_id == request_id {
                return Ok(envelope.reply);
            }
        }
    }
}

/// `Turn` and `Rollback` are refused from a seated connection; `SetFogEnabled` discloses every
/// seat's world. None is ever this process's to send.
fn is_host_verb(payload: &CommandPayload) -> bool {
    matches!(
        payload,
        CommandPayload::Turn { .. }
            | CommandPayload::Rollback { .. }
            | CommandPayload::SetFogEnabled { .. }
    )
}

// -------------------------------------------------------------------------------------------------
// The claim
// -------------------------------------------------------------------------------------------------

/// Connect the command socket and claim `faction` on it, retrying the two retryable refusals.
fn claim_seat(
    endpoints: Endpoints,
    faction: u32,
    generation: u64,
    sender: &Sender<Inbound>,
    inbound: &Receiver<Inbound>,
    next_request_id: &mut u64,
) -> Result<(TcpStream, SeatToken), LinkError> {
    let command = TcpStream::connect(endpoints.command).map_err(|source| LinkError::Connect {
        addr: endpoints.command,
        source,
    })?;
    let _ = command.set_nodelay(true);
    spawn_reply_reader(&command, generation, sender)?;
    let mut command = command;

    let mut occupied_attempts_left = SEAT_CLAIM_ATTEMPTS;
    loop {
        let request_id = *next_request_id;
        *next_request_id += 1;
        let claim = CommandEnvelope {
            payload: CommandPayload::ClaimSeat {
                request_id,
                faction_id: faction,
            },
            correlation_id: None,
        };
        let bytes = claim
            .encode_to_vec()
            .map_err(|err| LinkError::Encode(err.to_string()))?;
        write_frame(&mut command, &bytes)
            .map_err(|err| LinkError::CommandDropped(err.to_string()))?;

        let reply = await_claim_reply(inbound, generation, request_id, faction)?;
        if reply.ok {
            return Ok((command, SeatToken(reply.seat_token)));
        }
        match reply.error.as_str() {
            seat_error::SEAT_OCCUPIED if occupied_attempts_left > 0 => {
                occupied_attempts_left -= 1;
                info!(
                    faction,
                    attempts_left = occupied_attempts_left,
                    "seat occupied; retrying — a freed seat can lag its socket's EOF"
                );
                thread::sleep(SEAT_CLAIM_RETRY_BACKOFF);
            }
            seat_error::UNKNOWN_SEAT => {
                info!(faction, "seat not in this world's roster yet; waiting");
                thread::sleep(UNKNOWN_SEAT_RETRY_BACKOFF);
            }
            token => {
                return Err(LinkError::ClaimRefused {
                    faction,
                    token: token.to_owned(),
                })
            }
        }
    }
}

/// Block on the reply channel until the claim under `request_id` is answered.
fn await_claim_reply(
    inbound: &Receiver<Inbound>,
    generation: u64,
    request_id: u64,
    faction: u32,
) -> Result<SeatClaimReply, LinkError> {
    let deadline = Instant::now() + SEAT_CLAIM_REPLY_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match inbound.recv_timeout(remaining) {
            Ok(Inbound::Reply {
                generation: reply_generation,
                envelope,
            }) if reply_generation == generation && envelope.request_id == request_id => {
                return match envelope.reply {
                    QueryReply::SeatClaim(answer) => Ok(answer),
                    other => Err(LinkError::ClaimRefused {
                        faction,
                        token: format!("answered with {other:?} rather than a seat reply"),
                    }),
                };
            }
            Ok(Inbound::CommandDropped {
                generation: dropped_generation,
                detail,
            }) if dropped_generation == generation => {
                return Err(LinkError::CommandDropped(detail));
            }
            // Another generation's message, a frame, or an unrelated reply: not the answer.
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return Err(LinkError::ClaimUnanswered { faction }),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(LinkError::CommandDropped(
                    "reader channel closed".to_owned(),
                ))
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Sockets and threads
// -------------------------------------------------------------------------------------------------

/// Connect the stream socket and greet it with `token`; frames flow to `sender` from a thread.
fn open_stream(
    endpoints: Endpoints,
    token: SeatToken,
    generation: u64,
    sender: &Sender<Inbound>,
) -> Result<TcpStream, LinkError> {
    let mut stream =
        TcpStream::connect(endpoints.stream).map_err(|source| LinkError::StreamConnect {
            addr: endpoints.stream,
            source,
        })?;
    let _ = stream.set_nodelay(true);
    stream
        .write_all(&token.greeting())
        .and_then(|()| stream.flush())
        .map_err(|source| LinkError::StreamConnect {
            addr: endpoints.stream,
            source,
        })?;
    let reader = stream
        .try_clone()
        .map_err(|source| LinkError::StreamConnect {
            addr: endpoints.stream,
            source,
        })?;
    let sender = sender.clone();
    thread::Builder::new()
        .name("sim-ai-stream-reader".into())
        .spawn(move || read_frames(reader, generation, sender))
        .map_err(|source| LinkError::StreamConnect {
            addr: endpoints.stream,
            source,
        })?;
    Ok(stream)
}

fn spawn_reply_reader(
    command: &TcpStream,
    generation: u64,
    sender: &Sender<Inbound>,
) -> Result<(), LinkError> {
    let reader = command
        .try_clone()
        .map_err(|err| LinkError::CommandDropped(err.to_string()))?;
    let sender = sender.clone();
    thread::Builder::new()
        .name("sim-ai-command-reader".into())
        .spawn(move || read_replies(reader, generation, sender))
        .map_err(|err| LinkError::CommandDropped(err.to_string()))?;
    Ok(())
}

/// The command socket's read loop: framed `QueryReplyEnvelope`s until EOF or a bad frame.
fn read_replies(mut socket: TcpStream, generation: u64, sender: Sender<Inbound>) {
    loop {
        let bytes = match read_frame(&mut socket, MAX_PROTO_FRAME) {
            Ok(bytes) => bytes,
            Err(err) => {
                let _ = sender.send(Inbound::CommandDropped {
                    generation,
                    detail: err.to_string(),
                });
                return;
            }
        };
        match QueryReplyEnvelope::decode(&bytes) {
            Ok(envelope) => {
                if sender
                    .send(Inbound::Reply {
                        generation,
                        envelope,
                    })
                    .is_err()
                {
                    return;
                }
            }
            Err(err) => {
                let _ = sender.send(Inbound::CommandDropped {
                    generation,
                    detail: format!("reply decode error: {err}"),
                });
                return;
            }
        }
    }
}

/// The stream socket's read loop: framed FlatBuffers envelopes until EOF.
fn read_frames(mut socket: TcpStream, generation: u64, sender: Sender<Inbound>) {
    loop {
        match read_frame(&mut socket, MAX_STREAM_FRAME) {
            Ok(bytes) => {
                if sender.send(Inbound::Frame { generation, bytes }).is_err() {
                    return;
                }
            }
            Err(err) => {
                let _ = sender.send(Inbound::StreamDropped {
                    generation,
                    detail: err.to_string(),
                });
                return;
            }
        }
    }
}

/// One `[u32 LE length][payload]` frame.
fn read_frame(socket: &mut TcpStream, max_len: usize) -> io::Result<Vec<u8>> {
    let mut prefix = [0u8; LENGTH_PREFIX_BYTES];
    socket.read_exact(&mut prefix)?;
    let len = u32::from_le_bytes(prefix) as usize;
    if len == 0 || len > max_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame announced {len} bytes"),
        ));
    }
    let mut payload = vec![0u8; len];
    socket.read_exact(&mut payload)?;
    Ok(payload)
}

/// Write one `[u32 LE length][payload]` frame, bounded by [`MAX_PROTO_FRAME`].
fn write_frame(socket: &mut TcpStream, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_PROTO_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "command frame of {} bytes exceeds the wire limit",
                payload.len()
            ),
        ));
    }
    let mut framed = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    framed.extend_from_slice(payload);
    socket.write_all(&framed)?;
    socket.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token must not be printable — a `{:?}` in a log line is exactly how it would leak.
    #[test]
    fn the_token_debugs_redacted() {
        const SOME_TOKEN: u64 = 0xDEAD_BEEF;
        assert_eq!(
            format!("{:?}", SeatToken(SOME_TOKEN)),
            "SeatToken(<redacted>)"
        );
    }

    #[test]
    fn the_greeting_is_the_token_little_endian() {
        const SOME_TOKEN: u64 = 0x0102_0304_0506_0708;
        assert_eq!(SeatToken(SOME_TOKEN).greeting(), SOME_TOKEN.to_le_bytes());
        assert_eq!(SeatToken(SOME_TOKEN).greeting().len(), SEAT_TOKEN_BYTES);
    }

    #[test]
    fn the_three_host_verbs_are_never_this_processs_to_send() {
        assert!(is_host_verb(&CommandPayload::Turn { steps: 1 }));
        assert!(is_host_verb(&CommandPayload::Rollback { tick: 1 }));
        assert!(is_host_verb(&CommandPayload::SetFogEnabled {
            enabled: false
        }));
        assert!(!is_host_verb(&CommandPayload::Resync));
    }
}
