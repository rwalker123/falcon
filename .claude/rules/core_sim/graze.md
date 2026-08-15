---
paths:
  - "core_sim/src/graze.rs"
  - "core_sim/tests/grazing_*.rs"
---

<!-- Extracted verbatim from lines 2826-3193 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# The Graze (Pasture) Layer (Grazing Phase 2a)

**Humans and animals do not eat the same things.** The land carries **two vegetal stocks, on two food
webs** (authoritative design: `docs/plan_grazing_foundation.md`):

| | `ForagePatch.biomass` (Depletable Forage) | **`GrazePatch.biomass`** |
|---|---|---|
| Who eats it | **humans** (Forage assignments) | **animals** (herds, wild and penned) |
| Where it is | `FoodModuleTag` tiles — **not sparse**: nearly every biome is tagged, so in practice every food-bearing tile | **any vegetated land**, by biome (dense) |
| What it is | seeds, nuts, tubers, fruit, shellfish | grass, browse, forbs — **cellulose humans cannot digest** |
| Its capacity | `forage.capacity_by_biome` (`labor_config.json`) | `graze.capacity_by_biome` (`fauna_config.json`) |

That is not flavor: it is the economic basis of herding (a pastoralist converts a resource
**worthless to humans** into meat and milk), and it is why *your best farm is usually not your best
pasture*. `graze.rs` mirrors `forage.rs` (which mirrors the herd model) exactly — the proven,
rollback-persisted pattern.

## The two food webs — two tables, meant to disagree

**Both webs are per-biome tables over the same `TerrainType` set, in the same shape, with the same `validate()`
discipline** (total table required; a missing row would read as an invisible zero and **zero must be
stated**). They are per-**biome**, not per-`FoodModule`, precisely so they are comparable tile-for-tile
and can disagree *within* a module — **that disagreement is the agropastoral decision.** The
`FoodModuleTag` model is untouched: the module still decides what *kind* of gathering a tile offers
(and its `seasonal_weight`); the table decides *how much* is there.

| biome | graze (animals) | forage (humans) | the story |
|---|---|---|---|
| `PrairieSteppe` | **240** (the reference pasture) | 70 | grass: the animals feast, humans get seed heads |
| `RiverDelta` / `Floodplain` | 130 | **210 / 205** | the richest human ground there is |
| `AlluvialPlain` | **110** | **195** | silt + water = **cropland**. The FARM, not the pasture |
| `MixedWoodland` | **55** | **190** | nuts, mast, berries under a canopy that shades out the ground cover — **the flagship inversion** |
| `Tundra` / `AlpineMountain` | 100 / 65 | 25 / 20 | **rangeland**: pastoralism lives exactly where farming can't |
| `ContinentalShelf` / `CoralShelf` | **0** (water) | 130 / 180 | the coastal larder — a fishery is a food module on *water* |
| `RollingHills` / `PeatHeath` | 150 / 135 | 80 / 55 | |
| glacier / lava / salt flat / deep ocean | **0** | **0** | a *stated* zero |

**The silt lowlands were LOWERED on the graze side** (`AlluvialPlain` 230 → **110**, `Floodplain`/
`RiverDelta` 230/220 → 130): a river plain is prime *cropland*, not prime range, and its value moved to
the human web where it belongs. `AlluvialPlain` is additionally the tag solver's universal fallback
biome (~25% of all land even after the `FertileLowland` palette fix), so leaving it tied with prairie
for best pasture baked a **worldgen artifact into the fauna model**.

**Measured, not asserted** (`integration_tests/tests/graze_distribution.rs::two_food_web_report`,
earthlike 80×52, seeds 11/4242/90210 — run with `--nocapture` for the joint histogram):
- **Correlation between the two webs across living land: −0.11 / +0.03 / −0.01.** Near zero, as
  intended: knowing a tile's pasture tells you almost nothing about its farm. (Across *all* land it is
  +0.13…+0.24 — bare rock is a shared **zero**, an irreducible positive term that says nothing about
  the design claim; a farm-vs-pasture decision needs land that can feed *somebody*.)
