---
paths:
  - "core_sim/src/{mapgen,heightfield,hydrology,climate,biome_palette,map_preset,terrain,grid_utils}.rs"
  - "core_sim/src/systems/worldgen.rs"
  - "core_sim/src/data/map_presets.json"
  - "core_sim/tests/{elevation_authority,climate_authority,hydrology_earthlike}.rs"
  - "core_sim/tests/{navigable_mouth_delta,alpine_headwaters,relief_sweep,lake_abundance}.rs"
  - "core_sim/tests/food_module_reconcile.rs"
---

<!-- Extracted verbatim from lines 112-1063 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# World Generation Pipeline

Implements the procedural map pipeline producing terrain, coasts, rivers/lakes, climate bands, resources, and wildlife spawners. Player-facing framing: manual §3a World Bootstrapping, §3b Terrain Palette.

> ### Elevation is the sole authority
>
> **The land mask is a pure derived function of the heightfield — `land[i] = elevation[i] > sea_level`
> — and is never stored and edited. Any stage that wants to move a coastline writes elevation and
> re-derives.** Guarded by `core_sim/tests/elevation_authority.rs`.
>
> This is not style, it is the fix for a real defect: the mask used to be grown as a boolean blob and
> then repainted by later stages, so the published bathymetry and the published terrain disagreed —
> 543 water tiles sat *above* sea level and 218 land tiles *below* it on a sampled map. A water tile
> above sea level is now **unrepresentable** rather than merely rare, because no stage has a way to
> express one. `target_land_pct` is met by *shaping the field* (`anchor_contour_to_sea_level`), and
> `continents` by the continental bias term — never by repainting tiles.
>
> Consequences a new stage must respect: `place_islands` raises a seamount above sea level and then
> re-derives — it is now the **only** stage that edits a coastline. There is no
> `rebalance_land_ratio`, no tag-solver water branch and no `connect_inland_seas_via_straits` — all
> three were deleted because they corrected an outcome by repainting the map. Design:
> `docs/plan_elevation_authority.md`.
>
> **Legal is not the same as emergent, and the strait carver is why that distinction is written
> down.** `connect_inland_seas_via_straits` found landlocked water and *lowered land until it wasn't*.
> It obeyed the rule above to the letter — it wrote elevation and re-derived — and it was still the
> wrong shape: nothing in the terrain asked for that channel. It was a **topology-repair pass**, the
> same species as the two deletions beside it, and the arc that introduced this callout ported its
> mechanism without re-examining its premise. A stage that decides an outcome and edits the field to
> reach it is repainting, however it writes.

## Pipeline Stages
1. **Macro landmask** - `land[i] = elevation[i] > sea_level`, a pure threshold of the heightfield (`generate_land_mask`, `mapgen.rs`). `target_land_pct` is satisfied upstream by `anchor_contour_to_sea_level` putting that quantile exactly on `sea_level`; `continents` is satisfied by the continental bias term in `build_elevation_field`. **No BFS, no seeds, no area quotas, no jitter** — the pre-`elevation-authority` mask grew weighted-BFS blobs from spaced seeds to fixed per-continent area targets, which is what decoupled terrain from elevation (see the callout above).
2. **Tectonics** - Drift vectors, collision belts, fault seams, volcanic arcs, dome plateaus → mountain mask
3. **Polar microplates** - Subdivide polar tiles, converging vectors raise fold strength
4. **Heightfield** - Multi-octave height raster with erosion smoothing → `elevation_m`
5. **Coastal smoothing** - Blend shoreline tiles via 3×3 blur
6. **Ocean/coasts** - Distance-transform bands: Shelf → Slope → Deep Ocean; plus `InlandSea` for any water the mask leaves unconnected to the ocean — a lake is *classified*, never placed or repaired (see "Lakes are emergent" below). See "Continental shelf width" below — the shelf is a continuous ≥1-tile ring off gentle coasts, gated to deep water at steep/cliff coasts. A **final reconciliation post-pass** (`reconcile_coastal_shelf`, Startup chain after hydrology + tag solver + palette clamp) restamps the shelf so no Deep Ocean touches gentle land on the *final* map, covering coasts created later by deltas/marshes/solver tundra.
7. **Climate** - Temperature (latitude base − elevation lapse + jitter) is computed **first** and the biome band is derived from it via `climate::climate_band_for_temperature` — see "Temperature is the climate authority" below. Latitude is an input to temperature, never a parallel biome gate.
8. **Hydrology** - Rivers on hex **edges** + navigable rivers as water **hexes**. See "Rivers" below. `RiverDelta` is stamped **only here**, at the last **gentle-coast** land hex of each river that ends in a standing water body — the ocean *or* an inland sea/lake (lacustrine deltas). The mouth hex must border that water; the biome picker and tag solver never create deltas (those would scatter them with no river attached). Delta tiles are protected from the tag solver's **reduction *and* addition** passes so genuine river mouths survive — every branch that would restamp a tile carries a `terrain != RiverDelta` guard. This includes the **Fertile-add** branch (both its primary candidate filter and its fallback loop): a delta cut through a **polar/non-fertile** biome lacks the `Fertile` tag, so it is not caught by the Fertile/Water skips and was the one path that clobbered a real mouth back to `AlluvialPlain` (orphaning its `river_channel` bit on dry land). Guarded by `core_sim/tests/navigable_mouth_delta.rs` — the invariant *no hex carries a `river_channel` bit while rendering non-`NavigableRiver`/non-`RiverDelta` terrain*, run through the **real** Startup chain (hydrology → tag solver → palette clamp → reconcile) via `build_headless_app`, so a later-pass clobber cannot hide the way it does in the hydrology-last `hydrology_earthlike.rs` harness.
9. **Biomes** - Stamp `TerrainType` via `terrain_for_position` with micro-variant jitters
10. **Moisture transport** - Humidity blending with a wind-driven rain-shadow pass. A range's shadow is released at its **crest**, so the windward flank and the summit stay wet and the dry corridor starts on the descent — see "A range's rain shadow starts at its CREST" below.
11. **Resources** - Surface deposits biased by `TerrainDefinition.resource_bias`
12. **Wildlife** - Seed herd spawners, migratory paths, `game_density` raster
13. **Starting areas** - Place candidates respecting World Viability Contract

