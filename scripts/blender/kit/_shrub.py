"""Forest shrubs (refs: assets-source/refs/forest, ref-1's "undergrowth of ferns, shrubs and saplings"):
a low rounded woodland shrub, hazel-like, a dense dome of small leaves on a few woody stems. Scripted
route, no Cycles bake: the leaf-cluster tiles are drawn in numpy (`cluster_tile`), the stems are
colour + masks. `build` is the whole pipeline, so a variant is one parameter set.

Slots (one UV0):
- leaves: alpha-cutoff cluster cards on LEAF_TILES numpy tiles (a 2 x 2 grid above STEM_ROWS); the
  outer cards sit on an ellipsoid shell and face out from it, the inner ones (tile INNER_TILE, drawn
  darker and denser) fill the dome so gaps read as shade, not sky. masks A = 1 there (the ORM.R
  carries the tile AO);
- stems: colour + masks only (no texture of its own), UV0 mapped into the STEM_BOX strip of the
  masks map, which holds their AO gradient (dark at the litter, lighter up in the crown).
Normals: each card corner blends the ellipsoid's own normal (with an up bias) with half its face
normal (`project.md`, "Foliage normals": never in the card plane, so MikkTSpace tangents stay valid).
Wind (TEXCOORD_1 `wind`, the plant convention): U = 0 at and below the ground, rising with distance
from the pivot to about 1 at the crown's top and rim; V = a seeded phase per card and per stem.
LOD1/LOD2 are regenerated from the same card and stem lists (flat cards, fewer of them scaled up,
fewer stem sides), never decimated. Collider: an octagonal prism `core_r` wide and `core_h` high
round the dense lower dome, so the walker stops at the leaves (body 0.3 m + core_r near the dome's
reach) and scatter keeps other pieces' footprints out of it."""
from __future__ import annotations

import hashlib
import math
import os
import pathlib
import random

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, budgets, collider, export, geom, materials, scene, uv

import _props

ATLAS = 1024
NORMAL_SIZE = ATLAS // 2  # pitfalls: an alpha-card prop ships its normal at 512²
MASK_SIZE = 256
STEM_ROWS = 64  # atlas rows below the leaf tiles: the stems' masks strip
TILE = (ATLAS - STEM_ROWS) // 2  # 480 px leaf tiles in a 2 x 2 grid
LEAF_TILES, INNER_TILE = 4, 3
STEM_BOX = (0.02, 0.006, 0.98, 0.044)  # (u0, v0, u1, v1) of the stems' UV0 in the atlas
TILE_INSET = 4.0 / TILE
WIND_UV = "wind"
# sRGB palettes: hazel leaves a fresh mid green (between fern_clump's and the oak's), the stems a
# smooth grey-brown hazel bark.
DARK = np.array((0x26, 0x40, 0x17), np.float32) / 255
MID = np.array((0x48, 0x74, 0x2a), np.float32) / 255
LIGHT = np.array((0x8a, 0xae, 0x4c), np.float32) / 255
TWIG = np.array((0x5a, 0x48, 0x34), np.float32) / 255
BACK = np.array((0x20, 0x36, 0x14), np.float32) / 255


def unit(seed: str, *key) -> float:
    return _props.unit_hash(seed, *key)


# --- Leaf-cluster tiles (numpy, Blender pixel order: row 0 = v 0) ---

