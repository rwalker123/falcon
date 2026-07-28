---
paths:
  - "core_sim/src/network.rs"
  - "core_sim/tests/snapshot_socket.rs"
---

# The snapshot broadcast socket — staying alive when a client stops reading

`network.rs` is the only socket a published frame goes out on. This file is the contract for what
happens when the thing on the other end **stops reading** — a client at a debugger breakpoint, under
`SIGSTOP`, or with a hung render thread. It is not about which frame is published (that is
`turn-profiling.md`) or which world the frame belongs to (`world-handoff.md`).

The socket is loopback, so "slow" is not the failure mode worth designing for. **Wedged is.**

## The topology: two threads, a channel each way, no shared client list

```
publisher thread ──frames (bounded)──▶ ┌──────────────────┐
                                       │ broadcast thread │ owns Vec<TcpStream> exclusively
accept thread ────new clients (bnd)──▶ └──────────────────┘
```

- The **accept thread** blocks in `listener.accept()`, configures the socket, and hands it over a
  channel. It never writes to a client and never touches the client list.
- The **broadcast thread** `select!`s over the two receivers and owns every `TcpStream` outright.

**Both properties are load-bearing, and each fixes a distinct half of issue #406.** When one thread
did both jobs, a blocked `write_all` stalled the loop that also called `accept()` — so the first
symptom an operator saw was *"the server stopped taking connections"*, with the unbounded queue
growing behind it toward OOM. Splitting the threads is what makes accept immune; a shared
`Arc<Mutex<Vec<TcpStream>>>` would have re-created the stall through the lock, which is why the
client list is **owned**, not shared.

The accept loop is consequently **blocking**, with no poll interval. The nonblocking listener and its
50 ms sleep existed only because the same thread had to get back to the channel.

**Shutdown runs backwards along those arrows, and every disconnect arm must exit its loop rather than
ignore the error.** Dropping `SnapshotServer` closes the frame sender → the broadcast thread's
`select!` sees the disconnect and returns → the client receiver drops → the accept thread's next
handoff fails and it returns too. A disconnected crossbeam receiver is *permanently ready*, so a
`select!` arm that logged and continued would spin a core forever; an accept thread that ignored the
failure would accept-and-immediately-close every future connection, one leaked thread per server.
The accept thread does still sit in `accept()` until the next connection arrives — the listener stays
bound that long, which is inherent to a blocking accept and harmless for a process-lifetime socket.

## A write is timed, and a client that exceeds it is dropped

Accepted sockets carry `set_write_timeout(limits.write_timeout)`. A timed-out `write_all` is an
`Err`, and `broadcast_frame`'s `retain_mut` drops that client.

**Dropping is mandatory, not a policy choice.** The wire format is a `u32` length prefix followed by
the payload, and `write_all` does not report how much it wrote before failing — so a timed-out write
may have left a partial frame on the wire, and every byte after it would be read as a length. There
is no resuming such a stream.

**The cost of dropping is real and asymmetric**: the Godot client does **not** reconnect a snapshot
stream it loses (`SnapshotLoader.enable_stream` is called once, and `poll_stream` only warns on
`STATUS_ERROR`), so a dropped client is a dead session until the player restarts. That is why the
timeout is generous rather than snug — a client polling the socket on every rendered frame has no
legitimate multi-second pause, so the timeout is sized to catch only a wedged process.

## The frame queue is bounded, and `broadcast` never blocks

`SnapshotServer::broadcast` is called from the **publisher thread**, which must never block on the
socket (`turn-profiling.md` — publication was moved off the turn thread precisely so this path
belongs to nobody the simulation waits on). So the send is a `try_send` on a **bounded** channel, and
a full queue **drops the frame** and counts it.

**Dropping a frame is recoverable, which is what makes drop-on-full admissible.** The client's decoder
drops a delta whose `baseFrameSeq` names a frame it never applied and raises `resync_needed`
(`native/src/bridge/decoder.rs`); `Main._tick_resync` turns that into a `resync` command, which the
server answers with a fresh full frame. A blocking send in place of the `try_send` would trade a
recoverable gap for an unrecoverable stall of the publisher — do not "fix" it into one.

The queue also converges on its own: once the wedged client's write times out it is dropped, the
client list empties, writes become no-ops, and the queue drains.

## Limits — `SnapshotServerLimits`

Defaults live on the struct; `start_snapshot_server` uses them and
`start_snapshot_server_with_limits` is the seam the tests shrink them through.

| Field | Default | Why that number |
|---|---|---|
| `write_timeout` | 5 s | Two orders of magnitude beyond any legitimate pause in a client that polls every rendered frame, because the penalty for firing early is a dead session (no client reconnect, above). |
| `frame_queue_capacity` | 64 | Caps queue memory at 64 × the largest frame. Deltas are small and a full frame is rare, so it is only ever approached while a client is wedged. |
| `pending_client_capacity` | 32 | A file-descriptor bound on a connection flood, not a tuning knob — an accept that cannot be handed over is closed. |

`connected_clients()`, `queued_frames()` and `dropped_frames()` exist so the two failure modes are
observable rather than inferred from a log; `core_sim/tests/snapshot_socket.rs` asserts on all three.

**The obvious test cannot tell the thread split from the write timeout, and that is worth knowing
before writing another one.** "A stalled client must not stop new connections" is satisfied on a
*merged* server that merely has the timeout: the kernel completes a loopback handshake into the
listen backlog whether or not anyone calls `accept()`, and one write timeout later the single thread
drops the stalled peer, returns from the drain and registers the newcomer — so the test comes down
to whether the survivor happened to register before the frame it waits for. What discriminates is
**the pending-client backlog's refusal arm**, because closing a connection that has nowhere to go is
work only the accept thread can do *while a write is blocked*:
`the_accept_thread_keeps_running_while_a_write_is_blocked` caps the backlog at one, parks the
broadcaster, and asserts the third connection gets a prompt EOF.

Its synchronisation is the other transferable part. `queued_frames() > 0` does **not** establish
that the broadcaster is wedged — it may not have reached the channel yet, and one still in `select!`
picks up the next connection. Parked is *took at least one frame off, then stopped*, which is an
absorbing state and platform-independent; how much a socket absorbs before it blocks varies by stack
and by autotuning, so no frame count is assumed.

## What is deliberately not here

**Per-client outbound buffering on nonblocking sockets.** It would be strictly better behaviour — a
briefly-slow client would not be dropped, and one wedged client could not delay another's frames —
but it needs partial-write tracking and a readiness mechanism (or a busy poll), and the realistic
client count on this socket is one. The condition that would justify it is **more than one client
mattering at once**, not a slow client.

## See also

- `world-handoff.md` — a newly accepted client is sent **nothing** until the next broadcast, and the
  accept/broadcast ordering race that follows from it (closed on the client, by retry-until-answered).
- `turn-profiling.md` — which frame each publication puts on this socket, and why the publisher
  thread owns the write.
