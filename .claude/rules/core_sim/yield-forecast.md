---
paths:
  - "core_sim/src/{labor_config,orders}.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/snapshot/**"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/tests/labor_allocation.rs"
  - "core_sim/tests/bench_shed.rs"
---

<!-- Extracted verbatim from lines 42-42;3430-3568 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Pre-commit yield forecast (per-source, on the wire)

## Config files

| File | Purpose |
|------|---------|
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` (**no longer the wild biomass→food rate** — since #433 every patch converts at the share-weighted average of its own basket, and this survives as the **empty-basket fallback** plus the rung-3 quality normalization baseline), and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield`; **the whole per-stance lever set — `surplus_multiplier`, `market` (entirely, including its 4× `trade_goods_multiplier`) and `eradicate.take_fraction` — is DELETED by the harvest floor arc**, which replaced four fixed rates with one floor the player carries, after which *no option carries a factor of any kind* (`docs/plan_harvest_floor.md` §4) — **plus the Phase 1a `cultivation` block** — the plant ladder's **two rung payoffs (slice 7)**: **`tended_regrowth_gain` (1.0, rung 2 — NEUTRAL since Flora Roster S2, `docs/plan_flora_roster.md` §4.3: a tended stand regrows exactly as fast as wild. It began as the plant twin of `husbandry.pastoral_gain`, but once S1 made competitor-removal explicit a growth boost DOUBLE-COUNTS it, so tending pays through composition + conversion and the rung-2 "wild < tended" guarantee moved to the roster's own bar, `core_sim/tests/flora_roster.rs`; kept as a playtest dial in case a small boost is wanted back)** and **`field_provisions_per_biomass` (0.02, rung 3 — a managed rate on the standing crop, no drawdown, floor axis collapsed, because at rung 3 the source is YOURS)**; both PLAYTEST DIALS. `validate()` still enforces `tended < field`, now `field_provisions_per_biomass > tended_regrowth_gain × regrowth_rate/4 × provisions_per_biomass × tended_conversion_gain`, evaluated at tending's saturated best case so the crop's own rate cancels and the check stays scale-free; the `tended_regrowth_gain` check forbids only the INCOHERENT `< 1.0` (tending grows a stand slower than wild), not `<= 1.0`. **Plus the #433 pair `tended_weeding_gain` (1.5) / `tended_conversion_gain` (2.0)**, both validated finite and `>= 1.0`: the first is how far rung 2 **weeds** the favored species' share (`min(1.0, share × gain)`, the increase taken from the least abundant remaining species first), the second the conversion multiplier on that species' **whole yield vector**. Neither touches `K` — **the land owns `K` and no rung below 4 raises OR lowers it**. The retired `tended_concentration_gain` / `field_concentration_gain` pair multiplied the tile's `K` by `min(1, share × gain)` and **discarded the remainder**, which is the bug #433 fixed. See "Committing a patch to one plant". The retired `tended_provisions_per_biomass` (0.01) made rung 2 a *managed* rate a full rung earlier than the animal side's, so a tended patch could not be over-farmed and every policy paid the identical number (**the plant rung-2 BUILD dials — the old `progress_per_turn`/`decay_per_turn`/`cultivating_yield_fraction` — moved to `intensification_ladder.json`'s `plant:tended` rung**, and in slice 4 **the earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` moved to that file's ladder-level `knowledge` block** too, so both food webs climb *and learn* on the same numbers) (Rung 1a: cultivation is the explicit **`Cultivate` improvement**, which names its **own crew** (`cultivate … <workers>`) — the rung's `yield_fraction_while_building` is retired, so the gatherers beside the build carry exactly what they carried before and the build banks its own crew's work units (`docs/plan_standing_upkeep.md` §2.2); at 1.0 the completed tended patch is worked place-local, gathered to its floor on the (now neutral, = wild) tended ecology — so a *bare* patch pays exactly wild, and its yield advantage over wild comes from a **committed crop** (weeding + conversion, #433), not a regrowth boost — and goes feral if abandoned. Rung 1b: working a **wild** patch under a stewardship policy earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate verb — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**; the accrual is the ladder's, driven off the rung — see "The knowledge pattern"); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
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
> `larder_delta == food_income − food_consumption − raid_forfeit`, and fodder never touches the
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
>   stays on the row as the long-run MSY reference; the ⚠ is `components::take_overdraws`.
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
> What ships instead is the **terms**: `biomass`, `carryingCapacity`, and the
> source's **per-biomass yield vector** (`provisionsPerBiomass` / `fodderPerBiomass`,
> — the patch's basket-averaged rates, or the herd's own `HuntYield`). The client
> composes
>
> ```text
> ceiling(floor)   = max(0, B − floor·K) × <account>PerBiomass
> expected(workers) = min(workers × perWorkerYield, ceiling(floor))
> ```
>
> **THERE IS NO BUILD TERM IN IT** (`docs/plan_standing_upkeep.md` §2.2). The dip is retired: a build
> is raised by the band's **`builders` pool** rather than by the crew on the tile (§2.5), so the
> gatherers an `expected(workers)` describes are gathering and nothing is being multiplied. `expected` no longer takes a rung at all, which is what makes the curve one
> expression instead of one per verb.
>
> **ON THE ANIMAL WEB THAT `min()` HAS A THIRD ARM, and leaving it out overstates a light-bodied
> species' take by ~30×** (`docs/plan_hunt_through_combat.md` §2). Engagement caps how many animals a
> party can *reach* at all — `HerdTelemetryState.engageRate`, appended for exactly this:
>
> ```text
> reach(workers)    = workers × engageRate × bodyMass × <account>PerBiomass
> expected(workers) = min(crew term, ceiling(floor), reach(workers))
> ```
>
> It is a term rather than an answer for the same reason the two beside it are: linear in the crew and
> exact. **`engageRate <= 0` means "no engagement stage" and the term is dropped** — the wire's finite
> reading of the sim's `f32::INFINITY`, which is what an unresolvable species answers and what the
> plant web (never publishing the field) is.
>
> **A PEN publishes that same sentinel while the sim bounds it by a real number.** `engage_rate` is
> filtered on `is_corralled()` in `snapshot::subsistence`, so a penned row drops the term client-side;
> sim-side the tend branch, the forecast and both projections all bound the collection by
> `fauna::herd_engage_rate` — the species' rate times `husbandry.pen_engage_gain`. The shipped gain of
> `20` keeps the keepers' *carry* binding first on every pennable species, so the two readings agree
> except where the handling arm is genuinely reached. Measured before it shipped: a Wild Fowl herd with one hunter
> read **307 birds/turn** on the compose sheet (one hunter's 40 biomass of carry) against a take of
> **10** — the sheet promising 30× what the sim would pay, for the whole life of the field's absence.
> Pinned on the exported wire by
> `hunt_yield_vector::the_exported_terms_reproduce_the_engagement_bounded_take`.
>

> **This is a deliberate, narrow exception to *"the sim exports the answer"*, and the exception is
> sound for a stated reason.** That rule exists because a hunt take is rounded to WHOLE ANIMALS —
> `floor(ceiling / bodyMass)` is not linear, so no client can re-derive it and the sim must hand over
> the result. This expression is different in kind: **linear and exact**, so a client evaluating it
> lands on the number the sim would. The division of labour is therefore **the client draws the curve,
> the sim states the take**: `SourceYield.actual` for the *committed* assignment is still the sim's
> answer, quantisation and all, and the chart is a projection rather than a promise.
>
> **The four `*BuildFraction` slots are `(deprecated)` and no longer written**
> (`ForagePatchState.cultivateBuildFraction` / `sowBuildFraction`,
> `HerdTelemetryState.tameBuildFraction` / `corralBuildFraction`). They carried the rung's
> `yield_fraction_while_building`, which retired with the dip; the slots stay because FlatBuffers
> field ids are positional. What rides those tables now is the upkeep quartet — see "The standing
> upkeep on the wire" in `intensification.md`.
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
> | engagement bound | **term** — `HerdTelemetryState.engageRate` | `workers × engageRate × bodyMass` is linear in the crew, exactly like the carry term beside it |
> | a build's crew output | **term** — `buildWorkPerWorkerTurn` on both source tables | what ONE worker banks per turn at the food peak; `workers × this × floor/peak` is linear in the crew. Published rather than left a client `1.0` because `intensification::build_work_per_worker_turn` is a **sum of terms** with one term today |
> | a build's gear contribution | **terms** — `buildWorkPerWorker` × `buildWorkSaturatingCrew` on `PopulationCohortState.kitTiers[]` | coverage arms a **prefix** of a party, so the total is `min(workers, units held) × worth` — piecewise-linear and **saturating**. Both terms are facts about the **band's ledger**, so they ride the kit row and not a source row: an unstarted rung still has them, and a kit picker re-prices the whole estimate off the row of the kit under the cursor |
> | a build's turn count | **BOTH** — the terms above, and the answer `buildTurnsRemaining` | the sheet has a crew stepper and the tile card does not, so both are needed and neither replaces the other. See below |
> | the take | **the answer** — `SourceYield.actual` | `floor(ceiling / bodyMass)` is not linear; no client can re-derive it |
> | raid trip length | **an answer, ASKED FOR** — `HuntTripForecastQuery` on the command socket | a bounded forward simulation; there is no expression to hand over, and it depends on the asking band's kit and wear, which no per-herd row carries |
> | the growth curve | **sampled answers** — `regrowthSamples` × `REGROWTH_CURVE_SAMPLES` | see below |
>
> **THE BUILD'S TURN COUNT IS THE ONE ROW THAT SHIPS BOTH SHAPES, and the reason is the stepper.**
> `buildTurnsRemaining` is the sim's answer for the crew **already** working the source — the right
> and only thing for the tile card and the herd drawer, which have no crew control. A compose sheet
> has one, and *"add hands and watch it drop"* is the whole point of the field it sits next to, so it
> evaluates the form itself:
>
> ```text
> gear(w)  = min(w, buildWorkSaturatingCrew) × buildWorkPerWorker      ← the KIT row
> turns(w) = ceil((workCost − workDone − gear(w)) / (w × buildWorkPerWorkerTurn − meterRotPerTurn))
> ```
>
> The client draws the curve; the sim states the answer — the same division as the take, one row
> further along. **Evaluated at the committed crew and floor the two must agree exactly**, and that
> equality is the whole safety argument for the arrangement: a sheet that could disagree would lie
> about the very decision the card then reports differently. Pinned on the **exported snapshot** in
> two places — the gear term against the source's own `buildWorkFromGear`, then the whole form
> against `buildTurnsRemaining` — across both gear regimes (the saturation binding and inert), by
> `core_sim/tests/build_turns_closed_form.rs`.
>
> **WHICH TABLE A TERM RIDES FOLLOWS FROM WHAT IT IS A FACT ABOUT, and the gear pair is the case that
> makes it concrete.** Both halves of `gear(w)` are properties of the band's *kit and ledger* — units
> held, and each unit's reach — so they ride `kitTiers[]`, per **offered** kit. That is what lets an
> unstarted rung be quoted at all (no crew has worked it, so no source-side scratch exists) and what
> makes the sheet's **kit picker** re-price the estimate: picking a different kit reads a different
> row. A source-row copy would answer for whichever kit the committed crew happened to carry, which
> is the frozen-readout defect one control over. The source rows keep `buildWorkFromGear` — the
> **resolved** contribution for the crew that worked it this turn, which the running readout wants
> and no stepper can move.
>
> The **food peak** is not published: the client holds its own `SourceForecast.FLOOR_FOOD_PEAK`, which
> must equal the sim's `fauna::MSY_BIOMASS_FRACTION`. The same file pins them together by **reading
> the GDScript's own `const`** rather than a Rust transcription of it — a transcribed literal
> compares the sim with itself, so the client script could be retuned with no test firing.
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
> **It takes no `improvement`, and since the dip retired there is nothing left for one to change.**
> A ceiling is purely `max(0, B − floor·K) × rate` — linear and exact in terms already on the wire —
> and the crew term beside it is the take crew's alone. The build is **not in the take at all**
> (`docs/plan_standing_upkeep.md` §2.2), so `forecast == actual` with a build in flight holds for the
> stronger reason that neither side has a build term: `BuildDips` and `LadderConfig::build_dip` are
> deleted, and `fauna::forecast_expected_take` multiplies nothing.
>
> The standing-stock clamp inside `ceiling_at` is **belt-and-braces and inert** — an escapement
> ceiling is `B − floor·K ≤ B` for any floor `≥ 0` — and kept because a future ceiling that *could*
> exceed the stock must not silently over-report. `stock_cap` stays populated for wire stability.
>
> A **corralled herd** has `stock_cap: None` — it is never drawn down by the compose sheet's own
> reading. **A sown FIELD is not one**: the plant web's managed harvest is retired and a Field is
> drawn down like any other stand. *"This rung is not offerable here"* used to be said by publishing a
> dip of `0`; with the fractions retired it is said by the rung's own state (`isField` / `corralled`)
> and by `buildTurnsRemaining`'s `NO_BUILD_TURNS_ESTIMATE`, which is what the top of a ladder answers.

`ForagePatchState.tendedYield` and, on a herd, `corralYield` are what the source will pay **once the
improvement completes**, so the client can show **"now X → then Y"** *before* the player commits the
hands. (Sim-side the payoff is `SourceYieldForecast::managed_yield` — the two
rung-3 verbs are kind-exclusive, so one field serves both.)
**The `Tame` rung has its own payoff twin: `HerdTelemetryState.pastoralYield`** (sim
`SourceYieldForecast::pastoral_yield`) — what a Sustain hunt pays **once the herd is tamed**, so the
client can render Tame's `→ +Y` rather than only the wild hunt beside it, which hides that taming
out-yields wild hunting. `0` on a source that never
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
and these are rates; `B − floor·K` is `K/2` on every rung at `B = K`. **A live pen reads the same
sustained MSY**: with the managed harvest retired the pen is drawn down like every other rung, and the
rate it settles at *is* that MSY, so the payoff line and the take coincide at the operating point
rather than being two shapes. Pinned by
`fauna::tests::the_tame_rung_advertises_its_payoff_above_wild_sustain`.
- `perWorkerYield` = food/turn one worker contributes (throughput → provisions; **forage folds in the
  tile's `seasonal_weight`**, as `forage_take` does — it can be `0` in a dead season, so consumers must
  not divide by it; hunt has no seasonal factor).
- **`perWorkerBiomass` = the same throughput in BIOMASS**, before any account conversion:
  `per_worker_biomass_capacity × seasonal_weight` on a patch (`forage::forage_per_worker_biomass` at
  the *equipped reference* rate — a band's own basket tier rides its cohort row instead, see
  `equipment.md`; `0` in a dead season) and `labor_config.hunt.per_worker_biomass_capacity` on a herd (no seasonal
  factor). It is the term the **crew** half of the panel divides by — *"clear it now"* is
  `(B − floor·K) ÷ carry` and *"hold it after"* is the regrowth at that floor over the same
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
- Client composition: `expected(workers, floor) = min(workers × perWorkerYield, ceiling(floor))`,
  `max_useful_workers(floor) = ceil(ceiling(floor) / perWorkerYield)`. **Neither takes a rung** — the
  build is not in the take (`docs/plan_standing_upkeep.md` §2.2).
- **THE PLANT CREW FLOOR IS RETIRED WITH `crew_needed`.** It existed because `workers_needed` was
  inverted out of a **dipped** take, so a patch under a 25-turn Cultivate asked for *fewer* hands than
  the same patch merely gathered. With each role staffed on its own row there is no blended count to
  floor: `workers_needed` is the **take**'s own hands, `upkeepWorkersNeeded` is the **keeping**'s, and
  the builders are the band's own pool.
  `intensification::source_crew_needed`, `LadderConfig::build_crew` and the `cultivateCrewNeeded` /
  `sowCrewNeeded` wire slots are gone (the slots `(deprecated)`). **`herdersNeeded` /
  `herdersNeededIfManaged` keep their own fields** — a herd's keeper count is a fact about the herd,
  not about a build — and no longer fold into `workers_needed`.
- A **corralled herd** is *yours*, so **the floor axis collapses**: `ceiling_at` returns its
  `managed_production` at every floor (`SourceYieldForecast::managed`). **A sown FIELD is no longer one
  of these** — the plant web's rung-3 managed harvest is retired, so a Field is floor-live and drawn
  down through ordinary `forage_take`; see ":343" below, which states the same for the pen's own
  collection. **The worker cap does
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

> #### ⛔ AND `actual` MEANS THE TAKE AFTER THE **NEXT** LOGISTICS, NOT AGAINST TODAY'S STOCK
>
> A forecast is read between turns — the query answers a client, the capture publishes in the
> Snapshot stage — so every caller sees the source **after** the Population take. Priced against that
> stock the forecast is a whole turn stale, and on a worked source the staleness is the entire take.
> So `hunt_forecast` and `forage_forecast` resolve **both** stock terms off the regrow-first
> projection (`fauna::next_turns_quarry` / `forage::next_turns_stand`), which is what makes the
> identity hold on a source sitting at its floor. `fauna.md` → "A FORECAST REGROWS FIRST" carries the
> mechanism and the play autopsies.
>
> **Both terms, or neither.** Threading only the growth forward leaves the escapement arm stale and
> the identity still broken, one term smaller.
>
> **What this costs a harness:** a fixture that freezes a stock and reads a forecast is quoting a turn
> the sim has not run. It must resolve a turn in stage order (Logistics → Population) or quote the
> forecast **before** the regrowth. Two fixture shapes stopped being available with it — a bit-for-bit
> equality between a seeded and a resolved realized yield (its old pass came from a frozen herd taking
> nothing; it is bounded now), and *"a stripped patch is barren"*, since a stripped patch reseeds and
> pays next turn. Barren has to mean barren **ground**.
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
  A corralled herd's `managed_yield` is **gross**, and now also net — a pen's feed is fodder (its
  footprint's grass and the band's hay), so nothing in provisions stands against it.
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
shorthands, via `queue_build_on_working_bands`) **seeds the touched source's `SourceYield` from its
pre-commit forecast** right after mutating the `LaborAllocation` (`server.rs::seed_source_yield` →
`LaborAllocation::set_source_yield`). Because forecast == actual (above), the seeded number is exactly
what the turn then pays under unchanged conditions — **no jump** — and it is the same number the
client's compose-time "Expected yield" row promises. Shape:
- **The expected take** is the one shared helper `fauna::forecast_expected_take(&SourceYieldForecast,
  workers, floor) = min(workers × per_worker_yield, forecast.ceiling_at(floor))` — the take crew's
  throughput against the ceiling at the **assignment's own floor**. **It takes no `improvement`**: a
  build is staffed in its own right, so what the gatherers carry does not depend on what the builders
  beside them are doing (`docs/plan_standing_upkeep.md` §2.2). Once the
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

  > **⛔ A PEN FORECASTS NO FIGHT, BECAUSE ITS PAYOUT RESOLVES NONE.** A corralled herd never reaches
  > `systems::hunt_take`: the Hunt arm's tend branch `continue`s before it and walks the animal out
  > (`fauna::animals_handled`). So `hunt_forecast` builds `fight: NO_FIGHT_STAGE` for it, and the three
  > readings that price a pen — `forecast_production_and_take_at`, `project_realized_hunt`,
  > `project_arrivals_hunt` — each fork on `Herd::is_corralled()` and run the tend branch's own three
  > terms: the room above the floor, the keepers' handling, the crew's carry. `fight: Some(..)` for
  > **every** herd ran an engagement, a retreat and a fight the pen does not, gated on the quarry's
  > `defense` and the crew's *hunting* kit — so a bare-handed band with a penned **Wild Aurochs**
  > (`defense 6`) was quoted `0`, projected a steady `0` and an empty arrival schedule, and was then
  > paid a real take on the turn. The quantised readings call `animals_handled` itself rather than
  > re-composing it, so the quote and the payout are one expression; the smooth one
  > (`project_realized_hunt`) drops only the whole-animal floor, as it does on the wild arm.
  > Guarded on the exported wire by `hunt_useful_crew_on_the_wire.rs`
  > (`a_bare_handed_pen_is_quoted_the_take_the_turn_pays`,
  > `a_pens_quote_is_its_payout_at_every_keeper_count`,
  > `a_bare_handed_pen_projects_a_steady_income_and_a_delivery`), with the wild arm's fight gate pinned
  > beside them by `a_wild_row_is_still_gated_by_the_fight`.
