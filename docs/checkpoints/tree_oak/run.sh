#!/bin/bash
# Scatter-oak checkpoint (P3): tree_oak and tree_oak_b in the kit scenario on their own forest at
# (-2, 2) with a spruce behind them (kit_scene OAK_GROVE), shot with tree_oak.json natively at
# 1600x1000, tilt-shift off: play distance, a wide shot with the forest-floor spruces on (-2, 1) and
# the hero oak on (0, 1) for scale, close-ups of each oak on the sun side (+x/+z, the sun is at
# (200, 400, 150)) and the shade side, eye level at 1.7 m under tree_oak's canopy, and a wind pair
# 40 frames apart on the fixed clock. Then the textured EEVEE stills (still.py), a wind difference image and the review
# sheet (sheet.py). Native shots can come back black (idle display, high load): each PNG's mean is
# checked and the run retried once.
# Usage: docs/checkpoints/tree_oak/run.sh (from the repo root; the stills need the staged pieces in
# target/kit-stage/).
set -euo pipefail
DOCS=docs/checkpoints/tree_oak
OUT=target/shots
SHOTS="oak-play oak-scale-hero oak-sun oak-shade oak-b-sun oak-b-shade oak-eye oak-wind-a oak-wind-b"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/oak-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/tree_oak.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/oak-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/oak-still.log 2>&1
for n in tree_oak-eevee-three_quarter tree_oak-eevee-under tree_oak_b-eevee-three_quarter \
  tree_oak_b-eevee-under tree_oak-eevee-together; do
  ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"
done
python3 "$DOCS/sheet.py" "$OUT"
echo "wrote $DOCS/"
