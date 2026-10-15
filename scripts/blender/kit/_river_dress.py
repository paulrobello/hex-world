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


# Reed clumps (owner, 2026-10-14: "cat tails need more close up detail", for the eye-level walk mode).
# LOD0 blades are curved strap leaves: rows along the arc length, a shallow V (the midrib sits
# FOLD x the half-width behind the edges, so the upper side is a gutter and each half catches the
# light differently), a twist, and the longest blade of a clump bending over near its tip. The reed
# slot is colour-only and single-sided (back faces culled), so every blade carries a back sheet on
# its own vertices: the V on the upper side, a flat sheet behind it, each smooth along the length
# with its own normals, so both sides shade as the side the light falls on. Cattails are closed
# solids: a thin 3-sided stalk (reed slot) up into a 6-sided seed head (cattail slot) with a flat
# bottom cap and a rounded top, and a short 3-sided spike out of the top in the pale stone slot (no
# pale slot exists, and a slot change is not needed for one). Every count depends on the seed and
# the LOD only, so mirrored placements keep equal triangle counts. LOD1 keeps four flat back-to-back
# triangles per clump (the old LOD1 budget) along four of the LOD0 blades.
LOD0_MAX = 2500  # river dressing props (kit_gate RIVER_DRESSING_LOD0_MAX): owner, 2026-10-14, "Raise to 2,500"
FOLD = 0.3  # midrib depth behind the blade edges, as a fraction of the half-width (about 17 degrees)
REACH_MAX = 0.6  # horizontal reach of a blade or stalk from the clump anchor (bridge causeway clearance)
BURY = 0.07  # blades and stalks start this far below the anchor's ground height
HERO_ROWS = (0.0, 0.5, 0.76, 0.9)  # bending-over blade: rows by arc length, then the tip
TALL_ROWS = (0.0, 0.45, 0.76)
FILL_ROWS = (0.0, 0.55)
SPIKE_LAYER = "river_spike"  # bmesh face int layer: 1 on spike faces, which take the stone slot


def _add(a, b, k=1.0):
    return (a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2])


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _unit(a):
    n = math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2])
    return (a[0] / n, a[1] / n, a[2] / n)


def _centreline(base, az, length, th0, th1, power, steps=48):
    """Arc-length samples (s, point, tangent) of a blade rising from `base` and turning from th0 to
    th1 (radians from vertical) toward azimuth az, th = th0 + (th1 - th0) * s**power."""
    ca, sa = math.cos(az), math.sin(az)
    pts, p = [], base
    for i in range(steps + 1):
        s = i / steps
        th = th0 + (th1 - th0) * s ** power
        t = (math.sin(th) * ca, math.sin(th) * sa, math.cos(th))
        pts.append((s, p, t))
        if i < steps:
            thm = th0 + (th1 - th0) * ((i + 0.5) / steps) ** power
            p = _add(p, (math.sin(thm) * ca, math.sin(thm) * sa, math.cos(thm)), length / steps)
    return pts


def _sample(line, s):
    i = min(int(s * (len(line) - 1) + 1e-9), len(line) - 1)
    return line[i][1], line[i][2]


def _reach(line, anchor):
    return max(math.hypot(p[0] - anchor[0], p[1] - anchor[1]) for _s, p, _t in line)


def _blade_line(anchor, base, az, length, th0, th1, power):
    """The centre line, with th1 eased back toward th0 until the blade stays within REACH_MAX."""
    for _ in range(12):
        line = _centreline(base, az, length, th0, th1, power)
        if _reach(line, anchor) <= REACH_MAX:
            return line
        th1 = th0 + 0.8 * (th1 - th0)
    return line


