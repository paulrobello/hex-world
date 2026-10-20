#!/bin/bash
# Trains in play (P3): runs rail-play.json (written by rail_play.py) natively in the play scenario at
# 1600x1000 with `--clock fixed --diag`, tilt-shift off, so the run replays identically. The loop
# world: an overview at play distance and close-ups of a straight; the Y world: an overview, then
# shot sequences at the junction, the level crossing and at eye level beside the west arm, each shot
# with a state dump. The frames kept are named below (picked from the dumps: a train on each leg of
# the junction, a car held at the closed crossing, a train passing the walker); each is checked for
# a black frame and the run retried once. Usage: bash docs/checkpoints/rail/play.sh (repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail
OUT=target/shots
LOOP_CLOSE=${LOOP_CLOSE:-01}
JUNCTION=${JUNCTION:-16}
CROSSING=${CROSSING:-25}
WALK=${WALK:-17}
KEEP="rail-ghost:rail-ghost rail-loop:rail-loop rail-loop-close-$LOOP_CLOSE:rail-loop-close rail-y:rail-y
rail-junction-$JUNCTION:rail-junction rail-crossing-$CROSSING:rail-crossing rail-walk-$WALK:rail-walk-eye"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/rail-*.png "$OUT"/rail-*.json
  caffeinate -d -u -t 1800 target/release/hex-world --scenario play --script "$DOCS/rail-play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --diag > target/rail-play.log 2>&1
}
dark() {
  for pair in $KEEP; do
    n=${pair%%:*}
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
for pair in $KEEP; do
  ffmpeg -loglevel error -y -i "$OUT/${pair%%:*}.png" -q:v 2 "$DOCS/${pair##*:}.jpg"
  cp "$OUT/${pair%%:*}.json" "$DOCS/${pair##*:}.json"
done
echo "wrote $DOCS/rail-{ghost,loop,loop-close,y,junction,crossing,walk-eye}.{jpg,json}"