- **Land that is top-decile in BOTH webs: 0.0% on every seed** (independence would give 1%). *Your best
  farm is not your best pasture* — measured, not claimed. (The top-**quartile** overlap is printed too
  but **not** guarded: `AlluvialPlain` is ~25% of land, so the 75th-percentile graze cut lands *inside
  that one biome* and the number flips 0% ↔ 24% on a hair. That is a cliff, not a measurement — do not
  tune a capacity table to it.)
- **Balance impact on the human food economy: map-wide capacity −18…−20%, but the early game is flat.**
  The mean capacity of patches within `band_work_range` of the start is **123 / 128 / 99** vs the
  retired flat **120** (mean 117 across seeds, −3%). The map-wide drop is almost all tundra, bare rock
  and scrub — land nobody starts on, which the old flat 120 was pricing as richly as a river delta.
  Individual starts *do* move (a grassland/tundra start is thinner, a river-valley start richer): that
  spatial variance is the feature, and it is the thing to watch in a live campaign.

> **Phase 2a ships this layer INERT.** It seeds, regrows, persists and exports — and **nothing reads
> it for gameplay**. No herd behaviour changes, zero balance impact. Herd carrying capacity,
> competition, overgrazing, migration and spawn placement all become functions of it in Phase 2b/2c;
> the layer ships inert first so its *distribution can be looked at on a real map* before the fauna
> model is bet on it.

- **`GrazeRegistry`** (resource, `graze.rs`) — per-land-tile `GrazePatch { biomass, carrying_capacity,
  ecology_phase }`, keyed by tile coord. **Only tiles with a positive capacity hold a patch**, so
  "this biome has no pasture" is an *absent* reading, never a zero one.
- **Seeding** (`spawn_initial_graze`, Startup right after `spawn_initial_forage`): one full patch
  (`biomass = carrying_capacity`) per non-`WATER` land tile whose biome has a positive
  `graze.capacity_by_biome`. Idempotent (a restored world is skipped) — the `spawn_initial_forage`
  guard.
- **Regrowth** (`advance_graze_regrowth`, `TurnStage::Logistics` right after
  `advance_forage_regrowth`): **pure logistic regrowth over a reseed floor**, then a phase refresh.
  **No Allee / collapse branch — grass has no depensation**, and it **never despawns**: an eaten-out
  tile always recovers (slowly). Shares the one plant curve `fauna::reseeding_logistic_regrowth` with
  `forage::regrow_patch`, so the two stocks can never drift apart. Permanent degradation
  (desertification) is a deliberate later lever, not this arc.
- **Capacity is a property of the LAND, not the animal** — `graze.capacity_by_biome`, a **data table
  over every `TerrainType`, not a formula**, and **read against its twin** `forage.capacity_by_biome` (see
  "The two food webs" above, which owns the joint tuning table and the measurements). Anchor:
  `PrairieSteppe` = **240** is *the* reference pasture; every other row is a claim relative to it.
  `MixedWoodland` (55) / `BorealTaiga` (40) are deliberately **poor** — a closed canopy shades out the
  ground cover, the inversion the two-stock split exists to create. Cold/high **rangeland** (Tundra
  100, AlpineMountain 65, HighPlateau 75, SemiAridScrub 100) is deliberately *better for animals than
  for humans*: pastoralism exists precisely where farming cannot. Water / glacier / lava / salt flat
  are a **stated 0**. The absolute scale is a free parameter; only the ratios matter until Phase 2b's
  `fodder_per_biomass` denominates it into animals.
- **Config** (`fauna_config.json` `graze` — homed here, not in a file of its own, because graze is the
  *substrate of the fauna model*: every consumer of it is a fauna system, and it lets the block reuse
  `FaunaConfig::validate` verbatim): `capacity_by_biome`, `ecology` (`regrowth_rate` **0.40** —
  **grass is the fastest-renewing vegetal stock in the model**: wild fauna 0.05 ≪ forage 0.25 <
  **graze 0.40** ≪ a fed pen 0.90; `collapse_rate` is *inert* for graze, as it is for forage — pure
  logistic never reads it; `collapse_fraction`/`stressed_fraction` are the phase bands the overgrazing
  readout uses), `reseed_floor_fraction` (0.02, mirroring forage's — kept **below**
  `collapse_fraction` so the floor stops *permanent death* without *hiding overgrazing*).
