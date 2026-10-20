"""Compares two dumps of hay_bale_round_b (docs/checkpoints/hay-bale-seams/dump_glb.mjs): for every
LOD, whether each primitive's POSITION, NORMAL, TANGENT, TEXCOORD_0 and indices are exactly equal, and
for LODs that differ the largest normal change (degrees) on the bale's corners and on the apron's
(apron corners: glTF v < 0.425). Usage: python3 docs/checkpoints/hay-bale-b-apron/compare.py <before.json> <after.json>"""
import json
import math
import sys

import numpy as np

a, b = (json.loads(open(p).read()) for p in sys.argv[1:3])
assert sorted(a) == sorted(b), (sorted(a), sorted(b))
for name in sorted(a):
    pa, pb = a[name], b[name]
    same = len(pa) == len(pb) and all(
        x[k] == y[k] for x, y in zip(pa, pb) for k in ("material", "position", "normal", "tangent", "uv", "indices"))
    line = f"{name}: {'identical' if same else 'differs'}"
    if not same:
        for x, y in zip(pa, pb):
            fixed = all(x[k] == y[k] for k in ("position", "uv", "indices"))
            line += f"; position, uv, indices {'identical' if fixed else 'DIFFER'}"
            if not fixed:
                continue
            na, nb = np.array(x["normal"]).reshape(-1, 3), np.array(y["normal"]).reshape(-1, 3)
            apron = (1.0 - np.array(x["uv"]).reshape(-1, 2)[:, 1]) > 0.575
            ang = np.degrees(np.arccos(np.clip((na * nb).sum(1) / np.linalg.norm(na, axis=1) / np.linalg.norm(nb, axis=1), -1, 1)))
            line += (f"; normal change bale max {ang[~apron].max():.3f} deg ({(~apron).sum()} verts),"
                     f" apron mean {ang[apron].mean():.2f} max {ang[apron].max():.2f} deg ({apron.sum()} verts)")
    print(line)
