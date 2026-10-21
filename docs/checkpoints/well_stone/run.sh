#!/bin/bash
# well_stone / well_stone_b checkpoint: the covered well (house_cluster hero 3) in the kit scenario's
# house_cluster on (-1, 0) and the open variant in the variants' cluster on (-1, -1), both at tile
# (5.3, -7.7) yaw 60 (needs their manifest entries and the kit_scene placements). Runs kit.json natively
# at 1600x1000 with tilt-shift off: play distance (about 37 m), a sun-side and a shade-side close-up at
# 5 m, and a scale shot beside cottage 0, for each well. Then play.json in --scenario play: clusters at
# (1, 0) (the covered well) and (0, -1) (the open well, `hex_core` variant pick; both placed at rotation 0),
# and (-1, 0) (the open well; the game turns it to rotation 1), an overview,
# and walk mode at 1.7 m for each well (a lawn view and a look into the well from its apron, with a
# state dump of where the walker stands). Then the textured headless EEVEE stills (eevee_still.py) and
# the review sheet (sheet.py). Native shots can come back black (idle display, high load): each PNG's
# mean is checked and the run retried once.
# Usage: bash docs/checkpoints/well_stone/run.sh (from the repo root, after make kit-build PIECE=well_stone
# and PIECE=well_stone_b).
set -euo pipefail
DOCS=docs/checkpoints/well_stone
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="well-play well-close-sun well-close-shade well-scale wellb-play wellb-close-sun wellb-close-shade wellb-scale"
PLAY="well-play-overview well-walk-eye well-walk-apron wellb-walk-eye wellb-walk-apron"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/well-*.png "$OUT"/wellb-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/well-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/well-play.log 2>&1
}
run
# shellcheck disable=SC2086
if ! python3 docs/checkpoints/house_twostorey_c/means.py "$OUT" $SHOTS $PLAY; then
  echo "retrying once"
  run
  # shellcheck disable=SC2086
  python3 docs/checkpoints/house_twostorey_c/means.py "$OUT" $SHOTS $PLAY
fi
for n in $SHOTS $PLAY; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
for p in well_stone well_stone_b; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" "$p" > "target/$p-stills.log" 2>&1
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
