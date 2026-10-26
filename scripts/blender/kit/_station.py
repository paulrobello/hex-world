"""Shared code for the P3 side-platform station (owner A17; approved refs assets-source/refs/rail_station
ref-1 and ref-2: "A side platform of about 20 m; the ref-1 shelter is the house style"), two props
(owner Q16, 2026-10-19: split, so the walker stands on the platform and is blocked by the shelter):
- rail_station_platform: a raised brick platform with a pale stone coping and ramps at both ends,
  two lamps, a nameboard and two flower tubs (`rail_station_flowers`, foliage, owner Q17).
- rail_station_shelter: the ref-1 shelter (a half-timbered waiting room under one long tiled roof
  with an open waiting bay, a fretted valance, two brick stacks and a bench).

Frame: the platform shares rail_straight_center's frame and is placed with it (same cell, same
rotation). The track runs along Blender x through the hex centre (edge 3 to edge 0); the platform is
on +y (runtime -z). Blender (x, y, z) exports as runtime (x, z, -y). The shelter is authored in the
same frame and exported moved by -SHELTER_AT (and -H), so its pivot is its footprint centre on the
paving: place it with the same cell and rotation at runtime offset (SHELTER_AT.x, -SHELTER_AT.y) and
y = H. Both share the `rail_station_<slot>` slot names, as the rail and road families share theirs.

Clearances (the train, `_train.py`: carriage half-width 1.48 m over the footboards, engine 1.42 m over
the cab eaves; `_rail.py` section): the brick face stands at |y| = FACE_Y = 1.80, outside the ballast
shoulder (1.75), with its foot following the ballast profile 1 cm above it, so it never touches the
ballast or rails; the coping overhangs to COPE_Y = 1.66, 0.18 m clear of the carriage footboards.
The platform top is H = 1.38 m above the cap, 0.92 m above the rail top (0.46 m): the carriage
footboard (0.60-0.65 m above the rail top) lies 0.27 m below the platform edge and the carriage
floor (its body bottom, 1.20 m above the rail top) one 0.28 m step above it. Level platform 20 m (|x| <= 10), ramps to
|x| = 13.3 (0.56 m inside the border), so the whole piece stays in the hex centre region.

Masks: per piece, one 1024² atlas over its three LODs, Cycles AO + an EMIT hooks bake, the
unbaked-texel fill and the centroid check from _house.py, as the bakery. Each piece's AO is baked
with the other piece standing in place (`bake.bake_to(occluders=...)`), so the paving keeps the
shelter's contact shade and the shelter's walls the paving's, as the combined piece had them. The
platform's islands are scaled by PLATFORM_UV (its paving and retaining walls are large, flat and
evenly lit), as before the split."""
import math
import os
import pathlib
import statistics
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import _lod_copy  # noqa: E402
import _rail  # noqa: E402
from _house import (DROP, PLASTER_LIN, Z, Mesh, Wall, centroid_samples, fill_unbaked,  # noqa: E402
                    frame_matrix, roof_profile)
from lib import bake, budgets, collider, export, geom, materials, scene  # noqa: E402

P = "rail_station"
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
UV_MARGIN = 0.006
BAKE_MARGIN = 2
LOD_UV_SCALE = (1.0, 0.55, 0.35)
PLATFORM_UV = 0.4

TILEABLE = ("walls", "brick", "roof", "paving")
COLOURS = {
    "walls": ((1.0, 0.92, 0.72), 0.9),
    "brick": ((0.46, 0.17, 0.11), 0.9),
    "roof": ((0.30, 0.17, 0.11), 0.85),
    "paving": ((0.62, 0.58, 0.52), 0.9),
    "coping": ((0.62, 0.58, 0.48), 0.8),
    "timber": ((0.085, 0.055, 0.035), 0.8),
    "glass": ((0.2, 0.16, 0.1), 0.2),
    "iron": ((0.03, 0.03, 0.032), 0.55),
    "board": ((0.75, 0.73, 0.66), 0.6),
    "flowers": ((0.55, 0.06, 0.04), 0.7),
}
HOOKS = {"walls": (1.0, 0.5, 0.0), "brick": (1.0, 0.45, 0.0), "roof": (1.0, 0.35, 0.0),
         "paving": (1.0, 0.4, 0.0), "coping": (1.0, 0.35, 0.0), "timber": (1.0, 0.6, 0.0),
         "glass": (0.0, 0.05, 1.0), "iron": (1.0, 0.1, 0.0), "board": (1.0, 0.3, 0.0),
         "flowers": (1.0, 0.6, 0.0)}
S = {n: f"{P}_{n}" for n in COLOURS}
WIN = {"glass": S["glass"], "frame": S["timber"], "sill": S["coping"]}

