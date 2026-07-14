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
  `terrain_config.json` and loads via `Image.load_from_file`, bypassing Godot's import cache.
