#!/usr/bin/env bash
#
# Build terrain texture atlas if out of date.
# Usage: scripts/build_terrain_textures.sh [--force]
#
# Compares modification times of source PNGs against the output atlas.
# Rebuilds only if sources are newer or atlas is missing.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CLIENT_DIR="clients/godot_thin_client"
TEXTURES_DIR="$CLIENT_DIR/assets/terrain/textures/base"
ATLAS_PATH="$CLIENT_DIR/assets/terrain/textures/terrain_atlas.res"
BUILDER_SCRIPT="assets/terrain/TerrainAtlasBuilder.gd"

FORCE=false
if [[ "${1:-}" == "--force" ]]; then
  FORCE=true
fi

needs_rebuild() {
  # Rebuild if atlas doesn't exist
  if [[ ! -f "$ATLAS_PATH" ]]; then
    return 0
  fi

  # Rebuild if any source PNG is newer than the atlas
  if [[ -d "$TEXTURES_DIR" ]]; then
    while IFS= read -r -d '' png; do
      if [[ "$png" -nt "$ATLAS_PATH" ]]; then
        return 0
      fi
    done < <(find "$TEXTURES_DIR" -name "*.png" -print0 2>/dev/null)
  fi

  # Rebuild if builder script is newer than the atlas
  if [[ -f "$CLIENT_DIR/$BUILDER_SCRIPT" && "$CLIENT_DIR/$BUILDER_SCRIPT" -nt "$ATLAS_PATH" ]]; then
    return 0
  fi

  return 1
}

if [[ "$FORCE" == true ]]; then
  echo "[build_terrain_textures] Force rebuild requested..."
elif needs_rebuild; then
  echo "[build_terrain_textures] Terrain atlas out of date, rebuilding..."
else
  echo "[build_terrain_textures] Terrain atlas is up to date."
  exit 0
fi

# Check if source textures exist
if [[ ! -d "$TEXTURES_DIR" ]] || [[ -z "$(ls -A "$TEXTURES_DIR"/*.png 2>/dev/null)" ]]; then
  echo "[build_terrain_textures] No source textures found in $TEXTURES_DIR"
  echo "[build_terrain_textures] Run: godot --headless --path $CLIENT_DIR --script assets/terrain/TerrainTextureGenerator.gd"
  exit 0
fi

echo "[build_terrain_textures] Building terrain atlas from $(ls "$TEXTURES_DIR"/*.png 2>/dev/null | wc -l | tr -d ' ') textures..."
godot --headless --path "$CLIENT_DIR" --script "$BUILDER_SCRIPT"

if [[ -f "$ATLAS_PATH" ]]; then
  echo "[build_terrain_textures] Successfully built: $ATLAS_PATH"
else
  echo "[build_terrain_textures] Warning: Atlas file not created" >&2
  exit 1
fi
