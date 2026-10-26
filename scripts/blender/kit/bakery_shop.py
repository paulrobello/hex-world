"""bakery_shop: the village bakery (P3, owner A17; approved refs assets-source/refs/bakery ref-1 and
ref-2). A two-storey timber-framed English cottage shop in the house_cottage family: dark oak
half-timbering over cream lime render, a steep brown clay-tile roof with a jettied front cross gable,
two external red-brick gable stacks with clay pots, a wide small-paned bow window under the jetty, a
half-glazed door under a small tile canopy, and a bench with a bread basket.

The bread in the bow window is colour and masks, never geometry: each shelf row of the bay's glazing
is split into a warm loaf band (`loaf_glass`) under a darker shop-interior band (`display_glass`),
and on LOD0 the masks carry the loaves' shapes (AO darkens the gaps between seeded loaf ellipses,
night emission dims the interior between them). AO only scales ambient light at runtime, so the
loaf shapes read in shade and at night; in direct sun the colour bands carry the bread.

Scripted route. It imports _house.py's mesh accumulator, wall frame, casement window and roof
profile, and keeps the family's slot conventions: isotropic surfaces sample the world-space `plaster`
set tinted per slot, directional detail (tile courses, framing, joinery, glazing bars) is geometry,
timber, joinery, glass, wicker and bread are colour-only, and the one unique map is a 1024² masks
atlas shared by all three LODs (each LOD baked onto its own islands, as _house.build_piece does).
Faces nobody sees (roof undersides inside the walls, the plinth top under the floor, walls behind the
plinth, the bay and the wing) are not built, so the atlas spends its texels on visible faces.

Frame as _house.py: Blender (x, y, z) exports as runtime (x, z, -y), the pivot is the footprint
centre at ground level, and the shop front faces +x (yaw 0). Main block 5.0 m deep (x) by 7.0 m wide
(y), eaves at 4.4 m, 50° roof; the cross wing on the front-left is jettied 0.35 m at 2.75 m."""
import math
import os
import pathlib
import random
import statistics
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).parent))

import bmesh  # noqa: E402
import bpy  # noqa: E402
import numpy as np  # noqa: E402
from mathutils import Matrix, Vector  # noqa: E402

import _lod_copy  # noqa: E402
from _house import (DROP, PLASTER_LIN, PLINTH_OUT, PLINTH_TOP, Z, Mesh, Wall, centroid_samples,  # noqa: E402
                    fill_unbaked, frame_matrix, roof_profile)
from lib import bake, budgets, collider, export, geom, materials, scene, uv  # noqa: E402

P = "bakery_shop"
X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
MASK_SIZE = 1024
SAMPLES = 64
AO_DIST = 0.6
UV_MARGIN = 0.006  # island gap as a fraction of the atlas: about 6 px, so 2 px bake margins never meet
BAKE_MARGIN = 2
# UV scale of each LOD's islands before packing (LOD1 draws from 150 m, LOD2 from 400 m) and of LOD0's
# bow-window panes (the loaf pattern), so LOD0's visible walls keep the family's masks density.
LOD_UV_SCALE = (1.0, 0.55, 0.35)
DISPLAY_BOOST = 2.5

# Manifest colour and roughness per slot. Tileable slots multiply the plaster set's albedo at runtime,
# so their Blender colour is the manifest colour x PLASTER_LIN (the set's mean linear albedo).
TILEABLE = ("walls", "roof", "brick", "stone")
COLOURS = {
    "walls": ((1.0, 0.9, 0.66), 0.9),
    "roof": ((0.24, 0.12, 0.08), 0.85),
    "brick": ((0.44, 0.14, 0.08), 0.9),
    "stone": ((0.45, 0.42, 0.37), 0.9),
    "timber": ((0.07, 0.048, 0.033), 0.8),
    "frame": ((0.12, 0.075, 0.042), 0.75),
    "door": ((0.17, 0.105, 0.055), 0.7),
    "glass": ((0.2, 0.16, 0.1), 0.2),
    "display_glass": ((0.16, 0.1, 0.055), 0.25),
    "loaf_glass": ((0.55, 0.29, 0.1), 0.3),
    "wicker": ((0.45, 0.3, 0.14), 0.85),
    "bread": ((0.55, 0.27, 0.085), 0.6),
}
# masks hooks per slot: (snow capacity, porosity, night emissive).
HOOKS = {"walls": (1.0, 0.5, 0.0), "roof": (1.0, 0.35, 0.0), "brick": (1.0, 0.45, 0.0),
         "stone": (1.0, 0.4, 0.0), "timber": (1.0, 0.6, 0.0), "frame": (1.0, 0.5, 0.0),
         "door": (1.0, 0.5, 0.0), "glass": (0.0, 0.05, 1.0), "display_glass": (0.0, 0.05, 1.0),
         "loaf_glass": (0.0, 0.05, 1.0), "wicker": (1.0, 0.6, 0.0), "bread": (1.0, 0.4, 0.0)}
S = {n: f"{P}_{n}" for n in COLOURS}
WIN = {"glass": S["glass"], "frame": S["frame"], "sill": S["frame"]}

