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
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` (**no longer the wild biomass→food rate** — since #433 every patch converts at the share-weighted average of its own basket, and this survives as the **empty-basket fallback** plus the rung-3 quality normalization baseline), and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield` — **plus the §0-iii policy axis** `surplus_multiplier` / `market.{take_fraction,trade_goods_multiplier}` / `eradicate.take_fraction`, mirroring fauna's follow/market/hunt levers (**`market.trade_goods_per_biomass` is RETIRED at #433** — the basket's own trade vector is the rate, and `trade_goods_multiplier` became a `Deplete`-*policy* markup on it, applied at rungs 1 and 2 alike) so forage has Sustain/Surplus/Deplete/Eradicate parity with hunting — **plus the Phase 1a `cultivation` block** — the plant ladder's **two rung payoffs (slice 7)**: **`tended_regrowth_gain` (1.0, rung 2 — NEUTRAL since Flora Roster S2, `docs/plan_flora_roster.md` §4.3: a tended stand regrows exactly as fast as wild. It began as the plant twin of `husbandry.pastoral_gain`, but once S1 made competitor-removal explicit a growth boost DOUBLE-COUNTS it, so tending pays through composition + conversion and the rung-2 "wild < tended" guarantee moved to the roster's own bar, `core_sim/tests/flora_roster.rs`; kept as a playtest dial in case a small boost is wanted back)** and **`field_provisions_per_biomass` (0.02, rung 3 — a managed rate on the standing crop, no drawdown, policy axis collapsed, because at rung 3 the source is YOURS)**; both PLAYTEST DIALS. `validate()` still enforces `tended < field`, now `field_provisions_per_biomass > tended_regrowth_gain × regrowth_rate/4 × provisions_per_biomass × tended_conversion_gain`, evaluated at tending's saturated best case so the crop's own rate cancels and the check stays scale-free; the `tended_regrowth_gain` check forbids only the INCOHERENT `< 1.0` (tending grows a stand slower than wild), not `<= 1.0`. **Plus the #433 pair `tended_weeding_gain` (1.5) / `tended_conversion_gain` (2.0)**, both validated finite and `>= 1.0`: the first is how far rung 2 **weeds** the favored species' share (`min(1.0, share × gain)`, the increase taken from the least abundant remaining species first), the second the conversion multiplier on that species' **whole yield vector**. Neither touches `K` — **the land owns `K` and no rung below 4 raises OR lowers it**. The retired `tended_concentration_gain` / `field_concentration_gain` pair multiplied the tile's `K` by `min(1, share × gain)` and **discarded the remainder**, which is the bug #433 fixed. See "Committing a patch to one plant". The retired `tended_provisions_per_biomass` (0.01) made rung 2 a *managed* rate a full rung earlier than the animal side's, so a tended patch could not be over-farmed and every policy paid the identical number (**the plant rung-2 BUILD dials — the old `progress_per_turn`/`decay_per_turn`/`cultivating_yield_fraction` — moved to `intensification_ladder.json`'s `plant:tended` rung**, and in slice 4 **the earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` moved to that file's ladder-level `knowledge` block** too, so both food webs climb *and learn* on the same numbers) (Rung 1a: cultivation is the explicit **`Cultivate` policy** — while preparing, the patch yields only the `plant:tended` rung's `yield_fraction_while_building × its Sustain/MSY ceiling` (the investment cost) and accrues that rung's `progress_per_turn`; at 1.0 the completed tended patch is worked place-local, Sustain-gathered at its MSY on the (now neutral, = wild) tended ecology — so a *bare* patch pays exactly wild, and its yield advantage over wild comes from a **committed crop** (weeding + conversion, #433), not a regrowth boost — and goes feral if abandoned. Rung 1b: working a **wild** patch under a stewardship policy earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate policy — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**; the accrual is the ladder's, driven off the rung — see "The knowledge pattern"); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
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
> eat"* — a question with a consumption clock. Trade goods sit in the band's own store with nothing
> consuming them per turn, so a trade timetable would answer a question nobody asks. **And `food_income` stays
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

