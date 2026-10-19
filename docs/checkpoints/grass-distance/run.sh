#!/bin/bash
# Grass distance checkpoint (owner request 2026-10-19): the water world at radius 8 shot natively at
# 1280x800 on High, rich-3, tilt-shift off and fixed clocks, from 40, 80 and 150 m at Near and at
# Far (set live with set_grass_distance, 300 frames per view for the build budget to fill the new
# radius), with a state dump per view (grass.distance, groups, blades, mesh_bytes). Writes JPEGs and
# the dumps here. Under heavy host load a native frame can come back black: the run is deterministic
# (fixed clocks), so each good frame is kept and the capture repeats, up to 4 times, until every view
# has one. Usage: docs/checkpoints/grass-distance/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/grass-distance
OUT=target/shots
VIEWS="near-40m near-80m near-150m far-40m far-80m far-150m"
mkdir -p "$OUT"
node "$DOCS/script.mjs" target/grass-distance.json
cargo build -p hex-app --release
rm -f "$DOCS"/grass-distance-*.jpg
for attempt in 1 2 3 4; do
  rm -f "$OUT"/grass-distance-*
  caffeinate -d -u target/release/hex-world --scenario play --script target/grass-distance.json \
    --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed --tier high \
    --grade rich-3 > target/grass-distance.log 2>&1
  missing=0
  for v in $VIEWS; do
    [[ -f "$DOCS/grass-distance-$v.jpg" ]] && continue
    m=$(magick "$OUT/grass-distance-$v.png" -format '%[fx:mean]' info:)
    if awk -v m="$m" 'BEGIN { exit !(m > 0.02) }'; then
      ffmpeg -loglevel error -y -i "$OUT/grass-distance-$v.png" -q:v 2 "$DOCS/grass-distance-$v.jpg"
      cp "$OUT/grass-distance-state-$v.json" "$DOCS/"
    else
      echo "attempt $attempt: grass-distance-$v.png is black (mean $m)"
      missing=1
    fi
  done
  [[ "$missing" == 0 ]] && { echo "wrote $DOCS/"; exit 0; }
done
echo "frames still black after 4 attempts" >&2
exit 1