# Main block.
HD, HW, EAVE = 2.5, 3.5, 4.4
PITCH, OVER, GOVER, COURSES, KICK, THICK = math.radians(50.0), 0.3, 0.25, 11, 0.04, 0.14
SOLE, RAIL, PLATE = PLINTH_TOP + 0.08, 2.75, EAVE - 0.32  # the plate sits under the eave soffit (z 4.18)
GABLE_TOP = EAVE + HD * math.tan(PITCH) - 0.05
# Cross wing (front-left): jettied upper storey over the bow window. Its eaves overhang 0.2 m, so the
# left eave stays 5 cm inside the main gable plane and never shows as a ledge along the gable wall.
GY0, GY1 = -3.25, 0.35
GY, GHW = (GY0 + GY1) / 2, (GY1 - GY0) / 2
JET, JZ = 0.35, 2.75
WPITCH, WOVER, WFRONT, WCOURSES = math.radians(56.0), 0.2, 0.3, 10
XB, XF = 0.0, HD + JET + WFRONT  # wing roof sweep: buried in the main roof at XB, verge at XF
WAPEX = EAVE + GHW * math.tan(WPITCH) - 0.05
COLLAR = 5.6
# Bow window: a three-sided bay on the main front wall under the jetty.
BAY_BACK, BAY_FRONT, BAY_DEPTH = 1.5, 1.1, 0.55
SILL_Z, GLASS0, GLASS1, CORNICE_TOP = 0.55, 0.62, 2.42, 2.72
BAY_ROWS = 4  # shelves: the horizontal glazing bars split the bay into four rows
LOAF_BAND = 0.25  # loaf band height at the bottom of each 0.45 m row
# Shop door (main front, right of the wing) and its canopy.
DOOR_U, DOOR_W, DOOR_H = 1.92, 0.95, 2.15
# Gable stacks: (x centre, side, top of the stack below its oversail course).
CHIMNEYS = ((-0.8, -1, 8.2), (0.0, 1, 8.4))
BREAST, SHOULDER = 2.3, 2.9


def box(mesh, center, ax, ay, az, size, slot, drop=()):
    """Box along the right-handed frame (ax, ay, az), sized `size`; faces whose outward normal points
    along any `drop` direction are left out (buried or butting faces)."""
    assert ax.cross(ay).dot(az) > 0.99, (ax, ay, az)
    res = bmesh.ops.create_cube(mesh.bm, size=1.0, matrix=frame_matrix(Vector(center), ax, ay, az, size))
    for f in list({f for v in res["verts"] for f in v.link_faces}):
        f.normal_update()
        if any(f.normal.dot(d) > 0.9 for d in drop):
            mesh.bm.faces.remove(f)
        else:
            f.material_index = mesh.idx[slot]


def strut(mesh, a, b, side, width, depth, slot):
    """A member from a to b with its four long faces (no end caps): `side` is the width axis."""
    t = (b - a).normalized()
    n = t.cross(side).normalized()
    box(mesh, (a + b) / 2, t, side.normalized(), n, ((b - a).length, width, depth), slot, drop=(t, -t))


def beam(mesh, wall, u0, z0, u1, z1, width, proud, slot, out=0.0, sides=True):
    """A framing member on a wall plane: its front face and (sides) its two long sides. The ends are
    open; members butt into posts, rails, the eaves or each other."""
    a, b = wall.p(u0, z0, out), wall.p(u1, z1, out)
    t = (b - a).normalized()
    h = wall.n.cross(t) * (width / 2)
    f = wall.n * proud
    mesh.face([a - h + f, b - h + f, b + h + f, a + h + f], slot, wall.n)
    if sides:
        mesh.face([a + h, b + h, b + h + f, a + h + f], slot, h)
        mesh.face([a - h, a - h + f, b - h + f, b - h], slot, -h)


def wall_rect(mesh, y0, y1, z0, z1):
    mesh.face([(HD, y0, z0), (HD, y1, z0), (HD, y1, z1), (HD, y0, z1)], S["walls"], X)


def plinth(mesh):
    """The stone plinth as a ring: its four outer faces and the ledge over the wall line (no top over
    the floor, no faces behind it)."""
    o = PLINTH_OUT
    outer = [(HD + o, -HW - o), (HD + o, HW + o), (-HD - o, HW + o), (-HD - o, -HW - o)]
    inner = [(HD, -HW), (HD, HW), (-HD, HW), (-HD, -HW)]
    for i in range(4):
        j = (i + 1) % 4
        (ax, ay), (bx, by) = outer[i], outer[j]
        n = Vector(((ax + bx) / 2, (ay + by) / 2, 0.0)).normalized()
        mesh.face([(ax, ay, DROP), (bx, by, DROP), (bx, by, PLINTH_TOP), (ax, ay, PLINTH_TOP)], S["stone"], n)
        mesh.face([(ax, ay, PLINTH_TOP), (bx, by, PLINTH_TOP), (*inner[j], PLINTH_TOP), (*inner[i], PLINTH_TOP)], S["stone"], Z)


def plan_offset(pts, d):
    """The counter-clockwise open plan polyline pts offset outward by d, its ends kept on the wall
    line x = pts[0].x."""
    lines = []
    for a, b in zip(pts, pts[1:]):
        e = (b - a).normalized()
        lines.append((a + Vector((e.y, -e.x)) * d, e))
    p0, e0 = lines[0]
    out = [p0 + e0 * ((pts[0].x - p0.x) / e0.x)]
    for (p1, e1), (p2, e2) in zip(lines, lines[1:]):
        s = ((p2.x - p1.x) * e2.y - (p2.y - p1.y) * e2.x) / (e1.x * e2.y - e1.y * e2.x)
        out.append(p1 + e1 * s)
    pn, en = lines[-1]
    out.append(pn + en * ((pts[-1].x - pn.x) / en.x))
    return out


def slab(mesh, plan, z0, z1, slot, inner=None, bottom=False):
    """Prism over a plan outline against the wall: the outer sides, and the top (only the band outside
    `inner` when given: the rest sits behind the glass)."""
    lo = [Vector((p.x, p.y, z0)) for p in plan]
    hi = [Vector((p.x, p.y, z1)) for p in plan]
    if inner is None:
        mesh.face(hi, slot, Z)
    else:
        for i in range(len(plan) - 1):
            mesh.face([hi[i], hi[i + 1], Vector((*inner[i + 1].xy, z1)), Vector((*inner[i].xy, z1))], slot, Z)
    if bottom:
        mesh.face(lo, slot, -Z)
    for i in range(len(plan) - 1):
        e = plan[i + 1] - plan[i]
        mesh.face([lo[i], lo[i + 1], hi[i + 1], hi[i]], slot, Vector((e.y, -e.x, 0.0)))


