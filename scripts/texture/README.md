# Terrain texture toolchain

Post-processing for AI-generated terrain art (Leonardo.ai). These scripts were written during
the terrain-texture arc (PR #113) but lived in a session scratchpad and were never committed —
they are checked in here so the next texture arc doesn't re-derive them.

Prompts live in `clients/godot_thin_client/assets/terrain/texture_prompts.txt`.

## Requirements

```bash
python3 -m venv .venv && .venv/bin/pip install pillow numpy
```

## The workflow

A raw generation is almost never usable as-is. It fails in three distinct ways, and each
script fixes exactly one — run them in this order.

### 1. `tile_check.py <in> <out>` — diagnose first, always

Tiles the image 3×3. **Nothing else in this directory should be run until you have looked at
this.** A texture that looks perfect on its own routinely turns into a visible quilt when
repeated, and this is the only way to see it. The three failure signatures:

- **A hard seam line** — the edges don't meet. Fix with `seamless_edges.py`.
- **A checkerboard / lattice of dark anchors** — low-frequency tone structure (vignette,
  a dark smudge, directional sheen) that becomes periodic when repeated. Fix with
  `flatten_tone.py`.
- **A kaleidoscope** — mirrored features radiating from the tile corners. Not fixable by
  post-processing; re-roll.

### 2. `flatten_tone.py <in> <out> [radius] [gain_lo] [gain_hi] [contrast]`

Removes large-scale tone structure while preserving the fine grain. Homomorphic flatten:
estimate illumination as a heavy wrap-aware blur of luminance, then divide it out. Prints the
illumination spread before/after — **lower is flatter is better-tiling**. Under ~5 is good.

`devignette.py` is the narrower, older tool for the specific corner-darkening case;
`flatten_tone.py` supersedes it in most situations.

### 3. `seamless_edges.py <in> <out> [band]`

Tapered cross-blend of the outer band against the opposite edge, so the first and last row
become identical. Prints the edge match (0.00 = perfectly seamless).

**Only safe on SMOOTH, feature-poor surfaces** (open water, sky). On a feature-rich tile
(gravel, foliage, rock) it ghosts and smears recognizable features across the blend band —
which reads far worse than the seam it was fixing. On those, get seamlessness from the
generator instead, and re-roll if it won't.

### The gate: `make_seamless.py --check` — run after dropping in ANY base art

Where `seamless_edges.py` works on one file you name, `make_seamless.py` works on the whole shipped
roster: every base texture `terrain_config.json` registers (exactly the files the shader's
`biome_array` loads, in `clients/godot_thin_client/assets/terrain/textures/base/`). It runs from any
working directory.

- `--check` measures each texture's **wrap ratio** per axis — the mean luma step between the last and
  first column (the pair a repeat puts side by side) over the mean step between adjacent interior
  columns, rows likewise. Seamless scores ~1; the bar is `SEAM_RATIO_MAX` = 1.3. Exits 1 if any
  texture is over it.
- Without `--check` it rewrites IN PLACE only the textures over the bar, cross-fading each wrap edge
  with the image rolled by half its size (variance-preserving, so the band keeps its grain).
  Idempotent: a fixed texture measures under the bar and is skipped next time.

**The same ghosting caveat as `seamless_edges.py` applies**, and it was measured: on the first pass
the fade doubled glacier cracks and desert ripples inside the band, and left a vignette or colour
cast (woodland, rolling hills, snowfield) repeating as a grid. Those were regenerated. Check a
generation BEFORE fixing it: an image under the bar after the 512² resize needs no fade at all.

**Why the gate exists at all:** the shader samples base textures in continuous world space, so a
texture whose edges do not meet draws a straight line across the map at every repeat — through the
middle of hexes, and no blend setting can hide it.

### 4. `cool_grade.py <in> <out> [r_gain] [g_gain] [b_gain] [sat]`

Hue-shifts a warm tile toward cool blue-green while restoring chroma around luminance.

**A caution learned the hard way:** blending toward a flat target tint desaturates into grey
mud (it read as wet asphalt). Per-channel gain + chroma restore is why this script works.
And it has limits — it cannot rescue an image whose blue channel is genuinely the *lowest*
(a gold gravel photo will not become water). Check the mean RGB first; if `R - B` is large,
re-roll rather than grade.

### `magenta_key.py`

For RGBA overlays (canopy crowns, mountain peaks) generated on a magenta background — keys
the magenta out to transparency. Not needed for base terrain or the river-edge textures,
which are fully opaque.

## Gotchas

- **Overlay textures must not have painted edges.** Canopy, peaks, and the river-edge water
  are all shaped by a *shader mask*. A bank/vignette/soft edge baked into the art fights the
  mask and reads as a double rim.
- **Non-directional.** These tile across hexes at varying orientations, so a strong one-way
  streak visibly rotates at the seams. Swirls, not arrows.
- Base terrain is RGB 512×512 in `textures/base/`, named `%02d_%s.png` by terrain id. The
  filename *is* the registration — `TerrainTextureManager` derives it from the id and name in
  `terrain_config.json`.
- **New art does not appear until the project is RE-IMPORTED.** `TerrainTextureManager._load_asset_image`
  tries `ResourceLoader` first (so exported builds, where the PNG is a `.ctex` in the `.pck`, work), and
  in the editor tree that serves the IMPORTED copy in `.godot/imported/` — which a client launch does
  not refresh. `Image.load_from_file` is only the fallback for a path the loader does not know.
  `scripts/run_stack.sh` re-imports on launch when any importable asset is newer than its own stamp
  (`ensure_godot_import`, both the full stack and `--client-only`), so restarting through it is enough.
  A bare `godot` launch or a harness run (`scripts/preview.sh`) does NOT — run
  `godot --headless --path clients/godot_thin_client --import` first. Symptom of skipping it: the build
  stamp is current and the map still draws the old art. A replaced texture's
  `.godot/imported/<name>.png-*.md5` carries a `source_md5` that must equal the PNG's.
