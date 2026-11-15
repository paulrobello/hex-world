#!/bin/bash
# usage: build.sh <sha>...  builds hex-app release at each sha (+ a09e359's capture.rs when older),
# copies binary to /tmp/ovb/bin/<sha>, exports assets to /tmp/ovb/run-<sha>/assets.
set -u
W=.
mkdir -p /tmp/ovb/bin
cd "$W" || exit 1
for sha in "$@"; do
  echo "== $sha $(date -u +%H:%M:%SZ) load $(sysctl -n vm.loadavg)"
  git checkout -q -f --detach "$sha" || exit 1
  if ! git merge-base --is-ancestor a09e359 "$sha"; then
    git checkout -q a09e359 -- crates/hex-app/src/capture.rs
  fi
  CARGO_BUILD_JOBS=8 cargo build -q -p hex-app --release > /tmp/ovb/build-$sha.log 2>&1; ec=$?
  echo "build $sha EXIT=$ec $(date -u +%H:%M:%SZ)"
  if [ $ec -ne 0 ]; then tail -20 /tmp/ovb/build-$sha.log; git checkout -q -f --detach "$sha"; continue; fi
  cp target/release/hex-world /tmp/ovb/bin/$sha
  rm -rf /tmp/ovb/run-$sha; mkdir -p /tmp/ovb/run-$sha
  git archive "$sha" assets | tar -x -C /tmp/ovb/run-$sha
  git checkout -q -f --detach "$sha"
done
shasum /tmp/ovb/bin/* > /tmp/ovb/bin.sha1
