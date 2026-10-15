#!/usr/bin/env bash
# Turns the frames of a `make shots-bulldoze-dust` run (target/shots/dust-*.png) into the checkpoint:
# the bulldoze stills (before, mid-cloud, cloud thinning, after), a stage sheet, the bulldoze and undo
# clips, the undo puff still and the reduced-motion still, all next to this script. Frame fNNN of a
# case is captured NNN frames (NNN / 60 s) after the bulldoze or undo (see bulldoze-dust.mjs).
set -euo pipefail
shots=target/shots
out=docs/checkpoints/bulldoze-dust
jpg() {
  test -s "$1" || { echo "missing $1"; exit 1; }
  ffmpeg -loglevel error -y -i "$1" -q:v 2 "$2"
}

jpg "$shots/dust-bulldoze-before.png" "$out/dust-1-before.jpg"
jpg "$shots/dust-bulldoze-f016.png" "$out/dust-2-mid-cloud.jpg"
jpg "$shots/dust-bulldoze-f080.png" "$out/dust-3-thinning.jpg"
jpg "$shots/dust-bulldoze-f199.png" "$out/dust-4-after.jpg"
jpg "$shots/dust-undo-f012.png" "$out/dust-undo-puff.jpg"
jpg "$shots/dust-reduced-motion.png" "$out/dust-reduced-motion.jpg"

# Stage sheet: before, then 0.13 0.40 0.67 1.07 1.60 s after the bulldoze, left to right, top to
# bottom: the same phases of the cloud as the first look's 0.17 to 2.00 s, at 80% of its duration.
args=(-i "$shots/dust-bulldoze-before.png")
for f in 008 024 040 064 096; do
  test -s "$shots/dust-bulldoze-f$f.png" || { echo "missing dust-bulldoze-f$f.png"; exit 1; }
  args+=(-i "$shots/dust-bulldoze-f$f.png")
done
ffmpeg -loglevel error -y "${args[@]}" -filter_complex \
  "[0][1][2][3][4][5]xstack=inputs=6:layout=0_0|w0_0|w0+w1_0|0_h0|w0_h0|w0+w1_h0,scale=1920:-2" \
  -q:v 3 "$out/dust-stages.jpg"

for clip in bulldoze undo; do
  ffmpeg -loglevel error -y -framerate 60 -i "$shots/dust-$clip-f%03d.png" -vf scale=960:-2 \
    -c:v libx264 -pix_fmt yuv420p -crf 22 "$out/dust-$clip.mp4"
done
cp "$shots"/dust-state-*.json "$out/"
echo "wrote $out/"
