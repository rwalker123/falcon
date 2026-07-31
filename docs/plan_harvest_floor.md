# The Harvest Floor — one dial replaces the four-stance axis

**Status:** design, not started. Supersedes the Sustain / Surplus / Deplete / Eradicate policy axis on
**both** food webs.

**Provenance.** Every measurement below comes from `core_sim/src/forage/stance_probe.rs` — the
`#[ignore]`d harness on branch `worktree-investment-rung-toggle` (PR #448). Re-run with:

```
cargo test -p core_sim --lib stance_probe -- --ignored --nocapture --test-threads=1
```

**Depends on two unmerged branches.** PR #448 split stance from improvement (you can hold any stance
while building), and a second session turned Tame/Cultivate from actions into checkboxes. This arc
assumes both have landed: it deletes the extractive half of `FollowPolicy` and leaves the verb half
to whatever those branches settled on. **Confirm the state of both before starting.**

**Prototype.** An interactive mock of the resulting dialog — real escapement sim, shipped config
numbers — is the reference for §7:
https://claude.ai/code/artifact/339c0b6a-4d1b-41ca-a140-19321dac6a27

---

## 0. Why the four stances are not a coherent model

The axis promises a time-preference ladder: each rung takes more now and leaves less later. Two
checkable invariants follow, and the shipped model violates both.

### 0.1 Every plant attractor is a guard rail, not an equilibrium

Measured on a `K = 195` patch over 600 turns from full:

