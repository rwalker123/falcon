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
  *(Slice 4 did exactly that: both hard-coded earn sites are gone, replaced by the one rung-driven
  `RungDef::knowledge_earned` seam, and the duplicated levers moved onto the ladder's `knowledge`
  block — see §8 slice 4.)*
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
| Its "plant crops on **arbitrary** tiles" (`:152-159`) | **NOT superseded — it moves to rung 4** (corrected 2026-07-16). Rung 3 (`Sow`) places a Field only on **naturally food-bearing** ground; making unwilling ground farmable is **Worked Land**, rung 4. The earlier ruling here was wrong: it conflated "place a source" with "place it anywhere". |
| Its rung-4 name "**Husbandry**" (`:262`) | **Rejected — name collision.** `husbandry` already names the whole animal subsystem in code (`HusbandryConfig`, `advance_husbandry`, `husbandry_ceiling`). Rung 4 is **Selective Breeding**. |
| Its corral flat-rate tuning (`:296-301`, self-described "a stopgap") | **Stale — already resolved** upstream by grazing 2d. |
| Its §§1-2 (depletion, forage parity), §5 (yield vector), §6 (carrying capacity), Phase-0 persistence | **Untouched and complementary.** |

## 2a. Settled vocabulary

Player-facing names come from the **manual** (authoritative), which already had most of them
("*Farming path: tending patches → seed selection → fields*" / "*Pastoral path: herd growth + corrals*").

| | rung 1 | rung 2 | rung 3 | rung 4 (future) |
|---|---|---|---|---|
| **Plants** | wild forage patch | **Tended Patch** — verb `Cultivate` *(shipped)*, knowledge **Cultivation** *(2003, shipped)* | **Field** — verb **`Sow`** *(new)*, knowledge **Seed Selection** *(new, id 2005)*; **only on naturally food-bearing ground** | **Worked Land** — make unwilling ground farmable (irrigation / clearing / terracing). The true "arbitrary tiles" capability. |
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

## 2. The unified model: parallel ladders, one grammar

Both food webs climb the same shape (3 shipped rungs + a 4th deepening the mastery):

| rung | plants | animals | what you control |
|---|---|---|---|
| 1 — wild | forage patch | wild herd | nothing — take what's there |
| 2 — tended/tamed | tended patch | pastoral herd | you *manage the wild source in place* |
| 3 — placed | **Field** — sown on *already food-bearing* ground | corralled **Pen** | you control its *reproduction* **and its location** |
| 4 — mastery *(future)* | **Worked Land** — irrigate/clear, and unwilling ground takes seed | **Fodder / Hay** — grow the feed, and the pen stops depending on its tile · *(**Selective Breeding** — better stock — is a separate rung-4 escape: *what*, not *where*)* | you stop being limited by *where* (both webs) / by *what* (animals) |

