#!/bin/bash
# Waits for the 09:55Z load-sensitive window (deadline 10:05Z), then runs the interleaved parity chain.
# No run starts after 10:22Z (each run is under a minute), so the chain ends by 10:25Z.
# Log: /tmp/hw-parity/capture.log; runs in /tmp/hw-parity/runs/<label>
set -u
FIX=/Users/probello/Repos/hex-world/.claude/worktrees/agent-ae7322c2c6b46f949
BR=/tmp/hw-parity/branch
MAIN=/tmp/hw-parity/main
B=/tmp/hw-parity/bin
S=/tmp/hw-parity/flat-parity.json
L=/tmp/hw-parity/flat-parity-lod0.json
log() { echo "$(date -u +%H:%M:%SZ) $*"; }
start=$(date -u +%s)
until [ "$(date -u +%H%M)" -ge 0955 ]; do
  if [ $(( $(date -u +%s) - start )) -gt 14400 ]; then log "deadline passed waiting for 09:55Z"; exit 2; fi
  sleep 10
done
if [ "$(date -u +%H%M)" -ge 1005 ]; then log "started too late for the window"; exit 2; fi
log "capture window open, load $(sysctl -n vm.loadavg)"
run() { # tree bin label [script]
  if [ "$(date -u +%H%M)" -ge 1022 ]; then log "skip $3: past 10:22Z"; return 9; fi
  /tmp/hw-parity/run.sh "$1" "$2" "$3" "${4:-$S}"
}
# core arms, interleaved
for i in 1 2 3; do
  run "$FIX" "$B/fix" "fix-$i"
  run "$BR" "$B/branch" "branch-$i"
  run "$MAIN" "$B/main" "main-$i"
done
# instrumented (hold lengths, command frames)
for i in 1 2; do
  run "$FIX" "$B/fix-instr" "fixi-$i"
  run "$BR" "$B/branch-instr" "branchi-$i"
done
# forced LOD0
for i in 1 2; do
  run "$FIX" "$B/fix" "fix-lod0-$i" "$L"
  run "$BR" "$B/branch" "branch-lod0-$i" "$L"
done
# extra core replicates while the window lasts
for i in 4 5 6; do
  run "$FIX" "$B/fix" "fix-$i" || break
  run "$BR" "$B/branch" "branch-$i" || break
  run "$MAIN" "$B/main" "main-$i" || break
done
log "chain done, load $(sysctl -n vm.loadavg)"
