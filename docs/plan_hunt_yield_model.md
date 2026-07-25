# Plan: The Hunt Yield Model — product × intensity

Status: **Design — not yet implemented.** Tracked as issue **#337**. The authoritative spec for decoupling *what a hunt yields*
(the species) from *how hard you hunt* (the policy). It is the answer to the long-deferred **"Hunt
policy payoffs"** arc (issue **#213**; the open question of "what are Market's trade goods and
Eradicate's denial ultimately *for*"), and it is the general fix behind a specific predator wart the
Predators arc surfaced: **a wolf offers food-harvest policies (Sustain/Surplus) even though you do not
eat wolves.**

## The core realization — the policy axis secretly bundles two orthogonal things

Today a `FollowPolicy` (Sustain / Surplus / Market / Eradicate) decides **both** how much of the
animal you take **and** what you get for it:

| policy | intensity (rate) | product (today) |
|---|---|---|
| **Sustain** | MSY (`r·K/4`, stable at `K/2`) | **food** (+ husbandry accrual) |
| **Surplus** | `> MSY` (slow decline) | **food** |
| **Market** | `≫ MSY` (fast decline) | **trade goods** |
| **Eradicate** | the whole standing stock (extinction) | **nothing** (denial) |

The product is welded to the policy — Sustain *means* food, Market *means* trade. That is why a
**predator breaks it**: a wolf has no food product, so Sustain/Surplus are nonsense on it, while Market
(pelts → trade) and Eradicate (cull the competitor/threat) fit perfectly. The screenshot that motivated
this — a Grey Wolf Pack offering all four food-and-trade policies — is that conflation showing through.

**The fix is to separate the two axes and make them orthogonal:**

- **PRODUCT is a property of the SPECIES** — a wolf yields pelts (trade), a deer yields meat + hide
  (food + trade), a mammoth yields meat + ivory (food + a lot of trade).
- **INTENSITY is a property of the POLICY** — the four policies become a pure *harvest-pressure
  ladder*: how much of the stock you take per turn, from sustainable to extinct.
- **A hunt's yield = the take (set by the policy's rate) run through the species' product vector.**
  Every harvesting policy pays the species' products; only the *rate* differs.

Under that split the wolf falls out for free: `edible = false, tradeable = true` ⇒ every harvest policy
yields **pelts only**, at the policy's rate, and Eradicate culls them out.

## What is genuinely new

### 1. A per-species HUNT-YIELD VECTOR (the product)

Move the two conversion rates that are **global** today onto `SpeciesDef`, as a vector — exactly the
shape the **flora roster** already uses for plants (`provisions_per_biomass` / `fodder_per_biomass` /
`trade_goods_per_biomass`, `docs/plan_flora_roster.md`):

```
SpeciesDef.hunt_yield = {
    provisions_per_biomass:   f32,   // FOOD — meat. 0 ⇒ inedible.
    trade_goods_per_biomass:  f32,   // TRADE — pelt / hide / ivory. 0 ⇒ no commercial value.
}
```

- **`edible` and `tradeable` are DERIVED, never stored** — `edible ≡ provisions_per_biomass > 0`,
  `tradeable ≡ trade_goods_per_biomass > 0`. This is the read-the-mechanism discipline: one source
  of truth (the vector), the two flags are a comparison. (We discussed literal `edible`/`tradeable`
  bools; the vector is barely more work, richer — a wolf's pelt is worth more than a rabbit's, a
  mammoth carries ivory — and keeps the two food webs symmetric with flora.)
