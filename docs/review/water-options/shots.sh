#!/usr/bin/env bash
# Water-options review captures (owner A3 Q1/Q2, 2026-10-21): runs beach.json, river.json and
# bend60.json in play at 1280x800 on the default grade (rich-3) with fixed clocks and tilt-shift
# off, once per option index with `--water_look trail=<x>,churn=<g>`, and keeps each run's PNGs in
# target/wo/opt-<n>/. build.py then picks the stills, strips and clips for the review page.
#   bash docs/review/water-options/shots.sh [--scenes "beach river bend60"] [option index ...]
set -euo pipefail
docs=docs/review/water-options
TRAIL=(0.95 0.9 0.8 0.7 0.6)
CHURN=(0.25 0.5 1.0 2.0 3.5)
python3 "$docs/make_scripts.py"
cargo build -p hex-app --release
run=(target/release/hex-world)
scenes=(beach river bend60)
if [[ "${1:-}" == --scenes ]]; then
    read -ra scenes <<<"$2"
    shift 2
fi
opts=("$@")
[[ ${#opts[@]} -eq 0 ]] && opts=(1 2 3 4 5)
for n in "${opts[@]}"; do
    i=$((n - 1))
    look="trail=${TRAIL[$i]},churn=${CHURN[$i]}"
    rm -f target/shots/wo-*.png
    # Under heavy host load a capture can read back black frames; run.sh rejects the run, and the
    # fixed clocks make a retry produce the same frames.
    for scene in "${scenes[@]}"; do
        for attempt in 1 2 3 4; do
            bash scripts/shots/run.sh "${run[@]}" --scenario play --script "$docs/$scene.json" \
                --size 1280x800 --seed 1 --fps off --grade rich-3 --tilt off --clock fixed \
                --water_clock fixed --water_look "$look" && break
            [[ $attempt == 4 ]] && exit 1
            echo "option $n $scene: retry $attempt"
        done
    done
    mkdir -p "target/wo/opt-$n"
    mv -f target/shots/wo-*.png "target/wo/opt-$n/"
    echo "option $n ($look): $(ls "target/wo/opt-$n" | wc -l) frames"
done
