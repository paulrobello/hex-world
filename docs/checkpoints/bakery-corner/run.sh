#!/bin/bash
# bakery_shop_b back-left roof corner, before and after (owner A2, 2026-10-20: pull it in to 0.12 m from
# the hex border, as bakery_shop). Play scenario (corner.json): a bakery tile at (0, 1), whose hero slot
# draws bakery_shop_b (hex_core::hero_piece), and a meadow at (0, 0) across the border the corner faces.
# Shots: straight down over the corner with the hex grid overlay (the white line is the tile border), an
# oblique close-up from the meadow with the grid, and play distance (~40 m) without it, all in photo mode
# (dof off), which hides the slot outlines and the ghost that follow the real pointer. "Before" runs the
# same binary with cwd on a copy-on-write clone of assets/ holding BEFORE's bakery_shop_b files (native
# asset loading resolves cwd/assets); "after" runs on the worktree's assets/. Then border.mjs measures
# each LOD's distance to the border on both GLBs and sheet.py builds the sheet. Native at 1600x1000 with
# tilt-shift off; every PNG's mean is checked and a black run retried once.
# Usage: bash docs/checkpoints/bakery-corner/run.sh [BEFORE_REV] (from the repo root; default 900ac57).
set -euo pipefail
DOCS=docs/checkpoints/bakery-corner
BEFORE_REV=${1:-900ac57}
WORK=target/bakery-corner
SHOTS="corner-top corner-close corner-play"
P=assets/kit/p1/bakery_shop_b
cargo build -p hex-app --release
rm -rf "$WORK" && mkdir -p "$WORK/before" "$WORK/after"
cp -cR assets "$WORK/before/assets"
for f in bakery_shop_b.glb bakery_shop_b.masks.png bakery_shop_b.masks.bc7.ktx2 bakery_shop_b.masks.astc.ktx2; do
  git show "$BEFORE_REV:$P/$f" > "$WORK/before/$P/$f"
done
shoot() {  # $1 = before|after, $2 = cwd
  rm -f "$2"/target/shots/corner-*.png
  (cd "$2" && caffeinate -u -d -t 600 "$OLDPWD/target/release/hex-world" --scenario play \
    --script "$OLDPWD/$DOCS/corner.json" --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed) \
    > "$WORK/$1.log" 2>&1
  for n in $SHOTS; do cp "$2/target/shots/$n.png" "$WORK/$1/$n.png"; done
}
both() {
  shoot before "$WORK/before"
  shoot after .
}
uptime
both
if ! { python3 docs/checkpoints/bakery_shop_b/means.py "$WORK/before" $SHOTS && \
       python3 docs/checkpoints/bakery_shop_b/means.py "$WORK/after" $SHOTS; }; then
  echo "retrying once"
  both
  python3 docs/checkpoints/bakery_shop_b/means.py "$WORK/before" $SHOTS
  python3 docs/checkpoints/bakery_shop_b/means.py "$WORK/after" $SHOTS
fi
uptime
for s in before after; do
  for n in $SHOTS; do ffmpeg -loglevel error -y -i "$WORK/$s/$n.png" -q:v 2 "$DOCS/$s-$n.jpg"; done
done
node "$DOCS/border.mjs" "before=$WORK/before/$P/bakery_shop_b.glb" "after=$P/bakery_shop_b.glb" \
  "bakery_shop=assets/kit/p1/bakery_shop/bakery_shop.glb" > "$DOCS/border.json"
python3 "$DOCS/sheet.py"
echo "wrote $DOCS/"
