"""Solve bird perches for a hero oak against its LOD0 canopy + leaf triangles, with the checks of
crates/hex-play/src/boids/plan.rs `perches_sit_on_the_canopy` (foliage top within -0.02..+0.08 m in a
0.35 m disc, a shell vertex within 0.6 m, no shell vertex more than 0.35 m above within 0.8 m), plus: no
bark (the dead limbs) above a perch within 0.8 m, and perches at least MIN_GAP apart. Clumps come from
the piece's bake report (`clumps`, 1x, the `_oak_fork` spec form). Prints the Rust perch rows.
Usage: python3 docs/checkpoints/tree_hero_oak_c/perches.py <piece> <tris.json from measure.mjs> <count>"""
import json
import math
import sys

import numpy as np

SCALE, WIDE, TOP = 1.2, 1.15, 0.9
CLUMPS = [tuple(c[:4]) for c in json.load(open(f"assets-source/kit/{sys.argv[1]}/bake-report.json"))["clumps"]]
MIN_GAP = 0.8
d = json.load(open(sys.argv[2]))
want = int(sys.argv[3])
tris = np.array(d["canopy"] + d["leaves"], np.float64).reshape(-1, 3, 3)
canopy = np.array(d["canopy"], np.float64).reshape(-1, 3)
bark = np.array(d["bark"], np.float64).reshape(-1, 3)
a2, b2, c2 = tris[:, 0, [0, 2]], tris[:, 1, [0, 2]], tris[:, 2, [0, 2]]
e1, e2 = b2 - a2, c2 - a2
det = e1[:, 0] * e2[:, 1] - e1[:, 1] * e2[:, 0]
ok = np.abs(det) >= 1e-9


def surface(q):
    w = q[None] - a2
    u = (w[:, 0] * e2[:, 1] - w[:, 1] * e2[:, 0]) / np.where(ok, det, 1)
    v = (e1[:, 0] * w[:, 1] - e1[:, 1] * w[:, 0]) / np.where(ok, det, 1)
    inside = ok & (u >= 0) & (v >= 0) & (u + v <= 1)
    if not inside.any():
        return -1e9
    y = tris[:, 0, 1] + u * (tris[:, 1, 1] - tris[:, 0, 1]) + v * (tris[:, 2, 1] - tris[:, 0, 1])
    return float(y[inside].max())


def dome_point(k, el, daz):
    x, y, z, r = CLUMPS[k]
    c = np.array([x, y, z]) * SCALE
    r *= SCALE
    az = math.atan2(c[1], c[0]) + daz
    h = math.cos(math.radians(el)) * WIDE * r
    return np.array([c[0] + math.cos(az) * h, c[2] + math.sin(math.radians(el)) * TOP * r, -(c[1] + math.sin(az) * h)])


def top_at(p):
    offs = [np.zeros(2)] + [np.array([math.cos(k * math.pi / 4), math.sin(k * math.pi / 4)]) * 0.35 for k in range(8)]
    return max(surface(p[[0, 2]] + o) for o in offs)


cands = []
for k in range(len(CLUMPS)):
    for el in (68.0, 72.0, 76.0, 80.0):
        for daz in [round(-3.0 + 0.3 * i, 1) for i in range(21)]:
            if el == 80.0 and abs(daz) > 2.5:
                continue
            p0 = dome_point(k, el, daz)
            top = top_at(p0)
            if top < -1e8:
                continue
            lift = round((top + 0.03 - p0[1]) / SCALE, 2)
            if not -0.1 <= lift <= 0.2:
                continue
            p = p0 + np.array([0, lift * SCALE, 0])
            t2 = top_at(p)
            if not (t2 - 0.02 <= p[1] <= t2 + 0.08):
                continue
            near = np.linalg.norm(canopy - p, axis=1).min()
            if near >= 0.5 * SCALE:
                continue
            sel = np.hypot(canopy[:, 0] - p[0], canopy[:, 2] - p[2]) < 0.8
            if sel.any() and canopy[sel, 1].max() >= p[1] + 0.35:
                continue
            bs = np.hypot(bark[:, 0] - p[0], bark[:, 2] - p[2]) < 0.8
            if bs.any() and bark[bs, 1].max() >= p[1] - 0.3:
                continue
            cands.append((k, el, daz, lift, p, abs(lift), p[1] - t2))
print(f"{len(cands)} candidates; per clump", {k: sum(1 for c in cands if c[0] == k) for k in range(len(CLUMPS))})
# Greedy: round-robin over clumps (highest el first, smallest lift), at least MIN_GAP apart.
picked = []
by = {k: sorted([c for c in cands if c[0] == k], key=lambda c: (-c[1], c[5])) for k in range(len(CLUMPS))}
while len(picked) < want:
    added = False
    for k in sorted(by, key=lambda k: -CLUMPS[k][2]):
        for c in by[k]:
            if all(np.linalg.norm(c[4] - q[4]) >= MIN_GAP for q in picked):
                picked.append(c)
                by[k].remove(c)
                added = True
                break
        if len(picked) >= want:
            break
    if not added:
        break
for k, el, daz, lift, p, _, off in sorted(picked, key=lambda c: (c[0], -c[1], c[2])):
    print(f"    ({k}, {el:.1f}, {daz:.1f}, {lift:.2f}),  // y {p[1]:.2f} over top {off:+.3f}")
print(len(picked), "perches")
