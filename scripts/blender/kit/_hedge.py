"""Hedgerows (B2, owner A31 2026-10-27; refs: assets-source/refs/walls_hedges, ref-2 and ref-4):
untrimmed hawthorn-and-hazel field hedges, 1.9 m tall and 1.5 m wide at the foot, loose and shaggy
with an uneven top, laid along shared tile edges as runtime field boundaries (`hex_core::boundary`).

Pieces (Blender frame: x along the edge, z up; runtime (x, z, -y)):
- a run: 4 m along x, centred on the pivot, open at both ends (runs abut end to end);
- an end, a bend and a junction: one, two or three 2 m arms leaving the pivot, which sits on the
  hex corner, at 0, 120 and 240 degrees (the hex's 120 degree corners, never the refs' right
  angles); every arm is rounded in plan where it meets the pivot, so an end finishes the hedge
  there and two or three arms join round the corner.

Shape: each arm is a loaf, a plan-view distance d from the arm's segment under half width
w(z) = W (1 - (z / H)^P)^(1 / P), so the sides fall steeply and the top is rounded. Leaf-cluster
cards sit on that surface (rows along the arm, columns over the profile), jittered in and out and
in size so the outline is ragged, with a few sprigs pushed out past it, and darker inner cards fill
the inside so gaps read as shade. A dark core (the stems slot, colour + masks, no texture of its
own) fills the inside at 0.8 W so no sky shows through at eye level. Cards that land inside another
arm's loaf are dropped, so arms merge at a corner.

Look: `_shrub`'s numpy leaf-cluster atlas (hazel leaves on twigs, 4 tiles over a stem strip) in
bush_verge's palette with `_verge_bush.matte` leaf roughness (0.76-0.86, never glossy; owner
2026-10-26 "these bushes are too shiny"). `_shrub.py` and `_verge_bush.py` are used through their
module globals and are not edited.

Wind (TEXCOORD_1 `wind`, the plant convention): U = 0 at and below the ground, rising with height
to 1 at the top; V a seeded phase per card. LOD1/LOD2 are regenerated from the same card list
(flat cards, every n-th, scaled up), never decimated. Collider: one box per arm (disjoint parts,
so the walker is not blocked in the corner between two arms)."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, budgets, collider, export, geom, materials, scene, uv

import _shrub
import _verge_bush

assert (_shrub.ATLAS, _shrub.TILE, _shrub.STEM_ROWS) == (1024, 480, 64), (_shrub.ATLAS, _shrub.TILE)
assert (_shrub.LEAF_TILES, _shrub.INNER_TILE) == (4, 3)
assert _verge_bush.LEAF_ROUGHNESS == (0.76, 0.86), _verge_bush.LEAF_ROUGHNESS

# bush_verge's palette (scripts/blender/kit/bush_verge.py): the owner's "matte verge-bush green".
PALETTE = dict(DARK="#18300f", MID="#2c5219", LIGHT="#5f8a32", BACK="#0f1f09", TWIG="#3e3224")
HEIGHT, HALF_W, PROFILE_P = 1.9, 0.75, 2.2
CORE_INSET = 0.3
CARD_FOOT = 0.08  # the profile's foot for cards (m): the lowest row's cards reach below the ground
SINK = 0.25  # cards and core reach below the ground (the meadow relief dips to about -0.25 m)
ARM = 2.0  # corner-piece arm length (m); a run is 2 * ARM
WIND_UV = _shrub.WIND_UV
CORE_SCALE = 0.8
COLLIDER_HALF_W, COLLIDER_H = 0.62, 1.6


def half_width(z: float, w: float = HALF_W, h: float = HEIGHT) -> float:
    """The loaf's half width at height z (0 at and above h)."""
    if z >= h:
        return 0.0
    return w * (1.0 - (max(z, 0.0) / h) ** PROFILE_P) ** (1.0 / PROFILE_P)