> ### Anything DERIVED from terrain must be reconciled after the last stage that paints terrain
>
> The Startup chain paints terrain in five places (`spawn_initial_world` → `generate_hydrology` →
> `apply_tag_budget_solver` → `apply_biome_palette_clamp` → `reconcile_coastal_shelf`), so a value
> computed from `tile.terrain` **during** that chain is a snapshot of an intermediate map. There are
> now two reconciliation passes at the end of the chain for exactly this reason, and a third derived
> quantity would need a third.
>
> - `reconcile_coastal_shelf` — the shelf (see "Continental shelf width" below).
> - **`reconcile_food_modules`** — the `FoodModuleTag`. `spawn_initial_world` stamped it once from
>   `classify_food_module` on the pre-hydrology biome, and nothing re-read it, so a tile could publish
>   a module describing terrain it no longer had. The player-visible failure (issue #330) was the
>   *missing* case rather than the stale one: `spawn_initial_forage` seeds a `ForagePatch` only on
>   tagged tiles, so a hydrology-stamped `RiverDelta` whose pre-hydrology biome classified to `None`
>   got **no patch at all** and read "No forage" on the richest human ground on the map. Measured over
>   the 6 census seeds: **1,582 tiles mismatched at 80×52** (1,551 stale + 31 untagged) and **2,395 at
>   128×96** (2,357 + 38), concentrated in exactly the biomes hydrology and the solver stamp —
>   AlluvialPlain, PeatHeath, RiverDelta, NavigableRiver, FreshwaterMarsh. The pass runs immediately
>   after `reconcile_coastal_shelf` and immediately before the tag's consumers
>   (`place_wondrous_sites`, `spawn_initial_forage`, `spawn_initial_graze`), re-runs the *same*
>   `classify_food_module` over the final terrain, and inserts/retags/removes to match (an in-place
>   retag keeps the tile's `seasonal_weight` — terrain authors only `module`/`kind`). It also
>   relabels `FoodSiteRegistry` entries, which ride the wire as `foodModules`, and drops entries whose
>   tile now classifies to `None` — but it deliberately **adds no new entries**: *which* tiles are
>   curated is a spatial bucket/quota decision made once in `spawn_initial_world`, and re-running that
>   curation here would move the map's start-site geography. Deterministic (row-major, no `HashMap`
>   iteration, no RNG) and logged as `mapgen.food_modules.reconciled`. Guarded by
>   `core_sim/tests/food_module_reconcile.rs`, which drives the real Startup chain via
>   `build_headless_app` — both directions of the tag invariant, plus the symptom test *every
>   `RiverDelta` has a `ForagePatch`*.

## Gathering markers follow the fresh water (issue #466)

**The curated `FoodSiteRegistry` is not decoration — it is the only ground a player can act on.**
`food_modules` on the wire is the sole source the client's `_forage_compose_available` gate reads
(`DrawerComposeController.gd`), so a hex without a marker offers **no Forage button**; and `sow`
requires a band *already foraging* the tile. So the **~90 markers**, not the ~2,000 food-bearing
hexes, are the real denominator for "can I climb the plant ladder here" — and `plant:field`'s site
rule demands fresh water.

Curation could not see water. It runs inside `spawn_initial_world`, **before `generate_hydrology`**:
at selection time the map has lakes but no rivers, no deltas, no floodplains and no `river_edges`.
Its quality sort (`compare_food_site`) was inert anyway — every tile ships
`INITIAL_SEASONAL_WEIGHT = 1.0` — so *which* hex inside a spatial bucket won a marker was arbitrary
with respect to the one property rung 3 cares about. Measured before the fix: **33.8 of 90 markers
sowable** (mean, 6 seeds).

**`bias_food_sites_toward_fresh_water`** (`systems/worldgen.rs`) re-ranks the result over the final
terrain. Registered in the Startup chain immediately after `reconcile_food_modules` — which is what
guarantees every surviving entry still classifies to a real module — and before anything publishes
the list.

- **Site quality = `tile_forage_capacity(tile) + fresh_water_site_weight × (tile is fresh-watered)`**,
  read through the *same* `tile_forage_capacity` / `tile_is_fresh_watered` seams the patch seeding and
  the `plant:field` gate use, never a private table. Expressed in **capacity units** so the two terms
  are directly comparable: a watered tile outranks a dry one carrying up to `weight` more forage, and
  richer watered ground still outranks poorer watered ground.
- **RE-RANK ONLY — it relocates, it never adds or drops.** Every move stays inside the marker's **own
  spatial bucket** and re-checks `min_site_spacing` against every other marker, so the budget, the
  per-bucket quota and the latitude spread are preserved by construction. Gathering stays exactly as
  scarce as it was; it just sits on the river valleys. (Growing the budget is a separate decision, and
  its dial is `max_total_sites`.)
- **Why a separate pass rather than fixing curation in place.** Curation cannot simply move later:
  `best_start_tile` consumes the curated list and the starting population is spawned around its answer,
  all inside `spawn_initial_world`. Leaving selection where it is keeps start geography untouched and
  confines the change to a re-rank.
- **Deterministic**: candidates scored once, sorted score-DESC with an explicit `(y, x)` tie-break,
  markers visited in list order; no `HashMap` iteration, no RNG.
- **`fresh_water_site_weight = 0.0` is a provable no-op** — a marker's own tile is in its own candidate
  set, so nothing can outscore staying put. That is the A/B control every measurement here is taken
  against.
- **Measured** (6 seeds, 80×52 earthlike, through `build_headless_app`): sowable markers
  **33.8 → 58.7 (1.73×)**, marker count unchanged at 90 on every seed. Nearest sowable marker to the
  start tile is 0–4 hexes both before and after — the bias is about *how much* farmable ground a player
  meets, not about rescuing a start that had none. **One seed regressed** on that distance (777777777,
  1 → 4 hexes): a re-rank optimises marker quality, not proximity to spawn, and nothing pins the latter.
- Guarded by `core_sim/tests/food_site_water_bias.rs` — the A/B relation (never a literal count), the
  count-is-unchanged invariant, legality + spacing of relocated markers, the zero-weight no-op, and
  build-to-build determinism. Plus an `#[ignore]`d `gathering_marker_census`.

**Config** (`snapshot_overlays_config.json` → `food`):

| Key | Default | Meaning |
|---|---|---|
| `fresh_water_site_weight` | **60.0** | What fresh water is worth to a marker, in forage-capacity units. `0.0` disables the pass entirely. |
| `land_tiles_per_site` | 120 | Land tiles per marker once the area scaling binds. Was a bare literal in `spawn_initial_world`. |
| `min_scaled_sites` | 24 | Floor under the area-scaled budget. Was a bare literal. |

> **The site budget is a hybrid, and on the standard map the area scaling is DEAD.**
> `target_total = max(max_total_sites, max(land_tiles / land_tiles_per_site, min_scaled_sites))`. At
> 80×52 there are ~1,580 land tiles → `max(13, 24) = 24`, so the flat **`max_total_sites` (90)** wins.
> The ratio only takes over past ~10,800 land tiles (roughly a 192×148 grid). So "markers scale with
> map size" is true only of large maps; below that it is a flat number.

## Data Shapes
- **Rasters**: `elevation_m: i16`, `climate_band: u8`, `game_density: u8` (the square-8 hex `flow_dir` / `flow_accum` rasters are **deleted** — hydrology routes on the corner graph, see "Rivers")
- **Vectors**: `rivers: [RiverSegment]` — per-edge `RiverEdge { hex, dir, class, discharge: f32 }` chains + a navigable hex tail (see "Rivers")
- **Tiles**: `hydrology_id`, `substrate_material`, `terrain_type`, `TerrainTags`, `river_edges: u16`

## Rivers — a real drainage network on hex EDGES, with a class that grows downstream (`hydrology.rs`)

A river is **not** a polyline through hex centers. Minor/Major rivers run **along hex edges** (so a
future movement system can charge a crossing penalty on exactly the side the river is on), and a
river that outgrows the edge model becomes **water terrain**.

The **routing and extraction** are a real drainage network: steepest descent on a depression-filled,
precipitation-weighted elevation surface, decomposed into main stems and tributaries. Designs:
`docs/plan_rivers.md` (the edge/class/navigable *model*) and
`docs/plan_rivers_drainage_network.md` (the *network* that model expresses).

- **The corner graph.** The dual of "flow along edges" is "route between corners": every
  corner→corner step traverses exactly one hex edge. On a pointy-top odd-r grid each corner is
  shared by exactly 3 hexes, so `V = 6F/3 = 2F` — **two corners per hex**, indexed `(hex_x, hex_y,
  slot)` with `slot ∈ {TOP, BOTTOM}`. Each corner has 3 neighbour corners. A **border corner** (its
  3 hexes are not all on the map) is excluded from routing. Every hex step goes through
  `grid_utils::hex_neighbor`, so horizontal wrap is honored. Corner **elevation is the mean** of its
  3 hexes (not the min — the mean puts a corner low in the *trough* between two low hexes, so rivers
  settle into valleys) **plus a deterministic flat-tie jitter** (below). A corner is a **sink** iff
  any of its 3 hexes is an **OCEAN** hex (`WATER` *without* `FRESHWATER`) — see "Lakes flow through".
- **Canonical edges.** An edge `(H, d)` has two representations — `(H, d)` and `(neighbor,
  opposite(d))`. The canonical one is whichever has `dir ∈ {E, SE, SW}` (`canonical_edge`), so an
  edge has a single key regardless of which hex traced it. An edge exists only if **both** its hexes
  are on the map.
- **The flow field descends the LANDSCAPE, not a cost-to-sea distance transform** (`docs/plan_rivers_drainage_network.md`).
  1. **Jittered elevation.** Corner elevation gets `river_flat_jitter × (hash01(world_seed, corner) − 0.5)`
     — a pure splitmix64 hash, no RNG, no `HashMap`. Pure steepest descent on a plateau picks the same
     direction for every corner and carves artificial parallel channels; the jitter breaks those ties
     into a natural branching pattern, reproducibly. It is `≫ river_fill_epsilon` and `≪` real relief,
     so it decides only ties the terrain does not.
  2. **Priority-flood depression fill** (Barnes + epsilon): seed a min-heap with every sink at its own
     elevation and raise each neighbour to `max(elev[n], filled[popped] + river_fill_epsilon)`. Every
     non-sink corner ends **strictly above** the corner that flooded it, so a **strict descent to a
     sink always exists** — including across the flats of a filled depression, where a naive fill
     stalls. Unreachable corners keep `filled = INFINITY`.
  3. **Downstream = steepest descent on `filled`.** All 3 corner steps are the same length on a regular
     lattice, so "steepest" is simply "lowest filled neighbour"; ties break by corner index ascending.
  4. **Precipitation-weighted accumulation.** Each corner seeds
     `(river_base_runoff + river_moisture_weight × precip) / 2`, where `precip` is the mean of its 3
     hexes' `MoistureRaster` value. Dividing by the 2 corners-per-hex makes **discharge read directly
     as precipitation-weighted upstream drainage area, in HEX-EQUIVALENTS** — a fully-wet hex
     contributes exactly `1.0`. That is the unit the class thresholds live in, which is why they are
     **absolute and map-size independent**. A missing/mis-sized `MoistureRaster` falls back to uniform
     `precip = 1.0` with a warning (never a panic).
- **Extraction: main-stem decomposition, not N independent rivers.** `channel_min =
  river_channel_min_discharge / river_density`; a corner is a **channel** iff it is routable, not a
  sink, and `accumulation ≥ channel_min`. Accumulation is monotone non-decreasing downstream, so the
  channel corners + their descent links form a **forest of trees rooted at outlets, by construction** —
  nothing to reject, space, or count-target. Each outlet (largest first) is then walked **upstream**,
  always taking the largest unclaimed contributor: that path is the classic **main stem** ("the
  Missouri joins the Mississippi"), and every contributor it passes over becomes a tributary stem
  joining at exactly the corner it was passed over at. Every channel corner lands in exactly one river.
  - *Upstream-from-the-outlet, not downstream-from-headwaters*: every headwater's accumulation is
    barely above `channel_min` (nothing upstream of it is a channel), so "the biggest headwater" does
    **not** identify the main stem — but "always take the biggest contributor, walking up from the
    mouth" does, by definition.
  - A stem's final edge (`last corner → terminus`) is what makes a main stem **touch the shore** (the
    terminus is the ocean-touching sink corner) and a tributary **land on its trunk** (the terminus is
    a claimed corner of the parent stem). One uniform rule, no special case.
  - **Strahler order is computed on the real channel tree** (a channel corner with no channel
    contributors is order 1; otherwise `max(contributor orders)`, +1 iff ≥2 share that max) — where it
    is actually defined. The old per-tile computation on the hex flow field is gone.
  - `river_min_length` (in hexes) is the **only** noise gate left: an emitted river shorter than it is
    dropped. There is no spacing, no count target, no source category, and no acceptance loop.
- **Lakes FLOW THROUGH — only the ocean is a sink.** A lake / `InlandSea` corner is an ordinary low
  corner: the fill raises it to its lowest saddle and it **spills**, so the whole upstream catchment
  carries *through* the lake and out a genuine outlet. Real outlet rivers, and a big river below a big
  lake, fall out for free (replacing the old `lake_heads` hack). Two consequences:
  - **A river ENDS at standing water and CONNECTS to it; a new river begins where terrain drains out.**
    The run emits the **first water-touching edge as the mouth** (the connecting edge that reaches the
    water) and terminates there; the *rest* of the consecutive water-touching edges (the shore-hug + the
    submerged stretch) are **skipped, not drawn**, and a new run resumes at the next dry edge. So there
    is exactly **one water-touching edge per river and it is the LAST one** — the river runs *into* the
    lake/sea/trunk and stops rather than hugging the shore, and the drain-out below re-emerges as its own
    segment (connected on its source side, its first corner being water-adjacent). "Standing water" is a
    lake / inland sea / ocean on the terrain map **or** a previously-stamped navigable trunk
    (`StemEmitter::edge_touches_water`, reading `is_water_hex` + `existing_navigable`). The original
    both-banks rule hugged the lakeshore ("V" up a trunk hex); the first fix over-corrected and *dropped*
    the water-touching edge, leaving a visible **gap one step short of the water** — the current rule
    draws the mouth and skips only the shore-hug. The accumulation still flows through underneath
    (discharge/class unchanged), so the outlet stays a big river below a big lake and can independently go
    navigable again below it — **only the rendered segmentation changes.** The split is also required
    because a segment's edge chain and navigable chain are both *paths* — a chain with a water-shaped hole
    in it would be neither contiguous nor drawable. Guarded by
    `hydrology_earthlike::edge_rivers_terminate_at_water_not_along_it` (a river has **at most one**
    terrain-water-touching edge and it is the **last** — the mouth — so no river runs along a shore; the
    navigable-trunk "V" and the shore-hug tile proxy are tracked by the `drainage_census`).
  - **A navigable river must CONNECT to water, or it isn't navigable.** After the split a navigable chain
    must end at the water it connects to (its last hex is standing water, or hex-adjacent to it —
    `StemEmitter::navigable_reaches_water`). A chain that **dead-ends on dry land** (an endorheic run with
    no ocean) is **demoted to the river's edge (Major) form** — re-traced with the navigable model off,
    so the river survives on the edge model rather than stranding a landlocked navigable dead-end. A
    navigable run shorter than **`river_navigable_min_hexes`** (a 1- or 2-hex puddle) is demoted the same
    way. Both demotions run in `StemEmitter::emit_run`; guarded by
    `hydrology_earthlike::navigable_rivers_connect_to_water` (every navigable run reaches standing water
    and is ≥ the lever, swept over `CENSUS_SEEDS`). Aggregate over the 6-seed sweep: **14 navigable
    segments / 68 hexes, min run 3, max run 22, 0 landlocked, all mouth-connected** (the `drainage_census`
    now reports the landlocked count, the run histogram, and the mouth-connection count).
  - **Deltas are PER-TRANSITION, not per-terminus.** A river now both *enters* a standing water body
    and *leaves* it, so the delta scan stamps a delta at **every land→standing-water transition** along
    the river's ordered hex path (plus the mouth, where the path simply ends against the water) — each
    still **gentle-coast gated** and still required to actually border that water. A lacustrine delta
    and the ocean delta are different tiles on the same river. A delta may never take a **mid-chain**
    navigable hex (the channel flows through it; turning it into depositional land would break the
    chain in two).
- **Class is PER-EDGE and grows downstream.** `RiverEdge.discharge` = the corner accumulation at the
  edge's **upstream** corner, which is monotonically non-decreasing downstream — so a river is
  `Minor` at its headwater and `Major` in its lower course, never uniformly wide. `RiverClass`
  (`sim_runtime`) is `None = 0 | Minor = 1 | Major = 2`; **value 3 is reserved** — "navigable" is
  deliberately *not* a class (see below).
- **Navigable rivers are WATER TERRAIN, not edges.** Once discharge crosses
  `river_class_navigable_min_discharge` the river stops emitting edges: the lower **dry** of the two
  hexes flanking the **last emitted edge** becomes the first hex of a `TerrainType::NavigableRiver`
  chain, and the rest of the chain is read straight off the river's **own corner path** — the hex the
  channel is inside at each remaining step (`RiverSegment.navigable_hexes`). Consecutive steps share a
  corner and the three hexes at a corner are pairwise adjacent, so the chain is **contiguous by
  construction**. Two rules keep it a *simple path*: **sticky** (while the current hex still flanks the
  edge being crossed, the river has not left it) and **no self-crossing** (a channel that would double
  back onto a hex it already occupies ends there — a corner path never revisits a corner, but a *hex*
  is touched by many corners, so the hex path can). A giant river is
  a body of water you need a boat to enter, so it reuses every existing water mechanic.
  `NavigableRiver` mirrors `InlandSea` exactly (`WATER | FRESHWATER`, same movement/logistics/
  attrition profile), is in the biome palette's `must_have` set, and is protected from the tag
  solver's water-reduction pass — like `RiverDelta`, otherwise the solver would erase real rivers.
  - **A navigable hex is a valley with a river in it — it keeps the biome it cut.** The stamp
    (`hydrology.rs`) captures the pre-stamp biome into `Tile::underlying_terrain: Option<TerrainType>`
    *before* overwriting `terrain`/`terrain_tags` with `NavigableRiver`, so the tile stays
    **mechanically** water (movement/naval/logistics/attrition/tags/palette all keep keying on
    `terrain == NavigableRiver`, untouched) but its **RESOURCE** reads route through
    `Tile::resource_terrain()` (= `underlying_terrain` on a navigable hex, `terrain` everywhere else).
    So a giant river yields the valley it runs through, not open water: **forage** = the underlying
    biome's `forage.capacity_by_biome` **plus `forage.navigable_river_forage_bonus`** (default **80.0**,
    `labor_config.json` — a navigable river is always a fishery, so a navigable hex *always* seeds a
    forage patch, even over an otherwise-barren biome, at just the bonus there); **graze** = the plain
    underlying `graze.capacity_by_biome` (no bonus — you don't pasture on the channel; a navigable-over-
    grassland hex grazes like grassland). One shared helper `forage::tile_forage_capacity` sizes the
    seeded patch AND the wire's `forageCapacity`, so they can't drift. The `NavigableRiver` rows in
    `labor_config.json` (forage 130) and `fauna_config.json` (graze 0) are now **vestigial** (bypassed
    by the underlying-terrain routing; left only to keep the tables total). Exported as
    `TileState.underlyingTerrain:TerrainType` (append-only, = `resource_terrain()`, so it is the "real
    ground" biome always; the client consults it only when `terrain == NavigableRiver`).
  - **The join invariant: the edge chain and the hex chain share an EDGE, never a bare corner.**
    The hand-off anchors on the last **emitted** edge, *not* on the un-emitted edge whose discharge
    crossed the threshold. Both are incident to the same corner and **three hexes meet at a corner**,
    so anchoring on the un-emitted one could pick the third hex — one the edge chain never touches.
    The chains then met only at a point, the first navigable hex carried **no `river_edges` bits at
    all**, and a tributary visibly dead-ended at the trunk in the client. Anchoring on the last
    emitted edge makes the shared edge true by construction, so the first navigable hex always
    carries that edge's class in its mask. Guarded by
    `hydrology_earthlike::navigable_chain_joins_the_edge_chain_on_a_shared_edge` (asserts the shared
    edge *and* the resulting tile mask across a 6-seed sweep) and the
    `the_navigable_handoff_anchors_on_the_last_emitted_edge` unit test. A river that goes navigable
    on its very first step has emitted nothing to anchor to, so it falls back to the edge it stopped
    at.
  - `hex_contiguous_chain` survives as a belt-and-braces bridge (a waterway whose hexes don't touch is
    not a waterway), but the corner-path construction above already makes it an identity.
  - **Rivers MERGE ON CONTACT — a navigable river is a path, not a blob** (`truncate_at_existing_channel`).
    Stems are emitted **main-stem-first**, so a tributary that reaches its trunk finds it already
    stamped and **joins** it rather than digging a second channel alongside it: the first hex that is
    an already-stamped chain's hex **or adjacent to one** terminates the chain **on that trunk hex**
    (contact is adjacency, not identity — two water hexes that touch are one body of water). The
    confluence is a genuine shared chain hex, so both chains' `river_channel` bits meet there.
    (Historically the un-concentrated flow accumulation made *several branches of one drainage* cross
    the navigable threshold independently and each trace its own chain to the same sink, packing into a
    2–4 hex wide **blob**; with a real drainage tree the branches now merge *upstream* of the threshold
    in the first place, and merge-on-contact is the backstop.)
  - **The path invariant is asserted on the CHANNEL-EXIT MASK, not on terrain adjacency**
    (`hydrology_earthlike::navigable_rivers_are_paths_not_blobs`, swept over `CENSUS_SEEDS` +
    `BLOB_REGRESSION_SEED`): a mid-chain hex links to exactly **2** channel neighbours, an endpoint to
    **1**, a confluence to **3**; 4+ is a 2D water body. *Terrain* adjacency cannot express this — a hex
    chain that turns 60° puts hex `k` adjacent to hex `k+2` (the three hexes at a bend are mutually
    adjacent, unavoidably), so a bending chain with a tributary merging at the bend **touches** 4
    navigable hexes while remaining a perfectly good path. Terrain adjacency is still bounded, at the
    geometric ceiling a chain can reach (2 chain links + one bend skip-adjacency + one merging
    tributary = 4).
  - The chain's **mouth is a `RiverDelta`**, not open water — a river deposits its load where it
    meets the sea — so the delta contract is unchanged.
- **The gameplay primitive: `Tile.river_edges: u16`** — 2 bits per odd-r direction
  (`class = (river_edges >> (2 * dir)) & 0b11`), populated for **both** hexes flanking every river
  edge, so a hex and its neighbour always agree about the river between them. Helpers:
  `river_class_on_side(dir)` / `set_river_class_on_side(dir, class)` / `has_any_river_edge()`. This
  is what a movement system will read: *entering hex H across direction d crosses
  `H.river_class_on_side(d)`*. **Nothing consumes it yet — that is expected**; movement and fertility
  effects are a follow-up. Exported on the wire as `TileState.riverEdges:ushort`.
- **Where the tributary meets the trunk: `Tile.river_inflow: u16`** — the same 2-bits-per-slot
  packing as `river_edges`, but keyed by hex **CORNER** instead of by side. An edge river runs
  *along* a side, corner to corner, so it does not end mid-edge — **it ends at a vertex**, and that
  vertex is where the water enters the navigable hex. The edge mask cannot say where: a trunk hex
  can flank three river edges (the tributary ran along three of its sides before going navigable),
  which leaves two candidate chain-ends, so the client would be guessing and would draw an arm per
  edge. So the sim states it.
  - **Corner index convention (a wire contract).** Corner `i` is the vertex at screen angle
    `60*i + 30`, **+y down** — matching the client's `MapView._hex_points`: `0` lower-right,
    `1` bottom, `2` lower-left, `3` upper-left, `4` top, `5` upper-right. Mapped onto the sim's
    `(hex, TOP|BOTTOM)` corner model by `HEX_CORNER_LAYOUT` /
    `HexGrid::local_corner_index(hex, corner)` (`hydrology.rs`): `0 = TOP(SE(H))`, `1 = BOTTOM(H)`,
    `2 = TOP(SW(H))`, `3 = BOTTOM(NW(H))`, `4 = TOP(H)`, `5 = BOTTOM(NE(H))`. Side `dir` spans
    corners `{dir - 1, dir}` (`grid_utils::hex_edge_corner_indices`).
  - **Both tables are pinned ABSOLUTELY to the client's geometry, not merely to themselves.**
    `local_corner_index_is_a_bijection_on_every_hex` / `hex_edge_corner_indices_match_the_corner_model`
    only prove *internal consistency* (six distinct corners that round-trip) — **a table rotated by one
    position passes both happily** while putting every tributary on the wrong vertex. So
    `hex_corner_layout_matches_the_clients_corner_geometry` and
    `hex_edge_corner_indices_are_the_shared_edges_endpoints` (`hydrology.rs` tests) compute each
    corner's **world position** twice — once through the sim's `(hex, TOP|BOTTOM)` model (centre at
    `x = √3·R·(col + 0.5·(row&1))`, `y = 1.5·R·row`; `TOP = centre + (0,−R)`, `BOTTOM = centre +
    (0,+R)`, +y down) and once through the client's `corner i at angle 60i + 30` circle — and assert
    the two land on the same point. That is what makes the convention a *contract* rather than a
    convention.
  - **The semantics WIDENED with the drainage network** (`docs/plan_rivers_drainage_network.md` §A).
    `river_inflow` no longer means *"this hex is a navigable chain HEAD"* — it means **"a tributary
    hands over to the channel at this vertex."** Same field, same bits, same corner convention, same
    widest-wins rule; only the *meaning* widened. Two hand-overs are recorded:
    1. a river that **outgrows the edge model itself** hands over at the head of its own navigable
       chain (the old case), and
    2. an **edge-only tributary that lands on a navigable trunk** hands over at a vertex of that
       **trunk hex — mid-chain**. That is impossible without a real network (before it, tributaries
       could only meet a trunk at its head), and it is *the* payoff: without recording it, the
       tributary's edge band ends at a bare vertex while the trunk's arms only reach its edge
       *midpoints*, and the tributary visibly dead-ends short of the water it feeds.
    Both carry the class of the **last emitted edge** (the tributary's own width where it arrives). A
    river navigable from its first step emitted no edges, has no tributary, and reports `0` — no
    invented inflow. `RiverInflow` now carries the target `hex` alongside the `corner`/`class`.
  - **The render contract: `river_channel` is load-bearing for the head/mid-chain distinction.**
    The client cannot key its head-taper off `inflow != 0` any more — that was safe only while inflow
    *meant* "chain head". It now **popcounts the `river_channel` exit bits**: **1 exit = a genuine
    chain head** (taper the channel to a point), **≥ 2 = mid-chain** (full width — no hourglass at a
    tributary junction), **3 = a confluence**. The inflow spur is drawn unconditionally. So the
    channel mask is no longer only anti-web link topology: **the sim must keep its exit count exactly
    equal to the chain's real degree at every navigable hex**, or the trunk pinches or bulges in the
    render. Both halves are landed and verified (client: `terrain_blend.gdshader` + the
    `map_rivers_midchain` ui_preview fixture).
  - **Widest-wins on collision.** Three hexes meet at a corner, so two tributaries running down
    either bank can hand over at the *same* vertex of the same hex (a confluence at a corner). One
    slot holds one class, so `widen_tile_river_class` keeps the wider (`Major` > `Minor`), which is
    also emission-order independent.
  - Helpers: `river_class_at_corner(corner)` / `set_river_class_at_corner(corner, class)` /
    `has_any_river_inflow()`. Exported as `TileState.riverInflow:ushort`. Guarded by
    `hydrology_earthlike::every_river_inflow_is_a_real_tributary_handover_vertex` — the tile's inflow
    corners are exactly the hand-overs arriving there, at the widest arriving class, each an endpoint
    of its river's last emitted edge (checked by the **hex triple** that identifies the vertex, so a
    wrong corner cannot pass), and **mid-chain hand-overs must exist** (if none happen, the network is
    still a set of parallel rivers).
- **The trunk channel is a PATH: `Tile.river_channel: u8`** — **1 bit per odd-r direction**
  (`exits(dir) = (river_channel >> dir) & 1`, `RiverChannel::{BITS_PER_DIR, SLOT_MASK}` in
  `sim_schema`): does this hex's navigable channel flow out through side `dir`? Helpers:
  `channel_exits(dir)` / `set_channel_exit(dir)` / `has_any_channel_exit()`.
  - **Why it must exist.** A navigable river is a chain of water *hexes*, and a chain is a **path** —
    a hex links to its upstream and downstream neighbours and to nothing else. **Terrain cannot say
    which those are.** The client used to arm an arm from each navigable hex's centre to *every*
    neighbour that was navigable/water/`RiverDelta`, so wherever two chains ran adjacent (which,
    before merge-on-contact, was everywhere) or a chain doubled back, every hex cross-linked to every
    navigable neighbour and the trunk rendered as a **web with triangular holes** instead of a river.
    Only the tracer knows chain membership, so the sim states it. (Merge-on-contact removes most
    adjacent chains, but the mask is still the right primitive: two *legitimate* parallel rivers, or a
    bending chain, would cross-link without it.)
  - **Populated from each `RiverSegment.navigable_hexes` chain** in `generate_hydrology`, in two
    passes so the result is independent of trace order. **Pass 1 — the chain:** for each consecutive
    pair, the exit bit is set on **both** hexes facing each other (hex `A` → dir toward `B`, hex `B` →
    the opposite dir), symmetric exactly like `river_edges`. **Pass 2 — the mouth:** a chain's final
    hex also exits toward the water it drains into (the ocean, an inland sea, or the `RiverDelta` at
    its own mouth), or the drawn river would stop one hex short of the sea. That mouth bit is the one
    **asymmetric** bit in the mask — open water carries no channel of its own, so it is not mirrored
    back. Only a genuine **dead end** earns it: a tributary that merged into a trunk also *ends* on its
    last hex, but that hex is a confluence the water already flows on through, and a second exit there
    would draw a spurious arm off the side of the trunk ("has no exit but the one back upstream" is
    the test, and it does not depend on segment order).
  - The **head** needs no exit toward its tributary — the inflow SPUR (`river_inflow`) already draws
    that; double-encoding it would put two arms on one vertex. A hex on two chains (a confluence)
    accumulates the **union** of the bits (OR-ed, never overwritten).
  - Exported as `TileState.riverChannel:ubyte`. Guarded by
    `hydrology_earthlike::navigable_channel_exits_are_the_chain_and_only_the_chain`: symmetry,
    end-to-end chain connectivity, every chain reaching its water, and the **anti-web invariant** — *no
    navigable hex exits toward a navigable hex that no chain actually runs between*.
- **Wire format.** The `HydrologyOverlay` / `RiverSegment` / `HydrologyPoint` polyline tables are
  **deleted** from the snapshot and delta. The per-tile `riverEdges` + `riverInflow` + `riverChannel`
  masks plus the `NavigableRiver` terrain fully determine the render, so a parallel polyline overlay
  would be duplicated state. The client draws the trunk channel from **`riverChannel`** (arming *only*
  the sides whose bit is set — never inferring links from terrain), the edge rivers from `riverEdges`,
  and joins a tributary to its trunk hex at the `riverInflow` **corner** — never at a side midpoint,
  and never one arm per flanked edge.
- **Delta placement is gentle-coast gated.** A delta is a depositional fan, so it only forms where
  the river meets the water across low ground — reusing the shelf's own
  `ShelfConfig.coast_height_threshold` rather than inventing a second threshold. A river that meets
  the sea at a cliff has no delta (it is an estuary). This also keeps `reconcile_coastal_shelf`'s
  "no DeepOcean touches gentle land" invariant coherent: every delta is gentle land, so every delta
  gets a shelf seaward of it.
- **Config** (`hydrology` block of `simulation_config.json` → `HydrologyOverrides`, overriding the
  per-preset `river_*` keys in `map_presets.json` — overrides > preset > default):

  | Key | Default | Meaning |
  |---|---|---|
  | `river_density` | 1.0 | How wet the map reads. A **multiplier on the channel threshold**: `effective = river_channel_min_discharge / river_density` (higher density → lower threshold → more channels). Clamped to `[0.1, 5.0]`. |
  | `river_fill_epsilon` | 1e-5 | The depression fill's drainage gradient across flats. Far above `f32` noise at map elevations (~1e-7), far below the jitter. |
  | `river_flat_jitter` | 5e-4 | Elevation tie-break amplitude. **Must stay `≫ river_fill_epsilon`** (so it decides ties the fill cannot) **and `≪` real relief** (so it can never reorder genuine terrain). |
  | `river_base_runoff` | 0.2 | Per-hex runoff floor, so an arid basin still trickles. |
  | `river_moisture_weight` | 0.8 | How hard rainfall drives discharge. With `base_runoff = 0.2` a fully-wet hex contributes exactly **1.0** — which is what makes discharge read as hex-equivalents. |
  | `river_channel_min_discharge` | **3.0** | The network-extraction threshold. |
  | `river_class_major_min_discharge` | **12.0** | Minor → Major. |
  | `river_class_navigable_min_discharge` | **25.0** | Major → `NavigableRiver` hex chain. |
  | `river_navigable_enabled` | true | Kill switch for the navigable tail. |
  | `river_navigable_min_hexes` (`navigable_min_hexes` in the override block) | **3** | Shortest navigable hex chain that still reads as a river; a shorter run is demoted to the edge (`Major`) form (a 1–2 hex navigable is a puddle). |
  | `river_min_length` (`min_length` in the override block) | 2 hexes | The **only** noise gate. Keep it low. |

  **The three discharge thresholds are `f32` and ABSOLUTE.** Discharge means *precipitation-weighted
  upstream drainage area in hex-equivalents*, so a river draining 300 wet hex-equivalents is a big
  river on an 80×52 map and on a 256×192 map alike; a bigger map simply has more of them and longer
  ones. Do **not** re-express them as a fraction of the map maximum — one giant basin would skew it.

  **Determinism** is guarded by `integration_tests/tests/determinism.rs`: no `HashMap`/`HashSet`
  iteration order in the routing or extraction, no unseeded RNG, every sort has an explicit index
  tie-break, and the flat jitter is a pure hash of `(world_seed, corner_index)`.

  **The three discharge thresholds were tuned from a 45-cell sweep**, not guessed:
  `hydrology_earthlike::drainage_threshold_sweep` (`#[ignore]`d) crosses
  `channel × major × navigable` over `CENSUS_SEEDS` and reports rivers/edges/class-split/navigable
  runs per cell. Re-run the sweep before changing any of the three. **They were NOT re-tuned for the
  erosion pass** (below) — they were deliberately held fixed so the erosion A/B is attributable.

  **Measured** shape at those thresholds, on the **eroded** landscape
  (`hydrology_earthlike::drainage_census`, `#[ignore]`d; run with `-- --ignored --nocapture`),
  aggregate over 6 seeds of an 80×52 earthlike map (after the "connect to the mouth + demote landlocked/
  puddle navigable" fix): **14.5 rivers per map**, 81.1% Minor / 18.9% Major, **~2.3 navigable segments
  / ~11 navigable hexes per map** (14 segments / 68 hexes over the 6-seed sweep, min run 3, 0 landlocked
  — the shore-hugging false chains, the landlocked dead-ends, and the 1–2 hex puddles are all gone);
  land-corner accumulation p50 = 0.60 / p95 = 10.2 / p99 = 64.4 / **max 587**; corner confluences
  **11.6%** of land corners (4.1% before the drainage-network rewrite); Strahler on the drainage tree
  o1 = 12366, o2 = 2246, o3 = 837, o4 = 254, o5 = 34 (the accumulation/confluence/Strahler figures read
  off the corner network, which the segmentation fix does not touch). Per-seed spread is large and
  *should* be — see the verdict below.

  > **These figures are PRE-`elevation-authority` and no longer describe an 80×52 map.** Post-arc,
  > that size yields **zero navigable rivers on every seed**, and land-corner accumulation maxes at
  > **10–20**, not 587. This is **not a regression** — measured, the largest basin is ~5% of its
  > landmass both before and after the arc (the drainage surface is ~95% divided into small
  > independent basins either way). What changed is *landmass size*: the old BFS grew an accidental
  > ~1,580-tile supercontinent at 80×52 while the preset asked for 4 continents, and that surplus area
  > was the only thing clearing the navigable discharge threshold of 25.0. It cleared it as a
  > **lottery** — pre-arc counts across six seeds were `0, 1, 1, 6, 5, 1`, with one 41.7%-basin
  > outlier seed carrying most of them. The arc removed the bug that was masking a pre-existing
  > drainage deficiency.
  >
  > **Update (the divides arc).** The dome has been replaced by a warped / tilted / ridged envelope
  > (see `macro_land` below). At 80×52 navigable rivers now appear on **3 of 6 seeds** rather than 1,
  > and the standard map carries **49 sowable tiles** *(as counted by `relief_sweep`'s partial-chain
  > harness — the real Startup chain reads **174**; see "the '49 sowable tiles' was wrong" in
  > `cultivation.md`. The A/B here is still valid, since both arms use the same harness)* — but the
  > **coherence ratio is unchanged**, and
  > the measured reason is geometric, not tuning: with a mean depth-to-coast of ~2.9 tiles the largest
  > landmass has roughly one ocean-touching (⇒ sink) corner per two interior corners, so a basin
  > cannot grow long enough to clear a discharge of 25 except by luck. **Landmass area remains the
  > binding constraint at this grid size.**
  >
  > **A related correction:** the claim that the arc took sowable tiles "46 → 0" was a **test-harness
  > defect, not a worldgen result**. `core_sim/tests/forage_field.rs` never ran `generate_hydrology`,
  > so its map had no rivers, no `RiverDelta`, and no `river_edges` — and `plant:field`'s site rule
  > requires fresh water, which on that map nothing could satisfy. The harness now runs hydrology and
  > pins its own grid; see the test split below.
  >
  > Consequently the navigable structural invariants run against a **river-capable fixture** at
  > `NAVIGABLE_FIXTURE_GRID` = **128×96** (shipped presets, `continents: 4`, only the grid differs) —
  > the smallest grid producing navigable rivers on every seed (5–9 each). A sweep over
  > 80×52 / 128×96 / 192×128 / 256×192 × `continents` 4 / 2 / 1 showed `continents` barely moves the
  > result, confirming **landmass area** is the binding constraint. See `hydrology_earthlike.rs`.
  >
  > **Do not "fix" a dry map by lowering `river_class_navigable_min_discharge`** or any hydrology
  > threshold. Rivers are emergent; forcing a fixed river share onto whatever terrain exists is the
  > repaint-to-hit-a-quota pattern `elevation-authority` deleted. The input to change is basin
  > coherence or landmass size — issue #261, "Capture: move the divides, not the valleys".

## Fluvial erosion — the heightfield the drainage runs on
The drainage-network rewrite left the *router* correct and the *landscape* wrong: continents were
**sponges** (48–64% of a continent's tiles touched water, because the coastline is an iso-contour of
fractal noise) and they **shed radially** with no trunk valleys to capture drainage across a divide.
`heightfield::apply_fluvial_erosion` attacks the landscape directly, at the end of
`build_elevation_field` — **before** `mapgen::generate_land_mask`, which is the whole point: the mask
is a pure threshold of this field, so the coastline **is** a level set of it, and reshaping the field
reshapes the coast.

> Since `elevation-authority` this is *literally* true rather than approximately so. The passage
> above used to read "the mask **ranks** tiles by elevation" — it ranked them by
> `elevation + macro_land.jitter × noise`, which is a **reordering**, so the coastline was a rank
> contour over a jittered score and not a level set of anything. The conclusion held only by luck.
> `jitter` is retired; its coastline raggedness now lives in the field itself as
> `macro_land.coastline_roughness`, applied *before* `land_contour`, where it perturbs the shoreline
> without decoupling the mask from the surface.

- **The model** is the classic landscape-evolution equation minus uplift: `∂z/∂t = D∇²z − K·A^m·S^n`,
  iterated on the **square raster** (D8 — the hex/corner graph is hydrology's and stays there). Per
  pass: priority-flood the depressions (+`fill_epsilon`), route D8 steepest descent on the *filled*
  surface, accumulate **uniform** unit drainage (this is landscape evolution, *not* the
  precipitation-weighted discharge model), incise, then diffuse. Deterministic: pure arithmetic, no
  RNG, explicit index tie-breaks on every sort and every descent comparison.
- **Both terms are needed, and they do different jobs** (measured, not assumed): **stream power**
  carves the trunk valleys that give a continent *capture* but leaves the coastline noise untouched
  (it is concentrated where `A` is large, which is nowhere near a headwater coast); **diffusion** is
  what planes that noise off and *de-sponges*. Incision alone moved coastal 59.2% → 57.5%; with
  diffusion it reaches **52.8%**.

> #### Two things the pass had to learn the hard way — do not "simplify" them away
>
> **1. Base level is `land_contour`, which the anchor then makes equal to `sea_level`.**
> `apply_fluvial_erosion` still takes its base level from `heightfield::land_contour` (the
> `1 − target_land_pct` quantile) rather than from `sea_level`, and **that ordering still matters**:
> erosion runs *before* `anchor_contour_to_sea_level`, so at the moment it runs the two are not yet
> the same number.
>
> *Historical note — do not restore the old reasoning.* This note used to say base level is the
> "land-mask's **rank** contour, NOT `sea_level`", justified by only **24–37%** of cells sitting above
> `sea_level = 0.62` while the mask claimed **38%** for land, putting the coastline at **0.55–0.61,
> *below* sea level**. That gap was a symptom of the jittered-rank mask, and it is **gone**: realized
> land is now **37.7–37.8%** against a 0.38 target, and after the anchor the contour and `sea_level`
> are identical by construction. The warning the note was protecting is still live — a pass that
> freezes everything under a *wrong* base level freezes the coastal band it exists to reshape and
> measures as a no-op (it did: coastal 59.2% → 58.8%) — but the specific 24–37%-vs-38% discrepancy no
> longer describes this pipeline.
>
> **2. A valley incised *to* base level DROWNS.** Now direct rather than indirect: the mask is
> `elevation > sea_level`, so a trunk cut below the contour simply **is** water on the next derive —
> a sea inlet that takes its basin with it (measured pre-arc: seed 4's biggest basin collapsed
> **546 → 99**). `incision_floor` exists to bound this; it ships at **0.0** because measurement said
> the drowned stretches read as *estuaries* and leave the coast **smoother** — but the lever is there,
> and the failure mode is real.
>
> **3. `anchor_contour_to_sea_level` is what lets the carving reach hydrology at all.**
> `restamp_elevation`'s lowland branch is only order-preserving *above* sea level; below it,
> `((v − sea_level)/(1 − sea_level)).clamp(0,1)` is an **order-destroying clamp** that plates every
> such cell — **a third of all land** — flat onto exactly `sea_level`. Carving valleys there is
> pointless: they are erased before hydrology sees them. So the pass finishes with a strictly
> monotone, piecewise-linear rescale that puts the coastline exactly on `sea_level`, making the
> pipeline's "land ⟺ above sea level" assumption *true*. Monotone ⇒ it cannot reorder the field, so
> the land mask still picks the same tiles.
>
> Since `elevation-authority` the anchor is **load-bearing rather than merely helpful**: it is the
> only thing that makes `target_land_pct` come out right, because nothing downstream repaints the
> mask to hit the target any more. It is also the reason the invariant is exact — the mask thresholds
> the very surface the anchor just aligned. Its own justification finally holds too: monotonicity
> guarantees the mask is unchanged *only* if the mask ranks on elevation, which before this arc it
> did not.

**Config** — the `erosion` block of each preset in `map_presets.json` (`ErosionConfig`):

| Key | Default | Meaning |
|---|---|---|
| `enabled` | true | Kill switch. `false` reproduces the pre-erosion maps **exactly**, and is the A/B control the census measures against. |
| `iterations` | 40 | Passes. Past ~40 the sponge stops improving and the big basins start planing away. |
| `erodibility` | 0.1 | Stream-power `K`. Below ~0.05 nothing carves; above ~0.3 incision **saturates** against the downstream clamp (the result stops depending on `K` at all) and the coast gets *worse*. |
| `area_exponent` | 0.5 | `m` — classic. |
| `slope_exponent` | 1.0 | `n` — classic. |
| `timestep` | 0.1 | `Δt`. Only `K·Δt` matters; split for readability. |
| `min_slope` | 1e-4 | Slope floor, so a filled flat still incises and can cut itself an outlet. |
| `fill_epsilon` | 1e-6 | The priority-flood's gradient across a filled flat. |
| `diffusivity` | 1.0 | Hillslope `D`. **The term that de-sponges.** Past ~2 it planes real relief off the continent. |
| `incision_floor` | 0.0 | How far above base level a valley may cut, as a fraction of the land band. See note 2. |
| `anchor_contour_to_sea_level` | true | See note 3. |

**Measured A/B** (`hydrology_earthlike::drainage_census`, `#[ignore]`d, 6 seeds, 80×52, shipped
river thresholds held at 3.0/12.0/25.0 so the comparison is clean). **Measured PRE-`elevation-authority`**
— the erosion-OFF-vs-ON comparison is still valid (both arms moved together), but the absolute
navigable counts belong to the pre-arc landmass and are ~0 at this size today; see the note above:

| metric | erosion OFF | erosion ON |
|---|---|---|
| coastal tiles of the largest landmass (**SPONGE** — must fall) | **59.2%** (spread 14.3) | **52.8%** (spread **9.6**) |
| biggest basin / largest landmass (**CAPTURE** — must rise) | 11.0% (spread 39.5) | 13.3% (spread 34.1) |
| navigable rivers (post "end-at-water" fix) | 21 segments / 67 hexes / **max run 7** | 21 / 75 / **max run 21** |

> **Honest verdict: one of the two failures is fixed, the other is only dented.** The **sponge is
> genuinely better** — every seed improves and the spread halves — and the **~13-hex navigable
> ceiling is gone** (longest river 7 → **21** hexes post the "end-at-water" fix; the ceiling was never
> the threshold, it was the landscape). **Capture is not fixed.** The mean barely moves and the spread stays huge: seed 5 goes
> 4.7% → 21.0% and seeds 1/3 roughly double (2.2 → 4.2, 3.5 → 5.2), but seeds 1/3/TEST are still
> single-digit while seed 4 still runs at 38%. **Incision deepens the valleys a continent already
> has; it does not move its divides.** The divides come from the continent-scale fbm, so the next
> lever is the *noise*, not the erosion — see issue #261, "Capture: move the divides, not the valleys".
>
> **`elevation-authority` added `continental_weight` / `continental_radius`, and they do NOT fix
> capture.** They make `continents` a real lever for the first time (the old BFS grew a single
> accidental supercontinent at 80×52 while the preset asked for 4), but the bias is a **radial
> falloff — dome-shaped by construction**, so it moves landmasses apart without giving any one of them
> an internal divide structure. A dome sheds radially; that is exactly the "sheds radially with no
> trunk valleys" failure this section opens with, just at continent scale. Measured after the arc: the
> largest basin still tops out at **~5% of its landmass**, statistically unchanged pre/post. Capture
> needs a term that shapes *divides* — anisotropic/warped noise, tectonic uplift fields — not a
> smoother continent outline. Design context: `docs/plan_elevation_authority.md`.
>
> `apply_coastal_smoothing` was **measured, not assumed** (the suspicion was that its 3×3 blur would
> soften the incised valleys right where they matter). It does not blunt the result: the sponge metric
> is **bit-identical** with the blur zeroed (the land mask is decided from the base field *before*
> `restamp_elevation` ever runs), and zeroing it actually made rivers **worse** (max navigable run
> 25 → 15). Leave it alone.

## A range's rain shadow starts at its CREST — the belt used to shadow itself (issue #332)

**The history, compressed, because it is why the code looks like this.** `compute_moisture_field`
(`mapgen.rs`) used to add `rain_shadow_strength × relief` to the running `shadow` at *every*
mountain-mask cell and subtract it from every cell downwind. A fold belt is ~9 tiles wide
(`belt_width_tiles = 4` dilated both ways), so **the belt's own windward tiles shadowed the belt's
interior** — at earthlike numbers `rain_shadow_strength = 0.65` against alpine relief ≥ 1.85 is ~1.20
of shadow per tile (2.0 clamp, saturating in under two tiles, decaying at only
`rain_shadow_decay = 0.04`) versus ~0.22 of lift from `windward_moisture_bonus = 0.12`: shadow beat
lift ~5:1. Result: the mountain belt was the **driest** ground on the map when orography says it
should be the wettest — 82.8% of alpine tiles at literally zero precipitation, bimodal (mean 0.083,
median 0.000), with precipitation *rising* with distance from the belt core. Measured by
`core_sim/tests/alpine_headwaters.rs` (6 seeds × 2 grids) after a playtest reported "not enough
rivers issue out of Alpine ranges", and isolated by A/B (`what_dries_the_alpine_belt`) rather than
inferred: zeroing the shadow inverted the profile, zeroing `interior_aridity_strength` did not.

**The model now: one contiguous run of mountain cells along the wind path is ONE range, and it
releases its shadow at its crest.** Air is lifted all the way *up* a range and only dries once it has
crested and is descending, so the shadow a range casts belongs to the ground **behind** it, never to
ground still climbing it. `plan_orographic_row` plans each row before the humidity walk:

- A maximal run of cells with `mountains.get(idx).is_some()`, in wind order, is one **range**.
- Its **crest** is the run's maximum `relief_scale`, tie-broken to the **last** such cell — a flat
  summit is level ground, not yet a descent, so the whole summit zone keeps its lift.
- Cells from the run's first through the crest inclusive are **windward**; only they take the
  `windward_moisture_bonus × relief` lift and the `carry` boost. Descending air is not lifted.
- The run's **entire** shadow, `Σ rain_shadow_strength × relief` over the run, is released into
  `shadow` at the crest cell only, after that cell's own humidity is written (clamped to `[0, 2]`).
  Nowhere else in the run adds shadow, so a windward flank sees only shadow left over from ranges
  further upwind.
- The **volcanic** suppression term stays per-cell on windward and lee alike: it is a local plume
  effect, not part of the range's shadow.

**No config changed.** `rain_shadow_strength`, `rain_shadow_decay` and `windward_moisture_bonus` keep
their shipped numbers — this was a mechanism defect, not a tuning one. Pinned directly by
`mapgen::tests::range_shadow_is_released_at_the_crest_not_along_the_windward_flank` (a synthetic
5-tile range peaking in the middle: the flank and crest are no drier than the ground upwind, the
first lee cell is sharply drier), which reads 0.0 at the crest under the old per-cell behaviour.

Measured before/after, 6 seeds on the 384×288 statistics grid:

| figure | before | after |
|---|---|---|
| alpine precip mean / median | 0.083 / 0.000 | **0.265 / 0.204** |
| alpine bone-dry share | 82.8% | **29.1%** |
| all-land precip mean / bone-dry | 0.151 / 45.1% | 0.171 / **36.9%** |
| precip by distance from the core (0→9) | 0.083 → 0.190 (**rising**) | 0.265 → 0.196 (**falling**) |
| alpine tile → nearest river (all land) | 6.20 hexes (3.31) | **5.26** (3.20) |
| source enrichment at 2 / 3 / 4 hexes | 0.28× / 0.45× / 1.63× | **1.31× / 1.93× / 2.42×** |
| rivers over the sweep | 3721 | **4034** |

The belt is now the wettest ground on the map and the precipitation profile falls away from it, which
is what orography asks for; the channel-free collar narrows by about a hex rather than vanishing.

> **Do NOT read the "0 river edges on alpine tiles" figure as this defect.** It was separate then and
> is separate now: `restamp_elevation` floors mountains above `elevation_base`, so every alpine
> corner is a local maximum on the filled surface, and the shipped `alpine_relief_threshold = 1.85`
> makes the ribbon **3 tiles wide** — a corner gathers ~1–2 hex-equivalents against
> `river_channel_min_discharge = 3.0` and *cannot* clear it. **The crest fix left it where it was:
> 0.00% → 0.02%**, against 15.49% → 16.43% on other land. Two mechanisms; only the rain shadow was a
> defect, and fixing it did not buy channels *on* the crests. Never repaint that by lowering the
> discharge threshold — see "Do not 'fix' a dry map by lowering
> `river_class_navigable_min_discharge`" above.

**The balance movement is small, and it was measured, not assumed.** Humidity feeds
`dryness_thresholds` → the biome ladder → forage/graze capacity, so `alpine_headwaters.rs` reports a
**biome-share census of land** alongside the river figures. Only one biome moves more than 0.1 pp:
`CanyonBadlands` 3.88% → **3.47%**, the arid relief that was sitting on the self-shadowed belt, going
to `RollingHills` (6.66% → 6.82%) and `HighPlateau` (0.08% → **0.30%**). The eight largest shares
(prairie/alluvial/tundra/rocky/scrub/woodland/marsh/floodplain) all hold to within 0.03 pp. The
wetting is real but concentrated on the belt, where it belongs.

## Tile Temperature — latitude + elevation climate model
`Tile.temperature` is a real climate, **not** the old `(x+y)%4` element checkerboard. The single
source is `systems::climate_temperature(y, grid_height, above_sea_normalized, element, &ClimateConfig)`:

```
temperature = latitude_base(y, H) − elevation_lapse(elev) + element_jitter(element)
```

- **`latitude_base`** — equator-in-the-**middle**: `lat_frac = |y − (H−1)/2| / ((H−1)/2)` ∈ [0,1]
  (0 = center/equator, 1 = top *or* bottom edge/pole), `equator_temp − lat_frac·(equator_temp −
  polar_temp)`. Symmetric: the top and bottom edges are equally cold; the temperate band (~18°)
  lands at mid-latitudes (lat_frac ≈ 0.34).
- **`elevation_lapse`** — `ElevationField::above_sea_normalized` (height above sea remapped to [0,1])
  × `elevation_lapse_span`; higher ground is colder.
- **`element_jitter`** — the element's `thermal_bias` × `element_jitter_scale`, kept small (~±1.5°)
  so it is local texture, not the driver.

Config lives in the `climate` block of `simulation_config.json` (`equator_temp` 30.0, `polar_temp`
-5.0, `elevation_lapse_span` 12.0, `element_jitter_scale` 0.25). Worldgen seeds each tile at exactly
this value **after** elevation exists (a `climate_elevation` field with sea level attached), and
`simulate_materials` relaxes each turn toward the *same* recomputed climate temperature (no longer
the element target), so turn 1 has no jump. On an 80×52 map: equator ≈ 29–30°, mid-latitude ≈ 18°,
pole = −5° at sea level (mountains up to 12° colder).

## Temperature is the climate authority — the biome band is derived from it
Since the **climate-authority arc** (`docs/plan_climate_authority.md`), a biome's climate
eligibility is a function of the tile's **temperature**, never its latitude. Temperature is now
computed in the *first* worldgen loop (before the biome is assigned, `systems/worldgen.rs`), and one
seam — **`climate::climate_band_for_temperature(temp, &ClimateConfig) -> ClimateBand`** — maps it to
a four-rung ladder (**polar ≤ 0° / boreal ≤ 3° / temperate ≤ 18° / tropical**, cut points in the
`climate` block: `polar_max_temp` / `boreal_max_temp` / `temperate_max_temp`). `ClimateBand`'s
`admits_cold_biomes()` (polar **or** boreal) is THE predicate every cold-biome gate reads; **no call
site may re-derive a band or compare a raw temperature to a literal.** The gate reads the **jittered**
temperature deliberately, so band boundaries come out ragged rather than as clean horizontal lines
(design §8.2 — the lever if it is ever too noisy is `climate.element_jitter_scale`, never re-gating on
an un-jittered temperature).

- **What this retired.** The old `terrain_classifier.polar_latitude_cutoff` (0.35) and
  `high_latitude_threshold` (0.15) fields, `systems/mod.rs`'s `POLAR_LATITUDE_THRESHOLD` (which read
  the *default* preset, a latent desync bug), and `worldgen.rs`'s `climate_band_for_position` (a
  third arithmetic copy with a bare `0.18` literal) are all **gone**. The six former latitude-gate
  sites — the base classifier's cold ladder, the mountain glaciation branch, the two palette remaps
  (prototype-loop + post-solver `apply_biome_palette_clamp`), `bias_terrain_for_preset`, and the tag
  solver's wetland/fertile/polar passes — now all read the temperature band.
- **The boreal band is its own rung, not a wider polar.** Polar tiles get the ice ladder
  (Tundra/PeriglacialSteppe/SeasonalSnowfield), boreal tiles get the taiga ladder
  (BorealTaiga/MixedWoodland/PeatHeath). A single polar cut point could not express the boreal-fringe
  incoherence (measured: BorealTaiga was 1,601 of 4,397 warm-polar tiles), which is why the ladder has
  four rungs (§8.1).
- **The tag solver has a climate veto** (§5.4): its polar family pass may only paint a cold biome on a
  tile whose band admits one; where the `Polar` tag target cannot be met without violating climate it
  **under-fills and logs** (`mapgen.tag_solver.under_filled_climate_gated`) rather than repainting —
  the repaint-to-hit-a-quota pattern this repo has rejected before. This closed 64% of the warm-polar
  tiles (the fallback loop had **no** climate test at all).
- **Alpine tundra is now expressible** (§5.3): a cold mid-latitude highland (a mountain at −1.6°)
  glaciates to Glacier/SeasonalSnowfield because the mountain branch reads the band, not the row.
  Measured ~10% of land, up from ~0.
- **`boreal_max_temp` == the client's retired `cool_min` (3.0)** — one boundary, stated once (§5.2/§8.3).
  The sim owns the cut points and **publishes** them in the snapshot (`MapSection.climateBands`, the
  `ClimateBands` table — the same way `seaLevel` rides the elevation overlay) so the client renders the
  band it is told rather than keeping an independent opinion. **Client half (a separate task): consume
  `climateBands`, drop the local `tile_climate_config.json` `cool_min`, and render `Climate:` off the
  published bands.**
- **Secondary fixes that rode along** (§7): `PeatHeath` is now `POLAR`-tagged (it is the cold wetland
  — the only WETLAND+POLAR biome, and the classifier/solver/palette already treated it as such; only
  its tag disagreed); `RiverDelta` now takes its own definition's tags wholesale in `hydrology.rs`
  (it used to OR only WETLAND|FRESHWATER and *keep the underlying biome's tags*, leaking `POLAR`
  through a delta cut through Tundra into `BiomePalette::remap`, `food.rs` and the tag census).
- **Measured before/after** (`core_sim/tests/climate_authority.rs`, ≥5 seeds × 2 grids × both presets,
  run the `#[ignore]`d `climate_band_report` for the full tables): cold-but-temperate **6.9% → 0.16%**
  of land, warm-polar **7.9% → 0.00%** — both directions collapsed, neither traded for the other. Land
  share per band (aggregate): polar 19.5% / boreal 8.7% / temperate 48.1% / tropical 23.7%. The
  worldgen *tectonic* regression baselines (`mapgen.rs` fold/fault/dome counts, land ratio) are
  **unchanged** — this arc touches only which biome a land tile wears, not the elevation/mountain masks.

## Map Presets (`map_presets.json`)
Presets control: `seed_policy`, `dimensions`, `sea_level`, `continent_scale`, `mountain_scale`, `moisture_scale`, `river_density`, `terrain_tag_targets`, `locked_terrain_tags`, `biome_weights`.

**`macro_land` — landmass shape** (`MacroLandConfig`, `map_preset.rs`). Since `elevation-authority`
every one of these is honored by *shaping the heightfield*, never by editing the mask:

| Key | earthlike | Meaning |
|---|---|---|
| `target_land_pct` | 0.38 | Land fraction. Delivered by `anchor_contour_to_sea_level` putting this quantile exactly on `sea_level`; realized **37.7–37.8%** pre-island with nothing correcting it downstream. |
| `continents` | 4 | Number of continental bias centres, chosen deterministically from the world seed with Poisson-ish spacing (wrap-aware in x). Realized landmasses ≥`min_area`: 3–5. |
| `min_area` | 256 | The landmass size that counts as a continent when auditing the above. |
| `continental_weight` | 0.5 | Amplitude of the low-frequency continental bias added before erosion. `0.0` reproduces the pure fractal field — which thresholds into **one dominant supercontinent**, which is why the term exists. |
| `continental_radius` | 0.35 | A continent's radius of influence, as a fraction of the **smaller** grid dimension. Beyond it the bias saturates at its minimum, which is what actively sinks inter-continental gaps rather than merely making them less high. |
| `continental_falloff_exponent` | 1.5 | Shape of the falloff, `bias = 1 − 2·t^exponent` over `t = dist/radius`, taken as a **max over centres, not a sum** (summing fuses adjacent centres into a land bridge). |
| `continental_warp_amplitude` | 0.18 | **Domain warp** — how far the envelope's sample coordinates are displaced by low-frequency noise before the envelope is evaluated, as a fraction of the **smaller** grid dimension. Makes a continent lobed rather than circular. `0.0` restores a perfectly radial envelope. |
| `continental_warp_frequency` | 1.6 | Cycles of warp noise across the map. Low by design: the warp reshapes *landmasses*; fraying the *coastline* is `coastline_roughness`'s job. |
| `continental_tilt_strength` | **0.0 (off)** | **Per-continent tilt** — a directional gradient across each centre, its heading hashed per centre from the world seed, windowed by `1 − t^4` so it vanishes at the rim. A dome sheds water in every direction; a *tilted* surface drains one way. Ships as a **tilted trough**, not a tilted plane: `heightfield::CONTINENT_TROUGH_GAIN` (0.5) lifts the ground away from the drainage axis, because a bare tilt gives **parallel** flow (many short rivers) rather than convergence onto a trunk. **Both presets ship it at `0.0`**: at `2.0` it buys one extra seed-with-a-river in six but fuses continents into a supercontinent, collapsing `polar_contrast`'s fold belts by 85% (see the note below). The machinery is retained, live and inert at zero — raising it is how you get the drainage back, at that cost. |
| `continental_spine_amplitude` | 0.35 | **Ridged spine** — ridged noise gated to the continent interiors (`clamp(bias, 0, 1)`), so a landmass carries an internal **divide** with two drainage sides instead of one summit. Also the term that keeps mountain ranges narrow (see below). |
| `continental_spine_frequency` | 2.2 | Cycles of spine noise across the map — roughly how many range-scale divides a continent can carry. |
| `continental_basin_amplitude` | **0.4** (earthlike; polar_contrast 0.0/off) | **The lake lever.** How far the continent *interior* is planed down toward the coastline contour (`bias -= amplitude × bias.clamp(0, 1)`) — a broad near-sea-level interior plateau where the field's own fine-scale noise makes many small **enclosed** lakes. Gated to the interior (`bias ≤ 0` untouched), so it raises lake share **without** eroding the coast — unlike `continental_weight`, which lowers everything and costs cold-ocean seal habitat. `0.0` is byte-identical to no term. Earthlike median lake share ~1.5% → ~2.7%. See "Lakes are emergent" → "Abundance is the interior sink". |
| `coastline_roughness` | 0.05 | High-frequency shoreline raggedness, applied to the field **before** `land_contour`. Replaces the retired `macro_land.jitter`, which perturbed the mask's *ranking* instead of the field and thereby decoupled the two. |

> `macro_land.jitter` **no longer exists** and must not be reintroduced — it is the specific lever
> that broke "land ⟺ above sea level". Its intent lives on as `coastline_roughness`.
>
> **The bias is no longer purely radial.** `continental_weight`/`continental_radius`/
> `continental_falloff_exponent` are now only the *base envelope*; the warp, tilt and spine terms above
> shape divides and drainage direction on top of it. **The tilt ships at `0.0`** — the lever is live
> and fully wired, but both shipped presets set it to zero; see the two findings below.
>
> Measured over 6 seeds at 80×52 (`core_sim/tests/relief_sweep.rs`, `-- --ignored --nocapture`),
> dome → warp+spine (**the shipped configuration**): **sowable ground on the standard map 35 → 49
> tiles**, navigable rivers present on **1/6 → 2/6 seeds** (3 segments total), max drainage
> accumulation mean **25.4 → 28.4**, land fraction unchanged (0.387–0.400 → 0.386–0.397), landmasses
> ≥ `min_area` 2.8 → 2.2 per map. Adding the tilt on top buys one further seed with a river
> (3/6, 4 segments) and costs what the second finding below describes.
>
> **What it did NOT fix, and the trade-off it carries — read before retuning:**
> - **Basin coherence is still ~0.02–0.08** (max accumulation ÷ largest landmass), statistically where
>   the dome left it. Measured root cause: a corner is a sink iff any of its 3 hexes is ocean, and the
>   largest landmass has a **mean depth-to-coast of only ~2.9 tiles** with ~360 coastal (⇒ sink) corners
>   against ~800 land corners. Flow terminates within ~3 steps whatever the relief looks like, so at
>   80×52 with `continents: 4` a discharge of 25 is *geometrically marginal* — not a tuning failure.
>   Ruled out by direct measurement: `river_flat_jitter` (5e-4 → 1e-6 moves nothing) and
>   `continental_weight` (0.5 → 2.0 improves compactness but *lowers* max accumulation).
> - **NONE of these terms controls mountain-range WIDTH — the earlier claim that the tilt widens
>   ranges was a metric error.** It rested on mean alpine connected-component **area**, which does not
>   measure width (a long thin cordillera and a fat blob can share an area) and whose per-seed value
>   swings up to **27×** on one configuration — noise, read as signal off a single seed. Measuring
>   thickness directly (every alpine tile's hex distance to the nearest non-alpine tile,
>   `relief_sweep::alpine_thickness`, 6 seeds at 384×288) gives **mean 2.22 / 2.32 / 2.26 / 2.28 /
>   2.43 and p95 4.3–5.0 for dome / warp-only / spine-only / tilt-on / warp+spine** — flat. If ranges
>   read as too wide, the lever is downstream in the mountain mask (`derive_mountain_mask`'s
>   `belt_width_tiles` dilation, `apply_belt_relief`, `terrain_classifier.alpine_relief_threshold`),
>   **not** in the continental envelope. Do not re-derive a width claim from component area.
> - **The tilt FUSES CONTINENTS, and that is why it ships at `0.0`.** On `polar_contrast` it collapsed
>   the five multi-plate land components into **two**, the largest going 9,053 → **18,313 tiles — 85%
>   of all land in one body**. Fold belts form only between plates *within* a component and plate
>   count is area-bucketed with a **cap of 4**, so fusing the map into one supercontinent starves the
>   plate-boundary network: `polar_contrast` fold count fell **3556 → 544 (−85%)**. With the tilt off
>   it recovers to **3004**. This is the same land-bridging failure `CONTINENT_TILT_WINDOW_EXPONENT`
>   was introduced to prevent — the window mitigates it but does not eliminate it on this preset.
>   Measurement: `mapgen::tests::polar_contrast_fold_investigation`.

**Coastline-editing lever** — `place_islands` is the one stage permitted to move a coastline, and it
writes elevation and re-derives:

| Key | Block | Default | Meaning |
|---|---|---|---|
| `island_peak_margin` | `islands` | 0.06 | How far above `sea_level` an island's peak is raised. `place_islands` raises a radial dome and the mask is re-derived; this margin is what makes the dome *become* land. Placement (`continental_density`, `oceanic_density`, `min_distance_from_continent`) is unchanged. |

> #### Lakes are emergent, and the whole `inland_sea` config block is GONE
>
> A lake is a closed basin the heightfield produced whose floor is below `sea_level` and which the
> mask does not connect to the ocean. **Nothing places one, and nothing repairs one.**
>
> `connect_inland_seas_via_straits` (and with it `InlandSeaConfig`: `merge_strait_width`,
> `strait_depth_margin`, the always-dead `min_area`, plus the always-dead `lake_chance`) is
> **deleted**. It BFS'd from every landlocked water tile through up to `merge_strait_width` tiles of
> land to the sea and sank the corridor, so any basin within two tiles of a coast stopped being a
> lake. Because a large body has more perimeter, it had more chances to be caught — the carver ate
> the *big* lakes preferentially and left the specks.
>
> **Why it existed, and why that reason expired.** It dates from the pipeline's first commit
> (2025-11-02), when a lake was a river **terminus** (`TerminationClass::Lake`) and the outlet below
> it was *fabricated* — `lake_heads` spawned fresh river sources on the lakeshore under
> `SourceCategory::LakeOutlet`, with no upstream catchment. An inland sea genuinely was a drainage
> dead-end, and converting it to ocean was one way out. The drainage-network rewrite (`2ad0923`,
> 2026-07-14) made **lakes flow through** — only the ocean is a sink, the fill raises a lake corner
> to its saddle and it spills — and deleted `lake_heads`. The carver was a workaround that outlived
> its problem by a week, then survived `elevation-authority` because that arc changed *how* stages
> edit the map, not *whether* this one should.
>
> **Measured on deletion** (5 seeds each): 80x52 earthlike lake area **43 → 229 tiles**
> (0.53% → 2.8% of land), largest body **8 → 90 hexes**, bodies of ≥5 tiles **2 → 8**, and the share
> of lakes touching a river **21% → 39%**. 192x128: **402 → 1053** tiles. Land holds at 39.2% → 39.7%
> and shelf at 16.7% → 15.3% of ocean, with every `shelf_ratio` / `elevation_authority` /
> `hydrology_earthlike` / `navigable_mouth_delta` invariant unchanged.
>
> **The river-connection rate is a SIZE effect, not a separate defect** — measured across 15 maps,
> one-tile bodies touch a river 21% of the time, 5–19 tile bodies **89%**. A one-hex lake has a
> one-hex catchment, which is below `river_channel_min_discharge` (3.0), so no channel is drawn. That
> is honest hydrology. **Do not lower a discharge threshold to put rivers on puddles** — grow the
> lakes and the rivers follow.
>
> #### Abundance is the interior sink, not a repair pass
>
> Deleting the carver stopped SUBTRACTING lakes; it did not make the heightfield PRODUCE enough. The
> per-seed distribution stayed heavily right-skewed — **median ~1.5% of land, a third of maps under
> 1%** — so a playtester still read the map as lake-poor. The abundance lever is
> **`macro_land.continental_basin_amplitude`** (`heightfield::apply_continental_bias` step 4,
> earthlike **0.4**): it planes the continent *interior* down toward the coastline contour
> (`bias -= amplitude × bias.clamp(0, 1)`), so the top of the dome becomes a broad near-sea-level
> plateau where the field's own fine-scale fbm dips below the contour in many small **enclosed** pools.
> Median lake share rises to **~2.7% (mean ~3.4%, 24 seeds)** with **0 dry maps**.
>
> **Why the interior sink and not one of the obvious alternatives — all measured, all rejected:**
> - **The strait carver's inverse (carve discrete bowls).** This is the shape the term *started* as,
>   and the config name still says "basin". A bowl gouged into high interior drains to the sea and
>   reads as a coastal inlet (ocean), not a lake — it barely moved the lake count at any depth or
>   frequency. A lake needs a broad near-contour *area*, not a deep pit.
> - **Lowering `continental_weight`.** It works (it lowers the interior, the same mechanism) — but it
>   scales the *whole* envelope, so it also flattens the cold-ocean coastline, which **halved seal
>   spawns** (`fauna_coastal_habitat` measured 14 → ~6, under its guard). The interior sink's
>   `bias.clamp(0, 1)` gate pins the coast (`bias ≤ 0` untouched), so it raises lakes with seals and
>   `realized_land_fraction` both **unchanged and green** — that gate is the whole reason it is a
>   separate term.
> - **Raising `sea_level` / lowering `target_land_pct`.** Inert / just adds ocean — the contour anchor
>   holds the land∶water split regardless, and extra water goes to the sea, not enclosed basins.
>
> Guarded by `core_sim/tests/lake_abundance.rs` (sweep-median floor, the lake analog of the seal
> pinhole guard) — the tripwire the carver era lacked.
>
> **The tectonic baselines moved twice and were re-pinned** — for the carver deletion and again for
> the interior sink (both earthlike-only; `polar_contrast` carries neither). Not incidental: plates are
> area-bucketed from land **connected-components**, so anything that changes how land coheres — cutting
> straits, or planing the interior toward the contour — moves the fold/fault/uplift network downstream
> of it. The re-pinned centres are the tuned seed's deterministic output and the single-seed counts are
> high-variance, so read no *direction* into them; the stable guards are the continent *count* (median
> 3 at large/huge), seals, and land fraction, each in its own test. Same coupling the
> `continental_tilt_strength` note describes from the other direction.

The active preset's `sea_level` is carried on the `ElevationField` resource, attached at the field's origin in `build_elevation_field` and propagated through `restamp_elevation` (`heightfield.rs` / `mapgen.rs`; falls back to `DEFAULT_SEA_LEVEL` = 0.6 only when no preset resolves — which also logs a `warn`, because a preset-less field skips erosion and the contour anchor entirely). It is exported in the snapshot as `ElevationOverlay.seaLevel` — **normalized to the overlay's [minValue, maxValue] sample scale AND quantized onto the same u16 lattice as the samples** (`snapshot/map.rs` `elevation_overlay_from_field`, `ELEVATION_SAMPLE_SCALE`) so the Godot client can compare it directly against decoded samples for its relative-height / LOS readout.

> **Samples and the published `sea_level` must share one quantization lattice.** The client decodes
> `sample / 65535` and compares against `seaLevel`; publishing the threshold *unquantized* made every
> tile sitting exactly at sea level decode to `0.6200046 > 0.62` and read as land-height water — 42 of
> them in a live export, all with the identical raw sample `40632 = round(0.62 × 65535)`. Do not
> reintroduce a second `65535.0` literal. Guarded by
> `elevation_authority::the_published_sea_level_lies_on_the_sample_quantization_lattice`, which
> asserts on the **encoded overlay** rather than the in-process `ElevationField` — the earlier test
> read the f32 field, reported 0 violations, and missed all 42.

**Continental shelf width** (`classify_bands` + `effective_shelf_width`, `mapgen.rs`; `ShelfConfig`, `map_preset.rs`): `ContinentalShelf` is the ocean band within a computed distance of the coast (slope collapses to `DeepOcean` downstream, so only the shelf boundary affects ocean composition). The model mirrors real margins — a **continuous ≥1-tile shelf off gentle (passive-margin) coasts, and deep water right at steep/cliff (active-margin) coasts** — via two knobs on top of the width scaling:
- `min_width_tiles` (default **1.0**) — floors the computed width so a qualifying coast gets a *continuous* ≥1-tile ring instead of a sub-tile sparse fringe. Applied after the `width_frac`/`width_exp` (or `width_tiles`) computation, so a preset that bumps `width_frac` still scales the shelf wider than the floor on big maps.
- `coast_height_threshold` (default **0.10**, earthlike **0.10**) — the coast-height gate. A shelf-candidate ocean tile becomes `ContinentalShelf` only if the coast land it abuts rises *gently*: the MIN normalized rise (`elevation.sample − sea_level`) over its immediately-adjacent land tiles is **below** this. Cliff/mountain/highland coasts (rise ≥ threshold) instead show `ContinentalSlope`→deep water at the edge. On earthlike, lowland coasts rise into the compressed band `[sea_level, elevation_base]` (≤ ~0.10) while mountain-mask coasts jump to ≥ ~0.16, so the threshold sits in the bimodal gap and cleanly splits gentle vs. steep. This self-limits the shelf %: steep coasts add zero shelf, so the 1-tile floor doesn't blow the fraction up on small maps the way a blanket ring would.

  **The immediate coastal ring is HEX-aware (odd-r 6-neighbour).** The default 1-tile shelf ring's coast-adjacency uses the authoritative odd-r hex neighbours (`grid_utils::hex_neighbors_wrapped`, wrap-aware — the same adjacency gameplay + the client render), not 4-connected square neighbours. An ocean tile joins the ring iff it is hex-adjacent to ≥1 Land tile **and** the min rise over its Land hex-neighbours is `< coast_height_threshold`. This closes the old hex-diagonal gaps: the 4-cardinal set covers only two (E/W) of the six hex directions, so before the fix a gentle coast could sit directly against DeepOcean on a hex-diagonal (`min_adjacent_coast_rise` + `classify_bands`, `mapgen.rs`). The broader worldgen distance transforms (ocean-distance, mountain masks, rivers) remain **square-grid** — pre-existing modeling, out of scope; only the immediate shelf ring is hex-exact (a full hex distance-transform for `width_frac`-widened shelves, `full > 1`, is the follow-up). Guarded by `mapgen::tests::earthlike_bands_have_no_gentle_coast_shelf_gap` (0 DeepOcean-vs-gentle-Land hex adjacencies over real earthlike coastlines) + `classify_bands_shelf_covers_hex_diagonal_coast`.

  **Final reconciliation pass — the shelf is hex-exact on the *final* map, not just at band time.** `classify_bands` decides the shelf early (stage 6), but later Startup stages repaint terrain near the coast *after* the shelf exists: `generate_hydrology` stamps `RiverDelta`/`Floodplain`/`FreshwaterMarsh` at river mouths, and `apply_tag_budget_solver` paints polar `Tundra` over near-shore ocean — each creating fresh gentle-land-vs-`DeepOcean` adjacencies with no shelf between them (band-level zero-gap ≠ final-map zero-gap). `reconcile_coastal_shelf` (`systems.rs`) is a deterministic post-pass registered in the Startup `.chain()` **right after `apply_biome_palette_clamp`** (so after hydrology + tag solver + palette clamp — the last word on ocean tiles): every `DeepOcean` tile odd-r hex-adjacent (`grid_utils::hex_neighbors_wrapped`, wrap-aware, honoring the active `map_topology.wrap_horizontal`) to a **gentle** land tile — non-`WATER` tags, rise `elevation.sample − sea_level < coast_height_threshold` (the SAME gate + hex convention as `classify_bands`) — is reclassified to `ContinentalShelf` (a `must_have` palette biome, so no palette conflict). So downstream-created coasts (deltas, marshes, solver tundra) all get a shelf seaward, while **steep** coasts (every land hex-neighbour rises `≥` threshold) still keep deep water right at the edge. Guarantees on the final map: **no `DeepOcean` tile touches gentle land.** Guarded by `integration_tests/tests/shelf_ratio.rs::earthlike_no_deep_ocean_touches_gentle_land_on_final_map` (0 gaps across sizes/seeds, + a steep-coast-keeps-deep-water assertion) and `earthlike_delta_and_marsh_coasts_have_shelf_not_deep_water`.
- `width_tiles` (default 2) — legacy absolute band width, used only when `width_frac` is unset (e.g. `polar_contrast`). `width_frac` + `width_exp` (earthlike) scale the pre-floor width with map size as `width_frac * min(w, h)^width_exp`.

  Because the shelf is now a ~1-tile ring off *most* coastline, the fraction is **no longer** the old size-invariant 5-8%: it varies with coastline steepness and **shrinks as the open ocean grows** — measured full-pipeline (slope folded into deep water) with the hex-exact ring **plus** the final reconciliation pass it runs **~15-19% of ocean at 80×52 down to ~8.5% at 256×192** (re-measured after `elevation-authority`; the pre-arc figures were ~29-33% down to ~14%, and the drop is a *consequence* of the derived mask producing fewer, smoother landmasses — less coastline per unit of ocean, with the zero-gap invariant still holding) (a touch higher again than the band-only ring, since the post-pass also stamps shelf on the hydrology/tag-solver coasts; re-measured after the border-ring bathymetry fix below, which removed the orphaned offshore shelf the drowned border land used to strand). Guarded by `integration_tests/tests/shelf_ratio.rs`: a per-map sanity band (6-50%) plus the model assertion that coast land next to shelf tiles is lower than coast land next to deep-water-at-the-edge tiles. This is a pure ocean-tile reclassification — it does **not** touch the land mask, so mountains/rivers/land ratio are unchanged.

  The gate keys off the *immediately-adjacent* (hex-neighbour) coast land, which fully covers the 1-tile default (every shelf tile touches land). Deferred: a preset that widens the shelf past `d==1` leaves outer-ring tiles ungated (they touch no land, so they pass) and those outer rings still ride the square-connected `ocean_distance` — carrying the nearest-coast rise through a hex distance-transform is the follow-up for wide shelves. Also still deferred: a true *depth-based* shelf would need real offshore bathymetry (today ocean elevation is fractal noise with no coast-relative deepening); and if the narrower shelf's reduced `CoastalUpwelling` forage frontage matters for gameplay, lock the `Coastal` tag to stamp compensating `TidalFlat` (the tag solver's coastal pass). Neither shipped preset locks `Coastal` today.

