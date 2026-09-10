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

## 2. Perception — the frame is the view, and the contract crate reads it back

**`SeatView` is the decoded `WorldSnapshot` for this seat, kept current by applying each `WorldDelta`
as it arrives.** It is the same struct the server captured and redacted for this seat
(`sim_schema/src/world.rs:137`); the AI holds no second copy of "what can I see". Fog is a property of
the bytes: a tile the seat has not seen is not in them (`plan_multiplayer_seats.md` §4.3).

### The decoder lives beside the encoder, and is tested against it

Until #645, nothing in the workspace turned a FlatBuffers frame back into a `WorldSnapshot`:
`sim_schema` encoded (`codec/mod.rs` `encode_snapshot_flatbuffer` / `encode_delta_flatbuffer`, ~4,700
lines across the `codec/` modules, 130 tables) and the only decoder was the Godot native extension,
into Godot dictionaries. Tests that read a frame reached into the generated accessors by hand.

**The decoder and the delta merge are in `sim_schema`, beside the encoder** —
`decode_frame_flatbuffer` / `decode_snapshot_flatbuffer` / `decode_delta_flatbuffer`, one
`decode_<section>` per `serialize_<section>` in the same file, and `WorldSnapshot::apply_delta`
(`sim_schema/src/apply_delta.rs`). Not in `sim_ai`, for three reasons:

- **It is round-trip testable there and nowhere else.** `encode(decode(bytes)) == bytes` and
  `hash_snapshot(decode(encode(s))) == hash_snapshot(s)` hold on a saturated fixture
  (`sim_schema::fixture::saturated_snapshot`, every section and vector non-empty) against the
  *shipped* encoder. A partial decoder in the AI crate would fail silently — a section left undecoded
  reads as "empty", which is exactly what an unseen section also reads as. Every state struct is
  rebuilt as an exhaustive literal, so a field appended to the schema fails to compile until it is
  decoded.
- **`sim_schema` is the contract crate.** A schema change that breaks decoding fails the contract's
  own tests, not an AI test three crates away.
- **Any Rust seat occupant needs it** — the bench harness (§8), a replay viewer, a map inspector — and
  the AI is only the first.

`apply_delta` inverts the producer one diff shape at a time — keyed upsert then `removed_*`, whole
`Option` replace with `Some(empty)` clearing, the append-only event feed deduplicated on `seq` and
trimmed by tick window — and refuses a delta whose `base_frame_seq` or `world_epoch` does not match
the view it holds, because a silently skipped delta loses event history. It is proven against the
shipped publication path (`core_sim/tests/apply_delta_producer.rs`: two seats, 14 turns, a mid-tick
recapture applied twice), not against a hand-written model of it.

