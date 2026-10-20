#!/bin/bash
# Boats checkpoint (P3, refs D4): boat_rowing, boat_sail and landing_stage. Needs integration.patch
# applied (manifest entries, the BOATS layout in kit_scene, part drawing and the part gate) and the
# staged builds (make kit-build / kit-compress PIECE=<id> for each).
# 1. water_kit (water.json): the sailing boat on ocean tile (2, 0) and a rowing boat on (1, 1), on the
#    look-dev water: play distance, sun and shade close-ups of each, and a rowing-boat view near the water.
# 2. kit (kit.json): the landing stage on the dry river straight (-2, 6) with a rowing boat on the far
#    half of the channel (the kit scenario spawns no water, so the posts show against the bed): play
#    distance, the channel view, a sun-side close-up and eye level (1.7 m) on the deck.
# Then the EEVEE stills (still.py, stills.json: masks bound, on a water plane and the river profile) and
# the three review sheets (sheet.py). Native at 1600x1000, tilt-shift off. Native shots can come back
# black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/boats/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/boats
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
WATER_SHOTS="boats-water-play boat_sail-water-close-sun boat_sail-water-close-shade boat_rowing-water-close-sun boat_rowing-water-close-shade boat_rowing-water-eye"
KIT_SHOTS="boats-kit-play landing_stage-kit-channel landing_stage-kit-close-sun landing_stage-kit-eye"
mkdir -p "$OUT"
run() {
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/water.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/boats-water.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/boats-kit.log 2>&1
}
uptime
run
if ! python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $WATER_SHOTS $KIT_SHOTS; then
  echo "retrying once"
  run
  python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $WATER_SHOTS $KIT_SHOTS
fi
uptime
for n in $WATER_SHOTS $KIT_SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$DOCS/stills.json" > target/boats-stills.log 2>&1
for f in target/shots/boats-eevee/*.png; do ffmpeg -loglevel error -y -i "$f" -q:v 2 "$DOCS/eevee-$(basename "${f%.png}").jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
