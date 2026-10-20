#!/bin/bash
# Trains routing between stations (P3): runs play.json (written by rail_routing.py) natively in the
# play scenario at 1600x1000 under caffeinate with `--clock fixed`, tilt-shift off, so the run
# replays identically. A turnout with a station off each leg and one train: an overview
# (routing-overview), then a shot and a state dump every 1.5 s. From the dumps it keeps, for each
# station, the first frame with the train standing there (`waiting` "station") with that station
# as its `destination` (routing-east, routing-north), copies each dump beside its JPEG, and checks
# every kept PNG's mean (an idle display draws black frames), retrying the run once.
# Usage: bash docs/checkpoints/rail-routing/play.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail-routing
OUT=target/shots
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/routing-*.png "$OUT"/routing-*.json
  caffeinate -d -u -t 1800 target/release/hex-world --scenario play --script "$DOCS/play.json" \
    --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed \
    > target/rail-routing.log 2>&1
}
pick() {
  python3 "$DOCS/pick.py" "$OUT"
}
dark() {
  for pair in $(pick); do
    n=${pair%%:*}
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
for pair in $(pick); do
  ffmpeg -loglevel error -y -i "$OUT/${pair%%:*}.png" -q:v 2 "$DOCS/${pair##*:}.jpg"
  cp "$OUT/${pair%%:*}.json" "$DOCS/${pair##*:}.json"
done
echo "wrote $DOCS/routing-{overview,east,north}.{jpg,json}"
