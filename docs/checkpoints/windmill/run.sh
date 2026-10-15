#!/bin/bash
# Windmill checkpoint (P3): the windmill kit piece in the kit scenario, its rotor turning.
# Runs windmill.json natively at 1600x1000 with tilt-shift off and `--clock fixed` (the app clock
# steps exactly 1/60 s a frame, and the rotor angle is a function of that clock only), so every run
# replays identically. windmill-t1 and windmill-t2 are 60 frames (1 s) apart: at 4 rpm the sails
# turn 24° between them. Then a close-up of the cap, hub and fantail, and the windmill in the layout.
# Native shots can come back black (idle display, high load): each PNG's mean is checked and the run
# retried once. Usage: docs/checkpoints/windmill/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/windmill
OUT=target/shots
SHOTS="windmill-t1 windmill-t2 windmill-closeup windmill-context"
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/windmill-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/windmill.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/windmill-shots.log 2>&1
}
dark() {
  for n in $SHOTS; do
    m=$(magick "$OUT/$n.png" -format '%[fx:mean]' info:)
    awk -v m="$m" 'BEGIN { exit !(m < 0.02) }' && { echo "$n.png is black (mean $m)"; return 0; }
  done
  return 1
}
run
if dark; then echo "retrying once"; run; dark && exit 1; fi
for n in $SHOTS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
echo "wrote $DOCS/"
