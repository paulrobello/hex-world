"""Run one manifest-driven shots target (`make shots-<name>`, scripts/shots/targets.json).

Looks the name up in the manifest, builds `hex-app` once, then runs `piece.py` per run
of the row. It adds no capture logic of its own: piece.py's black-frame check, one
re-run, JPEG copies, dumps and shots.json report are the whole capture behavior. Extra
flags after the name go to every run, e.g. `python3 scripts/shots/target.py cars --tier medium`.
"""

from __future__ import annotations

import argparse
import pathlib
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import piece  # noqa: E402
import targets  # noqa: E402

BUILD = ["cargo", "build", "-p", "hex-app", "--release"]


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("name")
    ap.add_argument("extra", nargs="*", help="hex-world flags appended to every run")
    a = ap.parse_args(argv)
    try:
        row = targets.by_name()[a.name]
    except KeyError:
        known = ", ".join(targets.by_name())
        print(f"unknown shots target {a.name} (known: {known})", file=sys.stderr)
        return 2
    build = subprocess.run(BUILD)
    if build.returncode:
        return build.returncode
    for run in row["runs"]:
        rc = piece.main(targets.piece_args(run, a.extra))
        if rc:
            return rc
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
