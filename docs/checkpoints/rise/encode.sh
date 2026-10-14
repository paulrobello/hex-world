#!/usr/bin/env bash
# Turns the frames of a `make shots-rise` run (target/shots/rise-*.png, aff-*.png) into the checkpoint:
# a stage sheet and an MP4 per rise case, the fast-forward and undo clips with their key stills, the
# reduced-motion still and the affordance stills, all next to this script. Frame fNNN of a case shows
# the rise at t = (NNN + 2) / 60 s (see rise.mjs).
set -euo pipefail
shots=target/shots
out=docs/checkpoints/rise
jpg() { ffmpeg -loglevel error -y -i "$1" -q:v 2 "$2"; }

# Stage sheets: t = 0.10 0.30 0.50 0.70 0.90 1.10 1.30 1.60 2.20 s, left to right, top to bottom.
frames=(004 016 028 040 052 064 076 094 130)
for c in meadow house river ocean; do
  args=()
  for f in "${frames[@]}"; do
    test -s "$shots/rise-$c-f$f.png" || { echo "missing rise-$c-f$f.png"; exit 1; }
    args+=(-i "$shots/rise-$c-f$f.png")
  done
  ffmpeg -loglevel error -y "${args[@]}" -filter_complex \
    "[0][1][2][3][4][5][6][7][8]xstack=inputs=9:layout=0_0|w0_0|w0+w1_0|0_h0|w0_h0|w0+w1_h0|0_h0+h1|w0_h0+h1|w0+w1_h0+h1,scale=1920:-2" \
    -q:v 3 "$out/rise-$c-stages.jpg"
  ffmpeg -loglevel error -y -framerate 60 -i "$shots/rise-$c-f%03d.png" -vf scale=960:-2 \
    -c:v libx264 -pix_fmt yuv420p -crf 22 "$out/rise-$c.mp4"
done

# Fast-forward: the forest mid-rise, then the frame after the meadow lands (forest at its end state).
jpg "$shots/rise-ff-a-f028.png" "$out/rise-ff-1-forest-mid-rise.jpg"
jpg "$shots/rise-ff-b-f000.png" "$out/rise-ff-2-forest-jumped-to-end.jpg"
jpg "$shots/rise-ff-b-f039.png" "$out/rise-ff-3-meadow-rising.jpg"
# Undo mid-rise: the road mid-rise, sinking, gone.
jpg "$shots/rise-undo-a-f038.png" "$out/rise-undo-1-mid-rise.jpg"
jpg "$shots/rise-undo-b-f008.png" "$out/rise-undo-2-sinking.jpg"
jpg "$shots/rise-undo-b-f039.png" "$out/rise-undo-3-gone.jpg"
for clip in ff undo; do
  tmp=$(mktemp -d)
  i=0
  for f in "$shots"/rise-$clip-a-f*.png "$shots"/rise-$clip-b-f*.png; do
    cp "$f" "$tmp/$(printf %03d $i).png"
    i=$((i + 1))
  done
  ffmpeg -loglevel error -y -framerate 60 -i "$tmp/%03d.png" -vf scale=960:-2 \
    -c:v libx264 -pix_fmt yuv420p -crf 22 "$out/rise-$clip.mp4"
  rm -rf "$tmp"
done
jpg "$shots/rise-reduced-motion.png" "$out/rise-reduced-motion.jpg"
for n in aff-01-valid-ghost aff-02-incompatible-edge aff-03-refused-pulse aff-04-bulldoze-removable \
  aff-05-bulldoze-split aff-06-bulldoze-refused-pulse; do
  jpg "$shots/$n.png" "$out/$n.jpg"
done
cp "$shots"/rise-state-*.json "$shots"/aff-state-*.json "$out/"
echo "wrote $out/"
