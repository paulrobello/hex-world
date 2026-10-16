#!/usr/bin/env bash
set -euo pipefail

check_frame() {
    local file="$1" mean
    test -s "$file" || { printf 'missing %s\n' "$file" >&2; exit 1; }
    mean=$(magick "$file" -format '%[fx:mean]' info:)
    awk -v m="$mean" 'BEGIN { exit !(m > 0.02) }' || {
        printf '%s is black (mean %s): idle display or high load\n' "$file" "$mean" >&2
        exit 1
    }
}

if [[ "${1:-}" == --jpegs ]]; then
    out="$2"
    shift 2
    # Validate the entire batch before replacing any committed evidence.
    for name in "$@"; do check_frame "target/shots/$name.png"; done
    for name in "$@"; do
        ffmpeg -loglevel error -y -i "target/shots/$name.png" -q:v 2 "$out/$name.jpg"
    done
else
    mkdir -p target/shots
    marker=$(mktemp target/shots/.capture-XXXXXX)
    trap 'rm -f "$marker"' EXIT
    caffeinate -d -u "$@"
    frames=0
    while IFS= read -r -d '' file; do
        check_frame "$file"
        frames=$((frames + 1))
    done < <(find target/shots -name '*.png' -newer "$marker" -print0)
    if [[ "$frames" == 0 ]]; then
        printf 'capture produced no PNG frames\n' >&2
        exit 1
    fi
fi
