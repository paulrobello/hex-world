"""House family, village well: the builder behind well_stone.py (the covered well) and well_stone_b.py
(its open variant) (PRD §6 props and Variety; owner-approved refs assets-source/refs/well_stone,
2026-10-20: ref-1 the covered well, ref-2 the open one, ref-3 its place on a cottage lawn).

The well: a round fieldstone wall (grey and buff stones proud of a pale lime mortar face) about 1.5 m
across and 0.9 m high under a ring of pale dressed capstones, on a round flagstone apron; two stout
oak posts outside the wall carry a turned windlass with its hemp rope coiled on the drum and an iron
crank on the -y post, and a stave bucket with iron hoops and bail. The covered well (`roof`) carries a
small steep gabled clay-tile roof on tie beams, braces and wall plates, with dark oak bargeboards and
its ridge 2.4 m up; its bucket rests on the capstones and the rope hangs in a slack loop to the bail.
The open well joins taller posts with a crossbeam and hangs its bucket just above the opening.

Family conventions (_house.py): isotropic surfaces (the stones, mortar, capstones, flags and the roof
clay) sample the world-space `plaster` set tinted per slot; directional detail (stone courses, roof
tile courses, framing) is geometry; timber, wood, rope and iron are colour-only. The roof tint is
house_cottage's clay. The one unique map is the family's 1024² masks atlas, each LOD baked onto its
own islands, then `fill_unbaked` and the zero-unbaked-triangle assert. LODs are rebuilt from the spec
at lower detail, never decimated: LOD1 drops the stones (the wall face takes the grey stone tint),
the braces, hoops, bail and flag joints; LOD2 is the apron, wall, capstone disc, posts and roof or
crossbeam. Every piece is flat shaded by the family pipeline: the stave bucket and the hewn drum are
faceted by design.

Pipeline: `_house_longhouse.build_piece` runs unchanged, driven through its module globals as
`_house_slate` does: `build` is this module's builder, the fit bounds (named COTTAGE_* there) are the
well's ENVELOPE (so a fit failure prints "house_cottage's footprint"), and its `collider` is a shim
that makes the LOD2 hull in place of the box. The well is round, so its box would reach 1.96 m at the
corners, and grass and scatter clear the collider's reach (scatter::FOOTPRINTS): the hull reaches the
apron polygon's corners, 1.385 m, and covers the roof's corners (1.37 m). The walker stops at the wall
and posts instead (walk::DECLARED_PARTS), so it stands on the apron beside the wall.

Frame as _house.py: Blender (x, y, z) exports as runtime (x, z, -y); the pivot is the well's centre at
ground level; the open front (the bucket side) faces +x and the posts stand on the y axis. Every foot
(the mortar face, the flags' sides, the posts) runs down to DROP, under the meadow's deepest relief."""
import math
import random
import types

from mathutils import Vector

import _house_longhouse as lh
from _house import DROP, Z, Mesh, roof_profile
from _house_longhouse import sweep
from lib import collider

assert DROP == -0.35, DROP
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))

