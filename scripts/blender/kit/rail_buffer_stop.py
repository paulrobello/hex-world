"""rail_buffer_stop: a bent-rail buffer stop at an open rail end (owner D2, 2026-10-20: approved
ref assets-source/refs/rail_buffer_stop ref-2; trains stop 3.5 m inside the edge). A red timber
buffer beam with two sprung buffers and a lamp, carried on legs of rail bent up off the running
rails and braced back down to them behind the beam. Scripted route.

Frame and pivot. The piece stands on a track the rail centre piece already draws, so it carries no
rail, sleeper or ballast of its own. The pivot is the buffer contact plane on the track centreline at
cap level: Blender z = 0 is the cap (rail top at `_rail.RAIL_TOP`), the buffer heads' faces lie in
x = 0 and look along -x (at the train), and +x points to the open edge. Blender (x, y, z) exports as
runtime (x, z, -y). Placement (docs/checkpoints/rail_buffer_stop/runtime-notes.md): the pivot on the
track at path distance STOP_U from the open edge, local +x along the chord to the track point at
STOP_U - CHORD (the tangent on a straight), cap level. On the 8 m bend this keeps every foot that
rests on a rail head centred within HEAD_W / 2 of that rail (`bend_fit`), which tangent alignment does not
(the brace feet would drift 0.4 m).

Section reuse (`_rail`, read only): the legs and braces sweep `_rail.RAIL_PROFILE`, closed at the web
foot, so they are the running rails' own section; they stand on the running rails at |y| = RAIL_T.
Buffer centres match the train's (train_engine.py and train_carriage*.py: BUF_Z 1.05 m above the rail
top, BUF_Y 0.85 m).

Masks: one 512² atlas over the three LODs, a Cycles AO bake (with a straight track standing in as an
occluder, so the feet and the beam underside take the rails' and ballast's contact shade) and an
EMIT hooks bake, then `_house.fill_unbaked` and the centroid check, as the station."""
import json
import math
import os
import pathlib
import statistics
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Vector  # noqa: E402

import _rail  # noqa: E402
from _house import Mesh, centroid_samples, fill_unbaked  # noqa: E402
from lib import bake, budgets, collider, export, geom, hexgeo, materials, scene  # noqa: E402

PIECE = "rail_buffer_stop"
MASK_SIZE = 512
SAMPLES = 64
AO_DIST = 0.6
UV_MARGIN = 0.008
BAKE_MARGIN = 2
LOD_UV_SCALE = (1.0, 0.55, 0.35)

# Placement (owner D2 Q2): the buffer faces 3.5 m inside the open edge, along the track.
STOP_U = 3.5
CHORD = 2.24

RT = _rail.RAIL_TOP
RAIL_Y = _rail.RAIL_T
HEAD_W = _rail.HEAD_W
RAIL_H = _rail.RAIL_TOP - _rail.SLEEPER_TOP  # the rail section's height, 0.16 m
assert abs(RAIL_H - 0.16) < 1e-9 and abs(RAIL_Y - 0.84) < 1e-9 and abs(RT - 0.46) < 1e-9

BUF_Z, BUF_Y = RT + 1.05, 0.85  # train_engine.py / train_carriage.py: BUF_Z, BUF_Y over the rail top
HEAD_R, HEAD_T = 0.20, 0.05
PLUNGER_R, CASE_R = 0.075, 0.12
CASE_X0, PLATE_X0, BEAM_X0, BEAM_X1 = 0.16, 0.42, 0.45, 0.83
BEAM_Z0, BEAM_Z1, BEAM_HALF = 1.24, 1.78, 1.35
CHAMFER = 0.03
PLATE_HALF = 0.15
LEG_X = 0.72  # the legs' rear face (the rail foot); their head faces the train at LEG_X - RAIL_H
LEG_FOOT_X0 = 0.06
LEG_R = 0.45
BRACE_S = (0.66, 1.61)  # where each brace starts, wholly inside the beam
BRACE_ANGLE = 45.0
BRACE_R = 0.35
BRACE_END = 2.40
LAMP_X = (0.54, 0.74)
LAMP_Z = (1.86, 2.12)
LAMP_HALF = 0.11
LENS_R, LENS_Z = 0.065, 1.99

