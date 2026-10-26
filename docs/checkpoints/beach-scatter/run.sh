#!/bin/bash
# Beach scatter checkpoint (owner 2026-10-21): the beach-dressing coast and river-mouth worlds
# photographed before and after each coast tile draws 0 to 6 dressing pieces.
# Usage (from the repo root): docs/checkpoints/beach-scatter/run.sh <before|after> <binary>
# Natively at 1600x1000, seed 1, tilt-shift off, fixed clocks; stills are offscreen photo_exports
# into target/shots/, checked for black frames, then written here as JPEGs with the state dumps.
set -u
DOCS=$(cd "$(dirname "$0")" && pwd)
WHICH=$1
BIN=$2
python3 "$DOCS/scripts.py" "$WHICH" || exit 1
mkdir -p target/shots
rm -f target/shots/bs-"$WHICH"-*
for w in coast mouth; do
  caffeinate -u -d -t 900 "$BIN" --scenario play --script "$DOCS/bs-$WHICH-$w.json" --size 1600x1000 --seed 1 \
    --fps off --tilt off --clock fixed --water_clock fixed > "target/bs-$WHICH-$w.log" 2>&1
  echo "$w exit $?"
done
python3 "$DOCS/../beach-dressing/means.py" target/shots "bs-$WHICH-" || exit 1
for f in target/shots/bs-"$WHICH"-*.png; do
  ffmpeg -loglevel error -y -i "$f" -q:v 2 "$DOCS/$(basename "${f%.png}").jpg" || exit 1
done
cp target/shots/bs-"$WHICH"-*-state.json "$DOCS/"
rm -f "$DOCS/bs-$WHICH-coast.json" "$DOCS/bs-$WHICH-mouth.json"
echo "wrote $DOCS/"
