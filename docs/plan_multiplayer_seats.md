# Seats — the multiplayer architecture, and the only thing that drives a faction

**The foundation document.** `docs/plan_ai_opponents.md` is built on this one and does not restate
it. Issue #287 decided it; #646 builds it.

---

## 1. The invariant

> **The sim knows seats. It never knows who fills one.**

A world has N faction seats. A seat is **occupied** by whatever connection has claimed it, or
**vacant**. The simulation's entire model of "who is playing" is that sentence. There is no AI path,
no human path, no local path — there is a socket, and commands arrive on it.

Three consequences, all deliberate:

- **A human client and an AI are the same kind of thing.** Neither is privileged, neither has a
  private entry point, and neither can do anything the other cannot.
- **There is no `is_ai` branch anywhere in command handling.** The registry still records what a
  seat's *default occupant* is, because someone has to decide what to launch at New Game — but
  nothing downstream of the socket reads it.
- **Multiplayer is not a feature built on top.** It is what this architecture already is, with some
  of the processes on other machines.

## 2. Everything is a remote player, including the host's human

**One process per player.** The person who started the game is a remote player whose process happens
to be on the same machine as the server. "Host" describes which machine runs the sim, not a
different kind of participant.

This is the decision that keeps there being exactly one code path. The tempting alternative — run
the default AI in-thread for speed, and use the socket only for remote humans — creates a second
path, and the second path is where the shortcut appears that the socket path cannot take. Then the
AI is playing a different game from the player, which is the failure this whole architecture exists
to prevent.

**Cost of holding the line:** serialization on every AI decision, process supervision, and a dead
player to handle. All three are paid anyway the moment a second human connects.

**Benefit that pays for it:** an AI can only do what a client can do. Every AI limitation is a
client-API limitation, so fixing one fixes both. A 4X AI that needs bonuses to compete is usually an
AI that was allowed to diverge from the player's ruleset; here it structurally cannot.

## 3. Who launches what

`launcher/src/main.rs` is already a supervisor: it spawns the server, waits on a real readiness
signal (a fully-written ports handshake file, `core_sim::port_alloc`), spawns the Godot client, and
reaps children through a `Drop` guard plus a Windows kill-on-close Job Object. It exists because the
shell scripts it replaced slept a fixed two seconds and orphaned servers on a crash.

**Extending it to N players is the same job it already does, with a loop.** Start the server, wait
for ready, then start one process per locally-hosted seat — the Godot client for the human, an AI
player process for each rival — and reap them all.

The server does **not** spawn players. It cannot: in a real multiplayer game the other processes are
on other machines, so a server that spawns its players would be a special case that only works
locally, which is exactly the second path §2 rules out.

## 4. What a seat needs that does not exist today

### 4.1 ⛔ A connection has no identity — this is the hole

`spawn_command_listener` (`core_sim/src/bin/server.rs:1139`) accepts a socket, spawns a thread, and
hands it a **clone of the same `Sender<Command>`**. Nothing correlates a connection to a faction.
The faction a command acts on is the `faction_id` the client wrote on the wire, and the only check is
`registry.contains(faction)` in `apply_command` (`server.rs:10063`) — *does this faction exist*.

**So any connection can command any faction.** Entirely fine today: one client, local, yours. Not
fine with a second party, and not fine for an AI process you are debugging, where a bug in the AI can
silently move the player's bands.

**The fix, and it belongs in the first version:** a connection **claims a seat at handshake**, and
the server thereafter takes the faction from the *seat*, not from the wire. A `faction_id` that
disagrees with the claimed seat is an error, not a hint.

Get this wrong now and it is not one fix later — it is ~50 command handlers that each trusted a
client-supplied faction id, audited one at a time.

**Claiming is not authentication.** Phase-appropriate is: a seat may be claimed once, and a second
claimant is refused. Credentials, reconnection tokens and host authority are a later concern that
this shape does not foreclose.

### 4.2 Waiting is the default; auto-submit is the timeout

`resolve_turn_with_auto_orders` (`server.rs:10855`) force-submits `FactionOrders::end_turn()` for
every faction still awaited, then resolves. That is why an AI faction sits and passes forever.

Under seats it inverts: **the turn scheduler waits for every occupied seat to submit**, and
auto-submits only for a seat that is vacant or has not answered within a timeout.

The existing code is already the degenerate case of this with the timeout at zero, which is the
pleasant part — `TurnQueue` already awaits all registered factions regardless of control, so the
pacing model (simultaneous submission, resolve when all are ready) is right already and needs no
change.

**New failure mode, and the timeout is the whole mitigation:** a hung player process now stalls the
turn. That is true of a wedged AI and of a human who walked away, and one mechanism covers both.
It must be in the first version, not added after the first hang.

### 4.3 Each seat sees its own world

