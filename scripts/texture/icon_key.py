#!/usr/bin/env python3
"""Chroma-key a Lucid ICON render (flat vector art on a solid background) into an
RGBA icon with the background -> transparent.

Lucid outputs opaque JPEG, so icons are generated on a solid MAGENTA field and
keyed here -- the same trick the canopy/peak overlays use (see magenta_key.py).
This is the ICON variant and differs in two ways that matter:

  * magenta_key.py keys on `2G - R - B`, which is tuned for GREEN crowns. Earth-tone
    animal art (brown/tan) also scores negative on that metric, so it would erase the
    subject. Here the key is euclidean DISTANCE from the background colour instead,
    which is palette-agnostic: it works for a brown rabbit, a grey boar or a white
    fowl without per-subject retuning.
  * the background colour is SAMPLED from the corners by default rather than assumed,
    so a render that came back cream/white instead of magenta still keys correctly.

Soft thresholds keep the anti-aliased outline intact (a hard cutoff leaves the icon
with jagged stair-stepped edges at the 24px sizes these are drawn at), and a despill
pass pulls the key hue out of the surviving edge pixels so no magenta fringe rings the
silhouette when it is composited over terrain.

Usage: icon_key.py <in.jpg> <out.png> [--bg R,G,B] [--lo N] [--hi N] [--size N]
  --bg    background colour; default = sampled from the four corners
  --lo    distance below which a pixel is fully background (default 40)
  --hi    distance above which a pixel is fully opaque   (default 90)
  --size  output square size (default 512)

The lo/hi window is in 0-255 RGB distance. Widen it (raise --hi) if JPEG ringing
leaves a halo; narrow it if thin features (whiskers, antler tines) are being eaten.
"""
import argparse
import sys

import numpy as np
from PIL import Image

# Distance thresholds bounding the soft key. Below LO a pixel is pure background,
# above HI it is pure subject; between, alpha ramps so anti-aliased edges survive.
DEFAULT_LO = 40.0
DEFAULT_HI = 90.0
DEFAULT_SIZE = 512
# Corner patch (px) averaged to infer the background colour.
CORNER_PATCH = 8
# How far a kept pixel may exceed the background hue before it is pulled back during
# despill. Generous enough to leave real colour alone, tight enough to kill fringing.
DESPILL_SLACK = 12.0
# Per-channel distance above the background's own mean at which despill reaches full
# strength. Keeps a neutral (white/cream) background from being treated as a key hue.
DESPILL_KEY_NORM = 48.0
# Margin left around the silhouette when framing is normalized, as a fraction of the
# output square per side. Keeps the outline off the texture edge so filtering has room.
FRAME_PAD_FRACTION = 0.06


def smoothstep(lo, hi, x):
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3 - 2 * t)


def sample_background(rgb):
    """Average the four corner patches -- the render's background is uniform there."""
    h, w, _ = rgb.shape
    p = CORNER_PATCH
    corners = np.concatenate(
        [
            rgb[:p, :p].reshape(-1, 3),
            rgb[:p, w - p :].reshape(-1, 3),
            rgb[h - p :, :p].reshape(-1, 3),
            rgb[h - p :, w - p :].reshape(-1, 3),
        ]
    )
    return corners.mean(axis=0)