def bay_plan():
    hx = HD + BAY_DEPTH
    return [Vector((HD, GY - BAY_BACK)), Vector((hx, GY - BAY_FRONT)), Vector((hx, GY + BAY_FRONT)),
            Vector((HD, GY + BAY_BACK))]


def bay_faces():
    """(wall frame, length) of the bay's three faces, left side, front, right side; u runs from 0."""
    pts = bay_plan()
    out = []
    for a, b in zip(pts, pts[1:]):
        e = b - a
        out.append((Wall(Vector((e.y, -e.x, 0.0)).normalized(), (a.x, a.y, 0.0)), e.length))
    return out


def bow_window(mesh, lod):
    plan = bay_plan()
    faces = bay_faces()
    bands = ((DROP, PLINTH_TOP, S["stone"]), (PLINTH_TOP, SILL_Z, S["walls"])) if lod < 2 else ((DROP, GLASS0, S["walls"]),)
    row_h = (GLASS1 - GLASS0) / BAY_ROWS
    for w, ln in faces:
        for z0, z1, slot in bands:
            mesh.face([w.p(0, z0), w.p(ln, z0), w.p(ln, z1), w.p(0, z1)], slot, w.n)
        if lod == 2:
            mesh.face([w.p(0, GLASS0), w.p(ln, GLASS0), w.p(ln, GLASS1), w.p(0, GLASS1)], S["loaf_glass"], w.n)
            continue
        for r in range(BAY_ROWS):
            z0 = GLASS0 + r * row_h
            for za, zb, slot in ((z0, z0 + LOAF_BAND, S["loaf_glass"]), (z0 + LOAF_BAND, z0 + row_h, S["display_glass"])):
                mesh.face([w.p(0, za), w.p(ln, za), w.p(ln, zb), w.p(0, zb)], slot, w.n)
    if lod == 2:
        mesh.face([Vector((p.x, p.y, GLASS1)) for p in plan], S["timber"], Z)
        return
    slab(mesh, plan_offset(plan, 0.06), SILL_Z, GLASS0, S["frame"], inner=plan)
    slab(mesh, plan_offset(plan, 0.06), GLASS1, CORNICE_TOP, S["timber"])
    (wl, ll), (wf, lf), (wr, lr) = faces
    for w, u in ((wl, 0.05), (wf, 0.05), (wf, lf - 0.05), (wr, lr - 0.05)):
        beam(mesh, w, u, GLASS0, u, GLASS1, 0.1, 0.05, S["timber"])
    if lod == 0:
        # Glazing bars: front faces only, 2 cm proud (their sides would only show at grazing angles).
        for w, ln, cols in ((wl, ll, 2), (wf, lf, 6), (wr, lr, 2)):
            for k in range(1, cols):
                u = ln * k / cols
                beam(mesh, w, u, GLASS0, u, GLASS1, 0.035, 0.02, S["frame"], sides=False)
            for k in range(1, BAY_ROWS):
                z = GLASS0 + row_h * k
                beam(mesh, w, 0.0, z, ln, z, 0.035, 0.021, S["frame"], sides=False)
        # Brackets under the jetty at both ends of the bay (ref-2).
        for y in (GY0 + 0.1, GY1 - 0.1):
            strut(mesh, Vector((HD, y, JZ - 0.6)), Vector((HD + JET - 0.03, y, JZ - 0.02)), Y, 0.1, 0.1, S["timber"])


def shop_door(mesh, w, lod):
    u0, u1, g = DOOR_U - DOOR_W / 2, DOOR_U + DOOR_W / 2, 0.03
    mesh.face([w.p(u0, PLINTH_TOP, g), w.p(u1, PLINTH_TOP, g), w.p(u1, DOOR_H, g), w.p(u0, DOOR_H, g)], S["door"], w.n)
    gu0, gu1, gz0, gz1 = u0 + 0.12, u1 - 0.12, DOOR_H * 0.5 + 0.05, DOOR_H - 0.12
    mesh.face([w.p(gu0, gz0, g + 0.004), w.p(gu1, gz0, g + 0.004), w.p(gu1, gz1, g + 0.004),
               w.p(gu0, gz1, g + 0.004)], S["glass"], w.n)
    if lod == 0:
        beam(mesh, w, DOOR_U, gz0, DOOR_U, gz1, 0.03, 0.012, S["door"], out=g, sides=False)
        for k in (1, 2):
            z = gz0 + (gz1 - gz0) * k / 3
            beam(mesh, w, gu0, z, gu1, z, 0.03, 0.013, S["door"], out=g, sides=False)
    if lod < 2:
        for u in (u0 - 0.05, u1 + 0.05):
            beam(mesh, w, u, PLINTH_TOP, u, DOOR_H, 0.1, 0.07, S["frame"])
        beam(mesh, w, u0 - 0.1, DOOR_H + 0.05, u1 + 0.1, DOOR_H + 0.05, 0.1, 0.07, S["frame"])
        box(mesh, w.p(DOOR_U, (DROP + 0.18) / 2, 0.22), w.t, Z, w.n, (DOOR_W + 0.4, 0.18 - DROP, 0.44),
            S["stone"], drop=(-w.n, -Z))


