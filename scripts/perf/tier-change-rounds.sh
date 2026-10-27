#!/usr/bin/env bash
# Tier-change stall, before/after (docs/checkpoints/tier-change/README.md). Run from a directory
# holding `before/` and `after/` dists, `e2e/` (with node_modules) and `web/ui/src/`, on a quiet host.
# Each session is one `e2e/tier-stall.mjs` round: a fresh browser profile, the 208-tile water world,
# then high>medium, medium>high, high>medium. A cold session also starts with the test browser's
# macOS Metal shader cache empty: the cache lives per app bundle under DARWIN_USER_CACHE_DIR, not in
# the profile, so the original is moved aside for the whole run and put back on exit.
# usage: bash tier-change-rounds.sh <rounds> [arm ...]   arms: name=dist[?query] (default before, after)
set -euo pipefail
ulimit -n 10240
ROUNDS="${1:-3}"
shift || true
ARMS=("$@")
if [ "${#ARMS[@]}" -eq 0 ]; then ARMS=(before=before after=after); fi
CACHE="$(getconf DARWIN_USER_CACHE_DIR)"
HELPER="${CACHE%/}/com.google.chrome.for.testing.helper"
SAVED="${HELPER}.tier-change-saved"
mkdir -p results logs
if [ -e "$SAVED" ]; then
  echo "refusing: $SAVED exists (an earlier run did not restore it)" >&2
  exit 2
fi
if [ -e "$HELPER" ]; then mv "$HELPER" "$SAVED"; fi
restore() {
  rm -rf "$HELPER"
  if [ -e "$SAVED" ]; then mv "$SAVED" "$HELPER"; fi
}
trap restore EXIT

session() { # name dist query mode round
  local name="$1" dist="$2" query="$3" mode="$4" round="$5"
  if [ "$mode" = cold ]; then rm -rf "$HELPER"; fi
  local out="results/${name}-${mode}-r${round}.json"
  echo "$(date -u +%H:%M:%S) ${name} ${mode} round ${round} (load $(sysctl -n vm.loadavg))"
  TIER_ROUNDS=1 TIER_QUERY="$query" caffeinate -d -u node e2e/tier-stall.mjs "$dist" "$out" \
    > "logs/${name}-${mode}-r${round}.log" 2>&1 || echo "  failed: see logs/${name}-${mode}-r${round}.log"
}

n=${#ARMS[@]}
for ((r = 1; r <= ROUNDS; r++)); do
  # Per arm a cold session (it empties the cache), then a warm one, which reuses what the cold
  # one compiled. The arm order rotates by one each round.
  for ((k = 0; k < n; k++)); do
    arm="${ARMS[$(((k + r - 1) % n))]}"
    name="${arm%%=*}"
    spec="${arm#*=}"
    dist="${spec%%\?*}"
    query=""
    if [[ "$spec" == *\?* ]]; then query="${spec#*\?}"; fi
    for mode in cold warm; do
      session "$name" "$dist" "$query" "$mode" "$r"
    done
  done
done
echo "$(date -u +%H:%M:%S) done"
