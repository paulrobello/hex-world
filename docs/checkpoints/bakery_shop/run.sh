#!/bin/bash
# bakery_shop checkpoint (P3 village): the bakery on road tile (1, 0) in the kit scenario, north of the
# road and facing it (needs its manifest entry and kit_scene layout placement). Runs bakery.json
# natively at 1600x1000 with tilt-shift off: play distance (about 45 m), a sun-side close-up of the shop
# front 6 m out, a shade-side close-up of the back and left gable, eye level (1.7 m above the road,
# 11 m out; walk mode needs a placeable tile, which the bakery does not have yet), and a scale shot
# with the house_cluster cottages beyond. Then the textured headless EEVEE stills (eevee_still.py)
# and the review sheet (sheet.py). Native shots can come back black (idle display, high load): each
# PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/bakery_shop/run.sh (from the repo root, after make kit-build PIECE=bakery_shop).
set -euo pipefail
DOCS=docs/checkpoints/bakery_shop
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="bakery-play bakery-close-sun bakery-close-shade bakery-eye bakery-scale"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/bakery-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/bakery.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/bakery-shots.log 2>&1
}
dark() {
  for n in $SHOTS; do
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/bakery-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
