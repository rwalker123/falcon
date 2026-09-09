---
paths:
  - "core_sim/src/network.rs"
  - "core_sim/tests/snapshot_socket.rs"
---

# The snapshot broadcast socket — staying alive when a client stops reading

`network.rs` is the only socket a published frame goes out on. This file is the contract for
**which client each frame goes to**, and for what happens when the thing on the other end
**stops reading** — a client at a debugger breakpoint, under
`SIGSTOP`, or with a hung render thread. It is not about which frame is published (that is
`turn-profiling.md`) or which world the frame belongs to (`world-handoff.md`).

The socket is loopback, so "slow" is not the failure mode worth designing for. **Wedged is.**

## A frame is ADDRESSED to a seat, not broadcast

Since PR #648 a published frame is one viewer's world, and since the seat arc the capture builds one
per occupied seat (`factions.md` → "One frame per seat"). So `SnapshotServer::deliver(seat, frame)`
replaced `broadcast(frame)`: the frame carries the seat it was captured for, and the broadcast thread
writes it to **the clients holding that seat** and to nobody else.

### The greeting: a stream connection presents its seat token

A seat's two sockets are correlated by the token the claim reply hands back — the claiming
connection's `ConnectionId` (`factions.md` → "A connection has an identity"). The stream socket
presents it as the first `SEAT_TOKEN_BYTES` (8) it writes, little-endian and unframed: the socket is
one-way from there on, so a length prefix would only describe a payload whose size is a constant.
`ConnectionId::INTERNAL` (`0`) is the explicit *"I hold no seat"*, which is how a tool skips the wait
below. **The order is fixed** — claim on the command socket, then connect the stream and greet with
the token.

**The token is resolved to a seat at DELIVERY, not at the handshake**, off a table
(`SnapshotServer::set_seats`) the server rewrites whenever a claim or a release moves. Both
directions matter: a stream that greets a moment before its claim registers is seated as soon as the
claim lands, and a connection whose seat is released stops receiving that seat's frames *even though
its socket is still open* — otherwise the next occupant's private world would go to its predecessor.

### An unseated connection is registered, and receives nothing

Three ways to hold no seat: the peer sends no token within `handshake_timeout`, it sends
`ConnectionId::INTERNAL`, or its token names no live claim. All three are the same state, and it is
**silence, not a refusal**: the connection stays registered (a socket the server closes under a tool
is not a defined state) and no frame is ever written to it.

**Withholding is the only answer that cannot leak.** The alternative — falling back to some default
faction — hands a watcher the private world of whichever people that turned out to be, in full, which
is the disclosure #648 closed on the frame's *contents* reappearing one addressing decision over. A
spectator view would have to be its own capture with no viewer at all, and there is no spectator.
`an_unseated_client_receives_nothing_while_a_seated_one_receives_every_frame` pins **both** halves in
one run, because "the unseated client got nothing" passes just as well on a server that delivers
nothing to anyone.

> ⛔ **THE HANDSHAKE READ IS ON ITS OWN THREAD, AND IT IS SPAWNED ONLY AFTER THE HANDOFF SUCCEEDS.**
> Reading the greeting on the accept thread would let one client that connects and says nothing hold
> up every other connection for `handshake_timeout` — the head-of-line stall #406 split these threads
> to remove, in a new costume. But the handshake thread must not be spawned *before* the client is
> handed over either: it holds a `try_clone`d dup of the socket, so a connection the backlog then
> **refuses** would stay open until that thread's own timeout instead of getting a prompt EOF, which
> is exactly what `the_accept_thread_keeps_running_while_a_write_is_blocked` waits for. So the accept
> thread registers first, in order — which is also what keeps the backlog's refusal arm a statement
> about **accept order** — and the greeting catches up on the `Greeting` channel.
>
> A client is therefore **unseated between registration and its greeting** and receives nothing in
> that window. That is the dropped-first-frame race `world-handoff.md` already describes, healed the
> same way: the client asks.

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

`SnapshotServer::deliver` is called from the **publisher thread**, which must never block on the
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
| `pending_client_capacity` | 32 | A file-descriptor bound on a connection flood, not a tuning knob — an accept that cannot be handed over is closed. It also bounds the `Greeting` channel, whose entries are 16 bytes and whose only overflow arm leaves a client unseated. |
| `handshake_timeout` | 2 s | How long an accepted socket has to present its seat token. It costs a **seated** client nothing — it already holds the token when it opens this socket — so the wait is only ever paid by a connection that presents nothing, and what it buys is that such a connection is registered as *unseated* rather than dropped. |

`connected_clients()`, `seated_clients()`, `queued_frames()` and `dropped_frames()` exist so the
failure modes are observable rather than inferred from a log; `core_sim/tests/snapshot_socket.rs`
asserts on all four. **Registration and seating are two steps**, so a test that waits only for the
first races the delivery it is about to assert on — `connect_and_register` waits for both.

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
