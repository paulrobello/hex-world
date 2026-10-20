#!/bin/bash
# house_twostorey_c checkpoint (house_twostorey variant, house_cluster house 2): the variant in the kit
# scenario's second house_cluster on (-1, -1), house 2's slot (yaw 0, its wing front and door facing +x
# toward the yard and the kit sun), between house_longhouse and house_cottage_b, with sedans at park-0
# and park-1 (needs its manifest entry and kit_scene layout placement). Runs twostorey_c.json natively
# at 1600x1000 with tilt-shift off: play distance (about 35 m), a sun-side close-up of the wing front,
# a shade-side close-up of the back and the -z gable with the stack, the same two cameras on the
# approved house_twostorey (same slot and yaw, cluster on (-1, 0)), eye level 1.7 m in the yard and a
# scale shot with the parked cars. Then play.json in --scenario play (the integration patch applied):
# four clusters around the start meadow, where (1, 0) draws house_twostorey_c, (0, -1) and (-1, 1)
# house_twostorey_b and (-1, 0) house_twostorey (an overview and a ~50 m shot of _c, _b and the
# original at one scale), plus walk-mode eye-level shots in the (1, 0) yard
# and a walk into the paved corner beside the wing (state dump: where the box collider stops the
# walker). Then the textured headless EEVEE stills (eevee_still.py) and the review sheet (sheet.py).
# Native shots can come back black (idle display, high load): each PNG's mean is checked and the run
# retried once.
# Usage: bash docs/checkpoints/house_twostorey_c/run.sh (from the repo root, after make kit-build PIECE=house_twostorey_c).
set -euo pipefail
DOCS=docs/checkpoints/house_twostorey_c
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="twostorey_c-play twostorey_c-close-sun twostorey_c-close-shade twostorey-close-sun twostorey-close-shade twostorey_c-eye twostorey_c-scale"
PLAY="twostorey_c-play-overview twostorey_c-play-trio twostorey_c-walk-eye twostorey_c-walk-corner"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/twostorey_c-*.png "$OUT"/twostorey-close-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/twostorey_c.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/twostorey_c-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/twostorey_c-play.log 2>&1
}
run
# shellcheck disable=SC2086
if ! python3 "$DOCS/means.py" "$OUT" $SHOTS $PLAY; then
  echo "retrying once"
  run
  # shellcheck disable=SC2086
  python3 "$DOCS/means.py" "$OUT" $SHOTS $PLAY
fi
for n in $SHOTS $PLAY; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/twostorey_c-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
