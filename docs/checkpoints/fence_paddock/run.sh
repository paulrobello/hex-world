#!/bin/bash
# fence_paddock checkpoint (P3 farm): the paddock fence on the farm tile's replica in the kit scenario
# (kit_scene FARM_HEROES on (-1, 5), world centre (41.57, 120)), shot with fence.json natively at
# 1600x1000, tilt-shift off: play distance (about 60 m) with the barn, bales and animals, overhead,
# a 4 m close-up of the sun side (outside the south-east run; the sun is at (200, 400, 150)), a 6 m
# close-up of the shade side (the yard face of the north run and the gate), eye level (1.55 m above
# y = 0, 1.7-1.8 m above the relief) inside the paddock, the scale shot (fence, cow and barn), and a
# far shot at about 170 m (LOD1). Then the textured headless EEVEE stills (still.py) and the review
# sheet (sheet.py). Native shots can come back black (idle display, high load): each PNG's mean is
# checked and the run retried once.
# Usage: bash docs/checkpoints/fence_paddock/run.sh (from the repo root; needs the staged piece in
# target/kit-stage/fence_paddock for the stills).
set -euo pipefail
DOCS=docs/checkpoints/fence_paddock
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="fence-play fence-top fence-close-sun fence-close-shade fence-eye fence-scale fence-far"
mkdir -p "$OUT" target/fence-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/fence-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/fence.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/fence-shots.log 2>&1
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
for v in hero close; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/fence-stills/still-$v.png" "$v" \
    target/kit-stage/fence_paddock/fence_paddock.glb target/kit-stage/fence_paddock/fence_paddock.masks.png \
    assets/kit/manifest.json > "target/fence-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/fence-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
