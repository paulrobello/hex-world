#!/usr/bin/env bash
# Measurement-only variants of a built web dist (never shipped): copies <dist> to <out> with
# assets/shaders/kit_prepass.wesl's `STILL_CASCADE_TEXEL_M` set to <texel> (a negative value: no
# directional cascade sways foliage; 1e9: every cascade does), then re-versions the copy so it has
# its own build id. The kit shaders are assets, so this needs no wasm rebuild.
# usage: make-variant.sh <dist> <out> <texel>
set -euo pipefail
src=$1
out=$2
texel=$3
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
rm -rf "$out"
cp -cR "$src" "$out"
f="$out/assets/shaders/kit_prepass.wesl"
sed -i "" "s|^const STILL_CASCADE_TEXEL_M: f32 = .*;|const STILL_CASCADE_TEXEL_M: f32 = $texel;|" "$f"
grep -q "^const STILL_CASCADE_TEXEL_M: f32 = $texel;" "$f" || { echo "variant patch did not apply"; exit 1; }
node "$repo/scripts/web/version-dist.mjs" "$out"
