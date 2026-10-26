"""Bakery family, twin-gabled variant: the builder behind bakery_shop_b.py (PRD §6 Variety, the bakery
tile's hero slot; owner-approved refs assets-source/refs/bakery, A17).

A timber-framed village bakery after ref-1's twin front gables: cream lime render between dark oak
framing, a hipped brown clay-tile roof on low eaves (3.5 m) broken at the front by two gables with
casements, a broad brick bread-oven stack astride the ridge with three clay pots, a flat-fronted shop
window of shelved loaves between oak pilasters under a plain oak fascia beam, one long clay-tile
pentice on brackets over the window and the half-glazed door, and the bench and bread basket of
bakery_shop. Against bakery_shop (one long 50° gable roof on 4.4 m eaves, one jettied cross wing over a
bow window, two external gable-end stacks) it reads from above as a short-ridged hip with two front
gables and one central stack, and at eye level as a lower, symmetric-roofed shop with a continuous
tiled pentice along its front.

Plan (Blender x deep, y wide, the shop front faces +x as bakery_shop): walls 5.0 x 7.0 m (x ±2.5,
y ±3.5), bakery_shop's main block; the hip overhangs 0.3 m all round (x ±2.8, y ±3.8); gables 2.3 m wide
over the window (y -1.45) and the door (y 1.92); the pentice 0.6 m deep. Slot fit (data/tiles/bakery.ron
hero 0 at (-4.1, -8.5), yaw 90): LOD2, and so the box collider, stays inside bakery_shop's collider box
(scatter::GROUND_BOXES, Blender x -2.8..3.15, y ±4.0), LOD0 and LOD1 inside its LOD0 extents (x
-2.831..3.73, y ±4.1, measured on its GLB), and every vertex below 2.7 m inside hex-sim
tests/parking.rs's BAKERY boxes for bakery_shop plus one added box for this piece's shop front and
pentice (`ENVELOPE`), so the parking sweep covers this piece too: `envelope_errors` asserts it.

Pipeline: bakery_shop.py is a monolithic generator (it runs main() on import), so this family drives the
house family's pipeline, `_house_longhouse.build_piece`, through its module globals (as _house_slate.py
and _barn_b.py do) and sets them to bakery_shop's bake settings: `build` is this module's builder, the
fit bounds are bakery_shop's, UV_MARGIN 0.006 with a 2 px bake margin, `uv` is a shim whose
atlas_unwrap is bakery_shop's unwrap (each LOD's islands scaled by LOD_UV_SCALE and the shop panes by
DISPLAY_BOOST before a FRACTION pack), and `fill_unbaked` first writes the loaves into LOD0's shop-pane
masks (bakery_shop's loaf field on a flat front) and then runs _house.fill_unbaked. The masks bake
(1024², each LOD on its own islands), the zero-unbaked-triangle assert, budgets.check, the LOD2 box
collider and the export are the family's, and no approved helper or generator is edited. bakery_shop's
monolithic geometry steps used here (door, bench, basket, struts, the loaf field) are re-derived for
this plan (project.md, Variants).

LODs are rebuilt from the same parameters, never decimated: LOD1 drops the tile courses, the glazing
bars, minor timbers and the brackets; LOD2 is the wall shell, plain roofs, the stack, the pentice slab
and flat glass."""
import math
import random
import types

import bmesh
import numpy as np
from mathutils import Matrix, Vector

import _house
import _house_longhouse as lh
import _lod_copy
from _house import DROP, PLINTH_TOP, Z, Mesh, Wall, roof_profile
from _house_longhouse import beam, box, plinth, sweep
from lib import budgets, collider, geom, scene, uv
import bpy

X, Y = Vector((1.0, 0.0, 0.0)), Vector((0.0, 1.0, 0.0))
assert DROP == -0.35 and PLINTH_TOP == 0.32, (DROP, PLINTH_TOP)

# bakery_shop's bake settings (bakery_shop.py UV_MARGIN, BAKE_MARGIN, LOD_UV_SCALE, DISPLAY_BOOST).
UV_MARGIN = 0.006
BAKE_MARGIN = 2
LOD_UV_SCALE = (1.0, 0.55, 0.35)
DISPLAY_BOOST = 2.5