- **Only the source the command touched** is seeded (other sources keep their real actuals), and only
  where the turn would actually pay: out of `band_work_range` / past the hunt leash, an unseeded patch
  or a vanished herd keeps its zero row, and a **genuinely barren source still seeds `0.0`** — `+0.00`
  stays reachable, and correct, there. Consequence (intended): a fresh assignment now *previews* its
  contribution to the Food-line net rate + the Gathered/Hunted breakdown, and can pre-trip the
  overdraw ⚠ if the chosen floor draws below the food peak **and this crew can get the source down to
  it** (`components::take_overdraws`) — ⚠ is a leading flow signal by design.
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

### `workers_needed` IS THE TAKE'S OWN COUNT, and each activity states its own

`workers_needed` is written in **two** places — the resolved turn (`advance_labor_allocation`'s three
telemetry arms) and the assign-time seed (`forage::forage_source_yield_preview` /
`fauna::hunt_source_yield_preview` → `fauna::forecast_source_yield`) — and both now answer the same,
simpler question: **how many hands does it take to haul what this source offers?** It is inverted out
of an **undipped** take, because a building crew no longer changes what a gathering crew carries.

The **maintain** activity publishes its own count beside it (`upkeepWorkersNeeded` =
`ceil(upkeep_demand / PER_WORKER_OUTPUT)`, in keepers), and the **build**'s crew is simply the number
the player typed on the verb. A `max` across those units was the compromise a single allocation
forced, and it is what made a row read `workersNeeded: 1` beside `wastedYield: 0.80`.

