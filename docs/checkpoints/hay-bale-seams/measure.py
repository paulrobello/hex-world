"""Seam discontinuity of a round hay bale's maps, measured on its exported LODs.

For every geometric edge two triangles share, both sides are sampled at six points along the edge,
each one albedo texel inside its own triangle (in UV, toward the opposite corner). On a UV seam the
two sides sit in different island places; on an interior edge they are two texels apart in one
island, which gives the baseline (the straw's own texel-to-texel variation). Per edge class it reports
the albedo step (mean |dRGB|, sRGB 0-255), the AO step (ORM.R, 0-255) and the shading-normal step
(degrees between the object-space normals the runtime builds, TBN = (T, cross(N, T) * w, N) from the
GLB's normals and tangents and the normal map). Islands are told apart by their Blender UV place
(`_hay.py` layout; the glTF V is 1 - Blender V): barrel u < 0.495, caps beyond, the round_b apron
above Blender v 0.575. `tiltX` is the mean tangent-space normal-map x on each side of the barrel
sector seams (left = the island's u = 0 edge): a bake against a receiver whose edge normals lean
writes opposite tilts at the two edges.

Cap seams are split by the cap they ring ("+" or "-" along the lathe axis: glTF coordinate <axis>
about <mid>; hay_bale_round 0 about 0, hay_bale_round_b 1 about 0.6). The +x lathe cap is the baked
one; the -x cap samples it mirrored (on round_b that cap is the hidden foot).

Usage: python3 docs/checkpoints/hay-bale-seams/measure.py <dump.json> <map dir> <piece> <out.json> <axis> <mid>
(dump.json from dump_glb.mjs)."""
import json
import math
import pathlib
import sys

import numpy as np
from PIL import Image

dump_path, map_dir, piece, out_path = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3], sys.argv[4]
AXIS, MID = int(sys.argv[5]), float(sys.argv[6])
dump = json.loads(pathlib.Path(dump_path).read_text())


def load(channel):
    return np.asarray(Image.open(map_dir / f"{piece}.{channel}.png").convert("RGB"), np.float64)


ALB, NRM, ORM = load("albedo"), load("normal"), load("orm")
INSET = 1.0 / ALB.shape[1]


def bilinear(img, u, v):
    h, w = img.shape[:2]
    x, y = u * w - 0.5, v * h - 0.5
    x0, y0 = int(math.floor(x)), int(math.floor(y))
    fx, fy = x - x0, y - y0
    xs = [min(max(x0, 0), w - 1), min(max(x0 + 1, 0), w - 1)]
    ys = [min(max(y0, 0), h - 1), min(max(y0 + 1, 0), h - 1)]
    a = img[ys[0], xs[0]] * (1 - fx) + img[ys[0], xs[1]] * fx
    b = img[ys[1], xs[0]] * (1 - fx) + img[ys[1], xs[1]] * fx
    return a * (1 - fy) + b * fy


def region(uv):
    u, vb = uv[0], 1.0 - uv[1]
    if vb > 0.575:
        return "apron"
    return "cap" if u > 0.495 else "barrel"


def bary(p, a, b, c):
    v0, v1, v2 = b - a, c - a, p - a
    d = v0[0] * v1[1] - v1[0] * v0[1]
    if abs(d) < 1e-14:
        return np.array([1 / 3, 1 / 3, 1 / 3])
    l1 = (v2[0] * v1[1] - v1[0] * v2[1]) / d
    l2 = (v0[0] * v2[1] - v2[0] * v0[1]) / d
    return np.array([1 - l1 - l2, l1, l2])


