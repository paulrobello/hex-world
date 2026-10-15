"""Dressing centres for the river mouth tiles (refs/river_mouth, PLAN.md §2): beach_center's dune
grass, driftwood and pebbles on the two dry-sand shoulders either side of the channel, and
river_straight_center's bank stones at the channel waterline and reed clumps on the grass reach
(s <= -8). Colour-only slots plus the piece's §9.4 masks map on the planar UV0 (`masks_field`).

Every item is seated on the pinned mouth bed (`_mouth_profile.height`). The layout is generated
once in the "r" frame (River on edge 3) from one seed; the "l" piece reflects each 2D anchor and
axis across the coast's seaward axis and builds every item natively at the reflected anchor with
the "l" bed, so winding and normals stay authored (never a mirrored mesh)."""
import math
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np

from lib import bake, collider, export, geom, hexgeo, materials, scene, uv

import _beach_dress
import _mouth_profile as mp
import _river_dress

TUFT, WOOD, PEBBLE = _beach_dress.TUFT, _beach_dress.WOOD, _beach_dress.PEBBLE
STONE, REED = _river_dress.STONE, _river_dress.REED
DRESS_SLOTS = (TUFT, WOOD, PEBBLE, STONE, REED)
COLORS = {**_beach_dress.COLORS, STONE: ("#8a8378", 0.8), REED: ("#5d7a35", 0.85)}
SEED = "river_mouth_center"
DRY_S = _beach_dress.DRY_S
DRY_Y = -0.8  # driftwood and tufts stand on bed at least 0.6 m above the water
BORDER_M = _beach_dress.BORDER_M
STUB_M = 1.0  # river_edge owns the River edge's last metre
GRASS_S = mp.BEACH_TOP_S
MASK_SIZE = 256  # 8 px/m on the 32 m planar UV0
POROSITY = (1.0, 0.55)  # G at the water (bed <= WATER + 0.3) and on the grass (bed >= WATER + 2.4)
COAST = (mp.COS30, -mp.SIN30)  # unit seaward axis in Blender (x, y); the l/r mirror line


def reflect(p):
    """Mirror Blender (x, y) across the seaward axis: the "r" frame to the "l" frame and back."""
    k = 2.0 * (p[0] * COAST[0] + p[1] * COAST[1])
    return (k * COAST[0] - p[0], k * COAST[1] - p[1])


def point(s, a):
    """Blender (x, y) at seaward distance s and distance a along the coast (toward edge 5)."""
    return (s * mp.COS30 + a * mp.SIN30, -s * mp.SIN30 + a * mp.COS30)


def masks_field(hand):
    """The §9.4 masks (R, G, B, A), each a (size*size,) array in Blender pixel order over the planar
    UV0. An analytic field of the bed under (x, y), so no bake. R snow 1 on dry bed and 0 under the
    water. G porosity, wettest at the water and easing off up the beach and the banks. B 0. A 1: the
    slots had no AO, so the MVP look is unchanged."""
    c = (np.arange(MASK_SIZE, dtype=np.float64) + 0.5) / MASK_SIZE * 2 * hexgeo.HEX_R - hexgeo.HEX_R
    xs, ys = np.meshgrid(c, c)
    above = np.vectorize(lambda x, y: mp.height(hand, x, y))(xs, ys) - mp.WATER
    t = np.clip((above - 0.3) / 2.1, 0.0, 1.0)
    wet = POROSITY[0] + (POROSITY[1] - POROSITY[0]) * t * t * (3.0 - 2.0 * t)
    n = MASK_SIZE * MASK_SIZE
    snow = (above >= 0.0).astype(np.float32).ravel()
    return (snow, wet.ravel().astype(np.float32), np.zeros(n, np.float32), np.ones(n, np.float32))


def _sample(field, x, y):
    col = min(MASK_SIZE - 1, max(0, int((x + hexgeo.HEX_R) / (2 * hexgeo.HEX_R) * MASK_SIZE)))
    row = min(MASK_SIZE - 1, max(0, int((y + hexgeo.HEX_R) / (2 * hexgeo.HEX_R) * MASK_SIZE)))
    return field[row * MASK_SIZE + col]


