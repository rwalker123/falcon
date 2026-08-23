---
paths:
  - "core_sim/src/{fauna,fauna_config,creatures_config}.rs"
  - "core_sim/src/data/{fauna_config,creatures}.json"
  - "core_sim/tests/fauna_*.rs"
---

<!-- Extracted verbatim from lines 44-44;55-56;1099-1588 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Fauna & wild game

## Config files

| File | Purpose |
|------|---------|
| `src/data/fauna_config.json` | Wild-game species table (display, size class, migratory flag, route length = anchor count, biomass, host biomes, + movement cadence `dwell_turns` / migratory `loiter_turns [min,max]` / `loiter_radius`, + **`fodder_per_biomass`** (Grazing 2b-i — graze the herd eats per unit biomass/turn; cached on `Herd` at spawn) + **`regrowth_rate`** (Grazing 2b-ii — per-species WILD breeding rate, `Option`, cached on `Herd`; rabbit/fowl 0.35, deer/boar 0.10, migratory 0.04 — replaces the single global `ecology.regrowth_rate` for wild herds; see "Phase 2b-ii") + **`taming_cost_multiplier`** (a **per-species multiplier on the `animal:pastoral` rung's `work_cost`**, default **1.0**; the rung owns the taming mechanic, the species PRICES it (the `regrowth_rate`/`pastoral_gain` split again). It scales the **cost**, and `build_decay` reads `decay_fraction_per_turn` off that same scaled cost, so the rung's build:decay ratio is invariant for free: *slow to tame, slow to forget*. Roster: rabbit/fowl/crag_goat/wild_sheep/snow_hare 1.0 (50 work units), boar 1.25 (62.5), aurochs 2.0 (100), steppe_runner/marsh_grazer/reindeer/wild_horse 5.0 (250); deer/mammoth omit it (`wild` ceiling — never tame). **It inverted from the retired `taming_rate`** (`docs/plan_unit_costed_work.md` §3.1): `0.2` said *your people are five times worse at their job on this animal*, `5.0` says *the animal is five times the work* — same pacing at the same crew, honest sentence, and it composes with a later cost spread where a rate could not. **Playtest dials.** Validated finite & `> 0`; resolved live by display name (`FaunaConfig::taming_cost_multiplier_for`), *not* cached on `Herd`, so a retune reaches herds already on the map. See "The `Tame` verb") + **`husbandry_ceiling`** (Grazing 2d-δ — `wild`|`pastoral`|`pen`, default `pen`; how far up the ladder the species climbs — mammoth/deer `wild`, steppe_runner/marsh_grazer `pastoral`, boar/rabbit/fowl `pen`; cached on `Herd`, gates domestication + corral/extend; see "Phase 2d") + **`pastoral_density` / `pen_density`** (the per-species husbandry DENSITY (K) multiplier per rung, default **1.0** = neutral; domestication makes the LAND hold more animals, non-linearly by species — DISTINCT from the global r-gains, which scale the breeding rate not the ceiling. Roster: crag_goat/aurochs 2.0/5.0, boar 1.5/4.0, rabbit/fowl 1.1/1.5, steppe_runner/marsh_grazer 1.5/1.0 (pastoral only — pen inert), deer/mammoth omit both (wild → ×1). Applied at the one K seam `ecological_carrying_capacity` via `fauna::herd_density_gain`, resolved live by display name (`FaunaConfig::pen_density_for`/`pastoral_density_for`), *not* cached on `Herd`. **Playtest dials.** Validated finite & `>= 1.0` (a gain below 1 would make domestication reduce capacity). See "The husbandry yield ladder") + **`adjacent_water`** (the **shore predicate**, `none`\|`any`\|`salt`\|`fresh`, default **`none`** so every other species is byte-identical — a species that sets it may only spawn on a land tile that **borders open water of that kind** on one of its six hex sides (`fauna::adjacent_water_kinds`), the site rule filtering the short-range spawn's candidate list *before* the pick. **The kind is load-bearing:** `salt` = `WATER` **without** `FRESHWATER` (the ocean — the same rule `hydrology.rs`'s `TileWorld::is_ocean` states, in the same tag vocabulary), `fresh` = `WATER` **with** it (a lake, an `InlandSea`, a `NavigableRiver`), `any` = either. A blanket any-`WATER` test let a **Grey Seal colony haul out beside a one-hex freshwater lake** — seals are marine, so `seal` carries **`salt`**; the freshwater **`river_fish`** (Silt Catfish) carries **`any`**, which is byte-identical to its pre-split behaviour. Shipped on those two rows only; the seal pairs it with `host_biomes: ["boreal_arctic", "coastal_littoral"]` — **the cold half comes from `host_biomes`, NOT from a climate gate**: `climate::climate_band_for_temperature` is the single climate authority and a second one here would be a parallel authority that drifts from it. It **READS** the coastline geometry the worldgen stamped and never edits terrain. **Validated: `migratory: true` + any non-`none` value is REJECTED** — the migratory placement path (`suitable_tiles_for`/`build_migratory_route`) does not apply site rules, so the combination would be *silently ignored*; the unhandled state is made unrepresentable and loud instead. Measured on 6 seeds of the standard map: seals **2 → 14 colonies over the sweep** (0–1 → 0–4 per map), against 44–94 water-adjacent `boreal_arctic` tiles per map — see `core_sim/tests/fauna_coastal_habitat.rs`. **The guard is now habitat tiles PLUS colonies, because they catch different regressions**: the water-adjacent `boreal_arctic` count (floor 350 against a measured 419) is a raw terrain+adjacency reading, so it is causal and near-deterministic and is what a climate/moisture change actually moves, while the colony count (floor 4 against a measured 7) is a probabilistic roll under a map-wide cap and catches only a roster/site-rule regression. The colony floor was lowered 8 → 4 after the #332 crest-released rain shadow left habitat flat-or-up on all six seeds (413 → 419) while the placement roll reshuffled — it had been sitting on its own floor with zero headroom, so it fired on noise. **The seal pairs it with `route_len: [1, 1]`, and that is load-bearing, not incidental:** the site rule filters *placement* only — nothing in it stops `advance_herds` walking a colony inland on turn 1, and with the shipped `[1, 2]` it did (measured: a colony drifted `(24,21) → (23,22)`). A single anchor **is** the spawn tile, so `step_index` cycles `(0+1)%1 = 0` and `step_herd_toward` is handed the herd's own position — the colony is a fixed **haul-out**, which is what makes the shore invariant *structural* rather than placement-time. A rookery is a site the animals swim out from, not a herd that wanders overland. **Do not restore a multi-anchor route to a species carrying a site rule** without making roam site-aware, or the rule silently degrades to placement-only)) + per-biome spawn abundance (`abundance.per_biome` + the `max_total_game` cap for short-range game, and **`abundance.migratory`** — `tiles_per_herd` **800** / `min_herds` **2** / `max_herds` **12**, the SEPARATE per-map budget for long-route herds, read only by `MigratoryAbundanceConfig::herds_for_map`. Promoted from three bare literals in the retired `fauna::determine_herd_count` (`area/3000` clamped `[2,6]`), under which the standard 80x52 map computed 1, was clamp-floored to 2, and put every migratory species on only ~36% of maps. Validated `tiles_per_herd > 0` (a divisor), `min_herds >= 1`, `max_herds >= min_herds`. **A food-economy dial, not a variety dial** — a migratory herd is 4,000-12,000 biomass. See "The migratory herd budget is CONFIG") + `hunt` / `follow` / `ecology` (regrowth + depensation collapse thresholds) / `immigration` (respawn) / `husbandry` (**the flow-based yield ladder**: **per-species managed `r`** (Grazing 2d — `pastoral_gain` 2.0 / `pen_gain` 4.0 scale each species' own wild `r`, capped at `husbandry_regrowth_cap` 1.0, retiring the flat `pastoral.ecology.r` 0.25 / `pen.ecology.r` 0.90 which now carry phase bands only) and `pen` (**`upkeep_per_biomass`** — the pen's **gross** feed rate; `× biomass` is the `penUpkeep` wire field, the SAME basis `corralYield` uses. The footprint's pasture and any hay **offset** it into the separate net `larder_upkeep` the keeper actually pays (exported render-ready as `penLarderBill`/`penHayFood`) — the *lever itself stays gross* — / `starve_shrink_rate`; `capacity_fraction` is **deleted** — a penned herd's `K` is its fenced-footprint graze flow) + the **neglect-escape shed rates** (`docs/plan_fauna_neglect_escape.md`) `pastoral_escape_fraction` **0.25** / `pen_escape_fraction` **0.10** (fraction of an under-contained herd's labor-capacity *overage* that sheds to the wild web per turn — pen slower, the fence buys time) / `escape_fraction_jitter` **0.25** (the ±band the seeded RNG applies; validated finite, `>= 0`, `pen < pastoral`), the **`Corral` policy**'s investment levers having **moved to `intensification_ladder.json`'s `animal:pen` rung** (the old `corralling_yield_fraction` → `yield_fraction_while_building` 0.50, `corral_build_progress_per_turn` → the rung's **`work_cost` 75** work units, 25 turns at a reference crew of 3); every rung pays MSY against its own ecology, see "The husbandry yield ladder" / "Phase 2d") + **`hunt_yield`** (the per-species HUNT-YIELD VECTOR, `docs/plan_hunt_yield_model.md` §3 — `{provisions_per_biomass?, materials[]}`, what a take of this species PAYS per unit of biomass. The **product** half of *yield = product × intensity*; how MUCH biomass is the stance's job (`hunt_escapement_ceiling`), and the two axes are orthogonal. The rate omitted ⇒ the `hunt.*` global, so every species but the wolf is byte-identical; an explicit **`0.0` is a real value**, not 'unset' — it is how a wolf says *you do not eat me* (which is why the field is `Option`). `edible`/`yields_nothing` are **DERIVED** from the vector, never stored. Roster: only `wolf` declares the rate — `0.0` — and its whole payload is the `materials` rows beside it (hide + bone). Validated when PRESENT: finite & `>= 0`. Resolved live by display name (`FaunaConfig::hunt_yield_for` — THE single seam; no call site may read the `hunt.*` global for a take). Mirrors `flora_config`'s per-species `yield`, so the two food webs are the same shape. **`trade_goods_per_biomass` is RETIRED (arc #527)** on both the per-species block and the `hunt.*` global — it was written by every take site and read by none, while `materials` named the same take's concrete hide and bone; `HuntYield::yields_materials` is what carries the second half of *is this worth hunting*, which is what keeps an inedible species huntable at every floor. **Playtest dials.**) tuning (**the `market` block is RETIRED** — its 4× `trade_goods_multiplier` paid one rung a product bonus, re-welding product to policy, and the axis it multiplied went with arc #527) + **`graze`** (the pasture layer, Grazing Phase 2a — `capacity_by_biome` a **total** per-biome table (one row per `TerrainType`), `ecology` (`regrowth_rate` **0.40**, the fastest vegetal stock in the model), `reseed_floor_fraction` 0.02, **`overgraze_escapement_fraction` 0.25** (Grazing 2b-ii — grazing can't draw a patch below this, the constant-escapement floor that keeps the herd↔graze loop convergent); see "The Graze (Pasture) Layer" / "Phase 2b-ii"). **Validated** — `FaunaConfig::validate()` runs inside `from_json_str` (every load path), rejecting a pen that eats more than it yields, an inverted ladder, a dead ecology, or a **partial / all-zero / negative graze table** (a missing biome would silently read as an invisible zero-graze dead zone); a broken invariant is logged at **error** level (`fauna_config.invalid_rejected`) and the builtin is used |
| `src/data/creatures.json` | **The creatures roster** (Predators Phase 0; loader `creatures_config.rs`, env override `CREATURES_CONFIG_PATH`). Intrinsic `CombatStats` for **non-fauna** units — today one row, `"person"` (`attack 1, defense 1, range melee`). A human is not wildlife (not `fauna_config`) and its stats are not resolver tuning (not `combat_config`) — a combatant is *creature ⊕ equipment*, and this holds the base human creature. **Validated** inside `from_json_str` (the `"person"` row must exist; every row's `attack ≥ 0` finite, `defense > 0` finite); rejected at **error** level (`creatures_config.invalid_rejected`) → builtin. See "Combat & Casualties" |

## What a hunt is MADE OF is on the wire, per material (arc #527)

Retiring the trade-goods axis left an inedible quarry with **nothing to quote**. A wolf paid
`trade_goods_per_biomass: 0.02`, so its compose sheet had a rate; with that gone its
`provisionsPerBiomass` and `perWorkerYield` are honestly `0`, its board row and map label read
`+0.00`, and the pelts still landed in the band's store when the hunt resolved. The client was
reading the contract correctly — **the contract was wrong**.

Three fields close it, and they are the fauna mirror of the crop picker's cash quote
(`flora.md` → "The crop picker's cash quote is PER MATERIAL"), deliberately the same shape:

| Field | On | Answers |
|---|---|---|
| `HerdTelemetryState.materialPerBiomass` | the herd row | what **one unit of this herd's biomass** is made of — the material twin of `provisionsPerBiomass`, so it composes at **any** floor by the rule the scalar rates already use: `ceiling(floor) = max(0, B − floor·K) × rate` |
| `HerdTelemetryState.perWorkerMaterial` | the herd row | what **one hunter** brings home per turn — the twin of `perWorkerYield`, so a band preview clamps `min(workers × rate, ceiling)` **per material** exactly as it does for food |
| `LaborAssignment.materialYield` | the assignment row | what this source **actually credited this turn** — the third account beside `actualYield` and `fodderYield`, and the `+0.00` fix on both webs |

All three are `[MaterialPayoff { materialId, amount }]`, the table the flora quote already uses — **no
second table was minted**.

**Three contracts, all shared with the flora quote:**

- **Never summed.** One entry per material; a total is the retired trade axis under a new name.
- **Empty is "no row", never zero.** Most species are made of nothing anyone builds with, and a
  published `0` would read as a herd that pays badly rather than one that pays in meat alone.
- **The key is always present**, so a reader can tell *"no quote sent"* from *"this herd pays no
  material"*.

**Priced through the expressions the payout runs, not beside them.** The two rates are the species'
own `hunt_yield.materials` rows — the very rows `credit_material_yield` is handed at the take site —
through the same two biomass terms every other field on that row uses (`ONE_UNIT_OF_BIOMASS` and
`hunt.per_worker_biomass_capacity`). Nothing is re-derived, so a rate retune moves the quote and the
payout together.

**BAND-AGNOSTIC, at an output multiplier of `1.0`** (`FORECAST_OUTPUT_MULTIPLIER`), exactly like
every food field on the same row — a herd row serves every band, so **the consumer multiplies by its
band's `outputMultiplier`**, as `yield-forecast.md` requires of the whole row. A client that took the
rate as already-multiplied would under-report a productive band's haul while its food siblings scaled
correctly, so the row scales together or not at all. The band's real multiplier enters at the take
site, where `credit_material_yield` is paid.

> **`credit_material_yield` now RETURNS what it deposited**, merged per material id, and
> `SourceYield::materials` is that return verbatim — the *"reported, never recomputed"* discipline
> `SourceYield::fodder` already carries. It matters more here: the credit skips a sub-quantum amount
> and an unknown material, and **neither skip is visible to a second derivation**. That return is what
> makes the guard below an assertion about the store rather than about arithmetic.

**A PRE-COMMIT row publishes an empty `materialYield`, and that is a stated gap.** Projecting
materials needs the take in **biomass**; `forecast_production_and_take_at` resolves the take in
currency space, where an inedible species has no positive axis to count on (see
`yield-forecast.md` → "THE TRADE-GOODS ACCOUNT IS RETIRED"). So a freshly composed wolf assignment's
*row* still reads nothing until its first turn resolves — and it does not matter, because the number
a player decides on is the herd row's **rates**, which need no take at all. That is the same division
`perWorkerBiomass` already makes for the crew question.

**The guard is `hunt_yield_vector::a_wolfs_published_material_quote_is_what_the_hunt_credits`**, and a
wolf is the subject precisely because its *entire* yield is material — every food reading it publishes
is an honest zero, so nothing else on its row can cover for a missing quote. Four claims: the herd row
quotes a rate where its food rate is `0`; the resolved row publishes what it credited; **that amount
is what `LocalStore::material_total` actually holds** (asserted against the store, never against a
re-derivation); and every material's `credited ÷ published rate` is the **same** positive number — the
carried biomass — which a rate published from a second derivation would not satisfy.

**The two INVESTMENT rungs quote materials too** — `HerdTelemetryState.corralMaterial` /
`pastoralMaterial`, the twins of `corralYield`/`pastoralYield` and the replacement for the retired
`corralTrade`/`pastoralTrade`. Without them an inedible quarry's Tame and Corral rungs quoted nothing
at all: a wolf's food payoff on both is honestly `0`, so the compose sheet's *"→ then +Y"* had no
number on either. They are priced on the **same** pen/pastoral MSY biomass their food siblings are —
`SourceYieldForecast` now hands both over (`managed_yield_biomass` / `pastoral_yield_biomass`,
resolved once in `hunt_forecast`) precisely so a rung's two readouts cannot describe different
harvests. That the forecast has to *retain* a biomass at all is the same asymmetry
`forage::field_harvest_biomass` states one web over: the material account has no currency to scale
off on a species whose currency components are all `0`.

**That "one harvest" claim is asserted rather than asserted-in-prose.**
`crafting_wire::every_sources_material_rate_reaches_the_wire` reads each rung's vector off the
decoded snapshot, derives the **single** biomass factor it is `materialPerBiomass` scaled by, and
requires that factor `×` the row's own `provisionsPerBiomass` to reproduce `corralYield` /
`pastoralYield`. A rung priced off a second `sustainable_yield` call fails it; so does a swap of the
two slots, which the ordering check (`corral >= pastoral` — the pen breeds at `r × 4` against the
pastoral rung's `r × 2`) catches even on an inedible quarry, where there is no food sibling to tie
to.

**Deliberately out of scope: the denial raid's wasted materials.** A carcass left on the range takes
its hide with it, and `DenialForecast` still says nothing about that. **The reason is a decision, not
a shape problem** — `DenialRow.deliveredMaterial` proves a per-material vector states a projection
fine, and the *delivered* half is built. The waste is already legible as a percentage, so its material
twin buys a second reading of a fact the sheet states. Do not add a flat "wasted materials" scalar —
see `expeditions.md`.

## Fauna & Wild Game

Mobile animal **groups** (not individuals) graze-wander / migrate across the map
independent of the gather layer (see "Movement" below). One entity = one
band/warren/herd; `biomass` = group size.

**Species table** (`src/data/fauna_config.json`, loader `fauna_config.rs`): the
former hard-coded `HerdSpecies` enum is now a data-driven table. Each row has a
`display_name` (also the snapshot `species` string — it embeds the client icon
keyword, e.g. "Red Deer" → 🦌), `size_class` (`migratory`/`big`/`small`),
`migratory` flag, `route_len` `[min,max]` (= roaming range), `biomass` `[min,max]`
(group size), and `host_biomes` (a list of **`FoodModule` keys**, reusing
`classify_food_module`). Shipped roster (19 rows): **migratory** mammoth/steppe_runner/
marsh_grazer/reindeer/wild_horse (long routes); **big game** deer/boar/aurochs/seal/wild_elk
(2–3 tiles); **small game** rabbit/fowl/crag_goat/wild_sheep/alpine_ibex/gazelle/forest_grouse/
**river_fish**/**snow_hare** (~1 tile, stationary). The `pen`-ceiling **livestock** are
aurochs (🦬, wild r 0.09 → slow ranch cattle) on grass + woodland edge, **Crag Goats** (🐐,
wild r 0.22 → fast hardy hill stock) on highland/dry-upland, plus boar, rabbit, fowl,
wild_sheep and snow_hare.

**Regional signature — every biome offers distinct game, and every land biome offers a pen.**
Three roster rows close the last gaps:
- **`river_fish` ("Silt Catfish")** — the wet biomes' own game, hosting
  `riverine_delta`/`wetland_swamp`/`coastal_littoral`. Structurally the `seal` row: a
  non-grazing, non-migratory colony with `route_len [1,1]`, pinned to the shore by
  **`adjacent_water: "any"`** — freshwater game, so unlike the seal it wants a shore of *either* kind
  — so it needs no new Rust. A catfish inland is a bug.
- **`snow_hare` ("Snow Hare Warren")** — hosts **`boreal_arctic` alone** at a **`pen`** ceiling.
  Before it, `boreal_arctic` was the **only land biome with no `pen`-ceiling species at all**: the
  mammoth and elk there are `wild`, the reindeer only `pastoral`, so the intensification ladder's
  pen rung was flat unreachable from a northern start. The hare is what makes it reachable.

> **`host_biomes` names a MODULE, not a terrain — and `montane_highland` is the leaky one.**
> `CanyonBadlands` (an arid desert canyon) carries `ARID | HIGHLAND` and reaches the **fallback**
> arm of `classify_food_module_from_traits`, where the `HIGHLAND` test runs **before** the `ARID`
> test — so arid badlands classify as `montane_highland`. A live playtest duly found snow hares
> warrening in a desert canyon. There is no way to target one exact `TerrainType`, so the fix is to
> drop the module: `boreal_arctic` is an **explicit** arm (BorealTaiga | Tundra | PeriglacialSteppe
> | SeasonalSnowfield) and is exactly the hare's range. Nothing is lost but the occasional alpine
> warren, and no gameplay gap opens — `montane_highland` already has `crag_goat` at a `pen` ceiling.
> **The next cold-climate species pointed at `montane_highland` will hit this same trap.** Guarded
> by `fauna_wet_biome_roster::snow_hares_never_warren_the_highlands`, which asserts on the *spawned
> tile's module* — a count floor would not catch it, since re-adding the host raises the count.
- **`boar` gained `riverine_delta`**, giving the delta a big-game row beside the migratory marsh
  grazer and the small fowl/catfish.

Measured over `SWEEP_SEEDS` on the standard 80×52 earthlike map
(`core_sim/tests/fauna_wet_biome_roster.rs`, the guard against the silent never-spawn of an
unmatched `host_biomes` key): **20 catfish colonies** (1–6 per map, all water-adjacent), **37
snow-hare warrens** (4–8), **54 boar groups on delta tiles** (5–15) — each **0** before the
change. **The map-wide game cap is saturated** (125 herds per map against
`abundance.max_total_game` 120 + the 5 `abundance.migratory` slots — 122 when measured, at the
2-migratory budget issue #290 retired; identical pre- and post-change either way), so these three are
**displacing** other short-range game rather than adding to it — the roster shifts composition,
never density. Raise `max_total_game` if the intent is more game, not different game.

**Spawning** (`spawn_initial_herds`, `fauna.rs`): two passes into one
`HerdRegistry`.
1. **Migratory** — a few long-route walkers (`determine_herd_count`,
   `build_migratory_route`), species drawn from the config's `migratory` rows. **`host_biomes` is
   LIVE here** (it was previously ignored): a herd's loiter **anchors** are placed on tiles suitable
   for its species (`module_at ∈ host_biomes`), drawn from a regional home range
   (`MIGRATORY_HOME_RANGE_RADIUS`, spaced by `MIGRATORY_ANCHOR_MIN_SPACING`) around a random suitable
   seed tile and ordered into a walkable circuit by nearest-neighbour chaining — so the migration
   legs cross the less-suitable ground *between* the patches and the herd lives in its biome range
   across the map rather than clustered at the player start. A species whose host biomes the map
   lacks (empty `suitable`) **falls back to the start-anchored spiral** (`build_route`), so it still
   spawns somewhere.

> #### The migratory herd budget is CONFIG, and it was a two-slot lottery (issue #290)
>
> **The per-map migratory count is `abundance.migratory`** — `tiles_per_herd` (**800**) sets the
> density, `min_herds` (**2**) / `max_herds` (**12**) clamp it, and
> `MigratoryAbundanceConfig::herds_for_map` is the one seam that reads them. The retired
> `fauna::determine_herd_count` held those three numbers as **bare literals** (`area / 3000`, clamped
> `[2, 6]`), which is how the single number deciding whether a migratory species appears on a map
> became untunable — and, worse, *inert*.
>
> **Measured over 120 generated earthlike maps** at the shipped 80×52 grid, each built through the
> **real Startup chain** — worldgen → hydrology → tag budget solver → biome palette clamp → coastal
> shelf reconcile → `spawn_initial_herds`, via `build_headless_app()` + `run_schedule(Startup)`
> (`core_sim/tests/fauna_migratory_representation.rs`, a report-only harness — run it with `--ignored
> --nocapture`; it asserts no bound deliberately, because the point is the number and a floor on a
> probabilistic draw is the sitting-on-its-own-floor trap the seal guard already paid for). It reports
> **both** budgets over the same seeds on the same pipeline, so the before/after below is a controlled
> comparison rather than two readings taken under different conditions:
>
> | species | maps carrying it, `3000`/2 slots | **after, `800`/5 slots** | mean host tiles |
> |---|---|---|---|
> | Thunder Mammoths | 32% | **69%** | 620 (min 252) |
> | Marsh Grazers | 38% | **68%** | 410 (min 271) |
> | Wild Reindeer | 32% | **64%** | 620 (min 252) |
> | Wild Horses | 38% | **70%** | 350 (min 117) |
> | Steppe Runners | 41% | **71%** | 262 (min 116) |
>
> **The slot count was the whole cause — the roster draw was never biased.** An 80×52 map is area
> 4160, so `4160/3000` computed **1** and was **clamp-floored to 2**: the density lever was inert at
> the shipped size and `min_herds` silently decided everything. Two slots drawn uniformly **with
> replacement** from five rows predicts `1 − (4/5)² = 36%` presence per species; the sweep measured
> 32–41% (mean 36.2%), and χ² against a uniform draw is **1.8 (df 4)** against the 13.3 a biased pick
> would need — 2.7 after the fix. A doubled draw (~20% of maps at 2 slots) spent the second slot on a
> species the map already had.
>
> **So "mammoth & marsh-grazer are under-represented" was measured FALSE — but read the reason
> carefully, because the obvious phrasing is itself a noise artifact.** The five species were
> **statistically indistinguishable**: χ² = 1.8 on df 4 means the 32–41% spread is pure draw variance
> and the per-species *ordering within it carries no signal at all*. The premise is false not because
> the mammoth was well-represented — at 32% it was joint-lowest — but because **there was no
> per-species effect to find**. The shortage was roster-wide, and the only honest per-species statement
> is "all five, at the ~36% the slot count predicts".
>
> > **This block's own first draft got that wrong**, and the error is instructive enough to keep: it
> > read the then-measured 39%/37% and called mammoth and marsh grazer "the two best-represented"
> > species — ranking five samples whose χ² says they are one distribution. Re-measured on the correct
> > pipeline the ranking simply *reshuffled* (mammoth 39% → 32%, joint-last; Steppe Runners 32% → 41%,
> > first) while the aggregate barely moved. **Do not rank per-species presence off a 120-map sweep.**
>
> **Presence is NOT linear in the slot count** — with `n` rows it is `1 − ((n−1)/n)^herds`, so at 5
> rows: 2 → 36%, 3 → 49%, **5 → 67%**, 8 → 83%, 12 → 93%. Each extra slot buys less. `tiles_per_herd:
> 800` puts the standard map at exactly **5** — one slot per migratory row, so each row's *expected*
> count is 1 — and it is chosen so the **density**, not a clamp, is the authority at the shipped size
> (pinned by `the_migratory_budget_clamps_at_both_ends_but_scales_between_them`, which asserts 80×52
> lands *strictly inside* the clamps; that assertion is the regression guard against the inert-density
> failure recurring).
>
> **Raising it raises migratory BIOMASS steeply** — a migratory herd carries 4,000–12,000 biomass
> against a deer's 600–1,200 — so it is a **food-economy dial, not a variety dial**. Playtest dial.
>
> **Habitat was never the constraint** — mammoth/reindeer average 620 host tiles (never below 252) and
> the marsh grazer 410 (never below 271), against 2 slots. Raising presence was a *slot* question, not a
> `host_biomes` or abundance-table one, and `abundance.max_total_game` (120) is a **separate,
> already-saturated** budget that migratory herds do not draw from (so the map total moved 122 → 125,
> not 120).
>
> > **The habitat column is a POST-SOLVER quantity and must be measured as one.** The first version of
> > this table was read off a hand-rolled harness that ran `spawn_initial_world` and `spawn_initial_herds`
> > and *skipped* `generate_hydrology` → `apply_tag_budget_solver` → `apply_biome_palette_clamp` →
> > `reconcile_coastal_shelf`. Those passes rewrite exactly these modules — `earthlike` locks `Fertile`
> > (0.22) and `Wetland` (0.06), so the solver repaints into `riverine_delta`/`wetland_swamp`, and
> > hydrology stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh`/`NavigableRiver` into the same two — so
> > the pre-solver reading **understated the marsh grazer's habitat by ~50%** (276/min 124 → 410/min 271)
> > and **overstated the mammoth's by 25%** (775/min 314 → 620/min 252). Caught in review, not by the
> > harness. The sibling sweeps (`fauna_coastal_habitat.rs`, `fauna_wet_biome_roster.rs`) were already
> > doing it correctly via `build_headless_app()` + `run_schedule(Startup)`, and
> > `.claude/rules/core_sim/worldgen.md` already recorded this exact defect faking a whole finding once
> > (`forage_field.rs`'s "sowable tiles 46 → 0"). **Any fauna sweep must run the shipped schedule.**
>
> **A failed route no longer eats a slot.** `spawn_migratory_herds` used to `continue` the
> `0..herd_target` loop when `build_migratory_route` returned `None`, consuming the slot — measured: 1
> map in 120 spawned only **one** migratory herd (re-measured on the correct pipeline by pinning
> `MIGRATORY_ROUTE_ATTEMPTS_PER_HERD` to 1, which reproduces the old behaviour exactly: 239/240 herds at
> the 2-slot budget, 599/600 at the 5-slot one — so the frequency is real terrain, not the harness
> defect above). The loop now counts **seated** herds and re-draws on failure, bounded by that same
> constant (8) so a map that can seat none still terminates. The budget names how many herds a map
> should *hold*, and now it does: the post-fix sweep reads min 5 / max 5 across all 120 maps. An
> exhausted budget **logs** `fauna.migratory.under_filled` rather than passing silently — the tag
> solver's `under_filled_climate_gated` contract, and the readout whose absence let the old slot-eating
> `continue` cost a herd per ~120 maps unnoticed.
>
> **Beware the small sweep here.** At 24 maps this same harness read Steppe Runners at **1 herd / 4%
> of maps** — a 2σ small-sample artifact that reads exactly like a broken `host_biomes` key, and it
> would have been filed as one. A per-map *draw* needs ~100 maps before per-species counts mean
> anything; the six-seed sweeps the terrain guards use are sized for near-deterministic **tile**
> counts, not for this.
2. **Short-range game** — iterate land tiles, classify each via
   `classify_food_module`, roll `abundance.per_biome[module]`; the map-wide winners
   are shuffled then greedily placed respecting `min_spacing` up to `max_total_game`
   (bounded entity count, spread across the map rather than clustered by scan
   order). Route via `build_short_route` (`route_len == 1` → single stationary
   tile → no client trail).
   **The species candidate list is SITE-FILTERED before the pick** (`spawn_game_group_at`): a
   candidate carrying a site rule — today only **`adjacent_water`** — is dropped unless the winner
   tile satisfies **its own** kind (the neighbour scan runs once per tile and yields
   `(has_salt, has_fresh)`, against which each candidate is tested separately), and only then is the
   single `rng.gen_range` draw made (the draw count is
   unchanged; only its bound moves). **An empty filtered list spawns nothing on that tile** — a cold
   *inland* tile whose only candidate is a marine forager correctly stays empty rather than seating a
   seal on the tundra. The immigration path (`repopulate_fauna`) shares the helper, so a respawn
   obeys the same rule. **The migratory pass does NOT apply site rules** — which is why
   `migratory + adjacent_water` is validate-rejected rather than silently ignored.

**Movement — graze-wander + loiter-then-migrate** (`advance_herds`, `docs/plan_wildlife_hunting_overlay.md`
"Herd Movement"). A `Herd` carries a **live `current_pos`** (walked ≤1 hex/turn, land-clamped,
wrap-aware — `position()` returns it) over its sparse `route` (now **anchors**, not a per-turn path),
plus a `RoamState` + `dwell_remaining`. One primitive — **graze-wander** (dwell `dwell_turns`, then
step ≤1 hex) — split by `size_class`:
- **Wild game** (`Big`/`Small`): permanent `GrazeWander` toward the current cluster anchor (cycling);
  ≈ half speed (a `route_len==1` group stays put). Catchable by an equal-speed party during a graze
  turn.
- **Migratory**: a `Loiter { turns_left }` ↔ `Migrate` state machine over the anchors. **Loiter** —
  graze-wander within `loiter_radius` of the current anchor for `loiter_turns` (sampled). **Migrate** —
  1 hex/turn toward the next anchor, **no dwell**, then loiter at the new anchor. Fixes the old bug
  where `Herd::advance()` teleported 4–12 tiles/turn along the sparse route.

**Herd movement is a rung primitive** (intensification ladder slice 3b — the **first** behavior
primitive the engine reads). `advance_herds` resolves the herd's rung (`fauna::herd_rung`: penned →
`animal:pen`, tamed → `animal:pastoral`, else `animal:wild`) and dispatches on its
`behavior.movement`, so §3's proximity spine **far → near → fixed** is *config*, not a branch on
`is_domesticated()`. The one exception is **diet-resolved, not rung-assigned**: `fauna::movement_primitive`
overlays `pursue` on a **wild carnivore** (the husbandry rungs are diet-orthogonal — `animal:wild` is
one rung shared by a deer and a wolf — so a carnivore's food-seeking movement can't be a rung-record
field today; a future tamed wolf→dog would keep its rung's `drift_to_owner`).
- **`roam`** (wild herbivore) — the graze-wander / loiter-migrate machine below, over its own full range.
- **`drift_to_owner`** (pastoral) — each turn the herd first tries **one step toward the nearest band
  of its owning faction** (`ResidentBand` only — a camp, not a passing expedition party). It
  **composes with, never replaces, the 2b-i graze-aware roam**: the candidates are exactly the roam's
  own acceptable steps (`acceptable_steps` — land, and not barren), ordered by **(hex distance to the
  nearest camp ASC, graze capacity DESC, y ASC, x ASC)** — a *preference ordering*, so there is **no
  drift-strength lever** to tune. Only a step that genuinely closes the distance counts as a drift;
  once the herd is at the camp or hemmed in, the turn **falls through to the normal roam**, so a tamed
  herd grazes *around* its people instead of freezing on their tile. The species' own `dwell_turns`
  cadence still applies (taming makes an animal *near*, not fast), and the herd never crosses barren
  ground to reach its owner. An unowned herd, or an owner with **no bands**, roams normally. The last
  two sort keys are load-bearing: two candidates can tie on distance *and* capacity, and a tie broken
  by anything incidental is the ~20% flake `GrazeRegistry::richest_patch` already cost us.
  - *Emergent tension, deliberately unsolved (playtest):* a herd that prefers proximity will settle for
    adequate-but-poorer pasture near camp, which lowers its range-derived `K` and shrinks it — real
    pastoral overgrazing. It cannot **strip** the range: 2b-ii's `overgraze_escapement_fraction` floor
    still binds, so the pasture recovers and the herd stabilizes smaller.
- **`pursue`** (wild carnivore — Predators Phase 2, `docs/plan_predators.md`) — the **trophic transpose
  of `drift_to_owner`**: each turn the pack first tries **one step toward the nearest prey it can eat**,
  over the *same* shared attractor path (the `Pursue` dispatch is just `advance_herd_roam` handed prey
  tiles as its `attractor`, and `relocate_toward_resource` — the generalized drift — does the greedy
  one-hex step). **Prey targets are the clearable prey in `pursuit_radius`, sourced from `prey_index` +
  `attack_clears_defense`, NOT `HerdDensityMap`.** `HerdDensityMap` counts *every* herd (uneatable
  mammoths, other predators), so reading it would introduce a **second, divergent prey definition** —
  exactly the duplication Phase 1a eliminated by making `attack_clears_defense` the ONE prey rule shared
  by carnivore-`K`, predation and the spawn count; `pursue` reuses it, so a wolf chases only prey it can
  actually eat. `prey_index` is the **start-of-turn** snapshot (built before the mutable loop), so
  pursue targets **start-of-turn prey positions** — the same one-turn lag carnivore-`K` reads (a
  herbivore processed later this turn hasn't moved yet from the index's view; consistent and
  deterministic). It **composes with, never replaces, the roam**: same `acceptable_steps`, same
  `resource_step_order` total tie-break `(target distance ASC, graze capacity DESC, y ASC, x ASC)` the
  drift uses, and only a step that closes the distance counts — else the turn falls through to the
  graze-roam (a prey-starved pack keeps moving and re-acquires; a carnivore-free-of-prey map is
  byte-identical to today's roam). The species' own `dwell_turns` cadence still applies (**a wolf is not
  faster than prey** — pursue makes it *near*, not fast). **`pursuit_radius` (default 8) is deliberately
  WIDER than the feeding `prey_sense_radius` (shipped 4, code default 3)** — a pack tracks prey over a larger territory than the
  disk it feeds from, and the wider acquisition range is the real fix for the transient-zero-prey
  stranding that widening `prey_sense_radius` 3→4 only band-aided. See Also "Predation (Phase 1a)" /
  "Predator raids (Phase 1b)" in `combat.md`.
- **`fixed`** (pen) — pinned at `corralled_at`, no roam, no heading arrow.

Movement is **deterministic under rollback** — a per-herd/​per-turn `SmallRng` seeded from `map_seed ^
tick ^ HERD_MOVEMENT_SEED_SALT ^ fnv(herd.id)` (mirrors `repopulate_fauna`). Cadence levers are
per-species on `SpeciesDef` (`fauna_config.json`): `dwell_turns` (~1), `loiter_turns [min,max]`
(migratory, e.g. [12,24]), `loiter_radius` (~2), all `#[serde(default)]`. `advance_herds` resolves a
herd's levers via `FaunaConfig::species_by_display`. Movement is **independent of** `regrow_biomass`
(a loitering herd still grazes/regrows — ecology unchanged). Telemetry `next_position` is the next
`Migrate` hex (client heading arrow), `None` while loitering/grazing.

Abundance is a **tuning value, high to start** (design: game plentiful early,
thins under overhunting in later phases). Herds
flow to telemetry, the `HerdDensityMap`, and the snapshot (`HerdTelemetryState`,
which now also carries `size_class` + `huntable` so the client can offer the right
verbs — a free-form `species` string means new species need no schema change).

> ### Herd display telemetry is FOG-FILTERED; the herd registry is not
>
> **`WorldSnapshot.herds` publishes a herd only if the viewer faction's tile is `Active` for it this
> turn, or the viewer owns it** (`snapshot/subsistence.rs`, `HerdSnapshotInputs::herd_is_visible`).
> Before this the list went out unfiltered, so the client received the position, species, biomass,
> heading and full hunt-estimate table of every animal on the map — *wire-level fog was decorative
> for fauna*, and no client-side masking could fix a leak that had already crossed the socket.
>
> - **`Active`, not `Discovered`.** Ground you saw two hundred turns ago says nothing about where a
>   herd stands today, so `Discovered` would leak live positions across the whole explored map.
>   Remembering the **last seen** herd is a separate, deliberate feature (issue #214) layered on top
>   of this — never a weaker filter.
> - **Ownership is not a leak.** A tamed or penned herd is your property. Without that clause a
>   pastoral herd drifting a hex out of sight would take its `corralProgress` /
>   `penFedFraction` starving warning with it, and a pen alert that vanishes because of fog is a bug.
> - **The heading arrow is filtered separately** — `next_x`/`next_y` name a *second* tile, so a herd
>   visible at the edge of your sight would otherwise hand you a free look at where it is walking.
>   Withheld as the existing `-1` "no heading" sentinel.
> - **It fails CLOSED.** With no faction map for the viewer (before the first `calculate_visibility`,
>   or the turn after a rollback clears the ledger) **every** herd is hidden — which is exactly what
>   `visibility_raster_from_ledger` does in the same state (an all-unexplored, black raster). The two
>   read the same ledger for the same faction, so they cannot disagree about a herd on dark ground.
> - **Only the VIEW is filtered.** The authoritative record is the `HerdRegistry` itself, which the
>   **checkpoint** carries (`SimState::herds`) and which holds every live herd; restore rebuilds
>   `HerdTelemetry` from it (never from `snapshot.herds`), so rollback is untouched. **Consequence:**
>   an `export_map` JSON's `snapshot.herds` is the *player's view* and there is no unfiltered roster
>   beside it — the snapshot's `herd_registry` copy was deleted once the checkpoint arc left it
>   without a reader (`checkpoints.md` → "The client view carries no save state at all any more").
> - **A hunted herd stays visible for free** — `calculate_visibility` reveals `worked_source_sight_range`
>   around each worked Hunt herd's tile, so a herd your band is working is always `Active`.
> - **Known gap:** a hunting **expedition**'s target herd is *not* revealed (an expedition is
>   `Without<Expedition>`-excluded from live faction reveal; its discoveries are comm-range gated), so
>   a distant target is not published. The in-flight readout is unaffected — `expeditionEtaTurns` /
>   `expeditionProjectedDelivery` ride the *cohort*, not the herd.
> - **Per-faction snapshots are still a future arc.** The capture has ONE `ViewerFaction`, so this
>   closes the leak for the single-viewer stream the game ships today; true competitive MP needs a
>   per-faction capture.
> - **Turning fog OFF is the only way to reveal hidden fauna, and only the server can do it.**
>   `herd_is_visible` returns `true` unconditionally when `SimulationConfig::fog_enabled` is `false`
>   (set by the `set_fog <on|off>` command). This has to be server-side precisely *because* the filter
>   runs before encoding: a client render flag cannot restore an entity that never crossed the socket.
>   The visibility raster reads the same flag in the same capture, so the herd list and the shading
>   still agree by construction. Fog of war is now the ONLY fog concept — the "Fog of Knowledge"
>   overlay and the `FogRevealLedger` tracking pulse that follow-herd used to grant are both deleted.
>   See `.claude/rules/core_sim/ecs-systems.md` → Visibility Systems for the switch's full contract.
>
> Guarded by `core_sim/src/snapshot/mod.rs` unit tests (unseen / owned-in-the-dark / empty-ledger /
> heading suppression) and `integration_tests/tests/fauna_fog.rs`, which asserts on the **encoded
> FlatBuffers bytes** the client actually receives, decoded through the client's own accessor chain.

**Hunt (one-shot)** — the `hunt_fauna <faction> <herd_id> [band_id]`
command (`handle_hunt_fauna`, `server.rs`; full plumbing in `command.proto` /
`commands.rs` / `command_text.rs`) attaches a `FaunaPursuit` component (`components.rs`)
to a band (auto-picked when no band id is given). Each turn `advance_fauna_pursuits`
(`systems.rs`, `TurnStage::Population`) re-reads the herd's **live** position (herds
already moved in the earlier `Logistics` stage), steps the band up to
`hunt.pursuit_tiles_per_turn` toward it, and on closing to `hunt.pursuit_radius`
(=1, Chebyshev) resolves a one-shot take: `hunt.take_from(biomass)` biomass →
provisions (`hunt.provisions_per_biomass`), drawn from the group and added to
`FactionInventory`, then removes the component. An elusive herd is abandoned after
`hunt.max_pursuit_turns`. Config lives in the `hunt` block of `fauna_config.json`.

**Follow (`follow_herd`) is a RETIRED command** — the source-centric `assign_labor` replaced it, and
the server ignores it if a stale client still sends one. Its proto payload survives (a shipped field
number is immutable) carrying a free-form `policy` string that nothing parses. The
old one-shot teleport follow (and its `apply_herd_rewards`/`apply_herd_knowledge` helpers) is
retired, as is the tracking pulse it used to grant: that fed the `FogRevealLedger`, which was
deleted along with the Fog-of-Knowledge `fogRaster` overlay it existed to feed (fog of war is
`visibilityRaster` and is unaffected — see `docs/plan_exploration_and_sites.md`). The
`follow.reveal_radius` / `reveal_duration_turns` / `morale_gain` keys in `fauna_config.json` are
**dead levers with no reader** — they predate that deletion and are pending removal.

> #### The hunt axis is ONE NUMBER: the floor (`docs/plan_harvest_floor.md`)
>
> `fauna::hunt_escapement_ceiling` is the one source, and it is one expression parameterised by a
> **floor**: `escapement_ceiling(floor, B, K)` — `max(0, B − floor·K)`, and **nothing else**. The herd
> hands over the stock standing above the floor; the crew's throughput is the only other term.
>
> > **⛔ THE CEILING IS NO LONGER THE WHOLE TAKE — a GROWTH SHARE sits under it** (`§4.11`). The take is
> > `max(the room above the floor, growth × (1 − floor))`, and **the build's eligibility gate reads the
> > same expression** so a legal build target that yields nothing is unrepresentable. `escapement_ceiling`
> > itself is untouched — the backstop is a `max` *around* it, on both webs.
> >
> > **Why it exists.** The floor is `floor · K` and a rung RAISES `K`, so a rung raises the floor while
> > the herd stays the same size. Measured on aurochs begun exactly on its floor, the room reached zero
> > at turn 6 with one herder, 3 with four and 2 with eight — *building faster starved you sooner* — and
> > because the gate read that same room, **the tame then never completed at any crew size**. Five of the
> > eleven tameable species are on the losing side of that race.
> >
> > **`(1 − floor)` is the scaling and there is deliberately NO new dial.** The player's own floor governs
> > it: *you keep the share of the growth you were willing to take.* At `floor = 1.0` it pays **nothing**,
> > so *leave the whole herd standing* keeps meaning exactly that — at the take **and** at the gate, with
> > no special case. A flat share would have made a full floor cull every turn.
> >
> > **⛔ A FORECAST REGROWS FIRST, BECAUSE THE TAKE IT PRICES RUNS AFTER LOGISTICS.**
> > `fauna::hunt_crew_take_curve` resolves against `fauna::next_turns_quarry` — a private clone with
> > one `regrow_biomass` applied — and **not** the herd as the registry holds it. Every caller reads
> > it after the Population take (the query answers a client between turns; the capture publishes
> > `hunt_useful_crew` in the Snapshot stage), so the raw herd is a whole turn stale, and on a
> > **worked** source that staleness is the entire take rather than a rounding:
> >
> > - `escapement_ceiling` reads `biomass`, which the take has just drawn back toward the floor, so
> >   the room left standing is approximately nothing; and
> > - the growth-share backstop reads `Herd::growth_this_turn`, which is
> >   `biomass − biomass_before_regrowth` — and the take is subtracted from `biomass` **after**
> >   `regrow_biomass` stamps the pair. On a source harvested at or above its growth that field is
> >   **`0`**, so *the backstop that exists to pay a source sitting at its floor is switched off by
> >   exactly the harvesting that puts it there.*
> >
> > Measured in play on a Rabbit Warren (`K 10`, floor `0.5`, one trapper): the row published
> > `actualYield 0.0216` — four rabbits — with a positive `arrivalSchedule` in **all twenty slots**,
> > while the curve read **zero at every crew size**, the compose sheet said *"these hunters bring
> > down ≈0 Rabbit Warren/turn"*, and `huntUsefulWorkers` published `0` for a row that was feeding the
> > band. The stock the take saw was `5.914`; the stock the curve read was `5.039`.
> >
> > **`project_realized_hunt` was right about that herd throughout, and structurally so** — its loop
> > is `regrow` → read the room → take, every turn — which is why the Work board's `/turn` and the
> > compose sheet's headline disagreed by 8× rather than by a rounding. `next_turns_quarry` is that
> > loop's first step, named, so *"a forecast regrows first"* is one expression rather than a rule
> > each forecast path remembers.
> >
> > **⛔ THE RULE IS EVERY FORECAST PATH ON BOTH WEBS, NOT THE CREW CURVE ALONE.** `hunt_forecast` and
> > `forage_forecast` resolve **both** stock terms forward — the escapement arm as well as the growth
> > arm — off `fauna::next_turns_quarry` and its plant twin `forage::next_turns_stand`. Threading only
> > the *growth* into `SourceYieldForecast` was tried first and is **not enough**: it leaves the
> > escapement arm a turn stale, so on a herd sitting *at* its floor the published row and the take
> > still disagree, which is the same defect one term smaller. **A forecast either prices the whole
> > next turn or it prices none of it.**
> >
> > **The consequence for harnesses is the part that bites.** A fixture that freezes a stock and reads
> > a forecast is now quoting a turn the sim has not run, so it must either resolve a turn **in stage
> > order** (Logistics → Population) or quote the forecast **before** the regrowth. Six did neither
> > and had to be corrected. Two shapes stopped being available with it: an exact bit-for-bit equality
> > between a seeded and a resolved realized yield (its old pass came from a frozen herd taking
> > nothing — it is bounded now, `REALIZED_NO_JUMP_FRACTION`), and *"a stripped patch is barren"* — a
> > stripped patch **reseeds and pays next turn**, so a barren fixture has to state barren **ground**,
> > meaning zero capacity.
> >
> > **"Regrown" is not "larger".** Below the Allee threshold `regrow_biomass` takes the depensation
> > branch and the clone comes back *smaller*, which is the honest forecast for a collapsing herd. A
> > guard asserting the clone never shrinks looks obviously true and fires on the first thin-herd
> > fixture it meets.
> >
> > **The client's room arm was already forward** (`SourceForecast.escapement_room_next_turn`), so
> > before this the two halves of the sheet's own `min(room, haul, brought_down)` sat a turn apart and
> > the stale one won. Both halves are next-turn now; `forecast_query`'s
> > `the_curve_reproduces_the_take_on_a_herd_held_at_its_floor` is the guard, and its precondition —
> > *the standing room affords zero whole animals* — is what makes it a test of the frame rather than
> > of a number.
> >
> > **⛔ AND THE CURVE IS A RATE ALL THE WAY DOWN — `EngagementQuantum::Rate`, ITS ONLY CALLER.**
> > Regrowing fixed *which turn* the room is measured in; it does not stop the room being **rounded to
> > whole animals**, and on heavy-bodied quarry the rounding is the whole reading. `animals_affordable`
> > floors `room ÷ body_mass`, which is right for a take (bodies hit the ground) and wrong for a rate,
> > for the reason `SpeciesDef::body_mass`'s config note already states: *"when the herd cannot yet
> > spare a whole animal the hunt PAUSES and the herd regrows; that wait is constant escapement,
> > discretised, and the herd's own biomass is the accumulator (there is no credit meter)."* Flooring a
> > rate against that quantum reports **a cadence as a never**.
> >
> > It is the same correction `HuntFight::expected_brought_down` already makes one stage later, in the
> > same words — the fight arm was floored too, and published a curve of zeroes for crews genuinely
> > taking `0.75` a turn. The **room** arm simply never got the treatment.
> >
> > Reported from play on a **Wild Aurochs** (`body_mass 120`, wild `r 0.09`) standing on its 50% floor
> > at 1200 of 2400 biomass: one turn's growth is `54` biomass — **0.45 of one body** — which floors to
> > **zero animals**, so all 24 rows read `0`, the sheet said the hunters bring down nothing, and the
> > stepper offered **no crew to assign at all**. The herd pays one aurochs about every two and a half
> > turns.
> >
> > **BOTH FLOORS HAD TO GO, and that is the trap.** Un-flooring the room alone changes nothing:
> > `animals_that_stay` opens with `let stayers = engaged.floor()`, so a `0.45` engagement is handed to
> > the binomial as `0` one stage later and the curve is zero however the room was measured. Hence
> > `animals_sparable` **and** `HuntingParty::stayers_at_rate` — the binomial's `n·p` and
> > `√(n·p·(1−p))` are the continuous extension of the same distribution, so a fractional `n` is the
> > *same* reading rather than a new model. Both sabotages are pinned separately by
> > `hunt_useful_crew_on_the_wire::big_game_held_at_its_floor_publishes_a_rate_and_a_crew`.
> >
> > **EVERY TAKE PATH KEEPS ITS FLOOR** — `EngagementQuantum::WholeAnimals` is the default at all four
> > other call sites, `systems::hunt_take` included — which is what makes the change safe: a turn still
> > resolves in bodies, and only the reading documented as a rate is un-rounded.
> >
> > **⛔ AND THE PEN CURVE IS THE SAME CURVE.** `pen_crew_take_curve` published
> > `quantise_animal_take(…).killed as f32` — whole animals — while every stalking row beside it
> > published the un-floored rate, and the take path pays `killed = 0` whenever the room affords less
> > than one whole animal. So a **penned** aurochs whose next-turn room is 54 biomass read
> > `0.0` at every crew size, `hunt_useful_crew` fell to `NO_USEFUL_CREW`, and the Work board's `+`
> > shut on a pen collecting a beast every two and a half turns — the same cadence-as-a-never, one
> > function over. It publishes the rate now (`animals_sparable`, `ONE_WHOLE_ANIMAL`).
> >
> > **The fixture is why it survived the first pass.** The shipped pen fixture stood a **fat** herd up
> > and then mirrored the curve's own expression to predict it, so it agreed with the bug and could
> > not have failed. A curve that rounds is only visible on a source whose room is **smaller than one
> > body** — `a_thin_pen_publishes_a_rate_and_a_crew` is that fixture, and a rounding fixture needs a
> > thin subject the way a fog fixture needs a remembered hex.
> >
> > **A FROZEN-STOCK HARNESS CANNOT MEASURE THIS, and `forecast_query`'s reproduction sweep had to say
> > so.** `sim_take` holds the herd's biomass level between turns, which is also what discards the
> > remainder the accumulator lives in: a crew that may spare `3.9` bodies kills `3` and leaves `0.9`
> > standing, and a reset throws that `0.9` away every turn for ever. So the sweep compares the rate
> > against the frozen turn as a **bracket** whose slack is exactly the fraction the floor drops,
> > carried through the retreat — `0` on any fixture whose room divides evenly into bodies. The fight's
> > remainder needs no such allowance: `hunt_take` writes `herd.wounds` back and the harness keeps it,
> > so that quantum already integrates. **The room's was the one the harness dropped — the same
> > asymmetry the curve itself had.**
> >
> > **THE ESCAPEMENT PREDICATE WAS SPLIT, and the half that did NOT move is the point.** It fed two
> > seams. The **lesson** keeps the pure room, because `learn_multiplier`'s self-limit is load-bearing —
> > a floor just under `1.0` deliberately learns at nearly ×2 while taking almost nothing, and its doc
> > forbids clamping it. Widening that seam would have made a full floor free ×2 learning for ever. Only
> > the **build's** gate reads the backstop.
> **THE BUILD IS NOT IN IT AT ALL** (`docs/plan_standing_upkeep.md` §2.2): a build has its own crew,
> so neither the ceiling nor the hunters' throughput carries a build term — `hunt_escapement_ceiling`
> takes no `improvement` and no ladder, and nothing beside it does either. (It carried none even while
> the dip was live, when the dip multiplied the CREW; writing it here as `… × build_dip` — as this
> file did — handed a reader a double discount.) **There is no stance axis** —
> `FollowPolicy` is deleted, and the floor rides `LaborTarget::Hunt` (a resident band) or
> `ExpeditionMission::Hunt` (a raid) as an `f32` fraction of `K`.
>
> | floor | herd |
> |---|---|
> | `1.0` | nothing taken — deliberate under-harvest, which the retired axis could not express |
> | `0.50` (`MSY_BIOMASS_FRACTION`, the default) | settles ON `K/2`, the most productive biomass |
> | `0.30` | drawn down, still above the Allee brink |
> | `0.15` (`ecology.collapse_fraction`) | pinned AT the brink, Collapsing |
> | `0` | nothing standing — under `extinction_floor`, and gone |
> | a build in flight | **the same room** — the build's hands are staffed separately and are not hunting, so nothing about the herd's offer changes |
>
> **Validated `0.0..=1.0` at the command boundary and never clamped** (`components::floor_is_valid`);
> an absent floor becomes `DEFAULT_ESCAPEMENT_FLOOR`. The four values above are the ones the retired
> stances named — they are landmarks on a dial, not a menu.
>
> **`r`-INDEPENDENT, and structurally so.** `hunt_escapement_ceiling` takes no `EcologyConfig` and no
> `FaunaConfig`: how fast a herd breeds cannot reach the take at all. That is the property the retired
> multiples-of-MSY axis lacked, and it is why "where do I stop" is no longer a question about the
> growth curve. `sustainable_yield` survives for **telemetry only** — the `SourceYield.sustainable`
> reference line and the `pastoral_yield`/`managed_yield` rung payoffs — and **no take path may call
> it**.
>
> **Consequences worth stating, because they surprise:**
> - **The first harvest of an untouched source is its accumulated stock, not a rate**, so `actual`
>   legitimately exceeds `sustainable` at *every* floor, the peak included. The overdraw ⚠ is
>   `components::take_overdraws` — a floor below the food peak (`floor < K/2`, where you stop)
>   **and** a party that can actually draw the herd down to it (`fauna::hunt_take_overdraws`) — never
>   `actual > sustainable`. See `.claude/rules/core_sim/yield-forecast.md` → "THE ⚠ IS INTENT **AND**
>   ABILITY" for why the second conjunct is a question about throughput rather than about this turn's
>   take.
> - **A single turn cannot see the husbandry ladder.** At `B = K` the ceiling at the peak is `K/2` on
>   every rung, because `r` cancels. See the callout in `husbandry.md`, which has said this about the
>   pen since the pen was constant escapement; it is now true of the wild hunt too.
> - **Extinction is the floor-`0` case, and ONLY that.** A floor at the Allee brink leaves a
>   Collapsing remnant; only floor `0` ends a herd
>   (`fauna_deplete::deplete_pins_a_herd_at_the_brink_while_eradicate_ends_it`).
>
> **The retired `Market` naming, and its markup, are both gone.** The third extractive rung was once
> called `Market` because it produced trade goods *instead of* food; #337 made both accounts live on
> every harvest, the rung was renamed `Deplete` for the pressure it applied, and the harvest floor
> replaced the stance with the number it stood for. Its last vestige — `forage.market`'s **4×
> `trade_goods_multiplier`** — went with it: a factor attached to one drawdown depth re-welded product
> to intensity. **After the harvest floor, no option carries a factor of any kind** (plan §4). A deeper
> floor still out-earns a shallower one, because it *takes more biomass*. **The trade axis itself is
> retired** (arc #527): what a take pays beyond food is its species' `materials` rows, credited off
> the same carried biomass.
>
> **Monotone in take BY CONSTRUCTION — in the FLOOR.** A deeper floor leaves less standing, so it
> takes more, at every biomass and for every species. It needs no config invariant to hold, which is
> why `hunt.{surplus_multiplier, deplete_multiplier, surplus_escapement_fraction}` and their validator
> bounds are deleted: they existed to keep a *multiplier ladder* ordered, and there is no ladder.
> `fauna_deplete::hunt_policy_takes_are_strictly_ordered_at_every_biomass` sweeps the resulting takes
> across B × {fast, slow}, and `forage::stance_probe`'s property tests sweep the whole dial on both
> webs. **The regression guard against reintroducing a rate** — do not weaken it.
>
> **The kill-credit bank has LEFT the resident take path** (`Herd::hunt_credit`). Under escapement the
> ceiling is a *stock*, and banking a stock compounds it — the herd would hand over its whole surplus
> every turn plus everything it had already handed over. The accumulator the bank provided is now the
> herd's **own standing biomass**: a mammoth held at floor `0.5` regrows ~120/turn against an 800 body,
> so the room crosses one body after ~7 turns and `quantise_animal_take` pays exactly the
> wait-then-one pulse the bank used to produce. Same cadence, one fewer piece of state. **The
> expedition keeps its own use of the field** — `expedition_take_biomass` banks the *party's*
> processing throughput to meter when the next whole animal is ready, a different quantity.
>
> **The take reads the CURRENT biomass, not `biomass_before_regrowth`.** That pre-regrowth basis
> existed because a constant *catch* evaluated after Logistics regrowth takes more than the stock grew,
> leaking a below-`K/2` herd down. Constant escapement has no such leak — `B − floor·K` is the stock
> standing above the floor whenever it is measured — so a below-`K/2` herd holds or recovers by
> construction (`fauna_deplete::a_below_half_k_herd_under_sustain_recovers_never_declines`). The field
> survives for the `sustainable_yield` **projections** (`hunt_forecast`'s rung payoffs).
>
> **Shared herd (chosen handling, reported):** two hunters on one herd each take the standing surplus
> in turn, so the second finds less than the first — the stock itself is the shared resource, which is
> the correct and self-limiting reading. The intended invariant is still **one hunter per herd** (a
> resident band leashes to a nearby herd; expeditions target distant migratory ones).
>
> **THE AXIS IS PURE INTENSITY — the product comes from the SPECIES** (`docs/plan_hunt_yield_model.md`
> §3, issue #337). A rung decides *how much biomass* comes home; `SpeciesDef::hunt_yield` (resolved by
> `FaunaConfig::hunt_yield_for`) decides *what that biomass is worth*, and **every** rung is paid the
> same vector through `HuntYield::apply` — one call, both products, so no site can convert the meat
> and forget the pelt. Two consequences:
> - **`market.trade_goods_multiplier` is RETIRED** with its whole block, and so is the
>   **trade-goods axis** it multiplied (arc #527). A 4× bonus on the third rung alone re-welded
>   product to policy; the rate it scaled then turned out to be written by every take site and read
>   by nobody, while the species' `materials` rows beside it named the same take's actual hide and
>   bone. What a rung decides is still *how much biomass*; what a species decides is now its food
>   rate **and what it is made of**.
> - **A floor-`0` take pays a WINDFALL**, and the retired `delivers_food` predicate is gone (not adjusted).
>   Its premise — *"denial carries nothing home"* — is what the arc reverses: denial is the END STATE
>   (the species is gone, for you and everyone else), not a promise the carcasses were thrown away.
>   Its readers now ask the **species** (`HuntYield::edible`); the *intensity* fact it smuggled — the
>   strip case has no escapement floor to spend, so no party-side stop ends its trip — is stated as
>   `floor <= STRIP_IT_BARE` at the completion in `systems::expeditions` (the live arm and the
>   projection alike), a number rather than a variant. **It never meant "ignores the pack's carry
>   cap"**: a floor-`0` party hauls its real pack like every other, and the spell where it did not is
>   why its waste read `0` — see `expeditions.md` → "Denial is a MISSION, not a floor".
>   `FollowPolicy::Eradicate` is deleted, so the old `matches!(policy, Eradicate)` spelling this file
>   carried would not compile.
>
> **THE ONE CLAUSE A DENIAL RAID DROPS.** `quantise_animal_take` takes a `fauna::EngagementStop`:
> a hunt bounds the kill by what its pack seats (`fauna::animals_the_pack_seats`) — *hunters do not
> kill what they cannot use* — and a **denial raid** does not, which is the single line separating
> the two missions. `carried` is the same expression under both, so a raid still banks what it can
> haul and the rest is `wasted`. The
> escapement floor is a *number* and the pack is a *bound*, so no value of the first reaches the
> second — which is why denial is a mission and not a floor preset. `fauna::herd_past_recovery` is
> its win condition (`collapse_fraction · K`, read through `classify_ecology_phase`). Rationale:
> `.claude/rules/core_sim/expeditions.md` → "Denial is a MISSION, not a floor".
>
> **THE TAKE IS FOUR STEPS, AND THE WHOLE-ANIMAL QUANTUM SITS ON EXACTLY ONE OF THEM.**
>
> ```text
> 1. engage    reach = workers × engage_rate, bounded by what the herd can spare above the floor
> 2. retreat   a fraction of what was reached gets away (wariness)
> 3. fight     whole animals dead; the unfinished remainder banks on Herd::wounds
> 4. carry     min(pack, killed) in BIOMASS, unrounded — the rest is wasted on the ground
> ```
>
> *An animal dies whole; meat divides.* The quantum belongs on the **kill** (step 3), because that is
> the step that produces bodies; a hunter field-dresses and takes what fits. Three roundings sat
> outside that step and each cost the player food:
>
> - **`animals_engaged` no longer floors and no longer has a `max(1)`.** `floor(w × engage_rate).max(1)`
>   answered **one animal for every crew from 1 to 6** on the shipped Wild Boar, so four hunters fed a
>   band exactly as well as one (`0.18 food/turn` either way, from play). The reach is a rate and is
>   now written as one. The retired floor's stated defence — that flooring to zero would put a
>   headcount threshold in front of the attack-vs-defense gate — does not survive a plain multiply:
>   nothing reaches zero for a party that exists (a lone mammoth hunter reaches `0.05`). **A crew below
>   `1 / engage_rate` therefore takes strictly less than it used to**, which is the authored
>   `engage_rate` finally meaning what its config comment always said.
> - **A sub-body reach carries between turns on `Herd::wounds`, not on a second bank.** With the reach
>   fractional the retreat keeps the part body in closed form (`animals_that_stay` draws the whole
>   bodies and multiplies the remainder by `1 − wariness`), and `combat::DamageLedger` banks the damage
>   struck at it — so a lone boar hunter finishes a body about every fourth turn instead of flooring to
>   zero for ever. `Herd::hunt_credit` stays the expedition's alone: fight progress is already
>   accumulated, and a second meter over the same wait would count it twice.
> - **The carry arm rounds UP** (`fauna::animals_the_pack_seats`, `ceil`, never below one body). It was
>   `floor(collection / body_mass)`, so a party able to carry `1.5` animals killed one and left half its
>   pack idle every turn. It now kills the animal at the top of the load, hauls what fits and wastes the
>   remainder — the general form of the `max(1)` arm that has always said *a party that cannot carry one
>   still takes one*. `carried` itself was never rounded and still is not.
>
> **THE ESCAPEMENT ROOM IS SPENT AT STEP 1 — BY EVERY CALLER — AND `quantise_animal_take` NO LONGER
> HOLDS IT AT ALL.** The quantiser's `affordable` arm, its `affordable < 1 ⇒ take nothing` early
> return and its `policy_ceiling` parameter are **gone**: the engagement is already clamped by
> `animals_affordable`, the fight can only bring down what stayed, and `DamageLedger::pending` is below
> one body by invariant, so the arm was dead on every hunting path. What kept it alive was **the pen**,
> which has no engagement stage — and that is exactly the argument for removing it. A bound applied to
> bodies already on the ground let the one caller with no bound of its own look correct: `systems::labor`'s
> tend branch handed its keepers' raw handling rate in as `brought_down` and the post-hoc clamp quietly
> covered for it. The pen now spends the room at its own call site (`fauna::animals_handled` —
> `husbandry.md` → "THE PEN BOUNDS ITS COLLECTION"), so a caller that forgets is a **stripped herd in a
> test**, not a silent save.
>
> **The wait-for-regrowth turn moved with the room, and is unchanged.** A source that cannot spare a
> whole animal hands the quantiser a `brought_down` of `0` — the same nothing the early return used to
> produce — so the hunt still pauses while the herd regrows.
>
> **A curve is no longer a staircase.** With the reach linear in the crew and the fight linear in it
> too, on a herd nobody can exhaust one of them is smaller at *every* crew size — the fight/engagement
> flip that used to appear mid-sweep was the rounding, not a fact about hunting. What still bends a
> crew-take curve is the **room**, which does not grow with the crew at all, and gear coverage
> re-resolved per crew size. `hunt_useful_crew` still reads the **last rise** for those.
>
> **Quantisation never divides by a food number it has not established is positive.** The old
> "flooring in provisions and in biomass agree, a positive linear factor cancels" note is **false** for
> an inedible species: `provisions_per_biomass == 0` makes `floor(food_ceiling / food_per_animal)` a
> `0/0`. Operationally, the animal count is a **ratio**, so it is taken on
> `SourceYieldForecast::ratio_axis()` — the first component with a positive rate (`Provisions` for
> every edible species, bit-identical to the pre-arc arithmetic; `TradeGoods` for a wolf) — and
> `YieldPair::rescaled_to` carries that one count into the other currency. Correspondingly **"does this
> source quantise?" is `!body_mass_yield.is_zero()`**, not `body_mass_yield.provisions > 0`: whole
> animals are a property of the animal, not of what it is worth to you.
>
> **The forecast is a `YieldPair` per rung, and `forecast == actual` holds PER COMPONENT** — see
> `.claude/rules/core_sim/yield-forecast.md` for the wire fields, the plant side's
> food-only forecast, and why `huntPerWorkerProvisions` must not clamp a per-herd preview.
>
> **The picker.** The flags gate the yield *components*, not the buttons: a wolf shows the full ladder
> and is paid in pelts, because each rung is a meaningful *rate* at which to collect pelts. The only
> pruning rule is `fauna::hunt_policies_for` — the ONE seam the `assign_labor` validator and the
> snapshot's `huntPolicyCeilings`/`huntTripEstimates` export share — and it prunes only a
> `yields_nothing` species (worth neither meat nor pelt) down to `Eradicate` alone. No shipped species
> hits that branch; it is pinned on a synthetic config
> (`core_sim/tests/hunt_yield_vector.rs`).
>
> **Retired levers** (all stay retired): `follow.surplus_multiplier`, `surplus.take_fraction`,
> `market.take_fraction`, `hunt.take_fraction` / `min_take` / `take_from`; `ecology.
> surplus_escapement_fraction` and `fauna::hunt_policy_floor` were deleted with the ordered-floors cut.
> `ecology.collapse_fraction` is once again **only** the Allee/depensation threshold (it briefly doubled
> as Deplete's floor). The whole **`market` block** joined them (above), and `fauna::hunt_provisions`
> — the single *global* biomass→provisions conversion — is retired in favour of `HuntYield::apply`.
> Config: the per-species `hunt_yield` vector, and **that is now the whole hunt surface**.
> `hunt.{surplus_multiplier, deplete_multiplier}` are retired with the rest — they are listed above as
> deleted, and naming them here as live config contradicted that four lines later.

> #### Herding is standing labor, and it scales with the HERD (slice 8)
>
> **It is a STANDING UPKEEP** (`docs/plan_standing_upkeep.md` §2.4): both managed rungs declare
> `upkeep: { work_per_turn: 1.0, scaled_by: source_load }` and the herd supplies the **keeper load**
> (`fauna::herd_keeper_loads` = `head count / animals_per_herder`), so the demand is
> `ceil((biomass / body_mass) / animals_per_herder)` keepers — the number this has always been, now
> quoted in the same work units as every other cost on the ladder. It is owed **every turn** by a
> pastoral or penned herd, **including wait turns** when it cannot spare an animal. *Just because you
> aren't killing an animal doesn't mean you aren't tending them, keeping them from running off,
> repairing fences.* Before this a pen of 2 and a pen of 200 needed the same single keeper; only the
> feed scaled. See "THE KEEPER DEMAND IS AN UPKEEP RATE" in `husbandry.md`.
>
> - **Downward hysteresis — staff it once and it holds.** The bare `ceil` is stateless, so a
>   Sustain-hunted slow herd sitting near an `animals_per_herder` multiple (a Wild Aurochs near 12
>   head) **flickers 1↔2 every turn** as the lumpy whole-animal kill breathes its biomass ±1 animal
>   across the boundary — and because the staffing reading lags a turn, the player is told
>   "staff all 1", then "staff all 2", satisfies neither, and slips the tameness. So the requirement is
>   now a **persisted, deadband-stabilized `Herd::herders_needed`** (rewound by rollback with the
>   cloned registry, like `ladder_position`), updated every turn by `Herd::stabilize_herders_needed` in
>   `advance_husbandry`: **up immediately** when the raw need rises (under-herding is harmful), **down
>   only once the herd falls below `(current − 1)·animals_per_herder − band`** where `band =
>   animals_per_herder × husbandry.herders_hysteresis_fraction` (**0.25**, `fauna_config.json`, a
>   playtest dial; `0` restores the raw flicker). A herd bumped to 2 stays at 2 through a one-animal dip
>   and drops only on a genuine multi-band fall — wild = 0 unchanged (a wild herd isn't yours to
>   maintain). `herd_herders_needed` reads this stabilized field (falling back to the raw ceil only for
>   a not-yet-stabilized managed herd — the turn it is tamed, or a test fixture), so **every** consumer
>   (the shed, the take-crew `max()`, the `herdersNeeded` snapshot field) is steady; the wire field is
>   unchanged, just no longer churning. It is **handed** the raw count (`fauna::raw_herders_needed` —
>   the rung's own `upkeep_crew_needed`) rather than recomputing a `ceil`, so there is exactly one
>   definition of what a herd wants.
> - **Heads, not tonnes.** The denominator is per-**animal** (`SpeciesDef::animals_per_herder`,
>   per-species: fowl/rabbit 200, crag_goat 80, boar 15, steppe_runner/marsh_grazer 15, aurochs 12;
>   deer/mammoth are `wild`-ceiling and omit it). A shepherd minds ~300 sheep, a cowherd ~80 cattle —
>   you watch individuals, and a heavier beast is not proportionally more work. A per-*biomass* dial
>   says "one herder per 100 fowl but one per 2 boar" and invents a 45-herder steppe megaherd that is a
>   pure artifact of the unit (4,560 biomass of Steppe Runner is **86 animals** ⇒ ~6 herders).
> - **ONE need, not three — but "one need" means one CREW, not one formula.** The herders mind the herd,
>   *reach* it and *butcher* it, so a managed rung's **take** reports one number and staffs one team —
>   but that team must be big enough for **all three** jobs, which scale on **three different units**.
>   (The **build**'s hands are a separate allocation the player states on the verb, and the
>   **keeping**'s are `upkeepWorkersNeeded`; the shared `intensification::source_crew_needed` that used
>   to fold a build crew in here is retired — see "`workers_needed` IS THE TAKE'S OWN COUNT" in
>   `yield-forecast.md`.)
>
>   | term | unit | rate |
>   |---|---|---|
>   | `herd_herders_needed` | **heads** minded | `animals_per_herder` (one herder minds 12 aurochs) |
>   | `fauna::hunt_engage_workers` | **animals brought down** | `engage_rate × stay` (one hunter reaches 10 fowl, 0.05 mammoths — and keeps only what stands) |
>   | `fauna::hunt_haul_workers` | **biomass** carried | `per_worker_biomass_capacity` (one hauler carries 40) |
>
>   A shepherd minds ~300 sheep and could not carry three. So
>   `workersNeeded = max(herders_needed, hunt_engage_workers, hunt_haul_workers)` — `+` would be three
>   teams; `max` is one crew covering its busiest job. The take side's two are bound together in
>   **`fauna::hunt_take_workers`**, the single seam both the resolved Hunt arm and the assign-time seed
>   (`forecast_source_yield`) size their take half with. Do not "simplify" the `max()` away.
>
>   **Which term binds, measured against the shipped roster:**
>   - **The engagement term dominates the haul term for every huntable species**, and that is an
>     *authoring* fact rather than a coincidence: `SpeciesDef::engage_rate` is authored against
>     `engage_rate × body_mass` — the most biomass one hunter can take per turn — with the mammoth's
>     **40** as the roster's top. The two crews are `peak/rate` and `peak × body / 40`, so reach binds
>     wherever `engage_rate × body_mass ≤ per_worker_biomass_capacity`, which is the whole roster, and
>     the mammoth's `0.05 × 800 = 40` is the one exact tie. Retune either dial past that and the
>     haul term takes over — which is why it stays in the `max()` rather than being folded away.
>     (The retreat only widens that dominance: it divides the reach crew by `stay ≤ 1` and leaves the
>     haul crew alone, so the roster-wide tie above is the *calmest* case.)
>   - **THE RETREAT PRICES THE CREW, not only the take.** `hunt_engage_workers`' divisor is what one
>     hunter puts on the ground — `engage_rate × stay` — and never the raw reach, because
>     **a party that keeps one animal in four needs four times the hands to draw the same stock
>     down**. `stay` is the party's OWN [`HuntingParty::stay_fraction`] (the quarry's `wariness` folded
>     with the kit's `dispersion`, `equipment.md`), the identical term the take beside it is priced
>     with — never the species' bare `1 − wariness` and never the neutral `1.0`, or a sheet can size a
>     crew at one dispersion beside a take priced at another. The one exemption is the `fight: None`
>     branch (`fauna::NO_RETREAT_STAGE_STAY`), where the source has no engagement stage to speak of and
>     the term is already `0`.
>
>     The reach-only reading was visible in play: on a Wild Boar herd the compose sheet's *clear it
>     now* target divided the room by the retreat-aware rate (**108 hunters**) while the stepper cap
>     beside it divided by the raw reach (**82**), so the sheet named a crew the panel refused to let
>     the player assign — and 82 hunters demonstrably leave the herd short. Pinned in whole numbers by
>     `fauna::tests::a_wary_boar_herd_needs_the_hands_the_retreat_costs` (body 12, `engage_rate` 0.33,
>     `wariness` 0.25 at the spear line's neutral dispersion ⇒ `0.2475` boar down per hunter, so a
>     28-animal drop costs **114** hands where the raw reach reads 85), and on the exported row by
>     `hunt_yield_vector::the_exported_crew_pays_for_the_retreat`.
>
>     `stay == 0` reports **`0`**, which is the answer rather than a sentinel: nothing the party
>     reaches ever stands, so the take is identically zero at every party size and no crew achieves
>     it. The haul term still speaks, so the `max()` does not collapse with it.
>   - **The herder term dominates at a SHALLOW draw.** It counts the whole herd's heads; the other two
>     count only the drop standing above the floor. A fowl herd worked at floor `0.9` owes its full
>     keeper crew while the drop it can pay is a fraction of it.
>   - **A PEN and the plant web have no engagement stage at all** — `engage_rate_for` /
>     `SourceYieldForecast::managed` answer `f32::INFINITY`, `hunt_engage_workers` returns `0` for it,
>     and the `max()` collapses to the two terms those sources always had. A penned animal is not
>     stalked — and, since the fight landed, not fought either (`SourceYieldForecast::fight` is `None`
>     there and on every plant source).
>   - **THE FIGHT IS A FOURTH BOUND, and it is not a crew term** (`docs/plan_hunt_through_combat.md`
>     §4, slice 4). `quantise_animal_take`'s fourth argument is no longer the raw engagement: it is
>     `fauna::resolve_hunt_fight(...).brought_down` — the animals the party actually put on the
>     ground, already floored to whole animals, with the engagement capping it from above (you cannot
>     bring down what you never reached). It is deliberately **absent from `workersNeeded`**: adding
>     hunters raises damage, so a fight bound inverts to *"staff more"* without limit rather than to a
>     crew size, and the three terms above stay the crew's three jobs. See `combat.md` for the seam
>     and for why all six take/forecast paths call the one helper.
>
>     **WHICH bound actually ran out is an OUTPUT now** — `fauna::hunt_take_bound` →
>     `HuntTakeBound { Engagement, Floor, Throughput, Carry, Fight }`, carried on `HuntOutcome` beside
>     `engaged` and `fled` and published on the `hunt_report` feed line (`event-feed.md`). It is a
>     *reading* of the same terms `quantise_animal_take` was handed, through the same helpers
>     (`whole_animals` for the room, `animals_the_pack_seats` for the carry), so the named bound and
>     the paid take cannot disagree about what "affordable" or "carryable" mean; ties resolve
>     `Floor/Throughput → Carry → Fight/Engagement`, stated on the function.
>
>     **`Throughput` is split out of `Floor`, and the function takes a SECOND ceiling to do it.** A
>     detached party's take ceiling is its kill-credit bank clamped to the herd's escapement room
>     (`systems::expedition_take_biomass`), not the room itself — so a raid banking toward one
>     800-unit mammoth body reported *"the herd could not spare another whole animal"* with fifteen
>     mammoths standing there. `hunt_take_bound` therefore takes `escapement_room` beside
>     `take_ceiling` and compares the two **in whole animals**: fewer affordable than sparable means
>     the party's own throughput bound the turn. The two readings have opposite remedies — `Floor`
>     says *leave*, `Throughput` says *bring more hands*. A resident band passes its ceiling for both
>     (its ceiling **is** the escapement stock), so `Throughput` is unreachable for it by construction
>     rather than by a flag. Pinned by
>     `denial_raid::a_bank_bound_raid_reports_its_throughput_and_not_the_herds_floor`, which read
>     `floor` on 60 of 60 reports before the split. It exists for §11's
>     first open question: for most species the escapement floor binds long before engagement does, so
>     an `engage_rate` authored too low silently becomes a **second floor** — and `bound=engagement`
>     is what makes that visible rather than mysterious.
>
>     **The fight is also the one bound with MEMORY.** Damage carries between turns on `Herd::wounds`
>     (`combat::DamageLedger`), so a party below `ceil(durability / (attack − defense))` brings down
>     nothing for several turns and then a whole animal — the gate is *steep*, not absolute. Every
>     take and forecast path must resolve its quarry through **`fauna::herd_quarry_fight`** and store
>     `HuntFight::wounds` back, and every forward projection resolves the fight **inside** its loop.
>     `combat.md` → "Damage carries between turns" owns the mechanism and the rollback contract.
>
>   **The per-species figures that used to sit here were measured against the PRE-CORRECTION body
>   masses and are deleted rather than restated** (`docs/plan_hunt_through_combat.md` §4.3). Nineteen
>   of the twenty masses moved, and `herders_needed` divides by `body_mass`, so every one of them
>   changed — Boar 50→12 quadruples a 750-biomass herd's head count and takes it from **1** herder to
>   **5**, inverting the "haul-bound" example the old text used. `animals_per_herder` was deliberately
>   **not** retuned to compensate: it is a per-**head** dial ("a heavier beast is not proportionally
>   more work"), so a species that is genuinely lighter genuinely needs more hands per tonne, and the
>   new counts are the correct ones. Re-measure before quoting a number here again.
>
> - **An INVESTMENT policy (Tame/Corral) sizes the herder term ownership-INDEPENDENTLY**
>   (`fauna::would_be_herders_needed`, the taming-startup-lag fix). `herd_herders_needed` is
>   ownership-gated to `0` until Population's `accrue_domestication` records `owner`, so on the turn a
>   Tame assignment *starts* the crew used to collapse to the tiny Tame-dip haul count — "1 of N working"
>   on a full crew. `would_be_herders_needed` returns the biomass-derived crew for a species that *can* be
>   tamed regardless of recorded ownership (`0` only for a `wild` ceiling), preferring the stabilized
>   `herders_needed` so an already-managed herd is identical to `herd_herders_needed` (no re-flicker). The
>   labor arm's keeping/`workers_needed` **and** the assign-time seed (`forecast_source_yield`,
>   which now folds the herder term into a hunt row's `workers_needed`) both apply it for an investment
>   policy, `herd_herders_needed` for an extractive one — a wild Sustain-hunted herd must stay
>   ownership-gated to `0` or it would read a shortfall it does not owe and falsely shed. One definition, shared
>   with the `herdersNeededIfManaged` wire field. Pinned by
>   `labor::a_wild_herd_being_tamed_reports_its_full_crew_without_the_ownership_lag`.
>
> - **The haul term is the CEILING's carry crew, not this turn's `carried`** (`fauna::hunt_haul_workers`).
>   `workers_needed`'s hauling component is the crew that carries home the **peak animal drop the
>   ceiling allows** — `ceil((floor(ceiling/body) + 1)·body / per_worker)`, off the assignment's
>   `hunt_escapement_ceiling`, the same number the take is bounded by and the same count the client's
>   compose panel `_max_useful_workers` caps at. **Neither half carries a build term any more**
>   (`docs/plan_standing_upkeep.md` §2.2): the herd offers what stands above the floor, and a hauler
>   carries what a hauler carries, whether or not a separately-staffed crew is gentling the herd
>   beside them. While the dip was live the two halves had to be dipped *asymmetrically* (an undipped
>   ceiling over a dipped rate) or the row read "enough hands" for a crew that provably could not lift
>   the drop; with the dip gone the asymmetry has nothing left to reconcile. It is deliberately **not**
>   `workers_needed_for_take(take.carried, …)`: a slow breeder whose room above the floor is lighter
>   than one body drops **zero** animals on a wait turn, so inverting `carried` collapses
>   `workers_needed` to `0` — and, for a managed herd, to the bare herder count via `max()`. That made
>   the panel contradict itself: `workersNeeded: 1` beside a 50%-`wastedYield` at one worker — *drop
>   workers* and *add workers* on the same row, with half an aurochs rotting. Sizing the crew off the
>   ceiling makes it **equal to `wasted_yield`'s answer** by construction: `workers > workers_needed` ⇒
>   overstaffed, `wasted_yield > 0` ⇒ understaffed, and the two never disagree.
>   **On a full herd that count is large — the crew that would clear it to the floor in ONE turn — and
>   it is deliberately not clamped** (`docs/plan_harvest_floor.md` §7.6): it is what makes *"this crew
>   cannot draw the herd that low"* expressible instead of silently true.
>   Both hunt sites (wild/pastoral and pen) and the assign-time forecast seed
>   (`fauna::forecast_source_yield`, off `SourceYieldForecast::ceiling_at`) read this one helper. **Wild
>   hunting** gets the same steady haul crew (`herders_needed == 0`, so `max()` collapses to it) — so a
>   wild herd's `workers_needed` is the client max-useful too. **Forage is untouched** — a gather is
>   continuous (`body_mass_yield == 0`, no lumpiness), so it keeps the ordinary `workers_needed_for_take`
>   overstaffing inversion.
> - **The ENGAGEMENT term is sized off the SAME peak drop, and it is the one that binds on light game**
>   (`fauna::hunt_engage_workers`, `docs/plan_hunt_through_combat.md` §2) — `ceil(peak_animal_drop /
>   (engage_rate × stay))`, the inverse of the **closed-form per-hunter bring-down rate**,
>   sharing `peak_animal_drop` with the haul term so the two crews can never be sized against different
>   drops. The retreat rides it for the reason it rides the take: what a hunter *reaches* is
>   not what a hunter *lands* (see "THE RETREAT PRICES THE CREW" above).
>
>   **It is the inverse of the rate, NOT of `fauna::animals_engaged`.** That helper answers how many
>   animals the party gets near, which is strictly more than it kills wherever a species has any
>   wariness. The engagement is still asserted to cover the drop (you cannot bring down what you never
>   reached); the *count* comes from the rate. The two are now the same arithmetic in both directions
>   — `animals_engaged` is a plain `workers × engage_rate` — where the crew count used to invert a
>   rate the reach itself then floored, so the crew a panel named could under-deliver by up to a body.
>
>   Its absence was the same defect as the haul term's `carried` inversion, in the opposite direction
>   and on the same panel: a Wild Fowl herd standing ~470 head above its floor is **61 biomass**, so the
>   carry-only count read **2** — *"more workers would be idle"* — about a take each additional hunter
>   would have grown, because one hunter reaches 10 birds and dozens are needed to clear the drop.
>   Adding it can only *raise* `workers_needed`, so the invariant above tightens rather than bending:
>   the overstaffed region shrinks and `workers > workers_needed` still cannot coexist with
>   `wasted_yield > 0`. Pinned on the exported row by
>   `hunt_yield_vector::the_exported_crew_counts_the_hands_that_can_reach_the_herd` (both units, with
>   the pen's no-engagement-stage reading as the liveness half — that harness holds `wariness` at its
>   identity, so the retreat's own effect is pinned separately by
>   `the_exported_crew_pays_for_the_retreat`).
> - **Wild hunting is untouched, deliberately.** No maintenance (the herd isn't yours), but it keeps
>   its carry cap. **The models differ because the products differ: hunt = reach + carry; harvest =
>   maintain + take.**
> - **Understaffing SHEDS ANIMALS — it does not touch tameness (neglect-escape arc,
>   `docs/plan_fauna_neglect_escape.md`).** The tameness-bleed (`decay_under_herded`, and with it
>   `decay_domestication`) is **DELETED**: the herd's `ladder_position` is now permanent stock capital,
>   monotone-up (earned via `Tame`), never bled by a neglected turn. Instead an under-contained managed
>   herd (`is_corralled() || owner.is_some()`) **sheds whole animals over its labor capacity** into a
>   nearby wild herd of the same species. **The overage IS the upkeep shortfall, converted into
>   animals** — `shortfall_in_loads × animals_per_herder` (`fauna::uncontained_overage`) — so it is
>   **continuous in the staffing**: half the keepers a herd wants leaves half its animals uncontained,
>   where the retired `herded_fraction < FULLY_HERDED` gate answered only *whether* it was
>   under-contained. It is the same number the retired `herded_fraction × herders_needed ×
>   animals_per_herder` capacity reconstruction produced, and emphatically **not** the
>   `(1 − herded_fraction) × current` shorthand that predated it, which over-estimates hard at a `ceil`
>   boundary (101 @ aph 50 staffed at 2: true overage 1, shorthand 33.7 — a PR #329 review fix). It
>   sheds at the per-rung
>   `husbandry.{pastoral,pen}_escape_fraction × (1 + seeded jitter)`, whole animals with a min-1 floor
>   (`shed_uncontained_animals` → `place_shed_animals`, `advance_husbandry`). It is
>   **self-limiting** (a fraction of the *overage*, so the herd converges to its labor capacity and stops)
>   and **visible** (biomass, not an invisible stat). The binary corral escape is gone — total
>   abandonment is just the *whole demand unmet* limit of the same shed. **Total abandonment BLEEDS
>   OUT and DESPAWNS, symmetric between the webs.** A herd with **zero** keepers last turn has its
>   regrowth **suppressed** — `regrow_biomass` reads the same one-turn-lag signal, now
>   `Herd::upkeep_supplied <= 0`, and zeroes growth, the pastoral twin of the untended pen's
>   `pen_fed_fraction = NOT_FED`
>   (and `advance_herds`' dispersal-despawn exempts owned herds too, so it survives to bleed out) — so it
>   keeps shedding until it can no longer shed a whole animal (`biomass < body_mass`), at which point the
>   **emptied managed entity is despawned** (`advance_husbandry` Phase 3). Its flock is already in the wild
>   web via the shed; **tameness is never reset** — it leaves *with* the animals (each shed batch is a wild
>   herd at domestication 0), so there is **no ownerless-but-tame husk**. `owner`/pen state are **never
>   cleared at a floor** (clearing `owner` would drop the herd out of the managed set and stop the shed,
>   stranding the very husk this removes); the herd stays owned/corralled and bleeds all the way down. A
>   pen is announced lost (`announce_pen_lost` — the feed line only; the fence dies with the entity, no
>   reset). **PARTIAL** neglect (some keepers, but not enough) keeps normal regrowth and settles at a stable
>   smaller **tame** herd, owner intact — a **binary abandoned/not gate, never a scaling**. The pen's
>   **feed** path (`starve_underfed_pen`) floors a *fed* pen and keeps it, unchanged (a starving pen has a
>   keeper, so it never reaches the bleed-out); `herders_needed`/hysteresis unchanged. Levers
>   `husbandry.{pastoral_escape_fraction 0.25, pen_escape_fraction 0.10, escape_fraction_jitter 0.25}`
>   (validated finite, `>= 0`, `pen < pastoral`). Pinned by
>   `fauna_husbandry`'s `an_over_stocked_managed_herd_converges_to_its_labor_capacity`,
>   `a_partially_herded_pastoral_herd_stays_tame_with_regrowth`,
>   `a_fully_abandoned_pastoral_herd_goes_feral_without_decaying_its_taming`, `neglect_never_un_tames_a_herd`,
>   `a_pen_sheds_slower_than_a_pastoral_herd`, `total_abandonment_sheds_the_flock_and_loses_the_pen`,
>   `shed_animals_appear_in_the_wild_web`, `the_shed_is_deterministic`.
> - **The under-herded EDGE NOTICE (slice 2).** When a managed herd *becomes* under-contained (a shed
>   occurs — its herders can't hold all its animals), `advance_husbandry` pushes a
>   **`CommandEventKind::HerdUnderHerded`** (`"herd_under_herded"`) feed line to the owner, naming the
>   species — *"The Rabbit Warren has too few herders — animals are drifting off"* — with detail
>   `status=under_herded herded=<f> needed=<n> herd=<id> x=<x> y=<y>`. **Edge-gated on the persisted
>   `Herd::under_herded` bool**: it fires **once** on the `false → true` transition (not every turn it
>   stays under-contained), clears the turn the herd recovers (fully staffed / within capacity), and
>   re-fires on a later relapse. Distinct from the pen-*lost* (`announce_pen_lost`, Corral) and
>   pen-*starving* (`starve_underfed_pen`, Corral) edges — this is the herder-shortfall edge and it
>   fires for pastoral herds too. Unlike the transient `pen_starving`, `under_herded` is
>   **snapshot-persisted** (rewinds with rollback) so a restore does not spuriously re-announce. Pinned
>   by `fauna_husbandry`'s `the_under_herded_notice_{fires_once_on_becoming_under_contained,
>   re_fires_after_recovery_then_relapse}` + `the_persisted_under_herded_flag_suppresses_a_re_fire`,
>   `snapshot::mod`'s herd-state identity round-trip, and
>   `integration_tests/fauna_rollback::under_herded_edge_state_rewinds_on_rollback`. **Slice 3 (client,
>   not built):** rendering this line + the panel warning icon.

**Retired: single-task model → labor allocation (Early-Game Labor slice 3a).** The
one-task-per-band model (`reassign_band` + `HarvestAssignment`/`ScoutAssignment`/`FaunaPursuit`
and their systems `advance_harvest_assignments`/`advance_scout_assignments`/`advance_fauna_pursuits`,
plus the `scout`/`forage`/`hunt_fauna`/`follow_herd` command handlers) is **removed**. A band is now a
**labor pool**: a `LaborAllocation` component (`components.rs`) partitions its whole working-age workers
(`available_workers(working)` = `floor`) across `LaborTarget`s — `Forage { tile, policy, species }`,
`Hunt { fauna_id, policy }`, `Scout`, `Warrior`, and the two **keeping roles** `Agriculture` /
`Husbandry` (`docs/plan_standing_upkeep.md` §2.5 — a band-level maintenance pool per food web, one
row each, staffed exactly like Scout and Warrior) — with the invariant `Σ workers ≤ available`. Each
staffed row is a `LaborAssignment { target, workers, improvement }`: **`policy` is the harvest STANCE
and `improvement` is what the crew is BUILDING**, two independent axes since issue #442 — see "An
assignment has TWO axes" in `intensification.md`. `advance_labor_allocation`
(`systems.rs`, Population stage, replacing the three retired systems) resolves per-worker yields each
turn: Forage = `workers × per_worker_yield × seasonal_weight` from an in-range `FoodModuleTag` tile;
Hunt take = `min(workers × per_worker_biomass_capacity, policy_ceiling)` (reusing the per-policy ecology
ceilings — Sustain under-hunting lets a herd grow), tracking a roaming herd out to `band_work_range +
hunt_leash_tiles` before the assignment lapses (feed entry). Scout extends the band's live sight range
in `calculate_visibility` by posting forward-observer vantages (`scout.vantage_distance(scouts)` out
in all 6 hex directions, LOS revealed from each — re-marked Active every turn while scouts are
staffed, scaling with head-count); Warrior is inert until the predator slice. `move_band <faction> <band> <x> <y>` sets a `BandTravel` component that
`advance_band_movement` steps at `band_move_tiles_per_turn`/turn. `assign_labor` sets one target's
worker count (0 unassigns; clamps to free headroom); **`cancel_order <faction_id> [band_id]
[all|work|roles]`** clears the assignments its **scope** names — `all` (the default when the token is
omitted, and the historical behaviour) clears every assignment **and** stops movement (fully idle),
`work` unassigns only the worked Forage/Hunt sources, `roles` clears only the Scout/Warrior standing
roles. **The two narrow scopes never touch `BandTravel`** — moving is not working, so unassigning a
band's sources must not strand it mid-journey — and the rejection is **scope-aware**: a band with
sources but no roles accepts `work` and refuses `roles` ("…has no standing roles to clear."), rather
than claiming it is idle. The scope rides the wire as `CancelOrderCommand.scope` (absent == `all`);
an **unrecognised scope token is a hard text-parse error** (failing closed — silently defaulting to
`all` would mass-unassign a band that asked for `work`), while an unrecognised *proto* string decodes
to `all`. The Band panel's per-section clears are what the scope exists for. The snapshot exports `laborAssignments`/`idleWorkers`/`workingAge`, and still
summarizes `activity` (target-kind with most workers) + `huntMode` (largest Hunt's policy) for the
pre-3b client. Husbandry re-homes here — but **Sustain no longer tames** (slice 3a): a **`Tame`** Hunt
fills the meter, while any stewardship policy on a Thriving source earns the knowledge that source's
current **rung** teaches (slice 4 — see "The knowledge pattern"). The
**improvements** `Cultivate` (plant-only) / `Corral` (animal-only) also resolve here — a reduced
take while the improvement is prepared, then the managed yield; see "Cultivation" / "Corral". Config:
`labor_config.json`. Client allocation panel is PR 3b.

**Ecology — critical-depensation collapse (Phase D)** — `advance_herds` applies one
turn of `net_biomass_delta` (`fauna.rs`) toward each group's per-species carrying
capacity (`Herd.carrying_capacity` = the species' `biomass[1]`). The curve is **not**
plain logistic: above the Allee threshold (`ecology.collapse_fraction * cap`) the group
regrows logistically at `ecology.regrowth_rate`; **below** it the group is non-viable and
declines by `ecology.collapse_rate` per turn — an **irreversible crash to local
extinction even if hunting stops** (the overhunting point of no return). `advance_herds`
**despawns** any group below the viability floor (`ecology.extinction_floor * cap`), so a
collapse reaches zero in finite turns. So a hunt/follow draws a group down in
`Population`; it regrows (or, past the threshold, collapses) in the next turn's
`Logistics`; sustained overhunting drives it extinct permanently.

**Ecology phase** — each `Herd` carries a coarse `EcologyPhase` (`Thriving` / `Stressed` /
`Collapsing`), stamped by `Herd::refresh_ecology_phase` at the end of every `advance_herds`
pass from biomass vs `ecology.stressed_fraction`/`collapse_fraction` (`classify_ecology_phase`),
against the rung's own ecology and capacity (`herd_ecology` / `herd_capacity`), so the client can
warn the player before a group is doomed.

**It is a READOUT: nothing in the sim gates on it.** It used to gate domestication — husbandry
progress accrued only on a `Thriving` herd — and the same `EcologyPhase::Thriving` term stood on
every build and knowledge-earn site on both food webs. The harvest-floor arc replaced all of them
with the floor's learn multiplier (`docs/plan_harvest_floor.md` §3.2: *how deep are you drawing?*
rather than *is the source healthy?*), so a build now slows rather than stopping outright. What still
reads the stored word is the analytics log line, the display mirror `to_entry`, and the Telling's
`fauna.collapsing_group_count` / `most_collapsed_species` nouns.

**The word on the wire is RE-DERIVED at capture**, not copied from the stored one — see
`husbandry.md` → "A herd row is assembled from TWO frames", which owns the provenance rule. The
stored word is a turn old by the time the row is built, and the cut points published beside it come
from the live `herd_ecology`, so copying it made a completing Tame or Corral publish a word and a set
of cuts describing different rungs.

**Immigration** — `repopulate_fauna` (`fauna.rs`, `TurnStage::Logistics` right after
`advance_herds`) gives a low per-turn chance (`immigration.chance_per_turn`) to respawn one
short-range game group up to `abundance.max_total_game`, sampling up to
`immigration.max_attempts` random land tiles that host game and respect `min_spacing`. This
keeps an overhunted map slowly replenishing (early forager play stays game-rich) without
undoing a local extinction (the crashed group is gone; a *new* group may immigrate
elsewhere). Seeded per-turn from `map_seed ^ tick ^ salt` (deterministic under rollback).

**Domestication / husbandry (Phase E)** — the pastoral counter-force to depletion. A `Herd` carries
**one private `ladder_position`** (in **work units**, spanning both animal rungs) plus a **stamped
`standing`** derived from it, and `owner: Option<FactionId>`, exported as
`HerdTelemetryState.domestication`. The retired two-meter form — `domestication_progress` /
`corral_progress`, each with its own stamped `*_cost` — is **gone**; both webs are now on the one
position (`intensification.md` → "The storage: ONE position, and a STAMPED standing beside it").
- *Accrual — the **`Tame`** verb, not a side effect of hunting*: in `advance_labor_allocation`
  (Population), a Hunt assignment carrying **`Improvement::Tame`** adds the
  crew's own output in work units against `work_cost × the species' taming_cost_multiplier`, for the acting faction
  (sets `owner` on first accrual; only the owner accrues; gated on **Herding** + the species'
  husbandry ceiling + something standing above the crew's floor); the work lands on the herd's one
  `ladder_position`, and the stamped `standing` beside it is what says which rung that reaches.
  **There is no health gate** —
  `docs/plan_harvest_floor.md` §3.2 replaced it with the floor's rate on every rung of both webs, and
  the "Ecology phase" section above says the same. The herd is domesticated once its
  `ladder_position` carries the stamped `standing` to `animal:pastoral`, which is what
  `is_domesticated()` reads; the old normalized `1.0` retired
  with `RUNG_COMPLETE`. **A `Sustain` hunt tames nothing** — it only
  *teaches* the faction Herding. That de-conflation is slice 3a; see "The `Tame` verb".
- *Decay*: **there is none.** `ladder_position` is monotone-up since the neglect-escape arc —
  neglect sheds **animals**, never tameness — and the `decay_fraction_per_turn` the `animal:pastoral`
  rung used to carry has been retired from the ladder outright: on the plant branch the bleed is the
  unmet `upkeep` (`docs/plan_standing_upkeep.md` §2.4), and on this one there is nothing to bleed. What
  `advance_husbandry` does to a neglected herd is the **shed**, gated on the rung's `grace_turns` —
  see "Herding is standing labor" in `husbandry.md`.
- *Yield*: **none here — passive-free pastoral is RETIRED** (intensification ladder slice 3b, §3:
  every rung is worker-driven). A tamed herd used to pay its owner the pastoral MSY **with no worker
  at all**, split evenly across the owner's bands; `advance_husbandry` now pays **nothing** and a
  pastoral herd yields **only** through a normal `Hunt` assignment, exactly like a wild one. The
  taming payoff is **yield per worker**: `herd_ecology` puts a tamed herd on the pastoral ecology
  (`r` = wild × `pastoral_gain` 2.0), so the *same* hunters take ~2× the sustainable food from the
  same `K` (measured: `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`).
  That also deletes the "you are not paid twice" hazard structurally — with no second payment to
  stack, the `Corral` dip is a real cost again — so the `Herd::worked_this_turn` flag that guarded it
  is **gone**.
- *Collapse immunity*: `regrow_biomass` uses plain `logistic_regrowth` (never the collapse
  branch) for a domesticated herd — a managed group recovers and never crashes.
- *No early claim*: the `domesticate <faction_id> <herd_id>` command, its `husbandry.claim_threshold`
  lever and `Herd::claim_domestication` are **deleted** (slice 3a). Snapping progress to `1.0` let the
  player skip the investment, which is the entire decision — the plant side removed its twin for the
  same reason. **Proto field 30 is reserved and must never be reused.** The `tame` command that
  replaced it *sets the `Tame` policy*; it claims nothing.
- *Practice teaches the next rung*: working this herd under a **stewardship** policy earns the faction
  the knowledge its **current rung** teaches — **Herding** while it is wild, **Penning** once it is
  pastoral (slice 4, §4). See "The knowledge pattern".
- `HerdRegistry::domesticated_count(faction)` is the seam the future `SedentarizationScore`
  reads for its "domestication progress" input.

