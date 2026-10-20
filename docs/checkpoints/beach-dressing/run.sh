#!/bin/bash
# Beach dressing checkpoint (owner D3, 2026-10-20): play shots before and after the scatter landed.
# Usage: docs/checkpoints/beach-dressing/run.sh <before|after> <binary> <root>; the kit cell: <binary> --script
# docs/checkpoints/beach-dressing/bd-kit.json --size 1600x1000 --seed 1 --fps off --tilt off (from the root).
#   <root> is the directory the binary runs in (it loads <root>/assets): for "before", an extract of
#   the base commit's assets (git archive HEAD assets) and the base commit's release binary; for
#   "after", the worktree with integration.patch applied and its rebuilt binary.
# Natively at 1600x1000, seed 1, tilt-shift off, fixed clocks. Window screenshots come back black on
# this host today, so every still is an offscreen photo_export into <root>/target/shots/. Each PNG's
# mean is checked (a black frame has mean 0) and the run is retried once.
set -u
DOCS=$(cd "$(dirname "$0")" && pwd)
WHICH=$1
BIN=$2
ROOT=$3
cd "$ROOT" || exit 2
mkdir -p target/shots
run() {
  for w in mouth coast; do
    caffeinate -u -d -t 900 "$BIN" --scenario play --script "$DOCS/bd-$WHICH-$w.json" --size 1600x1000 --seed 1 \
      --fps off --tilt off --clock fixed --water_clock fixed > "target/bd-$WHICH-$w.log" 2>&1
    echo "$w exit $?"
  done
}
run
if ! python3 "$DOCS/means.py" target/shots "bd-$WHICH-"; then
  echo "retrying once"
  run
  python3 "$DOCS/means.py" target/shots "bd-$WHICH-" || exit 1
fi
