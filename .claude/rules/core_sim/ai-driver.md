---
paths:
  - "sim_ai/**"
  - "core_sim/src/record.rs"
  - "core_sim/tests/ai_seat_scenario.rs"
  - "core_sim/tests/ai_bench.rs"
  - "core_sim/tests/ai_record_import.rs"
  - "core_sim/tests/common/ai_process.rs"
  - "core_sim/tests/common/seat_harness.rs"
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
`instruments/` (`scoreboard.rs`, `decisions.rs`, `observations.rs`, the `Instruments` writer
trio), `bench/` (`mod.rs` the harness, `measures.rs` the logs → measures, `ratchet.rs` report ·
compare · check · baselines), `viewer/` (`mod.rs` the join and the page writer, `page.html` the
template), `import_record.rs` (a server run record → a seat log directory).

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
(repeatable, the ablations); `--seed <u64>`; `--turns <n>`; `--log-dir <path>` (opens the three
instruments below; absent, the process plays unmeasured). A first word of `bench` selects the
harness, `viewer` the run viewer and `import-record` the record importer; `play` is accepted and
stripped; anything else is the player, so the launcher's `sim_ai --ports-file … --faction N` is
unchanged.

**The brain's sink.** `Brain::decide(&mut self, view, rng, sink: &mut dyn DecisionSink)` — the
sink is a trait object from `instruments::decisions`, so a brain writes records without knowing
about files, and a brain with nothing to record (`PassBrain`) ignores it. The `ready` row is the
**loop's**, written after the `Orders { Ready }` send, because the loop is what submits — a brain
that overran its budget still gets one. `ScriptedBrain` records one accepted `Decision` per fired
line (`specialist: "scripted"`, `intent: "script"`, both scores `SCRIPT_SCORE` = 1.0, `reason` the
resolved command text), so a Scripted-vs-Pass comparison reads non-zero on the specialist row.