# Platform.
H = _rail.RAIL_TOP + 0.92
FACE_Y, COPE_Y, COPE_BACK, COPE_T = 1.80, 1.66, 2.10, 0.10
BACK_Y = 6.8
X_LEVEL, X_END, T_TOE = 10.0, 13.3, 0.25
# Shelter: room on the left (x < RX), open bay to the right, front wall at SY0 facing the track.
SX0, SX1, RX = -4.4, 3.8, -1.2
SY0, SY1, WT = 3.6, 6.4, 0.15
SYC, HALF_D = (SY0 + SY1) / 2, (SY1 - SY0) / 2
DADO, EAVE = H + 0.9, H + 2.7
PITCH, OVER, GOVER, COURSES, KICK, THICK = math.radians(42.0), 0.45, 0.3, 7, 0.04, 0.14
GABLE_TOP = EAVE + HALF_D * math.tan(PITCH) - 0.05
ZB = EAVE - OVER * math.tan(PITCH)
RIDGE = (HALF_D + OVER) * math.tan(PITCH)
EAVE_Y = SYC - HALF_D - OVER
CHIMNEYS = (-3.7, -1.9)
POSTS = (1.3, SX1 - 0.08)
LAMPS = ((-7.5, 2.55), (8.4, 2.55))
BOARD_X = 6.3
TUBS = ((SX0 - 0.45, 3.3), (SX1 + 0.45, 3.3))
# The shelter's pivot in the platform's Blender frame: its footprint centre, on the paving (z = H).
SHELTER_AT = ((SX0 + SX1) / 2, SYC)


def ballast_z(y):
    """The ballast surface height at lateral offset y >= 0 (`_rail.BALLAST_PROFILE`)."""
    prof = [(t, z) for t, z in _rail.BALLAST_PROFILE if t >= 0]
    for (t0, z0), (t1, z1) in zip(prof, prof[1:]):
        if t0 <= y <= t1:
            return z0 + (z1 - z0) * (y - t0) / (t1 - t0)
    return prof[-1][1] if y > prof[-1][0] else _rail.BALLAST_TOP


FOOT = [(FACE_Y, ballast_z(FACE_Y) + 0.01), (_rail.TOE_T, ballast_z(_rail.TOE_T) + 0.01),
        (_rail.SINK_T, ballast_z(_rail.SINK_T) + 0.01), (BACK_Y, DROP)]
XS = (-X_END, -X_LEVEL, X_LEVEL, X_END)
TS = (T_TOE, H, H, T_TOE)


def box(mesh, center, ax, ay, az, size, slot, drop=()):
    res = bmesh.ops.create_cube(mesh.bm, size=1.0, matrix=frame_matrix(Vector(center), ax, ay, az, size))
    for f in list({f for v in res["verts"] for f in v.link_faces}):
        f.normal_update()
        if any(f.normal.dot(d) > 0.9 for d in drop):
            mesh.bm.faces.remove(f)
        else:
            f.material_index = mesh.idx[slot]


def beam(mesh, wall, u0, z0, u1, z1, width, proud, slot):
    """A framing member on a wall: its front face and two long sides (ends open)."""
    a, b = wall.p(u0, z0), wall.p(u1, z1)
    t = (b - a).normalized()
    h = wall.n.cross(t) * (width / 2)
    f = wall.n * proud
    mesh.face([a - h + f, b - h + f, b + h + f, a + h + f], slot, wall.n)
    mesh.face([a + h, b + h, b + h + f, a + h + f], slot, h)
    mesh.face([a - h, a - h + f, b - h + f, b - h], slot, -h)


def cyl(mesh, c, r, z0, z1, n, slot, top=False, r1=None):
    res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=n, radius1=r, radius2=r if r1 is None else r1,
                                depth=z1 - z0, matrix=Matrix.Translation((c[0], c[1], (z0 + z1) / 2)))
    for f in list({f for v in res["verts"] for f in v.link_faces}):
        f.normal_update()
        if f.normal.z < -0.9 or (f.normal.z > 0.9 and not top):
            mesh.bm.faces.remove(f)
        else:
            f.material_index = mesh.idx[slot]


