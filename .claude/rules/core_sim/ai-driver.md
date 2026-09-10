---
paths:
  - "sim_ai/**"
  - "core_sim/tests/ai_seat_scenario.rs"
  - "core_sim/tests/ai_bench.rs"
  - "core_sim/tests/common/ai_process.rs"
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
binary, which `core_sim/tests/ai_seat_scenario.rs` and `ai_bench.rs` then run. The shipped
simulation config is a **file include** (`include_str!` in `bench/mod.rs`), the same file the
server embeds — a config the bench needs, not a crate it links.

Modules: `main.rs` (args, the `bench`/`play` dispatch, the turn loop), `link.rs` (claim · greet ·
hold · reconnect · resync, plus the unseated world-builder connection), `view.rs` (`SeatView` +
`Perception`), `brain.rs` (`Brain`, `PassBrain`, `ScriptedBrain`), `instruments/` (`scoreboard.rs`,
`decisions.rs`, the `Instruments` writer pair), `bench/` (`mod.rs` the harness, `measures.rs` the
logs → measures, `ratchet.rs` report · compare · check · baselines).

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
`--script <path>`; `--seed <u64>`; `--turns <n>`; `--log-dir <path>` (opens the two instruments
below; absent, the process plays unmeasured). A first word of `bench` selects the harness; `play`
is accepted and stripped; anything else is the player, so the launcher's `sim_ai --ports-file …
--faction N` is unchanged.

**The brain's sink.** `Brain::decide(&mut self, view, rng, sink: &mut dyn DecisionSink)` — the
sink is a trait object from `instruments::decisions`, so a brain writes records without knowing
about files, and a brain with nothing to record (`PassBrain`) ignores it. The `ready` row is the
**loop's**, written after the `Orders { Ready }` send, because the loop is what submits — a brain
that overran its budget still gets one. `ScriptedBrain` records one accepted `Decision` per fired
line (`specialist: "scripted"`, `intent: "script"`, both scores `SCRIPT_SCORE` = 1.0, `reason` the
resolved command text), so a Scripted-vs-Pass comparison reads non-zero on the specialist row.

## The instruments (`instruments/`)

Two JSON-lines files under `--log-dir`, one record per line, every write flushed so a killed process
leaves the ticks it saw behind. The bench reads nothing else.

**`scoreboard.jsonl`** — one `ScoreRow` per **acted** tick, written before `decide` runs, off the
`SeatView` (`plan_ai_driver.md` §8.1). `tick`, `faction`, `population_children/working/elders`
(this faction's `demographics` row), `food_stock` (Σ own bands' `stores[FOOD_CARGO_KEY]`, the wire's
fixed-point divided out by `FIXED_POINT_SCALE`), `food_income`, `food_consumption`,
`sustainable_yield`, `actual_yield` (Σ own labor rows), `runway_turns` (min `turns_of_food`;
`NOT_FOOD_LIMITED_TURNS` = 999.0 when no band is limited, restated from
`core_sim::snapshot::population`), `idle_workers`, `patches_owned` / `patches_improved`
(`owner == faction`; of those `is_cultivated || is_field`), `herd_biomass_in_view`,
`herds_corralled`, `intensification_knowledge` and `craft_knowledge` (id → progress),
`deaths_by_cause` (cause token → count) and `victory_progress` (mode id → progress; the frame is
viewer-scoped, so `victory.modes` is already this seat's).

⛔ **A `died` event's tick is one behind the frame that first carries it** (`EVENT_TICK_LAG` = 1).
The population systems run on tick T, then `advance_tick`, then `capture_snapshot` — all inside
`TurnStage::Snapshot` in that order (`core_sim/src/lib.rs`) — so the row at frame T counts the
`died` rows stamped T − 1: the turn that produced the frame. Counting at T reads every death as
zero. The cause vocabulary (`hunger` / `cold` / `heat` / `age`) and the
`kind` token `died` are restated from `DeathCause::as_str` and `CommandEventKind::as_str`; the
`count=` token is what is summed, so one row burying three people counts three.

**`decisions.jsonl`** — tagged by `kind`: `decision` (`tick`, `specialist`, `intent`, `score_raw`,
`score_final`, `outcome: accepted | rejected` with `rejected_by: <arbiter step>` on a rejection,
`reason`, `commands`), `plan` (`tick`, `stance`, `since_tick`, `budgets`, `priorities`), `alarm`
(`tick`, `specialist`, `alarm`), `ready` (`tick`), and `link` (`tick`, `event:
command_reconnect | stream_reopen`). Only `decision`, `ready` and `link` are written by the two
shipped brains; `plan` and `alarm` are the orchestrator's (`plan_ai_driver.md` §3).

## The bench (`sim_ai bench`)

`sim_ai bench --seeds <u64,…> --turns <n> --seats <faction>=<brain>[:<script>] … --out <dir>
[--server <path>] [--config <path>] [--compare <other-out-dir>] [--check <baselines.json>]
[--write-baselines <path>]`. `--server` defaults to `server` beside the executable; `--config` to
the embedded shipped config; seat `0` and a repeated faction are refused.

Per seed, under `<out>/<seed>/`: the scratch `simulation_config.json` (the shipped one with
`map_seed`, `default_ai_faction_count` = the seat count, and `faction_start_min_separation` = 6
pinned, and the four port keys rewritten to a probed free base from 46000 up — the same four-key
rewrite as `core_sim::apply_port_base`, restated), `ports.json`, `server.log`, `saves/`, and one
`seat_<f>/` per seat holding its two instruments and `sim_ai.log`. The server is started with
`SIM_CONFIG_PATH` / `SIM_PORTS_FILE` / `SIM_SAVE_DIR` set and `SIM_PORT_BASE` removed, exactly as
`core_sim/tests/query_seat_gate.rs` does; the world is a 24×16 `earthlike` / `late_forager_tribe`
`new_game` sent from an **unseated** connection and synchronised by a `ListSaves` question behind
it. Seats are this same executable, spawned with `--turns n --log-dir <out>/<seed>/seat_<f>`. The
server is killed on drop, panic or early return included. One seed of 6 turns is ~3 s; 30 turns ~2 s
more.

⛔ **The bench holds the human seat until every rival has claimed.** The turn gate resolves the
moment every *occupied* seat has submitted, so a rival that claimed and readied before its neighbour
claimed would advance the world alone, and which tick a seat first sees would depend on process
scheduling. So the harness claims seat 0 on a `Link` of its own before spawning the rivals, polls
`server.log` for `seat.claimed … faction=<f>` (ANSI stripped, token-matched so `faction=1` is not
`faction=10`) for each, then drops the link. Nothing is ever sent on it, and it is not a host: turns
resolve on the rivals' `ready` alone (`SeatTurnGate` → `TurnWait::Resolve`).

**Measures** (`bench/measures.rs`, `plan_ai_driver.md` §8.2), a flat map of dotted name → value,
`null` where the log cannot answer yet, written to `<out>/report.json` and printed as a table
(knowledge and victory rows omitted from the table only):

| Layer | Measures |
|---|---|
| whole seat | every `ScoreRow` scalar at the last row; `knowledge.intensification.<id>`, `knowledge.craft.<id>`, `victory.<mode>`; `deaths.<cause>` for **every** cause (0 when none, so two runs always carry the same keys); `hunger_deaths_total` over the run |
| per specialist (`specialist.<name>.`) | `accepted`, `rejected.<rejected_by>`, `acceptance_rate`, `liveness` (1.0 iff accepted > 0 in **every** window of `LIVENESS_WINDOW_TURNS` = 10 over the run's tick span), `intent_churn` (mean distinct accepted intents per window) |
| orchestrator | `orchestrator.stance_switches_per_100_turns`, `orchestrator.alarm_latency_turns` (mean ticks from an `alarm` to the next `plan` whose budgets differ from the one in force) — **`null` until a brain writes `plan`/`alarm` records** |
| link | `link.turns_observed` (distinct scoreboard ticks), `link.turns_lost_to_timeout` (observed ticks with no `ready`), `link.reconnects` (`command_reconnect` records; a stream reopen is not one) |

**`--compare`** requires the same seeds, turns and seats and writes `this − other` per measure
(`null` where either side is) into the report's `compare` and a delta column. **`--check`** loads
`{ seeds, turns, seats, measures: {seed: {seat: {measure}}}, tolerance: {measure: abs} }`, refuses
a shape mismatch, and lists every violation then exits 1: a measure in `tolerance` fails **below**
`baseline − tolerance`, except the lower-is-better set (`hunger_deaths_total`, `deaths.*`,
`link.turns_lost_to_timeout`, `link.reconnects`) which fails **above** `baseline + tolerance`.
Measures absent from `tolerance` are reported, never checked. **`--write-baselines`** writes the run
with tolerance `BASELINE_TOLERANCE` = 0 on the `RATCHETED_MEASURES` — `population_children`,
`population_working`, `population_elders`, `food_stock`, `hunger_deaths_total` — the primary and
the guard of §8.2 plus the larder.

**`sim_ai/bench/baselines.json`** is the all-Pass control: seeds `11, 23`, 30 turns, seats `1=pass
2=pass` (`BASELINE_SEEDS` / `BASELINE_TURNS` / `BASELINE_SEATS`, a unit test holds the file to
them). A Pass seat assigns nobody, so it starves: 22 hunger deaths per seat and 2 working left by
turn 30 on both seeds. Regenerate it in the PR that moves it, with the numbers in the PR body.

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

## The scenario and the bench test (`core_sim/tests/ai_seat_scenario.rs`, `ai_bench.rs`)

Both drive the built `server` and the built `sim_ai` over the real sockets and share
`core_sim/tests/common/ai_process.rs`: `Scratch`, the kill-on-drop `Process`, `server_binary`,
`sim_ai_binary` (the sibling, or the private fallback build into `target/ai_process_fallback`),
`strip_ansi`, `log_tail`.

`ai_bench.rs` runs the built bench three times on seed 11 for 6 turns: all-Pass twice, asserting
every measure identical and every `--compare` delta zero or null; then `1=scripted 2=pass` with the
scenario's split script, asserting `specialist.scripted.accepted > 0` and that at least one
scoreboard measure of seat 1 differs from the same seat's under Pass — "acts instead of passing" as a
number. Seconds, not minutes; the 30-turn baseline is not generated by a test.

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
