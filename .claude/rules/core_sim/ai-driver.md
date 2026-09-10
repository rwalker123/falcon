---
paths:
  - "sim_ai/**"
  - "core_sim/tests/ai_seat_scenario.rs"
---

# The AI driver: a player process on a seat

`sim_ai` is a player process (`docs/plan_ai_opponents.md` §1, `docs/plan_ai_driver.md` §1, §7). It
connects to the command and stream ports, claims a seat, is sent that seat's frames, and sends
commands. The server cannot tell it from the Godot client. This file is the as-built record of the
wire it speaks, the constants it restates, the script format its fixture brain reads, and the
launcher contract it is spawned under; the design is the two plan documents.

## The crate boundary is a build error

`sim_ai/Cargo.toml` depends on `sim_runtime` (and through it `sim_schema`) and **never on
`core_sim` or `bevy`**. `sim_ai/tests/crate_boundary.rs` reads the manifest and fails on either
name. The same integration-test target is what makes `cargo test --workspace` build the `sim_ai`
binary, which `core_sim/tests/ai_seat_scenario.rs` then runs.

Modules, this slice: `main.rs` (args, wiring, the turn loop), `link.rs` (claim · greet · hold ·
reconnect · resync), `view.rs` (`SeatView` + `Perception`), `brain.rs` (`Brain`, `PassBrain`,
`ScriptedBrain`).

## The wire sequence (`link.rs`)

1. Connect the **command** socket, `set_nodelay`, write `ClaimSeat { request_id, faction_id }` as
   the first `[u32 LE length][protobuf]` frame. Replies are `QueryReplyEnvelope` frames on the same
   socket, read on a thread and routed to the pending claim by `request_id`.
2. A grant carries the token. **Only then** connect the **stream** socket and write the token as its
   first `SEAT_TOKEN_BYTES` (8) bytes, little-endian, unframed; frames follow as
   `[u32 LE length][FlatBuffers envelope]`, read on a second thread.
3. Send `Resync`: a stream connection is sent nothing it did not ask for (`world-handoff.md`), so
   the first full frame is requested. The same on every reconnect.

⛔ **One command connection for the life of the process.** The seat belongs to the connection
(`factions.md` → Seats); every command goes on it. A dropped command link is rebuilt after
`RECONNECT_BACKOFF`, re-claimed (a fresh token), and the stream is closed and re-greeted with the new
token — a stale token is a stream that is silently sent nothing. A dropped *stream* alone is reopened
with the token the seat still holds. Reader threads are stamped with a connection generation, so a
stale thread's last words are ignored after either.

**Claim refusals.** `seat_occupied` is retried `SEAT_CLAIM_ATTEMPTS` (8) times, `SEAT_CLAIM_RETRY_BACKOFF`
(250 ms) apart — a reconnect races the server's read loop freeing the old socket's seat.
`unknown_seat` is retried **forever** on `UNKNOWN_SEAT_RETRY_BACKOFF` (2 s): the launcher spawns the
process off a roster event, and a world rebuild may seat the faction later. `already_seated` is a
bug and fails loudly.

**Restated constants.** `SEAT_TOKEN_BYTES`, the two claim-retry constants, `RECONNECT_BACKOFF` and
the 5 s claim-reply timeout are duplicated from `core_sim/src/network.rs` and the client's
`command_link.rs` by the same rule `SnapshotStream.gd` duplicates them: this crate must not link the
server, so the server's values are the authority and these are restated with a pointer.

**Host verbs are never sent.** `Turn`, `Rollback` and `SetFogEnabled` are refused in `Link::send`
by a debug assertion. **The token is a secret**: `SeatToken`'s `Debug` prints `SeatToken(<redacted>)`
and it has no `Display`.

## Perception (`view.rs`)

`SeatView { snapshot, last_acted_tick }`. A full frame replaces the snapshot (logged at info with
`world_epoch`, `frame_seq`, `tick`; never the token); a delta goes through
`WorldSnapshot::apply_delta`, and an `ApplyDeltaError` — or a delta before any full frame — marks the
chain broken: the loop sends `Resync` and every delta is dropped until a full frame lands.

## The turn loop (`main.rs`)

On every frame the view is updated. When the frame's `tick` is one the brain has not acted on,
`decide` runs with an rng seeded from `(seed, faction, tick)`, its commands go out on the link, and
`Orders { faction_id, Ready }` follows — **always**, even when `decide` returned nothing. A mid-turn
recapture arrives with the same tick and is never acted on twice. `decide` runs on the main thread
under `DECIDE_BUDGET` (30 s, well under the server's 120 s `seat_turn_timeout_seconds`); an overrun
is a warning and `ready` is submitted regardless. `--turns n` counts tick advances and exits 0 when
reached; the seat releases with the socket and is auto-submitted from then on. `--seed 0` derives the
seed from the faction.

**Arguments.** `--ports-file <path>` (default `$SIM_PORTS_FILE`; reads `host`, `command`,
`snapshot_flat`) *or* `--host --command-port --stream-port`; `--faction <u32>`; `--brain pass|scripted`;
`--script <path>`; `--seed <u64>`; `--turns <n>`; `--log-dir <path>` (accepted, unused until the
instruments slice).

## The script format (`ScriptedBrain`)

One command per line in `sim_runtime::command_text` form, prefixed with when it fires; `#` comments
and blank lines are ignored.

```text
12: split_band {faction} {own_band:0} 4      # absolute: fires on tick 12
+0: split_band {faction} {own_band:0} 4      # relative: the first tick this brain saw, plus 0
```

The `+<n>:` form exists because a script cannot know what tick the world it joins will be at. Two
substitutions, resolved against the view at fire time: `{faction}` is this seat's faction;
`{own_band:N}` is the `band_id` of the N-th `populations` row (row order, zero-based) whose
`faction` is this seat's. An unresolvable substitution or an unparsable line is a logged error and
the line is skipped.

## The launcher contract: `seats.roster`

The server emits one INFO event at every world build, from `retain_claimed_seats` in
`core_sim/src/bin/server.rs`, on the log stream (`log_stream.rs`, `[u32 LE length][JSON]` lines on
the `log` port):

```json
{"target":"shadow_scale::server","message":"seats.roster","fields":{"factions":"[0,1,2]","world_epoch":3}}
```

`factions` is the roster in order as a JSON array **inside a string** (`tracing` fields carry no
arrays); `world_epoch` is the build it belongs to. The launcher's `parse_roster_event` is the contract
twin. **Timing:** the launcher connects to the log port before it starts the human's client, and the
boot world is idle until that client asks for one, so no roster can precede its reader; the server
does not re-emit on connection. The supervisor and the exit rule are `launcher.md`.

## The scenario (`core_sim/tests/ai_seat_scenario.rs`)

A built `server` and a built `sim_ai --brain scripted --faction 1 --turns 3`, over the real sockets.
The script's one order is `split_band {faction} {own_band:0} 4` — the `settle.min_founding_workers`
floor, legal for a 30-person starting band with `parent_min_workers 6` to spare — so the rival's
resident band count moves by one. Frames are viewer-scoped and fogged, so the rival's world is read
through **seat 1** both times: before, on a connection released before the AI starts, and after, once
the AI has exited and released it. The test also asserts the tick advanced by at least three,
`seat.claimed … faction=1` is in the server log, and no `command.rejected` is. The `sim_ai` binary is
the server binary's sibling; if a narrower `cargo test` invocation left it unbuilt, the test builds
it into a private target directory (`target/ai_seat_scenario`), because the outer `cargo test` holds
the shared target directory's lock for the whole run.
