#!/usr/bin/env bash
# Builds the web dist of one historical commit from its archive tar, recompiling every workspace crate.
set -u
sha=$1
repo=/Users/probello/Repos/hex-world/.claude/worktrees/agent-a021ff452a3ba3e3b
dir=/tmp/a021/named-$sha
rm -rf "$dir"; mkdir -p "$dir"
tar -x -f /tmp/a021/$sha.tar -C "$dir"
cp -cR "$repo/target" "$dir/target"
cp -cR "$repo/web/ui/node_modules" "$dir/web/ui/node_modules"
sed -i "" "s|data-wasm-opt=\"z\" />|data-wasm-opt=\"z\" data-wasm-opt-params=\"-g\" data-keep-debug />|" "$dir/index.html"; grep -n "data-keep-debug" "$dir/index.html"
find "$dir/crates" "$dir/assets/shaders" "$dir/data" -type f -exec touch {} +
cargo clean --manifest-path "$dir/Cargo.toml" --target wasm32-unknown-unknown --profile wasm-release \
  -p hex-core -p hex-sim -p hex-assets -p hex-play-state -p hex-render -p hex-play -p hex-app
cd "$dir" && make build-web
echo "BUILD_EXIT=$?"
