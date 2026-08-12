# Improvements and knowledge cost WORK, not turns

**Issue #542**, under arc #184. Blocks #539 (the plant web's build tool).

**Status: DESIGN, model settled 2026-08-11.** §§1–9 are the model; §10 records what was decided and
the two tuning questions left. Nothing here is implemented — §11 is the slicing.

---

## 0. What is wrong today

**Every improvement on both food webs costs exactly the same amount, and the only thing that
differs is how fast you chip at it.** A rung declares a rate (`build.progress_per_turn`) and the
meter it fills always ends at `1.0` (`intensification::RUNG_COMPLETE`). All four shipped rungs
declare **0.04**, so cultivating a patch, sowing a field, taming a herd and fencing a pen are
*literally the same 25-turn job*. The knowledge half is built identically — one
`knowledge.progress_per_turn` of **0.05** against a `completion_threshold` of **1.0**, so **every
lesson on both webs takes the same ~20 turns**, Cultivation and Seed Selection alike.

Four consequences, and they are the reason this is worth reworking rather than retuning:

- **Nothing up the ladder can cost more than what is below it.** A Farm (rung 4, unbuilt) has
  nowhere to be a bigger job than a garden except by declaring a *worse rate*, which reads as "your
  people got slower at this" rather than "this is more work." Every 4X prices later tech above
  earlier tech; this ladder cannot.
