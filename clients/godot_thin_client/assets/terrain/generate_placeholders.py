#!/usr/bin/env python3
"""
Generate placeholder terrain textures with noise variation.
These are temporary textures that can be replaced with AI-generated ones.

Reads terrain definitions from terrain_config.json (single source of truth).
"""

import json
import os
from PIL import Image, ImageDraw, ImageFilter
import random
import math


def load_terrain_config():
    """Load terrain definitions from terrain_config.json."""
    config_path = os.path.join(os.path.dirname(__file__), "terrain_config.json")
    with open(config_path, 'r') as f:
        return json.load(f)


def generate_noise_texture(base_color, size=512, noise_strength=25, seed=None):
    """Generate a texture with Perlin-like noise variation."""
    if seed is not None:
        random.seed(seed)

    img = Image.new('RGB', (size, size))
    pixels = img.load()

    r, g, b = base_color

    # Generate multi-octave noise
    for y in range(size):
        for x in range(size):
            # Simple noise with multiple frequencies
            noise = 0.0
            for octave in range(3):
                freq = 2 ** octave
                amplitude = 1.0 / (octave + 1)
                noise += (random.random() - 0.5) * amplitude

            # Add some spatial coherence with sin waves
            wave1 = math.sin(x * 0.05 + random.random() * 0.1) * 0.3
            wave2 = math.sin(y * 0.07 + random.random() * 0.1) * 0.3
            noise += (wave1 + wave2) * 0.5

            variation = int(noise * noise_strength)

            new_r = max(0, min(255, r + variation))
            new_g = max(0, min(255, g + variation))
            new_b = max(0, min(255, b + variation))

            pixels[x, y] = (new_r, new_g, new_b)

    # Apply slight blur for smoother look
    img = img.filter(ImageFilter.GaussianBlur(radius=1.5))

    return img


def make_seamless(img):
    """Make texture seamless by blending edges."""
    size = img.size[0]
    result = img.copy()
    pixels = result.load()
    src_pixels = img.load()

    blend_width = size // 4

    for y in range(size):
        for x in range(blend_width):
            # Horizontal blend
            t = x / blend_width
            opposite_x = size - blend_width + x

            r1, g1, b1 = src_pixels[x, y]
            r2, g2, b2 = src_pixels[opposite_x, y]

            new_r = int(r1 * t + r2 * (1 - t))
            new_g = int(g1 * t + g2 * (1 - t))
            new_b = int(b1 * t + b2 * (1 - t))

            pixels[x, y] = (new_r, new_g, new_b)

    for x in range(size):
        for y in range(blend_width):
            # Vertical blend
            t = y / blend_width
            opposite_y = size - blend_width + y

            r1, g1, b1 = pixels[x, y]
            r2, g2, b2 = pixels[x, opposite_y]

            new_r = int(r1 * t + r2 * (1 - t))
            new_g = int(g1 * t + g2 * (1 - t))
            new_b = int(b1 * t + b2 * (1 - t))

            pixels[x, y] = (new_r, new_g, new_b)

    return result


def main():
    # Load terrain definitions from single source of truth
    config = load_terrain_config()
    terrains = config.get("terrains", [])

    output_dir = os.path.join(os.path.dirname(__file__), "textures", "base")
    os.makedirs(output_dir, exist_ok=True)

    print(f"Generating {len(terrains)} placeholder terrain textures...")

    for terrain in terrains:
        terrain_id = terrain["id"]
        name = terrain["name"]
        color = tuple(terrain["color"])
        filename = f"{terrain_id:02d}_{name}.png"
        filepath = os.path.join(output_dir, filename)

        # Generate texture with noise
        img = generate_noise_texture(color, size=512, noise_strength=20, seed=terrain_id * 42)

        # Make it seamless
        img = make_seamless(img)

        # Save
        img.save(filepath, "PNG")
        print(f"  Created: {filename}")

    print(f"\nDone! {len(terrains)} textures saved to {output_dir}")
    print("\nThese are placeholder textures. Replace with AI-generated textures for production.")


if __name__ == "__main__":
    main()