def canopy(mesh, w, lod):
    """A small pent canopy of clay tiles over the door on two brackets, in two courses at LOD0."""
    slope = (w.n * 0.85 - Z * 0.25).normalized()
    up = slope.cross(w.t)
    base = w.p(DOOR_U, 2.58, 0.0)
    if lod == 0:
        for k, (s0, s1, lift) in enumerate(((0.0, 0.48, 0.0), (0.42, 0.9, -0.035))):
            c = base + slope * ((s0 + s1) / 2) + up * (0.04 + lift)
            box(mesh, c, w.t, up, slope, (1.5, 0.07, s1 - s0), S["roof"], drop=(-slope,) if k == 0 else ())
        for du in (-0.62, 0.62):
            strut(mesh, w.p(DOOR_U + du, 2.0, 0.0), w.p(DOOR_U + du, 2.47, 0.62), w.t, 0.08, 0.08, S["timber"])
    else:
        box(mesh, base + slope * 0.45 + up * 0.04, w.t, up, slope, (1.5, 0.08, 0.9), S["roof"], drop=(-slope,))


def chimney(mesh, cx, side, top, lod):
    """An external red-brick stack on a gable end: breast, weathered shoulder, stack, oversail
    course and clay pots. `side` is -1 for the left gable (y = -HW), +1 for the right."""
    out = Y * side
    wy = side * HW

    def at(x, d, z):
        return Vector((x, wy + side * d, z))

    if lod == 2:
        box(mesh, at(cx, 0.2, (DROP + top) / 2), X, Y, Z, (0.84, 0.6, top - DROP), S["brick"], drop=(-Z,))
        return
    box(mesh, at(cx, 0.3, (DROP + BREAST) / 2), X, Y, Z, (1.3, 0.6, BREAST - DROP), S["brick"], drop=(-Z, -out))
    lo = [at(cx - 0.65, 0.0, BREAST), at(cx - 0.65, 0.6, BREAST), at(cx + 0.65, 0.6, BREAST), at(cx + 0.65, 0.0, BREAST)]
    hi = [at(cx - 0.42, 0.0, SHOULDER), at(cx - 0.42, 0.5, SHOULDER), at(cx + 0.42, 0.5, SHOULDER), at(cx + 0.42, 0.0, SHOULDER)]
    for i, nrm in ((0, -X), (1, out), (2, X)):
        mesh.face([lo[i], lo[i + 1], hi[i + 1], hi[i]], S["brick"], nrm + Z * 0.3)
    box(mesh, at(cx, 0.2, (SHOULDER + top) / 2), X, Y, Z, (0.84, 0.6, top - SHOULDER), S["brick"], drop=(-Z, Z))
    box(mesh, at(cx, 0.2, top + 0.06), X, Y, Z, (1.0, 0.76, 0.12), S["brick"])
    for px in ((-0.2, 0.2) if lod == 0 else (0.0,)):
        res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=6 if lod == 0 else 5, radius1=0.12,
                                    radius2=0.1, depth=0.4, matrix=Matrix.Translation(at(cx + px, 0.2, top + 0.32)))
        for f in list({f for v in res["verts"] for f in v.link_faces}):
            f.normal_update()
            if f.normal.z < -0.9:
                mesh.bm.faces.remove(f)
            else:
                f.material_index = mesh.idx[S["roof"]]


def roof(mesh, half_d, pitch, over, courses, at, s0, s1, soffit, verges, caps):
    """A roof from _house.roof_profile swept from s0 to s1; at(x, z, s) maps profile (x, z) at sweep
    position s to Blender space. Of the underside only the eave soffits (over the sweep span
    `soffit`) and the verge overhangs (the `verges` spans) are built: the rest faces the attic and
    would only spend masks texels. `caps`: sweep positions that get an end cap. Returns the ridge
    height above the eave line."""
    pts, ridge = roof_profile(half_d, pitch, over, courses, KICK, THICK)
    k = len(pts)
    e, u = Vector(pts[0]), Vector(pts[-1])
    wf = u + (e - u) * (half_d / e.x)  # the underside at the wall line
    wb = Vector((-wf.x, wf.y))
    top = (k - 2) // 2  # pts[top] is the ridge
    rings = {s: [mesh.bm.verts.new(at(x, z, s)) for x, z in pts] for s in (s0, s1)}
    # Top surface, course butts and the eave fronts: every profile segment but the two underside ones.
    for i in range(k - 2):
        f = mesh.bm.faces.new((rings[s0][i], rings[s1][i], rings[s1][i + 1], rings[s0][i + 1]))
        f.material_index = mesh.idx[S["roof"]]
        if i == top - 1:
            f.normal_update()
            assert f.normal.z > 0.3, f"roof sweep winds inward (normal {f.normal[:]})"
    under = [(e, wf, (soffit,)), (wb, Vector(pts[-2]), (soffit,)), (wf, u, verges), (u, wb, verges)]
    for a, b, spans in under:
        for t0, t1 in spans:
            if t1 - t0 > 1e-6:
                mesh.face([at(a.x, a.y, t0), at(a.x, a.y, t1), at(b.x, b.y, t1), at(b.x, b.y, t0)], S["roof"], -Z)
    for s in caps:
        f = mesh.face([at(x, z, s) for x, z in pts], S["roof"])
        f.normal_update()
        outward = Vector(at(0.0, 0.0, s + (1.0 if s == s1 else -1.0))) - Vector(at(0.0, 0.0, s))
        if f.normal.dot(outward) < 0:
            f.normal_flip()
    return ridge


def main_roof(mesh, lod):
    zb = EAVE - OVER * math.tan(PITCH)
    y0, y1 = -HW - GOVER, HW + GOVER
    roof(mesh, HD, PITCH, OVER, COURSES if lod == 0 else 0, lambda x, z, s: (x, s, zb + z),
         y0, y1, (y0, y1), ((y0, -HW), (HW, y1)), (y0, y1))


