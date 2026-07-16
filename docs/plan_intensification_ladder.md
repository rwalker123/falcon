# Unified Intensification Ladder — One Grammar for Plants and Animals

**Status:** **arc open** (branch `worktree-intensification-ladder`). Follows grazing 2d (pen economy,
PR #127, merged). Manual section landed in
`shadow_scale_strategy_game_concept_technical_plan_v_0.md` §2a ("The Intensification Ladder").
Sits on the 2d pen economy and the `docs/plan_grazing_foundation.md` two-food-web model.

**Scope decision:** **full symmetry** — this arc builds the plant rung 3 (**Field**/`Sow`) as well as
the animal rework, so both ladders ship complete. (`plan_intensification.md` deferred "plant crops on
arbitrary tiles" to "the next arc" — this is that arc.)

## 0. What is already shipped (recon, 2026-07-16) — read before planning work

The arc is **smaller than it looks**: it largely *finishes a correction the plant side already made*.

- **Practice-earns-knowledge is SHIPPED for rung 1, on both tracks.** Sustain-forage on a Thriving
  patch earns **Cultivation** (`CULTIVATION_DISCOVERY_ID = 2003`, `forage.rs:71`); Sustain-hunt on a
  Thriving herd earns **Herding** (`HERDING_DISCOVERY_ID = 2004`, `fauna.rs:60`) —
  `systems/labor.rs:131-135` and `:417-425`. Neither is start-granted (deliberate, `forage.rs:65-68`).
  Levers: `knowledge_progress_per_turn` 0.05 / `knowledge_completion_threshold` 1.0 (~20 turns) in both
  `labor_config.rs:150-153` and `fauna_config.rs:451-454`. **We extend this to rung 2; we do not build it.**
- **The plant side already de-conflated Sustain.** `plan_intensification.md:109`: "*Sustain no longer
  tames anything. It only teaches the faction Cultivation knowledge*", and `:107-108` removed the plant
  early-claim ("*it existed to skip the investment*"). `Cultivate` is already the direct, gated,
  Cultivate-shaped investment verb (`components.rs:971-982`, `server.rs:1569-1614`, `forage.rs:145-166`).
- **The animal side is the laggard — that is this arc's core fix.** `systems/labor.rs:417-425` still has
  ONE Sustain branch advancing **both** Herding knowledge **and** `accrue_domestication` (the §4.1
  conflation), and `domesticate` is still an **early-claim at `claim_threshold` 0.6**
  (`fauna_config.rs:416`) — the exact twin of the plant claim already removed. So: **apply the plant
  side's own correction to animals.**
- **Plant rung 3 does not exist.** Only forage → tended is built (grep: no `sow`/`germination`/
  `plant_crop` gameplay hits). In scope per the decision above.
- **No shared knowledge-gate helper** — the check is inlined 5× as `get_progress(faction, ID) >=
  threshold` (`labor.rs:194-196`, `:436-437`; `server.rs:1604-1605`, `:1647`, `:2737-2738`). A gate
  helper is the natural DRY seam for this arc.

## 0a. Reconciliation with `docs/plan_intensification.md` — the rulings

That doc owns the *content/pressure* arc (forage depletion, carrying capacity, sedentarization
plumbing, the yield-vector). **This doc owns the interaction + knowledge model.** Rulings:

| Topic | Ruling |
|---|---|
| Its animal path (`:161-169`) — "Sustain-hunt → accrue Domestication *and* Herding" | **Superseded.** Sustain teaches knowledge only; `Tame` fills the taming meter (§2, §4.1). |
| Its "**Herding gates only corralling**; mobile domestication stays **ungated**" (`:168-169`) | **Superseded.** One knowledge per transition: Herding gates `Tame`; **Penning** gates `Corral` (§4.3). |
| The animal `domesticate` early-claim (`claim_threshold` 0.6) | **Removed**, mirroring the plant early-claim it already removed for the same stated reason. |
| Its "preserve the **product asymmetry** — mobile livestock vs fixed patch; the asymmetry *is* the sedentarization pull" (`:133-140`) | **Upheld — no conflict.** This doc unifies the **grammar** (how you drive a rung), not the **product**. Rung 2 stays asymmetric: pastoral is *mobile* (nomadism keeps working), a tended patch is *a place*. Rung 3 pins **both** (Field, Pen) — which is exactly when you sedentarize. The ladder now *tells* that story rather than eroding it. |
| Its plant rung names — "Seed Germination" (`:152-159`) | **Superseded by the manual's vocabulary:** **Seed Selection** (knowledge) → **Field** (rung). See §2a. |
| Its rung-4 name "**Husbandry**" (`:262`) | **Rejected — name collision.** `husbandry` already names the whole animal subsystem in code (`HusbandryConfig`, `advance_husbandry`, `husbandry_ceiling`). Rung 4 is **Selective Breeding**. |
| Its corral flat-rate tuning (`:296-301`, self-described "a stopgap") | **Stale — already resolved** upstream by grazing 2d. |
| Its §§1-2 (depletion, forage parity), §5 (yield vector), §6 (carrying capacity), Phase-0 persistence | **Untouched and complementary.** |

## 2a. Settled vocabulary

Player-facing names come from the **manual** (authoritative), which already had most of them
("*Farming path: tending patches → seed selection → fields*" / "*Pastoral path: herd growth + corrals*").

| | rung 1 | rung 2 | rung 3 | rung 4 (future) |
|---|---|---|---|---|
| **Plants** | wild forage patch | **Tended Patch** — verb `Cultivate` *(shipped)*, knowledge **Cultivation** *(2003, shipped)* | **Field** — verb **`Sow`** *(new)*, knowledge **Seed Selection** *(new, id 2005)* | *(irrigation / rotation — unnamed)* |
| **Animals** | wild herd | **Pastoral** herd — verb **`Tame`** *(new; replaces the `domesticate` early-claim)*, knowledge **Herding** *(2004, shipped)* | **Pen** (manual: "corrals") — verb `Corral` *(shipped)*, knowledge **Penning** *(new, id 2006)* | **Selective Breeding** |

Next free discovery id is **2005** (existing: `nomadic_wayfinding` 2001, `portable_forge` 2002,
`cultivation` 2003, `herding` 2004). New ids need a `data/start_profile_knowledge_tags.json` mapping and
must **not** appear in any start profile's `starting_knowledge_tags` (nothing is start-granted).

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

## 8. Slice plan

- [x] **1 — Manual-first + reconciliation.** Manual §2a "The Intensification Ladder" landed (and the
  stale Wildlife & Hunting verb/policy list corrected); §0a rulings and §2a vocabulary settled.
- [ ] **2 — The rung engine + ladder config.** Extract today's bespoke rung logic into a generic engine
  driven by `intensification_ladder.json`; define the behavior primitive enums (§5). **Land it
  behavior-preserving first** (same rungs, same numbers, now data) so the refactor is separable from the
  design change. Fold in the **shared knowledge-gate helper** (retires the 5 inlined
  `get_progress(..) >= threshold` checks, §0). The load-bearing slice.
- [ ] **3 — Tame verb + worker-driven pastoral + proximity (animals).** Split the conflated Sustain
  branch (`labor.rs:417-425`): Sustain teaches **Herding** only; the new `Tame` verb fills
  `domestication_progress`. **Remove the `domesticate` early-claim** (`claim_threshold`). Retire
  passive-free pastoral → worker-driven-but-efficient. Add the `drift_to_owner` movement primitive.
- [ ] **4 — The knowledge pattern, rung 2.** Practicing rung 2 earns the rung-3 knowledge: `Tame` earns
  **Penning** (2006), `Cultivate` earns **Seed Selection** (2005). Gate reshuffle (§4.3). The
  **two-meter UI split** (faction knowledge vs per-source progress — the root UX fix, §4.1). Only
  stewardship policies teach (§4.2).
- [ ] **5 — Plant rung 3: Field + `Sow`.** New gameplay (not just parity): sow a **Field** on a chosen
  tile — a food source where none existed — gated on **Seed Selection**, riding the engine from slice 2.
  The animal twin of "place a source where you want it" is the Pen, so this completes the symmetry.
- [ ] **6 — Client.** Both new verbs + the two-meter split + the ladder readouts; ui_preview-verified.
- [ ] Parked (§6) as follow-on config rungs: secondary products, reliability, Selective Breeding (rung 4).

---

## 9. See also

- `docs/plan_grazing_2d.md` — the pen economy this sits on (self-feeding, per-species `r`, husbandry
  ceiling).
- `docs/plan_intensification.md` — the depletion → domestication → agriculture *content* arc this
  refines the interaction model of.
- `docs/plan_grazing_foundation.md` — the two-food-web foundation.
- `core_sim/CLAUDE.md` — "The husbandry yield ladder" (the flat-rung description this supersedes).
