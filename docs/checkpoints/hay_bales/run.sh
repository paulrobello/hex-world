#!/bin/bash
# Hay bales checkpoint (P3): hay_bale_round and hay_bale_stack in the kit scenario on the oak's meadow
# (kit_scene HAY_BALES), shot with hay_bales.json natively at 1600x1000, tilt-shift off: play
# distance, close-ups of each piece on the sun side (+x/+z, the sun is at (200, 400, 150)) and the
# shade side, eye level at 1.7 m, and beside the hero oak for scale. Then the textured EEVEE stills
# (still.py) and the review sheet (sheet.py). Native shots can come back black (idle display, high
# load): each PNG's mean is checked and the run retried once. Usage: docs/checkpoints/hay_bales/run.sh
# (from the repo root; needs the staged pieces in target/kit-stage/ for the stills).
set -euo pipefail
DOCS=docs/checkpoints/hay_bales
OUT=target/shots
SHOTS="hay-play hay-round-sun hay-round-shade hay-stack-sun hay-stack-shade hay-eye hay-scale-oak"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/hay-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/hay_bales.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/hay-shots.log 2>&1
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
for p in hay_bale_round hay_bale_stack; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$p" "$OUT" > target/hay-still.log 2>&1
  for v in three_quarter low_side; do ffmpeg -loglevel error -y -i "$OUT/$p-eevee-$v.png" -q:v 2 "$DOCS/$p-eevee-$v.jpg"; done
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
