---
paths:
  - "core_sim/src/{labor_config,orders}.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/snapshot/**"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/tests/labor_allocation.rs"
---

<!-- Extracted verbatim from lines 42-42;3430-3568 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Pre-commit yield forecast (per-source, on the wire)

## Config files

| File | Purpose |
|------|---------|
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` (**no longer the wild biomass→food rate** — since #433 every patch converts at the share-weighted average of its own basket, and this survives as the **empty-basket fallback** plus the rung-3 quality normalization baseline), and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield`; **the whole per-stance lever set — `surplus_multiplier`, `market` (entirely, including its 4× `trade_goods_multiplier`) and `eradicate.take_fraction` — is DELETED by the harvest floor arc**, which replaced four fixed rates with one floor the player carries, after which *no option carries a factor of any kind* (`docs/plan_harvest_floor.md` §4) — **plus the Phase 1a `cultivation` block** — the plant ladder's **two rung payoffs (slice 7)**: **`tended_regrowth_gain` (1.0, rung 2 — NEUTRAL since Flora Roster S2, `docs/plan_flora_roster.md` §4.3: a tended stand regrows exactly as fast as wild. It began as the plant twin of `husbandry.pastoral_gain`, but once S1 made competitor-removal explicit a growth boost DOUBLE-COUNTS it, so tending pays through composition + conversion and the rung-2 "wild < tended" guarantee moved to the roster's own bar, `core_sim/tests/flora_roster.rs`; kept as a playtest dial in case a small boost is wanted back)** and **`field_provisions_per_biomass` (0.02, rung 3 — a managed rate on the standing crop, no drawdown, floor axis collapsed, because at rung 3 the source is YOURS)**; both PLAYTEST DIALS. `validate()` still enforces `tended < field`, now `field_provisions_per_biomass > tended_regrowth_gain × regrowth_rate/4 × provisions_per_biomass × tended_conversion_gain`, evaluated at tending's saturated best case so the crop's own rate cancels and the check stays scale-free; the `tended_regrowth_gain` check forbids only the INCOHERENT `< 1.0` (tending grows a stand slower than wild), not `<= 1.0`. **Plus the #433 pair `tended_weeding_gain` (1.5) / `tended_conversion_gain` (2.0)**, both validated finite and `>= 1.0`: the first is how far rung 2 **weeds** the favored species' share (`min(1.0, share × gain)`, the increase taken from the least abundant remaining species first), the second the conversion multiplier on that species' **whole yield vector**. Neither touches `K` — **the land owns `K` and no rung below 4 raises OR lowers it**. The retired `tended_concentration_gain` / `field_concentration_gain` pair multiplied the tile's `K` by `min(1, share × gain)` and **discarded the remainder**, which is the bug #433 fixed. See "Committing a patch to one plant". The retired `tended_provisions_per_biomass` (0.01) made rung 2 a *managed* rate a full rung earlier than the animal side's, so a tended patch could not be over-farmed and every policy paid the identical number (**the plant rung-2 BUILD dials — the old `progress_per_turn`/`decay_per_turn`/`cultivating_yield_fraction` — moved to `intensification_ladder.json`'s `plant:tended` rung**, and in slice 4 **the earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` moved to that file's ladder-level `knowledge` block** too, so both food webs climb *and learn* on the same numbers) (Rung 1a: cultivation is the explicit **`Cultivate` improvement** — while preparing, the CREW carries the `plant:tended` rung's `yield_fraction_while_building ×` what a gathering crew of the same size carries — the dip multiplies **crew throughput, never the take ceiling** (`docs/plan_harvest_floor.md` §3.1), so it is floor-independent by construction and a crew big enough to saturate the standing stock pays nothing for it and accrues that rung's `progress_per_turn`; at 1.0 the completed tended patch is worked place-local, gathered to its floor on the (now neutral, = wild) tended ecology — so a *bare* patch pays exactly wild, and its yield advantage over wild comes from a **committed crop** (weeding + conversion, #433), not a regrowth boost — and goes feral if abandoned. Rung 1b: working a **wild** patch under a stewardship policy earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate verb — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**; the accrual is the ladder's, driven off the rung — see "The knowledge pattern"); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
## Pre-commit Yield Forecast (per-source, on the wire)

The **retained yield telemetry** (`SourceYield.actual/sustainable/workers_needed`, above) is
**post-hoc** — the player only learns they over-assigned *after* committing and advancing a turn. The
forecast is its pre-commit twin: per in-range source, the snapshot exposes enough for the client to
show a live **"Expected yield: +X.XX /turn"** and **cap its worker stepper at the max-useful count
while the player is composing an assignment**.