# The wall: the mortar face's radius, the shaft, the capstone ring (inner and outer chord corner radii,
# underside and top), the water down the shaft.
R_WALL, R_IN = 0.70, 0.46
CAP_IN, CAP_OUT, CAP_Z0, CAP_Z1 = 0.43, 0.79, 0.80, 0.90
CAPSTONES, CAP_JOINT, CAP_PHASE = 12, 0.012, math.radians(7.0)
SHAFT_Z = -0.9
# Stone courses from the apron top to the capstones: per course, stones of jittered length round the
# wall in running bond, each proud of the mortar face and buried BURY behind it.
COURSES, STONES = 6, 12
JOINT, BURY, PROUD = 0.03, 0.03, (0.022, 0.05)
ROLL = math.radians(4.0)  # each stone rolls about its own outward axis by up to this
BUFF_SHARE = 0.4
# The apron: flag sectors from inside the wall to a jittered outer edge, some split in two across the
# ring, with joints between them; LOD1 and LOD2 draw one polygon at APRON_R, outside every flag corner
# (so the LOD2 hull, and scatter's footprint, cover the whole apron).
APRON_IN, APRON_Z, APRON_OUT, APRON_SPLIT = 0.66, 0.05, (1.30, 1.38), (0.98, 1.06)
FLAGS, FLAG_GAP, SPLIT_SHARE = 13, 0.028, 0.35
APRON_R = 1.385
# Posts: centres on the y axis outside the wall, square section.
POST_Y, POST_W = 0.80, 0.17
POST_OUT = POST_Y + POST_W / 2
# The windlass drum (along y between the posts' inner faces) and the rope coil on it.
DRUM_R, COIL_R, COIL_Y = 0.07, 0.098, (-0.24, 0.14)
DRUM_END = POST_Y - POST_W / 2
# Bucket: bottom and top radius, height, stave wall, hoop heights over its foot.
BUCKET_RB, BUCKET_RT, BUCKET_H, STAVE = 0.11, 0.13, 0.26, 0.015
HOOPS = (0.05, 0.19)
# Covered well's roof: pitch, the wall-plate line's half width, eave overhang beyond it, tile courses,
# course kick, slab thickness, the ridge top, the gable overhang beyond the posts' outer faces.
PITCH, ROOF_HALF, OVERHANG, ROOF_COURSES, KICK, SLAB = math.radians(42.0), 0.66, 0.22, 6, 0.025, 0.08
RIDGE_TOP, GABLE_OVER = 2.40, 0.17
GABLE_Y = POST_OUT + GABLE_OVER
RUN = ROOF_HALF + OVERHANG
ZB = RIDGE_TOP - RUN * math.tan(PITCH) - KICK  # the roof profile's eave line (z = 0 of roof_profile)
PLATE_W, PLATE_IN = 0.10, ROOF_HALF - 0.05
TIE_H, TIE_W, TIE_HALF = 0.12, POST_W + 0.01, 0.74  # wider than the post, whose top face is left out
# Every LOD inside this Blender (x, y) box: the apron polygon's corners, the crank handle's end.
ENVELOPE = ((-1.4, 1.4), (-1.4, 1.4))


def underside(x):
    """The roof slab's underside height at horizontal distance x from the ridge (roof_profile's
    slab: the slope offset by SLAB along its normal)."""
    return ZB + (RUN - abs(x)) * math.tan(PITCH) - SLAB / math.cos(PITCH)


PLATE_TOP = underside(PLATE_IN) + 0.01  # the plate's outer top edge stays inside the slab
TIE_TOP = PLATE_TOP - 0.10
COVERED_POST_TOP = TIE_TOP - TIE_H


def box(mesh, center, ax, ay, az, size, slot, drop=()):
    """_house_longhouse.box's box, built face by face in a fixed order. That box removes its dropped
    faces while iterating a set of BMFaces, whose order follows memory addresses, so with the well's
    hundred boxes the face order, and the GLB's index buffers, changed between runs (make
    kit-determinism, 2026-10-20)."""
    assert ax.cross(ay).dot(az) > 0.99, (ax, ay, az)
    c = Vector(center)
    half = (ax * (size[0] / 2), ay * (size[1] / 2), az * (size[2] / 2))
    for i in range(3):
        a, b = half[(i + 1) % 3], half[(i + 2) % 3]
        for sgn in (1.0, -1.0):
            n = half[i].normalized() * sgn
            if any(n.dot(d) > 0.9 for d in drop):
                continue
            f = c + half[i] * sgn
            mesh.face([f - a - b, f + a - b, f + a + b, f - a + b], slot, n)


def lathe(mesh, origin, axis, rings, seg, slots, phase=0.0, ref=None):
    """A surface of revolution about `axis` through `origin`: `rings` [(t, r)] along the axis, band i
    (rings i to i + 1) in slots[i] (or one slot for every band). Each face points to the profile's
    outer side, (-dr, dt) in (axis, radial) terms: outward on a wall climbing the axis, inward on one
    descending it, toward lower t on a step out. A ring with r = 0 closes the end as a fan."""
    axis = Vector(axis).normalized()
    ref = ref or (Z if abs(axis.dot(Z)) < 0.9 else X)
    u = axis.cross(ref).normalized()
    v = axis.cross(u)
    o = Vector(origin)

    def pt(t, r, k):
        a = phase + 2.0 * math.pi * k / seg
        return o + axis * t + (u * math.cos(a) + v * math.sin(a)) * r

    for i in range(len(rings) - 1):
        (t0, r0), (t1, r1) = rings[i], rings[i + 1]
        slot = slots[i] if isinstance(slots, (list, tuple)) else slots
        for k in range(seg):
            a = phase + 2.0 * math.pi * (k + 0.5) / seg
            expect = axis * (r0 - r1) + (u * math.cos(a) + v * math.sin(a)) * (t1 - t0)
            if r0 == 0.0:
                pts = [pt(t0, 0.0, 0), pt(t1, r1, k), pt(t1, r1, k + 1)]
            elif r1 == 0.0:
                pts = [pt(t0, r0, k), pt(t0, r0, k + 1), pt(t1, 0.0, 0)]
            else:
                pts = [pt(t0, r0, k), pt(t0, r0, k + 1), pt(t1, r1, k + 1), pt(t1, r1, k)]
            mesh.face(pts, slot, expect)


