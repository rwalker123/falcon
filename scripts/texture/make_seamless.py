#!/usr/bin/env python3
"""
Make the BASE biome textures tile seamlessly, and gate that they do.

The terrain shader samples every base biome in CONTINUOUS world space with `repeat_enable`
(`base_uv = v_map / (2 * hex_radius) * base_texture_scale`), so one texture repeats every
1 / base_texture_scale hex-rows across the whole map. A texture whose left edge does not continue
its right edge (or top/bottom) therefore draws a STRAIGHT LINE across the map at every repeat —
through the middle of hexes, independent of any biome seam or of the edge blend.

    python3 scripts/texture/make_seamless.py --check   # measure every base texture; exit 1 if any is over the bar
    python3 scripts/texture/make_seamless.py           # rewrite, in place, only the textures over the bar

Runs from any working directory: the paths below are resolved from this file's own location.

THE MEASURE — the wrap ratio, per axis: the mean |dL| between the last and first column (the pair
the repeat puts side by side) divided by the mean |dL| between adjacent INTERIOR columns (the
texture's own grain); rows likewise. A seamless texture scores ~1 — the wrap looks like any other
pair of neighbours. L is Rec.709 luma.

THE FIX — cross-fade each wrap edge with the image rolled by half its size (the classic
"tile seamless" offset blend): near an edge the pixel takes the content from the middle of the
image, whose two halves are genuine neighbours, so the last column continues the first. The
horizontal pass runs first and the vertical pass on its result; the vertical weight depends on the
row alone and both of its operands are already horizontally seamless, so the corners come out
seamless on both axes. The blend is VARIANCE-PRESERVING (each operand's deviation from the image mean
is renormalised by sqrt(a^2 + b^2)): a plain mix of two uncorrelated patches halves the contrast and
reads as a flat, smeared band; this keeps the band's grain at the texture's own contrast.

Idempotent by construction: a fixed texture measures under SEAM_RATIO_MAX and is skipped next time.
Format, bit depth and dimensions are preserved. The texture list is terrain_config.json's terrain
roster — exactly the files TerrainTextureManager loads into the shader's `biome_array`.
"""

import argparse
import json
import os
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
# This file lives at <repo>/scripts/texture/; the terrain assets at <repo>/clients/godot_thin_client/assets/terrain/.
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
TERRAIN_DIR = os.path.join(REPO_ROOT, "clients", "godot_thin_client", "assets", "terrain")
CONFIG_PATH = os.path.join(TERRAIN_DIR, "terrain_config.json")
BASE_DIR = os.path.join(TERRAIN_DIR, "textures", "base")
# TerrainTextureManager's filename scheme: "%02d_%s.png" % [terrain_id, name].
BASE_FILENAME_FORMAT = "%02d_%s.png"

# The bar. A worse-axis wrap ratio above this is a visible line at every texture repeat. Interior
# neighbour pairs of the shipped art already vary by ~±20% between rows, so the bar sits just above
# that noise rather than at 1.0.
SEAM_RATIO_MAX = 1.3
# Width of the cross-fade band at each edge, as a fraction of the image size along that axis. Wider
# hides the join better but blends more of the texture with its offset copy (more chance of a doubled
# tuft or crack); 1/8 of a 512px texture is a 64px band.
BLEND_BAND_FRACTION = 1.0 / 8.0
# The half-size roll that supplies the partner content (see THE FIX above).
ROLL_FRACTION = 0.5
# Rec.709 luma weights, the same luma the shader's height term uses.
LUMA_WEIGHTS = np.array([0.2126, 0.7152, 0.0722])
# Floor on the interior neighbour difference, so a flat (constant) texture cannot divide by zero.
MIN_INTERIOR_DIFF = 1e-6


def base_texture_paths():
    """Every base texture biome_array loads, in terrain-id order (terrain_config.json's roster)."""
    with open(CONFIG_PATH) as f:
        config = json.load(f)
    return [
        os.path.join(BASE_DIR, BASE_FILENAME_FORMAT % (t["id"], t["name"]))
        for t in sorted(config["terrains"], key=lambda t: t["id"])
    ]


def luma(pixels):
    return pixels[..., :3] @ LUMA_WEIGHTS


def wrap_ratios(pixels):
    """(x_ratio, y_ratio): |dL| across the wrap over |dL| between interior neighbours, per axis."""
    lum = luma(pixels.astype(np.float64))
    inner_x = max(np.abs(np.diff(lum, axis=1)).mean(), MIN_INTERIOR_DIFF)
    inner_y = max(np.abs(np.diff(lum, axis=0)).mean(), MIN_INTERIOR_DIFF)
    wrap_x = np.abs(lum[:, 0] - lum[:, -1]).mean()
    wrap_y = np.abs(lum[0, :] - lum[-1, :]).mean()
    return wrap_x / inner_x, wrap_y / inner_y


def edge_weight(n):
    """Partner weight along an axis of length n: 1 at both edges, smoothstep to 0 one band in."""
    band = n * BLEND_BAND_FRACTION
    idx = np.arange(n) + 0.5                       # pixel centres
    dist = np.minimum(idx, n - idx)                # distance to the nearer edge
    t = np.clip(dist / band, 0.0, 1.0)
    return 1.0 - t * t * (3.0 - 2.0 * t)


def blend_axis(pixels, axis):
    """One seamless pass along `axis` (1 = horizontal wrap, 0 = vertical wrap)."""
    n = pixels.shape[axis]
    partner = np.roll(pixels, int(round(n * ROLL_FRACTION)), axis=axis)
    w = edge_weight(n)
    w = w[np.newaxis, :, np.newaxis] if axis == 1 else w[:, np.newaxis, np.newaxis]
    mean = pixels.reshape(-1, pixels.shape[2]).mean(axis=0)
    a, b = 1.0 - w, w
    # Variance-preserving blend: deviations from the mean, renormalised so the band keeps its contrast.
    return mean + ((pixels - mean) * a + (partner - mean) * b) / np.sqrt(a * a + b * b)


def make_seamless(pixels):
    out = blend_axis(pixels.astype(np.float64), axis=1)
    return blend_axis(out, axis=0)


def process(path, fix):
    img = Image.open(path)
    mode, info = img.mode, img.info
    pixels = np.asarray(img)
    rx, ry = wrap_ratios(pixels)
    worst = max(rx, ry)
    line = f"{os.path.basename(path):34s} x {rx:5.2f}  y {ry:5.2f}"
    if worst <= SEAM_RATIO_MAX:
        print(f"{line}  ok")
        return True
    if not fix:
        print(f"{line}  OVER {SEAM_RATIO_MAX}")
        return False
    max_value = np.iinfo(pixels.dtype).max
    fixed = np.clip(np.rint(make_seamless(pixels)), 0, max_value).astype(pixels.dtype)
    Image.fromarray(fixed, mode=mode).save(path, **{k: v for k, v in info.items() if k in ("dpi", "gamma")})
    fx, fy = wrap_ratios(fixed)
    print(f"{line}  -> x {fx:5.2f}  y {fy:5.2f}  FIXED")
    return max(fx, fy) <= SEAM_RATIO_MAX


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="measure only; exit 1 if any texture is over the bar")
    args = parser.parse_args()
    ok = True
    for path in base_texture_paths():
        if not os.path.exists(path):
            print(f"{os.path.basename(path):34s} MISSING")
            ok = False
            continue
        ok = process(path, fix=not args.check) and ok
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
