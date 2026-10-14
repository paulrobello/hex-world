"""The local .blend store (owner ruling O2). Stdlib only, so system python3, make and Blender share it.

Every .blend lives in the MAIN checkout's gitignored assets-source/blend/<id>/, shared by all
worktrees: <git common dir>/.. resolves to the main checkout from any worktree. HEXW_BLEND_STORE
overrides the store root.

CLI: python3 scripts/blender/lib/paths.py store | blend <id> | key <path>
"""
from __future__ import annotations

import os
import pathlib
import subprocess
import sys

ENV = "HEXW_BLEND_STORE"
_HERE = pathlib.Path(__file__).resolve().parent


def blend_store() -> pathlib.Path:
    """Absolute store root: $HEXW_BLEND_STORE, else <main checkout>/assets-source/blend."""
    override = os.environ.get(ENV)
    if override:
        return pathlib.Path(override)
    common = subprocess.run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
                            cwd=_HERE, capture_output=True, text=True, check=True).stdout.strip()
    return pathlib.Path(common).parent / "assets-source" / "blend"


def blend_path(piece: str) -> pathlib.Path:
    """The piece's high-poly source: <store>/<piece>/<piece>_high.blend."""
    return blend_store() / piece / f"{piece}_high.blend"


def blend_key(path: str | os.PathLike[str]) -> str:
    """Store-relative POSIX key for provenance; raises ValueError for a path outside the store."""
    return pathlib.Path(path).resolve().relative_to(blend_store().resolve()).as_posix()


def _main(argv: list[str]) -> int:
    if argv[1:2] == ["store"]:
        print(blend_store())
    elif len(argv) == 3 and argv[1] == "blend":
        print(blend_path(argv[2]))
    elif len(argv) == 3 and argv[1] == "key":
        print(blend_key(argv[2]))
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(_main(sys.argv))
