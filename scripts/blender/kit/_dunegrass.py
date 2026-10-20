"""Marram dune-grass clumps on their own low sand mounds (refs: assets-source/refs/dune_grass, approved
2026-10-20, BD-A2; owner BD-Q3: each clump on its own mound), scatter props of the beach tiles' dry
sand. ref-1 gives the plant: narrow blue-green blades fading to straw tips, a few dead blades and
two seed spikes on a small sand mound.

Alpha-tested cards on the leaf-card route of fern_clump and wildflower_clump: the blade tiles are
drawn in numpy (`atlas`), so no Cycles bake is needed. The atlas is 2 x 2 tiles of 512 px; each
tile is a fan of blades from its bottom centre, drawn back to front (dead straw blades first, seed
spikes on tiles 1 and 3 when the clump has them). A clump is `Clump.cards` near-vertical radial
cards round its axis, each rooted SINK below the mound's surface, leaning out at the top and bowed
in four segments at LOD0. Card normals blend a rounded clump normal with the card's own (never in the card
plane, so MikkTSpace tangents stay valid). LOD1 and LOD2 are card subsets drawn wider, never
decimated.

The mound is its own slot, `<id>_sand`: the world-space `plaster` set at `beach_sand`'s tint (the
manifest entry), so it is the beach's own sand wherever the scatter puts it. Its dome falls to
MOUND_SINK below the pivot at its radius and runs on as a buried apron down to APRON_DROP, so the
downhill side never floats on the beach slope (the scatter stands a piece at the lowest ground
under its centre and a 0.3 m ring; the dry band's slope is at most about 0.18), and its normals turn to +Z
from 0.6 of the dome's radius out, so where it meets the sand it shades as sand. Its UV0 sits in the
atlas's transparent bottom-left corner, where the masks carry sand values (R 1, G 0.85); the cards
take R 0.5 (blades shed snow) and G 0.6.

Wind (TEXCOORD_1 `wind`, project.md "Tree wind data", the wildflower convention): U = 0 at and
below the ground and on the mound, rising along each card to about 1 at its top; V = a seeded phase
per card."""
from __future__ import annotations

import hashlib
import math
import os
import pathlib
import random
from dataclasses import dataclass

import bmesh
import bpy
import numpy as np
from mathutils import Vector

from lib import bake, budgets, collider, export, geom, materials, scene, uv

import _props

ATLAS, TILE, NORMAL_SIZE, MASK_SIZE = 1024, 512, 512, 64
SINK = 0.05  # card roots below the mound surface
MOUND_SINK, APRON_DROP, APRON_W = 0.04, 0.3, 0.3
CORNER = 4  # masks texels of the sand corner (the mound's UV0), cleared on tile 0
GREEN_D = np.array((0x48, 0x6a, 0x64), np.float32) / 255
GREEN_L = np.array((0x8e, 0xac, 0xa4), np.float32) / 255
STRAW = np.array((0xd2, 0xc0, 0x8e), np.float32) / 255
DEAD = np.array((0xae, 0x9b, 0x70), np.float32) / 255
DEAD_L = np.array((0xcf, 0xbf, 0x95), np.float32) / 255
SPIKE = np.array((0xc2, 0xb0, 0x82), np.float32) / 255


@dataclass
class Clump:
    cards: int  # LOD0 cards
    height: tuple[float, float]  # card height range (m)
    width: float  # card width (m)
    blades: int  # blades per tile
    dead: float  # share of dead straw blades
    spikes: bool  # seed spikes on tiles 1 and 3
    mound_r: float  # the dome's radius (m) at MOUND_SINK
    mound_h: float  # its height over the pivot (m)
    lean: float = 0.25  # outward lean of a card's top (m per m of height)
    drift: tuple[float, float] = (0.0, 0.0)  # wind-bent: every top pushed this far (m) toward +x, +y
    straw_from: tuple[float, float] = (0.55, 0.8)  # where a green blade starts fading to straw


def _rng(seed: str, k: int) -> np.random.Generator:
    return np.random.default_rng(int.from_bytes(hashlib.sha256(f"{seed}:marram:{k}".encode()).digest()[:8], "big"))


