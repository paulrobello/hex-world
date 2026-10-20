#!/bin/bash
# tree_hero_oak_c checkpoint (the oak tile's third hero oak). Needs integration.patch applied (play draws
# the variant only through hex_core::HERO_VARIANTS). Cells (cells.py): (-1, 0) draws tree_hero_oak_c,
# (1, 0) tree_hero_oak and (1, -2) tree_hero_oak_b, with a house cluster at (0, -1) for scale. The
# variant's oak tile is placed first, alone, so the first free flock roosts on it: roost shots (wide and
# close, roosting count in oakc-roost-state), then the other tiles, play distance and top view of the
# variant and the hero (same cameras), a scale shot of all three oaks and the houses, sun-side and
# shade-side close-ups, the dead top, the trunk, a wind pair 75 frames apart, eye level 20 m south of the
# variant and the hero and under the variant's crown (walk mode), then a 7 s walk into its trunk
# (oakc-walk-state). Then the textured EEVEE stills (still.py) and the review sheet (sheet.py). Native
# shots can come back black (idle display, high load): each PNG's mean is checked and the run retried
# once. Usage: git apply docs/checkpoints/tree_hero_oak_c/integration.patch, then
#   bash docs/checkpoints/tree_hero_oak_c/run.sh (from the repo root, after make kit-build PIECE=tree_hero_oak_c).
set -euo pipefail
DOCS=docs/checkpoints/tree_hero_oak_c
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="oakc-roost oakc-roost-close oakc-play-pair oakc-play oak-play oakc-top oak-top oakc-scale oakc-close-sun
oakc-close-shade oakc-deadtop oakc-trunk-sun oakc-wind-a oakc-wind-b oakc-eye oak-eye oakc-under"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/oak-*.png "$OUT"/oakc-*.png
  caffeinate -u -d -t 900 target/release/hex-world --scenario play --script "$DOCS/oak_c.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/oakc-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$DOCS" > target/oakc-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
