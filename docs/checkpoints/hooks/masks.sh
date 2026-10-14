#!/bin/bash
# §9.4 hook masks on the small prop slots (2026-10-14): a coast, a river with the road bridge, the
# river bend, an ocean tile and a far overview (masks.json, play scenario, agents paused) under each
# `--kit_hooks` variant, plus the coast checkpoint's overview, headland (beach_outer_center) and bay
# (beach_inner_center) under none and snow, as JPEG stills in masks-<variant>/<view>-after.jpg. The
# *-before.jpg stills beside them came from the same scripts on the build before the masks (6cd0e3e
# and its assets). Run from the repo root after `cargo build -p hex-app --release`. caffeinate keeps
# the display awake (a sleeping display captures black frames).
set -euo pipefail
OUT=docs/checkpoints/hooks
BIN=target/release/hex-world
COMMON=(--seed 1 --fps off --clock fixed --water_clock fixed --tilt off --size 1600x1000)
mkdir -p target/shots

jpeg() { ffmpeg -loglevel error -y -i "target/shots/$1.png" -q:v 3 "$OUT/masks-$2/$3-after.jpg"; }

for h in none snow wet night autumn; do
  rm -f target/shots/masks-*.png
  caffeinate -d -u "$BIN" --scenario play --script "$OUT/masks.json" "${COMMON[@]}" --kit_hooks "$h"
  mkdir -p "$OUT/masks-$h"
  for v in coast ocean river river-bend river-bridge far; do jpeg "masks-$v" "$h" "masks-$v"; done
done

for h in none snow; do
  rm -f target/shots/coast-*.png
  caffeinate -d -u "$BIN" --scenario play --script docs/checkpoints/coast/coast.json "${COMMON[@]}" --kit_hooks "$h"
  for v in 01-overview 03-headland 04-bay; do jpeg "coast-$v" "$h" "masks-coastjson-$v"; done
done
echo "wrote $OUT/masks-*/"
