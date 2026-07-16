# Unified Intensification Ladder — One Grammar for Plants and Animals

**Status:** design, pre-implementation. The **next arc after grazing 2d** (pen economy). Manual-first
when built (new player-facing gameplay grammar). Sits on top of the 2d pen economy and the
[[grazing-foundation]] two-food-web model.

**Relationship to `docs/plan_intensification.md`:** that doc owns the *content* arc
(depletion → domestication → agriculture — making forage depletable and transposing husbandry to
plants). **This doc refines its *interaction and knowledge model*** — how the player drives
intensification and how the tech unlocks. A reconciliation pass against `plan_intensification.md` is
required when this arc starts (flagged in §8).

---

## 1. The problem this fixes

The two intensification paths shipped with **inconsistent interaction grammar**:

- **Plants:** `Cultivate` is a **direct, visible verb** — pick it, it pays a low yield while you prepare
  the patch, progress climbs, and it decays if you stop ("goes feral").
- **Animals:** there is **no direct verb** — taming accrues as a hidden side effect of the `Sustain`
  *harvest* policy, and the visible verb (`Corral`) is *disabled* until the herd is already tamed. The
  UI has to explain in text that "Sustain secretly tames."

Same conceptual act — "invest labor now for a higher managed yield later" — two different grammars, one
of which is undiscoverable. This arc unifies them.

---

## 2. The unified model: symmetric 3-rung ladders

Both food webs are **three rungs**, fully parallel:

| rung | plants | animals | what you control |
|---|---|---|---|
| 1 — wild | forage patch | wild herd | nothing — take what's there |
| 2 — tended/tamed | tended patch | pastoral herd | you *manage the wild source in place* |
| 3 — farm/pen | seeded farm | corralled pen | you control its *reproduction* (sow / breed) |

Every rung-transition is a **Cultivate-shaped verb**: pick it → **lower** yield while you work (the
gentle-policy cost) → **per-source build meter** climbs → decays if you abandon it → on completion the
source steps up a rung. Plants run it twice (Cultivate → Sow); animals run it twice (Tame → Corral).

---

## 3. Every rung is worker-driven; intensifying raises four dials

