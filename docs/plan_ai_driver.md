# The AI driver — the shape a player process is built in, and how each part is measured

**Built on `docs/plan_ai_opponents.md`, which decided *what* fills a seat and is not restated here.**
This document is the buildable shape of that decision: the layers inside a `sim_ai` process, the
contracts between them, the instruments that measure each layer on its own, and the procedure for
making the AI better without making it a different program. Issue #645 builds the first version.

The one-sentence version: **an orchestrator turns personality into a plan; specialists turn the plan
into scored proposals; an arbiter turns proposals into this turn's commands; and every one of those
hand-offs is a typed value that can be logged, replayed against a fixture, and compared across runs.**

> **"Orchestrator" means the AI's top layer and nothing else.** The server's turn-waiting logic is
> the *turn scheduler* (`SeatTurnGate` in code); the two design docs that once called it the
> orchestrator now say so.

---

## 1. The shape

```
                    frames (this seat's)                          commands
   sim server ─────────────────────────▶  Link  ─────────────────────────────▶ sim server
                                            │ ▲
                                    SeatView│ │ Vec<CommandPayload> + `order <f> ready`
                                            ▼ │
                              ┌─────────────────────────────┐
                              │  Orchestrator ── Plan ──┐   │   every N turns, or on an alarm
                              │                         ▼   │
                              │  Specialist × K ── Proposals│   every turn, each in its own domain
                              │                         │   │
                              │  Arbiter ◀─────────────┘   │   every turn: gate, budget, commit,
                              │     │  Decision records     │   select, resolve conflicts
                              └─────┼───────────────────────┘
                                    ▼
                          scoreboard.jsonl · decisions.jsonl        (the instruments, §8)
```

| Layer | Type | Reads | Produces | Runs |
|---|---|---|---|---|
| **Link** (§7) | struct | sockets | `SeatView` in, commands out | always |
| **Perception** (§2) | `SeatView` + `SeatMemory` | frames | the one view of the world | every frame |
| **Orchestrator** (§3) | `trait Orchestrator` | `SeatView`, `AiProfile`, alarms | `Plan` | every `goal_cadence` turns, or on an alarm |
| **Specialists** (§4) | `trait Specialist`, one per domain | `SeatView`, its slice of the `Plan`, own memory | `Vec<Proposal>`, optional `Alarm` | every turn |
| **Arbiter** (§5) | struct | proposals, `Plan`, `Difficulty`, last turn's choices | commands + `Decision` records | every turn |

The `Brain` trait from `plan_ai_opponents.md` §2 is still the outer plug — `decide(&SeatView, rng)
-> Vec<CommandPayload>` — and it is what an external program in another language would implement
against. Inside this crate, the three shipped brains are configurations of one composite:

| Brain | Orchestrator | Specialists | Arbiter |
|---|---|---|---|
| `PassBrain` | none | none | emits only `ready` |
| `ScriptedBrain` | none | one `Scripted` specialist replaying a command list at infinite score | pass-through |
| `UtilityBrain` | `ConstantStance` in v1 | `Food`, `Land` in v1 | the real one |

So `PassBrain` is *the control*, `ScriptedBrain` is *the fixture*, and both are the layered shape with
parts removed rather than separate code paths. That is what keeps the smoke test, the scenario test
and the real opponent on one code path.

---

## 2. Perception — the frame is the view, and there is nothing to decode it yet

**`SeatView` is the decoded `WorldSnapshot` for this seat, kept current by applying each `WorldDelta`
as it arrives.** It is the same struct the server captured and redacted for this seat
(`sim_schema/src/world.rs:137`); the AI holds no second copy of "what can I see". Fog is a property of
the bytes: a tile the seat has not seen is not in them (`plan_multiplayer_seats.md` §4.3).

### ⛔ The gap this issue has to close first