COLOURS = {  # linear, equal to the manifest slots
    "paint": ((0.42, 0.045, 0.03), 0.7),  # the rail family's red (rail_junction_center's rail_lever)
    "steel": ((0.05, 0.05, 0.055), 0.55),
    "glass": ((0.8, 0.05, 0.03), 0.15),
}
HOOKS = {"paint": (1.0, 0.35, 0.0), "steel": (0.8, 0.15, 0.0), "glass": (0.0, 0.05, 1.0)}
NAMES = list(COLOURS)
SLOTS = [f"{PIECE}_{n}" for n in NAMES]
S = dict(zip(NAMES, SLOTS))
X, Y, Z = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0)), Vector((0.0, 0.0, 1.0))


# --- primitives -----------------------------------------------------------------------------------

def box(mesh, lo, hi, slot, skip=()):
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    v = Vector
    faces = {
        "-x": ([v((x0, y0, z0)), v((x0, y1, z0)), v((x0, y1, z1)), v((x0, y0, z1))], -X),
        "+x": ([v((x1, y0, z0)), v((x1, y1, z0)), v((x1, y1, z1)), v((x1, y0, z1))], X),
        "-y": ([v((x0, y0, z0)), v((x1, y0, z0)), v((x1, y0, z1)), v((x0, y0, z1))], -Y),
        "+y": ([v((x0, y1, z0)), v((x1, y1, z0)), v((x1, y1, z1)), v((x0, y1, z1))], Y),
        "-z": ([v((x0, y0, z0)), v((x1, y0, z0)), v((x1, y1, z0)), v((x0, y1, z0))], -Z),
        "+z": ([v((x0, y0, z1)), v((x1, y0, z1)), v((x1, y1, z1)), v((x0, y1, z1))], Z),
    }
    for k, (pts, n) in faces.items():
        if k not in skip:
            mesh.face(pts, slot, expect=n)


def prism_x(mesh, x0, x1, cy, cz, r, n, slot, caps=(True, True), phase=0.5):
    """n-sided prism along +x from x0 to x1 about (cy, cz); caps (at x0, at x1)."""
    ang = [2 * math.pi * (k + phase) / n for k in range(n)]
    a = [Vector((x0, cy + r * math.cos(t), cz + r * math.sin(t))) for t in ang]
    b = [Vector((x1, cy + r * math.cos(t), cz + r * math.sin(t))) for t in ang]
    for k in range(n):
        kk = (k + 1) % n
        t = 2 * math.pi * (k + 0.5 + phase) / n
        mesh.face([a[k], a[kk], b[kk], b[k]], slot, expect=Vector((0.0, math.cos(t), math.sin(t))))
    if caps[0]:
        mesh.face(a, slot, expect=-X)
    if caps[1]:
        mesh.face(b, slot, expect=X)


def extrude_y(mesh, poly_xz, y0, y1, slot):
    """A convex x-z polygon extruded from y0 to y1, with both end caps."""
    cx = sum(p[0] for p in poly_xz) / len(poly_xz)
    cz = sum(p[1] for p in poly_xz) / len(poly_xz)
    a = [Vector((x, y0, z)) for x, z in poly_xz]
    b = [Vector((x, y1, z)) for x, z in poly_xz]
    n = len(poly_xz)
    for i in range(n):
        j = (i + 1) % n
        mx, mz = (poly_xz[i][0] + poly_xz[j][0]) / 2 - cx, (poly_xz[i][1] + poly_xz[j][1]) / 2 - cz
        mesh.face([a[i], a[j], b[j], b[i]], slot, expect=Vector((mx, 0.0, mz)))
    mesh.face(a, slot, expect=-Y)
    mesh.face(b, slot, expect=Y)


def sweep(mesh, y0, path, prof, slot, caps=(False, False)):
    """Sweep a closed profile [(t, h)] along an x-z path [(x, z, tx, tz)] in the plane y = y0: t runs
    along +y and h along the path's left normal (-tz, tx), so h = 0 is the rail foot and h = RAIL_H
    the head. Quads face out of the profile; caps close the first and last rings."""
    area = sum(prof[i][0] * prof[(i + 1) % len(prof)][1] - prof[(i + 1) % len(prof)][0] * prof[i][1]
               for i in range(len(prof)))
    rings = [[Vector((x - h * tz, y0 + t, z + h * tx)) for t, h in prof] for x, z, tx, tz in path]
    n = len(prof)
    for s in range(len(path) - 1):
        tx, tz = (path[s][2] + path[s + 1][2]) / 2, (path[s][3] + path[s + 1][3]) / 2
        for i in range(n):
            j = (i + 1) % n
            dt, dh = prof[j][0] - prof[i][0], prof[j][1] - prof[i][1]
            ot, oh = (dh, -dt) if area > 0 else (-dh, dt)
            mesh.face([rings[s][i], rings[s + 1][i], rings[s + 1][j], rings[s][j]], slot,
                      expect=Vector((-oh * tz, ot, oh * tx)))
    if caps[0]:
        mesh.face(rings[0], slot, expect=-Vector((path[0][2], 0.0, path[0][3])))
    if caps[1]:
        mesh.face(rings[-1], slot, expect=Vector((path[-1][2], 0.0, path[-1][3])))