def waterline(x, side):
    """Perpendicular distance from the "r" centre line (y = 0) to the water's edge on `side` at the
    axis point x, by bisection on the bed height."""
    lo, hi = 0.0, 9.0
    for _ in range(40):
        mid = 0.5 * (lo + hi)
        if mp.height("r", x, side * mid) < mp.WATER:
            lo = mid
        else:
            hi = mid
    return 0.5 * (lo + hi)


def placements():
    """Deterministic [(kind, x, y, params)] in the "r" frame (River on edge 3, centre line y = 0)."""
    rng = random.Random(SEED)
    items = []
    lane_x = -hexgeo.APOTHEM

    def free(x, y, r):
        return (hexgeo.dist_to_border(x, y) >= BORDER_M + r and x - lane_x >= STUB_M + r
                and all(math.hypot(x - i[1], y - i[2]) > r + i[3]["r"] + 0.15 for i in items))

    def dry(pts):
        return all(not mp.in_channel("r", px, py) and mp.height("r", px, py) > DRY_Y for px, py in pts)

    def ring(x, y, r, n=8):
        return [(x, y)] + [(x + r * math.cos(2 * math.pi * k / n), y + r * math.sin(2 * math.pi * k / n))
                           for k in range(n)]

    # Driftwood first (largest): logs lie roughly along the coast on the dry sand shoulders, two on
    # the wide edge-5 shoulder (y > 0) and one shorter log on the narrow edge-2 wedge (y < 0).
    for side, length_rng, yaw_deg in ((1, (1.6, 2.6), 25), (1, (1.6, 2.6), 25), (-1, (1.1, 1.5), 45)):
        length, radius = rng.uniform(*length_rng), rng.uniform(0.09, 0.15)
        yaw = math.radians(rng.uniform(-yaw_deg, yaw_deg))
        along = (mp.SIN30, mp.COS30)
        d = (along[0] * math.cos(yaw) - along[1] * math.sin(yaw), along[0] * math.sin(yaw) + along[1] * math.cos(yaw))
        for _t in range(600):
            x, y = point(rng.uniform(-7.5, DRY_S), rng.uniform(-14.0, 14.0))
            nrm = (-d[1], d[0])
            pts = [(x + d[0] * f * length + nrm[0] * o, y + d[1] * f * length + nrm[1] * o)
                   for f in (-0.5, -0.25, 0.0, 0.25, 0.5) for o in (-0.3, 0.3)]
            if y * side > 0 and mp.seaward(x, y) <= DRY_S and free(x, y, length / 2) and dry(pts):
                items.append((WOOD, x, y, {"r": length / 2, "seed": rng.randrange(1 << 30), "length": length,
                                           "radius": radius, "dir": d}))
                break
    # Bank stones at the channel waterline, both banks, on the land-banked reach (s < -1).
    for side, n, x_hi in ((1, 7, 2.0), (-1, 5, -3.5)):
        placed = 0
        for _t in range(400):
            if placed == n:
                break
            x = rng.uniform(lane_x + 2.0, x_hi)
            wl = waterline(x, side)
            y = side * wl * rng.uniform(0.92, 1.1)
            size = (rng.uniform(0.25, 0.55), rng.uniform(0.2, 0.45), rng.uniform(0.15, 0.3))
            if mp.seaward(x, y) < -1.0 and free(x, y, size[0]):
                items.append((STONE, x, y, {"r": size[0], "seed": rng.randrange(1 << 30), "size": size}))
                placed += 1
    # Reed clumps up the bank on the grass reach (s <= -8), both banks where it fits.
    for side in (1, -1):
        placed = 0
        for _t in range(300):
            if placed == 4:
                break
            x = rng.uniform(lane_x + 2.0, -5.0)
            y = side * (waterline(x, side) + rng.uniform(0.7, 1.7))
            if all(mp.seaward(px, py) <= GRASS_S for px, py in ring(x, y, 0.6)) and free(x, y, 0.6):
                items.append((REED, x, y, {"r": 0.6, "seed": rng.randrange(1 << 30)}))
                placed += 1
    def group(kind, side, per, s_rng, spread, r_rng, check_r):
        """One loose group of `per` items on the `side` shoulder: an anchor found by rejection, then
        the rest scattered within `spread` of it."""
        for _t in range(600):
            x0, y0 = point(rng.uniform(*s_rng), rng.uniform(-15.0, 15.0))
            r = rng.uniform(*r_rng)
            if y0 * side > 0 and free(x0, y0, r) and dry(ring(x0, y0, check_r)):
                items.append((kind, x0, y0, {"r": r, "seed": rng.randrange(1 << 30)}))
                break
        else:
            return
        got = 1
        for _t in range(120):
            if got == per:
                break
            x, y = x0 + rng.uniform(-spread, spread), y0 + rng.uniform(-spread, spread)
            r = rng.uniform(*r_rng)
            if s_rng[0] <= mp.seaward(x, y) <= s_rng[1] and free(x, y, r) and dry(ring(x, y, check_r)):
                items.append((kind, x, y, {"r": r, "seed": rng.randrange(1 << 30)}))
                got += 1

    # Dune-grass clumps in loose groups straddling the grass edge (s = -8) on the edge-5 shoulder;
    # the edge-2 wedge has no grass edge inside the border band, so its group stands on the upper sand.
    for side, s_rng in ((1, (-9.8, -5.8)), (1, (-9.8, -5.8)), (1, (-9.8, -5.8)), (-1, (-9.8, -4.0))):
        group(TUFT, side, 3 if side > 0 else 2, s_rng, 1.4, (0.4, 0.4), 0.7)
    # Pebbles at the grass edge (upper sand on the edge-2 wedge) in small groups, plus a few small
    # ones on the shelf off the fan.
    for side, s_rng in ((1, (-8.6, -6.2)), (1, (-8.6, -6.2)), (-1, (-9.0, -4.0))):
        group(PEBBLE, side, 3 if side > 0 else 2, s_rng, 1.0, (0.09, 0.18), 0.2)
    got = 0
    for _t in range(200):
        if got == 3:
            break
        s = rng.uniform(0.4, 2.0)
        x, y = point(s, s * math.tan(math.radians(30.0)) + rng.uniform(-3.5, 3.5))
        r = rng.uniform(0.06, 0.08)
        if free(x, y, r):
            items.append((PEBBLE, x, y, {"r": r, "seed": rng.randrange(1 << 30), "wet": True}))
            got += 1
    return items


