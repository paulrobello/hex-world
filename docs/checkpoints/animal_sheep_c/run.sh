#!/bin/bash
# Grazing sheep checkpoint (PRD §6 Variety): animal_sheep_c on the farm-variants farm in the kit scenario
# (kit_scene FARM_VARIANTS on (0, 5), world centre (69.28, 120); the sheep at local (-2.25, 9.625), world
# (67.03, 129.6), yaw 210, beside the lamb, the ewes and both cows), shot with animal_sheep_c.json
# natively at 1600x1000, tilt-shift off: play distance, close-ups on the sun side (the sun is at
# (200, 400, 150)) and the shade side, the muzzle at the grass, eye level at 1.7 m above y = 0, and the
# flock before the barn for scale. Then the textured EEVEE stills beside animal_sheep and animal_cow_b
# (still.py) and the review sheet (sheet.py). Shots are not timing measurements: they run under
# caffeinate without a load wait, each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/animal_sheep_c/run.sh (from the repo root). The stills need every piece
# staged in target/kit-stage/: make kit-build PIECE=animal_sheep_c, and HEXW_SKIP_BAKE=1 make kit-build
# PIECE=animal_sheep / animal_cow_b (their committed maps are bound; then
# git checkout -- assets-source/kit/animal_sheep assets-source/kit/animal_cow_b).
set -euo pipefail
DOCS=docs/checkpoints/animal_sheep_c
OUT=target/shots
STILL_OUT=target/sheepc-stills
SHOTS="sheepc-play sheepc-sun sheepc-shade sheepc-head sheepc-eye sheepc-scale-barn"
STILLS="lineup animal_sheep_c-eevee-front_three_quarter animal_sheep_c-eevee-head animal_sheep_c-eevee-top animal_sheep_c-eevee-lod2"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT" "$STILL_OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/sheepc-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/animal_sheep_c.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/sheepc-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$STILL_OUT" > "$STILL_OUT/still.log" 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$STILL_OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
