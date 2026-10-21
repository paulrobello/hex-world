#!/bin/bash
# Terrain-flatten review shots (owner D6-Q1, 2026-10-20: flatten the border rise in geometry). Runs
# the terrain-hump shots (3x3 meadow and forest fields, road row, beach) and the terrain-hump-water
# shots (spring, straight, bend, beach) unchanged, then farm-play.json: the farm tile and the
# wheat_field tile among meadows (paddock fence, hay bales, wheat edge), each at noon and with
# `--kit_hooks dusk` (the sun about 3 degrees up). Run once before and once after the rebuild.
# Usage: docs/checkpoints/terrain-flatten/run.sh <tag> <out-dir> [farm]   (farm: the farm set only)
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
TAG=$1
OUT=$2
BIN="$ROOT/target/release/hex-world"
SCRIPT="$ROOT/docs/checkpoints/terrain-flatten/farm-play.json"
SHOTS="fw-orbit fw-low fw-paddock fw-hay fw-wheat fw-wheat-edge"
mkdir -p "$OUT" "$ROOT/target/shots"
if [ "${3:-}" != farm ]; then
  bash "$ROOT/docs/checkpoints/terrain-hump/run.sh" "$TAG" "$OUT"
  bash "$ROOT/docs/checkpoints/terrain-hump-water/run.sh" "$TAG" "$OUT"
fi
run() {  # $1 = suffix, rest = extra flags
  local sfx=$1
  shift
  rm -f "$ROOT"/target/shots/fw-*.png
  (cd "$ROOT" && caffeinate -u -d -t 900 "$BIN" --scenario play --script "$SCRIPT" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed "$@" > "$OUT/$TAG-fw$sfx.log" 2>&1)
  for n in $SHOTS; do cp "$ROOT/target/shots/$n.png" "$OUT/$TAG-$n$sfx.png"; done
}
dark() {  # $1 = suffix
  python3 "$ROOT/docs/checkpoints/terrain-hump/means.py" "$OUT" "$TAG" "$1" $SHOTS
}
for v in noon dusk; do
  if [ "$v" = noon ]; then sfx=""; flags=(); else sfx="-dusk"; flags=(--kit_hooks dusk); fi
  run "$sfx" "${flags[@]+"${flags[@]}"}"
  if ! dark "$sfx"; then echo "retrying $v once"; run "$sfx" "${flags[@]+"${flags[@]}"}"; dark "$sfx"; fi
done