**Wire fields** (append-only, on both `WorldSnapshot` and `WorldDelta`): `perWorkerYield:float` on
both `ForagePatchState` (per tile) and `HerdTelemetryState` (per herd), plus the per-policy ceilings
(**food/turn**, at the source's CURRENT biomass) — which are carried **differently on the two sides**:
a patch keeps the scalars `ceilingSustain` / `ceilingSurplus` / `ceilingDeplete` / `ceilingEradicate`,
while a **herd carries them only as the `huntPolicyCeilings` list** (its scalar twins are retired
`(deprecated)` slots — a free-form `policy` string means a new policy needs no schema change, and the
list and the scalars were provably the same numbers). **Plus the investment rung**:

**`ceilingMarket` → `ceilingDeplete`** was a name change on the *same* FlatBuffers slot; **every one
of those scalars, and the per-policy row lists that replaced them, are now retired
`(deprecated)` slots.** A stance ceiling cannot be enumerated once the player drags a continuous
floor — see "THE CEILING LISTS ARE RETIRED" below.

> ### THE FORECAST IS A VECTOR, not a food scalar (issue #337) — and its trade half is RETIRED
>
> Every field of `SourceYieldForecast` is a **`YieldAccounts { provisions, fodder }`** —
> `per_worker_yield`, `per_biomass_yield`, `managed_yield`, `pastoral_yield`, `body_mass_yield`. So is
> `SourceYield`'s telemetry.
>
> **Why vectorised rather than sibling per-account scalars.** Sibling scalars double the surface and
> let the halves drift apart under a retune; one vector per rung cannot, because `ceiling_at` hands
> every component to every reader at once.
>
> **`forecast == actual` holds PER COMPONENT**, and that is the invariant this whole arc rests on: if
> the forecast can promise a number the sim will not pay in *any* account, the UI lies. Pinned on the
> **exported snapshot** (not the in-process struct) by
> `hunt_yield_vector::the_forecast_equals_the_paid_take_on_the_wire`, across a defaulting species and
> an inedible one × all four extractive rungs.
>
> **Quantisation picks an AXIS, and it is never assumed to be food.** `forecast_production_and_take_at`
> runs `quantise_animal_take` on `SourceYieldForecast::ratio_axis()` — the first component with a
> *positive* per-biomass rate — then `YieldAccounts::rescaled_to` carries the one animal count back
> into the other component. An animal count is a **ratio**, and a ratio is unit-free: any positive
> component gives the same answer, a zero one gives `0/0`. Correspondingly **"does this source
> quantise?" is `!body_mass_yield.is_zero()`**, not `body_mass_yield.provisions > 0`.
>
> > #### THE TRADE-GOODS ACCOUNT IS RETIRED (arc #527), AND A WOLF NOW FORECASTS `0`
> >
> > `YieldAccounts` carried a third component, `trade_goods`, and it was the *only* positive account
> > an inedible species had. It went because it was **written by every take site and read by none** —
> > there was no `take(TRADE_GOODS)` anywhere in the workspace — while the `credit_material_yield`
> > call beside each of those writes banked the same take's concrete hide, bone and fibre.
> >
> > **The consequence for this file is exact and worth stating plainly**: a wolf's forecast is now
> > `0` in every component the forecast carries, and that reading is *honest but incomplete*. It is
> > not food, and what it really pays — **material batches** — cannot live in a `YieldAccounts` at
> > all: batches carry a characteristic vector, and this type is the part that adds, scales and
> > `min`s componentwise. **Projecting materials is its own arc**; until it lands, an inedible
> > species' preview states its zero and the take banks its pelts.
> >
> > Two readings changed with it, both deliberately:
> > - **`ratio_axis` has two arms** (`Provisions`, then `Fodder`), so a wolf has none and takes the
> >   continuous branch. Its forecast is `0` either way, so no answer moved.
> > - **`HuntYield::yields_nothing` counts MATERIALS**, not a trade rate. Without that the picker's
> >   one pruning rule (`fauna::species_requires_denial`) would have collapsed a wolf to floor `0`
> >   alone — a real gameplay regression hiding inside a data removal.
> >
> > Retired wire slots, all `(deprecated)` in place and none deleted:
> > `LaborAssignment.tradeYield` / `realizedTradeYield` / `tradeYieldLow` / `tradeYieldHigh` ·
> > `HerdTelemetryState.perWorkerTrade` / `tradePerAnimal` / `pastoralTrade` / `corralTrade` /
> > `tradePerBiomass` · `ForagePatchState.tradePerBiomass` / `tendedTrade` / `fieldTrade` ·
> > `FloraShareInfo.sowTradePayoff` / `cultivateTradePayoff`. On the proto,
> > `HuntTripRow.delivers_trade` / `delivered_trade` and `DenialRow.delivered_trade` / `wasted_trade`
> > are **reserved field numbers**, never freed.
>
> **No fodder `arrivals` schedule, deliberately.** `arrivals` answers *"when does food land so my
> people eat"* — a question with a consumption clock. Nothing consumes the fodder store on that
> clock, so a fodder timetable would answer a question nobody asks. **And `food_income` stays
> `Σ actual` and must never include `fodder`**: that sum is one side of the pinned larder identity
> `larder_delta == food_income − food_consumption − pen_feed_upkeep`, and fodder never touches the
> larder.
>
> **THE PLANT SIDE'S FODDER COMPONENT IS `0.0` — a known gap, not a claim.** `forage_forecast` fills
> `forage::PLANT_FODDER_FORECAST_NOT_YET_PROJECTED` throughout: a hay Field really does credit the
> `FODDER` store, the sim simply has not *projected* it. The fodder a harvest **actually earns** is
> reported (`SourceYield::fodder` / `LaborAssignmentState::fodder_yield`).
>
> **The client renders a component's line ONLY when it is `> 0`** — so a source with no fodder shows
> *no fodder line* rather than a false "0/turn".
>
> **THE ROW CARRIES FODDER TOO — `SourceYield::fodder` / `LaborAssignment.fodderYield`**
> (issue #449, appended last). The take pays every account the vector names
> (`docs/plan_flora_roster.md` §3) and the row reported food alone, so a **sown hay Field** —
> `flora_config.json`'s `hay_grass`: no provisions, `fodder_per_biomass 0.20` — published `+0.00` in
> every compact yield readout while feeding the band's pens every turn. It is filled at the **two** sites that credit the `FODDER` store (the
> Field arm and the wild/tended gather arm of `advance_labor_allocation`) and nowhere else:
> - **It is the CREDITED value, never a recomputation**, gate included. The wild credit is gated on
>   *Foddering* at the credit site (`flora.md` → "Wild fodder is gated at the CONSUMER"), so a row
>   that re-derived `tended_take_fodder` would publish hay income to a faction that was paid nothing.
>   Pinned by `forage_basket_reweight::the_published_fodder_is_the_fodder_the_band_was_actually_credited`,
>   which sweeps both sides of the gate.
> - **It is not food income.** `food_income` stays `Σ actual`; fodder credits the band's `FODDER`
>   store and never touches the larder.
> - **There is no `realized_fodder` twin and no `YieldRange` fodder band, deliberately.** Fodder is
>   paid by the *plant* web alone, whose forward projection is the known gap
>   `PLANT_FODDER_FORECAST_NOT_YET_PROJECTED` names — so a projected-fodder field would be a constant
>   zero on the only web that can pay it. And every forage row's range is a point (nothing on the
>   plant web is stochastic), so bounds would only restate the scalar. The client reads the actual.
> - **A hunt row's `0.0` is structural**: no animal's `YieldAccounts` pays fodder. `forecast_source_yield`
>   reads `actual.fodder` off the take vector rather than writing a literal, so a **pre-commit seed**
>   still quotes `0` on both webs — the plant side because `forage::plant_food_only` keeps the forecast
>   food-only. **A fresh hay-Field assignment therefore
>   still previews `+0.00` until its first turn resolves**; closing it means giving the plant forecast a
>   fodder component, not adding a field.
>
> **`PopulationCohortState.huntPerWorkerProvisions` is SPECIES-BLIND — do not clamp a per-herd preview
> with it.** It is a per-*cohort* echo of the global `hunt.provisions_per_biomass`, and a cohort has no
> herd, so there is no species to resolve a vector from; left unqualified it quotes a wolf's hunters a
> positive food rate against all-zero food ceilings — a contradiction on the wire. The species-aware
> rate is the herd's own `perWorkerYield`, straight off its `hunt_forecast`, so
> `min(workers × perWorkerYield, ceiling(floor))` is honest per component — and for the *crew* side
> of the same question the herd carries **`perWorkerBiomass`**, which is positive on a wolf where
> both the cohort echo and the food rate mislead. The cohort field survives as the expedition
> **outfit** lever (rough carry arithmetic before a target is chosen); for a chosen target the client
> asks — `sim_runtime`'s `HuntTripForecastQuery`, answered at that band's kit and live wear.

