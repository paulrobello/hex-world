#!/bin/bash
# bush_verge_b checkpoint (road_straight refs approved 2026-10-13). Needs the piece staged
# (make kit-build PIECE=bush_verge_b) and docs/checkpoints/bush_verge_b/integration.patch applied
# (git apply), so play draws both verge variants on the road verges and the kit tile carries the
# bush_verge_b placement; restore it afterwards (git checkout -- crates). Play shots natively at
# 1600x1000, tilt-shift off, from vergeb-play.json (bush_verge's layout; the close-up and walk
# cameras aim at bush_verge_b instances at world seed 1, found by printing scatter::layout's
# instances per road tile), kit shots from vergeb-kit.json; then the textured EEVEE
# stills (still.py) and the review sheet (sheet.py). Native shots can come back black (idle display,
# high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/bush_verge_b/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/bush_verge_b
OUT=target/shots
SHOTS="vergeb-play vergeb-sun vergeb-shade vergeb-car vergeb-eye vergeb-kit-scale vergeb-kit-eye"
STILLS="vergeb-lineup vergeb-pair bush_verge_b-eevee-three_quarter bush_verge_b-eevee-low_side bush_verge_b-eevee-top"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/vergeb-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/vergeb-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/vergeb-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/vergeb-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off > target/vergeb-kit-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/vergeb-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
