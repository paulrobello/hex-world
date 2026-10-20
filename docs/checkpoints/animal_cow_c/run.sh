#!/bin/bash
# animal_cow_c checkpoint (the farm's standing cow slot's hero variant, PRD §6 Variety). Needs
# integration.patch applied (HERO_VARIANTS row, walk and scatter rows) and the staged builds:
# make kit-build PIECE=animal_cow_c, and HEXW_SKIP_BAKE=1 make kit-build PIECE=animal_cow / animal_cow_b
# for the EEVEE lineup (their committed maps are bound; then
# git checkout -- assets-source/kit/animal_cow assets-source/kit/animal_cow_b).
# 1. Play scenario (play.json): farms at (1, -2), whose cow slot (hero 4) draws animal_cow_c
#    (hex_core::hero_piece; pinned by variant.rs some_farm_cow_slots_draw_animal_cow_c), and (1, -1),
#    which keeps animal_cow, each with its grazing animal_cow_b, among meadows. The variant stands at
#    world (0.25, -39.75), facing east. Shots: play distance over both farms, a nearer play view, a
#    5 m close-up on the sun side (the sun is at (200, 400, 150)), the shade side from the west, the
#    face, scale before the barn, and walk mode at eye level (1.7 m) 6 m south-east of the cow.
# 2. The textured EEVEE stills (still.py) and the review sheet (sheet.py).
# Native at 1600x1000 with tilt-shift off. Shots are not timing measurements: they run under caffeinate
# without a load wait, each PNG's mean is checked (docs/checkpoints/bakery_shop_b/means.py) and the run
# retried once. Usage: bash docs/checkpoints/animal_cow_c/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/animal_cow_c
OUT=target/shots
STILL_OUT=target/cowc-stills
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
SHOTS="cowc-play cowc-play-near cowc-sun cowc-shade cowc-face cowc-scale-barn cowc-walk-eye"
STILLS="lineup animal_cow_c-eevee-front_three_quarter animal_cow_c-eevee-back_three_quarter animal_cow_c-eevee-face animal_cow_c-eevee-top animal_cow_c-eevee-lod2"
mkdir -p "$OUT" "$STILL_OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/cowc-*.png
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/cowc-play.log 2>&1
}
uptime
run
if ! python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $SHOTS; then
  echo "retrying once"
  run
  python3 docs/checkpoints/bakery_shop_b/means.py "$OUT" $SHOTS
fi
uptime
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$STILL_OUT" > "$STILL_OUT/still.log" 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$STILL_OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
