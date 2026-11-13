#!/bin/bash
# One fixed-clock flat-parity run with the 2026-10-30 evidence flags.
# usage: run.sh <tree> <binary> <label> [script] [extra args...]
# <tree> is the cwd (its assets/); the binary embeds its own tree's data paths.
# Output: /tmp/hw-parity/runs/<label>/{flat-*.png,flat-state.json,run.log,load.txt,sha.txt}
set -u
tree=$1; bin=$2; label=$3; script=${4:-/tmp/hw-parity/flat-parity.json}
shift 3; [ $# -gt 0 ] && shift
out=/tmp/hw-parity/runs/$label
rm -rf "$out"; mkdir -p "$out"
cd "$tree" || exit 1
{ echo "tree $tree"; shasum "$bin"; echo "script $script"; echo "extra $*"; } > "$out/sha.txt"
rm -f target/shots/flat-*.png target/shots/flat-state.json
echo "load_before $(sysctl -n vm.loadavg) $(date -u +%H:%M:%SZ)" > "$out/load.txt"
caffeinate -d -u "$bin" --scenario play --script "$script" --size 1600x1000 --seed 1 --tier high \
  --fps off --tilt off --clock fixed --water_clock fixed --offscreen on --grass normal "$@" > "$out/run.log" 2>&1
ec=$?
echo "EXIT=$ec" >> "$out/run.log"
echo "load_after $(sysctl -n vm.loadavg) $(date -u +%H:%M:%SZ)" >> "$out/load.txt"
cp target/shots/flat-*.png target/shots/flat-state.json "$out/" 2>/dev/null
echo "$label EXIT=$ec $(tr '\n' ' ' < "$out/load.txt")"
exit $ec
