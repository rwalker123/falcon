---
paths:
  - "core_sim/src/{demographics_config,generations,supply,supply_network_config}.rs"
  - "core_sim/src/{sedentarization,sedentarization_config,settlement_stage_config}.rs"
  - "core_sim/src/{wellbeing_config,victory,provinces,start_profile}.rs"
  - "core_sim/src/systems/{population,trade}.rs"
  - "core_sim/src/snapshot/population.rs"
  - "core_sim/src/data/{demographics_config,supply_network_config,sedentarization_config}.json"
  - "core_sim/tests/{supply_network,sedentarization}.rs"
---

<!-- Extracted verbatim from lines 48-51;4382-4760 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Campaign loop & system activation

## Config files

| File | Purpose |
|------|---------|
| `src/data/sedentarization_config.json` | Sedentarization Score tuning: soft/hard prompt thresholds, EMA `smoothing`, input `weights` (domestication/surplus/resource_density/population), and saturation `references` |
| `src/data/demographics_config.json` | Demographic population tuning: `initial_distribution` (children/working/elders split), `consumption` (per-capita food draw + per-bracket factors), `startup` (`food_reserve_days` seeded into each band's larder + `well_fed_morale_bonus`), `births` (`birth_rate` + the `reserve` stock factor (`bonus`/`saturation_turns`) + the `trend` flow factor (`surplus_gain`/`surplus_saturation`/`deficit_penalty`/`deficit_saturation`); morale-independent), `maturation_rate`/`aging_rate`/`elder_mortality_rate`, `scarcity` (starvation + per-bracket vulnerability, deficit-capped), `cold` (temperature-death). **This file is the SOLE source of demographics tuning** (#350): `demographics_config.rs` has no hand-written `Default` impls — `DemographicsConfig::default()` parses the builtin JSON, and every field is required with `deny_unknown_fields`, so a missing or unknown key is a parse error rather than a silent fallback to a second set of numbers that can drift (it did: `per_capita_draw` was 0.03 in Rust against 0.16 here). Do not re-add `#[serde(default)]` — the root `Default` parses through serde, so a container-level default would make it recurse. **The loader is strict to match**, and that strictness is no longer demographics-specific: it now lives in the shared `config_load.rs` seam and applies to every boot config (see `.claude/rules/core_sim/config-loading.md`). Strictness without a loud loader would only move the silent substitution one layer out — the whole file instead of one key |
| `src/data/supply_network_config.json` | Supply-network tuning: `reach_tiles` (connection radius), `throughput_per_turn` (max goods moved per node/turn), `friction` (fraction lost in transit), `min_transfer` (dead-band) |
| `src/data/wellbeing_config.json` | Civilization Wellbeing tuning: `discontent` (`content_morale`/`floor_morale` productivity curve, `grievance_gain`/`grievance_decay`/`trapped_multiplier`), `productivity` (`floor_mult`, `discontent_weight`), `migration` (own morale-scaled onset: `morale_threshold`, `max_rate`, `base_reach`, `attractive_morale`, `min_morale_gap`, `dependent_weight`) |
## Campaign Loop & System Activation

### Start Flow
- **Boot idle → `new_game`**: `bin/server.rs` boots **IDLE** — it binds its ports and command
  listener but does **not** run the Startup worldgen, so no world exists and nothing is captured or
  broadcast (Bevy's `Startup` schedule only fires on the first `app.update()`, so simply not calling
  `run_turn` leaves the world ungenerated; `ElevationField` stays uninserted, so the Snapshot stage
  must never run on the empty world — see the `world_active` guard). A world is generated **on
  demand** by `new_game <preset_id> <width> <height> <seed> <profile_id>` (proto field **43**; `seed
  == 0` randomizes, mirroring `map_size`/ResetMap; an unknown `profile_id` is rejected without
  building, an unknown `preset_id` falls through to the worldgen default). `new_game` and `map_size`
  (ResetMap) share one world-build helper (`rebuild_world_from_config`). A `turn` sent **before** a
  world exists is rejected with a warning. See `server-dev`'s boot flow in `bin/server.rs`.
- **Data**: `StartProfile` records with `starting_units`, `starting_knowledge_tags`, `inventory`
- **Spawn**: Worldgen seeds the profile's `starting_units`, unlocks `ScoutArea`, `FollowHerd`. Each spawned band's head-count comes from its unit's `band_size` (config lever in `start_profiles.json`; falls back to `DEFAULT_STARTING_BAND_SIZE` = 30 in `start_profile.rs`) — no hardcoded size. `late_forager_tribe` ships a **single ~30-person band** (labor-pool scale per `docs/plan_early_game_labor.md`), not the retired four-band/900-person opening.
- **Camps**: Transient settlement-likes with `PortableBuildings`, `CampStorage`, `DecayOnAbandon` (backlog — not yet built)
- **Sedentarization**: implemented — see the dedicated section below.
- **Founding**: `Command::FoundSettlement { q, r }` requires Founders unit, consumes provisions, spawns Settlement

### Population & Demographics (Settlement & Population Economy — Phase 1)
The bedrock number the rest of the economy builds on. Each `PopulationCohort` (a band — the first
"location"; tile-housed population arrives in Phase 3) carries three fixed-point **age brackets** —
**children / working-age / elders** — plus a local **`stores`** larder (food under the `FOOD` key).
`size` is a derived
`u32` cache of the bracket sum. Design: `docs/plan_settlement_population.md`.

`simulate_population` (`systems.rs`, `TurnStage::Population`) delegates each cohort to the pure
`advance_demographics` (config: `demographics_config.json`):
1. **Consume** — draw `per_capita_draw × weighted_mouths` (dependents eat less) from the band's
   own larder; shortfall is the food **deficit**.
2. **Deaths** — starvation scales with the deficit (dependents more vulnerable via `scarcity`
   weights); cold kills across brackets past `cold.temp_tolerance`.
3. **Births → children** — `birth_rate × working × hunger × reserve × trend` (see "Fertility is
   stock **and** flow" below). Births are **morale-independent** (Civilization Wellbeing — see
   below): contentment doesn't change procreation, and morale **never** causes faction population
   loss. `advance_demographics` no longer takes morale; the retired `births.morale_floor` lever is
   gone.
4. **Maturation** children→working, **aging** working→elders, **elder mortality**. All flows use
   the turn's *opening* values and apply together (a newborn doesn't mature the same turn); the
   total is clamped to `population_cap`. The **dependency ratio** `(children+elders)/working` is
   the core tension.

