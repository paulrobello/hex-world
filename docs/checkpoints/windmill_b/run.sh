#!/bin/bash
# windmill_b checkpoint (the windmill slot's hero variant, PRD §5.4 row 2b, §6 Variety). Needs
# integration.patch applied (HERO_VARIANTS row, walk and scatter rows, kit_scene placement) and the
# staged build (make kit-build PIECE=windmill_b, make kit-compress PIECE=windmill_b).
# 1. Play scenario (play.json): windmill tiles at (0, 1), whose slot draws windmill_b
#    (hex_core::hero_piece; pinned by variant.rs some_windmill_tiles_draw_the_smock_mill), and (1, 0),
#    which keeps windmill, with a house_cluster at (-1, 1) for scale: play distance and the same view
#    60 frames (1 s) later under --clock fixed (sails +24° at 4 rpm), sun and shade close-ups of the base
#    and stage, the scale shot, and walk mode at eye level (1.7 m) 9 m in front of the door.
# 2. Kit scenario (kit.json): windmill_b on its meadow at (4, -1): a closer pair 1 s apart and both mills.
# Then the EEVEE stills with the masks bound (eevee_still.py) and the review sheet (sheet.py). All native
# at 1600x1000 with tilt-shift off. Native shots can come back black (idle display, high load): each
# PNG's mean is checked (docs/checkpoints/bakery_shop_b/means.py) and the run retried once.
# Usage: bash docs/checkpoints/windmill_b/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/windmill_b
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
PLAY_SHOTS="windmill_b-play windmill_b-play-1s windmill_b-play-close-sun windmill_b-play-close-shade windmill_b-play-scale windmill_b-walk-eye"
KIT_SHOTS="windmill_b-kit-t1 windmill_b-kit-t2 windmill_b-kit-pair"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/windmill_b-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/windmill_b-play.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/windmill_b-kit.log 2>&1
}
uptime
run
if ! python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $PLAY_SHOTS $KIT_SHOTS; then
  echo "retrying once"
  run
  python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $PLAY_SHOTS $KIT_SHOTS
fi
uptime
for n in $PLAY_SHOTS $KIT_SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/windmill_b-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