**The brain's lens.** `Brain::lens(&self) -> BrainLens<'_>` is the read-only window the
observation record is captured through — the plan in force, the alarms pending since it, the
`SeatMemory`, and the profile's `land.horizon_tiles`. `Composite` answers all four; the default
(`PassBrain`) answers the empty lens, so a Pass seat's observations carry `plan: null` and no
`last_seen_tick`. The loop reads it **before** `decide`, so what the record shows is what the brain
was handed, not what it did.

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
for the *next* plan); the arbiter; `memory.record_choices` (the accepted intents and their memos —
a move target, a pending split); `memory.remember_runways`. `on_full_frame(tick)` forgets
everything stamped later than `tick`, drops a plan adopted after it, **and resets the orchestrator's
goal cadence** (`Orchestrator::forget_after`) so the dropped plan is re-planned at the new epoch's
first tick. Dropping the plan alone left `since_turn` in the future: `plan()` then returned `None`
for a full cadence and `plan_for` fell back to `Plan::pass_through`, whose budgets and priorities are
empty — a rival that played nothing for the first 8 turns after every New Game or Load.

**`ConstantStance`.** Stance = the archetype's. Budgets = the weights of the enabled specialists
normalised (`food_security → food`, `land_claim → land`; `contact_seeking` is read and funds nothing
until `Contact` exists); priorities = the same weights raw; goals = the profile's `goals` block for
`Food` when it is on the roster (`Land` has none in v1 — `Goals` is the enum that grows). Re-plans
when `tick − since_turn ≥ goal_cadence_turns` or an alarm arrived since the last plan; an alarm
moves `tuning.alarm_budget_shift` of worker share to the alarming specialist, taken from the others
pro rata, and the next cadence plan reverts it — the goals never move with an alarm. The `plan`
record and the observation's `plan` carry `goals: { <specialist>: { net_income_per_turn,
runway_turns, ground_rung } }` (flat, one shape for every specialist), and the viewer's orchestrator
table renders it as `net +1.0/turn · runway 12t · toward field`. The stance never moves, so `orchestrator.stance_switches_per_100_turns` is 0 by
construction and the ratchet pins it there.

**Budgets are a share of the seat's working-age pool per turn**, charged by the arbiter as
`floor(share × Σ own working_age)`. A specialist reads its share (`Plan::worker_share`) and sizes a
proposal to it — *negative income* caps the idle hands it places at its budget and moves the rest
next turn — because a proposal larger than the slice is `over_budget` whatever its score.

**The intent key** is `<specialist>:<kind>:<subject>` (`food:assign:2`, `land:move:2`,
`food:upgrade:2`, `food:settle:9`); the scripted fixture's is the bare `script`.
The middle token is the class the behaviour gate reads: `raid` needs `will_raid`, `trade` needs
`will_trade`. The bench's intent histogram groups on `<specialist>:<kind>`.

**The arbiter's rejection set**, fixed: `behavior_gated` (step 1), `outscored` (a second proposal
under an intent already accepted this turn), `conflict` (a band already ordered this turn — one
order per band), `over_budget`. Selection: sorted by final score; `selection_top_k = 1` is argmax;
above it each pass draws uniformly among the top k still unpicked from the `(seed, faction, tick)`
rng, so the order — not the set — is what difficulty moves.

### `Food` (`specialists/food/`: `mod.rs` the plumbing, `rules.rs` the five rules, `ledger.rs` the projection, `sources.rs` the source vocabulary)

Owns `runway_turns`; alarms `food_short` when the minimum own-band `turns_of_food` is below
`food.runway_floor_turns`. Every assignment is `assign_labor` with kit and floor left `None` (the
job's default on the wire); `policy` is left `None` too, because the field is **retired** —
*"a labor assignment carries a `floor`, not a stance … the server ignores it"*
(`CommandPayload::AssignLabor::policy`), so the balanced take is the default floor.

**The plan hands `Food` goals** (`Plan.goals[food]` = `Goals::Food(FoodGoals { net_income_per_turn,
runway_turns, ground_rung })`, from the profile's `goals` block), and **the goal gap is the score**:
every rule projects the band's book under its change through the ledger below and scores
`goal_progress × weight`. A rule handed no goals — `Plan::pass_through`, the scripted brain's plan,
whose brain has no `Food` — proposes nothing (`the_pass_through_plan_proposes_nothing_from_any_rule`).
Each rule yields at most one proposal per band, the arbiter's one-order-per-band rule keeps one, and
the `reason` is `"<rule>: <subject> [ledger: trough X at tN, positive again tM]"` so the viewer
shows which rule fired and what the ledger said. The rules, in `propose` order:

- **negative income** (`food:assign:<band>`) — fires on `food_income < food_consumption` **or**
  `idle_workers > 0` (idle hands are negative income against what they could earn). Weighs three
  reassignments within budget — (a) the idle hands onto the best source, (b) the *row to empty
  first* onto the best other source, (c) both onto the best source for the whole crew — and takes
  the one closing the most goal gap, ties broken by net income added (`closer`: once the goals
  are met every candidate closes the same nothing, and without the tiebreak the band took the
  first one offered). The row to empty first is an **overused** row (`actual_yield >
  sustainable_yield`), a hunt row the sim marks **`hunt_useful_workers == 0`**, or a **dead row**
  (below) — those need no gain guard — and failing one of those the lowest-paying row, which moves
  only onto ground out-paying it by `food.runway_gain_fraction` per worker **and** whose marginal
  take exceeds what the row earns today. ⛔ Distinctness is not improvement: with only "are these
  distinct rows" between them, two rows paying the same shuffled workers every turn under the alarm
  at the specialist's highest score, and one-order-per-band then rejected the idle hands as
  `conflict`. Not for a travelling band, nor for a child still walking to the site it was split
  toward (it must not strip the parent's ground — the same reason the next rule excludes it).
  Carries its change forward: rules 3–5 project **on top of it**.
- **feed while moving** (`food:feed_move:<band>`) — a band with a move target in memory and not
  yet `is_traveling` works what will fall **outside** its range from the target before it leaves:
  the idle hands and the crews of rows that stay in range after the move, onto the best source that
  will not. Never a band `born_by_split` within `food.split_settle_turns` of its birth.
- **split to feed** (`food:split:<band>`, then `food:settle:<child>`) — after rule 1's change the
  band's projected runway is still under `goals.runway_turns`, it holds `SPLIT_PARENT_CREWS` (2)
  crews of `food.split_band_workers` — *a parent keeps one crew of `split_band_workers` for
  itself, so the band must hold two crews* — no split is pending, **the sim has not refused a split
  of this band at its current size or larger** (`SeatMemory::split_refused_at`, below), and a
  discovered, workable, unowned-or-own site within
  `food.split_search_tiles` but **outside** `work_range` would pay a crew of `split_band_workers`
  more than that crew's consumption share: `split_band <workers>`, with `Memo::Split { target }`.
  The child appears on the parent's tile next turn (`split_band_from_parent`,
  `core_sim/src/systems/fission.rs`); `SeatMemory` matches it and **settle** walks it there with
  `move_band` under `food:settle:<child>` every turn until arrival (the commitment bonus), the
  travel priced at `BAND_MOVE_TILES_PER_TURN` (restated from `labor_config.json`, 1 tile a turn).
  The sim's `split_refusals` (`expedition_config.json`: `min_founding_workers`,
  `parent_min_workers`) are not on the wire and are **not copied here**: at exactly `2 × 5`
  working-age the split leaves 5 and is refused, the refusal shows in the failed-command log, the
  pending entry expires — and the memory learns from the frame that a band of *that* size cannot
  split, so the rule is silent until the band has grown.
- **spare hands into hunts** (`food:hunt:<band>`) — projected net after rule 1 is at
  `goals.net_income_per_turn` or within `food.near_positive_fraction` of it, and a live huntable
  herd is in reach: the most hands off the lowest-paying **forage rows** (never the idle hands —
  those are rule 1's, and a hunt drawn from them competed with the assignment for the band's one
  order) whose leaving keeps the projected net at the goal with the herd's take counted, and whose
  projection survives.
- **upgrade the ground** (`food:upgrade:<band>`) — `goals.ground_rung > wild`, the rung's gate
  knowledge known, and a worked forage patch below it with nothing queued (`build_destination_rung`
  is *"empty when no band has queued it"*, plus the band's own `build_queue`; not
  `build_queue_position`, which is a source-addressed readout of the *winning* band and defaults
  to `0` off the wire). Knowledge is `snapshot.intensification_knowledge[faction].knowledges[id]
  .progress >= KNOWLEDGE_COMPLETE` (1.0) — the row is *"0..1 (1.0 = known)"*, there is no `known`
  flag on the ladder row (`CraftKnowledgeState` has one; `LadderKnowledgeProgress` does not), and
  `FloraShareInfo::can_cultivate` is the **species ceiling**, not the gate. Tended if the patch is
  not `is_cultivated` and `cultivation` is known; field if the goal is `field`, the patch is
  cultivated, `seed_selection` is known and `sow_site_refusal` is empty. Priced by the ledger:
  `income_gained` = the committed (else largest legal share) plant's `cultivate_payoff` /
  `sow_payoff` minus the row's take today; `income_lost` = the builders' rows; `payoff_turn` =
  `ceil((work_cost − work_done) / (builders × build_work_per_worker_turn))` — there is **no**
  reduced yield during the build (`yield_fraction_while_building` is retired in the ladder JSON:
  *"the gatherers on a source take exactly what their hands carry whatever is being built beside
  them"*). Builders = the smallest crew from 1 up to the budget whose projection survives and whose
  payoff is inside `food.projection_horizon_turns`, drawn from the idle hands, then the hunt rows,
  then the lowest forage rows — never the patch's own row, which keeps the declaration attached.
  Commands: `cultivate`/`sow`, **then the row reductions, then** `assign_labor … builders <n>` —
  in that order, because `assign_labor` clamps a role to the band's idle hands at dispatch
  (`" (clamped from {} — only {} idle)"`, `core_sim/src/bin/server.rs`): builders named before the
  hands are freed would be clamped to zero.

### The projection ledger (`specialists/food/ledger.rs`)

A pure function of numbers, tested alone. `Book { stock, income, consumption }` is a band's food
book off the frame (`stores[FOOD_CARGO_KEY]` with the fixed-point divided out as the scoreboard
does, `food_income`, `food_consumption`); `Reassignment { income_lost, income_gained, payoff_turn }`
is a change; `project_all(book, changes, horizon)` walks `stock_t+1 = stock_t + income − Σ lost +
Σ gained(t ≥ its payoff) − consumption` for `food.projection_horizon_turns` and answers
`Projection { stock, trough: (min, turn), positive_again: first turn net ≥ 0, net_after,
runway_at_end: stock_end / consumption (NOT_FOOD_LIMITED_TURNS when the band eats nothing) }`.
`survives` is §4's rule verbatim: `trough > 0`. `goal_progress(goals, before, after)` is the goal
gap closed — `(goal − value).max(0) / goal` on the runway and on the net income, each normalised to
its goal, averaged over `GOAL_TERMS` (2) — so `1.0` closes both whole gaps, `0` is no change, and a
change away from the goals reads negative. A gap is not capped at 1: a band eating more than it
earns has a runway *below zero* at the horizon, and closing that is more than a goal's worth. Once
both goals are met every change reads `0`, which is why the rules break ties on net income added.

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
   `food.dead_row_turns` consecutive turns is **dead**: the row *negative income* empties first,
   and avoided while remembered.
   ⛔ **The web's mean can weigh a source down but never veto it** — it is consulted only while it
   is *positive*. A non-positive prior says nothing and the source falls back to its own forecast.
   Without that guard a single `0.0` folded into `realized_by_kind["hunt"]` rated **every** hunt
   source 0.0 forever; `best_source` filters on `expected() > 0`, so all of them dropped out and
   `Food` proposed nothing at all from tick 4 to tick 30 of bench seed 23.
3. **A patch row is not a gathering site.** `forage_patches` is published for every food-bearing
   tile, but `assign_labor forage` is refused *"nobody gathers here"* unless the tile carries a
   food module (`plant_rung_site_refusal` → `FoodSiteRegistry::is_site`), so a forage source must
   also be in `snapshot.food_modules`.

### `Land`

Owns `patches_owned`; alarms `land_short` when what the band's crew would **harvest per turn** on
the ground it stands on (`harvest_here` = `crew_take(working_age, own rate, biomass ×
provisions_per_biomass)`) is below its `food_consumption`, and no better patch is in view.

- *blind* — fewer than `land.known_tiles_floor` known tiles within `land.horizon_tiles` of a band
  posts `land.scout_workers` scouts with `assign_labor … scout <n>`, once. `land:scout:<band>`.
  ⛔ **The `scout <x> <y>` verb is retired server-side** (`command.retired=ignored`,
  `core_sim/src/bin/server.rs`); the standing scout role posts vantage points around the band.
- *better ground* — while the runway is falling, a discovered, unowned, **unoccupied**, **workable**
  patch within the horizon whose **per-worker yield** out-pays the band's own by
  `land.better_ground_gain_fraction` of its own (`(target − own) / target`), and which is **not the
  tile the band most recently left** (`SeatMemory::left_from`), proposes `move_band` with
  `Memo::Move { target, from: here }`, and the intent persists until arrival: the memory holds the
  target and re-proposes the same `land:move:<band>` each turn, which is what the commitment bonus
  rewards.

⛔ **Two guards on *better ground*, because the margin alone does not stop the oscillation.** On
bench seed 11 the band walked 20,8 → 18,8 (t12) → 20,8 (t17) → 18,8 (t19), and every arrival
dropped its rows (t19: 16 of 17 idle again). The rate `Land` ranks on is `patch_per_worker_yield`
— the band's **realized** rate where it has worked, else the frame's forecast — so the tile under
the band was read on what it had just stripped (20,8 fell 1.80 → 0.11 a turn under 17 hands) while
the tile it had left was read on a forecast, or on the last realized figure before it was left, that
the stripping had not yet reached. Each tile therefore always out-paid the other by a hair. The
margin refuses a move that buys nearly nothing — distinctness is not improvement, the runway
shuffle's lesson — and the departure memory refuses the one move the margin cannot judge: back onto
the tile whose reading is a forecast rather than the rate the band is realizing now. Neither guard
alone closed it; a tile can out-pay by the whole margin on a forecast the band's own arrival will
disprove.
- *room* — under `Expand`, a band above `land.split_size` standing on ground the faction owns
  proposes `split_band` with half its workers. `land:split:<band>`.

⛔ **`Land` ranks ground by what it pays a worker, not by what is standing on it.**
`carrying_capacity` is the land's biomass `K`; `per_worker_yield` is what the crew eats, and the two
disagree badly — on bench seed 23 the band's own tile carried the neighbourhood's most biomass
(195.0) at its **worst** rate (0.249/worker), beside a 150.0 tile paying 0.531 and a 70.0 tile
paying 0.548. Ranked on capacity, *better ground* correctly found nothing better and the band
starved where it stood: 17 → 5 workers, 19 hunger deaths, with `Land` silent for all 30 turns. So
`better_patch` filters **and** maximises on `patch_per_worker_yield` — the same accessor `Food`
rates a source with (`specialists/food.rs`), so the two specialists cannot drift apart — and
`carrying_capacity` survives only as the tiebreak between equal rates.

⛔ **And on ground `Food` will actually work.** `Food::reachable_sources` filters patches on
`is_food_site`, because `assign_labor … forage` is refused *"nobody gathers here"* off a food module
(`plant_rung_site_refusal`, `core_sim/src/bin/server.rs`). `Land` shares that predicate
(`workable_patch_at`, `specialists/food.rs`) on all four of its paths — `better_patch`,
`own_per_worker_yield`, `harvest_here` and `alarm`. Rate-eligibility is one accessor pair, not two:
without the site half, *better ground* walked the band onto a high-rate non-site patch that `Food`
then excluded on arrival, and a non-site patch counted as "something better in view" and suppressed
`land_short` from the other direction. The alarm above is the same
correction: it once compared a *stock* to a *rate* (`195.0 < 4.09`) and so could never fire.

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
since the last plan; per band the move target still being walked to **and the intent it was
accepted under** (`land:move:<band>` or `food:settle:<band>` — `move_intent`, what the observation's
`intent_in_force` reads; cleared on arrival) and last turn's `turns_of_food`; per worked row
(`<band>:<kind>:<x>,<y>` or `<band>:<kind>:<fauna_id>`) what it realized per worker and for how
many consecutive turns, kept when the row is emptied so a dead source is judged on its record.

**What to remember is stated by the proposal, not parsed from its commands.** `Proposal.memo:
Option<Memo>` — `Memo::Move { band, target, from }` (any specialist's `move_band`; `from` is the
tile the band stands on as it is accepted) or `Memo::Split { band, target, workers }` — is what
`record_choices(tick, (intent, memo)…)` reads, so the memory never has to know a verb's shape. A
`Move` also records `left_from[band] = (from, tick)` — the tile the band most recently departed,
which *better ground* never proposes walking back to; decayed by the horizon, cleared by
`forget_after`. Constructed with `SeatMemory::new(memory_horizon_turns,
food.split_settle_turns)`: the settle turns are a fact about the seat, so they are passed once at
construction and not to every `observe`.

**The split bookkeeping.** An accepted `Memo::Split` is `pending_splits[parent] = SplitPending {
tick, target, workers }`. `observe` keeps the own band ids of the last frame (`known_bands`); an
own band **not among them** standing on the tile of a parent with a pending entry is that split's
child, and the entry moves to `born_by_split[child] = SplitBirth { tick, target }`. A pending entry
no child has answered within `split_settle_turns` is a refused split: it is dropped, and
`split_refused[parent]` records the parent's `working_age` in that frame — what the sim refused
was a band of that size, and *split to feed* asks again only once the band is larger. That entry
is **kept across the memory horizon** (a refusal is a fact about the sim, not a sighting) and
cleared by `forget_after`. A birth is dropped when the child stands on its target or the memory
horizon passes. `forget_after` drops pending entries and births stamped later than the tick and
clears `known_bands`. `pending_split(band)` and
`born_by_split(band)` are what *split to feed* / *settle* / *feed while moving* read; the
observation's `born_by_split: Option<TilePos>` is the settle target, which the page shows as
`↳ split, settling to x,y` under the band.

⛔ **A row nobody was useful on is not a measurement.** `per_worker` is an `Option`, and the
denominator is `useful_workers(row)` — `hunt_useful_workers` on a hunt row, `workers` otherwise. A
row whose denominator is zero records **no** per-worker figure and never joins `realized_by_kind`:
`hunt_useful_workers == 0` means the hunt did not happen, not that the herd yields nothing. Folding
it in as a measured `0.0` is what poisoned the whole hunt web above.

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
| `food.runway_gain_fraction` | `Food` | the per-worker gain *negative income* must buy before it empties a merely lowest row |
| `food.projection_horizon_turns` | `Food` (the ledger) | how far ahead a band's stock is projected, and the longest payoff *upgrade the ground* waits for |
| `food.split_search_tiles` | `Food` | how far from a band *split to feed* looks for a site |
| `food.split_band_workers` | `Food` | the crew a split gives the new band |
| `food.split_settle_turns` | `SeatMemory`, `Food` | turns a pending split waits for its child; turns after birth a child is exempt from *feed while moving* |
| `food.near_positive_fraction` | `Food` | how far under the net-income goal *spare hands into hunts* still fires |
| `goals.net_income_per_turn` | `ConstantStance` → `Food` | the net-income target `Food` scores toward (positive: build stock) |
| `goals.runway_turns` | `ConstantStance` → `Food` | the runway target `Food` scores toward — the goal, where `food.runway_floor_turns` is the alarm |
| `goals.ground_rung` (`wild` / `tended` / `field`) | `ConstantStance` → `Food` | the rung *upgrade the ground* climbs toward |
| `land.known_tiles_floor` | `Land` | *blind*'s floor |
| `land.split_size` | `Land` | *room*'s band size |
| `land.horizon_tiles` | `Land` | how far *blind* counts and *better ground* looks |
| `land.scout_workers` | `Land` | how many scouts *blind* posts |
| `land.better_ground_gain_fraction` | `Land` | the per-worker gain, as a share of the target's rate, *better ground* must buy before it moves a band |

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

Three JSON-lines files under `--log-dir`, one record per line, every write flushed so a killed
process leaves the ticks it saw behind. The bench reads the first two and nothing else; the viewer
reads all three.

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
`reason`, `commands`, `commands_text`), `plan` (`tick`, `stance`, `since_tick`, `budgets`,
`priorities`), `alarm` (`tick`, `specialist`, `alarm`), `ready` (`tick`), and `link` (`tick`,
`event: command_reconnect | stream_reopen`). Only `decision`, `ready` and `link` are written by the
two shipped brains; `plan` and `alarm` are the orchestrator's (`plan_ai_driver.md` §3).

`commands_text` is the proposal's commands, one line each, in the **text-command grammar**:
`assign_labor 1 7001 forage 3 4 5`, `move_band 1 7001 4 9`, `split_band 1 7001 4`. The printer is
`sim_runtime::render_command_line`, beside the parser — **the one renderer both sides of the wire
share**: this log and the server's run record (`core_sim/src/record.rs`) write the same line for
the same payload, and a unit test there parses every rendered verb back through
`parse_command_line`. It renders the verbs a player process emits (`assign_labor`, `move_band`,
`split_band`, `order … ready`); any other verb falls back to its `Debug` form, readable and
deliberately unparsable. `commands` (the count) is unchanged, so slice 3's measures are not moved.

**`observations.jsonl`** — one `observation` record per acted tick (`plan_ai_driver.md` §8.4),
written at the same point as the `ScoreRow`, **before** `decide`, off the same view and the brain's
lens: `tick`, `faction`, `radius`, `grid` (`width`, `height`, `wrap_horizontal`), `plan`
(`stance`, `since_tick`, `budgets`, `priorities`; `null` for Pass and Scripted), `alarms`
(`specialist`, `alarm`, `since_tick` — the ones pending since that plan), `ledger` (`stock`,
`income`, `consumption`, `runway_turns`, `working_age`, `idle_workers`), `bands` (own resident
bands: `band_id`, `x`, `y`, `size`, `working_age`, `idle_workers`, `turns_of_food`, `food_income`,
`food_consumption`, `work_range`, `hunt_reach`, `is_traveling`, `assignments` [`job`, `target:
{x,y} | {herd_id} | null`, `workers`, `actual_yield`, `sustainable_yield`, `hunt_useful_workers`,
and the readout the client's Forage/Hunt sheets show — `workers_needed`, `wasted_yield`,
`overdraws`, `kit_id` (null on a band-wide role), `floor`, `species` (the commit crop, null for
the tile's pick), `take_species` (empty = the whole basket), `improvement` (the declared build
verb, null when none)], `build_queue` [`job`, `target`] in the band's order, `intent_in_force` —
the intent the memory holds a move target under, `land:move:<band>` or `food:settle:<band>` —
`move_target`, and `born_by_split` — the site a child band was split toward, while it has not
reached it), and `neighborhood`:
every **discovered** tile within `radius` hex steps of any own band, sorted `(y, x)`, with
`terrain` (the `tiles` row's variant name, `null` when the frame carries no row), `food_site`
(`food::is_food_site`), `forage_biomass`, `carrying_capacity`, `per_worker_yield` (the frame's
forecast), `rated_per_worker_yield` (`food::patch_per_worker_yield` for the nearest own band — the
number `Food` and `Land` actually rank on), `owner`, `cultivated`, `field`,
`cultivation_progress`, `field_progress`, `build` (the climb declared on the source —
`destination_rung`, `queue_position`, `turns_remaining`, `blocked_reason`, `kit_id`; null when no
rung is named), `upkeep` (`demand`, `supplied`, `shortfall`, `workers_needed`, `kit_id`; null when
the source demands nothing), `herd` (`id`, `species`, `biomass`, `per_worker_yield`, `huntable`,
`corralled`, `corral_progress`, and its own `build` / `upkeep`; the first herd on the tile),
`last_seen_tick` (`SeatMemory::last_seen`, undecayed) and `nearest_own_band_distance`. The
`ledger` also counts the seat's improved ground over the **whole frame** — `patches_owned`,
`patches_cultivated`, `patches_field` — because an owned patch may sit outside the radius. A tile
the seat has never discovered is **absent**, not null — the specialists filter on `is_discovered`
before reading anything, and so does the record. Every one of these is a field the frame carries;
the record derives nothing the client would have to (`labor-ui.md` → "THE ⚠ HAS ONE PRODUCER").

`radius` is `observation_radius(horizon)` = `max(OBSERVATION_RADIUS_FLOOR (3), land.horizon_tiles)`
— wide enough for a band's `work_range` and for everything `Land` looks at.

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
`map_seed` and `default_ai_faction_count` = the seat count pinned, and the four port keys rewritten
to a probed free base from 46000 up — the same four-key rewrite as `core_sim::apply_port_base`,
restated; **nothing else moves**, the separation is the shipped `faction_start_min_separation`),
`ports.json`, `server.log`, `saves/`, and one `seat_<f>/` per seat holding its three instruments
and `sim_ai.log`. The server is started with `SIM_CONFIG_PATH` / `SIM_PORTS_FILE` / `SIM_SAVE_DIR`
set and `SIM_PORT_BASE` removed, exactly as `core_sim/tests/query_seat_gate.rs` does. Seats are this
same executable, spawned with `--turns n --log-dir <out>/<seed>/seat_<f>`. The server is killed on
drop, panic or early return included.

**The world is one a player can select** (`plan_ai_driver.md` §8.4): the `earthlike` preset at the
New Game menu's smallest size, **Tiny = 56×36** (`MAP_WIDTH` / `MAP_HEIGHT`, restated from
`clients/godot_thin_client/src/scripts/MapSizes.gd`, which is the authority), start profile
`late_forager_tribe`, the shipped separation, seed pinned per run. Before `new_game` the harness
asks the server `FactionCapacity { width, height }` on the same unseated connection
(`UnseatedConnection::ask`) and fails the run with `WorldTooSmall` naming both numbers if
`max_ai_faction_count` is below the seats requested — the alternative is a clamped roster and a
rival waiting forever on `unknown_seat`. Then `new_game` is sent and synchronised by a `ListSaves`
question behind it. A 30-turn seed on Tiny is ~3–3.6 s wall (both shipped seat sets, debug build).

**The New Game recipe** — to open the world a bench seed played, from the client menu: preset
*Earthlike*, size *Tiny*, seed = the bench seed (`11` or `23` for the shipped baselines), start
profile *late_forager_tribe*, rivals = the number of `--seats` (2 for both shipped sets). The
human holds seat 0 — the seat the bench only *holds* and never plays — and the rivals are seats 1
and 2 in `--seats` order; the AI played seat 1 in both shipped sets (`1=utility:forager` or
`1=pass`), seat 2 was Pass. The world is the same; what differs is that the menu's game has the
human at seat 0 where the bench auto-submitted it.

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

**A specialist that is never once accepted is a violation, not a silence.** For the roster the seat
spec implies (`utility` → `DISABLEABLE_SPECIALISTS` minus its `~` ablations, `scripted` →
`scripted`, `pass` → none), `specialist.<name>.accepted` must reach `MIN_ACCEPTED` (1); an
**absent** key — the shape a specialist that never proposed leaves behind — reads
`NO_DECISIONS_ACCEPTED` and fails the same way. Held against the run alone, not the baseline.
Without it an inert specialist read as a passing check: `Land` proposed **nothing** across 30 turns
of bench seed 23 and `--check` was green.

⛔ **"Ignored" and "correctly declined" are different states, and only the file can tell them
apart.** Seed 23 is now the second kind. Once `better_patch` gained the food-site filter it shares
with `Food::reachable_sources`, no unowned gathering site within `land.horizon_tiles` out-pays the
ground the band already stands on — so `Land` raises `land_short` for all 30 turns and has nothing
legal to propose, and its measures go absent for the *right* reason. No measure carries that
judgement, so the file carries it: a `declined: [{seed, seat, specialist, note}]` list beside
`degenerate`, keyed on all three so an exemption covers exactly one cell. The note is the point —
it must say why the seat is inert, or the next reader "restores" the measures by relaxing the very
filter that stopped the band walking onto ground `assign_labor … forage` refuses.

It self-retires both ways it can rot: a declared specialist that starts winning again is a
`declined_marker` violation (withdraw the line), and a declaration with a blank note is a
`declined_marker` violation **and** exempts nothing, so a note-less row can never suppress the gate.
`a_declined_specialist_is_exempt_on_its_own_row_only` pins the narrowness — the sibling specialist
on the same row and the same specialist on another seed both still fail.

⚠ Unlike `degenerate`, which `as_baselines` **derives** from `population_working`, this list is a
hand-written judgement no regeneration can recompute — so `upsert` carries it across
`--write-baselines` explicitly. Without that, regenerating would delete the exemption and the next
`--check` would go red citing a violation the same command had just erased the explanation for.

⛔ **The gate is "never wins", not `liveness`.** §8.2's bar is *"a specialist whose proposals never
win is not being measured by the ablation, it is being ignored"* — and losing a single window to a
higher-scoring sibling is ordinary arbitration, not being ignored. On seed 11 `Land` wins at ticks 9
and 23 and loses five conflicts to `Food` in between (one band takes one order a turn), so its
`liveness` is 0 while it is plainly alive. Gating on `liveness` would fail that seat and the only
ways to clear it — rescoring `Land`, or splitting a band between specialists — are changing the
brain to move a number. So `liveness` stays **reported** and ratchetable through `tolerance`, and
never gates on its own.

**`sim_ai/bench/baselines.json`** holds two entries on seeds `11, 23` for 30 turns
(`BASELINE_SEEDS` / `BASELINE_TURNS` / `BASELINE_SEAT_SETS`; a unit test holds the file to them),
recorded on the Tiny `earthlike` world above: the all-Pass control `1=pass 2=pass` — a Pass seat
assigns nobody, so it starves: 22 hunger deaths and 2 working left by turn 30 on both seeds — and
the utility forager `1=utility:forager 2=pass` (seed 11: 6 working, 17 hunger deaths; seed 23: 3
working, 21 hunger deaths). `Land` wins on both seeds on this world, so the file carries no
`declined` entry. Regenerate an entry in the PR that moves it, with the numbers in the PR body.

## The run viewer (`sim_ai viewer`, `viewer/`)

`sim_ai viewer <run-dir> [--seed <s>] [--seat <f>] --out <page.html>` — `<run-dir>` is a bench
`--out` (the defaults are the lowest seed directory under it and the lowest `seat_<f>` under that)
**or a launcher run directory** (below). It reads the seat's `scoreboard.jsonl`, `decisions.jsonl`
and `observations.jsonl` and joins them **by tick** into one `RunModel { seed, faction, seat_dir,
turns, specialists }`: a `Turn` per tick any log names, carrying `score` (the `ScoreRow`),
`observation`, `decisions` (every proposal weighed that tick), `plan` (the one adopted **on** that
tick), `alarms` (raised that tick), `ready`, `link_events`, and the resolved `plan_in_force` /
`alarms_in_force`; `specialists` is every specialist the decision log names, sorted — the page's
tab set. A part no log wrote is `null` — a missing `observations.jsonl` is an empty one, and the
page is still written; the two measured logs are required. A link event with no tick lands on the
first turn.

⛔ **The plan in force is resolved by the writer, not read off the observation.** The observation is
captured *before* `decide`, so on the first acted tick `observation.plan` is `null` while the `plan`
record for that very tick exists — the orchestrator adopted it during that `decide`. Read naively,
turn 1 said "no orchestrator". `resolve_in_force` walks the turns ascending: the plan in force is
this tick's `plan` record, else the observation's plan, else what was in force on the previous
turn; the alarms follow the same rule (raised this tick, else the observation's pending set, else
the previous turn's). `None` only when the whole run adopted no plan, which is what the page shows
as "no orchestrator".

⛔ **The page is one file that fetches nothing.** The template (`viewer/page.html`, `include_str!`)
holds all CSS and JS inline and takes the model by string replacement of one marker
(`__RUN_MODEL_JSON__`) inside `<script id="run-model" type="application/json">`; the JSON has
`<`, `>` and `&` escaped to `<` … so a reason string holding `</script>` cannot end the
element early. `assert_self_contained` refuses to write a page containing `http://`, `https://` or
`src=` (`EXTERNAL_RESOURCE_MARKERS`) — it is published where every external host is blocked and
must open from a file with no network — and `core_sim/tests/ai_bench.rs` asserts the same on a
real run's page. The SVG is built as markup inside `<svg>` elements rather than through
`createElementNS`, so the page names no namespace URL either.

