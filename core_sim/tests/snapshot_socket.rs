//! Guards for snapshot-socket liveness (#406).
//!
//! The defect these pin was silent in the ordinary sense: a client suspended at a breakpoint or by
//! a hung render thread stops reading, the broadcast `write_all` blocks forever, and because the
//! thread that wrote was the thread that accepted, the server simply stopped taking connections —
//! with an unbounded frame queue growing behind it. Nothing logged, nothing crashed, so the only
//! way to notice was that a new client never connected.
//!
//! Every test injects short limits through `start_snapshot_server_with_limits` so the suite pins
//! *what happens at the limit* without waiting out the shipped five-second timeout, binds
//! `127.0.0.1:0` so concurrent tests cannot collide on a port, and polls to a deadline rather than
//! sleeping a fixed time — the only wall-clock assumption anywhere here is the timeout it injected.

use std::io::{ErrorKind, Read};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use core_sim::network::{start_snapshot_server_with_limits, SnapshotServer, SnapshotServerLimits};

/// How long a poll-until-true will wait before declaring the property broken. Generous because a
/// loaded CI box scheduling three threads is not the failure under test.
const POLL_DEADLINE: Duration = Duration::from_secs(10);
/// Gap between polls: short enough that a passing test is fast, long enough not to spin.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

fn bind_local() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local_addr").port();
    (listener, port)
}