def measure(prims):
    pos, nrm, tan, uv, tris = [], [], [], [], []
    off = 0
    for p in prims:
        pos.append(np.array(p["position"]).reshape(-1, 3))
        nrm.append(np.array(p["normal"]).reshape(-1, 3))
        tan.append(np.array(p["tangent"]).reshape(-1, 4))
        uv.append(np.array(p["uv"]).reshape(-1, 2))
        tris.append(np.array(p["indices"]).reshape(-1, 3) + off)
        off += len(pos[-1])
    pos, nrm, tan, uv, tris = (np.concatenate(x) for x in (pos, nrm, tan, uv, tris))
    key = [tuple(np.round(q / 1e-5).astype(np.int64)) for q in pos]
    edges = {}
    for t, tri in enumerate(tris):
        for k in range(3):
            a, b, c = tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]
            ka, kb = key[a], key[b]
            if ka > kb:
                a, b, ka, kb = b, a, kb, ka
            edges.setdefault((ka, kb), []).append((t, a, b, c))

    def side(t, a, b, c, s):
        pa, pb, pc = uv[a], uv[b], uv[c]
        e = pa + (pb - pa) * s
        d = pc - e
        d = d / max(np.linalg.norm(d), 1e-12)
        q = e + d * INSET
        w = bary(q, pa, pb, pc)
        n = w @ nrm[[a, b, c]]
        n /= np.linalg.norm(n)
        tg = w @ tan[[a, b, c], :3]
        tg = tg - n * tg.dot(n)
        tg /= max(np.linalg.norm(tg), 1e-12)
        bt = np.cross(n, tg) * tan[a, 3]
        ts = bilinear(NRM, q[0], q[1]) / 255.0 * 2 - 1
        sh = tg * ts[0] + bt * ts[1] + n * ts[2]
        return bilinear(ALB, q[0], q[1]), bilinear(ORM, q[0], q[1])[0], sh / np.linalg.norm(sh), ts, q

    acc = {}
    for (_ka, _kb), sides in edges.items():
        if len(sides) != 2:
            continue
        (t1, a1, b1, c1), (t2, a2, b2, c2) = sides
        seam = max(np.abs(uv[a1] - uv[a2]).max(), np.abs(uv[b1] - uv[b2]).max()) > 1e-5
        r1 = region((uv[a1] + uv[b1] + uv[c1]) / 3)
        r2 = region((uv[a2] + uv[b2] + uv[c2]) / 3)
        cls = ("seam " if seam else "interior ") + "|".join(sorted((r1, r2)))
        if "cap" in (r1, r2) and r1 != r2:
            cls += " +" if (pos[a1, AXIS] + pos[b1, AXIS]) / 2 > MID else " -"
        st = acc.setdefault(cls, {"edges": 0, "dRGB": [], "dAO": [], "dNormalDeg": [], "tiltLeft": [], "tiltRight": []})
        st["edges"] += 1
        for s in (np.arange(6) + 0.5) / 6:
            A1, O1, N1, T1, q1 = side(t1, a1, b1, c1, s)
            A2, O2, N2, T2, q2 = side(t2, a2, b2, c2, s)
            st["dRGB"].append(float(np.abs(A1 - A2).mean()))
            st["dAO"].append(float(abs(O1 - O2)))
            st["dNormalDeg"].append(float(math.degrees(math.acos(max(-1.0, min(1.0, N1.dot(N2)))))))
            if seam and r1 == r2 == "barrel":
                (tl, tr) = (T1, T2) if q1[0] < q2[0] else (T2, T1)
                st["tiltLeft"].append(float(tl[0]))
                st["tiltRight"].append(float(tr[0]))
    out = {}
    for cls, st in sorted(acc.items()):
        r = {"edges": st["edges"]}
        for k in ("dRGB", "dAO", "dNormalDeg"):
            a = np.array(st[k])
            r[k] = {"mean": round(float(a.mean()), 3), "p95": round(float(np.percentile(a, 95)), 3)}
        if st["tiltLeft"]:
            r["tiltX"] = {"left": round(float(np.mean(st["tiltLeft"])), 4),
                          "right": round(float(np.mean(st["tiltRight"])), 4)}
        out[cls] = r
    return out


result = {name: measure(prims) for name, prims in sorted(dump.items())}
pathlib.Path(out_path).write_text(json.dumps(result, indent=1))
for name, classes in result.items():
    for cls, r in classes.items():
        print(f"{name:24s} {cls:24s} edges {r['edges']:4d}  dRGB {r['dRGB']['mean']:6.2f} (p95 {r['dRGB']['p95']:6.2f})"
              f"  dAO {r['dAO']['mean']:6.2f}  dN {r['dNormalDeg']['mean']:5.2f} deg (p95 {r['dNormalDeg']['p95']:5.2f})"
              + (f"  tiltX {r['tiltX']}" if "tiltX" in r else ""))