**Decision (settled):** the shipped "passive-free pastoral" rung is **retired**. A tamed herd is not
"set and forget" — you still work it with people, it's just more productive per worker. This matches
plants (a farm isn't passive) and keeps every rung engaged. Intensifying up the ladder raises:

1. **Yield per worker** — tamer/managed stock is easier to harvest, so each worker brings back more.
   (This *is* the "buy freedom" payoff, delivered granularly: more food per worker frees the surplus
   workers for other tasks, rather than the binary "pastoral = zero workers".)
2. **Regeneration rate (`r`)** — managed/bred/seeded stock refills faster. This is the per-species
   husbandry-`r` ladder 2d already built (wild → ×`pastoral_gain` → ×`pen_gain`).
3. **Carrying ceiling (`K`)** — on good pasture the pen's footprint raises K (2d already).
4. **Proximity** — the spatial spine, animals only (plants can't move):
   - **Wild:** roams its full range — you chase it across tiles.
   - **Tamed (pastoral):** the herd **drifts toward the owning band** and stays near — less chasing =
     higher effective yield, and it *reads* as domesticated.
   - **Penned:** range collapses to the fence — zero chasing, fully fixed.

One legible motion up the ladder: **far → near → fixed**, with yield/regen/ceiling climbing alongside.

---

## 4. The knowledge pattern: practice rung N unlocks rung N+1

**Practicing a rung earns the knowledge that unlocks the next rung's verb.** You learn *herding* by
managing wild herds; *penning* by managing tamed ones. Knowledge emerges from doing — diegetic, DRY
(one mechanic per rung), and strictly sequential (you can't skip a rung you haven't practiced). It
extends indefinitely: practice the pen → learn *selective breeding* (rung 4); practice the farm → learn
*irrigation / rotation*. Same rule every time.

### 4.1 Two meters, two jobs — DO NOT MUDDLE (this is the root fix)

Conflated meters are exactly what made the original UX inconsistent. Keep them visibly distinct at
every rung:

- **Knowledge** (Herding, Penning, Cultivation, Farming): **faction-wide, earned once by cumulative
  practice, permanent.** "Can my people do this verb *at all*?" Accrues in the background as you work
  the tier below. Hooks into the existing discovery/knowledge system (do **not** build a parallel one).
- **Per-source build progress** (this herd's taming %, this patch's cultivation %): **local to one
  food source, filled by the verb, decays if abandoned.** "Have I done it to *this* source yet?"

Flow: *hunt → learn **Herding** (knowledge) → **Tame** verb appears → Tame fills **this herd's** meter
→ pastoral → practicing Tame earns **Penning** (knowledge) → **Corral** verb appears → …* Two meters
advance from one action — fine, as long as the UI shows "this animal's progress" and "my civilization's
skill" as different things.

### 4.2 Rules that keep the theme honest

- **Only stewardship policies teach.** Sustain / Tame / Tend earn the next knowledge; Market / Surplus /
  Eradicate do not. You learn husbandry by *managing*, not slaughtering — the same "restraint is the
  path" principle carried from the corral arc.
- **The two food webs learn separately.** Hunting feeds the animal track (Herding → Penning), foraging
  feeds the plant track (Cultivation → Farming). A master rancher isn't automatically a farmer.
- **Knowledge is general; the [[wildlife-overlay-phases]] husbandry ceiling is per-species.** Taming a
  `pastoral`-ceiling steppe-runner still teaches *Penning* knowledge — you just apply it to a boar,
  since the steppe-runner itself can't be penned. Knowledge = "I know how"; ceiling = "this animal
  allows it." Decoupled.

### 4.3 Gate reshuffle vs today

Today `Herding` gates `Corral` (rung 3) and taming is ungated. Under the pattern, one knowledge gates
each transition: **Herding gates Tame (rung 2); a new Penning gates Corral (rung 3)** — and symmetric
for plants (Cultivation gates Cultivate; Farming/Seeding gates Sow).

---

## 5. The ladder is configuration (the scaling ambition)

Same verdict as the fauna roster: **the ladder is data over a bounded set of coded primitives.** A rung
is a record; the ladder is a list; adding a rung after farm/pen is appending a record.

```jsonc
{ "id": "corral", "branch": "animal", "order": 3,
  "verb": "Corral",
  "unlock_knowledge": "penning",           // gate to select the verb
  "earns_knowledge": "selective_breeding",  // practicing THIS rung teaches the next
  "requires_rung": "pastoral",
  "ceiling_required": "pen",               // husbandry_ceiling gate
  "build": { "progress_per_turn": …, "decay_per_turn": …, "yield_fraction_while_building": … },
  "effects": { "regrowth_mult": 3.0, "yield_per_worker_mult": …, "ceiling_mult": … },
  "behavior": { "movement": "fixed", "feeding": "self_graze", "harvest": "worker_tend" } }
```

- **Dials = pure config** — the numbers and the links (unlock/earns knowledge, prev rung, ceiling,
  build/decay/yield rates, effect multipliers). A rung that's "the pen but more so" is a one-record edit.
- **Behaviors = config over coded primitives** — bounded enums: `movement: roam | drift_to_owner |
  fixed`, `feeding: forage | self_graze`, `harvest: worker_hunt | worker_tend` (extend as needed). A
  rung that *recombines* existing primitives is pure config; a rung needing a *new* primitive codes that
  one primitive once, after which it too is a config option.

**Core deliverable of the arc — the generic rung engine.** A system that, for any food source, reads
its current rung, advances the build meter, applies the effect dials, runs the behavior primitives, and
earns the next knowledge from practice — **replacing today's bespoke `pastoral`-vs-`pen` code branches**
(`herd_ecology`, `herd_capacity`, the labor FEED/HARVEST arms). Once it exists, the ladder is a JSON
file (`intensification_ladder.json` or per-branch). Seed the primitive set richly enough that the
interesting future rungs (selective breeding, irrigation, traction, crop rotation) are config-only.

---

## 6. Parked ideas (fold in as rung effects when wanted)

- **Secondary products** — a tamed/penned herd pays a *second* output beyond meat (hides/wool/milk →
  trade goods or a materials resource), like a farm gives grain *and* fiber. Differentiates the *kind*
  of output, and gives a reason to intensify even when not food-starved. Naturally a rung `effects`
  field (a yield vector, aligning with the "command yield-vector" task in `plan_intensification.md`).
- **Reliability / no flight** — a tamed herd is *owned*: won't migrate away, won't be hunted out by a
  rival, no failed-hunt variance. "Steady and owned" is worth intensifying for even at equal mean yield.
- **Selective breeding (rung 4)** — a long-penned herd slowly improves (`r`/`K` creep up): the patient
  rancher's payoff, and the first config-only rung the engine should be able to express.

---

## 7. Consequences to honor

- **Retiring passive-free pastoral** changes 2d's shipped pastoral rung (passive/free → worker-driven,
  more efficient). This is a deliberate re-tune; the "buy freedom" thesis is preserved via yield-per-
  worker. Measure in playtest.
- **Adding a rung-2 knowledge gate** (Herding now gates Tame) means taming is no longer ungated —
  paces the ladder to practice. Intended.
- The husbandry-ceiling work from 2d ([[grazing-2d-pen-economy]]) already provides the per-species
  applicability gate this model relies on.

---

## 8. Slice plan (rough — sequence when the arc opens)

1. **Reconcile with `plan_intensification.md`** and draft the manual section (manual-first). Settle the
   rung/knowledge names across both food webs.
2. **The rung engine + ladder config** — extract today's bespoke pastoral/pen (and cultivate) logic
   into the generic engine driven by `intensification_ladder.json`; define the primitive enums. This is
   the load-bearing refactor; land it behavior-preserving first (same rungs, now data), then …
3. **Tame verb + worker-driven pastoral + proximity** — the animal rung-2 rework (retire passive-free,
   add the direct verb, the drift-to-owner movement primitive).
4. **The knowledge pattern** — practice-earns-next-knowledge wiring on both tracks; the two-meter UI
   split (the root UX fix); the gate reshuffle.
5. **Plant parity** — confirm forage → tended → farm rides the same engine/verbs/knowledge grammar.
6. **Parked ideas** as follow-on config rungs (secondary products, selective breeding).

---

## 9. See also

- `docs/plan_grazing_2d.md` — the pen economy this sits on (self-feeding, per-species `r`, husbandry
  ceiling).
- `docs/plan_intensification.md` — the depletion → domestication → agriculture *content* arc this
  refines the interaction model of.
- `docs/plan_grazing_foundation.md` — the two-food-web foundation.
- `core_sim/CLAUDE.md` — "The husbandry yield ladder" (the flat-rung description this supersedes).