def mirrored(items, hand):
    """The "r" layout in `hand`'s frame: anchors and log axes reflected for "l"."""
    if hand == "r":
        return items
    out = []
    for kind, x, y, p in items:
        q = dict(p)
        if "dir" in p:
            q["dir"] = reflect(p["dir"])
        rx, ry = reflect((x, y))
        assert abs(mp.height("l", rx, ry) - mp.height("r", x, y)) < 1e-9, (kind, x, y)
        out.append((kind, rx, ry, q))
    return out


def _tuft(bm, ground, x, y, seed, blades):
    """_beach_dress._tuft on the mouth bed."""
    rnd = random.Random(seed)
    z0 = ground(x, y)
    for _ in range(blades):
        a = rnd.uniform(0, 2 * math.pi)
        r = rnd.uniform(0.0, 0.3)
        bx, by = x + r * math.cos(a), y + r * math.sin(a)
        h = rnd.uniform(0.45, 0.85)
        lean = rnd.uniform(0.1, 0.35)
        tx, ty = bx + lean * math.cos(a), by + lean * math.sin(a)
        px, py = -math.sin(a) * 0.035, math.cos(a) * 0.035
        zb = ground(bx, by) - 0.06
        co = [(bx + px, by + py, zb), (bx - px, by - py, zb), (tx, ty, z0 + h)]
        bm.faces.new([bm.verts.new(c) for c in co])
        bm.faces.new([bm.verts.new(c) for c in reversed(co)])