def _blade(bm, line, az, rows, hw0, twist, fold):
    """One blade on `line` with a row of edge points at each arc length in `rows`, then the tip.
    `fold`: the V upper side (left half, midrib, right half), else one flat strip."""
    p0 = (-math.sin(az), math.cos(az), 0.0)
    front, back = [], []
    for s in rows:
        c, t = _sample(line, s)
        up = _cross(t, p0)  # the upper side: away from the bend's centre
        phi = twist * s
        p = _add(_add((0, 0, 0), p0, math.cos(phi)), up, math.sin(phi))
        up = _add(_add((0, 0, 0), up, math.cos(phi)), p0, -math.sin(phi))
        hw = hw0 * math.sqrt(max(0.0, 1.0 - s ** 3))
        left, right = _add(c, p, hw), _add(c, p, -hw)
        mid = _add(c, up, -FOLD * hw)
        front.append([bm.verts.new(left)] + ([bm.verts.new(mid)] if fold else []) + [bm.verts.new(right)])
        back.append([bm.verts.new(left), bm.verts.new(right)])
    tip = line[-1][1]
    tf, tb = bm.verts.new(tip), bm.verts.new(tip)
    faces, sharp = [], []
    for i in range(len(rows) - 1):
        a, b = front[i], front[i + 1]
        for k in range(len(a) - 1):
            faces.append(bm.faces.new([a[k], a[k + 1], b[k + 1], b[k]]))
        if fold:
            sharp.append((a[1], b[1]))
        faces.append(bm.faces.new([back[i + 1][0], back[i + 1][1], back[i][1], back[i][0]]))
    last = front[-1]
    for k in range(len(last) - 1):
        faces.append(bm.faces.new([last[k], last[k + 1], tf]))
    if fold:
        sharp.append((last[1], tf))
    faces.append(bm.faces.new([tb, back[-1][1], back[-1][0]]))
    for f in faces:
        f.smooth = True
    for u, v in sharp:
        bm.edges.get((u, v)).smooth = False


def _ring(centre, axis, radius, n, phase):
    e1 = _unit(_cross(axis, (0.0, 0.0, 1.0) if abs(axis[2]) < 0.9 else (1.0, 0.0, 0.0)))
    e2 = _cross(axis, e1)
    return [_add(_add(centre, e1, radius * math.cos(phase + 2 * math.pi * k / n)), e2,
                 radius * math.sin(phase + 2 * math.pi * k / n)) for k in range(n)]


def _tube(bm, rings, close_bottom=False, apex_top=None):
    """Smooth quads between successive rings (each counter-clockwise about the axis), an optional
    flat bottom cap and an optional fan to a top apex. Returns the faces."""
    vs = [[bm.verts.new(c) for c in r] for r in rings]
    n = len(vs[0])
    faces = []
    for a, b in zip(vs, vs[1:]):
        for k in range(n):
            faces.append(bm.faces.new([a[k], a[(k + 1) % n], b[(k + 1) % n], b[k]]))
    if close_bottom:
        faces.append(bm.faces.new(list(reversed(vs[0]))))
    if apex_top is not None:
        t = bm.verts.new(apex_top)
        faces += [bm.faces.new([vs[-1][k], vs[-1][(k + 1) % n], t]) for k in range(n)]
    for f in faces:
        f.smooth = True
    return faces


def _plan(at, seed, heads):
    """The clump, drawn from the seed alone: blades (kind, azimuth, centre line, half-width, twist)
    and cattails (stalk base, axis, stalk height, head length, head radius, spike length, phase)."""
    import random
    rnd = random.Random(seed)
    scale = rnd.uniform(0.85, 1.25)
    bias = rnd.uniform(0, 2 * math.pi)
    n_cat = rnd.choice((2, 3, 3, 4))
    kinds = ["hero", "tall"] + ["fill"] * (7 if heads else 7 + 2 * (n_cat % 2))
    blades = []
    for i, kind in enumerate(kinds):
        az = bias + 2 * math.pi * (i + rnd.uniform(-0.3, 0.3)) / len(kinds)
        r = rnd.uniform(0.0, 0.06)
        base = (at[0] + r * math.cos(az), at[1] + r * math.sin(az), at[2] - BURY)
        if kind == "hero":
            length, th0, th1, power, hw = rnd.uniform(1.5, 1.85) * scale, 0.06, rnd.uniform(1.6, 2.2), 3.5, rnd.uniform(0.028, 0.034)
        elif kind == "tall":
            length, th0, th1, power, hw = rnd.uniform(1.35, 1.7) * scale, 0.05, rnd.uniform(0.25, 0.55), 2.2, rnd.uniform(0.026, 0.031)
        else:
            length, th0, th1, power, hw = rnd.uniform(0.8, 1.3) * scale, rnd.uniform(0.05, 0.14), rnd.uniform(0.35, 0.7), 1.8, rnd.uniform(0.022, 0.028)
        line = _blade_line(at, base, az, length, th0, th1, power)
        blades.append((kind, az, line, hw, rnd.uniform(-1.0, 1.0)))
    stalks = []
    spread = rnd.uniform(0, 2 * math.pi)
    for k in range(n_cat if heads else 0):
        az = spread + 2 * math.pi * (k + rnd.uniform(-0.25, 0.25)) / n_cat
        r = rnd.uniform(0.03, 0.1)
        lean = rnd.uniform(0.04, 0.11)
        axis = (math.sin(lean) * math.cos(az), math.sin(lean) * math.sin(az), math.cos(lean))
        base = (at[0] + r * math.cos(az), at[1] + r * math.sin(az), at[2] - BURY)
        stalks.append((base, axis, rnd.uniform(1.2, 1.55) * scale, rnd.uniform(0.2, 0.28),
                       rnd.uniform(0.022, 0.027), rnd.uniform(0.12, 0.19), rnd.uniform(0, 2 * math.pi)))
    return blades, stalks


