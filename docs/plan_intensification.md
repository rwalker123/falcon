# Plan: Intensification — Resource Depletion → Domestication → Agriculture

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
Thriving herd** (`progress_per_turn`), decays otherwise (`decay_per_turn`), and a `claim_threshold`
lets the `domesticate` command finish early. Domesticated = **steady yield without depleting biomass**
+ **collapse-immune regrowth** + **counts toward sedentarization**.

Transpose it wholesale onto forage patches as **cultivation**:
- `cultivation_progress` + `owner` on the (now stateful) patch.
- Accrues while **Sustain-Forage works a Thriving patch**; decays untended; `claim_threshold` → a
  `cultivate` command (mirror `domesticate`) finishes early.
- At `progress ≥ 1.0` the patch is **domesticated** — it converts into a tended patch (§4).

Low-invention by construction: this is the `HusbandryConfig` pattern (`progress_per_turn`,
`decay_per_turn`, `claim_threshold`, `provisions_per_biomass`) re-instantiated for plants.

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
| 4 | Cultivation = the **husbandry mechanic transposed** (`progress`/`owner`/Sustain-accrual/`claim_threshold`) | Lowest-invention path; a proven, clean template. |
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
1. **Cultivation + Corral.** Transpose domestication to plants (`cultivation_progress`/`owner`/
   Sustain-accrual/`cultivate` command); land the **tended-patch** (farming) and **corral**
   (pastoral) place-bound improvements — realizing the settlement arc's food-tending improvement
   class, knowledge-gated (`farming`/`herding`), built/tended/decaying. Pulls forward a slice of the
   settlement arc's `build`/improvement system. Feeds `SedentarizationScore`.
- **Cross-cutting: command yield-vector + pre-commit forecast.** Model the multi-dimensional output;
  surface husbandry/cultivation progress + trade goods live; add the compose-time forecast. Lands
  alongside Phase 0/1 (forecast is most valuable once policies create tradeoffs).
- **Deferred (documented):** the full improvement catalog (dwellings/storage/defense), larder
  spoilage + storage tiers, richer crop/livestock variety, the settlement-cluster derivation — all
  owned by `plan_settlement_population.md`; this arc delivers the food-tending seam that feeds them.

## Open tuning dials (settle live)

Forage regrowth rate & carrying capacity (vs herd equivalents); the forage policy set/semantics
(how literally Market/Eradicate map to gathering); cultivation `progress_per_turn` / `decay_per_turn`
/ `claim_threshold` (vs husbandry's 0.04 / 0.01 / 0.6); tended-patch yield vs a wild patch; the
sedentarization weight for cultivation/tended-patch; how aggressively depletion bites relative to
band growth (the whole loop's pacing). All config, per the no-magic-numbers convention.

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