> #### `elder_mortality_rate` sets the elder SHARE; `initial_distribution` is where the rates settle
>
> Two facts about the bracket flows that decide how this block is tuned, and both are structural
> rather than a property of the shipped numbers.
>
> **The elder bracket is a pure sink.** Elders neither work nor bear children, and nothing leaves the
> bracket except death: births are `working × fertility`, and the child/working pair never reads the
> elder count. So `elder_mortality_rate` does not appear in the growth rate **at all** — it decides
> only how long old age lasts (`1 / rate` turns) and therefore what fraction of a band is elderly.
> Shortening old age is a composition change, never a growth re-tune in disguise, and the pairing is
> pinned by `elder_mortality_moves_the_elder_share_and_not_the_growth_rate` (equal growth *while* the
> shares stay far apart, so a sim that had stopped ageing anyone could not pass it).
>
> **`initial_distribution` is the settled equilibrium of `maturation_rate` / `aging_rate` /
> `elder_mortality_rate` at neutral fertility**, not a free-standing flavour dial. A seed that
> disagrees with the rates is a band drifting off its own declared opening over its first tens of
> turns, which is exactly what makes an early-game figure measured on turn 0 stop describing the band
> by turn 40. Re-tune the rates and the seed moves with them — the shares are
> `C : W : E = (λ−1+aging)/maturation : 1 : aging/(λ−1+elder_mortality)`, normalized, where `λ` is the
> growth eigenvalue of the child/working pair.

