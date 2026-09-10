---
paths:
  - "clients/godot_thin_client/native/src/bridge/command_link.rs"
  - "clients/godot_thin_client/native/src/bridge/command.rs"
  # The query channel routes by the same rule and half of it now rides the seated link, so the file
  # that decides which socket a QUESTION takes has to load this too.
  - "clients/godot_thin_client/native/src/bridge/query.rs"
  - "clients/godot_thin_client/native/src/runtime.rs"
  - "clients/godot_thin_client/src/scripts/CommandClient.gd"
  - "clients/godot_thin_client/src/scripts/SeatClaim.gd"
  # The snapshot socket's FIRST BYTES are the seat token this seam obtained, so the two files that
  # open and greet on that socket load the rule that says what the greeting is.
  - "clients/godot_thin_client/src/scripts/SnapshotStream.gd"
  - "clients/godot_thin_client/src/scripts/SnapshotLoader.gd"
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

## A QUESTION is routed by whether it names a faction, and that one IS decidable

The command router refuses to classify by faction (above). The QUERY channel does exactly that, and
the difference is not inconsistency — it is that `QueryPayload` has **five** variants against
`CommandPayload`'s forty, and `bridge/query.rs` already matches all five in one place to build them.
So `names_a_faction` is an **exhaustive match with no wildcard arm**, and a sixth question is a
compile error until it says which socket it takes. That is the compile-time guard the command side
cannot have, which is why the command side answers the question a different way.

| Goes out on | What | Why |
|---|---|---|
| the **seated link** | `HuntTripForecast`, `DenialRaidForecast`, `HuntCrewTake` | Each carries a client-supplied `faction_id` and is answered with that faction's private state — a named band's live equipment wear, its idle workers, its take curve. Asked from an unseated connection that is the disclosure per-seat frames just closed, one channel over |
| a **connection per round trip** | `ListSaves`, `FactionCapacity`, and the three save verbs | They name no faction, and they are asked from `LandingScreen` **before `Main` exists and therefore before any seat is claimed**. The server answers both ahead of its `world_active` gate so the load menu opens with no world; routing them onto the link would make that menu wait on the seat machinery to answer a question no gate will ever apply to |

**A query written on the link is fire-and-forget out and correlated by `request_id` back**, exactly
as the claim is — `send_query` is called on Godot's main thread and must not block on the worker, so
there is no ack channel. The worker keeps a **deadline per outstanding question** instead, and the
three ways an answer can fail to arrive all land on the drain as an `error` rather than as silence:
the socket would not open, the socket died holding it (`on_dropped` fails everything outstanding), or
the server never answered (`QUERY_DETAIL_UNANSWERED`, at the timeout `bridge/query.rs` chose for that
kind — the bound is a property of the QUESTION, so the link is handed it rather than owning one).

⛔ **No queue, and none is needed.** A question asked while the link is down connects on the spot, and
`connect` writes the seat claim as the **first frame** on the new socket; the server reads one
connection's frames in order into one channel (`handle_proto_client`), so the claim is registered
before the question is evaluated. **Wire order is the queue** — which is why a question asked
mid-reconnect needs no holding pen, and a question asked before the grant landed is still asked from
a seated connection. A socket that will not open fails the question *now* rather than parking it: the
sheet renders one failure line and reopening asks again, where a parked question would come back at
an arbitrary later moment against a world that had moved on.

**Nothing is ever re-asked by the link.** A re-sent question is one the sheet did not ask, answered
against a later world — the same reason a command is written at most once. `ForecastQuery` owns the
retry it does have, and only for the transport token.

The drain is unchanged and still has two producers: `deliver_reply` lets the link put an answer it
read itself onto the same once-a-frame hop (`CommandBridge.poll_query_replies`), correlated by
`request_id` like every other seam's.

**Routing a question is separable from refusing one, and they land in that order.** `Command::Query`
sits in `commanding_faction`'s `None` arm, so the server answers a faction-bearing query whatever
connection it arrives on — which is what makes moving them **behaviour-neutral**: single-player
forecast sheets, the load menu and the New Game capacity ask are unchanged, and the only observable
difference is that the server logs one `command.client.connected` per session instead of one more per
question. A gate that refused before the client routed would break every forecast sheet in the game.

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

## …and a connection lasts exactly one RUN

**The seat's lifetime is the RUN, not the process.** The link worker is process-global
(`LINK_SENDER`, a `OnceLock`), so it outlives every `Main`, and the server frees a seat on one event
only: the connection holding it closing. Ending a run therefore has to SAY so —
`Main._exit_tree` calls `CommandClient.release_seat`, which drops the seat intent and shuts the socket
down, and the next run claims on a connection the server has never seated.

