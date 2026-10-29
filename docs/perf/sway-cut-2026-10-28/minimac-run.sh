#!/usr/bin/env bash
# Runs on MiniMac inside the booked window, from ~/perf/sw, one phase at a time.
# usage: minimac-run.sh web|shots <arm>...
# Layout: arms/<name> (web dists), native/<name>/{hex-world,assets} (release binaries, each with the
# assets it was built with), e2e/ (npm ci), web/ui/src, docs/perf/post-arc001-2026-10-16/local-world.hexworld,
# docs/checkpoints/scatter/scatter-mixed.json and this directory's shots.json.
export PATH=/opt/homebrew/opt/node@24/bin:/opt/homebrew/bin:$PATH
ulimit -n 10240
cd ~/perf/sw || exit 1
mkdir -p results
node --version
SAVE=docs/perf/post-arc001-2026-10-16/local-world.hexworld
phase=$1
shift
web() {
  # Each argument is an arm name; every arm runs at grass=normal like the baseline's sway arms.
  local arms=()
  for a in "$@"; do arms+=("$a=arms/$a?grass=normal"); done
  WORLDS=world2066 ROUNDS="${SWAY_ROUNDS:-3}" WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
    "${arms[@]}" "results/${OUT:-sway-web}.json" > "results/${OUT:-sway-web}.log" 2>&1
  echo "EXIT=$?" >> "results/${OUT:-sway-web}.log"
}
shots() {
  # One fixed-clock native run per arm (--clock fixed: 1/60 s per frame, so the wind pose at each
  # screenshot is the same frame in every arm); offscreen because MiniMac's console is locked.
  for a in "$@"; do
    for rep in ${SHOT_REPS:-1}; do
      local out="results/shots/$a-$rep"
      mkdir -p "$out"
      (cd "native/$a" && rm -rf target/shots && echo "load before: $(uptime)" > "../../$out/load.txt" &&
        caffeinate -d -u ./hex-world --scenario play --load "../../$SAVE" --script ../../shots.json \
          --size 2560x1440 --seed 1 --fps off --tilt off --clock fixed --offscreen on --grass normal \
          > "../../$out/run.log" 2>&1
        echo "EXIT=$?" >> "../../$out/run.log"
        echo "load after: $(uptime)" >> "../../$out/load.txt"
        cp target/shots/*.png "../../$out/" 2>/dev/null)
    done
  done
}
case "$phase" in
  web) web "$@" ;;
  shots) shots "$@" ;;
esac
echo "DONE $phase $* $(date -u +%H:%M:%SZ)" >> results/driver.log
