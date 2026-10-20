#!/bin/bash
# train_carriage_b checkpoint: the play shots (play.json, scenario=play, tilt off, fixed clock so the
# trains stand where the cameras expect them: three trains on the rail evidence loop with its spur run
# west to (-9, -1), each the engine, train_carriage and train_carriage_b in its own livery), then the
# EEVEE stills (stills.sh) and the review sheet (sheet.py). Needs integration.patch applied (trains draw
# train_carriage_b only with it). Native shots can come back black (idle display, high load): each
# PNG's mean is checked and the run retried once.
# Usage: bash docs/checkpoints/train_carriage_b/run.sh (from the repo root).
set -euo pipefail
DOCS=docs/checkpoints/train_carriage_b
OUT=target/shots
SHOTS="tcb-play tcb-scale tcb-close-sun-crimson tcb-close-sun tcb-close-shade tcb-eye"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/tcb-*.png
  caffeinate -u -d -t 900 target/release/hex-world --scenario play --script "$DOCS/play.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/tcb-shots.log 2>&1
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
bash "$DOCS/stills.sh"
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