- **Validated** (`FaunaConfig::validate`, so every load path is covered): the table must be **total**
  over every `TerrainType` (a missing row silently reads `0` — an invisible dead zone nothing would ever
  explain: **zero must be stated, never defaulted**), every row finite and `>= 0`, **at least one row
  positive** (an all-zero table disables the whole layer while parsing perfectly), the graze ecology
  live and phase-ordered, and `reseed_floor_fraction < collapse_fraction`.
- **Persistence** — `GrazeRegistry` survives a rollback exactly like `ForageRegistry`/`HerdRegistry`:
  the **checkpoint carries the registry whole** (`SimState::graze`). It reached that state by way of
  a per-tile `GrazeState` mirror captured coord-sorted onto `WorldSnapshot.graze_registry`; that copy
  was deleted once the checkpoint left it without a reader (`checkpoints.md`), and its map-sized
  per-turn sort went with it. The `GrazeState` record and `GrazeRegistry::{from_states,
  update_from_states}` went next, having nothing left to decode. Graze is **wild ground** — never owned, tended or improved.
- **Wire — on `TileState`, not a patch list.** `TileState.grazeBiomass:float` /
  `grazeCapacity:float` / `grazeEcologyPhase:ubyte` (`0` = none, `1` thriving, `2` stressed, `3`
  collapsing — the `moraleCause:ubyte` idiom; `none` is the default so "no pasture" can never be
  misread as "healthy pasture"). **Measured, not assumed** (earthlike 80×52, 1511 patches): the
  TileState fields cost **+12.9 KB** on a 3.63 MB FlatBuffers snapshot (**+0.36%**) and **+0.58 ms**
  on a ~22 ms turn; the rollback record costs +55.9 KB (+1.6%). A `ScalarRaster` channel — the obvious
  alternative for a dense per-tile scalar — would cost **33.3 KB** (2.6× more: it pays for all 4160
  tiles, water included), carry **one** scalar instead of three (no capacity → no % → no overgrazing
  signal on the tile card), and re-ship **whole** on any single tile's change, where `TileState` is
  **per-entity diffed** and so costs *zero* delta bytes on an ungrazed turn. The dense shape is the
  one place graze deliberately diverges from `ForagePatchState`.
- **Forage-potential twin — `TileState.forageCapacity:float`** (append-only, beside the graze fields on
  both `WorldSnapshot` and `WorldDelta`). The exact human-food mirror of `grazeCapacity`, so the client
  can draw a **Forage overlay** the same way it draws the pasture one. Sourced **directly from
  `forage.capacity_by_biome` (`ForageLaborConfig::capacity_for(tile.terrain)`)** for *every* tile —
  **not** from the `ForageRegistry` — so the biome's potential shows on **every** tile, including the
  water and the bare rock that hold no `ForagePatch` at all. (**Corrected:** this used to claim the
  registry was *sparse* — "~95% of tiles, all the best cropland, carry no `ForagePatch`". That is
  **false** and was measured false: `classify_food_module` tags essentially every biome, so
  `spawn_initial_forage` seeds a patch on **every** food-bearing tile — standard map, **2328
  food-bearing tiles, 2328 patches, zero bare**. The claim predates the per-biome capacity table, and
  it is what the `Sow` design originally reasoned from; see "The `Sow` verb + the Field".) Consequence, preserved deliberately: it is
  **non-zero on fishery water** (`ContinentalShelf` 130 / `CoralShelf` 180 / `InlandSea` 110 — a fishery
  is a food module on water), a real divergence from graze where all water is 0; only a *stated-zero*
  biome (deep ocean, glacier, lava, salt flat) reads 0. On a food-module tile that *does* hold a
  `ForagePatch`, that patch was seeded at the same `capacity_for(biome)`, so `forageCapacity` equals the
  patch's `carryingCapacity` — no drift between the potential and the realized patch. Cost: **+1 float
  per tile** (per-entity diffed, so zero delta bytes on an unchanged tile). Populated at capture beside
  the graze fields in `snapshot.rs::tile_state`.