def platform(mesh, lod):
    """The raised platform: paving and coping on top, brick faces round it, ramps at both ends."""
    for i in range(3):
        x0, x1, t0, t1 = XS[i], XS[i + 1], TS[i], TS[i + 1]
        level = i == 1
        front = COPE_Y if level else FACE_Y
        mesh.face([(x0, COPE_BACK, t0), (x1, COPE_BACK, t1), (x1, BACK_Y, t1), (x0, BACK_Y, t0)], S["paving"], Z)
        mesh.face([(x0, front, t0), (x1, front, t1), (x1, COPE_BACK, t1), (x0, COPE_BACK, t0)], S["coping"], Z)
        top = (t0 - COPE_T, t1 - COPE_T) if level else (t0, t1)
        zf = FOOT[0][1]
        if level and lod == 2:
            # LOD2 (400 m): one face from the foot to the coping edge, no nosing (15% LOD2 ceiling).
            mesh.face([(x0, FACE_Y, zf), (x1, FACE_Y, zf), (x1, COPE_Y, H), (x0, COPE_Y, H)], S["brick"], -Y)
        else:
            mesh.face([(x0, FACE_Y, zf), (x1, FACE_Y, zf), (x1, FACE_Y, top[1]), (x0, FACE_Y, top[0])], S["brick"], -Y)
        mesh.face([(x0, BACK_Y, DROP), (x0, BACK_Y, t0), (x1, BACK_Y, t1), (x1, BACK_Y, DROP)], S["brick"], Y)
        if level and lod < 2:
            # The coping's nosing over the edge; no underside of the platform is built (never seen).
            hi = H - COPE_T
            mesh.face([(x0, COPE_Y, hi), (x1, COPE_Y, hi), (x1, COPE_Y, H), (x0, COPE_Y, H)], S["coping"], -Y)
            mesh.face([(x0, COPE_Y, hi), (x0, FACE_Y, hi), (x1, FACE_Y, hi), (x1, COPE_Y, hi)], S["coping"], -Z)
            for x, s in ((x0, -1), (x1, 1)):
                mesh.face([(x, COPE_Y, hi), (x, FACE_Y, hi), (x, FACE_Y, H), (x, COPE_Y, H)], S["coping"], X * s)
    for x, s in ((-X_END, -1), (X_END, 1)):
        for (ya, za), (yb, zb) in zip(FOOT, FOOT[1:]):
            mesh.face([(x, ya, za), (x, ya, T_TOE), (x, yb, T_TOE), (x, yb, zb)], S["brick"], X * s)


def banded(mesh, pts_lo, pts_hi, n, lod):
    """A wall face: a brick dado up to DADO under the plastered panel (LOD2: plaster only)."""
    if lod == 2:
        mesh.face(pts_lo[:2] + pts_hi, S["walls"], n)
        return
    a, b = pts_lo
    am, bm = Vector((a[0], a[1], DADO)), Vector((b[0], b[1], DADO))
    mesh.face([a, b, bm, am], S["brick"], n)
    mesh.face([am, bm] + pts_hi, S["walls"], n)


def walls(mesh, lod):
    def v(x, y, z):
        return Vector((x, y, z))

    # Room front (facing the track), back wall outer, the two gable ends.
    banded(mesh, [v(SX0, SY0, H), v(RX, SY0, H)], [v(RX, SY0, EAVE), v(SX0, SY0, EAVE)], -Y, lod)
    banded(mesh, [v(SX1, SY1, H), v(SX0, SY1, H)], [v(SX0, SY1, EAVE), v(SX1, SY1, EAVE)], Y, lod)
    banded(mesh, [v(SX0, SY1, H), v(SX0, SY0, H)], [v(SX0, SY0, EAVE), v(SX0, SYC, GABLE_TOP), v(SX0, SY1, EAVE)],
           -X, lod)
    banded(mesh, [v(SX1, SY0, H), v(SX1, SY1, H)], [v(SX1, SY1, EAVE), v(SX1, SYC, GABLE_TOP), v(SX1, SY0, EAVE)],
           X, lod)
    # The open bay's inner faces: back wall, right end, the room's partition, the end wall's return.
    yi = SY1 - WT
    banded(mesh, [v(RX, yi, H), v(SX1 - WT, yi, H)], [v(SX1 - WT, yi, EAVE), v(RX, yi, EAVE)], -Y, lod)
    xi = SX1 - WT
    banded(mesh, [v(xi, yi, H), v(xi, SY0, H)], [v(xi, SY0, EAVE), v(xi, SYC, GABLE_TOP), v(xi, yi, EAVE)], -X, lod)
    banded(mesh, [v(RX, SY0, H), v(RX, yi, H)], [v(RX, yi, EAVE), v(RX, SYC, GABLE_TOP), v(RX, SY0, EAVE)], X, lod)
    banded(mesh, [v(xi, SY0, H), v(SX1, SY0, H)], [v(SX1, SY0, EAVE), v(xi, SY0, EAVE)], -Y, lod)


