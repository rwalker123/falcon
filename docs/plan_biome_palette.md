# Per-Map Biome Palette (world-gen)

Status: **design**. Owner: world-gen. Scope: **server-side `core_sim` only**.
Independent of the terrain-texture work — all 37 textures still exist; this only changes
*which biomes get placed* on a given map. **This replaces the current biome-placement behavior —
the palette is how maps generate, not an opt-in mode.** Delivered as **one PR** (design doc +
types + selection/enforcement + solver reconciliation + the 3 biome revivals + tests).

See also: `core_sim/CLAUDE.md` → "World Generation Pipeline" (the prose pipeline reference),
`docs/architecture.md`, `TASKS.md`.

---

## 1. Problem & goal

Falcon has **37 terrain biomes**. On **small maps**, world-gen lets (nearly) every biome appear
wherever its climate/elevation allows, producing a **busy patchwork of single-tile biomes** that
reads as visual noise and is hard to parse. Deleting biomes would spend replay variety we want to
keep.

Decouple two independent knobs:

- **Total biome library (37)** = content richness / per-playthrough uniqueness. **Keep all 37.**
- **Distinct biomes used *on a given map*** = legibility. **Restrict this, scaled to map size.**

So: each generated map draws from a **curated per-map biome palette** — a subset of the 37 chosen at
world-gen time, sized to the map, coherent across the map's climate range, and **seed-driven** so each
map feels different. A small map → few distinct biomes (readable); a large map → many (rich).

**Hard constraint:** the palette MUST cover the map's actual climate range. Never leave a climate
niche the map produces with no valid biome (tiles would fall back / look wrong). We pick
representatives *across* niches first (coverage), then thin *within* over-represented niches.

---

## 2. What the current generator actually does (the design constraints)

Three facts from the code that shape this design (`terrain.rs`, `systems.rs`):

1. **A large share of the noise is one mechanism.** On flat land, an `anomaly` slice
   (`(tile_noise >> 4) & 0xF`, values 0–4) deterministically claims **~31% of every non-coastal,
   non-polar lowland tile** for the "flavor" biomes — crater fields, sinkholes, karst cavern mouths,
   fumaroles, volcano slopes — *before moisture is even consulted*. These "rare" biomes are actually
   common. Thinning this set is the single highest-value lever.

2. **The existing `biome_weights` cannot exclude.** `bias_terrain_for_preset` (`systems.rs:1014`)
   treats a `0.0` weight as "demote one step along a successional chain", not "exclude". Those chains
   only cover the wet↔arid lowland spine (Floodplain→…→HotDesertErg). **Every highland, volcanic,
   polar, water, and anomaly biome has no chain edge**, so weights can't affect them at all. A real
   restriction needs a **hard allow-set + niche-nearest remap**, not a weight tweak.

3. **The tag-budget solver will undo a naive palette.** `apply_tag_budget_solver` (`systems.rs:1195`)
   runs after classification and re-stamps tiles to hit per-tag coverage targets using a **hardcoded
   per-tag replacement vocabulary** (Coastal→`TidalFlat`, Wetland→`FreshwaterMarsh`, etc.),
   independent of weights or palette. If we exclude a biome whose tag is *locked*, the solver puts it
   back. This must be reconciled explicitly (§6).

Also: classification and the bias seam use **position-only hashes** (no `world_seed`), so the lowland
biome pattern is seed-invariant today — the palette is where seed-driven variety enters. And **3
biomes are currently unreachable** by the generator (Glacier, BasalticLavaField, AquiferCeiling) —
they exist in the library but nothing places them. **This work revives them** (§3.6): the palette is
a filter, not a placer, so each gets a small placement hook so it becomes a real palette candidate.
"Keep all 37" then means literally 37, not the effective 34 today.

---

## 3. Design model

### 3.1 Niche = an explicit per-biome partition

Tags overlap (RollingHills is HIGHLAND+FERTILE), so a tag partition is not disjoint. Instead, assign
each of the 37 biomes **exactly one `BiomeNiche`** — an intrinsic biome property. The palette then
**thins within each niche** and **guarantees coverage across** the niches the map actually spans.

Proposed taxonomy (8 niches). `must_have` biomes (§3.2) are **bold**:

| Niche | Biomes | Notes |
|-------|--------|-------|
| `Ocean` | **DeepOcean**, **ContinentalShelf**, **InlandSea**, CoralShelf, HydrothermalVentField | open water + coast |
| `CoastWetland` | **RiverDelta**, TidalFlat, MangroveSwamp, FreshwaterMarsh, PeatHeath | water-adjacent lowland |
| `FertileLowland` | **AlluvialPlain**, **PrairieSteppe**, Floodplain, MixedWoodland | the fertile spine + solver fallback |
| `AridLowland` | HotDesertErg, RockyReg, SemiAridScrub, SaltFlat, OasisBasin, AshPlain | dry lowland (AshPlain is the very-dry pick) |
| `PolarLowland` | **Tundra**, PeriglacialSteppe, SeasonalSnowfield, **Glacier**, BorealTaiga | high-latitude lowland (BorealTaiga is POLAR-tagged — homed here, not FertileLowland, so an off-palette boreal tile remaps to Tundra, not temperate AlluvialPlain; **Glacier** is `must_have` — the extreme-relief member, §3.2b) |
| `Highland` | RollingHills, HighPlateau, AlpineMountain, KarstHighland, CanyonBadlands | mask/relief-driven — **physically gated, never thinned** (K = full membership, §3.2b) |
| `Volcanic` | ActiveVolcanoSlope, FumaroleBasin, BasalticLavaField | volcanic mask/anomaly — **physically gated, never thinned** (K = full membership, §3.2b) |
| `Anomaly` | ImpactCraterField, SinkholeField, KarstCavernMouth, AquiferCeiling | subsurface/hazard "discovery" flavor |

**Multi-path placement (impl note, resolve in phase 1):** a few biomes are placed by more than one
mechanism — `ActiveVolcanoSlope` (volcanic mask **and** anomaly slice 4), `AshPlain` (very-dry
lowland pick, but VOLCANIC-tagged), `SeasonalSnowfield` (polar pick **and** polar mountains). Each
gets **one** niche for palette membership (the table above is the proposal); the *placement paths*
still exist, they just remap to an allowed biome when their pick is off-palette. The `Anomaly` niche
is the user-requested "lump the discovery-flavor biomes together and pick N" group — note that the
anomaly *slice* also currently reaches KarstCavernMouth/FumaroleBasin/ActiveVolcanoSlope, so the
phase-1 remap for the anomaly slice draws from `Anomaly ∪ Volcanic` allowed members.

### 3.2 `must_have` = a per-biome flag

Some biomes must remain reachable on **every** map regardless of palette/seed — they anchor a niche
or are a solver/successional fallback. Add a `must_have: bool` to each biome's definition. Initial
set: **DeepOcean, ContinentalShelf** (open water + coast), **InlandSea** (lakes/inland seas —
without it an off-palette InlandSea tile falls to the first allowed Ocean member, `DeepOcean`, so
inland water renders as ocean), **AlluvialPlain, PrairieSteppe** (fertile spine + the solver's
universal fallback), **Tundra** (polar), **RiverDelta** (river mouths, stamped only by hydrology),
**Glacier** (the extreme-relief polar member — the polar analog of AlpineMountain, placed only on
polar tiles whose relief clears `alpine_relief_threshold`; without it a tall polar peak remaps down
to flat Tundra). These are always in the palette and count toward — but are never dropped from — their
niche's chosen set.

Per §6, the palette **also** force-includes the solver's fallback biome for every *locked* tag in the
active preset (computed at palette-build), so the solver can never reintroduce an off-palette biome.
That set is preset-dependent, so it's derived at build time rather than baked as `must_have`.

### 3.2b Physically-gated vs. interchangeable — the thinning principle

Thinning a niche means the palette allows only a seed-sampled subset of its members and remaps the
rest onto that subset. This is only safe when the members are **interchangeable** — climate/flavor
variants of the same lowland regime, where stamping one where another would have gone reads as a
plausible map, not a bug. It is **not** safe for **physically-gated** biomes: each maps to a specific
relief/moisture/mask regime, so *any* palette swap between them puts the wrong biome on a physically
specific tile (a towering Fold spine rendered as RollingHills, or a gentle plateau lifted to an
AlpineMountain). The legibility win comes entirely from thinning the interchangeable flat-land niches
(CoastWetland / FertileLowland / AridLowland / PolarLowland flats / Anomaly).

Two mechanisms keep the physically-gated content intact:

- **Whole physically-gated niches are un-thinned via full `K`.** `Highland` (relief/elevation/mask
  regimes) and `Volcanic` (volcanic-arc mask) set `k_small = k_large = full membership`, so every
  member is always allowed and nothing in the niche is ever remapped away. This is a **config** lever,
  not `must_have`, so a future per-map-type could re-thin if a real design reason emerges. Un-thinning
  Volcanic never forces volcanoes onto a non-volcanic map: the niche is simply *absent* (no member
  placed) on maps with no volcanic arc and no fumarole anomaly hit.