# Main block and hip roof.
HD, HW, EAVE = 2.5, 3.5, 3.5
PITCH, OVER, COURSES, KICK, THICK = math.radians(52.0), 0.3, 11, 0.035, 0.14
HIP_CAP_FROM = 0.15  # the hip caps start this far in from the eave corners (inside the LOD0 bounds)
# The back-left eave corner (ring index 3, Blender (-x, -y)) is the vertex nearest the hex border in the
# bakery slot. On LOD0's kicked eave ring it slides KICK·SP up its hip line, staying on both kicked
# planes, so in plan it sits on LOD1/LOD2's corner (-A, -B), BORDER_MIN from the border (owner A2,
# 2026-10-20: as bakery_shop's 0.12 m; `border_clearance` asserts it per LOD).
BORDER_CORNER, BORDER_MIN = 3, 0.12
A, B = HD + OVER, HW + OVER  # eave rectangle half sizes
TP, SP, CP = math.tan(PITCH), math.sin(PITCH), math.cos(PITCH)
ZB = EAVE - OVER * TP  # eave edge height
RIDGE_Z = ZB + A * TP  # ridge from y -(B - A) to B - A
SOFFIT_Z = EAVE - THICK / CP  # where the roof's underside meets the walls
SOLE, RAIL, BRESS = PLINTH_TOP + 0.08, 2.45, 2.95
# Front gables: (centre y, half width); their eaves sit on the main eave line at the wall.
GABLES = ((-1.45, 1.15), (1.92, 1.15))
WPITCH, WOVER, WFRONT, WCOURSES = math.radians(58.0), 0.2, 0.3, 8
XF = HD + WFRONT  # the gables' front verge
ZBG = EAVE - WOVER * math.tan(WPITCH)
OVERLAP = 0.08  # how far each gable roof vertex ends inside the main front slope (m, horizontal)
# Shop window: flat front between pilasters, a stallboard below, an oak fascia beam above.
SF_Y0, SF_Y1, SF_OUT = -3.1, 0.2, 0.1
STALL_TOP, GLASS0, GLASS1, FASCIA_TOP = 0.62, 0.68, 2.18, 2.42
SF_ROWS, LOAF_BAND = 3, 0.26
SF_COLS = 6
# Shop door (as bakery_shop: DOOR_U 1.92, 0.95 x 2.15) under the pentice.
DOOR_U, DOOR_W, DOOR_H = 1.92, 0.95, 2.15
# Pentice: a clay-tile lean-to from the wall at PENT_Z out 0.6 m at 22°, over y PENT_Y0..PENT_Y1.
PENT_Y0, PENT_Y1, PENT_Z, PENT_RUN, PENT_PITCH = -3.42, 3.42, 2.82, 0.6, math.radians(22.0)
PENT_BRACKETS = (SF_Y0 - 0.06, SF_Y1 + 0.06, DOOR_U - 0.62, DOOR_U + 0.62, 3.32)
# Bread-oven stack astride the ridge: centre (x, y), size (x, y), top.
STACK, STACK_SIZE, STACK_TOP = (-0.1, 0.0), (0.8, 1.5), 7.75

# bakery_shop's fit (Blender x, y): its collider box (scatter::GROUND_BOXES, the LOD2 hull's bounds) and
# its LOD0 extents, both measured on assets/kit/p1/bakery_shop/bakery_shop.glb.
BAKERY_BOX = ((-2.8, 3.15), (-4.0, 4.0))
BAKERY_LOD0_BOUNDS = ((-2.831, 3.73), (-4.1, 4.1))
# hex-sim tests/parking.rs BAKERY obstacle boxes of bakery_shop in this frame (tile (x, z) =
# (-4.1 + y, -8.5 + x)): (x0, x1, y0, y1). Every vertex below ENVELOPE_Z lies in one of them; the last
# box is this piece's shop front and pentice, added to BAKERY with this piece.
ENVELOPE_Z = 2.7
ENVELOPE = [
    ("shop body", (-2.56, 2.56, -3.56, 3.56)),
    ("chimney west", (-1.45, 0.65, -4.1, -3.5)),
    ("chimney east", (-1.45, 0.65, 3.5, 4.1)),
    ("bow window", (2.5, 3.11, -2.95, 0.05)),
    ("basket", (3.27, 3.73, -2.43, -1.97)),
    ("bench", (2.64, 2.96, 0.5, 1.22)),
    ("doorstep", (2.5, 2.94, 1.245, 2.595)),
    ("door canopy", (2.5, 3.3, 1.17, 2.67)),
    ("shop front and pentice (bakery_shop_b)", (2.5, 3.13, -3.45, 3.45)),
]

BUILT = []  # names of the LOD objects this builder made (checked after lh.build_piece returns)


def walls_of():
    return {"F": Wall((1, 0, 0), (HD, 0, 0)), "B": Wall((-1, 0, 0), (-HD, 0, 0)),
            "L": Wall((0, -1, 0), (0, -HW, 0)), "R": Wall((0, 1, 0), (0, HW, 0))}


def ring(t, off):
    """Corners (counter-clockwise from (+x, -y)) of the hip roof's section at horizontal inset t from the
    eave edge, on the roof surface offset `off` along its normal (KICK for course butts, -THICK for the
    underside). Adjacent faces share these corners, so the offsets meet exactly on the hip lines."""
    ax, ay = A - t + off * SP, B - t + off * SP
    z = ZB + t * TP + off * CP
    return [Vector((ax, -ay, z)), Vector((ax, ay, z)), Vector((-ax, ay, z)), Vector((-ax, -ay, z))]


def eave_ring(off):
    """ring(0, off) with the border corner pulled in to (-A, -B) in plan when the eave is kicked."""
    r = ring(0.0, off)
    if off > 0:
        r[BORDER_CORNER] = ring(off * SP, off)[BORDER_CORNER]
    return r


