---
paths:
  - "launcher/src/main.rs"
  - "launcher/Cargo.toml"
---

# The launcher: one process per locally-hosted seat, and every one of them reaped

`launcher/src/main.rs` is the packaged game's supervisor. It starts the simulation server, waits
for a **real** readiness signal, starts one process per seat this machine hosts, and reaps every
child on every exit path. It replaced `run.bat` / `run.command`, which slept a fixed two seconds
and orphaned a running server whenever the launcher did not exit cleanly.

Packaging layout (where the children sit inside the `.app` / the Windows folder) is
`docs/desktop_builds.md`. The handshake file itself is `.claude/rules/core_sim/ports.md`. The seat
model on the server side is `.claude/rules/core_sim/factions.md` → "Seats".

## A player is a process, and the host's human is not special

A world has N faction seats; the sim's whole model of "who is playing" is whether a seat is
occupied by some connection or vacant. **The person who starts the game is a remote player whose
process happens to be local**, so the Godot client is spawned as *the process filling a seat* —
`local_seats` returns the seats this machine hosts and `Session::fill_seat` starts each one, the
human's included.

**There is deliberately no in-process or in-thread path for a local player.** That second path is
where a shortcut appears which the socket cannot take, and an occupant that took it would be
playing a different game from everyone else. One spawn path, one socket, one set of rules.

**The server never spawns players.** In a real multiplayer game the other seats are on other
machines, so a spawning server would be a local-only special case — the same second path by
another name.

**The launcher passes no seat identity.** A client claims the seat it wants over the command
socket at handshake (`SeatClaim.gd` → `ClaimSeat`), and the server takes the acting faction from
the claim rather than from the wire. So the launcher decides only *how many processes to start and
how to reap them*.

`human_seat` is the one seat filled at boot: the human's client. **Rival seats are supervised**, not
listed — see the next section. A rival seat with no child is vacant, and the server's turn scheduler
auto-submits a vacant seat immediately, so a world whose AI has not started (or has crashed) paces
exactly the way single-player always has.

## The seat supervisor: the roster is the server's to announce

The launcher cannot know at boot how many AI processes to start. The server decides the roster at
**every world build** — `reset_map`, `new_game` from the client's menu (which picks the rival count),
`load_game` — and `retain_claimed_seats` drops the claims a rebuild orphans. So it announces the
roster instead:
one `seats.roster` INFO event per build on the log stream it already publishes (`log_stream.rs`,
`[u32 LE length][JSON]` on the `log` port), shaped
`{"target":"shadow_scale::server","message":"seats.roster","fields":{"factions":"[0,1,2]","world_epoch":3}}`
— `factions` as a JSON array in a string, because `tracing` fields carry no arrays. The doc comment
on `retain_claimed_seats` in `bin/server.rs` states the shape; `parse_roster_event` here is its
contract twin, and a unit test pins that it parses that shape and nothing else.

**A bare boot announces nothing.** The server starts with `world_active = false` and no world, and
`retain_claimed_seats` runs only from the three build arms above — so the first `seats.roster` event
of a session is the one the first map reset, new game or load emits. A launcher started against an
idle server supervises zero rivals until then, which is the expected state and not a lost event.