**The land decides; the rung only multiplies.** A pen's `K` is **its footprint's graze flow ÷ fodder** (2d);
`r` is the rung's (pen = 3× wild); the pulse is `body_mass ÷ (r·K/4)`. **Both terms matter, and the land
is the one that can go to zero.** A cattle pen on an alpine mountain grows nothing: `K` collapses,
`pasture_fraction` → 0 so you pay the *full* larder bill every turn, and the herd starves to the
extinction floor — the pen makes it *worse*, because now you're feeding it too. So "pens are steady" is
**false as stated**: a pen on good land slaughters continuously, a pen on poor land pulses, a pen on dead
land is a hole you pour food into. (This doc has repeatedly mistaken a measurement taken on good pasture
for a property of pens. It isn't one.)

**Rung 4 is where the two food webs finally couple.** They have been strictly parallel since 2b — human
`ForagePatch` vs animal `GrazePatch`, two stocks that never touch. **Fodder is the moment your fields feed
your herds**: the pen's ceiling stops coming from its tile and starts coming from your farming. That is
mixed farming, it is historically the step that let livestock leave the pasture, and it is a far bigger
deal than a rung-4 stat bump — it makes the plant ladder a *prerequisite* for the animal one's top rung.
Design it as a coupling, not as a lever.

Every rung-transition is a **Cultivate-shaped verb**: pick it → **lower** yield while you work (the
gentle-policy cost) → **per-source build meter** climbs → decays if you abandon it → on completion the
source steps up a rung. Plants run it twice (Cultivate → Sow); animals run it twice (Tame → Corral).

**Rung 3 places, but does not conjure — and scarcity is the point.** Rung 3 is *"I know how to take
seed from a plant and put it somewhere else — but I don't know fertilization yet, so the land must be
**very fertile already, near sources of water**"*. `Sow` puts a Field only on that ground: the
floodplain, the river delta, the alluvial valley. It can create a source where none existed (a
qualifying tile with no spawned forage site), but it cannot farm thin ground or dry ground; that is
rung 4 (**Worked Land** — plows and irrigation), which relaxes exactly those two constraints.

**Few sowable tiles is the mechanic, not a side effect**: it means *which* tile matters, and a band may
have to **move** to farm at all. Measured on the standard map (earthlike 80×52, seed 119304647): **46
sowable tiles of 4160 (1.1%)** — AlluvialPlain 31 + RiverDelta 15 — against 2328 tiles that merely bear
food. (An earlier cut of this spec said "any tile with non-zero forage capacity", which was **56% of the
map**: the constraint did no work and rung 3 collapsed into "tended but 2×".) So rung 3 **pulls you into
the river valleys** — which is exactly the sedentarization pull — and only rung 4 frees you from them.
Real order: gather → tend wild stands → plant the floodplains → irrigate the desert.

**Where the two webs legitimately differ** (grammar is unified; products are not):
- **Plants create from nothing at rung 3; animals must climb.** Seed travels; a herd you never tamed
  does not. `Sow` needs no prior patch; `Corral` needs a pastoral herd. **But plants are choosy about
  the *land* where animals are choosy about the *species*** — `site_requirement` is the plant twin of
  `ceiling_required`: a herd carries its site with it, a field cannot.
- **There is no "extend the tended patch".** The pen needs `ExtendPen` because ONE herd has ONE
  appetite and needs more grazing land. A patch has no such problem — you don't extend a field, **you
  sow another field**. Each tile is its own patch, so expansion is already free-form.

---

## 3. Every rung is worker-driven; intensifying raises four dials

**Decision (settled):** the shipped "passive-free pastoral" rung is **retired**. A tamed herd is not
"set and forget" — you still work it with people, it's just more productive per worker. This matches
plants (a farm isn't passive) and keeps every rung engaged. Intensifying up the ladder raises:

1. **Yield per LAND — density, not per-worker freedom.** *(Corrected slice 7 — see §3a; the original
   claim here was that intensifying raises yield **per worker** and so "buys freedom". Measurement
   refuted it.)* A rung raises what a patch of ground produces (0.61 → 0.91 → 3.90 on AlluvialPlain
   K=195) and how many workers it can usefully employ (2 → 3 → 10). It does **not** raise food per
   person: **production and collection are separate**, and collection is capped by
   `per_worker_biomass_capacity`, which the rungs never touch.
2. **Regeneration rate (`r`)** — managed/bred/seeded stock refills faster. This is the per-species
   husbandry-`r` ladder 2d already built (wild → ×`pastoral_gain` → ×`pen_gain`).
3. **Carrying ceiling (`K`)** — on good pasture the pen's footprint raises K (2d already).
4. **Proximity** — the spatial spine, animals only (plants can't move):
   - **Wild:** roams its full range — you chase it across tiles.
   - **Tamed (pastoral):** the herd **drifts toward the owning band** and stays near — less chasing =
     higher effective yield, and it *reads* as domesticated.
   - **Penned:** range collapses to the fence — zero chasing, fully fixed.

## 3a. Intensifying buys density; tools buy freedom (settled slice 7)

**The thesis this arc started with was wrong, and measurement caught it.** §3 originally claimed
intensifying raises yield *per worker* — "that's how you buy freedom". Slice 7 separated **production**
(what the ground offers) from **collection** (`workers × per_worker_biomass_capacity`, the cap the wild
path always had and the managed path skipped). With both modelled, the per-worker ladder collapses:

| | per-worker yield | what the rungs actually buy |
|---|---|---|
| **Plants** wild / tended / Field | **0.40 / 0.40 / 0.40** — flat; the cap already bound at **rung 1** | production 0.61→0.91→3.90; usefully-employable workers 2→3→10 |
| **Animals** Boar wild / pastoral / pen | 0.50 / 0.75 / **0.80** — the pen step is nearly gone (×1.07) | total production; only small game (Rabbit) stays under the cap |

**The settled model:**
- **Intensifying buys DENSITY** — more food per *tile*, which is why a band stops walking and settles.
  (Historically right: early farming was *more* labor per calorie than foraging; it won on calories per
  acre.) This is the same instinct as [[scarcity-drives-the-real-decision]] — the land is what varies.
- **TECHNOLOGY buys FREEDOM** — `per_worker_biomass_capacity` (forage 8 / hunt 40) is the only lever
  that frees hands, and nothing on the ladder touches it. **This makes the M1 equipment/TOE arc
  load-bearing**: baskets, granaries and plows are what turn density into surplus labor.

**Why not just raise the caps (the rejected option B):** measured — `hunt.per_worker 40 → 100` *would*
fully restore the animal ladder (×1.5 / ×2.0). But the **plant Field is unreachable by that lever at
any sane value**: a Field produces `0.02 × K` = 3.90, so per-worker only clears tended at
`forage.per_worker ≥ 78` (~10× today). Restoring it would mean cutting
`field_provisions_per_biomass` — trading away the Field's whole reason to exist. Option B was not just
worse, it was partly *impossible*. It would also defeat the waste mechanic: if nothing ever exceeds
carry, nothing is ever wasted.

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
  "ceiling_required": "pen",               // husbandry_ceiling gate — WHICH SPECIES may climb
  "site_requirement": null,                // WHAT LAND it may be placed on (the plant twin; null = anywhere)
  "build": { "progress_per_turn": …, "decay_per_turn": …, "yield_fraction_while_building": … },
  "effects": { "regrowth_mult": 3.0, "yield_per_worker_mult": …, "ceiling_mult": … },
  "behavior": { "movement": "fixed", "feeding": "self_graze", "harvest": "worker_tend" } }
```

**`site_requirement` is where the *scarcity* of a rung lives**, and it is why rung 4 is a config edit.
`plant:field` declares `{ min_forage_capacity: 195, requires_fresh_water: true }` — rung 3 moves seed
but cannot fertilize, so the land must do it. **Worked Land (rung 4) is that record, looser**: a lower
floor and `requires_fresh_water: false`. Nothing else changes.

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
  more efficient). A deliberate re-tune. ⚠ **This bullet's original claim — "the 'buy freedom' thesis is
  preserved via yield-per-worker" — was refuted in slice 7; see §3a.** The rungs buy *density*; tools
  buy freedom.
- **Adding a rung-2 knowledge gate** (Herding now gates Tame) means taming is no longer ungated —
  paces the ladder to practice. Intended.
- The husbandry-ceiling work from 2d ([[grazing-2d-pen-economy]]) already provides the per-species
  applicability gate this model relies on.

---

## 8. Slice plan

- [x] **1 — Manual-first + reconciliation.** Manual §2a "The Intensification Ladder" landed (and the
  stale Wildlife & Hunting verb/policy list corrected); §0a rulings and §2a vocabulary settled.
- [x] **2 — The rung engine + ladder config.** **Landed, behavior-preserving** —
  `core_sim/src/data/intensification_ladder.json` + `core_sim/src/intensification.rs` (`LadderConfig` /
  `RungDef` / `validate()` / the `RungDef::build_accrual`-`build_decay`-`yield_fraction_while_building`
  seam / the `knows` gate helper). Plant `tended` (Cultivate) and animal `pen` (Corral) are migrated onto
  the engine and their build dials moved into the ladder verbatim; animal rung 2 stays bespoke for slice
  3. Behavior primitives parse + validate, nothing reads them. Spec: Extract today's bespoke rung logic into a generic engine
  driven by `intensification_ladder.json`; define the behavior primitive enums (§5). **Land it
  behavior-preserving first** (same rungs, same numbers, now data) so the refactor is separable from the
  design change. Fold in the **shared knowledge-gate helper** (retires the 5 inlined
  `get_progress(..) >= threshold` checks, §0). The load-bearing slice.
- [x] **3 — Tame verb + worker-driven pastoral + proximity (animals).** **Landed in two slices.**
  **3a:** the conflated Sustain branch is split (Sustain teaches **Herding** only; the new `Tame` verb
  fills `domestication_progress` and pays the rung's investment dip) and the `domesticate` early-claim
  (`claim_threshold`) is **removed**. **3b:** **passive-free pastoral is retired** — a tamed herd
  yields *only* through a worker's Hunt assignment, and the payoff is **yield per worker**: the
  existing `herd_ecology` seam already puts it on the pastoral `r` (wild × `pastoral_gain` 1.5), so the
  same crew takes ~1.5× the food from the same `K` (the `worked_this_turn` no-double-pay flag went with
  the payout it guarded). The **`drift_to_owner`** movement primitive is built and
  `behavior.movement` is wired from the rung record — **the first primitive the engine reads**, so the
  behaviour change ships as a config diff (`animal:pastoral` → `movement: drift_to_owner`, `harvest:
  worker_take`). Flagged for playtest: a herd that prefers proximity may settle on poorer pasture near
  camp, lowering its `K` — real pastoral overgrazing, floored (it cannot strip the range) by 2b-ii's
  escapement.
- [x] **4 — The knowledge pattern.** **Server landed.** `RungDef::knowledge_earned` is the one earn
  seam: it reads the rung **the source currently stands on** and credits that rung's
  `earns_knowledge`, retiring both hard-coded per-web `Sustain && Thriving → <ID>` branches — so
  `earns_knowledge` went from declarative (slice 2) to **live for every rung, including the wild
  ones**. Working a **pastoral** herd earns **Penning** (2006), a **tended** patch **Seed Selection**
  (2005) — note the rule keys off the *rung*, not the verb, so the same Sustain hunt teaches Herding
  on a wild herd and Penning on a tamed one. Gate reshuffle (§4.3) done: `animal:pen` moved
  `herding` → `penning` (and `extend_pen`, riding the same rung, with it), so Herding gates `Tame`
  alone — one knowledge per transition, pinned by a "no two rungs share an unlock gate" assertion.
  Only stewardship teaches (§4.2) via `FollowPolicy::teaches_knowledge`, defined against the
  `EXTRACTIVE` grouping: Sustain + the investment verbs teach, Surplus/Market/Eradicate never do. The
  `Thriving` gate and rung-1 behaviour are unchanged. **DRY dividend:** the duplicated
  `knowledge_progress_per_turn`/`knowledge_completion_threshold` (identical in `labor_config` *and*
  `fauna_config`) collapsed onto the ladder's new `knowledge` block, their validate bounds with them.
  **Measured pacing consequence (intended):** a pen is now a **~97-turn, four-leg climb** for a Wild
  Boar — Herding 20 → Tame 32 → **Penning 20 (the new leg)** → Corral 25 — vs ~77 before.
  *Remaining:* the **two-meter UI split** (§4.1) is client-side (slice 6); the server exports both
  meters distinctly (`IntensificationKnowledgeState` gained `seedSelection`/`penning`, append-only).
- [x] **5 — Plant rung 3: Field + `Sow`.** **Server landed.** A **Field** is a `ForagePatch` at rung 3
  (its own `field_progress` meter beside `cultivation_progress`, exactly as a herd carries
  `corral_progress` beside `domestication_progress`), driven by the slice-2 engine and gated on **Seed
  Selection** — the consumer slice 4 earned that knowledge for. **Placed, not conjured:** `sow` is legal
  only on naturally food-bearing ground (`tile_forage_capacity > 0` — the same helper that sizes a wild
  patch), and refuses rock/ice/desert with a rejection that points at **rung 4**. It needs no prior patch
  (seed travels) — the create-from-nothing path is live (`ForagePatch::sown`: the tile's own biome
  capacity, biomass at the reseed floor). **Sow's accrual is deliberately NOT gated on Thriving** (unlike
  Cultivate): sown ground starts at the reseed floor, i.e. Collapsing, so a health gate would forbid the
  case the rung exists for — the same line `Tame` draws. **Payout:** `biomass ×
  field_provisions_per_biomass` (0.02 = **2× tended**; measured on one tile at one biomass, K 130 / B 65:
  wild Sustain 0.41 → tended 0.65 → **Field 1.30**). **Feral:** one rule for the whole plant web — an
  untended patch bleeds *both* meters, so an abandoned Field reverts to **wild**, not to a free tended
  patch. `requires_rung` kept `tended` and `validate()` untouched: it states where the rung *sits on the
  ladder*, never a per-source precondition (nothing reads it as one — each verb's own gate does that,
  and that is exactly where the two webs differ).
  - **Slice 5b tightened the site rule — the original "any tile with non-zero forage capacity" was
    56% of the map and did no work.** The rung now carries a **`site_requirement`** on its record
    (`{ min_forage_capacity: 195, requires_fresh_water: true }`, `RungSiteRequirement` — the plant twin
    of `ceiling_required`, keyed on the land): the fertility floor admits the river-deposit class
    (RiverDelta 210 / Floodplain 205 / AlluvialPlain 195) and stops just above ordinary MixedWoodland
    (190); the water rule wants a river along one of the hex's sides, fresh-water ground, or a
    lake/channel/marsh next door — **salt coast does not count**. **Measured: 46 sowable tiles of 4160
    (1.1%)**, AlluvialPlain 31 + RiverDelta 15, against 2328 food-bearing. **The conjunction is doing
    the work**: 337 clear the floor, and the water rule cuts 291 of them (86%) — so the water check is
    **not** redundant and stays. Refusals name the fault (too poor / too dry / both) and point at rung
    4. **Rung 4 (Worked Land) is now a looser copy of this one record** — the config-driven thesis paying
    out.
  - **MEASURED CAVEAT — the create-from-nothing case cannot occur on a generated map.**
    `classify_food_module` tags essentially every biome and `spawn_initial_forage` seeds a patch on every
    module tile with positive capacity, so **every** food-bearing tile already carries a `ForagePatch`
    (standard map: **2328 food-bearing tiles, 2328 patches, zero bare**). `Sow` therefore always
    *upgrades* an existing wild patch today; the "qualifying tile with no spawned forage site" of §2
    above does not exist. (The CLAUDE.md claim that "~95% of tiles carry no `ForagePatch`" was **stale**
    — it predates the per-biome capacity table — and is **corrected**.) The path is built and tested
    against a constructed bare tile. Still an open design call: make forage sites genuinely sparse, or
    accept that rung 3's freedom is "choose *which* qualifying tile" (which the tightened site rule now
    makes a real choice — 46 tiles, not 2317).
  - **Slice 6a exported the plant ladder to the wire** (append-only, `ForagePatchState` slots 36–44):
    `fieldProgress` + `isField` (beside the already-shipped `cultivationProgress`/`isCultivated`, so
    the client has both meters for the §4.1 split), `ceilingSow` + `fieldYield` (Sow's preparing→payoff
    pair), and **`sowSiteRefusal`** — `""` / `"too_poor"` / `"too_dry"` / `"too_poor_and_too_dry"`,
    resolved through the same `RungSiteRequirement::refusal` seam the command gates on. Shipping the
    *reason* rather than a bool is what makes 46-of-4160 legible: the client can't re-derive it, and
    "why can't I sow here?" must not be answered by making the command fail.
  - *Remaining:* the client (slice 6) — the native reader surfaces none of the five new fields, and no
    panel renders the two-meter split or the refusal.
- [x] **6 — Client.** **Landed (slice 6b).** The decoder was dropping **all five** slice-6a plant
  fields *and* slice-4's `seedSelection`/`penning` — present in the schema and bindings, absent from
  the dicts, arriving as zeros (the third time `native/src/lib.rs` has silently eaten appended
  fields); all seven now decode and MapView cross-refs the plant five onto `tile_info` as `patch_*`.
  **The two-meter split (§4.1)** is enforced by SURFACE, not styling: faction knowledge lives *only*
  in the top-bar strip, prefixed **"⚒ Your people know:"** and now carrying all four tracks; a
  source's own build meters live *only* in its drawer rows; the single place they meet is a gated
  verb's reason line, which pairs a KNOWLEDGE reason (fixed by **practice** — "Your people know
  Penning 45% — ♻ Sustain-hunt a tamed herd to learn it") with a SOURCE reason (fixed by the
  **verb** — "This herd is 40% tamed — ◎ Tame it to finish"). `Tame` (◎) and `Sow` (▦) ship as the
  6th option on their pickers — **as policies on the existing `assign_labor` path**, so no new
  command wiring; the standalone `tame`/`sow` verbs stay unused by the client. Sow's refusal is
  rendered as an *answer* per fault (too poor / too dry / both), never a failing button. The `Field`
  gets its own row beside `Cultivation` ("Sowing N%" → "▦ Field"), reading as a different thing from
  a Tended Patch. Stale copy retired: Sustain's "the hunt also tames it" + "pays food every turn
  without being hunted down", the `domesticate` reference, and Corral's Herding gate (now Penning).
  Added `_tame_stalled_hint` for the pause-not-gate Thriving rule. ui_preview: `two_meter_split`,
  `herd_tame`, `herd_tame_stalled`, `forage_sow`, `forage_sow_locked`, `forage_sow_too_dry`,
  `forage_sow_too_poor`, `forage_field_building`, `forage_field`, `herd_corral_locked{,_both}`.
  - *Remaining server-side:* **`Tame` has no quotable payoff on the wire.** Its dip rides
    `huntPolicyCeilings` (a 6th `tame` row — fine), but there is no `pastoralYield` twin of
    `tendedYield`/`corralYield`/`fieldYield`, and structurally there cannot be one today: 3b made the
    payoff a faster `r` a worker must still harvest, not a managed rate. So Tame alone cannot render
    the "preparing X → then Y" pair its three sibling rungs do — the client shows the real dip and
    states the payoff in words rather than fabricating a number. A `pastoralYield` (or an honest
    per-worker before→after) would complete the symmetry.
- [x] **7 — Production vs collection, and the plant rung-2 policy axis.** **Server landed.** Out of
  playtest: on a completed Tended Patch *every* policy forecast the identical +0.66/turn. Two
  pre-existing defects (`37e84d6`/`0df436a`, predating this arc), both now fixed:
  - **The plant web collapsed a rung early.** Rung 2 paid a flat `tended_provisions_per_biomass ×
    biomass`, policy-blind and never drawn down — a *managed* rate where the animal side's rung 2
    (pastoral) has always been a **boosted curve** you still hunt under the full policy axis. **Fixed by
    making tended the plant twin of pastoral**: the retired rate becomes **`tended_regrowth_gain`**
    (1.5, `labor_config.json` — mirroring `husbandry.pastoral_gain`'s home *and* its value), folded in
    by the new **`forage::patch_ecology`** (the plant twin of `fauna::herd_ecology`, and the one seam
    every consumer resolves a patch's ecology through). Rung 2 now flows through the ordinary
    `forage_take`: policy-live, worker-capped, drawn down — so **a tended patch can be over-farmed and
    its overdraw ⚠ can finally fire** (`sustainable == actual` was previously true by construction).
    *Chosen over "keep a managed rate as the Sustain ceiling and derive the other policies from it"*
    because that keeps a second, parallel yield model on the plant web forever; a gain **deletes** the
    special case — rungs 1 and 2 become the same code path with a different `r`.
  - **The managed harvest ignored worker collection capacity.** The wild path already separated
    **production** (what the source offers) from **collection** (`workers × per-worker throughput`) and
    paid the `min`; the managed branch skipped it, so **one worker collected everything the land
    offered**. Now applied at rung 3 too — on the **Field** *and* (confirmed: it had the same hole) the
    **Pen**. `TENDED_SOURCE_WORKERS_NEEDED = 1` is **retired**; `workers_needed` is derived everywhere.
    **Rung 3 collapses the POLICY axis, never the worker cap** — you always carry the harvest home.
  - **`wastedYield`** (append-only on `LaborAssignment`) is the new **understaffing** signal, the exact
    mirror of `workersNeeded`'s overstaffing one: `production − actual`. *Client renders it — slice 8.*
  - **The forecast's "preparing → then" copy bug is fixed.** `SourceYieldForecast::tended` set *every*
    ceiling to one number, so a completed rung-2 patch quoted "preparing 0.66 → then 0.66". It is now
    `::managed`, used by **rung 3 only** (where "nothing left to build" is true), and a tended patch
    forecasts policy-live. **Client keys off:** `ceilingCultivate → tendedYield` on a *wild* patch and
    `ceilingSow → fieldYield` on a *tended* one — `tendedYield` now means **"the Sustain skim once
    tended"** (rung 2's payoff on the boosted curve), not a managed rate.
  - **MEASURED — the ladder stays monotone** (`AlluvialPlain`, K 195, production/turn): wild **0.61** →
    tended **0.91** → Field **3.90**, needing **2 / 3 / 10** gatherers at 0.40 prov/worker.
  - **MEASURED CAVEAT — the per-worker ladder is FLAT on plants and now CAPPED on animals. Reported,
    not retuned.** §3's dial 1 ("intensifying raises yield per worker") does **not** hold at today's
    numbers. On **plants** it never did: one gatherer carries 0.40/turn while even a *wild* patch's MSY
    is 0.61, so the cap already bound at rung 1 — the plant payoff is **total production per tile**
    (and so how many workers a tile can usefully employ: 2 → 3 → 10). On **animals** slice 7 makes the
    cap bind where it did not: a hunter carries 0.80/turn, so a **Wild Boar** pen (production 1.57)
    now needs **2** keepers and pays **0.79/worker — the same as pastoral**, collapsing 3b's 2×
    per-worker step (wild 0.53 → pastoral 0.79 still holds at 1.5×); a **Red Deer** pen (2.03) needs 3.
    Only small game keeps the full ladder (a **Rabbit** pen, 0.61, still fits one hunter). The pen
    remains strictly better in **total** yield, and un-collected production is **not** slaughtered — it
    stays on the hoof above the `K/2` escapement point, which is stable from above. **The lever is
    `hunt.per_worker_biomass_capacity` (40) / `forage.per_worker_biomass_capacity` (8)** if the
    per-worker thesis is to be restored.
  - *Remaining:* the **client** (slice 8) — nothing renders `wastedYield`, and the tended patch's card
    still needs to show four policy rows rather than one number.
- [ ] **Rung 4 (future, own arc): Worked Land** (plants) — irrigation / clearing / terracing makes
  unwilling ground farmable; this is where `plan_intensification.md`'s "plant on **arbitrary** tiles"
  actually lives. Its animal twin is **Selective Breeding**. Both should be config-shaped rungs on the
  slice-2 engine; Worked Land likely needs one new behavior primitive (mutating a tile's suitability).
- [ ] Parked (§6) as follow-on config rungs: secondary products, reliability.

---

## 9. See also

- `docs/plan_grazing_2d.md` — the pen economy this sits on (self-feeding, per-species `r`, husbandry
  ceiling).
- `docs/plan_intensification.md` — the depletion → domestication → agriculture *content* arc this
  refines the interaction model of.
- `docs/plan_grazing_foundation.md` — the two-food-web foundation.
- `core_sim/CLAUDE.md` — "The husbandry yield ladder" (the flat-rung description this supersedes).
