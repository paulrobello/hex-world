#!/bin/bash
# rail_buffer_stop checkpoint (owner D2, ref-2). Needs integration.patch applied (the manifest entry and
# kit_scene BUFFER_STOPS: a stop with the engine buffered up to it at the open local edge 0 of the rail
# straight on (-3, 6) and of the rail bend on (1, 3)). Poses: poses.py; cameras: cameras.py writes
# shots.json and night.json. Native kit scenario at 1600x1000, tilt-shift off. Shots are not timing
# measurements: they run under caffeinate without a load wait, each PNG's mean is checked
# (docs/checkpoints/bakery_shop_b/means.py) and the run retried once. Then the review sheet (sheet.py).
# Usage: bash docs/checkpoints/rail_buffer_stop/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_buffer_stop
OUT=target/shots
SHOTS="bufstop-straight-play bufstop-straight-sun bufstop-straight-rear bufstop-straight-top bufstop-eye bufstop-bend-play bufstop-bend-sun bufstop-bend-rear bufstop-bend-top"
mkdir -p "$OUT"
cargo build -p hex-app --release
python3 "$DOCS/cameras.py"
run() {
  rm -f "$OUT"/bufstop-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/shots.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/bufstop-shots.log 2>&1
  caffeinate -u -d -t 300 target/release/hex-world --script "$DOCS/night.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --kit_hooks night > target/bufstop-night.log 2>&1
}
uptime
run
if ! python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $SHOTS bufstop-night; then
  echo "retrying once"
  run
  python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $SHOTS bufstop-night
fi
uptime
for n in $SHOTS bufstop-night; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 3 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