def tube(mesh, pts, radius, sides, slot):
    """A rope or bar along a polyline: `sides` faces round each segment, open ends (they end in the
    drum, the bail or a post), each ring square to the mean of its two segments."""
    pts = [Vector(p) for p in pts]
    ref = Vector((0.31, 0.53, 0.79)).normalized()
    rings = []
    for i, p in enumerate(pts):
        d = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized()
        assert abs(d.dot(ref)) < 0.97, d
        u = d.cross(ref).normalized()
        v = d.cross(u)
        rings.append([p + (u * math.cos(2 * math.pi * k / sides) + v * math.sin(2 * math.pi * k / sides)) * radius
                      for k in range(sides)])
    for i in range(len(pts) - 1):
        mid = (pts[i] + pts[i + 1]) / 2
        for k in range(sides):
            j = (k + 1) % sides
            q = [rings[i][k], rings[i][j], rings[i + 1][j], rings[i + 1][k]]
            mesh.face(q, slot, sum(q, Vector()) / 4 - mid)


def polar(r, a, z):
    return Vector((r * math.cos(a), r * math.sin(a), z))


def radial(a):
    return Vector((math.cos(a), math.sin(a), 0.0))


def tangent(a):
    return Vector((-math.sin(a), math.cos(a), 0.0))


def flag(mesh, slot, r0, r1, a0, a1, z, inner, outer=True):
    """One flagstone sector from radius r0 to r1 (r1 per end as (at a0, at a1)), angles a0 to a1:
    its top, its outer side and both ends down to DROP, and (inner) its inner side."""
    ro0, ro1 = r1 if isinstance(r1, tuple) else (r1, r1)
    i0, i1, o0, o1 = polar(r0, a0, z), polar(r0, a1, z), polar(ro0, a0, z), polar(ro1, a1, z)
    dz = Vector((0.0, 0.0, DROP - z))
    am = (a0 + a1) / 2
    mesh.face([i0, o0, o1, i1], slot, Z)
    if outer:
        mesh.face([o0 + dz, o1 + dz, o1, o0], slot, radial(am))
    if inner:
        mesh.face([i0, i1, i1 + dz, i0 + dz], slot, -radial(am))
    mesh.face([i0, i0 + dz, o0 + dz, o0], slot, -tangent(a0))
    mesh.face([i1, o1, o1 + dz, i1 + dz], slot, tangent(a1))


def apron(mesh, s, lod, rng):
    if lod > 0:
        seg = 12 if lod == 1 else 8
        # One flat polygon over every flag, its outer side down to DROP; its inner edge sits under the wall.
        rings = [(DROP, APRON_R), (APRON_Z, APRON_R), (APRON_Z, APRON_IN)] if lod == 1 else \
            [(DROP, APRON_R), (APRON_Z, APRON_R), (APRON_Z, 0.0)]
        lathe(mesh, (0, 0, 0), Z, rings, seg, s["stone"], phase=math.pi / seg)
        return
    widths = [rng.uniform(0.75, 1.25) for _ in range(FLAGS)]
    total = sum(widths)
    a = rng.uniform(0.0, 2 * math.pi)
    for w in widths:
        da = 2 * math.pi * w / total
        a0, a1 = a + FLAG_GAP / 2 / 1.0, a + da - FLAG_GAP / 2 / 1.0
        z = APRON_Z + rng.uniform(-0.006, 0.006)
        slot = s["stone_buff"] if rng.random() < BUFF_SHARE else s["stone"]
        r_out = (rng.uniform(*APRON_OUT), rng.uniform(*APRON_OUT))
        if rng.random() < SPLIT_SHARE:
            rs = rng.uniform(*APRON_SPLIT)
            flag(mesh, slot, APRON_IN, rs - FLAG_GAP / 2, a0, a1, z, inner=False)
            other = s["stone"] if slot == s["stone_buff"] else s["stone_buff"]
            flag(mesh, other, rs + FLAG_GAP / 2, r_out, a0, a1, APRON_Z + rng.uniform(-0.006, 0.006), inner=True)
        else:
            flag(mesh, slot, APRON_IN, r_out, a0, a1, z, inner=False)
        a += da