- **Distribution, measured on real maps** (`integration_tests/tests/graze_distribution.rs` — run with
  `--nocapture` for the histogram; the guards keep the model claims true under retuning). Earthlike
  80×52, three seeds: ~1500–1560 land tiles carry ~162–177 k total graze capacity, and only
  **0.8–1.0% of land is zero-graze** (glacier / volcanic / fumarole). Prairie is the richest per-tile
  pasture (240), as intended. Two earlier findings are now **closed**: the `FertileLowland` palette
  niche is no longer thinned (`k_small` 2 → 4, `map_presets.json`), so **forest and floodplain exist on
  the standard map** — the flagship inversion is observable in play — and `AlluvialPlain`, which was
  absorbing both of them as their niche-mate, no longer carries the map's pasture: at graze 110 its
  share of total graze falls to ~16–24% (from 37–48%), and the *dominant* pasture is the steppe again,
  not the fallback biome. See "The two food webs" for the joint (graze + forage) measurement.
- **Follow-ups:** the **client** pasture overlay + tile-card readout — and the twin **Forage overlay**
  off `TileState.forageCapacity` (both are client-dev slices: the data is on the wire; note each overlay
  must be built from `TileState`, since neither graze nor forage is a raster channel). **Phase 2b**
  (herds eat it, `K_herd` = `range graze flow / fodder_per_biomass`) and **Phase 2d** (the pen becomes
  fenced land, retiring `pen.capacity_fraction`) have since landed.

## Phase 2b-i — herds eat their range, movement is graze-aware (INERT on K)

The first 2b slice (`docs/plan_grazing_2b.md` §8). Herds now **draw the graze layer down** on the
tiles they occupy, and **movement avoids barren ground** — but **carrying capacity is still the
species constant**, so the hunting economy (hunt/forecast yields) is byte-identical to 2a. This
de-risks the K change (2b-ii) by proving the eating + movement first, exactly as 2a shipped the graze
layer inert.

- **`grid_utils::hex_range_tiles(center, radius, w, h, wrap)`** — every tile within odd-r hex distance
  `radius` (the hex disk: `1, 7, 19, …`), wrap-aware horizontally + pole-clamped. Bounding-box scan +
  exact `hex_distance_wrapped` filter. Shared by the herd range (and the pen/anything later).
- **`SpeciesDef.fodder_per_biomass`** (`fauna_config.json`, `#[serde(default)]`) — the fodder one unit
  of animal biomass demands per turn. **Cached onto `Herd` at spawn** (mirroring `carrying_capacity`)
  and rewound by rollback with the rest of the cloned registry (sim-side only — not on the client
  wire). Shipped anchors (smaller animals eat MORE per unit biomass; **inert this slice**,
  retuned from a measured anchor in 2b-ii): rabbit **0.10** / fowl **0.09** / boar **0.06** / deer
  **0.05** / steppe_runner **0.05** / marsh_grazer **0.03** / mammoth **0.011**. Each is
  `range_tiles × per-tile MSY (0.1·capacity) ÷ species K`, so a herd near its constant K eats ~its
  range's sustainable graze flow and holds the range near half capacity.
- **`Herd::graze_range_radius(&SpeciesDef)`** — the footprint a herd grazes, from `size_class`: Small
  → **0** (its one tile), Big → **1**, Migratory → **loiter_radius** (the current loiter cluster, not
  the whole route).
- **`advance_herd_grazing`** (Logistics, registered **after `advance_herds`** and **before
  `advance_graze_regrowth`**) — the `forage_take`-style draw-down: each **mobile, non-corralled** herd
  demands `fodder_per_biomass × biomass` and draws it from its range's `GrazeRegistry` patches,
  **proportional to each tile's available graze** and floored at each patch's `reseed_floor_fraction ×
  capacity` (never permanently kills a tile). Herds draw **sequentially in `HerdRegistry` order** (that
  Vec is rollback-persisted in a fixed order), so a shared tile is order-independent under rollback.
  Corralled herds are fed from the larder (`pen_upkeep`), not the land, so they are skipped.