def side_normal(i):
    d = (X, Y, -X, -Y)[i]
    return d * SP + Z * CP


def strip(mesh, p0, p1, width, rise, slot):
    """A ridge or hip cap from p0 to p1: two faces meeting in a crest `rise` above the line."""
    h = (p1 - p0).normalized()
    side = h.cross(Z).normalized()
    up = side.cross(h).normalized()
    if up.z < 0:
        up = -up
    a0, a1 = p0 - side * width, p1 - side * width
    b0, b1 = p0 + side * width, p1 + side * width
    c0, c1 = p0 + up * rise, p1 + up * rise
    mesh.face([a0, a1, c1, c0], slot, up - side)
    mesh.face([c0, c1, b1, b0], slot, up + side)


def hip_roof(mesh, s, lod):
    """The hipped main roof: tile courses (LOD0) or plain faces on all four sides, the eave fascia, the
    soffit ring back to the walls, and (LOD0/1) hip and ridge caps. No underside over the attic."""
    roof = s["roof"]
    n = COURSES if lod == 0 else 1
    kick = KICK if lod == 0 else 0.0
    for k in range(n):
        t0, t1 = A * k / n, A * (k + 1) / n
        lo, hi = eave_ring(kick) if k == 0 else ring(t0, kick), ring(t1, 0.0)
        for i in range(4):
            j = (i + 1) % 4
            nrm = side_normal(i)
            if (hi[i] - hi[j]).length < 1e-6:
                mesh.face([lo[i], lo[j], hi[i]], roof, nrm)
            else:
                mesh.face([lo[i], lo[j], hi[j], hi[i]], roof, nrm)
            if k < n - 1:
                # The course butt: a step facing down the slope.
                up = ring(t1, kick)
                d = (X, Y, -X, -Y)[i]
                mesh.face([hi[i], hi[j], up[j], up[i]], roof, d * CP - Z * SP)
    top = eave_ring(kick)
    bot = ring(0.0, -THICK)
    wall = [Vector((HD, -HW, SOFFIT_Z)), Vector((HD, HW, SOFFIT_Z)), Vector((-HD, HW, SOFFIT_Z)),
            Vector((-HD, -HW, SOFFIT_Z))]
    for i in range(4):
        j = (i + 1) % 4
        d = (X, Y, -X, -Y)[i]
        mesh.face([bot[i], bot[j], top[j], top[i]], roof, d * CP - Z * SP)
        mesh.face([bot[j], bot[i], wall[i], wall[j]], roof, -Z)
    if lod < 2:
        lift = Z * (KICK if lod == 0 else 0.01)
        crest = [p + lift for p in ring(A, 0.0)]
        eaves = [p + lift for p in ring(HIP_CAP_FROM, kick)]
        for i in range(4):
            strip(mesh, eaves[i], crest[i], 0.09, 0.06, roof)
        strip(mesh, crest[3] - Y * 0.04, crest[2] + Y * 0.04, 0.11, 0.07, roof)


def gable_roof(mesh, s, gy, ghw, lod):
    """A front gable's roof swept along +x from inside the main front slope to its verge XF: each
    profile vertex starts OVERLAP inside the main slope's plane, so the two roofs meet in clean valleys.
    Profile x maps to -y (bakery_shop's cross wing frame), so the sweep winds outward."""
    pts, ridge = roof_profile(ghw, WPITCH, WOVER, WCOURSES if lod == 0 else 0, KICK, THICK)
    starts = [A - (ZBG + z - ZB) / TP - OVERLAP for _, z in pts]
    assert max(starts) < XF - 0.05, starts
    sweep(mesh, s["roof"], pts, lambda x, z, t: (t, gy - x, ZBG + z), starts, XF, (False, True))
    return ridge


def strut(mesh, a, b, side, width, depth, slot):
    """A member from a to b with its four long faces (no end caps): `side` is the width axis."""
    t = (b - a).normalized()
    n = t.cross(side).normalized()
    box(mesh, (a + b) / 2, t, side.normalized(), n, ((b - a).length, width, depth), slot, drop=(t, -t))


