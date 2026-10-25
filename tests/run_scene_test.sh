#!/bin/bash
# Run the full scene test pipeline for a wallpaper:
#   1. Unpack the .pkg to /tmp/HyprWpE/<id>
#   2. Generate index.html via scene_generator.py (the code under test)
#   3. Run the headless WebGL harness to evaluate it automatically
#
# Usage: ./run_scene_test.sh <wallpaper_id_or_dir> [preview_image]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG="$ROOT/config"
WALLPAPER_DIR="${WALLPAPER_DIR:-$HOME/.steam/steam/steamapps/workshop/content/431960}"

ID="$1"
if [[ -d "$ID" ]]; then
    SRC_DIR="$ID"
    ID="$(basename "$SRC_DIR")"
else
    SRC_DIR="$WALLPAPER_DIR/$ID"
fi

WORK="/tmp/HyprWpE/$ID"
PREVIEW="${2:-$SRC_DIR/preview.gif}"
OUT="$ROOT/tests/output/$ID"

echo ">>> [1/3] Unpacking $SRC_DIR -> $WORK"
rm -rf "$WORK"
mkdir -p "$WORK"
python "$CONFIG/unpacker.py" "$SRC_DIR"/*.pkg "$WORK" >/dev/null 2>&1 || true
# scenes store project.json outside the pkg; copy it in if needed
[[ -f "$SRC_DIR/project.json" && ! -f "$WORK/project.json" ]] && cp "$SRC_DIR/project.json" "$WORK/"

echo ">>> [2/3] Generating scene index.html"
python "$CONFIG/scene_generator.py" "$WORK"
# place preview inside the served dir so the harness can load it without a 404
[[ -f "$PREVIEW" ]] && cp "$PREVIEW" "$WORK/$(basename "$PREVIEW")"

echo ">>> [3/3] Running headless WebGL evaluation harness"
exec node "$ROOT/tests/harness.js" "$WORK" "$PREVIEW" --out "$OUT"
