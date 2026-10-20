#!/bin/bash
# bush_verge checkpoint (road_straight refs approved 2026-10-13). Needs the piece staged
# (make kit-build PIECE=bush_verge) and docs/checkpoints/bush_verge/integration.patch applied
# (git apply), so play scatters the bushes on the road verges; restore it afterwards
# (git checkout -- crates data web). Play shots natively at 1600x1000, tilt-shift off, from
# verge-play.json; then the textured EEVEE stills (still.py) and the review sheet (sheet.py).
# Native shots can come back black (idle display, high load): each PNG's mean is checked and the
# run retried once.
# Usage: docs/checkpoints/bush_verge/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/bush_verge
OUT=target/shots
SHOTS="verge-play verge-sun verge-shade verge-car verge-eye verge-kit-scale verge-kit-eye"
STILLS="verge-lineup bush_verge-eevee-three_quarter bush_verge-eevee-low_side"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/verge-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/verge-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/verge-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/verge-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off > target/verge-kit-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/verge-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