**Reconciliation is on the main thread; only the reading is on a thread.** `spawn_roster_watcher`
dials the log port on a `seat-supervisor` thread, redials on a drop (never reporting a drop as an
empty roster, which would reap every rival over a socket hiccup), and forwards events over a channel.
`Session::wait_for_human` drains that channel while polling the human's process, and on each event
`Session::reconcile`: every roster faction other than the human's (`HUMAN_FACTION_ID`, the twin of
`HudConst.PLAYER_FACTION_ID`) with no *running* child gets a `sim_ai --ports-file <path> --faction
<id> --brain <brain>` (`current_dir` the data dir, stdin null, **adopted into the process group
right after the spawn** — `fill_seat`'s ordering); every running child whose faction is not named is killed and
reaped. A child that exited on its own is dropped at the next event and respawned by it if its
faction is still seated — **one respawn per roster event, never a tight loop**.

**Timing.** The watcher connects **before** `fill_seat` starts the human's client. The boot world is
idle until that client asks for one, so the first roster the server ever announces comes after the
reader exists; the server does not re-emit on connection, and needs no second socket.

⛔ **The brain is named on the spawn, because `sim_ai`'s own default is the control.**
`--brain` defaults to `Pass` *inside `sim_ai`* — the bench's control arm, which assigns no labor and
starves (22 hunger deaths by turn 30). A rival spawned without the flag is therefore a corpse, so
`rival_brain()` always passes one: `AI_BRAIN_DEFAULT` (`utility`), or `$SIM_AI_BRAIN` (`ENV_AI_BRAIN`)
when that is set to something non-blank — how a developer puts the rivals on `pass` or `scripted` for
one run without a rebuild. `brain_from_override` holds the rule apart from the environment so it can
be unit-tested without mutating it. Profile and difficulty are **not** launcher levers: `ai_profiles.json`
is embedded in `sim_ai` and there is no launcher-side profile source, so those stay at `sim_ai`'s
defaults.

`sim_ai` sits beside the server in the packaged layout (`Layout::ai`, `AI_STEM`), and
`Layout::resolve` requires it the way it requires the server. The program itself is
`.claude/rules/core_sim/ai-driver.md`.

## The run directory: a played game can be viewed afterwards

Every launcher session gets `<data_dir>/runs/<run_id>` (`RUNS_DIR`; the id is
`run-<start unix seconds>-<pid>`, `mint_run_id`, so ids sort by time and two launchers started in
the same second differ). The path is printed to the launcher's stderr (`report_info`) at start and
again at exit, with the viewer command, because it is the one thing a player needs to open the game
they just played. Under it:

- `record/` — the **server's** record: the launcher sets `SIM_RECORD_DIR` (`ENV_RECORD_DIR`, twin
  of `core_sim::record::RECORD_DIR_ENV`) on the server spawn, so every frame published to every
  seat — the human's included — and every logged command is written there by the server's own
  `run-recorder` thread. What is in it, and how `sim_ai import-record` reads it, is
  `ai-driver.md` → "A played game becomes a viewable run".
- `seat_<faction>/` — each rival's own instruments: `spawn_rival` passes `--log-dir` to every
  `sim_ai` it starts (`Session::rival_log_dir`, `SEAT_LOG_DIR_PREFIX`). The human's client writes
  none; its seat is imported from `record/` by the viewer.

**The launcher prints the exact lines.** `viewer_lines` renders, per seat, `<sim_ai> viewer <run
dir> --seat <f> --out <run dir>/seat_<f>.html` with `Layout::ai` (the very binary rivals are spawned
with) and the run directory as created, both absolute — paste-ready. The human's line is printed at
start under `VIEWER_LINES_LABEL_START` ("after quitting, open this run with:"), each rival's the
first time `spawn_rival` starts that faction (a roster is only known once the server announces it),
and every seat's again at exit under `VIEWER_LINES_LABEL_EXIT` — `Session::started_rivals` keeps
every faction ever started, so a rival reaped mid-session still gets its line and a respawned one
is not listed twice. Plain `eprintln!` (`report_viewer_lines`), the launcher's existing verbosity,
so a crash still leaves the recipe in the log.