**Elevation ↔ biome coupling** (`restamp_elevation`, `mapgen.rs`): mountain biomes come from the tectonic mountain mask + relief, so the elevation field is tied to that same signal to keep them consistent (mountains genuinely tall — see the `mountain_tiles_out_top_lowland_tiles` regression test). Every mountain-mask tile is floored into `[elevation_base, 1.0]`, ordered by relief and scaled by per-type prominence; non-mountain land is compressed into `[sea_level, elevation_base]`. Tunables live in each preset's `mountains` block: `elevation_base`, `fold_prominence`, `fault_prominence`, `volcanic_prominence`, `dome_prominence`, `belt_texture` (small spine-vs-edge elevation texture added on top of the relief floor; bounded so it never reorders relief bands). The non-mountain `elev ≥ high_dry_elevation → CanyonBadlands` / `elev ≥ high_wet_elevation → RollingHills` cutoffs (`terrain.rs`) live in `terrain_classifier` and default to the top of the compressed lowland band.

**Highland biomes are mask-driven, never noise-driven.** `classify_terrain` (the base climate classifier) does NOT pick AlpineMountain/HighPlateau/CanyonBadlands/etc. — it has no real elevation, so it used to invent them from a tile hash and scatter flat "mountains." Mountain biomes now come only from the tectonic mask (`select_mountain_terrain`) + the real-elevation `terrain.rs` branches. `apply_belt_relief` (`mapgen.rs`) scales belt-tile relief by belt strength (`mountains.relief_belt_gain`, default 1.2) so belt cores clear the AlpineMountain relief threshold (`terrain_classifier.alpine_relief_threshold`, **1.85**) and taper to plateaus/hills — genuine Alpine spines that are also tall. Polar rows are skipped (they keep their low-relief-basin tuning). Regression guards: `mountain_tiles_out_top_lowland_tiles`, `alpine_biome_tiles_are_tall`.

