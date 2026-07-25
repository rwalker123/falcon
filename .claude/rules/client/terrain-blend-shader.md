---
paths:
  - "clients/godot_thin_client/assets/terrain/*.gdshader"
  - "clients/godot_thin_client/src/scripts/ui/{TerrainRenderer,RiverEdges}.gd"
  - "clients/godot_thin_client/tools/blend_probe.gd"
---

<!-- Extracted verbatim from clients/godot_thin_client/CLAUDE.md lines 502-1446.
     Routing table and shared vocabulary live in clients/godot_thin_client/CLAUDE.md.
     Regenerate with scripts/split_claude_md.sh -->

# The terrain blend shader — edge blending, shore, canopy, peaks, rivers

## Edge Blending — per-pixel biome-blend shader (Approach B)
When `use_edge_blending` is enabled, biome **seams** blend per-pixel in a **fragment shader**
(`assets/terrain/terrain_blend.gdshader`): a symmetric **height blend** (texture splatting) where the two
biomes interlock across the boundary — each texture competes on its own per-pixel height, so one settles
into the *cracks* of the other. It is neither a gradient blur (blur ghosts on detailed textures) nor a
dither (see below). It is deliberately narrow in scope: a biome blends only against biomes of its **own
blend_class**, and only the *flat* and *water* classes blend at all; every other seam — rugged, and every
class change (notably the land↔water shoreline) — stays a **crisp hard edge**. Approach B replaced the earlier baked-overlay
dither (Approach A), fixing its three caveats: **symmetric** mutual intrusion (a tie at the exact edge via
signed distance), **no tiling** (world-space noise varies per hex), and **cleaner grain**.

**Eligibility — SAME CLASS (`blend_class`, config `terrain_config.json`):** every terrain carries a
`blend_class` of `flat` | `water` | `rugged` (id-map G channel: 0 water / 1 flat / 2 rugged, named
`CLASS_WATER`/`CLASS_FLAT`/`CLASS_RUGGED` in the shader). Blend fires for an edge **only** when both
sides share the **same blendable class** and their terrain ids differ:
- **flat↔flat** (grass↔soil ecotones) → blends.
- **water↔water** (deep_ocean ↔ continental_shelf ↔ inland_sea …) → **blends**. Two adjacent ocean
  depths are a gradient, not a cliff; before this rule the `water` class forbade *all* water blending
  and deep-vs-shelf showed razor-sharp hexagon silhouettes.
- **land↔water** (a CLASS CHANGE) → **hard**. That seam is the **shoreline**, owned by the foam/beach
  pass; softening it would wash the coastline out. This is the whole reason `water` is its own class —
  but the old gate over-reached and also banned water↔water.
- **rugged↔anything** → hard (forests/hills/mountains/volcanic — never bleed discrete-object textures),
  **unless `blend_rugged_land` is on** — see below.

`MapView._terrain_is_flat` / `_blend_class_code` read a cached `_terrain_blend_class` map
(`_build_terrain_blend_class_map`); `TerrainTextureManager.blend_class_for` mirrors it.

