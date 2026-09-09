---
paths:
  - "clients/godot_thin_client/native/src/bridge/command_link.rs"
  - "clients/godot_thin_client/native/src/bridge/command.rs"
  - "clients/godot_thin_client/native/src/runtime.rs"
  - "clients/godot_thin_client/src/scripts/CommandClient.gd"
  - "clients/godot_thin_client/src/scripts/SeatClaim.gd"
  # `Main` owns the claim, the refusal report and the End Turn submission, so the rule describing
  # them has to load when Main is touched.
  - "clients/godot_thin_client/src/scripts/Main.gd"
---

# The command transport — one seated connection, and the two verbs that stay off it

The server half is `.claude/rules/core_sim/factions.md` → "Seats". This file is the client's: which
socket a command goes out on, what claims the seat, and what the player is told when the claim fails.

## The seat belongs to the CONNECTION, so the connection has to live

The server takes the faction a command acts on from the seat the sending **connection** claimed, not
from the `faction_id` on the wire, and it releases that seat the instant the socket closes. The client
used to open a fresh `TcpStream` per command (`transmit_proto_command`: connect, write one frame, drop
it) with no read path at all — so a claim made on one socket was gone before the next command, and
every faction-bearing command arrived unseated:

```
command.client.connected connection=7 client=127.0.0.1:62889
command.rejected=not_this_connections_seat command="split_band" faction=0 connection=7 claimed_seat=None
```

**The rising connection id is the whole symptom.** A healthy session logs `seat.claimed connection=N`
once and nothing else until `seat.released` at exit.

`bridge/command_link.rs` is the fix: one worker thread owning one `TcpStream` for the session, a
reader thread over a `try_clone`d half, and a generation counter so a frame from a socket already
replaced is recognised as stale. Everything — a command to write, a claim to make, a frame read, a
socket that died — reaches the worker as one `LinkMessage`, so exactly one thread decides the seat's
state and exactly one writes to the socket.

> ⛔ **The client never restates its identity.** No session id, no token, no faction the server is
> asked to trust: the claim is made once per connection and after that the server knows who is
> speaking from *which socket the bytes arrived on*. A scheme where a later command re-asserts its
> seat re-opens the forgery hole seats exist to close (`docs/plan_multiplayer_seats.md` §4.1).

## The routing rule is two variants wide, deliberately

`command_link::dispatch` is the only place that decides which socket a command takes:

| Goes out on | What | Why |
|---|---|---|
| a **throwaway** connection | `Turn`, `Rollback` | The server refuses a host verb from a *seated* connection (`SeatRegistry::may_issue_host_verb`, "host" = holding no seat). A fresh socket holds no seat, which is exactly what these need — so the Inspector's `+1`/`+10` and autoplay keep working unchanged, `turn 100` included |
| the **seated link** | everything else | Including the world verbs (`new_game`, `map_size`, `resync`) that name no faction: a seated connection may send them, so a second socket would buy nothing |

**It is NOT "does this command carry a faction".** Answering that would mean restating the server's
40-arm `commanding_faction` match in the client, where it could drift silently — and **a misrouted
faction-bearing command is refused at runtime with no compile error**, which is how this whole class
of bug hides. So the classification is kept as small as it can be: the same two variants
`core_sim`'s `is_host_verb` names, and everything else takes the seated default.

**The forecast/save worker keeps its connection per round trip** (`bridge/query.rs`). Those payloads
name no commanding faction, so an unseated round trip is answered exactly as a seated one would be —
and a forecast the sim answers between turns has no business bounding how long a seat's orders queue
behind it. What changed there is only that the drain grew a second producer: `deliver_reply` lets the
link put an answer it read itself onto the same once-a-frame hop
(`CommandBridge.poll_query_replies`), correlated by `request_id` like every other seam's.

## Reconnect re-claims, because the alternative is silence

A dropped link is rebuilt after `RECONNECT_BACKOFF` and the claim is re-sent on the new socket. Without
that, one dropped connection means every later command is refused with nothing on screen saying so.

**A `seat_occupied` refusal on a reconnect is retried** (`SEAT_CLAIM_ATTEMPTS` ×
`SEAT_CLAIM_RETRY_BACKOFF`): the server frees the old seat when *its* read loop notices the EOF, which
races this side's reconnect, so for that window "the old socket has not been reaped yet" and "another
player holds this seat" are indistinguishable. Past the window the refusal is believed and reported —
a claim that retries forever is the silent failure with extra steps. The other two tokens
(`unknown_seat`, `already_seated`) are not retried, because nothing about retrying changes them.

