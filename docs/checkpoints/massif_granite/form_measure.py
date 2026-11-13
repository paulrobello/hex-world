"""The form pass's self-check on a built massif_granite GLB (brief targets 1-4): top-down rock
coverage and foot radius, ridge spines, the ground's radial profile, the top height, plus the
rock's surface split (seen from above or not) and a top-down class map for the review sheet.
Form pass 3 samples the profile by a ray at each exact point along the bearing (the 0.25 m map's
nearest cell read up to 13 cm of false inward drops on the steeper flanks) and adds `rimBand`, how
far in from the outline the ring's ground stays at tile level.

Run (from the repo root):
  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/massif_granite/form_measure.py -- \
      <massif stage dir> <out dir> [lod 0|1|2]

Writes <out>/form-measure.json and <out>/topdown-classes.npy (0 outside, 1 grass, 2 scree, 3 rock,
on a 0.25 m grid over x, y in [-45, 45], Blender XY, row = y index)."""
import json
import math
import pathlib
import sys

import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

args = sys.argv[sys.argv.index("--") + 1:]
STAGE, OUT = pathlib.Path(args[0]), pathlib.Path(args[1])
LOD = int(args[2]) if len(args) > 2 else 0
OUT.mkdir(parents=True, exist_ok=True)
R, C = 16.0, 16.0 * math.sqrt(3.0) / 2.0
AREA7 = 7 * 1.5 * math.sqrt(3.0) * R * R
STEP, EXT = 0.25, 45.0

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=str(STAGE / "massif_granite.glb"))
obj = bpy.data.objects[f"massif_granite_lod{LOD}"]
me = obj.data
mw = obj.matrix_world
verts = [mw @ v.co for v in me.vertices]
slot = [me.materials[p.material_index].name.split(".")[0].replace("massif_granite_", "") for p in me.polygons]
polys = [list(p.vertices) for p in me.polygons]
bvh = BVHTree.FromPolygons(verts, polys)
CLS = {"cap": 1, "scree": 2, "rock": 3, "skirt": 0}

# 1. Top-down classes: the first surface a ray from above meets.
g = np.arange(-EXT, EXT + 1e-9, STEP)
cls = np.zeros((len(g), len(g)), np.uint8)
zmap = np.full((len(g), len(g)), np.nan)
for j, y in enumerate(g):
    for i, x in enumerate(g):
        hit = bvh.ray_cast(Vector((x, y, 80.0)), Vector((0.0, 0.0, -1.0)))
        if hit[0] is not None:
            cls[j, i] = CLS.get(slot[hit[2]], 0)
            zmap[j, i] = hit[0].z
np.save(OUT / "topdown-classes.npy", cls)
cell = STEP * STEP
rock = cls == 3
X, Y = np.meshgrid(g, g)
Rr = np.hypot(X, Y)
cov = {"rockM2": round(float(rock.sum() * cell), 1), "screeM2": round(float((cls == 2).sum() * cell), 1),
       "grassM2": round(float((cls == 1).sum() * cell), 1)}
cov["rockShare"] = round(cov["rockM2"] / AREA7, 4)
cov["rockPlusScreeShare"] = round((cov["rockM2"] + cov["screeM2"]) / AREA7, 4)
cov["equalAreaDiscRadiusM"] = round(math.sqrt(cov["rockM2"] / math.pi), 2)

# Foot radius per bearing from the footprint centre: the farthest rock cell along the bearing.
bear = []
for b in range(72):
    a = math.radians(5 * b)
    rr = np.arange(0.0, 44.0, 0.1)
    ii = np.clip(np.round((rr * math.cos(a) + EXT) / STEP).astype(int), 0, len(g) - 1)
    jj = np.clip(np.round((rr * math.sin(a) + EXT) / STEP).astype(int), 0, len(g) - 1)
    on = rock[jj, ii]
    bear.append(float(rr[on].max()) if on.any() else 0.0)
bear = np.array(bear)
cov["footRadiusM"] = {"median": round(float(np.median(bear)), 2), "mean": round(float(bear.mean()), 2),
                      "min": round(float(bear.min()), 2), "max": round(float(bear.max()), 2)}
cov["footRadiusPer5Deg"] = bear.round(1).tolist()