**`blend_rugged_land` — the RUGGED-LAND gate (config bool, `terrain_config.json`, **SHIPPED `true`**;
`EDGE_BLEND_DEFAULT_RUGGED_LAND` in `ui/TerrainRenderer.gd` → the shader's `blend_rugged_land` uniform).** Under
the bare same-class rule a rugged biome's BASE FLOOR never blends, so it ends in a razor-straight
hexagon against its neighbour — and for a **peak** biome that floor is the *whole* ground under the
relief overlay (`rolling_hills`' base is plain grass; the mounds are a `peaks/` overlay), which is the
"rolling hills look CUT OFF at the hex edge" report. This gate widens the **land** half of the rule from
*same class* to *both sides are land*: flat↔rugged and rugged↔rugged blend through the **existing** flat
levers (no new tuning), so a hills/alpine hex feathers into its neighbour instead of cookie-cutting.
**land↔water is untouched** (still hard — that seam is the shoreline) and water keeps its depth field,
so it is **bit-identical** on every frame with no rugged hex (verified: `blend_bands_full`,
`blend_isolated_shipped`, `V7_coast_unchanged` and `V10_shore_dark_land_closeup` all byte-compare equal
with it on).
It shipped only after the **whole rugged roster** was swept for SHREDDING (the height term tearing holes
in a structured texture's interior — high-contrast rugged art is exactly what is at risk): **`blend_probe`
state R** renders EVERY rugged biome as an **ISOLATED hex surrounded by a contrasting one**, in a flat
field *and* in a rugged field, gate OFF vs ON. All held — interiors stay solid, only the rim feathers,
including the extreme-contrast cases (white `fumarole_basin` on dark rocky_reg; black `basaltic_lava_field`
and white `karst_highland` on orange `canyon_badlands`). **A straight band seam cannot show shredding —
never judge this gate on one.** What it *does* cost is that a high-contrast rugged pair (bright karst
against orange canyon) now reads as a wide hazy ecotone rather than crisp geology; that is a look call,
not a tear.

**WATER IS A DEPTH FIELD, NOT A SEAM** (the fix for the "dark patches of open water with hard
full-hexagon edges, FoW off" report). A hex's water id is a **quantized sample of a continuous seafloor**,
and the real map's ocean is **salt-and-pepper**, not clean blobs: a live 80×52 snapshot's id-map carried
**2332 deep_ocean↔continental_shelf hex adjacencies** and **16 deep hexes whose six water neighbours were all
a different id**. Under the flat↔flat *nearest-edge* seam blend such a hex can only ever read as a **dark
hexagon** — the rim feathers, but the interior keeps the (far darker) deep texture and the silhouette IS the
hex. That artifact is **TERRAIN, not the FoW tint**: with FoW off the shader never reads the vis-map at all
(`fow_enabled` gates the whole block) and `_rebuild_terrain_shader_maps` writes vis = 255 everywhere, so
**fog off already means every hex renders fully lit** — no mist, no dim, nowhere in the client (the CPU path's
`_visibility_state_at` returns `""` → `Color.WHITE`, and the overlay-color path is `_fow_enabled`-gated too).
So water takes its **own branch**: every qualifying water neighbour (same class, different id) contributes
**at once**, weighted by how close the fragment is to **that** shared edge — the same 6-neighbour cross-edge
weighting the FoW softening uses — and the result is the **normalized weighted mean** of the water textures.
The weight reaches 1 exactly **at** a shared edge, so the mean there is `(own + nb)/2` read from BOTH sides:
continuous across every boundary by construction. The flat↔flat interlock is **untouched** and water no longer
takes it. Verify with `blend_probe` **state 9 (X)** below.

**The three water levers** (`terrain_config.json` → `water_blend` block:
`blend_width` **0.45** / `blend_soft` **0.45** / `blend_noise_amount` **0.45**, vs the land
0.25/0.35/0.30; fallbacks are `WATER_BLEND_DEFAULT_*` in `ui/TerrainRenderer.gd`, pushed as the
`water_blend_band`/`water_blend_soft`/`water_blend_noise_amount` uniforms). They keep their names but, under
the depth field, they mean:
- `blend_width` → the field's **REACH**: how far into the own hex a neighbour's depth still bleeds.
- `blend_soft` → the **PLATEAU**, as a fraction of that reach: how far back from the shared edge a neighbour
  already carries FULL weight. **This is the lever that dissolves the hexagon** (a pure ramp only softens its
  rim). Capped in-shader by `WATER_FIELD_PLATEAU_MAX` (0.5) so a ramp always survives — a plateau spanning the
  whole reach would put a hard step at the reach's outer edge.
- `blend_noise_amount` → the amplitude of the world-noise displacement of the depth boundary (in reach units),
  so the depth contour meanders organically instead of tracing hex geometry. Sampled in map space, so a world
  point reads the same value from both sides of an edge — continuity survives it.

The wobble **cell** (`blend_noise_scale`) and the height nudge stay **shared with land** — a finer cell would
speckle, and the height term is a no-op on smooth low-variance water anyway.

**Mechanism — whole-map shader quad + hex splatmap:**
- `terrain_blend.gdshader` (canvas_item) is drawn as **one whole-map rect** by a dedicated child
  node `TerrainBlendQuad` (`show_behind_parent = true`, so it renders BEHIND MapView's grid/markers —
  a separate node is required because a canvas item's ShaderMaterial applies to *all* its draw
  commands). `TerrainRenderer._setup_terrain_blend_shader` builds it once; `TerrainRenderer.update_shader_quad`
  pushes uniforms each frame. Per fragment the shader **inverts the pointy-top odd-r hex layout**
  (MUST match `MapView._hex_center`/`_axial_center`/`_offset_to_axial` + the `hex_origin`/`hex_radius`
  uniforms exactly — this is the alignment contract with grid lines/selection/markers), reads its
  hex's biome from the **`sampler2DArray`**, and — if its class is blendable (flat or water) — checks the
  6 neighbours (wrap-aware) for a **same-class, different-id** biome; near the nearest qualifying shared
  edge it **height-blends** the neighbour's
  array sample in. The seam weight is **symmetric**: `p = clamp(0.5 + signed_dist_to_edge /
  (2·blend_band), 0, 1)` is 0.5 at the edge on both sides.
- **THE INVARIANT: the seam is ALWAYS a continuous weighted mix, never a 1-bit pick.** The splat weight
  `p` **leads**; two enveloped perturbations only **bend** it; a `smoothstep` feathers the result into a
  mix factor:
  ```glsl
  float env = 4.0 * p * (1.0 - p);                          // dies at both ends of the band
  float h   = (luma(own) - mean_luma(own_layer)) - (luma(nb) - mean_luma(nb_layer));
  float pw  = clamp(p + ((noise - 0.5) * blend_noise_amount - h * blend_height_influence) * env, 0.0, 1.0);
  result    = mix(own, nb, smoothstep(0.5 - blend_soft, 0.5 + blend_soft, pw));
  ```
  The **wobble** (world `vnoise`, cell `blend_noise_cell`) gives an organic, meandering boundary instead
  of the straight hex line, and carries **low-variance pairs** (smooth sand ↔ smooth soil) where there is
  little detail to follow. The **height term** is a *detail-following NUDGE*: with no height maps each
  texture's **zero-centred luminance** is its pseudo-height (`luma(rgb) − mean_luma(layer)`, Rec.709; the
  per-layer means come from `TerrainTextureManager.layer_luma_texture`, a 1×N single-channel texture
  fetched by layer index — **zero-centring is essential**, or a bright biome always out-heights a dark one
  and the seam collapses to one side), and it bends the boundary toward the darker/lighter side so it
  follows the textures' own tufts and grains. `blend_height_influence` **must stay small** (≤
  `EDGE_BLEND_MAX_HEIGHT_INFLUENCE` = 0.5) so it can never out-vote the distance weight. The `4·p·(1−p)`
  envelope guarantees no perturbation can leak neighbour texture into a hex interior nor leave a straight
  discontinuity at the band's outer edge.
- **Rejected alternatives (do NOT reintroduce)** — the first two are the SAME BUG (a 1-bit pick) in two
  disguises: (1) the **dither** (`result = neighbour if p > vnoise(...)`) — a binary pick makes every pixel
  100% one biome, so the seam can only ever be **discrete hard-edged blobs**; the user's verdict on the live
  game was "the blobs are too big… I shouldn't really even notice the blending, but it is very obvious". No
  noise tuning fixes it (coarse noise → chunky blobs, fine noise → pixel shimmer) — *the approach was the
  bug*. (1b) **Height blending with `blend_height_influence` 4.0 + a small overlap depth** (`blend_depth`,
  now gone): the luma term (±0.3 × 4 = ±1.2) **dwarfed** the 0..1 distance weight, so it degenerated into
  winner-takes-all-by-luminance — wherever prairie was dark, soil won outright, *deep inside the hex*. The
  user's verdict: prairie hexes looked **shredded**, "this isn't a blend at all". A straight band seam looks
  fine under this bug — **only an isolated hex surrounded by the other biome exposes it**, which is exactly
  what `blend_probe`'s isolated-hex state renders. (2) A plain
  linear crossfade — it ghosts two detailed textures over each other. (3) A 3-octave "wander" noise +
  an S-curve on `p` (tried under the dither) — big smooth lobes.
- **Base biome UV — CONTINUOUS world space** (like the canopy pass, NOT per-hex-normalized): the base
  biome is sampled at `base_uv = v_map / (2·hex_radius) · base_scale` (`v_map = v_world - hex_origin`,
  pan/zoom-anchored), so **one texture tile spans ~`1/base_scale` hex-rows** and adjacent hexes show
  DIFFERENT regions of it. This kills the **per-hex identical-repeat grid** (with diagonal seams) that
  any *detailed* (non-homogeneous) base texture used to show when each hex was mapped to one whole
  centered copy — invisible on homogeneous grass/water, obvious on a rocky/alpine texture. The
  **flat↔flat height blend samples the neighbour biome at the SAME `base_uv`** (only the array layer differs),
  so the cross-edge interlock stays continuous (two world-sampled biomes at one world point). `repeat_enable`
  tiles the array. The canopy pass already sampled this way; the base now matches it.
- **id-map splatmap** (`_rebuild_terrain_shader_maps`, per snapshot): a `grid_w × grid_h` **RGBA8**
  texture, R = terrain id, G = `blend_class` code (0 water / 1 flat / 2 rugged), B = canopy code
  (0 none, else canopy layer + 1), A = 255, NEAREST-sampled. A
  companion **R8 vis-map** carries FoW state (0 unexplored / 0.5 discovered / 1 active).
- **Config levers (all fallbacks mirrored as `EDGE_BLEND_DEFAULT_*` consts in `ui/TerrainRenderer.gd`):**
  - `blend_width` (**0.25** → `blend_band = blend_width · radius`, the half-band in px) — the **REACH**, i.e.
    the width of the ecotone. The user wants a **shallow** transition confined to the hex edge, so it is
    small: `0.25·radius` ≈ 19px at the on-screen r≈75, a band that never reaches a hex interior.
  - `blend_soft` (**0.35**, capped at `EDGE_BLEND_MAX_SOFT` = 0.5) — the **FEATHER SOFTNESS**: the
    smoothstep's half-width in seam-weight units. **Small** (≈0.03) ⇒ the mix snaps wherever the
    noise/detail carries the weight past 0.5 → a fine crisp **stipple**; **large** (≈0.35) ⇒ a smooth
    **gradient** the noise only leans. Floored in-shader (`BLEND_SOFT_MIN`) so it can never become a hard step.
  - `blend_height_influence` (**0.25**, hard-capped at `EDGE_BLEND_MAX_HEIGHT_INFLUENCE` = 0.5) — the
    detail-following **NUDGE** (see the invariant above). Typical zero-centred luma deviations are ±0.3, so
    0.25 moves the weight by ≤ ~0.08 — a fraction of the 0..1 distance weight it perturbs. `0` = a pure
    distance+noise feather. **Never raise it past the cap**: at 4.0 it out-voted the distance weight and
    shredded hex interiors (see Rejected alternatives).
  - `blend_noise_scale` (**0.25**, a **fraction of the hex radius** → the `blend_noise_cell` px uniform) —
    the **WAVELENGTH** of the boundary wobble: ≈19px at r=75, i.e. a few organic lobes per hex edge, which
    is what stops the seam reading as the straight hex polyline. Very fine (≈0.05) turns it into a
    per-pixel speckle instead (which only reads as a boundary at all when `blend_soft` is also tiny).
  - `blend_noise_amount` (**0.3**) — the wobble's amplitude, **added to** the seam weight (never
    thresholded against it — this is not a dither) and enveloped so it dies at both ends of the band.
  - `blend_rugged_land` (**true**, shipped) — the rugged-land eligibility gate (see the gate above). It
    changes only *which* seams blend, never *how*: rugged land reuses the five levers above verbatim.
  - **PER-TERRAIN `blend_profile` — because ONE ecotone does not fit every PAIR.** The five levers above are
    the GLOBAL ecotone, and they are tuned for the biome pairs that actually border each other: neighbours a
    few brightness points apart that share a hue. Their visible ramp is only ~`0.35·r` wide and the wobble
    displaces it by a fraction of that, so the boundary still essentially **traces the hex polyline** — which
    is invisible between two tan grasslands and *glaring* between two textures far apart in **both tone and
    hue**. The `NavigableRiver` **bank** (id 37) is exactly that: grey, low-contrast gravel (mean luma **89**)
    whose neighbours in a river corridor are prairie/scrub (**112–127**) on one side and floodplain/alluvial
    (**55–58**) on the other. Under the global levers alone the corridor renders as a **chain of grey
    hexagons** — the blend fires correctly, it is simply far too narrow and too straight to read as an
    ecotone at that contrast. **This is NOT fixable with the global levers** (widening them to suit the bank
    would move every biome seam main tuned), so a terrain entry may carry an optional block scaling the seams
    **it** is on, along three axes — the flat↔flat twin of the water side's `shore_profile`:
    `{ "id": 37, …, "blend_profile": { "width_scale": 2.6, "noise_scale": 2.2, "noise_cell_scale": 2.6 } }`
    * `width_scale` multiplies `blend_band` — the ecotone's **REACH**.
    * `noise_scale` multiplies `blend_noise_amount` — the boundary wobble's **AMPLITUDE**, so the boundary
      leaves the hexagon instead of tracing it.
    * `noise_cell_scale` multiplies `blend_noise_cell` — the wobble's **WAVELENGTH**. **Amplitude without
      wavelength is a fine fringe on a straight line**, not a meander: the lobes must scale with the (now
      wider) band. The two noise axes move together.
    * **CROSS-EDGE AGREEMENT — an edge takes the per-axis `max()` of its two terrains' profiles.** `max` is
      **commutative**, so both hexes flanking a seam derive the *identical* band, amplitude and cell; `p` is
      0.5 at the edge from both frames and the mix stays continuous across it, exactly as under the global
      levers. This is the same discipline that makes `shore_profile` key on the **water** side — if the two
      sides disagreed, the profile would itself draw the hard line it exists to remove.
    * **A terrain with no `blend_profile` is neutral (1, 1, 1) → a BIT-EXACT no-op**, and a seam between two
      unprofiled terrains is `max(1,1) = 1` on every axis. Verified: with only the bank profiled, **239 of
      247** harness frames are **byte-identical** to before it landed — the 8 that move are exactly the 8
      `map_rivers*` frames (i.e. every frame containing a navigable hex, and nothing else). Every
      `blend_bands_*` / `blend_isolated_*` / `V7_*` / `V10_*` / `H_*` / `R_*` / `S_*` / `G_*` / `D*` / `W_*`
      frame and `map_biome_blend` / `map_biome_shore_seam` / `map_swatch` are untouched.
    * Plumbing **mirrors `layer_shore_texture` exactly**: `TerrainTextureManager` packs the profiles into
      `layer_blend_texture` (1×N `FORMAT_RGBAF`, R = width_scale, G = noise_scale, B = noise_cell_scale),
      bound once by MapView as the `layer_blend_map` uniform and fetched in-shader by layer index
      (`blend_profile(layer)` → `vec3`; `edge_blend_profile()` is the `max` over the pair).
      `rebuild_layer_blend_map()` is public and updates the ImageTexture **in place** (so the binding
      survives) — that is how `blend_probe` state **17 (BANK)** sweeps it. Fallbacks are the
      `BLEND_PROFILE_DEFAULT_*` consts; `BLEND_PROFILE_MAX_SCALE` (4.0) guard-rails the reach, since the
      apothem is only 0.866·r and a wider band would collide with the opposite seam.
    * **Shipped:** only `navigable_river` (2.6 / 2.2 / 2.6) — chosen on `blend_probe` state 17, which renders
      the corridor against a **dark** field and a **bright** one in ONE frame. `1.8/1.6/2.0` still traced the
      hexagon; `3.4/2.8/3.2` started dissolving the bank's identity as a distinct silty corridor. Judge any
      new profile there, **including the isolated-hex shred crops** — a corridor seam cannot show a torn
      interior.
  - The blend look is **zoom-invariant** (band + wobble are both radius-relative), so a preview frame is an
    honest proxy for the game *only if it is rendered at the game's on-screen hex radius* (**r ≈ 75px**;
    hexes read ~150px across on the user's screen). `tools/blend_probe.tscn` pins that, and — critically —
    renders **isolated hexes surrounded by another biome**, the only state that exposes hex shredding.
    `tools/map_preview.gd` *fits* (r ≈ 83–178) and only ever shows straight band seams, so judgements made
    in it are not trustworthy for the blend.
  - `feature_noise_cell` (default `6.0`, the world-noise cell **px** for the
    shoreline reach/wisp + canopy treeline + peak footline; **decoupled** from the blend noise — it
    drives the shader's `noise_cell` uniform, so the seam can be retuned without moving any
    coastline/treeline/footline; verified by pixel-diff).
  - Top-level `base_texture_scale`
  (→ `base_scale`, default `0.25` = one base texture spans ~4 hex-rows; smaller covers MORE hexes,
  larger fewer — `BASE_DEFAULT_TEXTURE_SCALE` in `ui/TerrainRenderer.gd`). **LOD:** below `EDGE_BLEND_MIN_RADIUS`
  (`= ICON_MIN_DETAIL_RADIUS`) the shader renders base-only (no shimmer at far zoom). **FoW:** the
  shader applies the same discovered-mist multiply / unexplored-fog fill as the per-hex path
  (`_fow_texture_tint_for_state` semantics) via the vis-map — it dims, never drops, the blend. It also
  **softens the mist across hex boundaries** — see Fog-of-war softening below.

**Fog-of-war softening — the hex steps (shader path only).** The vis-map is **per-hex, NEAREST-sampled**
(0 unexplored / 0.5 discovered / 1 active), so reading it raw made every **active↔discovered adjacency a
hard HEXAGONAL brightness step** — straight edges cutting across even *uniform water*, where no terrain
seam exists at all. (This is why "hard straight edges are back" reports must be checked against
`blend_probe` state 5 *before* the blend is touched: the culprit was usually the FoW tint, not the blend.)
Fixed in two halves, both in the shader's `fow_enabled` block:
1. **Smooth the visibility SCALAR across hex boundaries**, reusing the same 6-neighbour
   signed-distance machinery as the blend/shore: each neighbour's visibility is weighted by
   `smoothstep(-fow_soft, 0, d)` — how close the fragment is to **that** shared edge — and the weighted
   mean replaces the raw per-hex value. At a shared edge the neighbour's weight → 1, so `vis → (own+nb)/2`
   from **both** sides — equal, hence **continuous across the boundary**; deep inside a hex all six weights
   → 0 and `vis → own`, so interiors are untouched.
2. **Map `vis` to the tint CONTINUOUSLY** (the old per-state `if` chain was itself a step function):
   `fog_amt = 1 − smoothstep(FOW_UNEXPLORED, FOW_DISCOVERED, vis)` and
   `mist_amt = (1 − smoothstep(FOW_DISCOVERED, FOW_ACTIVE, vis)) · mist_blend`, composited with the
   **existing** `mist_color`/`fog_color`/`mist_blend` uniforms. At the pure states this reproduces today's
   look **exactly** (verified bit-identical: vis 1 = clear, 0.5 = the same mist multiply, 0 = fog fill) —
   only the boundaries change.
- **Optional wispiness:** the smoothed scalar is perturbed by world `vnoise` (reusing `noise_cell`) so the
  fog line reads cloudy rather than a clean arc. It is **enveloped by `|smoothed − own|`** (normalized by
  `FOW_NOISE_EDGE_PEAK`, the 0.25 that a 6-neighbour average can shift the scalar across one state gap), so
  it bites **only at boundaries** and can never tint a pure Active/Discovered/Unexplored interior.
- **Config levers** (`heightfield_config.json` → `fog_of_war`, beside the existing mist/fog colours —
  FoW appearance stays in one place): `fow_softness` (**0.6**, a **fraction of the hex radius** → the
  `fow_soft` px uniform, like `blend_width`, so the gradient is zoom-invariant) and `fow_noise_amount`
  (**0.15**; `0` disables the wisps). Fallbacks are `FOW_DEFAULT_SOFTNESS` / `FOW_DEFAULT_NOISE_AMOUNT` in
  `MapView.gd`. The **per-hex CPU path is unaffected** (it is hard-edged by construction).
- **Verify** with `blend_probe` state 5 → `V8_water_fow_on.png`: on uniform shelf water the mist boundary
  must read as a soft cloudy gradient with **no hexagonal brightness steps**, while pure Active and pure
  Discovered areas are unchanged. State **8 (W)** makes the before/after explicit —
  `W_fow_on_same_terrain.png` (softness `0` = the unsmoothed tint) vs `W_fow_fixed_same_terrain.png`, on two
  adjacent **shelf** hexes across an Active/Discovered boundary. **This is the FIRST thing to render on any
  "hard straight full-hexagon edges are back in open water" report**: the tone-only steps in water are the
  FoW tint, NOT the blend (which `W_fow_off.png` shows already dissolving the deep-ocean silhouette). The
  mist multiply lands exactly on the hex boundary, so it **re-imposes a hard hexagonal edge on water the
  blend has just softened** — and it does so between hexes of the SAME terrain id, where no seam exists.
- **Integration:** the shader is the base-terrain renderer whenever `use_terrain_textures` and no
  overlay and `use_edge_blending` (`_shader_terrain_active`); it **bypasses the CPU map cache** (a
  single cheap GPU draw, so the cache's per-hex-loop purpose is moot). With `use_edge_blending` off,
  the **per-hex texture path** (`_build_hex_texture_cache` / `_draw_hex_textured_direct` +
  `CachedMapRenderer`) renders crisp hard hexes — that is the blend-OFF reference. Overlay/solid
  modes are unchanged.

**Shoreline — ONE continuous coastal profile straddling the coast (universal for now):** separate from the
flat↔flat interlock, every **land↔water** edge gets a coastal treatment in the same shader, reusing the
signed-distance-to-shared-edge machinery. It fires for any edge where **exactly one side is water**
(`blend_class` code 0) — so it's independent of the land side's class (**both flat-land and rugged-land**
coasts get it) and never touches inland edges (flat↔flat interlock and rugged↔* inland edges stay exactly
as before — both sides non-water → skipped). **The one exception is a `NavigableRiver` hex, whose edges are
excluded from the pass entirely — a river meeting the sea is not a coast; see Rivers → NavigableRiver for why
it cannot be expressed as a `shore_profile`.** Seaward read: **land → sand → surf → open water**, and the
requirement is that **NO boundary in that chain is a hard line** — not sand↔land, not sand↔foam, not
foam↔water.
- **THE SIGNED COAST COORDINATE `u` — why this can't step at the hex edge.** The shore pass computes
  `dist_in` = distance from the shared land↔water edge INTO the own hex, which tends to **0 on BOTH sides**
  at that edge. Negating it on the land side gives one coordinate running continuously through the
  coastline: `u < 0` inland · `u = 0` **exactly at the waterline** · `u > 0` seaward. Every shore weight is
  a `smoothstep` **of `u` alone**, so its value at `u = 0` is identical whether the fragment belongs to the
  land hex or the water hex — the profile is continuous across the boundary **by construction**, and no
  term can pop there. (The world-noise wobble that meanders the reaches is sampled in **map space**, so it
  too is the same value on both sides of the edge at a given world point.)
- **The three rejected passes — all the same bug class** (a term saturating AT the hex edge, or sand where
  the user does not want it). (1) A **two-sided** pass (tan beach on the land, foam on the water) with
  LINEAR fades `1 − dist_in/reach`, which are **≈1 AT the shared edge on BOTH sides**: the land went solid
  tan, the water solid white, and they met along the boundary — a **hard tan↔white line TRACING THE
  HEXAGON**. (2) The fix for *that* pushed everything onto the **water side** (`land_beach_width = 0`),
  which killed the sand↔foam line but left the sand **stopping dead at the hex edge against the raw land
  texture** — a **new hard sand↔land line**. (3) Sand on **BOTH** sides (`sand_land_band` + `sand_sea_band`)
  straddling the edge: every hard line was gone, but the beach then read **TWICE AS WIDE** — **sand in the
  water hex is not wanted at all**. Hence the shipped shape: sand is **LAND-ONLY**, and the sand↔foam blend
  is bought by letting the **surf wash INLAND over the beach** instead of by putting sand in the sea.
- **Sand — LAND SIDE ONLY** (`u ≤ 0`; the water hex gets **zero** sand, by construction — the term is
  ternary-gated on the sign of `u`). It is **FULL from the waterline across the surf's inland wash** (the
  **plateau**), then `smoothstep`-fades inland into the biome art over the rest of `sand_band`. Capped at
  `SHORE_SAND_OPACITY` (< 1) so the land art reads through and the beach never looks like flat paint, and
  its reach is deliberately SHORT (0.25·r) so it tints rather than buries the biome.
  **The plateau is anchored to `foam_inland_band`, and that anchor is load-bearing** (`SHORE_SAND_PLATEAU_MAX`
  caps it at 0.6 of the sand reach, so a fade window always survives): the surf is composited **over** the
  sand and peaks at ~1 at the waterline, so wherever the wash is strong the sand is whitewashed and
  contributes nothing. A sand that *also* decayed from the waterline (a plain `1 − smoothstep(0, sand_reach,
  −u)`) was down to ~30% opacity by the time the foam cleared and gone entirely a hair further inland — the
  beach was **invisible** and the coast read **land → surf → water with NO SAND AT ALL** (caught against a
  dark rocky-regolith coast, where white foam met bare rock; **prairie's tan hides this** — always judge the
  beach on a DARK land biome). Holding the sand full across the wash means the **retreating surf uncovers a
  full-strength beach** — that IS the sand↔foam crossfade.
- **THE WATERLINE BASE CROSS-FADE (`waterline_width`) — the last hard seam in the shader, and the reason the
  surf no longer has to be opaque.** Until this existed the **base texture itself stepped at `u = 0`**: on a
  beach coast the (sand-tinted) land met open water with nothing in between; on a **cliff** coast
  (`deep_ocean`, `sand_scale` 0) it was **raw land meeting raw water**. The full-strength foam peak was the
  ONLY thing papering over that flip — which is why **every previous attempt to "just soften the foam"
  re-exposed a hard land↔water line and had to be reverted** (four times). So the base now cross-fades across
  a short reach either side of the coastline: `mixed = mix(land_base, water_base, smoothstep(-w, w, u))`,
  held at full across `±w` and handing back to the true base over `SHORE_WATERLINE_FADE` beyond it.
  * **`land_base` / `water_base` are the SAME weighted-mean-over-{own + 6 neighbours} construction as the
    shore-profile field** (weight `smoothstep(−apothem, 0, d)`, unwobbled, own = 1) — so both are pure
    functions of the **id-map and the world UV**, never of `result` (which carries the own hex's
    interlock/depth-field history). The land hex and the water hex therefore compute the **same pair** at a
    given world point, `mixed` is frame-independent, and at `u = 0` both sides land on it **exactly**:
    continuous across the hex boundary by construction, like every other shore term. See
    `SHORE_PROFILE_REACH_APOTHEMS` for why that mean is exactly continuous.
  * **It is a WET EDGE, not an ecotone.** `waterline_width` **0.14** (·r) sits well under the sand's 0.25, so
    no land texture reads out to sea and no water texture reads up the beach. **Chosen on the foam-off step
    check** (`blend_probe` state **SURF**, cliff coast — the worst case, no sand out there either): 0.08
    already dissolves the step, **0.14** reads as a natural wet-rock rim, 0.20 starts **ghosting land pebbles
    into the water**. `0` disables it bit-exactly (and then `foam_opacity` must go back to 1).
  * **DO NOT envelope it with a ramp that also peaks at `u = 0`.** The first cut multiplied the cross-fade by
    `1 − smoothstep(0, w, |u|)` — two ramps peaking at the waterline — so the water content on the land side
    was already down to **8% at half the reach**: the visible gradient was a **quarter** of the configured one
    (~4px) and **the base step survived**. Hence `SHORE_WATERLINE_FADE`: full weight across the reach, fade
    back to the true base outside it.
- **Surf — peaks AT the waterline and washes BOTH ways.** Inland over the sand across `foam_inland_band`
  (the crossfade that kills the sand↔foam line) and seaward into open water across `foam_band`. **Its peak is
  the `foam_opacity` lever (shipped 0.55)** — and it is a lever *only because the waterline cross-fade above
  removed the base step it used to conceal*. With `waterline_width = 0` the peak is load-bearing again and
  `foam_opacity` must go back to ~1. It scales the **wisp** (`SHORE_WISP_STRENGTH`) too, so the whole surf
  mutes as one gesture rather than the peak fading while the offshore froth stays bright. `1.0` is a
  bit-exact no-op. This is what answers the **"obvious bright white outline on most land"** report: with the
  base step gone the surf can be a translucent highlight instead of an opaque cover-up.
- **Wisp — the faint SECOND surf line out over open water.** Its geometry is **its own pair of
  radius-relative levers** (`wisp_center_width` / `wisp_half_width` → the `wisp_center_band` /
  `wisp_half_band` px uniforms), **not** a multiple of `foam_band` as it once was — that chaining meant the
  surf could not be shortened without dragging the wisp in with it (and the wisp could not be pulled in at
  all). Config is responsible for keeping the wisp band **clear of the surf** (`wisp_center − wisp_half >
  foam_width`) so the two read as two lines; overlap just merges them into one wide white smear.
  `wisp_half_width = 0` turns the wisp off. Only its opacity (`SHORE_WISP_STRENGTH`) stays a shader const.
- **Every falloff is a `smoothstep`** (no linear ramp's slope kink, no hard cutoff anywhere). All reaches
  are noise-modulated by the SAME world-noise wobble (`mix(SHORE_REACH_NOISE_MIN, 1, noise)`, reusing
  `noise_cell`), so the sand's inland edge, the surf line and the wisp meander together as organic fingers
  rather than concentric clean stripes.
- **Config levers** (`terrain_config.json` → `shore` block): `sand_width` (**0.25** — sand reach INLAND of
  the coastline; **land-only**) / `foam_inland_width` (**0.15** — how far the surf washes UP the beach) /
  `foam_width` (**0.41** — surf reach SEAWARD) / `wisp_center_width` (**0.55**) / `wisp_half_width`
  (**0.13**) — the second surf line's centre and half-thickness, so it spans 0.42–0.68·r, clear of the surf
  that dies at 0.41·r — and **`waterline_width`** (**0.14** — the base cross-fade's half-reach; see the
  waterline bullet above). **All six are fractions of the hex radius** → the `sand_band` / `foam_inland_band` /
  `foam_band` / `wisp_center_band` / `wisp_half_band` / `waterline_band` px uniforms (computed in
  `TerrainRenderer.update_shader_quad` like `blend_width`), plus **`foam_opacity`** (**0.55** — the surf's +
  wisp's peak opacity, a unit scalar) and `foam_color` / `beach_color` (RGB 0–255, parsed by
  `MapView._shore_color` into normalized `vec3` uniforms). **`foam_color` ships MUTED — `[176, 194, 205]`, a
  grey-blue** (it was `[223, 242, 247]`, a near-white that read as a hard bright outline at map-scale zoom);
  the recolour alone was rendered as a candidate ("option A") and rejected — it only greys the ring, it does
  not stop it being an opaque ring, because the ring's opacity was structural. Fallbacks are the
  `SHORE_DEFAULT_*` consts in `ui/TerrainRenderer.gd`; the fixed feel-tuning (`SHORE_SAND_PLATEAU_MAX` /
  `SHORE_SAND_OPACITY` / `SHORE_WISP_STRENGTH` / `SHORE_WATERLINE_FADE`) is named consts in the shader. The `land_beach_width` / `sand_land_width` / `sand_sea_width` keys of the rejected passes are
  **gone**. Note the visible beach is intrinsically narrow: the surf covers the inner `foam_inland_width` of
  it, so only the `sand_width − foam_inland_width` annulus (0.10·r) reads as open sand — that is the
  specified geometry, not a bug. LOD-suppressed and FoW-tinted like the rest of the shader (shares the
  `blend_enabled` gate + the vis-map).
- **Verify at the game's hex radius** with `tools/blend_probe.tscn` **state 6 (V10)** — the shipped profile
  on the ragged coast at r≈75 → `V10_shore.png` + `V10_shore_closeup.png` **and `V10_shore_dark_land.png` +
  `V10_shore_dark_land_closeup.png`** (the same coast against **rocky_regolith**). **The dark-land frame is
  the decisive one** — prairie is tan and hides sand-vs-land contrast, which masked the invisible-beach bug
  through several passes. **Judge on the close-ups**: the full frame is downscaled when viewed, which hides
  exactly the 1px line this pass exists to prevent. A coast rendered in a *fitted* harness frame is not a
  trustworthy proxy either (the look is radius-relative — same caveat as the blend). `_render_variant` can
  still sweep the three width levers.
- **PER-WATER-TERRAIN shore profile (`shore_profile`) — A COAST IS NOT ONE THING.** The five levers above are
  the GLOBAL profile, tuned for OCEAN coasts. But the worldgen's water sequence is **deep_ocean →
  continental_shelf → land**: deep ocean *never* meets ordinary land, so where it DOES touch land it is a
  **CLIFF** (no beach at all, full dramatic surf), the **shelf** is the ordinary **beach** (sand, a muted
  wave), and an **`inland_sea`** is a handful of hexes that the ocean profile swamps (its offshore **wisp**
  reads as noise across the middle of a lake). So a WATER terrain entry in `terrain_config.json` may carry an
  optional block scaling the profile of **its own** coastline, along **three independent axes**:
  `{ "id": 1, "name": "continental_shelf", …, "shore_profile": { "sand_scale": 1.0, "foam_scale": 0.75, "wisp_scale": 0.5 } }`
  - `sand_scale` multiplies the beach's INLAND reach (`sand_band`). **`0.0` = no beach at all** (the cliff).
  - `foam_scale` multiplies the MAIN WAVE's reaches **both ways** (`foam_inland_band` = the wash up the beach
    **and** `foam_band` = the surf's seaward reach). **REACH only — the surf's PEAK is the GLOBAL
    `foam_opacity` lever**, not a per-water one (see the Surf bullet above). `foam_scale 0` is not a legal
    profile.
  - `wisp_scale` multiplies the secondary offshore disturbance — its **centre distance, its half-width AND its
    strength** — so it recedes toward the shore and fades as one gesture; `0.0` removes it cleanly.
  - **A water terrain with no `shore_profile` gets the neutral default (1, 1, 1)** —
    `SHORE_PROFILE_DEFAULT_{SAND,FOAM,WISP}_SCALE` in `TerrainTextureManager` — a bit-exact no-op (a partial
    block is legal too: a missing key is neutral on that axis).
  - **Plumbing mirrors the per-layer mean-luminance table** (`layer_luma_texture`): `TerrainTextureManager`
    packs the profiles into `layer_shore_texture`, a **1×N FORMAT_RGBAF** image (R = sand_scale, G =
    foam_scale, B = wisp_scale, one texel per terrain id), bound once by MapView as the `layer_shore_map`
    uniform and fetched in-shader by layer index (`shore_profile(layer)` → `vec3`).
    `rebuild_layer_shore_map()` is public and **updates the ImageTexture in place** (so the binding survives)
    — that is how `blend_probe` sweeps profiles live.
  - **THE PROFILE IS KEYED ON THE WATER, on BOTH sides of the waterline.** A *correctness* requirement, not a
    style choice: every shore weight is one smoothstep of the signed coast coordinate `u` evaluated on both
    sides of the shared edge, so if the two sides read different scales the profile would be discontinuous
    **at the hex edge** — reintroducing exactly the hard line `u` exists to prevent. The water is also the only
    side both fragments can agree on (the land biome varies along a coast; the body of water does not) and the
    meaningful one ("cliff, beach or lake?" is a property of the water).
  - **AND IT IS A CONTINUOUS FIELD, NEVER A NEAREST-PICK** (the fix for what used to be filed here as a "known
    limitation"). One land hex can border a deep_ocean hex **AND** a continental_shelf hex along the SAME
    coastline. Taking the *nearest* water neighbour's profile makes the profile **JUMP at the bisector**
    between them — and with `sand_scale` 0 on one side and 1 on the other that is a **HARD LINE of sand
    appearing out of nowhere along the beach** (it was only a faint seam while all the profiles were similar;
    the cliff/beach split makes it glaring). So **every water hex in `{own + 6 neighbours}` contributes at
    once**, weighted by proximity to **that** shared edge, and the profile is their **normalized weighted
    mean** — the water depth field's discipline. A cliff coast **transitions into** a beach coast over ~a hex
    instead of switching.
    * **The weight** is `smoothstep(−reach, 0, d)` on the signed distance `d` to that neighbour's shared edge
      (own water = weight 1 by construction; land contributes nothing — it has no profile), with `reach` =
      `SHORE_PROFILE_REACH_APOTHEMS` (**1.0**) × the hex **apothem** (the `half_dist` the loop already
      computes). It is deliberately **unwobbled** — a noise displacement here would break the cross-edge
      agreement below.
    * **Why 1.0 apothem is the cap, and why the mean is EXACTLY continuous across every hex edge** (including
      the land↔water one, where it must be, per `u` above). On the shared edge of hexes A|B: (i) a water hex
      **C that neighbours BOTH** reads the *same* signed distance from A's frame and from B's frame — the
      three bisectors meeting at that corner are symmetric under the 120° rotation about it — so both frames
      give C the same weight; (ii) a water hex enumerated from **only one** frame has signed distance
      `≤ −apothem` there, so its weight is exactly **0** and the frame that cannot see it agrees. Raising the
      reach past 1.0 apothem breaks (ii) and re-introduces a step at the hex boundary.
    * **The beach fades out with its own reach** (`sand_fade`): `SHORE_REACH_MIN_PX` floors every reach so no
      fade divides by ~0, but on a cliff coast (`sand_scale → 0`) that floor would keep a **1px, full-strength
      tan hairline** alive at the waterline — and worse, the beach would **POP** into existence at full
      opacity the instant `sand_scale` left 0 as a cliff profile blended into a beach one. So the sand's
      opacity is scaled by `min(sand_reach_raw / SHORE_REACH_MIN_PX, 1)`: exactly **1.0 (a bit-exact no-op)
      for any beach wider than the floor**, and a continuous grow-in from nothing below it.
  - **Shipped:** `deep_ocean` **(0, 1, 1)** — the cliff · `continental_shelf` **(1, 0.75, 0.5)** — the ordinary
    beach, main wave muted, disturbance halved · `inland_sea` **(0.5, 0.5, 0)** — the approved lake. Every
    other water terrain (coral_shelf, hydrothermal_vent_field) is neutral. Per-**LAND**-biome shore gating (a
    grassy shore vs a wooded shore) is still deliberately NOT built — all coasts render the same beach+foam
    art. Verify via `tools/map_preview.gd` State Q (`_biome_band_terrain` carves an ocean bay so the ocean
    borders BOTH prairie and woodland) → `map_biome_blend.png` + `map_biome_shore_seam.png` (coast close-up),
    the lake via `blend_probe` **state 10 (L)**, and the cliff/beach/mixed coasts via **state 15 (D)** below.
  - **NOTE for the next pixel-diff:** because the shipped `continental_shelf` profile is no longer neutral,
    `V7_coast_unchanged` / `V10_shore*` / `H_gate_coast` (whose sea IS the shelf) **moved** when it landed —
    that is the shipped muting, not a regression. They remain the bit-identical reference for any blend
    **eligibility** change; re-baseline them after a deliberate `shore_profile` edit. Frames with no ocean hex
    (`blend_bands_*`, every `H_*`/`S_*`/`G_*`, the `L*` lake) must stay byte-identical through both.

**Canopy overlay — forest = grass floor + overhanging tree crowns:** a forest biome is split into a
**ground layer** that blends like any flat land and a **canopy overlay** of whole crowns that overhang
the hex boundary and thin out, so a forest edge is a natural treeline instead of a razor-cut hex
silhouette. Today the only canopy biome is **12 (mixed_woodland)** — its `blend_class` is now **`flat`**
(the grass floor flat↔flat-blends with prairie and gets a shoreline at coasts, like any flat land); 13
(boreal_taiga) stays `rugged` (no canopy asset yet).
- **Assets:** `textures/base/NN_name.png` is the **forest-floor grass** (trees removed);
  `textures/canopy/NN_name.png` (**new dir**, RGBA crowns on transparency) is the canopy.
- **Second Texture2DArray:** `TerrainTextureManager` builds `canopy_textures` (a companion
  `Texture2DArray` from `textures/canopy/`, same once-only `Image.load_from_file` pattern as the base)
  plus `canopy_layer_by_id` (`terrain_id → canopy array layer`, `canopy_layer_for()` returns -1 for
  none). Only biomes with a canopy file get a layer. Two `sampler2DArray`s in **one** canvas shader work
  fine (base `biome_array` + `canopy_tex`).
- **Canopy code in the splatmap:** the id-map is now **RGBA8** (was RG8) — R=terrain id, G=blend_class
  code, **B=canopy code** (`0` none, else canopy layer + 1), A unused (`MapView._canopy_code`). This
  reuses the per-neighbour id-map fetch the shader already does rather than a separate id-indexed uniform
  array, so both own and neighbour canopy state come from one texture read.
- **Overhang density D (shader):** using the same signed-distance-to-shared-edge machinery vs the
  **canopy↔non-canopy** boundary (`s` = signed distance, + inside the forest): D = 1 deep inside, **~0.5
  at the exact edge**, ramping to 1 over `canopy_softness` px inside and down to 0 at `canopy_overhang` px
  **outside** the forest (crowns overhang the neighbour, then fade). The treeline is world-noise
  perturbed (`CANOPY_TREELINE_NOISE`, reusing `noise_cell`) so it's bumpy, not a clean arc. Interior
  forest hexes (all-canopy neighbours) → D=1. Composited **after** blend+shoreline, before FoW:
  `result = mix(result, crown.rgb, crown.a · D)`.
- **Map-space canopy UV:** `cuv = v_map / (2·hex_radius) · canopy_scale`, where `v_map = v_world -
  hex_origin` is the pan/zoom-anchored MAP coordinate (raw `v_world` is the quad-LOCAL/screen-fixed
  coord and would slide against the grid on pan/zoom — all map-space terms, canopy UV + the
  blend-wobble/shore/treeline noise, use `v_map`). Continuous across hexes (a crown straddling a boundary
  reads as one tree). The base biome now samples in the same continuous world space (see **Base biome
  UV** above), so `canopy_scale` and `base_scale` are the two independent world-UV density knobs (a
  crown tile per hex at `canopy_scale = 1.0`; a base tile per ~`1/base_scale` hexes). FoW-tinted like the rest.
- **Canopy LOD is DECOUPLED from the blend LOD** (own `canopy_lod_enabled` uniform, `radius ≥
  canopy_min_radius`, NOT the flat↔flat `blend_enabled`/`EDGE_BLEND_MIN_RADIUS` gate). `canopy_min_radius`
  sits WELL BELOW `EDGE_BLEND_MIN_RADIUS` (3.0 vs 16.0) so the canopy pass keeps running at far zoom:
  interior forest density (D=1) persists into a **distinct darker-green forest mass** (a forest region no
  longer reads as bare grassland when zoomed out); the edge overhang naturally shrinks to nothing as hexes
  shrink. The crown array (`canopy_textures`) is built **with mipmaps** and the `canopy_tex` sampler uses
  **trilinear** (`filter_linear_mipmap`) filtering, so far-zoom crowns AVERAGE into a smooth tone instead of
  shimmering/aliasing. (The base biome array has no mipmaps — `filter_linear` only; the canopy is the layer
  that visibly aliases at far zoom because whole crowns tile many times per tiny hex. If the base ever
  shimmers it can take mipmaps the same way.)
- **Config levers** (`terrain_config.json` → `canopy` block): `overhang_width` / `softness_width`
  (fractions of the hex radius → `canopy_overhang` / `canopy_softness` px uniforms, like `blend_width`),
  `texture_scale` (→ `canopy_scale`), and `canopy_min_radius` (the decoupled canopy LOD floor in px, ≪
  `EDGE_BLEND_MIN_RADIUS`). Fallbacks are the `CANOPY_DEFAULT_*` consts in `ui/TerrainRenderer.gd`.
- **Caveat — canopy is shader-only:** the blend-OFF **per-hex CPU path** (`use_edge_blending = false`,
  `map_biome_hard.png`) renders only the base, so forests there read as the **bare grass floor** (no
  crowns). The live client runs blend-on, so this affects only the reference/fallback path.
- Verify via `tools/map_preview.gd` State Q → `map_biome_blend.png` + `map_biome_woods_edge_seam.png`
  (the forest block borders prairie floor left + ocean top/right): whole crowns overhang + thin into a
  treeline, interior stays dense, the prairie↔forest floor blends softly, and the forest coast shows
  beach/foam with canopy overhanging the water. Far-zoom decoupled-canopy LOD via State Q-far →
  `map_biome_farzoom.png` (same four bands on a large grid so hexes go tiny): the woodland band reads as a
  distinct darker-green forest mass vs the prairie grass, smooth (mipmapped), not shimmering.

**Peak overlay — highland/volcanic relief = flat rocky floor + overhanging faceted peaks + cast shadow:**
the mountain-drama analog of the canopy overlay, built on the exact same machinery (DRY). A relief biome
keeps its flat rocky base floor and gets an RGBA **peaks overlay** of faceted mountains composited on top:
they overhang the hex boundary and thin to a footline (like the treeline), have an **elevation-driven
prominence**, and **cast a shadow** onto neighbouring hexes, so mountains read as raised relief on the 2D
map. Five relief biomes carry real AI-gen peak art today — **24 (rolling_hills)**, **25 (high_plateau)**,
**26 (alpine_mountain)**, **27 (karst_highland)**, **29 (active_volcano_slope)** — each a magenta-keyed,
offset-blend-seamless RGBA overlay in `textures/peaks/`. (28 canyon_badlands is intentionally NOT a peak
biome — its drama is incision, handled at the base-floor level, not raised relief.)
- **Assets + third Texture2DArray:** `textures/peaks/NN_name.png` (**new dir**, RGBA relief on
  transparency). `TerrainTextureManager` builds `peak_textures` (a THIRD `Texture2DArray`, same once-only
  `Image.load_from_file` + **mipmaps** pattern as the canopy) plus `peak_layer_by_id` /
  `peak_layer_for()` (`terrain_id → peak array layer`, -1 = none). Only biomes with a peak file get a
  layer. Three `sampler2DArray`s in one canvas shader (base + canopy + peaks) work fine.
- **Peak code in the splatmap A channel:** the id-map A channel (previously the unused `255`) now carries
  the **peak code** (`0` none, else peak layer + 1, `MapView._peak_code`) — the peak analog of B=canopy
  code, so both own and neighbour peak state come from the one id-map read the shader already does.
- **New elev-map (R8):** a companion `grid_w × grid_h` R8 texture (parallel to the vis-map), each texel =
  the hex's relative height (`MapView.relative_height_at` 0..100 → 0..255; `PEAK_ELEV_FALLBACK = 200` when
  a snapshot lacks an elevation raster, so relief still renders in preview/rehydrated frames). Drives the
  shader's per-hex `prominence` (`mix(peak_min_prominence, 1, elev)`) and shadow length.
- **Peak pass (shader), after canopy, before FoW:** mirrors the canopy signed-distance-to-boundary scan
  vs the **peak↔non-peak** boundary to get `s` (+inside relief) + `peak_code` (own, else nearest
  peak-neighbour's for the overhang/shadow region). Where `peak_code > 0`: (1) a multi-tap **cast shadow**
  looks back toward `peak_light_dir` (TOWARD the light; top-left = `(-0.7,-0.7)`, canvas +y DOWN) and
  darkens the ground by up to `peak_shadow_strength` where a peak occludes; (2) a **peak composite** over
  the shadowed ground using the shared `canopy_density(s, overhang, softness)` × prominence and the
  world-noise `CANOPY_TREELINE_NOISE` bumpy footline (reused, not duplicated). Peak UV = the same
  continuous map-space `v_map / (2·hex_radius) · peak_scale` as the canopy.
- **PEAK ↔ PEAK IS A SEAM TOO — and the TALLER relief overhangs the LOWER one** (the fix for the "rolling
  hills STILL have hard straight edges, even with `blend_rugged_land` on" report). A peak↔**non**-peak edge is
  a footline (the relief overhangs it and thins away), but an edge between two hexes that BOTH carry relief
  used to be **no boundary at all** — the scan skipped it (`own_is_peak == (ncode > 0) → continue`), so each
  hex composited its OWN peak layer at full density right up to the shared edge and the art switched **1-bit
  ON the hex line**: rolling_hills' green mounds ended in a razor-straight diagonal and alpine_mountain's
  spires began. **The base floor under them was blending correctly the whole time** — it is simply invisible
  under near-opaque relief art, which is why the `blend_rugged_land` gate did not help this seam (`blend_probe`
  state **G** proves it: `G_no_peaks` renders the same seam as a soft organic ecotone).
  So a relief↔relief edge now **cross-fades the two peak layers**, as a CONTINUOUS WEIGHTED MIX (never a pick),
  with the seam's **centre — not its shape — driven by elevation** (the `elev_map` the pass already reads):
  * `asym` = a smooth ODD function of Δelev (`2·smoothstep(−D, D, Δ) − 1`, `D = PEAK_BLEND_FULL_DELTA` = 0.25
    of the 0..1 relative-height scale): +1 when the neighbour towers over us, −1 when we tower over it, **0 at
    equal height**.
  * the 50/50 line sits at depth `m = (peak_overhang − peak_softness) · asym` **into our hex**, and the
    neighbour layer's coverage is `w = 1 − smoothstep(m − peak_softness, m + peak_softness, depth)`. So the
    taller relief spills across the edge and dies exactly `peak_overhang` px in — **the same reach it has onto
    flat ground, and no further** (offsetting by the *full* overhang stacks the feather on top of it and pushes
    the alpine art a whole hex radius into the hills, swallowing them), while the lower relief does **not climb
    uphill**. At Δelev → 0 it degrades to `m = 0`: a symmetric cross-fade of half-width `peak_softness`.
  * **CONTINUITY** (the shoreline's signed-coast-coordinate discipline): the neighbour computes the same edge
    with `asym`, hence `m`, **negated**, and smoothstep is symmetric about its centre — so at the shared edge
    (depth = 0 from **both** sides) the two hexes assign the **same** coverage to the **same** layer, for every
    elevation pair. The seam-centre **wobble** (world noise, so the cross-fade meanders instead of tracing the
    straight hex line) must therefore be applied **ANTISYMMETRICALLY**, signed by the peak **layer index** — a
    total order both sides agree on and that never ties.
  * Neighbours contribute **all at once**, weighted, and the result is their **weighted mean** (own weight =
    what the neighbours have not claimed; denominator `max(1, Σw)`, continuous) — the water depth field's
    discipline, so a hex meeting two different reliefs cannot seam along the bisector. Elevation is averaged
    with the same weights, so **prominence follows the art actually showing** (a tall neighbour's spires
    overhang at THEIR prominence, not faded down to ours). No new config levers: the reach and feather reuse
    `peaks.overhang_width` / `softness_width`.
  * The mean is taken in **PREMULTIPLIED alpha** (`premultiplied`/`unpremultiplied` helpers). Relief art is
    RGBA with large transparent regions, and a straight-alpha mean lets a transparent texel's keyed-out RGB
    pollute the colour wherever the other layer is opaque — it drew bright dotted fringes **along** the seam.
  * **Every `peak_tex` fetch uses `textureGrad`** with gradients taken **before any branch** (`puv_dx`/`puv_dy`,
    hoisted above `if (!in_map)`). The peak pass's fetches all sit in divergent control flow, where implicit-LOD
    `texture()` has **UNDEFINED derivatives**: on a 2×2 quad straddling a relief↔relief seam the lanes take
    different branches, the driver picks a garbage mip, and the fetch returns the wrong resolution — which drew
    a **1-pixel dark column exactly along the hex edge**, i.e. a razor line hiding inside an otherwise correct
    cross-fade. This was invisible before only because the seam it hid in was already hard.
  * A snapshot with **no elevation raster** (preview/rehydrated frames) writes `PEAK_ELEV_FALLBACK` for every
    hex, so Δelev = 0 everywhere and every relief↔relief seam is the symmetric cross-fade.
  * **Still a nearest-edge pick:** a **ground** hex touching two DIFFERENT reliefs picks the *nearest* one's
    layer for the overhang/shadow (`nb_peak_code`). Same 1-bit bug class, not yet hit in a frame; the
    accumulator above is the shape of the fix if it ever shows.
- **THE CAST SHADOW MUST DIE OFF WITH DISTANCE FROM THE RELIEF, NOT WITH THE HEX GRID** (the fix for the
  "dark hexagons in the rocky field next to the hills" report). `peak_code > 0` is true for any hex merely
  **ADJACENT** to relief, and the peak art is near-opaque wherever the occlusion taps sample it — so the
  raw occlusion term is roughly **CONSTANT across the whole neighbour hex** and then terminates on **that
  hex's own boundary**: a flat, hex-shaped dark patch painted into the neighbouring biome, on all six sides
  at once (not even directional). Fix: the occlusion is multiplied by a **`shadow_env` envelope** built from
  the very signed distance to the peak↔non-peak boundary the overhang already computes —
  `env = 1 − smoothstep(0, reach, out_dist)`, where `out_dist` is the distance **beyond the (noise-wobbled)
  footline**, so the envelope is FULL at the footline and 0 within `reach`. `reach` is
  `peak_shadow_len` × an **elevation** factor (`PEAK_SHADOW_ELEV_FLOOR`… 1 — a high massif throws a longer
  shadow) × a **DIRECTIONAL** factor: full length where the relief lies TOWARD the light (we are down-light
  of it → in its cast shadow), shrinking to `PEAK_SHADOW_UPLIGHT_REACH` of it on the LIT side. It stays a
  *directional cast shadow*, **not a symmetric halo** — but the lit side keeps a short **contact skirt**
  rather than a hard angular cutoff, because a hard `dot(light, normal) > 0` gate would step to zero right
  at the footline, where the art is only ~half opaque and the shadowed ground shows through: that trades the
  hexagon for a dark crescent.
  **Continuity, the same discipline as the shoreline's signed coast coordinate `u`:** the envelope is
  evaluated per boundary edge from quantities that read **identically on both sides of a shared edge**
  (the signed distance is 0 there from both hexes, and the relief-normal is the same vector), so nothing
  pops at the hex line. It is a **MAX over the qualifying edges, never a sum** — a hex touching relief on two
  sides takes the deeper of the two and cannot **double-darken into a seam**. (Enveloping by the *single
  nearest* edge only — the discipline `peak_code`/prominence still follow — would have been **discontinuous
  along the bisector** where the nearest edge switches, since the two edges' light alignments differ; a max
  of continuous functions is continuous everywhere.) Verify on **`blend_probe` state S**, and judge it on
  **`S_shadow_footprint*.png`** — the amplified diff against a `shadow_strength = 0` render, i.e. the cast
  shadow **in isolation**. That frame is necessary because the relief art overhangs the footline and is
  semi-transparent out there, so neither the eye nor a pixel sample can separate "shadow" from "dark mound
  fringe" in the composited frame.
- **Peak LOD is DECOUPLED from the blend LOD** (own `peaks_lod_enabled`, `radius ≥ peak_min_radius`,
  default 3.0 ≪ `EDGE_BLEND_MIN_RADIUS`), so the mountain mass persists at far zoom; trilinear-mipmapped
  peak array keeps it smooth (no shimmer).
- **Config levers** (`terrain_config.json` → `peaks` block): `overhang_width` / `softness_width`
  (→ `peak_overhang` / `peak_softness` px, like canopy), `texture_scale` (→ `peak_scale`),
  `peak_min_radius` (LOD floor px), `shadow_length` (→ `peak_shadow_len` px) / `shadow_strength`,
  `min_prominence`, and `light_dir_x` / `light_dir_y` (normalized → `peak_light_dir`). Fallbacks are the
  `PEAK_DEFAULT_*` consts in `ui/TerrainRenderer.gd`. Peaks are shader-only (same caveat as canopy).
- Verify via `tools/map_preview.gd` **State swatch** with `SWATCH_BIOME_ID = 26` (alpine) →
  `map_swatch.png` (+ `map_swatch_farzoom.png`): faceted peaks composite with light-left/dark-right
  self-shading, overhang the alpine↔prairie seam + cast a darkening shadow onto the prairie, and the
  far-zoom alpine band reads as a raised mountain mass. Restore `SWATCH_BIOME_ID = 2` after.

**Rivers — Minor/Major on hex EDGES, Navigable as a water TERRAIN:** rivers are two different kinds of
thing, and the split is the whole design (see `docs/plan_rivers.md`). A **Minor/Major** river lives on a
hex **edge** — that is where a future crossing cost can live ("the side the river is on is the side that
costs") — and is drawn by a **river pass in `terrain_blend.gdshader`** so the water is painted exactly on
the edge the penalty will apply to. A **Navigable** river is a body of water you are *in*, so **in the sim**
it stays an ordinary water terrain (`TerrainType::NavigableRiver`, **id 37** — blocking + boats fall out of
the existing water rules). **Its RENDER, though, is not a water hex** — see the navigable-channel pass
below: a water hex ran through the land↔water shore pass and came out a hex-shaped puddle with a sandy
beach and surf, i.e. **visually identical to an InlandSea lake** and nothing like a river. It is now drawn
as a silty **BANK with a wide channel through it**. The old `HydrologyOverlay` polyline (and
`MapView._draw_hydrology`) is **deleted** — the tiles now fully determine the render.
- **The wire primitive:** `TileState.riverEdges` (`ushort`), decoded in `native/src/lib.rs tile_to_dict`
  as `river_edges` (both the snapshot and delta tile paths share that one function). A **12-bit mask, 2
  bits per odd-r direction** — `class = (river_edges >> (2*dir)) & 0b11`, `0 = none / 1 = Minor / 2 =
  Major` (3 reserved). **Both hexes flanking an edge carry it** (hex `H` dir `d`; the neighbour dir
  `(d+3)%6`), so a hex answers "is there a river on my side `d`?" locally, with no cross-hex sampling.
  Ingested by `MapView.display_snapshot` into `tile_river_edges` (`Vector2i → int`, like
  `tile_habitability`).
- **The SECOND wire primitive — `TileState.riverInflow`** (`ushort`, decoded as `river_inflow` by the same
  `tile_to_dict`, ingested into `tile_river_inflow`): the same 12-bit / 2-bits-per-slot packing, but keyed
  by hex **CORNER** — `class = (river_inflow >> (2*corner)) & 0b11`. **Why it must exist:** an edge river
  runs *along* a side, so it does not end mid-edge, **it ends at a VERTEX** — and a trunk hex can flank two
  or three river edges (the tributary ran along several of its sides on the way in), which `river_edges`
  alone cannot disambiguate. So the sim names the hand-over vertex. It means **"a tributary hands over to the
  channel at this vertex"** — true of **ANY** navigable hex, **not** just a chain head (a real drainage network
  joins tributaries to trunks MID-CHAIN; the semantics widened with `docs/plan_rivers_drainage_network.md` §A —
  same field, same bits, same corner convention). A navigable hex with no tributary reports 0; more than one
  corner may be set (two tributaries can terminate on the same hex), so **loop all 6**. **Never read it as
  "this hex is a chain HEAD"** — that is what `river_channel`'s exit count says (below), and keying the head
  taper off inflow is exactly the HOURGLASS bug. Corner
  `i` is the vertex at angle `60*i + 30`, +y down — **exactly `MapView._hex_points` order** (0 lower-right,
  1 bottom, 2 lower-left, 3 upper-left, 4 top, 5 upper-right), so the shader derives it from the hex centre
  and radius with no table; side `dir` spans corners `{dir-1, dir}`. **Deliberately NOT surfaced in the
  Tile card / tooltip** — it is a rendering detail, not player-facing geography (`RiverEdges.gd` still
  reports the SIDES, which is what a crossing cost will key on).
- **The THIRD wire primitive — `TileState.riverChannel`** (`ubyte`, decoded as `river_channel` by the same
  `tile_to_dict`, ingested into `tile_river_channel`): **1 bit per odd-r direction** —
  `exits(dir) = (river_channel >> dir) & 1` — naming the sides a **navigable** hex's channel actually flows
  out through: its upstream and downstream neighbours in its own chain, plus (on the chain's LAST hex only)
  its exit into the sea / inland sea / `RiverDelta` mouth. **Why it must exist:** the trunk's connectivity
  is a **path**, and terrain cannot say which two of a hex's neighbours are on it. The renderer used to
  infer an arm for every navigable/water/`RiverDelta` neighbour, and wherever navigable hexes sat adjacent
  — parallel reaches, a chain bending back on itself, the blob a buggy worldgen once emitted — that rule
  **cross-linked them into a spider WEB with triangular holes**. Only the sim's tracer knows the path, so
  the sim states it and the shader arms **only the set bits**. Symmetric across a shared side **except at
  the mouth** (open water carries no channel, so that bit is not mirrored back) — so read the OWN hex's
  bits and never assume the neighbour agrees. It does **not** double-encode the head: the sim sets no exit
  toward the tributary, because the inflow SPUR (above) draws that. Do not "simplify" this back to a
  terrain test — `map_rivers_web.png` is the regression guard, and a web there is that bug returning.
- **The shader's `neighbor_offset` table IS a wire contract now.** It was reordered to the SIM's odd-r
  direction order (`core_sim` `grid_utils::HEX_NEIGHBOR_OFFSETS`, clockwise from E: 0=E, 1=SE, 2=SW, 3=W,
  4=NW, 5=NE) because the river pass indexes the mask **by direction**. The blend/shore/canopy/peak passes
  only ever loop over all 6 and are order-agnostic, so the reorder was free — but **do not reorder it
  again**.
- **RGBA8 river-map splatmap** (`_rebuild_terrain_shader_maps`): all four id-map channels are already
  taken (id / blend_class / canopy / peak), so the river masks get their **own** texture — and BOTH ride
  it: `R/G = river_edges` (low 8 / high 4), `B/A = river_inflow` (low 8 / high 4). Two 12-bit masks are 24
  bits, so they do not fit one RG8 texel; one RGBA8 texture is cheaper than a second sampler. NEAREST,
  rebuilt each snapshot — **after** the tile loop in `display_snapshot` (it reads `tile_river_edges` /
  `tile_river_inflow`, which the tiles populate). All 32 of ITS bits are now spoken for too, so
  `river_channel` (6 bits) rides a **second, R8 `river_channel_map`** built in the same pass, also NEAREST.
- **River pass (shader), after the shore pass, before canopy/peaks:** trees overhang a river and mountains
  sit above it; sitting before the FoW tint, a river in a Discovered tile **dims with the mist rather than
  disappearing**. Per fragment, for each of the own hex's carrying edges: distance to the **shared edge
  SEGMENT** — `mid ± perp * (hex_radius * 0.5)` (a regular hexagon's side == its circumradius), clamped to
  the segment, **not** the infinite bisector, which would smear the band across the whole hex — then keep
  the edge with the **max coverage** (`half_width - distance`). That min-distance-over-edges pick is what
  **rounds the corner joins for free**: a 120° turn softens with no spline math. The water samples in
  continuous map space (`v_map`, like the canopy) plus a **`TIME` scroll along the winning edge's tangent**
  so it flows.
- **THE HONEYCOMB, and what actually fixes it — read this before touching the river look.** An edge river
  drawn as a wide, constant-width, hard-edged stroke reads as *the hex borders, inked blue*. The instinct is
  to meander harder. **That is a dead end, and not because the meander is under-tuned:**
  - the amplitude ceiling is real — past ~`0.24` of the warp cell the warp's gradient exceeds the band
    half-width and the river **tears into disconnected pools**; and
  - more fundamentally the river is **edge-LOCKED by design**. The water must be painted on the edge the
    future crossing cost applies to ("the side the river is on is the side that costs"), so a warp can only
    displace the band about a band-width before it **detaches from its own edge and starts lying about the
    geometry**. Pushing meander trades a honeycomb for a lie.
  What actually kills the honeycomb, in order of impact: **(1) THINNESS** — halved to `minor_width 0.05` /
  `major_width 0.09`; a thin stroke reads as a river, a wide one as an outline. **(2) WIDTH VARIATION ALONG
  the river** (`width_variation`, low-frequency world noise on a `RIVER_WIDTH_NOISE_CELL = 2.6` hex-radii
  cell — deliberately several radii, so a swell is a property of the *reach*, not of the hex; a cell near 1
  would re-key the variation to the lattice and *reinforce* the honeycomb). **(3) RAGGED BANKS** — a
  higher-frequency wobble of the half-width (`bank_noise_width`, `RIVER_BANK_NOISE_CELL = 0.35`), the same
  idiom as the shore pass's noisy `reach`, plus a wider `softness_width`. Both noises are sampled in
  **world space** (`v_map`), so the two hexes flanking an edge get identical values at the shared boundary —
  the symmetric **no-seam** meeting of the two half-bands survives. A `RIVER_MIN_HALF_WIDTH` px floor keeps
  the noise from severing the band (and keeps it a legible hairline at far zoom).
- **MEANDER — a domain warp, not a distance bias.** Kept (it still bends the centerline rather than
  bulging/pinching a straight one) but **capped**, per the above: `RIVER_MEANDER_CELL = 0.9` hex radii,
  `meander_width = 0.22`. The warp cell is keyed to `hex_radius`, **not** the shared px-sized `noise_cell`
  (which would make the wander's character change with zoom and only fuzz the bank). It is warped ONCE per
  fragment in world space, so both flanking hexes warp the same point → no seam.
- **ONE river growing, not two spliced.** The two class textures are deliberately different art (`00_minor`
  light/shallow-over-gravel, `01_major` dark/deep), and untreated they meet as turquoise-next-to-near-black:
  a class change read as *two waterways joining*. Two shader fixes, no art edits: (a) the class **crossfades**
  — the pass tracks the best coverage per class and mixes the two layers by
  `smoothstep(-river_class_blend, river_class_blend, cov_major - cov_minor)`, so a hex carrying both classes
  dissolves one into the other over `class_blend_width` (a pure-class hex is unaffected: the loser stays at
  `-1e9`); and (b) `river_harmonize()` pulls both layers' luma toward `RIVER_DEPTH_PIVOT`
  (`depth_compress`) and their chroma toward `RIVER_SHARED_HUE` (`tint_strength`), preserving the luma
  ORDER — Minor stays lighter, Major deeper — which is the thing that should say *bigger*.
- **NAVIGABLE-CHANNEL pass (shader), right AFTER the Minor/Major pass** (so a Major feeding a navigable
  trunk composites into it), before canopy/peaks. **This is a RENDER-ONLY change — the sim is untouched.**
  Three parts:
  - **`blend_class` is `"flat"`, not `"water"`** (a *render* eligibility class with no sim meaning — the
    sim's `WATER | FRESHWATER` tags and water movement profile are unchanged). Treating it as land is
    correct — it takes the hex **out of the shore pass** (no beach, no foam) and lets it **blend softly
    into neighbouring flat land**, merging the corridor into the landscape.
  - **A navigable hex is a VALLEY with a river in it — its base renders the UNDERLYING biome, not a
    whole-hex bank** (rivers slice #3, `docs/plan_rivers.md` → "A navigable hex is a valley with a river
    in it"). The old whole-hex silty-bank base (`biome_array` layer 37) hid the land; now the hex body
    renders the **valley the river cut**, with only a **slim silty-bank skirt hugging the channel**. Two
    wire/shader pieces:
    - **The valley biome rides its OWN wire field + map.** `TileState.underlyingTerrain` (== the tile's own
      `terrain` on ordinary tiles, the preserved valley biome on a navigable hex) is decoded in
      `native/src/lib.rs` as `underlying_terrain`, ingested into `MapView.tile_underlying_terrain`, and
      packed into a NEW R8 `navigable_underlying_map` (built in `_rebuild_terrain_shader_maps` beside the
      river-channel map). The shader swaps the base sample from layer 37 to `navigable_underlying_map`'s id
      **only on a navigable hex** (`own_navigable`); everywhere else `base_layer == own_layer`, a no-op.
      **The `id_map` R channel STAYS terrain id 37** — that is the navigability signal the shader keys
      `own_navigable`/the channel pass on; only the *base texture* is swapped, never the id.
    - **The bank is a thin annulus riding the channel's distance field.** In the navigable channel pass, the
      silty bank (`biome_array` layer for id 37 — resolved via `river_navigable_terrain_id`, never hard-coded)
      is composited OVER the underlying base across an annulus just outside the water, out to
      `river_navigable_bank_half_width` beyond the channel edge (`bank_cov = best_cov +
      river_navigable_bank_half_width`, so it follows every arm/spur/taper/turn/mouth for free); the water
      channel then paints OVER the bank as before. Read across the hex: water (dist < navigable half-width) →
      thin bank gravel (out to + bank half-width) → underlying terrain. Config lever
      `rivers.navigable_bank_width` (**0.10**, hex-radius fraction → the `river_navigable_bank_half_width`
      uniform via `RIVER_DEFAULT_NAVIGABLE_BANK_WIDTH`).
    - The bank's base texture (`textures/base/37_navigable_river.png`) is still the **BANK ground**
      (placeholder: a copy of `09_floodplain`; real silty-bank art lands later) and its config `color` (the
      fallback solid + minimap pixel) is a bank tone. **The id-37 layer ALSO carries a per-terrain
      `blend_profile`** (`2.6 / 2.2 / 2.6` — see Edge Blending), retained for the bank's flat↔flat seams;
      judge the bank contrast on `blend_probe` state **17 (BANK)**. **The `blend_class` G-channel code stays
      "flat" (from terrain 37)** — since both the valley base and its flat neighbours are flat class, the
      flat↔flat blend fires and the navigable hex body merges seamlessly into the surrounding land with no
      hard hex seam (verified on `map_rivers_navigable.png`/`map_rivers_web.png`). Writing the underlying
      terrain's blend_class into the id-map for navigable hexes was NOT needed; a possible follow-up only if
      a valley biome of a *different* class (rugged) ever seams.
  - The shore pass additionally **skips a TRUE MOUTH edge — the one navigable edge whose `river_channel`
    exits through it INTO the water**: `blend_class` alone is not enough at the MOUTH, where the (now-land)
    bank would take a beach and the sea across from it would draw a **surf line across the river's mouth** —
    the river visibly walled off from the sea it drains into. A river meeting the sea is not a coast.
    **But the test must be per-EDGE, not "any navigable hex on either side"** — that over-broad exclusion
    (the original) also fired where a navigable river merely runs **ALONGSIDE** a lake without draining into
    it (a real @21,61 case: a one-hex `InlandSea` ringed by 3 navigable hexes, **none** of whose channels
    exit toward it), eating the lake's own shore ring on those edges and leaving a hard seam (glaring now
    that the bank renders the valley terrain, not neutral gravel). So the pass reads the sim-authored R8
    `river_channel_map` (the same mask the channel pass arms from; the shore loop's `dir` is already sim
    odd-r order, matching the channel bit index) and skips ONLY a true mouth — by the time the check runs
    exactly one side is navigable (flat/land) and the other genuine water, so it reads the channel from
    whichever side is navigable, toward the water: own navigable → its exit bit for `dir`; neighbour
    navigable → its exit bit toward us, `(dir+3)%6`. Everywhere else (alongside, no exit here) falls through
    to the normal coast, so the lake keeps its full ring and the valley/bank gets its beach. **This stays an
    EDGE-LEVEL exclusion, not a `shore_profile` entry** (see Shoreline → per-water-terrain shore profile):
    the profile is keyed on the **water** side — only a `CLASS_WATER` hex contributes one — and a navigable
    hex is land-class, so it can never supply one; the profile that would apply at the mouth is the
    **sea's/lake's**, which must keep its coast everywhere else. Dropping a mouth edge removes the whole
    chain at once (profile, waterline cross-fade, sand, surf, wisp all live under the pass's `best_d` guard)
    and does so symmetrically from **both** hexes' frames, so no half-drawn coast survives on one side of it.
    Judged on `map_rivers_lake_alongside.png` (the alongside lake keeps its ring) vs `map_rivers_mouth.png`
    (the true mouth stays open).
  - The **channel** (`river_tex` **layer 2**, `textures/rivers/02_navigable.png` — the deep teal water that
    used to be the terrain's base) is TWO kinds of stroke, unioned by the **max-coverage (min-distance)**
    pick — the same trick that rounds the Minor/Major corner joins, here fusing them into one connected
    body with rounded junctions for free:
    - **TRUNK ARMS**, at the channel's own (navigable) width: hex **CENTRE → the MIDPOINT** of each side
      **`river_channel` says the river flows out through** (`(mask >> dir) & 1`). **The connectivity is
      SIM-AUTHORED, not inferred from the neighbouring terrain** — see the third wire primitive above. The
      old rule (arm every navigable / water / `RiverDelta` neighbour) is exactly what drew the **WEB**:
      adjacent navigable hexes that are not consecutive on the chain got cross-linked, and the corridor
      filled with triangles. The mask also carries the mouth (the last hex's unmirrored exit into the sea /
      delta), so the river still does not dead-end a hex short of the sea. The arm needs **no neighbour
      fetch** — only the neighbour's CENTRE, which is pure math — so it also draws correctly at the map
      border and across the wrap seam.
    - **INFLOW SPURS**, at the arriving tributary's **own Minor/Major width**: hex **CENTRE → the CORNER**
      named by `river_inflow` (all 6 checked; a mask bit needs no neighbour fetch, so it spurs even at the
      map border / across the wrap seam). The spur wears the tributary's class art and **crossfades** into
      the channel over `class_blend_width` — the edge pass's Minor→Major crossfade, for the same reason:
      one river growing, not two waterways spliced. **This centre-hub form is used for a MID-CHAIN junction
      (`>= 2` exits with an inflow) — a hex the trunk passes THROUGH, whose centre is genuinely on the flow.**
    - **A CHAIN HEAD FED BY A TRIBUTARY routes STRAIGHT from the inflow corner to its single exit, NOT via
      the centre** (the notch fix). On a head (`exits == 1`) with an inflow, the centre-hub form draws the
      inflow as a centre→corner spur and the exit as a centre→edge-midpoint arm; when the inflow corner and
      the exit side flank the **same** vertex, that union is `inflow_corner → centre → exit_mid`, which
      **DOUBLES BACK into a NOTCH / inverted-V at the corner** (reads as "the tributary hooks into the wrong
      corner"). So a head-with-inflow instead draws ONE tapered segment per inflow corner — `inflow_corner →
      exit-midpoint` — narrow (the tributary's own width) at the corner, swelling to the full navigable width
      at the exit edge (the head taper, now laid along the true flow line), with the tributary art
      crossfading to the channel art along it (`head_class_mix`). `t_head` is the UNWARPED projection, same as
      the arm loop, so the exit edge still lands on exactly `navigable_half_width` and the downstream hex
      meets it with no step. It **rides the same `best_cov`** the bank annulus reads, so the slim bank follows
      the new flow line for free. Multiple inflow corners on one head (a Major+Minor confluence, the join
      frame) draw one segment each, unioned into a Y at the exit. Judged on `map_rivers_notch.png` (inflow at
      the bottom vertex, single exit on the adjacent SW side — the exact geometry that notched).
    - **HEAD TAPER — a trunk does not spring to full width at a hex centre.** On the **first hex of a
      chain** — **gated on the `river_channel` EXIT COUNT (`<= RIVER_CHANNEL_HEAD_MAX_EXITS`, i.e. 1), NOT on
      `river_inflow != 0`**: a head has only its downstream link; a mid-chain hex has its upstream one too (2),
      a confluence 3. Since the drainage network a tributary hands over at ANY navigable hex's vertex, so an
      inflow-gated taper would **pinch the full-width trunk to the tributary's width at a mid-chain junction's
      centre and swell it back out on both sides — a visible HOURGLASS in mid-channel.** The **SPUR stays
      unconditional**: it carries the tributary from the hex centre out to its vertex, and a mid-chain junction
      needs it MORE (without it the tributary dead-ends at the vertex, short of the arms, which only reach the
      edge midpoints). Judged on `map_rivers_midchain.png`. On a head, the arms **start at the half-width of the
      WIDEST class feeding in** (max over the 6 inflow corners — Major wins if any Major lands, and the
      sim already stores the widest class per corner) and **swell to the full navigable width by the hex
      EDGE**: `half_w = mix(inflow_half_width, navigable_half_width, pow(smoothstep(0,1,t), head_taper_curve))`,
      `t` = the arm's own centre→edge-midpoint projection. Without it a hairline Minor arrived at a vertex
      and was a great river a few px later — a jump-cut, not a river. Any hex that is **not** a chain head (or
      is a head with no tributary) is **unchanged**: `inflow_half_width` stays the navigable width and the mix
      is a no-op — no extra per-hex branching.
      **`t` is taken from the UNWARPED point** (unlike the distance-to-centerline `t`, which must use the
      meander-warped one), and that is load-bearing: every fragment on the shared edge projects to
      **exactly `1.0`** on the arm axis (the edge line's projection onto the arm direction is the apothem,
      whatever the lateral offset), so the taper lands on **exactly** `navigable_half_width` where the
      next, constant-width navigable hex takes over — no step, no notch at the head's downstream edge. The
      warped point's projection would wander by the meander amplitude and leave one. A hex with **>= 2 channel
      exits** is mid-chain and keeps the CONSTANT full navigable width, whatever its inflow. Width is a scalar
      field of world position here, the same as `river_width_mod` / `river_bank_wobble` (both also sampled
      unwarped), and the organic machinery rides **on top of** the tapered base width, so the continuity
      guarantees survive. The taper also makes the **spur→trunk join seamless**: the trunk now leaves the
      centre at the same width the spur arrives there with.
    - **An arm is NOT keyed off `river_edges`** — that was the fat-teal-blob bug. An edge river runs ALONG
      a side; it does not flow through the side's MIDPOINT, and a trunk head can flank two or three river
      edges, so the mask-armed rule drew three fat centre→midpoint arms **at the trunk's width** and the
      hex filled with water. Water enters a trunk hex at a **vertex**, which is what `river_inflow` names.
    A navigable hex with **zero arms** (the sim should never emit one; an inflow spur is not an arm) draws
    a centre **blob** rather than a hex of bare bank, and `MapView._warn_orphan_navigable_rivers`
    `push_warning`s it — now a pure MASK test (no `river_channel` exit **and** no `river_inflow` = water
    neither enters nor leaves), mirroring the shader's arm rule; keep the two in step.
  - It reuses the **same organic machinery** as the edge pass — the `river_meander_warp` domain warp, the
    low-frequency `river_width_mod` swell, the `river_bank_wobble` ragged bank (all three factored into
    shared shader functions rather than copied) — and `river_harmonize`, so the trunk reads as the same
    river grown bigger. All noise is sampled in **WORLD space**, which is exactly what makes the channel
    **continuous across adjacent navigable hexes**: both hexes warp the same point and read the same width
    at their shared boundary, so the half-channels line up with no seam, pinch or gap. The **spurs ride the
    same three**, which is why a tributary's band arrives at the vertex already warped exactly as the edge
    pass warped it on the far side — the two meet without a notch.
- **Config levers** (`terrain_config.json` → `rivers` block): `minor_width` / `major_width` /
  **`navigable_width`** (the channel HALF-width as a fraction of the hex radius — `0.14`: clearly the
  biggest water on the map, but **only somewhat** wider than Major's `0.09`. It shipped at `0.24` and read
  as a flood filling the hex, which is the puddle read this whole pass exists to kill; softness / meander / width-variation /
  bank-noise / flow-speed are **shared with the edge classes**, not duplicated per class) /
  `softness_width` / `meander_width` / `bank_noise_width` / `class_blend_width` (fractions of the hex radius
  → px uniforms, computed in `TerrainRenderer.update_shader_quad` exactly like `blend_width` / `canopy_overhang`),
  the unitless `width_variation` / `tint_strength` / `depth_compress` / **`head_taper_curve`** (the
  exponent on the head taper's smoothstep — `0.8` ships, i.e. swell slightly EARLY; `1.0` = plain
  smoothstep, `> 1` holds the tributary's width longer then flares. An exponent, never a width, so it
  cannot disturb the exact navigable-width match at the hex edge), plus `texture_scale`,
  `river_min_radius` (the LOD floor), and `flow_speed`. Fallbacks are the `RIVER_DEFAULT_*` consts in
  `ui/TerrainRenderer.gd`.
- **River LOD is DECOUPLED from the blend LOD** (own `rivers_lod_enabled`, `radius ≥ river_min_radius`,
  default 3.0 ≪ `EDGE_BLEND_MIN_RADIUS`) — a river is a landmark you navigate *by*, so it must survive
  zooming out; the mipmapped/trilinear river array keeps the thin band stable (no shimmer).
- **`set_highlight_rivers`** (the Map tab toggle) survives, repointed from the deleted polyline draw to the
  shader's `river_highlight` uniform.
- **TEXT surfacing — `ui/RiverEdges.gd`, ONE formatter, two surfaces.** Seeing the water isn't knowing
  which SIDES carry it — which is exactly what a crossing penalty will key on. `MapView._tile_info_at`
  copies the mask onto the tile dict as `river_edges` (from `tile_river_edges`; **deliberately NOT in
  `FOW_DISCOVERED_HIDDEN_KEYS`** — a river is permanent geography like the terrain label or a discovered
  Wondrous Site, so a remembered tile still reports it; never-seen tiles are already covered by the
  `unexplored` redaction), and both the **Tile card** (`Hud._tile_terrain_lines`, with the other
  terrain-intrinsic rows, before the FoW discovered early-return) and the **map hover tooltip**
  (`Hud.show_tooltip`, after `Terrain:`) render it from the same `RiverEdges.summary_lines(mask)` call.
  `RiverEdges` owns the vocabulary (classes + direction names + bit widths as named constants) and emits
  **one line PER CLASS, Major first** — `Major River: NE, NW` / `Minor River: SW` — plain `Key: Value`
  rows needing no `DetailFormat.detail_bbcode` tint case, and an **empty array on a riverless tile** so no
  empty label renders. It keeps **two direction orders**: the sim's `HEX_NEIGHBOR_OFFSETS` order
  (clockwise from E — the wire contract) DECODES the mask, and a **compass display order** (clockwise
  from NE) lists the directions within a line, because a compass reading is what a player parses.
  ui_preview: `river_tile_both` (two-class) / `river_tile_minor` (single-class) / `river_tile_none` (no row).
- **Caveat — rivers are shader-only** (same as canopy/peaks): the blend-OFF **per-hex CPU path** renders no
  rivers. That is the reference/fallback path only; the live client runs blend-on.
- Verify via `tools/map_preview.gd` State **rivers** → `map_rivers.png` (a Minor→Major edge river wandering
  west→east with corner turns, joining a NavigableRiver chain that turns corners of its own and drains to
  the eastern sea — **with a real InlandSea lake in the same frame as the control**: the lake keeps its
  beach + surf, the navigable hexes have neither, and the two must read as obviously different things) +
  `map_rivers_seam.png` (edge/corner close-up framing the class change: the band hugs the EDGE, joins are
  rounded, the two half-bands meet with no seam down the middle, Minor grows into Major) +
  `map_rivers_navigable.png` (the trunk: the Major edge river flowing INTO it, the corner turns, and the
  channel CONTINUOUS across adjacent navigable hexes) + `map_rivers_mouth.png` (the channel reaching open
  sea + its delta lobe — no dead-end, and no surf line across the mouth) +
  `map_rivers_head_minor.png` (the HEAD TAPER's own frame: a second, one-hex navigable branch fed by a
  **Minor tributary only** — its arm must start hairline at the centre and swell to the full channel width
  by the shared edge with the trunk, with **no step** there; the Major+Minor head in `map_rivers_join.png`
  is the other half of the test, starting at the **wider** — Major — width) +
  `map_rivers_farzoom.png` (decoupled LOD). The fixture generates the edge chain as the **boundary of a
  region** (hexes north of a bank row `f(x)`), which is contiguous by construction — no gaps — and turns a
  corner at every step; the navigable chain then WALKS `RIVER_NAV_STEPS` (E/SE/E/NE/E) out to the sea, so the
  trunk's arm/junction geometry is actually exercised. The river is kept in the map's **upper rows**
  deliberately: the map is cover-fit and that fit is the zoom FLOOR, so on a window wider than the grid's
  aspect the lower rows cannot be scrolled into view at all. **`RIVER_PATTERN` must stay a mostly-MONOTONE drift**: an up-down-up staircase makes
  the boundary wrap 4+ sides of the same hexagon, manufacturing a honeycomb that real hydrology (a downhill
  walk on the corner lattice) never produces — the original fixture did exactly that and made the render
  look far worse than it is.

**Texture readback fix (kept from A):** `TerrainTextureManager` retains the CPU-side layer Images
(`_layer_images`) captured once at build time; `get_terrain_image` serves duplicates from it and
**never** calls `Texture2DArray.get_layer_data()` again (a second readback returned a blank image on
some drivers, whitening the base). The `sampler2DArray` uniform is the same `terrain_textures`.

Verify via `tools/map_preview.gd` State Q → `ui_preview_out/map_biome_hard.png` (blend off, the
reference) vs `map_biome_blend.png` (Approach B on), plus `map_biome_blend_seam.png` (desert↔prairie
close-up): the flat pair blends symmetrically, prairie↔forest / forest↔ocean stay crisp, and terrain
stays aligned with the grid. **State S** (`map_repetition_after.png` + `..._zoom.png`) renders a large
detailed-rugged field (alpine id 26) beside a flat prairie band: the continuous world-space base
sampling means NO per-hex identical-repeat grid on the alpine (each hex shows a different region of the
texture, features flow across boundaries), while the prairie↔alpine seam stays a hard edge.

**Fallback considered:** a MultiMesh (one instance per hex) was the fallback if whole-map inverse-hex
alignment couldn't be matched; the splatmap alignment held, so the single-quad path was chosen (fewer
moving parts, no per-frame instance transforms). **Future:** blue-noise sample instead of hash value
noise. A **per-hex UV rotation+offset for rugged biomes** (hard-edged, so cross-edge rotation
discontinuities hide) was speced to break the texture's *own* tiling-period repeat, but the continuous
world-space base sampling alone removed the objectionable per-hex grid (verified on alpine id 26 at
`base_scale = 0.25`), so it was NOT needed. Do NOT rotate flat biomes — it would break their cross-edge
blend continuity.

---