**The same recipe is a file, `<run dir>/open_this_run.txt`** (`VIEWER_RECIPE_FILE`), because in
the packaged game stderr reaches nobody: a double-clicked `.app` has no terminal and the Windows
build has no console (`windows_subsystem = "windows"`). `write_viewer_recipe` rewrites it whole at
each of the three places the lines are printed — start, a rival's first spawn, exit — with
`Session::seats_of_run` (every seat known so far), so whenever the run ends the file is complete.
`viewer_recipe` renders the body: one heading line saying what the file is
(`VIEWER_RECIPE_HEADING`, "Paste one of these lines into a terminal to open the run in the
viewer:"), then one `viewer_lines` line per seat. A write failure is a `report_warning`, never a
fatal — the stderr copy is already out. `scripts/run_stack.sh` prints the equivalent recipe for the
dev stack at exit (`core_sim/CLAUDE.md` → Environment Overrides, `SIM_RECORD_DIR`).

**Pruning.** `create_run_dir` makes this session's directory and then removes every `run-*`
directory under `runs/` but the newest `KEPT_RUNS` (5), by name — a run holds every frame of every
seat, and a machine that plays daily would otherwise fill up with games nobody will look at again.
Only directories carrying `RUN_ID_PREFIX` are touched; removal is best-effort and never reported.

## The reaping guarantee, for N children

This binary exists because a child outliving it holds the port block and the next launch then fails
with a message about a healthy server. Two mechanisms cover the two ways the launcher can end, and
both are per-child rather than per-server:

| Exit | Mechanism | How it covers N |
|---|---|---|
| Orderly (quit, error return, readiness timeout, a seat's program failing to start) | `Session`'s `Drop` | `Session` owns the server **and** a `Vec<Child>` of the filled seats; `drop` kills and waits for each player, then the server, then removes the handshake file. No step may short-circuit the rest — a stop halfway down that list is an orphan. |
| This process killed outright (Windows) | Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` | The group is created **empty, before the first spawn**, and every child joins it through `ProcessGroup::adopt` immediately after it is started. Job membership is **not** inherited from the launcher, so a child never adopted would survive a force-kill exactly the way the orphaned server used to. |

Two ordering rules hold that up:

- **Ownership moves before the group call.** `fill_seat` pushes the spawned child into the
  `Session` and only then adopts it into the process group, so a job-object failure returns an
  error against a process the `Drop` guard is already responsible for.
- **Players are reaped before the server.** A player outliving the sim it was talking to spends its
  last moments reporting a dropped connection, which is a confusing thing to show on the way out of
  a clean quit.

**The human's client owns the session window.** `wait_for_human` is what ends the run: the launcher
returns when the human's process exits, and `Drop` reaps every AI child, then the players, then the
server, then removes the handshake file. An AI child exiting on its own does **not** end the run —
its seat goes vacant (auto-submitted) until the next roster event respawns it.

## Readiness is unchanged by seats

Seats are filled **after** `wait_for_ready` returns, so a player process never starts before the
server has published a *complete* handshake file (parses as an object carrying every key
`ServerPortsFile.gd` reads). A partially written file satisfying an existence check would fail
silently — that script degrades a failed parse to the hardcoded 41000 block and dials a dead port.

## Tests

The crate's tests are unit tests inside `main.rs`, because the launcher is a `[[bin]]`.

- `the_humans_seat_is_filled_with_the_client` pins that the human is a seat filled through the same
  path as any occupant.
- `a_roster_event_parses_and_other_lines_do_not` is the supervisor's half of the `seats.roster`
  contract.
- `reconcile_spawns_a_rival_per_roster_faction_and_reaps_the_departed` drives `reconcile` with a
  `sleep` stand-in program: a roster of `[0, 1, 2]` spawns children for 1 and 2 and none for the
  human; shrinking to `[0, 1]` reaps 2 (asserted via `ps`) and keeps 1; growing again respawns 2.
- `a_run_id_carries_the_prefix_and_the_start_time` and
  `creating_a_run_prunes_the_oldest_beyond_the_kept_count` pin the run id's shape and that a start
  keeps exactly `KEPT_RUNS` runs, the new one included, leaving a non-run directory alone.
- `every_child_is_reaped_when_the_session_drops` builds a `Session` over a server, two stand-in
  players and a stand-in rival and asserts, via `ps`, that none of them outlives the drop and that
  the handshake file is gone. Both are unix-gated for the `sleep` stand-in; the Windows half of the
  guarantee is the job object, which needs a real Windows host to mean anything and is not
  simulated.
- The stand-in children detach **all three** streams. A child that survived a broken reap while
  holding the harness's captured output pipe turns a clean assertion failure into a ten-minute
  hang — the failure reads as a hung test suite rather than as the bug it is.
