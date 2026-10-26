#!/bin/bash
# Riverbank scatter checkpoint (owner 2026-10-26): a river stretch photographed before and after the
# bank stones and reeds moved out of the river centre pieces into seeded runtime scatter (0 to 4),
# and after only, the bend, mouths, straights and springs of the beach-dressing mouth world.
# Usage (from the repo root): docs/checkpoints/riverbank-scatter/run.sh <before|after> <binary>
# Natively at 1600x1000, seed 1, tilt-shift off, fixed clocks; stills are offscreen photo_exports
# into target/shots/, checked for black frames, then written here as JPEGs with the state dump.
set -u
DOCS=$(cd "$(dirname "$0")" && pwd)
WHICH=$1
BIN=$2
python3 "$DOCS/scripts.py" "$WHICH" || exit 1
mkdir -p target/shots
rm -f target/shots/rs-"$WHICH"-*
caffeinate -u -d -t 900 "$BIN" --scenario play --script "$DOCS/rs-$WHICH-stretch.json" --size 1600x1000 --seed 1 \
  --fps off --tilt off --clock fixed --water_clock fixed > "target/rs-$WHICH-stretch.log" 2>&1
echo "stretch exit $?"
if [ "$WHICH" = after ]; then
  caffeinate -u -d -t 900 "$BIN" --scenario play --script "$DOCS/rs-after-family.json" --size 1600x1000 --seed 1 \
    --fps off --tilt off --clock fixed --water_clock fixed > target/rs-after-family.log 2>&1
  echo "family exit $?"
  rm -f "$DOCS/rs-after-family.json"
fi
python3 "$DOCS/../beach-dressing/means.py" target/shots "rs-$WHICH-" || exit 1
for f in target/shots/rs-"$WHICH"-*.png; do
  ffmpeg -loglevel error -y -i "$f" -q:v 2 "$DOCS/$(basename "${f%.png}").jpg" || exit 1
done
cp target/shots/rs-"$WHICH"-*-state.json "$DOCS/"
rm -f "$DOCS/rs-$WHICH-stretch.json"
echo "wrote $DOCS/"