- **Every tool has to be a multiplier, and a multiplier cannot be scale-sensitive.**
  `EquipmentStat::BuildRate` (#515) is ×1.5 on a garden and ×1.5 on a farm, because that is what
  multipliers do. The intuition a hoe must satisfy — real on a garden plot, meaningful on a tended
  patch, nearly nothing on a farm — is unreachable while the meter is normalized.
- **Per-species pacing is a rate fudge.** A Steppe Runner's `taming_rate` 0.2 makes the crew *worse
  at their job* for that species. The honest statement is that the animal is five times the work.
- **Gear cost is size-blind.** `equipment.md` records it plainly: "a build costs a fixed amount of
  gear whatever else is true of it" — a rabbit's tame and a mammoth's burn identical hurdles,
  because every build totals `1.0`.

## 1. The model

**Each improvement declares a fixed cost in WORK UNITS. A crew produces work units per turn. Turns
are the output.**

```
work_this_turn = Σ over the crew (per_worker_output) × learn_multiplier(floor)
progress      += work_this_turn                    // absolute units, on the source's own meter
complete when  progress >= effective_cost
effective_cost = rung.work_cost × source_cost_multiplier − Σ tool work contributions
```

Three properties fall out, and each is one of the four defects above closed:

- **A rung can be a bigger job than the rung below it** — one number, per rung, in config.
- **Turns fall as the faction improves** — more hands, better floor discipline, better tools all
  shorten the same fixed job. That is the progression statement the ladder cannot make today.
- **A tool's help shrinks as the job grows**, because the tool's contribution is a fixed number of
  units against a cost that is not (§6).

### 1.1 The unit is a worker-turn at the food peak

`per_worker_output` = **1.0**, and `learn_multiplier(floor)` is unchanged (`floor /
MSY_BIOMASS_FRACTION`, ×1.0 at the food peak). So **a work cost of 50 means "50 worker-turns at the
food peak with no gear"** and every number in the config reads itself. This is deliberately *not* a
new dial — making the worker's output a tunable would give two authorities over the same pacing,
and the cost side is the one the arc exists to expose.

### 1.2 No cap on how many workers may work a source — already decided

`crew_scale` (`min(workers / crew_needed, 1)`) **goes away**. Fifty workers finish a Cultivate in a
turn, and that is allowed. The constraint is opportunity cost across systems, not a rule that
forbids a play style. **Known and accepted:** today only food pushes back; crafting throughput,
defence and trade arrive as those systems land.

`crew_needed` **survives with one job instead of two**: it still floors the source's
`workers_needed` (`intensification::source_crew_needed`, on the wire as
`ForagePatchState.cultivateCrewNeeded` / `sowCrewNeeded`), so committing to a build never asks for
fewer hands than the harvest it replaced. It no longer touches the accrual.
`RungDef::build_crew_scale` and `FULL_CREW_SCALE` are deleted.

> **A consequence on the ANIMAL web, accepted.** Both animal rungs declare `crew_needed: null` and
> are therefore not merely uncapped but **crew-blind today** — a Tame takes 25 turns whether two
> hands or twenty work the herd. Under the work model every build is crew-scaled, so a large keeper
> crew tames materially faster. This follows directly from the decision above and is not something
> the neutral calibration can preserve: **the animal rungs' turn counts will move**, in proportion to
> `herders_needed`. A crew-blind build is exactly what the model removes.

## 2. Two currencies, deliberately — work and practice

The issue asks whether build work and learning work are one currency or two. **Two**, and the
reason is that they are earned by different acts and must scale differently:

| | **work units** | **practice units** |
|---|---|---|
| earned by | a worker-turn on the source | a **turn** the source is worked |
| scales with hands? | **yes** — that is what the arc is for | **no** |
| scaled by the floor? | yes (`learn_multiplier`) | yes (`learn_multiplier`) |
| tools contribute? | yes (§6) | no |
| spent on | a per-source build meter | the faction knowledge ledger |

**Learning must not scale with hands.** Knowledge is faction-level and credited **once per source
per turn** (`systems::labor::credit_rung_lesson`), so a per-worker rate would let a faction learn
ten times faster by piling hands onto one patch — the "no cap" decision without the opportunity-cost
brake that justifies it, since learning costs nothing extra. You learn by *watching the practice*,
not by counting the hands doing it.

Naming them apart (`work_cost` vs `lesson_cost`) is what stops anyone adding them.

## 3. What the improvements cost — pacing-neutral first

**Recommendation: land the model at exactly today's turn counts, then spread the costs in a
separate config-only PR.** The inversion is a large diff across two webs, the schema and the gear
model; riding a retune on top of it means any pacing regression is indistinguishable from an
intended change. The repo's own discipline says the shipped config describes what the sim does
today and later slices change behaviour by editing it.

Pacing-neutral means `work_cost = today's turns × the crew that made "25 turns" true`:

| rung | today | `work_cost` (neutral) | the eventual spread |
|---|---|---|---|
| `plant:tended` (Cultivate) | 25 turns at `crew_needed` 2 | **50** | 50 |
| `plant:field` (Sow) | 25 turns at `crew_needed` 3 | **75** | ~90 — rung 3 is the heavier job |
| `animal:pastoral` (Tame) | 25 turns, crew-blind | **50** | 50 |
| `animal:pen` (Corral) | 25 turns, crew-blind | **75** | ~90 |
| `plant:farm` (rung 4, unbuilt) | — | — | **~300** — born large, which is the point |

The two animal rungs have no crew today, so their neutral cost is a **reference crew** choice, not a
derivation: 2 for `pastoral` (it makes the same claim rung 2 makes on plants — you manage the wild
source in place) and 3 for `pen`. A herd's actual crew is `herders_needed`, so real pacing will
differ from 25 turns in both directions depending on herd size. See the callout in §1.2.

### 3.1 The species multiplier — a cost, not a rate

`taming_rate` inverts to a **per-species cost multiplier on the rung's `work_cost`**, default 1.0:

| species | `taming_rate` today | cost multiplier | Tame at 50 units |
|---|---|---|---|
| rabbit / fowl / crag_goat | 1.0 | **1.0** | 50 |
| boar | 0.8 | **1.25** | 62.5 |
| aurochs | 0.5 | **2.0** | 100 |
| steppe_runner / marsh_grazer | 0.2 | **5.0** | 250 |

Same pacing, honest statement: *the animal costs five times the work*, not *your people are five
times worse at their job*.

**The multiplier scales the DECAY too**, exactly as `timescale` does today — a beast that takes a
lifetime to gentle does not go feral in a season, and the rung's build:decay ratio must stay
invariant under it. Moot on the animal branch (both rungs' `decay_per_turn` is `null`), but the
rule is what keeps a future decaying rung correct.

#### 3.2 Decay of the source's meter, restated

`decay_per_turn` becomes **a fraction of the rung's own cost bled per turn**, keeping today's 0.01
(≈100 turns to fully lapse, whatever the job's size).

**This is about the SOURCE going feral** — a part-cleared patch reverting on the turns nobody works
it — not about tool durability, which is §6.3 and a separate quantum. The alternative (decay in
absolute units per turn, so a bigger investment lapses proportionally slower) is **rejected for
continuity**: the fractional form preserves today's ~100-turn lapse whatever a rung costs, keeps
build:decay a per-rung ratio, and keeps the grace bound of §7 meaningful. One line to reverse if
playtest disagrees.

## 4. What the lessons cost

Each knowledge declares a **`lesson_cost` in practice units**, where one practice unit is one turn
of working a teaching source at the food peak. Today's five lessons are all **20**.

**The ledger stays normalized.** `DiscoveryProgressLedger::add_progress` clamps to `1.0` and is
shared with great discoveries, espionage and the start profiles — widening its unit is a large
blast radius for no gain. So `RungDef::knowledge_accrual` returns
`learn_rate × learn_multiplier(floor) / lesson_cost` and `completion_threshold` stays `1.0` as the
ledger bar. The per-knowledge cost is a **divisor at the seam**, and the wire's
`IntensificationKnowledgeState` 0..1 fields and the client's knowledge meters are untouched.

`knowledge.progress_per_turn` (0.05) is replaced by `knowledge.learn_rate` = **1.0** (one practice
unit per worked turn at the peak) plus the per-knowledge costs. `1.0 / 20` reproduces 0.05 exactly.

**`lesson_per_crafted_item` moves with the currency rather than being left alone.** It is the same
question one quantum over — a craft lesson is charged per *item finished at a bench*. It becomes
`craft_lesson_per_item` in practice units, set to **4.0**, so a craft costing 20 is learned in 5
items exactly as today. Leaving it as a fraction of a normalized threshold while its sibling became
a cost is precisely the drift the slice-4 consolidation existed to prevent.

Eventual spread (a later config edit, same PR as §3's): the rung-2 lessons stay 20, the rung-3
lessons (`seed_selection`, `penning`) rise, `foddering` higher again.

## 5. What a worker is worth — and what knowledge does NOT do yet

`per_worker_output` is **1.0**, flat (§1.1). **Knowledge does not feed it, and the reason is a
design claim rather than a scoping one.**

Knowledge already reaches throughput — *through the tools it unlocks*. Knowing about wheels while
owning no wheels does nothing, and it should do nothing; what raises a crew's output is the crafted
item in their hands, which §6 already prices. A second, invisible knowledge→throughput term would
pay the player twice for the same discovery and would make the tool's contribution unreadable.

Worker output is nonetheless written as a **sum of terms**, so a future buff mechanic has a place to
land without re-inverting anything.

## 6. The gear — an additive contribution, and the arithmetic that decides its shape

`EquipmentStat::BuildRate` is the wrong shape and is replaced. The gear model has **no
"contributes N" representation today**: every effect names the value a stat *takes*
(`equipment.md` → "An effect names the value a stat TAKES"). Adding one is a model change.

### 6.1 The tool's help lands on the JOB, not on the crew's output

There are two places a tool's help can be added, and only one of them gives a hoe that fades on a
farm. **This is the load-bearing arithmetic in the design.**

**On the crew's output** — a hoed worker produces `w + h` per turn instead of `w`. A fully hoed crew
of `n` produces `n(w + h)`, so:

```
turns_bare = cost / (n·w)          turns_hoed = cost / (n·(w + h))
ratio      = w / (w + h)
```

**The cost cancels.** The hoe saves the same *percentage* of turns on a garden, a field and a farm
alike, forever — the multiplier problem in a different costume, which is the thing this arc exists
to escape.

**On the job** — the tool takes units off the cost:

```
effective_cost = cost − t          saving = t / cost
```

Now the job's own size decides. A tool contribution of 8 units is 16% of a 50-unit garden and 2.7%
of a 300-unit farm, and **the hoe never mentions either improvement by name**. This is the shape.

### 6.2 `t` is PER WORKER — the partly-equipped-party rule, already shipped

**The tool is wielded. A worker holding one contributes its worth; a worker without one does not.**

```
t = Σ over the crew ( work units this worker's tool takes off the job )
```

That is not a new rule — it is the **existing** one. `EquipmentCoverage` already resolves every
per-worker effect this way (`equipment.md` → "The partly-equipped party — ten spears arm ten hunters,
and the eleventh goes bare"), and `weighted_rate` is the seam: `Σ share × value(crew's kit)`, times
the head count, is exactly the sum above. Five hoed workers and five bare answer `5 × worth`, the
same shape as five sledded hunters and five sledless hauling `5 × 40 + 5 × 12`.

So the new stat is a **per-worker work contribution**, and it plugs into machinery that already
exists. It also answers the question `equipment.md` left open on `BuildRate` ("whether a digging tool
wants the covered reading instead"): **yes** — the old stat was uncovered only because averaging a
*multiplier* said "bring fewer keepers and the pen goes up faster", a pathology the per-worker
cost-side form does not have.

**The fade on a farm comes from the job's declared cost, which is config's job** (§3): a Farm is born
at ~300 units precisely so the hand tools of the era are noise against it, and that is a dial to tune,
not a property to build into the resolution rule.

Everything else about the stat:

- **Neutral at `0.0`.** Within one kit it resolves as the max of the live declaring items —
  `dispersion`/`exposure`'s shape, which `BuildRate` already uses — so a spent tool steps back to the
  neutral and a second declarer (#539's hoe beside `husbandry_gear`) is legal by construction.
- **Unclamped**: `effective_cost = cost − t`, and nothing floors it. A clamp shipped briefly and was
  **rejected** (2026-08-12): whether a tool can wipe out a job is decided by the job's `work_cost`
  and the tool's contribution, which are both dials, so a structural guard against a config outcome
  is the wrong instrument — and a floor forbids exactly the endgame the design wants, where the
  right tool reduces a late job to a small fraction of itself and bare hands are impractical. There
  is no arithmetic hazard to guard either: `build_fraction` divides by the **raw** stamped cost and
  `build_turns_remaining` by the crew's output, so a bar at or below zero simply completes the build
  on its first worked turn — the allowed no-cap outcome of §1.2.
- **Calibration.** `husbandry_gear`'s flint tier declares ×1.5 today, which on a 50-unit build is
  worth ≈17 units total. Against the reference keeper crew of 2 that is **≈8.5 per worker**, and a
  fully-geared animal build lands where it does today while a half-geared one is honestly slower —
  which it was not before.
- **Evaluated live each turn** off the crew's current kit, so gaining or wearing out a tool mid-build
  takes effect immediately and no state records "this build was geared."

### 6.3 Wear — the free win the arc pays out

`WearQuantum::BuildProgress` is charged over the meter's own increment, which under this model is
**in work units**. So a wear amount per work unit means **a farm eats more hoes than a garden does,
with no per-improvement authoring** — the thing `equipment.md` currently records as impossible.

Calibrate so a **50-unit build costs exactly what a build costs today** (100 durability / 8.0 per
unit of normalized progress = 12.5 builds → **0.16 per work unit**). A 90-unit Sow then eats 1.8×
the gear, for free.

**Charge on the units the meter TOOK, plus the units the tool pre-paid** — the tool did that work
and should wear for it. Keep `charge_build_wear`'s existing before/after delta discipline (a build
the source refuses spends no gear) and `ExtendPen`'s documented exception.

**One readout has to change.** `ItemDefinition::headline_wear` is the item's life gauge — the
sentence that today reads *"≈12.5 builds"* on the handling gear, the same way the sled reads
*"≈5000 biomass hauled"*. It works today only because every build is the same size. Once a Cultivate
is 50 units and a Farm is 300, *"12.5 builds"* is not a statement. **Quote it against a named
reference job** — *"≈12 gardens' worth"*, the reference being rung 2's cost — rather than in bare
work units, which mean nothing to a player holding a hoe.

## 7. What survives unchanged

- **The investment dip** (`yield_fraction_while_building`). It multiplies crew throughput and is
  orthogonal to how the work is counted. One interaction worth naming: with no crew cap, a big
  enough crew finishes in a turn and pays the dip for a turn — consistent with the dip already
  being *"a cost only while hands are the scarce thing."*
- **`learn_multiplier(floor)`** on both the build and the lesson, and its deliberate absence from
  decay.
- **The neglect grace** (`grace_turns`) — a count of unworked turns, not an amount of progress. Its
  validate bound restates as `grace_turns < work_cost / reference_output` (turns to build at
  `crew_needed`, or 1 where null, at the food peak).
- **Completion clears the improvement**, the once-per-source "nothing left to build" test, the
  newest-first plant unwind, the feed lines and the `announce_rung_lost` / `announce_pen_lost` edges.
- **`RungDef::build_accrual` stays THE one seam both webs call.** Its signature changes; its status
  as the single place a rung's build math lives does not.

## 8. The wire and the client

**The meter stores absolute work units; the wire keeps publishing a 0..1 fraction.** The sim
divides at capture, which is the repo's standing discipline (the client does zero arithmetic).

- `ForagePatchState.cultivationProgress` / `fieldProgress`, `HerdTelemetryState.corralProgress`:
  unchanged in type, meaning and range. `isCultivated` / `isField` / `corralled` unchanged. So
  **every shipped readout keeps working untouched** — verified: every consumer of the build meters
  (`SubjectDrawerController`, `MapView`, `RungGates`, `hud_compose_vocab`) renders a **percentage
  label** and nothing derives a turn estimate from them, so the issue's *"N turns remaining"* concern
  does not apply to shipped code.
- **Append `workDone` / `workCost`** on both plant meters and both animal ones. `% done` is exactly
  `workDone / workCost`, so the fraction fields stay as the redundant-but-free rendering path while
  the two absolutes are what let the UI say *"18 of 50 work"* — the sentence that makes the arc's
  thesis visible at all. Append-only, no cost to a client that ignores them.
- **Append `buildTurnsRemaining`.** The player-facing payoff of this arc is *turns fall as you
  improve*, and the player can only see that if something says so. The client **cannot** compute it —
  it holds neither the crew's output, nor the floor multiplier, nor the kit's coverage-weighted
  contribution — so the sim answers it, the same discipline as `penFeedUpkeep` and the yield
  forecast. `−1` (or an absent/false companion bool) where no build is in flight.
- The one existing turns-valued readout, `neglectGraceRemaining`, counts unworked turns and is
  computed sim-side. Unaffected.

**In-sim, `RUNG_COMPLETE = 1.0` retires** — each rung has its own completion value. Every
`>= RUNG_COMPLETE` comparison, `is_cultivated()` / `is_domesticated()` / `is_field()`, and the test
fixtures that fabricate a finished source via `accrue_domestication(faction, RUNG_COMPLETE)` are
call sites of this change.

## 9. Validation — `LadderConfig::validate` restated

| today | becomes |
|---|---|
| `0 < progress_per_turn` | `0 < work_cost`, finite |
| `0 < decay_per_turn < progress_per_turn` when present | `0 < decay_fraction_per_turn < 1` when present; `null` still means "does not bleed", a parked `0` still rejected |
| `grace_turns < 1 / progress_per_turn` | `grace_turns < work_cost / reference_output` (§7) |
| `crew_needed != Some(0)` | unchanged — it is a staffing floor now, and a floor of nobody is still nonsense |
| `knowledge.progress_per_turn > 0` | `knowledge.learn_rate > 0` **and** every `lesson_cost > 0` (a zero cost opens every gate on turn 1) |
| `0 < knowledge.completion_threshold <= 1` | unchanged — it is still the ledger bar |
| — | **new:** every declared tool work contribution `>= 0` and finite |

> **A build can still be out-run by its own decay** — one worker at a shallow floor produces 0.2
> units/turn against a 50-unit patch bleeding 0.5. That is true *today* too
> (`0.04 × 0.2 × 0.5 = 0.004` against `0.01`), and the arc improves it: the remedy is more hands,
> which the no-cap decision now actually permits. Not a new hazard, and not something validate can
> catch — it depends on the crew.

## 10. Decided, and what is left

**Settled (2026-08-11):** ship pacing-neutral first, with the cost spread as a config-only follow-up
(§3). The tool's contribution lands on the **job**, not the crew's output, and is **per equipped
worker** on the existing partly-equipped-party seam (§6.1–6.2). Source decay stays a **fraction of
the rung's cost** (§3.2). The animal web **becomes
crew-scaled** and that is accepted (§1.2). Knowledge does **not** feed throughput — it reaches it
through the tools it unlocks (§5). The wire **gains** `workDone` / `workCost` / `buildTurnsRemaining`
(§8). `headline_wear` quotes a **reference job** (§6.3).

**Left open, and both are tuning rather than model:**

1. **The cost spread itself** — what a Sow, a Corral and an eventual Farm cost relative to a
   Cultivate, and what each lesson costs. Wants measurement against a real campaign, not a decision
   in advance. It is slice 5, and it is the only thing left.

The second item this section carried — `MIN_BUILD_FRACTION`, a floor a tool contribution could not
shrink a job past — is **gone rather than open**. See §6.2: it was a structural guard against an
outcome the costs and contributions already decide.

## 11. Slicing

| slice | scope | ships |
|---|---|---|
| **1** | `core_sim` + schema — the build inversion: `work_cost`, absolute-unit meters, `crew_scale` deleted, species cost multiplier, decay restated, validate rewritten, capture divides; `workDone` / `workCost` / `buildTurnsRemaining` appended | pacing-neutral on plants; every existing plant turn-count test still passes |
| **2** | `core_sim` — the knowledge inversion: `lesson_cost`, `learn_rate`, `craft_lesson_per_item`, ledger stays normalized | pacing-neutral |
| **3** | `core_sim` — gear: `BuildRate` replaced by the coverage-weighted additive contribution, wear per work unit, `headline_wear` requoted | `husbandry_gear` at 17 units, a fully-geared animal build unmoved |
| **4** | **client** — the build readout becomes work and turns (below) | the arc becomes visible |
| **5** | config only — the cost spread: later rungs and later lessons cost more | the pacing payoff |
| **then** | **#539** — the hoe: two work contributions, one for breaking ground and one for already-prepared ground | |

### The client slice is NOT optional, and slice 5 is why

**A cost spread with no UX change is invisible.** Today the player sees *"Cultivation: 42%"*, and
after slice 5 they would see the same percentage filling at different speeds on different rungs with
nothing on screen saying why. The arc's whole thesis — *jobs have sizes, and your turns fall as you
get better* — lives entirely in numbers the current HUD does not show. So slice 4 lands **before**
the spread, and it is three readouts:

- **The build meter says work, not just percent** — *"Cultivation: 18 / 50 work (42%)"* on the tile
  card and the herd drawer, off the appended `workDone` / `workCost`.
- **A turn estimate beside it** — *"≈11 turns at this crew"*, off `buildTurnsRemaining`. This is the
  one that makes "turns are an output" legible: add hands and watch it drop.
- **The compose sheet quotes the job before you commit** — the cost of the improvement being
  offered, and what the current crew would take to finish it. Today the sheet offers a verb with no
  price at all, which was survivable when every verb cost the same 25 turns and will not be once
  they differ.

A fourth, once #539 lands: **what the gear took off the job** — *"your hoes: −8 work"* — which is
the only way the player can tell a hoe is worth carrying to a garden and not to a farm.

Slices 1–3 and 5 are `server-dev`; slice 4 is `client-dev`.

---

See also: `.claude/rules/core_sim/intensification.md` → "The build engine — THE seam both tracks
call" and "The knowledge pattern"; `.claude/rules/core_sim/equipment.md` → "The build axis";
`docs/plan_intensification_ladder.md`; `docs/plan_harvest_floor.md` §3.
