#!/bin/bash
# bush_verge_c checkpoint (road_straight refs approved 2026-10-13). Needs the piece staged
# (make kit-build PIECE=bush_verge_c) and docs/checkpoints/bush_verge_c/integration.patch applied
# (git apply), so play draws all three verge variants on the road verges and the kit tile carries the
# bush_verge_c placement; restore it afterwards (git checkout -- crates assets/kit/manifest.json).
# Play shots natively at 1600x1000, tilt-shift off, clock fixed, from vergec-play.json (bush_verge_b's
# layout; the close-up and walk cameras aim at the bush_verge_c instance at (11.45, 29.31) on tile
# (0, 1), world seed 1, found by printing scatter::layout's instances per road tile), kit shots from
# vergec-kit.json (scale, eye level, sun and shade close-ups and a wind pair 60 fixed frames = 1 s
# apart); then the textured EEVEE stills (still.py) and the review sheet (sheet.py). Native shots can
# come back black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/bush_verge_c/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/bush_verge_c
OUT=target/shots
SHOTS="vergec-play vergec-mid vergec-sun vergec-shade vergec-car vergec-eye vergec-kit-scale vergec-kit-eye vergec-kit-sun vergec-kit-shade vergec-wind-a vergec-wind-b"
STILLS="vergec-lineup vergec-trio vergec-trio-far bush_verge_c-eevee-three_quarter bush_verge_c-eevee-low_side bush_verge_c-eevee-top"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/vergec-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/vergec-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/vergec-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/vergec-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/vergec-kit-shots.log 2>&1
}
dark() {
  for n in $SHOTS; do
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    echo "$n.png mean $m"
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/vergec-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