Nothing in the workspace turns a FlatBuffers frame back into a `WorldSnapshot`. `sim_schema` encodes
(`codec/mod.rs:53` `encode_snapshot_flatbuffer`, `:60` `encode_delta_flatbuffer`, ~4,700 lines across
the `codec/` modules, 130 tables) and has no decoder. The only decoder in the repo is the Godot native
extension, and it decodes into Godot dictionaries (`native/src/bridge/decoder.rs:201`,
`snapshot/delta.rs` `DeltaAggregator`). Tests that read a frame today reach into the generated
accessors by hand (`core_sim/tests/seat_frames.rs:137` `decode_frame`).

**Decision: the decoder and the delta merge go in `sim_schema`, beside the encoder, as
`decode_snapshot_flatbuffer` / `decode_delta_flatbuffer` and `WorldSnapshot::apply_delta`.** Not in
`sim_ai`, for three reasons:

- **It is round-trip testable there and nowhere else.** `encode(decode(bytes)) == bytes` and
  `hash_snapshot(decode(encode(s))) == hash_snapshot(s)` are the definition of done, section by
  section, against the *shipped* encoder. A partial decoder in the AI crate would fail silently — a
  section left undecoded reads as "empty", which is exactly what an unseen section also reads as.
- **`sim_schema` is the contract crate.** A schema change that breaks decoding fails the contract's
  own tests, not an AI test three crates away.
- **Any Rust seat occupant needs it** — the bench harness (§8), a replay viewer, a map inspector — and
  the AI is only the first.

The dependency line becomes `sim_ai → sim_runtime → sim_schema → shadow_scale_flatbuffers`; still no
`core_sim`, still no Bevy. Two constants the Link needs live only in `core_sim` today — the 8-byte
token width and the 2 s greeting timeout (`core_sim/src/network.rs:74,84`) — and are restated in
`sim_ai` with a pointer comment, exactly as `SnapshotStream.gd` restates them.

### `SeatMemory` — what the frame no longer says

A frame says what is visible *now*. Three things a brain needs are only knowable across frames, and
they live in `SeatMemory`, a struct that is a pure function of the frames received:

| Remembered | Why |
|---|---|
| last-seen turn per tile, and the tile as it was | `plan_ai_opponents.md` §6's *view horizon* lever decays this; without it, "hard" and "easy" see the same world |
| last turn's chosen intents (§5) | the commitment term needs to know what it has been doing |
| alarms raised and when (§3) | so an orchestrator can measure its own response latency |

A `Resync` or a rollback delivers a full frame; `SeatView` is replaced and `SeatMemory` drops every
entry stamped later than the new tick. Nothing is patched — a memory that survives a rewind is a
plan for a world that no longer exists.

---

## 3. The orchestrator — personality becomes a plan

**Input:** `SeatView`, `AiProfile`, the alarms raised since it last ran, its own memory.
**Output:** a `Plan`. It never emits a command and never sees a proposal.

```rust
pub struct Plan {
    pub stance: Stance,                        // discrete: Expand | Consolidate | Seek (v1 set)
    pub budgets: BTreeMap<SpecialistId, Budget>, // the scarce shared units each specialist may spend
    pub priorities: BTreeMap<SpecialistId, f32>, // a multiplier on that specialist's scores
    pub since_turn: u64,                       // when this plan was adopted — hysteresis is measurable
}
```

**A stance is what the archetype resolves to** (`raider → Expand`), and the budgets are what a stance
means in units the specialists spend: under `Expand` the `Land` specialist holds more of the worker
budget and the `Food` specialist less. The profile's `weights` shape that split. Personality enters
the AI here and only here — a specialist reads *its* budget and priority, never the profile.

**When it runs.** Every `goal_cadence` turns (a `Difficulty` lever, §6) **or when a specialist raises
an alarm**. The alarm channel is the bottom-up half of the contract: a specialist that sees its domain
failing (`Food`: turns of food below a floor) says so, and the orchestrator answers by re-planning
early. Without that channel the plan is blind to the world between cadences and the only fix is a
shorter cadence, which is the difficulty lever being spent on robustness.

**Commitment lives here as a switch margin.** A new stance is adopted only when its score exceeds the
current stance's by `profile.commitment`. Below the margin the plan is renewed unchanged, which is the
"keep leaning toward what it has been doing" `plan_ai_opponents.md` §4 requires.