**The page** (phone-first: one column under 700 px; two from 700 px — map, ledger, work and
scoreboard down the left, orchestrator and decisions down the right; three from 1200 px — map |
ledger, work, scoreboard | orchestrator, decisions; `main` stops widening at `--page-max-width`
(1700 px) and centres, panels `align-items: start`. ⛔ **The map is capped, not scaled to the
column**: `#hexmap` is `width: auto; max-width: 100%; max-height: var(--map-max-height)` —
`min(60vh, 560px)` — centred, `preserveAspectRatio="xMidYMid meet"`, so a wider window shows
more panels rather than a bigger map that pushes them below the fold. The title's seat path is an
ellipsised `.path` span with the full path in its `title`, so it never forces horizontal scroll.
Light/dark by `prefers-color-scheme`; the system font stack): a **turn scrubber** (range input, ◀ ▶ buttons, ← → keys, the current tick)
over four inline-SVG **sparklines** of the run — stock, income vs consumption, runway (the 999
sentinel drawn as a gap), hunger deaths per tick — with the current tick marked; the **local map**,
the observation's neighbourhood as odd-r hexes unwrapped around the first own band, outlined by
`owner` (own / rival / none), with glyphs for cultivated (□) and field (≡), a herd disc sized by
biomass, band markers labelled `b<band_id> ·<size>` with a dashed ring at `work_range`, and a
legend that says never-seen tiles are not drawn, and **every hex a band works this tick** (a tile
target directly, a herd target through the herd's hex) outlined in `--worked` with a badge of the
workers on it — tapping a hex lists its record and then that tick's worked rows on it, each as the
client's readout states it (job, band, crew, useful workers on a hunt, "N would do" when
overstaffed, actual of sustainable per turn, ⚠ overdraws, uncollected yield, kit, floor, take,
commit, declared build), plus the tile's progress meters, build and upkeep; the **ledger** (the
six numbers, then a row per band with its worked rows in that same form and the intent in force);
the **Work** panel under it — per band, workers by every job kind its rows name (so a new role
appears with no template change) and idle of working-age, then the seat's improved ground
(owned / cultivated / fields off the ledger), every band's build queue joined to the source's
declared climb, and every upkeep row in view (owned patches, herds); the **seat scoreboard** under
that — every `ScoreRow` field for the tick, captioned as the seat's ratchet
numbers and not a tile score; the **orchestrator** panel — the plan in force (stance, since tick,
a row per specialist with budget share, priority and its goals — `net +1.0/turn · runway 12t ·
toward field` for `Food`, `—` for a specialist with none), the alarms in force, and this tick's
re-plan (with its goals) / alarm / link events; and
**decisions** as **tabs**, `All` plus one per entry of `specialists`, each showing that
specialist's accepted proposals then the rejected ones grouped by `rejected_by` (specialist,
intent, reason, raw → final score, `commands_text`), "no proposals" when it has none, and the
tick's `ready` line.

