---
paths:
  - "clients/godot_thin_client/assets/terrain/{TerrainTextureManager,TerrainDefinitions}.gd"
  - "clients/godot_thin_client/assets/terrain/terrain_config.json"
  - "clients/godot_thin_client/src/scripts/ui/TerrainRenderer.gd"
---

<!-- Extracted verbatim from clients/godot_thin_client/CLAUDE.md lines 220-221;396-501.
     Routing table and shared vocabulary live in clients/godot_thin_client/CLAUDE.md.
     Regenerate with scripts/split_claude_md.sh -->

# Terrain textures — assets, config, loading, 2D pipeline

## Key scripts

| Script | Purpose |
|--------|---------|
| `assets/terrain/TerrainTextureManager.gd` | Autoload singleton for terrain texture loading |
| `assets/terrain/TerrainDefinitions.gd` | Single source of truth for terrain definitions |
## Terrain Texture System

Optional terrain texture graphics for the 2D map view.

### Asset Structure
```
assets/terrain/
  textures/
    base/                        # 38 terrain textures (512x512 PNG); forest bases are grass FLOOR (no trees)
      00_deep_ocean.png
      ...
      37_navigable_river.png     # NavigableRiver's BANK ground (the channel water is rivers/02) — see Rivers
    canopy/                      # RGBA tree-crown overlays (transparency); one per canopy biome (3 today: 07/12/13)
    peaks/                       # RGBA mountain-relief overlays (transparency); one per relief biome (5 today: 24/25/26/27/29)
    rivers/                      # flowing water, NOT keyed by terrain id (see Rivers): 00_minor / 01_major
                                 # are the hex-EDGE classes (layer = class - 1); 02_navigable is the CHANNEL
                                 # water painted over a NavigableRiver hex's bank
    edges/                       # 6 edge masks for blending (optional)
    wang/                        # Wang tile variants (future)
  terrain_config.json            # Configuration
  TerrainTextureManager.gd       # Autoload singleton for centralized texture loading
  TerrainDefinitions.gd          # Single source of truth for terrain definitions
  TerrainTextureGenerator.gd     # CLI script to generate placeholder textures
```

### Enabling Terrain Textures
1. Generate placeholder textures from command line:
   ```bash
   godot --headless --path clients/godot_thin_client --script assets/terrain/TerrainTextureGenerator.gd
   ```
2. Replace placeholders in `assets/terrain/textures/base/` with AI-generated or hand-crafted textures
3. Set `"use_terrain_textures": true` in `terrain_config.json`

Textures are loaded at runtime from individual PNGs and combined into a `Texture2DArray`.

### Configuration (`terrain_config.json`)
```json
{
  "use_terrain_textures": true,
  "use_edge_blending": true,
  "texture_scale": 4.0,
  "blend_width": 0.25,
  "blend_soft": 0.35,
  "blend_height_influence": 0.25,
  "blend_noise_scale": 0.25,
  "blend_noise_amount": 0.3,
  "feature_noise_cell": 6.0,
  "water_blend": { "blend_width": 0.45, "blend_soft": 0.45, "blend_noise_amount": 0.45 },
  "lod_near_distance": 50.0,
  "lod_far_distance": 200.0
}
```
Every terrain entry also carries a `"blend_class"` (`flat` | `water` | `rugged`) — the single
source of truth for edge-blend eligibility, which is **same-class** (flat↔flat and water↔water blend;
land↔water and rugged stay hard — see Edge Blending below) — and may carry an optional
**`"blend_profile"`** block (`width_scale` / `noise_scale` / `noise_cell_scale`) scaling the flat↔flat seams
**it** is on, for a texture too far from its neighbours in tone+hue for the global ecotone (shipped on the
NavigableRiver bank only; neutral and bit-exact everywhere else — see Edge Blending → per-terrain
`blend_profile`). The top-level `blend_*` keys are the
**seam** levers, tuned for LAND (`blend_width` = the ecotone's reach, `blend_soft` = the feather
softness, `blend_height_influence` = the detail-following nudge, `blend_noise_scale`/`blend_noise_amount`
= the boundary wobble); the `water_blend` block **overrides width/soft/noise_amount for water↔water
only** (smooth low-variance water needs a wider, softer, wobblier seam). All documented under Edge
Blending below. `feature_noise_cell` is the value-noise cell size
(**raw px**) for the **other** noise-driven features — the shoreline reach/wisp, the canopy treeline and
the peak footline. The blend noise and the feature noise are deliberately **decoupled** (one uniform each)
so retuning the seam can never move a coastline, treeline or footline. **The units differ on purpose:**
`blend_noise_scale` is a **fraction of the hex radius** (→ `blend_noise_cell = blend_noise_scale · radius`
px) so the seam's character is identical at every zoom (a fixed px cell drifted — a hex is ~45px on screen
in-game but several times that in a zoomed-in preview frame, so the same 6px cell read very differently in
the game than in the preview it was judged in), while the shore/treeline/footline look is tuned in
absolute pixels. **Judge any blend change at the GAME's hex radius (~45px)** — use
`tools/blend_probe.tscn`, which pins it.

### Texture Loading (TerrainTextureManager)
- Autoload singleton loads textures once at startup for the 2D map renderer
- Builds `Texture2DArray` from individual PNGs in `textures/base/`
- Exposes: `terrain_textures` (Texture2DArray), `terrain_config`, `use_terrain_textures`, `use_edge_blending`
- Also computes each base layer's **mean luminance** at build time (`layer_mean_luma` /
  `get_layer_mean_luma()`, measured on a 16² Lanczos downscale of the retained CPU-side Image) and packs it
  into `layer_luma_texture` (a 1×N single-channel `ImageTexture`, one texel per terrain id). This is the
  zero-point of each texture's pseudo-height for the shader's flat↔flat **height blending** (see Edge
  Blending); MapView binds it once as the `layer_luma_map` uniform. The Rec.709 weights here MUST match the
  shader's `luma()` helper
- Also builds `canopy_textures` (a second Texture2DArray of RGBA crowns from `textures/canopy/`) +
  `canopy_layer_by_id` / `canopy_layer_for(id)` (`terrain_id → canopy array layer`, -1 = none) for the
  blend shader's canopy overlay (see Edge Blending → Canopy overlay), and `peak_textures` (a third
  Texture2DArray of RGBA mountain relief from `textures/peaks/`) + `peak_layer_by_id` / `peak_layer_for(id)`
  for the blend shader's peak overlay (see Edge Blending → Peak overlay), and `river_textures` (a FOURTH
  Texture2DArray of flowing water from `textures/rivers/`) for the blend shader's river pass (see Edge
  Blending → Rivers). The river array is the one array **not** keyed by terrain id — a river is not a
  biome, it rides an edge — so its layer is the file's numeric prefix = river **class - 1**, and there is
  no `river_layer_for(id)`

### 2D Rendering Pipeline
- `MapView` gets textures from `TerrainTextureManager` and pre-renders hex-masked textures on startup
- Cached as `ImageTexture` per terrain ID for efficient drawing
- Falls back to solid colors when overlay mode is active
- Textures only displayed in base view (empty overlay key)
- Fog of War keeps textures: the draw loop classifies each tile once via
  `_visibility_state_at()` — Active tiles draw full-brightness, Discovered tiles
  are tinted toward the mist color (cloudy) via `_fow_texture_tint_for_state()`,
  Unexplored tiles fill with the fog color.
- Runtime toggle: `T` key (`enable_terrain_textures` / `_toggle_terrain_textures`)
- Edge blending: a flat↔flat **per-pixel biome blend shader** at biome seams (see Edge Blending below)

