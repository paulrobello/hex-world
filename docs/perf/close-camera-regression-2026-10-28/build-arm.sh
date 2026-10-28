#!/usr/bin/env bash
# Builds the web dist of one commit from `git archive`, recompiling every workspace crate for wasm.
# usage: build-arm.sh <name> <tar> [nosway]
#   nosway: the sway ablation (docs/perf/grass-cost-2026-10-28/): every kit slot is built without its
#   `wind`, so no material carries FLAG_WIND_TREE/PLANT or a wind amplitude and the vertex stages skip
#   the foliage wind. Measurement only, never shipped.
set -u
name=$1
tar=$2
repo=${HEXW_REPO:-/Users/probello/Repos/hex-world/.claude/worktrees/agent-a6dbf1f948f80d6bd}
dir=/tmp/cc-arms/$name
rm -rf "$dir"; mkdir -p "$dir"
tar -x -f "$tar" -C "$dir"
cp -cR "$repo/target" "$dir/target"
cp -cR "$repo/web/ui/node_modules" "$dir/web/ui/node_modules"
if [ "${3:-}" = nosway ]; then
  f="$dir/crates/hex-render/src/kit_material.rs"
  sed -i "" 's|wind: slot.wind.map(\|a\| (a, p.kind == PieceKind::Tree)),|wind: None,|' "$f"
  if grep -q "slot.wind.map" "$f"; then echo "nosway patch did not apply"; exit 1; fi
fi
find "$dir/crates" "$dir/assets/shaders" "$dir/data" -type f -exec touch {} +
cargo clean --manifest-path "$dir/Cargo.toml" --target wasm32-unknown-unknown --profile wasm-release \
  -p hex-core -p hex-sim -p hex-assets -p hex-play-state -p hex-render -p hex-play -p hex-app
cd "$dir" && make build-web
echo "BUILD_EXIT=$?"
