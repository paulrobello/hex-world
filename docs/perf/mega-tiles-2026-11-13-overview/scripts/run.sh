#!/bin/bash
# One fixed-clock flat-parity run (2026-10-30 evidence flags), overview probe.
# usage: run.sh <sha> <label> [script]
# cwd /tmp/ovb/run-<sha> (its assets/); output /tmp/ovb/runs/<label>/
set -u
sha=$1; label=$2; script=${3:-/tmp/ovb/flat-parity-late.json}
tree=/tmp/ovb/run-$sha; bin=/tmp/ovb/bin/$sha
out=/tmp/ovb/runs/$label
rm -rf "$out"; mkdir -p "$out" "$tree/target/shots"
cd "$tree" || exit 1
{ echo "tree $tree"; shasum "$bin"; echo "script $script"; } > "$out/sha.txt"
rm -f target/shots/flat-*.png target/shots/flat-state.json
echo "load_before $(sysctl -n vm.loadavg) $(date -u +%H:%M:%SZ)" > "$out/load.txt"
caffeinate -d -u "$bin" --scenario play --script "$script" --size 1600x1000 --seed 1 --tier high \
  --fps off --tilt off --clock fixed --water_clock fixed --offscreen on --grass normal > "$out/run.log" 2>&1
ec=$?
echo "EXIT=$ec" >> "$out/run.log"
echo "load_after $(sysctl -n vm.loadavg) $(date -u +%H:%M:%SZ)" >> "$out/load.txt"
cp target/shots/flat-*.png target/shots/flat-state.json "$out/" 2>/dev/null
echo "$label EXIT=$ec $(tr '\n' ' ' < "$out/load.txt")"
exit $ec
