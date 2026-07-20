# Climate Authority — temperature decides the biome, not latitude

**Status:** design, not yet implemented.
**Branch:** `worktree-polar-boundary`.
**Sibling arc:** `docs/plan_elevation_authority.md` — this is the same shape of fix one layer up.
Elevation authority made the *land mask* a derived function of the heightfield; this makes the
*biome* a derived function of the climate.

---

## 1. What the player sees

A tile card reading **`Climate: Polar`** while the terrain is **`AlluvialPlain`** — a freezing
fertile floodplain. Reported from play, 2026-07-20.

It is not a rendering glitch and not a one-tile fluke. It is one visible instance of a systemic
disagreement, and the disagreement runs in **both** directions:

- cold tiles wearing temperate biomes (the reported case), and
- **polar biomes sitting in warm air** — tundra at 20°C — which is the *larger* of the two.

---

## 2. Root cause: two answers to "how cold is this tile?"

| | inputs | where |
|---|---|---|
| **Temperature** | latitude base − elevation lapse (≤ 12°) + element jitter | `climate_temperature`, `systems/worldgen.rs:332` |
| **The biome ladder** | **raw latitude only** | `terrain.rs:687`, `terrain.rs:865` |

The biome classifier **never looks at elevation.** So:

- High ground gets cold without ever becoming eligible for a cold biome.
- Ground near the poles gets a cold biome whether or not it is actually cold.

Both failures follow directly, and neither is reachable by tuning `polar_latitude_cutoff` — the
cutoff is a latitude, and the problem is that latitude is the wrong variable.

---

## 3. Measurements

20 runs — earthlike + polar_contrast × 5 seeds × {80×52, 120×78} — **55,398 land tiles**.
`cool_min = 3.0` read from `clients/godot_thin_client/src/config/tile_climate_config.json`;
polar membership keyed on `TerrainTags::POLAR`, never a biome-name list.

### 3.1 Cold tiles wearing temperate biomes

**3,847 tiles — 6.9% of land** (per-run range **2.8%–15.6%**).

| biome | count | share |
|---|---|---|
| CanyonBadlands | 1375 | 35.7% |
| **AlluvialPlain** | **795** | **20.7%** |
| PeatHeath | 605 | 15.7% |
| AlpineMountain | 589 | 15.3% |
| RollingHills | 306 | 8.0% |
| HighPlateau | 102 | 2.7% |
| KarstHighland / PrairieSteppe / other | 75 | 2.0% |

Temperature spread: `min −15.11, p25 −1.67, p50 +0.32, p75 +1.57, max 2.99`.
**45% are below 0°C** — only 17.8% sit in the 2–3° band where this could be dismissed as rounding.

**Cause split:**

- **(b) high elevation at mid/low latitude — 3,226 (83.9%).** The dominant case. Every one of the
  795 `AlluvialPlain` tiles is here. Worst pure-elevation instance: `AlpineMountain @(25,41)` at
  **−1.58°C** with `dist_from_equator = 0.304`, comfortably inside the temperate band.
- **(a) latitude-band disagreement — 621 (16.1%).** Not the band originally hypothesised: 97% is
  `PeatHeath` at **row 0** — the pole itself — at −5 to −15°C. That is a separate, narrower bug
  (§7.1), not the elevation story.

### 3.2 Polar biomes in warm air (the reverse case — larger)

**4,397 tiles — 7.9% of land.** `p50 = 8.06°C`, `max 20.5°C`, **41% above 10°C**.

| biome | count |
|---|---|
| Tundra | 2334 |
| BorealTaiga | 1601 |
| SeasonalSnowfield | 310 |
| PeriglacialSteppe | 57 |
| RiverDelta | 95 |

**Only 36% are inside the polar latitude band.** The other **64% are the tag budget solver's polar
family adds** at temperate latitudes — it paints `Tundra` to hit a `Polar` tag target with **no
temperature check at all**.

