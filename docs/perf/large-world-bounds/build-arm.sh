#!/usr/bin/env bash
# Builds the web dist of one commit from its archive tar, recompiling every workspace crate.
# usage: build-arm.sh <name> <tar> [named]   (named keeps the wasm name section for V8 profiles)
set -u
name=$1
tar=$2
repo=/Users/probello/Repos/hex-world/.claude/worktrees/agent-ae6a74790ce21ac0c
dir=/tmp/bounds-arms/$name
rm -rf "$dir"; mkdir -p "$dir"
tar -x -f "$tar" -C "$dir"
cp -cR "$repo/target" "$dir/target"
cp -cR "$repo/web/ui/node_modules" "$dir/web/ui/node_modules"
if [ "${3:-}" = named ]; then
  sed -i "" "s|data-wasm-opt=\"z\" />|data-wasm-opt=\"z\" data-wasm-opt-params=\"-g\" data-keep-debug />|" "$dir/index.html"
  grep -c "data-keep-debug" "$dir/index.html"
fi
find "$dir/crates" "$dir/assets/shaders" "$dir/data" -type f -exec touch {} +
cargo clean --manifest-path "$dir/Cargo.toml" --target wasm32-unknown-unknown --profile wasm-release \
  -p hex-core -p hex-sim -p hex-assets -p hex-play-state -p hex-render -p hex-play -p hex-app
cd "$dir" && make build-web
echo "BUILD_EXIT=$?"
