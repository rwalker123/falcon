# Plan: Intensification — Resource Depletion → Domestication → Agriculture

> **⚠ PARTIALLY SUPERSEDED — read `docs/plan_intensification_ladder.md` first.**
> This doc remains authoritative for the **content/pressure** arc: forage depletion + parity (§§1–2),
> the yield-vector (§5), the carrying-capacity thesis (§6), and the Phase-0 persistence work.
> Its **interaction and knowledge model is superseded** by the ladder doc (see that doc's §0a for the
> full rulings):
> - **§4b's ladder and the animal path (~§"Sustain-hunt → accrue Domestication *and* Herding")** —
>   Sustain now teaches knowledge **only**; a direct, Cultivate-shaped **`Tame`** verb fills the taming
>   meter. The animal **`domesticate` early-claim is removed**, mirroring the plant early-claim this doc
>   already removed for the same reason.
> - **"Herding gates only corralling; mobile domestication stays ungated"** — replaced by one knowledge
>   per transition: **Herding** gates `Tame`, a new **Penning** gates `Corral`.
> - **Naming:** "Seed Germination" → **Seed Selection** (knowledge) + **Field** (rung), per the manual.
>   Rung 4 "Husbandry" → **Selective Breeding** (`husbandry` already names the whole animal subsystem).
> - **The corral flat-rate tuning** (self-described "a stopgap") is **stale** — resolved by grazing 2d.
>
> **Upheld, not superseded:** this doc's *product asymmetry* thesis (mobile livestock vs fixed patch =
> the sedentarization pull). The ladder unifies the **grammar**, not the product: rung 2 stays mobile
> (pastoral) vs rooted (tended patch); rung 3 pins both (Pen, Field) — which is when you settle.

Status: **Design draft, not yet implemented.** The authoritative spec for how a nomadic band is
pushed — mechanically, never by a scripted gate — from foraging/hunting into **cultivation and
pastoralism**, and thence toward settlement. It completes the Neolithic transition the game half-has
today: the **animal** path (hunt → husbandry → domesticated herd) already exists; this arc adds the
missing **pressure** (resource depletion) and the **plant** path (forage → cultivation → farming),
built as a near-mechanical transpose of systems already shipped.

## Motivation — why the opening has no reason to intensify

Two facts about the current sim collide:

1. **Herds are depletable; forage is not.** A herd has `biomass` / `carrying_capacity` /
   logistic regrowth and can be overhunted to collapse. A forage tile is a *static biome enum*
   (`FoodModuleTag`) with no stock — **inexhaustible**. The just-shipped yield telemetry even
   hard-codes `forage sustainable ≡ actual` ("no tile depletion in today's model").
2. **Nothing forces intensification.** Farming, historically, is a *worse deal per hour* than
   foraging until population pressure exhausts the easy resources. With inexhaustible forage there
   is **never a reason to take up cultivation** — or to settle.

