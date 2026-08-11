# Improvements and knowledge cost WORK, not turns

**Issue #542**, under arc #184. Blocks #539 (the plant web's build tool).

**Status: DESIGN DRAFT.** §§1–8 propose a model and recommend numbers; §10 lists what is still
genuinely open, each with a recommendation to argue with. Nothing here is implemented.

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

> **This is a real behaviour change on the ANIMAL web, and it must be said out loud.** Both animal
> rungs declare `crew_needed: null` and are therefore **unscaled by crew today** — a Tame takes 25
> turns whether two hands or twenty work the herd. Under the work model every build is crew-scaled,
> so a large keeper crew now tames materially faster. That is the model working as intended, but it
> is not pacing-neutral and no test pins it today.

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

### 3.2 Decay, restated

`decay_per_turn` becomes **a fraction of the rung's own cost bled per turn**, keeping today's 0.01
(≈100 turns to fully lapse, whatever the job's size).

The alternative — decay in absolute units per turn — makes a bigger investment take
proportionally longer to lapse ("a farm's clearing outlasts a garden's"), which is arguably truer.
It is rejected for continuity: the fractional form preserves today's numbers, keeps the
build:decay ratio a per-rung statement, and keeps the grace bound (§7) meaningful. Flagged in §10
as reversible with one line.

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

`per_worker_output` is **1.0**, flat (§1.1). The issue asks how knowledge contributes to it.

**Recommendation: it does not, and that is deliberately out of scope.** Knowledge paces *unlocks*
today; making it also pace *throughput* is a second job for the same mechanic and a new balance
surface, on top of an arc that already touches two webs, the schema and the gear model. The model
leaves the room — worker output is written as a sum of terms — and the term can be added later
without re-inverting anything.

## 6. The gear — an additive contribution, and the arithmetic that decides its shape

`EquipmentStat::BuildRate` is the wrong shape and is replaced. The gear model has **no
"contributes N" representation today**: every effect names the value a stat *takes*
(`equipment.md` → "An effect names the value a stat TAKES"). Adding one is a model change.

### 6.1 THE FINDING: an additive RATE is not scale-sensitive. Only an additive COST is.

This is the load-bearing piece of arithmetic in the whole design, and it is easy to get backwards.

**Shape A — the tool is an extra pair of hands** (`work/turn = Σ workers + Σ tools`):

```
turns = cost / (W + t)      speedup = (W + t) / W
```

The speedup **does not mention the cost at all**. A hoe worth +1 against a 2-hand crew is +50% on a
garden *and* +50% on a farm. What varies is the crew size, not the job — so the hoe is diluted by
however many hands the player brings, and it is *most* valuable exactly when they bring the fewest.
Under the no-cap decision (§1.2) the player controls that dilution freely. **This shape does not
deliver the stated intent.**

**Shape B — the tool is worth N units of the job** (`effective_cost = cost − Σ tool_worth`):

```
turns = (cost − t) / W      saving = t / cost
```

The saving is **inversely proportional to the job's size** — 30% off a 50-unit garden, 6% off a
400-unit farm — and it does not mention the crew. This is exactly *"a tool says 'I am worth N
units,' and the job's own size decides whether that matters,"* and it is what makes **no item ever
name an improvement** true rather than aspirational.

**Recommendation: Shape B.**

### 6.2 What that means concretely

- A new stat, neutral at **0.0**, resolved as the **SUM** of what the kit's live declaring items
  state — not the max. Summing is what "additive" means and is the only way a hoe and a mattock
  both count; taking the max (`dispersion`/`exposure`'s shape, and what `BuildRate` uses today)
  would make the second tool worthless. This is a genuinely new resolution rule in
  `equipment_config.rs` and should be stated as one.
- **Summed over distinct declaring items, not over the batch count.** Ten hoes are not ten hoes'
  worth of help; the contribution is "the crew brought the right tool." Consistent with
  `BuildRate` already not being coverage-averaged. (Open — §10.)
- **Clamped**: `effective_cost = max(cost × MIN_BUILD_FRACTION, cost − Σ tool_worth)`, so a tool
  worth more than a small job cannot make it instant or negative.
- **Calibration.** `husbandry_gear`'s flint tier declares ×1.5 today, which on a 50-unit build is
  worth `50 × (1 − 1/1.5) ≈ 17` units. Ship **17** and the animal builds stay where they are.
- **Evaluated live each turn** off the crew's current kit, so acquiring or wearing out a tool
  mid-build takes effect immediately, and no state records "this build was geared."

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

`ItemDefinition::headline_wear`'s "≈12.5 builds" readout no longer has a fixed denominator. Quote
it in work units, or against a named reference build. (Open — §10.)

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
  unchanged in type, meaning and range. `isCultivated` / `isField` / `corralled` unchanged.
- **No client change is required.** *Verified*: every consumer of the build meters
  (`SubjectDrawerController`, `MapView`, `RungGates`, `hud_compose_vocab`) renders a **percentage
  label**; nothing derives a turn estimate from them. The issue's concern about *"N turns
  remaining"* readouts does not apply to shipped code.
- The one turns-valued readout, `neglectGraceRemaining`, counts unworked turns and is computed
  sim-side. Unaffected.
- **Optional, if a readout ever wants it:** append `workDone` / `workCost` so the client can say
  *"18 of 50"*. Not required by this arc.

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

## 10. Open — what still needs a decision

1. **Pacing-neutral first, or ship the cost spread with the model?** (§3) Recommendation:
   neutral first, spread as a config-only follow-up.
2. **Tool contribution: Shape A or Shape B?** (§6.1) Recommendation: B — it is the only one that
   delivers the stated scale-sensitivity. If A is wanted anyway, the hoe in #539 needs a different
   justification.
3. **Does a tool's contribution scale with how many the band holds, or with coverage?** (§6.2)
   Recommendation: neither — distinct items, counted once.
4. **Decay as a fraction of cost, or absolute units per turn?** (§3.2) Recommendation: fraction,
   for continuity. Absolute has a real argument ("a bigger clearing lasts longer").
5. **The animal web becomes crew-scaled** (§1.2). Accept, or give the animal rungs a `crew_needed`
   so the change is bounded? Recommendation: accept — a crew-blind build is the thing the model
   removes.
6. **Does knowledge feed worker throughput?** (§5) Recommendation: not in this arc.
7. **What `headline_wear` quotes** once a build has no fixed size (§6.3).
8. **Should the wire gain `workDone`/`workCost`?** (§8) Recommendation: no, until a readout asks.

## 11. Slicing

| slice | scope | ships |
|---|---|---|
| **1** | `core_sim` — the build inversion: `work_cost`, absolute-unit meters, `crew_scale` deleted, species cost multiplier, decay restated, validate rewritten, capture divides | pacing-neutral; every existing turn-count test still passes on the plant web |
| **2** | `core_sim` — the knowledge inversion: `lesson_cost`, `learn_rate`, `craft_lesson_per_item`, ledger stays normalized | pacing-neutral |
| **3** | `core_sim` — gear: `BuildRate` replaced by the additive contribution, wear per work unit | `husbandry_gear` at 17 units, animal builds unmoved |
| **4** | config only — the cost spread: later rungs and later lessons cost more | the visible payoff |
| **then** | **#539** — the hoe: two work contributions, one for breaking ground and one for already-prepared ground | |

Slices 1–3 are `server-dev` work against a settled spec. Slice 4 is a tuning pass that wants
measurement, not implementation.

---

See also: `.claude/rules/core_sim/intensification.md` → "The build engine — THE seam both tracks
call" and "The knowledge pattern"; `.claude/rules/core_sim/equipment.md` → "The build axis";
`docs/plan_intensification_ladder.md`; `docs/plan_harvest_floor.md` §3.
