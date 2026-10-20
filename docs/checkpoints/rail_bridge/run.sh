#!/bin/bash
# River rail bridge checkpoint (P3): river_rail_bridge_center on (-2, 5) rot 1 in the kit scenario
# (kit_scene.rs), world centre (13.86, 120), rail on world edges 5 and 2 meeting rail_straight_center
# on (-1, 4) (the rail junction's straight run) and (-3, 6), river on world edges 4 and 1 meeting
# river_straight on (-2, 4) and (-2, 6), the engine and a carriage on the deck. Runs bridge.json natively at 1600x1000 with tilt-shift off:
# play distance (about 70 m), both rail seams (side-on, 12 m), the upstream and downstream river
# seams, sun-side and shade-side close-ups of the arch from the channel (8 m), eye level (1.7 m
# beside the track on the north-east approach), and the train on the deck for scale. The kit
# scenario spawns no water (the channel reads dry); the EEVEE stills put a plane at the -1.4 m water
# level. Then the textured headless EEVEE stills (still.py: three-quarter, arch, train) and the
# review sheet (sheet.py). Native shots can come back black (idle display, high load): each PNG's
# mean is checked and the run retried once.
# Usage: bash docs/checkpoints/rail_bridge/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_bridge
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="bridge-play bridge-seam-rail-ne bridge-seam-rail-sw bridge-seam-river-up bridge-seam-river-down bridge-close-sun bridge-close-shade bridge-eye bridge-scale"
mkdir -p "$OUT" target/bridge-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/bridge-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/bridge.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/bridge-shots.log 2>&1
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
for v in three arch scale; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/bridge-stills/still-$v.png" "$v" \
    > "target/bridge-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/bridge-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
