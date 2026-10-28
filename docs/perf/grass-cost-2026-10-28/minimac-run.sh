#!/usr/bin/env bash
# Runs on MiniMac inside the booked window, from ~/perf/gc, one phase at a time.
# usage: minimac-run.sh m2|grass|native|sway
# Layout: arms/{main,c032d97,4897f2e,nosway} (web dists), e2e/ (npm ci), web/ui/src, docs/perf/... (save, scripts),
# docs/checkpoints/scatter/scatter-mixed.json, assets/, target/release/hex-world (main, native).
export PATH=/opt/homebrew/opt/node@24/bin:/opt/homebrew/bin:$PATH
ulimit -n 10240
cd ~/perf/gc || exit 1
mkdir -p results
node --version
SAVE=docs/perf/post-arc001-2026-10-16/local-world.hexworld
m2() {
  WORLDS=world2066 ROUNDS="${M2_ROUNDS:-4}" WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
    c032d97=arms/c032d97 b4897f2e=arms/4897f2e main=arms/main results/close-camera-web.json > results/close-camera-web.log 2>&1
  echo "EXIT=$?" >> results/close-camera-web.log
}
grass() {
  PERF_OUT=results/grass-web.json GRASS_LOAD=$SAVE GRASS_ROUNDS="${GRASS_ROUNDS:-3}" GRASS_ARMS="${GRASS_ARMS:-off,sparse,normal,lush}" \
    caffeinate -d -u node e2e/perf-grass.mjs arms/main > results/grass-web.log 2>&1
  echo "EXIT=$?" >> results/grass-web.log
}
sway() {
  WORLDS=world2066 ROUNDS="${SWAY_ROUNDS:-3}" WARMUP=1 caffeinate -d -u node e2e/perf-close-camera.mjs \
    "sway=arms/main?grass=normal" "nosway=arms/nosway?grass=normal" results/sway-web.json > results/sway-web.log 2>&1
  echo "EXIT=$?" >> results/sway-web.log
}
native() {
  ROUNDS="${NATIVE_ROUNDS:-3}" caffeinate -d -u node docs/perf/grass-cost-2026-10-28/grass-native.mjs \
    results/grass-native.json > results/grass-native.log 2>&1
  echo "EXIT=$?" >> results/grass-native.log
}
case "$1" in
  m2) m2 ;;
  grass) grass ;;
  sway) sway ;;
  native) native ;;
esac
echo "DONE $1 $(date -u +%H:%M:%SZ)" >> results/driver.log