> **`alpine_relief_threshold` is the alpine range's WIDTH lever — and it is the only one that
> narrows the range without also flattening or shrinking it.** Belt relief ramps linearly with belt
> strength (`1 + relief_belt_gain × strength / (belt_width + 1)`, `strength = belt_width + 1 −
> dist`), so the threshold picks an integer distance-from-plate-boundary cutoff `D`; the boundary is
> stamped on **both** plates, so the alpine ribbon is **`2D + 1` tiles wide** and two boundaries
> within `2D + 1` merge into a slab. At earthlike's `belt_width = 4` / `relief_belt_gain = 1.2` the
> bands are `≤ 1.60` ⇒ `D = 3` (a **7-tile slab**), `(1.60, 1.72]` ⇒ 5 tiles, `(1.72, 1.96]` ⇒
> **3 tiles (shipped, 1.85 — mid-band, so a small retune of `relief_belt_gain`/`mountain_scale`
> cannot silently step the ribbon a whole tile wider)**, `> 1.96` ⇒ single-tile peaks.
>
> **Measured** (`core_sim/tests/relief_sweep.rs::belt_sweep`, `--ignored --nocapture`, 6 seeds), the
> shipped `1.45 → 1.85` move at 384×288: alpine **thickness mean 2.43 → 1.57, p95 5.0 → 3.0**;
> alpine **6.1% → 3.0% of land**; connected **components 27.0 → 28.0** — the count *rises* as slabs
> break into distinct ranges, which is the wanted direction. At the shipped 80×52: thickness
> **1.80 → 1.36**, p95 **3.83 → 2.33**, alpine **15.9% → 7.0%** of land. Sowable ground at seed
> 119304647 is **unchanged at 49** and land fraction is unchanged (0.391).
>
> **The other two belt levers were measured and rejected**, both of which reach at best the same
> integer cutoff while costing something else: `mountains.belt_width_tiles` 3 → 2 only reaches
> `D = 2` and shrinks the whole belt — the **foothill skirt** goes with it (it also perturbs
> downstream terrain: sowable 49 → 51); `mountains.relief_belt_gain` 1.2 → 0.70 reaches `D = 1` but
> **lowers the belt core's relief 2.2 → 1.7**, i.e. it makes the mountains *shorter*, when the
> complaint was width. Raising the threshold leaves the relief profile — and so every peak height
> and the `restamp_elevation` relief ordering — **byte-identical**, and merely reclassifies the
> belt's shoulders to HighPlateau/RollingHills/CanyonBadlands. That is what makes a range read as a
> **range with foothills** instead of a slab. **Do not "simplify" this back into `belt_width_tiles`.**
>
> The continental-envelope terms (warp/tilt/spine) **cannot** do this — measured flat at 2.22–2.43
> thickness across every combination; see the `macro_land` note above.

