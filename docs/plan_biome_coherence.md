# Biome Coherence — fix the checkerboard, stop deleting biomes

**Status:** design, scheduled. Measured, not theorised — `integration_tests/tests/biome_speckle.rs`.

**Supersedes the coherence half of** `docs/plan_biome_palette.md`. The palette itself survives, with a
much narrower job.

---

## 1. The problem the palette was built for

Small maps came out as a **checkerboard of single-hex terrain**. The per-map biome palette
(`biome_palette.rs`) attacks that by **thinning the vocabulary**: each `BiomeNiche` keeps its
`must_have` members and seed-samples up to `k` others, and everything off-palette is **remapped to a
niche-mate**.

The reasoning was sound — fewer biome types, fewer possible neighbours, bigger blobs.

## 2. It does not work. Measured.

`biome_speckle.rs`, earthlike, Tiny (56×36), seed 11 — the exact case the palette exists for:

| | `FertileLowland k=2` (thinned) | `k=4` (full membership) |
|---|---|---|
| land hexes with **no** same-biome neighbour | **22.2%** | 24.5% |
| singleton regions | **63.4%** of all regions | 65.8% |
| **median same-biome region size** | **1 hex** | **1 hex** |

**The median biome region is ONE HEX — with the thinning on.** Two thirds of all regions are
singletons. The checkerboard is fully intact.

Thinning `FertileLowland` from 4 members to 2 buys **2.3 percentage points** of speckle. What it costs
is documented in §3: **every forest and every river floodplain on the map.**

**Why it fails:** speckle is a **spatial** property, not a vocabulary one. Shrinking the palette
doesn't make the map coherent — it makes the confetti *monochrome*. The tiles still flip biome hex to
hex; there are just fewer colours to flip between.

## 3. What it costs — and the deeper bug

`FertileLowland` has exactly **four** members: `AlluvialPlain`, `PrairieSteppe`, `Floodplain`,
`MixedWoodland`. The first two are `must_have`. So `k_small = 2` admitted **only the must-haves** and
silently deleted the other two from **every Standard map**:

- **`MixedWoodland`** — the map's only canopy/forest biome.
- **`Floodplain`** — stamped by **hydrology, at river valleys**. That is *physical gating*, the very
  property that already makes `RiverDelta` a `must_have`.

And they did not merely vanish: the clamp **remaps an off-palette biome onto a niche-mate**, so both
were folded into **`AlluvialPlain`** — which is why the tag solver's fallback biome carries **37–48% of
all land**, and why the pasture map reads so uniform. *The fallback biome has been eating the forests
and the river valleys.*

### 3.1 The niche is grouped by the wrong thing

`FertileLowland` groups by a **tag** — "fertile + lowland", a claim about soil and elevation. But its
members do **completely different things**:

| | graze (animals) | forage (humans) |
|---|---|---|
| `PrairieSteppe` | **high** | low |
| `MixedWoodland` | **low** | **high** |
| `AlluvialPlain` | mid | **high** |

They are not interchangeable *at all*. Thinning that niche was never "swap one grass for another
grass" — it was **deleting an entire food-profile from the world** and remapping it onto grass. That is
why thinning `PolarLowland` felt safe (its members genuinely *are* alike) and this one quietly wrecked
the map.

---

## 4. The fix: two mechanisms, cleanly separated

The palette conflates **coherence** with **variety**. Split them.

### 4a. Coherence — a de-speckle pass (this is what actually fixes the checkerboard)

A **minimum contiguous region size**: after biome stamping (and after the tag solver / palette clamp,
so it has the last word), absorb any same-biome region smaller than `min_region_hexes` into its
**dominant neighbouring biome**. Iterate to a fixed point — dissolving one island can leave another.

- **It works at any vocabulary size**, so coherence stops costing biomes. That is the whole point.
- It is a **morphological opening**, the standard tool for exactly this.
- **Protect the physically-gated:** `RiverDelta`, `Floodplain`, `Glacier`, volcanic and the anomaly
  biomes are *placed by a physical process*, not by climate noise. A lone delta hex at a river mouth is
  **correct**, not confetti. Those are exempt (they already carry the `must_have` / physical-gate
  markers the palette uses).
- Config: `min_region_hexes` (start at 2–3), per preset.
- **Acceptance:** island fraction `< 5%` on Tiny (from ~22–25% today), median region size well above 1,
  and **zero biomes deleted**. `biome_speckle.rs` is the acceptance test; tighten
  `MAX_ISLAND_LAND_FRACTION` when it lands.

**Cheaper first, if it turns out to be enough:** the speckle may be *generated* rather than merely
un-cleaned — a high-frequency term in the climate/moisture fields flipping adjacent hexes across a
classifier boundary. Check the classifier's noise scales before building the pass; smoothing the input
field is a smaller change than a post-pass. Measure both.

### 4b. Variety — re-cut the niches by FOOD PROFILE

Keep the palette. Give it the one job it is actually good at (making map A feel different from map B),
and make it **incapable of costing anything**:

> **Two biomes are interchangeable if and only if they produce the same thing.**

That is now a **testable** property, not a judgement call — graze and forage capacity are numbers
(`docs/plan_grazing_foundation.md`). Re-cut `BiomeNiche` along the food-web profile — *grassland*
(high graze / low forage), *woodland* (low graze / high forage), *wetland-alluvial* (mid / high),
*barren* (low / low), *water* — and then:

- **Thinning inside a niche becomes genuinely free.** Swapping `SavannaGrassland` for `PrairieSteppe`
  changes the art and the name and *nothing about the game*. That is what "interchangeable" was always
  supposed to mean.
- **The palette can be aggressive again** — it can no longer cost you a food profile.
- **`must_have` mostly dissolves.** It was a patch for "this niche contains members that aren't
  actually alike." Fix the grouping and most of the patch is unnecessary.

---

## 5. Sequencing

The `FertileLowland k_small: 2 → 4` change is **already in** (it restores forest + floodplain, and the
measurement above shows it costs ~2 points of speckle we were never buying anything with anyway). It is
a stopgap that stops the bleeding; it is **not** this plan.

1. **Diagnose** — is the speckle *generated* (classifier noise frequency) or merely *un-cleaned*?
   Measure before building.
2. **De-speckle pass** (§4a) + tighten the `biome_speckle.rs` guard to the 5% target.
3. **Re-cut the niches by food profile** (§4b); relax `k` and shrink `must_have` to the genuinely
   physically-gated.

**Ordering note:** ①+② are independent of the grazing arc and can land any time. ③ *depends on* the
per-biome graze/forage tables (`plan_grazing_foundation.md`), because those tables **are** the food
profile the niches get cut along.

---

## 6. What must be measured, not assumed

1. **Island fraction and median region size, on Tiny** — Tiny is the case the palette was built for and
   the one that fails hardest. A Standard-map number will flatter you.
2. **That de-speckling deletes no biome.** Count distinct biomes present before and after. The entire
   point is coherence *without* a vocabulary cost.
3. **That the physically-gated survive** — a river delta is one hex at a river mouth by *definition*.
   If the pass eats deltas, it is wrong.
4. **The map still looks right.** This is a rendering judgement in the end: `map_preview` on Tiny,
   before and after.

---

## See Also

- `docs/plan_biome_palette.md` — the palette's original design (its *coherence* rationale is superseded
  here; its variety rationale stands).
- `docs/plan_grazing_foundation.md` — the two food webs; §4b's niche re-cut is defined by those tables.
- `integration_tests/tests/biome_speckle.rs` — the measurement, and the acceptance test.
- `core_sim/CLAUDE.md` → World Generation Pipeline → Per-Map Biome Palette.