**`_exit_tree` rather than the Abandon handler, because a run ends five ways and they all free
`Main`**: Abandon, the pause menu's "Load — discards this run", Options → "Apply now",
`_return_to_landing`, and quitting. Hooking the one button leaves the other four stranding the seat.
The re-claim on the next run can race the server's reap of the old socket and come back
`seat_occupied`, which is the one refusal the link already retries, so the teardown needs nothing
further.

**The symptom when nothing released it was a game that could not be restarted without quitting.** The
second run's `ClaimSeat` went out on the still-seated socket of the first, `SeatRegistry::claim`
refused it `already_seated`, and the pre-reveal path above bounced the player to the landing screen
with that refusal's prose. `retain_seats` is no help and cannot be: faction 0 is in every roster, so a
`new_game` deliberately KEEPS the claim. And the message understated it — a refused claim leaves
`seat_token` at `NO_SEAT_TOKEN`, so the new run's snapshot stream is registered unseated and is sent
no frames at all.

**The release is stated in two places on purpose.** `LinkWorker::on_claim` releases a seat it still
holds before claiming, because a `Claim` means a new run is starting and a run starts on an unseated
connection; the GDScript call is the fix, and the link's own release is what keeps a teardown path
that forgets from stranding the run after it. **The shutdown is the mechanism, not the drop**: the
reader thread holds a `try_clone`d handle to the same socket, so releasing `self.write` alone leaves
the file description open and the server never sees the EOF.

**An idle client between runs holds no socket and no seat**, which is the other half of why the
release clears the intent rather than merely dropping the connection. A reconnect would re-claim; a
player sitting on the landing screen would then occupy a seat the server's turn gate waits
`seat_turn_timeout_seconds` on, every turn.

## A refused seat is reported where the player can act, and that is TWO different places

The failure to design against is *"nothing I click does anything"*. What the report looks like turns
on one question `Main._on_seat_refused` asks first — **has a world been revealed?** — because before
the reveal there is no run to stay in, and after it there is one that must not be taken away.

### Before the reveal: back to the landing screen, one sentence on the rail

Pre-reveal the only surface up is the loading overlay, and a refusal there used to re-word it: one
sentence centred on a black rectangle, with nothing to press and nothing ESC could do about it. So
`_return_to_landing` writes the reason into `GameLaunch.pending_landing_notice` and changes scene to
`LandingScreen`, which shows it on the shell's rail (`MenuShell.set_notice`) and clears it as it
reads — beside New Game and Load Game, the two moves that resolve it.

**The wording is one plain statement plus the one thing to do about it**, and it is deliberately not a
report on the mechanism: *"Unable to connect to the server. Please try restarting the game."* A player
has no model of seats, claims or requests and needs none in order to restart a game; that detail
belongs in the log line beside it and in this file. An earlier three-line version naming the
unanswered seat request was rejected on sight.

**The constant belongs to the SHELL, not to `Main` (`MenuShell.NOTICE_NO_SERVER`)**, because the
landing screen raises the same line for itself whenever its own `faction_capacity` ask cannot reach a
server (`.claude/rules/client/new-game-setup.md`). One constant is what makes the two paths ONE box:
a bounced session whose capacity ask then fails would otherwise say the same thing twice on one
screen, and a server coming up would clear only one of them.

**It is the transport token's wording only, and the other refusals keep their own.** A claim can also
come back `seat_occupied`, `unknown_seat` or `already_seated`, and none of those is "could not
connect" — a seat held by another player and a server running a different game have different causes
and different fixes. Each already has a one-sentence prose line (`SeatClaim.ERROR_PROSE`), and that
is what the notice carries for them.

**This is NOT `_abandon_resync`'s treatment, and the two differ on both axes that decide it.** There a
world is on screen and the detection is *inferred from silence*, so the response has to be undoable —
a pause menu over the last frame, dismissible. Here the failure is stated on the wire and there is no
world at all, so nothing is taken away by leaving and there would be nothing to dismiss the menu back
onto. One mechanism would have to be wrong at one end.