def wall(mesh, s, lod, rng):
    if lod > 0:
        seg = 12 if lod == 1 else 8
        lathe(mesh, (0, 0, 0), Z, [(DROP, R_WALL + 0.03), (CAP_Z0, R_WALL + 0.03)], seg, s["stone"])
        if lod == 1:
            lathe(mesh, (0, 0, 0), Z, [(CAP_Z0, R_IN), (SHAFT_Z, R_IN), (SHAFT_Z, 0.0)], seg,
                  [s["stone"], s["iron"]])
        return
    lathe(mesh, (0, 0, 0), Z, [(DROP, R_WALL), (CAP_Z0, R_WALL)], 20, s["mortar"])
    lathe(mesh, (0, 0, 0), Z, [(CAP_Z0, R_IN), (SHAFT_Z, R_IN)], 20, s["stone"])
    lathe(mesh, (0, 0, 0), Z, [(SHAFT_Z, R_IN), (SHAFT_Z, 0.0)], 12, s["iron"])
    ch = (CAP_Z0 - APRON_Z) / COURSES
    start = rng.uniform(0.0, 2 * math.pi)
    for c in range(COURSES):
        widths = [rng.uniform(0.55, 1.45) for _ in range(STONES)]
        total = sum(widths)
        # Running bond: each course starts half a mean stone round from the last.
        a = start + (c % 2) * math.pi / STONES + rng.uniform(-0.08, 0.08)
        z0 = APRON_Z + c * ch
        top = c == COURSES - 1  # under the capstones' overhang: no top face
        for w in widths:
            da = 2 * math.pi * w / total
            am = a + da / 2 + rng.uniform(-0.01, 0.01)
            yaw = rng.uniform(-0.05, 0.05)
            n, t = radial(am + yaw), tangent(am + yaw)
            roll = rng.uniform(-ROLL, ROLL)
            t, up = t * math.cos(roll) + Z * math.sin(roll), Z * math.cos(roll) - t * math.sin(roll)
            proud = rng.uniform(*PROUD)
            depth = proud + BURY
            length = 2 * R_WALL * math.sin(da / 2) - JOINT
            h = ch - JOINT - rng.uniform(0.0, 0.025)
            zc = z0 + ch / 2 + rng.uniform(-0.004, 0.004)
            centre = radial(am) * (R_WALL + proud - depth / 2) + Vector((0.0, 0.0, zc))
            slot = s["stone_buff"] if rng.random() < BUFF_SHARE else s["stone"]
            box(mesh, centre, n, t, up, (depth, length, h), slot, drop=(-n, -up, up) if top else (-n, -up))
            a += da


def capstones(mesh, s, lod):
    if lod > 0:
        seg = 12 if lod == 1 else 8
        if lod == 1:
            rings = [(CAP_Z0, CAP_OUT), (CAP_Z1, CAP_OUT), (CAP_Z1, CAP_IN), (CAP_Z0, CAP_IN)]
        else:
            rings = [(CAP_Z0, CAP_OUT), (CAP_Z1, CAP_OUT), (CAP_Z1, 0.0)]
        lathe(mesh, (0, 0, 0), Z, rings, seg, s["cap"], phase=CAP_PHASE)
        return
    step = 2 * math.pi / CAPSTONES
    for i in range(CAPSTONES):
        a0 = CAP_PHASE + i * step + CAP_JOINT / 2 / CAP_OUT
        a1 = CAP_PHASE + (i + 1) * step - CAP_JOINT / 2 / CAP_OUT
        i0, i1 = polar(CAP_IN, a0, CAP_Z1), polar(CAP_IN, a1, CAP_Z1)
        o0, o1 = polar(CAP_OUT, a0, CAP_Z1), polar(CAP_OUT, a1, CAP_Z1)
        dz = Vector((0.0, 0.0, CAP_Z0 - CAP_Z1))
        am = (a0 + a1) / 2
        mesh.face([i0, o0, o1, i1], s["cap"], Z)
        mesh.face([o0 + dz, o1 + dz, o1, o0], s["cap"], radial(am))
        mesh.face([i0, i1, i1 + dz, i0 + dz], s["cap"], -radial(am))
        mesh.face([i0, i0 + dz, o0 + dz, o0], s["cap"], -tangent(a0))
        mesh.face([i1, o1, o1 + dz, i1 + dz], s["cap"], tangent(a1))


