#!/usr/bin/env python3
"""Even out a terrain tile's DETAIL AMPLITUDE — how strong its grain is, as opposed to how bright it
is. The sibling of `flatten_tone.py`, which fixes large-scale BRIGHTNESS: a tile can be flat in mean
luma and still repeat as a visible lattice when its texture is strong in one region and smooth in
another (the shipped open-water tiles were rippled along one edge and near-glassy in the middle —
deep ocean's local std ran 0.6 → 8.3, a 14× swing — so every repeat printed the same smooth blob
inside the same rippled frame).

Method: split luma into a low-pass base (wrap-aware blur at `base_radius`) and the detail above it;
measure the detail's local RMS (wrap-aware blur of detail² at `amp_radius`); scale the detail toward
the tile-wide median RMS with a clamped per-pixel gain; add it back as a luma offset applied equally
to R, G, B so the hue is untouched. Prints the local-std spread (max/min over an 8×8 block grid)
before and after — nearer 1 is more even, which tiles better.

Usage: equalize_detail.py <in> <out> [base_radius] [amp_radius] [gain_lo] [gain_hi]
  base_radius px of the base/detail split (default 6; the grain lives below it)
  amp_radius  px over which the local detail amplitude is measured (default 48)
  gain_lo     min per-pixel detail gain (default 0.35 — damps the over-rippled region)
  gain_hi     max per-pixel detail gain (default 3.0 — lifts the glassy region without
              amplifying it into noise)
"""
import sys
import numpy as np
from PIL import Image
from scipy.ndimage import gaussian_filter

BLOCKS = 8  # the before/after report's grid, per axis


def wrap_blur(a, radius):
    # mode="wrap" makes the blur see across the tile's own repeat, so the result stays seamless.
    return gaussian_filter(a.astype(np.float32), radius, mode="wrap")


def block_std_spread(lum):
    s = lum.shape[0] // BLOCKS
    d = [lum[i * s:(i + 1) * s, j * s:(j + 1) * s].std() for i in range(BLOCKS) for j in range(BLOCKS)]
    return max(d) / max(min(d), 1e-3), min(d), max(d)


def equalize(path_in, path_out, base_radius=6.0, amp_radius=48.0, gain_lo=0.35, gain_hi=3.0):
    rgb = np.asarray(Image.open(path_in).convert("RGB")).astype(np.float32)
    lum = 0.2126 * rgb[:, :, 0] + 0.7152 * rgb[:, :, 1] + 0.0722 * rgb[:, :, 2]

    detail = lum - wrap_blur(lum, base_radius)
    amp = np.sqrt(np.maximum(wrap_blur(detail * detail, amp_radius), 1e-6))
    target = float(np.median(amp))
    gain = np.clip(target / amp, gain_lo, gain_hi)

    offset = (detail * (gain - 1.0))[:, :, None]
    out = np.clip(rgb + offset, 0, 255).astype(np.uint8)
    Image.fromarray(out, "RGB").save(path_out)

    lum2 = 0.2126 * out[:, :, 0] + 0.7152 * out[:, :, 1] + 0.0722 * out[:, :, 2]
    b = block_std_spread(lum)
    a = block_std_spread(lum2.astype(np.float32))
    print(f"{path_out}: local-std spread before={b[0]:.1f}x ({b[1]:.1f}..{b[2]:.1f}) "
          f"after={a[0]:.1f}x ({a[1]:.1f}..{a[2]:.1f}) mean luma {lum.mean():.1f}->{lum2.mean():.1f}")


if __name__ == "__main__":
    a = sys.argv
    equalize(a[1], a[2],
             float(a[3]) if len(a) > 3 else 6.0,
             float(a[4]) if len(a) > 4 else 48.0,
             float(a[5]) if len(a) > 5 else 0.35,
             float(a[6]) if len(a) > 6 else 3.0)
