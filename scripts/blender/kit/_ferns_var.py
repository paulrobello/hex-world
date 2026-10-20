"""Fern variants of fern_clump (PRD §6 Variety): the same once-pinnate fronds on alpha cards, in other
clump shapes (refs: assets-source/refs/forest, "ferns"). `fern_clump.py` is frozen with the original
(provenance hashes it), so its card, normal, wind, texture and collider steps are carried over here
unchanged, with the frond layout as a parameter. The frond tiles come from the approved `_ferns`
helper, unchanged and seeded by the variant's id, so each variant draws its own pinnae in the
family's palette and shapes.

A frond is a dict: azimuth `a`, crown centre `c` (x, y; the original has one crown at the pivot),
start radius `r0`, start and end elevation `el0`/`el1` (radians), length `L`, `roll`, frond `tile`,
wind `phase` and `sway`. Every frond starts SINK below the pivot, so its stipe enters the litter
wherever the floor dips (fern_clump: 0.08 m).

Wind (TEXCOORD_1 `wind`) is fern_clump's: U = 0 at and below the ground, sway x min(1, s^1.5 x 1.05)
along the frond (s = fraction of its length), V = a seeded phase per frond. LOD1/LOD2 are regenerated
from the same frond list (fewer fronds, wider cards, fewer segments), never decimated. The collider
is a crown core prism: ferns are walk-through, and the core is the footprint scatter keeps clear."""
from __future__ import annotations

import math
import os
import pathlib

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, budgets, collider, export, geom, materials, scene, uv

import _ferns

# What the card code below relies on in the approved tile helper.
assert _ferns.ATLAS == 1024 and _ferns.TILES == 3 and _ferns.TILE_H == 336, (_ferns.ATLAS, _ferns.TILES, _ferns.TILE_H)
assert abs(_ferns.ASPECT - _ferns.ATLAS / _ferns.TILE_H) < 1e-9

ATLAS = _ferns.ATLAS
NORMAL_SIZE = ATLAS // 2  # as fern_clump: a 1024² normal alone would take ~0.6 MB of the 1.5 MB budget
MASK_SIZE = 64
FOLD = math.radians(30)
EPS = 0.5 / ATLAS
INSET = 3.0 / _ferns.TILE_H  # tile-local inset across the tile (v), so taps never reach a neighbour
# (every, segments, folded, card width scale) per LOD, as fern_clump.
LODS = ((1, 6, True, 1.0), (2, 4, True, 1.12), (3, 2, False, 1.3))


def path(f: dict, sink: float, n: int = 48) -> list[Vector]:
    """Points along the frond's centre line at n + 1 even arc-length steps (crown first)."""
    h = Vector((math.cos(f["a"]), math.sin(f["a"]), 0.0))
    cx, cy = f.get("c", (0.0, 0.0))
    p = Vector((cx, cy, -sink)) + h * f["r0"]
    pts, ds = [p.copy()], f["L"] / n
    for j in range(n):
        t = (j + 0.5) / n
        el = f["el0"] + (f["el1"] - f["el0"]) * t ** 1.25
        p = p + (h * math.cos(el) + Vector((0, 0, math.sin(el)))) * ds
        pts.append(p.copy())
    return pts


def tile_uv(tile: int, s: float, c: float) -> tuple[float, float]:
    """Atlas UV at frond fraction s (along u) and side c in [-1, 1] (across v) of frond tile `tile`."""
    q = INSET + (0.5 + 0.5 * c) * (1 - 2 * INSET)
    return (EPS + s * (1 - 2 * EPS), (tile + q) * _ferns.TILE_H / ATLAS)


def clump_normal(p: Vector, face_n: Vector, round_r: float, keep: float = 0.45) -> Vector:
    """Rounded clump normal (up and out from the pivot, over `round_r` m) blended with the card's own."""
    radial = Vector((p.x, p.y, 0.0)) / round_r + Vector((0.0, 0.0, 1.0))
    if face_n.dot(radial) < 0:
        face_n = -face_n
    return (face_n.normalized() * keep + radial.normalized() * (1.0 - keep)).normalized()