# --- the bent-rail frame ----------------------------------------------------------------------------

RAIL_SECTION = [(t, z - _rail.SLEEPER_TOP) for t, z in _rail.RAIL_PROFILE]  # closed at the web foot
BAR = [(-HEAD_W / 2, 0.0), (-HEAD_W / 2, RAIL_H), (HEAD_W / 2, RAIL_H), (HEAD_W / 2, 0.0)]


def leg_path(arc_segs):
    """The leg's foot line: along the running rail head from LEG_FOOT_X0, a quarter turn of radius
    LEG_R up into the vertical at x = LEG_X, and on up into the beam."""
    x0 = LEG_X - LEG_R
    pts = [(LEG_FOOT_X0, RT, 1.0, 0.0)]
    for k in range(arc_segs + 1):
        f = math.radians(90.0 * k / arc_segs)
        pts.append((x0 + LEG_R * math.sin(f), RT + LEG_R * (1 - math.cos(f)), math.cos(f), math.sin(f)))
    pts.append((LEG_X, BEAM_Z0 + 0.06, 0.0, 1.0))
    return pts


def brace_knee():
    a = math.radians(BRACE_ANGLE)
    sx, sz = BRACE_S
    az = RT + BRACE_R * (1 - math.cos(a))
    ax = sx + (sz - az) / math.tan(a)
    return a, ax + BRACE_R * math.sin(a)  # angle, x where the foot meets the rail head


def brace_path(arc_segs):
    """From inside the beam down at BRACE_ANGLE, a turn of radius BRACE_R onto the running rail head,
    and along it to BRACE_END."""
    a, xk = brace_knee()
    sx, sz = BRACE_S
    pts = [(sx, sz, math.cos(a), -math.sin(a))]
    for k in range(arc_segs + 1):
        p = a * (1 - k / arc_segs)
        pts.append((xk - BRACE_R * math.sin(p), RT + BRACE_R * (1 - math.cos(p)), math.cos(p), -math.sin(p)))
    pts.append((BRACE_END, RT, 1.0, 0.0))
    return pts


def buffers(mesh, lod):
    for sy in (1, -1):
        y = sy * BUF_Y
        if lod == 0:
            prism_x(mesh, 0.0, HEAD_T, y, BUF_Z, HEAD_R, 12, S["steel"])
            prism_x(mesh, HEAD_T, CASE_X0, y, BUF_Z, PLUNGER_R, 8, S["steel"], caps=(False, False))
            prism_x(mesh, CASE_X0, PLATE_X0, y, BUF_Z, CASE_R, 12, S["paint"], caps=(True, False))
            box(mesh, (PLATE_X0, y - PLATE_HALF, BUF_Z - PLATE_HALF), (BEAM_X0, y + PLATE_HALF, BUF_Z + PLATE_HALF),
                S["paint"], skip=("+x",))
            for by in (-0.105, 0.105):
                for bz in (-0.105, 0.105):
                    box(mesh, (PLATE_X0 - 0.025, y + by - 0.0175, BUF_Z + bz - 0.0175),
                        (PLATE_X0, y + by + 0.0175, BUF_Z + bz + 0.0175), S["steel"], skip=("+x",))
        elif lod == 1:
            prism_x(mesh, 0.0, HEAD_T, y, BUF_Z, HEAD_R, 8, S["steel"])
            prism_x(mesh, HEAD_T, BEAM_X0, y, BUF_Z, CASE_R, 8, S["paint"], caps=(False, False))
            box(mesh, (PLATE_X0, y - PLATE_HALF, BUF_Z - PLATE_HALF), (BEAM_X0, y + PLATE_HALF, BUF_Z + PLATE_HALF),
                S["paint"], skip=("+x",))
        else:
            prism_x(mesh, 0.0, HEAD_T, y, BUF_Z, HEAD_R, 6, S["steel"], caps=(True, False))
            box(mesh, (HEAD_T, y - CASE_R, BUF_Z - CASE_R), (BEAM_X0, y + CASE_R, BUF_Z + CASE_R), S["paint"],
                skip=("-x", "+x"))