**Nothing can bounce the player in a loop**, for three independent reasons and the first is
sufficient: the landing screen **claims no seat** — it opens a command client for `list_saves` and
`faction_capacity` and nothing else — so arriving there cannot reproduce the failure, and only a
player press returns to `Main`. With nothing listening neither press is offered anyway: `MenuShell`
disables "Begin the trail" and the saves list reports the failure in place of rows
(`.claude/rules/client/new-game-setup.md`). And `_seat_bounce_taken` allows one scene change per
`Main` — `change_scene_to_file` is deferred to the end of the frame, so a second refusal in the same
frame would otherwise ask for the swap twice — while the run's armed launch parameters
(`active_new_game`, `active_load_slot`, both pending slots) are cleared on the way out, the same
clearing `_on_pause_abandon` performs, so nothing left behind re-launches anything.

### After the reveal: the two standing surfaces, and the run stays

A seat lost mid-game means a world on screen that will no longer take orders, so the report goes to
both surfaces that can be looking at the moment it lands:

- **the event dock's System channel, as an alert** — the client's standing surface for a fault the
  player did not cause, the same one a dropped command socket and a `resync` report on, and the only
  one still there once the game is running;
- **the loading overlay**, while it is still up, re-worded exactly as a refused load re-words it
  (`SaveSlots`' precedent). "Generating world…" is a lie when the world that appears will not take
  the player's orders.

Deliberately not a modal: neither the client nor the player can fix this, and a dialog would take away
the one thing left — reading the map of a game they cannot command.

The loading overlay is still re-worded on this path, because the reveal gate can be holding it open
over a world that HAS been revealed once — a re-grant under a new token, a rebuild.

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

## The snapshot stream greets with the seat token, so it opens AFTER the claim is answered

**The seat decides what the client is SENT, not only what it may send.** A frame is one viewer's
world, and `SnapshotServer::deliver` addresses it to the connections whose token resolves to that
seat — so a snapshot socket that presents no token is registered *unseated* and receives **nothing**,
for the life of the connection, with no error anywhere on the client. The only trace is a server line:

```
Snapshot client 127.0.0.1:65369 presented no seat token (…); it will receive no frames
```

The greeting is **exactly `SEAT_TOKEN_BYTES` = 8 little-endian bytes and nothing else** — no framing,
no length prefix, no reply. `SnapshotStream.SEAT_TOKEN_BYTES` mirrors `core_sim::network`'s constant
of the same name and the two must move together: a width disagreement is unobservable from the client
side, because the server simply reads a token nobody holds and the map stays blank. `0`
(`SnapshotStream.NO_SEAT_TOKEN` = `core_sim::SeatToken::NONE`) is the explicit *"I hold no seat"*, which
registers a watching tool as unseated at once instead of making it sit out the server's
`DEFAULT_HANDSHAKE_TIMEOUT`.

**The write cannot happen in `connect_to`.** `StreamPeerTCP.connect_to_host` is asynchronous, so the
socket is still `STATUS_CONNECTING` on return; `SnapshotStream.poll` writes the greeting the first
time it sees `STATUS_CONNECTED`, which is within a frame of the connect and far inside the server's
two-second handshake window. `encode_s64`, not `encode_u64`: the bridge hands the token up as an
`i64` (a reinterpreted `u64`, `query.rs`), and only the signed encoder accepts the high-bit case.

That ordering is why **`SnapshotLoader.enable_stream` is not called from `_ready` any more.** It runs
from `Main._on_seat_seated`, which is the first moment the token exists — the claim's answer carries
it (`SeatClaim.seated(faction_id, seat_token)`, read from the reply's `seat_token`). A refused claim
therefore opens no stream at all, which is honest: an unseated connection would receive nothing from
it anyway, and the refusal is already on the overlay and the System channel.

**A reconnect mints a NEW token, so the stream is replaced.** The seat belongs to the command
connection; when the seated link reconnects it re-claims, and every granted claim mints a fresh random
`SeatToken` (`core_sim/.../factions.md` → "A token is a secret; the connection id is an identity") —
the token is a per-claim secret, not the connection's id, so a client cannot predict or reuse one.
The old token then names a claim the server has forgotten, and every later frame
is addressed past us — the same dead stream as the no-token case, wearing a different hat. So
`Main._open_snapshot_stream` compares the granted token with the one the open socket greeted with and,
when they differ, closes the socket and greets again with the new one. It is idempotent when the token
is unchanged. Because the replacement socket missed whatever was published while it was down, a
`resync` is then sent through `_tick_resync`'s existing bookkeeping — but only once a world has been
revealed, since before that the world request's own retry is what covers the gap.

**The world request waits for the greeting**, not merely for the connect: `_try_send_world_request`
and `_tick_new_game_retry` both gate on `Main._snapshot_stream_ready()` →
`SnapshotLoader.stream_presented_seat_token()`. `new_game`'s answer *is* the new world's first full
snapshot, and a socket the snapshot server still holds as unseated is skipped when that frame is
delivered — the race `_tick_new_game_retry`'s phase 2 exists to recover from. Waiting on the token
removes it instead. Waiting is not a rejection, so phase 1's bounded retry burst does not tick down
while the claim is outstanding.

## The client logs the seat HANDSHAKE and never the TOKEN

A `SeatToken` is minted per claim from a CSPRNG and is the whole of what the stream socket presents to
be sent this seat's frames, so it is a **bearer secret**: anything holding one can read another
player's world. The server logs none — `SeatToken` has no `Display` and its `Debug` renders
`SeatToken(redacted)` — so a client that printed the value would be the only way to correlate a
greeting with a claim by reading logs, which is the hole the server side closed.

**Every client-side line about a token therefore prints
`SnapshotStream.SEAT_TOKEN_LOG_REDACTION` in place of the value**, and there are four of them:
`SnapshotStream`'s "presented its seat token", `SnapshotLoader`'s connect line, `Main`'s
"faction N seated", and `Main`'s "snapshot stream token changed", which names no value at all. The
harnesses are held to the same rule — `live_seat_probe` prints `token_granted=true`, not the token.

**The events stay, because they are how both transport regressions on this branch were diagnosed**: a
stream that greeted with a stale token is a live socket receiving nothing, and the only trace of it is
the reconnect line. So the EVENT is logged and the VALUE never is.

**A truncation or a fingerprint is not a middle ground.** A partial secret is still a secret, and a
stable hash is a correlator — which is the whole of what reading logs would buy. `NO_SEAT_TOKEN` (`0`,
`SeatToken::NONE`) is the one value that is not a secret, which is why it may be named.

## "The server holds no world" is an ANSWER, so the client stops asking

A `resync` is retried until it is answered (`Main._tick_resync`, `RESYNC_ANSWER_TIMEOUT` = 2 s),
because a client with no applicable baseline renders a frozen world and cannot recover on its own.
That retry used to be **unbounded**, and a developer restarting the server mid-session is what showed
why it cannot be: the fresh process boots idle, `Resync` finds no world to publish and logs
`resync.no_world`, and the client asked again every two seconds for the rest of the session — writing a
System-channel line each time. Retrying could never help, because nothing changes until a human starts
or loads a game.

**The answer is not on the wire, which is why the client counts.** `resync` is a fire-and-forget
command: the server's `resync.no_world` is a line in *its* log and reaches nothing here, and no reply
channel exists for a runtime command (only `Query`, `ClaimSeat` and the save verbs are answered). The
only evidence available to the client is silence, so `RESYNC_UNANSWERED_ATTEMPT_BUDGET` (6, i.e. 12 s)
is what turns silence into a conclusion. It is sized far past any legitimate delay — the answer is a
re-encode of a world the server already holds, so the only thing that can hold one up is the command
loop being inside a turn.

**It is bounded, not widened.** A dropped socket keeps its own retry: the seated link reconnects with
`RECONNECT_BACKOFF` forever and re-claims, and that is correct because a transport failure says nothing
about whether a world exists. What stops is the *ask about the world*. The rest of the client already
splits this way and `_tick_resync` was the outlier:

| Seam | Re-asked | Never re-asked |
|---|---|---|
| `ForecastQuery` | `QUERY_ERROR_TRANSPORT`, after `TRANSPORT_RETRY_AFTER_MSEC` | every token the server spelled, `no_active_world` among them |
| `Main._on_save_op_finished` (a load) | `SaveSlots.ERROR_TRANSPORT` | `no_such_slot`, `unreadable` — a statement about THIS slot |
| `command_link`'s claim | `seat_occupied`, `SEAT_CLAIM_ATTEMPTS` times with a backoff | `unknown_seat`, `already_seated` |
| `Main._tick_resync` | nothing, once the budget is spent | silence past `RESYNC_UNANSWERED_ATTEMPT_BUDGET` asks |

`_tick_new_game_retry`'s phase 2 is deliberately still unbounded and is **not** the same shape: a
re-sent `new_game` makes the server *build* a world, so retrying is progress rather than a spin, and a
permanently stuck loading screen is the unrecoverable state it exists to prevent.

**The player is left where they can act, and the run is not taken from them.** `_abandon_resync` reports
once on the event dock's System channel as an ALERT — the standing surface for a fault the player did
not cause, the same one a refused seat and a dropped command socket use — and opens the **pause menu**,
which is the only surface holding both moves that resolve this: `Load — discards this run`, and
`Abandon`, which returns to the landing screen that owns New Game. It deliberately does **not** change
scene to the landing screen itself: the detection is inferred from silence, so it can in principle fire
on a server that was merely wedged, and being wrong must not destroy a run in progress. An opened menu
is dismissible with ESC and leaves the last frame standing behind it.

**And the alert is retracted if the world comes back**, on the same channel and for the same reason
`SEAT_RECOVERED_MESSAGE` exists: a full frame clears the latch, so a standing "this world is gone" that
has become false does not outlive the fact. The message says the world is *gone* rather than that the
client is still trying, because after a server restart it is.

## Key scripts

| Script | Holds |
|---|---|
| `SnapshotStream.gd` | The snapshot socket: `SEAT_TOKEN_BYTES` / `NO_SEAT_TOKEN`, `SEAT_TOKEN_LOG_REDACTION` (the token is never printed), the greeting written on the first `STATUS_CONNECTED` poll and retried until it lands, and `seat_token_presented` |
| `SnapshotLoader.gd` | `enable_stream(host, port, seat_token)` and `stream_presented_seat_token` — the loader is where the token reaches the socket |
| `native/src/bridge/command_link.rs` | The seated link: the worker that owns the socket and the seat, the reader thread, the reconnect/re-claim clock, `dispatch`'s two-arm routing, and the one-shot transmit the host verbs use. It also carries the faction-bearing QUESTIONS (`send_query`) and the deadline per outstanding one. The levers are `RECONNECT_BACKOFF`, `SEAT_CLAIM_REPLY_TIMEOUT`, `SEAT_CLAIM_RETRY_BACKOFF`, `SEAT_CLAIM_ATTEMPTS`, `LINK_ACK_TIMEOUT` |
| `native/src/bridge/query.rs` | The query channel and the split above: `names_a_faction` (exhaustive over `QueryPayload`), `routes_over_seated_link`, and the per-round-trip worker the faction-free questions still use with their own `QUERY_REPLY_TIMEOUT` / `SAVE_REPLY_TIMEOUT` |
| `native/src/bridge/command.rs` | `CommandBridge` (`#[godot_api]`) — `send_line`, `send_query`, `claim_seat`, `release_seat`, `poll_query_replies` — and the worker that keeps a send off Godot's main thread. It decides *when* a command is written; `command_link` decides *where* |
| `native/src/runtime.rs` | The embedded script host. Its `commands.issue` path takes the SAME `command_link::dispatch`, so a script's faction-bearing command is seated like a panel's |
| `CommandClient.gd` | The GDScript face of the bridge: endpoint precedence, `send_line`'s two-error contract, `send_query`, `claim_seat`, `release_seat` |
| `GameLaunch.gd` | `pending_landing_notice` — the one message a failed pre-reveal session leaves for the landing screen, in the same handoff direction the launch parameters travel the other way. Written by `Main`, read AND CLEARED by `LandingScreen`, so it is reported exactly once |
| `ui/MenuShell.gd` | `NOTICE_NO_SERVER` and `set_notice(text)` — the rail notice above the nav: ONE sentence in a `DANGER`-bordered box, no eyebrow and no heading, hidden when there is nothing to say (every healthy path). `_notice_line` resolves the owner's text over the shell's own unreachable latch, so the two paths can never stack. Built with the RAIL, not with a pane, because what it reports is a fact about the SESSION and has to survive every pane change |
| `SeatClaim.gd` | The seat seam: one reserved request id, the refusal tokens and their prose, `seated(faction_id, seat_token)` / `refused`. Asked once — the link re-claims by itself, and each re-grant carries a new token |
| `Main.gd` | Builds the seam and claims at boot, pumps the drain into it, reports a refusal — pre-reveal by returning to the landing screen with the reason (`_return_to_landing`, `MenuShell.NOTICE_NO_SERVER`, `_seat_bounce_taken`), post-reveal on the two standing surfaces above — opens/replaces the snapshot stream from the grant (`_open_snapshot_stream`), gates the world request on `_snapshot_stream_ready`, and sends `order <faction> ready` for End Turn. It also owns the baseline chase — `_tick_resync`, the one `_ask_for_resync` sender, `RESYNC_ANSWER_TIMEOUT` / `RESYNC_UNANSWERED_ATTEMPT_BUDGET`, and `_abandon_resync`. `_exit_tree` releases the seat, since every way a run ends frees this node |
