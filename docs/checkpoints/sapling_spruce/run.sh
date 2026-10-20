#!/bin/bash
# Forest undergrowth checkpoint (sapling_spruce and shrub_forest, forest refs approved 2026-10-13):
# the kit scenario's undergrowth forest at (-3, 2) (kit_scene UNDERGROWTH), shot with undergrowth.json
# natively at 1600x1000, tilt-shift off: play distance, each piece close up on the sun side (+x/+z,
# the sun is at (200, 400, 150)) and the shade side, eye level at 1.7 m, and a scale view beside
# tree_forest, tree_forest_c and fern_clump. Then the textured EEVEE stills (still.py) and one review
# sheet per piece (sheet.py, docs/checkpoints/<piece>/sheet.jpg). Native shots can come back black
# (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/sapling_spruce/run.sh (from the repo root; the stills need every piece
# staged in target/kit-stage/: make kit-build PIECE=<id> for both pieces, and
# HEXW_SKIP_BAKE=1 make kit-build PIECE=<id> for tree_forest and fern_clump, whose committed maps
# the stills bind; git restore the two bake-report.json files that staging rewrites).
set -euo pipefail
DOCS=docs/checkpoints/sapling_spruce
OUT=target/shots
SHOTS="ug-play ug-shrub-sun ug-shrub-shade ug-sapling-sun ug-sapling-shade ug-eye ug-scale"
STILLS="undergrowth-lineup sapling_spruce-eevee-three_quarter sapling_spruce-eevee-low_side shrub_forest-eevee-three_quarter shrub_forest-eevee-low_side"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/ug-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/undergrowth.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/undergrowth-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/undergrowth-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/ and docs/checkpoints/shrub_forest/sheet.jpg"