**The floor it replaced was a real fix to a real defect, and the defect is gone rather than
re-solved.** `intensification::source_crew_needed` existed because the seed inverted a *dipped* take:
a patch staffed to the `plant:tended` rung's crew of 2 had its compose sheet say *"max 2 workers
useful here"* while the tile card beside it said *"only 1 of 2 working"* — the same patch, the same
frame, the same (correct) yield. With the dip retired the two halves invert the *same* number, so
there is nothing left for a floor to reconcile.

### THE ⚠ IS INTENT **AND** ABILITY — one predicate, one producer per web

`SourceYield::overdraws` is **`components::take_overdraws(floor, crew_biomass_per_turn,
peak_regrowth_in_band)`**, and nothing else may write that field. Two conjuncts:

- **INTENT** — `components::floor_overdraws(floor)`, unchanged: the dial is set below the food peak.
- **ABILITY** — the crew's per-turn throughput exceeds the **biggest one-turn regrowth anywhere
  between the floor and the stock standing today**. While it does not, the stock stalls at that
  point and holds, and a floor the crew never reaches is a floor nothing is drawn below.

**The intent half alone was the shipped bug, reported from play**: a Wild Boar herd at 85/105 with
four herders and a 39% floor flew `⚠ overdrawing` on the tile card — `+0.18 a turn` against
`+0.63 sustainable`, an *under*-draw by a factor of three — beside a compose sheet reading *"this
crew can't draw it that low. It settles at 92% and holds there — 16 herders would reach the floor"*.
Two surfaces, one question, opposite answers, because the mark read the **dial** and the sentence
read the **crew**.

