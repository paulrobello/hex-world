#!/bin/bash
# hay_bale_round_b apron seam before/after in the web build (headless GPU Chromium through
# e2e/shots.mjs, tilt off), Force LOD 2 then 0 inside the script. Both sets run on clones of one
# dist/: `before` with round_b's files from git revision <rev>, `after` with the working tree's.
# Usage (repo root, after make build-web): bash docs/checkpoints/hay-bale-b-apron/run-web.sh <rev> <scratch dir>
# The sheet used 758cc39, the seam-fix commit this change builds on.
set -euo pipefail
REV=$1
SCR=$2
DOCS=docs/checkpoints/hay-bale-b-apron
SHOTS="apron-sun apron-near apron-high-a apron-high-b"
for tag in before after; do
  rm -rf "$SCR/dist-$tag"
  cp -cR dist "$SCR/dist-$tag"
done
python3 "$DOCS/piece_files.py" "$REV" "$SCR/dist-before"
python3 "$DOCS/piece_files.py" tree "$SCR/dist-after"
for tag in before after; do
  mkdir -p "$SCR/$tag-web"
  node e2e/shots.mjs "$SCR/dist-$tag" "$DOCS/shots-web.json" "tilt=off"
  for n in $SHOTS; do
    for lod in lod2 lod0; do cp "e2e/test-results/web-$n-$lod.png" "$SCR/$tag-web/$n-$lod.png"; done
  done
  python3 docs/checkpoints/hay_variants/means.py "$SCR/$tag-web" $(for n in $SHOTS; do echo "$n-lod2 $n-lod0"; done)
done
echo "wrote $SCR/before-web and $SCR/after-web"