def profile(n: int, w: float, h: float) -> list[tuple[float, float, float, float]]:
    """n points evenly spaced in arc length along one side of the profile, from the foot (s = w,
    z = -SINK) to the top (s = 0, z = h): (s, z, ns, nz), with the outward 2D normal."""
    zs = np.linspace(CARD_FOOT, h, 400)
    ss = np.array([half_width(z, w, h) for z in zs])
    seg = np.hypot(np.diff(ss), np.diff(zs))
    arc = np.concatenate([[0.0], np.cumsum(seg)])
    out = []
    for k in range(n):
        a = arc[-1] * (k + 0.5) / n
        i = min(int(np.searchsorted(arc, a)), len(zs) - 1)
        i0 = max(i - 1, 0)
        ds, dz = ss[i] - ss[i0], zs[i] - zs[i0]
        ln = math.hypot(ds, dz) or 1.0
        # Tangent (ds, dz) runs up and inward; the outward normal is (dz, -ds) rotated to face out.
        ns, nz = dz / ln, -ds / ln
        out.append((float(ss[i]), float(zs[i]), ns, nz))
    return out


def unit(seed: str, *key) -> float:
    return _shrub.unit(seed, *key)


def arm_dirs(arms: tuple[float, ...]) -> list[Vector]:
    return [Vector((math.cos(math.radians(a)), math.sin(math.radians(a)), 0.0)) for a in arms]


def plan_distance(p: Vector, a: Vector, d: Vector, length: float) -> float:
    """Plan-view distance from p to the segment a + d t, t in [0, length]."""
    q = Vector((p.x - a.x, p.y - a.y, 0.0))
    t = max(0.0, min(length, q.dot(d)))
    return (q - d * t).length


def inside_other(p: Vector, segs: list[tuple[Vector, Vector, float]], own: int, margin: float) -> bool:
    for i, (a, d, ln) in enumerate(segs):
        if i == own:
            continue
        if plan_distance(p, a, d, ln) < half_width(p.z) - margin:
            return True
    return False


def segments(kind: str, arms: tuple[float, ...]) -> list[tuple[Vector, Vector, float]]:
    """(start, direction, length) per arm: a run is one 4 m segment centred on the pivot; corner
    pieces have one 2 m arm per angle, from the pivot."""
    if kind == "run":
        return [(Vector((-ARM, 0.0, 0.0)), Vector((1.0, 0.0, 0.0)), 2 * ARM)]
    return [(Vector((0.0, 0.0, 0.0)), d, ARM) for d in arm_dirs(arms)]