**The ability half is a question about THROUGHPUT, never about this turn's take** — which is exactly
what keeps the first-harvest rationale intact. `actual > sustainable` remains the wrong test (a
stocked source's first harvest is accumulated stock and exceeds one turn's regrowth at every floor,
the peak included), and it is untouched here: at or above the peak the intent conjunct is already
`false`, so the ⚠ cannot fire however large the first haul.

**Why the peak in the band, and not "is the stock falling this turn".** The regrowth curve peaks at
`K/2` and an overdraw floor is by definition below it, so a crew descending from a full source has
the peak still to cross. A crew that merely out-takes *today's* regrowth can settle **at** the peak
and hold there for ever — which is the case the client's own gate got wrong before this landed.

**Two producers, one per web, because there are two growth curves** — the same split
`snapshot::patch_regrowth_samples` / `herd_regrowth_samples` already makes:

| web | producer | crew throughput | curve |
|---|---|---|---|
| plant | `forage::forage_take_overdraws` | `workers × forage_per_worker_biomass` (no engagement stage) | `fauna::reseeding_logistic_regrowth` at the patch's own `patch_ecology` |
| animal | `fauna::hunt_take_overdraws` | `min(carry, animals_engaged × stay_fraction × body_mass)` | `fauna::regrowth_delta_at` at the herd's own `herd_ecology` — **the seam that picks the curve**, logistic for a domesticated herd and `net_biomass_delta` otherwise, so the ⚠ samples what `regrow_biomass` will actually pay. Sampling the wild curve under a managed herd standing below its collapse fraction reads a *negative* regrowth where the real one is positive, and the ability conjunct then passes on a crew that cannot draw the herd down |

Both call `fauna::peak_regrowth_between` over `fauna::floor_reach_band`, and both feed the one
`take_overdraws`. The band is **anchored at `floor·K`, never below it**: a source already under its
floor hands over nothing, and what decides whether the crew holds it there is the regrowth at the
floor itself. `fauna::forecast_source_yield` no longer derives the flag — it is handed the answer and
applies only the `managed` veto (a rung-3 Field or pen takes at most its escapement MSY, so it cannot
overdraw whatever the dial says).

**`peak_regrowth_between` is exact, not sampled.** Both webs' curves are logistic above their
low-stock branch, so the only interior maximum either can have is the food peak; every other piece is
monotone and its maximum sits on an endpoint. It evaluates those three candidates rather than walking
`REGROWTH_CURVE_SAMPLES`, which is a **display** resolution and must not be what a verdict turns on.

**The engagement bound is in the animal crew term and the FIGHT deliberately is not.** Sizing on
carry alone would call a two-hunter party capable of drawing down a herd of fowl it can barely touch
— the error `fauna::hunt_engage_workers` already exists to keep out of the crew counts. The fight's
damage accumulates across turns (`project_realized_hunt` resolves it *inside* its loop for that
reason), so it has no per-turn rate to compare a regrowth against; leaving it out can only make the
crew look more capable, which leaves the ⚠ lit where a fight would have blocked it.

**The fixtures that could not see this.** Every overdraw test the arc shipped with staffs a crew that
trivially out-takes its source's regrowth, so all of them pass identically under the floor-only
predicate and under this one. `core_sim/tests/labor_allocation.rs` therefore carries
`TINY_PER_WORKER_HAUL` / `TINY_PER_WORKER_GATHER` fixtures on both webs
(`a_floor_this_crew_cannot_reach_is_not_an_overdraw`,
`the_plant_web_warns_only_where_the_gatherers_can_reach_the_floor`) beside the two that pin the
warning still firing (`a_crew_that_can_reach_a_below_peak_floor_warns_on_the_first_turn`,
`a_stocked_source_taken_at_the_peak_out_takes_its_regrowth_without_warning`). Reverting the ability
conjunct fails exactly the first two and nothing else, which is the measurement that says the older
fixtures are blind to it.

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