def beam(mesh, lod):
    if lod == 0:
        c = CHAMFER
        poly = [(BEAM_X0 + c, BEAM_Z0), (BEAM_X1 - c, BEAM_Z0), (BEAM_X1, BEAM_Z0 + c), (BEAM_X1, BEAM_Z1 - c),
                (BEAM_X1 - c, BEAM_Z1), (BEAM_X0 + c, BEAM_Z1), (BEAM_X0, BEAM_Z1 - c), (BEAM_X0, BEAM_Z0 + c)]
        extrude_y(mesh, poly, -BEAM_HALF, BEAM_HALF, S["paint"])
        # The coupling hook on the beam face, between the buffers.
        box(mesh, (BEAM_X0 - 0.02, -0.08, BUF_Z - 0.10), (BEAM_X0, 0.08, BUF_Z + 0.10), S["steel"], skip=("+x",))
        box(mesh, (BEAM_X0 - 0.15, -0.03, BUF_Z - 0.01), (BEAM_X0 - 0.02, 0.03, BUF_Z + 0.05), S["steel"],
            skip=("+x",))
        box(mesh, (BEAM_X0 - 0.15, -0.03, BUF_Z - 0.08), (BEAM_X0 - 0.10, 0.03, BUF_Z - 0.01), S["steel"],
            skip=("+z",))
    else:
        box(mesh, (BEAM_X0, -BEAM_HALF, BEAM_Z0), (BEAM_X1, BEAM_HALF, BEAM_Z1), S["paint"])


def frame(mesh, lod):
    for sy in (1, -1):
        y = sy * RAIL_Y
        if lod == 0:
            sweep(mesh, y, leg_path(6), RAIL_SECTION, S["steel"], caps=(True, False))
            sweep(mesh, y, brace_path(4), RAIL_SECTION, S["steel"], caps=(False, True))
        elif lod == 1:
            sweep(mesh, y, leg_path(3), BAR, S["steel"], caps=(True, False))
            sweep(mesh, y, brace_path(2), BAR, S["steel"], caps=(False, True))
        else:
            sweep(mesh, y, [(LEG_X, RT, 0.0, 1.0), (LEG_X, BEAM_Z0 + 0.06, 0.0, 1.0)], BAR, S["steel"])
            sx, sz = BRACE_S
            d = Vector((BRACE_END - sx, 0.0, RT - sz)).normalized()
            sweep(mesh, y, [(sx, sz, d.x, d.z), (BRACE_END, RT, d.x, d.z)], BAR, S["steel"], caps=(False, True))


def ring_path(xc, zc, r, n):
    pts = []
    for k in range(n + 1):
        t = 2 * math.pi * k / n
        pts.append((xc + r * math.cos(t), zc + r * math.sin(t), -math.sin(t), math.cos(t)))
    return pts


def lamp(mesh, lod):
    (x0, x1), (z0, z1), w = LAMP_X, LAMP_Z, LAMP_HALF
    xm = (x0 + x1) / 2
    if lod == 0:
        box(mesh, (xm - 0.04, -0.04, BEAM_Z1), (xm + 0.04, 0.04, z0), S["paint"], skip=("-z", "+z"))
        box(mesh, (x0, -w, z0), (x1, w, z1), S["paint"], skip=("-z", "+z"))
        box(mesh, (x0 - 0.02, -w - 0.02, z1), (x1 + 0.02, w + 0.02, z1 + 0.04), S["paint"])
        sq = [(-0.007, -0.007), (-0.007, 0.007), (0.007, 0.007), (0.007, -0.007)]
        sweep(mesh, 0.0, ring_path(xm, z1 + 0.04 + 0.065, 0.065, 8), sq, S["steel"])
        prism_x(mesh, x0 - 0.015, x0, 0.0, LENS_Z, LENS_R, 10, S["glass"], caps=(True, False))
    elif lod == 1:
        box(mesh, (x0, -w, BEAM_Z1), (x1, w, z1 + 0.04), S["paint"], skip=("-z",))
        prism_x(mesh, x0 - 0.015, x0, 0.0, LENS_Z, LENS_R, 6, S["glass"], caps=(True, False))
    else:
        box(mesh, (x0, -w, BEAM_Z1), (x1, w, z1), S["paint"], skip=("-z",))
        mesh.face([Vector((x0 - 0.005, -LENS_R, LENS_Z - LENS_R)), Vector((x0 - 0.005, LENS_R, LENS_Z - LENS_R)),
                   Vector((x0 - 0.005, LENS_R, LENS_Z + LENS_R)), Vector((x0 - 0.005, -LENS_R, LENS_Z + LENS_R))],
                  S["glass"], expect=-X)


