#!/bin/bash
# shrub_forest_c checkpoint (forest refs approved 2026-10-13, ref-1: "undergrowth of ferns, shrubs and
# saplings"). Needs the piece staged (make kit-build PIECE=shrub_forest_c) and
# docs/checkpoints/shrub_forest_c/integration.patch applied (git apply), so play draws all three shrub
# variants from the pine forest's shrub rule and the kit undergrowth forest at (-3, 2) carries the
# shrub_forest_c placement; restore it afterwards (git restore crates). Play shots natively at
# 1600x1000, tilt-shift off, fixed clock, from shrubc-play.json (five pine forests at world seed 1; the
# close-up, wind and walk cameras aim at the shrub_forest_c instance at (17.72, 1.00) on tile (1, 0),
# found by dumping scatter::layout per forest cell from a temporary test), kit shots from
# shrubc-kit.json; then the textured EEVEE stills (still.py) and the review sheet (sheet.py). Native
# shots can come back black (idle display, high load): each PNG's mean is checked and the run retried
# once.
# Usage: docs/checkpoints/shrub_forest_c/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/shrub_forest_c
OUT=target/shots
SHOTS="shrubc-play shrubc-play-near shrubc-sun shrubc-wind shrubc-shade shrubc-eye shrubc-kit-play shrubc-kit-scale shrubc-kit-sun shrubc-kit-shade shrubc-kit-eye"
STILLS="shrubc-lineup shrub_forest_c-eevee-three_quarter shrub_forest_c-eevee-low_side shrub_forest_c-eevee-top shrub_forest_c-eevee-close"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/shrubc-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/shrubc-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/shrubc-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/shrubc-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off > target/shrubc-kit-shots.log 2>&1
}
dark() {
  python3 "$DOCS/means.py" "$OUT" $SHOTS
}
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/shrubc-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
