#!/bin/bash
# wildflower_single_e checkpoint (refs: assets-source/refs/meadow_props, the single wildflowers' refs; the
# reference shows no poppy). Needs docs/checkpoints/wildflower_single_e/integration.patch applied and, for
# the weighted 1-in-10 mix, scatter::VARIANT_WEIGHTS (landing branch land-b7, 1f6102d) plus weights.patch;
# restore crates afterwards. Play shots natively at 1600x1000, tilt-shift off, fixed clock, from
# wfe-play.json: seven meadows around the start meadow at world seed 1; the close-up, wind, detail and
# walk cameras aim at the poppy at (-5.11, 3.69) on the start meadow (0, 0), found by dumping
# scatter::layout per cell seed from a temporary test. wfe-uniform.json takes the same play-distance view
# with integration.patch alone (uniform picks, 1 in 5), wfe-kit.json the kit meadow (-1, 1) placement.
# Then the textured EEVEE stills (still.py) and the review sheet (sheet.py). Native shots can come back
# black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/wildflower_single_e/run.sh [uniform] (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/wildflower_single_e
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
if [ "${1:-}" = uniform ]; then
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/wfe-uniform.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/wfe-uniform-shots.log 2>&1
  python3 "$DOCS/means.py" "$OUT" wfe-uni-play && exit 1
  ffmpeg -loglevel error -y -i "$OUT/wfe-uni-play.png" -q:v 2 "$DOCS/wfe-uni-play.jpg"
  exit 0
fi
SHOTS="wfe-play wfe-orbit wfe-sun wfe-wind wfe-shade wfe-detail wfe-eye wfe-kit-close"
STILLS="wfe-lineup wildflower_single_e-eevee-three_quarter wildflower_single_e-eevee-low_side wildflower_single_e-eevee-top wildflower_single_e-eevee-close"
run() {
  rm -f "$OUT"/wfe-play.png "$OUT"/wfe-orbit.png "$OUT"/wfe-sun.png "$OUT"/wfe-wind.png "$OUT"/wfe-shade.png \
    "$OUT"/wfe-detail.png "$OUT"/wfe-eye.png "$OUT"/wfe-kit-close.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/wfe-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/wfe-play-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/wfe-kit.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off > target/wfe-kit-shots.log 2>&1
}
run
if python3 "$DOCS/means.py" "$OUT" $SHOTS; then echo "retrying once"; run; python3 "$DOCS/means.py" "$OUT" $SHOTS && exit 1; fi
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/wfe-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