| stance | settles at | which config constant that is |
|---|---|---|
| Sustain | 0.507 K | — (the MSY feedback rule's own fixed point, by design) |
| Surplus | 0.123 K | `forage.ecology.collapse_fraction` **0.15** |
| Deplete | 0.020 K | `forage.reseed_floor_fraction` **0.02** |
| Eradicate | 0.017 K | `forage.reseed_floor_fraction` **0.02** |

None of the three non-Sustain resting points was designed. Each is a safety valve doing load-bearing
economic work by accident, through one of two seams:

**Seam 1 — the plant take rule believes in a collapse threshold the plant growth rule does not have.**
`regrow_patch` → `reseeding_logistic_regrowth` → `logistic_regrowth`: pure logistic, no Allee, and
that is deliberate (plants reseed). But Sustain/Surplus resolve their ceiling through
`sustainable_yield` → `net_biomass_delta`, which **does** apply the Allee cutoff and returns `0` below
`0.15 K`. So Surplus over-harvests until the patch drops under the threshold, its own take switches
off, the patch regrows past it, and it takes again — a sawtooth around `collapse_fraction`. That is
the "permanent, profitable Collapsing camp": not a balance decision, a depensation term shared
between two webs that model growth differently.

**Seam 2 — the reseed floor is an income stream, not a recovery device.** Deplete and Eradicate both
drive to the floor and then farm it: the patch is lifted to `0.02 K` free every turn and each stance
takes its fraction of the resulting regrowth. Long-run income is therefore `take_fraction × the
floor's regrowth`, which is why **Eradicate out-earns Deplete forever by exactly `0.30 / 0.20 = 1.5×`**
(measured 0.084 vs 0.056 food/turn). The harsher stance pays more in perpetuity because it takes a
bigger slice of a floor that refills for nothing.

**The animal web is coherent for one reason:** `regrow_biomass` and `sustainable_yield` both go
through `net_biomass_delta`, so growth and take share the same depensation assumption, and herds have
no reseed floor (they have `extinction_floor` and despawn instead). Take model and growth model agree.
On plants they disagree, in two places, in opposite directions.

### 0.2 The stance is inert exactly when the choice matters

All four ceilings are worker-capped, so "which stance" and "how many workers" are two dials on one
number. On a full `K = 195` patch (`per_worker_biomass_capacity` 8.0):

| workers | Sustain | Surplus | Deplete | Eradicate |
|---|---|---|---|---|
| 1 | 8 | 8 | 8 | 8 |
| 2 | 12.19 | 16 | 16 | 16 |
| 5 | 12.19 | 19.5 | 39 | 40 |
| 8 | 12.19 | 19.5 | 39 | 58.5 |

You need **8 gatherers before Eradicate differs from Deplete**, 5 before Deplete differs from Surplus,
and below 3 the harsher three are the same number. Worse: the fraction-of-stock ceilings shrink with
biomass, so the crew binds hardest on a *healthy* patch and stops binding once the patch is wrecked —
the stance is a no-op while the decision is live and bites hardest once it is too late. A lone forager
on Eradicate parks the patch at **0.79 K, permanently Thriving**.

### 0.3 The build dip is a fraction of the policy ceiling, so the harshest stance builds free

`yield_fraction_while_building` multiplies the *policy* ceiling. Dipped ×0.25, all four stances stay
Thriving for a whole 25-turn Cultivate, all complete on schedule, and **Eradicate pays 16.62 food to
Sustain's 4.40 (3.8×)**, leaving the patch at 0.68 K. Zero cost, strictly dominant. The `Thriving`
start gate does not catch it because `Thriving` is a *level* band and a 25-turn Eradicate never leaves
it.

### 0.4 Three more, same root

- **Rung 3 has no health gate on either web.** Sow completes under Eradicate; Corral completes around
  a herd at 0.008 K.
- **Teaching is Sustain-only at every rung, both webs** — the whole knowledge ladder needs one stance
  held, and nothing says so.
- **Two of five probed species can never be tamed** (`husbandry_ceiling: wild` — deer, mammoth; also
  wolf). There the stance is the *entire* decision, which raises the bar on its coherence rather than
  lowering it.

---

## 1. The model

> **Workers set the rate. The floor says where to stop.**

```
take = min(workers × per_worker_carry × seasonal × build_dip,
           max(0, B − floor × K))
```

That is the whole take path, for both webs. No multiplier, no fraction of stock, no second lever
competing with the first.

This is **constant escapement**, and it is not a new concept here: `core_sim` already uses
`max(0, B − K/2)` as a penned herd's harvest rule, and `yield-forecast.md` already records the
property that makes it the right shape — escapement is **`r`-independent**, unlike MSY.

- `floor` is a **fraction of `K`**, carried per labor assignment. Fraction, not absolute, because `K`
  varies per tile and the player thinks in "half the herd"; and because the phase bands are already
  fractions of `K`, so a floor and a colour on the bar are the same object.
- **Whole-animal quantisation is unchanged.** `quantise_animal_take(policy_ceiling, collection,
  body_mass)` keeps its shape; the `policy_ceiling` argument simply becomes `max(0, B − floor × K)`.
  The kill/carry/`wasted` distinction, the one-animal floor, and the `hunt_credit` bank all survive —
  the bank is still needed for a slow breeder whose per-turn escapement is under one body mass.
- **Seasonal weight still folds into per-worker throughput**, exactly as `forage_take` does today. It
  can be `0` in a dead season, so consumers must not divide by it.

### 1.1 What "take everything" means is web-specific, and that stays

`floor = 0` means "harvest maximally," and the two webs answer it differently *by config that already
exists*: a patch is lifted by `reseed_floor_fraction` 0.02 and comes back; a herd falls under
`extinction_floor` 0.02 and despawns. Same constant, opposite consequence. Keep it, and **say it in
the UI** rather than letting the player discover it.

---

## 2. The floor's meaning, and the pivot at K/2

Sustained take at floor `f` is the regrowth there: `r · fK · (1 − f)`, which peaks at **`f = 0.5`**.
So:

- **Below K/2** you are spending the future for calories now.
- **Above K/2** you are giving up calories — and §3 is what you get in exchange.

Before this arc a floor above K/2 was strictly dominated (less food, no compensation) and half the
axis was dead. With §3 it is the knowledge-investment half, and every position on the dial sits on one
side of a real trade.

**Presets** (shortcuts to positions; the continuum is the drag):

| preset | floor | intent |
|---|---|---|
| Take everything | 0 | strip it; patch reseeds, herd dies |
| Best harvest | 0.50 | the food peak — max sustained calories |
| Learn from it | 0.80 | trade calories for ladder progress |

Naming is not settled — see §10.

---

## 3. Learning and build progress ride the floor

**One rule replaces three broken gates.** Both the faction knowledge accrual and the per-source build
accrual are multiplied by

```
learn_mult = floor / MSY_BIOMASS_FRACTION      // = floor / 0.5
```

gated only on `take > 0` (you must actually be working the source). Normalised so the food peak is
×1.0, meaning today's 25-turn Cultivate stays 25 turns at the floor a player is most likely to pick.

This deletes, without replacement:

- the **Sustain-only** condition on `RungDef::knowledge_earned` (§0.4);
- the **`Thriving` start gate** on Cultivate, along with `validate_labor_policy`'s exemption for a
  build underway, `ForagePatch::cultivation_underway`, and the whole start-gate/continue-gate ruling —
  there is no gate left to lapse (§0.4);
- rung 3's **missing** health gate — it now has the same one as every other rung, which is no gate at
  all, just a rate.

Non-degenerate at both ends: `floor = 1.0` takes nothing, so watching teaches nothing; `floor = 0`
leaves nothing standing, so stripping teaches nothing.

### 3.1 The build dip moves onto crew throughput

`yield_fraction_while_building` multiplies **`workers × per_worker_carry`**, not a policy ceiling. The
people are clearing ground, not gathering. Two consequences:

- It is **stance-independent by construction** — there is no stance left to dodge it with, which is
  §0.3 fixed rather than patched.
- It is legible: at 25% carry it takes four times the people to clear the same standing surplus.

---

## 4. The yield vector is unchanged — minus every factor

A take of `B` biomass pays `B × vector` into three accounts, with no role branch:

| account | rate | lands in |
|---|---|---|
| provisions | `patch_provisions_per_biomass` / `HuntYield` | the **band's** `FOOD` store |
| fodder | `patch_fodder_per_biomass` | the **band's** `FODDER` store |
| trade goods | `patch_trade_per_biomass` / `HuntYield` | the **faction** `trade_goods` stockpile |

**`forage.market.trade_goods_multiplier` (4.0) is deleted.** It is a vestige of the policy's old name
`Market`, when that rung produced trade goods *instead of* food. Both accounts are live on every
harvest now, so a markup attached to one drawdown rate has no meaning. **No option carries a factor of
any kind after this arc.**

Rung-2 weeding and conversion are untouched: `tended_weeding_gain` 1.5 on the favoured share (increase
taken from the least abundant first), `tended_conversion_gain` 2.0 on the favoured species' whole
vector. Rung 2 does not change *how much* you take — the take is still the floor's regrowth — it
changes what the take converts to.

---

## 5. What is deleted

**Config** (`labor_config.json`, `fauna_config.json`):

- `forage.surplus_multiplier` 1.6
- `forage.market.take_fraction` 0.20, `forage.market.trade_goods_multiplier` 4.0
- `forage.eradicate.take_fraction` 0.30
- `hunt.surplus_multiplier` 1.5, `hunt.deplete_multiplier` 2.5, `hunt.surplus_escapement_fraction` 0.3

Replaced by one `floor` fraction per assignment (plus, optionally, a `default_floor` for a fresh
assignment — see §10).

**Code:**

- `FollowPolicy`'s four extractive variants and `parse_follow_policy`'s handling of them;
  `FollowPolicy::as_str` for those variants; `valid_for_forage` / `valid_for_hunt`'s extractive arms.
- `forage_policy_ceiling`'s four extractive arms and `hunt_policy_ceiling` / `hunt_policy_rate`'s.
- `sustainable_yield`'s use **in the take path** (it survives for telemetry: `SourceYield.sustainable`
  is still MSY, and the overdraw ⚠ still means "you are drawing this down").

