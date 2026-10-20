"""fern_clump: a forest-floor fern, about 0.75 m tall and 1 m across, a shuttlecock of arching
once-pinnate fronds (refs: assets-source/refs/forest, "ferns"). Scripted route (PRD §11.1), prop kind
(LOD0 <= 1,500 tris). Usage: make kit-build PIECE=fern_clump.

Alpha-tested cards on the leaf-card route of wildflower_clump: the frond tiles are drawn in numpy
(`_ferns.frond_atlas`), so no Cycles bake is needed. Each frond is one card along an arching path
from the crown (sunk SINK below the pivot, so the stipe enters the litter wherever the floor dips),
rising steeply and drooping toward its tip, folded along the rachis so the pinnae hang to both sides.
Inner fronds stand steeper and taller, outer ones lean out. Card normals blend a rounded clump normal
with the card's own (never in the card plane, so MikkTSpace tangents stay valid).

Wind (TEXCOORD_1 `wind`, project.md "Tree wind data", the wildflower convention): U = 0 at and below
the ground, rising along each frond to about 1 at its tip; V = a seeded phase per frond.
LOD1/LOD2 are regenerated from the same frond list (fewer fronds, wider cards, fewer segments),
never decimated. The collider is the crown's core (CORE_R): ferns are walk-through, and the core lets
them tuck in against spruce trunks as in ref-2."""
import math
import os
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np
from mathutils import Vector

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, budgets, collider, export, geom, materials, scene, uv  # noqa: E402
import _ferns  # noqa: E402
import _props  # noqa: E402

PIECE, MASK_SIZE = "fern_clump", 64
ATLAS = _ferns.ATLAS
NORMAL_SIZE = ATLAS // 2  # pitfalls: a 1024² normal alone would take ~0.6 MB of the 1.5 MB budget
FRONDS, SINK, CORE_R = 22, 0.08, 0.3
FOLD = math.radians(30)
MAT = f"{PIECE}_foliage"
out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
scene.reset_scene(PIECE)
rng = random.Random(PIECE)
EPS = 0.5 / ATLAS
INSET = 3.0 / _ferns.TILE_H  # tile-local inset across the tile (v), so taps never reach a neighbour

# Frond layout: golden-angle azimuths, inner (young) fronds steep and tall, outer ones leaning out.
fronds = []
for i in range(FRONDS):
    a = i * 2.39996 + rng.uniform(-0.25, 0.25)
    ring = (i % 3) / 2.0  # 0 inner, 1 outer
    fronds.append({"a": a, "r0": 0.03 + 0.03 * ring,
                   "el0": math.radians(rng.uniform(84, 88) - 24 * ring),
                   "el1": math.radians(rng.uniform(25, 35) - 56 * ring),
                   "L": rng.uniform(0.95, 1.0) * (1.0 - 0.28 * ring),
                   "roll": rng.uniform(-0.18, 0.18), "tile": i % _ferns.TILES,
                   "phase": _props.unit_hash(PIECE, "frond", i), "sway": rng.uniform(0.9, 1.0)})


def path(f, n=48):
    """Points along the frond's centre line at n + 1 even arc-length steps (crown first)."""
    h = Vector((math.cos(f["a"]), math.sin(f["a"]), 0.0))
    p = h * f["r0"] + Vector((0.0, 0.0, -SINK))
    pts, ds = [p.copy()], f["L"] / n
    for j in range(n):
        t = (j + 0.5) / n
        el = f["el0"] + (f["el1"] - f["el0"]) * t ** 1.25
        p = p + (h * math.cos(el) + Vector((0, 0, math.sin(el)))) * ds
        pts.append(p.copy())
    return pts


def tile_uv(tile, s, c):
    """Atlas UV at frond fraction s (along u) and side c in [-1, 1] (across v) of frond tile `tile`."""
    q = INSET + (0.5 + 0.5 * c) * (1 - 2 * INSET)
    return (EPS + s * (1 - 2 * EPS), (tile + q) * _ferns.TILE_H / ATLAS)


def clump_normal(p, face_n, keep=0.45):
    """Rounded clump normal (up and out of the crown) blended with the card's own normal."""
    radial = Vector((p.x, p.y, 0.0)) / 0.6 + Vector((0.0, 0.0, 1.0))
    if face_n.dot(radial) < 0:
        face_n = -face_n
    return (face_n.normalized() * keep + radial.normalized() * (1.0 - keep)).normalized()


