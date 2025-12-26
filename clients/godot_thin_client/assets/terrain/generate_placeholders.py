#!/usr/bin/env python3
"""
Generate placeholder terrain textures with noise variation.
These are temporary textures that can be replaced with AI-generated ones.
"""

import os
from PIL import Image, ImageDraw, ImageFilter
import random
import math

# Terrain colors from MapView.gd TERRAIN_COLORS
TERRAIN_COLORS = {
    0: (11, 30, 61),      # Deep Ocean
    1: (20, 64, 94),      # Continental Shelf
    2: (28, 88, 114),     # Inland Sea
    3: (21, 122, 115),    # Coral Shelf
    4: (47, 127, 137),    # Hydrothermal Vent Field
    5: (184, 176, 138),   # Tidal Flat
    6: (155, 195, 123),   # River Delta
    7: (79, 124, 56),     # Mangrove Swamp
    8: (92, 140, 99),     # Freshwater Marsh
    9: (136, 182, 90),    # Floodplain
    10: (201, 176, 120),  # Alluvial Plain
    11: (211, 165, 77),   # Prairie Steppe
    12: (91, 127, 67),    # Mixed Woodland
    13: (59, 79, 49),     # Boreal Taiga
    14: (100, 85, 106),   # Peatland/Heath
    15: (231, 195, 106),  # Hot Desert Erg
    16: (138, 95, 60),    # Rocky Reg Desert
    17: (164, 135, 85),   # Semi-Arid Scrub
    18: (224, 220, 210),  # Salt Flat
    19: (58, 162, 162),   # Oasis Basin
    20: (166, 199, 207),  # Tundra
    21: (127, 183, 161),  # Periglacial Steppe
    22: (209, 228, 236),  # Glacier
    23: (192, 202, 214),  # Seasonal Snowfield
    24: (111, 155, 75),   # Rolling Hills
    25: (150, 126, 92),   # High Plateau
    26: (122, 127, 136),  # Alpine Mountain
    27: (74, 106, 85),    # Karst Highland
    28: (182, 101, 68),   # Canyon Badlands
    29: (140, 52, 45),    # Active Volcano Slope
    30: (64, 51, 61),     # Basaltic Lava Field
    31: (122, 110, 104),  # Ash Plain
    32: (76, 137, 145),   # Fumarole Basin
    33: (91, 70, 57),     # Impact Crater Field
    34: (46, 79, 92),     # Karst Cavern Mouth
    35: (79, 75, 51),     # Sinkhole Field
    36: (47, 143, 178),   # Aquifer Ceiling
}

TERRAIN_NAMES = {
    0: "deep_ocean",
    1: "continental_shelf",
    2: "inland_sea",
    3: "coral_shelf",
    4: "hydrothermal_vent_field",
    5: "tidal_flat",
    6: "river_delta",
    7: "mangrove_swamp",
    8: "freshwater_marsh",
    9: "floodplain",
    10: "alluvial_plain",
    11: "prairie_steppe",
    12: "mixed_woodland",
    13: "boreal_taiga",
    14: "peat_heath",
    15: "hot_desert_erg",
    16: "rocky_reg",
    17: "semi_arid_scrub",
    18: "salt_flat",
    19: "oasis_basin",
    20: "tundra",
    21: "periglacial_steppe",
    22: "glacier",
    23: "seasonal_snowfield",
    24: "rolling_hills",
    25: "high_plateau",
    26: "alpine_mountain",
    27: "karst_highland",
    28: "canyon_badlands",
    29: "active_volcano_slope",
    30: "basaltic_lava_field",
    31: "ash_plain",
    32: "fumarole_basin",
    33: "impact_crater_field",
    34: "karst_cavern_mouth",
    35: "sinkhole_field",
    36: "aquifer_ceiling",
}

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
    output_dir = os.path.join(os.path.dirname(__file__), "textures", "base")
    os.makedirs(output_dir, exist_ok=True)

    print(f"Generating {len(TERRAIN_COLORS)} placeholder terrain textures...")

    for terrain_id, color in TERRAIN_COLORS.items():
        name = TERRAIN_NAMES[terrain_id]
        filename = f"{terrain_id:02d}_{name}.png"
        filepath = os.path.join(output_dir, filename)

        # Generate texture with noise
        img = generate_noise_texture(color, size=512, noise_strength=20, seed=terrain_id * 42)

        # Make it seamless
        img = make_seamless(img)

        # Save
        img.save(filepath, "PNG")
        print(f"  Created: {filename}")

    print(f"\nDone! {len(TERRAIN_COLORS)} textures saved to {output_dir}")
    print("\nThese are placeholder textures. Replace with AI-generated textures for production.")

if __name__ == "__main__":
    main()