> **The hunt ceilings are the STEADY sustainable per-turn rate — the credit bank drives the lumpy
> TAKE, not the displayed readout.** `hunt_forecast`'s `ceiling` closure passes `credit = 0.0` to
> `hunt_credit_ceiling`, so each extractive/investment ceiling is `min(hunt_policy_rate, biomass)` —
> the sustainable rate the confirmed-allocation row already headlines (`sustainable_yield`) — **not**
> the credit-inclusive `min(credit + rate, biomass)` this-turn burst the take path cashes. For a slow
> breeder whose MSY < `body_mass` (e.g. Wild Aurochs, `r ≈ 0.09`) the bank accumulates ~a whole animal,
> and quoting `credit + rate` inflated every extractive ceiling by that banked amount — reading the Tame
> dip *above* its own payoff and Sustain *above* Tame, inverting the ladder. Steady, the compose
> forecast agrees with the resolved headline (no jump between them) and the aurochs ladder reads in
> order: `Sustain 0.72 < Surplus 1.08 < Deplete 1.80`, Tame dip `+0.36 → payoff +1.44`, Corral payoff
> `2.88`. **Eradicate is unchanged** (its rate is the whole stock `B`; it bypasses the bank). The take
> path (`hunt_take` / `hunt_credit_ceiling`) keeps the bank untouched — only the readout is steady.
> Pinned by `fauna::tests::the_forecast_ceilings_are_the_steady_rate_not_the_banked_burst` (a full-bank
> herd) and the empty-bank `forecast == actual` tests. **Forage has no credit bank** (foraging is
> continuous), so `forage_forecast`'s ceilings were already steady `sustainable_yield` — unchanged.

