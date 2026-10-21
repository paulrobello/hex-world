#!/usr/bin/env bash
# The "before" side of the road_bend_60 inner-corner crease pair (card 01a13887daba755ab6e588eabf7242f1).
#
#   bash docs/checkpoints/road_bend_60-crease/prefix.sh <out dir> <glb path>...   (from the repo root)
#
# The crease was filed on 2026-10-13, before 69f7f6f gave every road_verge loop a +Z normal. This
# rebuilds that state with today's binary and kit layout: it clones assets/ into
# target/rb60-prefix/ (cp -c, copy on write), overwrites the named GLBs there with their versions
# from 69f7f6f^ (git archive), and runs crease-kit.json through scripts/shots/piece.py with that
# directory as the working directory, because native runs load `<cwd>/assets` (hex-app
# asset_plugin) and have no asset-root flag; `make shots-piece` always runs from the repo root.
# piece.py still does the caffeinate, black-frame check and retry, and records host load.
#
#   before/       road_straight_edge.glb and road_bend_60_center.glb from 69f7f6f^ (the 10-13 state)
#   only-stub/    road_straight_edge.glb only
#   only-centre/  road_bend_60_center.glb only
# The run writes every frame of crease-kit.json; only the frames sheet.py shows are committed.
set -euo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
out="$1"
shift
case "$out" in /*) ;; *) out="$root/$out" ;; esac
work="$root/target/rb60-prefix"
cd "$root"
cargo build -p hex-app --release
rm -rf "$work"
mkdir -p "$work/target/shots"
cp -cR "$root/assets" "$work/assets"
git archive --output="$work/prefix.tar" 69f7f6f^ "$@"
tar -xf "$work/prefix.tar" -C "$work"
for p in "$@"; do
  if cmp -s "$root/$p" "$work/$p"; then
    echo "error: $p is unchanged since 69f7f6f^" >&2
    exit 1
  fi
done
cd "$work"
python3 "$root/scripts/shots/piece.py" --script "$root/docs/checkpoints/road_bend_60-crease/crease-kit.json" \
  --out "$out" --bin "$root/target/release/hex-world" -- --clock fixed
# The record names the checkout; keep it repo-relative like `make shots-piece` records.
sed -i '' "s#$root/##g" "$out/crease-kit-shots.json"
