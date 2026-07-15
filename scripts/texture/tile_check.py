#!/usr/bin/env python3
"""Tile a base texture 3x3 to expose edge seams / brightness-block quilting fast."""
import sys
from PIL import Image
img = Image.open(sys.argv[1]).convert("RGB")
w, h = img.size
out = Image.new("RGB", (w*3, h*3))
for j in range(3):
    for i in range(3):
        out.paste(img, (i*w, j*h))
out.resize((768, 768), Image.LANCZOS).save(sys.argv[2])
print(f"tiled {sys.argv[1]} 3x3 -> {sys.argv[2]}")