- **Graze-aware movement** (§4.1): `advance_herd_roam` (`best_land_neighbor_toward` /
  `wander_near_anchor`) **never steps onto a zero-graze tile** (no patch / zero capacity) and **biases
  toward higher graze capacity** among candidates, folding graze into the *existing* per-turn seeded
  RNG (deterministic under rollback). A herd hemmed in by barren stays put. `build_route` (spawn-time)
  biases migratory anchors onto the most fertile nearby ground, reading capacity **directly from
  `graze.capacity_by_biome`** (graze patches don't exist yet — `spawn_initial_herds` runs before
  `spawn_initial_graze`). Movement keys off **capacity** (stable land fertility), *not* live biomass —
  chasing *receding* grass (leaving a cluster because it was eaten out) is the emergent 2c dynamic,
  deliberately deferred. `advance_herds` takes the graze layer as `Option<Res<GrazeRegistry>>`: a
  `None`/empty registry falls back to plain land movement (the isolated fauna test harnesses).
- **Measured** (`core_sim/tests/grazing_2b.rs`, earthlike seed 119304647): herd-occupied pasture sits
  below untouched pasture (grazing visibly draws range down); a vacated cluster recovers to capacity
  once herds leave; ~0 herds end a turn on a zero-graze tile (movement avoids barren). NB the 2b-i
  draw-down floor moved from the reseed floor to `graze.overgraze_escapement_fraction` in 2b-ii.

See Also: `docs/plan_grazing_foundation.md` (design), `docs/plan_grazing_2b.md` (the 2b arc),
"Depletable Forage" (the human-edible twin and the `ForageRegistry` pattern this mirrors), "Fauna &
Wild Game" (the model this becomes the substrate of in Phase 2b).

## Phase 2b-ii — carrying capacity becomes ecological; `regrowth_rate` becomes per-species

The big rebalance (`docs/plan_grazing_2b.md` §2/§3/§5). A mobile herd's `K` is **no longer the species
constant** — it is derived each turn from the graze its range yields, and each wild species breeds at
its **own** rate. Gated by a convergence test (§2.2), because a coupled consumer–resource system
oscillates or crashes if built carelessly.

- **`K` is range-derived, recomputed in `advance_herds`.** After a mobile (non-corralled) herd roams,
  `ecological_carrying_capacity` sets `herd.carrying_capacity =
  Σ_range graze_sustainable_flow(G_tile) / fodder_per_biomass` over `hex_range_tiles(current_pos,
  graze_range_radius)` — the **same** tiles `advance_herd_grazing` eats, at their **current** (drawn-
  down) biomass. So overgrazing a range lowers its flow → lowers `K` → shrinks the herd (the emergent
  overgrazing spiral); a range held at/above its MSY point yields full flow → `K` at max. This is the
  **one** write; `herd_capacity(herd, fauna)` still reads the cached field, so **every downstream
  consumer is unchanged** (no `&GrazeRegistry` threaded through the ~15 capacity call sites). Since
  **Grazing 2d** a **corralled** herd's `K` is likewise recomputed — over its *fenced footprint*
  (`hex_range_tiles(corralled_at, pen_radius)`), via the same `ecological_carrying_capacity` seam (a
  wholly-barren footprint keeps the frozen `K` and is fully larder-fed). A non-grazing herd
  (`fodder ≤ 0`) or an absent graze layer keeps the constant `K`.
- **`graze_sustainable_flow` — NOT `sustainable_yield`.** The K flow is pure logistic at the MSY-clamped
  biomass (`logistic_regrowth(min(G, cap/2), cap, r_graze)`), deliberately **without** the Allee cutoff
  `sustainable_yield` applies — **grass has no depensation**, so a heavily-but-recoverably grazed tile
  must still yield a positive `K` (the design's formula named `sustainable_yield`, but that would read
  `K = 0` below `collapse_fraction` and crash a herd on ground that in fact regrows).
- **Per-species `regrowth_rate` (`SpeciesDef.regrowth_rate: Option<f32>`, `#[serde(default)]`).** Cached
  on `Herd` at spawn (`regrowth_rate_or(fauna.ecology.regrowth_rate)`), rewound by rollback with the
  cloned registry (sim-side only). **`herd_ecology` now returns an owned `EcologyConfig`**
  with the wild curve's `regrowth_rate` swapped for the herd's own (phase bands stay shared); pastoral
  (0.25) / pen (0.90) keep their rung's rate. This is still THE single seam — every consumer reads the
  folded rate there. Anchors: rabbit/fowl **0.35**, deer/boar **0.10**, migratory **0.04** (was one
  global 0.05). **This is the PR #117 fix**: small game bred at a mammoth's rate was the artifact behind
  "a rabbit warren can't provision an expedition."
