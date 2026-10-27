"""Water-surface scatter props (B3, owner A26, 2026-10-27): lily-pad clusters floating on the river and
spring water and a dense common-reed bed standing in the shallows. Refs: assets-source/refs/water_props
(ref-1 the pads' colour and form, ref-2 the reed bed; ref-3 and ref-4 layout only). The runtime scatter
places them (`hex-play` scatter): a pad cluster's pivot at the water at rest plus 0.01 m (y -1.39, as
river_spring_center's pads), a reed bed's pivot at the water at rest (y -1.4) with its +y (Blender)
pointing up the bank.

- Lily clusters (`lily`): `river_spring_center`'s pad, copied verbatim (a 7-vertex disc fan with one
  90-degree wedge cut out, 6 tris) in its `river_lily` slot and colour, packed into a disc of the
  cluster's reach. LOD1 keeps every pad as 3 tris (a 4-vertex fan that keeps the notch); LOD2 keeps the
  biggest pads as 2-tri quads, as many as the 15% ceiling allows (the gate needs a LOD2 mesh, so the
  pads cannot leave it entirely). `lily_cluster_b` carries a white water lily (15 cupped double-sided
  petals and a yellow heart), `lily_cluster_c` a pink bud on a short stalk; at LOD1 each reduces to a
  few tris and both leave LOD2 with a measured reason (`FLOWER_LOD2_DROP`).
- Reed bed (`reed_bed`): about 70 common-reed stems in a 4 x 2 m ellipse, tallest in the middle and
  splaying outward, each a 3-sided stem with two drooping double-sided leaves, most with a feathery
  plume of three crossed double-sided fins (mauve-buff, `reed_bed_plume`); a few buff stems take the
  plume slot too. Stem bases follow the river bank profile across the bed (`_river.profile_z` about
  3.3 m out) 0.3 m into the bed, so the bank-side stems rise out of the bank and the channel-side ones
  out of the water. LOD1 about 400 tris (the tallest stems, straight, with two-fin plumes, and leaf
  fans); LOD2 two crossed tapered cards, leaf band below and plume band on top, so both slots stay.

Colour-only slots plus a constant 16x16 section 9.4 masks map (R 1 snow, G porosity, B 0, A 1), no Cycles
bake. The pads' UVs point at texel (0, 0), as the spring's pads do; every other slot is planar.
Wind (TEXCOORD_1 `wind`) on the reed bed only: U 0 at the water surface or the stem's own base
(whichever is higher) rising to `_river_dress.wind_scale` of the stem's visible height at its tip,
V a phase per stem. The lily pieces carry no wind."""
import math
import pathlib
import random
import sys

import bmesh
import bpy
import numpy as np

from lib import bake, export, geom, materials, scene, uv

import _river_dress as rd
from _river import WATER, profile_z

LILY, PETAL, HEART, BUD = "river_lily", "water_lily_petal", "water_lily_heart", "water_lily_bud"
LEAF, PLUME = "reed_bed_leaf", "reed_bed_plume"
COLORS = {
    LILY: ("#4f6b3a", 0.7),  # river_spring_center's river_lily, exactly
    PETAL: ("#eeebe2", 0.6),
    HEART: ("#e2b62e", 0.7),
    BUD: ("#d98aa6", 0.6),
    LEAF: ("#7a8a5e", 0.85),
    PLUME: ("#b39a8f", 0.9),
}
MASK_SIZE = 16
LILY_UV = (0.5 / MASK_SIZE, 0.5 / MASK_SIZE)  # texel (0, 0), the spring's dry off-hex texel
LILY_POROSITY = 0.8  # the spring's off-hex texel: dry meadow G
REED_POROSITY = 0.9  # a reed bed stands at the waterline (1.0) below a bank (0.8), as the bank props
GAP = 0.04  # least gap between two pads of a cluster
COLLIDER_H = 0.05
PX_400 = 0.31  # metres per pixel at 400 m (45 degree vertical field of view, 1080 rows)
REED_D = 3.3  # nominal distance of a reed bed's centre from the river centre line (scatter band 3.0-3.6)
REED_BURY = 0.3