**A claim is written at most once per connection.** A granted seat is left alone until the connection
it was granted on goes away, since a second claim on a live connection is refused (`already_seated`)
by design. A command is likewise written at most once: a write that fails drops the link and tells the
caller, rather than replaying onto a fresh socket where the server may already have had it.

## A refused seat is reported on two surfaces

The failure to design against is *"nothing I click does anything"*. `Main._on_seat_refused` therefore
puts every refusal — including a claim that went unanswered, which arrives carrying the transport
token — on both surfaces that can be looking at the moment it lands:

- **the event dock's System channel, as an alert** — the client's standing surface for a fault the
  player did not cause, the same one a dropped command socket and a `resync` report on, and the only
  one still there once the game is running;
- **the loading overlay**, while it is still up, re-worded exactly as a refused load re-words it
  (`SaveSlots`' precedent). "Generating world…" is a lie when the world that appears will not take
  the player's orders.

Deliberately not a modal: neither the client nor the player can fix this, and a dialog would take away
the one thing left — reading the map of a game they cannot command.

**A grant that follows a report RETRACTS it**, on the same channel and, if the player is still on the
loading overlay, on that too. This is what makes the alert safe to raise on a transient fault: the
link goes on reconnecting and re-claiming after a report, so a standing "orders will not be obeyed"
that has since become false would be worse than never having shown it.

**A server that is not listening yet is not a refusal.** The client is routinely started beside the
server, so a claim that cannot connect waits for the reconnect instead of reporting. The bound on
that silence is the same `SEAT_CLAIM_REPLY_TIMEOUT`, armed from the moment the claim was *asked for*
rather than from the moment it was written — so a server that never comes up is still reported, once,
and one that comes up in two seconds is never mentioned.

## End Turn is a submission, not a resolution

The turn orb sends **`order <faction> ready`**, not `turn 1`. `Turn` resolves the world for everyone
and is the host's verb; with two players either one could end a turn the other was still taking. The
server resolves once every occupied seat has submitted, or when `seat_turn_timeout_seconds` runs out.

**Single-player pacing is unchanged and that is the acceptance criterion**: a vacant seat never holds
the turn, so with one player and N vacant rivals the submission resolves the turn in the same instant
`turn 1` used to. `steps` is unused on that path because a seat cannot submit a batch — "I am done" is
not a number — and advancing several turns at once stays the Inspector's host verb.

## The seat is claimed before the world is asked for

`Main._ready` builds `SeatClaim` and claims **faction 0** (`HudConst.PLAYER_FACTION_ID`) ahead of
`_try_send_world_request`. The order matters in one direction only: `new_game` names no faction and is
legal from an unseated connection, but everything the player can do afterwards is not, so claiming
first puts the grant in flight before the first band exists rather than after the player has clicked
something. Faction 0 is always in the roster (`FactionRegistry::with_ai_factions`), so a world rebuild
— `new_game`, a load — keeps the claim rather than dropping it (`SeatRegistry::retain_seats`).

## Key scripts

| Script | Holds |
|---|---|
| `native/src/bridge/command_link.rs` | The seated link: the worker that owns the socket and the seat, the reader thread, the reconnect/re-claim clock, `dispatch`'s two-arm routing, and the one-shot transmit the host verbs use. The levers are `RECONNECT_BACKOFF`, `SEAT_CLAIM_REPLY_TIMEOUT`, `SEAT_CLAIM_RETRY_BACKOFF`, `SEAT_CLAIM_ATTEMPTS`, `LINK_ACK_TIMEOUT` |
| `native/src/bridge/command.rs` | `CommandBridge` (`#[godot_api]`) — `send_line`, `send_query`, `claim_seat`, `poll_query_replies` — and the worker that keeps a send off Godot's main thread. It decides *when* a command is written; `command_link` decides *where* |
| `native/src/runtime.rs` | The embedded script host. Its `commands.issue` path takes the SAME `command_link::dispatch`, so a script's faction-bearing command is seated like a panel's |
| `CommandClient.gd` | The GDScript face of the bridge: endpoint precedence, `send_line`'s two-error contract, `send_query`, `claim_seat` |
| `SeatClaim.gd` | The seat seam: one reserved request id, the refusal tokens and their prose, `seated` / `refused`. Asked once — the link re-claims by itself |
| `Main.gd` | Builds the seam and claims at boot, pumps the drain into it, reports a refusal on the two surfaces above, and sends `order <faction> ready` for End Turn |
