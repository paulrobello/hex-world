"""Pixel diff of the closed-state renders (state_render.py) against the approved look-1 renders at 170094d.
Usage: python3 diff_renders.py <repo> <closed_jpg_dir> <scratch> [rev]"""
import pathlib
import subprocess
import sys

import numpy as np
from PIL import Image

repo, closed, scratch = (pathlib.Path(a) for a in sys.argv[1:4])
rev = sys.argv[4] if len(sys.argv) > 4 else "170094d"
scratch.mkdir(parents=True, exist_ok=True)
for v in ("back", "front", "left", "right", "three_quarter", "top"):
    data = subprocess.check_output(["git", "show", f"{rev}:assets-source/kit/rail_tunnel_portal/renders/{v}.jpg"], cwd=repo)
    (scratch / f"{v}.jpg").write_bytes(data)
    a = np.asarray(Image.open(scratch / f"{v}.jpg").convert("RGB")).astype(int)
    b = np.asarray(Image.open(closed / f"{v}.jpg").convert("RGB")).astype(int)
    d = np.abs(a - b).max(axis=-1)
    same = (scratch / f"{v}.jpg").read_bytes() == (closed / f"{v}.jpg").read_bytes()
    print(f"{v}: bytes identical {same}, max {d.max()}, mean {d.mean():.4f}, px>8 {int((d > 8).sum())}, "
          f"px>24 {int((d > 24).sum())} of {d.size}")