def wing_roof(mesh, lod):
    """The cross gable's roof, swept along +x from inside the main roof to the front verge. Profile
    x maps to -y, which keeps the (profile, sweep, z) frame's handedness of the main roof's sweep, so
    the same quad winding gives outward normals (checked in `roof`). Its eave soffits start at the
    main wall line: behind it they sit inside the main roof."""
    zb = EAVE - WOVER * math.tan(WPITCH)
    ridge = roof(mesh, GHW, WPITCH, WOVER, WCOURSES if lod == 0 else 0, lambda x, z, s: (s, GY - x, zb + z),
                 XB, XF, (HD, XF), ((HD + JET, XF),), (XF,))
    return zb, ridge


# Framing: (wall, u0, z0, u1, z1, kept at LOD1, width). Rails run 5 cm past a corner so the members
# on the two walls close the corner; rails stop at the chimney breasts and the door.
TIMBERS = [
    ("F", -3.42, SOLE, -3.42, PLATE + 0.08, True, 0.16), ("F", 0.43, SOLE, 0.43, PLATE + 0.08, True, 0.16),
    ("F", 3.42, SOLE, 3.42, PLATE + 0.08, True, 0.16),
    ("F", GY1, SOLE, 1.33, SOLE, True, 0.15), ("F", 2.51, SOLE, HW + 0.05, SOLE, True, 0.15),
    ("F", GY1, RAIL, HW + 0.05, RAIL, True, 0.15), ("F", GY1, PLATE, HW + 0.05, PLATE, True, 0.15),
    ("F", 2.95, SOLE, 2.95, RAIL, False, 0.13),
    ("F", 1.28, RAIL, 1.28, PLATE, False, 0.13), ("F", 2.56, RAIL, 2.56, PLATE, False, 0.13),
    ("F", 0.51, PLATE - 0.08, 1.2, RAIL + 0.08, False, 0.13), ("F", 2.64, RAIL + 0.08, 3.34, PLATE - 0.08, False, 0.13),
    ("WF", GY0 - 0.05, JZ + 0.125, GY1 + 0.05, JZ + 0.125, True, 0.25),
    ("WF", GY0 - 0.05, EAVE - 0.09, GY1 + 0.05, EAVE - 0.09, True, 0.18),
    ("WF", GY0 + 0.08, JZ + 0.25, GY0 + 0.08, EAVE - 0.18, True, 0.16),
    ("WF", GY1 - 0.08, JZ + 0.25, GY1 - 0.08, EAVE - 0.18, True, 0.16),
    ("WF", GY, EAVE, GY, WAPEX - 0.35, True, 0.15),
    ("WF", GY - 0.8, JZ + 0.25, GY - 0.8, EAVE - 0.18, False, 0.13),
    ("WF", GY + 0.8, JZ + 0.25, GY + 0.8, EAVE - 0.18, False, 0.13),
    ("WF", GY0 + 0.16, JZ + 0.3, GY - 0.88, EAVE - 0.2, False, 0.13),
    ("WF", GY1 - 0.16, JZ + 0.3, GY + 0.88, EAVE - 0.2, False, 0.13),
    ("WF", GY - 0.86, COLLAR, GY + 0.86, COLLAR, False, 0.14),
    ("WF", GY - 0.08, EAVE + 0.1, GY - 0.8, COLLAR - 0.08, False, 0.12),
    ("WF", GY + 0.08, EAVE + 0.1, GY + 0.8, COLLAR - 0.08, False, 0.12),
    ("B", -HW - 0.05, SOLE, HW + 0.05, SOLE, True, 0.15), ("B", -HW - 0.05, RAIL, HW + 0.05, RAIL, True, 0.15),
    ("B", -HW - 0.05, PLATE, HW + 0.05, PLATE, True, 0.15),
    ("B", -3.42, SOLE, -3.42, PLATE + 0.08, True, 0.16), ("B", 3.42, SOLE, 3.42, PLATE + 0.08, True, 0.16),
    ("B", 0.0, SOLE, 0.0, PLATE, False, 0.13),
    ("B", -3.3, RAIL + 0.08, -2.55, PLATE - 0.08, False, 0.13), ("B", 3.3, RAIL + 0.08, 2.55, PLATE - 0.08, False, 0.13),
    ("L", -HD - 0.05, SOLE, -1.5, SOLE, True, 0.15), ("L", -0.1, SOLE, HD + 0.05, SOLE, True, 0.15),
    ("L", -HD - 0.05, RAIL, -1.5, RAIL, True, 0.15), ("L", -0.1, RAIL, HD + 0.05, RAIL, True, 0.15),
    ("L", -HD - 0.05, EAVE - 0.06, -1.27, EAVE - 0.06, True, 0.18), ("L", -0.33, EAVE - 0.06, HD + 0.05, EAVE - 0.06, True, 0.18),
    ("L", -2.42, SOLE, -2.42, EAVE, True, 0.16), ("L", 2.42, SOLE, 2.42, EAVE, True, 0.16),
    ("L", 0.0, EAVE, 0.0, GABLE_TOP - 0.35, False, 0.15),
    ("L", 0.55, SOLE, 0.55, RAIL, False, 0.13), ("L", 1.85, SOLE, 1.85, RAIL, False, 0.13),
    ("R", -HD - 0.05, SOLE, -0.7, SOLE, True, 0.15), ("R", 0.7, SOLE, HD + 0.05, SOLE, True, 0.15),
    ("R", -HD - 0.05, RAIL, -0.7, RAIL, True, 0.15), ("R", 0.7, RAIL, HD + 0.05, RAIL, True, 0.15),
    ("R", -HD - 0.05, EAVE - 0.06, -0.47, EAVE - 0.06, True, 0.18), ("R", 0.47, EAVE - 0.06, HD + 0.05, EAVE - 0.06, True, 0.18),
    ("R", -2.42, SOLE, -2.42, EAVE, True, 0.16), ("R", 2.42, SOLE, 2.42, EAVE, True, 0.16),
    ("R", -0.85, SOLE, -0.85, RAIL, False, 0.13), ("R", 0.85, RAIL, 0.85, EAVE, False, 0.13),
]
# Casements: (wall, u, sill z, width, height, major). Major windows keep their frame at LOD1 and
# their sill and bars at LOD0; the rest are frame-only at LOD0 and glass-only from LOD1.
WINDOWS = [("WF", GY, 3.12, 1.2, 1.0, True), ("F", DOOR_U, 2.97, 0.9, 0.9, True),
           ("B", -1.6, 2.97, 0.9, 0.9, False), ("B", 1.6, 2.97, 0.9, 0.9, False), ("B", 1.6, 0.95, 0.9, 1.0, False),
           ("L", 1.2, 0.95, 0.8, 1.0, False), ("R", -1.5, 0.95, 0.8, 1.0, False), ("R", 1.5, 3.05, 0.8, 0.95, False)]