- **A single extreme-relief member inside an otherwise-thinnable niche is `must_have`.** The Ocean and
  PolarLowland niches keep their interchangeable flats thinnable, but their one physically-specific,
  extreme member is pinned: **InlandSea** in Ocean (inland water must not read as DeepOcean) and
  **Glacier** in PolarLowland (a tall polar peak must not read as flat Tundra). `must_have` is reserved
  for exactly this surgical case — do **not** add the other highland biomes to `must_have`; un-thinning
  their niche via `K` already keeps them always-available while staying tunable.

Naively making a highland member `must_have` on a small map backfires: with the niche's `K` rounding
to 1 it becomes the *only* allowed highland, so gentle plateau/hill tiles remap *up to* it — the exact
inverse failure. Un-thinning the whole niche (every member allowed, none remapped) is the correct fix.

### 3.3 K sized to a whole-map "distinct biome budget" — all config

Per-niche count `K` scales with map area, fully driven by the preset config so it's a one-file tweak:

```
K_niche = round( lerp( k_small, k_large, smoothstep(area_t) ) )       // per niche
area_t  = clamp( (tiles - small_map_tiles) / (large_map_tiles - small_map_tiles), 0, 1 )
K_niche = clamp( K_niche, must_have_count(niche), reachable_count(niche) )
```

- A **small** map → each spanned niche gets ~its `k_small` (often just the `must_have` anchor → very
  readable, near one biome per climate zone).
- A **large** map → approaches every reachable biome (rich).
- `K` is floored at the niche's `must_have` count (coverage) and capped at how many biomes in that
  niche the map's climate can actually produce (never invent an empty niche).

### 3.4 The palette-selection algorithm (seeded, coverage-first)

Run **once**, at world-gen start, right after `world_seed` is resolved. Deterministic from
`palette_seed = world_seed ^ PALETTE_SEED_SALT`:

1. **Determine spanned niches.** From the map's climate/elevation envelope (latitude span, elevation
   range, whether it has ocean/coast/mountains/polar rows), mark which niches the map will produce.
   (Phase-1 detail: derive from the same signals `terrain_for_position_with_classifier` consumes, or
   conservatively mark all niches spanned and let per-niche `reachable_count` clamp K.)
2. **Per niche, choose `K_niche` biomes:** start with the niche's `must_have` members, then sample the
   remainder from that niche's candidates (seeded shuffle) up to `K_niche`. The `Anomaly` niche is the
   "pick N discovery biomes" case — small K, seed-varied, so each map has its own signature set of odd
   sites.
3. **Force-include solver fallbacks** for every locked tag (§6).
4. Store the result as a `BiomePalette` resource: the **allow-set** (which biomes) + a **niche-nearest
   remap** helper (given an off-palette biome, return the nearest allowed biome in its niche — walk the
   successional chain when on the lowland spine, else nearest by a small niche-local ordering).

### 3.5 Enforcement — two plug-in points

- **At classification output (`bias_terrain_for_preset`, `systems.rs:366-368`):** after the existing
  weight/climate logic, if the resulting biome ∉ palette, replace it with `palette.remap(biome)`. This
  is where the anomaly-slice biomes, highland biomes, etc. — none of which the weight chains can touch —
  get restricted.
- **Solver reconciliation (§6).**

### 3.6 Reviving the 3 unreachable biomes

Glacier, BasalticLavaField, and AquiferCeiling exist in the library but no code path places them
today. The palette **restricts** which placed biomes survive; it can't make a never-placed biome
appear. So each gets a minimal **placement hook** in the classifier so it becomes a genuine candidate
in its niche (then the palette governs it like any other member):

- **Glacier** (`PolarLowland`) — add to the polar pick for the coldest/highest polar tiles (very high
  `dist_from_equator`, or polar + high real elevation), alongside Tundra/SeasonalSnowfield.
- **BasalticLavaField** (`Volcanic`) — add to the volcanic mask output next to `ActiveVolcanoSlope`
  (e.g. a cooled-flow variant on lower-relief volcanic tiles) in `select_mountain_terrain`.
- **AquiferCeiling** (`Anomaly`) — one of the six anomaly biomes the rarity gate cycles across (see
  the anomaly-rarity note below), so the subsurface set includes it.

