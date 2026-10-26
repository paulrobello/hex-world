"""river_spring_center: the dressing of the river_spring tile, on river_spring_base (which carries the
bed). river_straight_center bank stones and reed clumps along the stream, a ring of lichen-grey
granite around the spring pool at its bank, reeds on the pool's back bank and a few flat lily pads
floating on the pool. Every item is seated on the pinned bed (`_spring.bed_z`); the edge-0 stub
metre and 1.5 m from every hex edge stay clear. Colour-only slots plus the §9.4 masks map as an
analytic field of (x, y) on the planar UV0 (no Cycles bake): R snow 0 under water, G porosity, B 0,
A 1. The lily pads float over water, so their UVs point at one off-hex texel (dry meadow: R 1,
G 0.8). Scripted route (PRD section 11.1)."""
import math
import pathlib
import random
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv  # noqa: E402

import _river_dress  # noqa: E402
import _spring  # noqa: E402
from _river import WATER  # noqa: E402

PIECE = "river_spring_center"
STONE, REED, CATTAIL, GRANITE, LILY = "river_stone", "river_reed", "river_cattail", "river_granite", "river_lily"
SLOTS = (STONE, REED, CATTAIL, GRANITE, LILY)
COLORS = {STONE: ("#8a8378", 0.8), REED: ("#5d7a35", 0.85), CATTAIL: ("#5a3a22", 0.9),
          GRANITE: ("#8c8b80", 0.8), LILY: ("#4f6b3a", 0.7)}
MASK_SIZE = 256  # 8 px/m on the 32 m planar UV0: every channel is a smooth field of (x, y)
LILY_UV = (0.5 / MASK_SIZE, 0.5 / MASK_SIZE)  # texel (0, 0): Blender (-15.94, -15.94), off the hex
EDGE_CLEAR = 1.5
NX, NY = _spring.NODE


def clear(x, y, radius):
    return hexgeo.dist_to_border(x, y) >= EDGE_CLEAR + radius


def masks_field():
    """(R, G, B, A) as (size*size,) arrays in Blender pixel order over the planar UV0. R 0 where the
    bed lies under the water at rest, 1 elsewhere; G 1.0 at and below the swash line easing to 0.8 up
    the bank; B 0; A 1 (colour-only slots had no AO, so the look is unchanged)."""
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    bed = np.vectorize(_spring.bed_z)(xs, ys)
    t = np.clip((bed + 1.0) / 0.7, 0.0, 1.0)
    wet = 1.0 - 0.2 * t * t * (3.0 - 2.0 * t)
    n = MASK_SIZE * MASK_SIZE
    snow = np.where(bed < WATER, 0.0, 1.0)
    return (snow.ravel().astype(np.float32), wet.ravel().astype(np.float32), np.zeros(n, np.float32),
            np.ones(n, np.float32))


def lily(bm, at, radius, rot):
    """A flat pad: a 7-segment disc fan with one wedge cut out (6 tris), facing up."""
    c = bm.verts.new(at)
    ring = []
    for k in range(7):
        a = rot + 2 * math.pi * k / 8
        ring.append(bm.verts.new((at[0] + radius * math.cos(a), at[1] + radius * math.sin(a), at[2])))
    return [bm.faces.new([c, ring[k], ring[k + 1]]) for k in range(6)]


def placements():
    rnd = random.Random(PIECE)
    stones, granite, reeds, lilies = [], [], [], []
    # Stream bank stones at the waterline and reeds up the banks, as river_straight_center.
    for side in (-1, 1):
        for i in range(5):
            x = 4.2 + (11.6 - 4.2) * (i + 0.5 + 0.5 * (rnd.random() - 0.5)) / 5
            y = side * rnd.uniform(3.6, 4.5)
            size = (rnd.uniform(0.25, 0.5), rnd.uniform(0.2, 0.4), rnd.uniform(0.15, 0.28))
            if clear(x, y, size[0]):
                stones.append(((x, y, _spring.bed_z(x, y) + 0.03), size, rnd.randrange(1 << 30)))
        for i in range(3):
            x = 4.8 + (11.3 - 4.8) * (i + 0.3 + 0.4 * rnd.random()) / 3
            y = side * rnd.uniform(4.8, 5.6)
            if clear(x, y, 0.6):
                reeds.append(((x, y, _spring.bed_z(x, y)), rnd.randrange(1 << 30)))
    # Granite ring on the pool bank (waterline r = 5.5), round the back of the pool, clear of the outlet.
    n = 12
    for i in range(n):
        a = math.radians(62 + (298 - 62) * (i + 0.5 + 0.4 * (rnd.random() - 0.5)) / n)
        r = rnd.uniform(5.35, 5.95)
        x, y = NX + r * math.cos(a), NY + r * math.sin(a)
        size = (rnd.uniform(0.42, 0.75), rnd.uniform(0.35, 0.6), rnd.uniform(0.28, 0.45))
        granite.append(((x, y, _spring.bed_z(x, y) + 0.02), size, rnd.randrange(1 << 30)))
    # Reeds on part of the pool's back bank.
    for i in range(4):
        a = math.radians(150 + (250 - 150) * (i + 0.5) / 4 + rnd.uniform(-6, 6))
        r = rnd.uniform(6.3, 6.9)
        x, y = NX + r * math.cos(a), NY + r * math.sin(a)
        reeds.append(((x, y, _spring.bed_z(x, y)), rnd.randrange(1 << 30)))
    # Lily pads floating on the pool inside r < 4, kept apart.
    while len(lilies) < 5:
        a = rnd.uniform(0, 2 * math.pi)
        r = rnd.uniform(1.0, 3.4)
        rad = rnd.uniform(0.32, 0.52)
        x, y = NX + r * math.cos(a), NY + r * math.sin(a)
        if all(math.hypot(x - q[0][0], y - q[0][1]) > q[1] + rad + 0.4 for q in lilies):
            lilies.append(((x, y, WATER + 0.01), rad, rnd.uniform(0, 2 * math.pi)))
    for at, size, _seed in stones + granite:
        assert clear(at[0], at[1], size[0]), at
    stones.sort(key=lambda st: -st[1][0] * st[1][1])
    granite.sort(key=lambda st: -st[1][0] * st[1][1])
    return stones, granite, reeds, lilies


