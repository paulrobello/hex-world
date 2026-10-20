#!/bin/bash
# Hay variants checkpoint (PRD §6 Variety): hay_bale_round_b and hay_bale_stack_b.
# 1. The kit scenario (hay_variants.json, natively at 1600x1000, tilt-shift off): the hay-variants farm
#    on (1, 5) (kit_scene FARM_HAY_VARIANTS, world centre (96.99, 120)) at play distance, the farm's
#    originals on (-1, 5) from the same angle, each variant close up on the sun side (the sun is at
#    (200, 400, 150)) and the shade side, a 1.7 m kit camera, and scale beside the barn.
# 2. Play (walk.json, needs integration.patch applied: the farm at (1, 0) draws hay_bale_round_b in
#    its first round slot through the hero variant resolver): play distance, eye level 1.7 m in walk
#    mode, then a walk into the bale that dumps the block (hayv-walk-state.json: blocked `hay`).
# Then the textured EEVEE stills (still.py) and the review sheet (sheet.py). Shots are not timing
# measurements: they run under caffeinate without a load wait, each PNG's mean is checked
# (means.py) and a dark run retried once.
# Usage: bash docs/checkpoints/hay_variants/run.sh (from the repo root, after make kit-build for both
# variants so target/kit-stage/ holds their GLBs and maps; the originals' committed maps are bound).
set -euo pipefail
DOCS=docs/checkpoints/hay_variants
OUT=target/shots
STILL_OUT=target/hayvar-stills
KIT="hayv-play hayv-play-originals hayv-stack-sun hayv-stack-shade hayv-round-sun hayv-round-shade hayv-eye-kit hayv-scale-barn"
PLAY="hayv-walk-play hayv-walk-eye"
STILLS="hay-round-pair hay-stack-pair hay_bale_round_b-eevee-three_quarter hay_bale_round_b-eevee-low_side hay_bale_stack_b-eevee-three_quarter hay_bale_stack_b-eevee-low_side"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT" "$STILL_OUT"
cargo build -p hex-app --release
kit() {
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/hay_variants.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/hay-variants-kit.log 2>&1
}
play() {
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/walk.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/hay-variants-play.log 2>&1
}
kit
python3 "$DOCS/means.py" "$OUT" $KIT || { echo "retrying kit shots"; kit; python3 "$DOCS/means.py" "$OUT" $KIT; }
play
python3 "$DOCS/means.py" "$OUT" $PLAY || { echo "retrying play shots"; play; python3 "$DOCS/means.py" "$OUT" $PLAY; }
for n in $KIT $PLAY; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
cp "$OUT/hayv-walk-state.json" "$DOCS/"
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$STILL_OUT" > "$STILL_OUT/still.log" 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$STILL_OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