**v1 is `ConstantStance`**, as `plan_ai_opponents.md` §3 recommends: the stance is the archetype's,
the budgets are the profile's weights normalised, and an alarm is answered by moving budget toward the
alarming specialist for one cadence. That is enough to prove the plan channel and the alarm channel
are both live. `UtilityOrchestrator` (scores stances against the view) and `LlmOrchestrator`
(`plan_ai_opponents.md` §5 — tens of tokens, every N turns, falls back to `ConstantStance` on timeout)
both produce the same `Plan`, so the specialists, the arbiter and the instruments never change when
the orchestrator does.

---

## 4. Specialists — one domain each, proposing and never sending

```rust
pub trait Specialist {
    fn id(&self) -> SpecialistId;
    fn propose(&mut self, view: &SeatView, plan: &Plan, memory: &SeatMemory) -> Proposals;
}

pub struct Proposal {
    pub commands: Vec<CommandPayload>, // the wire actions, already faction-tagged
    pub intent: IntentKey,             // "what this is for", stable across turns — the commitment key
    pub score: f32,                    // this specialist's utility, before priority and commitment
    pub cost: Cost,                    // scarce units it spends: workers, the bands it moves
    pub reason: &'static str,          // the consideration that produced it — the decision log's why
}

pub struct Proposals { pub proposals: Vec<Proposal>, pub alarm: Option<Alarm> }
```

**A specialist is a pure function of the view, its plan slice and its memory.** That sentence is the
whole test strategy: hand it a recorded frame and assert on the proposals, with no server, no socket
and no other specialist in the room. It never sends — the arbiter does — so a specialist cannot
overspend, cannot move a band another specialist is moving, and cannot bypass a behaviour gate.

**Each specialist owns one scoreboard metric and one alarm.** `Food` owns *turns of food* and alarms
when it drops below the profile's floor; `Land` owns *tiles held* and alarms when a band's patch is
below its need. Ownership is what makes §8's per-specialist measurement honest: the specialist is
judged on the number it exists to move.

### The roster

