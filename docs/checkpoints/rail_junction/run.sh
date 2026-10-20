#!/bin/bash
# Rail junction checkpoint (P3, refs A17): rail_junction_center on (0, 3) at rot 5 in the kit scenario
# (kit_scene.rs), world centre (41.57, 72): its toe (world edge 5) meets the rail bend on (1, 2) at
# (48.5, 60.0), its straight (world edge 2) the rail straight on (-1, 4) at (34.6, 84.0), and its 8 m
# branch (world edge 0) the rail bend on (1, 3) at (55.4, 72.0). Runs junction.json natively at
# 1600x1000 with tilt-shift off: play distance (about 65 m), the three seams, close-ups about 6 m out
# of the switch and frog on the sun side (south-east) and the shade side (north-west), and eye level
# (1.7 m beside the point lever). Then the textured headless EEVEE stills (still.py: three-quarter,
# the switch blades, the frog, top-down over the switch) and the review sheet (sheet.py).
# Native shots can come back black (idle display, high load): each PNG's mean is checked and the
# run retried once.
# Usage: bash docs/checkpoints/rail_junction/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_junction
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="junction-play junction-seam-toe junction-seam-straight junction-seam-branch junction-close-sun junction-close-shade junction-eye"
mkdir -p "$OUT" target/junction-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/junction-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/junction.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/junction-shots.log 2>&1
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
for v in three switch frog top; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/junction-stills/still-$v.png" "$v" \
    > "target/junction-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/junction-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
