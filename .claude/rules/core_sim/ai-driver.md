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
hold · reconnect · resync, plus the unseated world-builder connection), `view.rs` (`SeatView`,
`SeatMemory`, `Perception`), `geometry.rs` (the odd-r hex distance, restated), `profile.rs`
(`AiProfile`, `Difficulty`, the file), `orchestrator/` (`Plan`, `ConstantStance`), `specialists/`
(the trait, `Food`, `Land`, `Scripted`), `arbiter.rs` (the six steps), `brain.rs` (`Brain`,
`PassBrain`, and the `Composite` the scripted and utility brains are configurations of),
`instruments/` (`scoreboard.rs`, `decisions.rs`, the `Instruments` writer pair), `bench/` (`mod.rs`
the harness, `measures.rs` the logs → measures, `ratchet.rs` report · compare · check · baselines).

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
with the token the seat still holds. **The two sockets carry separate generations** — `generation`
for the command link, `stream_generation` for the stream — and a reader thread is stamped with its
own, so a stale thread's last words are ignored. `reconnect` bumps both and spawns a new reply
reader; `reopen_stream` bumps only the stream's and spawns **none**, because the command socket is
still live and a second `read_exact` on it would split a reply frame between two readers. Exactly one
reply reader exists per command socket, for that socket's life.

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