def shop_front(mesh, s, w, lod):
    """The flat shop window: stallboard and sill, shelved panes (each row a loaf band under a darker
    interior band), oak pilasters, the fascia beam and (LOD0) glazing bars."""
    span = SF_Y1 - SF_Y0
    mid = (SF_Y0 + SF_Y1) / 2
    g = SF_OUT
    if lod == 2:
        mesh.face([w.p(SF_Y0, DROP, g), w.p(SF_Y1, DROP, g), w.p(SF_Y1, GLASS0, g), w.p(SF_Y0, GLASS0, g)], s["brick"], X)
        mesh.face([w.p(SF_Y0, GLASS0, g), w.p(SF_Y1, GLASS0, g), w.p(SF_Y1, GLASS1, g), w.p(SF_Y0, GLASS1, g)], s["loaf_glass"], X)
        mesh.face([w.p(SF_Y0, GLASS1, g), w.p(SF_Y1, GLASS1, g), w.p(SF_Y1, FASCIA_TOP, g), w.p(SF_Y0, FASCIA_TOP, g)], s["timber"], X)
        for y, ny in ((SF_Y0, -1), (SF_Y1, 1)):
            mesh.face([w.p(y, DROP, 0.0), w.p(y, DROP, g), w.p(y, FASCIA_TOP, g), w.p(y, FASCIA_TOP, 0.0)], s["timber"], Y * ny)
        mesh.face([w.p(SF_Y0, FASCIA_TOP, 0.0), w.p(SF_Y1, FASCIA_TOP, 0.0), w.p(SF_Y1, FASCIA_TOP, g),
                   w.p(SF_Y0, FASCIA_TOP, g)], s["timber"], Z)
        return
    box(mesh, w.p(mid, (PLINTH_TOP + STALL_TOP) / 2, 0.07), Y, Z, X, (span, STALL_TOP - PLINTH_TOP, 0.14), s["brick"],
        drop=(-X, -Z))
    box(mesh, w.p(mid, (STALL_TOP + GLASS0) / 2, 0.09), Y, Z, X, (span, GLASS0 - STALL_TOP, 0.18), s["frame"], drop=(-X, -Z))
    row_h = (GLASS1 - GLASS0) / SF_ROWS
    for r in range(SF_ROWS):
        z0 = GLASS0 + r * row_h
        for za, zb, slot in ((z0, z0 + LOAF_BAND, s["loaf_glass"]), (z0 + LOAF_BAND, z0 + row_h, s["display_glass"])):
            mesh.face([w.p(SF_Y0, za, g), w.p(SF_Y1, za, g), w.p(SF_Y1, zb, g), w.p(SF_Y0, zb, g)], slot, X)
    for y in (SF_Y0 - 0.06, SF_Y1 + 0.06):
        box(mesh, w.p(y, (PLINTH_TOP + GLASS1) / 2, 0.09), Y, Z, X, (0.12, GLASS1 - PLINTH_TOP, 0.18), s["timber"], drop=(-X, -Z))
    box(mesh, w.p(mid, (GLASS1 + FASCIA_TOP) / 2, 0.1), Y, Z, X, (span + 0.24, FASCIA_TOP - GLASS1, 0.2), s["timber"], drop=(-X,))
    if lod == 0:
        # Glazing bars on the front only, 2 cm proud of the glass.
        gw = Wall((1, 0, 0), (HD + g, 0, 0))
        for k in range(1, SF_COLS):
            y = SF_Y0 + span * k / SF_COLS
            beam(mesh, gw, y, GLASS0, y, GLASS1, 0.035, 0.02, s["frame"], sides=False)
        for k in range(1, SF_ROWS):
            z = GLASS0 + row_h * k
            beam(mesh, gw, SF_Y0, z, SF_Y1, z, 0.035, 0.021, s["frame"], sides=False)


def shop_door(mesh, s, w, lod):
    """bakery_shop's half-glazed shop door, its frame and its stone step (re-derived)."""
    u0, u1, g = DOOR_U - DOOR_W / 2, DOOR_U + DOOR_W / 2, 0.03
    mesh.face([w.p(u0, PLINTH_TOP, g), w.p(u1, PLINTH_TOP, g), w.p(u1, DOOR_H, g), w.p(u0, DOOR_H, g)], s["door"], w.n)
    gu0, gu1, gz0, gz1 = u0 + 0.12, u1 - 0.12, DOOR_H * 0.5 + 0.05, DOOR_H - 0.12
    mesh.face([w.p(gu0, gz0, g + 0.004), w.p(gu1, gz0, g + 0.004), w.p(gu1, gz1, g + 0.004),
               w.p(gu0, gz1, g + 0.004)], s["glass"], w.n)
    if lod == 0:
        beam(mesh, w, DOOR_U, gz0, DOOR_U, gz1, 0.03, 0.012, s["door"], out=g, sides=False)
        for k in (1, 2):
            z = gz0 + (gz1 - gz0) * k / 3
            beam(mesh, w, gu0, z, gu1, z, 0.03, 0.013, s["door"], out=g, sides=False)
    if lod < 2:
        for u in (u0 - 0.05, u1 + 0.05):
            beam(mesh, w, u, PLINTH_TOP, u, DOOR_H, 0.1, 0.07, s["frame"])
        beam(mesh, w, u0 - 0.1, DOOR_H + 0.05, u1 + 0.1, DOOR_H + 0.05, 0.1, 0.07, s["frame"])
        box(mesh, w.p(DOOR_U, (DROP + 0.18) / 2, 0.22), w.t, Z, w.n, (DOOR_W + 0.4, 0.18 - DROP, 0.44),
            s["stone"], drop=(-w.n, -Z))


