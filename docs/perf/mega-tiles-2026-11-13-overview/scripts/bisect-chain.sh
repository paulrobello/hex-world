#!/bin/bash
# Waits for the 11:45Z window, then runs one flat-parity-late capture per bisect binary,
# controls twice. No run starts after 13:38Z.
set -u
log() { echo "$(date -u +%H:%M:%SZ) $*"; }
start=$(date -u +%s)
until [ "$(date -u +%H%M)" -ge 1145 ]; do
  if [ $(( $(date -u +%s) - start )) -gt 7200 ]; then log "deadline waiting"; exit 2; fi
  sleep 5
done
log "window open, load $(sysctl -n vm.loadavg)"
for spec in "$@"; do
  sha=${spec%%:*}; label=${spec#*:}
  if [ "$(date -u +%H%M)" -ge 1338 ]; then log "skip $label: past 13:38Z"; continue; fi
  [ -x /tmp/ovb/bin/$sha ] || { log "no binary $sha"; continue; }
  /tmp/ovb/run.sh "$sha" "$label"
done
log "chain done, load $(sysctl -n vm.loadavg)"
