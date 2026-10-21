"""Black-frame check for scripts/shots/run.sh: `piece.frame_problem` on every PNG named.

Prints one line per bad frame (missing, empty, unreadable, or black: RGB mean below 0.02 of full
scale) to stderr and exits 1 if any frame is bad, 0 otherwise.

Usage: python3 scripts/shots/check_frames.py <png>...
"""

from __future__ import annotations

import pathlib
import sys

from piece import frame_problem


def main(paths: list[str]) -> int:
    bad = 0
    for p in paths:
        problem, _ = frame_problem(pathlib.Path(p))
        if problem:
            hint = ": idle display or high load" if problem.startswith("black") else ""
            print(f"{p} is {problem}{hint}", file=sys.stderr)
            bad = 1
    return bad


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