- **The convergence gate — `graze.overgraze_escapement_fraction` (0.25).** Grazing (`graze_take`) may
  draw a patch down to this fraction of capacity but **no lower** in a turn — constant-*escapement*, the
  same lesson the corral learned (`docs/plan_corral_managed_population.md` §3). Without it the herd's
  constant-*catch* demand strips an over-subscribed range into a permanently-stripped attractor at the
  reseed floor (a stunted remnant on dead ground); with it an **overgrazed range recovers** to a stable
  smaller herd. Validated `>` `reseed_floor_fraction` and `< 0.5` (the graze MSY point — overgrazing
  below the productive intensity stays possible/visible). It bounds `K` below at ≈ 0.84·`K_max`, so
  overgrazing shrinks a herd by ≤ ~16% — a modest but stable force; lower it for deeper overgrazing at
  rising crash risk.
- **Turn order (discretization that converges):** recompute `K` from **pre-eat** graze → herd grows
  toward it (clamped) → herd eats (`advance_herd_grazing`) → graze regrows (`advance_graze_regrowth`).
  The hard clamp `biomass ≤ K` plus the flat-K plateau above `cap/2` plus the escapement floor make it
  converge monotonically (no growing oscillation) from **every** start.
- **Measured — the convergence gate** (`core_sim/tests/grazing_2b_convergence.rs`, ≥300 turns, pinned):
  every regime (rabbit `r`=0.35, deer 0.10, mammoth 0.04, and the hottest `r`=0.40 = graze) reaches a
  **stable fixed point** from under-grazed / over-populated / over-grazed / two-herds-sharing starts;
  under- and over-populated starts converge to the **same** `K`; an overgrazed range (graze 0.12)
  **recovers** to graze ~0.33–0.61 / herd 88–100% `K_max`, never the stripped floor; the coupled system
  is deterministic (two runs bit-identical). Biomass tail bands are 0; the graze fraction holds a fixed
  ≤0.7% micro-2-cycle (a small band, not growing).
- **Measured — the K distribution + hunting economy** (`grazing_2b::the_2b_ii_measurement_report`,
  earthlike seed 119304647, 120 turns): Red Deer `K` mean **1352** (460 forest → 2150 steppe) vs the
  retired **1200**; Rabbit **163** (48–240) vs 200; Wild Boar **1049** vs 1000 — the sedentary species
  land near their old constants with real biome spread. Migratory `K` came in **below** the old
  constants (Steppe Runners 3212 vs 9000, Marsh Grazers 5629 vs 9000) — their loiter-cluster range ×
  cap doesn't reach the old biomass-max, a **retune flag** (lower migratory `fodder` to raise `K` if
  the megafauna hunting economy wants it). Sustain MSY (`r·K/4·p`) roughly **doubled** for deer/boar
  (both `r` and `K` up) and rose **~5.7×** for rabbit (**0.05 → 0.285** food/turn) — the **small-game
  viability reversal**: a rabbit warren is now a fast provisioner (and the small/Deplete hunting
  expedition, which never filled under the old uniform `r`, now completes).