> **A stance ceiling is now a STOCK, on both webs — `max(0, B − floor·K)`.** Since
> `docs/plan_harvest_floor.md` slice 1 the four rows on each list are the stock standing above each
> stance's escapement floor (`fauna::hunt_escapement_ceiling` / `forage::forage_escapement_ceiling`),
> which is **exactly** what the take path pays: the readout and the take are one call, not two numbers
> kept in step. The retired steady-rate-vs-banked-burst distinction went with the kill-credit bank the
> resident take path no longer reads (`Herd::hunt_credit`), and `hunt_forecast`'s
> `the_forecast_ceilings_are_the_escapement_stock_and_stay_ordered` pins the replacement.
>
> **Two readings therefore changed shape, and consumers must not order them against each other:**
> - **A ceiling row can exceed `sustainable`.** The first harvest of an untouched source is its
>   accumulated stock, so `actual > sustainable` under *every* stance including Sustain. `sustainable`
>   stays on the row as the long-run MSY reference; the ⚠ is `components::floor_overdraws`.
> - **A ceiling row cannot be compared with a rung PAYOFF** (`tendedYield`, `fieldYield`,
>   `pastoralYield`, `corralYield`). Those are long-run rates and carry `r`; a stance ceiling is
>   `r`-free (`B − floor·K` is `K/2` on every rung at `B = K`). "Preparing +X → then +Y" is therefore
>   a stock beside a rate today, which the client's ladder redesign (`docs/plan_harvest_floor.md` §7,
>   slice 4) resolves.

