#!/usr/bin/env bash
set -euo pipefail

# The black-frame check is piece.py's frame_problem (Pillow), shared with make shots-piece.
check_frames="$(dirname "${BASH_SOURCE[0]}")/check_frames.py"

# --state-only: the capture writes only state dumps; any PNG it does write is still checked.
state_only=0
if [[ "${1:-}" == --state-only ]]; then
    state_only=1
    shift
fi

if [[ "${1:-}" == --jpegs ]]; then
    out="$2"
    shift 2
    # Validate the entire batch before replacing any committed evidence.
    frames=()
    for name in "$@"; do frames+=("target/shots/$name.png"); done
    python3 "$check_frames" ${frames[@]+"${frames[@]}"}
    for name in "$@"; do
        ffmpeg -loglevel error -y -i "target/shots/$name.png" -q:v 2 "$out/$name.jpg"
    done
else
    mkdir -p target/shots
    marker=$(mktemp target/shots/.capture-XXXXXX)
    trap 'rm -f "$marker"' EXIT
    caffeinate -d -u "$@"
    frames=()
    while IFS= read -r -d '' file; do
        frames+=("$file")
    done < <(find target/shots -name '*.png' -newer "$marker" -print0)
    if [[ ${#frames[@]} -gt 0 ]]; then
        python3 "$check_frames" "${frames[@]}"
    elif [[ "$state_only" == 0 ]]; then
        printf 'capture produced no PNG frames\n' >&2
        exit 1
    fi
fi
