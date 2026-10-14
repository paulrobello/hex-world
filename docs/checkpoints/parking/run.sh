#!/bin/bash
# House parking checkpoint (PRD §7.1): parked cars, one car pulling out (reversing out of the yard,
# turning, onto the lane) and one parking (nosing into a free spot at the other cluster).
# Runs parking.json natively at 1600x1000 with tilt-shift off and `--clock fixed` (one 20 Hz sim
# tick every third frame), so every run replays the same trips on the same frames. Layout: house
# clusters at (1,0) and (3,0) joined by a road_straight at (2,0); every house tile is in the village
# pack, and each shot's state dump records `packs.village == "ready"` (checked below), so no shot
# shows placeholders. The script dumps state every 15 frames (`parking-sample-*`), which also keeps
# the frame cadence of the run the shots were chosen from. Fails on an all-black frame.
# Usage: caffeinate -d -u docs/checkpoints/parking/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/parking
OUT=target/shots
SHOTS="parking-01-parked-overview parking-02-parked-a parking-03-parked-b parking-04-pullout-1
parking-05-pullout-2 parking-06-pullout-3 parking-07-parking-1 parking-08-parking-2
parking-09-parked-after"
mkdir -p "$OUT"
rm -f "$OUT"/parking-*.png "$OUT"/parking-*.json
cargo build -p hex-app --release
target/release/hex-world --scenario play --script "$DOCS/parking.json" --size 1600x1000 --seed 1 \
  --fps off --tilt off --clock fixed > target/parking.log 2>&1
for s in $SHOTS; do
  python3 - "$OUT/$s.png" "$OUT/$s.json" <<'EOF'
import json, sys
from PIL import Image, ImageStat
png, state = sys.argv[1], sys.argv[2]
mean = sum(ImageStat.Stat(Image.open(png).convert("RGB")).mean) / 3
village = json.load(open(state))["packs"]["village"]
print(f"{png}: mean {mean:.1f}, village {village}")
if mean < 5 or village != "ready":
    sys.exit(f"{png}: all black or the village pack not ready")
EOF
  ffmpeg -loglevel error -y -i "$OUT/$s.png" -q:v 2 "$DOCS/$s.jpg"
  cp "$OUT/$s.json" "$DOCS/$s.json"
done
echo "wrote $DOCS/"