# 2. Spines: local maxima of the foot radius over bearing, at least 4 m above the lower of the
# two flanking minima and reaching past the centre cell's apothem.
def spines(b):
    n = len(b)
    out = []
    for k in range(n):
        if b[k] < C or not (b[k] >= b[(k - 1) % n] and b[k] >= b[(k + 1) % n]):
            continue
        lo = []
        for d in (-1, 1):
            m, j = b[k], k
            for _ in range(n // 2):
                j = (j + d) % n
                m = min(m, b[j])
                if b[j] > b[k]:
                    break
            lo.append(m)
        if b[k] - max(lo) >= 4.0:
            out.append({"bearingDeg": 5 * k, "reachM": round(float(b[k]), 1)})
    # merge plateaus (equal neighbouring maxima)
    merged = []
    for s in out:
        if merged and (s["bearingDeg"] - merged[-1]["bearingDeg"]) <= 10:
            continue
        merged.append(s)
    if len(merged) > 1 and (merged[0]["bearingDeg"] + 360 - merged[-1]["bearingDeg"]) <= 10:
        merged.pop()
    return merged


sp = spines(bear)
cov["spines"] = {"count": len(sp), "list": sp,
                 "intoRingCells": sum(1 for s in sp if s["reachM"] > C)}

# 3. Ground profile: the cap/scree surface height from the rim inward to the rock foot, along
# bearings from the centre, on the top-down height map; monotonic rise and concavity.
prof = {}
viol = 0
worst = 0.0
concave = 0
nb = 0


def cast(x, y):
    hit = bvh.ray_cast(Vector((x, y, 80.0)), Vector((0.0, 0.0, -1.0)))
    return (CLS.get(slot[hit[2]], 0), hit[0].z) if hit[0] is not None else (0, np.nan)


for b in range(0, 360, 10):
    a = math.radians(b)
    rr = np.arange(0.0, 44.0, 0.25)
    cz = [cast(r0 * math.cos(a), r0 * math.sin(a)) for r0 in rr]
    c, z = np.array([q[0] for q in cz]), np.array([q[1] for q in cz])
    ground = (c == 1) | (c == 2)
    if not ground.any():
        continue
    rim = int(np.nonzero(ground)[0].max())
    foot = int(np.nonzero(c == 3)[0].max()) + 1 if (c == 3).any() else 0
    seg = z[foot:rim + 1][::-1]  # from the rim inward
    okm = ground[foot:rim + 1][::-1]
    seg, rseg = seg[okm], rr[foot:rim + 1][::-1][okm]
    # The flat ring keeps meadow_base's hollows (z <= 0, up to 0.25 m deep, the seam contract):
    # the slope starts at the outermost point from which the ground stays above 2 cm.
    low = np.nonzero(seg <= 0.02)[0]
    seg, rseg = (seg[low.max() + 1:], rseg[low.max() + 1:]) if len(low) else (seg, rseg)
    if len(seg) < 4:
        continue
    d = np.diff(seg)
    drops = -d[d < 0]
    w = float(drops.max()) if len(drops) else 0.0
    worst = max(worst, w)
    viol += int(w > 0.05)
    nb += 1
    # concavity: mean slope over the inner third (at the foot) vs the outer third
    risen = seg[seg > 0.05]
    if len(risen) > 9:
        k3 = len(risen) // 3
        outer = (risen[k3] - risen[0]) / (k3 * 0.25)
        inner = (risen[-1] - risen[-1 - k3]) / (k3 * 0.25)
        concave += int(inner > outer)
    prof[str(b)] = {"worstInwardDropM": round(w, 3), "worstDropAtR": round(float(rseg[int(np.argmin(d))]), 2), "footR": round(float(rr[foot]), 2), "footZ": round(float(z[foot]) if foot < len(z) else 0.0, 2),
                    "z_at_r": {str(r0): round(float(z[int(r0 / 0.25)]), 2) for r0 in (15, 20, 25, 30, 35)
                               if int(r0 / 0.25) < len(z) and not np.isnan(z[int(r0 / 0.25)])
                               and (c[int(r0 / 0.25)] in (1, 2))}}
cov["groundProfile"] = {"bearings": nb, "bearingsWithInwardDropOver5cm": viol, "worstInwardDropM": round(worst, 3),
                        "bearingsSteepestAtFoot": concave, "perBearing": prof}

# 3b. The rim band: ring-cell ground (outside the centre cell) at tile level (under 0.10 m, the
# meadow hollows included), by its distance in from the flower outline.
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3] / "scripts/blender"))
from lib import hexgeo  # noqa: E402

segs = []
for j in range(18):
    ring_i, edge = j // 3, (j // 3 + 5 + j % 3) % 6
    cx, cy = (2.0 * C * c0 for c0 in hexgeo.edge_normal(ring_i))
    p0, p1 = hexgeo.corner((edge - 1) % 6), hexgeo.corner(edge)
    segs.append(((cx + p0[0], cy + p0[1]), (cx + p1[0], cy + p1[1])))
od = np.full(X.shape, np.inf)
for (ax, ay), (bx, by) in segs:
    dx, dy = bx - ax, by - ay
    t = np.clip(((X - ax) * dx + (Y - ay) * dy) / (dx * dx + dy * dy), 0.0, 1.0)
    od = np.minimum(od, np.hypot(X - ax - t * dx, Y - ay - t * dy))
centre = np.max([np.abs(X * n[0] + Y * n[1]) for n in (hexgeo.edge_normal(k) for k in range(6))], axis=0) < C
ringg = ((cls == 1) | (cls == 2)) & ~centre
level = ringg & (zmap < 0.10)
cov["rimBand"] = {"maxInM": round(float(od[level].max()), 2) if level.any() else 0.0,
                  "p99InM": round(float(np.percentile(od[level], 99)), 2) if level.any() else 0.0,
                  "tileLevelM2": round(float(level.sum() * cell), 1),
                  "ringGroundTileLevelShare": round(float(level.sum() / max(ringg.sum(), 1)), 3)}

# 4. Top height and the rock surface split.
zs = np.array([v.z for v in verts])
cov["topM"] = round(float(zs.max()), 3)
rock_faces = [k for k, s in enumerate(slot) if s == "rock"]
area_up = area_all = 0.0
for k in rock_faces:
    p = me.polygons[k]
    area_all += p.area
    if p.normal.z > 0.0:
        area_up += p.area
cov["rockSurfaceM2"] = {"all": round(area_all, 1), "upFacing": round(area_up, 1)}
cov["tris"] = {s: sum(1 for t in slot if t == s) for s in ("cap", "scree", "rock", "skirt")}
json.dump(cov, open(OUT / "form-measure.json", "w"), indent=1)
print(json.dumps({k: v for k, v in cov.items() if k not in ("footRadiusPer5Deg",)}, indent=None)[:3000])
