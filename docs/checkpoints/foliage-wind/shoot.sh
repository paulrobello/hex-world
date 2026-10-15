#!/usr/bin/env bash
# Foliage wind evidence (`make shots-foliage-wind`): fixed-clock frame pairs 50 frames (0.83 s of wind
# time) apart of a pine forest, the hero oak, a river reed bed and wildflower clumps, and 180 frames
# (3 s, a gust change) apart of the ground where the oak and the pines cast their shadows, plus a
# difference image per pair (bright = moved), so planted bases read dark and swaying tops and their
# shadows bright. Play scenario for the tiles, kit scenario for the wildflowers.
# Usage: docs/checkpoints/foliage-wind/shoot.sh <hex-world binary>
set -euo pipefail
bin="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
docs="$(cd "$(dirname "$0")" && pwd)"
mkdir -p target/shots
rm -f target/shots/foliage-*.png target/shots/foliage-*.json
echo "load before: $(uptime)"
"$bin" --scenario play --script "$docs/play.json" --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed
"$bin" --script "$docs/kit.json" --size 1600x1000 --seed 1 --fps off --tilt off --clock fixed
echo "load after: $(uptime)"
for n in forest oak shadow reeds flowers; do
  a="target/shots/foliage-$n-t1.png"
  b="target/shots/foliage-$n-t2.png"
  test -s "$a" && test -s "$b"
  ffmpeg -loglevel error -y -i "$a" -q:v 3 "$docs/$n-t1.jpg"
  ffmpeg -loglevel error -y -i "$b" -q:v 3 "$docs/$n-t2.jpg"
  magick "$a" "$b" -compose difference -composite -colorspace gray -auto-level -quality 85 "$docs/$n-diff.jpg"
  magick "$a" "$b" -compose difference -composite -colorspace gray -format "$n diff mean %[fx:mean] max %[fx:maxima]\n" info:
done
# The oak's ground shadow alone (no canopy in the crop), levelled on its own: the moving edge band.
magick target/shots/foliage-shadow-t1.png target/shots/foliage-shadow-t2.png -crop 560x760+440+60 +repage \
  -compose difference -composite -colorspace gray -auto-level -quality 85 "$docs/shadow-edge-diff.jpg"
cp target/shots/foliage-state.json "$docs/foliage-state.json"