def pentice(mesh, s, w, lod):
    """One clay-tile lean-to along the front over the window and the door: two courses on brackets at
    LOD0, one slab at LOD1 and LOD2."""
    slope = (w.n * math.cos(PENT_PITCH) - Z * math.sin(PENT_PITCH)).normalized()
    up = slope.cross(w.t)
    length = PENT_RUN / math.cos(PENT_PITCH)
    mid = (PENT_Y0 + PENT_Y1) / 2
    width = PENT_Y1 - PENT_Y0
    base = w.p(mid, PENT_Z, 0.0)
    if lod == 0:
        for k, (s0, s1, lift) in enumerate(((0.0, 0.36 * length / 0.66, 0.0), (0.3 * length / 0.66, length, -0.035))):
            c = base + slope * ((s0 + s1) / 2) + up * (0.035 + lift)
            box(mesh, c, w.t, up, slope, (width, 0.07, s1 - s0), s["roof"], drop=(-slope,) if k == 0 else ())
        for u in PENT_BRACKETS:
            strut(mesh, w.p(u, PENT_Z - 0.62, 0.0), w.p(u, PENT_Z - 0.2, 0.42), w.t, 0.08, 0.08, s["timber"])
    else:
        box(mesh, base + slope * (length / 2), w.t, up, slope, (width, 0.08, length), s["roof"], drop=(-slope,))


def stack(mesh, s, lod):
    """The bread-oven flue: a broad brick stack astride the ridge with an oversail course and three clay
    pots; its foot sits inside the roof, below both slopes at its faces."""
    (cx, cy), (sx, sy) = STACK, STACK_SIZE
    foot = ZB + (A - (abs(cx) + sx / 2)) * TP - 0.3
    box(mesh, (cx, cy, (foot + STACK_TOP) / 2), X, Y, Z, (sx, sy, STACK_TOP - foot), s["brick"], drop=(-Z,))
    if lod == 2:
        return
    box(mesh, (cx, cy, STACK_TOP + 0.06), X, Y, Z, (sx + 0.14, sy + 0.14, 0.12), s["brick"], drop=(-Z,))
    for py in ((-0.45, 0.0, 0.45) if lod == 0 else (0.0,)):
        res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=6 if lod == 0 else 5, radius1=0.13,
                                    radius2=0.105, depth=0.42, matrix=Matrix.Translation((cx, cy + py, STACK_TOP + 0.33)))
        for f in list({f for v in res["verts"] for f in v.link_faces}):
            f.normal_update()
            if f.normal.z < -0.9:
                mesh.bm.faces.remove(f)
            else:
                f.material_index = mesh.idx[s["roof"]]


def basket(mesh, s, c, r, h):
    """bakery_shop's wicker basket heaped with bread (re-derived)."""
    res = bmesh.ops.create_cone(mesh.bm, cap_ends=False, segments=8, radius1=r, radius2=r * 1.15, depth=h,
                                matrix=Matrix.Translation(c + Z * (h / 2)))
    for f in {f for v in res["verts"] for f in v.link_faces}:
        f.material_index = mesh.idx[s["wicker"]]
    res = bmesh.ops.create_cone(mesh.bm, cap_ends=True, segments=8, radius1=r * 1.15, radius2=0.03, depth=0.12,
                                matrix=Matrix.Translation(c + Z * (h + 0.06)))
    for f in {f for v in res["verts"] for f in v.link_faces}:
        f.material_index = mesh.idx[s["bread"]]


def bench(mesh, s, w):
    """bakery_shop's bench between the window and the door with a basket of loaves on it (re-derived,
    same spot: the parking test's bench box)."""
    u, out = 0.86, 0.3
    box(mesh, w.p(u, 0.445, out), w.t, Z, w.n, (0.72, 0.05, 0.32), s["timber"])
    for du in (-0.28, 0.28):
        box(mesh, w.p(u + du, (-0.15 + 0.42) / 2, out), w.t, Z, w.n, (0.05, 0.57, 0.28), s["timber"], drop=(-Z,))
    c = w.p(u - 0.08, 0.47 + 0.09, out)
    box(mesh, c, w.t, Z, w.n, (0.36, 0.18, 0.26), s["wicker"], drop=(-Z,))
    a, b = c + Z * 0.09 - w.t * 0.18, c + Z * 0.09 + w.t * 0.18
    ra, rb = a + Z * 0.08 + w.t * 0.06, b + Z * 0.08 - w.t * 0.06
    for sg in (1, -1):
        e = w.n * (0.13 * sg)
        mesh.face([a + e, b + e, rb, ra], s["bread"], Z + w.n * sg)
    mesh.face([a - w.n * 0.13, a + w.n * 0.13, ra], s["bread"], -w.t)
    mesh.face([b + w.n * 0.13, b - w.n * 0.13, rb], s["bread"], w.t)


