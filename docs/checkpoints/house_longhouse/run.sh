#!/bin/bash
# house_longhouse checkpoint (house_cluster variant): the longhouse in the kit scenario's second
# house_cluster on (-1, -1), in house 0's slot (yaw 90, front to the yard), beside house_cottage and
# house_twostorey, with a sedan parked at park-0 (needs its manifest entry and kit_scene layout
# placement). Runs longhouse.json natively at 1600x1000 with tilt-shift off: play distance (about
# 40 m), a sun-side close-up of the yard front 6 m out, a shade-side close-up of the back and the hall
# gable, eye level (a 1.7 m camera on the driveway; walk mode needs play, where no tile draws the
# variant yet), and a scale shot with the parked car. Then the textured headless EEVEE stills
# (eevee_still.py) and the review sheet (sheet.py). Native shots can come back black (idle display,
# high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/house_longhouse/run.sh (from the repo root, after make kit-build PIECE=house_longhouse).
set -euo pipefail
DOCS=docs/checkpoints/house_longhouse
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="longhouse-play longhouse-close-sun longhouse-close-shade longhouse-cottage-shade longhouse-eye longhouse-scale"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/longhouse-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/longhouse.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/longhouse-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/longhouse-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
