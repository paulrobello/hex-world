#!/bin/bash
# sapling_spruce_c checkpoint (forest refs approved 2026-10-13). Needs the piece staged (make kit-build
# PIECE=sapling_spruce_c) and its neighbours staged without touching their committed reports
# (HEXW_SKIP_BAKE=1 "$BLENDER" -b --factory-startup --python-exit-code 1 -P scripts/blender/kit/<id>.py
# -- target/kit-stage/<id> for sapling_spruce, sapling_spruce_b and tree_forest_c), and
# docs/checkpoints/sapling_spruce_c/integration.patch applied (git apply), so play draws all three sapling
# variants and the kit undergrowth forest at (-3, 2) carries sapling_spruce_c at [5, -4]; restore the
# crates afterwards (git restore, one file per call). Play shots natively at 1600x1000, tilt-shift off,
# from saplingc-play.json (the forest layout at world seed 1 on tiles (1, 0), (1, -1), (2, -1), (2, 0),
# (1, 1) and (3, -1); the close-up and walk cameras aim at the sapling_spruce_c instance at
# (79.25, -27.52) on (3, -1), the one nearest open ground among the cells a play script can place next
# to the cluster, found by dumping scatter::layout per cell seed from a temporary test: the instances
# inside the cluster all stand under spruce crowns, Q46);
# kit shots from saplingc-kit.json under --clock fixed (the wind pair is 60 frames, 1 s, apart); then
# the textured EEVEE stills (still.py) and the review sheet (sheet.py). Native shots can come back black
# (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/sapling_spruce_c/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/sapling_spruce_c
OUT=target/shots
SHOTS="saplingc-play saplingc-sun saplingc-eye saplingc-kit-play saplingc-kit-sun saplingc-kit-wind saplingc-kit-shade saplingc-kit-eye"
STILLS="saplingc-lineup sapling_spruce_c-eevee-three_quarter sapling_spruce_c-eevee-low_side sapling_spruce_c-eevee-top"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/saplingc-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/saplingc-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/saplingc-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/saplingc-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/saplingc-kit-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/saplingc-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
