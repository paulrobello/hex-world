"""Dressing centres for the river family: bank stones at the waterline and reed clumps on the banks,
placed along the channel centre line of a `_river.LAYOUTS` layout. Colour-only slots plus the
piece's §9.4 masks map on the planar UV0 (`masks_field`). Kept apart from `_river.py` so the bases and the mouth stub do not depend on it (provenance hashes
every imported helper)."""
import math
import pathlib
import sys

import bpy
import numpy as np

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

from _river import BEND_C, BEND_R, JOINT_X, LAYOUTS, profile_z


STONE, REED, CATTAIL = "river_stone", "river_reed", "river_cattail"
DRESS_SLOTS = (STONE, REED, CATTAIL)
MASK_SIZE = 256  # 8 px/m on the 32 m planar UV0: every channel is a smooth field of (x, y)
POROSITY = (1.0, 0.8)  # G at the waterline (d <= 4.5 m; the water edge is d = 4) and up the bank (d >= 6 m)


def porosity(d):
    """Masks G at distance d from the channel centre line: wettest at the waterline, easing off up
    the bank where the reeds stand."""
    t = np.clip((np.asarray(d, np.float64) - 4.5) / 1.5, 0.0, 1.0)
    return POROSITY[0] + (POROSITY[1] - POROSITY[0]) * t * t * (3.0 - 2.0 * t)


def masks_field(dist):
    """The §9.4 masks (R, G, B, A), each a (size*size,) array in Blender pixel order over the planar
    UV0 (`uv.planar_cap_uv`, the 2R square), for a channel whose centre-line distance is dist(x, y).
    An analytic field, so no bake and no change to the GLB. R 1: snow lies on stones and reeds alike,
    and the shader keeps it to up-facing faces. G `porosity`. B 0: nothing glows. A 1: these slots
    had no AO before the map, so the MVP look is unchanged."""
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    wet = porosity(np.vectorize(dist)(xs, ys))
    n = MASK_SIZE * MASK_SIZE
    return (np.ones(n, np.float32), wet.ravel().astype(np.float32), np.zeros(n, np.float32),
            np.ones(n, np.float32))


def arc_angles():
    """Bend arc angles (about BEND_C) where the arc crosses the edge-0 and edge-2 joint planes."""
    cx, cy = BEND_C
    ex, ey = hexgeo.rotate_xy((JOINT_X, 0.0), 2)
    return math.atan2(0.0 - cy, JOINT_X - cx), math.atan2(ey - cy, ex - cx)


def centre_frame(layout, s):
    """Point and unit normal (toward +offset) on the channel centre line at fraction s in [0, 1],
    joint to joint."""
    if layout == "straight":
        return (JOINT_X - 2.0 * JOINT_X * s, 0.0), (0.0, 1.0)
    a0, a1 = arc_angles()
    a = a0 + (a1 - a0) * s
    n = (math.cos(a), math.sin(a))
    return (BEND_C[0] + BEND_R * n[0], BEND_C[1] + BEND_R * n[1]), n


def _stone(bm, at, size, seed):
    """A faceted bank stone: an icosahedron (20 tris) squashed to `size` with seeded jitter."""
    import random

    import bmesh
    rnd = random.Random(seed)
    m = bmesh.ops.create_icosphere(bm, subdivisions=1, radius=1.0)
    for v in m["verts"]:
        j = 1.0 + 0.3 * (rnd.random() - 0.5)
        v.co.x = at[0] + v.co.x * size[0] * j
        v.co.y = at[1] + v.co.y * size[1] * j
        v.co.z = at[2] + v.co.z * size[2] * j


def _reed_clump(bm, at, seed, blades, heads):
    """Blades as tapered triangles, each face doubled back to back (the slot is single-sided), and
    a cattail head (a thin back-to-back card) on every third blade when `heads`."""
    import random
    rnd = random.Random(seed)
    tops = []
    for b in range(blades):
        a = rnd.uniform(0, 2 * math.pi)
        r = rnd.uniform(0.0, 0.35)
        x, y = at[0] + r * math.cos(a), at[1] + r * math.sin(a)
        h = rnd.uniform(0.9, 1.5)
        lean = rnd.uniform(0.05, 0.25)
        tx, ty = x + lean * math.cos(a), y + lean * math.sin(a)
        px, py = -math.sin(a) * 0.045, math.cos(a) * 0.045
        co = [(x + px, y + py, at[2] - 0.1), (x - px, y - py, at[2] - 0.1), (tx, ty, at[2] + h)]
        bm.faces.new([bm.verts.new(c) for c in co])
        bm.faces.new([bm.verts.new(c) for c in reversed(co)])
        if heads and b % 3 == 0:
            tops.append((x + (tx - x) * 0.78, y + (ty - y) * 0.78, at[2] + 0.78 * h, px, py))
    return tops