def lily(bm, at, radius, rot):
    """A flat pad: a 7-segment disc fan with one wedge cut out (6 tris), facing up
    (`river_spring_center.lily`, verbatim)."""
    c = bm.verts.new(at)
    ring = []
    for k in range(7):
        a = rot + 2 * math.pi * k / 8
        ring.append(bm.verts.new((at[0] + radius * math.cos(a), at[1] + radius * math.sin(a), at[2])))
    return [bm.faces.new([c, ring[k], ring[k + 1]]) for k in range(6)]


def lily_lo(bm, at, radius, rot):
    """LOD1 pad: the fan on ring vertices 0, 2, 4 and 6 of `lily` (3 tris), so the notch stays."""
    c = bm.verts.new(at)
    ring = [bm.verts.new((at[0] + radius * math.cos(rot + math.pi * k / 2),
                          at[1] + radius * math.sin(rot + math.pi * k / 2), at[2])) for k in range(4)]
    return [bm.faces.new([c, ring[k], ring[k + 1]]) for k in range(3)]


def lily_quad(bm, at, radius, rot):
    """LOD2 pad: the square on ring vertices 0, 2, 4 and 6 of `lily` (2 tris)."""
    ring = [bm.verts.new((at[0] + radius * math.cos(rot + math.pi * k / 2),
                          at[1] + radius * math.sin(rot + math.pi * k / 2), at[2])) for k in range(4)]
    return [bm.faces.new([ring[0], ring[1], ring[2]]), bm.faces.new([ring[0], ring[2], ring[3]])]


def pad_plan(seed, radii, reach, keep_out=()):
    """Pads (centre, radius, rotation), largest first, packed toward the centre of a disc of radius
    `reach` (|c| + r <= reach), at least GAP apart and clear of `keep_out` [(x, y, r)]."""
    rnd = random.Random(seed)
    best = None
    for _attempt in range(800):
        pads = []
        for r in sorted(radii, reverse=True):
            for _ in range(600):
                a, q = rnd.uniform(0, 2 * math.pi), (reach - r) * math.sqrt(rnd.random())
                x, y = q * math.cos(a), q * math.sin(a)
                if any(math.hypot(x - p[0][0], y - p[0][1]) < p[1] + r + GAP for p in pads):
                    continue
                if any(math.hypot(x - kx, y - ky) < kr + r + GAP for kx, ky, kr in keep_out):
                    continue
                pads.append(((x, y, 0.0), r, rnd.uniform(0, 2 * math.pi)))
                break
            else:
                break
        if len(pads) < len(radii):
            continue
        # Tightest layout: the smallest sum of gaps between each pad and its nearest neighbour.
        score = sum(min(math.hypot(p[0][0] - o[0][0], p[0][1] - o[0][1]) - p[1] - o[1]
                        for o in pads if o is not p) for p in pads)
        if best is None or score < best[0]:
            best = (score, pads)
    assert best is not None, (seed, radii, reach)
    return best[1]


def _double(bm, cos_top):
    """A face on its own vertices facing +z (by winding), plus its back face on other vertices."""
    a = [bm.verts.new(c) for c in cos_top]
    f = bm.faces.new(a)
    f.normal_update()
    if f.normal.z < 0:
        f.normal_flip()
    b = [bm.verts.new(c) for c in cos_top]
    g = bm.faces.new(b)
    g.normal_update()
    if g.normal.z > 0:
        g.normal_flip()
    return [f, g]