def build_lod(lod, mats):
    mesh = Mesh(SLOTS)
    buffers(mesh, lod)
    beam(mesh, lod)
    frame(mesh, lod)
    lamp(mesh, lod)
    o = mesh.obj(f"{PIECE}_lod{lod}", mats)
    for p in o.data.polygons:
        p.use_smooth = True
    o.data.set_sharp_from_angle(angle=math.radians(38.0))
    return o


# --- checks ---------------------------------------------------------------------------------------

def path_point(kind, u):
    """Tile-frame Blender (x, y) of the track centreline at path distance u in from edge 0: the
    straight runs along x, the bend on `_rail.BEND_R` about `_rail.BEND_C` (corner 0)."""
    a = hexgeo.APOTHEM
    if kind == "straight":
        return Vector((a - u, 0.0))
    t = u / _rail.BEND_R
    return Vector((a - _rail.BEND_R * math.sin(t), _rail.BEND_C[1] + _rail.BEND_R * math.cos(t)))


def placement(kind):
    """(origin, yaw radians) of the pivot in the tile frame, Blender top view."""
    o = path_point(kind, STOP_U)
    d = (path_point(kind, STOP_U - CHORD) - o).normalized()
    return o, math.atan2(d.y, d.x)


def place(co, o, yaw):
    c, s = math.cos(yaw), math.sin(yaw)
    return Vector((o.x + c * co.x - s * co.y, o.y + s * co.x + c * co.y, co.z))


def rail_error(kind, p):
    """Plan distance from p to the nearer running rail's centreline."""
    if kind == "straight":
        return min(abs(p.y - RAIL_Y), abs(p.y + RAIL_Y))
    r = math.hypot(p.x - _rail.BEND_C[0], p.y - _rail.BEND_C[1])
    return min(abs(r - (_rail.BEND_R - RAIL_Y)), abs(r - (_rail.BEND_R + RAIL_Y)))


def bend_fit(lod0):
    """Per end kind: the worst plan distance of a foot centreline (the vertices resting on a rail head,
    z = RAIL_TOP, moved onto |y| = RAIL_T) from its rail centreline, the
    tangent-aligned alternative's, and the nearest any vertex comes to the hex border."""
    out = {}
    feet = [Vector((v.co.x, math.copysign(RAIL_Y, v.co.y), v.co.z)) for v in lod0.data.vertices if abs(v.co.z - RT) < 1e-5]
    assert feet, "no vertex rests on the rail heads"
    for kind in ("straight", "bend"):
        o, yaw = placement(kind)
        t = path_point(kind, STOP_U - 0.01) - o
        tangent_yaw = math.atan2(t.y, t.x)
        err = max(rail_error(kind, place(p, o, yaw)) for p in feet)
        err_t = max(rail_error(kind, place(p, o, tangent_yaw)) for p in feet)
        border = min(hexgeo.dist_to_border(*place(v.co, o, yaw)[:2]) for v in lod0.data.vertices)
        out[kind] = {"originBlender": [round(o.x, 4), round(o.y, 4)], "yawDeg": round(math.degrees(yaw), 3),
                     "tangentYawDeg": round(math.degrees(tangent_yaw), 3), "footRailErrorM": round(err, 4),
                     "footRailErrorTangentM": round(err_t, 4), "minBorderDistM": round(border, 3)}
    return out