> That is the **repaint-to-hit-a-quota** pattern this codebase has rejected twice already (the land
> mask's `rebalance_land_ratio`, and the proposal to make river navigability a percentile). It
> survives here because nobody had looked at the polar family. A target share is legitimate only as
> an **input to generation**, never as a reassignment applied afterward.

---

## 4. The design principle

> **Temperature is the climate authority. A biome's climate eligibility is a derived function of
> the tile's temperature — never of its latitude.**

Latitude remains an *input to temperature*, where it belongs. It stops being a second, parallel
answer that can disagree with the first.

Consequences a new stage must respect:

- A tile that is cold gets cold biomes, wherever it is. **Alpine tundra becomes expressible** (§5.3).
- A tile that is warm does not get polar biomes, however close to the pole it sits.
- The tag solver may not add a climate-gated biome to a tile whose temperature forbids it. If a
  tag target cannot be met without violating climate, the **target is not met** and that is
  reported — it is not met by lying.

---

## 5. What changes

### 5.1 The gate moves from latitude to temperature

Six sites share the latitude cutoff today and must move together:

| site | gates |
|---|---|
| `core_sim/src/terrain.rs:687` | base classifier's polar biome ladder |
| `core_sim/src/terrain.rs:865` | `is_polar_lat` → mountain becomes Glacier / SeasonalSnowfield |
| `core_sim/src/systems/worldgen.rs:251` | palette remap `is_polar`, prototype loop |
| `core_sim/src/systems/worldgen.rs:923` | `bias_terrain_for_preset` |
| `core_sim/src/systems/worldgen.rs:2180` | `apply_biome_palette_clamp`, post-solver |
| `core_sim/src/map_preset.rs:900,948` | the config field itself (default 0.35) |

The two palette sites are load-bearing: `BiomePalette::remap(terrain, is_polar)` is the
climate-safety rule, and if it keeps deciding `is_polar` by latitude the post-solver clamp will
re-stamp temperate biomes onto cold tiles and undo the fix.

**Ordering is not a blocker** — verified, not assumed. Terrain is assigned in the prototype loop
(`worldgen.rs:~200–280`) and temperature computed in a second loop at `:332`, but
`climate_elevation` (`:305`) derives from `bands.elevation` / `base_elevation_field`, **both already
available before the prototype loop** — terrain classification itself consumes elevation. Only code
arrangement is in the way, not data.

### 5.2 One threshold, stated once

The sim's biome gate and the client's `Climate:` band must be the **same boundary**, or the tile
card can still show a biome and a climate that disagree — the exact defect this arc exists to
remove. Today the client owns `cool_min = 3.0` in its own
`clients/godot_thin_client/src/config/tile_climate_config.json` and the sim has no equivalent.

The sim must own the authoritative thresholds; the client reads them or is derived from them. Open
question in §8.

### 5.3 Alpine tundra becomes expressible

**A missing capability, not just a mislabel.** Today a mid-latitude mountain at −1.6°C cannot be
tundra, because the classifier only knows its latitude. Once the gate reads temperature, cold
highland *at any latitude* becomes eligible for the cold biome ladder, which is both physically
right and a gameplay gain: high ground stops being uniformly hospitable, and altitude becomes a
real settlement constraint rather than a texture.

This interacts with the palette: `Highland` and `Volcanic` niches are deliberately never thinned
(`docs/plan_biome_palette.md` §3.2b), so their members are always available. The cold-highland
members must be reachable through `remap` under a temperature-derived `is_polar`, and
`biome_palette.rs:162-172` currently treats some niches as climate-neutral — that classification
needs re-checking against the new gate.

### 5.4 The tag solver gets a climate veto

The polar family pass must not stamp a climate-gated biome onto a tile whose temperature forbids
it. Where a target cannot be met, under-fill and report rather than repaint. This is the same
correction the water branch already received in the elevation-authority arc, applied to the polar
family.

---

## 6. Config levers

| lever | today | after |
|---|---|---|
| `terrain_classifier.polar_latitude_cutoff` | 0.35, latitude | **retired** — latitude is no longer the gate |
| *(new)* polar temperature threshold | — | the temperature at or below which the cold ladder applies; must agree with the client's band |
| `climate.equator_temp` / `polar_temp` / `elevation_lapse_span` | 30.0 / −5.0 / 12.0 | unchanged — these now drive biomes too, so they become **more** load-bearing |
| `POLAR_LATITUDE_THRESHOLD` (`systems/mod.rs:80`) | default-bound, see §7.3 | retired with the cutoff |

Note `elevation_lapse_span` becomes a **biome-shaping** lever, not just a temperature one. Raising
it pushes cold biomes further down the mountains. That is the intended coupling, but it means
retuning it is now a worldgen change and must be measured as one.

Neither shipped preset currently overrides `polar_latitude_cutoff` — both run the 0.35 default, so
there is no per-preset migration to do.

---

## 7. Secondary bugs found on the way

Each is **separable** and can land independently of the main arc.

### 7.1 `PeatHeath` is not POLAR-tagged, and appears at the pole

605 tiles, 97% of the "latitude-band disagreement" case, at **row 0** at −5 to −15°C. The tag
solver's wetland pass (`worldgen.rs:1355`, `:1379`) stamps `PeatHeath` inside the polar band, and
`PeatHeath` carries no `POLAR` tag — so it reads as a temperate biome sitting at the pole. Either
the biome should be polar-tagged, or the wetland pass should respect the polar band. Decide which;
do not do both.

### 7.2 `RiverDelta` leaks the underlying biome's tags

95 `RiverDelta` tiles carry `POLAR`. `hydrology.rs:2098` stamps delta terrain but only ORs
`WETLAND | FRESHWATER`, **keeping the underlying biome's tags** — so a delta cut through Tundra
renders as `RiverDelta` while still carrying `POLAR`. That tag then feeds
`BiomePalette::remap(is_polar)`, `food.rs:196`, and the tag census, so the leak propagates into
three unrelated systems. Worth fixing regardless of this arc.

### 7.3 `POLAR_LATITUDE_THRESHOLD` reads the default, not the active preset

`systems/mod.rs:80` binds it to `TerrainClassifierConfig::default_values().polar_latitude_cutoff` —
the **default**, not the preset in play. Both shipped presets use the default, so they agree *by
luck*. A preset that overrode the cutoff would silently desync the tag solver from the biome
ladder, with no error. Latent, and it will be removed with the cutoff itself — but note that
`climate_band_for_position` (`worldgen.rs:1029`) is a **third arithmetic copy** of the same
latitude rule, with its own bare `0.18` temperate/tropical literal and no config lever. Its
consumers (`worldgen.rs:1006`, and the tag solver at `:1354`, `:1378`, `:1412`, `:1470`, `:1529`)
all move with this arc.

---

## 8. Decisions

Settled 2026-07-20. Recorded here because each one closes off an alternative a future reader would
otherwise reasonably reach for.

### 8.1 A band ladder, not a single cut point

Climate is a **ladder of bands** — polar / boreal / temperate / tropical — each with its own
temperature cut point, and the biome ladder keys off which band a tile lands in.

*Why not the simpler single "polar" threshold:* the measured incoherence is **not** confined to the
polar edge. `BorealTaiga` was **1,601** of the 4,397 warm-polar tiles — the second-largest offender
— which is a *boreal-band* problem specifically. A single polar cut point cannot express it, and we
would be back here for the boreal fringe. Cost accepted: four numbers to tune instead of one, and
more of the biome-selection chain moves.

### 8.2 The jitter DOES reach the biome gate — ragged transitions are wanted

The gate reads the **jittered** temperature, so per-tile variation carries through into biome
selection and band boundaries come out ragged.

*Why:* today's latitude gate produces **clean horizontal edges**, and that reads as artificial on a
real map — straight lines of biome change across the world. Natural transitions are patchy. The
±0.6° `element_jitter` is exactly the right magnitude to break a boundary up without moving it.

*Consequence to accept:* scattered single tiles of the neighbouring band along every boundary. That
is the intent, not a defect — do not "clean it up" later without re-reading this section. If it
proves too noisy the lever is `climate.element_jitter_scale`, **not** re-gating on un-jittered
temperature.

### 8.3 The sim owns the thresholds and publishes them

The band cut points live in sim config and ship in the snapshot; the client renders the band it is
told rather than deciding one. The client's own `cool_min = 3.0` in
`clients/godot_thin_client/src/config/tile_climate_config.json` is **retired**.

*Why:* if the client keeps an independent opinion about where "Polar" starts, the tile card can
still show a biome and a climate that disagree — the exact defect this arc exists to remove, merely
relocated. Precedent: the sim already publishes `seaLevel` on the elevation overlay so the client
need not re-derive it (`docs/plan_elevation_authority.md`).

### 8.4 The secondary bugs land in this arc

All three of §7 ride along rather than being split out. §7.1 and §7.3 are polar-classification bugs
that the main change touches anyway, and §7.3 disappears outright when the latitude cutoff is
retired. §7.2 (the `RiverDelta` tag leak) is independent but small, and its tag feeds
`BiomePalette::remap(is_polar)` — which this arc re-keys — so landing it separately would mean
touching the same predicate twice.

### 8.5 Still to be measured, not decided

**How much the map changes.** Biome distribution will move everywhere, not just at the poles: every
worldgen regression baseline will move, and the biome palette's niche-membership assumptions need
re-checking against the new gate. Measure before committing to cut-point values — and re-pin
baselines from measurement, never widen a tolerance to absorb the drift.

---

## 9. What this arc does **not** do

- It does not change the temperature model's own inputs (`equator_temp`, `polar_temp`,
  `elevation_lapse_span` keep their values) — only what *consumes* them.
- It does not touch hydrology, the land mask, or the mountain mask.
- It does not attempt to hit any biome-share target. If the measured outcome is that cold biomes
  become more or less common, that is the honest consequence of tying them to temperature, and the
  response is to tune the **climate inputs**, never to repaint the output.
