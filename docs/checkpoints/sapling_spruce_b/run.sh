#!/bin/bash
# sapling_spruce_b checkpoint (forest refs approved 2026-10-13). Needs the piece and its neighbours
# staged (make kit-build PIECE=sapling_spruce_b; HEXW_SKIP_BAKE=1 make kit-build PIECE=<id> for
# sapling_spruce and tree_forest_c, then git restore their rewritten bake-report.json) and
# docs/checkpoints/sapling_spruce_b/integration.patch applied (git apply), so play draws both sapling
# variants and the kit undergrowth forest at (-3, 2) carries sapling_spruce_b at [0, 2]; restore it
# afterwards (git restore, one file per call). Play shots natively at 1600x1000, tilt-shift off, from
# saplingb-play.json (sapling_spruce's forest layout at world seed 1; the close-up and walk cameras aim
# at sapling_spruce_b instances (63.91, 5.02) (sun side) and (52.67, -0.02) (shade side) on tile (2, 0)
# and (38.39, -4.81) on tile (1, 0) (eye level), found by printing scatter::layout's instances and probing four camera
# azimuths around each for a clear view between the 16 m spruces), kit shots from saplingb-kit.json; then the textured EEVEE
# stills (still.py) and the review sheet (sheet.py). Native shots can come back black (idle display,
# high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/sapling_spruce_b/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/sapling_spruce_b
OUT=target/shots
SHOTS="saplingb-play saplingb-sun saplingb-shade saplingb-eye saplingb-kit-play saplingb-kit-sun saplingb-kit-shade saplingb-kit-eye"
STILLS="saplingb-lineup sapling_spruce_b-eevee-three_quarter sapling_spruce_b-eevee-low_side sapling_spruce_b-eevee-top"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/saplingb-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/saplingb-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/saplingb-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/saplingb-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off > target/saplingb-kit-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/saplingb-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
