#!/usr/bin/env bash
# Oak trunk evidence: in-play shots of the hero oak (oak tile at (0, -1), centre (-13.86, 0, -24)):
# low trunk close-ups about 3.7 m from the trunk on the sun and shade sides (KIT_SUN_DIR points +x +z),
# an eye-level view about 9 m out, and two play distances showing the tree's size on its tile.
# Fixed clock, tilt-shift off. JPEG copies are tagged with the label.
# Usage: docs/checkpoints/oak-trunk/shoot.sh <before|after> [stage dir]
# A stage dir holds a release `hex-world` binary and a copy of `assets/` (a checkout's state frozen
# for the run, e.g. main's binary and kit for "before"); without one the checkout runs via cargo.
set -euo pipefail
label="$1"
root="$(cd "$(dirname "$0")/../../.." && pwd)"
docs="$root/docs/checkpoints/oak-trunk"
run="${2:-$root}"
cd "$run"
mkdir -p target/shots
rm -f target/shots/oak-trunk-*.png target/shots/oak-trunk-*.json
args=(--scenario play --script "$docs/oak-trunk.json" --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed)
if [ -n "${2:-}" ]; then
  caffeinate -u -d -t 600 ./hex-world "${args[@]}"
else
  caffeinate -u -d -t 600 cargo run -p hex-app --release -- "${args[@]}"
fi
for n in oak-trunk-closeup-sun oak-trunk-closeup-shade oak-trunk-eye oak-trunk-play oak-trunk-play-far; do
  test -s "target/shots/$n.png"
  ffmpeg -loglevel error -y -i "target/shots/$n.png" -q:v 2 "$docs/${n/oak-trunk-/oak-trunk-$label-}.jpg"
done
