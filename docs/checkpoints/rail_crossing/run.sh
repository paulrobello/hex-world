#!/bin/bash
# Rail-road level crossing checkpoint (P3, refs A17 ref-3): rail_road_crossing_center on (-1, 2) in
# the kit scenario (kit_scene.rs), world centre (0, 48), its rail lane on edge 0 meeting
# rail_straight_center on (0, 2) and road_straight_edge stubs on edges 1 and 4. Runs crossing.json
# natively at 1600x1000 with tilt-shift off: play distance (about 65 m), the seam with the rail
# straight (world x = 13.86), the seam with the edge-1 road stub (the r = 6 joint at (3, 53.2)), close-ups
# about 6 m out of the deck and a gate on the sun side (south-east) and shade side (north-west), and
# eye level (1.7 m on the edge-4 road approach). Then the textured headless EEVEE stills (still.py:
# three-quarter, deck close-up, a sedan on the deck for scale) and the review sheet (sheet.py).
# Native shots can come back black (idle display, high load): each PNG's mean is checked and the
# run retried once.
# Usage: bash docs/checkpoints/rail_crossing/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_crossing
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="crossing-play crossing-seam-rail crossing-seam-road crossing-close-sun crossing-close-shade crossing-eye"
mkdir -p "$OUT" target/crossing-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/crossing-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/crossing.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/crossing-shots.log 2>&1
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
for v in three deck scale; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/crossing-stills/still-$v.png" "$v" \
    > "target/crossing-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/crossing-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