def build_lod(name: str, fronds: list[dict], sink: float, round_r: float, mat, every: int, segs: int,
              folded: bool, wscale: float):
    """Every `every`-th frond as one card of `segs` segments (folded: a ridge along the rachis)."""
    verts, tris, uvs, winds = [], [], [], []
    cols = (-1.0, 0.0, 1.0) if folded else (-1.0, 1.0)
    for i, f in enumerate(fronds):
        if i % every:
            continue
        pts = path(f, sink)
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
            for c in cols:
                drop = abs(c) * half * math.sin(FOLD) if folded else 0.5 * half * math.sin(FOLD)
                q = p + sd * (c * half * math.cos(FOLD)) - upr * drop
                verts.append(tuple(q))
                uvs.append(tile_uv(f["tile"], s, c))
                sway = f["sway"] * min(1.0, s ** 1.5 * 1.05) if q.z > 0 else 0.0
                winds.append((sway, f["phase"]))
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
            loop_n.append(clump_normal(Vector(verts[vi]), poly.normal, round_r))
    for p in me.polygons:
        p.use_smooth = True
    me.normals_split_custom_set(loop_n)
    me.update()
    o = scene.link(bpy.data.objects.new(name, me))
    o.data.materials.append(mat)
    return o


def build(piece: str, out: pathlib.Path, generator: str, *, fronds: list[dict], sink: float, round_r: float,
          core_r: float, core_h: float, height: tuple[float, float], reach: tuple[float, float]) -> None:
    """fern_clump.py's pipeline for variant `piece` (see the module docstring). `height` and `reach`
    bound the built LOD0's top and horizontal reach (m), `core_r`/`core_h` size the collider."""
    scene.reset_scene(piece)
    mat = materials.pbr_material(f"{piece}_foliage", materials.srgb("#3f722a"), 0.7)
    lods = [build_lod(f"{piece}_lod{i}", fronds, sink, round_r, mat, *cfg) for i, cfg in enumerate(LODS)]
    tris = [geom.tri_count(o) for o in lods]
    texel = uv.measure_texel_density(lods[0], ATLAS)
    zs = [v.co.z for v in lods[0].data.vertices]
    top = max(zs)
    far = max(math.hypot(v.co.x, v.co.y) for v in lods[0].data.vertices)
    print(f"{piece}: fronds {len(fronds)}, tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{top:.3f}, "
          f"reach {far:.3f}")
    budgets.check(tris, "prop", piece)
    assert texel["median"] >= 256, texel
    assert height[0] <= top <= height[1] and reach[0] <= far <= reach[1], (top, far)

    # Textures: the numpy atlas (albedo with the alpha mask in A, normal, ORM with the crown AO).
    alb, nrm, rough, ao = _ferns.frond_atlas(piece)
    n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32)
    orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(ATLAS, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    # masks (linear), as fern_clump: R snow capacity 0.5, G porosity 0.6, B no night emissive, A = 1.
    bake.make_masks(MASK_SIZE, 0.5, 0.6, 0.0, None, str(out / f"{piece}.masks.png"))
    coverage = _ferns.tile_coverage(alb)

    # Wind self-check: every LOD carries `wind` in [0, 1], U = 0 at and below the ground, LOD0 sways.
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

    # Collider: the crown core, an octagonal prism core_r wide from the ground to core_h.
    cbm = bmesh.new()
    for z in (0.0, core_h):
        for k in range(8):
            a = 2 * math.pi * (k + 0.5) / 8
            cbm.verts.new((core_r * math.cos(a), core_r * math.sin(a), z))
    bmesh.ops.convex_hull(cbm, input=cbm.verts)
    cme = bpy.data.meshes.new("collider_src")
    cbm.to_mesh(cme)
    cbm.free()
    csrc = scene.link(bpy.data.objects.new("collider_src", cme))
    col = collider.collider_from(csrc, "hull", f"{piece}_collider")
    bpy.data.objects.remove(csrc, do_unlink=True)
    bpy.context.view_layer.update()
    cpts = [col.matrix_world @ v.co for v in col.data.vertices]
    col_reach = max(math.hypot(p.x, p.y) for p in cpts)
    col_top = max(p.z for p in cpts)
    print(f"{piece}: coverage {coverage}")
    print(f"{piece}: collider reach {col_reach:.4f} top {col_top:.4f}")

    root = scene.empty(piece)
    for o in lods:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none (numpy-drawn tiles, no Cycles bake)", samples=0, marginPx=0,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                     "orm": {"size": ATLAS, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel,
                           bakeStats={"alphaCoverageByMip": coverage, "alphaCutoff": 0.5},
                           extentZ=[min(zs), top], reachM=far, sinkM=sink, fronds=len(fronds), coreRadiusM=core_r,
                           colliderReachM=col_reach, colliderTopM=col_top,
                           skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