# Framing: (wall, u0, z0, u1, z1, kept at LOD1, width). Front u runs along +y, back along -y, the left
# end (y -3.5) along +x, the right end (y 3.5) along -x. Rails run 5 cm past a corner so the members on
# the two walls close it.
TIMBERS = [
    ("F", -3.42, SOLE, -3.42, BRESS, True, 0.16), ("F", 3.42, SOLE, 3.42, BRESS, True, 0.16),
    ("F", -HW - 0.05, BRESS, HW + 0.05, BRESS, True, 0.2),
    ("F", SF_Y1 + 0.12, SOLE, DOOR_U - DOOR_W / 2 - 0.1, SOLE, True, 0.15),
    ("F", DOOR_U + DOOR_W / 2 + 0.1, SOLE, HW + 0.05, SOLE, True, 0.15),
    ("F", 2.95, SOLE, 2.95, BRESS - 0.1, False, 0.13),
    ("F", 0.45, SOLE + 0.08, 1.2, PENT_Z - 0.3, False, 0.12),
    ("F", 3.34, SOLE + 0.08, 3.03, 1.6, False, 0.12),
    ("B", -HW - 0.05, SOLE, HW + 0.05, SOLE, True, 0.15), ("B", -HW - 0.05, RAIL, HW + 0.05, RAIL, True, 0.15),
    ("B", -3.42, SOLE, -3.42, SOFFIT_Z, True, 0.16), ("B", 3.42, SOLE, 3.42, SOFFIT_Z, True, 0.16),
    ("B", 0.0, SOLE, 0.0, SOFFIT_Z, True, 0.15),
    ("B", -0.85, SOLE + 0.08, -0.1, RAIL - 0.08, False, 0.12), ("B", 0.85, SOLE + 0.08, 0.1, RAIL - 0.08, False, 0.12),
    ("B", -3.3, RAIL + 0.08, -2.55, SOFFIT_Z - 0.04, False, 0.12), ("B", 3.3, RAIL + 0.08, 2.55, SOFFIT_Z - 0.04, False, 0.12),
    ("L", -HD - 0.05, SOLE, HD + 0.05, SOLE, True, 0.15), ("L", -HD - 0.05, RAIL, HD + 0.05, RAIL, True, 0.15),
    ("L", -2.42, SOLE, -2.42, SOFFIT_Z, True, 0.16), ("L", 2.42, SOLE, 2.42, SOFFIT_Z, True, 0.16),
    ("L", -0.15, SOLE, -0.15, RAIL, False, 0.13), ("L", -2.3, SOLE + 0.08, -1.0, RAIL - 0.08, False, 0.12),
    ("R", -HD - 0.05, SOLE, HD + 0.05, SOLE, True, 0.15), ("R", -HD - 0.05, RAIL, HD + 0.05, RAIL, True, 0.15),
    ("R", -2.42, SOLE, -2.42, SOFFIT_Z, True, 0.16), ("R", 2.42, SOLE, 2.42, SOFFIT_Z, True, 0.16),
    ("R", 0.55, SOLE, 0.55, RAIL, False, 0.13), ("R", 2.3, SOLE + 0.08, 0.95, RAIL - 0.08, False, 0.12),
    ("R", 0.0, RAIL, 0.0, SOFFIT_Z, False, 0.13),
]
# Casements: (wall, u, sill z, width, height, major). Major windows keep their frame at LOD1 and their
# sill and bars at LOD0; the rest are frame-only at LOD0 (no sill, so the right end stays inside the
# parking test's shop body) and glass-only from LOD1.
WINDOWS = [("F", GABLES[0][0], 3.72, 0.72, 0.68, True), ("F", GABLES[1][0], 3.72, 0.72, 0.68, True),
           ("B", -1.6, 0.95, 0.9, 1.0, False), ("B", 1.6, 0.95, 0.9, 1.0, False),
           ("B", -1.6, 2.62, 0.8, 0.45, False), ("B", 1.6, 2.62, 0.8, 0.45, False),
           ("L", 1.1, 0.95, 0.8, 1.0, False), ("L", -1.0, 2.62, 0.7, 0.45, False),
           ("R", -1.3, 0.95, 0.8, 1.0, False)]


def gable_timbers(mesh, s, w, gy, ghw):
    """A front gable's framing: a tie beam over the main eave, a king post above the casement and two
    braces, all inside the gable triangle."""
    beam(mesh, w, gy - ghw + 0.02, EAVE + 0.04, gy + ghw - 0.02, EAVE + 0.04, 0.14, 0.05, s["timber"])
    apex = EAVE + ghw * math.tan(WPITCH) - 0.05
    beam(mesh, w, gy, 4.5, gy, apex - 0.3, 0.12, 0.05, s["timber"])
    for sg in (-1, 1):
        beam(mesh, w, gy + sg * 0.95, EAVE + 0.14, gy + sg * 0.6, 4.2, 0.11, 0.05, s["timber"])


def envelope_errors(o):
    """Vertices below ENVELOPE_Z outside every ENVELOPE box (module docstring)."""
    bad = []
    for v in o.data.vertices:
        x, y, z = v.co
        if z >= ENVELOPE_Z:
            continue
        if not any(x0 - 0.005 <= x <= x1 + 0.005 and y0 - 0.005 <= y <= y1 + 0.005 for _, (x0, x1, y0, y1) in ENVELOPE):
            bad.append((round(x, 3), round(y, 3), round(z, 3)))
    return bad