def _cattail(bm, top):
    x, y, z, px, py = top
    s = 0.06 / 0.045
    co = [(x + px * s, y + py * s, z), (x - px * s, y - py * s, z),
          (x - px * s, y - py * s, z + 0.3), (x + px * s, y + py * s, z + 0.3)]
    bm.faces.new([bm.verts.new(c) for c in co])
    bm.faces.new([bm.verts.new(c) for c in reversed(co)])


def build_dressing(piece, layout, generator, per_side=14, skip=None, extra=None):
    """Bank stones at the waterline and reed clumps on the banks, both sides of the channel, seeded
    by the piece id. Colour-only slots plus the masks map (`masks_field`, written to `out`). LOD1 keeps half the stones and reeds,
    LOD2 the six biggest stones. `skip(x, y)` drops a placement (the bridge footprint); `extra(li)`
    returns more LOD objects to join (the bridge)."""
    import random

    import bmesh
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {
        STONE: materials.pbr_material(STONE, materials.srgb("#8a8378"), 0.8),
        REED: materials.pbr_material(REED, materials.srgb("#5d7a35"), 0.85),
        CATTAIL: materials.pbr_material(CATTAIL, materials.srgb("#5a3a22"), 0.9),
    }
    rnd = random.Random(piece)
    dist = LAYOUTS[layout]["dist"]
    stones, reeds = [], []
    for side in (-1, 1):
        for i in range(per_side):
            s = (i + 0.5 + 0.6 * (rnd.random() - 0.5)) / per_side
            p, n = centre_frame(layout, s)
            d = side * rnd.uniform(3.6, 4.5)
            x, y = p[0] + n[0] * d, p[1] + n[1] * d
            size = (rnd.uniform(0.25, 0.55), rnd.uniform(0.2, 0.45), rnd.uniform(0.15, 0.3))
            stones.append(((x, y, profile_z(dist(x, y)) + 0.03), size, rnd.randrange(1 << 30)))
        for i in range(5):
            s = (i + 0.3 + 0.4 * rnd.random()) / 5
            p, n = centre_frame(layout, s)
            d = side * rnd.uniform(4.7, 5.7)
            x, y = p[0] + n[0] * d, p[1] + n[1] * d
            reeds.append(((x, y, profile_z(dist(x, y))), rnd.randrange(1 << 30)))
    if skip:
        stones = [st for st in stones if not skip(st[0][0], st[0][1])]
        reeds = [r for r in reeds if not skip(r[0][0], r[0][1])]
    # Big stones first, so the coarse LODs keep the ones that read from far away.
    stones.sort(key=lambda st: -st[1][0] * st[1][1])
    lods = []
    for li in range(3):
        bm = bmesh.new()
        for at, size, seed in stones[: (len(stones), len(stones) // 2, 6)[li]]:
            _stone(bm, at, size, seed)
        n_stone = len(bm.faces)
        tops = []
        for at, seed in (reeds, reeds[::2], [])[li]:
            tops += _reed_clump(bm, at, seed, 9 if li == 0 else 4, li == 0)
        n_reed = len(bm.faces)
        for t in tops:
            _cattail(bm, t)
        bm.faces.ensure_lookup_table()
        for fi, f in enumerate(bm.faces):
            f.material_index = 0 if fi < n_stone else (1 if fi < n_reed else 2)
            f.smooth = False
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(f"{piece}_lod{li}_dress")
        bm.to_mesh(mesh)
        bm.free()
        o = scene.link(bpy.data.objects.new(f"{piece}_lod{li}_dress", mesh))
        for name in DRESS_SLOTS:
            o.data.materials.append(mats[name])
        parts = [o] + (extra(li, mats) if extra else [])
        for q in parts:
            uv.planar_cap_uv(q)
        body = geom.join(parts, f"{piece}_lod{li}") if len(parts) > 1 else o
        body.name = body.data.name = f"{piece}_lod{li}"
        body.parent = root
        lods.append(body)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    assert tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    bake.write_png(MASK_SIZE, np.stack(masks_field(dist), axis=1), str(out / f"{piece}.masks.png"))
    return root, lods, out, {"stones": len(stones), "reedClumps": len(reeds), "tris": tris}


def finish_dressing(root, lods, out, piece, generator, stats, **report):
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, dressing=stats,
                           skipped=True,
                           reason="no Cycles bake: the masks are an analytic field of (x, y) on the planar UV0",
                           masks={"snow": 1.0, "porosity": list(POROSITY), "night": 0.0, "ao": 1.0}, **report)