def walls_of():
    return {"F": Wall((1, 0, 0), (HD, 0, 0)), "WF": Wall((1, 0, 0), (HD + JET, 0, 0)),
            "B": Wall((-1, 0, 0), (-HD, 0, 0)), "L": Wall((0, -1, 0), (0, -HW, 0)), "R": Wall((0, 1, 0), (0, HW, 0))}


def basket(mesh, c, r, h):
    """A round wicker basket heaped with bread: a flared drum and a low bread dome over its mouth."""
    res = bmesh.ops.create_cone(mesh.bm, cap_ends=False, segments=8, radius1=r, radius2=r * 1.15, depth=h,
                                matrix=Matrix.Translation(c + Z * (h / 2)))
    for f in {f for v in res["verts"] for f in v.link_faces}:
        f.material_index = mesh.idx[S["wicker"]]
    res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=8, radius1=r * 1.15, radius2=0.03, depth=0.12,
                                matrix=Matrix.Translation(c + Z * (h + 0.06)))
    for f in {f for v in res["verts"] for f in v.link_faces}:
        f.material_index = mesh.idx[S["bread"]]


def bench(mesh, w):
    """The bench between the bow window and the door, with a basket of bread on it (ref-2)."""
    u, out = 0.86, 0.3
    box(mesh, w.p(u, 0.445, out), w.t, Z, w.n, (0.72, 0.05, 0.32), S["timber"])
    for du in (-0.28, 0.28):
        box(mesh, w.p(u + du, (-0.15 + 0.42) / 2, out), w.t, Z, w.n, (0.05, 0.57, 0.28), S["timber"], drop=(-Z,))
    c = w.p(u - 0.08, 0.47 + 0.09, out)
    box(mesh, c, w.t, Z, w.n, (0.36, 0.18, 0.26), S["wicker"], drop=(-Z,))
    # A ridge of loaves heaped along the basket.
    a, b = c + Z * 0.09 - w.t * 0.18, c + Z * 0.09 + w.t * 0.18
    ra, rb = a + Z * 0.08 + w.t * 0.06, b + Z * 0.08 - w.t * 0.06
    for s in (1, -1):
        e = w.n * (0.13 * s)
        mesh.face([a + e, b + e, rb, ra], S["bread"], Z + w.n * s)
    mesh.face([a - w.n * 0.13, a + w.n * 0.13, ra], S["bread"], -w.t)
    mesh.face([b + w.n * 0.13, b - w.n * 0.13, rb], S["bread"], w.t)


def build(lod, slots, mats):
    mesh = Mesh(slots)
    W = walls_of()
    z0 = PLINTH_TOP if lod < 2 else DROP  # walls start on the plinth ledge
    # Main front wall around the wing and the bay (the wall behind them is never seen).
    wall_rect(mesh, GY1, HW, z0, EAVE)
    wall_rect(mesh, -HW, GY0, z0, EAVE)
    wall_rect(mesh, GY0, GY - BAY_BACK, z0, JZ)
    wall_rect(mesh, GY + BAY_BACK, GY1, z0, JZ)
    mesh.face([(-HD, -HW, z0), (-HD, HW, z0), (-HD, HW, EAVE), (-HD, -HW, EAVE)], S["walls"], -X)
    for y, ny in ((HW, 1), (-HW, -1)):
        mesh.face([(HD, y, z0), (-HD, y, z0), (-HD, y, EAVE), (0.0, y, GABLE_TOP), (HD, y, EAVE)], S["walls"], Y * ny)
    if lod < 2:
        plinth(mesh)
    # Jettied wing: front gable pentagon, two short sides and the soffit under the jetty.
    xf = HD + JET
    mesh.face([(xf, GY0, JZ), (xf, GY1, JZ), (xf, GY1, EAVE), (xf, GY, WAPEX), (xf, GY0, EAVE)], S["walls"], X)
    for y, ny in ((GY0, -1), (GY1, 1)):
        mesh.face([(HD, y, JZ), (xf, y, JZ), (xf, y, EAVE), (HD, y, EAVE)], S["walls"], Y * ny)
    mesh.face([(HD, GY0, JZ), (HD, GY1, JZ), (xf, GY1, JZ), (xf, GY0, JZ)], S["walls"], -Z)

    main_roof(mesh, lod)
    zbw, ridge_w = wing_roof(mesh, lod)
    for cx, side, top in CHIMNEYS:
        chimney(mesh, cx, side, top, lod)
    bow_window(mesh, lod)
    shop_door(mesh, W["F"], lod)
    if lod < 2:
        canopy(mesh, W["F"], lod)
    for name, u, zs, ww, hh, major in WINDOWS:
        detail = lod if major else (1 if lod == 0 else 2)
        W[name].window(mesh, u, zs, ww, hh, WIN, detail, bars=True)
    if lod < 2:
        for name, u0, za, u1, zb, keep, width in TIMBERS:
            if lod == 0 or keep:
                beam(mesh, W[name], u0, za, u1, zb, width, 0.05, S["timber"])
        # Bargeboards along the wing's front verges.
        run = GHW + WOVER
        for s in (-1, 1):
            beam(mesh, W["WF"], GY + s * run, zbw - 0.05, GY, zbw + ridge_w - 0.05, 0.24, 0.05, S["timber"], out=WFRONT + 0.002)
    if lod == 0:
        bench(mesh, W["F"])
        basket(mesh, Vector((HD + BAY_DEPTH + 0.45, GY - 0.75, -0.1)), 0.2, 0.36)
    return mesh.obj(f"{P}_lod{lod}", mats)