def build():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    scene.reset_scene(PIECE)
    root = scene.empty(PIECE)
    mats = {s: materials.pbr_material(s, materials.srgb(c), r) for s, (c, r) in COLORS.items()}
    stones, granite, reeds, lilies = placements()
    plan = (  # per LOD: stream stones, granite, reeds (cattails), lilies
        (stones, granite, reeds, True, lilies),
        (stones[:4], granite[:8], reeds[::2], True, lilies[:3]),
        ([], granite[:5], [], False, []),
    )
    lods = []
    for li, (st, gr, rd, heads, ly) in enumerate(plan):
        bm = bmesh.new()
        spans = []

        def mark(slot):
            spans.append((len(bm.faces), SLOTS.index(slot)))
        mark(STONE)
        for at, size, seed in st:
            _river_dress._stone(bm, at, size, seed)
        mark(GRANITE)
        for at, size, seed in gr:
            _river_dress._stone(bm, at, size, seed)
        mark(REED)
        tops = []
        for at, seed in rd:
            tops += _river_dress._reed_clump(bm, at, seed, li, heads)
        mark(CATTAIL)
        for t in tops:
            _river_dress._cattail(bm, t)
        mark(LILY)
        lily_from = len(bm.faces)
        for at, rad, rot in ly:
            lily(bm, at, rad, rot)
        bm.faces.ensure_lookup_table()
        for fi, f in enumerate(bm.faces):
            f.material_index = [m for start, m in spans if start <= fi][-1]
            if SLOTS[f.material_index] not in (REED, CATTAIL):
                f.smooth = False
        _river_dress.retag_spikes(bm, SLOTS.index(STONE))
        lily_faces = len(bm.faces) - lily_from
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(f"{PIECE}_lod{li}")
        bm.to_mesh(mesh)
        bm.free()
        o = scene.link(bpy.data.objects.new(f"{PIECE}_lod{li}", mesh))
        for s in SLOTS:
            o.data.materials.append(mats[s])
        uv.planar_cap_uv(o)
        layer = o.data.uv_layers["UVMap"].data
        lily_idx = SLOTS.index(LILY)
        for p in o.data.polygons:
            if p.material_index == lily_idx:
                for k in p.loop_indices:
                    layer[k].uv = LILY_UV
        assert sum(1 for p in o.data.polygons if p.material_index == lily_idx) == lily_faces
        print(f"{PIECE}_lod{li}: wind U max {_river_dress.wind_uv(o):.3f}")
        o.parent = root
        lods.append(o)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{PIECE}: tris per LOD {tris}")
    assert tris[0] <= _river_dress.LOD0_MAX and tris[2] <= 120, tris
    assert tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    col = collider.collider_from(lods[2], "box", f"{PIECE}_collider")
    col.parent = root
    bake.write_png(MASK_SIZE, np.stack(masks_field(), axis=1), str(out / f"{PIECE}.masks.png"))
    export.export_glb(root, str(out / f"{PIECE}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
    stats = {"streamStones": len(stones), "granite": len(granite), "reedClumps": len(reeds),
             "lilyPads": len(lilies), "tris": tris}
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, dressing=stats,
                           skipped=True,
                           reason="no Cycles bake: the masks are an analytic field of (x, y) on the planar UV0",
                           masks={"snow": "0 under water, 1 elsewhere", "porosity": [1.0, 0.8], "night": 0.0,
                                  "ao": 1.0, "lilyUv": list(LILY_UV)},
                           lod2Drops={CATTAIL: _river_dress.CATTAIL_LOD2_DROP})


build()