def _log(bm, ground, x, y, p, sides, segs):
    """_beach_dress._log on the mouth bed, along the placement's own axis `dir`."""
    rnd = random.Random(p["seed"])
    d = p["dir"]
    nrm = (-d[1], d[0])
    L, R = p["length"], p["radius"]
    bend = rnd.uniform(-0.15, 0.15)
    rings = []
    for i in range(segs + 1):
        t = i / segs
        cx = x + d[0] * (t - 0.5) * L + nrm[0] * bend * math.sin(math.pi * t)
        cy = y + d[1] * (t - 0.5) * L + nrm[1] * bend * math.sin(math.pi * t)
        r = R * (1.0 - 0.35 * t) * (1 + rnd.uniform(-0.08, 0.08))
        cz = ground(cx, cy) + 0.45 * r
        ring = []
        for k in range(sides):
            a = 2 * math.pi * k / sides
            ring.append(bm.verts.new((cx + nrm[0] * r * math.cos(a), cy + nrm[1] * r * math.cos(a),
                                      cz + r * math.sin(a))))
        rings.append(ring)
    for i in range(segs):
        for k in range(sides):
            a, b = rings[i][k], rings[i][(k + 1) % sides]
            c, e = rings[i + 1][(k + 1) % sides], rings[i + 1][k]
            bm.faces.new([a, b, c, e])
    bm.faces.new(list(reversed(rings[0])))
    bm.faces.new(rings[-1])


def _pebble(bm, ground, x, y, p):
    """_beach_dress._pebble on the mouth bed."""
    rnd = random.Random(p["seed"])
    m = bmesh.ops.create_icosphere(bm, subdivisions=1, radius=1.0)
    w, h = p["r"], p["r"] * rnd.uniform(0.45, 0.65)
    z = ground(x, y) + 0.2 * h
    for v in m["verts"]:
        j = 1.0 + 0.2 * (rnd.random() - 0.5)
        v.co.x = x + v.co.x * w * j
        v.co.y = y + v.co.y * w * 0.85 * j
        v.co.z = z + v.co.z * h * j


def _check_masks(piece, lods, field):
    """Mirror of the hooks gate (`channel_errors`): per LOD and slot, the masks sampled at every
    triangle's UV0 centroid must carry R > 0 and G > 0 somewhere."""
    r, g = field[0], field[1]
    for o in lods:
        best = {}
        for poly in o.data.polygons:
            cx, cy = poly.center.x, poly.center.y
            name = o.data.materials[poly.material_index].name
            br, bg = best.get(name, (0.0, 0.0))
            best[name] = (max(br, _sample(r, cx, cy)), max(bg, _sample(g, cx, cy)))
        for name, (br, bg) in best.items():
            assert br > 0.0 and bg > 0.0, f"{piece} {o.name} {name}: masks max R {br} G {bg}"


