#!/usr/bin/env bash
# Pixel differences between two arms' fixed-clock shots (minimac-run.sh shots), per camera: the
# mean and maximum absolute difference (0-1, grey of the per-channel difference), the pixels that
# differ by more than 2% and 8% of full scale, and a levelled difference image (bright = moved).
# usage: diff.sh <dir-a> <dir-b> <label> <out-dir>
set -euo pipefail
a=$1
b=$2
label=$3
out=$4
mkdir -p "$out"
for cam in close play overview; do
  pa="$a/sway-$cam.png"
  pb="$b/sway-$cam.png"
  stats=$(magick "$pa" "$pb" -compose difference -composite -colorspace gray -format "%[fx:mean] %[fx:maxima]" info:)
  px2=$(magick compare -metric AE -fuzz 2% "$pa" "$pb" null: 2>&1 || true)
  px8=$(magick compare -metric AE -fuzz 8% "$pa" "$pb" null: 2>&1 || true)
  echo "$label $cam mean_max $stats px_over_2pct $px2 px_over_8pct $px8"
  magick "$pa" "$pb" -compose difference -composite -colorspace gray -auto-level -quality 85 "$out/$label-$cam-diff.jpg"
done
