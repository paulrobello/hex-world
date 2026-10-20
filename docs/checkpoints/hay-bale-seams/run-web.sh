#!/bin/bash
# The run.sh cameras in the web build (headless GPU Chromium through e2e/shots.mjs, 1280x800, tilt
# off), before and after on one build: `after` shoots dist/ as built, `before` a clone of dist/ with
# the round bales' maps from git revision <rev> (old_maps.py). Native shots of a window that another
# app covers come back black (pitfalls.md), and a headless browser has no window to cover.
# Usage (repo root, after make build-web): bash docs/checkpoints/hay-bale-seams/run-web.sh <rev> <scratch dir>
# <rev> is the revision whose maps are "before": the sheet used 29a51b6, the last commit before the fix.
set -euo pipefail
REV=$1
SCR=$2
DOCS=docs/checkpoints/hay-bale-seams
SHOTS="seam-round-barrel seam-round-cap seam-roundb-sun seam-roundb-near"
rm -rf "$SCR/dist-before"
cp -cR dist "$SCR/dist-before"
python3 "$DOCS/old_maps.py" "$REV" "$SCR/dist-before"
for tag in before after; do
  d=dist
  [ "$tag" = before ] && d="$SCR/dist-before"
  mkdir -p "$SCR/$tag-web"
  node e2e/shots.mjs "$d" "$DOCS/shots-web.json" "tilt=off"
  for n in $SHOTS; do
    for lod in auto lod1; do cp "e2e/test-results/web-$n-$lod.png" "$SCR/$tag-web/$n-$lod.png"; done
  done
  python3 docs/checkpoints/hay_variants/means.py "$SCR/$tag-web" $(for n in $SHOTS; do echo "$n-auto $n-lod1"; done)
done
echo "wrote $SCR/before-web and $SCR/after-web"
