#!/usr/bin/env bash
#
# Download the spoken narrator's model files (docs/plan_telling_narrator.md) into the gitignored
# clients/godot_thin_client/narrator_model/ directory the client looks in by default.
#
#   scripts/fetch_narrator_model.sh            # int8 model (88 MB, ~2x faster on CPU) + voices
#   scripts/fetch_narrator_model.sh --fp32     # the 310 MB fp32 model instead (preferred over int8 when both are present)
#
# Idempotent: a file already present is skipped. Narration also needs espeak-ng installed on the
# machine (brew install espeak-ng / apt install libespeak-ng1); it is loaded at runtime, never bundled.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT_DIR/clients/godot_thin_client/narrator_model"
BASE_URL="https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0"
MODEL="kokoro-v1.0.int8.onnx"
VOICES="voices-v1.0.bin"

case "${1:-}" in
  "") ;;
  --fp32) MODEL="kokoro-v1.0.onnx" ;;
  -h|--help)
    sed -n '2,10p' "${BASH_SOURCE[0]}"
    exit 0
    ;;
  *)
    echo "unknown argument: $1 (try --fp32 or --help)" >&2
    exit 2
    ;;
esac

mkdir -p "$DEST"

fetch() {
  local name="$1"
  if [[ -s "$DEST/$name" ]]; then
    echo "have $name"
    return
  fi
  echo "fetching $name"
  # Download beside the target and rename, so an interrupted run never leaves a truncated file
  # that the idempotence check above would then accept.
  curl -fL --retry 3 -o "$DEST/$name.part" "$BASE_URL/$name"
  mv "$DEST/$name.part" "$DEST/$name"
}

fetch "$MODEL"
fetch "$VOICES"
echo "narrator model ready in $DEST"
