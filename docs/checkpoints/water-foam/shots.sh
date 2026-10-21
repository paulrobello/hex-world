#!/usr/bin/env bash
# Water-foam checkpoint captures (water card criterion 3): runs beach.json and river.json in play at
# 1280x800 with fixed clocks and tilt-shift off, then writes JPEG stills and MP4 clips into
# docs/checkpoints/water-foam/<label>/ (before or after). Extra arguments pass to hex-app (--tier low).
#   bash docs/checkpoints/water-foam/shots.sh after [--tier low]
set -euo pipefail
label="${1:?label (before|after)}"
shift
docs=docs/checkpoints/water-foam
out="$docs/$label"
mkdir -p "$out" target/shots
python3 "$docs/make_scripts.py"
rm -f target/shots/wf-*.png
# HEXW_BIN=target/release/hex-world reuses a built binary (shader edits load from assets/ at run time).
run=(cargo run -p hex-app --release --)
[[ -n "${HEXW_BIN:-}" ]] && run=("$HEXW_BIN")
for scene in beach river bend60; do
    bash scripts/shots/run.sh "${run[@]}" --scenario play --script "$docs/$scene.json" \
        --size 1280x800 --seed 1 --fps off --tilt off --clock fixed --water_clock fixed "$@"
done
stills=$(cd target/shots && ls wf-*.png | grep -v -- '-[0-9][0-9][0-9]\.png$' | sed 's/\.png$//')
# shellcheck disable=SC2086
bash scripts/shots/run.sh --jpegs "$out" $stills
for c in wf-beach-motion wf-river-fork-motion wf-river-bend60-motion; do
    ffmpeg -loglevel error -y -framerate 30 -i "target/shots/$c-%03d.png" -c:v libx264 -pix_fmt yuv420p -crf 22 "$out/$c.mp4"
    for f in 000 045; do
        ffmpeg -loglevel error -y -i "target/shots/$c-$f.png" -q:v 2 "$out/$c-$f.jpg"
    done
done
rm -f target/shots/wf-*-motion-*.png
echo "wrote $out/"
