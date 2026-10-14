#!/bin/bash
# Car kit checkpoint (P2, cars-kit): the three car kit pieces with runtime per-instance paint.
# Runs cars-kit.json (day) and cars-kit-night.json (`--kit_hooks night`) natively at 1600x1000 with
# tilt-shift off and `--clock fixed` (one 20 Hz sim tick every third frame, so every run replays
# identically), then copies JPEG stills, the state dumps and the car batch census next to this file.
# Layout: a three-armed road tree from a hub junction at (1,0), six house clusters at the leaves, so
# about ten cars drive at once. The close-ups sample the hub every 30 frames; CLOSEUPS names the
# frames kept per body type. Usage: docs/checkpoints/cars-kit/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/cars-kit
OUT=target/shots
CLOSEUPS="sedan:${SEDAN:-41} van:${VAN:-32}"
NIGHT=${NIGHT_SHOT:-07}
mkdir -p "$OUT"
rm -f "$OUT"/cars-kit-*.png "$OUT"/cars-kit-*.json
cargo build -p hex-app --release
for run in day night; do
  script=$DOCS/cars-kit.json hooks=()
  [ "$run" = night ] && script=$DOCS/cars-kit-night.json hooks=(--kit_hooks night)
  target/release/hex-world --scenario play --script "$script" --size 1600x1000 --seed 1 --fps off \
    --tilt off --clock fixed "${hooks[@]}" > "target/cars-kit-$run.log" 2>&1
done
jpg() { ffmpeg -loglevel error -y -i "$OUT/$1.png" -q:v 2 "$DOCS/$2.jpg"; }
jpg cars-kit-01-busy cars-kit-01-busy
jpg cars-kit-02-hub cars-kit-02-hub
for c in $CLOSEUPS; do jpg "cars-kit-closeup-${c#*:}" "cars-kit-03-closeup-${c%%:*}"; done
jpg cars-kit-night-hub cars-kit-04-night-hub
jpg "cars-kit-night-closeup-$NIGHT" cars-kit-05-night-closeup
cp "$OUT"/cars-kit-00-empty.json "$OUT"/cars-kit-01-busy.json "$OUT"/cars-kit-03-closeups.json "$OUT"/cars-kit-06-night-state.json "$DOCS/"
grep -h 'cars census\|kit car pieces ready' target/cars-kit-day.log > "$DOCS/batch-census.log"
echo "wrote $DOCS/"