def posts(mesh, s, lod, top, capped):
    for y in (-POST_Y, POST_Y):
        box(mesh, (0.0, y, (DROP + top) / 2), X, Y, Z, (POST_W, POST_W, top - DROP), s["timber"],
            drop=(-Z, Z) if capped else (-Z,))


def strut(mesh, a, b, w, slot, side=X):
    """A square brace from a to b, its ends open (buried in the post and the beam)."""
    a, b = Vector(a), Vector(b)
    d = (b - a).normalized()
    u = d.cross(side).normalized() * (w / 2)
    v = d.cross(u).normalized() * (w / 2)
    c = [u + v, -u + v, -u - v, u - v]
    mid = (a + b) / 2
    for k in range(4):
        q = [a + c[k], a + c[(k + 1) % 4], b + c[(k + 1) % 4], b + c[k]]
        mesh.face(q, slot, sum(q, Vector()) / 4 - mid)


def roof(mesh, s, lod):
    """Tie beams on the post tops, braces, wall plates, the tiled slab and its bargeboards."""
    for y in (-POST_Y, POST_Y):
        box(mesh, (0.0, y, TIE_TOP - TIE_H / 2), X, Y, Z, (2 * TIE_HALF, TIE_W, TIE_H), s["timber"],
            drop=(-Z,) if lod == 2 else ())
        if lod == 0:
            for sx in (-1.0, 1.0):
                strut(mesh, (0.0, y, COVERED_POST_TOP - 0.36), (sx * 0.44, y, TIE_TOP - TIE_H + 0.02), 0.07,
                      s["timber"], side=Y)
    if lod < 2:
        for sx in (-1.0, 1.0):
            box(mesh, (sx * (PLATE_IN + PLATE_W / 2), 0.0, PLATE_TOP - 0.05), X, Y, Z,
                (PLATE_W, 2 * (GABLE_Y - 0.08), 0.10), s["timber"], drop=(Z,))
    pts, ridge = roof_profile(ROOF_HALF, PITCH, OVERHANG, ROOF_COURSES if lod == 0 else 0, KICK, SLAB)
    sweep(mesh, s["roof"], pts, lambda x, z, y: (x, y, ZB + z), -GABLE_Y, GABLE_Y, (True, True))
    if lod == 2:
        return
    # Bargeboards: a dark board on each gable edge, its top 2 cm over the tiles, from the eave to the
    # ridge, standing 1.5 cm proud of the slab's end.
    slope = RUN / math.cos(PITCH)
    hb, tb = 0.14, 0.03
    for sx in (1.0, -1.0):
        u = Vector((-sx * math.cos(PITCH), 0.0, math.sin(PITCH)))  # up the slope from the eave
        nrm = Vector((sx * math.sin(PITCH), 0.0, math.cos(PITCH)))
        eave_top = Vector((sx * RUN, 0.0, ZB + KICK))
        for sy in (1.0, -1.0):
            c = eave_top + u * (slope / 2 + 0.03) + nrm * (0.02 - hb / 2) + Vector((0.0, sy * (GABLE_Y + tb / 2), 0.0))
            ay = Y if sx > 0 else -Y
            box(mesh, c, ay, u, nrm, (tb, slope + 0.06, hb), s["timber"])


def windlass(mesh, s, lod, zw):
    seg = 8 if lod == 0 else (6 if lod == 1 else 4)
    origin = (0.0, 0.0, zw)
    if lod == 2:
        lathe(mesh, origin, Y, [(-DRUM_END, DRUM_R), (DRUM_END, DRUM_R)], seg, s["wood"], ref=Z)
        return
    y0, y1 = COIL_Y
    rings = [(-DRUM_END, DRUM_R), (y0, DRUM_R), (y0, COIL_R), (y1, COIL_R), (y1, DRUM_R), (DRUM_END, DRUM_R)]
    lathe(mesh, origin, Y, rings, seg, [s["wood"], s["rope"], s["rope"], s["rope"], s["wood"]],
          phase=math.pi / seg, ref=Z)


def crank(mesh, s, lod, zw):
    """The iron crank on the -y post: an axle stub through the post's outer face, an arm angled down
    and forward, a wooden handle along -y."""
    y_face = -POST_OUT
    end = Vector((0.07, 0.0, -0.20))  # the handle's axis from the axle's, down and forward
    arm_y = y_face - 0.07
    if lod == 0:
        lathe(mesh, (0.0, y_face + 0.01, zw), -Y, [(0.0, 0.028), (0.075, 0.028)], 6, s["iron"], ref=Z)
    d = end.normalized()
    w = Y.cross(d)
    box(mesh, Vector((0.0, arm_y, zw)) + end / 2, Y, d, w, (0.018, end.length + 0.04, 0.04), s["iron"])
    rings = [(0.0, 0.02), (0.12, 0.02), (0.12, 0.0)] if lod == 0 else [(0.0, 0.02), (0.12, 0.02)]
    lathe(mesh, (end.x, arm_y - 0.009, zw + end.z), -Y, rings, 6 if lod == 0 else 4, s["wood"], ref=Z)