### The band's hay ledger — three fields, and the sim does the arithmetic

**A pen's upkeep changed currency, and the readout followed it.** `fodderStore` used to be a bare
stock on the band panel — no rate, no demand, no runway — beside a Food line that had income,
consumption, a runway in turns and an arrivals strip. Three cohort fields close it, the fodder twins
of `foodIncome` / `foodConsumption` / `turnsOfFood`, all in fodder units per turn:

| field | what it is | written by |
|---|---|---|
| `fodderNeed` | Σ over the pens this band keeps of each pen's `penHayNeed` — **the gap**, not the gross demand (`graze.md` → "The hay bill is published as the GAP") | `advance_labor_allocation`, accumulated by the corral arm onto `LaborAllocation::last_fodder_need` |
| `fodderIncome` | the hay this band's fodder Fields **harvested this turn** — `band_fodder_inflow`, which previously reached only the pens' `K_pen` term and never the wire | the same pass, onto `last_fodder_inflow` |
| `turnsOfFodder` | the runway: `fodderStore ÷ (need − income)` | the capture, through `larder_runway_turns` |

**⛔ THE SIM SUMS IT, AND A CLIENT MUST NOT.** The standing rule the retired `pen_feed_upkeep` was
minted under — the client renders, it does not sum — and on this ledger it is load-bearing rather
than stylistic: **herd rows are fog-filtered**, so a pen out of sight silently drops out of any
client-side total, while the band certainly still owes its hay.

**⛔ THE RUNWAY IS `larder_runway_turns`, SENTINEL AND ALL.** One phrasing for one concept: a client
reads `turnsOfFodder` exactly as it reads `turnsOfFood` and must not branch two ways on *"turns of
buffer left"*. `NOT_FOOD_LIMITED_TURNS` (`999`, now `pub` for exactly this reason — nothing should be
spelling the literal) is the reading for **anything that is not draining**: income that meets the
need, and a band with no pens at all. It is asked with an **empty arrival schedule**, which is what
selects that function's smooth `stock ÷ net drain` arm — a hay Field is a steady harvest into a
stock, where the food runway walks per-source arrivals because a hunt lands in lumps.

**The income is the RAW harvest, not the Foddering-gated share** stamped onto
`Herd::fodder_delivery_rate`. What was grown is a fact about the Fields; what a pen may draw is a
fact about what the faction has learned, and conflating them would tell a band its hay had failed
when it had merely not yet learned to feed it out.

`core_sim/tests/grazing_hay_readout.rs` pins all four fields off the **encoded envelope**, including
a real hay Field beside a pen it out-grows — which is what keeps `fodderIncome` from being a field
that is only ever asserted at zero.

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
  connected same-faction bands inside `SupplyNetworkConfig.reach_tiles` share them and bands beyond
  it do not —
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
different mechanic in every respect: it connects same-faction bands that have met to *each other*,
and
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

### Two ways of having no countdown, and the capture is what tells them apart

A source stores `Option<BuildTurns>`, and its `None` covered two different facts:

- **the estimate pass ran and had no number** — nobody works the source, its gate refuses, or a
  running build banked nothing and is genuinely **stalled**;
- **no estimate pass has ever run for this entry** — the player queued it since the last turn
  resolved, and the server re-captures after every command, so that frame reaches the client.

`published_build_countdown` (`snapshot/subsistence.rs`) is the one seam both webs' rows go through,
and it splits them: `sim_schema::BUILD_NOT_YET_ESTIMATED` (`-5`) for the second,
`NO_BUILD_TURNS_ESTIMATE` (`-1`) for the first. What each means, and why a client must not render
`-5` as a warning, is in `intensification.md` → "THE COUNTDOWN HAS SIX ANSWERS".

**The test is the estimate pass, never the meter.** A genuinely stalled build sits at `0%` too, so a
rule reading progress reproduces the defect it is fixing. What *is* knowable is that
`publish_build_chain` calls `publish_entry` for **every entry in the queue it walks** — whether or not
that entry has a quote — and `publish_entry` always stamps the entry's 0-based place, while the
Logistics decay passes (`forage::advance_cultivation`, `fauna::advance_husbandry`) clear the place
back to `NOT_IN_ANY_BUILD_QUEUE` every turn along with `build_turns_remaining` itself.

**Two terms, and each is load-bearing:**

| term | what it rules out |
|---|---|
| the source is in a band's **live** queue (`BuildKitIds::patch_is_queued` / `herd_is_queued`) | every unworked patch on the map, which also carries the cleared place, reading as a build about to start |
| its stamped place is still `NOT_IN_ANY_BUILD_QUEUE` | a genuinely stalled entry, which *is* live-queued, reading the same way |

The queue membership is read **live off the bands' own `build_queue`s** rather than off the
turn-written row, for the reason `buildKitId` beside it already is: the row's scratch lags a command
by a whole turn, and this state exists precisely in the frame before that turn. `BuildKitIds` was
already that index, so it grew a membership predicate rather than a second walk.

**The legs keep `-1`.** `published_build_legs` maps an undated leg to `NO_BUILD_TURNS_ESTIMATE` and
is untouched: `build_legs` is cleared by the same reset, so an entry no pass has reached publishes
**no legs at all** and the case cannot arise there.

`core_sim/tests/build_queue.rs` drives it on both webs — queued-with-no-turn, then one turn to a real
count, then a live-queued entry the pass reached and could not date — plus the unqueued-patch control.

## Shedding a crew the band can no longer field

`LaborAllocation::normalize(available, facts)` runs once per band at the head of
`advance_labor_allocation` and trims `Σ workers` back to what the band actually has. It answers the
one question a command-side clamp cannot: the band **lost people** since the command landed, so hands
already committed have to go somewhere.

**It fires only at zero slack.** Idle hands absorb a shrinking pool by themselves, so only a fully
committed band ever reaches the order below — it is an edge-case handler, which is why the order is
decided in code and there is no config lever competing with it.

### Every shed is announced, and a trim is a shed

It returns a `ShedCrew` per row it touched — `remaining > 0` for a row it merely cut, `0` for one it
destroyed outright — and `announce_shed_crew` pushes a `status=trimmed` or `status=lapsed` line for
each on that source's own feed channel. **Each of those lines names the band on a `band=` detail
token**, which is what carries the dock's per-row *"Work tab"* link — see `event-feed.md` → "The
`band=` token is what makes a loss line clickable".