These hooks are small and self-contained; the placement *rates* stay low (they're flavor), and the
palette's per-niche K keeps them from over-appearing. Because they're now placed, they also drop off
the "unreachable" list — a latent content bug fixed in passing.

**Anomaly rarity is now a config-driven fraction.** The original classifier turned **6 of 16**
(`~37.5%`) of eligible flat lowland into an anomaly biome (a fixed `(noise >> 4) & 0x0F` slice `0..=5`
*before* the humidity ladder), so anomalies blanketed the land and the palette then concentrated the
excluded ones onto a few survivors (Karst Cavern Mouths everywhere; volcanic-slice tiles inflating
High Plateau). The gate is now a **rarity roll** on a fresh, disjoint hash field
(bits 16–23) compared against `terrain_classifier.anomaly_fraction` (default **0.04** — 4% of eligible
lowland); only tiles that pass become an anomaly, and they split evenly across the six anomaly biomes
via a separate selection field. Total anomaly coverage is ≈ `anomaly_fraction` of eligible lowland, so
they read as genuinely rare "discovery" sites while all six (incl. the revived AquiferCeiling) stay
reachable. The fraction is a per-preset lever (wired explicitly into `earthlike`).

---

## 4. Config schema

### 4.1 Per-biome (Rust `TerrainDefinition`, `terrain.rs`)

Two intrinsic fields added to each biome's `def(...)`:

- `niche: BiomeNiche` — the §3.1 partition (new enum).
- `must_have: bool` — §3.2.

These live with the biome definition because they're intrinsic to the biome, not the map. (Biome
defs are a hardcoded Rust table today; this keeps them there. A future migration to a biomes JSON is
out of scope.)

### 4.2 Per-preset (`map_presets.json` → `MapPreset`)

A new `biome_palette` block. The palette is **always applied** (it's the generation, not a mode); this
block only **tunes the per-niche counts**. If the block is absent, sensible built-in defaults apply:

```json
"biome_palette": {
  "small_map_tiles": 2016,
  "large_map_tiles": 10240,
  "niches": {
    "Ocean":          { "k_small": 2, "k_large": 4 },
    "CoastWetland":   { "k_small": 1, "k_large": 4 },
    "FertileLowland": { "k_small": 2, "k_large": 5 },
    "AridLowland":    { "k_small": 1, "k_large": 4 },
    "PolarLowland":   { "k_small": 1, "k_large": 3 },
    "Highland":       { "k_small": 5, "k_large": 5 },
    "Volcanic":       { "k_small": 3, "k_large": 3 },
    "Anomaly":        { "k_small": 2, "k_large": 4 }
  }
}
```

Defaults above are illustrative starting points, chosen so a small map reads ~one biome per climate
zone plus a couple of discovery-flavor anomalies (per the design intent), and a large map fills back
out. Because it's config, tuning is a JSON edit — no rebuild-of-logic.

The `small_map_tiles`/`large_map_tiles` anchors are the selectable map presets (client
`MapPanel.gd`): **Tiny 56×36 = 2016** tiles anchors `k_small` (the most legible), **Huge 128×80 =
10240** anchors `k_large` (the richest), and **Standard 80×52 = 4160** lands partway up the smoothstep
curve between them. Anchoring the span to the actual selectable range (rather than a non-selectable
256×192 = 49152 upper bound) is what lets Huge actually reach the near-full `k_large` counts instead of
clustering every size at the sparse end.

`Highland` (5/5) and `Volcanic` (3/3) are set to their **full membership at both endpoints** — they
are physically relief/elevation/mask-gated niches that must never be thinned (§3.2b), so their `K`
never rounds down to a partial set that would remap towering tiles onto the wrong biome. The
size-legibility delta therefore comes entirely from the interchangeable flat-land niches
(CoastWetland / FertileLowland / AridLowland / PolarLowland flats / Anomaly / Ocean).

---

## 5. Determinism / seed

- **Seed the palette *selection*** from `palette_seed = world_seed ^ PALETTE_SEED_SALT` (the repo's
  established `world_seed ^ <domain const>` convention, e.g. `mapgen.rs`). Different seeds ⇒ different
  allow-sets ⇒ different biome sets per playthrough — this is most of the "unique per map" value.
- **Leave the per-tile hashes position-only** for now. The differing allow-set already yields strong
  per-seed variety without destabilizing terrain *shape* (which already varies via elevation/moisture/
  mountains). Seed-varying the per-tile intra-niche pick is noted as a **future lever**, not phase 1.

---

## 6. Tag-budget-solver reconciliation (the critical interaction)

The solver (`systems.rs:1195`) re-stamps tiles for *locked* tags using a hardcoded per-tag biome
vocabulary, independent of the palette. Reconciliation, in order of preference:

1. **Force-include locked-tag fallbacks in the palette** (primary): at palette-build, add the solver's
   replacement biome(s) for every tag in the preset's `locked_terrain_tags` to the allow-set. Then the
   solver's stamps are always on-palette by construction. (Today `earthlike` locks only
   Water/Fertile/Wetland → DeepOcean, Floodplain/AlluvialPlain, FreshwaterMarsh/PeatHeath — a small
   set.)