def _stroke(rgb, alpha, nrm, pts, half, cols):
    """Stamp discs along a blade (pts (n, 2) px, half-widths (n,), colours (n, 3)), folded across."""
    h, w = alpha.shape
    for i in range(len(pts)):
        cx, cy = pts[i]
        hw = half[i]
        j = min(i + 1, len(pts) - 1)
        k = max(i - 1, 0)
        tx, ty = pts[j] - pts[k]
        tl = math.hypot(tx, ty) or 1.0
        nx_, ny_ = -ty / tl, tx / tl
        x0, x1 = max(int(cx - hw - 1), 0), min(int(cx + hw + 2), w)
        y0, y1 = max(int(cy - hw - 1), 0), min(int(cy + hw + 2), h)
        if x0 >= x1 or y0 >= y1:
            continue
        ys, xs = np.mgrid[y0:y1, x0:x1].astype(np.float32) + 0.5
        dx, dy = xs - cx, ys - cy
        inside = dx * dx + dy * dy <= hw * hw
        if not inside.any():
            continue
        across = np.clip((dx * nx_ + dy * ny_) / hw, -1.0, 1.0)
        col = cols[i][None, None] * (1.0 - 0.2 * across ** 2)[..., None]
        sub = (slice(y0, y1), slice(x0, x1))
        rgb[sub] = np.where(inside[..., None], np.clip(col, 0, 1), rgb[sub])
        alpha[sub] = np.where(inside, 1.0, alpha[sub])
        fx, fy = nx_ * across * 0.45, ny_ * across * 0.45
        nz = np.sqrt(np.clip(1 - fx * fx - fy * fy, 0.05, 1))
        nrm[sub] = np.where(inside[..., None], np.stack([fx, fy, nz], -1) * 0.5 + 0.5, nrm[sub])