- **The fast-breeder ladder inversion — FIXED in 2d.** A wild rabbit's `r`=0.35 exceeded the retired
  flat pastoral 0.25, so taming a rabbit *used* to be a growth downgrade. Grazing 2d makes the managed
  rungs a *multiple* of each species' own wild `r` (§ "Phase 2d"), so pastoral `r = wild_r × 2.0 >
  wild_r` for every species and the inversion is gone.
  `fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder` asserts the per-species
  gross growth-rate ladder — as a **long-run average**, for the reason set out in "The husbandry yield
  ladder" (escapement makes a single turn read stock, not rate).

See Also: `docs/plan_grazing_2b.md` §2.2 (the convergence risk), §9 (the measure list),
`docs/plan_corral_managed_population.md` §3 (the constant-escapement lesson this reuses).

## Phase 2d — the pen economy: a pen becomes fenced land

The pen slice (`docs/plan_grazing_2d.md`). A pen stops being a special case (a single frozen tile fed
entirely from the larder) and becomes **a piece of fenced land the herd grazes**:

- **`Herd.pen_radius`** (default `0` = today's single tile) — the pen's footprint is
  `hex_range_tiles(corralled_at, pen_radius)`. All footprint logic (`herd_footprint`) reads it; the
  `ExtendPen` command grows it (2d-β, below).
- **Footprint `K`** — `advance_herds` recomputes a penned herd's `K` over its footprint via the same
  `ecological_carrying_capacity` seam a mobile herd uses (penned herds stop being frozen). A
  **wholly-barren** footprint keeps the frozen `K` and is fully larder-fed (§2.3's preserved worst case).
- **Penned grazing** — `advance_herd_grazing` no longer skips corralled herds; a pen draws its footprint
  down with the same `graze_take` + `overgraze_escapement_fraction` (0.25) floor as a wild herd,
  capturing `footprint_intake`.
- **The larder offset** (§2.3) — the FEED phase pays only `pen.upkeep_per_biomass × biomass ×
  (1 − pasture_fraction)`, `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass),
  0, 1)`; `pen_fed_fraction` = the total fed share (pasture + the paid part of the reduced bill). The
  food-ledger identity (`penFeedUpkeep`) is untouched — it draws the *actual* paid amount.
- **Per-species husbandry `r`** (§3) — retires flat pastoral 0.25 / pen 0.90 for `min(cap, wild_r ×
  gain)` (`pastoral_gain` 1.5, `pen_gain` 3.0, `husbandry_regrowth_cap` 0.75). `capacity_fraction` /
  `pen_capacity` are **deleted**; `herd_capacity` collapses to `herd.carrying_capacity`.
- **Per-species husbandry DENSITY (K)** — the ceiling twin of the `r`-gains: the per-species
  `SpeciesDef.pastoral_density` / `pen_density` (default **1.0**) multiply a tamed / penned herd's
  range-or-footprint-derived `K` at the one seam `ecological_carrying_capacity` (via
  `fauna::herd_density_gain`), so domestication makes the land hold *more* animals, big for the prime
  grazer domesticates (goat/aurochs **2.0 / 5.0**). Orthogonal to `r`, byte-identical for a wild herd
  (`×1.0`), and scale-free in the pen net-positive floor. See "The husbandry yield ladder" for the
  roster and the resolver.
- **The net-positive invariant** is reworked to a **best-case floor** (§2.4): validate guarantees only
  the *fastest* species' pen nets positive when fully larder-fed; a slow breeder or poor-pasture pen may
  run at a **loss by design** (it pays off only when self-feeding drives upkeep → 0).
- **Wire** (append-only on `HerdTelemetryState`): `penRadius`, `penFootprintTiles` (server in-bounds
  count), `penPastureFraction`, `penExtendProgress`. Convergence gated by
  `core_sim/tests/grazing_2d_pen.rs` (a pen converges at radius 0/1; lush → free, barren → full bill).

**2d-β — the `ExtendPen` command + build ladder** (§4). Growing a pen's fenced footprint is a labor
investment worked off over turns, reusing the corral build ladder — no materials economy:

- **`Command::ExtendPen { faction, target_x, target_y }`** (full proto/runtime/text/server plumbing —
  `ExtendPenCommand` proto field **39** with its `workers` field `reserved`, verb
  `extend_pen <faction> <x> <y>` — it **queues** the ring as `BuildJob::ExtendPen` and names no crew,
  which the band's `builders` pool raises when it reaches the head), routed like
  `Corral`
  through `handle_extend_pen`. It reuses `CommandEventKind::Corral` (one kind for the pen's whole life).
  Validation (each with a clear rejection): a herd **penned exactly at that tile** (`corralled_at`, the
  fixed anchor — *not* the roaming `position()` `corral` keys off), owned by the faction, the faction
  knows **Herding**, `pen_radius < husbandry.pen_radius_max`, **no extension already in flight**, and a
  band is **keeping** it (a Hunt assignment on the herd — else the ring never accrues and an untended
  pen escapes anyway). On success it sets the herd's **`pen_extending`** state via
  `Herd::begin_pen_extension` (which re-checks penned / not-extending / below-max, so the command's
  validation and the mutation can never disagree).
- **The build ladder** rides the corral-tend branch of `advance_labor_allocation`: while `pen_extending`,
  the keeper's HARVEST is **dipped to the `animal:pen` rung's `yield_fraction_while_building`** (the forgone yield *is* the labor
  cost of the ring, the same dip the corral *build* pays), and `Herd::accrue_pen_extension` adds
  that same rung's `progress_per_turn` (0.04 → ~25 turns/ring) to `pen_extend_progress` **after**
  the take. At `1.0` the ring completes: `pen_radius += 1` (saturating at `pen_radius_max`),
  `pen_extend_progress` resets, `pen_extending` clears, and a `Corral` feed line fires; the larger
  footprint's higher K arrives on the next `advance_herds`. The FEED (larder offset) is unchanged while
  extending — self-feeding and the harvest dip are orthogonal.
- **Config:** `husbandry.pen_radius_max` (**2** → up to a 19-tile footprint; validated `>= 1`). The only
  new lever. **`pen_extending`** rides the checkpoint alongside `pen_radius` / `pen_extend_progress`,
  so a rollback rewinds an in-flight extension. `penExtendProgress` on the wire now carries the live ring
  meter (α left it at 0) for a client "Fencing N%" badge.
- **Tests:** `grazing_2d_pen::extend_pen_accrues_a_ring_flips_the_radius_raises_k_and_caps_at_max` (the
  ring accrues over ~25 turns, flips `pen_radius` 0→1, K rises with the 7-tile footprint, and caps at
  `pen_radius_max`); `server::tests::extend_pen_*` (the five validation rejections + the happy path).
- **Deferred (2d-γ, client):** the footprint highlight, the feed-split readout (`penPastureFraction` +
  `penUpkeep`), and the extend affordance / "Fencing N%" badge (`penExtendProgress`).

**2d-δ — the husbandry ceiling: which species climb the ladder** (§4a). Not every animal can be herded,
and not every herdable one can be penned. The ladder is a **sequence** (wild → pastoral → pen), so a
species' reach is a single **enum** (`fauna_config::HusbandryCeiling` = `Wild | Pastoral | Pen`), not
two flags — which makes the incoherent "pennable but not tameable" state unrepresentable (no
`validate()` combo guard).

- **`SpeciesDef.husbandry_ceiling`** (`#[serde(default)]` = `Pen`, so an untagged/future species keeps
  the full ladder) is **cached onto `Herd` at spawn** (mirroring `regrowth_rate`/`fodder_per_biomass`),
  rewound by rollback with the cloned registry, and read by the gates via `Herd::can_domesticate()`
  / `can_pen()`. Roster: **mammoth/deer = `wild`** (hunt-only), **steppe_runner/marsh_grazer =
  `pastoral`** (nomadic herding — follow, don't fence), **boar/rabbit/fowl = `pen`** (pigs/hutches/poultry).
- **Three gates.** (1) **Domestication accrual** — `Herd::accrue_domestication` self-guards on
  `can_domesticate()`, so a `wild` species never tames and never picks up an `owner` (robust regardless
  of call site). (2) **The `domesticate` claim** — `handle_domesticate` rejects a `wild` species
  ("{Species} is wild game — hunt-only…"). (3) **The `corral` / `extend_pen` commands + the `Corral`
  policy accrual** — `validate_improvement`'s `Corral` arm (the one gate `handle_corral` routes through) and
  the `Corral` accrual in `advance_labor_allocation` both require `can_pen()` (only `pen`), so a
  `pastoral` species tames and roams but the pen path is closed ("{Species} cannot be penned").
  `handle_extend_pen` carries the same check belt-and-braces (unreachable via the gated corral path).
- **Wire:** `HerdTelemetryState.husbandryCeiling:string` (`wild`|`pastoral`|`pen`; append-only, mirrors
  `sizeClass`/`ecologyPhase`) so the client can hide the corral/extend affordance on a non-`pen` herd and
  the whole domestication track on a `wild` one.
- **Note — a mid-build gate change:** the `Corral` accrual gate is checked each turn, so a
  (command-unreachable) non-`pen` herd mid-corral-build would simply **stop progressing** — a soft
  stall, not a crash — and there are no shipped saves to carry such a state.

---