def flower(bm, at, rot, lod):
    """White water lily at `at`: petal faces, then heart faces (returned as two lists)."""
    petals, heart = [], []
    x0, y0, z0 = at
    rings = ((8, 0.15, 0.058, math.radians(18), 0.0), (7, 0.11, 0.048, math.radians(48), 0.5))
    if lod == 1:
        rings = ((5, 0.15, 0.07, math.radians(25), 0.0),)
    for n, length, width, elev, off in rings:
        for k in range(n):
            a = rot + 2 * math.pi * (k + off) / n
            e = (math.cos(a) * math.cos(elev), math.sin(a) * math.cos(elev), math.sin(elev))
            p = (-math.sin(a), math.cos(a), 0.0)
            b = (x0 + 0.022 * math.cos(a), y0 + 0.022 * math.sin(a), z0 + 0.01)
            if lod == 0:
                mid = rd._add(b, e, 0.45 * length)
                left = rd._add(rd._add(mid, p, 0.5 * width), (0, 0, 1), 0.012)
                right = rd._add(rd._add(mid, p, -0.5 * width), (0, 0, 1), 0.012)
                tip = rd._add(b, e, length)
                petals += _double(bm, (b, right, tip))
                petals += _double(bm, (b, tip, left))
            else:
                tip = rd._add(b, e, 0.85 * length)
                petals += _double(bm, (b, rd._add(tip, p, -0.5 * width), rd._add(tip, p, 0.5 * width)))
    n, r = (6, 0.035) if lod == 0 else (3, 0.04)
    apex = bm.verts.new((x0, y0, z0 + 0.065))
    ring = [bm.verts.new((x0 + r * math.cos(rot + 2 * math.pi * k / n), y0 + r * math.sin(rot + 2 * math.pi * k / n),
                          z0 + 0.03)) for k in range(n)]
    heart += [bm.faces.new((ring[k], ring[(k + 1) % n], apex)) for k in range(n)]
    return petals, heart


def bud(bm, at, tilt, lod):
    """Pink bud: a closed teardrop (5 sides: two bands and an apex, 15 tris; LOD1 a 3-sided spindle,
    6 tris), tilted by `tilt` (radians, about the bud's local y), returning its faces."""
    x0, y0, z0 = at
    lean = (math.sin(tilt), 0.0, math.cos(tilt))

    def pt(h, r, a):
        return (x0 + lean[0] * h + r * math.cos(a) * math.cos(tilt), y0 + r * math.sin(a),
                z0 + lean[2] * h - r * math.cos(a) * math.sin(tilt))
    if lod == 0:
        n, rows, apex_h = 5, ((0.0, 0.045), (0.09, 0.058)), 0.22
    else:
        n, rows, apex_h = 3, ((0.08, 0.055),), 0.2
    faces = []
    if lod == 0:
        rings = [[bm.verts.new(pt(h, r, 2 * math.pi * k / n)) for k in range(n)] for h, r in rows]
        for lo, hi in zip(rings, rings[1:]):
            faces += [bm.faces.new((lo[k], lo[(k + 1) % n], hi[(k + 1) % n], hi[k])) for k in range(n)]
        top = rings[-1]
    else:
        bottom = bm.verts.new(pt(0.0, 0.0, 0.0))
        top = [bm.verts.new(pt(h, r, 2 * math.pi * k / n)) for h, r in rows for k in range(n)]
        faces += [bm.faces.new((top[(k + 1) % n], top[k], bottom)) for k in range(n)]
    apex = bm.verts.new(pt(apex_h, 0.0, 0.0))
    faces += [bm.faces.new((top[k], top[(k + 1) % n], apex)) for k in range(n)]
    return faces


def stalk(bm, at, tilt):
    """The bud's green stalk from just under the water to the bud's foot (3-sided, 6 tris)."""
    foot = (at[0], at[1], at[2])
    base = (at[0] - 0.04 * math.sin(tilt), at[1], -0.06)
    axis = rd._unit(tuple(f - b for f, b in zip(foot, base)))
    rings = [rd._ring(base, axis, 0.011, 3, 0.0), rd._ring(foot, axis, 0.009, 3, 0.0)]
    return rd._tube(bm, rings)


def _collider_disc(piece, reach):
    """A thin octagonal prism of circumradius `reach` (the scatter footprint, `footprints_match_the_kit`)."""
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=8, radius1=reach, radius2=reach, depth=COLLIDER_H)
    bmesh.ops.translate(bm, verts=bm.verts, vec=(0.0, 0.0, -COLLIDER_H / 2))
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(f"{piece}_collider")
    bm.to_mesh(mesh)
    bm.free()
    obj = scene.link(bpy.data.objects.new(f"{piece}_collider", mesh))
    obj["collider"] = "hull"
    return obj