`ForagePatchState.ceilingCultivate` + `tendedYield` and, on a herd, `huntPolicyCeilings`' **corral**
row + `corralYield`. The investment policy's ceiling is the **preparing** yield
(`fraction × the Sustain/MSY ceiling` — the dip); `tendedYield`/`corralYield` is what the source will pay
**once the improvement completes**, so the client can show **"preparing X → then Y"** *before* the
player commits to the cost. (Sim-side both live on the shared `SourceYieldForecast` as
`ceiling_prepare` / `managed_yield` — the two investment policies are kind-exclusive, so one field
serves both.) **The `Tame` rung has its own payoff twin: `HerdTelemetryState.pastoralYield`** (sim
`SourceYieldForecast::pastoral_yield`) — what a Sustain hunt pays **once the herd is tamed**, so the
client can render Tame's `→ +Y` instead of quoting only its during-building dip (`ceiling_tame`, which
reads *below* wild Sustain and hides that taming out-yields wild hunting). `0` on a source that never
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
escapement (`max(0, B − K/2)`) is `r`-independent, the sustained form is what makes the ladder visible
at a single turn: **`ceiling_sustain < pastoral_yield < managed_yield`** (wild `r·K/4` < pastoral
`r×2.0` < pen `r×4.0`, MSY-capped; measured ≈ 0.5 < 1.0 < 2.0 on a full Wild Boar). The old escapement
projection read `pastoral_yield == managed_yield` (≈ 10 = 10) and could not show the ladder the field
exists for. **The penned-herd `managed_yield` stays the escapement take** — a live corralled herd hits
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
- A **rung-3 managed source** (a sown **Field** / a **corralled herd**) is *yours*, so **the policy axis
  collapses**: every ceiling is its managed yield (`SourceYieldForecast::managed`). **The worker cap does
  not collapse** — `perWorkerYield` is the crew's real throughput, so `max_useful_workers =
  ceil(production / perWorkerYield)` is an honest count that grows with the source (slice 7; it used to
  be a hardcoded `1`, which claimed one worker could carry home whatever the land offered). A **tended
  patch is NOT this shape** — it is rung 2, a wild stand on a boosted curve, and forecasts policy-live
  like a wild patch.

**Invariant: forecast == actual — no duplicated yield math.** The forecast and the take path read the
*same* pure helpers, so the UI can never promise a number the sim won't pay:
- forage (`forage.rs`): `forage_policy_ceiling` (the 4 extractive rungs **+ Cultivate**, biomass) · `forage_per_worker_biomass`
  (`per_worker_biomass_capacity × seasonal`) · `forage_provisions` (biomass→provisions ×
  `output_multiplier`) · `tended_provisions` (the tended-patch managed harvest) — all called by both
  `forage_take` / the tended-patch arm of `advance_labor_allocation` **and** `forage_forecast`.
- fauna (`fauna.rs`): `hunt_policy_ceiling` (the 4 extractive rungs **+ Corral**) · the species'
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
shorthands, via `set_policy_on_working_bands`) **seeds the touched source's `SourceYield` from its
pre-commit forecast** right after mutating the `LaborAllocation` (`server.rs::seed_source_yield` →
`LaborAllocation::set_source_yield`). Because forecast == actual (above), the seeded number is exactly
what the turn then pays under unchanged conditions — **no jump** — and it is the same number the
client's compose-time "Expected yield" row promises. Shape:
- **The expected take** is the one shared helper `fauna::forecast_expected_take(&SourceYieldForecast,
  workers, policy) = min(workers × per_worker_yield, forecast.ceiling_for(policy))`
  (`SourceYieldForecast::ceiling_for` is the `ceiling[policy]` lookup; the two investment policies
  share `ceiling_prepare`, the reduced `yield_fraction_while_building` bite of the rung being built —
  once the improvement *completes* the source is `::tended`, whose every ceiling already **is**
  `managed_yield`). The client preview, the seed, and the forecast==actual tests all call it.
- The kind-specific seeds `forage::forage_source_yield_preview` / `fauna::hunt_source_yield_preview`
  compose the full row through the shared `forecast_source_yield`: `actual` = the expected take,
  `sustainable` = the same MSY value the resolution path records (a *managed* source — **rung 3 only**
  — reads `sustainable == actual`, no ⚠), `workers_needed` = the same overstaffing signal the resolution
  path writes (the continuous inversion for a forage patch; the **steady peak-drop carry crew**
  `hunt_haul_workers` off `SourceYieldForecast::ceiling_for` for a whole-animal source, so the seed
  matches the client's max-useful cap), and `wasted` = the understaffing mirror. No new formula, no new
  config lever.
- **Only the source the command touched** is seeded (other sources keep their real actuals), and only
  where the turn would actually pay: out of `band_work_range` / past the hunt leash, an unseeded patch
  or a vanished herd keeps its zero row, and a **genuinely barren source still seeds `0.0`** — `+0.00`
  stays reachable, and correct, there. Consequence (intended): a fresh assignment now *previews* its
  contribution to the Food-line net rate + the Gathered/Hunted breakdown, and can pre-trip the
  overdraw ⚠ if the chosen policy would overdraw — ⚠ is a leading flow signal by design.
- `LaborAllocation` now keeps `last_yields` **index-aligned with `assignments`** across every mutation
  (`set_assignment`/`normalize`/`clear` — the snapshot zips the two by index, so a row left behind by a
  removed assignment used to be attributed to the *next* source). New rows default to
  `SourceYield::ZERO`.
- Guarded by `server::tests::{assigning_forage,assigning_hunt}_workers_seeds_the_expected_yield_before_the_turn`,
  `resolved_{forage,hunt}_yield_equals_the_seeded_yield` (the no-jump property),
  `changing_the_policy_reseeds_the_expected_yield`, `a_barren_source_seeds_zero`,
  `unassigning_a_source_drops_its_yield_row`.

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

### Trade goods are a BAND-LOCAL store, and the faction figure is derived

`TRADE_GOODS` is a **third key on the same `PopulationCohort::stores` `LocalStore`** as `FOOD` and
`FODDER` — a band/city holds what it produced until a trade network reaches it. Every ongoing credit
site works the way the `FODDER` lines beside them do:

```rust
let trade_goods = scalar_from_f32(production.min(collection));
if trade_goods > scalar_zero() {
    cohort.stores.add(TRADE_GOODS, trade_goods);
}
```

The five sites are the Field harvest, the drawn-down forage take, the pen harvest and the wild hunt
(all `systems/labor.rs`), plus the expedition's delivery (`systems::expeditions::settle_carried_trade`,
which credits the **home band**). There is **no faction-level total anywhere** — nothing in the sim
reads accumulated trade goods, and a faction figure is a sum over its bands, computed where it is
wanted rather than stored.

**Three things fall out for free, which is why this is a key and not a new account:**
- the snapshot already ships every key generically (`snapshot/population.rs` iterates `cohort.stores`),
  so there is no schema change and no decoder change;
- `balance_supply_networks` (`supply.rs`) collects `commodities` from whatever keys the member nodes
  hold, so same-faction bands inside `SupplyNetworkConfig.reach_tiles` share their trade goods and
  bands beyond it do not — **no trade-specific path belongs in that system**;
- a `LocalStore` is fixed-point, so per-turn flows accumulate instead of rounding.

> **The rounding those credits used to do was a live bug.** `FactionInventory` is an `i64` stockpile,
> so each site banked `(production.min(collection)).round() as i64` — which discards **every** per-turn
> trade income below `0.5`. A forage patch paying `0.04` trade/turn contributed exactly nothing,
> forever, while the client honestly reported `+0.04 /turn` off `SourceYield::trade`. Small sources now
> genuinely accrue, which is an observable balance change, not just a refactor. Pinned by
> `forage_tended_vector::a_sub_unit_trade_income_accumulates_instead_of_vanishing` (a sub-unit
> per-turn credit whose running total clears a whole good) and
> `::trade_income_lands_in_the_producing_bands_store_not_the_faction_stockpile`.

**`FactionInventory` survives on the start-profile path alone.** `seed_starting_inventory`
(`systems/worldgen.rs`) writes a `StartProfileOverrides::inventory` grant into it and the **Startup-only**
`apply_trade_goods_bonus` drains the `TRADE_GOODS` grant into the opening trade-link openness bonus.
That conversion is the resource's only remaining reader, and it never sees ongoing income.

---

