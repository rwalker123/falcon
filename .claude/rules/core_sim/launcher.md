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

`local_seats` returns exactly one seat: the human's client. Rival seats are **vacant**, and the
server's turn scheduler auto-submits a vacant seat immediately, so a world with N rival seats paces
exactly the way single-player always has. Filling another seat is adding an entry to that list, not
adding a code path.

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

`wait_for_players` is what ends the run: the launcher returns once the locally-hosted player
processes have exited, and `Drop` takes the server down behind it. With the single seat that exists
today this is the same "wait for the game window to close" the one-child launcher did.

## Readiness is unchanged by seats

Seats are filled **after** `wait_for_ready` returns, so a player process never starts before the
server has published a *complete* handshake file (parses as an object carrying every key
`ServerPortsFile.gd` reads). A partially written file satisfying an existence check would fail
silently — that script degrades a failed parse to the hardcoded 41000 block and dials a dead port.

## Tests

The crate's tests are unit tests inside `main.rs`, because the launcher is a `[[bin]]`.

- `the_only_local_seat_today_is_the_humans_client` pins both halves of the seat list: the human is a
  seat, and the rivals are vacant.
- `every_child_is_reaped_when_the_session_drops` builds a `Session` over a server and two stand-in
  players and asserts, via `ps`, that none of them outlives the drop and that the handshake file is
  gone. It is unix-gated for its `sleep` stand-in; the Windows half of the guarantee is the job
  object, which needs a real Windows host to mean anything and is not simulated.
- The stand-in children detach **all three** streams. A child that survived a broken reap while
  holding the harness's captured output pipe turns a clean assertion failure into a ten-minute
  hang — the failure reads as a hung test suite rather than as the bug it is.