⛔ **Only a food site takes the biomass ramp.** `forage_biomass` is published for every
food-bearing tile, but `assign_labor … forage` is refused off a food module (the same fact that
shapes `Food::reachable_sources`), so painting every tile by biomass made land the band cannot
gather from read as rich as a site. A tile with `food_site` is filled on the `--ramp0 → --ramp1`
ramp (the maximum is taken over food sites only) and carries a small filled dot at its centre
(`--site`); land with a patch row but no site is one neutral `--land`; a tile with no patch row
(water, bare ground) keeps `--nopatch`. The herd disc is unchanged.

**Per-band focus.** Tapping a band's row in the ledger or its marker on the map focuses it: the
marker and its work-range ring take `--focus`, and every decisions tab is filtered to proposals
that name the band — the intent's subject token, or the band token of a command line (the third
token for `assign_labor` / `move_band` / `split_band`; any token for a verb the page does not know).
An "all bands" control clears it. Tab and focus are page state in memory, never in the URL.

## A played game becomes a viewable run (`core_sim/src/record.rs`, `sim_ai import-record`)

The instruments above are a `sim_ai` process's own; the human's client writes none. So the
**server** records what every seat was sent and what every seat said, and `import-record` turns
that into the same three logs — the page is then the same for the AI's seat and the human's, on
the same world.

