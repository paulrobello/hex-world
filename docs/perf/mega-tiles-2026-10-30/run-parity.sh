#!/bin/bash
# Native flat-world pixel parity (no massif): one fixed-clock run of flat-parity.json with a checkout's
# release binary, from that checkout (its assets/), PNGs and state copied to <out>/<label>.
# GPU capture: run only inside a booked load-sensitive window.
# usage: run-parity.sh <checkout> <label> <out> <expected-head> <expected-binary-sha1>
# Refuses to run when the checkout's HEAD or its binary is not the one expected.
set -u
co=$1; label=$2; out=$3/$label; want_head=$4; want_bin=$5
script=$(cd "$(dirname "$0")" && pwd)/flat-parity.json
mkdir -p "$out"
cd "$co" || exit 1
head=$(git rev-parse HEAD); bin=$(shasum target/release/hex-world | cut -d' ' -f1)
case "$head" in "$want_head"*) ;; *) echo "HEAD $head is not $want_head"; exit 3 ;; esac
case "$bin" in "$want_bin"*) ;; *) echo "binary $bin is not $want_bin"; exit 3 ;; esac
git rev-parse HEAD > "$out/sha.txt"
shasum target/release/hex-world >> "$out/sha.txt"
rm -f target/shots/flat-*.png target/shots/flat-state.json
echo "load_before $(sysctl -n vm.loadavg) $(date -u +%H:%M:%SZ)" > "$out/load.txt"
caffeinate -d -u ./target/release/hex-world --scenario play --script "$script" --size 1600x1000 --seed 1 --tier high \
  --fps off --tilt off --clock fixed --water_clock fixed --offscreen on --grass normal > "$out/run.log" 2>&1
ec=$?
echo "EXIT=$ec" >> "$out/run.log"
echo "load_after $(sysctl -n vm.loadavg) $(date -u +%H:%M:%SZ)" >> "$out/load.txt"
cp target/shots/flat-*.png target/shots/flat-state.json "$out/"
exit $ec
