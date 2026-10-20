#!/bin/bash
# Round hay bale sector seams (owner Q55): in-engine close-ups of hay_bale_round (on the oak's meadow
# (0, 1)) and hay_bale_round_b (the hay-variants farm (1, 5)) from fixed cameras, natively at
# 1600x1000 with tilt-shift off, once at the automatic LOD (LOD0 at these distances) and once with
# `--debug lod1` (Force LOD 1), plus the texture-space seam measurement (dump_glb.mjs + measure.py)
# of the GLBs and maps in assets/kit/p1/. Run it with tag `before` on the old assets and `after` on
# the rebuilt ones, then sheet.py builds sheet.jpg from both. Shots are not timing measurements:
# they run under caffeinate without a load wait, each PNG's mean is checked and a dark run retried.
# Usage (repo root, after cargo build -p hex-app --release): bash docs/checkpoints/hay-bale-seams/run.sh <tag> <scratch dir>
set -euo pipefail
TAG=$1
SCR=$2
DOCS=docs/checkpoints/hay-bale-seams
SHOTS="seam-round-barrel seam-round-cap seam-roundb-sun seam-roundb-near"
mkdir -p "$SCR/$TAG"
for lod in auto lod1; do
  extra=()
  [ "$lod" = lod1 ] && extra=(--debug lod1)
  ok=0
  for attempt in 1 2; do
    caffeinate -u -d -t 600 target/release/hex-world --script "$DOCS/shots.json" --size 1600x1000 \
      --seed 1 --fps off --tilt off "${extra[@]}" > "$SCR/$TAG/kit-$lod.log" 2>&1
    if python3 docs/checkpoints/hay_variants/means.py target/shots $SHOTS; then ok=1; break; fi
    echo "dark shots, retrying ($attempt)"
  done
  # A window behind another app's window captures black on every retry: use run-web.sh then.
  [ "$ok" = 1 ] || { echo "shots still dark: the window is probably covered; see run-web.sh"; exit 1; }
  for n in $SHOTS; do cp "target/shots/$n.png" "$SCR/$TAG/$n-$lod.png"; done
done
for spec in hay_bale_round:0:0 hay_bale_round_b:1:0.6; do
  IFS=: read -r p axis mid <<< "$spec"
  node "$DOCS/dump_glb.mjs" "assets/kit/p1/$p/$p.glb" "$SCR/$TAG/$p.dump.json"
  python3 "$DOCS/measure.py" "$SCR/$TAG/$p.dump.json" "assets/kit/p1/$p" "$p" "$DOCS/measure-$TAG-$p.json" "$axis" "$mid"
done
echo "wrote $SCR/$TAG and $DOCS/measure-$TAG-*.json"
