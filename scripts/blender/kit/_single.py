"""Single-stem wildflowers (owner, 2026-10-14: "Wildflowers do not always have to be in a clump"),
one species per piece, spread one by one across the meadow grass by their own scatter rule. The
species, colours and drawing are the clumps' (`_props`: refs assets-source/refs/meadow_props), so
a single daisy matches the daisies in `wildflower_clump` and `wildflower_clump_b`.

Kept cheap because a tile carries dozens: LOD0 is two crossed vertical cards of three rows (the
stem, its leaves and the head seen from the side, 12 tris) plus one head card tilted up for the
strategic camera (2 tris); LOD1 the two cards in one row plus the head card (6); LOD2 the head card
alone (2, 14% of LOD0), the colour dot a distant camera sees. Every card's bottom edge starts SINK
below the pivot, so the stem never floats over the relief.

Sized for the chunky meadow grass (blades about 0.35 m, up to 0.7 m): the stems are 0.7-0.95 m (clover, daisy, buttercup, cornflower),
so the heads stand above it, and a head is about 0.15 m across. Side cards are SIDE_ASPECT times as
tall as they are wide, and their tiles (SIDE_W x SIDE_H px) have that aspect, so a head is drawn
round, not stretched. Atlas (ATLAS px square): two side tiles in the left half, two top tiles of
TOP_PX in the right half. The head card sits at the side view's head height (HEAD_V), so a low
camera sees its edge inside the side-view head rather than as a sliver above it.
Wind (TEXCOORD_1 `wind`, the clumps' convention): U = 0 at and below the ground rising to about 1 at
the head, V = one seeded phase for the whole flower."""
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

ATLAS, MASK_SIZE = 512, 64
NORMAL_SIZE = ATLAS // 2
SIDE_W, SIDE_H, TOP_PX = 128, 512, 256
SIDE_ASPECT = SIDE_H / SIDE_W
SINK, HEAD_CARD, MARGIN = 0.05, 0.26, 0.06
HEAD_R = (0.3, 0.34)  # side-view head radius, in card widths
HEAD_V = (SIDE_ASPECT - MARGIN - sum(HEAD_R) / 2 - 0.025) / SIDE_ASPECT
# Atlas rectangles (u0, v0, u1, v1): side tiles 0 and 1, top tiles 0 and 1.
SIDE_RECTS = ((0.0, 0.0, 0.25, 1.0), (0.25, 0.0, 0.5, 1.0))
TOP_RECTS = ((0.5, 0.0, 1.0, 0.5), (0.5, 0.5, 1.0, 1.0))


