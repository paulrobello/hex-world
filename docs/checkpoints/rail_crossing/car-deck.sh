#!/bin/bash
# Cars over the level crossing (cars follow the road surface height, owner 2026-10-19): the
# cars-kit road tree with a level crossing tile `rail_road_crossing` (rail_road_crossing_center as
# a road tile, road on edges 1 and 4) in place of the road_straight east of the hub, at (2, 0),
# world centre (55.4, 0). Runs car-deck.json natively at 1600x1000 with `--clock fixed`, so the run
# replays identically: an overview after 30 s, then 30 close-ups of the deck 8 frames apart with a
# state dump each (car positions under play.car_list). DECK and RAMP name the close-ups kept: a
# pickup and a van level on the deck, and the two pitched on the ramps.
# Runs against the real `rail_road_crossing` tile (data/tiles/rail_road_crossing.ron). Its rail ends
# face empty cells, so the stub has no train and the crossing stays open.
# Usage: bash docs/checkpoints/rail_crossing/car-deck.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_crossing
OUT=target/shots
DECK=${DECK:-06}
RAMP=${RAMP:-03}
mkdir -p "$OUT"
rm -f "$OUT"/car-deck-*.png "$OUT"/car-deck-*.json
cargo build -p hex-app --release
caffeinate -d -u -t 900 target/release/hex-world --scenario play --script "$DOCS/car-deck.json" \
  --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed > target/car-deck.log 2>&1
ffmpeg -loglevel error -y -i "$OUT/car-deck-overview.png" -q:v 2 "$DOCS/car-deck-overview.jpg"
ffmpeg -loglevel error -y -i "$OUT/car-deck-$DECK.png" -q:v 2 "$DOCS/car-deck.jpg"
ffmpeg -loglevel error -y -i "$OUT/car-deck-$RAMP.png" -q:v 2 "$DOCS/car-deck-ramp.jpg"
echo "wrote $DOCS/car-deck.jpg (close-up $DECK), car-deck-ramp.jpg ($RAMP) and car-deck-overview.jpg"