def tile(seed: str, k: int, c: Clump):
    """One fan of marram blades (TILE x TILE px, rows bottom-up). Returns (albedo RGBA with sRGB RGB,
    normal RGB, roughness, ao)."""
    rng = _rng(seed, k)
    n = TILE
    rgb = np.zeros((n, n, 3), np.float32)
    alpha = np.zeros((n, n), np.float32)
    nrm = np.empty((n, n, 3), np.float32)
    nrm[:] = (0.5, 0.5, 1.0)
    margin = 5.0
    blades = []
    for b in range(c.blades):
        dead = rng.random() < c.dead
        x0 = n * 0.5 + rng.normal(0, n * 0.045)
        ang = rng.normal(0, 0.42) + (x0 / n - 0.5) * 1.4  # radians from vertical, spreading the fan
        ang = float(np.clip(ang, -1.15, 1.15))
        length = n * rng.uniform(0.62, 0.97) * (1.0 - 0.32 * abs(ang)) * (0.85 if dead else 1.0)
        droop = rng.uniform(0.05, 0.3) * (0.5 + abs(ang)) * (1.4 if dead else 1.0)
        blades.append((dead, x0, ang, length, droop, rng.uniform(0, 1), rng.uniform(*c.straw_from)))
    blades.sort(key=lambda b: (not b[0], b[5]))  # dead blades at the back
    for dead, x0, ang, length, droop, tone, s0 in blades:
        steps = max(int(length / 1.2), 8)
        t = np.linspace(0.0, 1.0, steps)
        side = math.copysign(1.0, ang) if abs(ang) > 0.05 else (1.0 if tone > 0.5 else -1.0)
        a = ang + side * droop * t ** 1.6  # the blade arches outward, more toward its tip
        dxy = np.stack([np.sin(a), np.cos(a)], -1) * (length / steps)
        pts = np.cumsum(dxy, 0) + np.array([x0, 3.0])
        pts[:, 0] = np.clip(pts[:, 0], margin, n - margin)
        pts[:, 1] = np.clip(pts[:, 1], 2.0, n - margin)
        half = (3.4 if not dead else 2.6) * (1.0 - t) ** 0.7 + 0.6
        if dead:
            cols = DEAD[None] + (DEAD_L - DEAD)[None] * (0.4 * tone + 0.6 * t[:, None])
        else:
            base = GREEN_D + (GREEN_L - GREEN_D) * (0.25 + 0.6 * tone)
            fade = np.clip((t - s0) / (1.0 - s0), 0, 1)[:, None] ** 0.8
            cols = base[None] * (0.82 + 0.25 * t[:, None]) * (1 - fade) + STRAW[None] * fade
        _stroke(rgb, alpha, nrm, pts, half, cols)
    if c.spikes and k in (1, 3):
        for sx, sa in ((0.47, -0.12), (0.55, 0.16))[: 2 if k == 1 else 1]:
            length = n * rng.uniform(0.8, 0.92)
            steps = int(length / 1.2)
            t = np.linspace(0.0, 1.0, steps)
            a = sa + 0.06 * t
            pts = np.cumsum(np.stack([np.sin(a), np.cos(a)], -1) * (length / steps), 0) + np.array([sx * n, 3.0])
            pts[:, 1] = np.clip(pts[:, 1], 2.0, n - margin)
            head = t > 0.72
            half = np.where(head, 5.2 * np.sin(np.clip((t - 0.72) / 0.28, 0, 1) * math.pi) ** 0.5 + 1.8, 1.5)
            cols = np.where(head[:, None], SPIKE[None] * (0.9 + 0.2 * t[:, None]), DEAD_L[None] * 0.95)
            _stroke(rgb, alpha, nrm, pts, half, cols)
    alpha[:, : int(margin)] = 0.0
    alpha[:, n - int(margin):] = 0.0
    alpha[n - int(margin):] = 0.0
    if k == 0:
        cut = CORNER * ATLAS // MASK_SIZE
        alpha[:cut, :cut] = 0.0  # the sand corner: the mound's masks texels
    rgb = _props.dilate(rgb, alpha, iters=40)
    rgb[alpha <= 0.5] = np.where(rgb[alpha <= 0.5].sum(-1, keepdims=True) > 0, rgb[alpha <= 0.5], GREEN_D)
    ys = (np.arange(n, dtype=np.float32) + 0.5) / n
    ao = np.repeat((0.58 + 0.42 * np.clip(ys / 0.45, 0, 1) ** 0.8)[:, None], n, 1).astype(np.float32)
    rough = np.where(alpha > 0.5, 0.66, 0.8).astype(np.float32)
    return np.concatenate([rgb, alpha[..., None]], -1), nrm, rough, ao


