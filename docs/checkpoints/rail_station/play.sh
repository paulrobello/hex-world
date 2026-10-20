#!/bin/bash
# Rail station tile in play (P3): runs play.json natively at 1600x1000 under caffeinate with
# --clock fixed, so the shuttle replays identically. It places the station on (0, -1) between two
# rail straights each side, and shoots the train standing at the platform from play distance
# (station-play-train), eye level on the paving looking along the platform at the train
# (station-play-eye), the east ramp (station-play-ramp) and the shelter close-up from the paving
# (station-play-shelter). A state dump beside each train shot must report the train waiting at the
# station, and every PNG's mean is checked (an idle display draws black frames).
# Usage: bash docs/checkpoints/rail_station/play.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/rail_station
OUT=target/shots
SHOTS="station-play-train station-play-eye station-play-ramp station-play-shelter"
mkdir -p "$OUT"
rm -f "$OUT"/station-play-*.png "$OUT"/station-play-*.json
cargo build -p hex-app --release
caffeinate -u -d -t 900 target/release/hex-world --scenario play --script "$DOCS/play.json" \
  --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed \
  > target/station-play.log 2>&1
for n in $SHOTS; do
  test -s "$OUT/$n.png" || { echo "missing $n.png"; exit 1; }
  m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
  awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m): run again"; exit 1; }
done
for d in station-play-train-state station-play-eye-state; do
  python3 -c '
import json, sys
s = json.load(open(sys.argv[1]))
def find(v):
    if isinstance(v, dict):
        if "train_list" in v:
            return v["train_list"]
        for x in v.values():
            r = find(x)
            if r is not None:
                return r
    return None
trains = find(s) or []
waiting = [t["waiting"] for t in trains]
assert "station" in waiting, f"{sys.argv[1]}: no train at the station: {trains}"
print(sys.argv[1], "trains", trains)
' "$OUT/$d.json"
done
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
cp "$OUT/station-play-train-state.json" "$OUT/station-play-eye-state.json" "$DOCS/"
echo "wrote $DOCS/"
