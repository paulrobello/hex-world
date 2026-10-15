"""Wildflower-clump variants of wildflower_clump (PRD §6 Variety): the same ~1 m clump of meadow
grass and alpha-tested cards, with another species mix (refs: assets-source/refs/meadow_props).
`wildflower_clump.py` and `_props.py` are frozen with the approved original (provenance hashes
them), so this module carries the original's clump layout and LOD builder over unchanged in
shape and height range, and draws its tiles with `_props`' own tuft, blade and flower painters.

What differs is the species: `species` (a subset of `_props.SPECIES`) fills the eight species
tuft tiles (four variants each for two species) and the three top-view rosette cards, which carry
four heads of each species instead of two of each of the original's four, so the strategic camera
reads the variant's colours."""
from __future__ import annotations

import math
import os
import pathlib
import random

import bpy
import numpy as np
from mathutils import Vector

from lib import bake, collider, export, geom, materials, scene, uv
import _props

TILE_PX, MASK_SIZE = 256, 64
ATLAS = TILE_PX * _props.GRID
NORMAL_SIZE = ATLAS // 2
TUFTS, RADIUS, SINK = 32, 0.42, 0.07


def layout(species: tuple[str, ...]) -> dict:
    """`_props.LAYOUT` with the eight species tiles (rows 1 and 2) split evenly over `species`."""
    per = 8 // len(species)
    lay = {cr: v for cr, v in _props.LAYOUT.items() if v[1] is None}
    cells = [(c, r) for r in (1, 2) for c in range(_props.GRID)]
    for i, cr in enumerate(cells):
        lay[cr] = ("tuft", species[i // per], i % per)
    return lay


def top_tile(px: int, seed: str, k: int, species: tuple[str, ...]):
    """`_props.top_tile` with the heads drawn from `species` (eight heads, as the original)."""
    rng = random.Random(f"{seed}:top:{k}")
    ys, xs = np.mgrid[0:px, 0:px].astype(np.float32)
    x, y = (xs + 0.5) / px, (ys + 0.5) / px
    rgb = np.zeros((px, px, 3), np.float32)
    alpha = np.zeros((px, px), np.float32)
    nrm = np.empty((px, px, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    for i in range(110):
        a = rng.uniform(0, 2 * math.pi)
        r0, r1 = rng.uniform(0.0, 0.1), rng.uniform(0.3, 0.44)
        tone = rng.random()
        col = _props.GRASS_DARK + (_props.GRASS_LIGHT - _props.GRASS_DARK) * tone
        _props._blade(rgb, alpha, nrm, x, y, 0.5 + r0 * math.cos(a), 0.5 + r0 * math.sin(a),
                      0.5 + r1 * math.cos(a), 0.5 + r1 * math.sin(a), rng.uniform(0.06, 0.09), col, rng)
    order = list(species) * (8 // len(species))
    rng.shuffle(order)
    for i, sp in enumerate(order):
        a = 2 * math.pi * (i + rng.random() * 0.6) / len(order)
        d = rng.uniform(0.1, 0.3)
        _props.flower_head(rgb, alpha, nrm, x, y, 0.5 + d * math.cos(a), 0.5 + d * math.sin(a),
                           rng.uniform(0.065, 0.09), sp, rng)
    rough = np.where(alpha > 0.5, 0.7, 0.8).astype(np.float32)
    rgb = _props.dilate(rgb, alpha)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough


def clump_atlas(px: int, seed: str, lay: dict, species: tuple[str, ...]):
    """`_props.clump_atlas` over layout `lay` (tiles from `_props.tuft_tile` and `top_tile`)."""
    n = _props.GRID * px
    alb = np.zeros((n, n, 4), np.float32)
    nrm = np.zeros((n, n, 3), np.float32)
    rough = np.zeros((n, n), np.float32)
    ao = np.ones((n, n), np.float32)
    v = (np.arange(px, dtype=np.float32) + 0.5) / px
    for (c, r), (kind, sp, k) in lay.items():
        a, nm, ro = top_tile(px, seed, k, species) if kind == "top" else _props.tuft_tile(px, sp, seed, k)
        sl = (slice(r * px, (r + 1) * px), slice(c * px, (c + 1) * px))
        alb[sl], nrm[sl], rough[sl] = a, nm, ro
        if kind == "tuft":
            ao[sl] = (0.65 + 0.35 * np.clip(v / 0.5, 0, 1))[:, None]
    return alb, nrm, rough, ao


def build(piece: str, out: pathlib.Path, generator: str, *, species: tuple[str, ...]) -> None:
    """wildflower_clump.py's pipeline for variant `piece` with flower species `species`."""
    assert 8 % len(species) == 0 and set(species) <= set(_props.SPECIES), species
    lay = layout(species)
    scene.reset_scene(piece)
    rng = random.Random(piece)
    tuft_tiles = {sp: [cr for cr, (kind, s, _) in lay.items() if kind == "tuft" and s == sp]
                  for sp in (None, *species)}
    top_tiles = [cr for cr, (kind, _, _) in lay.items() if kind == "top"]
    eps = 0.5 / ATLAS

    def tile_uv(cr, s, t):
        c, r = cr
        g = _props.GRID
        return (c / g + eps + s * (1 / g - 2 * eps), r / g + eps + t * (1 / g - 2 * eps))

    tufts = []
    for i in range(TUFTS):
        a = 2 * math.pi * rng.random()
        d = RADIUS * math.sqrt(rng.random()) * (0.35 + 0.65 * rng.random())
        centre = d / RADIUS
        sp = None if rng.random() < 0.15 + 0.4 * centre else species[i % len(species)]
        tufts.append({"pos": (d * math.cos(a), d * math.sin(a)), "rot": rng.uniform(0, math.pi / 3),
                      "h": rng.uniform(0.36, 0.5) * (1.2 - 0.6 * centre), "w": rng.uniform(0.32, 0.44),
                      "lean": rng.uniform(0.04, 0.1), "sp": sp,
                      "phase": _props.unit_hash(piece, "tuft", i), "sway": rng.uniform(0.85, 1.0)})
    tops = []
    for i, cr in enumerate(top_tiles):
        a = 2 * math.pi * (i + rng.uniform(-0.2, 0.2)) / len(top_tiles)
        d = rng.uniform(0.1, 0.22)
        tops.append({"pos": (d * math.cos(a), d * math.sin(a)), "rot": rng.uniform(0, 2 * math.pi),
                     "z": rng.uniform(0.2, 0.25), "s": rng.uniform(0.5, 0.6), "tile": cr,
                     "phase": _props.unit_hash(piece, "top", i)})

    def clump_normal(p, face_n, keep=0.45):
        radial = Vector((p.x, p.y, 0.0)) / (RADIUS * 1.2) + Vector((0.0, 0.0, 0.9))
        n = face_n.normalized() * keep + radial.normalized() * (1.0 - keep)
        return n.normalized()

    def build_lod(name, every, cards_per_tuft, rows, wscale, hscale):
        verts, tris, uvs, winds, nrms = [], [], [], [], []

        def quad_strip(cols):
            base = len(verts)
            n = len(cols[0])
            for col in cols:
                for p, uv_, w, nm in col:
                    verts.append(tuple(p))
                    uvs.append(uv_)
                    winds.append(w)
                    nrms.append(tuple(nm))
            for j in range(n - 1):
                a, b, c, d = base + j, base + n + j, base + n + j + 1, base + j + 1
                tris.extend(([a, b, c], [a, c, d]))

        for i, t in enumerate(tufts):
            if i % every:
                continue
            cx, cy = t["pos"]
            h, w = t["h"] * hscale, t["w"] * wscale
            pal = tuft_tiles[t["sp"]]
            for k in range(cards_per_tuft):
                th = t["rot"] + k * math.pi / cards_per_tuft
                along = Vector((math.cos(th), math.sin(th), 0.0))
                fn = Vector((-math.sin(th), math.cos(th), 0.0))
                if fn.dot(Vector((cx, cy, 0.0))) < 0:
                    fn = -fn
                grass = tuft_tiles[None][(i + k) % len(tuft_tiles[None])]
                tile = grass if (t["sp"] is None or k == 2) else pal[(i + k) % len(pal)]
                cols = []
                for s in (0.0, 1.0):
                    col = []
                    for j in range(rows + 1):
                        f = j / rows
                        p = Vector((cx, cy, -SINK)) + along * (s - 0.5) * w + Vector((0, 0, h * f))
                        p += fn * t["lean"] * f * f
                        sway = t["sway"] * f ** 1.5 if p.z > 0 else 0.0
                        col.append((p, tile_uv(tile, s, f), (sway, t["phase"]), clump_normal(p, fn)))
                    cols.append(col)
                if along.cross(fn).z < 0:
                    cols.reverse()
                quad_strip(cols)
        for t in tops:
            cx, cy = t["pos"]
            s = t["s"] * (wscale ** 0.5)
            th = t["rot"]
            fn = (Vector((cx, cy, 0.0)).normalized() * 0.35 + Vector((0.0, 0.0, 1.0))).normalized()
            ay = fn.cross(Vector((math.cos(th), math.sin(th), 0.0))).normalized()
            ax = ay.cross(fn).normalized()
            cols = []
            for su in (0.0, 1.0):
                col = []
                for sv in (0.0, 1.0):
                    p = Vector((cx, cy, t["z"])) + ax * (su - 0.5) * s + ay * (sv - 0.5) * s
                    col.append((p, tile_uv(t["tile"], su, sv), (0.7 if p.z > 0 else 0.0, t["phase"]),
                                clump_normal(p, fn, keep=0.6)))
                cols.append(col)
            quad_strip(cols)
        me = bpy.data.meshes.new(name)
        me.from_pydata(verts, [], tris)
        me.validate(clean_customdata=False)
        uvl = me.uv_layers.new(name="UVMap")
        wl = me.uv_layers.new(name="wind")
        me.uv_layers.active = uvl
        loop_n = []
        for loop in me.loops:
            uvl.data[loop.index].uv = uvs[loop.vertex_index]
            u, v = winds[loop.vertex_index]
            wl.data[loop.index].uv = (min(max(u, 0.0), 1.0), min(max(v, 0.0), 1.0))
            loop_n.append(nrms[loop.vertex_index])
        for p in me.polygons:
            p.use_smooth = True
        me.normals_split_custom_set(loop_n)
        me.update()
        o = scene.link(bpy.data.objects.new(name, me))
        o.data.materials.append(mat)
        return o

    mat = materials.pbr_material(f"{piece}_foliage", materials.srgb("#5d7f2e"), 0.75)
    lods = [build_lod(f"{piece}_lod0", 1, 3, 3, 1.0, 1.0),
            build_lod(f"{piece}_lod1", 2, 3, 2, 1.3, 1.1),
            build_lod(f"{piece}_lod2", 6, 2, 1, 1.7, 1.15)]
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    texel = uv.measure_texel_density(lods[0], ATLAS)
    assert texel["median"] >= 256, texel

    alb, nrm, rough, ao = clump_atlas(TILE_PX, piece, lay, species)
    albedo = alb.reshape(-1, 4)
    n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32).reshape(-1, 4)
    orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1).reshape(-1, 4)
    bake.write_png(ATLAS, albedo, str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, normal, str(out / f"{piece}.normal.png"))
    bake.write_png(ATLAS, orm, str(out / f"{piece}.orm.png"))
    bake.make_masks(MASK_SIZE, 0.5, 0.6, 0.0, None, str(out / f"{piece}.masks.png"))
    coverage = {}
    for (c, r), (kind, sp, k) in lay.items():
        a = alb[r * TILE_PX:(r + 1) * TILE_PX, c * TILE_PX:(c + 1) * TILE_PX, 3]
        coverage[f"{kind}_{sp or 'grass'}_{k}"] = _props.coverage_by_mip(a)
    coverage["atlas"] = _props.coverage_by_mip(alb[..., 3])

    for o in lods:
        wl = o.data.uv_layers["wind"].data
        w = np.empty(len(wl) * 2)
        wl.foreach_get("uv", w)
        assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
        for loop in o.data.loops:
            if o.data.vertices[loop.vertex_index].co.z <= 0.0:
                assert wl[loop.index].uv[0] == 0.0, o.name
    assert [u.name for u in lods[0].data.uv_layers] == ["UVMap", "wind"]

    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    zs = [v.co.z for v in lods[0].data.vertices]
    reach = max(math.hypot(v.co.x, v.co.y) for v in col.data.vertices)
    print(f"{piece}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, collider reach "
          f"{reach:.3f}, coverage atlas {coverage['atlas']}")

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
                           extentZ=[min(zs), max(zs)], sinkM=SINK, tufts=TUFTS, species=list(species),
                           skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