The trim half was silent for the whole life of the pass: only destroyed rows were handed back, so a
crew going `6 → 3` on a band one worker short published a smaller number with **no event anywhere**.
From the player's side that is a crew they had just raised moving on its own, which is
indistinguishable from the command having been refused.
`a_crew_that_is_only_trimmed_is_announced_and_says_what_is_left` asserts on the **event**, because the
worker count was correct throughout and a test that read only the count would have passed the whole
time.

### The shedding order — the eleven steps, and why they are in that order

`ShedStep` (`components.rs`) is the list, one variant per step, walked top to bottom; the first step
that names a staffed row gives **one** hand, and the walk re-runs for the next hand. Nothing about it
is positional.

| | Step | Band |
|---|---|---|
| 1 | a **scout** | *Nothing is lost* |
| 2 | a **warrior**, if nothing threatens the band | |
| 3 | a **keeper above the keeping demand** — Agriculture first, then Husbandry | |
| 4 | a **builder**, while more than one remains and something is queued | |
| 5 | **thin the least-productive worked source that has two or more hands — and the crafting BENCH, ranked beside them** — "least productive" is the four-level test below, whose second level passes over a source still accruing knowledge | *Output falls, nothing ends* |
| 5b | **the crafting bench's LAST hand** — the job stalls, keeping its recipe, its progress and the pile it drew | |
| 6 | **empty the least-productive source carrying no improvement and no queued build** | *Something ends* |
| 7 | a **warrior**, unconditionally | |
| 8 | a **keeper below the demand** — improvements begin to rot | |
| 9 | **empty the least-productive improved source with no queued build** | |
| 10 | **empty a source carrying a queued build** — the row drops and the declaration goes with it | |
| 11 | **the last builder** — every queued build stalls | |

Below that, `ShedStep::LastHand`: a single worker on a single row, taken, and the row ends. Steps 6,
9 and 10 partition every staffed *source* row between them and the role steps name every staffed
*role* row, so the walk is already total and that arm is the assertion rather than a case.

**Thinning beats emptying, and that is the sharp line.** The builders have been a band-level pool
since `docs/plan_standing_upkeep.md` §2.5, so taking a hand off a source mid-build does not slow that
build at all — only **emptying** the row does, because an entry requires a row (§3.2) and dropping
the row drops the entry. The cliff is emptying, never building. Two consequences of the same
reasoning: **9 is worse than 8** (an improved source with no take crew still owes its upkeep and now
pays nothing, where rot is gradual and recoverable), and **7 sits after 6** (pulling the guard under
a real threat can cost people, which is worse than losing a row nothing was invested in).

**One hand per pass of the walk**, because the picture changes with every hand taken — a keeper
surplus falls, a two-hand row becomes a one-hand row, a builder pool reaches its last. A step that
*empties* a row still takes one hand, and that is not a coincidence: step 5 names every source row
with two or more hands, so by the time the walk reaches step 6 no source row has more than one.

**`normalize` does not hold the facts the order needs**, so `advance_labor_allocation` resolves them
into `ShedFacts` and hands them in — and **only the facts**: the steps are walked in one place, so no
seam knows half the order. Every fact is struck against the allocation the *player* left, before a
hand is shed:

| Fact | Resolved from |
|---|---|
| `threatened` | the **same trigger** `advance_predator_raids` fires on — a carnivore with `aggression > 0` inside `predators.raid_radius`. That pass runs straight after this one off the same herd positions, so a band the pack reaches this turn keeps its guard. A band whose tile will not resolve reads **threatened**: the guard is the reading that costs people when it is wrong |
| `spare_*_keepers` | `keeping_claims` — the **one** definition of the band's keeping bill, which `maintenance_shares` also splits its pools against — summed per web and divided by `build_work_per_worker_turn`, so the surplus is struck against the supply the split will actually make |
| `accruing_knowledge` | the source's rung names a lesson, the faction has not completed it, and the floor leaves practice to be had. It deliberately does **not** ask the escapement room the live credit is also gated on: that room comes from this turn's take, which has not happened yet, so this is *"is there a lesson here to lose"* — the conservative direction, which protects a row from being thinned and never exposes one. **Step 5 alone reads it, and reads it as a LEVEL** (below) |
| `improved` | `patch_at_risk_cost` / `herd_at_risk_cost` above `RUNG_UNSTARTED` — work on the ladder, finished or in flight |

`banking` and the keeping gear are therefore resolved **twice** per band: once here against the
pre-shed allocation, and once below against what survived, which is the reading the split funds. A
band whose builders row was emptied funds no head at all, and the split must not fund one it no
longer has the hands to bank.

**"Least productive" is FOUR levels at step 5 and THREE everywhere else, and the top one is the
player's own.**

1. **The row's `SourcePriority`** — `Low`, then `Normal`, then `High` (the variant order *is* this
   order, and the derived `Ord` is what reads it). See "The player's rank on a worked row" below.
2. **Is this row still accruing knowledge** — a learner ranks **last**, so it is passed over while
   any other candidate exists. **Step 5 only**: the four steps that *empty* a row
   (`least_productive_row`) hand a constant here, so their order is the three-level one — rank, then
   the two the shed has always used. **The rank itself is new**: before it, every step ordered on
   `pays_any_account` → `yield_per_worker` alone, so "unchanged" here means *unchanged since the rank
   landed*, not *untouched by it*. See the callout below.
3. **Does this row pay into ANY account** — food, fodder or materials (`pays_any_account`, read off
   the same retained `SourceYield`). A row paying nothing ranks below one that pays something, so it
   is shed first.
4. **Then `last_yields[i].realized ÷ crew`** — the row's own published headline yield, the number the
   band panel and the map annotation state, divided by the hands on it. Ties go to the earliest row,
   so the choice is stable.

> #### ⛔ THE LESSON SKIP IS A LEVEL, NOT A FILTER — AND AS A FILTER IT SILENCED A `Low` MARK
>
> Step 5 used to *exclude* a learning row from its candidate set and fall back to the unfiltered call
> only when **every** thinnable row was learning:
>
> ```rust
> least_productive_row(|i, a| thinnable(a) && !facts.source(i).accruing_knowledge)
>     .or_else(|| least_productive_row(|_, a| thinnable(a)))
> ```
>
> So a learner was struck out **before** `SourcePriority` was ever read. Reported from play: a band
> with three Forage rows (one `High`, two unmarked) and a **`Low`-marked five-hand hunt that was
> still learning** thinned both unmarked Forage rows `2 → 1` and left the marked row untouched. The
> `High` mark worked; the `Low` mark did nothing at all.
>
> **It is not a regression the rank introduced** — the hunt also carried the lowest yield per worker
> on the board (`0.054` a head against `0.15`–`0.165`), so the filter had always been able to protect
> the least productive row. What changed is that the player has now *said something* about that row,
> which is what makes the old behaviour read as broken.
>
> **The fix is 9b's own stated shape**: an explicit rank on top, the shipped ordering surviving as
> the tie-break beneath it. The knowledge skip is part of the shipped ordering, so it belongs below
> the mark — `least_productive_row_passing_over_lessons`, which is step 5's entry point and nobody
> else's.
>
> **At equal priority it is bit-identical to the filter**, which is the claim that made this a
> refactor rather than a retune: among candidates of one rank the minimum is the
> `(pays, yield, earliest)`-minimum of the **non-learners**, exactly what the filtered call returned;
> with no non-learners it is that minimum over all of them, exactly what the fallback returned.
> **The `or_else` is therefore gone rather than kept.** A filter that excludes every candidate
> returns `None`; a level that is constant across every candidate returns what the next level would.
> The only `None` left is *"`admits` named nothing"*, which the fallback could not fix either.
>
> **A level that leaked into steps 6, 9, 10 or the terminal would be a worse defect than the one it
> fixed** — by then the question is *which row ends*, and a lesson is not a reason to end a different
> one. `the_lesson_level_does_not_reach_the_steps_that_empty_a_row` pins it.