def _canvas(w: int, h: int):
    ys, xs = np.mgrid[0:h, 0:w].astype(np.float32)
    rgb = np.zeros((h, w, 3), np.float32)
    nrm = np.empty((h, w, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    # Both axes in units of the tile width, so circles stay round on a tall tile.
    return (xs + 0.5) / w, (ys + 0.5) / w, rgb, np.zeros((h, w), np.float32), nrm


def side_tile(species: str, seed: str, k: int):
    """One flower from the side on a SIDE_W x SIDE_H tile (x in [0, 1] across, y in [0, SIDE_ASPECT]
    up, both in card widths): a stem, three or four strap leaves from its base and one head near the
    top; alpha 0 a margin inside every edge but the bottom (sunk)."""
    rng = random.Random(f"{seed}:side:{k}")
    x, y, rgb, alpha, nrm = _canvas(SIDE_W, SIDE_H)
    g0, g1 = _props.GRASS_DARK, _props.GRASS_LIGHT
    for i in range(rng.randint(3, 4)):
        side = -1 if i % 2 else 1
        bx = 0.5 + side * rng.uniform(0.0, 0.04)
        ln = rng.uniform(0.9, 1.5)
        ang = math.radians(rng.uniform(6, 18)) * side
        tx, ty = bx + math.sin(ang) * ln, 0.08 + math.cos(ang) * ln
        col = g0 + (g1 - g0) * rng.uniform(0.25, 0.6)
        _props._blade(rgb, alpha, nrm, x, y, bx, 0.06, min(max(tx, MARGIN), 1 - MARGIN), ty,
                      rng.uniform(0.16, 0.2), col, rng)
    r = rng.uniform(*HEAD_R) * (1.08 if species == "clover" else 1.0)
    hx = 0.5 + rng.uniform(-0.04, 0.04)
    hy = SIDE_ASPECT - MARGIN - r - rng.uniform(0.0, 0.05)
    _props._blade(rgb, alpha, nrm, x, y, 0.5, 0.04, hx, hy, 0.07, g0 * 1.2, rng)
    _props.flower_head(rgb, alpha, nrm, x, y, hx, hy, r, species, rng)
    rough = np.where(alpha > 0.5, 0.7, 0.8).astype(np.float32)
    rgb = _props.dilate(rgb, alpha)
    rgb[alpha <= 0.5] = np.where(rgb[alpha <= 0.5].sum(-1, keepdims=True) > 0, rgb[alpha <= 0.5], g0)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough


def top_tile(species: str, seed: str, k: int):
    """One flower from above on a TOP_PX tile: the head over a few leaf tips, inside a disc well
    inside the tile."""
    rng = random.Random(f"{seed}:top:{k}")
    x, y, rgb, alpha, nrm = _canvas(TOP_PX, TOP_PX)
    g0, g1 = _props.GRASS_DARK, _props.GRASS_LIGHT
    for i in range(5):
        a = 2 * math.pi * (i + rng.uniform(-0.25, 0.25)) / 5
        r1 = rng.uniform(0.38, 0.45)
        _props._blade(rgb, alpha, nrm, x, y, 0.5, 0.5, 0.5 + r1 * math.cos(a), 0.5 + r1 * math.sin(a),
                      rng.uniform(0.12, 0.15), g0 + (g1 - g0) * rng.uniform(0.2, 0.6), rng)
    _props.flower_head(rgb, alpha, nrm, x, y, 0.5, 0.5, rng.uniform(0.31, 0.34), species, rng)
    rough = np.where(alpha > 0.5, 0.7, 0.8).astype(np.float32)
    rgb = _props.dilate(rgb, alpha)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough


def atlas(species: str, seed: str):
    alb = np.zeros((ATLAS, ATLAS, 4), np.float32)
    nrm = np.zeros((ATLAS, ATLAS, 3), np.float32)
    rough = np.zeros((ATLAS, ATLAS), np.float32)
    ao = np.ones((ATLAS, ATLAS), np.float32)
    v = (np.arange(SIDE_H, dtype=np.float32) + 0.5) / SIDE_H
    tiles = [(rect, side_tile(species, seed, k)) for k, rect in enumerate(SIDE_RECTS)]
    tiles += [(rect, top_tile(species, seed, k)) for k, rect in enumerate(TOP_RECTS)]
    for rect, (a, nm, ro) in tiles:
        sl = rect_slice(rect)
        alb[sl], nrm[sl], rough[sl] = a, nm, ro
        if rect in SIDE_RECTS:
            ao[sl] = (0.7 + 0.3 * np.clip(v / 0.3, 0, 1))[:, None]
    return alb, nrm, rough, ao


def rect_slice(rect):
    u0, v0, u1, v1 = (int(round(t * ATLAS)) for t in rect)
    return slice(v0, v1), slice(u0, u1)


def build(piece: str, out: pathlib.Path, generator: str, *, species: str, height: float) -> None:
    """A single `species` flower `height` m tall (head centre about 0.05 m below)."""
    assert species in _props.SPECIES, species
    scene.reset_scene(piece)
    rng = random.Random(piece)
    eps = 0.5 / ATLAS
    phase = _props.unit_hash(piece, "flower")
    rot = rng.uniform(0, math.pi / 2)
    card_w = (height + SINK) / SIDE_ASPECT

    def tile_uv(rect, s, t):
        u0, v0, u1, v1 = rect
        return (u0 + eps + s * (u1 - u0 - 2 * eps), v0 + eps + t * (v1 - v0 - 2 * eps))

    def build_lod(name, side_rows, head):
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

        up = Vector((0.0, 0.0, 1.0))
        h = height + SINK
        for k in range(2 if side_rows else 0):
            th = rot + k * math.pi / 2
            along = Vector((math.cos(th), math.sin(th), 0.0))
            fn = Vector((-math.sin(th), math.cos(th), 0.0))
            n = (fn * 0.45 + up * 0.55).normalized()
            cols = []
            for s in (0.0, 1.0):
                col = []
                for j in range(side_rows + 1):
                    f = j / side_rows
                    p = Vector((0.0, 0.0, -SINK)) + along * (s - 0.5) * card_w + Vector((0, 0, h * f))
                    sway = min(1.0, (p.z / height) ** 1.5) if p.z > 0 else 0.0
                    col.append((p, tile_uv(SIDE_RECTS[k], s, f), (sway, phase), n))
                cols.append(col)
            if along.cross(fn).z < 0:
                cols.reverse()
            quad_strip(cols)
        if head:
            tilt = Vector((math.cos(rot + 0.8), math.sin(rot + 0.8), 0.0))
            fn = (tilt * 0.3 + up).normalized()
            ay = fn.cross(Vector((math.cos(rot), math.sin(rot), 0.0))).normalized()
            ax = ay.cross(fn).normalized()
            c0 = Vector((0.0, 0.0, -SINK + HEAD_V * (height + SINK)))
            cols = []
            for su in (0.0, 1.0):
                col = []
                for sv in (0.0, 1.0):
                    p = c0 + ax * (su - 0.5) * HEAD_CARD + ay * (sv - 0.5) * HEAD_CARD
                    col.append((p, tile_uv(TOP_RECTS[0], su, sv), (0.9, phase), (fn * 0.6 + up * 0.4).normalized()))
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
    lods = [build_lod(f"{piece}_lod0", 3, True), build_lod(f"{piece}_lod1", 1, True),
            build_lod(f"{piece}_lod2", 0, True)]
    tris = [geom.tri_count(o) for o in lods]
    assert tris[0] <= 1500 and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    texel = uv.measure_texel_density(lods[0], ATLAS)
    assert texel["median"] >= 256, texel

    alb, nrm, rough, ao = atlas(species, piece)
    n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32).reshape(-1, 4)
    orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1).reshape(-1, 4)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, normal, str(out / f"{piece}.normal.png"))
    bake.write_png(ATLAS, orm, str(out / f"{piece}.orm.png"))
    bake.make_masks(MASK_SIZE, 0.5, 0.6, 0.0, None, str(out / f"{piece}.masks.png"))
    coverage = {f"side_{k}": _props.coverage_by_mip(alb[rect_slice(rect)][..., 3]) for k, rect in enumerate(SIDE_RECTS)}
    coverage.update({f"top_{k}": _props.coverage_by_mip(alb[rect_slice(rect)][..., 3]) for k, rect in enumerate(TOP_RECTS)})

    for o in lods:
        wl = o.data.uv_layers["wind"].data
        w = np.empty(len(wl) * 2)
        wl.foreach_get("uv", w)
        assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
        for loop in o.data.loops:
            if o.data.vertices[loop.vertex_index].co.z <= 0.0:
                assert wl[loop.index].uv[0] == 0.0, o.name
    assert [u.name for u in lods[0].data.uv_layers] == ["UVMap", "wind"]

    col = collider.collider_from(lods[0], "box", f"{piece}_collider")
    zs = [v.co.z for v in lods[0].data.vertices]
    reach = max(math.hypot(v.co.x, v.co.y) for v in col.data.vertices)
    print(f"{piece}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, collider reach "
          f"{reach:.3f}, coverage {coverage}")

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
                           extentZ=[min(zs), max(zs)], sinkM=SINK, species=species, heightM=height,
                           skipped=os.environ.get("HEXW_SKIP_BAKE") == "1")