**The record** is on while `SIM_RECORD_DIR` names a directory (`core_sim::record::RECORD_DIR_ENV`;
the launcher sets it, below). Under it:

```text
<record>/run.json                          RunInfo: map_preset_id, width, height, map_seed (as built —
                                           a requested 0 is resolved by then), start_profile_id,
                                           roster, world_epoch; rewritten at every world build
<record>/commands.jsonl                    CommandRecord per line: tick, faction (the seat the sending
                                           connection held; null unseated), connection (the opaque
                                           id, never the token), verb, command
<record>/seat_<f>/frames/<world_epoch>/<frame_seq>.bin
                                           every frame published to seat f, the FlatBuffers envelope
                                           exactly as the socket writes it, without its u32 length,
                                           under the world that published it
```

⛔ **A frame is filed under its world, and an import reads exactly one world.** One `SIM_RECORD_DIR`
covers a whole launcher session, but `SeatPublishState.frame_seq` is fresh per world (a rebuild is a
brand-new `App`) and is dropped when the seat is released — and a session rebuilds routinely, since
`load_game` bumps the epoch exactly as `new_game` does and `new_game` is re-armed after a theme
change. Flat filenames therefore had world 2's `1.bin, 2.bin, …` overwrite world 1's file for file,
and `import-record` replayed the splice as one run without a word, because both chains base off the
same origin. The epoch directory keeps them apart; `frame_files` takes the **latest** epoch present —
the world the per-build `run.json` describes — and `warn!`s on stderr naming the earlier ones it
passed over. A rival's disconnect/reclaim mid-world stays within its own epoch and chains as before.

