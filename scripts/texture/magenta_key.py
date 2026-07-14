#!/usr/bin/env python3
"""Chroma-key a Lucid canopy render (green crowns on a magenta/crimson background)
into an RGBA canopy tile with the background -> transparent.

Lucid outputs opaque, so we generate crowns on solid magenta and key here:
  metric m = 2G - R - B   (magenta/crimson -> very negative; green crown -> positive)
  alpha   = smoothstep(lo, hi, m)                 soft crown edges, hard-clear background
  despill: on kept pixels clamp R,B toward G to kill the pink fringe halo
Then a light toroidal edge cross-fade on the *premultiplied* RGB + alpha so the
crown tile is seamless (the shader also thins crowns at the treeline, so residual
seams are cheap, but this keeps interior tiling clean).

Usage: magenta_key.py <in> <out.png> [lo] [hi] [band]
  lo,hi = metric thresholds (default -40, 15). band = edge blend px (default 32).
"""
import sys
import numpy as np
from PIL import Image


def smoothstep(lo, hi, x):
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def key(path_in, path_out, lo=-40.0, hi=15.0, band=32):
    img = Image.open(path_in).convert("RGB").resize((512, 512), Image.LANCZOS)
    rgb = np.asarray(img).astype(np.float32)
    r, g, b = rgb[:, :, 0], rgb[:, :, 1], rgb[:, :, 2]
    m = 2 * g - r - b
    alpha = smoothstep(lo, hi, m)

    # Despill: clamp the magenta channels (R,B) so they never exceed the green
    # channel by more than a small slack -> removes the pink halo on crown edges.
    slack = 10.0
    r2 = np.minimum(r, g + slack)
    b2 = np.minimum(b, g + slack)
    out = np.stack([r2, g, b2], -1)

    # Toroidal edge cross-fade on RGB and alpha so the crown field tiles.
    def wrapblend(ch):
        o = ch.copy()
        w = np.linspace(0.0, 0.5, band)[:, None]  # 0 at edge -> 0.5 at band interior
        # top<->bottom
        o[:band, :] = ch[:band, :] * (0.5 + w) + ch[-band:, :][::-1, :] * (0.5 - w)
        o[-band:, :] = ch[-band:, :] * (0.5 + w[::-1]) + ch[:band, :][::-1, :] * (0.5 - w[::-1])
        # left<->right
        o2 = o.copy()
        wv = w.reshape(1, band)
        o2[:, :band] = o[:, :band] * (0.5 + wv) + o[:, -band:][:, ::-1] * (0.5 - wv)
        o2[:, -band:] = o[:, -band:] * (0.5 + wv[:, ::-1]) + o[:, :band][:, ::-1] * (0.5 - wv[:, ::-1])
        return o2

    a = wrapblend(alpha)
    rgb_b = np.stack([wrapblend(out[:, :, k]) for k in range(3)], -1)

    rgba = np.empty((512, 512, 4), np.float32)
    rgba[:, :, :3] = rgb_b
    rgba[:, :, 3] = np.clip(a, 0, 1) * 255
    rgba = np.clip(rgba, 0, 255).astype(np.uint8)
    Image.fromarray(rgba, "RGBA").save(path_out)

    al = rgba[:, :, 3]
    print(f"{path_out}: transparent%={100*(al<128).mean():.1f}  meanA={al.mean():.0f}  "
          f"fullyTransp%={100*(al==0).mean():.1f}")

    # Preview composited over mid-grey (what it roughly looks like over the floor).
    bg = np.full((512, 512, 3), 90, np.float32)
    af = (al.astype(np.float32) / 255)[..., None]
    comp = (rgba[:, :, :3].astype(np.float32) * af + bg * (1 - af)).astype(np.uint8)
    Image.fromarray(comp, "RGB").save(path_out.replace(".png", "_overgrey.png"))


if __name__ == "__main__":
    a = sys.argv
    key(a[1], a[2],
        float(a[3]) if len(a) > 3 else -40.0,
        float(a[4]) if len(a) > 4 else 15.0,
        int(a[5]) if len(a) > 5 else 32)