def late_bench(lods):
    """LOD1 keeps the bench and the bread baskets (owner A2 Q4, 2026-10-21: every piece has a middle
    LOD): LOD0's wicker and bread faces and the bench's timber (face centres below 0.6 m and within
    1 m of a basket face), copied after the bake with their baked UVs (_lod_copy). Returns the joined
    LOD1 and the copied triangles."""
    return _lod_copy.copy_with_support(lods[0], lods[1], (S["wicker"], S["bread"]), S["timber"], 0.6, 1.0)


def late_drum(lods):
    """LOD2 keeps the drum basket of bread standing on the ground by the bay (0.46 m across, about
    1.5 px at 400 m), copied from LOD0 with its baked UVs after the LOD2 hull collider is built, so
    the collider is unchanged; the bench and its 0.36 m basket stay LOD1-only (under one pixel at
    400 m). Returns the joined LOD2 and the copied triangles."""
    return _lod_copy.copy_faces(lods[0], lods[2], lambda c, m: m in (S["wicker"], S["bread"]) and c.z < 0.4)


def unwrap(lods, display):
    """One atlas over every LOD (as lib.uv.atlas_unwrap). Before packing, each LOD's islands are
    scaled by LOD_UV_SCALE and LOD0's bow-window panes by DISPLAY_BOOST; packing keeps relative
    island scale."""
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
            boost = DISPLAY_BOOST if o is lods[0] and p.material_index in display else 1.0
            for li in p.loop_indices:
                uvl[li].uv = uvl[li].uv * (k * boost)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=True, margin_method="FRACTION", margin=UV_MARGIN)
    bpy.ops.object.mode_set(mode="OBJECT")


def density_by_slot(obj, size, slots):
    """Median px/m of the masks atlas per material slot (masks-only slots: reported, not gated)."""
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


def loaf_field(s, z):
    """The bread on the bay's shelves as (AO multiplier, night multiplier) at bay arc position s (m
    from the bay's centre line) and height z: seeded loaf ellipses sitting on each shelf in the loaf
    band, darker gaps between them, and the dim shop interior above."""
    rng = random.Random(P)
    a_mul = np.full(s.shape, 0.62, np.float32)
    b_mul = np.full(s.shape, 0.55, np.float32)
    row_h = (GLASS1 - GLASS0) / BAY_ROWS
    span = BAY_FRONT + math.hypot(BAY_DEPTH, BAY_BACK - BAY_FRONT)
    for row in range(BAY_ROWS):
        z0 = GLASS0 + row * row_h
        band = (z >= z0) & (z < z0 + LOAF_BAND)
        a_mul[band] = 0.45
        a_mul[(z >= z0) & (z < z0 + 0.03)] = 0.35  # the shelf edge
        n = int(2 * span / 0.24) + 1
        for i in range(n):
            ca = -span + 0.12 + 0.24 * i + rng.uniform(-0.035, 0.035)
            ha = rng.uniform(0.08, 0.115)
            hb = rng.uniform(0.07, 0.1)
            cz = z0 + 0.03 + hb
            e = ((s - ca) / ha) ** 2 + ((z - cz) / hb) ** 2
            inside = (e < 1.0) & (z < z0 + LOAF_BAND)
            dome = np.sqrt(np.clip(1.0 - e, 0.0, 1.0))
            lit = np.minimum(0.7 + 0.3 * dome * np.clip(0.5 + (z - cz) / (2 * hb), 0.0, 1.0) + 0.1, 1.0)
            a_mul = np.where(inside, lit, a_mul)
            b_mul = np.where(inside, 0.8 + 0.2 * dome, b_mul)
    return a_mul, b_mul


def tri_texels(obj, size, keep):
    """Texel indices (Blender pixel order) whose centres fall in obj's triangles of material indices
    `keep`, with each texel's world position."""
    m = obj.data
    m.calc_loop_triangles()
    uvl = m.uv_layers["UVMap"].data
    idx, pos = [], []
    for t in m.loop_triangles:
        if t.material_index not in keep:
            continue
        tri = np.array([uvl[li].uv[:] for li in t.loops]) * size
        co = np.array([m.vertices[v].co[:] for v in t.vertices])
        lo_, hi_ = np.floor(tri.min(0)).astype(int), np.ceil(tri.max(0)).astype(int)
        xs, ys = np.meshgrid(np.arange(lo_[0], hi_[0]) + 0.5, np.arange(lo_[1], hi_[1]) + 0.5)
        (ax, ay), (bx, by), (cx, cy) = tri
        d = (by - cy) * (ax - cx) + (cx - bx) * (ay - cy)
        if abs(d) < 1e-12:
            continue
        l1 = ((by - cy) * (xs - cx) + (cx - bx) * (ys - cy)) / d
        l2 = ((cy - ay) * (xs - cx) + (ax - cx) * (ys - cy)) / d
        l3 = 1.0 - l1 - l2
        inside = (l1 >= -1e-6) & (l2 >= -1e-6) & (l3 >= -1e-6)
        w = np.stack([l1[inside], l2[inside], l3[inside]], 1)
        idx.append(ys[inside].astype(int).clip(0, size - 1) * size + xs[inside].astype(int).clip(0, size - 1))
        pos.append(w @ co)
    return np.concatenate(idx), np.concatenate(pos)