We already learned (and reverted, PR #110's predecessor) that a **flat carry-capacity population
cap** is not the answer — an artificial ceiling that, grounded in labor, scales *with* population
and can never bind. The honest ceiling is **local resource depletion under population pressure**:
a growing band draws down the game and forage it can reach, income falls, and the band must **move,
intensify, or settle**. Depletion is the engine; this arc builds it and the responses to it.

## The core loop (the spine)

> population grows → draws down local **game and forage** → the easy resources thin out →
> the band must **move** (stay nomad), **intensify** (domesticate → livestock / farming), or
> **settle** (build place-bound tended patches / corrals it cannot carry).

Every mechanic below serves this loop. It is the emergent-carrying-capacity model that replaces the
scrapped carry-cap, and it is the Neolithic transition expressed as player-driven ambition, not a
button that unlocks.

## The model

### 1. Forage becomes a depletable resource (Phase 0 — "forage parity with hunting")

Transpose the herd's depletion machinery onto forage tiles. A worked patch gains:
- a mutable **`biomass`** + **`carrying_capacity`** (mirroring `Herd`), and
- per-turn **logistic regrowth** toward capacity (reuse `regrow_biomass` / `logistic_regrowth` /
  `net_biomass_delta`), including the **thriving / stressed / collapsing** ecology phases.

Foraging then **draws the stock down**; the patch regrows if worked within its sustainable rate and
**depletes** (eventually to a collapsed/feral state) if over-harvested. The yield instrument
shipped in PR #110 lights up for forage the moment this lands: `forage sustainable` stops being
`≡ actual` and becomes the real `net_biomass_delta`-based rate, so the **overdraw ⚠** the panel and
map already render starts flagging **over-foraging** exactly as it flags overhunting today. *No new
UI — the instrument was built forward-compatible for this.*

**Persistence note (load-bearing).** Depletion is meaningless if it resets on save/rollback — and
scoping this exposed a **latent bug**: `HerdRegistry` biomass is *not* actually snapshot-persisted or
rewound on rollback today either (only display telemetry is captured), so herd ecology silently keeps
its post-rollback value. Both are fixed by **one shared mechanism**: a serde `EcologyState` record
`{ biomass, carrying_capacity, ecology_phase, progress, owner }` that round-trips through the rollback
snapshot via the codebase's uniform capture/restore convention (the `DiscoveredSites` template).
**Herds** persist a `HerdState` (movement/identity **+** embedded `EcologyState`) — a full round-trip
that also closes the existing gap; **forage** persists a `ForageState` (= `EcologyState`, keyed by
tile) in a dedicated ecology `Resource` mirroring `HerdRegistry`. `FoodModuleTag` (the static worldgen
*classification*) stays as-is; the new mutable stock lives in the persisted ecology resource. **This
shared persistence foundation + the herd fix is Phase 0's first slice, landing ahead of the
forage-depletion mechanics.**

### 2. Policy parity for forage (Phase 0)

Hunting already carries a **policy-as-tradeoff** axis — Sustain / Surplus / Market / Eradicate vary
take amount, trade-goods generation, husbandry accrual, and depletion trajectory. **Forage gets the
same axis** (this is the entire point of "parity"):

| Policy | Forage behavior (mirrors the hunt semantics) |
|--------|----------------------------------------------|
| **Sustain** | Conservative gather at ≈ the patch's regrowth; patch stays Thriving; **builds cultivation** (see §3). |
| **Surplus** | More food now, slow patch decline. |
| **Market** | Gather for trade goods (multiplied), faster decline. |
| **Eradicate** | Strip the patch bare — max now, drives it feral. |

The exact policy set/semantics for gathering is a tuning question (Market/Eradicate map less
literally to plants than to game), but the **Sustain → builds domestication** rung is the essential
one — it is the seam the plant-domestication path hangs on, mirroring `Sustain-Hunt on a Thriving
herd → husbandry`.

### 3. Domestication transpose → cultivation (Phase 1)

The animal template is two fields on `Herd` — `domestication_progress: f32` (0→1) and
`owner: Option<FactionId>` — where progress accrues **only while a Sustain assignment works a
Thriving herd** (`progress_per_turn`) and decays otherwise (`decay_per_turn`). Domesticated =
**steady yield without depleting biomass** + **collapse-immune regrowth** + **counts toward
sedentarization**.

Transpose it onto forage patches as **cultivation** — `cultivation_progress` + `owner` on the (now
stateful) patch, reaching a tended patch (§4) at `progress ≥ 1.0`.

> **Amended in implementation — a free upgrade is not a decision.** The first cut had cultivation
> accrue *silently and for free under Sustain*, with a `claim_threshold` early-claim mirroring
> `domesticate`. That was wrong, and playtest killed it. The argument that cultivating "anchors you,
> and the anchor is the cost" does not hold: **foraging already requires a worker standing on that
> same tile**, so tending costs nothing extra — a free ~3× yield upgrade is *always* taken, and there
> is no choice to make. Cultivation is therefore an **explicit `Cultivate` policy with a real
> up-front cost** (and its animal twin, `Corral`):
>
> - **While preparing, the patch pays only `cultivating_yield_fraction × its Sustain/MSY ceiling`**
>   (0.25) — the crew is clearing and planting, not gathering. That dip *is* the investment. It is a
>   fraction of MSY, so it stays sustainable and the patch remains Thriving (which the accrual gate
>   needs) — the cost is forgone yield, not depletion.
> - **The early-claim is removed.** It existed to skip the investment, which is the whole point.
> - **Sustain no longer tames anything.** It only *teaches* the faction Cultivation knowledge (§4b).
>
> Break-even at the shipped defaults: ~7 provisions forgone over ~25 preparing turns, repaid ~8–9
> turns after the patch completes. **Cultivating is correct only if you intend to stay** — which is
> precisely the decision the free version erased, and precisely the decision this arc exists to
> create.

Still low-invention: the `HusbandryConfig` pattern (`progress_per_turn`, `decay_per_turn`,
`provisions_per_biomass`) re-instantiated for plants, plus the one new investment lever.

### 4. The place-bound payoff: tended patch + corral (Phase 1)

Domestication *completes* into a **place-bound improvement** — and this is where the arc plugs
directly into the **already-designed** improvement catalog in `plan_settlement_population.md`, which
names, as first catalog content, **"tended patches" (the farming path)** and **"corrals" (the
pastoral path)**:

- **Tended patch** — a completed cultivation. A place-bound improvement giving a **higher, steady
  yield without depletion** (like a domesticated herd's managed harvest), must be tended, and
  **decays if abandoned** ("your patch goes feral"). Gated on a `farming` knowledge tag, built/
  finalized through the settlement arc's `build` flow.
- **Corral** — the place-bound form of the *existing* herd domestication: pen a domesticated herd
  into a fixed improvement (higher managed yield, but anchored). Gated on a `herding` tag.

**The asymmetry we deliberately preserve** — the process is symmetric (Sustain → domesticate), but
the *product* differs and that difference is the settle mechanic:
- a domesticated **herd** can stay **mobile** → pastoralism travels with the band;
- a tended **patch** (and a corral) is **fixed** → it *pins* the band.

So plant domestication carries a sedentarization pull that mobile pastoralism does not. **This arc is
the bridge** that wires the new mechanics (forage depletion + cultivation) into the already-specced
improvement/settlement system — it does not reinvent it.

### 4b. The intensification ladder — earned knowledge, labor-tended tiers (refines §3–4)

Intensification is not a single step; it is an **earned tech ladder where you unlock the next tier by
*doing* the current one, and every tier's yield requires population tending the hex.** Knowledge is a
faction-level thing accrued through the activity (the same shape as Sustain-hunt already accruing
domestication) — never start-granted. Each rung raises output *and* deepens the anchor, so the settle
pull grows with intensification.

**Plant path:**
1. **Sustain-forage** a Thriving hex → accrue faction **Cultivation** knowledge (~20 turns). This is
   *all* Sustain earns — it never tames the patch itself.
2. Know Cultivation → **choose** the `Cultivate` policy on a Thriving patch and **pay the investment**
   (a reduced take for ~25 turns, §3) → it becomes a **tended patch**: higher output, but
   **worker-tended and place-local** (paid to the band that staffs it, near it), and it **goes feral**
   if abandoned.
3. **Tend patches** → accrue **Seed Germination** knowledge.
4. Know Seed Germination → **plant crops on arbitrary tiles** (not just existing forage) — higher
   output still, still worker-tended.

**Animal path (parallel, and mechanically the same shape):**
1. **Sustain-hunt** a Thriving herd → accrue **Domestication** on that herd *and* faction **Herding**
   knowledge → **choose** the `Corral` policy and pay the same kind of investment (a reduced take
   while the pen is built) → a **corralled herd**: higher yield, worker-tended, place-local →
   accrue **Husbandry** → …

**Asymmetry worth keeping straight:** Herding gates only *corralling*. Mobile domestication
(pastoralism) stays ungated — a herd you drive with you needs no place-binding knowledge, whereas a
patch cannot even begin to be tamed until the faction knows Cultivation.

**The load-bearing invariant:** a tier's yield is **place-local and requires a tending worker** — the
band that staffs the hex collects it, and an unstaffed improvement decays/goes feral. That is the
"pins the band" mechanic, and it is what makes intensification *cause* sedentarization rather than
merely correlate with it. (§3's per-patch `cultivation_progress` is the *local* "how tended is this
hex"; the ladder adds the *faction-level* earned knowledge that gates each rung.)

Build the ladder **one rung at a time** (see Phasing): the tended-patch mechanic (worker-tended /
place-local / feral) first, then the Cultivation-knowledge gate, then Seed Germination → crops, then
the corral/Husbandry rungs. The generic settlement `build`/footprint/decay *catalog* stays deferred
to the settlement arc — this arc delivers the earned, labor-tended food-tending ladder that plugs
into it.

### 5. The command yield-vector + pre-commit forecast (cross-cutting)

A command's output is not one number — it is a **vector** across dimensions (food + domestication/
cultivation progress + trade goods + vision/discovery), and **policy is the tradeoff dial** across
it. The hunt policies already vary four output dimensions; the arc generalizes this into a
first-class concept and surfaces it two ways:

- **Live** — the multi-dimensional output shown on the assignment (food is done in PR #110;
  husbandry/cultivation progress and trade goods are the next dimensions to surface).
- **Pre-commit forecast** — show the expected vector *at compose time*, before committing workers:
  "assign 4 here on Sustain → expect +0.46 food/turn, +2% cultivation/turn." This needs a
  **projection function** that mirrors the sim's yield math *without mutating state*, and it
  generalizes to every command (forage, hunt, expedition). It is what makes the intensification
  choice legible — Sustain visibly *trades food-now for domestication progress*, so the farming
  path is intentional rather than an invisible side effect.

The yield vector is, in effect, the **engine of this whole arc**: domestication happens *because*
Sustain's vector includes progress. Sequencing-wise the forecast is most valuable once policies
create real tradeoffs (i.e. alongside/after Phase 0), but the vector *model* underpins cultivation.

### 6. Depletion as the carrying-capacity mechanic

This replaces the reverted flat carry-cap. The population ceiling is **emergent**: a growing band
exhausts its reachable game + forage, sustainable income falls, and the band hits the move /
intensify / settle decision. Where the band wanders sets its natural ceiling (a rich valley
supports more than thin scrub) — making the **already-shipped wildlife/forage density overlay** a
true carrying-capacity instrument. Intensification (domestication) and settlement (tended patches)
are the *ways past* the ceiling, and both **feed the existing `SedentarizationScore`** (which
already has a `domestication` input; cultivation/tended-patch count plugs in as a driver, or feeds
the `surplus`/`resource_density` terms).

## Decisions & rationale

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Depletion is the engine; **local resource exhaustion under population pressure** is the carrying-capacity mechanic | Replaces the scrapped flat carry-cap with the honest driver; makes intensification/settling *rational* rather than gated. |
| 2 | Forage reaches **full parity with hunting** — depletable + the policy axis (Phase 0) | The plant path is a transpose of the animal path; parity is the prerequisite and the elegance. |
| 3 | **Mirror the herd resource model closely** (biomass / carrying_capacity / logistic regrowth / ecology phases) | The code is literally reusable; the parallelism *is* the design. |
| 4 | Cultivation = the **husbandry mechanic transposed** (`progress`/`owner`/gated accrual) | Lowest-invention path; a proven, clean template. |
| 4b | A higher tier must have a **real cost**, so it is an **explicit policy with an up-front investment** (`Cultivate`/`Corral`: a reduced take while preparing), not free accrual under Sustain. The `claim_threshold` early-claim is **removed** | "It anchors you" is *not* a cost when foraging already requires a worker on that same tile — a free 3× upgrade is always taken and there is no decision. The dip makes intensifying a bet on staying. (Corrected in implementation; see §3.) |
| 5 | Domestication *completes* into a **place-bound improvement** — tended patch (farming) / corral (pastoral) | These are already named in `plan_settlement_population.md`'s catalog; the arc **realizes** them, not reinvents. |
| 6 | **Preserve the product asymmetry** — mobile livestock vs fixed patch/corral | The asymmetry *is* the sedentarization pull; the honest nomad→settle bridge. |
| 7 | Depletable ecology state persists via **one shared `EcologyState` record**, fixing herds (a latent rollback gap) *and* enabling forage together | Herd biomass isn't actually rollback-persisted today either; a shared record + the codebase's uniform snapshot convention makes it a one-time foundation, not per-resource bespoke work. Phase 0's first slice. |
| 8 | **Command yield-vector + pre-commit forecast** as a first-class, cross-cutting concept | Makes policy-as-tradeoff (and thus the whole intensification choice) legible; forecasting generalizes to all commands. |
| 9 | Feed the **existing `SedentarizationScore`**, don't build a parallel gate | Settling is emergent ambition (the arc's philosophy + the settlement plan's), never a button. |

## Phasing (design → sequential impl slices, each its own PR)

0. **Forage parity with hunting** — lands in three focused slices:
   - **0-i · Shared ecology persistence + herd rollback fix.** The `EcologyState` record + snapshot
     round-trip; `HerdState` persists the authoritative `HerdRegistry` (closing the latent herd
     rollback gap). Self-contained, ships a real bug fix, lays the foundation.
   - **0-ii · Forage depletion.** A dedicated per-tile ecology `Resource` (reusing `EcologyState`,
     persisted) + a regrowth system (mirroring `advance_herds`/`regrow_biomass`) + draw-down in the
     forage yield arm + the real `net_biomass_delta`-based `sustainable` (auto-lights the over-forage
     ⚠ from PR #110's instrument).
   - **0-iii · Forage policy axis** (Sustain/Surplus/Market/Eradicate) — the five-site mirror of how
     Hunt carries its policy.
   *No cultivation yet — this just makes forage a real depletable resource with tradeoffs and turns
   on the pressure.*
1. **Cultivation + the intensification ladder (Phase 1) — SHIPPED.** The earned, labor-tended ladder
   of §4b, built one rung at a time:
   - **1 (shipped).** Cultivation transpose: `cultivation_progress`/`owner` on the patch, the
     `cultivate` command, folded sedentarization signal.
   - **1a (shipped).** **Tended patch = worker-tended + place-local + higher-output +
     feral-if-abandoned** — replaces the even-split passive yield with the "pins the band" mechanic
     (paid to the tending band; decays to wild if unstaffed).
   - **1b (shipped).** **Cultivation-knowledge ladder + gate** — Sustain-forage a Thriving patch
     accrues faction **Cultivation** knowledge (`DiscoveryProgressLedger`), which gates the
     `Cultivate` policy. **Amended from the original plan:** Sustain accrues *knowledge only*; the
     patch itself is tamed by paying the `Cultivate` investment (§3).
   - **1c (shipped).** **Corral** — pen a domesticated herd (place-local, worker-tended), behind an
     earned **Herding** knowledge gate and the same investment cost.
   - **Client rendering (shipped).** Both ladders on screen: cultivation/corral progress, the
     Cultivation/Herding knowledge meters, tended-patch + corralled indicators, per-source yields on
     the panel and the map, and locked rungs that name their remedy ("♻ Sustain-hunt this Thriving
     herd to finish taming it") rather than just their prerequisite.
   - **Later rungs — the next arc, not Phase 2.** **Seed Germination** → plant crops on *arbitrary*
     tiles (the payoff rung: today cultivation is place-*bound* — you upgrade a patch where nature put
     it; germination is what actually invents agriculture). Then **Husbandry** past the corral.
   Feeds `SedentarizationScore`. The generic settlement `build`/footprint/decay catalog stays with the
   settlement arc.
- **Cross-cutting: command yield-vector + pre-commit forecast — SHIPPED.** Per-source `actual` vs
  `sustainable` yields with an overdraw ⚠, an overstaffing signal, and a compose-time **"Expected
  yield"** (including "preparing X → then Y" for the investment rungs). The load-bearing invariant is
  **forecast == actual**: the forecast and the take path call the *same* pure helpers
  (`sustainable_yield` / `*_policy_ceiling` / `forecast_expected_take`), guarded by tests, so the UI
  cannot promise a number the sim won't pay. A fresh assignment is seeded from its own forecast, so it
  shows its real expected yield instead of `+0.00` before the turn resolves.
- **Deferred (documented):** the full improvement catalog (dwellings/storage/defense), larder
  spoilage + storage tiers, richer crop/livestock variety, the settlement-cluster derivation — all
  owned by `plan_settlement_population.md`; this arc delivers the food-tending seam that feeds them.

## Open tuning dials (settle live)

All config, per the no-magic-numbers convention. The ones that decide whether the arc *feels* right:

- **The investment bite** — `cultivating_yield_fraction` / `corralling_yield_fraction` (0.25: you
  keep a quarter of the source's Sustain yield while preparing) and `progress_per_turn` /
  `corral_build_progress_per_turn` (0.04 → ~25 turns). Together these are the price of intensifying.
  Too cheap and it's the free upgrade again; too dear and nobody ever settles.
- **The knowledge grind** — `knowledge_progress_per_turn` (0.05 → ~20 Sustain turns to learn
  Cultivation or Herding). This is the gate on the whole ladder.
- **The payoff** — `tended_provisions_per_biomass` / `corral_provisions_per_biomass` vs the wild MSY
  skim (the tended patch is currently ~3.2×). Keep
  `tended_provisions_per_biomass > regrowth_rate/4 × forage.provisions_per_biomass` or intensifying
  never pays. The **corral is anchored to Market**, deliberately: `corral_provisions_per_biomass` =
  `3 × market.take_fraction × hunt.provisions_per_biomass` = `3 × 0.20 × 0.02` = **0.012**, so a
  finished pen pays **3× the Market rate** — and pays it **sustainably** (a managed harvest, no
  biomass drawn down), where Market reaches its rate only by crashing the herd. **Residual, honestly:**
  that is still **~48× the Sustain (MSY) baseline** (Market is itself ~16× Sustain), because the pen
  and Market price a share of standing **stock** while Sustain prices regrowth **flow** — different
  denominators, not reconcilable by any choice of scalar. Measured at capacity (prov/turn): Red Deer
  K=1200 → Sustain 0.30 / Market 4.80 / build-dip 0.075 / penned **14.40**; Rabbit K=200 → 0.05 / 0.80
  / 0.0125 / **2.40**. **This flat rate is a stopgap.** The intended model is the corral as a *managed
  population* — its yield a function of the animal count, which is in turn a function of the food you
  feed it each turn (upkeep), turning the pen from a one-off 25-turn build that prints food forever
  into a sustained commitment with a running cost. Tracked in `TASKS.md` → **"Corral as a managed
  population (food upkeep → herd size → yield)"**; the flat-rate model above is what that arc replaces.
  See also `core_sim/CLAUDE.md` → Fauna & Wild Game → Corral (Intensification Rung 1c).
- Forage regrowth rate & carrying capacity (vs the herd equivalents); how literally Market/Eradicate
  map to gathering; the sedentarization weight for cultivation/tended-patch; and how aggressively
  depletion bites relative to band growth — the whole loop's pacing.

## See Also

- `plan_settlement_population.md` — the **improvement catalog** (tended patches / corrals / storage /
  dwellings), the `build` command, knowledge-gating, and decay-as-sunk-cost that this arc's payoffs
  plug into; and the `SedentarizationScore`-as-tether framing.
- `plan_wildlife_hunting_overlay.md` — the herd/hunt/**husbandry** template being transposed (the
  `domestication_progress`/`owner`/Sustain-accrual mechanic, Phase E), plus the density overlay that
  reads carrying capacity.
- `plan_early_game_labor.md` — the band-as-labor-pool + source-centric allocation (Forage/Hunt
  assignments and their policies) this arc extends; the **food ledger / yield instrument** (PR #110)
  that already renders actual/sustainable/overdraw and lights up for depletable forage for free.
- `core_sim/CLAUDE.md` — Fauna & Wild Game (husbandry/domestication, `HusbandryConfig`, ecology
  phases, `regrow_biomass`), the food-module classification, labor allocation & yield telemetry.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — §"Start of Game — Nomadic Default"
  and the Wildlife & Hunting material; the manual should gain the plant-domestication/farming
  counterpart to the existing pastoral framing (a finishing step once this design is agreed).