/// Polls `cond` until it holds or [`POLL_DEADLINE`] elapses; returns whether it held.
fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + POLL_DEADLINE;
    loop {
        if cond() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// How many consecutive polls a frame queue must sit still to count as parked — see
/// [`wait_until_broadcaster_parks`]. At [`POLL_INTERVAL`] apiece this is a fifth of a second, five
/// orders of magnitude longer than a write to a socket that has room.
const PARKED_SAMPLES: u32 = 20;

/// Waits until the broadcast thread is parked inside `write_all` on a peer that never reads.
///
/// `queued_frames() > 0` alone does not say that: the broadcaster may simply not have reached the
/// channel yet, and one still sitting in `select!` would pick up the next connection rather than
/// leave it in the handoff slot. Parked is instead *took at least one frame off, then stopped
/// taking them* — `frames_broadcast` is the count published, and the queue holding steady below it
/// for [`PARKED_SAMPLES`] polls is the evidence. How much a socket absorbs before it blocks varies
/// by platform and by autotuning, so how many frames the broadcaster gets through first is
/// deliberately not assumed.
///
/// The state is absorbing, which is what lets the caller assert after the fact: the peer never
/// reads, and the callers set a write timeout beyond the test's own runtime, so a parked
/// broadcaster stays parked.
fn wait_until_broadcaster_parks(server: &SnapshotServer, frames_broadcast: usize) -> bool {
    let deadline = Instant::now() + POLL_DEADLINE;
    let mut last = usize::MAX;
    let mut unchanged = 0;
    loop {
        let queued = server.queued_frames();
        if queued > 0 && queued < frames_broadcast && queued == last {
            unchanged += 1;
            if unchanged >= PARKED_SAMPLES {
                return true;
            }
        } else {
            unchanged = 0;
            last = queued;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Connects, then waits for the broadcast thread to register the socket.
///
/// Registration is what the tests synchronise on rather than the connect: accept, handoff and
/// broadcast are three separate steps, so a frame published between connect and registration is
/// legitimately not delivered (`.claude/rules/core_sim/world-handoff.md` — the dropped-first-frame
/// race, which the client heals by retrying). Waiting on `connected_clients` keeps that known race
/// out of these assertions.
fn connect_and_register(port: u16, server: &SnapshotServer, expected: usize) -> TcpStream {
    let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to snapshot server");
    assert!(
        wait_until(|| server.connected_clients() == expected),
        "the server never registered the client: connected_clients()={} after {:?}, expected {}",
        server.connected_clients(),
        POLL_DEADLINE,
        expected
    );
    stream
}

/// Reads one length-prefixed frame off the wire, the way the client's decoder does.
///
/// Both reads carry the read timeout the caller set, so a timeout here is the failure it is named
/// for: nothing arrived. That is the shape the eviction defect takes on this side of the socket —
/// the broadcaster is still parked in `write_all` on a peer that will never read.
fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).expect(
        "no length prefix arrived within the read timeout: the broadcaster never delivered, which \
         is what a wedged client that is never evicted looks like from a healthy client",
    );
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).expect(
        "a length prefix arrived but its payload did not: a frame reached the wire only partly, \
         which desynchronises the stream permanently",
    );
    payload
}

fn frame(bytes: Vec<u8>) -> Arc<Vec<u8>> {
    Arc::new(bytes)
}

#[test]
fn a_client_that_reads_receives_every_frame() {
    let (listener, port) = bind_local();
    let server = start_snapshot_server_with_limits(
        listener,
        SnapshotServerLimits {
            write_timeout: Duration::from_millis(500),
            ..SnapshotServerLimits::default()
        },
    );

    let mut client = connect_and_register(port, &server, 1);
    client
        .set_read_timeout(Some(POLL_DEADLINE))
        .expect("set read timeout");

    let frames: Vec<Arc<Vec<u8>>> = (0u8..4).map(|i| frame(vec![i; 16 + i as usize])).collect();
    for f in &frames {
        server.broadcast(f);
    }

    for (index, expected) in frames.iter().enumerate() {
        let received = read_frame(&mut client);
        assert_eq!(
            &received,
            expected.as_ref(),
            "frame {index} came back wrong: the broadcaster must deliver every frame, in order, \
             length-prefixed"
        );
    }
    assert_eq!(
        server.dropped_frames(),
        0,
        "no frame may be dropped while the only client is draining its socket"
    );
}

#[test]
fn a_stalled_client_does_not_block_a_new_connection() {
    // Short enough that the stalled client is evicted inside the test, long enough that a merely
    // descheduled thread is not mistaken for a wedged one.
    const WRITE_TIMEOUT: Duration = Duration::from_millis(300);
    // Comfortably more than the socket buffers on either side, so the broadcast thread is
    // genuinely parked inside `write_all` rather than just slow.
    const STALL_FRAME_BYTES: usize = 1 << 20;
    const STALL_FRAME_COUNT: usize = 8;
    // A connect that has to wait on a wedged writer would take the whole write timeout; this is
    // well under it and well over a loopback connect.
    const PROMPT_CONNECT: Duration = Duration::from_millis(100);

    let (listener, port) = bind_local();
    let server = start_snapshot_server_with_limits(
        listener,
        SnapshotServerLimits {
            write_timeout: WRITE_TIMEOUT,
            // Big enough that the frames below queue instead of being dropped — this test is about
            // the accept path, not the queue bound.
            frame_queue_capacity: STALL_FRAME_COUNT * 2,
            ..SnapshotServerLimits::default()
        },
    );

    // Client A connects and never reads a byte.
    let _stalled = connect_and_register(port, &server, 1);

    let big = frame(vec![0xAB; STALL_FRAME_BYTES]);
    for _ in 0..STALL_FRAME_COUNT {
        server.broadcast(&big);
    }

    // (a) The connect itself must complete promptly. This is the weaker half of the headline
    // symptom on purpose: the kernel completes a loopback handshake into the listen backlog whether
    // or not the server ever calls `accept()`, so a *broken* server also lets `connect` return —
    // what it cannot do is register the socket or write to it. (b) below is what proves the
    // connection is live; this only catches an accept path that has started waiting on anything.
    let started = Instant::now();
    let mut fresh = TcpStream::connect(("127.0.0.1", port))
        .expect("a second client must be able to connect while the first is stalled");
    let connect_elapsed = started.elapsed();
    assert!(
        connect_elapsed < PROMPT_CONNECT,
        "connecting took {connect_elapsed:?}; a stalled client must not delay the accept path \
         (>= {PROMPT_CONNECT:?} means accept is waiting behind a blocked write)"
    );

    // (b) The survivor receives whole, correctly framed data. It may first be handed whatever of
    // the backlog was still queued when it registered — those frames must be intact too, since a
    // stalled peer's timed-out write must never leave a half-frame on somebody else's stream.
    fresh
        .set_read_timeout(Some(POLL_DEADLINE))
        .expect("set read timeout");
    let after = frame((0u8..64).collect::<Vec<u8>>());
    server.broadcast(&after);

    let mut delivered = false;
    for _ in 0..=STALL_FRAME_COUNT {
        let received = read_frame(&mut fresh);
        if received == *after.as_ref() {
            delivered = true;
            break;
        }
        assert_eq!(
            received,
            *big.as_ref(),
            "the surviving client received a frame that is neither of the two that were \
             broadcast: the stream is desynchronised, which is what a partially written frame \
             looks like on the wire"
        );
    }
    assert!(
        delivered,
        "the frame broadcast after the stalled peer was evicted never reached the surviving client"
    );

    // (c) And by now the stalled client is gone — the survivor is registered (it just received a
    // frame), so a count of one means the peer whose write timed out was evicted. The order
    // matters: the broadcaster cannot register the new client until it is out of the blocked
    // write, so this can only be checked once delivery has proved registration.
    assert_eq!(
        server.connected_clients(),
        1,
        "connected_clients()={} with only one live reader: a client whose write exceeds the write \
         timeout must be dropped, because its stream can no longer be framed",
        server.connected_clients()
    );
}

/// Pins the half of the #406 fix that the test above cannot: that accepting is a *separate thread*
/// from writing.
///
/// `a_stalled_client_does_not_block_a_new_connection` cannot fail a merged-thread server *on the
/// property it names*. Its connect check concedes as much in place — the kernel completes a loopback
/// handshake into the listen backlog whether or not anyone calls `accept()` — and its delivery check
/// is satisfied one write timeout later, when the lone thread drops the stalled peer, returns from
/// the drain, loops back to `accept()` and registers the survivor. Nothing in it bounds how long
/// that takes, so a regression that merged the threads back while keeping the timeout comes down to
/// whether the survivor happened to register before the frame it is waiting for was broadcast.
///
/// The refusal arm of the pending-client backlog is what discriminates, because closing a connection
/// that has nowhere to go is work only the accept thread can do *while a write is blocked*. With the
/// backlog capped at one and the broadcaster parked in `write_all`, the second connection takes the
/// single handoff slot and the third is accepted, finds no room, and is closed — a clean EOF on the
/// client side. Under a merged thread the third connection is never accepted at all: it waits in the
/// listen backlog and its read times out instead.
#[test]
fn the_accept_thread_keeps_running_while_a_write_is_blocked() {
    // Long enough that it never fires during this test: the wedge has to hold for the whole run,
    // and an evicted client would drain the queue and free the broadcaster.
    const WRITE_TIMEOUT: Duration = Duration::from_secs(120);
    // One handoff slot, so the second connection made during the wedge fills it and the third has
    // nowhere to be put.
    const PENDING_CLIENT_CAPACITY: usize = 1;
    // 32 MiB in total, far beyond the socket buffers on either side of loopback however they are
    // autotuned, so the broadcaster is certain to run out of window part-way through and park.
    const STALL_FRAME_BYTES: usize = 1 << 20;
    const STALL_FRAME_COUNT: usize = 32;
    // Short on purpose: the property under test is that the refusal arrives *promptly*. A generous
    // timeout here would turn a stopped accept path into a slow hang instead of a clear failure.
    const REFUSAL_DEADLINE: Duration = Duration::from_secs(2);

    let (listener, port) = bind_local();
    let server = start_snapshot_server_with_limits(
        listener,
        SnapshotServerLimits {
            write_timeout: WRITE_TIMEOUT,
            // Twice what is broadcast below, so no frame can be dropped — which is what lets a
            // queue shorter than `STALL_FRAME_COUNT` mean exactly one thing: the broadcaster took
            // frames off it and then stopped.
            frame_queue_capacity: STALL_FRAME_COUNT * 2,
            pending_client_capacity: PENDING_CLIENT_CAPACITY,
        },
    );

    // Client A registers and then never reads a byte.
    let _stalled = connect_and_register(port, &server, 1);
    let big = frame(vec![0xEF; STALL_FRAME_BYTES]);
    for _ in 0..STALL_FRAME_COUNT {
        server.broadcast(&big);
    }
    assert!(
        wait_until_broadcaster_parks(&server, STALL_FRAME_COUNT),
        "the broadcaster kept draining {} MiB into a client that never reads, so it is still in \
         `select!` rather than inside `write_all` and the connections below would be serviced \
         normally",
        (STALL_FRAME_BYTES * STALL_FRAME_COUNT) >> 20
    );

    // Client B fills the single handoff slot. The accept thread hands it over; the broadcaster is
    // inside `write_all` and cannot return to `select!` to register it.
    let _pending = TcpStream::connect(("127.0.0.1", port))
        .expect("a client must be able to connect while the broadcaster is wedged");

    // Client C is accepted by the same still-running accept thread, finds the slot taken, and has
    // its socket closed. The accept thread processes connections in backlog order on one thread, so
    // B is already through the handoff by the time C is accepted.
    let mut refused = TcpStream::connect(("127.0.0.1", port))
        .expect("a client must be able to connect while the broadcaster is wedged");
    refused
        .set_read_timeout(Some(REFUSAL_DEADLINE))
        .expect("set read timeout");

    let mut probe = [0u8; 1];
    match refused.read(&mut probe) {
        // The refusal itself: the accept thread dropped the socket, so the stream ends. A close can
        // surface as an RST instead of a FIN depending on the stack; both say the one thing this
        // test asks, that the accept thread answered while a write was blocked.
        Ok(0) => {}
        Err(err) if err.kind() == ErrorKind::ConnectionReset => {}
        Ok(n) => panic!(
            "the refused client received {n} byte(s): with the handoff backlog full the accept \
             thread must close the socket, not register a client the broadcaster has no room for"
        ),
        Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => panic!(
            "nothing came back within {REFUSAL_DEADLINE:?} (connected_clients()={}): the socket was \
             never accepted at all, which means accepting stops while a write is blocked — the \
             single-threaded topology #406 was filed for",
            server.connected_clients()
        ),
        Err(err) => panic!("the refused client's read failed unexpectedly: {err}"),
    }

    assert_eq!(
        server.connected_clients(),
        1,
        "connected_clients()={}: only the wedged client may be registered, because a broadcaster \
         parked in `write_all` cannot return to `select!` to pick up either later connection",
        server.connected_clients()
    );
}

#[test]
fn a_stalled_client_cannot_grow_the_broadcast_queue_without_bound() {
    // Long enough that it never fires during this test: the property here is the queue bound, and a
    // client evicted mid-run would drain the queue and hide it.
    const WRITE_TIMEOUT: Duration = Duration::from_secs(120);
    const QUEUE_CAPACITY: usize = 8;
    const STALL_FRAME_BYTES: usize = 1 << 20;
    const STALL_FRAME_COUNT: usize = 4;
    const FLOOD_FRAMES: usize = 500;
    // 500 `try_send`s are microseconds of work; anything near a second means `broadcast` blocked,
    // which on the real publisher thread would stall turn resolution behind a wedged client.
    const PROMPT_BROADCAST: Duration = Duration::from_secs(1);

    let (listener, port) = bind_local();
    let server = start_snapshot_server_with_limits(
        listener,
        SnapshotServerLimits {
            write_timeout: WRITE_TIMEOUT,
            frame_queue_capacity: QUEUE_CAPACITY,
            ..SnapshotServerLimits::default()
        },
    );

    let _stalled = connect_and_register(port, &server, 1);

    // Fill the client's socket buffers so the broadcast thread parks inside `write_all`.
    let big = frame(vec![0xCD; STALL_FRAME_BYTES]);
    for _ in 0..STALL_FRAME_COUNT {
        server.broadcast(&big);
    }
    assert!(
        wait_until(|| server.queued_frames() > 0),
        "the broadcaster drained every large frame, so the client is not actually stalled and the \
         rest of this test would prove nothing"
    );

    let small = frame(vec![0x01; 32]);
    let started = Instant::now();
    for _ in 0..FLOOD_FRAMES {
        server.broadcast(&small);
        assert!(
            server.queued_frames() <= QUEUE_CAPACITY,
            "queued_frames()={} exceeded the configured capacity of {QUEUE_CAPACITY}: the frame \
             channel must be bounded, or a wedged client grows it until the server is OOM-killed",
            server.queued_frames()
        );
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed < PROMPT_BROADCAST,
        "broadcasting {FLOOD_FRAMES} frames took {elapsed:?}; `broadcast` runs on the publisher \
         thread and must never block on the socket (>= {PROMPT_BROADCAST:?} means it did)"
    );
    assert!(
        server.dropped_frames() > 0,
        "no frame was reported dropped even though the queue is capped at {QUEUE_CAPACITY} and \
         {FLOOD_FRAMES} frames were pushed at a stalled client: drop-on-full must be counted and \
         logged, not silent"
    );
}