def roof(mesh, lod):
    """The tiled roof swept along x, its ridge on the shelter's centre line (as _house.build)."""
    pts, _ = roof_profile(HALF_D, PITCH, OVER, COURSES if lod == 0 else 0, KICK, THICK)
    s0, s1 = SX0 - GOVER, SX1 + GOVER
    rings = [[mesh.bm.verts.new((s, SYC - px, ZB + pz)) for px, pz in pts] for s in (s0, s1)]
    faces = []
    k = len(pts)
    for i in range(k):
        j = (i + 1) % k
        faces.append(mesh.bm.faces.new((rings[0][i], rings[1][i], rings[1][j], rings[0][j])))
    faces += [mesh.bm.faces.new(rings[0]), mesh.bm.faces.new(rings[1])]
    bmesh.ops.recalc_face_normals(mesh.bm, faces=faces)
    for f in faces:
        f.material_index = mesh.idx[S["roof"]]
    top = max(faces, key=lambda f: f.calc_center_median().z)  # a slope face at the ridge (course butts face down)
    assert top.normal.z > 0, "roof winds inward"


def chimney(mesh, x, lod):
    y = SYC + 0.35
    top = ZB + RIDGE + 0.7
    box(mesh, (x, y, (EAVE + top) / 2), X, Y, Z, (0.7, 0.55, top - EAVE), S["brick"], drop=(-Z,))
    if lod < 2:
        box(mesh, (x, y, top + 0.06), X, Y, Z, (0.86, 0.7, 0.12), S["brick"], drop=(-Z,))
    if lod == 0:
        for px in (-0.16, 0.16):
            cyl(mesh, (x + px, y), 0.1, top + 0.12, top + 0.48, 6, S["roof"], top=True, r1=0.085)


def front_detail(mesh, lod):
    """Posts, brackets, the eave plate over the open bay, the fretted valance and the framing."""
    front = Wall((0, -1, 0), (0, SY0, 0))
    if lod < 2:
        for x in POSTS:
            box(mesh, (x, SY0 + 0.08, (H + EAVE) / 2), X, Y, Z, (0.16, 0.16, EAVE - H), S["timber"], drop=(-Z, Z))
        box(mesh, (0.5 * (RX + SX1), SY0 + 0.08, EAVE - 0.09), X, Y, Z, (SX1 - RX, 0.18, 0.18), S["timber"],
            drop=(Z,))
    if lod == 0:
        for x in POSTS:
            for s in (-1, 1):
                if x + s * 0.7 > SX1:
                    continue
                a, b = Vector((x, SY0 + 0.08, EAVE - 0.7)), Vector((x + s * 0.6, SY0 + 0.08, EAVE - 0.16))
                t = (b - a).normalized()
                box(mesh, (a + b) / 2, t, Y, t.cross(Y), ((b - a).length, 0.1, 0.1), S["timber"], drop=(t, -t))
    # Valance: a timber board under the front eave with a sawtooth lower edge, both faces.
    x0, x1 = SX0 - GOVER, SX1 + GOVER
    zt, zm = ZB - 0.05, ZB - 0.3
    for side, yv in ((-1, EAVE_Y + 0.1), (1, EAVE_Y + 0.11)):
        n = Y * side
        if lod == 2:
            continue
        mesh.face([(x0, yv, zm), (x1, yv, zm), (x1, yv, zt), (x0, yv, zt)], S["timber"], n)
        if lod == 0:
            teeth = 26
            w = (x1 - x0) / teeth
            for i in range(teeth):
                a = x0 + i * w
                mesh.face([(a, yv, zm), (a + w / 2, yv, zm - 0.12), (a + w, yv, zm)], S["timber"], n)
    if lod == 2:
        return
    # Half-timbering on the room front and the left gable: corner posts, rails, studs, a brace.
    plate = EAVE - 0.08
    for u0, z0, u1, z1, keep in ((SX0 + 0.08, DADO, SX0 + 0.08, plate, True), (RX - 0.08, DADO, RX - 0.08, plate, True),
                                 (SX0, DADO + 0.04, RX, DADO + 0.04, True), (SX0, plate, RX, plate, True),
                                 (-3.6, DADO, -3.6, plate, False), (-1.85, DADO, -1.85, plate, False)):
        if keep or lod == 0:
            beam(mesh, front, u0, z0, u1, z1, 0.14, 0.05, S["timber"])
    left = Wall((-1, 0, 0), (SX0, 0, 0))
    for u0, z0, u1, z1, keep in ((-SY0 - 0.08, DADO, -SY0 - 0.08, plate, True), (-SY1 + 0.08, DADO, -SY1 + 0.08, plate, True),
                                 (-SY1, DADO + 0.04, -SY0, DADO + 0.04, True), (-SY1, plate, -SY0, plate, True),
                                 (-SYC, EAVE, -SYC, GABLE_TOP - 0.25, False),
                                 (-SY0 - 0.15, DADO + 0.1, -SYC + 0.65, plate - 0.1, False),
                                 (-SY1 + 0.15, DADO + 0.1, -SYC - 0.65, plate - 0.1, False)):
        if keep or lod == 0:
            beam(mesh, left, u0, z0, u1, z1, 0.14, 0.05, S["timber"])
    back = Wall((0, 1, 0), (0, SY1, 0))
    for u, keep in ((-SX0 - 0.08, True), (-SX1 + 0.08, True), (-RX, False), (-1.3, False)):
        if keep or lod == 0:
            beam(mesh, back, u, DADO, u, plate, 0.14, 0.05, S["timber"])
    if lod == 0:
        beam(mesh, back, -SX1, DADO + 0.04, -SX0, DADO + 0.04, 0.14, 0.05, S["timber"])
        beam(mesh, back, -SX1, plate, -SX0, plate, 0.14, 0.05, S["timber"])
    # Windows: the room front and the left gable.
    front.window(mesh, -2.8, H + 1.1, 1.2, 1.0, WIN, min(lod, 1), bars=True)
    left.window(mesh, -SYC, H + 1.1, 0.9, 1.0, WIN, min(lod, 1), bars=True)