> **The flows are REPORTED, not discarded** (issue #272). `DemographicOutcome::flows` carries
> births, **both** age transitions (maturations and agings) and the per-bracket death terms (with the
> dominant `DeathCause` per bracket)
> out of `advance_demographics`, and a per-band `DemographicFlowAccumulator` turns each rate into
> whole-person feed events (`born` / `came_of_age` / `aged` / `died`, plus `migrated` off the
> already-whole
> `last_emigrated`/`last_immigrated`). A rate has to accumulate before it can be an event — a
> thirty-person band earns a fraction of a birth per turn — and the carry is checkpoint state.
> **`elder_mortality` is one of those death terms**, not a separate aging transition: it rides in
> `flows.elder_deaths` with `DeathCause::Age`, because in a fed band in fair weather it is the *only*
> mortality there is, and leaving it out made a healthy band shrink in silence.
> See `.claude/rules/core_sim/event-feed.md`.

> #### Fertility is stock **and** flow — three named factors, not two larder ratios
>
> **`fertility = birth_rate × hunger × reserve × trend`** (`fertility_factors`, design:
> `docs/plan_population_growth_model.md`). The retired model read the **larder only** — both its
> terms divided `food_store` by one turn's demand — so **negative net food did not stop growth and
> barely slowed it**: `surplus_ratio` saturated at a **two-turn** buffer, and the shipped 30-person
> band with income at zero spent ~**18 of its 20 turns** of runway at *peak* fertility before the
> brake engaged, accelerating into the cliff its own growth was causing. The unfiled mirror was just
> as wrong: a band whose income exactly covered consumption read as poor purely for not hoarding.
>
> - **`hunger`** = `consumed / demand` — the retired `fed_ratio`, and the **gate**: the only factor
>   that reaches 0, so an empty larder bears nobody however the other two are tuned. `reserve` ∈
>   `[1, 1.5]` and `trend` ∈ `[0.25, 1.25]` both bracket 1.0 and cannot zero the product, which is
>   why the stack needs **no floor lever** (an early draft had one; it was inert and was dropped).
> - **`reserve`** (stock) = `1 + bonus × min(reserve_turns / saturation_turns, 1)`. Same shape as the
>   retired `surplus_ratio` term with the saturation point promoted to config —
>   **`saturation_turns = 1.0` reproduces the old curve exactly** (pinned by
>   `reserve_saturation_turns_reproduces_the_old_curve_at_one`); the shipped **10.0** makes a band
>   bank roughly a season to earn the full bonus.
> - **`trend`** (flow, new) — two-sided around 1.0 off
>   `net_ratio = (steady_income − demand − pen_feed_upkeep) / demand`, so surplus *raises* fertility
>   as well as deficit lowering it. **`steady_income` is Σ per-source `SourceYield.realized`, never
>   Σ `actual`** — `actual` is lumpy by design (a big-game hunt pays 0 for six turns then spikes) and
>   fertility must not sawtooth with whole-animal timing. **Subtracting `pen_feed_upkeep` is what
>   makes `net_flow` the negation of the same net drain `turnsOfFood` divides by**, so a band whose
>   panel shows a shrinking runway is exactly a band whose `trend` is below 1 — the two readouts
>   cannot disagree about direction.
>
> **Damp, not stop.** `trend.deficit_penalty` (0.75) is the single damp-vs-stop lever: a collapsed
> band still breeds at 25% of base, leaving starvation mortality as the real consequence of a deficit
> rather than punishing one bad stretch twice. **`1.0` stops growth outright** — a config change, not
> a code change (pinned by `deficit_penalty_of_one_stops_growth_outright`).
>
> **`None` flow is NO DATA, never a famine.** `last_yields`/`last_pen_feed_upkeep` are rebuilt each
> turn, so `band_food_flow` must distinguish *unprojected* from *genuinely zero* — the same trap
> already documented for the arrivals schedule in `larder_runway_turns`. **Staffed assignments with
> empty `last_yields`** = telemetry no turn has written → `None` → neutral trend (otherwise a band
> that has not resolved yet would be denied births); **empty `assignments`** = a really idle band → `Some` with zero income. The
> disambiguation is `assignments.is_empty()` and it is pinned by `food_flow_tests`.
>
> Because `simulate_population` runs *before* `advance_labor_allocation`, the flow reading is one
> turn stale by construction — correct, since fertility should track the trend a band has been
> living rather than a single turn's haul.
>
> **The factors are EXPORTED, and the wire's not-projected sentinel is a ZERO RESERVE.** The set
> `simulate_population` resolved is parked on the cohort as `last_fertility_factors` — recomputed
> each turn, exactly like `last_morale_contributions` — and published as
> `PopulationCohortState.fertilityHunger/Reserve/Trend` (fixed-point, **neutral at 1e6, not at 0**:
> these multiply, they do not sum). `advance_demographics` returns them alongside the new bracket
> state (`DemographicOutcome`) rather than the capture re-deriving them, because they are resolved on
> the turn's *opening* brackets and *pre-meal* larder — a recomputation at capture would publish
> numbers that never drove a birth (`the_capture_publishes_the_factors_that_actually_drove_the_births`).
> A cohort that has not yet been through a turn publishes the all-zero default, and **`reserve == 0` is what makes that
> unambiguous**: a computed `reserve` is `1 + bonus × ramp` with both terms ≥ 0, so it is ≥ 1 by
> construction, while `hunger` and `trend` both legitimately reach 0. The client keys "no reading" off
> it and must never render a missing set as a famine — the same no-data rule the `trend` factor itself
> obeys, one level up. `the_returned_factors_multiply_out_to_the_births_they_explain` pins the
> attribution against the observed births, because a breakdown that adds up to the wrong answer is
> worse than no breakdown. Client half: the band panel's **Growth** row + its itemized disclosure.

**Morale attribution (why morale/population falls).** Morale is now computed as the signed sum of a
**named contributor set** (`MoraleContributions` on the cohort — the Layer-1 spine of Civilization
Wellbeing, below): `settling` (`+population_growth_rate`), `terrain` (`−terrain pressure`),
`climate` (`−cold pressure`), `unrest` (crisis impacts + cultural sentiment, signed). Their sum IS
`last_morale_delta`; adding a future factor is a new `MoraleFactor` variant + one field, not a
rewrite of the morale update. The dominant *negative* contributor becomes `last_morale_cause`
(`MoraleCause` ∈ `None | Terrain | Cold | Unrest`) when the delta is negative, else `None`. Drivers:
`Terrain` = terrain attrition + logistics hardness, `Cold` = temperature-difference penalty,
`Unrest` = crisis impacts + cultural sentiment.
Starvation is deliberately **not** a morale cause — it stays on the days-of-food path. The two
place-based (negative) terms come from the shared **`tile_morale_pressure(terrain, temperature,
&MoralePressureConfig)`** helper (`systems.rs`), which returns the tile-intrinsic per-turn morale
drain (terrain + cold, ≥ 0; KarstCavernMouth ≈ 0.0825 at ambient temperature) so the sim and the
snapshot read from one source. The cold term has a **tolerance dead-band**: `max(0, |temp − ambient|
− temperature_morale_tolerance) × temperature_morale_penalty` (config `temperature_morale_tolerance`
= 9.0 in `simulation_config.json`), so temperate mid-latitudes (|Δ| ≤ 9°) bleed **zero** climate
morale and only genuine extremes (poles/high-alt/equator) drain — e.g. at ambient 18° a −5° pole
(|Δ| = 23°) drains `(23−9)·0.004 = 0.056`, a 30° equator (|Δ| = 12°) drains `0.012`. Habitability
reuses this helper, so most of the map rates Hospitable/Fair and only extremes read Harsh/Hostile. These fields are **recomputed each turn** by
`simulate_population`. Exported as `PopulationCohortState.moraleDelta`
(fixed-point `long`, `FIXED_POINT_SCALE` = 1e6) + `moraleCause:ubyte` (`0=None, 1=Terrain, 2=Cold,
3=Unrest`). `TileState.habitability:long` carries the band-independent `tile_morale_pressure` total
for the tile (same fixed-point scale) so the client can rate a hex's harshness. All three are wired
through `sim_schema`/`snapshot.rs`; the client consumes them for a morale trend arrow + named cause
and a Tile-card Habitability line (client half).

**Food is band-local from day one** (the same store a settlement/storage-pit will hold later at
scale). Provisions **left `FactionInventory` entirely**: labor income (forage + hunt, in
`advance_labor_allocation`) and husbandry (`advance_husbandry`, split across the
owner's bands) income now credit the acting band's local `stores` (food under the `FOOD` key). At Startup
(`seed_cohort_demographics`) each band is seeded with `startup.food_reserve_days` turns of its own
demand (`food_demand`, shared with the consumption path) plus a well-fed morale bonus — no faction
provisions grant to distribute. Bands **share** via the supply network (below); storage-pit
distribution is a later addition. Starvation is deficit-capped (a 10% shortfall kills at most 10%)
so a dry larder bleeds down over several turns rather than in one.

Each band's goods live in a `LocalStore` (`components.rs`) — a commodity-keyed bag (food under the
`FOOD` = `"provisions"` key) held on `PopulationCohort.stores`, so the same store carries any future
good. Brackets + store ride the client wire as `PopulationCohortState.stores` so the HUD can render
the exact larder. A per-faction age-structure + dependency-ratio HUD readout ships as
`PopulationDemographicsState` (new `.fbs` table aggregated at capture, wired through
sim_schema/snapshot/native/`Hud.gd` exactly like `SedentarizationState`).

> #### THE WIRE CARRIES WHOLE PEOPLE — the fraction is an accumulator, and it stays sim-side
>
> A bracket is fixed-point because it is a **growth accumulator**: `births = working × fertility` is
> a fraction of a person per turn on a thirty-person band, and a bracket that rounded every turn
> would either invent a birth or never record one (the same reasoning as
> `DemographicFlowAccumulator` in `event-feed.md`). That fraction is not a fact about people, and it
> has exactly **one** correct resolution into people — so the resolution belongs to the sim and the
> raw Scalars do not cross.
>
> `PopulationCohortState.children` / `working` / `elders` are therefore `(deprecated)` FlatBuffers
> slots (the `i64`s survive on the Rust struct — `food_demand`, the fission split and the JSON map
> export all read masses). What a client reads is the whole triple **`childrenCount` / `workingAge`
> / `eldersCount`**, with `childrenCount + workingAge + eldersCount == size` guaranteed because
> `size` is *written* as that sum.
>
> **One derivation, `snapshot::population::whole_age_brackets`, used per band and summed for the
> faction.** Workers are the floored `available_workers` every command already clamps against
> (`BandWorkforce::pool`); dependents are `size − workers`, split between children and elders in
> proportion to their masses, round-half on children with elders taking the remainder so the two sum
> exactly. `snapshot_demographics` adds up the **bands' own published triples**, so the faction page
> cannot disagree with the panels it aggregates.
>
> **A cohort with no dependent mass has no dependents**, and the published head count is its workers.
> With `children == elders == 0` and `working == 16.6` the cached `size` is 17 while 16 people can be
> staffed; splitting that leftover by mass has no basis to split by, and banking it in `elders`
> invented an elder who ate nothing and could never die. The symptom that made this visible was the
> other half of the same rounding: the PEOPLE bar read "17" beside "0 idle of 16" in the WORKFORCE
> header of the same panel, because the client rounded the raws for itself.

### Supply Network (logistics from turn 0)
Bands are small logistics nodes: `balance_supply_networks` (`supply.rs`, `TurnStage::Logistics`,
before Population consumes) connects **same-faction** bands within `reach_tiles` (via
`grid_utils::wrapped_distance_sq`) into **supply networks** (union-find connected components) and
each turn moves every commodity toward a **population-weighted per-capita balance** across the
network. Transfers are **throughput-limited** (`throughput_per_turn` per node) and lose `friction`
in transit; sub-`min_transfer` moves are dropped. So a gatherer band automatically feeds a scout
band it's near (you can specialize labor), while a band beyond reach lives off its own larder.
Reach decides *who* shares, throughput *how fast*, friction the leak — "free neighbor sharing" is
just the high-throughput/low-friction limit. The per-commodity math is the pure, unit-tested
`balance_commodity`. Config: `supply_network_config.json`.

Each turn the same pass also records **network membership** in the `SupplyNetworkMembership`
resource (`entity → id`, cleared and rebuilt every turn): each connected component with ≥ 2 bands
gets a stable id (`1, 2, …` in the BTreeMap's sorted-root order), singletons get none. The capture
reads it into each cohort's snapshot field `supplyNetworkId:uint` (`0` = not in a multi-band
network, `>= 1` = shared id) so the client can draw supply links between co-networked bands. It is
derived, not snapshot-persisted — a rehydrated cohort reads `0` until the next turn's balance.

The cohort snapshot also carries two derived per-band food-readout fields the client renders:
`turnsOfFood:float` — **the honest larder runway: TURNS until the larder is empty, income
included** — and `activity:string` (`idle | forage | hunt | scout | warrior`, the target-kind
with the most workers in the band's `LaborAllocation`). Both are computed at capture in
`population_state`.

> #### `turnsOfFood` is `larder / net drain` — ONE formula for a band and an expedition
>
> **`runway = larder / (consumption + penFeedUpkeep − income)`.** An expedition has no labor income
> and keeps no pens, so it reduces to `provisions / consumption` — **exactly** the historical
> reading, unchanged (pinned by `snapshot::population::tests::an_expedition_reports_provisions_over_consumption`).
> A resident band with real income gets the honest number instead of the old `larder / demand`, which
> **assumed the band stops gathering and hunting** and so read badly pessimistic — a header saying "4"
> above a FOOD OUTLOOK chart showing ~9. Do not special-case the two actors.
>
> It is resolved the way that chart resolves it (`snapshot::population::larder_runway_turns`), so
> they cannot disagree by a turn or two on the same panel: (1) walk the larder forward over the
> **merged per-source `arrivals` schedules**, debiting `consumption + penFeedUpkeep` per turn and
> clamping at 0 — the first turn to reach 0 is the answer; (2) it survives the horizon (or **no
> source was projected at all** — an empty schedule is *no data*, never a famine): fall back to the
> smooth `larder / net_drain` on the **steady** income (Σ per-source `realized`, computed locally at
> capture — see the retirement note below), capped at the sentinel; (3)
> `net_drain <= 0` (net-positive): the `999.0` **not-food-limited** sentinel, which the client
> renders as ∞.
>
> **Consumption here is the forward `food_demand`** (what the people will *want* to eat), not
> `last_food_consumption`: `demand` is always resolvable, where the actual debit is `0` before a
> band's first turn and falls short of demand in a famine. The client's chart drains by
> `foodConsumption` instead, so the two differ **only for a band already eating short** — where the
> sim is the pessimistic (correct) one.
>
> **Consequence, intended:** a band with strong income now reads healthier and **stops tripping
> starvation alerts it should never have tripped** (the map food dot, the turn-orb `starving`
> producer and `_food_is_concerning` all key off this field). Measured on a fed 30-person forage
> band: **4 turns → ∞**. A genuinely starving band is unchanged to within the walk's ±1 clamp. The
> UI thresholds (`band_status_config.json` warn 10 / critical 5) are now measured against a runway
> that is *income-inclusive*, so they fire later by construction — retune there if red arrives too
> late to act on.

Alongside them the snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`,
plus `workRange` (from `labor_config.json` `band_work_range`, global config today, surfaced per-band
for the work-range ring) and `scoutRevealRadius` (**repurposed**: now carries the band's effective
**scout vantage distance** — `scout.vantage_distance(scouts)` = `min(vantage_distance_base + scouts ×
vantage_distance_per_scout, vantage_distance_max)`, `0` with no scouts — since scouts now reveal by
posting forward-observer vantages that see around obstacles; field name kept for wire compat).

**Per-source food-income breakdown (retained yield telemetry).** `advance_labor_allocation` rebuilds
`LaborAllocation.last_yields` each turn — one `SourceYield { actual, sustainable, wasted, workers_needed, overdraws, realized }`
(f32 provisions + a worker count)
per assignment, **in the same index order** as `assignments` (so the snapshot zips by index — every
`LaborAllocation` mutator keeps the two aligned; see "Assign-time yield seeding"). It is **excluded
from `LaborAllocation`'s equality** (manual `PartialEq` compares assignments only) so per-turn
telemetry can't perturb a comparison of two allocations' intent. A row is also written **at assign time**, seeded from the source's
pre-commit forecast, so a brand-new assignment shows its expected yield instead of `+0.00` before the
turn resolves (see "Assign-time yield seeding (the `+0.00` fix)" under Pre-commit Yield Forecast). Definitions: **`actual`** = the provisions the source produced this turn
(the value added to the larder); **`sustainable`** = what it could yield without drawing down its
stock. As of §0-ii **forage is depletable too**, so a forage `sustainable =
sustainable_yield(biomass_before, carrying_capacity, forage.ecology) × forage.provisions_per_biomass ×
output_multiplier`** (**MSY** — regrowth at the most-productive biomass K/2, so a *full* patch still
reads a positive sustainable harvest, no longer 0) — the plant mirror of the
**hunt `sustainable = sustainable_yield(biomass_before, carrying_capacity, ecology) ×
hunt.provisions_per_biomass × output_multiplier`** (MSY at the *pre-take* biomass). `sustainable_yield`
is shared by hunt + forage (`fauna.rs`); `net_biomass_delta` remains the **actual** per-turn biomass
evolution used by `regrow_biomass`/`advance_herds` (0 at K — correct there, unchanged).
A Sustain gather/hunt reads `actual ≈ sustainable`; an over-draw reads `actual > sustainable` (the
overdraw ⚠). Scout/Warrior push `{0,0,0}`. **`workers_needed`** is the parallel **overstaffing**
signal — and it has **two shapes, because the two webs' products differ**:
- **Forage (continuous)** — the *minimum* assigned workers that would have produced the same take:
  `ceil(take_biomass / per_worker_capacity)` clamped into `[1, assigned]` when anything was taken, else
  `0`, via the shared `workers_needed_for_take` helper (capacity = `forage.per_worker_biomass_capacity ×
  seasonal_weight`, matching `forage_take`'s worker cap so a low-season labor-bound patch isn't falsely
  flagged).
- **Hunt (whole animals)** — the **carry crew for the peak animal drop the ceiling allows**
  (`fauna::hunt_haul_workers`,
  `ceil((floor(ceiling/body)+1)·body / hunt.per_worker_biomass_capacity)`, off the stance's
  `hunt_escapement_ceiling`), **NOT** the lumpy `workers_needed_for_take(take.carried, …)`.
  A slow breeder whose room above its floor is lighter than one body drops **0** animals on a wait
  turn, so inverting `carried` would collapse `workers_needed` (to `0`, or the herder count for a
  managed herd) and **contradict the same row's `wasted_yield`** on that turn. Taking the crew on the
  same ceiling the take is bounded by makes the two agree by construction, and it equals the client
  compose panel's `_max_useful_workers` cap. A managed herd wraps it in
  `max(herders_needed, hunt_haul_workers)`; a wild herd (`herders_needed == 0`) reports it directly. See
  "Herding is standing labor" for the full note.
**Every rung derives it** (slice 7 — a managed source used to be fixed at `1`, which asserted that one
worker could carry home whatever the land offered). When the binding constraint on a source's take is **not** labor
(policy ceiling / biomass / regrowth), `workers_needed < assigned` → the source is overstaffed and the
extra workers were idle. The snapshot surfaces all of this: each `LaborAssignment` row
carries `actualYield`/`sustainableYield`/**`workersNeeded`** (client accessor `workersNeeded()`), and
each `PopulationCohortState` carries band-level
`foodIncome` (Σ per-source `actual`) + `foodConsumption` (the food the people **actually ate** this
turn — `PopulationCohort::last_food_consumption`, the real `stores` debit at the turn's *opening*
brackets, **not** a `food_demand` re-derived at capture on the post-turn brackets; the same turn's
births would inflate that and break the larder ledger identity by exactly the growth. `turnsOfFood`
drains by the post-turn `food_demand` instead — a forward "turns I can last", a different question;
see the runway callout above).
All derived at capture (0 on a band no turn has resolved yet). **The client
consumes these next** (allocation-panel rows + tooltip + ledger footer, a follow-up PR): a per-turn
`actual > sustainable` is the client-derived **overhunting signal** — a *leading* flow indicator,
distinct from the stock-based `ecology_phase` — and `workers > workersNeeded` is the **overstaffing**
indicator (flag the wasted labor on the source row + the forage biomass/cap tile-card row).

**The steady headline — `realized` / `realizedYield`.** The lumpy per-source
`actual` makes the band panel's "Food /turn" **swing** turn-to-turn (a whole-animal hunt pays 0 for
~6 turns then a spike). So each `SourceYield` also carries **`realized`** — a **FORWARD PROJECTION**:
the average food/turn the source will deliver over the next `labor_config.yield_average_horizon_turns`
(default **40**) turns, computed by simulating the herd/patch forward from its CURRENT state under the
assignment's policy + worker count (`fauna::project_realized_hunt` / `forage::project_realized_forage`,
mirroring the real turn order Logistics-regrow → Population-take, exactly as
`systems::expeditions::hunt_trip_forecast` does). It is a **pure function of state** — no history, no
persistence — so the assign-time seed and the resolved row compute the identical number (exact
forecast == actual, the true no-jump: `resolved_hunt_realized_equals_the_seeded_realized`). **Simulated
UNQUANTISED:** whole-animal rounding decides *when* the food arrives, never the N-turn total, so
projecting the smooth `hunt_escapement_ceiling` gives the smooth average directly. **Why
not the instantaneous rate** (the bug this replaced): the instantaneous steady rate is
`sustainable_yield(current biomass)`, and biomass *sawtooths* every time a whole animal is killed
(drops one body, regrows between), so an instantaneous reading tracks that sawtooth — the projection's
N-turn average does not. **A food-peak projection sits ABOVE `sustainable` on a source standing over its
floor**, because the first projected turn draws the accumulated surplus down to `K/2` and the rest of
the horizon pays the regrowth; that is honest, not an overdraw. It **uses the assignment's actual
floor**, so dragging the floor re-projects (a herd settled at the food peak reads flat ≈ MSY over the
full horizon; one held below it declines within the window and the average honestly reflects it). A
**self-terminating** floor — `0`, which strips the herd and is the ONLY extinction case
(`fauna.md`: any floor above `0` strips to it and stops) — **breaks the loop early and divides by
the turns ACTUALLY simulated** (not the full cap), so it reads the high strip-rate it delivers *while
the source lasts* instead of a horizon-diluted average (`REALIZED_PROJECTION_TAKE_EPSILON` is the
negligible-take floor that ends the loop). Reuses the shared model helpers (`regrow_biomass`,
`hunt_escapement_ceiling`, `pen_yield_biomass`, `HuntYield::apply`, `forage_take`,
`herd_ecology`/`herd_capacity`) — no second copy of the ecology or take math. On the wire, `LaborAssignment.realizedYield` is appended
(append-only). **The `actual` value and the ledger identity are unchanged — `realized` is a parallel
steady value, never a replacement.** `PopulationCohortState.foodIncome` = Σ `actual` stays exactly as
it is: it is the real arrivals and is load-bearing for the
`larder_delta == foodIncome − foodConsumption − penFeedUpkeep − raidForfeit` ledger identity.

> **The ledger identity gained a fourth term with Predators Phase 3 (`combat.md`).**
> `PopulationCohortState.raidForfeit` (`LaborAllocation::last_raid_forfeit`) is the food a
> casualty-causing predator raid forfeits — `predators.raid_yield_forfeit_fraction` of that turn's
> income, a real `LocalStore::take` debit that lands in **neither** `foodIncome` nor `foodConsumption`,
> exactly like `penFeedUpkeep`. So the identity is now
> `larder_delta == foodIncome − foodConsumption − penFeedUpkeep − raidForfeit`, pinned through a real
> raid turn by `integration_tests/tests/raid_food_ledger.rs`. **It is a PAST-turn stochastic debit, not
> a recurring cost, so — unlike `penFeedUpkeep` — it does NOT enter the `turnsOfFood` forward-runway
> drain** (the runway drains only by `consumption + penFeedUpkeep`; see the runway callout above).

> **RETIRED: the band-level `PopulationCohortState.foodIncomeAverage` (= Σ `realized`).** The client
> sums the Food line's income half **itself**, from the per-source `realizedYield` of the breakdown
> rows it renders, so the headline equals the Gathered + Hunted rows it sits above **by construction**
> rather than being a second, independently-computed total that could drift from them. That made a
> band-level duplicate redundant, and it was read by nobody. Marked `(deprecated)` in `snapshot.fbs`
> rather than deleted — deleting frees the field id for the next appender, and this repo is worked by
> concurrent sessions that append to these tables, so a freed slot is exactly how two branches collide
> on one id. **Do not re-add it**: if a band-level steady income is ever wanted again, sum the rows.
> The Σ `realized` value still exists as a *local* in `snapshot::population`, because
> `larder_runway_turns` needs a steady income term; it is simply not exported.

**WHEN the food lands — `arrivals` / `arrivalSchedule`.** The discrete twin of `realized`, from the
**same** forward simulation run **WITH** the kill-credit bank (`fauna::project_arrivals_hunt` /
`forage::project_arrivals_forage`) — because the two answer opposite halves of one question: the bank
decides *when* a whole animal lands and never *how much* lands over the window, so `realized` drops it
to get the smooth average and this keeps it to get the timing. `SourceYield.arrivals[i]` is the food
delivered `i + 1` turns from now, `labor_config.arrivals_horizon_turns` (**20**) entries long, `0.0` on
a turn nothing lands. A big-game Sustain hunt reads genuinely lumpy (zeros between hauls); a forage
patch — or fast game whose MSY clears a body every turn — is positive in **every** slot, a *continuous*
source the client renders as a solid run, with no special case in the projection. Their totals agree by
construction: `Σ arrivals ≈ realized × horizon`, to within the partial body still banked at the end.
- **It starts from the herd's REAL `hunt_credit`, never zero**, and it is projected from the source's
  **POST-take** state — so slot 0 is genuinely the *next* delivery, not the one this turn already paid.
  Both are load-bearing and both are pinned: zeroing the bank, or projecting pre-take, shifts the
  **first** arrival — the one the player cares most about — and
  `labor_allocation::the_arrival_schedule_matches_a_real_driven_hunt` fails on the exact turn index.
- **Pinned to real behaviour, not to another forecast** (the ~34-vs-~6-turn lesson): that test reads
  the schedule published on turn 0, then drives the **real** systems forward `horizon` turns and
  asserts the sim delivered on exactly the named turns in exactly the named amounts.
- Reuses the shared take helpers verbatim (`regrow_biomass`, `hunt_escapement_ceiling`,
  `quantise_animal_take`, `pen_yield_biomass`, `HuntYield::apply`,
  `forage_take`, `herd_ecology`/`herd_capacity`) — **no second copy of the take math** — and simulates
  on a clone, never the live source. Unlike the `realized` projection it does **not** break on a
  zero take: there a zero means *spent* and would dilute an average; here it is a **wait** turn, which
  is the entire mechanic. Only the extinction floor ends a hunt schedule early.
- **`arrivals_horizon_turns` is its OWN lever** (`labor_config.json`, default **20**, validated `> 0`),
  deliberately separate from `yield_average_horizon_turns` (40): this is a *display span* the client
  charts turn-by-turn, that one is a *smoothing window*.
- On the wire: `LaborAssignment.arrivalSchedule:[float]` (append-only, after `realizedYield`), on both
  `WorldSnapshot` and `WorldDelta`. A flat `[float]` rather than a vector of `{turn, amount}` tables is
  deliberate — **the index IS the turn offset**, so it needs no per-entry table and stays compact. An
  **empty** vector means *not projected* (Scout/Warrior, or an unresolved `SourceYield::ZERO`), which the
  client must read as "no data", never as famine. **Client follow-up:** nothing renders it yet; the
  merged per-band larder projection is the client's to compose from these plus consumption — the sim
  owns the model (when + how much), walking the larder is presentation.

**The understaffing mirror — `wastedYield`** (slice 7, appended to `LaborAssignment`). `workersNeeded`
only ever answered *"are there too many workers here?"*; nothing answered *"too few?"*. `SourceYield.wasted`
= `production − actual` — what the source **offered that the crew could not collect**, where *production*
is what it hands over this turn (the policy ceiling on a wild/tended source, the managed rate on a
Field/pen) and *collection* is `workers × per-worker throughput`. The pair now answers both halves:
`workers > workersNeeded` ⇒ drop some, `wastedYield > 0` ⇒ add some. On a **Field or a pen** it is
genuinely food left standing (the crop rots / the meat stays on the hoof); on the **drawn-down rungs** it
simply stays in the stock and regrows. **Client follow-up:** nothing renders it yet.

All of the above is **post-hoc** (it reports what a committed turn produced). Its **pre-commit** twin —
the per-source `perWorkerYield` + policy ceilings the client uses to show an expected yield and cap the
worker stepper *before* the player commits — is the "Pre-commit Yield Forecast" section below, which
shares the take path's yield helpers so forecast == actual.

This is the general mechanism the arc scales: raise reach/throughput for settlements/cities. The
cross-faction half is no longer a "trade policy on the supply network" — that framing died with the
`TradeLink` slice it assumed. `docs/plan_contact_and_logistics.md` §Q4 **re-founds this network on a
primitive instead**: proximity produces a *connection*, a logistics link is a *rider* on one, and
over a short distance it is cheap enough to hold itself — so two bands standing near each other
behave exactly as they do today, by construction. A cross-faction edge then differs only in whose
endpoints it joins, never in a branch in the code. *v1:* population is the universal balancing weight, so a zero-population storage
node would compute a 0 fair share — revisit (→ capacity weight) when storage-pits land. The
connected-components pass is also what Phase 4 will use to derive settlement clusters.

### Sedentarization
The emergent per-faction "pressure to root in place" — the first slice of the pastoral→
settlement chain, and the consumer of Phase E's domestication seam.

`sedentarization_tick` (`sedentarization.rs`, `TurnStage::Population` after
`advance_labor_allocation`) computes a per-faction 0–100 **`SedentarizationScore`** each turn as
a config-weighted blend of normalized inputs, then **EMA-smooths** it (`smoothing`):
- **domestication** = `(HerdRegistry::domesticated_count(faction) +
  ForageRegistry::cultivated_count(faction)) / references.domesticated_herds` (the Phase E seam +
  the Phase 1a cultivation fold-in — plant + animal domestication share one driver; see "Cultivation"),
- **surplus** = Σ band `stores` food larders / `references.surplus` (band-local food, Phase 1),
- **resource density** = `HerdDensityMap::normalized_average()` (map-wide game richness — a v1
  baseline; per-faction-local density is a future refinement),
- **population** = Σ cohort size / `references.population`.

On a **rising** crossing of `soft_threshold` (~40, "establish a seasonal base?") or
`hard_threshold` (~70, "settle?") it pushes a `CommandEventKind::SedentarizationPrompt` to the
command feed (edge-gated on the stored `SedentarizationStage` so it doesn't re-fire; a fall
lowers the stage silently). The score is exported per-faction in the snapshot
(`SedentarizationState`, mirroring `factionInventory`) and shown as a HUD meter. Tunables live
in `data/sedentarization_config.json` (`sedentarization_config.rs`).

> **Reframed by the Settlement & Population Economy arc** (`docs/plan_settlement_population.md`):
> settlements are *derived* from clustered populated tiles + tended improvements (there is no
> discrete founding), and `SedentarizationScore` becomes an emergent readout of accumulated
> *tether* rather than a gate. See that design doc for the population/labor/improvement model
> this score ultimately feeds.

### Civilization Wellbeing (Morale → Discontent → Consequences)
The three-layer spine **factors → morale → discontent → consequences** (Phase 1). Authoritative
design: `docs/plan_civ_wellbeing.md`. Config: `wellbeing_config.rs` / `data/wellbeing_config.json`.
Extension seams are present and empty — future factors/consequences slot in without a rewrite.

- **Layer 1 — factors → morale.** `simulate_population` builds `MoraleContributions` (see morale
  attribution above); morale trends by their signed sum. Adding a factor = a new `MoraleFactor`
  variant + one field. The contributor set doubles as the client's itemized morale breakdown.
- **Layer 2 — discontent state (productivity only).** Each turn the cohort's `discontent_fraction =
  clamp((content_morale − morale) / (content_morale − floor_morale), 0, 1)` (0 at ≥`content_morale`
  0.6, 1 at ≤`floor_morale` 0.1). This drives **productivity only** — migration has its own onset
  (Layer 3b). A `grievance` accumulator (severity × duration) rises by `grievance_gain ×
  discontent_fraction` (× `trapped_multiplier` when *trapped* — below the migration threshold with no
  reachable destination) and decays by `grievance_decay` while content. **Phase 1 only populates
  `grievance`** — no consequence reads it (reserved for a future revolution trigger); it rides the
  client wire as `PopulationCohortState.grievance` (like `age_turns`) so the HUD can show brewing unrest.
- **Layer 3a — productivity modifier stack.** `output_multiplier(cohort, cfg) = Π(modifiers)`
  (`systems.rs`). Phase 1 has one entry, `discontent_output_modifier = max(floor_mult, 1 −
  discontent_fraction × discontent_weight)` (floor 0.5, weight 1.0). Applied at **payout** at every
  yield site via a single `output_multiplier` call — forage + hunt take (`advance_labor_allocation`),
  husbandry (`advance_husbandry`, `fauna.rs`). Adding
  an education/tech/government modifier is one line in `output_multiplier`, not per-site edits.
- **Layer 3b — tech-gated migration (own morale onset).** `advance_population_migration`
  (`systems.rs`, `TurnStage::Population`, **after** demographics + this turn's payouts).
  **Decoupled from `discontent_fraction`** — migration has its own morale-scaled onset at
  `migration.morale_threshold` (0.25): each band sheds `total × move_fraction`, where
  `move_fraction = max_rate × clamp((morale_threshold − morale) / morale_threshold, 0, 1)` — 0 at
  morale ≥ 0.25, 7.5% at 0.125, up to `max_rate` (0.15) at rock-bottom (gentle at onset, ramping to
  the cap). The total is split across brackets ∝ `bracket_size × weight` (working = 1.0, dependents
  = `dependent_weight` 0.4), so leavers are mostly workers while the headline fraction stays exact.
  They seek the **highest-morale eligible same-faction band within reach** (`base_reach` 4 hexes ×
  a movement-tech factor). *No concrete movement/transport tech signal exists yet, so the factor is
  stubbed at 1.0 with a `TODO(phase2)` hook.* Eligible = `morale ≥ attractive_morale` (0.5) AND
  `morale > source + min_morale_gap` (0.05). Found → **relocate** (source shrinks, destination
  grows; `last_emigrated`/`last_immigrated` recorded); none reachable → **stay** (grievance accrues
  faster via the trapped bonus). **Morale never causes faction population loss** — population is
  conserved within the faction; loss stays with starvation/cold only. Destinations are chosen from
  one pre-migration snapshot and all moves are computed before any is applied, so relocation is
  order-independent.
- **Snapshot.** `PopulationCohortState` gains `outputMultiplier`, `discontentFraction`, `grievance`,
  `lastEmigrated`/`lastImmigrated`, and the four itemized contributions
  `moraleSettling/Terrain/Climate/Unrest` (surfaced so the client can render the breakdown). All
  fixed-point except the two head-counts; all derived per-turn except `grievance` (persisted). The
  birth path's parallel trio `fertilityHunger/Reserve/Trend` rides beside them — see "Fertility is
  stock **and** flow" for why its neutral point is 1.0 and its sentinel is a zero reserve.

### Capability Flags
`CapabilityFlags` bitflags: `AlwaysOn`, `Construction`, `IndustryT1/T2`, `Power`, `NavalOps`, `AirOps`, `EspionageT2`, `Megaprojects`. Systems are inert until corresponding flag is set.

### Victory Engine
`VictoryState` with per-mode progress meters. Modes: Hegemony, Ascension, Economic, Diplomatic, Stewardship, Survival. `victory_tick` runs after end-of-turn accounting.

---