def build(piece, hand, generator):
    """Three LODs: LOD0 everything; LOD1 coarser logs, the bigger half of the stones and pebbles,
    half the tufts and reeds with fewer blades; LOD2 the logs as 5-sided single segments and the
    three biggest stones."""
    mp.check_spots()
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    scene.reset_scene(piece)
    root = scene.empty(piece)
    mats = {k: materials.pbr_material(k, materials.srgb(c), r) for k, (c, r) in COLORS.items()}
    base = placements()
    items = mirrored(base, hand)

    def ground(x, y):
        return mp.height(hand, x, y)

    def of(kind):
        return [i for i in items if i[0] == kind]

    logs, tufts, reeds = of(WOOD), of(TUFT), of(REED)
    stones = sorted(of(STONE), key=lambda i: -i[3]["size"][0] * i[3]["size"][1])
    pebbles = sorted(of(PEBBLE), key=lambda i: -i[3]["r"])
    counts = {
        "logs": len(logs), "tufts": len(tufts), "stones": len(stones), "reedClumps": len(reeds),
        "pebbles": len(pebbles), "pebblesUnderWater": sum(1 for i in pebbles if i[3].get("wet")),
        "dryItemsEdge5Shoulder": sum(1 for i in base if i[0] in (WOOD, TUFT) and i[2] > 0),
        "dryItemsEdge2Shoulder": sum(1 for i in base if i[0] in (WOOD, TUFT) and i[2] < 0),
        "reedsEdge5Bank": sum(1 for i in base if i[0] == REED and i[2] > 0),
        "reedsEdge2Bank": sum(1 for i in base if i[0] == REED and i[2] < 0),
    }
    print(f"{piece}: {counts}")
    plan = (
        {"log": (8, 3), "stones": stones, "reeds": reeds, "tufts": tufts, "blades": 7,
         "pebbles": pebbles},
        {"log": (6, 2), "stones": stones[: len(stones) // 2], "reeds": reeds[::2],
         "tufts": tufts[::2], "blades": 4, "pebbles": pebbles[: len(pebbles) // 2]},
        {"log": (5, 1), "stones": stones[:3], "reeds": [], "tufts": [], "blades": 0,
         "pebbles": []},
    )
    lods = []
    for li, pl in enumerate(plan):
        bm = bmesh.new()
        spans = []
        for _k, x, y, p in pl["tufts"]:
            _tuft(bm, ground, x, y, p["seed"], pl["blades"])
        spans.append(len(bm.faces))
        for _k, x, y, p in logs:
            _log(bm, ground, x, y, p, *pl["log"])
        spans.append(len(bm.faces))
        for _k, x, y, p in pl["pebbles"]:
            _pebble(bm, ground, x, y, p)
        spans.append(len(bm.faces))
        for _k, x, y, p in pl["stones"]:
            _river_dress._stone(bm, (x, y, ground(x, y) + 0.03), p["size"], p["seed"])
        spans.append(len(bm.faces))
        for _k, x, y, p in pl["reeds"]:
            _river_dress._reed_clump(bm, (x, y, ground(x, y)), p["seed"], li, False)
        bm.faces.ensure_lookup_table()
        for fi, f in enumerate(bm.faces):
            f.material_index = next((si for si, end in enumerate(spans) if fi < end), len(spans))
            if DRESS_SLOTS[f.material_index] != REED:
                f.smooth = DRESS_SLOTS[f.material_index] in (WOOD, PEBBLE)
        bmesh.ops.recalc_face_normals(bm, faces=[f for f in bm.faces if DRESS_SLOTS[f.material_index] in (WOOD, PEBBLE)])
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(f"{piece}_lod{li}")
        bm.to_mesh(mesh)
        bm.free()
        o = scene.link(bpy.data.objects.new(f"{piece}_lod{li}", mesh))
        for name in DRESS_SLOTS:
            o.data.materials.append(mats[name])
        uv.planar_cap_uv(o)
        print(f"{piece}_lod{li}: wind U max {_river_dress.wind_uv(o):.3f}")
        o.parent = root
        lods.append(o)
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    assert tris[0] <= _river_dress.LOD0_MAX and tris[1] <= 388 and tris[2] <= 120, tris
    assert tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    (lx, ly), (ux, uy) = mp.axis(hand)
    for o in lods:
        for v in o.data.vertices:
            x, y = v.co.x, v.co.y
            assert hexgeo.dist_to_border(x, y) >= 1.0, (o.name, tuple(v.co))
            assert (x - lx) * ux + (y - ly) * uy >= STUB_M, (o.name, "river_edge stub", tuple(v.co))
    for kind, x, y, _p in items:
        if kind in (WOOD, TUFT):
            assert not mp.in_channel(hand, x, y) and ground(x, y) > DRY_Y, (kind, x, y)
    field = masks_field(hand)
    _check_masks(piece, lods, field)
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    bake.write_png(MASK_SIZE, np.stack(field, axis=1), str(out / f"{piece}.masks.png"))
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, skipped=True,
                           reason="no Cycles bake: the masks are an analytic field of the mouth bed on the planar UV0",
                           masks={"snow": "1 on dry bed, 0 under the water", "porosity": list(POROSITY),
                                  "night": 0.0, "ao": 1.0},
                           dressing={**counts, "hand": hand, "tris": tris})
    return root, lods
