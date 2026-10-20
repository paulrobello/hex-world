#!/bin/bash
# house_twostorey_b checkpoint (house_twostorey variant): the variant in the kit scenario's third
# house_cluster on (1, -2) at rot 1, in house 2's slot (front turned to world yaw 60, toward the kit
# sun), between two cottages, with a sedan parked at park-2 (needs its manifest entry and kit_scene
# layout placement). Runs twostorey_b.json natively at 1600x1000 with tilt-shift off: play distance
# (about 35 m), a sun-side close-up of the jettied front, a shade-side close-up of the back and the
# left hip end, the same two cameras on the approved house_twostorey (yaw 0, cluster on (-1, 0)) for
# comparison, eye level (a 1.7 m camera on the drive; walk mode needs play, where no tile draws the
# variant until its HERO_VARIANTS row lands) and a scale shot with the parked car. Then yaw-check.json
# with --shadows off (the yawed variant's sun-side front beside the yaw-0 original's), the textured
# headless EEVEE stills (eevee_still.py) and the review sheet (sheet.py). Native shots can come back
# black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/house_twostorey_b/run.sh (from the repo root, after make kit-build PIECE=house_twostorey_b).
set -euo pipefail
DOCS=docs/checkpoints/house_twostorey_b
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="twostorey_b-play twostorey_b-close-sun twostorey_b-close-shade twostorey-close-sun twostorey-close-shade twostorey_b-eye twostorey_b-scale"
YAW="yaw-twostorey_b-sun yaw-twostorey-sun"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/twostorey*.png "$OUT"/yaw-twostorey*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/twostorey_b.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/twostorey_b-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/yaw-check.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --shadows off > target/twostorey_b-yaw.log 2>&1
}
run
# shellcheck disable=SC2086
if ! python3 "$DOCS/means.py" "$OUT" $SHOTS $YAW; then
  echo "retrying once"
  run
  # shellcheck disable=SC2086
  python3 "$DOCS/means.py" "$OUT" $SHOTS $YAW
fi
for n in $SHOTS $YAW; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/twostorey_b-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
