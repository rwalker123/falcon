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
| `src/data/labor_config.json` | Early-Game Labor allocation: `band_work_range` (true odd-r **hex-distance** radius of in-range sources — `grid_utils::hex_distance_wrapped`, wrap-aware), `worked_source_sight_range` (fog reveal range around each worked Forage tile / Hunt herd tile in `calculate_visibility`), `hunt_leash_tiles` (extra leashed-follow reach for Hunt), `band_move_tiles_per_turn` (`move_band` speed), `forage` (**depletable-forage** ecology, §0-ii: **`capacity_by_biome`** — the **human food web's** per-biome capacity table, a **total** table (one row per `TerrainType`) mirroring `fauna_config.json`'s `graze.capacity_by_biome` (the *animal* web) row-for-row and meant to **disagree** with it (see "The two food webs"); it replaces the retired flat `carrying_capacity` of 120 — `per_worker_biomass_capacity` gather throughput, `provisions_per_biomass` biomass→food conversion, and an `ecology` block reusing fauna's `EcologyConfig` — `regrowth_rate` tuned higher than fauna's 0.05, plus `collapse_fraction`/`stressed_fraction` phase bands; supersedes the retired flat `per_worker_yield` — **plus the §0-iii policy axis** `surplus_multiplier` / `market.{take_fraction,trade_goods_multiplier,trade_goods_per_biomass}` / `eradicate.take_fraction`, mirroring fauna's follow/market/hunt levers so forage has Sustain/Surplus/Deplete/Eradicate parity with hunting — **plus the Phase 1a `cultivation` block** — the plant ladder's **two rung payoffs (slice 7)**: **`tended_regrowth_gain` (1.0, rung 2 — NEUTRAL since Flora Roster S2, `docs/plan_flora_roster.md` §4.3: a tended stand regrows exactly as fast as wild. It began as the plant twin of `husbandry.pastoral_gain`, but once S1 made concentration explicit a growth boost DOUBLE-COUNTS competitor-removal, so tending now pays through concentration + conversion and the rung-2 "wild < tended" guarantee moved to the roster's own bar, `core_sim/tests/flora_roster.rs`; kept as a playtest dial in case a small boost is wanted back)** and **`field_provisions_per_biomass` (0.02, rung 3 — a managed rate on the standing crop, no drawdown, policy axis collapsed, because at rung 3 the source is YOURS)**; both PLAYTEST DIALS. `validate()` still enforces `tended < field` (scale-free in `K`); the `tended_regrowth_gain` check now forbids only the INCOHERENT `< 1.0` (tending grows a stand slower than wild), not `<= 1.0`. **Plus the Flora Roster S1 pair `tended_concentration_gain` (1.5) / `field_concentration_gain` (2.5)** — how hard each rung concentrates a **committed** species into the tile's basket (`concentration = min(1.0, share × gain)`, applied to the tile's own `K`; validated finite and `>= 1.0`, capped at 1.0 because **the land owns `K`**). See "Committing a patch to one plant". The retired `tended_provisions_per_biomass` (0.01) made rung 2 a *managed* rate a full rung earlier than the animal side's, so a tended patch could not be over-farmed and every policy paid the identical number (**the plant rung-2 BUILD dials — the old `progress_per_turn`/`decay_per_turn`/`cultivating_yield_fraction` — moved to `intensification_ladder.json`'s `plant:tended` rung**, and in slice 4 **the earned-knowledge levers `knowledge_progress_per_turn`/`knowledge_completion_threshold` moved to that file's ladder-level `knowledge` block** too, so both food webs climb *and learn* on the same numbers) (Rung 1a: cultivation is the explicit **`Cultivate` policy** — while preparing, the patch yields only the `plant:tended` rung's `yield_fraction_while_building × its Sustain/MSY ceiling` (the investment cost) and accrues that rung's `progress_per_turn`; at 1.0 the completed tended patch is worked place-local, Sustain-gathered at its MSY on the (now neutral, = wild) tended ecology — so a *bare* patch pays exactly wild, and its yield advantage over wild comes from a **committed crop** (concentration + conversion, S1), not a regrowth boost — and goes feral if abandoned. Rung 1b: working a **wild** patch under a stewardship policy earns faction **Cultivation** knowledge in the `DiscoveryProgressLedger`, the gate on the Cultivate policy — Sustain itself never tames a patch, and the old `claim_threshold` early-claim is **removed**; the accrual is the ladder's, driven off the rung — see "The knowledge pattern"); see "Cultivation"), `hunt.per_worker_biomass_capacity` (per-hunter take cap; biomass→provisions/trade reuses `fauna_config.hunt.*_per_biomass`), `scout.vantage_distance_base`/`vantage_distance_per_scout`/`vantage_distance_max`/`vantage_range` (staffed scouts post forward-observer vantages in all 6 hex directions and reveal LOS from each in `calculate_visibility`, so they see *around* obstacles). **Validated** — `LaborConfig::validate()` runs inside `from_json_str` (every load path, the `fauna_config.rs` convention), rejecting a **partial / all-zero / negative `forage.capacity_by_biome`** (a missing biome would silently read as an invisible zero-forage dead zone — **zero must be stated, never defaulted**); a broken invariant is logged at **error** level (`labor_config.invalid_rejected`) and the builtin is used |
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
offers Tame (a forage patch, or a herd already penned/forage-tended). **Both `pastoralYield` and the
un-penned `corralYield` projection (`managed_yield`) are the SUSTAINED MSY on the improved ecology** —
`hunt_provisions(sustainable_yield(biomass_before_regrowth, carrying_capacity, &{pastoral,pen}_ecology_for(..)))`,
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
- fauna (`fauna.rs`): `hunt_policy_ceiling` (the 4 extractive rungs **+ Corral**) · `hunt_provisions` ·
  **`managed_yield_biomass`** (the husbandry harvest, via `pen_yield_biomass`) · **`herd_ecology` /
  `herd_capacity`** (which ecology/capacity a herd lives under — *no call site may re-derive either*) —
  called by both `systems::hunt_take` / the corral arm of `advance_labor_allocation` **and**
  `hunt_forecast`. The shared `SourceYieldForecast` struct (with `::tended`) is the common return shape.
  A corralled herd's `managed_yield` is **gross**; its `penUpkeep` is exported separately.
- Guarded by `systems::labor_yield_tests::{forage,hunt}_forecast_equals_actual_take_for_every_policy_and_staffing`
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

---