It is the retained telemetry rather than a fresh derivation: this pass runs before the take, so it is
the only yield reading that exists, and a second source here would order the shedding on a number the
player has never been shown.

> #### ⛔ LEVEL 1 IS A PRESENCE TEST AND MAY NEVER BECOME A COMBINED SCORE
>
> A hay Field and the five cash crops read **zero in both scalar accounts and are paid entirely by
> their materials rows** (`flora_config.json`), so a productive tobacco Field and a genuinely dead row
> both quote `0` provisions per worker. Under `realized ÷ crew` alone they *tie*, and which one was
> shed came down to list position — the thing nothing in this order is allowed to depend on.
>
> The obvious repair is a combined number, and `labor_config.json`'s `_comment_weeding` refuses
> exactly it: ranking by amount would mean *"comparing a food rate against a trade rate, an exchange
> rate this codebase does not have and should not invent"*. Asking only **whether** a row pays
> sidesteps the question — a presence check invents no exchange rate — which is why level 1 is a
> `bool` and not a term.
>
> **The levels are in this order so the standing behaviour cannot invert.** A food row pays *and*
> carries a positive per-worker yield, so it still outranks every non-food row: a band short of hands
> keeps its people on food and drops the tobacco. The presence test decides only the tie beneath that.
>
> **The rank above them is the same kind of thing and carries the same ban.** It is a lexicographic
> level, never a weight: multiplying or summing a stated preference with a food rate invents an
> exchange rate between two things even less comparable than two accounts. So is the lesson level
> between them — *"is there a lesson here to lose"* is a `bool` for the same reason *"does this row
> pay"* is, and pricing a part-earned discovery against a food rate would be the same invention
> again.
>
> The three accounts are asked in their own published terms, because that is what `SourceYield`
> carries: `realized` for food (the forward projection, so a big-game hunt on a wait turn still reads
> as paying), `fodder` and `materials` for the other two, both of which are this turn's credited
> amounts and have no projected twin to read. The material account is asked **row by row and never
> summed** — the standing rule for it — though any one paying row answers the question.
>
> `shedding_order.rs` pins all three claims on the **published wire rows**: the dead row gives from
> either list position, a food row still outranks a materials-only row, and two dead rows fall back
> to the earliest-row tie-break rather than becoming order-dependent.

**An edited row is not a zero-yield row.** `set_assignment` drops the edited row's telemetry with the
row, and the `assign_labor` command re-seeds it immediately from the source's pre-commit forecast
(`set_source_yield`) — so a crew the player has just staffed carries the number the compose sheet
quoted rather than a `0.0` that would make it the first thing thinned. That seeding is load-bearing
to this order, not merely a display nicety.

#### ⛔ It used to be the EDIT order, and that is what this replaced

`set_assignment` removes the row it is editing and re-pushes it at the **end** of `assignments`, and
`normalize` trimmed from the **end**. So the row a player had just touched was always first in the
shedding order. Reported from play: a Field's tenders were raised `2 → 3`, an elder died that turn,
and the worker came straight back off the row that had just been chosen — which reads as the game
ignoring the order.

The two halves were individually reasonable — an edited row is naturally re-appended, and shedding
from the tail means *"where each row falls in the shedding order is where the player put it in the
list"*, which is a statement the player can make. Their composition is what nobody chose: the list
position a player controls is silently overwritten by the act of editing the row. **List position
must never be the shedding order again**; nothing in the eleven steps is positional.

`core_sim/tests/shedding_order.rs` pins the reported case on the **encoded envelope** — the raise
stands and the poorer ground per head gives the hand — because the claim is about the crew count the
player watched move.

### The player's rank on a worked row

`SourcePriority` (`components.rs`) is a field on `LaborAssignment` — `High`, `Normal` (the default)
or `Low` — set by `work_priority <faction> <band> <source…> high|normal|low` and published as
`LaborAssignmentState::priority` / `snapshot.fbs`'s `SourcePriority`. It is the **outermost** level of
the shedding comparison above and of the pen-feed split (`graze.md` → "The pen feed is settled across
every pen at once").

**It is a stated value on the row and never a list position.** `set_assignment` removes the row it
edits and re-pushes it at the **end** of `assignments`, so a rank derived from a vector index would be
reset by the `−`/`+` that triggered the edit — the composition that made list position the shedding
order in the first place (see the callout above). `set_assignment` therefore carries the rank across
the re-push on **every** path, staffed or unstaffed, because `assign_labor` states a crew and a tier
and says nothing about priority.

**A rank orders candidates. It never creates or removes one.**

- Steps 1–4, 7, 8 and 11 select by **role**, and none of them consults it: a spare scout still gives
  before a spare builder.
- It is a level **inside** a step, not a way out of one. An unimproved row marked `High` is still
  emptied at step 6 while an improved `Normal` row waits at step 9 — pinned, because it is the design
  and reads like a bug.
- `LastHand` still takes the band's last worker off its last row whatever it is marked.
- With every row at the default the level is **constant**, so the comparison collapses to exactly the
  order it had before. That is what makes an explicit rank a rule that fires only on a deliberate pick.
- **And nothing above it may exclude a row before it is read.** Step 5's knowledge skip used to be an
  eligibility filter wrapping the comparator, which struck a learning row out before the mark was
  seen and made a `Low` mark on such a row do nothing at all — see the callout on the levels above.
  A term that decides *which rows are candidates* sits above the rank by construction, so anything
  that is really part of the shipped **ordering** has to be a level beneath it instead.

**It is intent, so it is inside `LaborAllocation`'s hand-written `PartialEq`** (it rides
`assignments`), unlike `last_yields` and `last_raid_forfeit`, which are derived telemetry and are
deliberately outside it. Two allocations differing only in a mark are two different orders, and a
rollback record or a command no-op guard that could not tell them apart would report *nothing changed*
on the one input the scarcity handlers read.

