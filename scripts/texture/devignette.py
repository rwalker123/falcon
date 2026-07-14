#!/usr/bin/env python3
"""Flat-field de-vignette with WRAP-AWARE field estimation.

Flattens the smooth low-frequency luminance structure Lucid Origin bakes into tiles
(center-bright/edge-dark vignette AND edge-darkening), while preserving high-frequency
detail. The low-frequency field is estimated on a WRAPPED (periodic) copy so the resulting
gain is seamless at the tile edges -> no dark boundary line when the tile repeats.
This is luminance flattening only; it does NOT heal/match edge CONTENT.
Usage: devignette.py <in.png> <out.png> [radius]
"""
import sys
import numpy as np
from PIL import Image, ImageFilter

def devignette(path_in, path_out, radius=None):
    img = Image.open(path_in).convert("RGB")
    w, h = img.size
    if radius is None:
        radius = max(w, h) / 8.0  # ~64 on a 512 tile
    rgb = np.asarray(img).astype(np.float32)
    lum = 0.299 * rgb[:, :, 0] + 0.587 * rgb[:, :, 1] + 0.114 * rgb[:, :, 2]
    # WRAP-AWARE low-freq field: tile luminance 3x3, blur, crop the center tile back out.
    # Blurring the periodic tiling makes the field periodic -> gain matches on opposite edges.
    tiled = np.tile(lum, (3, 3))
    tiled_img = Image.fromarray(np.clip(tiled, 0, 255).astype(np.uint8), "L")
    tiled_blur = np.asarray(tiled_img.filter(ImageFilter.GaussianBlur(radius))).astype(np.float32)
    field = tiled_blur[h:2 * h, w:2 * w]        # center tile = periodic low-freq field
    field = np.maximum(field, 1.0)
    target = float(field.mean())
    gain = np.clip(target / field, 0.55, 1.8)
    out = np.clip(rgb * gain[:, :, None], 0, 255).astype(np.uint8)
    Image.fromarray(out, "RGB").save(path_out)
    print(f"{path_in}: field ratio {field.max()/field.min():.2f}x  edge-gain L/R "
          f"{gain[:,0].mean():.3f}/{gain[:,-1].mean():.3f} T/B {gain[0,:].mean():.3f}/{gain[-1,:].mean():.3f}"
          f"  (wrap, radius {radius:.0f})")

if __name__ == "__main__":
    r = float(sys.argv[3]) if len(sys.argv) > 3 else None
    devignette(sys.argv[1], sys.argv[2], r)