def border_clearance(o):
    """The smallest distance (m) from o's vertices to the hex border in the bakery slot (tile (x, z) =
    (-4.1 + y, -8.5 + x), as ENVELOPE), with the vertex."""
    apothem = 8.0 * math.sqrt(3.0)
    normals = [(math.cos(k * math.pi / 3), math.sin(k * math.pi / 3)) for k in range(6)]
    return min((apothem - max((-4.1 + v.co.y) * nx + (-8.5 + v.co.x) * nz for nx, nz in normals),
                tuple(round(c, 4) for c in v.co)) for v in o.data.vertices)


def build(spec, lod, slots, mats):
    s = spec["slots"]
    mesh = Mesh(slots)
    W = walls_of()
    z0 = PLINTH_TOP if lod < 2 else DROP  # walls start on the plinth ledge
    walls = s["walls"]
    # Walls to the eave; the front around the shop window (the wall behind its glass is never seen).
    if lod < 2:
        mesh.face([(HD, -HW, z0), (HD, SF_Y0, z0), (HD, SF_Y0, EAVE), (HD, -HW, EAVE)], walls, X)
        mesh.face([(HD, SF_Y1, z0), (HD, HW, z0), (HD, HW, EAVE), (HD, SF_Y1, EAVE)], walls, X)
        mesh.face([(HD, SF_Y0, GLASS1), (HD, SF_Y1, GLASS1), (HD, SF_Y1, EAVE), (HD, SF_Y0, EAVE)], walls, X)
    else:
        mesh.face([(HD, -HW, z0), (HD, SF_Y0, z0), (HD, SF_Y0, EAVE), (HD, -HW, EAVE)], walls, X)
        mesh.face([(HD, SF_Y1, z0), (HD, HW, z0), (HD, HW, EAVE), (HD, SF_Y1, EAVE)], walls, X)
        mesh.face([(HD, SF_Y0, FASCIA_TOP), (HD, SF_Y1, FASCIA_TOP), (HD, SF_Y1, EAVE), (HD, SF_Y0, EAVE)], walls, X)
    mesh.face([(-HD, -HW, z0), (-HD, HW, z0), (-HD, HW, EAVE), (-HD, -HW, EAVE)], walls, -X)
    for y, ny in ((HW, 1), (-HW, -1)):
        mesh.face([(HD, y, z0), (-HD, y, z0), (-HD, y, EAVE), (HD, y, EAVE)], walls, Y * ny)
    for gy, ghw in GABLES:
        apex = EAVE + ghw * math.tan(WPITCH) - 0.05
        mesh.face([(HD, gy - ghw, EAVE), (HD, gy + ghw, EAVE), (HD, gy, apex)], walls, X)
    if lod < 2:
        plinth(mesh, [(HD, -HW), (HD, HW), (-HD, HW), (-HD, -HW)], s["stone"])

    hip_roof(mesh, s, lod)
    ridges = [gable_roof(mesh, s, gy, ghw, lod) for gy, ghw in GABLES]
    assert all(ZBG + r < RIDGE_Z - 0.3 for r in ridges), (ridges, RIDGE_Z)
    stack(mesh, s, lod)
    shop_front(mesh, s, W["F"], lod)
    shop_door(mesh, s, W["F"], lod)
    pentice(mesh, s, W["F"], lod)
    for name, u, zs, ww, hh, major in WINDOWS:
        detail = lod if major else (1 if lod == 0 else 2)
        W[name].window(mesh, u, zs, ww, hh, s, detail, bars=True)
    if lod < 2:
        for name, u0, za, u1, zb, keep, width in TIMBERS:
            if lod == 0 or keep:
                beam(mesh, W[name], u0, za, u1, zb, width, 0.05, s["timber"])
        for (gy, ghw), ridge in zip(GABLES, ridges):
            if lod == 0:
                gable_timbers(mesh, s, W["F"], gy, ghw)
            run = ghw + WOVER
            for sg in (-1, 1):
                beam(mesh, W["F"], gy + sg * run, ZBG - 0.05, gy, ZBG + ridge - 0.05, 0.22, 0.05, s["timber"],
                     out=WFRONT + 0.002)
    if lod == 0:
        bench(mesh, s, W["F"])
        basket(mesh, s, Vector((3.5, -2.2, -0.1)), 0.2, 0.36)
    o = mesh.obj(f"{spec['piece']}_lod{lod}", mats)
    bad = envelope_errors(o)
    assert not bad, f"{o.name}: {len(bad)} vertices below {ENVELOPE_Z} m outside the parking envelope, e.g. {bad[:6]}"
    gap, at = border_clearance(o)
    print(f"bakery_b: {o.name} border clearance {gap:.4f} m at {at}")
    assert gap >= BORDER_MIN, f"{o.name}: a vertex sits {gap:.4f} m from the hex border at {at} (< {BORDER_MIN})"
    BUILT.append(o.name)
    return o