**Number of ranges** is emergent tectonics: land connected-components → plates (area buckets, ≤4/continent) → fold belts form only where two plates' drift *converges* (`dot <= mountains.belt_convergence`, `derive_mountain_mask`). Drift is radial-outward so most boundaries diverge; raising `belt_convergence` toward 0 (earthlike default **0.25**; polar_contrast keeps the tighter **−0.1** to preserve its low-relief-basin contrast) lets more boundaries become ranges. Range count also scales strongly with **map size** — a full 256×192 map has 30+ ranges, an 80×52 "Standard" ~4–13, a 56×36 "Tiny" ~2–6.

**`classify_terrain`'s map-border "edge rings" are LEGACY, preset-less-only.** The classifier opens with three `edge < coastal_deep_ocean_edge / coastal_shelf_edge / coastal_inland_edge` early-returns that stamp DeepOcean / shelf / InlandSea+marsh. `edge` is the distance to the **map frame**, not to a coastline: it was the only coastline proxy the pre-bands (preset-less) world had. Under a preset the map has **real bathymetry** — `classify_bands` already partitioned it into Land / ContinentalShelf / InlandSea / DeepOcean, and `terrain_for_position_with_classifier` is called *only* for band-`Land` tiles — so running the rings there noise-coin-flipped **248–295 band-`Land` tiles per 80×52 map (~16–19% of all land)** into water biomes hugging the map border, deleting the land out from under legitimate shelf rings (118–153 **orphaned** shelf tiles with no land hex-neighbour, sitting 3–7 hexes out) and pinching off isolated deep pockets. The rings are therefore **skipped whenever real bathymetry is present** (`BathymetryContext::Present`, derived from the caller passing `Some(elevation)` — the *context*, never a config flag), and the tile falls through to the normal polar/anomaly/humidity **land** ladder. The preset-less fallback path passes `None` → `BathymetryContext::Absent` and keeps its historical behavior exactly. Invariant: **a band-`Land` tile can never end WATER-tagged.** Guards: `mapgen::tests::earthlike_band_land_never_ends_water_tagged`, `mapgen::tests::earthlike_shelf_is_never_orphaned`.

