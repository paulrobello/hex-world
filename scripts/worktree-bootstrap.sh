#!/usr/bin/env bash
# One-command bootstrap for a fresh git worktree of this repo (asset agents, feature worktrees).
# Clones the main checkout's gitignored build state (target/, node_modules) with APFS copy-on-write,
# installs the KTX tools and npm deps that main may not have, and is a no-op on a second run.
# Usage: bash scripts/worktree-bootstrap.sh [--web]   (--web also installs e2e deps for browser gates)
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
common="$(cd "$root" && git rev-parse --path-format=absolute --git-common-dir)"
main="$(dirname "$common")"
web=0
[ "${1:-}" = "--web" ] && web=1

say() { printf 'bootstrap: %s\n' "$*"; }

if [ "$root" = "$main" ]; then
  say "this is the main checkout ($main); nothing to clone"
else
  say "worktree $root, main checkout $main"
  if [ -d "$root/target" ]; then
    say "target/ present, kept"
  elif [ -d "$main/target" ]; then
    cp -cR "$main/target" "$root/target"
    # hex-core and hex-assets embed absolute data paths at build time; rebuild them for this tree.
    cargo clean --manifest-path "$root/Cargo.toml" -p hex-core -p hex-assets
    say "target/ cloned from main (copy-on-write), hex-core and hex-assets cleaned"
  else
    say "main has no target/; the first cargo build compiles from scratch"
  fi
fi

npm_dir() {
  local dir="$1"
  if [ -f "$root/$dir/node_modules/.npm-ci.stamp" ] && [ "$root/$dir/node_modules/.npm-ci.stamp" -nt "$root/$dir/package-lock.json" ]; then
    say "$dir/node_modules current, kept"
    return
  fi
  if [ "$root" != "$main" ] && [ ! -d "$root/$dir/node_modules" ] && [ -d "$main/$dir/node_modules" ] \
    && cmp -s "$root/$dir/package-lock.json" "$main/$dir/package-lock.json"; then
    cp -cR "$main/$dir/node_modules" "$root/$dir/node_modules"
    touch "$root/$dir/node_modules/.npm-ci.stamp"
    say "$dir/node_modules cloned from main (same lockfile)"
    return
  fi
  (cd "$root/$dir" && npm ci --no-audit --no-fund --loglevel=error)
  touch "$root/$dir/node_modules/.npm-ci.stamp"
  say "$dir/node_modules installed (npm ci)"
}

npm_dir web/ui
npm_dir scripts/assets
[ "$web" = 1 ] && npm_dir e2e

if [ -x "$root/tools/ktx-4.4.2/KTX-Software-4.4.2-Darwin-arm64-tools.pkg/Payload/usr/local/bin/ktx" ]; then
  say "KTX tools present, kept"
elif [ "$root" != "$main" ] && [ -d "$main/tools/ktx-4.4.2" ]; then
  mkdir -p "$root/tools"
  cp -cR "$main/tools/ktx-4.4.2" "$root/tools/ktx-4.4.2"
  bash "$root/scripts/fetch-ktx.sh" > /dev/null
  say "KTX tools cloned from main"
else
  bash "$root/scripts/fetch-ktx.sh" > /dev/null
  say "KTX tools installed"
fi

say "done; HEAD $(cd "$root" && git log --oneline -1)"