def bay_arc(pos):
    """Arc position along the bay (m from its centre line) of points on its glazing."""
    side = math.hypot(BAY_DEPTH, BAY_BACK - BAY_FRONT)
    dy = pos[:, 1] - GY
    on_front = pos[:, 0] > HD + BAY_DEPTH - 1e-3
    along_side = BAY_FRONT + (HD + BAY_DEPTH - pos[:, 0]) / BAY_DEPTH * side
    return np.where(on_front, dy, np.sign(dy) * along_side)


def main():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
    skip_bake = os.environ.get("HEXW_SKIP_BAKE") == "1"
    scene.reset_scene(P)
    root = scene.empty(P)
    slots = list(S.values())
    mats = []
    for name, slot in S.items():
        col, rough = COLOURS[name]
        k = PLASTER_LIN if name in TILEABLE else 1.0
        mats.append(materials.pbr_material(slot, (col[0] * k, col[1] * k, col[2] * k, 1.0), rough))
    lods = [build(i, slots, mats) for i in range(3)]
    tris = [geom.tri_count(o) for o in lods]
    print(f"{P}: tris per LOD {tris} (LOD1 {tris[1] / tris[0]:.3f}, LOD2 {tris[2] / tris[0]:.3f})")
    budgets.check(tris, "prop", P)

    display = {slots.index(S["display_glass"]), slots.index(S["loaf_glass"])}
    unwrap(lods, display)
    signature = bake.receiver_signature(lods)
    density = {f"lod{i}": density_by_slot(o, MASK_SIZE, slots) for i, o in enumerate(lods)}
    print(f"{P}: masks px/m per slot {density}")
    stats = None
    if not skip_bake:
        bake.setup_cycles(samples=SAMPLES)
        scn = bpy.context.scene
        scn.world = scn.world or bpy.data.worlds.new(P)  # reset_scene leaves no world
        scn.world.light_settings.distance = AO_DIST
        ao = bake.new_image(f"{P}.ao", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_to(o, "AO", ao, margin=BAKE_MARGIN, clear=i == 0)
        ao_png = bake.save_png(ao, str(out / f"{P}.ao.tmp.png"))
        # Hooks through one EMIT bake: each material's base colour temporarily carries
        # (snow, porosity, night) and is restored afterwards.
        keep = {m.name: tuple(m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value) for m in mats}
        for name, m in zip(S, mats):
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (*HOOKS[name], 1.0)
        hooks = bake.new_image(f"{P}.hooks", MASK_SIZE, non_color=True)
        for i, o in enumerate(lods):
            bake.bake_emit_input([], o, "Base Color", hooks, margin=BAKE_MARGIN, clear=i == 0)
        hooks_png = bake.save_png(hooks, str(out / f"{P}.hooks.tmp.png"))
        for m in mats:
            m.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = keep[m.name]
        _, hk = bake.load_png(hooks_png)
        _, aop = bake.load_png(ao_png)
        os.remove(ao_png)
        os.remove(hooks_png)
        px = np.stack([hk[:, 0], hk[:, 1], hk[:, 2], aop[:, 0]], axis=1)
        # The loaves, authored over LOD0's baked bow-window texels (LOD1 keeps the colour bands only:
        # from 150 m a loaf is under two texels). The bake margin around the islands keeps the baked
        # values, so mips blend toward the plain glass.
        idx, pos = tri_texels(lods[0], MASK_SIZE, display)
        am, bm = loaf_field(bay_arc(pos), pos[:, 2])
        px[idx, 3] *= am
        px[idx, 2] *= bm
        px, filled = fill_unbaked(px, lods, MASK_SIZE, [HOOKS[n] for n in S])
        masks_png = bake.write_png(MASK_SIZE, px, str(out / f"{P}.masks.png"))
        _, px = bake.load_png(masks_png)
        covered = px[:, 1] > 0.01  # every slot has porosity >= 0.05; empty texels stay 0
        av = px[covered, 3]
        samples = centroid_samples(lods, px, MASK_SIZE, slots, P)
        stats = {"aoMean": float(av.mean()), "aoP05": float(np.percentile(av, 5)),
                 "nightTexels": int((px[:, 2] > 0.5).sum()), "coverage": float(covered.mean()),
                 "loafTexels": int(np.unique(idx).size), "filledTexels": filled, "centroids": samples}
        print(f"{P}: masks {stats}")
        bad = {k: v["unbakedTris"] for k, v in samples.items() if v["unbakedTris"]}
        assert not bad, f"triangles on unbaked masks texels: {bad}"

    lods[1], late = late_bench(lods)
    tris[1] += late
    print(f"{P}: LOD1 + bench and baskets {late} tris -> tris per LOD {tris}")
    budgets.check(tris, "prop", P)
    for o in lods:
        o.data.shade_flat()
        o.parent = root
    col = collider.collider_from(lods[2], "hull", f"{P}_collider")
    col.parent = root
    lods[2], late = late_drum(lods)
    tris[2] += late
    print(f"{P}: LOD2 + drum basket {late} tris -> tris per LOD {tris}")
    budgets.check(tris, "prop", P)
    export.export_glb(root, str(out / f"{P}.glb"))
    export.write_export_report(str(out / "export-report.json"), root, P, __file__)
    bake.write_bake_report(str(out / "bake-report.json"), receiverSignature=signature,
                           device=os.environ.get("HEXW_BAKE_DEVICE", "CPU"), samples=SAMPLES, marginPx=BAKE_MARGIN,
                           aoDistanceM=AO_DIST, channels={"masks": {"size": MASK_SIZE, "colorspace": "linear"}},
                           texelDensityPxPerM=uv.measure_texel_density(lods[0], MASK_SIZE),
                           masksDensityBySlot=density, lodUvScale=list(LOD_UV_SCALE), displayBoost=DISPLAY_BOOST,
                           maskStats=stats, skipped=skip_bake)


main()
