#!/bin/bash
# barn_red checkpoint (P3 farm): the barn on the oak's meadow in the kit scenario (needs its manifest
# entry and kit_scene layout placement). Runs barn.json natively at 1600x1000 with tilt-shift off:
# play distance (about 60 m), close-ups 6-9 m from the sun-side door gable and the shade-side back
# gable, eye level (1.7 m above the meadow, 12 m out), and a scale shot with the oak beyond. Then the
# textured headless EEVEE stills (still.py: hero, back, and the barn beside house_cottage and
# house_twostorey) and the review sheet (sheet.py). Native shots can come back black (idle display,
# high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/barn_red/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/barn_red
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="barn-play barn-close-sun barn-close-shade barn-eye barn-scale"
mkdir -p "$OUT" target/barn-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/barn-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/barn.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/barn-shots.log 2>&1
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
for v in hero back scale; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/barn-stills/still-$v.png" "$v" \
    assets/kit/p1/barn_red/barn_red.glb assets/kit/p1/barn_red/barn_red.masks.png assets/kit/manifest.json \
    > "target/barn-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/barn-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