def furniture(mesh, lod, shelter):
    """The shelter's bench in the bay; the platform's lamps, nameboard and flower tubs."""
    if shelter:
        if lod == 0:
            bx, by = 1.3, SY1 - WT - 0.35
            box(mesh, (bx, by, H + 0.45), X, Y, Z, (1.8, 0.4, 0.05), S["timber"])
            box(mesh, (bx, by + 0.2, H + 0.75), X, Y, Z, (1.8, 0.04, 0.35), S["timber"], drop=(Y,))
            for dx in (-0.8, 0.8):
                box(mesh, (bx + dx, by, H + 0.21), X, Y, Z, (0.06, 0.4, 0.42), S["iron"], drop=(-Z, Z))
        return
    if lod == 0:
        for x, y in TUBS:
            cyl(mesh, (x, y), 0.32, H, H + 0.38, 8, S["timber"])
            cyl(mesh, (x, y), 0.34, H + 0.38, H + 0.62, 8, S["flowers"], top=True, r1=0.12)
    for x, y in LAMPS:
        if lod == 2:
            continue
        if lod == 0:
            cyl(mesh, (x, y), 0.06, H, H + 2.75, 6, S["iron"])
            box(mesh, (x, y, H + 0.15), X, Y, Z, (0.2, 0.2, 0.3), S["iron"], drop=(-Z,))
            cyl(mesh, (x, y), 0.26, H + 3.15, H + 3.35, 4, S["iron"], top=True, r1=0.02)
        else:
            box(mesh, (x, y, H + 1.4), X, Y, Z, (0.1, 0.1, 2.8), S["iron"], drop=(-Z,))
        box(mesh, (x, y, H + 2.95), X, Y, Z, (0.3, 0.3, 0.4), S["glass"], drop=(-Z,) if lod else ())
    if lod < 2:
        y = 3.0
        for dx in (-1.0, 1.0):
            box(mesh, (BOARD_X + dx, y, H + 1.1), X, Y, Z, (0.08, 0.08, 2.2), S["iron"], drop=(-Z,))
        box(mesh, (BOARD_X, y, H + 1.95), X, Y, Z, (2.3, 0.06, 0.55), S["iron"])
        if lod == 0:
            for side in (-1, 1):
                yy = y + side * 0.031
                mesh.face([(BOARD_X - 1.07, yy, H + 1.73), (BOARD_X + 1.07, yy, H + 1.73),
                           (BOARD_X + 1.07, yy, H + 2.17), (BOARD_X - 1.07, yy, H + 2.17)], S["board"], Y * side)


# Why these slots leave LOD2 (bake report `lod2Drops`, kit_gate `lod_slot_errors`): one pixel at 400 m is about
# 0.31 m (45 degree vertical field of view, 1080 rows).
_NO_ROOM = "no room: LOD2 is 36 tris against its 36.6-tri ceiling (15% of 244), so not one more triangle fits"
LOD2_DROPS = {
    "platform": {"timber": _NO_ROOM + " (the 0.68 m flower tubs)", "flowers": _NO_ROOM + " (the tubs' flowers)",
                 "board": _NO_ROOM + " (the 2.3 x 0.55 m nameboard)"},
    "shelter": {"coping": "under a pixel at 400 m: the window sills are 0.08 m tall (0.26 px at 0.31 m/px), and "
                          "LOD2's own windows (1.2 and 0.9 m) are not LOD0's (1.5 and 1.2 m)",
                "iron": "under a pixel at 400 m: the bay bench's legs are 0.06 m (0.2 px) under the roof, and LOD2 "
                        "has no bench"},
}