2. **Post-solver clamp** (insurance): a final pass after `apply_tag_budget_solver` remaps any stray
   off-palette biome to `palette.remap(...)`. Cheap, and catches anything a future locked tag or edge
   path introduces. **Recommended to include** so the palette is a true invariant.
3. (Rejected as primary) rewriting the solver's replacement picks to be palette-aware — more invasive
   and duplicates the remap logic; #1 + #2 give the same guarantee more simply.

`RiverDelta` remains solver-protected as today (it's `must_have` anyway).

---

## 7. Where it plugs into the pipeline

Pipeline order (Bevy `Startup .chain()`, `core_sim/src/lib.rs:519-531`):
`spawn_initial_world → hydrology → apply_tag_budget_solver → sites/herds`.

- **Build the `BiomePalette` resource** in `spawn_initial_world` (`systems.rs:185`) right after
  `world_seed` is resolved (`systems.rs:246-263`), threaded down like `preset`/`world_seed`.
- **Enforce at the bias seam** (`systems.rs:366-368`) — niche-nearest remap of off-palette biomes.
- **Reconcile the solver** — §6 (#1 build-time, #2 a new post-solver clamp system inserted after
  `apply_tag_budget_solver` in the chain).

---

## 8. Delivery (one PR, logical steps)

Delivered as a **single PR** (this doc + all code). The steps below are the logical build order, not
separate PRs:

1. **Types + config**: `BiomeNiche` enum, `niche`/`must_have` on `TerrainDefinition` (with the §3.1
   assignments + §3.2 flags), the `biome_palette` `MapPreset` block + JSON parse. Unit tests: every
   biome has a niche; the `must_have` set is covered.
2. **Revive the 3 biomes** (§3.6): placement hooks for Glacier / BasalticLavaField / AquiferCeiling,
   with tests asserting each is now reachable.
3. **Palette selection + enforcement**: the `BiomePalette` resource + seeded coverage-first selection
   (§3.4), the bias-seam remap (§3.5), the solver reconciliation (§6, including the post-solver clamp).
4. **Tune the defaults**: set the `k_small`/`k_large` defaults so a small map reads clean and a large
   map stays rich; validate via the biome-histogram export (§9).

The palette is active for every generated map from this PR on — there is no legacy fallback to
preserve.

---

## 9. Verification

Via the map-export + biome-histogram tooling (`export_map` command / the `export-map` skill / the
client Terrain tab's Export Map + biome histogram):

- **Small map → few distinct biomes** (readable); **large map → many** (rich).
- **Palettes vary by seed** (different biome sets across seeds at the same size).
- **Climate coverage preserved** — no spanned niche is empty; ocean/coast/fertile/polar/highland all
  represented when the map spans them.
- **Solver can't reintroduce** an excluded biome (post-clamp holds): export a map with a locked tag +
  a palette that would exclude that tag's fallback, confirm the fallback is force-included and no
  off-palette biome appears.
- **The 3 revived biomes are reachable**: a large map (or a targeted classifier unit test) actually
  places Glacier, BasalticLavaField, and AquiferCeiling.
- **No off-palette biome anywhere**: scan a generated map's histogram — every present biome is in the
  computed palette (the palette is a hard invariant of the final map).

---

## 10. Open questions for implementation

- **Spanned-niche detection** (§3.4 step 1): derive precisely from the climate envelope, or
  conservatively mark all niches and lean on `reachable_count` clamping? (Lean conservative first;
  refine if a niche is wrongly starved.)
- **`reachable_count(niche)`**: computed from the map's climate envelope, or approximated per niche?
- **Niche-nearest ordering** for off-spine biomes (highland/volcanic/anomaly have no successional
  chain): define a small per-niche fallback order in phase 3.
- Whether to also seed-vary the intra-niche per-tile pick (§5) once base variety is validated.