**Wire** (`snapshot.fbs`) — these become dead slots, per the repo's append-only discipline; do **not**
renumber:

- `ForagePatchState.ceilingSustain` / `ceilingSurplus` / `ceilingDeplete` / `ceilingEradicate`
- `ForagePatchState.ceilingCultivate` / `ceilingSow`
- `HerdTelemetryState.huntPolicyCeilings`
- The investment payoff twins (`tendedYield`, `fieldYield`, `pastoralYield`, `corralYield`,
  `pastoralTrade`, `corralTrade`) **stay** — they answer "what will this pay once complete," which is
  still a live question and is what the prototype's focus row renders.

**New wire:** `LaborAssignment.floor:float` (append at the end of its table) and the matching
`AssignLaborCommand.floor`. `LaborAssignmentState.floor` for the rollback checkpoint.

---

## 6. Not in scope

- **Denial** — tracked separately as **#456**. `floor = 0` is maximal *harvest*, which is not denial:
  destroying a food source should be cheap and fast, harvesting is neither. It becomes a **party you
  send** with no rate and no carry cap, sized by the animal rather than by what a crew can carry home.
  Note this arc's dependency runs the other way too: `ExpeditionMission::Hunt` carries a
  `policy: FollowPolicy` whose Eradicate arm is documented as *"hunt to extinction … denial is the end
  state"*, so slice 2 must not delete that variant out from under the expedition without #456 landing
  or the mission being stubbed.
