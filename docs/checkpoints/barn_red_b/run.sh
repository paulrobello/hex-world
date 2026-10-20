#!/bin/bash
# barn_red_b checkpoint (farm barn-slot variant, PRD §6 Variety). Needs the manifest entry, the
# HERO_VARIANTS row and the kit_scene placement from integration.patch applied.
# 1. Kit scenario (barn_b.json): barn_red_b in the barn slot of the hay-variants farm on (1, 5), with
#    barn_red in the same slot next door on (0, 5): play distance (about 60 m), a sun-side close-up of
#    the door gable, the open shed from the yard (about 7 m), the shade side (back gable and the closed
#    long wall, 7.5 m) and a scale shot of both barns.
# 2. Play scenario (play.json): farms placed at (0, 1) and (-1, 0), which draw barn_red_b in the barn
#    slot (hex_core::hero_piece), and (1, 0), which keeps barn_red: overviews at play distance from the
#    east and from the south (the start camera's side, which sees the open shed), the seam between (0, 1)'s edge 5 (the barn's back, 0.5 m inside it) and (1, 0), and walk mode at eye
#    level (1.7 m) in the (0, 1) yard facing the shed and in front of the doors.
# Then the textured headless EEVEE stills (still.py) and the review sheet (sheet.py). All native at
# 1600x1000 with tilt-shift off. Native shots can come back black (idle display, high load): each
# PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/barn_red_b/run.sh (from the repo root, after make kit-build PIECE=barn_red_b
# and make kit-compress PIECE=barn_red_b).
set -euo pipefail
DOCS=docs/checkpoints/barn_red_b
OUT=target/shots
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
KIT_SHOTS="barn_b-play barn_b-close-sun barn_b-close-shed barn_b-close-shade barn_b-scale"
PLAY_SHOTS="barn_b-play-overview barn_b-play-south barn_b-seam barn_b-walk-eye barn_b-walk-doors"
mkdir -p "$OUT" target/barn_b-stills
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/barn_b-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/barn_b.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/barn_b-shots.log 2>&1
  caffeinate -u -d -t 600 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/barn_b-play.log 2>&1
}
dark() {
  for n in $KIT_SHOTS $PLAY_SHOTS; do
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    echo "$n mean $m"
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
uptime
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
uptime
for n in $KIT_SHOTS $PLAY_SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
for v in hero back scale; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/barn_b-stills/still-$v.png" "$v" \
    assets/kit/p1/barn_red_b/barn_red_b.glb assets/kit/p1/barn_red_b/barn_red_b.masks.png assets/kit/manifest.json \
    > "target/barn_b-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/barn_b-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
