#!/bin/bash
# Forest-floor checkpoint (P3): fern_clump and log_fallen in the kit scenario on the forest at (-2, 1)
# (kit_scene FOREST_FLOOR), shot with forest_floor.json natively at 1600x1000, tilt-shift off: play
# distance, the log and a fern close up on the sun side (+x/+z, the sun is at (200, 400, 150)) and the
# shade side, eye level at 1.7 m in the forest, and the log beside a spruce and a parked sedan for
# scale. Then the textured EEVEE stills (still.py) and the review sheet (sheet.py). Native shots can
# come back black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/forest_floor/run.sh (from the repo root; the stills need the staged pieces
# in target/kit-stage/).
set -euo pipefail
DOCS=docs/checkpoints/forest_floor
OUT=target/shots
SHOTS="floor-play floor-log-sun floor-log-shade floor-fern-sun floor-fern-shade floor-fern-trunk floor-eye floor-scale"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/floor-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/forest_floor.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/floor-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/floor-still.log 2>&1
for n in fern_clump-eevee-three_quarter fern_clump-eevee-low_side log_fallen-eevee-three_quarter \
  log_fallen-eevee-low_side forest_floor-eevee-together; do
  ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
