#!/bin/bash
# Station checkpoint (P3, refs A17): rail_station_platform and rail_station_shelter beside rail_straight_center at (0, 2) in the kit
# scenario (kit_scene.rs), with the train consist on the bend at (1, 2). Runs station.json natively at
# 1600x1000 with tilt-shift off: play distance, close-ups about 6 m out on the sun side (south, the
# track side) and the shade side (north, behind the shelter), eye level 1.7 m on the platform looking
# along the track, and a scale shot with the carriages, the barn and the bakery. Then the textured
# EEVEE still (still.py) and the review sheet (sheet.py). Native shots can come back black (idle
# display): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/rail_station/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_station
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="station-play station-close-sun station-close-shade station-eye station-scale"
mkdir -p "$OUT" target/station-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/station-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/station.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/station-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- target/station-stills/still.png \
  > target/station-stills/still.log 2>&1
ffmpeg -loglevel error -y -i target/station-stills/still.png -q:v 2 "$DOCS/still.jpg"
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
