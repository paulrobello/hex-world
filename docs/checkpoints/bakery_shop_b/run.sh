#!/bin/bash
# bakery_shop_b checkpoint (the bakery slot's hero variant, PRD §6 Variety). Needs integration.patch applied
# (manifest entry, HERO_VARIANTS row, kit_scene placement) and the staged build (make kit-build
# PIECE=bakery_shop_b, make kit-compress PIECE=bakery_shop_b).
# 1. Kit scenario (kit.json): bakery_shop_b across the road from bakery_shop on road tile (1, 0), its front
#    to the kit sun: play distance (about 45 m), both bakeries from straight above, a sun-side close-up of
#    the shop front (6 m), the shade side (back, 7 m) and a scale shot down the road with house_cluster.
# 2. Play scenario (play.json): bakery tiles at (0, 1), whose slot draws bakery_shop_b
#    (hex_core::hero_piece; `cargo test -p hex-core bakery_b_cells -- --ignored --nocapture`), and (1, 1),
#    which keeps bakery_shop, with a house_cluster at (-1, 1) on the same road so its cars visit: play
#    distance, the variant's front (sun) and back (shade) close up, the pair side by side, and walk mode at
#    eye level (1.7 m) on the forecourt mouth facing the shop.
# Then the textured headless EEVEE stills (eevee_still.py) and the review sheet (sheet.py). All native at
# 1600x1000 with tilt-shift off. Native shots can come back black (idle display, high load): each PNG's
# mean is checked and the run retried once.
# Usage: bash docs/checkpoints/bakery_shop_b/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/bakery_shop_b
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
KIT_SHOTS="bakery_b-kit-play bakery_b-kit-top bakery_b-kit-close-sun bakery_b-kit-close-shade bakery_b-kit-scale"
PLAY_SHOTS="bakery_b-play bakery_b-play-close-sun bakery_b-play-close-shade bakery_b-play-pair bakery_b-walk-eye"
mkdir -p "$OUT" target/bakery_b-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/bakery_b-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/bakery_b-kit.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/bakery_b-play.log 2>&1
}
uptime
run
if ! python3 "$DOCS/means.py" "$OUT" $KIT_SHOTS $PLAY_SHOTS; then
  echo "retrying once"
  run
  python3 "$DOCS/means.py" "$OUT" $KIT_SHOTS $PLAY_SHOTS
fi
uptime
for n in $KIT_SHOTS $PLAY_SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/eevee_still.py" -- "$DOCS" > target/bakery_b-stills.log 2>&1
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
