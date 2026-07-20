# Elevation Authority — Terrain Must Follow the Heightfield

## Problem

Water tiles sit above sea level and land tiles sit below it. On a sampled
`earthlike` map: **543 of 2601 water tiles above sea level** (deep ocean reaching
28 on the client's 0–100 relative-height readout) and **218 of 1559 land tiles
below it**. The mirror-image counts are the tell — this is symmetric drift, not a
one-way corruption.

The client is not at fault. `MapView.gd:2437-2445` computes

```
height = round(clamp((raw/65535 - seaLevel) / (1 - seaLevel), 0, 1) * 100)
```

and floors everything at or below sea level to 0. A deep-ocean tile reading 12 is
an honest report of a raster that really does place that tile above sea level.

### Root cause: elevation is an input that later stages overrule

Every coastline mutator in `mapgen.rs` has the same signature shape:

```rust
fn rebalance_land_ratio(
    land: &mut [bool],           // written
    is_ocean: &mut [bool],       // written
    elevation: &ElevationField,  // read-only, always
```

`place_islands`, `adjust_land_tiles`, and `connect_inland_seas_via_straits` are
identical in this respect. **After the heightfield is built, elevation is never an
output of any stage.** Terrain is decided by a boolean mask that consults
elevation and then overrides it, and the published `elevation_overlay` is a
snapshot of the field taken *before* those overrides. Nothing reconciles them.

Specifically, `generate_land_mask` (`mapgen.rs:643`) takes a sea-level parameter
and **never uses it** — the binding is literally named `_sea_level`. Instead:

1. Tiles are scored `elevation + jitter` (`jitter = 0.18` on earthlike — nearly
   half the entire ocean→land elevation spread).
2. The top-scoring, spaced-apart tiles become `continents` seeds.
3. Land grows by priority-flood BFS from those seeds until each continent hits a
   fixed **area target** summing to `target_land_pct`.

Two failure modes follow directly:

- Growth is **quota-driven**, so it swallows tiles below sea level to reach
  `desired_land` → sunken land.
- Growth is **seed-reachable-only**, so high ground not connected to an expanding
  front stays ocean at any height → the 335 open-ocean high tiles (of 387 high
  deep-ocean tiles, only 14 are land-adjacent, ruling out the tag solver's drown
  branch as the main contributor).

### The intended invariant already exists, and is broken one function later

`anchor_contour_to_sea_level` (`heightfield.rs:163`) warps the field with a
monotone piecewise-linear map so the `target_land_pct` quantile lands exactly on
`sea_level`. Its doc comment states the goal: *"makes the whole pipeline's 'land
⟺ above sea level' assumption true"*, justified by *"being strictly monotone, it
cannot reorder the field — the land mask's elevation ranking, and therefore the
land it selects, is preserved."*

That reasoning is correct and is then invalidated immediately: the mask does not
rank on elevation, it ranks on `elevation + 0.18 * noise`, which **is** a
reordering. The anchor arranges for the 38% contour to sit at sea level; the mask
then selects a different 38%.

## Measurements

A temporary harness (`mapgen::tests::experiment_threshold_vs_bfs_mask`) compares
the current mask against a pure `elevation > sea_level` threshold on the
`earthlike` preset. Components are 4-connected, matching the BFS's own topology.

```
                land%   components   >=256   <16    largest components
seed 1   thresh  37.5%      61          3      46   12248, 3630, 1955
         current 38.8%     174          4     140    4931, 4318, 4200, 3384
seed 7   thresh  37.5%      45          3      37   16424, 1254, 467
         current 38.0%     127          1     116   17907, 71, 70
seed 42  thresh  37.4%      43          3      29   15612, 1265, 971
         current 38.0%     128          2     117   12270, 5741, 69
seed 1234 thresh 37.6%      33          2      21   16795, 1157
         current 39.1%     179          3     159   14416, 2337, 666
seed 99991 thresh 37.3%     65          2      47   16800, 628
         current 39.3%     166          2     143   17150, 694
```

Three conclusions:

1. **`target_land_pct` is already satisfied by the field alone** — 37.3–37.6%
   against a 38% target, with no rebalancing. The anchor works. `rebalance_land_ratio`
   and the tag solver's drown branch are correcting an error that isn't there.
2. **The BFS worsens fragmentation** — 127–179 components and 116–159 sub-16-tile
   specks, against 33–65 and 21–47 for the threshold. Roughly 3× the fragments and
   4× the specks, none of it elevation-derived.
3. **The BFS's continent guarantee is illusory** — landmasses ≥256 tiles come out
   at 4, 1, 2, 3, 2 (current) versus 3, 3, 3, 2, 2 (threshold). Indistinguishable,
   and on seed 7 the current path yields a single 17,907-tile Pangaea despite
   `continents: 4`. Seeds grow together and merge.

Point 3 is decisive: the machinery responsible for the entire decoupling does not
deliver the property it exists to deliver.

## Design principle

> **Elevation is the sole authority. The land mask is a derived, pure function of
> the heightfield — `land[i] = elevation[i] > sea_level` — never stored and
> edited. Anything that wants to move a coastline edits the field and re-derives.**

Corollaries:

- A water tile above sea level becomes **unrepresentable**, not merely rare.
- `target_land_pct` is met by *shaping the field* (the existing anchor), never by
  repainting tiles.
- `continents` is met by *shaping the field*, not by growing boolean blobs.

`continents` and `min_area` remain preset config (`map_presets.json` →
`macro_land`); this arc changes the mechanism that honors them, not their status
as levers.

## Implementation

### Phase 1 — Continental structure in the heightfield

Thresholding raw fractal noise gives one dominant supercontinent (see the
measurements: a single component holds 12k–17k of ~18.5k land tiles). Deliberate
continent separation must therefore be *added to the field* before it becomes a
new deficiency.

In `heightfield.rs`, before erosion and anchoring, add a low-frequency
continental bias:

- Choose `macro_land.continents` centers deterministically from the world seed,
  Poisson-spaced (reuse the spirit of the existing seed-spacing rule, in
  continuous coordinates, honoring `wrap_horizontal`).
- `bias(x, y) = max_i(falloff(dist_i / radius))`, mapped to `[-1, 1]` so inter-continental
  gaps are pushed **below** sea level rather than merely being less high. `max`,
  not sum, so adjacent centers do not fuse into a land bridge.
- `elevation = fractal + continental_weight * bias`.

Then `land_contour` + `anchor_contour_to_sea_level` run as they do today, so
`target_land_pct` continues to hold exactly, by construction, for free.

New preset levers under `macro_land` (no bare literals — see repo convention):
`continental_weight`, `continental_radius`, `continental_falloff_exponent`.
`min_area` becomes a *rejection/reseed* criterion on the derived mask (regenerate
centers if a landmass lands under it) rather than a growth quota.

### Phase 2 — Derive the mask

- `generate_land_mask` becomes `land[i] = elevation.sample(i) > sea_level`. The
  `_sea_level` parameter becomes real; jitter, seeds, area targets, and the
  priority-flood all go.
- The coastline raggedness the jitter was reaching for moves into the heightfield
  as a genuine high-frequency noise term, applied **before** `land_contour`. There
  it is harmless: the anchor runs on the field that is actually thresholded, so it
  perturbs the coastline without decoupling anything.

### Phase 3 — Retire the mask mutators

- Delete `rebalance_land_ratio` and `adjust_land_tiles` (Phase 1 makes them inert).
- `place_islands` writes **elevation** — seamounts raised above sea level — then the
  mask is re-derived. Island count/size stay config-driven.
- `connect_inland_seas_via_straits` lowers a corridor **below** sea level, then
  re-derives.
- `apply_tag_budget_solver` (`worldgen.rs:1285-1340`) loses its water branch
  entirely; water share is an elevation outcome. This also retires the warning
  comment at `map_presets.json:94` about the solver "inventing bathymetry the
  pipeline never modeled."
- Hydrology's `NavigableRiver` / `RiverDelta` stamps are water-tagged tiles on land
  by design. These are **freshwater** and legitimately sit above sea level; the
  invariant below is therefore scoped to salt water via `is_ocean`.

### Phase 4 — Sea-level provenance

The sampled snapshot shipped `sea_level = 0.6` while `earthlike` specifies **0.62**
(`map_presets.json:9`). `ElevationField::new` resets to `DEFAULT_SEA_LEVEL = 0.6`
(`heightfield.rs:32`) and `restamp_elevation` returns through it (`mapgen.rs:2690`);
`worldgen.rs:146` re-attaches the preset value via `.with_sea_level()`.

**Static analysis says current source is correct**, and the discrepancy is so far
unreconciled. Ruled out: `MapPresets::get` has no defaulting fallback (plain map
lookup); `ElevationField` has no `Default` impl and is not `init_resource`d, so the
capture's `Res<ElevationField>` can only be the one worldgen inserted; the shipped
`simulation_config.json` does name `map_preset_id: "earthlike"`, which resolves.
The remaining hypothesis is that the sampled server was a **stale binary or a build
from another worktree** — the export's `preset` field echoes `config.map_preset_id`
regardless of what was actually loaded, so it is not evidence the preset resolved.

Resolve empirically by restarting the stack from this worktree and re-exporting.
Regardless of outcome, harden: replace the hardcoded `.unwrap_or(0.6)` at
`worldgen.rs:95` with `DEFAULT_SEA_LEVEL`, and log a warning when the preset id
fails to resolve rather than silently falling back — a silent fallback here also
disables erosion and contour anchoring entirely (`heightfield.rs:128`), which would
be a far larger defect than the sea-level offset.

Also replace the hardcoded fallback `.unwrap_or(0.6)` at `worldgen.rs:95` with
`DEFAULT_SEA_LEVEL` so the two cannot drift.

Under the current code this is a ~5-point display offset. Once the threshold *is*
the map, it moves the actual coastline — so it must land with this arc, not after.

### Phase 5 — Invariant test

A regression test asserting, across several seeds and every preset:

- no `is_ocean` tile has `elevation > sea_level`
- no land tile has `elevation <= sea_level`
- realized land% is within tolerance of `target_land_pct`
- landmass count ≥ `min_area` respects `continents`

The first two are tautological once the mask is derived — which is the point.
They are cheap insurance against a future stage reintroducing a mask edit.

## Postscript — drainage, and what this arc did *not* break

Making the mask honest removed an accidental supercontinent, and with it the only reason
navigable rivers ever formed. Measured across 4 configs × 5 seeds plus a pre-arc baseline:

| config | largest landmass | basins | max accum | coherence ratio |
|---|---|---|---|---|
| shipped (w0.5 r0.35 rough0.05) | 513–1311 | 60–127 | 82–126 | 0.076–0.164 |
| roughness 0.0 | 512–1276 | 67–148 | 66–111 | 0.070–0.150 |
| weight 0, roughness 0 | 431–1318 | 52–211 | 42–81 | 0.042–0.099 |
| **pre-arc (BFS)** | 387–1847 | 48–170 | 43–84 | **0.045–0.111** |

The coherence ratio is **unchanged pre- and post-arc**. Two hypotheses were tested and
both refuted by direct measurement: `coastline_roughness` (removing it changes nothing)
and the continental bias term (removing it changes nothing). Max basin tops out at ~5% of
landmass in *every* era.

Pre-arc navigable rivers were a **lottery**: 0, 1, 1, 6, 5, 1 segments across six seeds,
with a single 41.7%-basin outlier (against a 2–8% norm) carrying most of them. The old
BFS's ~1,580-tile supercontinent supplied the raw area; correctly-sized ~400-tile
continents do not, and 5% of 400 is 20 against a discharge threshold of 25.

**So this is not a regression.** The arc removed a bug that was masking a pre-existing
drainage deficiency. Zero navigable rivers on a small landmass is honest emergent
behaviour. The real defect — the surface being 95% fragmented into micro-basins that drain
straight to the coast — predates this work and is tracked in `TASKS.md` → "Capture: the
divides, not the valleys."

**A trap this arc walked into, recorded so the next one doesn't:** `continental_weight` /
`continental_radius` apply a *radial* falloff — a dome. `TASKS.md` had already identified
noise-dome continents as the thing that sheds radially and prevents trunk rivers. The bias
term makes `continents` a real lever (it does, measurably) but is dome-shaped by
construction and so cannot produce trunk rivers. Replacing it with tilted / warped relief
is the follow-on arc.

**Rejected approach, recorded so it is not re-proposed:** making navigability a percentile
of the accumulation distribution ("top N% of drainage is navigable"). That guarantees a
fixed river share on any terrain — a quota applied to the output, which is exactly the
pattern this arc exists to delete. Rivers must emerge from the field. If more rivers are
wanted, the input to change is basin coherence or landmass size.

## Consequences

- **Every seed changes.** Map fixtures and any test asserting on specific hexes
  must be regenerated.
- **Server-only arc.** The client already consumes the overlay correctly; no
  Godot-side work is required.
- Fragmentation and speck-island counts should *improve* (measurements above),
  but continent aesthetics need a visual pass before this is called done.