def bucket(mesh, s, lod, foot):
    """A tapered stave bucket standing at `foot` (its bottom centre): staves, rim, inside and floor,
    two iron hoops and the iron bail across it (LOD0)."""
    seg = 8 if lod == 0 else 6
    rb, rt, h, st = BUCKET_RB, BUCKET_RT, BUCKET_H, STAVE
    if lod == 2:
        lathe(mesh, foot, Z, [(0.0, rb), (h, rt), (h, 0.0)], 4, s["wood"])
        return
    rings = [(0.0, rb), (h, rt), (h, rt - st), (0.03, rb - st)] + ([(0.03, 0.0)] if lod == 0 else [])
    lathe(mesh, foot, Z, rings, seg, s["wood"], phase=math.pi / seg)
    if lod > 0:
        return
    for zh in HOOPS:
        r0 = rb + (rt - rb) * zh / h + 0.007
        r1 = rb + (rt - rb) * (zh + 0.028) / h + 0.007
        lathe(mesh, foot, Z, [(zh, r0), (zh + 0.028, r1)], seg, s["iron"], phase=math.pi / seg)


def bail(foot, toward):
    """The bail's polyline over a bucket at `foot`, its ears on the rim across `toward` (horizontal)."""
    f = Vector(foot)
    t = Vector((-toward.y, toward.x, 0.0)).normalized()
    top = BUCKET_H - 0.02
    return [f + t * (BUCKET_RT + 0.008) + Vector((0, 0, top)), f + t * 0.09 + Vector((0, 0, BUCKET_H + 0.09)),
            f - t * 0.09 + Vector((0, 0, BUCKET_H + 0.09)), f - t * (BUCKET_RT + 0.008) + Vector((0, 0, top))]


def build(spec, lod, slots, mats):
    s = spec["slots"]
    mesh = Mesh(slots)
    apron(mesh, s, lod, random.Random(f"{spec['seed']}-apron"))
    wall(mesh, s, lod, random.Random(f"{spec['seed']}-wall"))
    capstones(mesh, s, lod)
    covered = spec["roof"]
    zw = spec["windlass_z"]
    posts(mesh, s, lod, COVERED_POST_TOP if covered else spec["post_top"], capped=covered)
    if covered:
        roof(mesh, s, lod)
    else:
        z0, z1 = spec["crossbeam"]
        box(mesh, (0.0, 0.0, (z0 + z1) / 2), X, Y, Z, (0.12, 2 * DRUM_END, z1 - z0), s["timber"], drop=(Y, -Y))
    windlass(mesh, s, lod, zw)
    if lod < 2:
        crank(mesh, s, lod, zw)
    foot = Vector(spec["bucket"])
    bucket(mesh, s, lod, foot)
    if lod < 2:
        # The rope leaves the coil's front, then (rope) sags to the bail's crown.
        crown = foot + Vector((0.0, 0.0, BUCKET_H + 0.09))
        path = [Vector(p) for p in spec["rope"]] if lod == 0 else []
        pts = [Vector((COIL_R, spec["rope_y"], zw))] + path + [crown]
        tube(mesh, pts, 0.016, 4 if lod == 0 else 3, s["rope"])
        if lod == 0:
            toward = Vector((foot.x, foot.y, 0.0))
            toward = toward if toward.length > 0.2 else Vector((1.0, 0.0, 0.0))
            tube(mesh, bail(foot, toward), 0.007, 4, s["iron"])
    return mesh.obj(f"{spec['piece']}_lod{lod}", mats)


def _hull(src, kind, name):
    """lh.build_piece asks for its LOD2 box; the well takes the LOD2 hull (module docstring)."""
    assert kind == "box", kind
    return collider.collider_from(src, "hull", name)


def build_piece(spec, generator):
    lh.build = build
    lh.COTTAGE_BOUNDS = ENVELOPE
    lh.COTTAGE_LOD0_BOUNDS = ENVELOPE
    lh.collider = types.SimpleNamespace(collider_from=_hull)
    return lh.build_piece(spec, generator)