def late_platform(lods):
    """LOD1 keeps the flower tubs and the nameboard's lettered faces (owner A2 Q4, 2026-10-21: every
    piece has a middle LOD) inside the 50% ceiling (LOD1 was 116 of 122 tris): the nameboard's two
    posts become 3-sided prisms (10 -> 6 tris each), the board's two broad faces take the board slot,
    and each tub is a 3-sided timber drum under a flower triangle (7 tris; circumradii 1.178x LOD0's
    octagon for the same mean width). Edited after the masks bake: every new face's UVs sit on one
    baked texel (a LOD0 tub or flower face, or the replaced LOD1 post), so the atlas and LOD0 stay as
    built."""
    def uv_at(obj, slot, near, reach=0.6):
        """UV centroid of the first `slot` face of obj within `reach` (plan) of `near`."""
        me = obj.data
        names = [m.name for m in me.materials]
        layer = me.uv_layers.active.data
        for p in me.polygons:
            if names[p.material_index] == slot and (p.center.xy - Vector(near)).length < reach:
                return sum((layer[li].uv for li in p.loop_indices), Vector((0.0, 0.0))) / p.loop_total
        raise AssertionError(f"no {slot} face near {near} in {obj.name}")

    o = lods[1]
    idx = {m.name: i for i, m in enumerate(o.data.materials)}
    tub_uv = {t: (uv_at(lods[0], S["timber"], t), uv_at(lods[0], S["flowers"], t)) for t in TUBS}
    post_uv = {dx: uv_at(o, S["iron"], (BOARD_X + dx, 3.0), 0.06) for dx in (-1.0, 1.0)}
    bm = bmesh.new()
    bm.from_mesh(o.data)
    bm.normal_update()
    uvl = bm.loops.layers.uv.active
    gone = []
    for f in bm.faces:
        if f.material_index != idx[S["iron"]]:
            continue
        c = f.calc_center_median()
        if any(abs(c.x - BOARD_X - dx) < 0.05 for dx in (-1.0, 1.0)) and abs(c.y - 3.0) < 0.05 and c.z < H + 2.21:
            gone.append(f)
        elif abs(abs(c.y - 3.0) - 0.03) < 1e-3 and abs(f.normal.y) > 0.9 and abs(c.x - BOARD_X) < 1.15:
            f.material_index = idx[S["board"]]
    assert len(gone) == 2 * 10, len(gone)  # two posts, five faces each, triangulated
    bmesh.ops.delete(bm, geom=gone, context="FACES")

    def ring(c, r, z):
        return [bm.verts.new((c[0] + r * math.cos(a), c[1] + r * math.sin(a), z))
                for a in (math.pi / 2 + 2 * math.pi * k / 3 for k in range(3))]

    def drum(c, r, z0, z1, slot, uv, cap=None):
        lo, hi = ring(c, r, z0), ring(c, r, z1)
        faces = [bm.faces.new([lo[k], lo[(k + 1) % 3], hi[(k + 1) % 3], hi[k]]) for k in range(3)]
        for f in faces:
            f.material_index = idx[slot]
        if cap:
            top = bm.faces.new(hi)
            top.material_index = idx[cap[0]]
            faces.append(top)
        for f in faces:
            f.smooth = False
            for loop in f.loops:
                loop[uvl].uv = cap[1] if cap and f.material_index == idx[cap[0]] else uv
        return faces

    new = []
    for dx in (-1.0, 1.0):
        new += drum((BOARD_X + dx, 3.0), 0.0616, H, H + 2.2, S["iron"], post_uv[dx])
    for t in TUBS:
        new += drum(t, 0.32 * 1.178, H, H + 0.45, S["timber"], tub_uv[t][0], cap=(S["flowers"], tub_uv[t][1]))
    bmesh.ops.triangulate(bm, faces=new)
    bm.to_mesh(o.data)
    bm.free()


def build_platform(lod, mesh):
    platform(mesh, lod)
    furniture(mesh, lod, shelter=False)


def build_shelter(lod, mesh):
    walls(mesh, lod)
    roof(mesh, lod)
    for x in CHIMNEYS:
        chimney(mesh, x, lod)
    front_detail(mesh, lod)
    furniture(mesh, lod, shelter=True)
    if lod == 2:
        for wall, u, w in ((Wall((0, -1, 0), (0, SY0, 0)), -2.8, 1.2), (Wall((-1, 0, 0), (SX0, 0, 0)), -SYC, 0.9)):
            wall.window(mesh, u, H + 1.1, w, 1.0, WIN, 2, bars=False)