def cluster_tile(px: int, seed: str, k: int, inner: bool) -> tuple[np.ndarray, ...]:
    """One cluster of ovate, finely serrated hazel leaves on short twigs radiating from the tile
    centre, over a shadowed backing blob (alpha 1) that holds coverage down the mip chain.
    Returns (albedo RGBA with sRGB RGB and the alpha mask in A, normal RGB, roughness, ao)."""
    rng = random.Random(f"{seed}:cluster:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    rgb = np.zeros((px, px, 3), np.float32)
    alpha = np.zeros((px, px), np.float32)
    nrm = np.empty((px, px, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((px, px), 0.62, np.float32)
    ao = np.ones((px, px), np.float32)
    # Backing blob: a lumpy disc of deep shade.
    r = np.hypot(x - 0.5, y - 0.5)
    th = np.arctan2(y - 0.5, x - 0.5)
    lump = 0.27 + 0.035 * np.sin(5 * th + rng.uniform(0, 6.28)) + 0.025 * np.sin(9 * th + rng.uniform(0, 6.28))
    blob = r < lump * (1.08 if inner else 1.0)
    rgb[blob] = BACK * (0.85 + 0.3 * rng.random())
    alpha[blob] = 1.0
    ao[blob] = 0.55
    rough[blob] = 0.85
    # Twigs: short brown lines from the centre outward (drawn under the leaves).
    twigs = []
    for j in range(rng.randint(5, 7)):
        a = 2 * math.pi * (j + rng.uniform(-0.3, 0.3)) / 6
        twigs.append((a, rng.uniform(0.33, 0.41)))
    for a, ln in twigs:
        c, s = math.cos(a), math.sin(a)
        t = np.clip(((x - 0.5) * c + (y - 0.5) * s) / ln, 0, 1)
        d = np.hypot(x - 0.5 - t * ln * c, y - 0.5 - t * ln * s)
        on = (d < 0.006 * (1.2 - 0.6 * t)) & (((x - 0.5) * c + (y - 0.5) * s) > 0)
        rgb[on], alpha[on], ao[on] = TWIG, 1.0, 0.7
    # Leaves, back to front: later ones nearer the viewer, brighter and less occluded.
    n_leaves = 300 if inner else 270
    for i in range(n_leaves):
        layer = i / (n_leaves - 1)
        # Centres fill a disc evenly (coverage), each leaf pointing out along its nearest twig.
        pr, pa = 0.4 * math.sqrt(rng.random()), rng.uniform(0, 2 * math.pi)
        cx, cy = 0.5 + pr * math.cos(pa), 0.5 + pr * math.sin(pa)
        tw_a = min((t[0] for t in twigs), key=lambda t: abs(math.remainder(t - pa, 2 * math.pi)))
        ang = pa + 0.5 * math.remainder(tw_a - pa, 2 * math.pi) + rng.uniform(-0.3, 0.3)
        length = rng.uniform(0.085, 0.12)
        half_w = length * rng.uniform(0.36, 0.42)
        theta = ang + rng.uniform(-0.6, 0.6)
        c, s = math.cos(theta), math.sin(theta)
        ext = length * 0.6
        x0, x1 = max(int((cx - ext) * px), 0), min(int((cx + ext) * px) + 1, px)
        y0, y1 = max(int((cy - ext) * px), 0), min(int((cy + ext) * px) + 1, px)
        if x0 >= x1 or y0 >= y1:
            continue
        dx, dy = x[y0:y1, x0:x1] - cx, y[y0:y1, x0:x1] - cy
        sl = (dx * c + dy * s) / (length / 2)  # -1 stalk .. +1 tip
        q = -dx * s + dy * c
        # Broad-ovate with a short point (hazel): widest below the middle; fine serration.
        env = np.sqrt(np.clip(1 - sl * sl, 0, 1)) ** 0.8 * (1.0 - 0.28 * np.clip(sl, 0, 1) ** 1.5)
        serr = 0.92 + 0.08 * np.abs(np.sin((sl + 1) * math.pi * 7 + rng.uniform(0, 3)))
        half = half_w * env * serr
        inside = (np.abs(sl) <= 1) & (np.abs(q) < half)
        if not inside.any():
            continue
        tone = (rng.uniform(0.0, 0.6) + 0.5 * layer) * (0.55 if inner else 1.0)
        base = DARK + (MID - DARK) * min(tone * 1.6, 1.0) + (LIGHT - MID) * max(tone - 0.62, 0.0)
        across = np.clip(np.abs(q) / np.maximum(half, 1e-6), 0, 1)
        col = base[None, None] * ((1.0 - 0.22 * across ** 2) * (0.9 + 0.1 * (sl + 1) / 2))[..., None]
        vein = (np.abs(q) < 0.0022) | ((np.abs(np.sin((sl * 4.5 - np.abs(q) / max(half_w, 1e-3) * 3.2) * math.pi))
                                        < 0.08) & (across < 0.85))
        col = np.where(vein[..., None], col * 1.12, col)
        fold = np.sign(q) * across * 0.42
        ta, tb = rng.uniform(-0.25, 0.25), rng.uniform(-0.25, 0.25)
        nx = -s * fold + c * ta - s * tb
        ny = c * fold + s * ta + c * tb
        nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0.05, 1))
        sub = (slice(y0, y1), slice(x0, x1))
        rgb[sub] = np.where(inside[..., None], np.clip(col, 0, 1), rgb[sub])
        alpha[sub] = np.where(inside, 1.0, alpha[sub])
        nrm[sub] = np.where(inside[..., None], np.stack([nx, ny, nz], -1) * 0.5 + 0.5, nrm[sub])
        rough[sub] = np.where(inside, rng.uniform(0.5, 0.64), rough[sub])
        ao[sub] = np.where(inside, (0.62 + 0.38 * layer) * (0.8 if inner else 1.0), ao[sub])
    m = int(round(TILE_INSET * px))
    alpha[:m], alpha[-m:], alpha[:, :m], alpha[:, -m:] = 0.0, 0.0, 0.0, 0.0
    rgb = _props.dilate(rgb, alpha, iters=48)
    rgb[alpha <= 0.5] = np.where(rgb[alpha <= 0.5].sum(-1, keepdims=True) > 0, rgb[alpha <= 0.5], MID)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough, ao


def tile_origin(k: int) -> tuple[int, int]:
    """(x, y) pixel origin of leaf tile k in the atlas (2 x 2 grid above the stem rows)."""
    return (k % 2) * TILE, STEM_ROWS + (k // 2) * TILE


def atlas(seed: str) -> tuple[np.ndarray, ...]:
    """(albedo RGBA, normal RGB, roughness, ao, per-tile coverage by mip) of side ATLAS."""
    alb = np.zeros((ATLAS, ATLAS, 4), np.float32)
    alb[..., :3] = MID
    nrm = np.zeros((ATLAS, ATLAS, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    rough = np.full((ATLAS, ATLAS), 0.85, np.float32)
    ao = np.ones((ATLAS, ATLAS), np.float32)
    coverage = {}
    for k in range(LEAF_TILES):
        a, n, r, o = cluster_tile(TILE, seed, k, k == INNER_TILE)
        x0, y0 = tile_origin(k)
        sl = (slice(y0, y0 + TILE), slice(x0, x0 + TILE))
        alb[sl], nrm[sl], rough[sl], ao[sl] = a, n, r, o
        coverage[f"tile_{k}"] = _props.coverage_by_mip(a[..., 3])
    coverage["tiles"] = _props.coverage_by_mip(alb[STEM_ROWS:STEM_ROWS + 2 * TILE, : 2 * TILE, 3])
    return alb, nrm, rough, ao, coverage


# --- Geometry (Blender frame, Z up, pivot at the stem cluster's base) ---

def cards(seed: str, crown: dict) -> list[dict]:
    """Outer cards on the ellipsoid shell (Fibonacci points, skipping the underside) and inner
    cards inside it. Each: centre p, normal n, size, roll, tile, kind, phase."""
    rng = random.Random(f"{seed}:cards")
    cx, cz, a, b = crown["cx"], crown["cz"], crown["a"], crown["b"]
    out = []
    n = crown["outer"]
    k = 0
    for i in range(n * 3):
        if k >= n:
            break
        zf = 1 - 2 * (i + 0.5) / (n * crown["spread"])  # top to well below the equator
        if zf < crown["zmin"]:
            break
        rr = math.sqrt(max(0.0, 1 - zf * zf))
        phi = i * 2.39996 + rng.uniform(-0.15, 0.15)
        d = Vector((rr * math.cos(phi), rr * math.sin(phi), zf))
        jitter = rng.uniform(0.86, 1.0)
        p = Vector((cx + a * d.x * jitter, a * d.y * jitter, cz + b * d.z * jitter))
        nrm = Vector((d.x / a, d.y / a, d.z / b)).normalized()
        nrm = (nrm + Vector((0, 0, 0.25))).normalized()
        out.append({"p": p, "n": nrm, "size": crown["card"] * rng.uniform(0.85, 1.12), "roll": rng.uniform(0, 6.28),
                    "tile": k % (LEAF_TILES - 1), "kind": "outer", "phase": unit(seed, "card", len(out))})
        k += 1
    for i in range(crown["inner"]):
        zf = rng.uniform(-0.45, 0.7)
        phi = rng.uniform(0, 2 * math.pi)
        rad = rng.uniform(0.3, 0.68)
        rr = math.sqrt(max(0.0, 1 - zf * zf)) * rad
        p = Vector((cx + a * rr * math.cos(phi), a * rr * math.sin(phi), cz + b * zf * rad))
        nrm = Vector((math.cos(phi) * rr, math.sin(phi) * rr, 0.6 + zf)).normalized()
        out.append({"p": p, "n": nrm, "size": crown["card"] * rng.uniform(0.9, 1.15), "roll": rng.uniform(0, 6.28),
                    "tile": INNER_TILE, "kind": "inner", "phase": unit(seed, "card", len(out))})
    return out


def stems(seed: str, spec: dict) -> list[dict]:
    """Woody stems from the base cluster, arching up and out into the crown: points and radii."""
    rng = random.Random(f"{seed}:stems")
    out = []
    n = spec["count"]
    for i in range(n):
        az = 2 * math.pi * (i + rng.uniform(-0.25, 0.25)) / n
        h = Vector((math.cos(az), math.sin(az), 0.0))
        base = h * rng.uniform(0.02, 0.1) + Vector((0, 0, -spec["sink"]))
        length = spec["length"] * rng.uniform(0.85, 1.1)
        el0, el1 = math.radians(rng.uniform(76, 86)), math.radians(rng.uniform(48, 62))
        pts, p, steps = [base.copy()], base.copy(), 12
        for j in range(steps):
            t = (j + 0.5) / steps
            el = el0 + (el1 - el0) * t ** 1.3
            p = p + (h * math.cos(el) + Vector((0, 0, math.sin(el)))) * (length / steps)
            pts.append(p.copy())
        radii = [spec["r0"] + (spec["r1"] - spec["r0"]) * (j / steps) ** 0.8 for j in range(steps + 1)]
        out.append({"pts": pts, "radii": radii, "phase": unit(seed, "stem", i)})
    return out


def wind_u(p: Vector, reach: float) -> float:
    """Plant wind U: 0 at and below the ground, rising with distance from the pivot."""
    if p.z <= 0.0:
        return 0.0
    return min(1.0, (p.length / reach) ** 1.1)


def card_mesh(name: str, cs: list[dict], every: int, grid: int, scale: float, reach: float, crown: dict,
              keep_inner: int) -> bpy.types.Object:
    """Cluster cards: every `every`-th outer card and every `keep_inner`-th inner one, each a
    grid x grid quad (grid 2: its centre lifted CUSHION out, a soft cushion; 1: flat)."""
    verts, faces, uvs, winds, normals = [], [], [], [], []
    io = ii = 0
    cx, cz, a, b = crown["cx"], crown["cz"], crown["a"], crown["b"]
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
        x0, y0 = tile_origin(c["tile"])
        base = len(verts)
        for j in range(grid + 1):
            for i in range(grid + 1):
                su, sv = i / grid, j / grid
                q = c["p"] + u_ax * ((su - 0.5) * 2 * half) + v_ax * ((sv - 0.5) * 2 * half)
                if grid == 2 and i == 1 and j == 1:
                    q = q + nrm * crown["cushion"] * scale
                elif grid == 2:
                    q = q - nrm * crown["cushion"] * 0.25 * scale * (abs(su - 0.5) + abs(sv - 0.5))
                verts.append(tuple(q))
                ins = TILE_INSET * 0.5
                uvs.append(((x0 + TILE * (ins + su * (1 - 2 * ins))) / ATLAS,
                            (y0 + TILE * (ins + sv * (1 - 2 * ins))) / ATLAS))
                winds.append((wind_u(q, reach), c["phase"]))
                d = Vector(((q.x - cx) / a ** 2, q.y / a ** 2, (q.z - cz) / b ** 2)).normalized()
                normals.append((d + Vector((0, 0, 0.35))).normalized())
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


def stem_mesh(name: str, ss: list[dict], keep: int, sides: int, stride: int, reach: float) -> bpy.types.Object:
    """The first `keep` stems as open tubes (`sides` sides, every `stride`-th skeleton point), UV0 in
    STEM_BOX (u around, v along), wind per vertex."""
    bm = bmesh.new()
    uvl = bm.loops.layers.uv.new("UVMap")
    wnd = bm.loops.layers.uv.new(WIND_UV)
    u0, v0, u1, v1 = STEM_BOX
    for si, st in enumerate(ss[:keep]):
        idx = list(range(0, len(st["pts"]), stride))
        if idx[-1] != len(st["pts"]) - 1:
            idx.append(len(st["pts"]) - 1)
        rings, uvrows, wrows = [], [], []
        for n_, k in enumerate(idx):
            p, r = st["pts"][k], st["radii"][k]
            d = (st["pts"][min(k + 1, len(st["pts"]) - 1)] - st["pts"][max(k - 1, 0)]).normalized()
            ref = Vector((0, 0, 1)) if abs(d.z) < 0.95 else Vector((1, 0, 0))
            e1 = d.cross(ref).normalized()
            e2 = d.cross(e1).normalized()
            ring = [bm.verts.new(p + (e1 * math.cos(2 * math.pi * j / sides) + e2 * math.sin(2 * math.pi * j / sides)) * r)
                    for j in range(sides)]
            rings.append(ring)
            t = k / (len(st["pts"]) - 1)
            uvrows.append([(u0 + (u1 - u0) * (si + j / sides) / len(ss), v0 + (v1 - v0) * t) for j in range(sides + 1)])
            wrows.append((wind_u(p, reach), st["phase"]))
        for a_ in range(len(rings) - 1):
            for j in range(sides):
                jn = (j + 1) % sides
                f = bm.faces.new((rings[a_][j], rings[a_][jn], rings[a_ + 1][jn], rings[a_ + 1][j]))
                f.smooth = True
                for lp, (ra, cj) in zip(f.loops, ((a_, j), (a_, j + 1), (a_ + 1, j + 1), (a_ + 1, j))):
                    lp[uvl].uv = uvrows[ra][cj]
                    lp[wnd].uv = wrows[ra]
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.uv_layers.active = me.uv_layers["UVMap"]
    return scene.link(bpy.data.objects.new(name, me))


def build(piece: str, out: pathlib.Path, generator: str, *, crown: dict, stem: dict, lods: tuple[dict, ...],
          height: tuple[float, float], reach_range: tuple[float, float], core_r: float, core_h: float) -> None:
    """The whole shrub: geometry, numpy maps, LOD joins, collider, export and reports into `out`."""
    scene.reset_scene(piece)
    leaf_mat = materials.pbr_material(f"{piece}_leaves", materials.srgb("#48742a"), 0.6)
    stem_mat = materials.pbr_material(f"{piece}_stems", materials.srgb("#6b5a48"), 0.8)
    cs = cards(piece, crown)
    ss = stems(piece, stem)
    reach_w = crown["a"] + 0.6 * crown["card"]
    top_w = crown["cz"] + crown["b"] + 0.4 * crown["card"]
    wreach = math.hypot(reach_w, top_w) * 0.95
    lod_objs = []
    for i, c in enumerate(lods):
        leaves = card_mesh(f"leaves_l{i}", cs, c["every"], c["grid"], c["scale"], wreach, crown, c["inner"])
        leaves.data.materials.append(leaf_mat)
        st = stem_mesh(f"stems_l{i}", ss, c["stems"], c["sides"], c["stride"], wreach)
        st.data.materials.append(stem_mat)
        o = geom.join([leaves, st], f"{piece}_lod{i}")
        lod_objs.append(o)
    tris = [geom.tri_count(o) for o in lod_objs]
    leaf0 = lod_objs[0]
    # Per-material texel density on LOD0 (leaves only: stems are colour + masks).
    tmp = leaf0.copy()
    tmp.data = leaf0.data.copy()
    scene.link(tmp)
    bm = bmesh.new()
    bm.from_mesh(tmp.data)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if tmp.data.materials[f.material_index].name != leaf_mat.name],
                     context="FACES")
    bm.to_mesh(tmp.data)
    bm.free()
    texel = {leaf_mat.name: uv.measure_texel_density(tmp, ATLAS)}
    bpy.data.objects.remove(tmp, do_unlink=True)
    zs = [v.co.z for v in leaf0.data.vertices]
    reach = max(math.hypot(v.co.x, v.co.y) for o in lod_objs for v in o.data.vertices)
    low_leaf = min(v.co.z for p in leaf0.data.polygons if leaf0.data.materials[p.material_index].name == leaf_mat.name
                   for v in (leaf0.data.vertices[k] for k in p.vertices))
    print(f"{piece}: tris per LOD {tris}, cards {sum(c['kind'] == 'outer' for c in cs)} outer + "
          f"{sum(c['kind'] == 'inner' for c in cs)} inner, stems {len(ss)}, texel {texel}, "
          f"z {min(zs):.3f}..{max(zs):.3f}, reach {reach:.3f}, lowest leaf {low_leaf:.3f}")
    budgets.check(tuple(tris), "prop", piece)
    assert texel[leaf_mat.name]["median"] >= 256, texel
    assert height[0] <= max(zs) <= height[1] and reach_range[0] <= reach <= reach_range[1], (max(zs), reach)

    # Maps: numpy atlas (albedo with alpha, normal at 512², ORM with the tile AO), masks.
    alb, nrm, rough, ao, coverage = atlas(piece)
    for k in range(LEAF_TILES):
        assert coverage[f"tile_{k}"][0] >= 0.5, coverage
    n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32)
    orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(ATLAS, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    # masks (linear): leaves R snow 0.6 (a dense dome holds snow), G porosity 0.6, A = 1; the stem
    # strip R 0.3, G 0.5, A from 0.5 at the litter to 0.9 up in the crown (and a halo of it, so the
    # mips under the stems keep their values).
    mk = np.empty((MASK_SIZE, MASK_SIZE, 4), np.float32)
    mk[:] = (0.6, 0.6, 0.0, 1.0)
    su0, sv0, su1, sv1 = STEM_BOX
    rows = slice(0, STEM_ROWS * MASK_SIZE // ATLAS)  # the whole reserved strip: a halo of 4+ texels
    assert (sv1 * MASK_SIZE) + 4 <= rows.stop, (sv1, rows)
    vv = (np.arange(MASK_SIZE, dtype=np.float32) + 0.5) / MASK_SIZE
    t = np.clip((vv - sv0) / (sv1 - sv0), 0, 1)
    mk[rows, :, 0] = 0.3
    mk[rows, :, 1] = 0.5
    mk[rows, :, 3] = (0.5 + 0.4 * t[rows])[:, None]
    bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))

    # Wind self-check: every LOD carries `wind` in [0, 1], U = 0 at and below the ground.
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

    # Collider: an octagonal prism round the dense lower dome.
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
    print(f"{piece}: coverage {coverage}")

    root = scene.empty(piece)
    for o in lod_objs:
        o.parent = root
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lod_objs[0]]),
                           device="none (numpy-drawn tiles, no Cycles bake)", samples=0, marginPx=0,
                           channels={"albedo": {"size": ATLAS, "colorspace": "srgb"},
                                     "normal": {"size": NORMAL_SIZE, "colorspace": "linear"},
                                     "orm": {"size": ATLAS, "colorspace": "linear"},
                                     "masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=texel, bakeStats={"alphaCoverageByMip": coverage, "alphaCutoff": 0.5},
                           extentZ=[min(zs), max(zs)], reachM=reach, lowestLeafM=low_leaf, crown=crown, stem=stem,
                           coreRadiusM=core_r, skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
