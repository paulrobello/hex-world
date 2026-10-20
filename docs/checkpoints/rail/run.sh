#!/bin/bash
# Rail checkpoint (P3, refs A17): rail_straight_center on (0, 2) and rail_bend_60_center at rot 2 on
# (1, 2) in the kit scenario (kit_scene.rs), meeting at world (41.57, 0, 48). Runs rail.json natively
# at 1600x1000 with tilt-shift off: play distance (about 65 m), the straight/bend seam, close-ups
# about 5.5 m out on the sun side (south) and shade side (north), eye level (1.6 m, beside the
# track), and a scale shot with the barn and the farm animals on the oak's meadow beyond. Then the
# textured headless EEVEE stills (still.py) and the review sheet (sheet.py). Native shots can come
# back black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/rail/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="rail-play rail-seam rail-close-sun rail-close-shade rail-eye rail-scale"
mkdir -p "$OUT" target/rail-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/rail-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/rail.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/rail-shots.log 2>&1
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
for v in straight bend close scale; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/rail-stills/still-$v.png" "$v" \
    > "target/rail-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/rail-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