> ### THE CEILING LISTS ARE RETIRED; THE CLIENT COMPOSES THE CURVE (`docs/plan_harvest_floor.md` §5)
>
> `foragePolicyCeilings` and `huntPolicyCeilings` are `(deprecated)` slots the sim no longer writes,
> and so are the scalar `ceiling*` fields before them. **Four rows can answer four questions; a player
> dragging a continuous floor asks a different one every frame.**
>
> What ships instead is the **terms**: `biomass`, `carryingCapacity`, the build-dip fractions, and the
> source's **per-biomass yield vector** (`provisionsPerBiomass` / `fodderPerBiomass`,
> — the patch's basket-averaged rates, or the herd's own `HuntYield`). The client
> composes
>
> ```text
> ceiling(floor)        = max(0, B − floor·K) × <account>PerBiomass
> expected(workers, rung) = min(workers × perWorkerYield × <rung>BuildFraction, ceiling(floor))
> ```
>
> **ON THE ANIMAL WEB THAT `min()` HAS A THIRD ARM, and leaving it out overstates a light-bodied
> species' take by ~30×** (`docs/plan_hunt_through_combat.md` §2). Engagement caps how many animals a
> party can *reach* at all — `HerdTelemetryState.engageRate`, appended for exactly this:
>
> ```text
> reach(workers, rung) = floor(workers × engageRate × <rung>BuildFraction) × bodyMass × <account>PerBiomass
> expected(workers, rung) = min(crew term, ceiling(floor), reach(workers, rung))
> ```
>
> It is a term rather than an answer for the same reason the two beside it are: linear in the crew and
> exact. **`engageRate <= 0` means "no engagement stage" and the term is dropped** — the wire's finite
> reading of the sim's `f32::INFINITY` for a **pen** (a penned animal is not stalked) and for the plant
> web, which never publishes the field. Measured before it shipped: a Wild Fowl herd with one hunter
> read **307 birds/turn** on the compose sheet (one hunter's 40 biomass of carry) against a take of
> **10** — the sheet promising 30× what the sim would pay, for the whole life of the field's absence.
> Pinned on the exported wire by
> `hunt_yield_vector::the_exported_terms_reproduce_the_engagement_bounded_take`.
>
> **THE BUILD FRACTION MULTIPLIES THE CREW, NOT THE CEILING** (`docs/plan_harvest_floor.md` §3.1).
> It moved there because dipping the ceiling made a deeper floor build for free — a fraction of a
> bigger standing stock still filled the crew's baskets, so every stance completed a 25-turn Cultivate
> on schedule. On throughput it is floor-independent by construction. The client-visible consequence:
> a crew big enough to saturate the source's stock pays **no** dip, and the remedy for a slow build is
> to add hands (at the shipped 50% carry, twice as many).
>
> **This is a deliberate, narrow exception to *"the sim exports the answer"*, and the exception is
> sound for a stated reason.** That rule exists because a hunt take is rounded to WHOLE ANIMALS —
> `floor(ceiling / bodyMass)` is not linear, so no client can re-derive it and the sim must hand over
> the result. This expression is different in kind: **linear and exact**, so a client evaluating it
> lands on the number the sim would. The division of labour is therefore **the client draws the curve,
> the sim states the take**: `SourceYield.actual` for the *committed* assignment is still the sim's
> answer, quantisation and all, and the chart is a projection rather than a promise.
>
> The dip still ships as the factor it is — **`ForagePatchState.cultivateBuildFraction` /
> `sowBuildFraction`** and **`HerdTelemetryState.tameBuildFraction` / `corralBuildFraction`**, each the
> rung's `yield_fraction_while_building`.
>
> ### THE BOUNDARY, stated once — it is the thing a future reader will get wrong
>
> **Where a closed form exists the sim ships the TERMS and the client evaluates it; where one does
> not, the sim ships ANSWERS and the client interpolates between them.** The two halves now sit side
> by side on the same tables, so the line between them has to be legible:
>
> | | shape | why |
> |---|---|---|
> | escapement ceiling | **terms** — `biomass`, `carryingCapacity`, `*PerBiomass` | `max(0, B − f·K) × rate` is linear and exact; this is what retired the four stance rows |
> | build dip | **terms** — the four `*BuildFraction` fields | a factor on the crew term, likewise exact |
> | engagement bound | **term** — `HerdTelemetryState.engageRate` | `workers × engageRate × dip × bodyMass` is linear in the crew, exactly like the carry term beside it |
> | the take | **the answer** — `SourceYield.actual` | `floor(ceiling / bodyMass)` is not linear; no client can re-derive it |
> | raid trip length | **an answer, ASKED FOR** — `HuntTripForecastQuery` on the command socket | a bounded forward simulation; there is no expression to hand over, and it depends on the asking band's kit and wear, which no per-herd row carries |
> | the growth curve | **sampled answers** — `regrowthSamples` × `REGROWTH_CURVE_SAMPLES` | see below |
>
> **`regrowthSamples` is sampled, and NOT because the curve is hard to write down.** It is **two
> different functions**: a patch is pure logistic with a reseed floor and **no Allee term**, a herd has
> **critical depensation** below `collapse_fraction`. Publishing `r` plus the thresholds would put a
> second copy of both models in a language with no tests over them, and the drift would be *invisible*
> — either implementation still draws a plausible chart. Sampled through the same seams the turn uses
> (`fauna::reseeding_logistic_regrowth` under `patch_ecology`; `fauna::net_biomass_delta` under
> `herd_ecology`/`herd_capacity`), so the chart and the turn cannot part company.
>
> Three panel readings are all this one curve: the *"hold it after"* crew target (the regrowth at the
> chosen floor over the crew's carry), the verdict line, and the projection under the floor. Two
> properties are load-bearing and pinned
> (`snapshot::subsistence::tests::the_plant_curve_never_declines_and_the_animal_curve_does_below_the_allee_point`):
> the plant curve is **non-negative at every sample** and its `0.0` entry is the **reseed floor's
> lift**; the herd curve's low samples are **negative**, and a client must render them as decline
> rather than clamp them — that crash is why floor `0` ends a herd and only sets a patch back. **The
> peak of the curve IS the food peak** at `K/2` and is deliberately not published separately: one
> number derived two ways is how the two start disagreeing. The samples are evenly spaced over
> `0.0..=1.0` of `K`, so the x-axis is implicit; `REGROWTH_CURVE_SAMPLES` is a display-resolution
> choice, not a model fact, and is named so a set of readings cannot quietly re-become a set of
> states. It is now the only sampled answer left on the wire: the raid tables that sat beside it were
> sampled for affordability rather than because sampling was right, and asking replaced them.
>
> Sim-side that is **`SourceYieldForecast::ceiling_at(floor)`** — one computation, which answers *any*
> floor because the player drags a continuous one. It is backed by the forecast's **terms**
> (`biomass`, `carrying_capacity`, `per_biomass_yield`) rather than by stored rows: four `ceiling_*`
> fields could only answer four questions, and every row added was a second place the ceiling was
> computed.
>
> **It takes no `improvement`, and that is what makes the curve composable at all.** With the build
> dip on crew throughput a ceiling is purely `max(0, B − floor·K) × rate` — linear and exact in terms
> already on the wire. The dip still ships (`BuildDips` sim-side, the `*BuildFraction` fields on the
> wire) and is applied by **`fauna::forecast_expected_take`**, which multiplies the crew term; the
> take path (`forage_take` / `systems::hunt_take`) reads the same `LadderConfig::build_dip` seam, so
> forecast == actual holds per component with a build in flight
> (`hunt_yield_vector::the_forecast_equals_the_paid_take_with_a_build_in_flight_at_every_floor`, swept
> over the floor × both binding regimes × both build verbs).
>
> The standing-stock clamp inside `ceiling_at` is **belt-and-braces and inert** — an escapement
> ceiling is `B − floor·K ≤ B` for any floor `≥ 0` — and kept because a future ceiling that *could*
> exceed the stock must not silently over-report. `stock_cap` stays populated for wire stability.
>
> A **rung-3 managed** source has `stock_cap: None` (it is never drawn down) and reports both
> fractions as **`0`** (`BuildDips::NOTHING_LEFT_TO_BUILD` → `NO_BUILD_REMAINING_FRACTION`), which is
> deliberately outside the documented `0 < f < 1` dip range: *"this rung is not offerable here"*. It
> published the identity `1.0` until PR #448, which said the build was free **and** still available;
> the client's compose sheet already declines to quote a deal on a non-positive fraction, so the
> sentinel needed no client change. `BuildDips::of` still answers the identity for a `None` slot —
> the wire value and the multiplier are different questions.

`ForagePatchState.tendedYield` and, on a herd, `corralYield` are what the source will pay **once the
improvement completes**, so with the dip above the client shows **"preparing X → then Y"** *before* the
player commits to the cost. (Sim-side the payoff is `SourceYieldForecast::managed_yield` — the two
rung-3 verbs are kind-exclusive, so one field serves both.)
**The `Tame` rung has its own payoff twin: `HerdTelemetryState.pastoralYield`** (sim
`SourceYieldForecast::pastoral_yield`) — what a Sustain hunt pays **once the herd is tamed**, so the
client can render Tame's `→ +Y` instead of quoting only its during-building dip, which reads *below*
the undipped stance and hides that taming out-yields wild hunting. `0` on a source that never
offers Tame (a forage patch, or a herd already penned/forage-tended).

**`pastoralTrade` / `corralTrade` were the trade halves of the very same
`SourceYieldForecast::pastoral_yield` / `managed_yield` vectors, and are `(deprecated)` slots since
arc #527** — with the axis gone, an inedible species' investment rungs quote `0`, exactly as its
extractive ones do. **Both `pastoralYield` and the
un-penned `corralYield` projection (`managed_yield`) are the SUSTAINED MSY on the improved ecology** —
`HuntYield::apply(sustainable_yield(biomass_before_regrowth, carrying_capacity, &{pastoral,pen}_ecology_for(..)))`,
the long-run rate — **NOT** the one-turn constant-escapement take. Because MSY is `r`-dependent while
escapement (`max(0, B − floor·K)`) is `r`-independent, the sustained form is the axis on which the
ladder is expressible at all: **`pastoral_yield < managed_yield`** (pastoral `r×2.0` < pen `r×4.0`,
MSY-capped; measured ≈ 1.0 < 2.0 on a full Wild Boar). The old escapement projection read
`pastoral_yield == managed_yield` (≈ 10 = 10) and could not show the ladder the field exists for.
**A stance/floor ceiling is NOT on that ladder and must not be ordered against it** — it is a stock
and these are rates; `B − floor·K` is `K/2` on every rung at `B = K`. **The penned-herd `managed_yield` stays the escapement take** — a live corralled herd hits
`hunt_forecast`'s `is_corralled()` early-return, which returns `corral_provisions` (the actual
constant-escapement corral yield), so forecast == actual for a real pen; only the *un-penned
projection* is the sustained MSY. Pinned by
`fauna::tests::the_tame_rung_advertises_its_payoff_above_the_dip_and_wild_sustain`.
- `perWorkerYield` = food/turn one worker contributes (throughput → provisions; **forage folds in the
  tile's `seasonal_weight`**, as `forage_take` does — it can be `0` in a dead season, so consumers must
  not divide by it; hunt has no seasonal factor).
- **`perWorkerBiomass` = the same throughput in BIOMASS**, before any account conversion:
  `per_worker_biomass_capacity × seasonal_weight` on a patch (`forage::forage_per_worker_biomass` at
  the *equipped reference* rate — a band's own basket tier rides its cohort row instead, see
  `equipment.md`; `0` in a dead season) and `labor_config.hunt.per_worker_biomass_capacity` on a herd (no seasonal
  factor). It is the term the **crew** half of the panel divides by — *"clear it now"* is
  `(B − floor·K) ÷ (carry × dip)` and *"hold it after"* is the regrowth at that floor over the same
  carry, both arithmetic in biomass.
  > **It is not a duplicate of `perWorkerYield`, and it must not be re-derived from it.** The
  > quotient `perWorkerYield ÷ provisionsPerBiomass` is exact and **undefined on precisely the
  > sources that pay no food** — a sown Field of cotton, flax or hay, and a wolf herd — where the
  > panel would then be unable to state a crew number at all. A rate that exists on every source
  > cannot come from two that can both be zero.
  >
  > It was absent because a per-worker scalar was held unable to state a *policy-dependent* rate,
  > which the plant web's retired trade account then had (`Deplete` marked it up). That markup is
  > deleted (`docs/plan_harvest_floor.md` §4) and no factor rides the depth of the draw anywhere in
  > the model, so throughput is policy-blind in fact and one scalar states it honestly.
  >
  > **It supersedes `PopulationCohortState.huntPerWorkerProvisions` for a per-herd preview** — see
  > the trap that field carries, below. It is the same species-aware split `perWorkerYield` makes for
  > the food account, one level down. The cohort field stays: it is still the expedition **outfit**
  > lever, quoted before a target is chosen.
- `ceiling(floor)` = the stock standing above that floor, in food/turn, **already clamped to the
  source's remaining biomass** (belt-and-braces — an escapement ceiling cannot exceed the stock).
- **`collapseFraction` / `stressedFraction` = the ecology phase BANDS**, as fractions of
  `carryingCapacity` — the cut points `fauna::classify_ecology_phase` uses, in **the same units the
  floor is in**. `ecologyPhase` ships which band a source is in; these ship *where the bands are*,
  which is what turns them into the chart's background for the floor line: a floor and a phase are
  the same kind of object.
  > **Per source, not a global echo, because a herd's bands come from the RUNG it stands on.**
  > `fauna::herd_ecology` resolves wild / pastoral / pen and the managed blocks carry their own cut
  > points, so one global pair would be right for plants and wrong for a tamed or penned herd. Each
  > is read through the *same* seam the phase **word** is classified with (`forage::patch_ecology`,
  > `fauna::herd_ecology`), and `core_sim/tests/ecology_bands_on_the_wire.rs` pins the two against
  > **each other** — the published word must be the one the published cuts imply for the published
  > stock — rather than each against a literal, so a rung's ecology can be retuned without touching
  > the test and only a genuine disagreement fails.
  >
  > On the animal web `collapseFraction` is also the **Allee threshold**: it is where
  > `regrowthSamples` turns negative. The two fields describe the same cliff from either side.
- Captured at `output_multiplier = 1.0` (the productivity multiplier is per-band): the client scales
  every field by the acting band's `PopulationCohortState.outputMultiplier` — a linear factor, so
  `max_useful_workers` is invariant to it.
- Client composition: `expected(workers, floor, rung) = min(workers × perWorkerYield ×
  <rung>BuildFraction, ceiling(floor))`, `max_useful_workers(floor, rung) = ceil(ceiling(floor) /
  (perWorkerYield × <rung>BuildFraction))`.
- **A crew BUILDING something floors that cap**, on both webs, because a build's crew is dipped and
  inverting a dipped throughput could otherwise ask for fewer hands than the rung's own `crew_needed`:
  `max_useful_workers(floor, rung) = max(ceil(ceiling / (perWorker × dip)), <crew floor>)`. The herd's floor
  is `herdersNeeded` / `herdersNeededIfManaged` (derived from the herd's size) and has shipped for
  slices; the patch's is the appended **`cultivateCrewNeeded` / `sowCrewNeeded`**, the rung's own
  `crew_needed`. **The same number is the build's denominator** — plant accrual is
  `progress_per_turn × min(workers / crew_needed, 1)` — so staffing the cap the panel offers is what
  buys the rung's stated build length, and under-staffing costs turns rather than nothing. Sim-side
  the floor is `intensification::source_crew_needed`, one `max()` for both webs **and for both halves
  of the row** — see "The crew floor is ONE definition" below.
- A **rung-3 managed source** (a sown **Field** / a **corralled herd**) is *yours*, so **the floor axis
  collapses**: `ceiling_at` returns its `managed_production` at every floor
  (`SourceYieldForecast::managed`). **The worker cap does
  not collapse** — `perWorkerYield` is the crew's real throughput, so `max_useful_workers =
  ceil(production / perWorkerYield)` is an honest count that grows with the source (slice 7; it used to
  be a hardcoded `1`, which claimed one worker could carry home whatever the land offered). A **tended
  patch is NOT this shape** — it is rung 2, a wild stand either way, and forecasts floor-live like a
  wild patch.

> ### THE INVARIANT IS RESTATED: `forecast == actual` IS NOW A CLAIM ABOUT A DISTRIBUTION
>
> **`docs/plan_hunt_through_combat.md` §6.4, slice 6.** The old form — *"the forecast is the number
> the sim will pay"* — cannot survive a stochastic take, and the reason is structural rather than a
> gap someone could close:
>
> > **A forecast has no event seed.** `fauna::retreat_seed` is composed from
> > `(map_seed, tick, herd, party)`, and a projection is projecting into ticks that have not happened.
> > There is no tick for it to name, so a preview **physically cannot draw** the retreat — or the
> > attack rolls — the live take will draw.
>
> So the invariant reads:
>
> > **`SourceYield::actual` is the take's EXPECTATION over the seed, and the take the sim pays lies
> > within `SourceYield::range`.** Where no stage is stochastic the distribution is **degenerate** and
> > `low == actual == high == the take`, bit-for-bit.
>
> **It landed degenerate and that is why it landed safely.** When slice 6 shipped, `wariness` was `0`
> across the roster and `hit_chance` was `1.0`, so both binomials took their exact identities at every
> quantile, the reported range was a **point**, and the band could be wired through every forecast
> path with **no number in the game moving**.
>
> **Slice 7 authored the wariness, and the band is now real on the animal web.** A wild hunt's
> forecast reports a genuine `low < likely < high`; the **second** sentence still governs everywhere
> nothing is stochastic — the whole plant web, a pen, and a species held at `wariness 0` by config —
> and those stay bit-for-bit exact. Because a forecast reports the **expectation**, a `forecast ==
> actual` equality test on a wild hunt is no longer a meaningful assertion: it would be comparing one
> draw against a mean. Every pre-existing suite therefore holds the roster at `0` through
> `FaunaConfig::without_retreat` (the shared spelling of `hunt_yield_vector::steady_quarry`'s move),
> and the variance is asserted in one place, `core_sim/tests/hunt_wariness.rs`.
>
> **The rejected alternative was to make the draw forecast-reproducible** by taking the tick out of
> `retreat_seed`. It was refused for three reasons, all fatal: the draw would become a per-`(herd,
> party)` **constant**, so a pairing that rolled well on turn 1 would roll identically on turn 40 and
> "risk" would never vary in play; a player could therefore *learn* the answer, which is exactly the
> spreadsheet §4.7 says variance exists to prevent; and §6.2's seeding is **per event**, whose event
> is `(herd, tick, party)` — a tick-free seed is no longer per-event at all.
>
> **How the three readings are produced.** `fauna::HuntDraw` carries *how* a hunt resolves its two
> stochastic stages — `Seeded(u64)` for a live take, `Quantile { sigmas }` for a forecast — and is
> threaded through `animals_that_stay`, `resolve_hunt_fight`, `hunt_take` and
> `expedition_take_biomass`, so **the forecast runs the take's own code** rather than a second copy of
> it. Its combat half is `combat::StrikeDraw` on `CombatTuning`, read by the one
> `combat::landed_strikes` seam. `fauna::forecast_take_range` evaluates
> `forecast_production_and_take_at` at `−k`, the mean, and `+k`; every arm is monotone
> non-decreasing in the draw, so `low <= likely <= high` is a property of the arithmetic rather than
> a clamp. `combat_config.forecast_range_sigmas` (**2.0**) is the width, and it is a **readout lever**
> — no resolution path reads it, so widening the band cannot move a single animal.
>
> **The range is an ANSWER, not a term, and the boundary rule above says why**: the take passes
> through `quantise_animal_take`'s `floor()`, so a band on the animals brought down is **not** a band
> on the food — on a slow breeder both bounds routinely land on the same whole animal. Publishing
> `wariness` / `hit_chance` as terms would put a second, non-linear copy of the take model in a
> language with no tests over it, the same reasoning that makes `regrowthSamples` sampled.
>
> **A RESOLVED row is a fact, not a forecast**, and reports `YieldRange::certain` — the take happened,
> so there is no distribution left. Only the assign-time seed carries a real band.
>
> **The plant web's range is a point by construction** — a gather has no engagement, no retreat and no
> fight (`SourceYieldForecast::fight` is `None`, `engage_rate` is `INFINITY`) — so the old invariant
> survives there unchanged, at any configured width.
>
> Wire: `LaborAssignment.actualYieldLow` / `actualYieldHigh` (append-only, after `floor`; their
> `tradeYield*` siblings are `(deprecated)` slots since arc #527). Guarded by
> `core_sim/tests/hunt_forecast_range.rs` on the exported snapshot: the
> degenerate identity (bit-for-bit, animal web × a defaulting and an inedible species × the floor),
> the plant web's structural point-ness at an absurd width, a resolved row's collapse under a **live**
> sub-1 `hit_chance`, and the widened band's containment across 400 seeds — paired with three liveness
> assertions, because *"the answer is between 6 and 11"* passes when the feature is dead (§6.3). The
> sensitive halves are unit tests on the two quantile functions
> (`combat::tests::a_certain_hit_chance_has_no_spread_to_quantile`,
> `fauna::tests::zero_wariness_has_no_spread_for_the_forecast_to_report`), because the wire test reads
> the take *after* the quantiser, which absorbs a small perturbation.
>
> **That file holds the roster at `wariness 0` and states the degenerate half; the LIVE half is
> `core_sim/tests/hunt_wariness.rs`**, which runs the same containment sweep on the **shipped**
> config — the exported band widens, contains 400 live takes, and its `likely` tracks their mean —
> plus the ordering (a warier quarry yields less to the same crew, one field changed on one species so
> "all else equal" is a fact), the hunter-turns identity (the herd loses exactly what was *killed*,
> never what fled), and the surviving `wariness 0` identity, which is config-only now and is what
> every other suite installs.

**Invariant: forecast == actual — no duplicated yield math.** The forecast and the take path read the
*same* pure helpers, so the UI can never promise a number the sim won't pay:
- forage (`forage.rs`): `forage_escapement_ceiling` (the stock standing above the floor, in biomass — **no dip**) · `forage_per_worker_biomass`
  (`per_worker_biomass_capacity × seasonal`) · `forage_provisions` (biomass→provisions ×
  `output_multiplier`) · `tended_provisions` (the tended-patch managed harvest) — all called by both
  `forage_take` / the tended-patch arm of `advance_labor_allocation` **and** `forage_forecast`.
- fauna (`fauna.rs`): `hunt_escapement_ceiling` (the stock standing above the floor — **no dip**) · the species'
  `HuntYield::apply` (which retired the global `hunt_provisions`) ·
  **`managed_yield_biomass`** (the husbandry harvest, via `pen_yield_biomass`) · **`herd_ecology` /
  `herd_capacity`** (which ecology/capacity a herd lives under — *no call site may re-derive either*) —
  called by both `systems::hunt_take` / the corral arm of `advance_labor_allocation` **and**
  `hunt_forecast`. The shared `SourceYieldForecast` struct (with `::tended`) is the common return shape.
  A corralled herd's `managed_yield` is **gross**; its `penUpkeep` is exported separately.
- Guarded across **both products, on the exported snapshot**, by
  `core_sim/tests/hunt_yield_vector.rs` (`the_forecast_equals_the_paid_take_on_the_wire`,
  `a_wolves_exported_rate_reads_no_food_and_it_is_still_huntable_at_every_floor`,
  `a_composed_ceiling_carries_the_windfall_at_floor_zero`), and on the food component by
  `systems::labor_yield_tests::{forage,hunt}_forecast_equals_actual_take_for_every_floor_and_staffing`
  (every sampled floor × labor-bound/ceiling-bound staffing, comparing against the payout of a real
  `advance_labor_allocation` run) and `tended_patch_and_corral_forecast_full_yield_with_one_worker`.
  **Any change to the take math must go through these helpers** — never re-derive a ceiling or a
  biomass→provisions conversion at a call site.

Capture: `snapshot_forage_patches` / `herd_snapshot_entries` (`snapshot.rs`); the herd's
`carrying_capacity` (absent from the display telemetry) is resolved from the authoritative
`HerdRegistry`, and the per-tile `seasonal_weight` from the `FoodModuleTag` query.
**Client follow-up:** rendering the live "Expected yield" line + the worker-stepper cap in the
forage/herd assign controls.

### Assign-time yield seeding (the `+0.00` fix)

The retained `SourceYield` telemetry used to be written **only** during turn resolution, so between
"player assigns workers" and "player advances the turn" a brand-new source had no row and the display
snapshot serialized `actual_yield = 0.0` — the map annotation and the Band panel read **`+0.00`** for
every fresh assignment, and the client cannot distinguish "0 because not computed yet" from "0 because
the source is barren". Fixed server-side: `handle_assign_labor` (and the `cultivate`/`corral` policy
shorthands, via `set_improvement_on_working_bands`) **seeds the touched source's `SourceYield` from its
pre-commit forecast** right after mutating the `LaborAllocation` (`server.rs::seed_source_yield` →
`LaborAllocation::set_source_yield`). Because forecast == actual (above), the seeded number is exactly
what the turn then pays under unchanged conditions — **no jump** — and it is the same number the
client's compose-time "Expected yield" row promises. Shape:
- **The expected take** is the one shared helper `fauna::forecast_expected_take(&SourceYieldForecast,
  workers, floor, improvement) = min(workers × per_worker_yield × build_dip(improvement),
  forecast.ceiling_at(floor))` — the crew's throughput, **dipped by whatever it is building**
  (`None` is the identity), against the ceiling at the **assignment's own floor**. Once the
  improvement *completes* the source is `::managed`, whose ceiling is its `managed_production` at
  every floor, so this one lookup covers both sides of every investment. The client preview, the seed,
  and the forecast==actual tests all call it.
- The kind-specific seeds `forage::forage_source_yield_preview` / `fauna::hunt_source_yield_preview`
  compose the full row through the shared `forecast_source_yield`: `actual` = the expected take,
  `sustainable` = the same MSY value the resolution path records (a *managed* source — **rung 3 only**
  — reads `sustainable == actual`, no ⚠), `workers_needed` = the same overstaffing signal the resolution
  path writes (the continuous inversion for a forage patch; the **peak-drop carry crew**
  `hunt_haul_workers` off `SourceYieldForecast::ceiling_at` for a whole-animal source, so the seed
  matches the client's max-useful cap), and `wasted` = the understaffing mirror. No new formula, no new
  config lever.
- **BOTH seed arms resolve the ASSIGNMENT's own kit tier.** The Hunt seed reads carry through
  `EquipmentConfig::hunt_per_worker_biomass_capacity` (the **sled**) and **attack** through
  `EquipmentConfig::hunter_profile`; the Forage seed reads
  `EquipmentConfig::forage_per_worker_biomass_capacity` (the **baskets**). Those are the same seams
  `advance_labor_allocation` reads (see `equipment.md`). It has to: a band-agnostic equipped rate
  would promise a sledless band a kitted haul or a bare-handed crew a basketful, and since the take
  resolves through the fight (`docs/plan_hunt_through_combat.md` §4) a band-agnostic *attack* would
  promise a bare-handed band a mammoth. forecast == actual is exactly what that breaks.

  Since kit selection the tier is masked by **the crew's chosen kit** as well as by the band's wear
  (`equipment.md` → "A kit is a MASK"), and `seed_source_yield` reads that choice off the assignment
  `set_assignment` has just stored rather than off the band — so the seed and the turn resolve the
  identical mask, and a crew sent out with `none` is previewed bare-handed.
- **The fight is a forecast term now**, threaded as `fauna::HuntingParty` through `hunt_forecast`,
  `hunt_source_yield_preview`, `project_realized_hunt`, `project_arrivals_hunt` and
  `forecast_production_and_take_at`, so all six take/forecast paths resolve the *identical* fight via
  the one `fauna::resolve_hunt_fight` helper. A projection cannot know the tick it is projecting, so
  it resolves at `fauna::HuntDraw::EXPECTED` — **no draw at all**, rather than the stand-in seed the
  first cut used (`FORECAST_FIGHT_SEED` survives only as the unread stream seed a quantile-mode fight
  hands to `resolve_fight`). At the shipped `combat_config.hit_chance` of `1.0` the fight makes no
  draw either way, so this is bit-identical to what the seed produced; what it buys is that a sub-1
  chance now yields the **expectation** instead of one arbitrary sample. See "THE INVARIANT IS
  RESTATED" below.
- **Only the source the command touched** is seeded (other sources keep their real actuals), and only
  where the turn would actually pay: out of `band_work_range` / past the hunt leash, an unseeded patch
  or a vanished herd keeps its zero row, and a **genuinely barren source still seeds `0.0`** — `+0.00`
  stays reachable, and correct, there. Consequence (intended): a fresh assignment now *previews* its
  contribution to the Food-line net rate + the Gathered/Hunted breakdown, and can pre-trip the
  overdraw ⚠ if the chosen floor draws below the food peak (`components::floor_overdraws`) — ⚠ is a
  leading flow signal by design.
- `LaborAllocation` now keeps `last_yields` **index-aligned with `assignments`** across every mutation
  (`set_assignment`/`normalize`/`clear` — the snapshot zips the two by index, so a row left behind by a
  removed assignment used to be attributed to the *next* source). New rows default to
  `SourceYield::ZERO`.
- **The seed carries the RANGE, and it is the only row that does.** `forecast_source_yield` takes
  `combat_config.forecast_range_sigmas` and fills `SourceYield::range` from
  `fauna::forecast_take_range`; the resolved arms of `advance_labor_allocation` fill
  `YieldRange::certain`, because a take that has happened has no distribution left. Both webs pass
  through the one `forecast_source_yield`, so the plant side's structural point-ness and the animal
  side's real band are one code path.
- **A FLOOR change re-seeds too**, not just a staffing change: the floor is a mutable property of the
  same source (`LaborTarget::same_source` ignores it), so `handle_assign_labor` runs the same seed
  after replacing the assignment. `changing_the_floor_reseeds_the_expected_yield` sweeps the dial
  rather than two stances, because a re-seed path that only fired at the four values the retired
  stance axis named would pass a two-point check and fail in play.
- **The floor itself fails closed**: `floor_is_valid` (finite, `0.0..=1.0`) rejects an out-of-range
  value with a command failure instead of clamping, and an absent one becomes
  `DEFAULT_ESCAPEMENT_FLOOR`. A clamp would turn a typo into a quiet policy change on the one number
  the harvest model turns on.
- Guarded by `server::tests::{assigning_forage,assigning_hunt}_workers_seeds_the_expected_yield_before_the_turn`,
  `resolved_{forage,hunt}_yield_equals_the_seeded_yield` (the no-jump property),
  `changing_the_floor_reseeds_the_expected_yield`, `a_barren_source_seeds_zero`,
  `unassigning_a_source_drops_its_yield_row`.

### The crew floor is ONE definition, reachable from BOTH halves of the row

`workers_needed` is written in **two** places — the resolved turn (`advance_labor_allocation`'s three
telemetry arms) and the assign-time seed (`forage::forage_source_yield_preview` /
`fauna::hunt_source_yield_preview` → `fauna::forecast_source_yield`) — so the crew rule has to live
where both can reach it. It does: **`intensification::source_crew_needed(standing_crew, take_workers)
= max(...)`**, on the rung engine, with the *standing* half supplied per web:

| web | standing crew | resolved by |
|---|---|---|
| plant | the building rung's `crew_needed` | **`LadderConfig::build_crew(improvement)`** — `NO_BUILD_CREW` for a pure gather, and for both animal rungs, which size a crew off the herd |
| animal | the herd's `herders_needed` | `fauna::herd_herders_needed`, or `would_be_herders_needed` while a build is in flight (the ownership-lag rule above) |

**The seed used to pass `0` for the plant standing crew**, so `forecast_source_yield`'s continuous
branch inverted the *dipped* take alone: a patch staffed to the `plant:tended` rung's crew of 2 had its
compose sheet say *"max 2 workers useful here"* (which reads `cultivateCrewNeeded` off the wire) while
the tile card beside it said *"only 1 of 2 working"* — **the same patch, the same frame, the same
(correct) yield**. It self-healed on the next turn's resolve, which is exactly why it survived: the row
was wrong only while the player was looking at it. Pinned as a *relation* by
`labor::a_patch_being_cultivated_seeds_the_same_build_crew_the_turn_resolves` and its animal twin
`labor::a_wild_herd_being_tamed_reports_its_full_crew_without_the_ownership_lag` — **a test that reads
only the resolved turn cannot see this class of bug at all.**

Two consequences worth stating, because both are places the floor now applies where it visibly did not:
- **A source that yields nothing in either currency reports its standing crew, not `0`.** A build crew
  (or a herd's keepers) is owed whether or not the source pays this turn — which is what the resolved
  arms already said.
- **A finished Field still floors on a stale verb.** The once-per-source "nothing left to build" test
  hands a second crew's `Sow` back *after* the Field arm's early return, so for one turn a band can
  hold a verb for a rung that is already built, and the seed prices that same stale verb. Whichever
  number is right there, both halves say it.

### An out-of-range source is ABANDONED, not parked at `+0.00`

A Forage assignment whose tile falls outside `band_work_range` **lapses**: `advance_labor_allocation`
drops the assignment, removes its `last_yields` row with it, and the workers return to the pool — the
plant twin of the Hunt leash lapse. It fires the same turn the move takes the band out of range, because
`advance_band_movement` runs earlier in the Population stage, so labor reads the fresh position.

Keeping it was the other half of the `+0.00` story. A kept assignment reads a *correct* `+0.00` forever
while the tile still renders as worked and its workers stay booked — indistinguishable from a barren
source, and the labor is silently dead. **`+0.00` must mean "this source is barren", never "your band is
elsewhere".**

**A patch gets no leash and a herd does, for a reason.** A herd MOVES, so `hunt_leash_tiles` buys the
band time to follow it; a patch is fixed, so out of range can only mean the band walked away — a
decision, not a drift.

Every other zero-yield forage path — no food module, a dead season, an unseeded patch — still **keeps**
its assignment: those are source conditions that can recover in place.

The abandonment pushes a `CommandEventKind::Forage` feed entry (`status=lapsed reason=out_of_range
x=… y=…`) naming the tile, so the player is told rather than discovering a dead row. Surfacing that log
player-facing is issue #272's notification system.

Guarded by `labor_allocation::forage_lapses_when_the_band_walks_out_of_work_range`, which asserts in the
same run that an in-range band's assignment survives, so "lapse" cannot silently widen to "always drop".

### RETIRED: trade goods were a BAND-LOCAL store, and now there is no such store at all

`TRADE_GOODS` was a third key on `PopulationCohort::stores` beside `FOOD` and `FODDER`, credited by
five sites — the Field harvest, the drawn-down forage take, the pen harvest, the wild hunt and the
expedition's delivery. **Arc #527 retired it**, because those five writes had no reader: there was no
`take(TRADE_GOODS)` anywhere in the workspace, and beside every one of them sat a
`credit_material_yield` banking the same take's concrete hide, bone and fibre.

**What survives is the shape, and it is the material store's now.** Everything this section argued for
a commodity key holds for a material batch:

- the snapshot ships batches generically (`snapshot/crafting.rs`), so a new material needs no schema
  change;
- `balance_supply_networks` pools them per **`(material id, band key)`** (`supply::MaterialKey`), so
  same-faction bands inside `SupplyNetworkConfig.reach_tiles` share them and bands beyond it do not —
  and pooling can never average a mammoth hide into a hare pelt;
- a batch's amount is fixed-point, so per-turn flows accumulate instead of rounding to zero.

**`FactionInventory` survives on the start-profile path alone**, and now carries no `trade_goods`
grant either: `seed_starting_inventory` (`systems/worldgen.rs`) writes whatever a
`StartProfileOverrides::inventory` names, and nothing spends it.

#### `accessibleStockpile` is an unread wire table, and `reach_tiles` is the real radius

`PopulationCohortState.accessible_stockpile` is **always `None`**. The field once carried a copy of
the faction's `FactionInventory` stockpile onto any band whose home tile sat within
`StartProfileOverrides.stockpile_access_radius` (Manhattan distance) of the faction's **start
position** — the position the campaign seeded the faction at, not any band, node or route. Nothing in
the manual or the design docs describes such a rule, so the readout it fed was retired along with the
lever, the `DEFAULT_STOCKPILE_ACCESS_RADIUS` default and the distance test; a band's own
`cohort.stores` is the only store it has.

**The band-to-band radius that does exist is `SupplyNetworkConfig.reach_tiles`** (default `3`) — a
different mechanic in every respect: it connects same-faction bands to *each other*, and
`balance_supply_networks` **equalizes their `stores`** rather than publishing a second store beside
them. Reaching for "bands near each other can pool without a route" means reaching for that lever.

The FlatBuffers table survives unread on purpose: `AccessibleStockpile` /
`AccessibleStockpileEntry` (`sim_schema/schemas/snapshot.fbs`), their `*State` twins
(`sim_schema/src/state/population.rs`), the codec (`sim_schema/src/codec/population.rs`) and the
client decoder all stay, because deleting a FlatBuffers field costs a schema rebuild plus a
decode-guard golden re-record for a table nothing reads. It simply always serializes as absent. The
decode guard still exercises the decode path for it — that fixture is **synthetic**
(`xtask/src/decode_fixture.rs` builds it, not the sim), so its golden is independent of what the
capture publishes.

### `IntensificationKnowledgeState` publishes a CAPABILITY beside its four rung gates

`snapshot_intensification_knowledge` emits five meters, and the fifth is a different kind of thing.
Cultivation / Seed Selection / Herding / Penning are one per **rung transition** — a rung waits on
each. **`foddering` (2007) is a capability**: no rung waits on it, the pen rung *teaches* it
(`intensification_ladder.json`, corral's `earns_knowledge`), and what it unlocks is every fodder seam
a faction has — the pen's hay draw, the pen's `K` fodder term, and the **wild** forage patch's
`FODDER` credit in `systems/labor.rs`. The design ruling behind that credit gate is
`.claude/rules/core_sim/flora.md` → "Wild fodder is gated at the CONSUMER".

**It is published because a rate alone cannot answer the question the client is asking.**
`ForagePatchState.fodderPerBiomass` states what the **land** pays and is deliberately knowledge-blind
(the gate lives at the credit site, so the rate seam stays commodity-generic). Without the capability
beside it, a viewer holding a patch row cannot tell a **refused** fodder credit — real hay, no
Foddering — from an **absent** one, and composes an account the sim will discard. That is issue #485.

> **`foddering` must stay in this function's all-zero skip**, and it is the one meter there that does
> not fall out of a lower rung. The four gates chain (you cannot hold Penning without Herding), so
> dropping any one of them from the condition changes nothing; dropping `foddering` silently loses the
> row for a faction whose only ladder progress is Foddering. Pinned by
> `snapshot_intensification_knowledge_reports_foddering_on_its_own`.

**Appended last, after `penning`.** The table is append-only, and the field's Rust struct, codec entry
and capture all read `foddering`; `sim_schema/src/lib.rs`'s roundtrip asserts on the **decoded**
`fb::IntensificationKnowledgeState::foddering()` rather than the in-process struct, because a field
that never reached the codec still passes an in-process assertion.

---