- **A market axis.** With the 4× gone, every harvest sells its trade goods and *"am I selling this or
  eating it?"* is asked nowhere. That is a real design gap this arc creates and does not fill.

---

## 7. UI

Reference prototype: https://claude.ai/code/artifact/339c0b6a-4d1b-41ca-a140-19321dac6a27

Panel order is coarse-to-fine, and each control depends only on what is above it:

1. **Header** — source, and the state in words (`195 of 195 standing · Thriving`), so an intent can be
   chosen without reading the chart.
2. **Intent presets** (§2) — three buttons. Named positions on the chart below, not a separate concept.
3. **The chart** — the stock bar and the projection are **one instrument**: phase bands are horizontal
   zones, the floor is a draggable horizontal line, the 60-turn projection is the curve beneath, and
   the food peak is marked. A gradient rail on the right encodes `learn_mult`, with a marker at the
   floor. This is the height win: two tall elements become one, and the instrument gets better.
4. **The build row** — a **single** checkbox, because a source stands on one rung and offers exactly
   one next verb (you cannot Sow what you have not Cultivated). Shows the carry cost.
5. **The crop focus row** — appears only when the build is checked, plant side only. Chips are the
   tile's *realized* basket; the commitment rides the assignment
   (`LaborTarget::Forage { tile, floor, species }`), so it belongs here, not on a separate surface.
   Under it, the three-account payoff once the rung completes versus gathering it wild now.
6. **Crew** — a stepper plus **two clickable targets**, because the floor model creates a distinction
   the rate model did not have:
   - *clear it now* = `(B − floor·K) ÷ (carry × dip)`
   - *hold it after* = the regrowth at the floor over the same carry, rounded up to one body on a
     whole-animal source (which is what `hunt_haul_workers` already does)
7. **Readout** — the three accounts, **rendering only where the vector pays** (a cash crop shows no
   food line, not `0.00 food/turn`; a wolf shows no food line at all), each labelled with where it
   lands (`→ camp` / `→ stockpile`); the whole-animal waste line when kill > carry; the verdict; and
   the idle-crew and teaching notes.

### 7.1 The verdict line is the point of the redesign

The four-stance picker let you select Eradicate with one worker and never eradicate anything. Because
the crew and the floor are now independent statements, the panel can compare them and say which is
binding:

- *Reaches the floor in 9 turns, then holds it — taking only what grows back.*
- *This crew can't draw it that low. It settles at 62% and holds there — 11 gatherers would reach the
  floor.*
- *Already at or below the floor. This crew takes nothing until it grows past 98.*

### 7.2 Idle crew is reported, never released

