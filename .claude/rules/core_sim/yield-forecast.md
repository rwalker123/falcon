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
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` (**no longer the wild biomass→food rate** — since #433 every patch converts at the share-weighted average of its own basket, and this survives as the **empty-basket fallback** plus the rung-3 quality normalization baseline), and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield` — **plus the §0-iii policy axis** `surplus_multiplier` / `market.{take_fraction,trade_goods_multiplier}` / `eradicate.take_fraction`, mirroring fauna's follow/market/hunt levers (**`market.trade_goods_per_biomass` is RETIRED at #433** — the basket's own trade vector is the rate, and `trade_goods_multiplier` became a `Deplete`-*policy* markup on it, applied at rungs 1 and 2 alike) so forage has Sustain/Surplus/Deplete/Eradicate parity with hunting — **plus the Phase 1a `cultivation` block** — the plant ladder's **two rung payoffs (slice 7)**: **`tended_regrowth_gain` (1.0, rung 2 — NEUTRAL since Flora Roster S2, `docs/plan_flora_roster.md` §4.3: a tended stand regrows exactly as fast as wild. It began as the plant twin of `husbandry.pastoral_gain`, but once S1 made competitor-removal explicit a growth boost DOUBLE-COUNTS it, so tending pays through composition + conversion and the rung-2 "wild < tended" guarantee moved to the roster's own bar, `core_sim/tests/flora_roster.rs`; kept as a playtest dial in case a small boost is wanted back)** and **`field_provisions_per_biomass` (0.02, rung 3 — a managed rate on the standing crop, no drawdown, policy axis collapsed, because at rung 3 the source is YOURS)**; both PLAYTEST DIALS. `validate()` still enforces `tended < field`, now `field_provisions_per_biomass > tended_regrowth_gain × regrowth_rate/4 × provisions_per_biomass × tended_conversion_gain`, evaluated at tending's saturated best case so the crop's own rate cancels and the check stays scale-free; the `tended_regrowth_gain` check forbids only the INCOHERENT `< 1.0` (tending grows a stand slower than wild), not `<= 1.0`. **Plus the #433 pair `tended_weeding_gain` (1.5) / `tended_conversion_gain` (2.0)**, both validated finite and `>= 1.0`: the first is how far rung 2 **weeds** the favored species' share (`min(1.0, share × gain)`, the increase taken from the least abundant remaining species first), the second the conversion multiplier on that species' **whole yield vector**. Neither touches `K` — **the land owns `K` and no rung below 4 raises OR lowers it**. The retired `tended_concentration_gain` / `field_concentration_gain` pair multiplied the tile's `K` by `min(1, share × gain)` and **discarded the remainder**, which is the bug #433 fixed. See "Committing a patch to one plant". The retired `tended_provisions_per_biomass` (0.01) made rung 2 a *managed* rate a full rung earlier than the animal side's, so a tended patch could not be over-farmed and every policy paid the identical number (**the plant rung-2 BUILD dials — the old `progress_per_turn`/`decay_per_turn`/`cultivating_yield_fraction` — moved to `intensification_ladder.json`'s `plant:tended` rung**, and in slice 4 **the earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` moved to that file's ladder-level `knowledge` block** too, so both food webs climb *and learn* on the same numbers) (Rung 1a: cultivation is the explicit **`Cultivate` improvement** — while preparing, the patch yields only the `plant:tended` rung's `yield_fraction_while_building ×` the assignment's own stance ceiling (the investment cost; it rode Sustain's unconditionally until issue #442) and accrues that rung's `progress_per_turn`; at 1.0 the completed tended patch is worked place-local, Sustain-gathered at its MSY on the (now neutral, = wild) tended ecology — so a *bare* patch pays exactly wild, and its yield advantage over wild comes from a **committed crop** (weeding + conversion, #433), not a regrowth boost — and goes feral if abandoned. Rung 1b: working a **wild** patch under a stewardship policy earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate verb — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**; the accrual is the ladder's, driven off the rung — see "The knowledge pattern"); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
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

**`ceilingMarket` → `ceilingDeplete`** (sim-side `SourceYieldForecast::ceiling_deplete`) with the
policy rename `Market` → `Deplete` — a name change on the *same* FlatBuffers slot, so the wire layout
is unchanged. The herd's per-policy rows re-key themselves off `FollowPolicy::as_str`, which now
returns `"deplete"`. The rung is named for its harvest **pressure** rather than a product, because
every policy sells the source's trade goods; see `docs/plan_hunt_yield_model.md` §2.

