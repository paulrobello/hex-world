#!/usr/bin/env bash
# Oak canopy gap evidence: in-play shots of the hero oak (oak tile at (0, -1), centre (-13.86, 0, -24))
# at canopy close-ups (sun side, shade side, grazing across a dome top; KIT_SUN_DIR points +x +z) and
# at two play distances, JPEG copies tagged with the label, then the gap/leaf statistics of the
# shipped maps (measure.py, classified on the pre-fix albedo of commit 69f7f6f).
# Usage: docs/checkpoints/oak-canopy/shoot.sh <before|after>
set -euo pipefail
label="$1"
root="$(cd "$(dirname "$0")/../../.." && pwd)"
docs="$root/docs/checkpoints/oak-canopy"
cd "$root"
mkdir -p target/shots
rm -f target/shots/oak-canopy-*.png target/shots/oak-canopy-*.json
cargo run -p hex-app --release -- --scenario play --script "$docs/oak-canopy.json" --size 1600x1000 \
  --seed 1 --fps off --tilt off --clock fixed
for n in oak-canopy-closeup-sun oak-canopy-closeup-shade oak-canopy-closeup-grazing oak-canopy-play oak-canopy-play-far; do
  test -s "target/shots/$n.png"
  ffmpeg -loglevel error -y -i "target/shots/$n.png" -q:v 2 "$docs/${n/oak-canopy-/oak-canopy-$label-}.jpg"
done
git show 69f7f6f:assets/kit/p1/tree_hero_oak/tree_hero_oak.albedo.png > target/oak-canopy-ref-albedo.png
python3 "$docs/measure.py" assets/kit/p1/tree_hero_oak target/oak-canopy-ref-albedo.png "$docs/maps-$label.json"