The dependency line is `sim_ai → sim_runtime → sim_schema → shadow_scale_flatbuffers`; no `core_sim`,
no Bevy. Two constants the Link needs live only in `core_sim` — the 8-byte token width and the 2 s
greeting timeout (`core_sim/src/network.rs`) — and are restated in `sim_ai` with a pointer comment,
exactly as `SnapshotStream.gd` restates them.

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
    pub goals: BTreeMap<SpecialistId, Goals>,    // the targets each specialist scores progress toward
    pub since_turn: u64,                       // when this plan was adopted — hysteresis is measurable
}
```

**Goals, not only weights.** A weight says how much a specialist matters; a goal says what it is
for. The orchestrator hands `Food` targets in the units the frame reports — food per turn, total
stock, minimum runway in turns — and `Food` scores a proposal by the progress it makes toward them.
Personality shapes the targets (a raider tolerates a shorter runway than a forager), so a profile
still enters here and only here, but a specialist can now explain a choice as *"this closes the
runway gap by four turns"* rather than as a number times a weight. Goals are also what let a
specialist accept an **investment**: a reassignment that dips income negative is fine when the stock
lasts until the payoff (§4, the projection ledger), and that judgement needs the target to judge
against.

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
    pub reason: String,                // the consideration that produced it, naming its target — the decision log's why
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
| `Land` | where the people are | `MoveBand`, `AssignLabor … scout`, `SplitBand`, `FoundSettlement`, `FollowHerd` | **yes** |
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
- **`Land`.** *Blind* — few known tiles around a band proposes a scout assignment (`assign_labor … scout`; the `scout <x> <y>` verb is retired server-side). *Better ground* — a seen
  patch with higher carrying capacity than the band's own, and a falling runway, proposes `MoveBand`
  with an intent that persists until arrival. *Room* — a band above the split size on a claimed patch
  proposes `SplitBand` under `Expand`.

Those three are the placeholders the first version shipped with. They prove the seam; they are not
the specialist. The specialist is the rule set below.

### Rules read the surroundings; recipes do not

⛔ **A specialist is a set of rules over what the frame shows, never an opening.** "Always split the
start band into two bands of five" works on the maps where it was learned and fails elsewhere; a rule
that says *when* to split reads the tiles and decides. The orchestrator helps by supplying the goals
(§3); the specialist supplies the reading of the ground.

The `Food` rule set — each a named rule with its own unit test on a recorded frame, each producing a
proposal whose `reason` names the rule and its subject:

| Rule | Reads | Proposes |
|---|---|---|
| **Negative income** | income below consumption | reassign to the tiles with the highest food per worker-turn, preferring the balanced take policy |
| **Feed while moving** | a band with a movement intent in force | forage or hunt what will fall *outside* the new range on the way — not for a freshly split band, which must not strip the parent's ground |
| **Split to feed** | after assignment the start band is still short; reachable food within three tiles that a smaller band could work | split a band toward it, since small bands are easier to feed; the split rule lives here because feeding is its reason |
| **Spare hands into hunts** | income positive or near it | put the surplus into hunts, which is what opens penning |
| **Upgrade the ground** | a worked forage site, the cultivation rung known | cultivate, then sow, drawing workers from hunts and poor tiles; a field and a tended patch feed a population in the tens, after which food stops being the constraint and herding becomes the work |

**The projection ledger** is what makes the last rule safe to fire. Learning cultivation costs
income now for income later; a per-turn score cannot see that trade. So `Food` keeps a small what-if
ledger: given the current stock, income and consumption from the frame, project the stock turn by
turn under a proposed reassignment, using the per-source yields per worker the labor rows carry. A
proposal that takes income negative is acceptable **iff the projected stock stays above zero until
the projected payoff**, and the proposal's `reason` carries the turn it goes positive again. That is
the forward-projection discipline the food-arrivals arc already uses on the server side, applied to
the seat's own view. The ledger is also how the specialist answers the goals it was handed: the gap
between projected runway and the target is the score.

### The demand board — specialists never talk to each other

A crafter needs bone; a builder needs stone; a herder needs fodder. Those needs are met by other
specialists, and the moment specialists call one another the design is a web whose edges nobody can
test in isolation. **So a need is a `Demand` posted on a board, and the board is read by the
orchestrator alone.**

```rust
pub struct Demand {
    pub requester: SpecialistId,
    pub resource: ResourceKey,   // bone, stone, fodder, food, workers … one vocabulary with Cost
    pub amount: u32,
    pub by_tick: u64,
    pub priority: f32,           // the requester's view; the orchestrator's is what counts
}
```

The lifecycle is `posted → planned → fulfilled | expired`, and every transition is a log record. The
orchestrator weighs the open demands against its goals and personality, and writes the ones it
accepts into the **supplier's** plan slice as goals — so `Hunt`'s goals this cadence may include
"three bone by tick 40 for the crafter". The supplier never knows who asked; it has a target. The
requester never knows who supplies; it has a board. Conflicts between demands are decided in one
place, with the same commitment margin a stance has, instead of in pairwise negotiations.

Three consequences: specialists stay pure functions of `(view, plan slice, own memory)` and remain
unit-testable alone; the web of relationships is a star with the orchestrator at its centre; and the
board is measurable — fulfilment rate and latency per requester/supplier pair, and the demands a
personality lets expire. ⛔ **No specialist that needs another lands before the board does.**

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

**The rival roster is not known at boot.** The server decides it at every world build — boot,
`new_game` from the client's menu (which picks the rival count), a load — and `retain_claimed_seats`
drops the claims a rebuild orphans. So the launcher cannot fill rival seats from a static list; it
**supervises** them. The server emits one structured event, `seats.roster` (the registered faction
ids and the `world_epoch`), at each world build, on the log stream it already publishes
(`core_sim/src/log_stream.rs`, JSON lines on the `log` port). The launcher reads that port on a
supervisor thread and reconciles on every event: one `sim_ai --faction <id>` child per rival faction
not yet running, adopted into the process group before anything else; a child whose faction left the
roster is reaped. The human's client is filled first, exactly as today, and is told nothing new. An
AI child that exits on its own leaves its seat vacant — auto-submitted — until the next roster event
respawns it.

**The decision #646 left here: the human's client owns the session window.** `wait_for_players`
becomes *wait for the human's seat*; when that process exits, `Session`'s `Drop` reaps the AI
processes and then the server, in that order. An AI process exiting early does not end the run — its
seat goes vacant and the game continues, which is also the correct behaviour for a crashed rival.

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

### 8.4 The run viewer — look before tuning

A number says *that* a run went badly; it cannot say whether the ground fed five people or the
specialist assigned them to fodder. **So a bench run also records what each specialist was looking
at**, per turn and per seat: each band's tile and size; the tiles within its reach with their food
yield, owner and improvement; the food ledger (stock, income, consumption, runway, and the projection
the ledger made); every proposal with its score, outcome and reason; the plan and goals in force; the
alarms and demands open. Written as data beside the two logs, and shown as a page with a turn
scrubber and a small local hex map, so a run can be read turn by turn on a phone.

⛔ **Nobody touches a weight, a rule or a profile until the viewer has been looked at.** The failure
this exists to prevent is the one the first bench produced: a starving band read as "the AI is weak"
before anyone had seen the map. The viewer is the difference between measuring an AI and guessing at
one.

The bench runs on the **shipped map presets and sizes a player can select**, never a fixture world
chosen for speed. A world nobody plays measures nothing a player will meet.

---

## 9. Making it better without making it different

Each of these is a procedure with a done-bar, and none of them touches the server.

- **Add a specialist.** Implement `Specialist`; declare the metric it owns and its alarm; add its
  costs to the shared vocabulary if it spends something new; record a fixture and write the unit
  tests; add it to the ablation set; run the bench; land it with its ablation delta in the PR body.
- **Add a rule.** A named rule inside an existing specialist, reading the frame and its goals, with
  a unit test on a recorded frame that shows the proposal and its `reason`. Done when the viewer
  shows it firing where it should and the bench shows profile divergence did not shrink — a rule
  every profile weighs identically is character-neutral.
- **Add a demand.** A new `ResourceKey`, a requester that posts it, a supplier whose goals can carry
  it. Done when the board's log shows the lifecycle end to end and the fulfilment measure reads.
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
- ⛔ **Specialists never name each other.** A need is a `Demand` on the board; the orchestrator turns
  it into a supplier's goal. A specialist is a rule set over the frame, never an opening.
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
| 1 | FlatBuffers → `WorldSnapshot` decoder and `apply_delta` | `sim_schema` | round-trip and hash tests pass for every section; `seat_frames.rs` reads frames through it; the merge matches the producer over real turns |
| 2 | `sim_ai` crate: Link, `SeatView`, `Brain`, `PassBrain`, `ScriptedBrain`; launcher fills rival seats and adopts the exit rule; scenario test | `sim_ai`, `launcher`, `integration_tests` | a rival seat claimed by `sim_ai` moves a band from a script and the game is won or lost with it seated |
| 3 | Instruments and bench: scoreboard, decision log, `bench`, baselines | `sim_ai`, `integration_tests` | `PassBrain` vs `PassBrain` reads as zero on every measure; `ScriptedBrain` vs `PassBrain` reads as non-zero |
| 4 | `UtilityBrain`: `ConstantStance`, `Food`, `Land`, the arbiter with commitment; `ai_profiles.json` with two profiles; delete `StartProfileOverrides::ai_profile_overrides` | `sim_ai`, `core_sim` | lands with its Pass delta, its two ablations, and its profile-divergence number in the PR body |

| 5 | The run viewer (§8.4) and the bench on shipped presets | `sim_ai` | a 30-turn run can be read turn by turn — bands, reachable tiles with yields, ledger, proposals with reasons, plan, alarms; the baselines are regenerated on a shipped preset |
| 6 | Goals in the `Plan`, the projection ledger, the `Food` rule set replacing the v1 considerations | `sim_ai` | every rule has a fixture test and a viewer-visible firing; the forager reaches the cultivation rung on the bench with zero hunger deaths, or the viewer shows why the ground could not carry it |
| 7 | The demand board and its measures | `sim_ai` | one demand round-trips posted → planned → fulfilled in a scenario test; fulfilment rate and latency read on the bench |

Slice 1 is the one the issue did not anticipate — "there is nothing to build in `core_sim`" is still
true, but there was something to build in `sim_schema`, and it is the largest of the four. Slice 3
comes before 4 deliberately: the real brain lands measured, on a harness already proven to read zero
where zero is correct. Slices 5–7 are the order the first bench taught: see first, then give the
specialist goals and rules worth seeing, then let specialists need each other — and no second
specialist that needs a first lands before 7.

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
