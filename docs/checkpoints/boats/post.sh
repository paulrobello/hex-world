#!/bin/bash
# The half of run.sh after the native shots: JPEGs of the engine shots, the EEVEE stills and the
# three review sheets. Use it when the native runs were taken by hand (2026-10-20: both native runs
# came back all black from inside `bash run.sh` at host load ~350, and rendered when run directly).
# Usage: bash docs/checkpoints/boats/post.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/boats
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="boats-water-play boat_sail-water-close-sun boat_sail-water-close-shade boat_rowing-water-close-sun boat_rowing-water-close-shade boat_rowing-water-eye boats-kit-play landing_stage-kit-channel landing_stage-kit-close-sun landing_stage-kit-eye"
python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $SHOTS
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$DOCS/stills.json" > target/boats-stills.log 2>&1
for f in target/shots/boats-eevee/*.png; do ffmpeg -loglevel error -y -i "$f" -q:v 2 "$DOCS/eevee-$(basename "${f%.png}").jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