def normalize_framing(icon, size):
    """Crop to the silhouette, then re-centre it in a square canvas with fixed padding.

    The generator frames every subject differently -- a rabbit fills its canvas, a deer
    wastes half of it on empty space above the antlers. Drawn into the map's fixed marker
    box those differences read as random size jitter between species, so framing is
    normalized HERE rather than left to the renderer: every icon ends up the same square
    with the same margin, and the drawn size is then purely the marker's business.
    """
    bbox = icon.getbbox()  # alpha-aware: the keyed background is fully transparent
    if bbox is None:
        return icon
    cropped = icon.crop(bbox)
    inner = max(1, int(round(size * (1.0 - 2.0 * FRAME_PAD_FRACTION))))
    w, h = cropped.size
    scale = min(inner / w, inner / h)
    fitted = cropped.resize((max(1, round(w * scale)), max(1, round(h * scale))), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.alpha_composite(fitted, ((size - fitted.size[0]) // 2, (size - fitted.size[1]) // 2))
    return canvas


def key(path_in, path_out, bg=None, lo=DEFAULT_LO, hi=DEFAULT_HI, size=DEFAULT_SIZE, trim=True):
    img = Image.open(path_in).convert("RGB").resize((size, size), Image.LANCZOS)
    rgb = np.asarray(img).astype(np.float32)

    if bg is None:
        bg = sample_background(rgb)
    bg = np.asarray(bg, dtype=np.float32)

    dist = np.sqrt(((rgb - bg) ** 2).sum(axis=-1))
    alpha = smoothstep(lo, hi, dist)

    # Despill: on surviving pixels, pull back any channel the background hue bleeds
    # into, so no coloured fringe rings the silhouette.
    #
    # This is weighted by how SATURATED the key is, per channel, and that weighting is
    # essential: a neutral background (white/cream, which is what Lucid returns when it
    # ignores the magenta instruction) has every channel equal to its own mean, so the
    # weight is 0 and despill is a no-op. Without that guard a white key made every
    # channel look like spill and desaturated the whole icon -- warm brown art came out
    # olive. Only a genuinely saturated key (magenta: R,B far above the mean) despills.
    # It is ALSO weighted by (1 - alpha), which confines it to the anti-aliased rim.
    # Spill is background light physically mixed into a partially covered pixel, so it
    # can only exist where alpha < 1; a fully opaque interior pixel has none. Without
    # that term the despill drains the key's dominant channel out of the artwork itself
    # -- a magenta/crimson key is red-dominant and so is brown fur, so every brown icon
    # came back olive. Edge-only despill kills the fringe and leaves the fills alone.
    key_strength = np.clip((bg - bg.mean()) / DESPILL_KEY_NORM, 0.0, 1.0)
    mid = rgb.mean(axis=-1, keepdims=True)
    edge = (1.0 - alpha)[:, :, None]
    spill = np.clip(rgb - (mid + DESPILL_SLACK), 0.0, None) * key_strength * edge
    out = np.clip(rgb - spill, 0.0, 255.0)

    rgba = np.dstack([out, alpha * 255.0]).astype(np.uint8)
    icon = Image.fromarray(rgba, "RGBA")
    if trim:
        icon = normalize_framing(icon, size)
    icon.save(path_out)

    covered = float((alpha > 0.5).mean())
    print(
        f"{path_in} -> {path_out}  bg=({bg[0]:.0f},{bg[1]:.0f},{bg[2]:.0f})  "
        f"subject covers {covered * 100:.1f}% of the canvas"
    )
    # A silhouette filling almost nothing or almost everything means the key missed.
    if covered < 0.02:
        print("  WARNING: subject nearly empty -- background colour or --lo/--hi wrong?")
    elif covered > 0.95:
        print("  WARNING: almost nothing keyed out -- is the background uniform?")


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("src")
    ap.add_argument("dst")
    ap.add_argument("--bg", default=None, help="background colour as R,G,B")
    ap.add_argument("--lo", type=float, default=DEFAULT_LO)
    ap.add_argument("--hi", type=float, default=DEFAULT_HI)
    ap.add_argument("--size", type=int, default=DEFAULT_SIZE)
    ap.add_argument(
        "--no-trim",
        action="store_true",
        help="keep the generator's framing instead of re-centring the silhouette",
    )
    args = ap.parse_args(argv)

    bg = None
    if args.bg:
        bg = [float(c) for c in args.bg.split(",")]
        if len(bg) != 3:
            ap.error("--bg wants R,G,B")

    key(
        args.src,
        args.dst,
        bg=bg,
        lo=args.lo,
        hi=args.hi,
        size=args.size,
        trim=not args.no_trim,
    )


if __name__ == "__main__":
    main(sys.argv[1:])
