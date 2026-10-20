#!/bin/bash
# Terrain-hump review shots (card "Terrain caps: remove the hump along tile borders"). Plays
# hump-play.json natively at 1600x1000, tilt-shift off, fixed clock: a 3x3 meadow field, a 3x3
# forest field, a road row, a beach and two ocean tiles, then the same poses at noon and with
# `--kit_hooks dusk` (the low sun, grazing light). Run once before and once after the base rebuild.
# Native shots can come back black (idle display, high load): each PNG's mean is checked and the
# run retried once.
# Usage: docs/checkpoints/terrain-hump/run.sh <tag> <out-dir> [assets-cwd]
#   tag        prefix for the copied PNGs (before, after, or a candidate name)
#   out-dir    where the PNGs land (<out-dir>/<tag>-<shot>[-dusk].png)
#   assets-cwd optional directory holding an `assets/` tree to run against (a scratch clone with
#              candidate GLBs); defaults to the repo root
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
TAG=$1
OUT=$2
CWD=${3:-$ROOT}
SCRIPT="$ROOT/docs/checkpoints/terrain-hump/hump-play.json"
BIN="$ROOT/target/release/hex-world"
SHOTS="hump-orbit hump-orbit-nograss hump-low-nograss hump-low hump-forest hump-seam-forest hump-seam-road hump-seam-beach hump-eye hump-eye-b"
mkdir -p "$OUT" "$CWD/target/shots"
run() {  # $1 = suffix, rest = extra flags
  local sfx=$1
  shift
  rm -f "$CWD"/target/shots/hump-*.png
  (cd "$CWD" && caffeinate -u -d -t 900 "$BIN" --scenario play --script "$SCRIPT" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed "$@" > "$OUT/$TAG$sfx.log" 2>&1)
  for n in $SHOTS; do cp "$CWD/target/shots/$n.png" "$OUT/$TAG-$n$sfx.png"; done
  cp "$CWD/target/shots/hump-state.json" "$OUT/$TAG-state$sfx.json" 2>/dev/null || true
}
dark() {  # $1 = suffix
  python3 "$ROOT/docs/checkpoints/terrain-hump/means.py" "$OUT" "$TAG" "$1" $SHOTS
}
for v in noon dusk; do
  if [ "$v" = noon ]; then sfx=""; flags=(); else sfx="-dusk"; flags=(--kit_hooks dusk); fi
  run "$sfx" "${flags[@]+"${flags[@]}"}"
  if ! dark "$sfx"; then echo "retrying $v once"; run "$sfx" "${flags[@]+"${flags[@]}"}"; dark "$sfx"; fi
done