- Today's global `hunt.provisions_per_biomass` and `market.trade_goods_per_biomass` become the
  **default** the vector falls back to, so every existing species is byte-identical *for its food
  component*. The **change** for existing species is that they now *also* yield their trade component
  under the food policies (a deer's hide sells whether you Sustain or Deplete it) — see the rebalance
  note in Decisions.
- The wolf's vector: `{ provisions 0, trade_goods <high — pelts> }`. It is the first `edible = false`
  species.

### 2. The policy becomes a pure INTENSITY ladder — and Market is renamed

The four policies keep their **rates** and lose their product identity:

| policy | rate | outcome |
|---|---|---|
| **Sustain** | MSY | stable at `K/2` |
| **Surplus** | `surplus_multiplier × MSY` | slow, reversible decline |
| **Deplete** *(was Market)* | `deplete_multiplier × MSY` | fast decline |
| **Eradicate** | whole standing stock | extinction |

- **"Market" is renamed to "Deplete."** Once *every* policy sells the species' trade goods, "Market"
  (= "the one that sells") is a category error. The axis is now sustainable → surplus → over-exploit →
  gone, so the third rung is named for its *pressure*, not its product. (`Deplete` reads as "draw it
  down hard" and stays clearly distinct from `Eradicate` = "wipe it out". Alternatives weighed:
  Overhunt, Exploit.) The wire `policy` string is free-form, so this is a rename of the key + the
  client label, not a schema change.
- The `market.*` config block's *rate* multiplier moves to a policy-intensity lever (`deplete_multiplier`);
  its `trade_goods_per_biomass` becomes the per-species vector default.

### 3. Yield = take × the species vector, for EVERY harvesting policy

`hunt_take` / `expedition_take_biomass` stop routing product by policy. Instead:

```
take_biomass  = hunt_policy_rate(policy, …)                 // intensity — unchanged shape
food          = take_biomass × species.provisions_per_biomass   // 0 for a wolf
trade_goods   = take_biomass × species.trade_goods_per_biomass  // pelts
```

Both are credited (food → the band larder, trade goods → `FactionInventory`), for Sustain, Surplus and
Deplete alike. A wolf produces `food = 0, trade = pelts`; a deer produces both.

### 4. Eradicate yields the WINDFALL, not nothing

Taking the whole stock hands you a **one-shot final haul** of the species' products (food + trade),
after which the herd is extinct. This is a deliberate change from today's `Eradicate ⇒ no yield`:
**"denial" is the END STATE (the resource is gone for you and everyone else), not a promise that you
threw the carcasses away.** You wipe the wolves out and you keep the pelts. (Settled with Ray.)

### 5. Picker availability follows the vector — it does NOT prune by `edible`

The flags gate the yield *components*, **not the buttons**. A species that yields *anything* (`edible`
**or** `tradeable`) shows the full ladder — Sustain / Surplus / Deplete / Eradicate — because each rung
is a meaningful *rate* at which to collect that product. So a wolf shows all four; it just pays in
pelts. The **only** case that prunes the harvest options is the degenerate **yields-nothing** species
(`edible = false` **and** `tradeable = false`, a pure pest): Sustain/Surplus/Deplete would collect
nothing, so only **Eradicate** (pure cull) is offered. No shipped species is that today; the rule is
stated so the picker derives correctly when one arrives.

## What already exists (reused / touched)

- **`fauna::hunt_policy_rate` / `hunt_credit_ceiling`** — the intensity math. Keep; rename the `Market`
  arm to `Deplete`; the rates are unchanged.
- **`fauna::hunt_provisions`** — the biomass→food conversion. Becomes the food *half* of a two-part
  conversion; a symmetric `hunt_trade_goods` half is added, both reading the per-species vector.
- **`FollowPolicy`** (`sim_runtime`) — `Market` variant renamed to `Deplete` (wire key + `from_str`).
  `EXTRACTIVE` / `HUNT_POLICIES` groupings unchanged in shape.
- **The flora yield vector** (`docs/plan_flora_roster.md`) — the precedent this mirrors; the fauna side
  gains the same per-species `provisions`/`trade_goods` vector plants already have.
- **The whole yield-READOUT pipeline assumes "food"** — and this is the arc's real weight. Every one of
  these currently headlines a single food number and must learn to carry (or route) a *trade* product:
  the retained `SourceYield` telemetry, the pre-commit `hunt_forecast` + `huntPolicyCeilings`, the
  expedition raid forecast (`deliveredFood` / `hunt_trip_estimates`), the band food ledger + `foodIncome`,
  and every client readout (the herd drawer, the compose picker's per-policy metric, the map yield
  labels, the parties "next delivery" line). A wolf hunt delivering **pelts, not food**, has to flow
  through all of it — cleanly, without a wolf's take showing up as phantom food or breaking the
  larder-ledger identity.

## Staging — each phase independently testable

- **Phase 0 — the sim model (product × intensity).**
  Add `SpeciesDef.hunt_yield` (per-species `provisions`/`trade_goods`, defaulting to today's globals);
  rename `Market → Deplete` end-to-end (`FollowPolicy`, config, command text); make `hunt_take` /
  `expedition_take_biomass` credit **both** products per the species vector for every harvesting policy;
  make **Eradicate** pay the whole-stock windfall. Seed the **wolf** vector (`provisions 0`, `trade_goods`
  high). Thread the trade product through the yield **telemetry + forecasts** so `forecast == actual`
  still holds (the invariant that the UI can never promise a number the sim won't pay).
  **Testable:** hunting a wolf credits trade goods and **zero** food under every policy; hunting a deer
  credits meat + hide; Eradicate pays a final windfall; the larder-ledger identity still reconciles
  (a wolf hunt adds nothing to `foodIncome`).

- **Phase 1 — client legibility.**
  The renamed intensity ladder in the picker; per-policy metrics + the compose forecast showing the
  species' actual product(s) — a wolf reads **"pelts / turn" (trade), not food**; the map yield labels,
  the herd-drawer readout, the expedition raid "delivers ≈X pelts", and the band ledger's trade line.
  Consumes only free-form wire fields where possible.
  **Testable:** the wolf drawer + compose sheet read in pelts; a deer reads meat + hide; the Deplete
  label reads correctly everywhere; no readout claims food for a wolf.

**Deferred / adjacent (noted, not built here):**
- **What trade goods actually DO.** This arc gives every hunt a *trade* output, but trade goods are
  still economically thin ("trade has little effect yet"). Making pelts/ivory matter — a real trade
  economy — is its own arc; this one only produces the goods honestly.
- **A named trade good per species** (pelt vs ivory vs hide) is a *flavor* layer on top of the scalar
  `trade_goods_per_biomass`; not needed for the mechanic.
- **Husbandry accrual** stays on Sustain (a stewardship signal), unchanged — orthogonal to the product
  split.

## Decisions & rationale

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Product (species) and intensity (policy) are ORTHOGONAL** | The current welding is exactly what makes a predator's picker wrong. Separating them makes deer/wolf/rabbit one system: *what* you get is the animal, *how much* is the policy. |
| 2 | **Product is a per-species YIELD VECTOR** (`provisions`/`trade_goods` per biomass), not two bools | Mirrors the flora roster's vector, keeps the two food webs symmetric, and is richer (a mammoth's ivory ≫ a rabbit's fur). `edible`/`tradeable` are derived comparisons, never stored. |
| 3 | **Every harvesting policy yields the species' product(s) at its rate** | A wolf's Sustain is a *sustainable fur trade*; a deer's Deplete is meat + hide, fast. The rate is the only difference. |
| 4 | **`Market` → `Deplete`** | Once every policy sells, "Market" mislabels one rung. The axis is a harvest-pressure ladder; the rung is named for its pressure. Free-form wire string ⇒ no schema change. |
| 5 | **Eradicate pays the whole-stock WINDFALL** | "Denial" is the end state (extinct), not zero take. You wipe the wolves out and keep the pelts. Consistent with "every policy yields product." |
| 6 | **The flags gate yield COMPONENTS, not buttons** | `edible = false` does not hide Sustain/Surplus; it zeroes their food. A wolf keeps the full ladder, paid in pelts. Only a *yields-nothing* species (neither flag) prunes to Eradicate-only. |
| 7 | **Existing species get their trade component under the food policies too** (a rebalance) | Today a deer's hide only sells under Market; now it sells under Sustain/Surplus/Deplete as well. Deliberate — a real hunt yields meat AND hide. Trade goods are economically thin today, so the balance impact is small; measure it. Defaults keep the *food* component byte-identical. |
| 8 | **`forecast == actual` is preserved across a second product** | The one invariant that must not break: the pre-commit forecast, the seed, and the resolved take all read the same helpers, now for food AND trade. A wolf that previews "pelts" must deliver exactly that. |

## See Also

- `docs/plan_predators.md` — the predator arc that surfaced the wolf-offers-food wart (Phase 1a shipped
  the wolf; this arc fixes its hunt policies).
- `docs/plan_flora_roster.md` — the per-species **yield vector** this mirrors on the fauna side.
- `docs/plan_exploration_and_sites.md` §2b — the expedition raid + the original **"Hunt policy payoffs"**
  gap (issue #213) this answers; the raid forecast is one of the readouts Phase 1 must re-product.
- `core_sim/CLAUDE.md` → "The hunt policy axis" / "Pre-commit Yield Forecast" — the intensity ladder,
  the kill-credit bank, and the forecast==actual invariant this must keep.
