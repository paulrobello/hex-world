#!/bin/bash
# Farm animals checkpoint (P3): animal_cow and animal_sheep in the kit scenario on the oak's meadow
# (kit_scene FARM_ANIMALS, in open sun on the oak's +x/+z side), shot with farm_animals.json natively
# at 1600x1000, tilt-shift off: play distance, close-ups of each piece on the sun side (the sun is at
# (200, 400, 150)) and the shade side, eye level at 1.7 m, and beside the hero oak and hay bales for
# scale. Then the textured EEVEE stills (still.py) and the review sheets (sheet.py). Native shots can
# come back black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/farm_animals/run.sh (from the repo root; needs the staged pieces in
# target/kit-stage/ for the stills, and the manifest entries plus the layout placement).
set -euo pipefail
DOCS=docs/checkpoints/farm_animals
OUT=target/shots
SHOTS="farm-play farm-cow-sun farm-cow-shade farm-sheep-sun farm-sheep-shade farm-eye farm-scale-oak"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/farm-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/farm_animals.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/farm-shots.log 2>&1
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
for p in animal_cow animal_sheep; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$p" "$OUT" > target/farm-still.log 2>&1
  for v in left front_three_quarter back_three_quarter; do
    ffmpeg -loglevel error -y -i "$OUT/$p-eevee-$v.png" -q:v 2 "$DOCS/$p-eevee-$v.jpg"
  done
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
