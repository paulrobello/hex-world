#!/bin/bash
# Forest-floor variants checkpoint (PRD §6 Variety): fern_clump_b, fern_clump_c and log_fallen_b in the
# kit scenario on the forest at (-3, 1) (kit_scene FOREST_FLOOR_VARIANTS), shot with
# forest_floor_variants.json natively at 1600x1000, tilt-shift off: play distance, an overview with an
# original fern and log beside them, the log and each fern close up on the sun side (+x/+z, the sun is
# at (200, 400, 150)) and the shade side, eye level at 1.7 m, and the variants beside a spruce for
# scale. Then the textured EEVEE stills beside
# the originals (still.py) and the review sheet (sheet.py). Native shots can come back black (idle
# display, high load): each PNG's mean is checked and the run retried once.
# Usage: docs/checkpoints/forest_floor_variants/run.sh (from the repo root; the stills need every
# piece staged in target/kit-stage/: make kit-build PIECE=<id> for the variants and fern_clump, and
# HEXW_SKIP_BAKE=1 make kit-build PIECE=log_fallen, whose committed maps the stills bind).
set -euo pipefail
DOCS=docs/checkpoints/forest_floor_variants
OUT=target/shots
SHOTS="var-play var-overview-sun var-log-sun var-log-shade var-fern-b-sun var-fern-b-shade var-fern-c-sun var-fern-c-shade var-eye var-scale"
STILLS="ferns-lineup logs-lineup fern_clump_b-eevee-low_side fern_clump_c-eevee-three_quarter log_fallen_b-eevee-break log_fallen_b-eevee-moss_flank"
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p "$OUT"
cargo build -p hex-app --release
run() {
  rm -f "$OUT"/var-*.png
  caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/forest_floor_variants.json" --size 1600x1000 \
    --seed 1 --fps off --tilt off --clock fixed > target/floor-variants-shots.log 2>&1
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
"$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "$OUT" > target/floor-variants-still.log 2>&1
for n in $STILLS; do ffmpeg -loglevel error -y -i "$OUT/$n.png" -q:v 2 "$DOCS/$n.jpg"; done
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