def _reed_clump(bm, at, seed, lod, heads):
    """A reed clump at `at`: LOD0 the full clump (`_plan`), with each cattail's stalk built here (reed
    slot) and its head and spike returned for `_cattail` (built after every reed face, so the
    callers' slot ranges hold); LOD1 four flat back-to-back triangles along the first four LOD0
    blades (every plan has at least four). Returns the cattail tops."""
    blades, stalks = _plan(at, seed, heads)
    if lod > 0:
        tris = []
        for _kind, az, line, hw, _tw in blades:
            p = (-math.sin(az), math.cos(az), 0.0)
            b = line[0][1]
            tris.append((_add(b, p, 1.6 * hw), _add(b, p, -1.6 * hw), _sample(line, 0.85)[0]))
        for co in tris[:4]:
            bm.faces.new([bm.verts.new(c) for c in co])
            bm.faces.new([bm.verts.new(c) for c in reversed(co)])
        return []
    rows = {"hero": HERO_ROWS, "tall": TALL_ROWS, "fill": FILL_ROWS}
    for kind, az, line, hw, tw in blades:
        _blade(bm, line, az, rows[kind], hw, tw, kind != "fill")
    tops = []
    for base, axis, hs, hl, rad, sl, ph in stalks:
        _tube(bm, [_ring(base, axis, 0.0075, 3, ph), _ring(_add(base, axis, hs + 0.04), axis, 0.0065, 3, ph)])
        tops.append((_add(base, axis, hs), axis, hl, rad, sl, ph))
    return tops


def _cattail(bm, top):
    """A seed head on its stalk: 6 sides, a flat bottom cap, tapering to 0.82 of the radius and
    rounding to an apex on top (smooth normals round the bottom edge too), then a pale 3-sided
    spike (stone slot, tagged in SPIKE_LAYER) out of the apex: 6 + 22 + 3 triangles with the stalk."""
    bottom, axis, hl, rad, sl, ph = top
    layer = bm.faces.layers.int.get(SPIKE_LAYER) or bm.faces.layers.int.new(SPIKE_LAYER)
    rings = [_ring(bottom, axis, rad, 6, ph), _ring(_add(bottom, axis, hl - 0.7 * rad), axis, 0.82 * rad, 6, ph)]
    _tube(bm, rings, close_bottom=True, apex_top=_add(bottom, axis, hl))
    spike = _tube(bm, [_ring(_add(bottom, axis, hl - 0.02), axis, 0.0055, 3, ph)],
                  apex_top=_add(bottom, axis, hl + sl))
    for f in spike:
        f[layer] = 1


def retag_spikes(bm, stone_index):
    """Moves the cattail spikes (SPIKE_LAYER) into the stone slot, after the callers' range-based
    slot assignment."""
    layer = bm.faces.layers.int.get(SPIKE_LAYER)
    if layer is None:
        return
    for f in bm.faces:
        if f[layer]:
            f.material_index = stone_index


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
            tops += _reed_clump(bm, at, seed, li, li == 0)
        n_reed = len(bm.faces)
        for t in tops:
            _cattail(bm, t)
        bm.faces.ensure_lookup_table()
        for fi, f in enumerate(bm.faces):
            f.material_index = 0 if fi < n_stone else (1 if fi < n_reed else 2)
            if fi < n_stone:
                f.smooth = False
        retag_spikes(bm, 0)
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
    assert tris[0] <= LOD0_MAX and tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
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