def _finish(piece, generator, root, lods, slots_per_lod, porosity, drops, stats, wind):
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris}")
    assert tris[1] <= 0.5 * tris[0] and tris[2] <= 0.15 * tris[0], tris
    n = MASK_SIZE * MASK_SIZE
    masks = np.stack((np.ones(n, np.float32), np.full(n, porosity, np.float32), np.zeros(n, np.float32),
                      np.ones(n, np.float32)), axis=1)
    bake.write_png(MASK_SIZE, masks, str(out / f"{piece}.masks.png"))
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    stats = dict(stats, tris=tris, slotsPerLod=slots_per_lod)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=bake.receiver_signature([lods[0]]),
                           device="none", samples=0, marginPx=0,
                           channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           props=stats, skipped=True,
                           reason="no Cycles bake: colour-only slots and a constant masks map on the planar UV0",
                           masks={"snow": 1.0, "porosity": porosity, "night": 0.0, "ao": 1.0,
                                  **({"lilyUv": list(LILY_UV)} if not wind else {})},
                           lod2Drops=drops)
    return tris


def _mesh(piece, li, bm, slots, spans, mats):
    """Triangulates `bm` into object `<piece>_lod<li>`; `spans` [(first face, slot)] in face order."""
    bm.faces.ensure_lookup_table()
    for fi, f in enumerate(bm.faces):
        f.material_index = slots.index([s for start, s in spans if start <= fi][-1])
        f.smooth = False
    bmesh.ops.triangulate(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(f"{piece}_lod{li}")
    bm.to_mesh(mesh)
    bm.free()
    o = scene.link(bpy.data.objects.new(f"{piece}_lod{li}", mesh))
    for s in slots:
        o.data.materials.append(mats[s])
    uv.planar_cap_uv(o)
    return o


def build_lily(piece, generator, radii, reach, extra=None, seed=None):
    """A pad cluster of `radii` in a disc of `reach`; `extra` is None, "flower" or "bud"."""
    scene.reset_scene(piece)
    root = scene.empty(piece)
    slots = [LILY] + {None: [], "flower": [PETAL, HEART], "bud": [BUD]}[extra]
    mats = {s: materials.pbr_material(s, materials.srgb(COLORS[s][0]), COLORS[s][1]) for s in slots}
    rnd = random.Random(f"{piece}-extra")
    spot = None
    if extra:
        a, q = rnd.uniform(0, 2 * math.pi), rnd.uniform(0.05, 0.2)
        spot = (q * math.cos(a), q * math.sin(a), 0.17 if extra == "flower" else 0.08)
    pads = pad_plan(seed if seed is not None else piece, radii, reach, [spot] if spot else ())
    lod0_tris = 6 * len(pads) + {None: 0, "flower": 15 * 4 + 6, "bud": 15 + 6}[extra]
    n_lod2 = min(len(pads), int(0.15 * lod0_tris) // 2)
    lods, slots_per_lod = [], []
    rot_x = rnd.uniform(0, 2 * math.pi)
    tilt = math.radians(rnd.uniform(6, 14))
    for li in range(3):
        bm = bmesh.new()
        spans = [(0, LILY)]
        for at, r, rot in pads[: (len(pads), len(pads), n_lod2)[li]]:
            (lily, lily_lo, lily_quad)[li](bm, at, r, rot)
        if extra == "bud" and li == 0:
            stalk(bm, (spot[0], spot[1], 0.03), tilt)
        if extra and li < 2:
            if extra == "flower":
                spans.append((len(bm.faces), PETAL))
                petals, heart = flower(bm, (spot[0], spot[1], 0.0), rot_x, li)
                spans.append((len(bm.faces) - len(heart), HEART))
            else:
                spans.append((len(bm.faces), BUD))
                bud(bm, (spot[0], spot[1], 0.03), tilt, li)
        o = _mesh(piece, li, bm, slots, spans, mats)
        layer = o.data.uv_layers["UVMap"].data
        for p in o.data.polygons:
            if p.material_index == 0:
                for k in p.loop_indices:
                    layer[k].uv = LILY_UV
        slots_per_lod.append(sorted({slots[p.material_index] for p in o.data.polygons}))
        o.parent = root
        lods.append(o)
    reach_m = max(math.hypot(v.co.x, v.co.y) for o in lods for v in o.data.vertices)
    col = _collider_disc(piece, reach_m)
    col.parent = root
    drops = {}
    if extra == "flower":
        drops = {PETAL: FLOWER_LOD2_DROP, HEART: FLOWER_LOD2_DROP}
    elif extra == "bud":
        drops = {BUD: BUD_LOD2_DROP}
    stats = {"pads": len(pads), "lod2Pads": n_lod2, "reachM": round(reach_m, 4),
             "padRadii": [round(r, 3) for _, r, _ in pads], "extra": extra}
    tris = _finish(piece, generator, root, lods, slots_per_lod, LILY_POROSITY, drops, stats, False)
    assert tris[0] == lod0_tris, (tris, lod0_tris)
    print(f"{piece}: reach {reach_m:.3f} m, pads {len(pads)}, LOD2 pads {n_lod2}")


FLOWER_LOD2_DROP = ("about a pixel at 400 m: the flower is 0.33 m across and 0.08 m tall "
                    f"(1.1 x 0.3 px at {PX_400} m/px), and LOD2 is held to 15% of LOD0 by the pads it keeps")
BUD_LOD2_DROP = (f"under a pixel at 400 m: the bud is 0.12 m across and 0.25 m tall (0.4 x 0.8 px at {PX_400} m/px)")


# ---------------------------------------------------------------------------------------------
# Reed bed

def _bank_z(y):
    """Bed height (Blender z, relative to the water at rest) under a reed-bed point `y` m up the bank
    from its centre, at the nominal centre REED_D from the river centre line."""
    return profile_z(REED_D + y) - WATER


def reed_plan(seed, count=70, a=1.8, b=0.9):
    """Stems (x, y, height, lean azimuth, lean, plume kind, seed) in an a x b ellipse, tallest in the
    middle. Plume kinds: 2 plume, 1 buff stem with a plume, 0 leaves only."""
    rnd = random.Random(seed)
    out = []
    tries = 0
    while len(out) < count and tries < 20000:
        tries += 1
        u, v = rnd.uniform(-1, 1), rnd.uniform(-1, 1)
        if u * u + v * v > 1.0:
            continue
        x, y = a * u, b * v
        if any(math.hypot(x - s[0], y - s[1]) < 0.13 for s in out):
            continue
        e = math.sqrt(u * u + v * v)
        h = rnd.uniform(1.95, 2.22) - 0.5 * e * e
        az = math.atan2(v / b, u / a) + rnd.uniform(-0.5, 0.5)
        lean = math.radians(3 + 14 * e + rnd.uniform(-2, 3))
        r = rnd.random()
        kind = 0 if r < 0.2 else (1 if r < 0.32 else 2)
        out.append((x, y, h, az, lean, kind, rnd.randrange(1 << 30)))
    return out


def _stem_line(s, segments):
    x, y, h, az, lean, _kind, _sd = s
    zb = _bank_z(y) - REED_BURY
    z0 = max(zb, 0.0)
    pts = []
    for i in range(segments + 1):
        t = i / segments
        z = zb + (h - zb) * t
        above = max(0.0, z - z0) / max(h - z0, 1e-6)
        off = math.tan(lean) * (h - z0) * above ** 1.6
        pts.append((x + off * math.cos(az), y + off * math.sin(az), z))
    return zb, z0, pts


def _u(z, z0, h):
    return rd.wind_scale(h - z0) * min(1.0, max(0.0, z - z0) / max(h - z0, 1e-6))


def _reed_stem(bm, s, lod, phase):
    """One stem's faces by slot: {LEAF: [...], PLUME: [...]}, wind set."""
    x, y, h, az, lean, kind, sd = s
    rnd = random.Random(sd)
    lu, lv = rd.wind_layers(bm)
    faces = {LEAF: [], PLUME: [], CARDS: []}
    zb, z0, line = _stem_line(s, 2 if lod == 0 else 1)
    stem_slot = PLUME if kind == 1 else LEAF
    n0 = len(bm.verts)
    rings = [rd._ring(p, rd._unit(tuple(b - a for a, b in zip(line[max(i - 1, 0)], line[min(i + 1, len(line) - 1)]))),
                      0.009 if i == 0 else 0.0075, 3, phase) for i, p in enumerate(line)]
    faces[stem_slot] += rd._tube(bm, rings)
    bm.verts.ensure_lookup_table()
    for vert in bm.verts[n0:]:
        vert[lu], vert[lv] = _u(vert.co.z, z0, h), phase
    top = line[-1]
    axis = rd._unit(tuple(b - a for a, b in zip(line[-2], line[-1])))

    def leaf(at_t, laz, length, width, droop):
        zt = z0 + (h - z0) * at_t
        t = (zt - zb) / (h - zb) * (len(line) - 1)
        bi = max(0, min(len(line) - 2, int(t)))
        f = t - bi
        base = tuple(a + (b - a) * f for a, b in zip(line[bi], line[bi + 1]))
        d = (math.cos(laz), math.sin(laz))
        perp = (-d[1], d[0], 0.0)
        mid = (base[0] + d[0] * 0.5 * length * 0.8, base[1] + d[1] * 0.5 * length * 0.8, base[2] + 0.5 * length * 0.55)
        tip = (base[0] + d[0] * length * 0.9, base[1] + d[1] * length * 0.9, base[2] + length * (0.55 - droop))
        cos_ = (rd._add(base, perp, width * 0.5), rd._add(mid, perp, width * 0.5),
                rd._add(mid, perp, -width * 0.5), rd._add(base, perp, -width * 0.5))
        n1 = len(bm.verts)
        if lod == 0:
            quad = [bm.verts.new(c) for c in cos_]
            tipv = bm.verts.new(tip)
            fs = [bm.faces.new(quad), bm.faces.new((quad[1], tipv, quad[2]))]
            quad2 = [bm.verts.new(c) for c in reversed(cos_)]
            tipv2 = bm.verts.new(tip)
            fs += [bm.faces.new(quad2), bm.faces.new((quad2[2], quad2[1], tipv2))]
        else:
            a1, a2 = bm.verts.new(cos_[0]), bm.verts.new(cos_[3])
            t1 = bm.verts.new(tip)
            b1, b2 = bm.verts.new(cos_[3]), bm.verts.new(cos_[0])
            t2 = bm.verts.new(tip)
            fs = [bm.faces.new((a1, a2, t1)), bm.faces.new((b1, b2, t2))]
        bm.verts.ensure_lookup_table()
        for vert in bm.verts[n1:]:
            vert[lu], vert[lv] = _u(vert.co.z, z0, h), phase
        faces[LEAF] += fs
        faces[CARDS] += fs

    if lod == 0:
        for j in range(2):
            leaf(rnd.uniform(0.35, 0.7), az + rnd.uniform(-2.4, 2.4) + math.pi * j, rnd.uniform(0.5, 0.75),
                 rnd.uniform(0.035, 0.05), rnd.uniform(0.3, 0.55))
    if kind:
        fins = 3 if lod == 0 else 2
        plen = rnd.uniform(0.32, 0.45)
        droop_az = az + rnd.uniform(-0.6, 0.6)
        n2 = len(bm.verts)
        # A nodding panicle: its axis leaves the stem top outward and a little up, its tip hangs.
        q = rd._unit((0.7 * math.cos(droop_az), 0.7 * math.sin(droop_az), 0.45))
        e1 = rd._unit(rd._cross(q, (0.0, 0.0, 1.0)))
        e2 = rd._cross(q, e1)
        w = rnd.uniform(0.12, 0.16)
        base = rd._add(top, q, -0.06 * plen)
        mid = rd._add(rd._add(top, q, 0.45 * plen), (0.0, 0.0, 1.0), 0.03)
        tip = rd._add(rd._add(top, q, plen), (0.0, 0.0, 1.0), -0.3 * plen)
        for k in range(fins):
            fa = math.pi * k / fins + rnd.uniform(-0.2, 0.2)
            p = rd._add(rd._add((0.0, 0.0, 0.0), e1, math.cos(fa)), e2, math.sin(fa))
            cos_ = (base, rd._add(mid, p, 0.5 * w), tip, rd._add(mid, p, -0.5 * w))
            v1 = [bm.verts.new(c) for c in cos_]
            v2 = [bm.verts.new(c) for c in reversed(cos_)]
            fin_faces = [bm.faces.new(v1), bm.faces.new(v2)]
            faces[PLUME] += fin_faces
            faces[CARDS] += fin_faces
        bm.verts.ensure_lookup_table()
        u_top = rd.wind_scale(h - z0)
        for vert in bm.verts[n2:]:
            vert[lu], vert[lv] = u_top, phase
    return faces


def _reed_lod1_fans(bm, plan, phase0):
    """LOD1 leaf fans: one double-sided triangle fan per leaf-only stem, at its middle height."""
    lu, lv = rd.wind_layers(bm)
    faces = []
    for i, s in enumerate(plan):
        x, y, h, az, lean, _kind, sd = s
        zb, z0, line = _stem_line(s, 1)
        phase = rd.wind_phase(sd, 0)
        base = (x, y, z0 - 0.05)
        cos_ = (rd._add(base, (-math.sin(az), math.cos(az), 0.0), 0.12),
                rd._add(base, (-math.sin(az), math.cos(az), 0.0), -0.12), line[-1])
        n1 = len(bm.verts)
        for cs in (cos_, tuple(reversed(cos_))):
            vs = [bm.verts.new(c) for c in cs]
            faces.append(bm.faces.new(vs))
        bm.verts.ensure_lookup_table()
        for vert in bm.verts[n1:]:
            vert[lu], vert[lv] = _u(vert.co.z, z0, h), phase
    return faces


def _reed_cards(bm, a, b, h):
    """LOD2: two crossed tapered cards (along x, `a` half-length, and along y, `b`), double-sided,
    leaf band below 0.68 h and plume band above. Returns {LEAF: faces, PLUME: faces}."""
    lu, lv = rd.wind_layers(bm)
    faces = {LEAF: [], PLUME: [], CARDS: []}
    zb, zm, zt = -0.15, 0.68 * h, h
    for axis, half in (((1.0, 0.0), a), ((0.0, 1.0), b)):
        def p(t, z):
            return (axis[0] * t, axis[1] * t, z)
        rows = ((zb, half * 0.85), (zm, half), (zt, half * 0.7))
        for (z0, w0), (z1, w1), slot in ((rows[0], rows[1], LEAF), (rows[1], rows[2], PLUME)):
            cos_ = (p(-w0, z0), p(w0, z0), p(w1, z1), p(-w1, z1))
            n1 = len(bm.verts)
            for cs in (cos_, tuple(reversed(cos_))):
                f = bm.faces.new([bm.verts.new(c) for c in cs])
                faces[slot].append(f)
                faces[CARDS].append(f)
            bm.verts.ensure_lookup_table()
            for vert in bm.verts[n1:]:
                vert[lu], vert[lv] = _u(vert.co.z, 0.0, h), 0.37 if axis[0] else 0.71
    return faces


CARDS = "cards"
CARD_LAYER = "reed_card"
CARD_UP = 0.65  # share of +z in a leaf, plume or card normal (`project.md`, "Foliage normals")


def card_normals(mesh):
    """Custom normals: each leaf, plume fin and LOD2 card face (bmesh face layer CARD_LAYER) takes
    its face normal blended toward +z, so both sides of a steep blade light like a rounded clump
    under the high kit sun instead of one side going near black; stems keep their face normals.
    Drops the layer's attribute."""
    flags = [0] * len(mesh.polygons)
    mesh.attributes[CARD_LAYER].data.foreach_get("value", flags)
    mesh.attributes.remove(mesh.attributes[CARD_LAYER])
    loops = [None] * len(mesh.loops)
    for p, flag in zip(mesh.polygons, flags):
        n = p.normal
        if flag:
            m = (n.x * (1 - CARD_UP), n.y * (1 - CARD_UP), n.z * (1 - CARD_UP) + CARD_UP)
            k = math.sqrt(sum(c * c for c in m))
            n = tuple(c / k for c in m)
        else:
            n = tuple(n)
        p.use_smooth = True
        for li in p.loop_indices:
            loops[li] = n
    mesh.normals_split_custom_set(loops)


def build_reed_bed(piece, generator, seed=None):
    scene.reset_scene(piece)
    root = scene.empty(piece)
    slots = [LEAF, PLUME]
    mats = {s: materials.pbr_material(s, materials.srgb(COLORS[s][0]), COLORS[s][1]) for s in slots}
    plan = reed_plan(seed if seed is not None else piece)
    by_height = sorted(plan, key=lambda s: -s[2])
    lod1_stems = [s for s in by_height if s[5]][:24]
    lod1_fans = [s for s in by_height if not s[5]][:8]
    lods, slots_per_lod = [], []
    for li in range(3):
        bm = bmesh.new()
        rd.wind_layers(bm)
        card = bm.faces.layers.int.new(CARD_LAYER)
        parts = {LEAF: [], PLUME: [], CARDS: []}
        if li == 0:
            for i, s in enumerate(plan):
                for k, fs in _reed_stem(bm, s, 0, rd.wind_phase(s[6], i)).items():
                    parts[k] += fs
        elif li == 1:
            for i, s in enumerate(lod1_stems):
                for k, fs in _reed_stem(bm, s, 1, rd.wind_phase(s[6], plan.index(s))).items():
                    parts[k] += fs
            fans = _reed_lod1_fans(bm, lod1_fans, 0)
            parts[LEAF] += fans
            parts[CARDS] += fans
        else:
            for k, fs in _reed_cards(bm, 1.85, 0.95, 2.2).items():
                parts[k] += fs
        order = {f: LEAF for f in parts[LEAF]}
        order.update({f: PLUME for f in parts[PLUME]})
        bm.faces.ensure_lookup_table()
        for f in bm.faces:
            f.material_index = slots.index(order[f])
            f.smooth = False
        for f in parts[CARDS]:
            f[card] = 1
        bmesh.ops.triangulate(bm, faces=bm.faces)
        mesh = bpy.data.meshes.new(f"{piece}_lod{li}")
        bm.to_mesh(mesh)
        bm.free()
        card_normals(mesh)
        o = scene.link(bpy.data.objects.new(f"{piece}_lod{li}", mesh))
        for s in slots:
            o.data.materials.append(mats[s])
        uv.planar_cap_uv(o)
        print(f"{piece}_lod{li}: wind U max {rd.wind_uv(o):.3f}")
        slots_per_lod.append(sorted({slots[p.material_index] for p in o.data.polygons}))
        o.parent = root
        lods.append(o)
    reach_m = max(math.hypot(v.co.x, v.co.y) for v in lods[1].data.vertices)
    reach0 = max(math.hypot(v.co.x, v.co.y) for v in lods[0].data.vertices)
    col = _collider_disc(piece, reach_m)
    col.parent = root
    zs = [v.co.z for v in lods[0].data.vertices]
    stats = {"stems": len(plan), "plumes": sum(1 for s in plan if s[5]), "lod1Stems": len(lod1_stems),
             "lod1Fans": len(lod1_fans), "reachM": round(reach_m, 4), "lod0ReachM": round(reach0, 4),
             "topM": round(max(zs), 3), "bottomM": round(min(zs), 3)}
    tris = _finish(piece, generator, root, lods, slots_per_lod, REED_POROSITY, {}, stats, True)
    assert tris[0] <= rd.LOD0_MAX, tris
    print(f"{piece}: reach LOD1 {reach_m:.3f} m (LOD0 {reach0:.3f}), top {max(zs):.2f} m, stems {len(plan)}")
