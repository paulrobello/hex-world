#!/bin/bash
# Farm animal variants checkpoint (PRD §6 Variety): animal_cow_b (grazing) and animal_sheep_b (lamb) on
# the variants' farm in the kit scenario (kit_scene FARM_VARIANTS on (0, 5), world centre (69.28, 120):
# the farm's barn, fence and ewes, animal_cow beside the grazing cow, the lamb beside a ewe), shot with
# animal_variants.json natively at 1600x1000, tilt-shift off: play distance, the grazing cow close up on
# the sun side (the sun is at (200, 400, 150)) and the shade side, the lamb on both sides, eye level at
# 1.7 m above y = 0, and both in front of the barn for scale. Then the textured EEVEE stills beside the
# originals (still.py) and the review sheet (sheet.py). Shots are not timing measurements: they run
# under caffeinate without a load wait, each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/animal_variants/run.sh (from the repo root). The stills need every piece
# staged in target/kit-stage/: make kit-build PIECE=<id> for the variants, and
# HEXW_SKIP_BAKE=1 make kit-build PIECE=animal_cow / animal_sheep (their committed maps are bound; then
# git checkout -- assets-source/kit/animal_cow assets-source/kit/animal_sheep).
set -euo pipefail
DOCS=docs/checkpoints/animal_variants
OUT=target/shots
STILL_OUT=target/animalvar-stills
SHOTS="var-play var-cow-sun var-cow-shade var-lamb-sun var-lamb-shade var-eye var-scale-barn"
STILLS="cows-lineup sheep-lineup animal_cow_b-eevee-front_three_quarter animal_cow_b-eevee-head animal_cow_b-eevee-lod2 animal_sheep_b-eevee-front_three_quarter animal_sheep_b-eevee-head animal_sheep_b-eevee-lod2"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT" "$STILL_OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/var-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/animal_variants.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/animal-variants-shots.log 2>&1
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