`snapshot/capture.rs` reads a single global `ViewerFaction` resource and `SnapshotServer::broadcast`
sends **one frame to every connected client**. So every seat currently receives the same viewer's
view.

This is no longer merely limiting. PR #648 made the published frame viewer-scoped section by section
(`core_sim/tests/frame_is_viewer_scoped.rs` pins it on the *encoded* envelope). With one frame going
to everyone, **a second connected client now sees its own people as a foreign band** — redacted to
identity and position, absent from its own roster, demographics reading zero. The failure moved from
*a global view* to *a wrong view*.

**Why this is the honest-AI mechanism, not just a multiplayer chore.** Once a seat's perception is
the frame it receives, an AI cannot cheat — the tiles it has not seen are not in the bytes it was
given. Fog stops being a policy an AI author must respect and becomes a property of the transport.
That is a stronger guarantee than any amount of review.

#### ⛔ The cost this has to answer for

**Capture is most of the turn, and it is the half still on the turn thread.**
`.claude/rules/core_sim/turn-profiling.md`, 80×52 release, mean of 30 turns:

| | ms |
|---|---|
| `run_turn` (publisher idle) | **4.55** |
| `snapshot.build` — the capture | **3.16** |
| publisher, per frame — diff + encode | ~0.8 |

⛔ **Do not use the older "7.6 ms of an 8.4 ms turn" figure**, which is still quoted in places. It
predates #393, which moved hashing, diffing, encoding and the socket write onto a **publisher
thread**. A turn now executes 4.55 ms, and capture is ~70% of it.

That split is what makes the design decision, because the two halves scale differently:

- **N captures land on the turn thread** — the critical path, and the real cost.
- **N diffs and encodes land on the publisher** — already off the turn thread and already parallel.

So a naive capture-per-connection multiplies the *worst-placed* work by the seat count. **The loop is
a design answer and it is probably the wrong one.** Two shapes worth costing:

- **capture once into a viewer-independent form, then project per viewer at publish time** — the
  attractive one, precisely because projection lands on the publisher thread, which is already off
  the critical path;
- capture per viewer but **share the world-level sections** — terrain, rasters, catalogues — that
  carry no faction at all.

`.claude/rules/core_sim/factions.md` already has the input for either: its viewer-scope table names,
section by section, what is world-visible and what is the viewer's. That is the same partition a
shared/per-viewer split needs.

**Check before assuming the delta survives.** `.claude/rules/core_sim/event-feed.md`'s append-only
delta is safe today *because a viewer never changes mid-session*. Per-connection capture is exactly
the change that can break that: N seats means N baselines, and `diff_appended`'s cursor and the
indexed diffs' missing `removed_*` lists both assume one.

### 4.4 Rollback

Two consequences, both small and both silent if missed.

**It becomes a weapon.** `Command::Rollback` rewinds the world for everyone. That is a debug tool
with one player and a grief vector with several. It needs to be host-only, or dev-build-only.

**It desynchronises a player's head.** After `handle_rollback` an occupant's own memory and plans are
ahead of the world it now sees. `Command::Resync` already exists; the seat protocol has to deliver
that signal, and a player process has to react to it.

### 4.5 What seats are *not*

**Not save state.** A save is a world with N seats; who sat in them is a session fact. Nothing about
occupancy belongs in `SimState`.

## 5. What is already true

Worth stating plainly, because most of this architecture is present and only stubbed to one.

| Already works | Where |
|---|---|
| The command socket accepts **many concurrent clients** | `spawn_command_listener`, thread per connection |
| Faction membership is checked **once, in one place** | `apply_command`, `server.rs:10063` |
| The turn queue **awaits all factions**, control-blind | `TurnQueue` |
| Fog is **already keyed per faction** | `VisibilityLedger` |
| The published frame is **already viewer-scoped** | PR #648, `frame_is_viewer_scoped.rs` |
| Commands are **already logged and replayable** | `LogEntry::Command`, `log_dispatched_command` |
| The launcher **already supervises children with a readiness handshake** | `launcher/src/main.rs` |

**Replay and rollback survive this architecture unchanged, and that is not luck.** Because a seat
emits commands rather than mutating the world, an occupant's decisions land in the command log and a
rollback replays them without ever re-consulting the occupant. **A non-deterministic player — an LLM,
a human — costs the determinism suites nothing.** This is the strongest single argument for the seat
model.

## 6. Ports

The block is four: `base+0` reserved (it carried the retired bincode snapshot socket, #388), `+1`
command, `+2` flat/stream, `+3` log (`scripts/run_stack.sh`). A player process needs the command port
and the stream port. Nothing here requires a new port — seats multiplex over the existing two, which
is what "many concurrent clients" above already means.

## 7. Open

- **Correlating a seat's two sockets.** Command and snapshot are separate connections. Today that
  does not matter; under seats, the handshake has to tie them to one claim.
- **The delta baseline per viewer** — see §4.3's warning. Needs measuring, not guessing.
