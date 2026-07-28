use std::io::{self, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, select, Receiver, Sender, TrySendError};

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
}

impl Default for SnapshotServerLimits {
    fn default() -> Self {
        Self {
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            frame_queue_capacity: DEFAULT_FRAME_QUEUE_CAPACITY,
            pending_client_capacity: DEFAULT_PENDING_CLIENT_CAPACITY,
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

pub struct SnapshotServer {
    /// Frames ride as the `Arc` the encoder produced. They used to be copied into a fresh `Vec`
    /// per broadcast — a whole frame memcpy'd on the turn thread for nothing, since the sender
    /// already owned a shared, immutable buffer.
    sender: Sender<Arc<Vec<u8>>>,
    connected: Arc<AtomicUsize>,
    dropped_frames: Arc<AtomicU64>,
}

impl SnapshotServer {
    /// Queues a frame for every connected client. **Never blocks**, whatever the clients do.
    ///
    /// This runs on the publisher thread (`snapshot::publish`, via [`FrameSink`]), which must
    /// never wait on a socket — see `.claude/rules/core_sim/turn-profiling.md`. `try_send` is what
    /// guarantees that.
    pub fn broadcast(&self, bytes: &Arc<Vec<u8>>) {
        match self.sender.try_send(Arc::clone(bytes)) {
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
    /// handed over is not counted yet.
    pub fn connected_clients(&self) -> usize {
        self.connected.load(Ordering::Relaxed)
    }

    /// Frames queued for the broadcast thread but not yet written.
    pub fn queued_frames(&self) -> usize {
        self.sender.len()
    }

    /// Frames discarded because the queue was full — see [`SnapshotServer::broadcast`]. Monotonic;
    /// a rising number means some client is not draining its socket.
    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames.load(Ordering::Relaxed)
    }
}

/// The snapshot socket is where the publisher thread's frames go (`snapshot::publish`).
impl FrameSink for SnapshotServer {
    fn publish_frame(&self, frame: &Arc<Vec<u8>>) {
        self.broadcast(frame);
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
    let (sender, frames) = bounded::<Arc<Vec<u8>>>(limits.frame_queue_capacity);
    let (client_sender, clients) = bounded::<TcpStream>(limits.pending_client_capacity);
    let connected = Arc::new(AtomicUsize::new(0));
    let dropped_frames = Arc::new(AtomicU64::new(0));

    // The listener blocks. The old loop polled it non-blocking every 50 ms so the same thread could
    // also drain the frame channel; with the threads split there is nothing else for this one to do
    // between connections.
    if let Err(err) = listener.set_nonblocking(false) {
        log::warn!("Failed to set blocking mode on the snapshot listener: {err}");
    }

    spawn_accept_thread(listener, client_sender, limits.write_timeout);
    spawn_broadcast_thread(clients, frames, Arc::clone(&connected));

    SnapshotServer {
        sender,
        connected,
        dropped_frames,
    }
}

fn spawn_accept_thread(
    listener: TcpListener,
    client_sender: Sender<TcpStream>,
    write_timeout: Duration,
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
                match client_sender.try_send(stream) {
                    Ok(()) => {}
                    Err(TrySendError::Full(stream)) => {
                        // Bounds file descriptors under a connection flood: the socket closes with
                        // the dropped stream rather than piling up awaiting registration.
                        log::warn!(
                            "Snapshot client backlog is full; refusing {addr} without registering it"
                        );
                        drop(stream);
                    }
                    Err(TrySendError::Disconnected(stream)) => {
                        // The broadcaster exits when `SnapshotServer` is dropped, i.e. at
                        // shutdown. There is nothing left to accept for, so stop rather than
                        // accept-and-close every future connection.
                        log::info!("Broadcast thread is gone; snapshot listener is shutting down");
                        drop(stream);
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

fn spawn_broadcast_thread(
    new_clients: Receiver<TcpStream>,
    frames: Receiver<Arc<Vec<u8>>>,
    connected: Arc<AtomicUsize>,
) {
    thread::spawn(move || {
        // Owned outright by this thread — no `Arc<Mutex<…>>`, because the only other thread that
        // ever wanted the list (accept) now hands sockets over the channel instead.
        let mut clients: Vec<TcpStream> = Vec::new();
        loop {
            select! {
                recv(new_clients) -> client => match client {
                    Ok(stream) => {
                        clients.push(stream);
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
                recv(frames) -> frame => match frame {
                    Ok(frame) => {
                        broadcast_frame(&mut clients, &frame);
                        connected.store(clients.len(), Ordering::Relaxed);
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

fn write_frame(stream: &mut TcpStream, frame: &[u8]) -> io::Result<()> {
    let len = frame.len() as u32;
    let mut buffer = Vec::with_capacity(4 + frame.len());
    buffer.extend_from_slice(&len.to_le_bytes());
    buffer.extend_from_slice(frame);
    stream.write_all(&buffer)
}

fn broadcast_frame(clients: &mut Vec<TcpStream>, frame: &[u8]) {
    clients.retain_mut(|stream| match write_frame(stream, frame) {
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
    });
}
