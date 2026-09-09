---
paths:
  # The script AND its scene, for the same reason every guard lists both: the scene is the entry
  # point every invocation names, so gating on the `.gd` alone would leave the wiring edit — the one
  # that can silently stop the gate running at all — without this file.
  - "clients/godot_thin_client/tools/live_seat_probe.gd"
  - "clients/godot_thin_client/tools/live_seat_probe.tscn"
---

# `live_seat_probe` — the one harness that needs a server

Every other tool in `tools/` is deliberately serverless: `ui_preview` feeds canned fixture
Dictionaries to the real HUD, the guards decode a golden fixture, and none of them opens a socket.
This one is the opposite, and exists for the path the others cannot reach — **a real client's seated
command link against a real sim.**

The shared contract it obeys (the exit-status verdict, the quiet window, the hang guard) is
`test-harnesses.md`. This file is what is particular to it. No `## Key scripts` table here, per that
file's shape for harnesses: a wrapped `##` section per script instead.

## Why a live harness exists at all

The client/server transport broke **twice on one branch**: a per-command `TcpStream` that dropped the
seat between commands (so every faction-bearing command arrived unseated), and then a snapshot socket
that opened without writing its seat-token greeting (so a seated client received no frames at all).
Both are invisible to a fixture harness — there is no socket in one — and invisible to a compile
check, because nothing is missing, only unsent. Each time, the only thing that caught them was
driving a real client against a real server, by hand.

macOS blocks synthetic mouse input, so a probe cannot click the HUD. It calls `Main`'s handlers by
name instead — `_on_hud_split_band`, `_on_hud_next_turn` — which are exactly the functions the HUD's
signals are connected to, so everything from the handler down is the shipped path.

> ⛔ **A pass here is not evidence about what a player sees or can reach.** It says the bytes make the
> round trip and the world obeys. Reachability stays a human's judgement in a live client.

## What it proves — three assertions, in session order

**1. The seat handshake.** `SeatClaim.seated_now` is true, the grant carries a token that is not
`SnapshotStream.NO_SEAT_TOKEN`, and `Main._world_revealed` becomes true. A failure separates the three
causes by name: the claim was refused, or it was granted with nothing for the stream to greet with, or
frames addressed to that token never arrived — which is the missing-greeting regression exactly.

**2. One faction-bearing command.** `split_band` through `Main._on_hud_split_band`, and the world
comes back holding one more resident band. **The assertion is the EFFECT, not the send**: a `true`
from `_send_runtime_command` only means the frame reached the socket, which was the case throughout
both regressions. The band count changing is the only thing that says the world obeyed. A failure
means the command never reached the seated link, or the server refused it — the dropped-seat
regression prints `command.rejected=not_this_connections_seat` on the server side.

**3. One turn submission.** `order <faction> ready` through `Main._on_hud_next_turn`, and `turn`
advances. **The seat's verb, not the host's**: `turn N` is a host verb that `command_link::dispatch`
puts on a throwaway connection, so asserting on it would prove nothing about the seat. With one seated
player and vacant rivals the submission alone resolves the turn.

**It stays three assertions wide.** The seat, one command that names a faction, one turn. Anything
else about gameplay belongs in `core_sim`'s own tests, where it costs no window and no server.

**The split's worker count is read off the cohort, never copied from config.**
`founding_min_workers` / `founding_parent_min_workers` cross the wire per band
(`native/src/dict/population.rs`) — the same numbers the compose sheet states — so a retuned floor
moves the probe with it. A band that cannot legally split **fails loudly** rather than skipping: a
gate that quietly proves less than it claims is worse than one that fails.

## How it is invoked

It needs a **running server on a port block this process can see**, and nothing else. It instances the
real `Main.tscn` and lets it boot: resolve endpoints, claim the seat, greet the stream, send its
dev-default `new_game`. So the server may be freshly booted and idle — the probe asks for its own
world.

```bash
scripts/run_stack.sh --server-only --port-base 41040      # never the default 41000-41003 block
godot --headless --path clients/godot_thin_client --import
env STREAM_ENABLED=true STREAM_HOST=127.0.0.1 STREAM_PORT=41042 \
    COMMAND_HOST=127.0.0.1 COMMAND_PORT=41041 COMMAND_PROTO_PORT=41041 \
    scripts/preview.sh res://tools/live_seat_probe.tscn
echo $?
```

**Through `scripts/preview.sh`, and never with `--headless`.** It stands up the whole client scene,
`MapView`'s shaders included, so it needs a real renderer — and the wrapper is what stops the window
taking the keyboard from another session (`test-harnesses.md` → "The harness window is quiet, the
GAME's is not"). Only the `--import` step is headless.

**The port block rides in the environment because the client's own precedence is env → ports file →
default** (`ServerPortsFile`). Passing the three variables is what keeps a probe run off the default
block *and* off whatever block another checkout's server is holding; without them the probe reads a
`ports.json` written by whichever server booted last, which in a repo worked by several worktrees is
nobody's in particular.

## `tools/live_seat_probe.gd`

The whole probe: the three assertions above, the `_fail` sink holding the file's only `push_error`,
and the single `_finish` that quits `EXIT_OK`/`EXIT_FAILED` and disarms the hang guard — the same
contract the render harnesses keep, and for the same reason (`grep FAIL` and `grep ERROR:` both lie).

`_player_bands` filters `populations` to the player's own **resident** bands, excluding parties
exactly as `Hud.update_band_alerts` excludes them: a split makes a band, and a detached party would
flatter the count that assertion 2 turns on.

**Its budgets are wall clock, not frame counts**, so a slow frame rate cannot shorten one.
`REVEAL_TIMEOUT_MSEC` (75 s) covers a cold server — connect, claim, `new_game`, a full worldgen and
the first frame — sized off `Main`'s own `NEW_GAME_ANSWER_TIMEOUT` plus room for one of its re-sends,
because a re-sent `new_game` is a legitimate way for this to succeed.
`COMMAND_EFFECT_TIMEOUT_MSEC` (15 s) is a round trip and not a turn: the server re-captures and
broadcasts immediately after applying a command. `TURN_TIMEOUT_MSEC` (30 s) is one resolve.

## `tools/live_seat_probe.tscn`

The entry point every invocation names: the probe node, plus the shared `Watchdog` sibling
(`preview_watchdog.gd`, `harness_name = "live_seat_probe"`).

**It carries that guard because this harness has the same fatal shape as the render ones** — a whole
run inside one `await`ing `_ready()`, where a runtime error aborts the function without ever reaching
`get_tree().quit()`, leaving a process that idles forever with no status at all.
`_await_until` notes progress before each wait, so the guard's stall limit bounds a *wedged phase*
while the probe's own budgets bound the run.