`run.json` is written from `retain_claimed_seats` — the `seats.roster` moment. A command line is
written from `dispatch_connection_command`, **beside `log_dispatched_command` and under its
exclusion policy** (`is_replayable`): the record holds the timeline and never a query, a claim, a
save, a rollback or a resync. The line is `render_command_line` on the wire payload, rendered by
the reader thread (`WireLine`, the third element of `CommandDelivery`) only while a recorder is
open; the server's own senders carry none and are not recorded. The tick is `SimulationTick` at
dispatch: the seat's `order … ready` for turn T is stamped T, and so are the orders it sent that
turn.

⛔ **Frames are recorded at three sites, because the publisher is not the only sender.** The
publisher's sink is wrapped (`RecordingSink`: the socket first, then the record); and the two
frames the command loop delivers itself — a resync's full frame (`handle_resync`) and a rollback's
(`handle_rollback`) — go through `deliver_frame`, which records too. A rival's chain **starts** on
a resync frame (it claims, then asks), and that frame is minted on its own fresh `frame_seq` that
every following delta bases on; a record without it would apply nothing.

⛔ **Nothing on the turn path waits for the disk.** `RunRecorder` owns one `run-recorder` thread
behind an unbounded channel; the publisher thread and the command loop enqueue and return. A write
that fails is a `warn!` (`record.frame.failed` / `record.command.failed` / `record.run.failed`)
and the job is dropped. The writer names a frame by decoding only its header
(`sim_runtime::decode_frame_header`), never the world.