> ### THE FORECAST IS A PAIR, not a food scalar (issue #337)
>
> Every field of `SourceYieldForecast` is a **`YieldPair { provisions, trade_goods }`** —
> `per_worker_yield`, all five `ceiling_*`, `managed_yield`, `pastoral_yield`, `body_mass_yield`. So is
> `SourceYield`'s telemetry: `trade` (the twin of `actual`) and `realized_trade` (the twin of
> `realized`) ride beside the food ones.
>
> **Why vectorised rather than sibling `*_trade` scalars.** A wolf's food ceilings are all `0`
> (`hunt_yield.provisions_per_biomass == 0`), so a food-denominated forecast cannot express its yield
> **at all** — the client would read "0/turn" on every rung and the forecast would be *false*, not
> merely incomplete. Sibling scalars double the surface and let the two halves drift under a retune;
> one pair per rung cannot, because `ceiling_for` hands both components to every reader at once.
>
> **`forecast == actual` now holds PER COMPONENT**, and that is the invariant this whole arc rests on:
> if the forecast can promise a number the sim will not pay in *either* currency, the UI lies. Pinned
> on the **exported snapshot** (not the in-process struct) by
> `hunt_yield_vector::the_forecast_equals_the_paid_take_in_both_products_on_the_wire`, across a
> defaulting species and an inedible one × all four extractive rungs.
>
> **Quantisation picks an AXIS, and it is never assumed to be food.** `forecast_production_and_take`
> runs `quantise_animal_take` on `SourceYieldForecast::ratio_axis()` — the first component with a
> *positive* per-biomass rate (`Provisions` preferred, so every edible species divides exactly the
> numbers it divided before this arc; `TradeGoods` for a wolf) — then `YieldPair::rescaled_to` carries
> the one animal count back into the other currency. An animal count is a **ratio**, and a ratio is
> unit-free: any positive component gives the same answer, a zero one gives `0/0`. Correspondingly
> **"does this source quantise?" is now `!body_mass_yield.is_zero()`**, not
> `body_mass_yield.provisions > 0` — the old test would call a pack of wolves *continuous* and hand
> back a smooth fraction of a wolf. Every pre-#337 source reads identically (plants are zero in both
> components), so it is a widening, not a change.
>
> **No trade `arrivals` schedule, deliberately.** `arrivals` answers *"when does food land so my people
> eat"* — a question with a consumption clock. Trade goods go to a faction stockpile nothing consumes
> per turn, so a trade timetable would answer a question nobody asks. **And `food_income` stays
> `Σ actual` and must never include `trade`**: that sum is one side of the pinned larder identity
> `larder_delta == food_income − food_consumption − pen_feed_upkeep`, and trade never touches the
> larder.
>
> **THE PLANT SIDE'S TRADE COMPONENT IS `0.0` — a known gap, not a claim.** `forage_forecast` fills
> `forage::PLANT_TRADE_FORECAST_NOT_YET_PROJECTED` throughout, and `realized_trade` is `0` on every
> forage source: the `Deplete` gather really does sell (`labor_config`'s `forage.market.*`), the sim
> simply has not *projected* it — #337 vectorised the animal web, and the plant web's trade forecast is
> its own arc. The trade a gather **actually earns** is reported (`SourceYield::trade` /
> `LaborAssignmentState::trade_yield`). It is safe to ship because of the rendering rule below.
>
> **The client renders a trade line ONLY when `trade_goods > 0`** — exactly the rule flora's cash-crop
> line already uses — so a plant shows *no trade line* rather than a false "0 trade goods/turn".
>
> **New wire fields** (each appended at the END of its table — slots are positional):
> `HuntPolicyCeiling.tradeGoodsPerTurn` · `HuntTripEstimate.deliveredTrade` (beside the already-shipped
> `deliversTrade`) · `HerdTelemetryState.perWorkerTrade` / `tradePerAnimal` ·
> `LaborAssignment.tradeYield` / `realizedTradeYield`. The investment rungs' twins
> `HerdTelemetryState.pastoralTrade` / `corralTrade` followed in issue #397 (below).
>
> **`PopulationCohortState.huntPerWorkerProvisions` is SPECIES-BLIND — do not clamp a per-herd preview
> with it.** It is a per-*cohort* echo of the global `hunt.provisions_per_biomass`, and a cohort has no
> herd, so there is no species to resolve a vector from; left unqualified it quotes a wolf's hunters a
> positive food rate against all-zero food ceilings — a contradiction on the wire. The species-aware
> rates are the herd's own `perWorkerYield` / `perWorkerTrade`, straight off its `hunt_forecast`, so
> `min(workers × perWorkerYield, huntPolicyCeilings[p].provisionsPerTurn)` is honest per component.
> The cohort field survives as the expedition **outfit** lever (rough carry arithmetic before a target
> is chosen); for a chosen target the sim exports the answer in `huntTripEstimates`.

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
>   stays on the row as the long-run MSY reference; the ⚠ is `FollowPolicy::overdraws`.
> - **A ceiling row cannot be compared with a rung PAYOFF** (`tendedYield`, `fieldYield`,
>   `pastoralYield`, `corralYield`). Those are long-run rates and carry `r`; a stance ceiling is
>   `r`-free (`B − floor·K` is `K/2` on every rung at `B = K`). "Preparing +X → then +Y" is therefore
>   a stock beside a rate today, which the client's ladder redesign (`docs/plan_harvest_floor.md` §7,
>   slice 4) resolves.

