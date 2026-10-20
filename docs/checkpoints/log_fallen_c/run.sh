#!/bin/bash
# log_fallen_c checkpoint (forest refs approved 2026-10-13). Needs log_fallen_c staged
# (make kit-build PIECE=log_fallen_c), log_fallen and log_fallen_b staged for the stills
# (HEXW_SKIP_BAKE=1 make kit-build PIECE=<id>; the stills bind their committed maps), and
# docs/checkpoints/log_fallen_c/integration.patch applied (git apply), so the kit forest at (-3, 1)
# carries log_fallen_c in the spruce's shade and play draws all three logs; restore it afterwards
# (git restore crates). Kit shots from logc-kit.json and a play-distance shot of six pine forest tiles
# from logc-play.json, natively at 1600x1000, tilt-shift off; then the textured EEVEE stills
# (still.py) and the review sheet (sheet.py). Native shots can come back black (idle display, high
# load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/log_fallen_c/run.sh (from the repo root)
set -euo pipefail
DOCS=docs/checkpoints/log_fallen_c
OUT=target/shots
SHOTS="logc-kit-play logc-kit-sun logc-kit-mouth logc-kit-shade logc-kit-eye logc-play logc-play-sun logc-play-shade"
STILLS="logs-lineup logs-top log_fallen_c-eevee-three_quarter log_fallen_c-eevee-mouth log_fallen_c-eevee-low_side"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/logc-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/logc-kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/logc-kit-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/logc-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/logc-play-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/logc-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
