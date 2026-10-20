#!/bin/bash
# shrub_forest_b checkpoint (forest refs approved 2026-10-13, ref-1: "undergrowth of ferns, shrubs and
# saplings"). Needs the piece staged (make kit-build PIECE=shrub_forest_b) and
# docs/checkpoints/shrub_forest_b/integration.patch applied (git apply), so play draws both shrub
# variants from the pine forest's shrub rule and the kit undergrowth forest at (-3, 2) carries the
# shrub_forest_b placement; restore it afterwards (git restore crates). Play shots natively at
# 1600x1000, tilt-shift off, from shrubb-play.json (five pine forests at world seed 1; the close-up and
# walk cameras aim at the shrub_forest_b instance at (17.72, 1.00) on tile (1, 0), found by printing
# scatter::layout's instances per forest tile), kit shots from shrubb-kit.json; then the textured EEVEE
# stills (still.py) and the review sheet (sheet.py). Native shots can come back black (idle display,
# high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/shrub_forest_b/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/shrub_forest_b
OUT=target/shots
SHOTS="shrubb-play shrubb-play-near shrubb-sun shrubb-shade shrubb-eye shrubb-kit-play shrubb-kit-sun shrubb-kit-shade shrubb-kit-eye"
STILLS="shrubb-lineup shrub_forest_b-eevee-three_quarter shrub_forest_b-eevee-low_side shrub_forest_b-eevee-top shrub_forest_b-eevee-close"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/shrubb-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/shrubb-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/shrubb-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/shrubb-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off > target/shrubb-kit-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/shrubb-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