PIECES = {"platform": (f"{P}_platform", build_platform), "shelter": (f"{P}_shelter", build_shelter)}


def unwrap(lods, boost_of):
    """One atlas over every LOD of one piece; each LOD's islands scaled by LOD_UV_SCALE and by
    `boost_of(polygon)` before packing."""
    scene.select_only(lods)
    for o in lods:
        o.data.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.152, island_margin=UV_MARGIN, area_weight=0.2)
    bpy.ops.object.mode_set(mode="OBJECT")
    for o, k in zip(lods, LOD_UV_SCALE):
        uvl = o.data.uv_layers["UVMap"].data
        for p in o.data.polygons:
            boost = boost_of(p)
            for li in p.loop_indices:
                uvl[li].uv = uvl[li].uv * (k * boost)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=True, margin_method="FRACTION", margin=UV_MARGIN)
    bpy.ops.object.mode_set(mode="OBJECT")


def density_by_slot(obj, size, slots):
    m = obj.data
    uvl = m.uv_layers["UVMap"].data
    vals = {}
    for p in m.polygons:
        if p.area < 1e-6:
            continue
        uvs = [uvl[li].uv for li in p.loop_indices]
        a = sum(abs((uvs[i] - uvs[0]).cross(uvs[i + 1] - uvs[0])) / 2 for i in range(1, len(uvs) - 1))
        vals.setdefault(slots[p.material_index][len(P) + 1:], []).append(math.sqrt(a * size * size / p.area))
    return {k: round(statistics.median(v), 2) for k, v in sorted(vals.items())}


def clearance_errors(lods):
    """Every vertex keeps out of the train's and the track's space: nothing at |y| < COPE_Y, and
    nothing below the ballast surface (+1 cm) at |y| < SINK_T."""
    errs = []
    for o in lods:
        for v in o.data.vertices:
            x, y, z = v.co
            if y < COPE_Y - 1e-4:
                errs.append(f"{o.name}: vertex {tuple(v.co)} inside the train envelope")
            elif y < _rail.SINK_T and z < ballast_z(y) + 0.005:
                errs.append(f"{o.name}: vertex {tuple(v.co)} in the ballast")
    return errs[:8]


def slot_names(which):
    """The slots a piece's LODs actually use, in the helper's slot order (material order in the GLB)."""
    used = {"platform": ("brick", "paving", "coping", "timber", "glass", "iron", "board", "flowers"),
            "shelter": ("walls", "brick", "roof", "coping", "timber", "glass", "iron")}[which]
    return [n for n in S if n in used]