def build_lod(name, every, segs, folded, wscale):
    """Every `every`-th frond as one card of `segs` segments (folded: a ridge along the rachis)."""
    verts, tris, uvs, winds, nrms = [], [], [], [], []
    cols = (-1.0, 0.0, 1.0) if folded else (-1.0, 1.0)
    for i, f in enumerate(fronds):
        if i % every:
            continue
        pts = path(f)
        n = len(pts) - 1
        side = Vector((-math.sin(f["a"]), math.cos(f["a"]), 0.0))
        half = 0.5 * f["L"] / _ferns.ASPECT * wscale
        base = len(verts)
        for j in range(segs + 1):
            s = j / segs
            k = min(int(round(s * n)), n)
            p = pts[k]
            d = (pts[min(k + 1, n)] - pts[max(k - 1, 0)]).normalized()
            up = side.cross(d).normalized()
            if up.z < 0:
                up = -up
            sd = (side * math.cos(f["roll"]) + up * math.sin(f["roll"])).normalized()
            upr = sd.cross(d).normalized()
            if upr.z < 0:
                upr = -upr
            # The blade widens past the stipe; the fold's drop grows with it.
            for c in cols:
                drop = abs(c) * half * math.sin(FOLD) if folded else 0.5 * half * math.sin(FOLD)
                q = p + sd * (c * half * math.cos(FOLD)) - upr * drop
                verts.append(tuple(q))
                uvs.append(tile_uv(f["tile"], s, c))
                sway = f["sway"] * min(1.0, s ** 1.5 * 1.05) if q.z > 0 else 0.0
                winds.append((sway, f["phase"]))
                nrms.append(upr)
        w = len(cols)
        for j in range(segs):
            for c in range(w - 1):
                a, b = base + j * w + c, base + j * w + c + 1
                tris.extend(([a, b, b + w], [a, b + w, a + w]))
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], tris)
    me.validate(clean_customdata=False)
    uvl = me.uv_layers.new(name="UVMap")
    wl = me.uv_layers.new(name="wind")  # after UVMap: UVMap stays TEXCOORD_0 and active
    me.uv_layers.active = uvl
    loop_n = []
    for poly in me.polygons:
        for li in poly.loop_indices:
            vi = me.loops[li].vertex_index
            uvl.data[li].uv = uvs[vi]
            u, v = winds[vi]
            wl.data[li].uv = (min(max(u, 0.0), 1.0), min(max(v, 0.0), 1.0))
            loop_n.append(clump_normal(Vector(verts[vi]), poly.normal))
    for p in me.polygons:
        p.use_smooth = True
    me.normals_split_custom_set(loop_n)
    me.update()
    o = scene.link(bpy.data.objects.new(name, me))
    o.data.materials.append(mat)
    return o


mat = materials.pbr_material(MAT, materials.srgb("#3f722a"), 0.7)
lods = [build_lod(f"{PIECE}_lod0", 1, 6, True, 1.0),
        build_lod(f"{PIECE}_lod1", 2, 4, True, 1.12),
        build_lod(f"{PIECE}_lod2", 3, 2, False, 1.3)]
tris = [geom.tri_count(o) for o in lods]
texel = uv.measure_texel_density(lods[0], ATLAS)
zs = [v.co.z for v in lods[0].data.vertices]
reach = max(math.hypot(v.co.x, v.co.y) for v in lods[0].data.vertices)
print(f"{PIECE}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, reach {reach:.3f}")
budgets.check(tris, "prop", PIECE)
assert texel["median"] >= 256, texel
assert 0.6 <= max(zs) <= 0.9 and 0.45 <= reach <= 0.7, (max(zs), reach)

# Textures: the numpy atlas (albedo with the alpha mask in A, normal, ORM with the crown AO).
alb, nrm, rough, ao = _ferns.frond_atlas(PIECE)
n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32)
orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1)
bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{PIECE}.albedo.png"), non_color=False)
bake.write_png(NORMAL_SIZE, normal.reshape(-1, 4), str(out / f"{PIECE}.normal.png"))
bake.write_png(ATLAS, orm.reshape(-1, 4), str(out / f"{PIECE}.orm.png"))
# masks (linear): R snow capacity 0.5 (fronds shed snow), G porosity 0.6, B no night emissive,
# A = 1 (the unique ORM.R carries the crown AO).
bake.make_masks(MASK_SIZE, 0.5, 0.6, 0.0, None, str(out / f"{PIECE}.masks.png"))
coverage = _ferns.tile_coverage(alb)

# Wind self-check: every LOD carries `wind` in [0, 1], U = 0 at and below the ground, and LOD0 sways.
for o in lods:
    wl = o.data.uv_layers["wind"].data
    w = np.empty(len(wl) * 2)
    wl.foreach_get("uv", w)
    assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
    for loop in o.data.loops:
        if o.data.vertices[loop.vertex_index].co.z <= 0.0:
            assert wl[loop.index].uv[0] == 0.0, o.name
    assert [u.name for u in o.data.uv_layers] == ["UVMap", "wind"], o.name
tips = [lp.uv[0] for lp in lods[0].data.uv_layers["wind"].data]
assert max(tips) > 0.8, max(tips)

# Collider: the crown's core, an octagonal prism CORE_R wide from the ground to 0.5 m.
cbm = bmesh.new()
for z in (0.0, 0.5):
    for k in range(8):
        a = 2 * math.pi * (k + 0.5) / 8
        cbm.verts.new((CORE_R * math.cos(a), CORE_R * math.sin(a), z))
bmesh.ops.convex_hull(cbm, input=cbm.verts)
cme = bpy.data.meshes.new("collider_src")
cbm.to_mesh(cme)
cbm.free()
csrc = scene.link(bpy.data.objects.new("collider_src", cme))
col = collider.collider_from(csrc, "hull", f"{PIECE}_collider")
bpy.data.objects.remove(csrc, do_unlink=True)
bpy.context.view_layer.update()
print(f"{PIECE}: coverage {coverage}")

root = scene.empty(PIECE)
for o in lods:
    o.parent = root
col.parent = root
export.export_glb(root, str(out / f"{PIECE}.glb"))
export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                       device="none (numpy-drawn tiles, no Cycles bake)", samples=0, marginPx=0,
                       channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                 "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                 "orm": {"size": ATLAS, "colorspace": "linear"},
                                 "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                       texelDensityPxPerM=texel,
                       bakeStats={"alphaCoverageByMip": coverage, "alphaCutoff": 0.5},
                       extentZ=[min(zs), max(zs)], reachM=reach, sinkM=SINK, fronds=FRONDS, coreRadiusM=CORE_R,
                       skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
