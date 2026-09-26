#!/usr/bin/env bash
#
# Terrain texture build script (placeholder).
# Terrain textures are now loaded at runtime from individual PNGs.
# This script remains for backwards compatibility with run_stack.sh.
#
# The pre-built atlas approach was removed because Godot's Texture2DArray
# doesn't serialize image data properly when saved via ResourceSaver.

echo "[build_terrain_textures] No pre-build needed; changed PNGs are re-imported by run_stack.sh (ensure_godot_import)."