Workers above the *hold* number contribute nothing, ever — but at-the-floor is the most **reversible**
condition in the model (lower the floor, or let seasonal weight move the hold number, and they are
needed again). The repo's own discriminator applies: a *permanent* condition rewrites an assignment
(out-of-range lapse, a completed build retiring its verb); a *reversible* one does not (a build whose
gate lapses "holds its progress and simply stops accruing — it is not lost and the policy is not
silently switched"). So: no auto-release, no notification. The surplus is a number on the row, which
is what `SourceYield.workers_needed` already computes and the band panel already renders.

---

## 8. Slices

Each lands on its own PR.

1. **The take path.** `forage_take` and `hunt_take` take a `floor` and use escapement. Keep
   `FollowPolicy` alive and map each old variant onto a floor so nothing else breaks yet
   (`Sustain → 0.5`, `Surplus → 0.3`, `Deplete → 0.15`, `Eradicate → 0.0`). Forecast follows through
   the same shared helpers — `forecast == actual` must hold at every step, per component.

   **Deplete is `0.15`, not `0.1`** — the value `systems::expeditions::hunt_expedition_floor` already
   ships (`ecology.collapse_fraction`). Reusing it is what lets the resident band's transitional
   table and the raid's floor table collapse into **one** (`FollowPolicy::escapement_floor`), instead
   of shipping two tables a turn apart from each other for the length of a slice.
2. **The floor on the wire and the assignment.** `LaborTarget::Forage`/`Hunt` carry a floor, the
   command text and proto carry it, the checkpoint round-trips it. Extractive `FollowPolicy` variants
   deleted, the mapping from slice 1 removed, dead config keys removed.
3. **Learning and build accrual ride the floor.** `RungDef::knowledge_earned` and `build_accrual` take
   the multiplier; the `Thriving` gate and its exemption machinery come out; the dip moves onto crew
   throughput.
4. **Client.** The merged chart, presets, single build checkbox, focus row, two crew targets, the
   three-account readout with routing labels.

---

## 9. Validation

**Promote the probe.** `stance_probe.rs` becomes a non-`#[ignore]`d property test asserting the two
invariants the old axis violated, swept over both webs and a range of crew sizes:

- **turn-1 take is monotone decreasing in the floor** — a lower floor never takes less *now*;
- **the 600-turn total is monotone INCREASING in the floor below K/2** — a lower floor always takes
  less *in the end*. Together those two are the whole trade below the peak: now against later, and
  the sign flips between them.
- **the learn-multiplier is monotone increasing in the floor** (slice 3) — which is what gives the
  half of the dial *above* K/2 something to trade, since there the 600-turn total starts falling.

> The second bullet used to read *"decreasing in the floor below K/2"*, which is the opposite of
> what §2 derives: sustained take is `r·fK·(1−f)`, rising across `[0, 0.5]`, and 600 turns of it
> swamps the one-off drawdown windfall a deeper floor collects (measured on the reference patch:
> floor 0.5 totals ~7.4k against floor 0's ~0.8k). Below the peak a higher floor is better on
> **both** axes — that is not a missing trade, it is why the interesting half of the dial is above
> K/2.

Plus:

- `forecast == actual` per component, on the **exported snapshot**, across both webs × a defaulting
  species × an inedible one (`wolf`: `provisions_per_biomass 0.0`) × labor-bound and escapement-bound
  staffing. The existing `hunt_yield_vector.rs` tests are the model.
- **A liveness assertion beside every monotonicity one.** A diff-based metric improves when a feature
  breaks; assert the take is non-zero where it should be, not merely ordered.
- `crewToHold`'s sim twin agrees with `hunt_haul_workers` on a whole-animal source, and with the
  continuous inversion on a patch — the panel's targets and `workers_needed` must be the same number.
- A patch driven to `floor = 0` still recovers (`reseed_floor_fraction`), and a herd driven to
  `floor = 0` still despawns (`extinction_floor`). Same constant, opposite outcome, both pinned.

---

## 10. Open questions

| # | Question | Notes |
|---|---|---|
| 1 | **The learning curve's shape.** | Linear in the floor is what is specified. If knowledge should read as a commitment rather than a dividend it wants a knee — little below the food peak, steep above. Config lever either way. |
| 2 | **Preset naming.** | *Keep it thriving / take everything* name the sim's band; the player may want the promise instead. |
| 3 | **The default floor on a fresh assignment.** | `0.5` (the food peak) is the safe answer and makes the common case one click. Needs a config lever or a constant. |
| 4 | **Where the market question goes.** | See §6. |
| 5 | **The Foddering gate.** | A *wild* patch's fodder credit is gated on the faction knowing Foddering; rungs 2–3 are ungated. Does that gate survive, and if so how does the panel say so without a fourth state on the readout row? |

---

## See Also

- `.claude/rules/core_sim/intensification.md` — the ladder engine, and the corrected stance note this
  arc acts on
- `.claude/rules/core_sim/flora.md` — the yield vector and its three accounts
- `.claude/rules/core_sim/cultivation.md` — the `Thriving` start gate this arc deletes
- `.claude/rules/core_sim/yield-forecast.md` — `forecast == actual`, the invariant every slice must hold
- `docs/plan_intensification_ladder.md` — the ladder this axis sits beside
- `docs/plan_hunt_yield_model.md` — the `Market → Deplete` rename whose vestige §4 removes
