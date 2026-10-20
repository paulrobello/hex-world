#!/bin/bash
# tree_hero_oak_b checkpoint (the oak tile's hero variant). Needs integration.patch applied (play draws
# the variant only through hex_core::HERO_VARIANTS): oak tiles at (1, 0), which draws tree_hero_oak, and
# (-1, 0), which draws tree_hero_oak_b, among meadows, in play at 1600x1000 with tilt-shift off and the
# fixed clock. Shots: the pair, play distance and top view of each oak (same cameras), sun-side and
# shade-side close-ups of the variant's crown, its trunk and fork, and eye level 20 m south of each (walk
# mode), then a 3 s walk into the trunk (oakb-walk-state in the log). Then the textured EEVEE stills and
# wind render (eevee_still.py), the six-view comparison and the review sheet. Native shots can come back
# black (idle display, high load): each PNG's mean is checked and the run retried once.
# Usage: git apply docs/checkpoints/tree_hero_oak_b/integration.patch, then
#        bash docs/checkpoints/tree_hero_oak_b/run.sh (from the repo root, after make kit-build PIECE=tree_hero_oak_b).
set -euo pipefail
DOCS=docs/checkpoints/tree_hero_oak_b
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="oakb-play-pair oakb-play oak-play oakb-top oak-top oakb-close-sun oakb-close-shade oakb-trunk-sun oakb-eye oak-eye"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/oak-*.png "$OUT"/oakb-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/oak_b.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/oakb-shots.log 2>&1
}
run
# shellcheck disable=SC2086
if ! python3 docs/checkpoints/house_twostorey_b/means.py "$OUT" $SHOTS; then
  echo "retrying once"
  run
  # shellcheck disable=SC2086
  python3 docs/checkpoints/house_twostorey_b/means.py "$OUT" $SHOTS
fi
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/oakb-stills.log 2>&1
python3 "$DOCS/compare.py"
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
