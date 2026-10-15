#!/usr/bin/env bash
# Cattail close-up evidence: a play scenario with river_straight at axial (1, 1) between the river
# mouth chain and the spring, shot from 3 m above a reed clump on its SE bank, at eye level (1.7 m)
# along that bank, back-lit from the channel side, and from above. Fixed clocks, tilt off.
# Usage: docs/checkpoints/cattails/shoot.sh <label> <run-dir>
# run-dir holds the binary (hex-world) and the assets/ it loads, e.g. a snapshot of main's build.
set -euo pipefail
label="$1"
run="$(cd "$2" && pwd)"
docs="$(cd "$(dirname "$0")" && pwd)"
cd "$run"
# Host load guard (owner rule): wait up to 15 minutes for the 1-minute load to reach 15, then run once.
deadline=$(( $(date +%s) + 900 ))
until awk -v l="$(sysctl -n vm.loadavg | awk '{print $2}')" 'BEGIN{exit !(l <= 15)}'; do
  if [ "$(date +%s)" -ge "$deadline" ]; then echo "load guard: deadline reached, running anyway"; break; fi
  sleep 10
done
mkdir -p target/shots
rm -f target/shots/cattails-*.png target/shots/cattails-*.json
echo "load before: $(uptime)"
caffeinate -u -d -t 600 ./hex-world --scenario play --script "$docs/cattails.json" --size 1600x1000 \
  --seed 1 --fps off --tilt off --clock fixed
echo "load after: $(uptime)"
for n in cattails-closeup cattails-eye-level cattails-closeup-backlit cattails-overview; do
  test -s "target/shots/$n.png"
  magick "target/shots/$n.png" -format "$n mean %[fx:mean]\n" info:
  ffmpeg -loglevel error -y -i "target/shots/$n.png" -q:v 2 "$docs/${n/cattails-/cattails-$label-}.jpg"
done
cp target/shots/cattails-state.json "$docs/cattails-$label-state.json"