**The wire numbering is not the shedding order.** `snapshot.fbs` puts `Normal = 0` so the default
costs no bytes, while the Rust variants are declared `Low < Normal < High` so `min_by` lands on the
row the player marked to give up. The codec maps the two rather than casting, so neither can drift
into the other.

`work_priority` names a **band**, like `build_order` and unlike the source-addressed `unqueue` /
`build_kit`: the orderings it feeds partition one band's own rows and serve one band's own stores.
`xtask`'s command guard classifies it as band-addressed for that reason. An unknown level is refused
**by name** (`upkeep_mode`'s rule) — a mistyped rank must not silently land on the default, which is
the one value that would look like it worked.

### `normalize` and the commands now measure the same pool

**The invariant the walk drives is `Σ assignments.workers + bench.workers ≤ available_workers(cohort.working)`** —
the band's whole working-age head-count on the right, with nothing netted out of it, and both of the
places its people go on the left. Equivalently: the walk runs until `BandWorkforce::idle()` is zero.

It used to be `assigned_total() > available` with the bench nowhere in the expression, which was two
defects in one line. The bench was **invisible to the shed** (see the section below), and because
`available` was already the raw pool while every command clamps against `assignable()` (`pool −
benched`), an allocation that spent the bench's hands twice was *tolerated* rather than corrected.
The two passes disagreed about what a band's spendable pool is, against `BandWorkforce`'s own claim
to be the single authority over that number. One term closes both.

### The crafting bench is a candidate in the walk

`BandBench` spends the same pool `assign_labor` does, but it is **not** a `LaborTarget` and not a row
in `assignments` — *"make IS the assignment"*, and a bench is not an in-range source, so giving it a
target would put a fictitious row on every yield readout in the game. `normalize` walks
`assignments`, so the bench was invisible to it: **a starving band stripped every worked row, every
standing role and its last builder while the crafters kept hammering.**

`normalize` therefore takes `Option<&mut BandBench>`, and `ShedCrew` reports a `ShedSubject`
(`Row(LaborTarget)` | `Bench`) rather than a bare target. An absent bench contributes nothing and is
never a candidate, so a band without one walks exactly the order it always did.

**It is ranked in step 5, and it is NOT given a step of its own above or below the rows.** A step
boundary sits **above** the player's rank by construction, so a bench in its own step would be
protected from — or sacrificed to — a marked row purely by step order. That is the defect the lesson
level was repaired for, one arc earlier, in a different costume. Its four levels
(`ShedRank::of_bench`):

| level | the bench's reading | why |
|---|---|---|
| priority | its own mark (`BandBench::priority`) | set by `bench_priority`; the outermost level, as for a row |
| lesson | `NO_LESSON_AT_STAKE` | **a decision, argued below** |
| `pays_any_account` | `false` | a craft pays into no food, fodder or material account. It *consumes* materials and produces items, and items are not one of the three accounts the shed can read |
| `yield_per_worker` | `0` | there is no per-worker take to read |

So an **unmarked bench is thinned before any paying row**. That is the right default in a famine and
is exactly what the mark exists to override. A tie on all four levels goes to the **row**, a stated
order rather than one depending on which candidate was examined first — the same reason ties between
rows go to the earliest row.

> #### ⛔ THE BENCH IS NOT A LEARNER, AND THAT IS ARGUED RATHER THAN DEFAULTED
>
> A craft *does* charge a lesson per finished item (`credit_craft_lesson`), so thinning the bench
> genuinely costs knowledge — which looks like the source rows' `accruing_knowledge` case. It is not.
>
> The lesson level exists because **a source's lesson is invisible to the yield figure the choice is
> otherwise made on**. The bench has no yield figure at all: it already ranks bottom on both account
> levels, so its "invisible value" is fully expressed. Marking it a learner would lift it **above
> every non-learning row, including the food rows** — a famine band would strip its own larder to
> protect a craft that happened to be teaching something. The term would not add the missing
> information; it would invert the one thing the bench's other two levels get right.

**Step 5b takes the bench's LAST hand and the job stalls.** Numbered `5b` rather than renumbering six
steps and forty-odd references to them — the repo's own `4.7a` / `9b` convention for a late
insertion, and `ShedStep`'s **order** is the authority anyway. It sits above step 6 because a stalled
craft **ends nothing**, where emptying a source drops the row and takes its queued build with it.

**A `High` bench therefore stalls before a `Low` source is emptied, and that is intended.** It is the
same rule already pinned for rows, where an unimproved `High` row is emptied at step 6 before an
improved `Normal` one is a candidate at step 9. **The steps encode consequence; the mark orders
candidates within a step.**

> #### ⛔ THE SHED MUST NEVER CALL `clear_job`
>
> `BandBench::clear_job` is `*self = Self::default()`, which **forfeits the drawn pile** — the
> materials are dropped, not returned to the store. The shed uses `shed_one_worker`, which takes one
> hand and leaves the recipe, the progress, the finished count, the last grade and the drawn pile
> standing, so re-staffing **resumes** rather than restarts. That is also the crafting system's own
> shipped answer to a pass it cannot advance: *"the player chose this job, and silently emptying
> their bench is a worse answer than a job that makes no progress."*

**`status=stalled` is a third token on the shed feed line, and neither existing one would have been
true.** `trimmed` means *the crew is smaller than you set and the source is still worked*, which a
bench at zero is not; `lapsed` means *the row is GONE and its investment with it*, and is ranked ALERT
for that reason — the bench keeps everything, so `lapsed` would be false *and* would shout. A bench
that still has hands on it **is** a trim in the token's own terms and reuses it. `stalled` ranks with
`trimmed` (NOTABLE), not with `lapsed`: it is recoverable by one command and costs nothing that
cannot be got back.

**A zero-crew bench draws nothing** (`systems::crafting`, gated on `AN_IDLE_BENCH`). `advance_crafting`
runs its draw *before* the workers term is used anywhere, so an idle bench would keep withdrawing a
pass's inputs every turn — a famine quietly draining the material store into a bench nobody is at.
The gate is on the **draw**, not the pile: a bench that had already cut its materials keeps them and
simply banks no progress, which falls out of `rate_per_turn(0, …)` on its own. This state did not
exist before the shed could take a crew to zero without ending the job.

`core_sim/tests/bench_shed.rs` drives the whole of it through a real turn; the unit half is in
`components.rs`'s own tests, and the draw gate is pinned in `core_sim/tests/crafting.rs`.

### The pool `normalize` reads is not the pool the player composed against

`simulate_population` and `advance_labor_allocation` are chained in that order inside
`TurnStage::Population` (`lib.rs`). A command therefore clamps against the pool the **published
frame** showed, and `normalize` re-clamps one system later against the pool demographics has already
rewritten for this turn. The formula is the same on both sides — it is the instant that differs — so
a band sitting at full commitment sheds on any turn that costs it a working-age person.

---
