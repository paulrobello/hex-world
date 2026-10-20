#!/bin/bash
# Train checkpoint (P3, refs A17): train_engine plus two train_carriage on rail_bend_60_center at
# (1, 2) in the kit scenario (kit_scene.rs TRAIN_ON_BEND, poses from consist.py), running out across
# the joint toward the straight at (0, 2). Runs train.json natively at 1600x1000 with tilt-shift off:
# play distance, the consist on the bend, close-ups about 5-7 m out on the sun side (south) and the
# shade side (north), eye level (1.7 m beside the track) and a scale shot toward the barn. The kit
# scenario draws kit pieces with KitMaterial, which has no paint input, so in engine the body shows
# its unpainted white; the runtime colours are the EEVEE stills (car_material.wesl's formula).
# Then the textured headless EEVEE stills (still.py) and the review sheet (sheet.py). Native shots can
# come back black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/train/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/train
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="train-play train-bend train-close-sun train-close-shade train-eye train-scale"
mkdir -p "$OUT" target/train-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/train-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/train.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/train-shots.log 2>&1
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
for v in engine carriage straight bend close scale paint-a paint-b; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/train-stills/still-$v.png" "$v" \
    > "target/train-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/train-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
