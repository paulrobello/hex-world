#!/bin/bash
# The EEVEE half of run.sh alone (no engine build): the textured stills into docs/checkpoints/train.
set -euo pipefail
DOCS=docs/checkpoints/train
BLENDER=${BLENDER:-/Applications/Blender.app/Contents/MacOS/Blender}
mkdir -p target/train-stills
for v in engine carriage straight bend close scale paint-a paint-b; do
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P "$DOCS/still.py" -- "target/train-stills/still-$v.png" "$v" \
    > "target/train-stills/still-$v.log" 2>&1
  ffmpeg -loglevel error -y -i "target/train-stills/still-$v.png" -q:v 2 "$DOCS/still-$v.jpg"
done
echo "stills written"
