#!/bin/bash
# house_cottage_b checkpoint (house_cluster cottage-slot variant): house_cottage_b in the kit scenario's
# second house_cluster on (-1, -1), in house 1's slot (yaw 270, front to the yard), across the yard from
# house_longhouse, with sedans parked at park-0 and park-1 (needs its manifest entry and kit_scene
# layout placement). Runs cottage_b.json natively at 1600x1000 with tilt-shift off: play distance
# (about 35 m), a sun-side close-up of the back and the +x gable, a shade-side close-up of the yard
# front and porch, the same camera on the approved house_cottage in the same slot and yaw on (-1, 0),
# eye level (a 1.7 m camera on the driveway; walk mode needs play, where the tile draws the variant only
# at cells such as (1, 0)), and a scale shot with the parked cars. Then the textured headless EEVEE
# stills (eevee_still.py) and the review sheet (sheet.py). Native shots can come back black (idle
# display, high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/house_cottage_b/run.sh (from the repo root, after make kit-build PIECE=house_cottage_b).
set -euo pipefail
DOCS=docs/checkpoints/house_cottage_b
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="cottage_b-play cottage_b-close-sun cottage_b-close-shade cottage_b-cottage-shade cottage_b-eye cottage_b-scale"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/cottage_b-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/cottage_b.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/cottage_b-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/cottage_b-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
