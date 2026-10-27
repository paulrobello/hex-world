#!/usr/bin/env bash
# Low-camera profile on MiniMac (docs/perf/low-camera/README.md): ROUNDS interleaved rounds, each a
# web round (e2e/frame-pacing.mjs lc_* arms) then a native round (e2e/perf-low-camera-native.mjs),
# with the arm order rotated per round. Every run is inside the host-load guard.
# Run from the shipped directory (dist/, e2e/ with node_modules, web/ui/src/, assets/,
# target/release/hex-world). usage: bash low-camera-minimac.sh [rounds] [first-round]
set -u
export PATH="/opt/homebrew/bin:$PATH"
ulimit -n 10240
ROUNDS=${1:-3}
FIRST=${2:-1}
WEB=(lc_base lc_timing lc_shadows_off lc_cascades_2 lc_post_none lc_grass_off lc_medium lc_low lc_1080)
NATIVE=(base_off base shadows_off cascades_2 post_none grass_off medium low s1080)
rot() { local k=$1; shift; local a=("$@"); local n=${#a[@]}; local out=(); for ((i = 0; i < n; i++)); do out+=("${a[$(((i + k) % n))]}"); done; (IFS=,; echo "${out[*]}"); }
mkdir -p results
for ((r = FIRST; r < FIRST + ROUNDS; r++)); do
  echo "round $r web $(date -u +%H:%M:%S) $(uptime)"
  PACING_GUARD=1 PACING_ARMS=$(rot $((r - 1)) "${WEB[@]}") PACING_OUT=results/web-$r.json \
    caffeinate -d -u node e2e/frame-pacing.mjs dist
  echo "round $r native $(date -u +%H:%M:%S) $(uptime)"
  ROUNDS=1 ARMS=$(rot $((r - 1)) "${NATIVE[@]}") caffeinate -d -u node e2e/perf-low-camera-native.mjs . results/native-$r.json
done
echo "done $(date -u +%H:%M:%S) $(uptime)"
