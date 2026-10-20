#!/bin/bash
# The textured headless EEVEE stills of train_carriage_b (still.py) into docs/checkpoints/train_carriage_b.
# Usage: bash docs/checkpoints/train_carriage_b/stills.sh [--stage] (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/train_carriage_b
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
OUT=target/train_carriage_b-stills
mkdir -p "$OUT"
for v in tail side pair straight bend close; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT/still-$v.png" "$v" "$@" \
    > "$OUT/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "$OUT/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
echo "stills written"