⛔ **`last_acted_tick` does not survive a world rebuild** (`carried_acted_tick`). The turn loop skips
a tick it has already acted on (`tick <= acted`), so a tick carried across a rebuild silences the
seat: a New Game or Load from the client's menu leaves the child running and its claim intact
(`retain_claimed_seats` keeps a faction still in the roster), and the AI resyncs at the new world's
tick 0. Every turn up to the old world's last-acted tick would then skip both `decide` **and** the
`Orders{Ready}` send — an occupied, silent seat, so each of those turns burns the full
`seat_turn_timeout_seconds` (120 s) before the server auto-submits, and `turns_observed` stops
climbing so `--turns n` never ends. The tick is therefore carried only while the frame's
`world_epoch` matches **and** its tick is ≥ the one held; anything else clears it. A mid-turn
recapture at the same epoch keeps it.

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
`snapshot_flat`) *or* `--host --command-port --stream-port`; `--faction <u32>`;
`--brain pass|scripted|utility`; `--script <path>`; `--profile <id>` (default the file's first
entry); `--difficulty <id>` (default `normal`); `--profiles <path>` (a file replacing the embedded
one whole — missing or broken fails the process, never falls back); `--disable <specialist>`
(repeatable, the ablations); `--seed <u64>`; `--turns <n>`; `--log-dir <path>` (opens the two
instruments below; absent, the process plays unmeasured). A first word of `bench` selects the harness; `play`
is accepted and stripped; anything else is the player, so the launcher's `sim_ai --ports-file …
--faction N` is unchanged.

**The brain's sink.** `Brain::decide(&mut self, view, rng, sink: &mut dyn DecisionSink)` — the
sink is a trait object from `instruments::decisions`, so a brain writes records without knowing
about files, and a brain with nothing to record (`PassBrain`) ignores it. The `ready` row is the
**loop's**, written after the `Orders { Ready }` send, because the loop is what submits — a brain
that overran its budget still gets one. `ScriptedBrain` records one accepted `Decision` per fired
line (`specialist: "scripted"`, `intent: "script"`, both scores `SCRIPT_SCORE` = 1.0, `reason` the
resolved command text), so a Scripted-vs-Pass comparison reads non-zero on the specialist row.

## The utility brain (`brain.rs`, `orchestrator/`, `specialists/`, `arbiter.rs`)

`docs/plan_ai_driver.md` §1–§6 is the design; this is what stands. The three brains are one
`Composite { orchestrator, specialists, arbiter, memory, plan }` with parts removed:

| Brain | Orchestrator | Specialists | Arbiter |
|---|---|---|---|
| `PassBrain` | — | — | emits nothing (a unit struct, not a composite) |
| `ScriptedBrain` | none (`Plan::pass_through`) | `Scripted` | `Arbiter::PassThrough` — every proposal accepted in order, raw = final |
| `UtilityBrain` | `ConstantStance` | `Food`, `Land` (minus `--disable`) | `Arbiter::Weighing` — the six steps |

**`decide`, in order:** `memory.observe` (sightings, arrivals, what every worked row realized, and
a `warn!` per command the sim refused last turn — the feed's `… failed` rows); the orchestrator
(a `plan` record when it re-plans); every specialist proposes (an `alarm` record per alarm, queued
for the *next* plan); the arbiter; `memory.record_choices` (the accepted intents, and the move
target any accepted `MoveBand` carries); `memory.remember_runways`. `on_full_frame(tick)` forgets
everything stamped later than `tick` and drops a plan adopted after it.

**`ConstantStance`.** Stance = the archetype's. Budgets = the weights of the enabled specialists
normalised (`food_security → food`, `land_claim → land`; `contact_seeking` is read and funds nothing
until `Contact` exists); priorities = the same weights raw. Re-plans when `tick − since_turn ≥
goal_cadence_turns` or an alarm arrived since the last plan; an alarm moves `tuning.alarm_budget_shift`
of worker share to the alarming specialist, taken from the others pro rata, and the next cadence
plan reverts it. The stance never moves, so `orchestrator.stance_switches_per_100_turns` is 0 by
construction and the ratchet pins it there.

**Budgets are a share of the seat's working-age pool per turn**, charged by the arbiter as
`floor(share × Σ own working_age)`. A specialist reads its share (`Plan::worker_share`) and sizes a
proposal to it — `Food` caps *idle hands* at its budget and moves the rest next turn — because a
proposal larger than the slice is `over_budget` whatever its score.

**The intent key** is `<specialist>:<kind>:<subject>` (`food:assign:2`, `land:move:2`,
`food:relieve:19,12`, `food:relieve:game_boar_05`); the scripted fixture's is the bare `script`.
The middle token is the class the behaviour gate reads: `raid` needs `will_raid`, `trade` needs
`will_trade`. The bench's intent histogram groups on `<specialist>:<kind>`.

**The arbiter's rejection set**, fixed: `behavior_gated` (step 1), `outscored` (a second proposal
under an intent already accepted this turn), `conflict` (a band already ordered this turn — one
order per band), `over_budget`. Selection: sorted by final score; `selection_top_k = 1` is argmax;
above it each pass draws uniformly among the top k still unpicked from the `(seed, faction, tick)`
rng, so the order — not the set — is what difficulty moves.

### `Food`

Owns `runway_turns`; alarms `food_short` when the minimum own-band `turns_of_food` is below
`food.runway_floor_turns`. Every command is `assign_labor` with kit and floor left `None` (the
job's default on the wire).

- *idle hands* — a band's idle workers onto the source a crew of that size takes the most from,
  as many as the budget allows. Intent `food:assign:<band>`.
- *runway* — under the alarm, the band's lowest-yielding worked row is emptied onto its highest,
  as far as the budget reaches. `food:runway:<band>`.
- *overuse* — a row whose `actual_yield > sustainable_yield`, a hunt row the sim marks
  `hunt_useful_workers == 0`, or a **dead row** (below) is emptied onto the next-best source.
  `food:relieve:<source>`.

⛔ **A source is ranked on what the crew will take, and on what this seat has measured — never on
the published per-worker rate alone.** Three facts of the frame forced this:

1. `per_worker_yield` is a *rate*; a crew of `n` takes `min(n × rate, ceiling)` with the ceiling
   composed from wire terms as `biomass × provisions_per_biomass` (the take at a zero escapement
   floor; the sim's default floor is not published, so it is an upper bound). Ranking on the rate
   sent seventeen hands to a herd of two animals.
2. **Every herd's `per_worker_yield` reads 0.8** on the fixture worlds, and a bare-handed crew
   realizes ~0.01/worker on it — the row's `sustainable_yield` and `hunt_useful_workers` only exist
   once the row is worked. So `SeatMemory` keeps what every worked row **realized** per worker and
   the mean per web (`realized_for_kind`), and a source is ranked on its own realized rate, else the
   web's, else the forecast. A row realizing under `food.poor_yield_fraction` of its forecast for
   `food.dead_row_turns` consecutive turns is **dead**: relieved, and avoided while remembered.
3. **A patch row is not a gathering site.** `forage_patches` is published for every food-bearing
   tile, but `assign_labor forage` is refused *"nobody gathers here"* unless the tile carries a
   food module (`plant_rung_site_refusal` → `FoodSiteRegistry::is_site`), so a forage source must
   also be in `snapshot.food_modules`.

### `Land`

Owns `patches_owned`; alarms `land_short` when a band's ground (`carrying_capacity` of the patch it
stands on) is below its `food_consumption` and no better patch is in view.

- *blind* — fewer than `land.known_tiles_floor` known tiles within `land.horizon_tiles` of a band
  posts `land.scout_workers` scouts with `assign_labor … scout <n>`, once. `land:scout:<band>`.
  ⛔ **The `scout <x> <y>` verb is retired server-side** (`command.retired=ignored`,
  `core_sim/src/bin/server.rs`); the standing scout role posts vantage points around the band.
- *better ground* — while the runway is falling, a discovered, unowned, **unoccupied** patch within
  the horizon with a higher `carrying_capacity` than the band's own proposes `move_band`, and the
  intent persists until arrival: the memory holds the target and re-proposes the same
  `land:move:<band>` each turn, which is what the commitment bonus rewards.
- *room* — under `Expand`, a band above `land.split_size` standing on ground the faction owns
  proposes `split_band` with half its workers. `land:split:<band>`.

⛔ **Contact hands a band over.** The sim's knowledge migration (`advance_population_migration`,
`core_sim/src/systems/population.rs`) rewrites the faction of a band that is settled
`migration_min_settled_turns`, above `migration_morale_threshold` morale, carries knowledge, and is
in **contact** with another people — in either direction. On the bench's seed 11 the rival's band
joined the utility seat at tick 6 and the seat's own band left at tick 18, which is why its row at
turn 30 reads `population_working 0` with `hunger_deaths_total 0`. *Better ground* excludes ground a
visible foreign band stands on; it cannot see a rival the fog hides, and it does not model sight
range, so the exposure remains.

### `SeatMemory` (`view.rs`)

A pure function of the frames received, dropped past a full frame's tick. Per tile, the last tick
it was `Active` (or first known, for ground discovered before the process watched), decayed by the
difficulty's `memory_horizon_turns` (`0` never decays); last turn's chosen intents; the alarms
since the last plan; per band the `land:move` target still being walked to (cleared on arrival) and
last turn's `turns_of_food`; per worked row (`<band>:<kind>:<x>,<y>` or `<band>:<kind>:<fauna_id>`)
what it realized per worker and for how many consecutive turns, kept when the row is emptied so a
dead source is judged on its record.

**Restated constants.** The visibility raster is fixed-point: `Active` = `Scalar::SCALE` (1.0),
`Discovered` = `SCALE / 2`, `Unexplored` = 0 (`visibility_raster_from_ledger`,
`core_sim/src/snapshot/vision.rs`) — `VISIBILITY_ACTIVE` / `VISIBILITY_DISCOVERED` in `view.rs`.
`geometry.rs` restates `hex_distance_wrapped` (odd-r offset → axial, cube distance, the shortest
wrapped column delta) from `core_sim/src/grid_utils.rs`, because the sim's assignment loop lapses a
row outside `work_range` / `hunt_reach` in that metric.

## The profile file (`data/ai_profiles.json`, `profile.rs`)

Embedded at build time; `deny_unknown_fields` on every struct, `schemars` derives the schema a
unit test validates the shipped file against, and `validate()` refuses an incoherent file (unknown
or negative weight, zero cadence, a share outside 0..1). Two profiles ship, `forager` (consolidate)
and `rover` (expand). Each key has one consumer:

| Key | Consumer | Effect |
|---|---|---|
| `archetype` (`expand` / `consolidate` / `seek`) | `ConstantStance` | the stance |
| `behaviors.will_raid`, `behaviors.will_trade` | arbiter step 1 | gate the `raid` / `trade` intent classes |
| `weights.food_security`, `weights.land_claim` | `ConstantStance` | budget share (normalised) and priority (raw) of `Food` / `Land`; each specialist also scales its own scores by its weight |
| `weights.contact_seeking` | none yet | kept in the schema for `Contact` |
| `commitment` | arbiter step 3 | `score *= 1 + commitment` on an intent chosen last turn (the orchestrator's switch margin has no switching stance to apply to in v1) |
| `food.runway_floor_turns` | `Food` | the `food_short` alarm and *runway* |
| `food.dead_row_turns` | `Food` | consecutive poor turns before a row is dead |
| `food.poor_yield_fraction` | `Food` | the share of the forecast a row must realize per worker |
| `land.known_tiles_floor` | `Land` | *blind*'s floor |
| `land.split_size` | `Land` | *room*'s band size |
| `land.horizon_tiles` | `Land` | how far *blind* counts and *better ground* looks |
| `land.scout_workers` | `Land` | how many scouts *blind* posts |

| Difficulty key | Consumer | Effect |
|---|---|---|
| `selection_top_k` | arbiter step 4 | 1 = argmax; k = uniform among the top k per pass |
| `goal_cadence_turns` | `ConstantStance` | turns a plan stands |
| `memory_horizon_turns` | `SeatMemory` | a tile seen longer ago is unknown again; 0 = never |

| Tuning key | Consumer | Effect |
|---|---|---|
| `alarm_budget_shift` | `ConstantStance` | worker share moved to an alarming specialist for one cadence |

`StartProfileOverrides::ai_profile_overrides` and the `late_forager_tribe` block that carried
`scout_bias` / `camp_rotation_period` are deleted: a start profile is per campaign and AI tuning is
per seat (`docs/plan_ai_opponents.md` §7).

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
`deaths_by_cause` (cause token → count), `victory_progress` (mode id → progress; the frame is
viewer-scoped, so `victory.modes` is already this seat's), and `commands_failed` — this faction's
feed rows whose label ends `" failed"` (`emit_command_failure`, `core_sim/src/bin/server.rs`: a
command the *sim* refused; the seat gate's refusals are `command.rejected` log lines and never
reach the feed).

⛔ **A `died` event's tick is one behind the frame that first carries it** (`EVENT_TICK_LAG` = 1).
The population systems run on tick T in `TurnStage::Population`, then `advance_tick` and
`capture_snapshot` run in `TurnStage::Snapshot` (`core_sim/src/lib.rs`), in that order — so the row
at frame T counts the `died` rows stamped T − 1: the turn that produced the frame. Counting at T
reads every death as zero. The cause vocabulary (`hunger` / `cold` / `heat` / `age`) and the
`kind` token `died` are restated from `DeathCause::as_str` and `CommandEventKind::as_str`; the
`count=` token is what is summed, so one row burying three people counts three.

**`decisions.jsonl`** — tagged by `kind`: `decision` (`tick`, `specialist`, `intent`, `score_raw`,
`score_final`, `outcome: accepted | rejected` with `rejected_by: <arbiter step>` on a rejection,
`reason`, `commands`), `plan` (`tick`, `stance`, `since_tick`, `budgets`, `priorities`), `alarm`
(`tick`, `specialist`, `alarm`), `ready` (`tick`), and `link` (`tick`, `event:
command_reconnect | stream_reopen`). Only `decision`, `ready` and `link` are written by the two
shipped brains; `plan` and `alarm` are the orchestrator's (`plan_ai_driver.md` §3).

## The bench (`sim_ai bench`)

`sim_ai bench --seeds <u64,…> --turns <n> --seats <spec> … --out <dir> [--server <path>]
[--config <path>] [--compare <other-out-dir>] [--check <baselines.json>] [--write-baselines <path>]`.
`--server` defaults to `server` beside the executable; `--config` to the embedded shipped config;
seat `0` and a repeated faction are refused. An operator's `RUST_LOG` is passed through to the
seats (default `info`).

**The seat spec** is `<faction>=<brain>[:<script|profile>][@<difficulty>][~<specialist>]*`: the
token after `:` is a script for `scripted` and a profile for `utility`; `@` names a difficulty and
each `~` a specialist left off the roster, both `utility` only. `1=pass`, `2=scripted:orders.txt`,
`1=utility:forager`, `1=utility:rover@hard~land~food`.

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
| whole seat | every `ScoreRow` scalar at the last row; `knowledge.intensification.<id>`, `knowledge.craft.<id>`, `victory.<mode>`; `deaths.<cause>` for **every** cause (0 when none, so two runs always carry the same keys); `hunger_deaths_total` and `commands_failed_total` over the run |
| per specialist (`specialist.<name>.`) | `accepted`, `rejected.<rejected_by>`, `acceptance_rate`, `liveness` (1.0 iff accepted > 0 in **every** window of `LIVENESS_WINDOW_TURNS` = 10 over the run's tick span), `intent_churn` (mean distinct accepted intents per window); and `intent.<specialist>:<kind>`, the share of every accepted decision under each intent class |
| orchestrator | `orchestrator.stance_switches_per_100_turns`, `orchestrator.alarm_latency_turns` (mean ticks from an `alarm` to the next `plan` whose budgets differ from the one in force) — `null` on a seat whose brain writes no `plan`/`alarm` records (Pass, Scripted) |
| link | `link.turns_observed` (distinct scoreboard ticks), `link.turns_lost_to_timeout` (observed ticks with no `ready`), `link.reconnects` (`command_reconnect` records; a stream reopen is not one) |

**`--compare`** requires the same seeds, turns and seat **factions** (the brains may differ —
that is what a comparison is for) and writes `this − other` per measure (`null` where either side
is) into the report's `compare` and a delta column, plus `intent_distance_l1` per seat: Σ |Δ| over
the two runs' `intent.*` shares (0 = the same behaviour, 2 = disjoint) — the number "two profiles
that visibly differ" resolves to. **`--check`** loads `{ runs: { "<seats joined by a space>": {
seeds, turns, seats, measures: {seed: {seat: {measure}}}, tolerance: {measure: abs} } } }`, finds
the entry with this run's exact seeds, turns and seat specs (none is a mismatch naming what the
file holds), and lists every violation then exits 1: a measure in `tolerance` fails **below**
`baseline − tolerance`, except the lower-is-better set (`hunger_deaths_total`, `deaths.*`,
`link.turns_lost_to_timeout`, `link.reconnects`, `commands_failed_total`,
`orchestrator.stance_switches_per_100_turns`) which fails **above** `baseline + tolerance`.
Measures absent from `tolerance`, or `null` on either side, are reported, never checked.
**`--write-baselines`** upserts this run's entry into the file (merging, not replacing) with
tolerance `BASELINE_TOLERANCE` = 0 on the `RATCHETED_MEASURES` — `population_children`,
`population_working`, `population_elders`, `food_stock`, `hunger_deaths_total`,
`commands_failed_total`, `orchestrator.stance_switches_per_100_turns`.

⛔ **A dead seat is not a bar, and `--check` says so before it compares anything.** A run that
starves to `population_working` 0 records `hunger_deaths_total` 0 and `food_stock` 0 as well — and
under lower-is-better at tolerance 0 those zeros would fail *the fix*: reviving the band produces
deaths where the baseline holds none. So the file carries a first-class `degenerate: [{seed, seat,
note}]` list, `--write-baselines` marks every row it writes whose `population_working` is
`NO_WORKING_POPULATION`, and `--check` runs a liveness precondition first:

- the list and the rows must agree — a dead row nobody marked, or a marked row that is alive, is a
  `degenerate_marker` violation, so the file cannot quietly drift into blessing a corpse;
- a row recorded dead **gates nothing** — its outcome measures are not comparable;
- a row recorded *alive* whose run has no working population is a `seat_alive` violation in its own
  right, which is what still catches a regression into death.

**A specialist that proposes nothing is a violation, not a silence.** `specialist.<name>.liveness`
must read 1.0 for the roster the seat spec implies (`utility` → `DISABLEABLE_SPECIALISTS` minus its
`~` ablations, `scripted` → `scripted`, `pass` → none); an **absent** key — the shape a specialist
that never proposed leaves behind — is a violation, held against the run alone rather than the
baseline. Without this an inert specialist reads as a passing check.

**`sim_ai/bench/baselines.json`** holds two entries on seeds `11, 23` for 30 turns
(`BASELINE_SEEDS` / `BASELINE_TURNS` / `BASELINE_SEAT_SETS`; a unit test holds the file to them):
the all-Pass control `1=pass 2=pass` — a Pass seat assigns nobody, so it starves: 22 hunger deaths
and 2 working left by turn 30 on both seeds — and the utility forager `1=utility:forager 2=pass`.
Regenerate an entry in the PR that moves it, with the numbers in the PR body.

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

`ai_bench.rs` runs the built bench on seed 11: all-Pass twice over `TURNS` (6), asserting every
measure identical and every `--compare` delta zero or null; then `1=scripted 2=pass` with the
scenario's split script, asserting `specialist.scripted.accepted > 0` and that at least one
scoreboard measure of seat 1 differs from the same seat's under Pass — "acts instead of passing" as
a number; then `1=utility:forager 2=pass`, asserting `specialist.food.accepted > 0`,
`specialist.food.liveness` 1.0, `commands_failed_total` 0, and no `command.rejected` /
`command.split.rejected` line in the server log — a proposal the server refuses is a bug in the
specialist's command construction, and this is where it shows.

⛔ **The utility leg runs `UTILITY_TURNS`, not `TURNS`, and the test proves its own span first.**
`liveness` cuts windows of `LIVENESS_WINDOW_TURNS` (10, restated here from `measures.rs`), so over a
6-tick span there is exactly **one** window and `liveness == 1.0` is arithmetically the same claim as
`accepted > 0` — which the line above it already makes. `UTILITY_TURNS` is `LIVENESS_WINDOW_TURNS +
2`, and the test asserts `link.turns_observed > LIVENESS_WINDOW_TURNS` *before* it asserts liveness,
so the assertion cannot silently decay back into a restatement. Seconds, not minutes; the 30-turn
baselines are not generated by a test.

`ai_seat_scenario.rs` is a built `server` and a built `sim_ai --brain scripted --faction 1 --turns
3`, over the real sockets. The script's one order is `split_band {faction} {own_band:0} 4` — the
`settle.min_founding_workers` floor, legal for a 30-person starting band with `parent_min_workers 6`
to spare — so the rival's resident band count moves by one. Frames are viewer-scoped and fogged, so
the rival's world is read through **seat 1** both times: before, on a connection released before the
AI starts, and after, once the AI has exited and released it. The test also asserts the tick
advanced by at least three,
`seat.claimed … faction=1` is in the server log, and no `command.rejected` is. The `sim_ai` binary is
the server binary's sibling; if a narrower `cargo test` invocation left it unbuilt, the test builds
it into a private target directory (`FALLBACK_TARGET_DIR` = `ai_process_fallback`, as above),
because the outer `cargo test` holds the shared target directory's lock for the whole run.