> ### THE CEILING LISTS ARE FOUR STANCE ROWS; A BUILD DIP IS A FRACTION (issue #442)
>
> `foragePolicyCeilings` and `huntPolicyCeilings` carry **one row per `FollowPolicy`, i.e. four**. The
> `cultivate`/`sow`/`tame`/`corral` rows are gone, and so are the sim-side `ceiling_prepare` /
> `ceiling_tame` / `ceiling_sow` fields they came from: each of those stated the rung's fraction of the
> **Sustain** ceiling and nothing else, which was only expressible while a build verb *was* the policy.
>
> The dip now ships as the factor it is — **`ForagePatchState.cultivateBuildFraction` /
> `sowBuildFraction`** and **`HerdTelemetryState.tameBuildFraction` / `corralBuildFraction`**, each the
> rung's `yield_fraction_while_building` — and the client composes
>
> ```text
> preparing(stance, rung) = <list>[stance].provisionsPerTurn × <rung>BuildFraction
> ```
>
> Sim-side that is **`SourceYieldForecast::ceiling_at(floor, improvement)`** — one computation, which
> answers *any* floor because the player drags a continuous one. It is backed by the forecast's
> **terms** (`biomass`, `carrying_capacity`, `per_biomass_yield`, `build_dips`) rather than by stored
> rows: four `ceiling_*` fields could only answer four questions, and every row added was a second
> place the ceiling was computed. **The four wire rows are now `ceiling_for(policy)`, which IS
> `ceiling_at(policy.escapement_floor(), None)`** — a projection of the one computation, so a row
> cannot disagree with the take.
> Two rungs keep two numbers for `ceiling_tame`'s original reason: the dials are independently tunable
> and today's equality (0.25/0.25, 0.50/0.50) is a coincidence.
>
> **THE DIP IS APPLIED INSIDE THE STANDING-STOCK CLAMP — `min(rate × dip, stock)`, never
> `min(rate, stock) × dip`** — the order both take paths use (each dips inside its own
> `*_escapement_ceiling` and *then* clamps to the standing stock). The forecast keeps the two terms
> apart accordingly: the four `ceiling_*` rows are the **pre-clamp** stance ceilings and
> `SourceYieldForecast::stock_cap` is the bound, which `ceiling_for` applies (so the wire value is
> unchanged) and `ceiling_at` applies *second*, inside the same call.
>
> **The clamp is now INERT on both webs, and that is asserted rather than assumed.** An escapement
> ceiling is `B − floor·K`, so `room × dip ≤ room ≤ B` for any floor `≥ 0` and any dip `≤ 1` — the two
> orders agree everywhere, including on the drawn-down sources where the retired rate-based ceilings
> did not commute (measured then on a pastoral `crag_goat` at `B = 0.20·K` under Deplete + Corral:
> previewed `0.10·K`, paid `0.1375·K`). The `hunt_forecast_equals_actual_take…` sweep asserts
> `ceiling_at(floor, improvement) == ceiling_at(floor, None) × dip` on every row at both stock levels,
> which is the positive form
> of the same guarantee. `stock_cap` stays populated for wire stability and as belt-and-braces.
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
offers Tame (a forage patch, or a herd already penned/forage-tended). **Each investment payoff is a
PAIR on the wire** (issue #397): `pastoralTrade` / `corralTrade` carry the `trade_goods` half of the
very same `SourceYieldForecast::pastoral_yield` / `managed_yield` `YieldPair`s their food siblings read
`provisions` from, so an inedible-but-valuable species' Tame/Corral rungs quote the same vector its four
extractive rungs already do (before them a Wild Boar's picker read `→ 1.48 food` on the investment rungs
beside `0.74 food · 0.18 trade` on the extractive ones). `corralTrade` is **gross** like `corralYield` —
the pen's feed (`penUpkeep`) is a provisions debit and never touches the trade component — and each
component renders only when non-zero, the rule `perWorkerTrade` follows. **Both `pastoralYield` and the
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
- Each `ceiling*` = that policy's food/turn cap, **already clamped to the source's remaining biomass**.
- Captured at `output_multiplier = 1.0` (the productivity multiplier is per-band): the client scales
  every field by the acting band's `PopulationCohortState.outputMultiplier` — a linear factor, so
  `max_useful_workers` is invariant to it.
- Client composition: `expected(workers, policy) = min(workers × perWorkerYield, ceiling[policy])`,
  `max_useful_workers(policy) = ceil(ceiling[policy] / perWorkerYield)`.
- **A crew BUILDING something floors that cap**, on both webs, because the take a build is paid is the
  **dip** and inverting a dip asks for fewer hands than gathering the same source does:
  `max_useful_workers(stance, rung) = max(ceil(ceiling / perWorker), <crew floor>)`. The herd's floor
  is `herdersNeeded` / `herdersNeededIfManaged` (derived from the herd's size) and has shipped for
  slices; the patch's is the appended **`cultivateCrewNeeded` / `sowCrewNeeded`**, the rung's own
  `crew_needed`. **The same number is the build's denominator** — plant accrual is
  `progress_per_turn × min(workers / crew_needed, 1)` — so staffing the cap the panel offers is what
  buys the rung's stated build length, and under-staffing costs turns rather than nothing. Sim-side
  the floor is `intensification::source_crew_needed`, one `max()` for both webs **and for both halves
  of the row** — see "The crew floor is ONE definition" below.
- A **rung-3 managed source** (a sown **Field** / a **corralled herd**) is *yours*, so **the policy axis
  collapses**: every ceiling is its managed yield (`SourceYieldForecast::managed`). **The worker cap does
  not collapse** — `perWorkerYield` is the crew's real throughput, so `max_useful_workers =
  ceil(production / perWorkerYield)` is an honest count that grows with the source (slice 7; it used to
  be a hardcoded `1`, which claimed one worker could carry home whatever the land offered). A **tended
  patch is NOT this shape** — it is rung 2, a wild stand on a boosted curve, and forecasts policy-live
  like a wild patch.

**Invariant: forecast == actual — no duplicated yield math.** The forecast and the take path read the
*same* pure helpers, so the UI can never promise a number the sim won't pay:
- forage (`forage.rs`): `forage_escapement_ceiling` (the stance's floor × the build dip, biomass) · `forage_per_worker_biomass`
  (`per_worker_biomass_capacity × seasonal`) · `forage_provisions` (biomass→provisions ×
  `output_multiplier`) · `tended_provisions` (the tended-patch managed harvest) — all called by both
  `forage_take` / the tended-patch arm of `advance_labor_allocation` **and** `forage_forecast`.
- fauna (`fauna.rs`): `hunt_escapement_ceiling` (the stance's floor × the build dip) · the species'
  `HuntYield::apply` (which retired the global `hunt_provisions`) ·
  **`managed_yield_biomass`** (the husbandry harvest, via `pen_yield_biomass`) · **`herd_ecology` /
  `herd_capacity`** (which ecology/capacity a herd lives under — *no call site may re-derive either*) —
  called by both `systems::hunt_take` / the corral arm of `advance_labor_allocation` **and**
  `hunt_forecast`. The shared `SourceYieldForecast` struct (with `::tended`) is the common return shape.
  A corralled herd's `managed_yield` is **gross**; its `penUpkeep` is exported separately.
- Guarded across **both products, on the exported snapshot**, by
  `core_sim/tests/hunt_yield_vector.rs` (`the_forecast_equals_the_paid_take_in_both_products_on_the_wire`,
  `a_wolves_exported_ceilings_read_no_food_and_real_trade_on_every_rung`,
  `the_eradicate_ceiling_carries_the_windfall_for_an_edible_species`), and on the food component by
  `systems::labor_yield_tests::{forage,hunt}_forecast_equals_actual_take_for_every_policy_and_staffing`
  (every policy × labor-bound/ceiling-bound staffing, comparing against the payout of a real
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
  workers, floor, improvement) = min(workers × per_worker_yield, forecast.ceiling_at(floor,
  improvement))` — the ceiling at the **assignment's own floor**, dipped by whatever the crew is
  building (`None` is the identity). Once the improvement *completes* the source is `::managed`,
  whose ceiling is its `managed_production` at every floor, so this one lookup covers both sides of
  every investment. The client preview, the seed, and the forecast==actual tests all call it.
- The kind-specific seeds `forage::forage_source_yield_preview` / `fauna::hunt_source_yield_preview`
  compose the full row through the shared `forecast_source_yield`: `actual` = the expected take,
  `sustainable` = the same MSY value the resolution path records (a *managed* source — **rung 3 only**
  — reads `sustainable == actual`, no ⚠), `workers_needed` = the same overstaffing signal the resolution
  path writes (the continuous inversion for a forage patch; the **peak-drop carry crew**
  `hunt_haul_workers` off `SourceYieldForecast::ceiling_at` for a whole-animal source, so the seed
  matches the client's max-useful cap), and `wasted` = the understaffing mirror. No new formula, no new
  config lever.
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

---