**`sim_ai import-record <record-dir> --seat <f> --out <log-dir>`** reads
`seat_<f>/frames/<latest world_epoch>/*.bin` in `frame_seq` order through the same
`decode_frame_flatbuffer` + `apply_delta` chain a live seat uses (a delta before a full frame, or
off a broken chain, is dropped with a warning and the replay resumes at the next full frame), and for each tick writes the `ScoreRow` and `Observation` a
`sim_ai` process would have written off that view with **no brain lens** — `plan` and `alarms`
null, radius `OBSERVATION_RADIUS_FLOOR`. Every `commands.jsonl` line of that seat becomes one
accepted `Decision`: specialist `human`, intent `human:<verb>`, both scores 1.0, `commands_text`
the recorded line; an `order` line becomes the tick's `ReadyRecord` and no decision. The record's
layout constants and the two record types are restated in `import_record.rs` (`core_sim` is the
authority; this crate cannot link it).

⛔ **A tick's state is its LAST frame.** A mid-tick recapture carries the same tick as the turn
frame before it, so a tick can have several frames; the importer captures a tick's row the moment a
frame of another tick arrives — after the last of them — and once more at the end. That is the
world with the tick's own orders applied, so the human's ledger at tick T shows the rows T's
commands set. A live `sim_ai` observes the **first** frame of a tick, before its own commands: the
two pages differ by exactly the seat's own orders on that tick.