**Tag Budget Solver**: After biome stamping, iterates locked tag families (water → wetlands → fertile → coastal → highland → polar → arid → volcanic → hazardous) nudging tiles until coverage falls within `tolerance`. Every family that stamps or forbids a **cold** biome (wetland picks `PeatHeath` vs `FreshwaterMarsh`; fertile skips cold tiles; the polar family paints Tundra/SeasonalSnowfield) gates on the tile's **temperature band** (`TileInfo.band`, resolved once from `Tile.temperature`), not its latitude — see "Temperature is the climate authority". The polar family carries a **climate veto**: it under-fills and logs rather than painting a cold biome in warm air (`docs/plan_climate_authority.md` §5.4).

  **The solver has NO water branch, and `terrain_tag_targets.Water` is INERT.** Water share is an
  elevation outcome: the mask is a pure threshold and the contour anchor already places
  `target_land_pct` exactly. `elevation-authority` deleted the branch outright — it converted arbitrary
  land tiles to `DeepOcean` (and ocean back to `Tundra`/`AlluvialPlain`) **with no elevation term at
  all**, which is precisely how a "water" tile ended up above sea level. Listing `Water` in
  `locked_terrain_tags` no longer does anything.

  The target is kept only so the tag census has a reference figure, and should still track
  `1 − macro_land.target_land_pct` for that reading to be meaningful (earthlike `0.62`;
  polar_contrast was corrected `0.64 → 0.58` against its `target_land_pct = 0.42` during the arc —
  the map had been right and the target stale). *Historical:* when the branch existed, a mismatched
  target made the solver invent bathymetry the pipeline never modeled — earthlike's old `Water = 0.65`
  vs `target_land_pct = 0.38` would have drowned ~125 `COASTAL` tiles. That failure mode is now
  structurally impossible rather than avoided by convention.

