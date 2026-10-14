#!/bin/bash
# §9.4 hook verification shots (PRD D7): the kit and coast checkpoints under each `--kit_hooks`
# variant, as JPEG stills next to this script. `none` is the MVP look; every other variant shows its
# hook doing something. Run from the repo root after `cargo build -p hex-app --release`.
# caffeinate keeps the display awake (a sleeping display captures black frames).
set -euo pipefail
OUT=docs/checkpoints/hooks
BIN=target/release/hex-world
COMMON=(--seed 1 --fps off --clock fixed --water_clock fixed)
mkdir -p target/shots "$OUT"

jpeg() { ffmpeg -loglevel error -y -i "target/shots/$1.png" -q:v 3 "$OUT/$2.jpg"; }

for h in none snow wet night dusk autumn; do
  rm -f target/shots/kit-*.png
  caffeinate -d -u "$BIN" --script scripts/shots/kit.json --size 1600x1000 "${COMMON[@]}" --kit_hooks "$h"
  jpeg kit-overview "kit-overview-$h"
  case $h in none | snow | night | dusk | autumn) jpeg kit-tree-game "kit-tree-game-$h" ;; esac
done

for h in none freeze dusk night; do
  rm -f target/shots/coast-*.png
  caffeinate -d -u "$BIN" --scenario play --script docs/checkpoints/coast/coast.json --size 1600x1000 \
    --tilt off "${COMMON[@]}" --kit_hooks "$h"
  jpeg coast-01-overview "coast-overview-$h"
  case $h in none | freeze) jpeg coast-03-headland "coast-headland-$h" ;; esac
done
echo "wrote $OUT/"