def clearance_errors(lods):
    """Nothing stands in front of the buffer faces (x < 0) above the rail heads, where the train's
    buffer beam and valance are; the buffer faces lie in x = 0."""
    errs = []
    for o in lods:
        for v in o.data.vertices:
            if v.co.x < -1e-5 and v.co.z > RT + RAIL_H + 1e-4:
                errs.append(f"{o.name}: vertex {tuple(v.co)} in front of the buffer faces")
        xs = [v.co.x for v in o.data.vertices if abs(abs(v.co.y) - BUF_Y) <= HEAD_R + 1e-4
              and abs(v.co.z - BUF_Z) <= HEAD_R + 1e-4]
        if abs(min(xs)) > 1e-5:
            errs.append(f"{o.name}: buffer face at x {min(xs)}, not 0")
    return errs[:8]


def unwrap(lods):
    scene.select_only(lods)
    for o in lods:
        o.data.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.152, island_margin=UV_MARGIN, area_weight=0.2)
    bpy.ops.object.mode_set(mode="OBJECT")
    for o, k in zip(lods, LOD_UV_SCALE):
        uvl = o.data.uv_layers["UVMap"].data
        for li in range(len(uvl)):
            uvl[li].uv = uvl[li].uv * k
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=True, margin_method="FRACTION", margin=UV_MARGIN)
    bpy.ops.object.mode_set(mode="OBJECT")


def density_by_slot(obj):
    m = obj.data
    uvl = m.uv_layers["UVMap"].data
    vals = {}
    for p in m.polygons:
        if p.area < 1e-6:
            continue
        uvs = [uvl[li].uv for li in p.loop_indices]
        a = sum(abs((uvs[i] - uvs[0]).cross(uvs[i + 1] - uvs[0])) / 2 for i in range(1, len(uvs) - 1))
        vals.setdefault(NAMES[p.material_index], []).append(math.sqrt(a * MASK_SIZE * MASK_SIZE / p.area))
    return {k: round(statistics.median(v), 2) for k, v in sorted(vals.items())}


def track_occluder():
    """A straight track under the stop (the rail centre piece's own section and sleeper spacing,
    sleeper k centred (k + 0.5) pitches in from the edge, the edge at x = STOP_U), for the AO bake."""
    pitch = 2 * hexgeo.APOTHEM / round(2 * hexgeo.APOTHEM / _rail.SLEEPER_PITCH)
    frames = [((x, 0.0), (0.0, 1.0)) for x in (-4.0, STOP_U)]
    sleepers = [((STOP_U - (k + 0.5) * pitch, 0.0), (0.0, 1.0), (1.0, 0.0)) for k in range(11)]
    m = _rail.track(frames, sleepers, top_only=False)
    mat = materials.pbr_material(f"{PIECE}_occluder", (0.5, 0.5, 0.5, 1.0), 0.9)
    return m.obj(f"{PIECE}_track_occ", {s: mat for s in _rail.SLOTS})


# --- the piece ------------------------------------------------------------------------------------