def unwrap(lods, margin):
    """bakery_shop's unwrap: one smart-project atlas over every LOD, each LOD's islands scaled by
    LOD_UV_SCALE and LOD0's shop panes by DISPLAY_BOOST before a FRACTION pack (packing keeps relative
    island scale), so LOD0's visible walls keep bakery_shop's masks density."""
    display = {i for i, m in enumerate(lods[0].data.materials) if m.name.endswith(("_display_glass", "_loaf_glass"))}
    assert len(display) == 2, display
    scene.select_only(lods)
    for o in lods:
        if not o.data.uv_layers:
            o.data.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=1.152, island_margin=margin, area_weight=0.2)
    bpy.ops.object.mode_set(mode="OBJECT")
    for o, k in zip(lods, LOD_UV_SCALE):
        uvl = o.data.uv_layers["UVMap"].data
        for p in o.data.polygons:
            boost = DISPLAY_BOOST if o is lods[0] and p.material_index in display else 1.0
            for li in p.loop_indices:
                uvl[li].uv = uvl[li].uv * (k * boost)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(rotate=True, margin_method="FRACTION", margin=margin)
    bpy.ops.object.mode_set(mode="OBJECT")


def tri_texels(obj, size, keep):
    """Texel indices (Blender pixel order) whose centres fall in obj's triangles of material indices
    `keep`, with each texel's world position (bakery_shop's, re-derived)."""
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


def loaf_field(piece, s, z):
    """The bread on the shelves as (AO multiplier, night multiplier) at position s (m along the window
    from its centre) and height z: seeded loaf ellipses on each shelf in the loaf band, darker gaps
    between them and the dim shop interior above (bakery_shop's field on a flat front)."""
    rng = random.Random(piece)
    a_mul = np.full(s.shape, 0.62, np.float32)
    b_mul = np.full(s.shape, 0.55, np.float32)
    row_h = (GLASS1 - GLASS0) / SF_ROWS
    span = (SF_Y1 - SF_Y0) / 2
    for row in range(SF_ROWS):
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


LOAF = {}  # the loaf pass's stats, for the build log


def fill_with_loaves(px, lods, size, hooks, **kw):
    """The loaves, authored over LOD0's baked shop-pane texels (LOD1 keeps the colour bands only), then
    _house.fill_unbaked, in bakery_shop's order."""
    keep = {i for i, m in enumerate(lods[0].data.materials) if m.name.endswith(("_display_glass", "_loaf_glass"))}
    idx, pos = tri_texels(lods[0], size, keep)
    am, bm = loaf_field(lods[0].name.rsplit("_lod", 1)[0], pos[:, 1] - (SF_Y0 + SF_Y1) / 2, pos[:, 2])
    px = px.copy()
    px[idx, 3] *= am
    px[idx, 2] *= bm
    LOAF["loafTexels"] = int(np.unique(idx).size)
    print(f"bakery_b: loaves over {LOAF['loafTexels']} shop-pane texels")
    return _house.fill_unbaked(px, lods, size, hooks, **kw)


def _late_then_box(src, kind, name):
    """lh.build_piece's collider call is its last step before the export, after the bake: LOD1 first
    takes the bench and the bread basket (bakery_shop.late_bench's rule; owner A2 Q4, 2026-10-21:
    every piece has a middle LOD), copied from LOD0 with their baked UVs, then the box as before, then
    LOD2 takes the drum basket (bakery_shop.late_drum's rule) after the box, so the box is unchanged."""
    piece = name[: -len("_collider")]
    lod0, lod1 = (bpy.data.objects[f"{piece}_lod{i}"] for i in (0, 1))
    wicker, bread = f"{piece}_wicker", f"{piece}_bread"
    _, late = _lod_copy.copy_with_support(lod0, lod1, (wicker, bread), f"{piece}_timber", 0.6, 1.0)
    col = collider.collider_from(src, kind, name)
    _, drum = _lod_copy.copy_faces(lod0, src, lambda c, m: m in (wicker, bread) and c.z < 0.4)
    tris = [geom.tri_count(bpy.data.objects[f"{piece}_lod{i}"]) for i in range(3)]
    print(f"{piece}: LOD1 + bench and basket {late}, LOD2 + drum basket {drum} tris -> tris per LOD {tris}")
    budgets.check(tris, "prop", piece)
    return col


def build_piece(spec, generator):
    """`_house_longhouse.build_piece` with this module's builder and bakery_shop's fit and bake settings
    (module docstring)."""
    lh.build = build
    lh.collider = types.SimpleNamespace(collider_from=_late_then_box)
    lh.COTTAGE_BOUNDS = BAKERY_BOX
    lh.COTTAGE_LOD0_BOUNDS = BAKERY_LOD0_BOUNDS
    lh.UV_MARGIN = UV_MARGIN
    lh.BAKE_MARGIN = BAKE_MARGIN
    lh.uv = types.SimpleNamespace(atlas_unwrap=unwrap, measure_texel_density=uv.measure_texel_density)
    lh.fill_unbaked = fill_with_loaves
    BUILT.clear()
    root, lods = lh.build_piece(spec, generator)
    assert BUILT == [f"{spec['piece']}_lod{i}" for i in range(3)], BUILT
    return root, lods
