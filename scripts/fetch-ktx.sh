#!/usr/bin/env bash
# Installs KTX-Software 4.4.2 tools into ./tools (gitignored). macOS arm64 only (local asset builds).
set -euo pipefail
# The pinned package is the macOS arm64 build: say so before any download (make kit-compress,
# kit-material and kit-fixture run this first). HEXW_HOST overrides `uname -sm` for the self-test.
host="${HEXW_HOST:-$(uname -s) $(uname -m)}"
if [ "$host" != "Darwin arm64" ]; then
  echo "fetch-ktx: KTX-Software 4.4.2 is pinned to its macOS arm64 build, so kit-compress runs on macOS arm64 only and this host is $host. Run make kit-compress, kit-material and kit-fixture on an Apple Silicon Mac." >&2
  exit 1
fi
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/tools/ktx-4.4.2"
bin="$dest/KTX-Software-4.4.2-Darwin-arm64-tools.pkg/Payload/usr/local/bin"
lib="$dest/KTX-Software-4.4.2-Darwin-arm64-library.pkg/Payload/usr/local/lib"
if [ ! -x "$bin/ktx" ]; then
  mkdir -p "$root/tools"
  pkg="$root/tools/KTX-Software-4.4.2-Darwin-arm64.pkg"
  curl -fL --max-time 120 https://github.com/KhronosGroup/KTX-Software/releases/download/v4.4.2/KTX-Software-4.4.2-Darwin-arm64.pkg -o "$pkg"
  rm -rf "$dest"
  pkgutil --expand-full "$pkg" "$dest"
fi
printf 'export PATH="%s:$PATH"\nexport DYLD_LIBRARY_PATH="%s${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"\n' "$bin" "$lib" > "$root/tools/ktx-env.sh"
. "$root/tools/ktx-env.sh"
ktx --version
