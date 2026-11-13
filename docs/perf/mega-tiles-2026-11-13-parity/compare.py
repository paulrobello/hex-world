"""Pixel diff of two runs' PNGs, one JSON row per camera: differing (any RGB channel differs),
over_2 (largest channel delta > 2), max (largest channel delta), bytes_identical.
usage: compare.py <label-a> <label-b> [cam,cam,...]"""
import json
import pathlib
import sys

import numpy as np
from PIL import Image

R = pathlib.Path("/tmp/hw-parity/runs")
a, b = R / sys.argv[1], R / sys.argv[2]
want = sys.argv[3].split(",") if len(sys.argv) > 3 else None
for pa in sorted(a.glob("flat-*.png")):
    cam = pa.stem.removeprefix("flat-")
    if want and cam not in want:
        continue
    pb = b / pa.name
    row = {"pair": f"{sys.argv[1]}:{sys.argv[2]}", "cam": cam}
    if not pb.exists():
        row["missing_in_b"] = True
    else:
        ia = np.asarray(Image.open(pa).convert("RGB"), dtype=np.int16)
        ib = np.asarray(Image.open(pb).convert("RGB"), dtype=np.int16)
        d = np.abs(ia - ib).max(axis=2)
        row.update(bytes_identical=pa.read_bytes() == pb.read_bytes(), differing=int((d > 0).sum()),
                   over_2=int((d > 2).sum()), max=int(d.max()), pixels=int(d.size))
    print(json.dumps(row))
