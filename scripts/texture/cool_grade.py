#!/usr/bin/env python3
"""Cool-grade a warm shallow-water tile toward clear stream water.

Blending toward a flat tint desaturates into grey mud (tried; it read as wet asphalt).
Instead: per-channel gain to swing the hue from gold toward blue-green, THEN restore
chroma, so the bed keeps its stony colour variation and only the water's cast changes.
Usage: cool_grade.py <in> <out> [r_gain] [g_gain] [b_gain] [sat]"""
import sys
import numpy as np
from PIL import Image

def grade(src, dst, rg=0.82, gg=1.02, bg=1.30, sat=1.30):
    rgb = np.asarray(Image.open(src).convert("RGB")).astype(np.float32) / 255.0
    out = rgb * np.array([rg, gg, bg], dtype=np.float32)[None, None, :]
    lum = (0.299 * out[:, :, 0] + 0.587 * out[:, :, 1] + 0.114 * out[:, :, 2])[:, :, None]
    out = lum + (out - lum) * sat          # restore/boost chroma around luminance
    out = np.clip(out, 0, 1)
    Image.fromarray((out * 255).astype(np.uint8), "RGB").save(dst)
    m = out.reshape(-1, 3).mean(axis=0) * 255
    print(f"{dst}: mean RGB = ({m[0]:.0f}, {m[1]:.0f}, {m[2]:.0f})  gains=({rg},{gg},{bg}) sat={sat}")

if __name__ == "__main__":
    a = sys.argv
    grade(a[1], a[2], *(float(x) for x in a[3:7]) if len(a) > 6 else (0.82, 1.02, 1.30, 1.30))
