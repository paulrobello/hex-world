#!/bin/bash
# tree_oak_c checkpoint (forest refs approved 2026-10-13). Needs tree_oak_c staged
# (make kit-build PIECE=tree_oak_c; the stills bind its staged maps) and
# docs/checkpoints/tree_oak_c/integration.patch applied (git apply), so the kit's oak grove on the
# forest at (-2, 2) carries tree_oak_c at offset (0, 10) and play draws all three scatter oaks;
# restore it afterwards (git restore PRD.md crates). Kit shots from oakc-kit.json (play distance,
# scale beside tree_oak, tree_oak_b, a spruce and the hero oak on (0, 1), sun and shade close-ups
# (and tree_oak from the same shade angle, for the leaf sheen toward the sun),
# a 1.7 m camera under the crown, and a wind pair 60 frames = 1 s apart on the fixed clock), then
# play shots of two oak tiles at world seed 1 from oakc-play.json: the scan in the report puts
# tree_oak_c on (1, 0) beside tree_oak and twice on (2, -1); a walk-mode eye-level shot (walk mode
# draws a black sky on main). Natively at 1600x1000, tilt-shift off. Then the textured EEVEE stills
# (still.py), a wind difference image and the review sheet (sheet.py). Native shots can come back
# black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/tree_oak_c/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/tree_oak_c
OUT=target/shots
SHOTS="oakc-kit-play oakc-kit-scale oakc-kit-sun oakc-kit-shade oakc-kit-shade-tree_oak oakc-kit-eye oakc-kit-wind-a oakc-kit-wind-b oakc-play oakc-play-close oakc-play-eye"
STILLS="tree_oak_c-eevee-three_quarter tree_oak_c-eevee-under oaks-lineup oaks-elevation"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/oakc-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/oakc-kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/oakc-kit-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/oakc-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/oakc-play-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/oakc-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py" "$OUT"
echo "wrote $DOCS/"