**The viewer accepts a launcher run directory.** `<data_dir>/runs/<run_id>` (`launcher.md`) holds
`seat_<f>/` for every rival the launcher spawned with `--log-dir`, and `record/`. `locate_seat`
treats a directory holding `record/` or `seat_<f>/` directly as the seed directory itself: a seat
with logs is read; a seat the record has frames for and no logs — the human's — is imported into
`seat_<f>/` on the fly; with no `--seat` the lowest seat, logged or recorded, is the default; the
page is labelled `<run_id> (seed <map_seed>)` off `run.json`.

**The recipe — play, quit, then paste.** Launch the packaged game, play some turns against a
rival, quit. The launcher's log (stderr) holds `run directory: <path>` and, under "open this run
with:", **one complete command per seat** — the human's at start, each rival's the moment the
supervisor started it, and all of them again at exit (`launcher.md` → the run directory):

```text
<sim_ai> viewer <run dir> --seat 0 --out <run dir>/seat_0.html    # your seat, imported from the record
<sim_ai> viewer <run dir> --seat 1 --out <run dir>/seat_1.html    # the rival's, from its own logs
```

`<sim_ai>` is the packaged binary the launcher itself spawned rivals with and `<run dir>` is
absolute, so a line pastes as printed. `--seat 0` is the human (`HUMAN_FACTION_ID`); the rivals are
the roster's other ids. The record of the last `KEPT_RUNS` sessions is kept.

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

## The scenario, the bench and the record tests (`core_sim/tests/ai_seat_scenario.rs`, `ai_bench.rs`, `ai_record_import.rs`)

All three drive the built `server` and the built `sim_ai` over the real sockets and share
`core_sim/tests/common/ai_process.rs`: `Scratch`, the kill-on-drop `Process`, `server_binary`,
`sim_ai_binary` (the sibling, or the private fallback build into `target/ai_process_fallback`),
`strip_ansi`, `log_tail`. The scenario and the record test also share
`core_sim/tests/common/seat_harness.rs`: the small world (24×16 `earthlike`, seed 11, one rival,
separation shrunk to 6), `start_server(case, port_base, record_dir)`, `build_world` (a `new_game`
on an unseated connection, synchronised by a `ListSaves` behind it), `run_scripted_sim_ai`, and
`Link` — claim · greet · resync · full frame, the shipped client's own handshake. Each test holds
its own port block (45300, 45400) so the two servers cannot collide.

`ai_record_import.rs` starts the server with `SIM_RECORD_DIR` set, seats the scripted `sim_ai` for
3 turns **without** `--log-dir` (standing in for the human), and asserts: `run.json` carries the
seed and a two-seat roster; every `commands.jsonl` line carries exactly `tick, faction,
connection, verb, command` (no token); the seat's lines are one `split_band` and three `order`s;
`import-record --seat 1` then `viewer <run dir> --seat 1` writes a page whose `specialists` is
`["human"]`, with `AI_TURNS + 1` turns (the tick the seat claimed at plus one per turn played —
it exits on seeing the last, which the record still holds), a score row and a lens-less
observation on each, one `human:split_band` decision on the first whose line is the grammar's, a
`ready` on the three acted ticks and none on the last. About 3 s.

`ai_bench.rs` runs the built bench on seed 11: all-Pass twice over `TURNS` (6), asserting every
measure identical and every `--compare` delta zero or null; then `1=scripted 2=pass` with the
scenario's split script, asserting `specialist.scripted.accepted > 0` and that at least one
scoreboard measure of seat 1 differs from the same seat's under Pass — "acts instead of passing" as
a number; then `1=utility:forager 2=pass`, asserting `specialist.food.accepted > 0`, that the
**seat** is live (below), `commands_failed_total` 0, and no `command.rejected` /
`command.split.rejected` line in the server log — a proposal the server refuses is a bug in the
specialist's command construction, and this is where it shows. Then `sim_ai viewer` on that run:
the page exists, inlines exactly `UTILITY_TURNS` turns each carrying its observation, score row
and `ready`, and contains none of the external-resource markers.

⛔ **The utility leg runs `UTILITY_TURNS`, not `TURNS`, and the test proves its own span first.**
Liveness cuts windows of `LIVENESS_WINDOW_TURNS` (10, restated here from `measures.rs`), so over a
6-tick span there is exactly **one** window and "won in every window" is arithmetically the same
claim as `accepted > 0` — which the line above it already makes. `UTILITY_TURNS` is
`LIVENESS_WINDOW_TURNS + 2`, and the test asserts `link.turns_observed > LIVENESS_WINDOW_TURNS`
*before* it asserts liveness, so the assertion cannot silently decay back into a restatement.
Seconds, not minutes; the 30-turn baselines are not generated by a test.

⛔ **The liveness the test gates on is the seat's, read off `decisions.jsonl`, not
`specialist.food.liveness`.** A one-band seat takes one order a turn, so a window in which `Land`
wins the band twice is a window `Food` lost to ordinary arbitration — on Tiny seed 11 that is the
run's two-tick second window, and `specialist.food.liveness` reads 0 while the seat is plainly
playing. Gating on it would fail a healthy seat, and the only ways to clear it change the brain
(the same argument the ratchet's "never wins" gate rests on, above). So the test cuts the same
windows over the scoreboard's tick span and requires an accepted decision from **some** specialist
in each.

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
