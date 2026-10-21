#!/bin/bash
# The wide rail bend in play: runs play.json (written by play.py) natively in the play scenario at
# 1600x1000 with `--clock fixed`, tilt-shift off, so the run replays identically: six rail_bend_120
# tiles close a 24 m circle round the start meadow with a train on it. Then kit.json in the kit
# scenario: the wide bend on (-4, 7) and its seam with the rail straight on (-3, 6). Every shot is an
# offscreen photo export; each kept frame is checked for a black frame and its run retried once. The
# series frames kept are named below (picked from the state dumps: the train in view). Then sheet.py.
# Usage: bash docs/checkpoints/rail_bend_120/play.sh (repo root, after cargo build --release -p hex-app).
set -euo pipefail
DOCS=docs/checkpoints/rail_bend_120
OUT=target/shots
CLOSE=${CLOSE:-15}
TRAIN=${TRAIN:-07}
EYE=${EYE:-22}
PLAY_KEEP="rb120-play:distance rb120-close-$CLOSE:close rb120-train-$TRAIN:train rb120-eye-$EYE:eye rb120-sun:sun rb120-shade:shade"
KIT_KEEP="rb120-kit-overview:kit-overview rb120-kit-seam:kit-seam"
mkdir -p "$OUT"
run_play() {
  rm -f "$OUT"/rb120-*.png "$OUT"/rb120-*.json
  caffeinate -d -u -t 1800 target/release/hex-world --scenario play --script "$DOCS/play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/rb120-play.log 2>&1
}
run_kit() {
  rm -f "$OUT"/rb120-kit-*.png
  caffeinate -d -u -t 600 target/release/hex-world --script "$DOCS/kit.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off > target/rb120-kit.log 2>&1
}
dark() {
  for pair in $1; do
    n=${pair%%:*}
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
run_play
if dark "$PLAY_KEEP"; then echo "retrying the play run once"; run_play; dark "$PLAY_KEEP" && exit 1; fi
cp "$OUT/rb120-placed.json" "$DOCS/placed.json"
for pair in $PLAY_KEEP; do
  ffmpeg -loglevel error -y -i "$OUT/${pair%%:*}.png" -q:v 2 "$DOCS/${pair##*:}.jpg"
  cp "$OUT/${pair%%:*}.json" "$DOCS/${pair##*:}.json"
done
run_kit
if dark "$KIT_KEEP"; then echo "retrying the kit run once"; run_kit; dark "$KIT_KEEP" && exit 1; fi
for pair in $KIT_KEEP; do
  ffmpeg -loglevel error -y -i "$OUT/${pair%%:*}.png" -q:v 2 "$DOCS/${pair##*:}.jpg"
done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