def main():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    out.mkdir(parents=True, exist_ok=True)
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(PIECE)
    root = scene.empty(PIECE)
    mats = [materials.pbr_material(S[n], (*COLOURS[n][0], 1.0), COLOURS[n][1]) for n in NAMES]
    lods = [build_lod(i, mats) for i in range(3)]
    tris = [geom.tri_count(o) for o in lods]
    lod0_max, r1, r2 = budgets.lod_budget("prop", PIECE)
    print(f"{PIECE}: tris per LOD {tris} (LOD1 {tris[1] / tris[0]:.3f}, LOD2 {tris[2] / tris[0]:.3f}; "
          f"ceilings {lod0_max}, {r1}, {r2}; floor LOD0 >= {budgets.min_lod0(tris[2], 'prop')})")
    fit = bend_fit(lods[0])
    print(f"{PIECE}: placement fit {json.dumps(fit)}")
    errs = clearance_errors(lods)
    for kind, f in fit.items():
        if f["footRailErrorM"] > HEAD_W / 2:
            errs.append(f"{kind}: a foot sits {f['footRailErrorM']} m off its rail (> {HEAD_W / 2})")
        if f["minBorderDistM"] < 0.5:
            errs.append(f"{kind}: a vertex {f['minBorderDistM']} m from the border (< 0.5)")
    assert not errs, "\n".join(errs)
    budgets.check(tris, "prop", PIECE)

    unwrap(lods)
    signature = bake.receiver_signature(lods)
    density = {f"lod{i}": density_by_slot(o) for i, o in enumerate(lods)}
    print(f"{PIECE}: masks px/m per slot {density}")
    stats = None
    if not skip_bake:
        occ = track_occluder()
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(PIECE)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{PIECE}.ao", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=BAKE_MARGIN, clear=i == 0, occluders=(occ,))
        ao_png = bake.save_png(ao, str(out / f"{PIECE}.ao.tmp.png"))
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for n, m in zip(NAMES, mats):
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*HOOKS[n], 1.0)
        hooks = bake.new_image(f"{PIECE}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{PIECE}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        _, aop = bake.load_png(ao_png)
        os.remove(ao_png)
        os.remove(hooks_png)
        px = np.stack([hk[:, 0], hk[:, 1], hk[:, 2], aop[:, 0]], axis=1)
        px, filled = fill_unbaked(px, lods, MASK_SIZE, [HOOKS[n] for n in NAMES])
        masks_png = bake.write_png(MASK_SIZE, px, str(out / f"{PIECE}.masks.png"))
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01
        av = px[covered, 3]
        samples = centroid_samples(lods, px, MASK_SIZE, SLOTS, PIECE)
        stats = {"aoMean": float(av.mean()), "aoP05": float(np.percentile(av, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "filledTexels": filled, "centroids": samples}
        print(f"{PIECE}: masks {stats}")
        bad = {k: v["unbakedTris"] for k, v in samples.items() if v["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"
        bpy.data.objects.remove(occ, do_unlink=True)
        bpy.context.view_layer.update()

    for o in lods:
        o.parent = root
    col = collider.collider_from(lods[2], "box", f"{PIECE}_collider")
    col.data.materials.clear()
    # A box from LOD2 keeps no usable UVs: an oblique planar UV, flat shaded (as _train.py).
    col.data.shade_flat()
    if "UVMap" not in col.data.uv_layers:
        col.data.uv_layers.new(name="UVMap")
    for poly in col.data.polygons:
        for li in poly.loop_indices:
            co = col.data.vertices[col.data.loops[li].vertex_index].co
            col.data.uv_layers["UVMap"].data[li].uv = (0.05 * (co.x + 0.37 * co.z),
                                                       0.05 * (co.y + 0.61 * co.z + 0.23 * co.x))
    col.parent = root
    export.export_glb(root, str(out / f"{PIECE}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, PIECE, __file__)
    xs, ys, zs = zip(*[v.co[:] for v in col.data.vertices])
    box_bl = {"x": [round(min(xs), 3), round(max(xs), 3)], "y": [round(min(ys), 3), round(max(ys), 3)],
              "z": [round(min(zs), 3), round(max(zs), 3)]}
    a, xk = brace_knee()
    bake.write_bake_report(
        str(out / "bake-report.json"), receiverSignature=signature,
        device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
        aoDistanceM=AO_DIST, aoOccluder="straight track (_rail.track)",
        channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}}, masksDensityBySlot=density,
        lodUvScale=list(LOD_UV_SCALE), maskStats=stats, skipped=skip_bake, colliderBoxBlender=box_bl, lodTris=tris,
        bufferStop={
            "pivot": "track centreline at the buffer contact plane, cap level (z 0); +x toward the open edge",
            "stopInsideEdgeM": STOP_U, "chordM": CHORD, "bufferFaceX": 0.0,
            "bufferCentres": {"y": [-BUF_Y, BUF_Y], "zAboveCap": BUF_Z, "zAboveRailTop": round(BUF_Z - RT, 3),
                              "headRadius": HEAD_R},
            "beam": {"x": [BEAM_X0, BEAM_X1], "y": [-BEAM_HALF, BEAM_HALF], "z": [BEAM_Z0, BEAM_Z1]},
            "legs": {"y": [-RAIL_Y, RAIL_Y], "footX": [LEG_FOOT_X0, LEG_X - LEG_R], "uprightX": [LEG_X - RAIL_H, LEG_X]},
            "braces": {"y": [-RAIL_Y, RAIL_Y], "angleDeg": BRACE_ANGLE, "footX": [round(xk, 3), BRACE_END]},
            "lamp": {"x": list(LAMP_X), "zTop": LAMP_Z[1] + 0.04 + 0.13, "lensZ": LENS_Z},
            "placementFit": fit})
    print(f"{PIECE}: collider box (Blender frame) {box_bl}")
    print(json.dumps({"piece": PIECE, "tris": tris}))


main()