def cards(seed: str, kind: str, arms: tuple[float, ...], shape: dict) -> list[dict]:
    """Outer cards on each arm's loaf (both sides along the arm, plus the rounded cap round the
    pivot end for corner pieces), sprigs pushed out past it, and inner cards inside. Each card:
    centre p, normal n, size, roll, tile, kind, phase."""
    rng = random.Random(f"{seed}:cards")
    segs = segments(kind, arms)
    spacing, card = shape["spacing"], shape["card"]
    # Card centres sit a little inside the loaf, so the cards' outer halves reach its outline.
    prof = profile(shape["rows"], HALF_W - 0.08, HEIGHT - 0.15)
    out: list[dict] = []

    def add(p: Vector, n: Vector, own: int, kind_: str, size_k: float = 1.0) -> None:
        if inside_other(p, segs, own, 0.12):
            return
        tile = INNER if kind_ == "inner" else rng.randrange(_shrub.LEAF_TILES - 1)
        # Tilt each card off the surface normal so the cards do not lie like shingles.
        tilt = shape["tilt"]
        n = n.normalized() + Vector((rng.uniform(-tilt, tilt), rng.uniform(-tilt, tilt), rng.uniform(-tilt, tilt)))
        out.append({"p": p, "n": n.normalized(), "size": card * size_k * rng.uniform(0.75, 1.25),
                    "roll": rng.uniform(0, 2 * math.pi), "tile": tile, "kind": kind_,
                    "phase": unit(seed, "card", len(out))})

    for si, (a, d, ln) in enumerate(segs):
        perp = Vector((-d.y, d.x, 0.0))
        # The open ends of a run (and the far ends of corner arms) run a card's half width past
        # the joint, so abutting pieces overlap instead of showing a seam.
        t0 = -0.15 if kind == "run" else 0.0
        t1 = ln + 0.15
        n_along = max(2, round((t1 - t0) / spacing))
        for side in (1.0, -1.0):
            for k, (s, z, ns, nz) in enumerate(prof):
                for j in range(n_along):
                    t = t0 + (t1 - t0) * (j + 0.5 + rng.uniform(-0.3, 0.3)) / n_along
                    out_j = rng.uniform(-0.06, 0.1) + shape["ragged"] * (z / HEIGHT) ** 2 * rng.uniform(-1, 1)
                    zz = z + rng.uniform(-0.05, 0.05)
                    p = a + d * t + perp * side * (s + out_j * ns) + Vector((0, 0, zz + out_j * nz))
                    n = perp * side * ns + Vector((0, 0, nz + 0.25))
                    add(p, n, si, "outer")
        if kind != "run":
            # The rounded cap round the pivot end: half a turn from +perp through -d to -perp.
            for k, (s, z, ns, nz) in enumerate(prof):
                n_round = max(1, round(math.pi * s / spacing))
                for j in range(n_round):
                    phi = math.pi * (j + 0.5 + rng.uniform(-0.25, 0.25)) / n_round - math.pi / 2
                    radial = perp * math.sin(phi) - d * math.cos(phi)
                    out_j = rng.uniform(-0.06, 0.1)
                    p = a + radial * (s + out_j * ns) + Vector((0, 0, z + out_j * nz))
                    add(p, radial * ns + Vector((0, 0, nz + 0.25)), si, "outer")
        # Sprigs: a few cards well out past the outline, high on the hedge (untrimmed).
        for j in range(shape["sprigs"]):
            t = rng.uniform(0.1, ln - 0.1)
            s, z, ns, nz = prof[rng.randrange(len(prof) // 3, len(prof))]
            side = rng.choice((1.0, -1.0))
            push = rng.uniform(0.12, 0.28)
            p = a + d * t + perp * side * (s + push * ns) + Vector((0, 0, z + push * nz))
            add(p, perp * side * ns + Vector((0, 0, nz + 0.3)), si, "outer", 0.8)
        # Inner cards fill the loaf at 0.3-0.75 of its width.
        n_inner = round(shape["inner_per_m"] * ln)
        for j in range(n_inner):
            t = rng.uniform(0.05, ln - 0.05)
            z = rng.uniform(0.1, HEIGHT * 0.85)
            s = half_width(z) * rng.uniform(0.3, 0.75) * rng.choice((1.0, -1.0))
            p = a + d * t + perp * s + Vector((0, 0, z))
            add(p, perp * (1 if s > 0 else -1) + Vector((0, 0, 0.6)), si, "inner")
    return out


INNER = _shrub.INNER_TILE


def wind_u(z: float) -> float:
    if z <= 0.0:
        return 0.0
    return min(1.0, (z / HEIGHT) ** 1.1)


def card_mesh(name: str, cs: list[dict], every: int, grid: int, scale: float, keep_inner: int) -> bpy.types.Object:
    """Cluster cards (every `every`-th outer card, every `keep_inner`-th inner one), each a
    grid x grid quad, grid 2 with its centre lifted (a soft cushion), with corner normals blending
    the card's surface normal (up-biased) and its face normal (never in the card plane)."""
    verts, faces, uvs, winds, normals = [], [], [], [], []
    io = ii = 0
    atlas, tile = _shrub.ATLAS, _shrub.TILE
    cushion = 0.05
    for c in cs:
        if c["kind"] == "outer":
            io += 1
            if (io - 1) % every:
                continue
        else:
            ii += 1
            if not keep_inner or (ii - 1) % keep_inner:
                continue
        nrm = c["n"]
        ref = Vector((0, 0, 1)) if abs(nrm.z) < 0.95 else Vector((1, 0, 0))
        t1 = nrm.cross(ref).normalized()
        t2 = nrm.cross(t1).normalized()
        cr, sr = math.cos(c["roll"]), math.sin(c["roll"])
        u_ax, v_ax = t1 * cr + t2 * sr, -t1 * sr + t2 * cr
        half = 0.5 * c["size"] * scale
        x0, y0 = _shrub.tile_origin(c["tile"])
        base = len(verts)
        dome = (nrm + Vector((0, 0, 0.35))).normalized()
        for j in range(grid + 1):
            for i in range(grid + 1):
                su, sv = i / grid, j / grid
                q = c["p"] + u_ax * ((su - 0.5) * 2 * half) + v_ax * ((sv - 0.5) * 2 * half)
                if grid == 2 and i == 1 and j == 1:
                    q = q + nrm * cushion * scale
                elif grid == 2:
                    q = q - nrm * cushion * 0.25 * scale * (abs(su - 0.5) + abs(sv - 0.5))
                verts.append(tuple(q))
                ins = _shrub.TILE_INSET * 0.5
                uvs.append(((x0 + tile * (ins + su * (1 - 2 * ins))) / atlas,
                            (y0 + tile * (ins + sv * (1 - 2 * ins))) / atlas))
                winds.append((wind_u(q.z), c["phase"]))
                normals.append(dome)
        w = grid + 1
        for j in range(grid):
            for i in range(grid):
                v00 = base + j * w + i
                faces.append([v00, v00 + 1, v00 + w + 1])
                faces.append([v00, v00 + w + 1, v00 + w])
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.validate(clean_customdata=False)
    uvl = me.uv_layers.new(name="UVMap")
    wl = me.uv_layers.new(name=WIND_UV)
    me.uv_layers.active = uvl
    loop_n = []
    for poly in me.polygons:
        poly.use_smooth = True
        for li in poly.loop_indices:
            vi = me.loops[li].vertex_index
            uvl.data[li].uv = uvs[vi]
            wl.data[li].uv = winds[vi]
            dome = normals[vi]
            face = poly.normal if poly.normal.dot(dome) >= 0 else -poly.normal
            loop_n.append((dome * 0.55 + face * 0.45).normalized())
    me.normals_split_custom_set(loop_n)
    me.update()
    return scene.link(bpy.data.objects.new(name, me))


def core_mesh(name: str, kind: str, arms: tuple[float, ...], sides: int) -> bpy.types.Object:
    """The dark core: each arm's loaf at CORE_SCALE as a closed prism (`sides` profile segments),
    UV0 in the stems' masks strip (u along the profile, v up), wind U from height."""
    bm = bmesh.new()
    uvl = bm.loops.layers.uv.new("UVMap")
    wnd = bm.loops.layers.uv.new(WIND_UV)
    u0, v0, u1, v1 = _shrub.STEM_BOX
    w, h = HALF_W * CORE_SCALE, HEIGHT * 0.86
    # Profile polygon (s, z), from the right foot over the top to the left foot.
    zs = [-SINK] + [h * (k / (sides // 2)) ** 0.8 for k in range(sides // 2)]
    right = [(half_width(max(z, 0.0), w, h), z) for z in zs]
    ring = right + [(0.0, h)] + [(-s, z) for s, z in reversed(right)]
    for a, d, ln in segments(kind, arms):
        perp = Vector((-d.y, d.x, 0.0))
        # A run's core stops short of its open ends, so a gap beside it shows leaves, not the core.
        t0, t1 = (CORE_INSET, ln - CORE_INSET) if kind == "run" else (0.0, ln - CORE_INSET)
        ends = []
        for t in (t0, t1):
            ends.append([bm.verts.new(a + d * t + perp * s + Vector((0, 0, z))) for s, z in ring])
        n = len(ring)
        for k in range(n):
            kn = (k + 1) % n
            f = bm.faces.new((ends[0][k], ends[0][kn], ends[1][kn], ends[1][k]))
            for lp, (kk, e) in zip(f.loops, ((k, 0), (kn, 0), (kn, 1), (k, 1))):
                z = ring[kk][1]
                lp[uvl].uv = (u0 + (u1 - u0) * (kk / n), v0 + (v1 - v0) * min(1.0, max(0.0, (z + SINK) / (h + SINK))))
                lp[wnd].uv = (wind_u(z), unit("core", kk, e))
        for e, verts in enumerate(ends):
            f = bm.faces.new(verts if e == 1 else list(reversed(verts)))
            for lp in f.loops:
                z = lp.vert.co.z
                lp[uvl].uv = (u0 + 0.5 * (u1 - u0), v0 + (v1 - v0) * min(1.0, max(0.0, (z + SINK) / (h + SINK))))
                lp[wnd].uv = (wind_u(z), 0.5)
    bmesh.ops.triangulate(bm, faces=bm.faces[:])
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.uv_layers.active = me.uv_layers["UVMap"]
    for p in me.polygons:
        p.use_smooth = False
    return scene.link(bpy.data.objects.new(name, me))


def collider_mesh(name: str, kind: str, arms: tuple[float, ...]) -> bpy.types.Object:
    """One box per arm, from the pivot (or the run's start) along the arm: disjoint parts."""
    bm = bmesh.new()
    for a, d, ln in segments(kind, arms):
        perp = Vector((-d.y, d.x, 0.0))
        t0 = 0.0 if kind == "run" else -COLLIDER_HALF_W * 0.5
        vs = []
        for z in (0.0, COLLIDER_H):
            for t, s in ((t0, -1), (ln, -1), (ln, 1), (t0, 1)):
                vs.append(bm.verts.new(a + d * t + perp * s * COLLIDER_HALF_W + Vector((0, 0, z))))
        for f in ((0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)):
            bm.faces.new([vs[i] for i in f])
    me = bpy.data.meshes.new(name + "_src")
    bm.to_mesh(me)
    bm.free()
    return scene.link(bpy.data.objects.new(name + "_src", me))


def install_palette() -> None:
    for name, value in PALETTE.items():
        setattr(_shrub, name, _verge_bush.srgb(value))


def build(piece: str, out: pathlib.Path, generator: str, *, kind: str, arms: tuple[float, ...] = (),
          shape: dict, lods: tuple[dict, ...], trunk_clear: float = 0.0) -> None:
    """The whole hedge piece: cards, core, numpy maps, LOD joins, collider, export and reports.
    `trunk_clear` > 0 drops outer cards within that plan radius of the pivot (the hedgerow oak's
    trunk rises there)."""
    assert kind in ("run", "corner"), kind
    install_palette()
    scene.reset_scene(piece)
    leaf_mat = materials.pbr_material(f"{piece}_leaves", materials.srgb("#2c5219"), 0.8)
    stem_mat = materials.pbr_material(f"{piece}_stems", materials.srgb("#1a140c"), 0.9)
    cs = cards(piece, kind, arms, shape)
    if trunk_clear:
        cs = [c for c in cs if math.hypot(c["p"].x, c["p"].y) > trunk_clear or c["p"].z > 2.4]
    lod_objs = []
    for i, c in enumerate(lods):
        leaves = card_mesh(f"leaves_l{i}", cs, c["every"], c["grid"], c["scale"], c["inner"])
        leaves.data.materials.append(leaf_mat)
        core = core_mesh(f"core_l{i}", kind, arms, c["core_sides"])
        core.data.materials.append(stem_mat)
        lod_objs.append(geom.join([leaves, core], f"{piece}_lod{i}"))
    tris = [geom.tri_count(o) for o in lod_objs]
    leaf0 = lod_objs[0]
    tmp = leaf0.copy()
    tmp.data = leaf0.data.copy()
    scene.link(tmp)
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if tmp.data.materials[f.material_index].name != leaf_mat.name],
                     context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    texel = {leaf_mat.name: uv.measure_texel_density(tmp, _shrub.ATLAS)}
    bpy.data.objects.remove(tmp, do_unlink=True)
    zs = [v.co.z for v in leaf0.data.vertices]
    print(f"{piece}: tris per LOD {tris}, cards {sum(c['kind'] == 'outer' for c in cs)} outer + "
          f"{sum(c['kind'] == 'inner' for c in cs)} inner, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}")
    budgets.check(tuple(tris), "prop", piece)
    assert texel[leaf_mat.name]["median"] >= 256, texel
    assert 1.8 <= max(zs) <= 2.4, max(zs)  # 1.9 m nominal, sprigs and tilted top cards above it

    # Maps: the family's numpy atlas with the leaf roughness made matte.
    tile = _shrub.cluster_tile

    def matte_tile(px: int, seed: str, k: int, inner: bool) -> tuple:
        rgba, nrm, rough, ao = tile(px, seed, k, inner)
        return rgba, nrm, _verge_bush.matte(rough), ao

    _shrub.cluster_tile = matte_tile
    try:
        alb, nrm, rough, ao, coverage = _shrub.atlas(piece)
    finally:
        _shrub.cluster_tile = tile
    leaf_px = alb[..., 3] > 0.5
    assert float(rough[leaf_px].min()) >= 0.76, float(rough[leaf_px].min())
    for k in range(_shrub.LEAF_TILES):
        assert coverage[f"tile_{k}"][0] >= 0.5, coverage
    ns = _shrub.NORMAL_SIZE
    n2 = nrm.reshape(ns, 2, ns, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((ns, ns, 1))], -1).astype(np.float32)
    orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1)
    atlas_px = _shrub.ATLAS
    bake.write_png(atlas_px, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(ns, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(atlas_px, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    # masks (linear): leaves R snow 0.6, G porosity 0.6, A = 1 (the tile AO is in ORM.R); the core
    # strip R 0.3, G 0.5, A from 0.45 at the foot to 0.75 up in the hedge.
    ms = _shrub.MASK_SIZE
    mk = np.empty((ms, ms, 4), np.float32)
    mk[:] = (0.6, 0.6, 0.0, 1.0)
    su0, sv0, su1, sv1 = _shrub.STEM_BOX
    rows = slice(0, _shrub.STEM_ROWS * ms // atlas_px)
    assert (sv1 * ms) + 4 <= rows.stop, (sv1, rows)
    vv = (np.arange(ms, dtype=np.float32) + 0.5) / ms
    t = np.clip((vv - sv0) / (sv1 - sv0), 0, 1)
    mk[rows, :, 0] = 0.3
    mk[rows, :, 1] = 0.5
    mk[rows, :, 3] = (0.45 + 0.3 * t[rows])[:, None]
    bake.write_png(ms, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))

    for o in lod_objs:
        wl = o.data.uv_layers[WIND_UV].data
        w = np.empty(len(wl) * 2)
        wl.foreach_get("uv", w)
        assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
        for loop in o.data.loops:
            if o.data.vertices[loop.vertex_index].co.z <= 0.0:
                assert wl[loop.index].uv[0] == 0.0, o.name
        assert [u.name for u in o.data.uv_layers] == ["UVMap", WIND_UV], o.name
        assert {m.name for m in o.data.materials} == {leaf_mat.name, stem_mat.name}, o.name

    csrc = collider_mesh(piece, kind, arms)
    col = collider.collider_from(csrc, "cap", f"{piece}_collider")
    bpy.data.objects.remove(csrc, do_unlink=True)
    bpy.context.view_layer.update()
    print(f"{piece}: coverage {coverage}")

    root = scene.empty(piece)
    for o in lod_objs:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lod_objs[0]]),
                           device="none (numpy-drawn tiles, no Cycles bake)", samples=0, marginPx=0,
                           channels={"albedo": {"size": atlas_px, "colorspace": "srgb"},
                                     "normal": {"size": ns, "colorspace": "linear"},
                                     "orm": {"size": atlas_px, "colorspace": "linear"},
                                     "masks": {"size": ms, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, bakeStats={"alphaCoverageByMip": coverage, "alphaCutoff": 0.5},
                           extentZ=[min(zs), max(zs)], hedge={"kind": kind, "arms": list(arms), "heightM": HEIGHT,
                                                              "halfWidthM": HALF_W, "armM": ARM,
                                                              "colliderHalfWidthM": COLLIDER_HALF_W,
                                                              "colliderHeightM": COLLIDER_H},
                           shape=shape, skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