| Specialist | Domain | Commands it proposes | v1 |
|---|---|---|---|
| `Food` | the food loop | `AssignLabor`, `ForageTile`, `HuntFauna`, `HuntGame`, `Cultivate`, `Sow`, `WorkPriority` | **yes** |
| `Land` | where the people are | `MoveBand`, `ScoutArea`, `SplitBand`, `FoundSettlement`, `FollowHerd` | **yes** |
| `Herd` | animals | `Tame`, `Corral`, `ExtendPen`, `SetHerdOutput` | later |
| `Build` | improvements and their upkeep | `BuildOrder`, `BuildKit`, `UpkeepMode`, `UpkeepKit`, `Abandon`, `Unqueue` | later |
| `Craft` | the bench | `SetBench`, `BenchCrew`, `BenchPriority` | later |
| `Contact` | other people | `SendExpedition`, `SendTradeExpedition`, `SendDenialRaid` | later (#231, #369) |
| `Scripted` | the test fixture | whatever the script says, at infinite score | **yes** |

`Food` and `Land` are the food/land loop the issue names. The v1 considerations, so the first
version is concrete rather than a trait with no body:

- **`Food`.** *Idle hands* — `idle_workers > 0` proposes `AssignLabor` to the best-yield source in
  reach. *Runway* — `turns_of_food` below the floor raises the alarm and proposes moving workers from
  the lowest-yield job to the highest. *Overuse* — a source whose `actualYield` exceeds its
  `sustainableYield` proposes shifting workers off it (the intensification arc's row-level signal,
  read straight off the frame).
- **`Land`.** *Blind* — few known tiles around a band proposes `ScoutArea`. *Better ground* — a seen
  patch with higher carrying capacity than the band's own, and a falling runway, proposes `MoveBand`
  with an intent that persists until arrival. *Room* — a band above the split size on a claimed patch
  proposes `SplitBand` under `Expand`.

### Budgets and costs share one vocabulary

The shared scarce things a seat spends are **workers** (working-age population, assigned per job) and
**bands** (a band takes one movement order per turn). `Budget` and `Cost` are both denominated in
those units, so "may this specialist afford this proposal" is arithmetic the arbiter does, not a
judgement a specialist makes. Materials and work-points join the vocabulary when `Build` does.

---

## 5. The arbiter — proposals become the turn

Runs once per turn, in this order, and every step is a reason a proposal can be rejected — which is
what the decision log records:

1. **Behaviour gate.** A proposal whose intent class the profile's `behaviors` forbid
   (`will_raid: false` → any `SendDenialRaid`) is rejected `behavior_gated`. This is where the
   booleans from `plan_ai_opponents.md` §4 act.
2. **Priority.** `score *= plan.priorities[specialist]`.
3. **Commitment.** A proposal whose `intent` matches one chosen last turn gets
   `score *= 1 + profile.commitment`. Same number as the orchestrator's switch margin, second use:
   the stance holds *across cadences*, the intent holds *across turns*, and a player reads both as
   one people that means what it does.
4. **Selection under difficulty.** Argmax at the top; sample among the top-*k* below it
   (`Difficulty.selection_top_k`). The AI evaluates correctly at every difficulty and only its
   follow-through varies (`plan_ai_opponents.md` §6).
5. **Feasibility.** Walk the ordered list, charging each `cost` against the specialist's `Budget`
   and against the turn's conflict set — one order per band, workers not assigned twice. Reject
   `over_budget` or `conflict` and continue down the list.
6. **Emit.** The accepted proposals' commands, then `Orders { directive: Ready }`. Always — an
   arbiter that emits nothing still ends the turn, because a silent seat is auto-submitted at
   `seat_turn_timeout_seconds` (120 s) and *loses its turn* (`factions.md` → Seats).

Every proposal, accepted or not, becomes a `Decision { turn, specialist, intent, score_raw,
score_final, outcome, reason }` on the decision log (§8). The rng is seeded from
`(map_seed, faction, turn)`, so the same world and the same profile produce the same commands — the
bench (§8) relies on that, and it costs nothing because the server already pins `map_seed`.

---

## 6. Personality and difficulty land in different places

`ai_profiles.json` is `plan_ai_opponents.md` §4's shape, owned by `sim_ai` (`sim_ai/data/`). Each
field has exactly one consumer:

| Profile field | Consumed by | As |
|---|---|---|
| `archetype` | orchestrator | the stance it starts from (and, later, the stance it scores toward) |
| `behaviors.*` | arbiter step 1 | intent classes that are gated off |
| `weights.*` | orchestrator | the budget split and priorities in the `Plan` |
| `commitment` | orchestrator + arbiter step 3 | stance switch margin; intent bonus |
| per-specialist floors (`food.runway_floor_turns`, …) | that specialist | alarm thresholds |

**Difficulty is not a profile field.** A profile says *who* this people is; difficulty says *how well
it follows through*, and the two vary independently — the same `raider` plays at every level.
`Difficulty { selection_top_k, goal_cadence_turns, memory_horizon_turns }` is passed to the process
beside the profile and consumed by the arbiter, the orchestrator's cadence and `SeatMemory`'s decay
respectively. No lever anywhere grants material; a seat has nowhere to receive it.

Two shipped profiles that visibly differ are the issue's bar, and "visibly" is a number in §8.

---

## 7. The Link and the process

The Link is the `bridge/command_link.rs` shape in Rust, with no Godot in it. The brain never sees a
socket.

- **Claim, then greet, then hold.** Connect to the command port, write `ClaimSeat { request_id,
  faction_id }` as the first frame, read the `SeatClaimReply`, open the stream port and write the
  token as its first 8 little-endian bytes. **One command connection for the life of the process** —
  the seat belongs to the connection (`factions.md` → Seats), and the issue's own history says what
  a socket-per-command does.
- **Reconnect re-claims and re-greets.** A dropped command link is rebuilt with the retry constants
  the client uses (`SEAT_CLAIM_ATTEMPTS = 8`, `SEAT_CLAIM_RETRY_BACKOFF = 250 ms`); a re-claim mints a
  new token, so the stream socket is closed and reopened with it. A stale token is a socket that is
  silently sent nothing.
- **Host verbs are never sent.** `Turn` and `Rollback` are refused from a seated connection, and the
  AI has no unseated connection. It submits orders; it does not resolve turns.
- **A decide has a time budget.** `decide()` runs under `decide_budget_ms`, well inside the 120 s
  seat timeout; on expiry the Link submits `ready` with whatever the arbiter had accepted. A brain
  that blocks — an LLM orchestrator waiting on a network — loses flavour, not the turn.
- **The token is a secret.** Held in a type with a redacted `Debug` and no `Display`, mirroring
  `core_sim::SeatToken`, so it cannot reach the log by accident.

**Arguments.** `--ports-file` (the launcher's `SIM_PORTS_FILE`, read the way `ServerPortsFile.gd`
reads it), `--faction`, `--brain pass|scripted|utility`, `--profile <id>`, `--difficulty <id>`,
`--seed`, `--log-dir` (where the instruments write), and for the bench `--turns N` (exit after N
resolved turns; a released seat becomes vacant and is auto-submitted, so an exited AI never stalls
the others). Pass ports explicitly when several processes share a machine — the handshake file is one
path per machine, and a stray process can resolve to another session's server.

### The launcher

`local_seats()` gains the rival seats, each a `LocalSeat` carrying the faction it fills and the
program `sim_ai`; `fill_seat` passes the faction through. The unit test that pins "exactly one local
seat" (`launcher/src/main.rs:803`) changes to pin the roster shape instead.

**The decision #646 left here: the human's client owns the session window.** `wait_for_players`
becomes *wait for the human's seat*; when that process exits, `Session`'s `Drop` reaps the AI
processes and then the server, in that order. An AI process exiting early does not end the run — its
seat goes vacant and the game continues, which is also the correct behaviour for a crashed rival. The
rule goes into `.claude/rules/core_sim/launcher.md` when the code does.

---

## 8. Measurement — three instruments, one harness, a number per layer

The question "is the AI better" has no answer until it is "better *at what*, measured *how*, against
*which control*". This section fixes all three so that every future change to the AI lands with a
number rather than an impression.

### 8.1 The instruments

**Scoreboard** — `scoreboard.jsonl`, one row per turn per seat, read from the `SeatView` the process
already holds. No new server code: everything below is in the frame this seat is served.

| Metric | Read from |
|---|---|
| population (children / working / elders) | `demographics[faction]` |
| food stock | Σ `cohort.stores["provisions"]` |
| net food income | Σ `food_income` − Σ `food_consumption` |
| sustainable vs actual income | Σ `LaborAssignment.sustainableYield` / `actualYield` |
| runway | min `turns_of_food` across bands |
| idle workers | Σ `idle_workers` |
| land | count of `ForagePatch` with `owner == faction`; of those, `isCultivated || isField` |
| herds | Σ `biomass` in view; count `corralled` |
| knowledge | `intensification_knowledge[faction]`, `craftKnowledge[faction]` |
| deaths, by cause | `CommandEvent` where `kind == died && faction == seat`, `cause=` token |
| victory progress | `victory.modes[faction][*].progress` |

**Decision log** — `decisions.jsonl`, one row per proposal per turn (§5), plus one row per `Plan`
adopted and per `Alarm` raised. This is the instrument that measures a *part* rather than the whole.

**Bench** — `sim_ai bench`: start a server on a private port block with `map_seed` and
`default_ai_faction_count` pinned (the config keys a headless run is documented to set), seat one
`sim_ai` process per rival with the requested brain/profile/difficulty/seed, and let it run. **No host
is needed** — once every occupied seat has submitted, the server resolves the turn itself
(`SeatTurnGate` → `TurnWait::Resolve`) — so a bench is exactly N AI processes and a server, and the
run ends when they reach `--turns`. It collects the two logs per seat and writes a report. The
subprocess shape is `core_sim/tests/query_seat_gate.rs` (`start_server`, `write_test_config`,
`await_ports_file`); the bench lives in `integration_tests/` or under `xtask`, because
`CARGO_BIN_EXE_server` is visible only to `core_sim`'s own tests.

### 8.2 What each layer is measured on

| Layer | Measure | Control / bar |
|---|---|---|
| **Whole seat** | scoreboard at turn *T*, primary = population, guard = zero hunger deaths | `PassBrain` on the same seed. The issue's bar "acts instead of passing" is *any* primary-metric difference from Pass at T; the later bar is a positive one |
| **A specialist** | *ablation*: full brain minus this specialist vs full, on the same seeds | the delta on the metric it owns. If disabling it changes nothing it is dead, and the liveness check below says so before the ablation reads as "no effect" |
| | *liveness*: accepted proposals per K turns > 0 | a specialist whose proposals never win is not being measured by the ablation, it is being ignored |
| | *acceptance rate* and *rejection mix* (`behavior_gated` / `over_budget` / `conflict` / `outscored`) | a specialist mostly rejected `over_budget` is under-funded by the plan, not wrong |
| | *intent churn*: distinct intents chosen per K turns | the anti-oscillation check at the specialist level |
| **The orchestrator** | *stance churn*: switches per 100 turns | the commitment check; an upper bound is pinned |
| | *alarm latency*: turns from `Alarm` to a budget change | the bottom-up channel is live |
| | *ablation*: uniform budgets vs the orchestrator's | the plan is worth having |
| | *profile divergence*: two profiles, same seed — distance between their intent histograms and their scoreboards | the *Vox Deorum* prize (`plan_ai_opponents.md` §5): recognisably different peoples. This is the number "two shipped profiles that visibly differ" resolves to |
| **Difficulty** | primary metric ordered easy ≤ normal ≤ hard across seeds | the levers do what §6 says, monotonically |
| **The Link** | turns lost to timeout; reconnects; frames received vs turns | zero, zero, and one-to-one — a robustness floor, not a quality measure |

Every measure above is computed from the two logs by the bench report. None needs the server to know
the AI exists.

### 8.3 The ratchet

`sim_ai/bench/baselines.json` holds the measures for a pinned set of seeds and a pinned *T*. A bench
run compares against it and fails on a drop beyond tolerance. The AI is deterministic given
`(map_seed, faction, turn)`, so the comparison is exact on a replay of the same configuration and a
change in the numbers is a change in the AI, never noise. The baseline is updated in the PR that
moves it, with the number in the PR body — the same discipline the terrain-preview PNG baselines
follow.

Three tiers, by cost:

| Tier | What | Cost | Runs |
|---|---|---|---|
| unit | a specialist over a recorded fixture frame | ms | every `cargo test` |
| scenario | `ScriptedBrain` against a real server, asserting an *effect* (a band moved), the `live_seat_probe` shape | seconds | every `cargo test`, in `integration_tests/` |
| bench | N seeds × T turns × the comparisons in §8.2 | minutes | on demand and before a baseline change |

Fixture frames for the unit tier are recorded from a real run through the server's `FrameSink` seam
(`snapshot/publish.rs:66`, whose doc names a file writer as an intended implementor) — a specialist is
tested on the shipped representation, not on a hand-built struct.

---

## 9. Making it better without making it different

Each of these is a procedure with a done-bar, and none of them touches the server.

- **Add a specialist.** Implement `Specialist`; declare the metric it owns and its alarm; add its
  costs to the shared vocabulary if it spends something new; record a fixture and write the unit
  tests; add it to the ablation set; run the bench; land it with its ablation delta in the PR body.
- **Add a consideration.** A new `reason` inside an existing specialist, usually reading a new
  profile weight. Done when the unit test shows the proposal, and the bench shows profile divergence
  did not shrink — a consideration every profile weighs identically is character-neutral.
- **Change the orchestrator.** A new `Orchestrator` impl producing the same `Plan`. Done when stance
  churn stays under its bound, alarm latency does not grow, and the ablation against uniform budgets
  is still positive. The LLM orchestrator lands through this door and nothing else changes.
- **Tune difficulty.** Only the three levers in `Difficulty`. Done when monotonicity holds.
- **Widen personality.** A new key in `ai_profiles.json`, read by one consumer from the §6 table.
  Done when two profiles that differ only in that key diverge on the bench.

---

## 10. Invariants

- ⛔ **The frame is the only perception.** No query to the server that the client cannot also make;
  no second visibility model; no decoding shortcut that reads sections the seat was not sent.
- ⛔ **Specialists propose; only the arbiter emits.** A `CommandPayload` reaches the Link from
  nowhere else, and every one of them has a `Decision` row behind it.
- ⛔ **Commitment is in the first version** — both the switch margin and the intent bonus.
- ⛔ **No `core_sim` in `sim_ai`'s dependency graph.** The crate boundary is the honesty guarantee.
- ⛔ **The seat token is never logged**, and the decision log carries the faction id, never the token.
- ⛔ **A turn is always submitted.** The decide budget is a fraction of the seat timeout, and expiry
  submits what is ready.
- ⛔ **Personality is a file, difficulty is a lever, and neither grants material.**

---

## 11. Slices, in the order they unblock each other

| # | Slice | Home | Done when |
|---|---|---|---|
| 1 | FlatBuffers → `WorldSnapshot` decoder and `apply_delta` | `sim_schema` | round-trip and hash tests pass for every section; the existing hand-written `decode_frame` in `seat_frames.rs` is replaced by it |
| 2 | `sim_ai` crate: Link, `SeatView`, `Brain`, `PassBrain`, `ScriptedBrain`; launcher fills rival seats and adopts the exit rule; scenario test | `sim_ai`, `launcher`, `integration_tests` | a rival seat claimed by `sim_ai` moves a band from a script and the game is won or lost with it seated |
| 3 | Instruments and bench: scoreboard, decision log, `bench`, baselines | `sim_ai`, `integration_tests` | `PassBrain` vs `PassBrain` reads as zero on every measure; `ScriptedBrain` vs `PassBrain` reads as non-zero |
| 4 | `UtilityBrain`: `ConstantStance`, `Food`, `Land`, the arbiter with commitment; `ai_profiles.json` with two profiles; delete `StartProfileOverrides::ai_profile_overrides` | `sim_ai`, `core_sim` | lands with its Pass delta, its two ablations, and its profile-divergence number in the PR body |

Slice 1 is the one the issue did not anticipate — "there is nothing to build in `core_sim`" is still
true, but there is something to build in `sim_schema`, and it is the largest of the four. Slice 3
comes before 4 deliberately: the real brain lands measured, on a harness already proven to read zero
where zero is correct.

---

## 12. Crate layout

```
sim_ai/
  Cargo.toml            depends on sim_runtime (and through it sim_schema); never core_sim
  data/ai_profiles.json the profiles — archetype / behaviors / weights / commitment / floors
  bench/baselines.json  the ratchet
  src/
    main.rs             args, wiring, the turn loop
    link.rs             claim · greet · hold · reconnect · resync
    view.rs             SeatView (decoded frame + deltas) and SeatMemory
    brain.rs            Brain trait; PassBrain; ScriptedBrain; the composite
    orchestrator/       trait; constant.rs (v1); later utility.rs, llm.rs
    specialists/        trait, Proposal, Cost; food.rs, land.rs; later herd, build, craft, contact
    arbiter.rs          the six steps, Decision records
    profile.rs          AiProfile, Difficulty, the json schema
    instruments/        scoreboard.rs, decisions.rs
    bench.rs            the harness driver and report
```

The engineering rationale for what gets built — the as-built notes, the config key tables, the
mechanism behind each guard — goes into a new `.claude/rules/core_sim/ai-driver.md` with `paths:`
covering `sim_ai/`, when the code exists. This document is the target; that file will be the record.

---

## See also

- `docs/plan_ai_opponents.md` — the decisions: brain patterns, personality model, the LLM path,
  difficulty as decision quality
- `docs/plan_multiplayer_seats.md` — the seat model the process plugs into
- `.claude/rules/core_sim/factions.md` → Seats — the as-built claim / gate / token contract
- `.claude/rules/client/command-transport.md` — the client's seated link, the Link's reference
- `core_sim/tests/query_seat_gate.rs` — the subprocess harness shape the bench copies
