#!/usr/bin/env python3
"""Remove LOW-FREQUENCY tone structure (vignette, dark organic smudges, directional sheen) from a
terrain tile while preserving the high-frequency pebble/grain detail. Those large-scale tone blobs
are what turn into a visible TILING LATTICE when the tile repeats (periodic dark anchors + chevron
sheen). Homomorphic-style flatten: estimate illumination as a heavy wrap-aware blur of luminance,
then divide it out (renormalized to the tile mean) so only local contrast remains. Optional contrast
pull afterwards to further mute the repeat.

Usage: flatten_tone.py <in> <out> [radius] [gain_lo] [gain_hi] [contrast]
  radius   px of the illumination blur (default 96; bigger = only very-large tone removed)
  gain_lo  min per-pixel gain clamp (default 0.7)   gain_hi max gain (default 1.5)
  contrast post contrast, 1.0=unchanged (default 1.0)
"""
import sys
import numpy as np
from PIL import Image, ImageFilter, ImageEnhance

PAD = 192  # wrap-pad margin (> blur radius) so the flatten is seam-consistent


def wrap_pad(a, pad):
    return np.pad(a, ((pad, pad), (pad, pad), (0, 0)), mode="wrap")


def flatten(path_in, path_out, radius=96.0, gain_lo=0.7, gain_hi=1.5, contrast=1.0):
    img = Image.open(path_in).convert("RGB")
    w, h = img.size
    rgb = np.asarray(img).astype(np.float32)

    # Illumination = heavy wrap-aware blur of luminance.
    lum = (0.299 * rgb[:, :, 0] + 0.587 * rgb[:, :, 1] + 0.114 * rgb[:, :, 2])
    lp = wrap_pad(lum[:, :, None].astype(np.uint8), PAD)[:, :, 0]
    illum = np.asarray(Image.fromarray(lp, "L").filter(ImageFilter.GaussianBlur(radius))).astype(np.float32)
    illum = illum[PAD:PAD + h, PAD:PAD + w]

    mean = float(illum.mean())
    gain = np.clip(mean / np.clip(illum, 1e-3, None), gain_lo, gain_hi)[:, :, None]
    out = np.clip(rgb * gain, 0, 255).astype(np.uint8)
    p = Image.fromarray(out, "RGB")
    if contrast != 1.0:
        p = ImageEnhance.Contrast(p).enhance(contrast)
    p.save(path_out)

    # Report residual low-freq spread after flatten (lower = flatter = tiles better).
    l2 = (0.299 * np.asarray(p)[:, :, 0] + 0.587 * np.asarray(p)[:, :, 1] + 0.114 * np.asarray(p)[:, :, 2])
    lp2 = wrap_pad(l2[:, :, None].astype(np.uint8), PAD)[:, :, 0]
    il2 = np.asarray(Image.fromarray(lp2, "L").filter(ImageFilter.GaussianBlur(radius))).astype(np.float32)[PAD:PAD + h, PAD:PAD + w]
    print(f"{path_out}: illum spread before={illum.max()-illum.min():.0f} after={il2.max()-il2.min():.0f} "
          f"(radius={radius} gain[{gain_lo},{gain_hi}] contrast={contrast})")


if __name__ == "__main__":
    a = sys.argv
    flatten(a[1], a[2],
            float(a[3]) if len(a) > 3 else 96.0,
            float(a[4]) if len(a) > 4 else 0.7,
            float(a[5]) if len(a) > 5 else 1.5,
            float(a[6]) if len(a) > 6 else 1.0)