def build_piece(which, generator):
    """Build `which` ("platform" or "shelter") and its LODs, with the other piece built alongside as
    an AO occluder only (the shelter stands on the paving, so each shades the other at runtime).
    The platform's pivot is the hex centre, as rail_straight_center; the shelter's geometry is moved
    by -SHELTER_AT so its pivot sits on the paving at the footprint centre."""
    piece, _ = PIECES[which]
    other = "shelter" if which == "platform" else "platform"
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(piece)
    root = scene.empty(piece)
    names = slot_names(which)
    slots = [S[n] for n in names]
    mats = []
    for name in names:
        col, rough = COLOURS[name]
        k = PLASTER_LIN if name in TILEABLE else 1.0
        mats.append(materials.pbr_material(S[name], (col[0] * k, col[1] * k, col[2] * k, 1.0), rough))
    other_mat = materials.pbr_material(f"{piece}_occluder", (0.5, 0.5, 0.5, 1.0), 0.9)

    def make(kind, lod, name, slot_list, mat_list):
        mesh = Mesh(slot_list)
        PIECES[kind][1](lod, mesh)
        return mesh.obj(name, mat_list)

    lods = [make(which, i, f"{piece}_lod{i}", slots, mats) for i in range(3)]
    other_slots = [S[n] for n in slot_names(other)]
    occ = [make(other, i, f"{piece}_occ{i}", other_slots, [other_mat] * len(other_slots)) for i in range(3)]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{piece}: tris per LOD {tris} (LOD1 {tris[1] / tris[0]:.3f}, LOD2 {tris[2] / tris[0]:.3f})")
    errs = clearance_errors(lods)
    assert not errs, "\n".join(errs)
    budgets.check(tris, "prop", piece)

    boost = (lambda p: PLATFORM_UV) if which == "platform" else (lambda p: 1.0)
    unwrap(lods, boost)
    signature = bake.receiver_signature(lods)
    density = {f"lod{i}": density_by_slot(o, MASK_SIZE, slots) for i, o in enumerate(lods)}
    print(f"{piece}: masks px/m per slot {density}")
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(piece)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{piece}.ao", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=BAKE_MARGIN, clear=i == 0, occluders=(occ[i],))
        ao_png = bake.save_png(ao, str(out / f"{piece}.ao.tmp.png"))
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for name, m in zip(names, mats):
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*HOOKS[name], 1.0)
        hooks = bake.new_image(f"{piece}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{piece}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        _, aop = bake.load_png(ao_png)
        os.remove(ao_png)
        os.remove(hooks_png)
        px = np.stack([hk[:, 0], hk[:, 1], hk[:, 2], aop[:, 0]], axis=1)
        px, filled = fill_unbaked(px, lods, MASK_SIZE, [HOOKS[n] for n in names])
        masks_png = bake.write_png(MASK_SIZE, px, str(out / f"{piece}.masks.png"))
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01
        av = px[covered, 3]
        samples = centroid_samples(lods, px, MASK_SIZE, slots, P)  # P: the slot-name prefix
        stats = {"aoMean": float(av.mean()), "aoP05": float(np.percentile(av, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled, "centroids": samples}
        print(f"{piece}: masks {stats}")
        bad = {k: v["unbakedTris"] for k, v in samples.items() if v["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"

    for o in occ:
        bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    if which == "shelter":
        # LOD1 keeps the window sills and the bay bench (owner A2 Q4, 2026-10-21: every piece has a
        # middle LOD): LOD0's coping and iron faces and the bench's timber (inside the legs' plan box
        # grown by 0.15 m, below 0.95 m over the paving), copied after the bake with their baked UVs
        # (_lod_copy). The box keeps out the wall's long rail 0.55 m behind the bench.
        names = [m.name for m in lods[0].data.materials]
        legs = [p.center for p in lods[0].data.polygons if names[p.material_index] == S["iron"]]
        lo = [min(c[i] for c in legs) - 0.15 for i in (0, 1)]
        hi = [max(c[i] for c in legs) + 0.15 for i in (0, 1)]

        def bench_part(c, m):
            inside = all(lo[i] <= c[i] <= hi[i] for i in (0, 1)) and c.z < H + 0.95
            return m == S["iron"] or (m == S["timber"] and inside)
        lods[1], sills = _lod_copy.copy_faces(lods[0], lods[1], lambda _c, m: m == S["coping"])
        lods[1], bench = _lod_copy.copy_faces(lods[0], lods[1], bench_part)
        tris[1] += sills + bench
        print(f"{piece}: LOD1 + sills {sills} + bench {bench} tris -> tris per LOD {tris}")
        budgets.check(tris, "prop", piece)
    else:
        late_platform(lods)
        tris = [geom.tri_count(o) for o in lods]
        print(f"{piece}: LOD1 with tubs and the nameboard face -> tris per LOD {tris}")
        budgets.check(tris, "prop", piece)
    shift = Matrix.Translation((-SHELTER_AT[0], -SHELTER_AT[1], -H)) if which == "shelter" else Matrix.Identity(4)
    for o in lods:
        o.data.transform(shift)
        o.data.shade_flat()
        o.parent = root
    col = collider.collider_from(lods[2], "box", f"{piece}_collider")
    col.parent = root
    export.export_glb(root, str(out / f"{piece}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, piece, generator)
    xs, ys, zs = zip(*[v.co[:] for v in col.data.vertices])
    box_bl = {"x": [round(min(xs), 3), round(max(xs), 3)], "y": [round(min(ys), 3), round(max(ys), 3)],
              "z": [round(min(zs), 3), round(max(zs), 3)]}
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           masksDensityBySlot=density, lodUvScale=list(LOD_UV_SCALE),
                           platformUvScale=PLATFORM_UV if which == "platform" else None,
                           aoOccluder=PIECES[other][0], maskStats=stats, skipped=skip_bake,
                           lod2Drops={S[n]: r for n, r in LOD2_DROPS[which].items()},
                           colliderBoxBlender=box_bl,
                           station={"platformTopAboveCapM": round(H, 3), "platformTopAboveRailTopM": 0.92,
                                    "faceOffsetM": FACE_Y, "copingEdgeOffsetM": COPE_Y,
                                    "levelLengthM": 2 * X_LEVEL, "overallLengthM": 2 * X_END,
                                    "carriageFootboardClearanceM": round(COPE_Y - 1.48, 3),
                                    "shelterAtBlender": [SHELTER_AT[0], SHELTER_AT[1], round(H, 3)]})
    print(f"{piece}: collider box (Blender frame) {box_bl}")
