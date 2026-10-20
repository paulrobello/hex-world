"""Root-cause probe for hay_bale_round_b's straw-apron seams, on a dump from
docs/checkpoints/hay-bale-seams/dump_glb.mjs. For every apron vertex that sits on a segment seam
(a position two apron UV islands share), per LOD it reports:
- `vertexStepDeg`: the angle between the two islands' exported vertex normals at that position (the
  geometric part of the shading step, before the normal map);
- `vsLod0Deg`: the angle between each island's vertex normal and LOD0's normal for the same island at
  the same position (the tangent-space normal map was baked against LOD0's normals, so a LOD whose
  frame differs from LOD0's reads the map in the wrong frame).
Apron corners are told apart from the bale's by UV (glTF v < 0.425, Blender v > 0.575); the island
is the UV place (u0 column, v row) of `_hay_var.apron`'s layout.

Usage: python3 docs/checkpoints/hay-bale-b-apron/diag.py <dump.json>"""
import json
import math
import sys

import numpy as np

dump = json.loads(open(sys.argv[1]).read())


def apron_corners(prims):
    out = []
    for p in prims:
        pos = np.array(p["position"]).reshape(-1, 3)
        nrm = np.array(p["normal"]).reshape(-1, 3)
        uv = np.array(p["uv"]).reshape(-1, 2)
        for q, n, t in zip(pos, nrm, uv):
            vb = 1.0 - t[1]
            if vb <= 0.575:
                continue
            col = 0 if t[0] < 0.336 else 1
            row = 0 if vb < 0.70 else (1 if vb < 0.82 else 2)
            out.append((tuple(np.round(q / 1e-4).astype(int)), (col, row), n / np.linalg.norm(n)))
    return out


def deg(a, b):
    return math.degrees(math.acos(max(-1.0, min(1.0, float(a.dot(b))))))


lod0 = {}
for key, isl, n in apron_corners(dump["hay_bale_round_b_lod0"]):
    lod0[(key, isl)] = n
for name in sorted(dump):
    by_pos = {}
    for key, isl, n in apron_corners(dump[name]):
        by_pos.setdefault(key, {})[isl] = n
    steps, vs0 = [], []
    for key, sides in by_pos.items():
        if len(sides) < 2:
            continue
        ns = list(sides.values())
        steps.append(deg(ns[0], ns[1]))
        for isl, n in sides.items():
            if (key, isl) in lod0:
                vs0.append(deg(n, lod0[(key, isl)]))
    print(f"{name}: seam positions {len(steps)}, vertexStepDeg mean {np.mean(steps):.2f} max {np.max(steps):.2f}, "
          f"vsLod0Deg mean {np.mean(vs0) if vs0 else float('nan'):.2f} max {np.max(vs0) if vs0 else float('nan'):.2f} "
          f"(n {len(vs0)})")
