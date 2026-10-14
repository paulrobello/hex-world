#!/usr/bin/env bash
# Road-verge evidence: in-play shots of a road_straight between meadows on both sun sides, JPEG
# copies tagged with the label, then the band/meadow luminance and 4x contact crops (measure.py).
# Usage: docs/checkpoints/road-verge/shoot.sh <before|after>
set -euo pipefail
label="$1"
root="$(cd "$(dirname "$0")/../../.." && pwd)"
docs="$root/docs/checkpoints/road-verge"
cd "$root"
mkdir -p target/shots
rm -f target/shots/road-verge-*.png target/shots/road-verge-*.json
cargo run -p hex-app --release -- --scenario play --script "$docs/road-verge.json" --size 1600x1000 \
  --seed 1 --fps off --tilt off --clock fixed
for n in road-verge-play road-verge-play-north road-verge-top-sun road-verge-top-shade road-verge-sun-side road-verge-shade-side; do
  test -s "target/shots/$n.png"
  ffmpeg -loglevel error -y -i "target/shots/$n.png" -q:v 2 "$docs/${n/road-verge-/road-verge-$label-}.jpg"
done
cp target/shots/road-verge-state.json "$docs/road-verge-$label-state.json"
python3 "$docs/measure.py" "$label"