**Per-Map Biome Palette** (`biome_palette.rs`, design `docs/plan_biome_palette.md`): a curated,
seed-driven, map-size-scaled subset of the 37 biomes chosen at world-gen time — small maps read
legibly, large maps stay rich, and the full library is preserved for replay variety. **This is how
maps generate now, not an opt-in mode.** Each biome carries an intrinsic `BiomeNiche` (8-way
partition) + `must_have` flag (`terrain.rs` `biome_niche`/`biome_must_have`, folded into
`TerrainDefinition` by `def`). The `BiomePalette` resource is built in `spawn_initial_world` from
`world_seed ^ PALETTE_SEED_SALT`: per niche it keeps the `must_have` members and seed-samples up to
`K` (size-interpolated from the preset's `biome_palette` block — `small_map_tiles`/`large_map_tiles`
+ per-niche `k_small`/`k_large`), then force-includes the solver's locked-tag fallback biomes.
Enforcement is a **climate-aware niche-nearest remap** (`BiomePalette::remap(biome, is_polar)`): at
the `bias_terrain_for_preset` seam and again in the post-solver `apply_biome_palette_clamp` system
(inserted in the Startup chain right after `apply_tag_budget_solver`), any off-palette biome is
replaced by an allowed member of the same niche — polar tiles only remap to POLAR-tagged members, so
the palette never stamps temperate plains/marshes at the poles; `RiverDelta` is `must_have` so real
river mouths pass through. **Must-have set** (`biome_must_have`, 9): DeepOcean, ContinentalShelf,
InlandSea, AlluvialPlain, PrairieSteppe, Tundra, RiverDelta, Glacier, **NavigableRiver** (the last
for the same reason as `RiverDelta` — it is hydrology-placed, and off-palette it would remap to
`DeepOcean` and cut the continent in half with open sea; adding it gave the Ocean niche a **fourth**
must-have, so earthlike's Ocean `k_large` was widened 4 → 6 to keep the two *interchangeable* ocean
flavours, CoralShelf and HydrothermalVentField, reachable at all). `must_have` is reserved for a
single *physically-gated* member inside an otherwise-thinnable niche: `InlandSea` in Ocean (else
off-palette inland water renders as DeepOcean) and `Glacier` in PolarLowland (else a tall polar peak
remaps down to flat Tundra — it's the polar analog of AlpineMountain, placed only where relief clears
`alpine_relief_threshold`). **Physically-gated-vs-interchangeable principle** (`docs/plan_biome_palette.md`
§3.2b): thinning only ever applies to interchangeable flat-land climate/flavor niches. The fully
physically-gated niches — `Highland` (relief/elevation/mask regimes) and `Volcanic` (volcanic-arc
mask) — are **never thinned**: their palette `K` is set to full membership at both endpoints
(`Highland` 5/5, `Volcanic` 3/3, in the `BiomePaletteConfig` default + earthlike JSON), so AlpineMountain
and every highland/volcanic member is always available and never remapped away. Un-thinning Volcanic
never forces volcanoes onto a non-volcanic map (the niche is simply absent with no arc + no fumarole
hit). Do **not** add other highland biomes to `must_have` — the niche's full `K` already keeps them
always-available while staying tunable. Reconciled with the
tag solver by construction (force-included fallbacks) plus the clamp as insurance. Also revives 3
previously-unreachable biomes (`§3.6`): Glacier (high-relief polar mountains), BasalticLavaField
(low-relief volcanic mask via `terrain_classifier.basaltic_relief_threshold`), AquiferCeiling (one of
the six anomaly biomes) — so "all 37" is now literal. **Anomaly rarity:** anomaly/"discovery" biomes
(crater/sinkhole/karst-cavern/fumarole/volcano/aquifer) are gated in `classify_terrain` by a config
lever `terrain_classifier.anomaly_fraction` (default 0.04 — ~4% of eligible flat lowland, split evenly
across the six), replacing the old fixed 6-of-16 (~37%) slice that blanketed the land. **Niche note:** BorealTaiga is homed in `PolarLowland` (not `FertileLowland` as
the design table lists) because it is POLAR-tagged — see the comment on `biome_niche`. Biome ids are
unchanged (no client/schema impact). Independent of terrain-texture work.

---

