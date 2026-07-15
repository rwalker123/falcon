#!/usr/bin/env python3
"""Make a SMOOTH (feature-free) tile edge-seamless via a tapered edge cross-blend.

For each axis, the outermost `band` rows/cols are cross-faded with the opposite
(wrapping) edge so that the first and last line become identical (seam-continuous
when tiled) and fade back to the original content inward. On a smooth low-contrast
surface (open water) this is invisible; do NOT use on feature-rich tiles (it would
ghost/smear features — exactly the failure mode we avoid on land).
Usage: seamless_edges.py <in.png> <out.png> [band]
"""
import sys
import numpy as np
from PIL import Image


def blend_axis(arr, axis, band):
    n = arr.shape[axis]
    band = int(min(band, n // 2))
    arr = np.moveaxis(arr, axis, 0)  # work along axis 0
    out = arr.copy()
    for i in range(band):
        w = 0.5 * (1.0 - i / band)          # 0.5 at the very edge -> 0 at band depth
        a = arr[i].astype(np.float32)        # i-th row from the start
        b = arr[n - 1 - i].astype(np.float32)  # mirror row from the end
        out[i] = (1.0 - w) * a + w * b
        out[n - 1 - i] = (1.0 - w) * b + w * a
    return np.moveaxis(out, 0, axis)


def seamless(path_in, path_out, band=None):
    img = Image.open(path_in).convert("RGB")
    w, h = img.size
    if band is None:
        band = max(w, h) // 6  # ~85 on 512
    arr = np.asarray(img).astype(np.float32)
    arr = blend_axis(arr, 0, band)  # vertical seam (top<->bottom)
    arr = blend_axis(arr, 1, band)  # horizontal seam (left<->right)
    out = np.clip(arr, 0, 255).astype(np.uint8)
    Image.fromarray(out, "RGB").save(path_out)
    # report edge-match: mean abs diff between opposite edge lines (0 = perfectly seamless)
    top, bot = out[0].astype(np.float32), out[-1].astype(np.float32)
    left, right = out[:, 0].astype(np.float32), out[:, -1].astype(np.float32)
    print(f"{path_in}: edge match after blend — T/B rows Δ {np.abs(top-bot).mean():.2f}, "
          f"L/R cols Δ {np.abs(left-right).mean():.2f}  (band {band})")


if __name__ == "__main__":
    b = int(sys.argv[3]) if len(sys.argv) > 3 else None
    seamless(sys.argv[1], sys.argv[2], b)