def atlas(seed: str, c: Clump):
    """2 x 2 tiles: tile k at column k % 2, row k // 2 (row 0 at the bottom)."""
    alb = np.zeros((ATLAS, ATLAS, 4), np.float32)
    nrm = np.zeros((ATLAS, ATLAS, 3), np.float32)
    rough = np.zeros((ATLAS, ATLAS), np.float32)
    ao = np.zeros((ATLAS, ATLAS), np.float32)
    for k in range(4):
        a, nn, r, o = tile(seed, k, c)
        sl = (slice((k // 2) * TILE, (k // 2 + 1) * TILE), slice((k % 2) * TILE, (k % 2 + 1) * TILE))
        alb[sl], nrm[sl], rough[sl], ao[sl] = a, nn, r, o
    assert alb[:CORNER * ATLAS // MASK_SIZE, :CORNER * ATLAS // MASK_SIZE, 3].max() == 0.0, "sand corner not clear"
    return alb, nrm, rough, ao


def coverage(alb):
    return {f"tile_{k}": _props.coverage_by_mip(alb[(k // 2) * TILE:(k // 2 + 1) * TILE,
                                                    (k % 2) * TILE:(k % 2 + 1) * TILE, 3]) for k in range(4)}


def mound_z(c: Clump, r: float) -> float:
    """The mound's surface over the pivot at radius r: a dome tangent-free at its top, MOUND_SINK
    below the pivot at mound_r, then the buried apron down to APRON_DROP over APRON_W."""
    if r <= c.mound_r:
        q = r / c.mound_r
        return (c.mound_h + MOUND_SINK) * (1 - q * q) ** 1.5 - MOUND_SINK
    return -MOUND_SINK - (APRON_DROP - MOUND_SINK) * min((r - c.mound_r) / APRON_W, 1.0)


def build_piece(piece: str, out: pathlib.Path, generator: str, c: Clump, height_range, reach_range) -> None:
    skip = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    rng = random.Random(piece)
    cards = []
    for i in range(c.cards):
        a = i * 2.39996 + rng.uniform(-0.2, 0.2)
        ring = (i % 3) / 2.0
        cards.append({"a": a, "r0": 0.03 + 0.07 * ring, "H": rng.uniform(*c.height) * (1.0 - 0.15 * ring),
                      "lean": c.lean * (0.6 + 0.8 * ring), "tile": i % 4, "flip": (i // 4) % 2 == 1,
                      "phase": _props.unit_hash(piece, "card", i), "sway": rng.uniform(0.9, 1.0)})
    foliage = materials.pbr_material(f"{piece}_foliage", materials.srgb("#6f8f7a"), 0.66)
    sand = materials.pbr_material(f"{piece}_sand", materials.srgb("#e8dcc0"), 0.9)
    eps = 0.5 / ATLAS

    def card_uv(card, s, t):
        col, row = card["tile"] % 2, card["tile"] // 2
        u = (1.0 - s) if card["flip"] else s
        return (col * 0.5 + eps + u * (0.5 - 2 * eps), row * 0.5 + eps + t * (0.5 - 2 * eps))

    def build_lod(name, every, segs, wscale, sides, rings):
        bm = bmesh.new()
        uvl = bm.loops.layers.uv.new("UVMap")
        wl = bm.loops.layers.uv.new("wind")
        # Mound: a centre point, `rings` rings out to mound_r, one apron ring.
        radii = [c.mound_r * (j / rings) ** 0.8 for j in range(1, rings + 1)] + [c.mound_r + APRON_W]
        ctr = bm.verts.new((0.0, 0.0, mound_z(c, 0.0)))
        ring_vs = []
        for r in radii:
            ring_vs.append([bm.verts.new((r * math.cos(2 * math.pi * (k + 0.5 * (len(ring_vs) % 2)) / sides),
                                          r * math.sin(2 * math.pi * (k + 0.5 * (len(ring_vs) % 2)) / sides),
                                          mound_z(c, r))) for k in range(sides)])
        faces = []
        for k in range(sides):
            faces.append(bm.faces.new((ctr, ring_vs[0][k], ring_vs[0][(k + 1) % sides])))
        for j in range(len(radii) - 1):
            a, b = ring_vs[j], ring_vs[j + 1]
            for k in range(sides):
                faces.append(bm.faces.new((a[k], b[k], b[(k + 1) % sides], a[(k + 1) % sides])))
        corner = 1.0 / MASK_SIZE  # a 2-texel square inside the sand corner of the masks
        for f in faces:
            f.material_index = 1
            f.smooth = True
            for lp in f.loops:
                p = lp.vert.co
                lp[uvl].uv = (corner + (p.x / (2 * radii[-1]) + 0.5) * 2 / MASK_SIZE,
                              corner + (p.y / (2 * radii[-1]) + 0.5) * 2 / MASK_SIZE)
                lp[wl].uv = (0.0, 0.0)
        # Cards.
        for i, cd in enumerate(cards):
            if i % every:
                continue
            h = Vector((math.cos(cd["a"]), math.sin(cd["a"]), 0.0))
            side = Vector((-h.y, h.x, 0.0))
            half = 0.5 * c.width * wscale
            z0 = mound_z(c, cd["r0"]) - SINK
            rows = []
            for j in range(segs + 1):
                t = j / segs
                bow = 4 * 0.04 * t * (1 - t)
                p = h * (cd["r0"] + cd["lean"] * cd["H"] * t ** 1.4 + bow) + Vector((0, 0, z0 + cd["H"] * t))
                p += Vector((c.drift[0], c.drift[1], 0.0)) * t ** 1.8
                rows.append((bm.verts.new(p - side * half), bm.verts.new(p + side * half), t))
            for j in range(segs):
                (a0, a1, t0), (b0, b1, t1) = rows[j], rows[j + 1]
                f = bm.faces.new((a0, a1, b1, b0))
                f.material_index = 0
                f.smooth = True
                for lp, (s, t) in zip(f.loops, ((0, t0), (1, t0), (1, t1), (0, t1))):
                    lp[uvl].uv = card_uv(cd, s, t)
                    u = cd["sway"] * min(1.0, t ** 1.5 * 1.05) if lp.vert.co.z > 0 else 0.0
                    lp[wl].uv = (min(max(u, 0.0), 1.0), cd["phase"])
        bmesh.ops.triangulate(bm, faces=bm.faces)
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        me.uv_layers.active = me.uv_layers["UVMap"]
        # Normals: the mound's analytic ones; the cards a rounded clump normal blended with their own.
        loops_n = [None] * len(me.loops)
        for poly in me.polygons:
            poly.use_smooth = True
            for li in poly.loop_indices:
                p = me.vertices[me.loops[li].vertex_index].co
                if poly.material_index == 1:
                    r = math.hypot(p.x, p.y)
                    dzdr = (mound_z(c, r + 1e-3) - mound_z(c, max(r - 1e-3, 0.0))) / 2e-3
                    nn = Vector((-dzdr * p.x / r, -dzdr * p.y / r, 1.0)).normalized() if r > 1e-6 else Vector((0, 0, 1))
                    # Toward +Z from 0.6 of the dome's radius out, and +Z on the apron, so where the
                    # mound meets the sloping sand it shades like the sand, not as a ledge.
                    q = min(max((r / c.mound_r - 0.6) / 0.4, 0.0), 1.0)
                    nn = nn.lerp(Vector((0.0, 0.0, 1.0)), q * q * (3 - 2 * q)).normalized()
                else:
                    fn = poly.normal.copy()
                    radial = Vector((p.x, p.y, 0.0)) / 0.5 + Vector((0.0, 0.0, 1.0))
                    if fn.dot(radial) < 0:
                        fn = -fn
                    nn = (fn.normalized() * 0.45 + radial.normalized() * 0.55).normalized()
                loops_n[li] = tuple(nn)
        me.normals_split_custom_set(loops_n)
        me.update()
        o = scene.link(bpy.data.objects.new(name, me))
        o.data.materials.append(foliage)
        o.data.materials.append(sand)
        return o

    lods = [build_lod(f"{piece}_lod0", 1, 4, 1.0, 12, 3),
            build_lod(f"{piece}_lod1", 2, 2, 1.12, 8, 1),
            build_lod(f"{piece}_lod2", 4, 1, 1.3, 5, 1)]
    for o in lods:
        assert [u.name for u in o.data.uv_layers] == ["UVMap", "wind"], o.name
        names = {o.data.materials[p.material_index].name for p in o.data.polygons}
        assert names == {f"{piece}_foliage", f"{piece}_sand"}, (o.name, names)
    tris = [geom.tri_count(o) for o in lods]
    card_lod0 = lods[0]
    # Texel density on the cards only (the sand is world-space tileable, not a UV0 density slot).
    tmp = card_lod0.copy()
    tmp.data = card_lod0.data.copy()
    scene.link(tmp)
    tbm = bmesh.new()
    tbm.from_mesh(tmp.data)
    bmesh.ops.delete(tbm, geom=[f for f in tbm.faces if f.material_index != 0], context="FACES")
    tbm.to_mesh(tmp.data)
    tbm.free()
    texel = uv.measure_texel_density(tmp, ATLAS)
    bpy.data.objects.remove(tmp, do_unlink=True)
    zs = [v.co.z for v in lods[0].data.vertices]
    card_reach = max(math.hypot(v.co.x, v.co.y) for p in lods[0].data.polygons if p.material_index == 0
                     for v in (lods[0].data.vertices[i] for i in p.vertices))
    print(f"{piece}: tris per LOD {tris}, texel {texel}, z {min(zs):.3f}..{max(zs):.3f}, card reach {card_reach:.3f}")
    budgets.check(tris, "prop", piece)
    assert texel["median"] >= 256, texel
    assert height_range[0] <= max(zs) <= height_range[1] and reach_range[0] <= card_reach <= reach_range[1], \
        (max(zs), card_reach)

    alb, nrm, rough, ao = atlas(piece, c)
    n2 = nrm.reshape(NORMAL_SIZE, 2, NORMAL_SIZE, 2, 3).mean(axis=(1, 3)) * 2.0 - 1.0
    n2 /= np.linalg.norm(n2, axis=-1, keepdims=True)
    normal = np.concatenate([n2 * 0.5 + 0.5, np.ones((NORMAL_SIZE, NORMAL_SIZE, 1))], -1).astype(np.float32)
    orm = np.stack([ao, rough, np.zeros_like(ao), np.ones_like(ao)], -1)
    bake.write_png(ATLAS, alb.reshape(-1, 4), str(out / f"{piece}.albedo.png"), non_color=False)
    bake.write_png(NORMAL_SIZE, normal.reshape(-1, 4), str(out / f"{piece}.normal.png"))
    bake.write_png(ATLAS, orm.reshape(-1, 4), str(out / f"{piece}.orm.png"))
    # masks (linear): the cards R 0.5 (blades shed snow), G 0.6; the sand corner (the mound's UV0)
    # R 1, G 0.85; B no night emissive; A 1 (the cards' AO is in ORM.R, the mound has none).
    mk = np.zeros((MASK_SIZE, MASK_SIZE, 4), np.float32)
    mk[:] = (0.5, 0.6, 0.0, 1.0)
    mk[:CORNER, :CORNER] = (1.0, 0.85, 0.0, 1.0)
    bake.write_png(MASK_SIZE, mk.reshape(-1, 4), str(out / f"{piece}.masks.png"))
    cov = coverage(alb)

    for o in lods:
        wlay = o.data.uv_layers["wind"].data
        w = np.empty(len(wlay) * 2)
        wlay.foreach_get("uv", w)
        assert w.min() >= 0.0 and w.max() <= 1.0, (o.name, w.min(), w.max())
        for loop in o.data.loops:
            if o.data.vertices[loop.vertex_index].co.z <= 0.0:
                assert wlay[loop.index].uv[0] == 0.0, o.name
    tips = [lp.uv[0] for lp in lods[0].data.uv_layers["wind"].data]
    assert max(tips) > 0.8, max(tips)

    # Collider: the mound's disc as an octagonal prism to its top (scatter's ground reach; the grass
    # is walk-through, so walk mode never reads it).
    cbm = bmesh.new()
    for z in (-MOUND_SINK, c.mound_h):
        for k in range(8):
            a = 2 * math.pi * (k + 0.5) / 8
            cbm.verts.new((c.mound_r * math.cos(a), c.mound_r * math.sin(a), z))
    bmesh.ops.convex_hull(cbm, input=cbm.verts)
    cme = bpy.data.meshes.new("collider_src")
    cbm.to_mesh(cme)
    cbm.free()
    csrc = scene.link(bpy.data.objects.new("collider_src", cme))
    col = collider.collider_from(csrc, "hull", f"{piece}_collider")
    bpy.data.objects.remove(csrc, do_unlink=True)
    bpy.context.view_layer.update()
    col_reach = max(math.hypot(*(col.matrix_world @ v.co).xy) for v in col.data.vertices)
    print(f"{piece}: coverage {cov}")
    print(f"{piece}: collider reach {col_reach:.4f}")

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
                           bakeStats={"alphaCoverageByMip": cov, "alphaCutoff": 0.5},
                           extentZ=[min(zs), max(zs)], cardReachM=card_reach, sinkM=SINK, cards=c.cards,
                           mound={"radiusM": c.mound_r, "heightM": c.mound_h, "sinkM": MOUND_SINK,
                                  "apronDropM": APRON_DROP, "apronWidthM": APRON_W},
                           colliderReachM=col_reach, skipped=skip)
